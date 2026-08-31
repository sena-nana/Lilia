use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use reqwest::blocking::Client;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, USER_AGENT};
use url::Url;

const MAX_MARKDOWN_IMAGE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct LoadedMarkdownImage {
    bytes: Arc<[u8]>,
    media_type: String,
}

impl LoadedMarkdownImage {
    pub(crate) fn media_type(&self) -> &str {
        &self.media_type
    }

    pub(crate) fn encoded_len(&self) -> usize {
        self.bytes.len()
    }
}

pub(crate) fn load_markdown_image(source: &str) -> Result<LoadedMarkdownImage, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("image source is empty".to_owned());
    }
    if source.starts_with("data:") {
        return load_data_image(source);
    }
    if let Ok(url) = Url::parse(source) {
        return match url.scheme() {
            "http" | "https" => load_remote_image(url),
            "file" => url
                .to_file_path()
                .map_err(|_| "file image URL is invalid".to_owned())
                .and_then(|path| load_file_image(&path)),
            _ => Err("image URL scheme is not supported".to_owned()),
        };
    }
    let path = PathBuf::from(source);
    if path.is_absolute() {
        load_file_image(&path)
    } else {
        Err("relative image paths need an application base directory".to_owned())
    }
}

fn load_remote_image(url: Url) -> Result<LoadedMarkdownImage, String> {
    if !url.username().is_empty() || url.password().is_some() {
        return Err("image URL credentials are not supported".to_owned());
    }
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .map_err(|error| format!("image client initialization failed: {error}"))?;
    let mut response = client
        .get(url)
        .header(USER_AGENT, "LiliaCode-Native/markdown-image")
        .send()
        .map_err(|error| format!("image request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("image request returned {}", response.status()));
    }
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_MARKDOWN_IMAGE_BYTES)
    {
        return Err("image response is too large".to_owned());
    }
    let declared_media_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(normalize_image_media_type);
    let bytes = read_bounded(&mut response)?;
    let media_type = declared_media_type
        .or_else(|| detect_image_media_type(&bytes))
        .ok_or_else(|| "image response content type is not supported".to_owned())?;
    loaded_image(bytes, media_type)
}

fn load_data_image(source: &str) -> Result<LoadedMarkdownImage, String> {
    let (metadata, payload) = source
        .strip_prefix("data:")
        .and_then(|value| value.split_once(','))
        .ok_or_else(|| "image data URL is invalid".to_owned())?;
    let mut parts = metadata.split(';');
    let media_type = parts
        .next()
        .and_then(normalize_image_media_type)
        .ok_or_else(|| "image data URL content type is not supported".to_owned())?;
    if !parts.any(|part| part.eq_ignore_ascii_case("base64")) {
        return Err("image data URL must use base64 encoding".to_owned());
    }
    let estimated = payload.len().saturating_mul(3) / 4;
    if estimated as u64 > MAX_MARKDOWN_IMAGE_BYTES {
        return Err("image data URL is too large".to_owned());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| "image data URL base64 is invalid".to_owned())?;
    loaded_image(bytes, media_type)
}

fn load_file_image(path: &Path) -> Result<LoadedMarkdownImage, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("image file metadata failed: {error}"))?;
    if !metadata.is_file() {
        return Err("image path is not a file".to_owned());
    }
    if metadata.len() > MAX_MARKDOWN_IMAGE_BYTES {
        return Err("image file is too large".to_owned());
    }
    let media_type = path
        .extension()
        .and_then(|value| value.to_str())
        .and_then(image_media_type_for_extension)
        .ok_or_else(|| "image file type is not supported".to_owned())?;
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("image file could not be opened: {error}"))?;
    let bytes = read_bounded(&mut file)?;
    loaded_image(bytes, media_type)
}

fn read_bounded(reader: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_MARKDOWN_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("image body could not be read: {error}"))?;
    if bytes.len() as u64 > MAX_MARKDOWN_IMAGE_BYTES {
        return Err("image body is too large".to_owned());
    }
    Ok(bytes)
}

fn loaded_image(bytes: Vec<u8>, media_type: String) -> Result<LoadedMarkdownImage, String> {
    if bytes.is_empty() || !payload_matches_media_type(&bytes, &media_type) {
        return Err("image body does not match its content type".to_owned());
    }
    if let Some(format) = raster_image_format(&media_type) {
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16_384);
        limits.max_image_height = Some(16_384);
        limits.max_alloc = Some(256 * 1024 * 1024);
        let mut reader = image::ImageReader::with_format(Cursor::new(&bytes), format);
        reader.limits(limits);
        reader
            .decode()
            .map_err(|_| "image body could not be decoded".to_owned())?;
    }
    Ok(LoadedMarkdownImage {
        bytes: Arc::from(bytes),
        media_type,
    })
}

fn raster_image_format(media_type: &str) -> Option<image::ImageFormat> {
    match media_type {
        "image/png" => Some(image::ImageFormat::Png),
        "image/jpeg" => Some(image::ImageFormat::Jpeg),
        "image/gif" => Some(image::ImageFormat::Gif),
        "image/webp" => Some(image::ImageFormat::WebP),
        "image/bmp" => Some(image::ImageFormat::Bmp),
        _ => None,
    }
}

fn normalize_image_media_type(value: &str) -> Option<String> {
    let value = value.split(';').next()?.trim().to_ascii_lowercase();
    matches!(
        value.as_str(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/bmp" | "image/svg+xml"
    )
    .then_some(value)
}

fn image_media_type_for_extension(extension: &str) -> Option<String> {
    let media_type = match extension.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    Some(media_type.to_owned())
}

fn detect_image_media_type(bytes: &[u8]) -> Option<String> {
    [
        "image/png",
        "image/jpeg",
        "image/gif",
        "image/webp",
        "image/bmp",
        "image/svg+xml",
    ]
    .into_iter()
    .find(|media_type| payload_matches_media_type(bytes, media_type))
    .map(str::to_owned)
}

fn payload_matches_media_type(bytes: &[u8], media_type: &str) -> bool {
    match media_type {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "image/webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        "image/bmp" => bytes.starts_with(b"BM"),
        "image/svg+xml" => {
            let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);
            let prefix = prefix.trim_start_matches('\u{feff}').trim_start();
            prefix.starts_with("<svg") || (prefix.starts_with("<?xml") && prefix.contains("<svg"))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_images_are_bounded_and_verified_against_the_declared_type() {
        let png = concat!(
            "data:image/png;base64,",
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        );
        let loaded = load_markdown_image(png).unwrap();
        assert_eq!(loaded.media_type(), "image/png");
        assert!(loaded.encoded_len() > 0);

        assert!(
            load_markdown_image("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB").is_err()
        );
        assert!(load_markdown_image("data:image/png;base64,PGh0bWw+").is_err());
        assert!(load_markdown_image("javascript:alert(1)").is_err());
    }
}

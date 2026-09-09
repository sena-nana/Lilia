use std::io::{Cursor, Read};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
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

/// Load a markdown image.
///
/// Local/`file:` sources must resolve under `workspace_root` after canonicalization.
/// Remote `http(s)` keeps size limits and applies HostHttp-style SSRF host checks.
pub(crate) fn load_markdown_image(
    source: &str,
    workspace_root: Option<&Path>,
) -> Result<LoadedMarkdownImage, String> {
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
            "file" => {
                let path = url
                    .to_file_path()
                    .map_err(|_| "file image URL is invalid".to_owned())?;
                load_workspace_file_image(&path, workspace_root)
            }
            _ => Err("image URL scheme is not supported".to_owned()),
        };
    }
    let path = PathBuf::from(source);
    load_workspace_file_image(&path, workspace_root)
}

fn load_workspace_file_image(
    path: &Path,
    workspace_root: Option<&Path>,
) -> Result<LoadedMarkdownImage, String> {
    let Some(root) = workspace_root else {
        return Err(
            "local markdown images require a workspace root (absolute/file paths are restricted)"
                .to_owned(),
        );
    };
    let root = root
        .canonicalize()
        .map_err(|error| format!("workspace root could not be canonicalized: {error}"))?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("image path could not be canonicalized: {error}"))?;
    if !canonical.starts_with(&root) {
        return Err("image path escapes the workspace root".to_owned());
    }
    load_file_image(&canonical)
}

fn load_remote_image(url: Url) -> Result<LoadedMarkdownImage, String> {
    validate_markdown_remote_url(&url)?;
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("image client initialization failed: {error}"))?;
    let mut current = url;
    let mut response = client
        .get(current.clone())
        .header(USER_AGENT, "LiliaCode-Native/markdown-image")
        .send()
        .map_err(|error| format!("image request failed: {error}"))?;
    // Manual redirect cap with per-hop SSRF re-validation (HostHttp caution).
    let mut hops = 0u8;
    while response.status().is_redirection() {
        hops += 1;
        if hops > 3 {
            return Err("image request exceeded redirect limit".to_owned());
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| "image redirect is missing a location".to_owned())?;
        let next = current
            .join(location)
            .map_err(|_| "image redirect location is invalid".to_owned())?;
        validate_markdown_remote_url(&next)?;
        current = next;
        response = client
            .get(current.clone())
            .header(USER_AGENT, "LiliaCode-Native/markdown-image")
            .send()
            .map_err(|error| format!("image request failed: {error}"))?;
    }
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

fn validate_markdown_remote_url(url: &Url) -> Result<(), String> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err("image URL scheme is not supported".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("image URL credentials are not supported".to_owned());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "image URL is missing a host".to_owned())?;
    if host_is_blocked(host) {
        return Err(format!(
            "refusing private/link-local/metadata image host `{host}`"
        ));
    }
    let port = url.port_or_known_default().unwrap_or(80);
    let authority = format!("{host}:{port}");
    match authority.to_socket_addrs() {
        Ok(addrs) => {
            for addr in addrs {
                if ip_is_blocked(addr.ip()) {
                    return Err(format!(
                        "refusing image URL whose resolved address is private/link-local/metadata ({})",
                        addr.ip()
                    ));
                }
            }
        }
        Err(error) => {
            if host.parse::<IpAddr>().is_err() {
                return Err(format!("failed to resolve image host `{host}`: {error}"));
            }
        }
    }
    Ok(())
}

fn host_is_blocked(host: &str) -> bool {
    let lower = host.trim().trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
    if lower == "localhost"
        || lower == "metadata.google.internal"
        || lower == "metadata"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
    {
        return true;
    }
    if let Ok(ip) = lower.parse::<IpAddr>() {
        return ip_is_blocked(ip);
    }
    // IPv4-mapped / dotted forms already handled; reject link-local DNS labels.
    false
}

fn ip_is_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => ipv4_is_blocked(v4),
        IpAddr::V6(v6) => ipv6_is_blocked(v6),
    }
}

fn ipv4_is_blocked(ip: Ipv4Addr) -> bool {
    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.octets()[0] == 0
        || (ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 64) // 100.64/10
        || ip.octets()[0] == 169 && ip.octets()[1] == 254
}

fn ipv6_is_blocked(ip: Ipv6Addr) -> bool {
    if ip.is_unspecified() || ip.is_loopback() {
        return true;
    }
    let segments = ip.segments();
    // Unique local fc00::/7, link-local fe80::/10.
    if (segments[0] & 0xfe00) == 0xfc00 || (segments[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    if let Some(v4) = ip.to_ipv4_mapped() {
        return ipv4_is_blocked(v4);
    }
    if let Some(v4) = ip.to_ipv4() {
        return ipv4_is_blocked(v4);
    }
    false
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
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn data_images_are_bounded_and_verified_against_the_declared_type() {
        let png = concat!(
            "data:image/png;base64,",
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        );
        let loaded = load_markdown_image(png, None).unwrap();
        assert_eq!(loaded.media_type(), "image/png");
        assert!(loaded.encoded_len() > 0);

        assert!(load_markdown_image(
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB",
            None
        )
        .is_err());
        assert!(load_markdown_image("data:image/png;base64,PGh0bWw+", None).is_err());
        assert!(load_markdown_image("javascript:alert(1)", None).is_err());
    }

    #[test]
    fn local_images_must_stay_under_workspace_root() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lilia-md-img-{stamp}"));
        let nested = root.join("assets");
        fs::create_dir_all(&nested).unwrap();
        let png_bytes = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        let inside = nested.join("dot.png");
        fs::write(&inside, &png_bytes).unwrap();
        let outside = std::env::temp_dir().join(format!("lilia-md-img-outside-{stamp}.png"));
        fs::write(&outside, &png_bytes).unwrap();

        assert!(load_markdown_image("assets/dot.png", Some(&root)).is_ok());
        assert!(load_markdown_image(outside.to_str().unwrap(), Some(&root)).is_err());
        assert!(load_markdown_image(outside.to_str().unwrap(), None).is_err());
        let file_url = Url::from_file_path(&outside).unwrap();
        assert!(load_markdown_image(file_url.as_str(), Some(&root)).is_err());

        let _ = fs::remove_file(&outside);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn remote_image_hosts_block_private_and_metadata_targets() {
        for target in [
            "http://127.0.0.1/x.png",
            "https://localhost/x.png",
            "http://169.254.169.254/latest/meta-data",
            "https://metadata.google.internal/",
            "http://192.168.0.1/a.png",
        ] {
            let error = validate_markdown_remote_url(&Url::parse(target).unwrap())
                .expect_err(target);
            assert!(
                error.contains("private")
                    || error.contains("metadata")
                    || error.contains("refusing")
                    || error.contains("resolve"),
                "target={target} error={error}"
            );
        }
    }
}

use std::collections::{HashSet, VecDeque};
use std::env;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use lilia_contracts::{ChatAttachment, ChatAttachmentDirectoryMeta, ChatAttachmentKind};
use uuid::Uuid;

use crate::application::{
    DesktopApplication, DesktopClipboardImage, DesktopHostAction, DesktopHostError,
    DesktopHostResult,
};

const DIRECTORY_SCAN_LIMIT: usize = 1000;
const MAX_CLIPBOARD_IMAGE_DIMENSION: u32 = 16_384;
const MAX_CLIPBOARD_IMAGE_RGBA_BYTES: usize = 64 * 1024 * 1024;
const MAX_CLIPBOARD_ENCODED_IMAGE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_CLIPBOARD_TEXT_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024;
pub const LONG_CLIPBOARD_TEXT_ATTACHMENT_THRESHOLD: usize = 2_000;

static CLIPBOARD_IMAGE_DISPLAY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static CLIPBOARD_TEXT_DISPLAY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, PartialEq, Eq)]
pub struct DesktopClipboardEncodedImage {
    pub bytes: Vec<u8>,
    pub mime: Option<String>,
    pub name: Option<String>,
}

impl fmt::Debug for DesktopClipboardEncodedImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopClipboardEncodedImage")
            .field("byte_len", &self.bytes.len())
            .field("mime", &self.mime)
            .field("name", &self.name)
            .finish()
    }
}

pub fn clipboard_text_should_be_attachment(text: &str) -> bool {
    text.encode_utf16().count() >= LONG_CLIPBOARD_TEXT_ATTACHMENT_THRESHOLD
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopAttachmentError {
    #[error(transparent)]
    Host(#[from] DesktopHostError),
    #[error("clipboard host returned an unexpected result")]
    UnexpectedHostResult,
    #[error("clipboard image is invalid: {message}")]
    InvalidClipboardImage { message: String },
    #[error("clipboard text is invalid: {message}")]
    InvalidClipboardText { message: String },
    #[error("failed to encode clipboard image: {message}")]
    ImageEncoding { message: String },
    #[error("failed to {operation}: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },
}

impl DesktopApplication {
    pub fn read_clipboard_file_paths(&self) -> Result<Vec<PathBuf>, DesktopAttachmentError> {
        match self.inner.host.execute(
            &self.inner.host_context,
            DesktopHostAction::ReadClipboardFilePaths,
        )? {
            DesktopHostResult::ClipboardFilePaths(paths) => {
                let mut seen = HashSet::new();
                Ok(paths
                    .into_iter()
                    .filter(|path| !path.as_os_str().is_empty())
                    .filter(|path| seen.insert(path.clone()))
                    .collect())
            }
            _ => Err(DesktopAttachmentError::UnexpectedHostResult),
        }
    }

    pub fn capture_clipboard_file_attachments(
        &self,
    ) -> Result<Vec<ChatAttachment>, DesktopAttachmentError> {
        self.read_clipboard_file_paths()
            .map(describe_attachment_paths)
    }

    pub fn capture_clipboard_image_attachment(
        &self,
    ) -> Result<Option<ChatAttachment>, DesktopAttachmentError> {
        match self.inner.host.execute(
            &self.inner.host_context,
            DesktopHostAction::ReadClipboardImage,
        )? {
            DesktopHostResult::ClipboardImage(Some(image)) => {
                save_clipboard_image_attachment(&self.inner.host_context.home, image).map(Some)
            }
            DesktopHostResult::ClipboardImage(None) => Ok(None),
            _ => Err(DesktopAttachmentError::UnexpectedHostResult),
        }
    }

    pub fn cache_encoded_clipboard_image_attachment(
        &self,
        image: DesktopClipboardEncodedImage,
    ) -> Result<ChatAttachment, DesktopAttachmentError> {
        save_encoded_clipboard_image_attachment(&self.inner.host_context.home, image)
    }

    pub fn cache_clipboard_text_attachment(
        &self,
        text: &str,
    ) -> Result<ChatAttachment, DesktopAttachmentError> {
        save_clipboard_text_attachment(&self.inner.host_context.home, text)
    }
}

pub fn save_clipboard_image_attachment(
    home: &Path,
    image: DesktopClipboardImage,
) -> Result<ChatAttachment, DesktopAttachmentError> {
    validate_clipboard_image(&image)?;
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            &image.rgba,
            image.width,
            image.height,
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| DesktopAttachmentError::ImageEncoding {
            message: error.to_string(),
        })?;
    let directory = home.join("cache").join("clipboard-images");
    fs::create_dir_all(&directory).map_err(|error| DesktopAttachmentError::Storage {
        operation: "create clipboard image cache",
        message: error.to_string(),
    })?;
    let path = directory.join(format!("clipboard-{}-{}.png", now_millis(), Uuid::new_v4()));
    write_new_cache_file(&path, &png, "save clipboard image")?;
    let mut attachment = describe_attachment_path(&path);
    attachment.name = "剪贴板图片.png".to_owned();
    attachment.mime = Some("image/png".to_owned());
    Ok(attachment)
}

pub fn save_encoded_clipboard_image_attachment(
    home: &Path,
    image: DesktopClipboardEncodedImage,
) -> Result<ChatAttachment, DesktopAttachmentError> {
    if image.bytes.is_empty() || image.bytes.len() > MAX_CLIPBOARD_ENCODED_IMAGE_BYTES {
        return Err(DesktopAttachmentError::InvalidClipboardImage {
            message: format!("unsupported encoded byte length {}", image.bytes.len()),
        });
    }
    let (extension, mime) = clipboard_image_format(image.mime.as_deref(), image.name.as_deref());
    let directory = home.join("cache").join("clipboard-images");
    fs::create_dir_all(&directory).map_err(|error| DesktopAttachmentError::Storage {
        operation: "create clipboard image cache",
        message: error.to_string(),
    })?;
    let path = directory.join(format!(
        "clipboard-{}-{}.{}",
        now_millis(),
        Uuid::new_v4(),
        extension
    ));
    write_new_cache_file(&path, &image.bytes, "save clipboard image")?;
    let sequence = CLIPBOARD_IMAGE_DISPLAY_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;
    let mut attachment = describe_attachment_path(&path);
    attachment.name = format!("图片 {sequence}.{extension}");
    attachment.mime = Some(mime.to_owned());
    Ok(attachment)
}

pub fn save_clipboard_text_attachment(
    home: &Path,
    text: &str,
) -> Result<ChatAttachment, DesktopAttachmentError> {
    if text.is_empty() || text.len() > MAX_CLIPBOARD_TEXT_ATTACHMENT_BYTES {
        return Err(DesktopAttachmentError::InvalidClipboardText {
            message: format!("unsupported UTF-8 byte length {}", text.len()),
        });
    }
    let directory = home.join("cache").join("clipboard-texts");
    fs::create_dir_all(&directory).map_err(|error| DesktopAttachmentError::Storage {
        operation: "create clipboard text cache",
        message: error.to_string(),
    })?;
    let path = directory.join(format!("clipboard-{}-{}.txt", now_millis(), Uuid::new_v4()));
    write_new_cache_file(&path, text.as_bytes(), "save clipboard text")?;
    let sequence = CLIPBOARD_TEXT_DISPLAY_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;
    let mut attachment = describe_attachment_path(&path);
    attachment.name = format!("粘贴文本 {sequence}.txt");
    attachment.mime = None;
    Ok(attachment)
}

fn clipboard_image_format(mime: Option<&str>, name: Option<&str>) -> (&'static str, &'static str) {
    match mime
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "image/avif" => return ("avif", "image/avif"),
        "image/bmp" => return ("bmp", "image/bmp"),
        "image/gif" => return ("gif", "image/gif"),
        "image/jpeg" | "image/jpg" => return ("jpg", "image/jpeg"),
        "image/png" => return ("png", "image/png"),
        "image/svg+xml" => return ("svg", "image/svg+xml"),
        "image/webp" => return ("webp", "image/webp"),
        _ => {}
    }
    match name
        .and_then(|name| Path::new(name).extension())
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "avif" => ("avif", "image/avif"),
        "bmp" => ("bmp", "image/bmp"),
        "gif" => ("gif", "image/gif"),
        "jpg" | "jpeg" => ("jpg", "image/jpeg"),
        "svg" => ("svg", "image/svg+xml"),
        "webp" => ("webp", "image/webp"),
        _ => ("png", "image/png"),
    }
}

fn write_new_cache_file(
    path: &Path,
    bytes: &[u8],
    operation: &'static str,
) -> Result<(), DesktopAttachmentError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| DesktopAttachmentError::Storage {
            operation,
            message: error.to_string(),
        })?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_data()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(DesktopAttachmentError::Storage {
            operation,
            message: error.to_string(),
        });
    }
    Ok(())
}

fn validate_clipboard_image(image: &DesktopClipboardImage) -> Result<(), DesktopAttachmentError> {
    if image.width == 0
        || image.height == 0
        || image.width > MAX_CLIPBOARD_IMAGE_DIMENSION
        || image.height > MAX_CLIPBOARD_IMAGE_DIMENSION
    {
        return Err(DesktopAttachmentError::InvalidClipboardImage {
            message: format!("unsupported dimensions {}x{}", image.width, image.height),
        });
    }
    let expected = usize::try_from(image.width)
        .ok()
        .and_then(|width| {
            usize::try_from(image.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|bytes| *bytes <= MAX_CLIPBOARD_IMAGE_RGBA_BYTES);
    if expected != Some(image.rgba.len()) {
        return Err(DesktopAttachmentError::InvalidClipboardImage {
            message: format!(
                "RGBA byte length {} does not match dimensions",
                image.rgba.len()
            ),
        });
    }
    Ok(())
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub fn describe_attachment_path(path: impl AsRef<Path>) -> ChatAttachment {
    let raw_path = path.as_ref();
    let normalized_path = if raw_path.is_absolute() {
        raw_path.to_path_buf()
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(raw_path))
            .unwrap_or_else(|_| raw_path.to_path_buf())
    };
    let metadata = fs::metadata(&normalized_path).ok();
    let kind = metadata
        .as_ref()
        .map(|metadata| {
            if metadata.is_file() {
                ChatAttachmentKind::File
            } else if metadata.is_dir() {
                ChatAttachmentKind::Directory
            } else {
                ChatAttachmentKind::Unknown
            }
        })
        .unwrap_or(ChatAttachmentKind::Unknown);
    let size = metadata
        .as_ref()
        .filter(|metadata| metadata.is_file())
        .map(fs::Metadata::len);
    let mime = (kind == ChatAttachmentKind::File)
        .then(|| image_mime_for_path(&normalized_path))
        .flatten();
    let directory = metadata
        .as_ref()
        .filter(|metadata| metadata.is_dir())
        .map(|_| scan_directory_meta(&normalized_path));
    let path = normalized_path.to_string_lossy().to_string();
    let name = normalized_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.clone());

    ChatAttachment {
        id: format!("att-{}", Uuid::new_v4()),
        name,
        path,
        kind,
        size,
        exists: metadata.is_some(),
        mime,
        directory,
    }
}

pub fn describe_attachment_paths(
    paths: impl IntoIterator<Item = impl AsRef<Path>>,
) -> Vec<ChatAttachment> {
    paths.into_iter().map(describe_attachment_path).collect()
}

fn image_mime_for_path(path: &Path) -> Option<String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())?
        .to_ascii_lowercase();
    let mime = match extension.as_str() {
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        _ => return None,
    };
    Some(mime.to_owned())
}

fn scan_directory_meta(path: &Path) -> ChatAttachmentDirectoryMeta {
    let mut result = ChatAttachmentDirectoryMeta {
        file_count: 0,
        directory_count: 0,
        total_size: 0,
        truncated: false,
        unreadable_count: 0,
    };
    let mut scanned = 0usize;
    let mut queue = VecDeque::from([PathBuf::from(path)]);
    'scan: while let Some(directory) = queue.pop_front() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                result.unreadable_count = result.unreadable_count.saturating_add(1);
                continue;
            }
        };
        for entry in entries {
            if scanned >= DIRECTORY_SCAN_LIMIT {
                result.truncated = true;
                break 'scan;
            }
            scanned += 1;
            let Ok(entry) = entry else {
                result.unreadable_count = result.unreadable_count.saturating_add(1);
                continue;
            };
            let Ok(file_type) = entry.file_type() else {
                result.unreadable_count = result.unreadable_count.saturating_add(1);
                continue;
            };
            if file_type.is_dir() {
                result.directory_count = result.directory_count.saturating_add(1);
                queue.push_back(entry.path());
            } else if file_type.is_file() {
                result.file_count = result.file_count.saturating_add(1);
                match entry.metadata() {
                    Ok(metadata) => {
                        result.total_size = result.total_size.saturating_add(metadata.len());
                    }
                    Err(_) => {
                        result.unreadable_count = result.unreadable_count.saturating_add(1);
                    }
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Arc;

    struct ClipboardFileHost {
        paths: Vec<PathBuf>,
    }

    impl crate::application::DesktopHost for ClipboardFileHost {
        fn execute(
            &self,
            _context: &crate::application::DesktopHostContext,
            action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            if action == DesktopHostAction::ReadClipboardFilePaths {
                Ok(DesktopHostResult::ClipboardFilePaths(self.paths.clone()))
            } else {
                Ok(DesktopHostResult::Completed)
            }
        }
    }

    #[test]
    fn describes_files_with_frontend_compatible_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.png");
        fs::File::create(&path).unwrap().write_all(b"png").unwrap();

        let attachment = describe_attachment_path(&path);

        assert_eq!(attachment.name, "capture.png");
        assert_eq!(attachment.kind, ChatAttachmentKind::File);
        assert_eq!(attachment.size, Some(3));
        assert_eq!(attachment.mime.as_deref(), Some("image/png"));
        assert!(attachment.exists);
    }

    #[test]
    fn missing_paths_remain_visible_as_unknown_attachments() {
        let directory = tempfile::tempdir().unwrap();
        let attachment = describe_attachment_path(directory.path().join("missing.txt"));

        assert_eq!(attachment.kind, ChatAttachmentKind::Unknown);
        assert!(!attachment.exists);
        assert_eq!(attachment.size, None);
    }

    #[test]
    fn clipboard_rgba_is_validated_encoded_and_described_as_png() {
        let home = tempfile::tempdir().unwrap();
        let attachment = save_clipboard_image_attachment(
            home.path(),
            DesktopClipboardImage {
                width: 2,
                height: 1,
                rgba: vec![255, 0, 0, 255, 0, 255, 0, 255],
            },
        )
        .unwrap();

        assert_eq!(attachment.name, "剪贴板图片.png");
        assert_eq!(attachment.mime.as_deref(), Some("image/png"));
        assert_eq!(attachment.kind, ChatAttachmentKind::File);
        assert!(Path::new(&attachment.path).starts_with(home.path()));
        assert_eq!(image::image_dimensions(&attachment.path).unwrap(), (2, 1));
    }

    #[test]
    fn invalid_clipboard_rgba_never_creates_a_cache_file() {
        let home = tempfile::tempdir().unwrap();
        let error = save_clipboard_image_attachment(
            home.path(),
            DesktopClipboardImage {
                width: 2,
                height: 2,
                rgba: vec![0; 4],
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            DesktopAttachmentError::InvalidClipboardImage { .. }
        ));
        assert!(!home.path().join("cache").exists());
    }

    #[test]
    fn encoded_clipboard_images_preserve_supported_format_and_bytes() {
        let home = tempfile::tempdir().unwrap();
        let bytes = b"GIF89a clipboard fixture".to_vec();
        let attachment = save_encoded_clipboard_image_attachment(
            home.path(),
            DesktopClipboardEncodedImage {
                bytes: bytes.clone(),
                mime: Some("image/gif".to_owned()),
                name: Some("ignored.png".to_owned()),
            },
        )
        .unwrap();

        assert!(attachment.name.starts_with("图片 "));
        assert!(attachment.name.ends_with(".gif"));
        assert_eq!(attachment.mime.as_deref(), Some("image/gif"));
        assert_eq!(attachment.size, Some(bytes.len() as u64));
        assert_eq!(fs::read(&attachment.path).unwrap(), bytes);
    }

    #[test]
    fn clipboard_text_is_preserved_as_a_utf8_attachment() {
        let home = tempfile::tempdir().unwrap();
        let text = "很长的粘贴文本\nwith ascii";
        let attachment = save_clipboard_text_attachment(home.path(), text).unwrap();

        assert!(attachment.name.starts_with("粘贴文本 "));
        assert!(attachment.name.ends_with(".txt"));
        assert_eq!(attachment.mime, None);
        assert_eq!(attachment.size, Some(text.len() as u64));
        assert_eq!(fs::read_to_string(&attachment.path).unwrap(), text);
    }

    #[test]
    fn long_clipboard_text_threshold_matches_the_frontend_utf16_boundary() {
        assert!(!clipboard_text_should_be_attachment(&"中".repeat(1_999)));
        assert!(clipboard_text_should_be_attachment(&"中".repeat(2_000)));
        assert!(clipboard_text_should_be_attachment(&"😀".repeat(1_000)));
    }

    #[test]
    fn empty_clipboard_payloads_do_not_create_cache_directories() {
        let home = tempfile::tempdir().unwrap();
        assert!(matches!(
            save_encoded_clipboard_image_attachment(
                home.path(),
                DesktopClipboardEncodedImage {
                    bytes: Vec::new(),
                    mime: Some("image/png".to_owned()),
                    name: None,
                }
            ),
            Err(DesktopAttachmentError::InvalidClipboardImage { .. })
        ));
        assert!(matches!(
            save_clipboard_text_attachment(home.path(), ""),
            Err(DesktopAttachmentError::InvalidClipboardText { .. })
        ));
        assert!(!home.path().join("cache").exists());
    }

    #[test]
    fn clipboard_file_paths_are_deduplicated_and_described_by_the_application() {
        let home = tempfile::tempdir().unwrap();
        let file = home.path().join("notes.md");
        fs::write(&file, "context").unwrap();
        let directory = home.path().join("sources");
        fs::create_dir(&directory).unwrap();
        let application = DesktopApplication::bootstrap(
            crate::application::DesktopApplicationConfig::new(home.path(), "attachment-test").unwrap(),
            Arc::new(ClipboardFileHost {
                paths: vec![
                    file.clone(),
                    PathBuf::new(),
                    file.clone(),
                    directory.clone(),
                ],
            }),
        )
        .unwrap();

        let attachments = application.capture_clipboard_file_attachments().unwrap();

        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0].path, file.to_string_lossy());
        assert_eq!(attachments[0].kind, ChatAttachmentKind::File);
        assert_eq!(attachments[1].path, directory.to_string_lossy());
        assert_eq!(attachments[1].kind, ChatAttachmentKind::Directory);
    }
}

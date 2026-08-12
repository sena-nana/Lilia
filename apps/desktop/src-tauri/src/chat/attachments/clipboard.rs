use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use base64::{engine::general_purpose, Engine as _};
use uuid::Uuid;

use crate::chat::attachments::describe::describe_attachment_path;
use crate::chat::state::now_millis;
use crate::chat::types::{ChatAttachment, ClipboardImageInput, ClipboardTextInput};
use crate::store;

static CLIPBOARD_IMAGE_DISPLAY_SEQ: AtomicU64 = AtomicU64::new(0);
static CLIPBOARD_TEXT_DISPLAY_SEQ: AtomicU64 = AtomicU64::new(0);

pub(super) fn clipboard_image_extension(mime: Option<&str>, name: Option<&str>) -> &'static str {
    let normalized = mime.unwrap_or("").trim().to_ascii_lowercase();
    match normalized.as_str() {
        "image/avif" => "avif",
        "image/bmp" => "bmp",
        "image/gif" => "gif",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/svg+xml" => "svg",
        "image/webp" => "webp",
        _ => name
            .and_then(|value| Path::new(value).extension())
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .and_then(|ext| match ext.as_str() {
                "avif" => Some("avif"),
                "bmp" => Some("bmp"),
                "gif" => Some("gif"),
                "jpg" | "jpeg" => Some("jpg"),
                "png" => Some("png"),
                "svg" => Some("svg"),
                "webp" => Some("webp"),
                _ => None,
            })
            .unwrap_or("png"),
    }
}

fn normalize_clipboard_image_mime(mime: Option<&str>, ext: &str) -> String {
    let normalized = mime.unwrap_or("").trim().to_ascii_lowercase();
    if normalized.starts_with("image/") {
        return normalized;
    }
    match ext {
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "gif" => "image/gif",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        _ => "image/png",
    }
    .to_string()
}

pub(super) fn clipboard_images_cache_dir(home: &Path) -> PathBuf {
    home.join("cache").join("clipboard-images")
}

fn clipboard_image_path(home: &Path, mime: Option<&str>, name: Option<&str>, now: u64) -> PathBuf {
    let ext = clipboard_image_extension(mime, name);
    clipboard_images_cache_dir(home).join(format!("clipboard-{now}-{}.{}", Uuid::new_v4(), ext))
}

pub(super) fn clipboard_image_display_name(ext: &str, seq: u64) -> String {
    format!("图片 {seq}.{ext}")
}

pub(super) fn clipboard_texts_cache_dir(home: &Path) -> PathBuf {
    home.join("cache").join("clipboard-texts")
}

fn clipboard_text_path(home: &Path, now: u64) -> PathBuf {
    clipboard_texts_cache_dir(home).join(format!("clipboard-{now}-{}.txt", Uuid::new_v4()))
}

pub(super) fn clipboard_text_display_name(seq: u64) -> String {
    format!("粘贴文本 {seq}.txt")
}

pub(super) fn save_clipboard_image_to_cache(
    home: &Path,
    input: ClipboardImageInput,
    now: u64,
    display_seq: u64,
) -> Result<ChatAttachment, String> {
    let path = clipboard_image_path(home, input.mime.as_deref(), input.name.as_deref(), now);
    let parent = path
        .parent()
        .ok_or_else(|| "无法解析剪贴板图片缓存目录".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("创建剪贴板图片缓存目录失败：{e}"))?;
    let bytes = general_purpose::STANDARD
        .decode(input.bytes_base64.trim())
        .map_err(|e| format!("解析剪贴板图片失败：{e}"))?;
    fs::write(&path, bytes).map_err(|e| format!("保存剪贴板图片失败：{e}"))?;
    let mut attachment = describe_attachment_path(path.to_string_lossy().to_string());
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    attachment.name = clipboard_image_display_name(&ext, display_seq);
    attachment.mime = Some(normalize_clipboard_image_mime(input.mime.as_deref(), &ext));
    Ok(attachment)
}

pub(super) fn save_clipboard_text_to_cache(
    home: &Path,
    input: ClipboardTextInput,
    now: u64,
    display_seq: u64,
) -> Result<ChatAttachment, String> {
    let path = clipboard_text_path(home, now);
    let parent = path
        .parent()
        .ok_or_else(|| "无法解析剪贴板文本缓存目录".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("创建剪贴板文本缓存目录失败：{e}"))?;
    fs::write(&path, input.text.as_bytes()).map_err(|e| format!("保存剪贴板文本失败：{e}"))?;
    let mut attachment = describe_attachment_path(path.to_string_lossy().to_string());
    attachment.name = clipboard_text_display_name(display_seq);
    attachment.mime = None;
    Ok(attachment)
}

#[cfg(windows)]
pub(super) fn read_windows_clipboard_file_paths() -> Result<Vec<String>, String> {
    use clipboard_win::{formats::FileList, Clipboard, Getter};

    let _clipboard = Clipboard::new_attempts(10).map_err(|e| format!("打开剪贴板失败：{e}"))?;
    let mut paths = Vec::<String>::new();
    match FileList.read_clipboard(&mut paths) {
        Ok(_) => Ok(paths),
        Err(_) => Ok(Vec::new()),
    }
}

#[cfg(not(windows))]
pub(super) fn read_windows_clipboard_file_paths() -> Result<Vec<String>, String> {
    Ok(Vec::new())
}

pub(super) fn read_clipboard_file_paths() -> Result<Vec<String>, String> {
    read_windows_clipboard_file_paths().map(|paths| {
        paths
            .into_iter()
            .filter(|path| !path.trim().is_empty())
            .collect()
    })
}

pub(super) fn save_clipboard_image(input: ClipboardImageInput) -> Result<ChatAttachment, String> {
    let home = store::resolve_lilia_home();
    let display_seq = CLIPBOARD_IMAGE_DISPLAY_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    save_clipboard_image_to_cache(&home, input, now_millis(), display_seq)
}

pub(super) fn save_clipboard_text(input: ClipboardTextInput) -> Result<ChatAttachment, String> {
    let home = store::resolve_lilia_home();
    let display_seq = CLIPBOARD_TEXT_DISPLAY_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    save_clipboard_text_to_cache(&home, input, now_millis(), display_seq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_context_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!("lilia-context-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn clipboard_image_extension_uses_mime_then_safe_name_extension() {
        assert_eq!(
            clipboard_image_extension(Some("image/jpeg"), Some("ignored.png")),
            "jpg"
        );
        assert_eq!(clipboard_image_extension(None, Some("screen.webp")), "webp");
        assert_eq!(
            clipboard_image_extension(Some("text/plain"), Some("note.txt")),
            "png"
        );
    }

    #[test]
    fn clipboard_image_display_name_uses_short_sequence_name() {
        assert_eq!(clipboard_image_display_name("png", 1), "图片 1.png");
        assert_eq!(clipboard_image_display_name("webp", 12), "图片 12.webp");
    }

    #[test]
    fn clipboard_text_display_name_uses_short_sequence_name() {
        assert_eq!(clipboard_text_display_name(1), "粘贴文本 1.txt");
        assert_eq!(clipboard_text_display_name(12), "粘贴文本 12.txt");
    }

    #[test]
    fn clipboard_image_is_saved_under_cache_and_described_as_image_file() {
        let home = temp_context_root("clipboard-image");
        let input = ClipboardImageInput {
            mime: Some("image/png".to_string()),
            bytes_base64: general_purpose::STANDARD.encode([1_u8, 2, 3, 4]),
            name: None,
        };

        let attachment = save_clipboard_image_to_cache(&home, input, 12345, 1).unwrap();

        assert_eq!(attachment.name, "图片 1.png");
        assert_eq!(attachment.kind, lilia_contracts::ChatAttachmentKind::File);
        assert_eq!(attachment.exists, true);
        assert_eq!(attachment.mime.as_deref(), Some("image/png"));
        let path = PathBuf::from(&attachment.path);
        assert!(path.exists());
        assert_eq!(
            path.parent().unwrap(),
            clipboard_images_cache_dir(&home).as_path()
        );
        assert_eq!(fs::read(&path).unwrap(), vec![1_u8, 2, 3, 4]);

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn clipboard_text_is_saved_under_cache_and_described_as_file() {
        let home = temp_context_root("clipboard-text");
        let input = ClipboardTextInput {
            text: "很长的粘贴文本\nwith ascii".to_string(),
        };

        let attachment = save_clipboard_text_to_cache(&home, input, 12345, 1).unwrap();

        assert_eq!(attachment.name, "粘贴文本 1.txt");
        assert_eq!(attachment.kind, lilia_contracts::ChatAttachmentKind::File);
        assert_eq!(attachment.exists, true);
        assert_eq!(attachment.mime, None);
        assert_eq!(
            attachment.size,
            Some("很长的粘贴文本\nwith ascii".len() as u64)
        );
        let path = PathBuf::from(&attachment.path);
        assert!(path.exists());
        assert_eq!(
            path.parent().unwrap(),
            clipboard_texts_cache_dir(&home).as_path()
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "很长的粘贴文本\nwith ascii"
        );

        let _ = fs::remove_dir_all(home);
    }
}

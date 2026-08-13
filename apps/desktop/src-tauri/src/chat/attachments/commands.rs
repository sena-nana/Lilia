use base64::{engine::general_purpose, Engine as _};
use lilia_desktop_application::{DesktopApplication, DesktopClipboardEncodedImage};
use tauri::State;

use crate::chat::attachments::context_search::search_context_attachments;
use crate::chat::attachments::describe::describe_attachment_path;
use crate::chat::types::{
    ChatAttachment, ChatContextSearchResult, ClipboardImageInput, ClipboardTextInput,
};

#[tauri::command]
pub fn chat_describe_attachments(paths: Vec<String>) -> Result<Vec<ChatAttachment>, String> {
    Ok(paths
        .into_iter()
        .filter(|path| !path.trim().is_empty())
        .map(describe_attachment_path)
        .collect())
}

#[tauri::command]
pub fn chat_read_clipboard_file_paths(
    application: State<'_, DesktopApplication>,
) -> Result<Vec<String>, String> {
    application
        .read_clipboard_file_paths()
        .map(|paths| {
            paths
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect()
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn chat_save_clipboard_image(
    input: ClipboardImageInput,
    application: State<'_, DesktopApplication>,
) -> Result<ChatAttachment, String> {
    let bytes = general_purpose::STANDARD
        .decode(input.bytes_base64.trim())
        .map_err(|error| format!("解析剪贴板图片失败：{error}"))?;
    application
        .cache_encoded_clipboard_image_attachment(DesktopClipboardEncodedImage {
            bytes,
            mime: input.mime,
            name: input.name,
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn chat_save_clipboard_text(
    input: ClipboardTextInput,
    application: State<'_, DesktopApplication>,
) -> Result<ChatAttachment, String> {
    application
        .cache_clipboard_text_attachment(&input.text)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn chat_search_context_attachments(
    project_cwd: String,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<ChatContextSearchResult>, String> {
    search_context_attachments(project_cwd, query, limit)
}

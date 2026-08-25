use std::path::PathBuf;

use crate::{PlatformError, PlatformResult};

/// Decoded clipboard bitmap in straight RGBA8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// `Ok(None)` means the clipboard holds no value of the requested kind, which
/// is a normal outcome rather than a failure.
pub fn read_text() -> PlatformResult<Option<String>> {
    match open()?.get_text() {
        Ok(value) => Ok(Some(value)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(PlatformError::transient("clipboard_read_failed", error)),
    }
}

pub fn read_image() -> PlatformResult<Option<ClipboardImage>> {
    match open()?.get_image() {
        Ok(image) => {
            let width = u32::try_from(image.width).map_err(|_| invalid_image("width"))?;
            let height = u32::try_from(image.height).map_err(|_| invalid_image("height"))?;
            Ok(Some(ClipboardImage {
                width,
                height,
                rgba: image.bytes.into_owned(),
            }))
        }
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(PlatformError::transient("clipboard_read_failed", error)),
    }
}

pub fn read_file_paths() -> PlatformResult<Vec<PathBuf>> {
    match open()?.get().file_list() {
        Ok(paths) => Ok(paths),
        Err(arboard::Error::ContentNotAvailable) => Ok(Vec::new()),
        Err(error) => Err(PlatformError::transient("clipboard_read_failed", error)),
    }
}

pub fn write_text(value: impl Into<String>) -> PlatformResult<()> {
    open()?
        .set_text(value.into())
        .map_err(|error| PlatformError::transient("clipboard_write_failed", error))
}

fn open() -> PlatformResult<arboard::Clipboard> {
    arboard::Clipboard::new().map_err(|error| PlatformError::transient("clipboard_open_failed", error))
}

fn invalid_image(dimension: &str) -> PlatformError {
    PlatformError::rejected(
        "clipboard_image_invalid",
        format!("clipboard image {dimension} is unsupported"),
    )
}

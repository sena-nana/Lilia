use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::application::DesktopApplicationConfig;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopHostContext {
    pub home: PathBuf,
    pub instance_identity: String,
}

impl From<&DesktopApplicationConfig> for DesktopHostContext {
    fn from(config: &DesktopApplicationConfig) -> Self {
        Self {
            home: config.home().to_path_buf(),
            instance_identity: config.instance_identity().to_owned(),
        }
    }
}

pub trait DesktopHost: Send + Sync {
    /// Executes one OS integration request.
    ///
    /// Credential and single-instance implementations must scope their storage
    /// and registration to `context.instance_identity`.
    fn execute(
        &self,
        context: &DesktopHostContext,
        action: DesktopHostAction,
    ) -> Result<DesktopHostResult, DesktopHostError>;

    fn execute_update(
        &self,
        context: &DesktopHostContext,
        action: DesktopUpdateAction,
        _on_download_progress: &mut dyn FnMut(Option<f32>),
    ) -> Result<DesktopHostResult, DesktopHostError> {
        self.execute(context, DesktopHostAction::Update(action))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopHostAction {
    Window(DesktopWindowAction),
    FileDialog(DesktopFileDialogRequest),
    Tray(DesktopTrayAction),
    Shortcut(DesktopShortcutAction),
    Credential(DesktopCredentialAction),
    ReadClipboardText,
    ReadClipboardImage,
    ReadClipboardFilePaths,
    WriteClipboardText(String),
    OpenPath(PathBuf),
    OpenTerminal(PathBuf),
    OpenCodeEditor(PathBuf),
    OpenExternal(String),
    SetSystemAwake { active: bool, reason: String },
    NotifySecondInstance(DesktopSingleInstanceRequest),
    ForwardCli(DesktopCliRequest),
    Update(DesktopUpdateAction),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopWindowAction {
    SetVisible { window_id: String, visible: bool },
    Focus { window_id: String },
    Close { window_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopFileDialogRequest {
    pub dialog_id: String,
    pub title: Option<String>,
    pub initial_directory: Option<PathBuf>,
    pub filters: Vec<DesktopFileFilter>,
    pub select_directories: bool,
    pub multiple: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopFileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopTrayAction {
    Set {
        tooltip: Option<String>,
        items: Vec<DesktopTrayItem>,
    },
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopTrayItem {
    Action {
        id: String,
        label: String,
        enabled: bool,
        checked: bool,
    },
    Separator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopShortcutAction {
    Register { id: String, accelerator: String },
    Unregister { id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopCredentialAction {
    Read {
        key: String,
    },
    Write {
        key: String,
        secret: DesktopSecret,
    },
    Delete {
        key: String,
    },
    ImportConfirmed {
        source_instance_identity: String,
        entries: Vec<DesktopCredentialImportEntry>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCredentialImportEntry {
    pub source_service: String,
    pub source_account: String,
    pub target_key: String,
}

pub use lilia_contracts::Secret as DesktopSecret;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopSingleInstanceRequest {
    pub arguments: Vec<String>,
    pub working_directory: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopCliRequest {
    pub request_id: String,
    pub arguments: Vec<String>,
    pub working_directory: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopCliResult {
    pub accepted: bool,
    pub exit_code: Option<i32>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopUpdateAction {
    Check { channel: String },
    Install { version: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopUpdateResult {
    UpToDate,
    Available {
        version: String,
        notes: Option<String>,
    },
    InstallerLaunched {
        version: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopHostResult {
    Completed,
    FileDialogSelection(Vec<PathBuf>),
    Credential(Option<DesktopSecret>),
    CredentialImport(HostCredentialImportResult),
    ClipboardText(Option<String>),
    ClipboardImage(Option<DesktopClipboardImage>),
    ClipboardFilePaths(Vec<PathBuf>),
    SecondInstanceAccepted(bool),
    Cli(DesktopCliResult),
    Update(DesktopUpdateResult),
}

#[derive(Clone, PartialEq, Eq)]
pub struct DesktopClipboardImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl fmt::Debug for DesktopClipboardImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopClipboardImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("rgba_bytes", &self.rgba.len())
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCredentialImportResult {
    pub imported: u32,
    pub skipped: u32,
    pub failed: u32,
    pub available_target_keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct DesktopHostError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl DesktopHostError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_debug_output_never_contains_secret_bytes() {
        let action = DesktopHostAction::Credential(DesktopCredentialAction::Write {
            key: "provider-key".into(),
            secret: DesktopSecret::new(b"do-not-log".to_vec()),
        });

        let debug = format!("{action:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("do-not-log"));
    }

    #[test]
    fn clipboard_image_debug_output_reports_shape_without_pixel_payload() {
        let image = DesktopClipboardImage {
            width: 1,
            height: 1,
            rgba: vec![17, 34, 51, 68],
        };

        let debug = format!("{image:?}");
        assert!(debug.contains("rgba_bytes: 4"));
        assert!(!debug.contains("17, 34, 51, 68"));
    }
}

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::DesktopApplicationConfig;

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
    WriteClipboardText(String),
    OpenPath(PathBuf),
    OpenTerminal(PathBuf),
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
        keys: Vec<String>,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub struct DesktopSecret(Vec<u8>);

impl DesktopSecret {
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self(secret.into())
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

impl fmt::Debug for DesktopSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DesktopSecret([REDACTED])")
    }
}

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

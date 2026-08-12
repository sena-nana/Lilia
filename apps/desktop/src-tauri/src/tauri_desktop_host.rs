use std::path::PathBuf;

use lilia_desktop_application::{
    DesktopCredentialAction, DesktopFileDialogRequest, DesktopHost, DesktopHostAction,
    DesktopHostContext, DesktopHostError, DesktopHostResult, DesktopSecret, DesktopWindowAction,
};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_opener::OpenerExt;

#[derive(Clone)]
pub struct TauriDesktopHost {
    app: AppHandle,
}

impl TauriDesktopHost {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl DesktopHost for TauriDesktopHost {
    fn execute(
        &self,
        context: &DesktopHostContext,
        action: DesktopHostAction,
    ) -> Result<DesktopHostResult, DesktopHostError> {
        match action {
            DesktopHostAction::Window(action) => self.window(action),
            DesktopHostAction::FileDialog(request) => self.file_dialog(request),
            DesktopHostAction::Credential(action) => credential(context, action),
            DesktopHostAction::ReadClipboardText => read_clipboard_text(),
            DesktopHostAction::WriteClipboardText(value) => write_clipboard_text(value),
            DesktopHostAction::OpenPath(path) => self.open_path(path),
            DesktopHostAction::OpenExternal(uri) => self.open_external(uri),
            DesktopHostAction::SetSystemAwake { active, .. } => {
                crate::system_wake::set_system_awake(active)
                    .map_err(|error| host_error("system_awake_failed", error, true))?;
                Ok(DesktopHostResult::Completed)
            }
            _ => Err(host_error(
                "tauri_host_capability_unavailable",
                "this host capability has not been routed through DesktopApplication yet",
                false,
            )),
        }
    }
}

impl TauriDesktopHost {
    fn window(&self, action: DesktopWindowAction) -> Result<DesktopHostResult, DesktopHostError> {
        let window_id = match &action {
            DesktopWindowAction::SetVisible { window_id, .. }
            | DesktopWindowAction::Focus { window_id }
            | DesktopWindowAction::Close { window_id } => window_id,
        };
        let window = self.app.get_webview_window(window_id).ok_or_else(|| {
            host_error(
                "window_not_found",
                format!("window `{window_id}` does not exist"),
                false,
            )
        })?;
        let result = match action {
            DesktopWindowAction::SetVisible { visible, .. } => {
                if visible {
                    window.show()
                } else {
                    window.hide()
                }
            }
            DesktopWindowAction::Focus { .. } => window
                .show()
                .and_then(|_| window.unminimize())
                .and_then(|_| window.set_focus()),
            DesktopWindowAction::Close { .. } => window.close(),
        };
        result.map_err(|error| host_error("window_action_failed", error.to_string(), true))?;
        Ok(DesktopHostResult::Completed)
    }

    fn file_dialog(
        &self,
        request: DesktopFileDialogRequest,
    ) -> Result<DesktopHostResult, DesktopHostError> {
        let mut dialog = self.app.dialog().file();
        if let Some(title) = request.title {
            dialog = dialog.set_title(title);
        }
        if let Some(initial_directory) = request.initial_directory {
            dialog = dialog.set_directory(initial_directory);
        }
        for filter in request.filters {
            let extensions = filter
                .extensions
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            dialog = dialog.add_filter(filter.name, &extensions);
        }
        let selected = match (request.select_directories, request.multiple) {
            (true, true) => dialog.blocking_pick_folders().unwrap_or_default(),
            (true, false) => dialog.blocking_pick_folder().into_iter().collect(),
            (false, true) => dialog.blocking_pick_files().unwrap_or_default(),
            (false, false) => dialog.blocking_pick_file().into_iter().collect(),
        };
        let paths = selected
            .into_iter()
            .map(file_path)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DesktopHostResult::FileDialogSelection(paths))
    }

    fn open_path(&self, path: PathBuf) -> Result<DesktopHostResult, DesktopHostError> {
        let path = path.canonicalize().map_err(|error| {
            host_error(
                "path_open_invalid",
                format!("failed to resolve {}: {error}", path.display()),
                false,
            )
        })?;
        self.app
            .opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|error| host_error("path_open_failed", error.to_string(), true))?;
        Ok(DesktopHostResult::Completed)
    }

    fn open_external(&self, uri: String) -> Result<DesktopHostResult, DesktopHostError> {
        let uri = validated_external_uri(&uri)?;
        self.app
            .opener()
            .open_url(uri, None::<&str>)
            .map_err(|error| host_error("external_open_failed", error.to_string(), true))?;
        Ok(DesktopHostResult::Completed)
    }
}

fn file_path(path: FilePath) -> Result<PathBuf, DesktopHostError> {
    path.into_path()
        .map_err(|error| host_error("dialog_path_invalid", error.to_string(), false))
}

fn validated_external_uri(uri: &str) -> Result<String, DesktopHostError> {
    let uri = uri.trim();
    if uri.is_empty() || uri.chars().any(char::is_control) {
        return Err(host_error(
            "external_uri_invalid",
            "external URI must be a non-empty HTTP or HTTPS URL",
            false,
        ));
    }
    let scheme = uri
        .split_once(':')
        .map(|(scheme, _)| scheme.to_ascii_lowercase());
    if !matches!(scheme.as_deref(), Some("http" | "https")) {
        return Err(host_error(
            "external_uri_unsupported",
            "only HTTP and HTTPS links can be opened externally",
            false,
        ));
    }
    Ok(uri.to_owned())
}

fn credential(
    context: &DesktopHostContext,
    action: DesktopCredentialAction,
) -> Result<DesktopHostResult, DesktopHostError> {
    match action {
        DesktopCredentialAction::Read { key } => {
            match credential_entry(context, &key)?.get_secret() {
                Ok(value) => Ok(DesktopHostResult::Credential(Some(DesktopSecret::new(
                    value,
                )))),
                Err(keyring::Error::NoEntry) => Ok(DesktopHostResult::Credential(None)),
                Err(error) => Err(host_error(
                    "credential_read_failed",
                    error.to_string(),
                    true,
                )),
            }
        }
        DesktopCredentialAction::Write { key, secret } => {
            credential_entry(context, &key)?
                .set_secret(secret.expose())
                .map_err(|error| host_error("credential_write_failed", error.to_string(), true))?;
            Ok(DesktopHostResult::Completed)
        }
        DesktopCredentialAction::Delete { key } => {
            match credential_entry(context, &key)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(DesktopHostResult::Completed),
                Err(error) => Err(host_error(
                    "credential_delete_failed",
                    error.to_string(),
                    true,
                )),
            }
        }
        DesktopCredentialAction::ImportConfirmed { .. } => Err(host_error(
            "credential_import_manifest_required",
            "credential import requires an explicit source credential manifest",
            false,
        )),
    }
}

fn credential_entry(
    context: &DesktopHostContext,
    key: &str,
) -> Result<keyring::Entry, DesktopHostError> {
    if key.trim().is_empty() {
        return Err(host_error(
            "credential_key_invalid",
            "credential key must not be empty",
            false,
        ));
    }
    keyring::Entry::new(&context.instance_identity, key)
        .map_err(|error| host_error("credential_entry_failed", error.to_string(), false))
}

#[cfg(windows)]
fn read_clipboard_text() -> Result<DesktopHostResult, DesktopHostError> {
    let value = clipboard_win::get_clipboard(clipboard_win::formats::Unicode)
        .map_err(|error| host_error("clipboard_read_failed", error.to_string(), true))?;
    Ok(DesktopHostResult::ClipboardText(Some(value)))
}

#[cfg(not(windows))]
fn read_clipboard_text() -> Result<DesktopHostResult, DesktopHostError> {
    Err(host_error(
        "clipboard_unavailable",
        "the Tauri DesktopHost clipboard adapter is currently Windows-only",
        false,
    ))
}

#[cfg(windows)]
fn write_clipboard_text(value: String) -> Result<DesktopHostResult, DesktopHostError> {
    clipboard_win::set_clipboard(clipboard_win::formats::Unicode, value)
        .map_err(|error| host_error("clipboard_write_failed", error.to_string(), true))?;
    Ok(DesktopHostResult::Completed)
}

#[cfg(not(windows))]
fn write_clipboard_text(_value: String) -> Result<DesktopHostResult, DesktopHostError> {
    Err(host_error(
        "clipboard_unavailable",
        "the Tauri DesktopHost clipboard adapter is currently Windows-only",
        false,
    ))
}

fn host_error(
    code: impl Into<String>,
    message: impl Into<String>,
    retryable: bool,
) -> DesktopHostError {
    DesktopHostError::new(code, message, retryable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_uri_validation_rejects_local_and_executable_schemes() {
        assert_eq!(
            validated_external_uri(" HTTPS://example.com/docs ").unwrap(),
            "HTTPS://example.com/docs"
        );
        assert_eq!(
            validated_external_uri("file:///C:/Windows/System32/cmd.exe")
                .unwrap_err()
                .code,
            "external_uri_unsupported"
        );
        assert_eq!(
            validated_external_uri("javascript:alert(1)")
                .unwrap_err()
                .code,
            "external_uri_unsupported"
        );
    }

    #[test]
    fn credential_keys_are_validated_before_keyring_access() {
        let context = DesktopHostContext {
            home: "C:/lilia".into(),
            instance_identity: "liliacode.test".to_owned(),
        };
        let error = match credential_entry(&context, " ") {
            Ok(_) => panic!("empty credential key unexpectedly accepted"),
            Err(error) => error,
        };
        assert_eq!(error.code, "credential_key_invalid");
    }
}

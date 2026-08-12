use lilia_desktop_application::{
    DesktopClipboardImage, DesktopCredentialAction, DesktopFileDialogRequest, DesktopHost,
    DesktopHostAction, DesktopHostContext, DesktopHostError, DesktopHostResult, DesktopSecret,
};

#[derive(Debug, Default)]
pub struct PreviewHost;

impl DesktopHost for PreviewHost {
    fn execute(
        &self,
        context: &DesktopHostContext,
        action: DesktopHostAction,
    ) -> Result<DesktopHostResult, DesktopHostError> {
        match action {
            DesktopHostAction::FileDialog(request) => open_file_dialog(request),
            DesktopHostAction::Credential(action) => credential(context, action),
            DesktopHostAction::ReadClipboardText => {
                let mut clipboard = arboard::Clipboard::new().map_err(|error| {
                    host_error("clipboard_open_failed", error.to_string(), true)
                })?;
                match clipboard.get_text() {
                    Ok(value) => Ok(DesktopHostResult::ClipboardText(Some(value))),
                    Err(arboard::Error::ContentNotAvailable) => {
                        Ok(DesktopHostResult::ClipboardText(None))
                    }
                    Err(error) => Err(host_error("clipboard_read_failed", error.to_string(), true)),
                }
            }
            DesktopHostAction::ReadClipboardImage => {
                let mut clipboard = arboard::Clipboard::new().map_err(|error| {
                    host_error("clipboard_open_failed", error.to_string(), true)
                })?;
                match clipboard.get_image() {
                    Ok(image) => {
                        let width = u32::try_from(image.width).map_err(|_| {
                            host_error(
                                "clipboard_image_invalid",
                                "clipboard image width is unsupported".to_owned(),
                                false,
                            )
                        })?;
                        let height = u32::try_from(image.height).map_err(|_| {
                            host_error(
                                "clipboard_image_invalid",
                                "clipboard image height is unsupported".to_owned(),
                                false,
                            )
                        })?;
                        Ok(DesktopHostResult::ClipboardImage(Some(
                            DesktopClipboardImage {
                                width,
                                height,
                                rgba: image.bytes.into_owned(),
                            },
                        )))
                    }
                    Err(arboard::Error::ContentNotAvailable) => {
                        Ok(DesktopHostResult::ClipboardImage(None))
                    }
                    Err(error) => Err(host_error("clipboard_read_failed", error.to_string(), true)),
                }
            }
            DesktopHostAction::WriteClipboardText(value) => {
                let mut clipboard = arboard::Clipboard::new().map_err(|error| {
                    host_error("clipboard_open_failed", error.to_string(), true)
                })?;
                clipboard.set_text(value).map_err(|error| {
                    host_error("clipboard_write_failed", error.to_string(), true)
                })?;
                Ok(DesktopHostResult::Completed)
            }
            DesktopHostAction::OpenPath(path) => open_path(path),
            DesktopHostAction::OpenTerminal(path) => open_terminal(path),
            DesktopHostAction::OpenExternal(uri) => open_external(uri),
            DesktopHostAction::SetSystemAwake { active, .. } => set_system_awake(active),
            DesktopHostAction::Update(action) => crate::updater::execute(context, action),
            _ => Err(DesktopHostError::new(
                "native_preview_host_unavailable",
                "this host capability is not available in Native Preview",
                false,
            )),
        }
    }
}

#[cfg(all(windows, not(test)))]
fn set_system_awake(active: bool) -> Result<DesktopHostResult, DesktopHostError> {
    use windows::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };

    let flags = if active {
        ES_CONTINUOUS | ES_SYSTEM_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    let previous = unsafe { SetThreadExecutionState(flags) };
    if previous.0 == 0 {
        Err(host_error(
            "system_awake_failed",
            "SetThreadExecutionState failed".to_owned(),
            true,
        ))
    } else {
        Ok(DesktopHostResult::Completed)
    }
}

#[cfg(any(not(windows), test))]
fn set_system_awake(_active: bool) -> Result<DesktopHostResult, DesktopHostError> {
    Ok(DesktopHostResult::Completed)
}

fn open_path(path: std::path::PathBuf) -> Result<DesktopHostResult, DesktopHostError> {
    let path = path
        .canonicalize()
        .map(platform_compatible_path)
        .map_err(|error| {
            host_error(
                "path_open_invalid",
                format!("failed to resolve {}: {error}", path.display()),
                false,
            )
        })?;
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(&path).spawn().map_err(|error| {
        host_error(
            "path_open_failed",
            format!("failed to open {}: {error}", path.display()),
            true,
        )
    })?;
    Ok(DesktopHostResult::Completed)
}

fn platform_compatible_path(path: std::path::PathBuf) -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        let value = path.to_string_lossy();
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return std::path::PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return std::path::PathBuf::from(rest);
        }
    }
    path
}

fn open_terminal(path: std::path::PathBuf) -> Result<DesktopHostResult, DesktopHostError> {
    let path = validated_terminal_directory(path)?;
    #[cfg(target_os = "windows")]
    {
        match std::process::Command::new("wt.exe")
            .arg("-d")
            .arg(&path)
            .spawn()
        {
            Ok(_) => return Ok(DesktopHostResult::Completed),
            Err(windows_terminal_error) => {
                std::process::Command::new("powershell.exe")
                    .args(["-NoExit", "-Command", "Set-Location", "-LiteralPath"])
                    .arg(&path)
                    .spawn()
                    .map_err(|powershell_error| {
                        host_error(
                            "terminal_open_failed",
                            format!(
                                "failed to open a terminal for {}: Windows Terminal: {windows_terminal_error}; PowerShell: {powershell_error}",
                                path.display()
                            ),
                            true,
                        )
                    })?;
            }
        }
    }
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .args(["-a", "Terminal"])
        .arg(&path)
        .spawn()
        .map_err(|error| {
            host_error(
                "terminal_open_failed",
                format!("failed to open a terminal for {}: {error}", path.display()),
                true,
            )
        })?;
    #[cfg(all(unix, not(target_os = "macos")))]
    std::process::Command::new("x-terminal-emulator")
        .arg("--working-directory")
        .arg(&path)
        .spawn()
        .map_err(|error| {
            host_error(
                "terminal_open_failed",
                format!("failed to open a terminal for {}: {error}", path.display()),
                true,
            )
        })?;
    Ok(DesktopHostResult::Completed)
}

fn validated_terminal_directory(
    path: std::path::PathBuf,
) -> Result<std::path::PathBuf, DesktopHostError> {
    let path = path
        .canonicalize()
        .map(platform_compatible_path)
        .map_err(|error| {
            host_error(
                "terminal_working_directory_invalid",
                format!("failed to resolve {}: {error}", path.display()),
                false,
            )
        })?;
    if !path.is_dir() {
        return Err(host_error(
            "terminal_working_directory_invalid",
            format!("{} is not a directory", path.display()),
            false,
        ));
    }
    Ok(path)
}

fn open_external(uri: String) -> Result<DesktopHostResult, DesktopHostError> {
    let uri = validated_external_uri(&uri)?;
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(&uri).spawn().map_err(|error| {
        host_error(
            "external_open_failed",
            format!("failed to open `{uri}`: {error}"),
            true,
        )
    })?;
    Ok(DesktopHostResult::Completed)
}

fn validated_external_uri(uri: &str) -> Result<String, DesktopHostError> {
    let uri = uri.trim();
    if uri.is_empty() || uri.chars().any(char::is_control) {
        return Err(host_error(
            "external_uri_invalid",
            "external URI must be a non-empty HTTP or HTTPS URL".into(),
            false,
        ));
    }
    let scheme = uri
        .split_once(':')
        .map(|(scheme, _)| scheme.to_ascii_lowercase());
    if !matches!(scheme.as_deref(), Some("http" | "https")) {
        return Err(host_error(
            "external_uri_unsupported",
            "Native Preview only opens HTTP and HTTPS links".into(),
            false,
        ));
    }
    Ok(uri.to_owned())
}

fn open_file_dialog(
    request: DesktopFileDialogRequest,
) -> Result<DesktopHostResult, DesktopHostError> {
    #[cfg(debug_assertions)]
    if let Some(environment) = match request.dialog_id.as_str() {
        "project-workspace" => Some("LILIA_NATIVE_AGENT_DEBUG_WORKSPACE"),
        "project-clone-parent" => Some("LILIA_NATIVE_AGENT_DEBUG_CLONE_PARENT"),
        _ => None,
    } {
        if let Some(path) = std::env::var_os(environment).filter(|path| !path.is_empty()) {
            return Ok(DesktopHostResult::FileDialogSelection(vec![
                std::path::PathBuf::from(path),
            ]));
        }
    }

    let mut dialog = rfd::FileDialog::new();
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
        (true, true) => dialog.pick_folders().unwrap_or_default(),
        (true, false) => dialog.pick_folder().into_iter().collect(),
        (false, true) => dialog.pick_files().unwrap_or_default(),
        (false, false) => dialog.pick_file().into_iter().collect(),
    };
    Ok(DesktopHostResult::FileDialogSelection(selected))
}

fn credential(
    context: &DesktopHostContext,
    action: DesktopCredentialAction,
) -> Result<DesktopHostResult, DesktopHostError> {
    match action {
        DesktopCredentialAction::Read { key } => {
            let entry = credential_entry(context, &key)?;
            match entry.get_secret() {
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
        DesktopCredentialAction::ImportConfirmed {
            source_instance_identity,
            keys,
        } => import_confirmed_credentials(context, &source_instance_identity, &keys),
    }
}

fn import_confirmed_credentials(
    context: &DesktopHostContext,
    source_instance_identity: &str,
    keys: &[String],
) -> Result<DesktopHostResult, DesktopHostError> {
    if source_instance_identity.trim().is_empty()
        || source_instance_identity.chars().any(char::is_control)
        || source_instance_identity == context.instance_identity
        || keys.len() > 4096
        || !keys.windows(2).all(|pair| pair[0] < pair[1])
        || !keys.iter().all(|key| valid_import_credential_key(key))
    {
        return Err(host_error(
            "credential_import_manifest_invalid",
            "credential import manifest is invalid".to_owned(),
            false,
        ));
    }

    let mut result = lilia_desktop_application::HostCredentialImportResult {
        imported: 0,
        skipped: 0,
        failed: 0,
    };
    for key in keys {
        let source = keyring::Entry::new(source_instance_identity, key);
        let target = keyring::Entry::new(&context.instance_identity, key);
        let (Ok(source), Ok(target)) = (source, target) else {
            result.failed += 1;
            continue;
        };
        let secret = match source.get_secret() {
            Ok(secret) => secret,
            Err(keyring::Error::NoEntry) => {
                result.failed += 1;
                continue;
            }
            Err(_) => {
                result.failed += 1;
                continue;
            }
        };
        match target.get_secret() {
            Ok(existing) if existing == secret => result.skipped += 1,
            Ok(_) => result.failed += 1,
            Err(keyring::Error::NoEntry) => match target.set_secret(&secret) {
                Ok(()) => result.imported += 1,
                Err(_) => result.failed += 1,
            },
            Err(_) => result.failed += 1,
        }
    }
    Ok(DesktopHostResult::CredentialImport(result))
}

fn valid_import_credential_key(key: &str) -> bool {
    let Some(secret_id) = key.strip_prefix("agentkit.") else {
        return false;
    };
    !secret_id.is_empty()
        && key.len() <= 256
        && secret_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
        })
}

fn credential_entry(
    context: &DesktopHostContext,
    key: &str,
) -> Result<keyring::Entry, DesktopHostError> {
    if key.trim().is_empty() {
        return Err(DesktopHostError::new(
            "credential_key_invalid",
            "credential key must not be empty",
            false,
        ));
    }
    keyring::Entry::new(&context.instance_identity, key)
        .map_err(|error| host_error("credential_entry_failed", error.to_string(), false))
}

fn host_error(code: &'static str, message: String, retryable: bool) -> DesktopHostError {
    DesktopHostError::new(code, message, retryable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lilia_desktop_application::DesktopUpdateAction;

    #[cfg(windows)]
    #[test]
    fn confirmed_credential_import_copies_only_manifest_keys_without_overwrite() {
        let suffix = uuid::Uuid::new_v4();
        let source_identity = format!("liliacode.import-source.{suffix}");
        let target_identity = format!("liliacode.native-preview.import-target.{suffix}");
        let keys = vec![
            "agentkit.existing-target".to_owned(),
            "agentkit.missing-source".to_owned(),
            "agentkit.new-target".to_owned(),
            "agentkit.same-target".to_owned(),
        ];
        let source_existing = keyring::Entry::new(&source_identity, &keys[0]).unwrap();
        let source_new = keyring::Entry::new(&source_identity, &keys[2]).unwrap();
        let target_existing = keyring::Entry::new(&target_identity, &keys[0]).unwrap();
        let target_new = keyring::Entry::new(&target_identity, &keys[2]).unwrap();
        let source_same = keyring::Entry::new(&source_identity, &keys[3]).unwrap();
        let target_same = keyring::Entry::new(&target_identity, &keys[3]).unwrap();
        source_existing.set_secret(b"source-existing").unwrap();
        source_new.set_secret(b"source-new").unwrap();
        target_existing.set_secret(b"keep-target").unwrap();
        source_same.set_secret(b"same-secret").unwrap();
        target_same.set_secret(b"same-secret").unwrap();

        let result = credential(
            &DesktopHostContext {
                home: "C:/preview".into(),
                instance_identity: target_identity.clone(),
            },
            DesktopCredentialAction::ImportConfirmed {
                source_instance_identity: source_identity.clone(),
                keys: keys.clone(),
            },
        )
        .unwrap();
        assert_eq!(
            result,
            DesktopHostResult::CredentialImport(
                lilia_desktop_application::HostCredentialImportResult {
                    imported: 1,
                    skipped: 1,
                    failed: 2,
                }
            )
        );
        assert_eq!(target_existing.get_secret().unwrap(), b"keep-target");
        assert_eq!(target_new.get_secret().unwrap(), b"source-new");
        assert_eq!(target_same.get_secret().unwrap(), b"same-secret");

        for identity in [&source_identity, &target_identity] {
            for key in &keys {
                if let Ok(entry) = keyring::Entry::new(identity, key) {
                    let _ = entry.delete_credential();
                }
            }
        }
    }

    #[test]
    fn updater_without_an_embedded_public_key_fails_without_claiming_success() {
        if option_env!("LILIA_NATIVE_UPDATER_PUBKEY").is_some() {
            return;
        }
        let context = DesktopHostContext {
            home: "C:/preview".into(),
            instance_identity: "liliacode.native-preview.test".to_owned(),
        };
        let error = PreviewHost
            .execute(
                &context,
                DesktopHostAction::Update(DesktopUpdateAction::Check {
                    channel: "preview".to_owned(),
                }),
            )
            .unwrap_err();

        assert_eq!(error.code, "native_updater_unconfigured");
    }

    #[test]
    fn empty_credential_keys_are_rejected_before_os_access() {
        let context = DesktopHostContext {
            home: "C:/preview".into(),
            instance_identity: "liliacode.native-preview.test".to_owned(),
        };
        let error = credential(
            &context,
            DesktopCredentialAction::Read {
                key: " ".to_owned(),
            },
        )
        .unwrap_err();

        assert_eq!(error.code, "credential_key_invalid");
    }

    #[test]
    fn external_links_accept_web_urls_and_reject_local_or_executable_schemes() {
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
    fn terminal_working_directory_must_be_an_existing_directory() {
        let directory = std::env::temp_dir().join(format!(
            "lilia-native-terminal-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let resolved = validated_terminal_directory(directory.clone()).unwrap();
        assert!(resolved.is_dir());

        let file = directory.join("file.txt");
        std::fs::write(&file, b"not a directory").unwrap();
        assert_eq!(
            validated_terminal_directory(file).unwrap_err().code,
            "terminal_working_directory_invalid"
        );
        assert_eq!(
            validated_terminal_directory(directory.join("missing"))
                .unwrap_err()
                .code,
            "terminal_working_directory_invalid"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}

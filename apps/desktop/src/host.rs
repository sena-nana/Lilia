use crate::application::{
    DesktopClipboardImage, DesktopCredentialAction, DesktopCredentialImportEntry,
    DesktopFileDialogRequest, DesktopHost, DesktopHostAction, DesktopHostContext, DesktopHostError,
    DesktopHostResult, DesktopSecret, DesktopUpdateAction,
};
use lilia_platform::{clipboard, dialog, launcher, power, CredentialEntry, PlatformError};

const LEGACY_AI_CREDENTIAL_SERVICE: &str = "com.lilia.desktop.ai";
const LEGACY_ASSISTANT_AI_ACCOUNT: &str = "assistant-ai";
const LEGACY_GITHUB_CREDENTIAL_SERVICE: &str = "com.lilia.desktop.github";
const ASSISTANT_AI_TARGET_KEY: &str = "assistant-ai";
const GITHUB_TARGET_KEY: &str = "github.oauth.token";

#[derive(Debug, Default)]
pub struct NativeDesktopHost;

impl DesktopHost for NativeDesktopHost {
    fn execute(
        &self,
        context: &DesktopHostContext,
        action: DesktopHostAction,
    ) -> Result<DesktopHostResult, DesktopHostError> {
        match action {
            DesktopHostAction::FileDialog(request) => Ok(DesktopHostResult::FileDialogSelection(
                open_file_dialog(request),
            )),
            DesktopHostAction::Credential(action) => return credential(context, action),
            DesktopHostAction::ReadClipboardText => {
                clipboard::read_text().map(DesktopHostResult::ClipboardText)
            }
            DesktopHostAction::ReadClipboardImage => clipboard::read_image().map(|image| {
                DesktopHostResult::ClipboardImage(image.map(|image| DesktopClipboardImage {
                    width: image.width,
                    height: image.height,
                    rgba: image.rgba,
                }))
            }),
            DesktopHostAction::ReadClipboardFilePaths => {
                clipboard::read_file_paths().map(DesktopHostResult::ClipboardFilePaths)
            }
            DesktopHostAction::WriteClipboardText(value) => {
                clipboard::write_text(value).map(|()| DesktopHostResult::Completed)
            }
            DesktopHostAction::OpenPath(path) => {
                launcher::open_path(path).map(|()| DesktopHostResult::Completed)
            }
            DesktopHostAction::OpenTerminal(path) => {
                launcher::open_terminal(path).map(|()| DesktopHostResult::Completed)
            }
            DesktopHostAction::OpenCodeEditor(path) => {
                launcher::open_code_editor(path).map(|()| DesktopHostResult::Completed)
            }
            DesktopHostAction::OpenExternal(uri) => {
                launcher::open_external(&uri).map(|()| DesktopHostResult::Completed)
            }
            DesktopHostAction::SetSystemAwake { active, .. } => {
                power::set_system_awake(active).map(|()| DesktopHostResult::Completed)
            }
            DesktopHostAction::Update(action) => return crate::updater::execute(context, action),
            _ => {
                return Err(DesktopHostError::new(
                    "native_desktop_host_unavailable",
                    "this host capability is not available in LiliaCode",
                    false,
                ))
            }
        }
        .map_err(host_error)
    }

    fn execute_update(
        &self,
        context: &DesktopHostContext,
        action: DesktopUpdateAction,
        on_download_progress: &mut dyn FnMut(Option<f32>),
    ) -> Result<DesktopHostResult, DesktopHostError> {
        crate::updater::execute_with_progress(context, action, on_download_progress)
    }
}

fn open_file_dialog(request: DesktopFileDialogRequest) -> Vec<std::path::PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(environment) = match request.dialog_id.as_str() {
        "project-workspace" => Some("LILIA_AGENT_DEBUG_WORKSPACE"),
        "project-clone-parent" => Some("LILIA_AGENT_DEBUG_CLONE_PARENT"),
        _ => None,
    } {
        if let Some(path) = std::env::var_os(environment).filter(|path| !path.is_empty()) {
            return vec![std::path::PathBuf::from(path)];
        }
    }

    dialog::pick(dialog::FileDialogRequest {
        title: request.title,
        initial_directory: request.initial_directory,
        filters: request
            .filters
            .into_iter()
            .map(|filter| dialog::FileFilter {
                name: filter.name,
                extensions: filter.extensions,
            })
            .collect(),
        select_directories: request.select_directories,
        multiple: request.multiple,
    })
}

fn credential(
    context: &DesktopHostContext,
    action: DesktopCredentialAction,
) -> Result<DesktopHostResult, DesktopHostError> {
    match action {
        DesktopCredentialAction::Read { key } => entry(&context.instance_identity, &key)?
            .read()
            .map(|secret| DesktopHostResult::Credential(secret.map(DesktopSecret::new)))
            .map_err(host_error),
        DesktopCredentialAction::Write { key, secret } => entry(&context.instance_identity, &key)?
            .write(secret.expose())
            .map(|()| DesktopHostResult::Completed)
            .map_err(host_error),
        DesktopCredentialAction::Delete { key } => entry(&context.instance_identity, &key)?
            .delete()
            .map(|()| DesktopHostResult::Completed)
            .map_err(host_error),
        DesktopCredentialAction::ImportConfirmed {
            source_instance_identity,
            entries,
        } => import_confirmed_credentials(context, &source_instance_identity, &entries),
    }
}

fn entry(service: &str, key: &str) -> Result<CredentialEntry, DesktopHostError> {
    CredentialEntry::open(service, key).map_err(host_error)
}

fn import_confirmed_credentials(
    context: &DesktopHostContext,
    source_instance_identity: &str,
    entries: &[DesktopCredentialImportEntry],
) -> Result<DesktopHostResult, DesktopHostError> {
    if source_instance_identity.trim().is_empty()
        || source_instance_identity.chars().any(char::is_control)
        || source_instance_identity == context.instance_identity
        || entries.len() > 4096
        || !entries
            .windows(2)
            .all(|pair| import_entry_key(&pair[0]) < import_entry_key(&pair[1]))
        || !entries
            .iter()
            .all(|entry| valid_import_credential_entry(source_instance_identity, entry))
    {
        return Err(DesktopHostError::new(
            "credential_import_manifest_invalid",
            "credential import manifest is invalid",
            false,
        ));
    }

    let mut result = crate::application::HostCredentialImportResult {
        imported: 0,
        skipped: 0,
        failed: 0,
        available_target_keys: Vec::new(),
    };
    for entry in entries {
        let source = CredentialEntry::open(&entry.source_service, &entry.source_account);
        let target = CredentialEntry::open(&context.instance_identity, &entry.target_key);
        let (Ok(source), Ok(target)) = (source, target) else {
            result.failed += 1;
            continue;
        };
        let Ok(Some(secret)) = source.read() else {
            result.failed += 1;
            continue;
        };
        match target.read() {
            Ok(Some(existing)) if existing == secret => {
                result.skipped += 1;
                result.available_target_keys.push(entry.target_key.clone());
            }
            Ok(Some(_)) => result.failed += 1,
            Ok(None) => match target.write(&secret) {
                Ok(()) => {
                    result.imported += 1;
                    result.available_target_keys.push(entry.target_key.clone());
                }
                Err(_) => result.failed += 1,
            },
            Err(_) => result.failed += 1,
        }
    }
    Ok(DesktopHostResult::CredentialImport(result))
}

fn import_entry_key(entry: &DesktopCredentialImportEntry) -> (&str, &str, &str) {
    (
        entry.target_key.as_str(),
        entry.source_service.as_str(),
        entry.source_account.as_str(),
    )
}

fn valid_import_credential_entry(
    source_instance_identity: &str,
    entry: &DesktopCredentialImportEntry,
) -> bool {
    if entry.source_service == source_instance_identity {
        return entry.source_account == entry.target_key
            && valid_agentkit_import_credential_key(&entry.target_key);
    }
    if entry.source_service == LEGACY_AI_CREDENTIAL_SERVICE {
        return entry.source_account == LEGACY_ASSISTANT_AI_ACCOUNT
            && entry.target_key == ASSISTANT_AI_TARGET_KEY;
    }
    if entry.source_service == LEGACY_GITHUB_CREDENTIAL_SERVICE {
        return entry.target_key == GITHUB_TARGET_KEY && valid_github_login(&entry.source_account);
    }
    false
}

fn valid_agentkit_import_credential_key(key: &str) -> bool {
    let Some(secret_id) = key.strip_prefix("agentkit.") else {
        return false;
    };
    !secret_id.is_empty()
        && key.len() <= 256
        && secret_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
        })
}

fn valid_github_login(login: &str) -> bool {
    !login.is_empty()
        && login.len() <= 39
        && !login.starts_with('-')
        && !login.ends_with('-')
        && login
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn host_error(error: PlatformError) -> DesktopHostError {
    DesktopHostError::new(error.code, error.message, error.retryable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::DesktopUpdateAction;

    #[cfg(windows)]
    #[test]
    fn confirmed_credential_import_copies_only_manifest_keys_without_overwrite() {
        let suffix = uuid::Uuid::new_v4();
        let source_identity = format!("liliacode.import-source.{suffix}");
        let target_identity = format!("liliacode.import-target.{suffix}");
        let keys = vec![
            "agentkit.existing-target".to_owned(),
            "agentkit.missing-source".to_owned(),
            "agentkit.new-target".to_owned(),
            "agentkit.same-target".to_owned(),
        ];
        let entries = keys
            .iter()
            .map(|key| DesktopCredentialImportEntry {
                source_service: source_identity.clone(),
                source_account: key.clone(),
                target_key: key.clone(),
            })
            .collect::<Vec<_>>();
        let source_existing = CredentialEntry::open(&source_identity, &keys[0]).unwrap();
        let source_new = CredentialEntry::open(&source_identity, &keys[2]).unwrap();
        let target_existing = CredentialEntry::open(&target_identity, &keys[0]).unwrap();
        let target_new = CredentialEntry::open(&target_identity, &keys[2]).unwrap();
        let source_same = CredentialEntry::open(&source_identity, &keys[3]).unwrap();
        let target_same = CredentialEntry::open(&target_identity, &keys[3]).unwrap();
        source_existing.write(b"source-existing").unwrap();
        source_new.write(b"source-new").unwrap();
        target_existing.write(b"keep-target").unwrap();
        source_same.write(b"same-secret").unwrap();
        target_same.write(b"same-secret").unwrap();

        let result = credential(
            &DesktopHostContext {
                home: "C:/preview".into(),
                instance_identity: target_identity.clone(),
            },
            DesktopCredentialAction::ImportConfirmed {
                source_instance_identity: source_identity.clone(),
                entries,
            },
        )
        .unwrap();
        assert_eq!(
            result,
            DesktopHostResult::CredentialImport(crate::application::HostCredentialImportResult {
                imported: 1,
                skipped: 1,
                failed: 2,
                available_target_keys: vec![
                    "agentkit.new-target".to_owned(),
                    "agentkit.same-target".to_owned(),
                ],
            })
        );
        assert_eq!(target_existing.read().unwrap().unwrap(), b"keep-target");
        assert_eq!(target_new.read().unwrap().unwrap(), b"source-new");
        assert_eq!(target_same.read().unwrap().unwrap(), b"same-secret");

        for identity in [&source_identity, &target_identity] {
            for key in &keys {
                if let Ok(entry) = CredentialEntry::open(identity, key) {
                    let _ = entry.delete();
                }
            }
        }
    }

    #[test]
    fn updater_without_an_embedded_public_key_fails_without_claiming_success() {
        if option_env!("LILIA_UPDATER_PUBKEY").is_some() {
            return;
        }
        let context = DesktopHostContext {
            home: "C:/preview".into(),
            instance_identity: "liliacode.test".to_owned(),
        };
        let error = NativeDesktopHost
            .execute(
                &context,
                DesktopHostAction::Update(DesktopUpdateAction::Check {
                    channel: "stable".to_owned(),
                }),
            )
            .unwrap_err();

        assert_eq!(error.code, "native_updater_unconfigured");
    }

    #[test]
    fn empty_credential_keys_are_rejected_before_os_access() {
        let context = DesktopHostContext {
            home: "C:/preview".into(),
            instance_identity: "liliacode.test".to_owned(),
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
    fn import_manifest_accepts_only_known_legacy_source_mappings() {
        let source_identity = "liliacode";
        let valid = [
            DesktopCredentialImportEntry {
                source_service: LEGACY_AI_CREDENTIAL_SERVICE.to_owned(),
                source_account: LEGACY_ASSISTANT_AI_ACCOUNT.to_owned(),
                target_key: ASSISTANT_AI_TARGET_KEY.to_owned(),
            },
            DesktopCredentialImportEntry {
                source_service: LEGACY_GITHUB_CREDENTIAL_SERVICE.to_owned(),
                source_account: "octocat".to_owned(),
                target_key: GITHUB_TARGET_KEY.to_owned(),
            },
            DesktopCredentialImportEntry {
                source_service: source_identity.to_owned(),
                source_account: "agentkit.provider-secret".to_owned(),
                target_key: "agentkit.provider-secret".to_owned(),
            },
        ];
        assert!(valid
            .iter()
            .all(|entry| valid_import_credential_entry(source_identity, entry)));

        let arbitrary_source = DesktopCredentialImportEntry {
            source_service: "untrusted.service".to_owned(),
            source_account: "secret".to_owned(),
            target_key: ASSISTANT_AI_TARGET_KEY.to_owned(),
        };
        let remapped_agentkit = DesktopCredentialImportEntry {
            source_service: source_identity.to_owned(),
            source_account: "agentkit.source".to_owned(),
            target_key: "agentkit.target".to_owned(),
        };
        assert!(!valid_import_credential_entry(
            source_identity,
            &arbitrary_source
        ));
        assert!(!valid_import_credential_entry(
            source_identity,
            &remapped_agentkit
        ));
    }
}

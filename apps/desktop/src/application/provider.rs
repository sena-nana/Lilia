use std::sync::atomic::Ordering;

use lilia_agent::{
    CredentialDescriptorView, NativeModelRuntimeConfiguration, ProductCredentialBridge,
    ProductCredentialImportInput, ProductCredentialLoginInput, SecretStore,
    SqliteProductCredentialRegistry,
};
use mutsuki_agent_contracts::{AgentError, AgentResult, CredentialKind, CredentialRef};

use crate::application::{DesktopApplication, DesktopApplicationError, DesktopEventKind, DesktopSecret};
use crate::application::{
    DesktopCredentialAction, DesktopHost, DesktopHostAction, DesktopHostContext, DesktopHostResult,
};

use lilia_feature_provider::normalize_optional;

pub(crate) use lilia_feature_provider::AgentRuntimeSettingsState as DesktopAgentRuntimeSettingsState;
pub use lilia_feature_provider::{
    AgentRuntimeSettings as DesktopAgentRuntimeSettings,
    AgentRuntimeSettingsUpdate as DesktopAgentRuntimeSettingsUpdate,
    CapabilityLimit as DesktopCapabilityLimit,
    ProviderCapabilityView as DesktopProviderCapabilityView,
    ProviderCredentialImportInput as DesktopProviderCredentialImportInput,
    ProviderCredentialInput as DesktopProviderCredentialInput,
    ProviderCredentialKind as DesktopCredentialKind,
    ProviderCredentialStatus as DesktopCredentialStatus,
    ProviderCredentialView as DesktopCredentialView, ProviderError as DesktopProviderError,
    ProviderRuntimeState as DesktopProviderRuntimeState,
    ProviderSnapshot as DesktopProviderSnapshot, ProviderView as DesktopProviderView,
    RemoteQuotaState as DesktopRemoteQuotaState,
};

/// The agent runtime owns [`NativeModelRuntimeConfiguration`] and the provider
/// feature owns the settings, so the desktop crate that depends on both bridges
/// them here.
pub(crate) fn runtime_configuration(
    settings: &DesktopAgentRuntimeSettings,
) -> NativeModelRuntimeConfiguration {
    NativeModelRuntimeConfiguration {
        openai_endpoint_override: settings.openai_endpoint.clone(),
        anthropic_endpoint_override: settings.anthropic_endpoint.clone(),
        model_override: settings.model.clone(),
    }
}

fn desktop_credential_view(value: CredentialDescriptorView) -> DesktopCredentialView {
    DesktopCredentialView {
        credential_id: value.credential_id,
        revision: value.revision,
        provider_id: value.provider_id,
        kind: value.kind.into(),
        status: value.status.into(),
        account_label: value.account_label,
        source: value.source,
        model_inference: value.model_inference,
    }
}

struct ValidatedProviderCredential {
    provider_id: String,
    kind: CredentialKind,
    secret_material: String,
    account_label: Option<String>,
    source: Option<String>,
}

pub(crate) fn persistent_credential_bridge(
    config: &crate::application::DesktopApplicationConfig,
    host: std::sync::Arc<dyn DesktopHost>,
) -> Result<ProductCredentialBridge, DesktopProviderError> {
    let paths = config.data_paths();
    paths.ensure_layout().map_err(|error| {
        DesktopProviderError::Persistence(format!("prepare credential storage: {error}"))
    })?;
    let registry = SqliteProductCredentialRegistry::open(paths.agent_runtime_db())
        .map_err(|error| DesktopProviderError::Persistence(error.to_string()))?;
    ProductCredentialBridge::with_persistence(
        std::sync::Arc::new(DesktopHostSecretStore {
            host,
            context: DesktopHostContext::from(config),
        }),
        std::sync::Arc::new(registry),
    )
    .map_err(|error| DesktopProviderError::Persistence(error.to_string()))
}

struct DesktopHostSecretStore {
    host: std::sync::Arc<dyn DesktopHost>,
    context: DesktopHostContext,
}

impl SecretStore for DesktopHostSecretStore {
    fn put(&self, secret_id: &str, material: &str) -> AgentResult<()> {
        match self.execute(DesktopCredentialAction::Write {
            key: credential_secret_key(secret_id),
            secret: DesktopSecret::new(material.as_bytes().to_vec()),
        })? {
            DesktopHostResult::Completed => Ok(()),
            _ => Err(secret_store_error(
                "credential write returned an unexpected host result",
            )),
        }
    }

    fn get(&self, secret_id: &str) -> AgentResult<Option<String>> {
        match self.execute(DesktopCredentialAction::Read {
            key: credential_secret_key(secret_id),
        })? {
            DesktopHostResult::Credential(secret) => secret
                .map(|secret| {
                    String::from_utf8(secret.into_inner()).map_err(|_| {
                        secret_store_error("stored credential is not valid UTF-8 text")
                    })
                })
                .transpose(),
            _ => Err(secret_store_error(
                "credential read returned an unexpected host result",
            )),
        }
    }

    fn delete(&self, secret_id: &str) -> AgentResult<()> {
        match self.execute(DesktopCredentialAction::Delete {
            key: credential_secret_key(secret_id),
        })? {
            DesktopHostResult::Completed => Ok(()),
            _ => Err(secret_store_error(
                "credential delete returned an unexpected host result",
            )),
        }
    }
}

impl DesktopHostSecretStore {
    fn execute(&self, action: DesktopCredentialAction) -> AgentResult<DesktopHostResult> {
        self.host
            .execute(&self.context, DesktopHostAction::Credential(action))
            .map_err(|error| secret_store_error(format!("{}: {}", error.code, error.message)))
    }
}

pub(crate) fn credential_secret_key(secret_id: &str) -> String {
    format!("agentkit.{secret_id}")
}

fn secret_store_error(message: impl Into<String>) -> AgentError {
    AgentError::new("credential.secret_store", message)
}

impl DesktopApplication {
    pub(crate) fn read_host_credential_text_result(
        &self,
        key: &str,
    ) -> Result<Option<String>, DesktopApplicationError> {
        match self.inner.host.execute(
            &self.inner.host_context,
            DesktopHostAction::Credential(DesktopCredentialAction::Read {
                key: key.to_owned(),
            }),
        )? {
            DesktopHostResult::Credential(secret) => secret
                .map(|secret| {
                    String::from_utf8(secret.into_inner()).map_err(|_| {
                        DesktopApplicationError::InvalidInput {
                            field: "credential",
                            message: "stored credential is not valid UTF-8 text".to_owned(),
                        }
                    })
                })
                .transpose(),
            _ => Err(DesktopApplicationError::InvalidInput {
                field: "credential",
                message: "credential read returned an unexpected host result".to_owned(),
            }),
        }
    }

    pub(crate) fn read_host_credential_text(&self, key: &str) -> Option<String> {
        self.read_host_credential_text_result(key).ok().flatten()
    }

    pub fn provider_runtime_settings(
        &self,
    ) -> Result<DesktopAgentRuntimeSettings, DesktopApplicationError> {
        self.inner
            .provider_settings
            .lock()
            .map(|settings| settings.current())
            .map_err(|_| DesktopProviderError::SettingsStateUnavailable.into())
    }

    pub fn save_provider_runtime_settings(
        &self,
        update: DesktopAgentRuntimeSettingsUpdate,
    ) -> Result<DesktopAgentRuntimeSettings, DesktopApplicationError> {
        let runtime = self.authority().shared_runtime();
        let mut state = self
            .inner
            .provider_settings
            .lock()
            .map_err(|_| DesktopProviderError::SettingsStateUnavailable)?;
        let previous = state.current();
        let next = state.prepare_update(update)?;
        state.persist(&next)?;
        if let Err(error) = runtime
            .inner()
            .configure_model_runtime(runtime_configuration(&next))
        {
            let rollback = state.persist(&previous);
            return Err(DesktopProviderError::RuntimeSettingsApply {
                message: error.to_string(),
                rollback_failed: rollback.err().map(|error| error.to_string()),
            }
            .into());
        }
        state.commit(next.clone());
        drop(state);
        let revision = self
            .inner
            .provider_revision
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        self.emit_event(DesktopEventKind::ProviderChanged {
            provider_id: None,
            revision,
        });
        Ok(next)
    }

    pub fn provider_snapshot(&self) -> DesktopProviderSnapshot {
        let runtime = self.authority().shared_runtime();
        let runtime = runtime.inner();
        let providers = runtime
            .credentials()
            .providers()
            .into_iter()
            .map(|provider| DesktopProviderView {
                provider_id: provider.provider_id,
                display_name: provider.display_name,
                protocol_families: provider.protocol_families,
                supported_kinds: provider
                    .supported_kinds
                    .into_iter()
                    .map(DesktopCredentialKind::from)
                    .collect(),
                supports_browser_login: provider.supports_browser_login,
                enterprise_identity: provider.enterprise_identity,
            })
            .collect();
        let diagnostics = runtime.independent_diagnostics();
        let quota = runtime.native_quota_surface();
        DesktopProviderSnapshot {
            revision: self.inner.provider_revision.load(Ordering::Acquire),
            providers,
            credentials: diagnostics
                .credential
                .credentials
                .into_iter()
                .map(desktop_credential_view)
                .collect(),
            broker_ready: diagnostics.credential.broker_ready,
            broker_degraded: diagnostics.credential.broker_degraded,
            credential_recovery_issue_count: diagnostics.credential.recovery_issues.len(),
            runtime: DesktopProviderRuntimeState {
                backend: diagnostics.runtime_backend,
                runtime_ready: diagnostics.runtime_ready,
                profile_id: diagnostics.profile_id,
                profile_has_credential_refs: diagnostics.profile_has_credential_refs,
                live_model_adapter_drives_turn: diagnostics.live_model_adapter_drives_turn,
            },
            remote_quota: DesktopRemoteQuotaState::Unavailable {
                note: quota.remote_quota_note.to_owned(),
            },
            capability_limits: quota
                .providers
                .into_iter()
                .map(|provider| DesktopProviderCapabilityView {
                    provider_id: provider.provider_id,
                    display_name: provider.display_name,
                    adapter_id: provider.adapter_id.map(str::to_owned),
                    credential_health: provider.credential_health.to_owned(),
                    has_usable_credential: provider.has_usable_credential,
                    known_limits: provider
                        .known_limits
                        .into_iter()
                        .map(|limit| DesktopCapabilityLimit {
                            kind: limit.kind.to_owned(),
                            label: limit.label.to_owned(),
                            value: limit.value,
                            note: limit.note.to_owned(),
                        })
                        .collect(),
                    note: provider.note.to_owned(),
                })
                .collect(),
            subscription_not_equated_to_api_quota: quota.subscription_not_equated_to_api_quota,
        }
    }

    pub fn login_provider_credential(
        &self,
        input: DesktopProviderCredentialInput,
    ) -> Result<DesktopCredentialView, DesktopApplicationError> {
        let credential = self.validate_provider_credential_input(input)?;
        let runtime = self.authority().shared_runtime();
        let descriptor = runtime
            .inner()
            .credentials()
            .login(ProductCredentialLoginInput {
                provider_id: credential.provider_id.clone(),
                kind: credential.kind,
                secret_material: credential.secret_material,
                account_label: credential.account_label,
                source: credential.source,
            })
            .map_err(|error| DesktopProviderError::Runtime(error.to_string()))?;
        runtime
            .inner()
            .refresh_product_profile(None)
            .map_err(|error| DesktopProviderError::Runtime(error.to_string()))?;
        Ok(self.finish_provider_credential_change(credential.provider_id, descriptor))
    }

    pub fn import_provider_credential(
        &self,
        input: DesktopProviderCredentialImportInput,
    ) -> Result<DesktopCredentialView, DesktopApplicationError> {
        let credential = self.validate_provider_credential_input(input.credential)?;
        let runtime = self.authority().shared_runtime();
        let descriptor = runtime
            .inner()
            .credentials()
            .import_generated_api_key(ProductCredentialImportInput {
                provider_id: credential.provider_id.clone(),
                kind: credential.kind,
                secret_material: credential.secret_material,
                account_label: credential.account_label,
                source: credential.source,
                permissions_summary: input.permissions_summary,
                independent_revoke_uri: input.independent_revoke_uri,
            })
            .map_err(|error| DesktopProviderError::Runtime(error.to_string()))?;
        runtime
            .inner()
            .refresh_product_profile(None)
            .map_err(|error| DesktopProviderError::Runtime(error.to_string()))?;
        Ok(self.finish_provider_credential_change(credential.provider_id, descriptor))
    }

    pub fn revoke_provider_credential(
        &self,
        credential_id: impl Into<String>,
        revision: u64,
        reason: Option<String>,
    ) -> Result<DesktopCredentialView, DesktopApplicationError> {
        let credential_id = credential_id.into();
        let credential_id = credential_id.trim();
        if credential_id.is_empty() {
            return Err(DesktopProviderError::InvalidCredentialId.into());
        }
        let runtime = self.authority().shared_runtime();
        let descriptor = runtime
            .inner()
            .credentials()
            .revoke(
                CredentialRef {
                    credential_id: credential_id.to_owned(),
                    revision,
                },
                reason,
            )
            .map_err(|error| DesktopProviderError::Runtime(error.to_string()))?;
        runtime
            .inner()
            .refresh_product_profile(None)
            .map_err(|error| DesktopProviderError::Runtime(error.to_string()))?;
        let provider_id = descriptor.provider_id.clone();
        Ok(self.finish_provider_credential_change(provider_id, descriptor))
    }

    pub fn refresh_provider_runtime(
        &self,
        provider_id: Option<String>,
    ) -> Result<DesktopProviderSnapshot, DesktopApplicationError> {
        self.authority()
            .shared_runtime()
            .inner()
            .refresh_product_profile(None)
            .map_err(|error| DesktopProviderError::Runtime(error.to_string()))?;
        let revision = self
            .inner
            .provider_revision
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        self.emit_event(DesktopEventKind::ProviderChanged {
            provider_id,
            revision,
        });
        Ok(self.provider_snapshot())
    }

    fn validate_provider_credential_input(
        &self,
        input: DesktopProviderCredentialInput,
    ) -> Result<ValidatedProviderCredential, DesktopProviderError> {
        let provider_id = input.provider_id.trim().to_owned();
        let runtime = self.authority().shared_runtime();
        let provider = runtime
            .inner()
            .credentials()
            .providers()
            .into_iter()
            .find(|provider| provider.provider_id == provider_id)
            .ok_or_else(|| DesktopProviderError::UnknownProvider(provider_id.clone()))?;
        let kind = CredentialKind::from(input.kind);
        if !provider.supported_kinds.contains(&kind) {
            return Err(DesktopProviderError::UnsupportedCredentialKind {
                provider_id,
                kind: input.kind,
            });
        }
        let secret_material = String::from_utf8(input.secret.into_inner())
            .map_err(|_| DesktopProviderError::InvalidSecretEncoding)?;
        if secret_material.trim().is_empty() {
            return Err(DesktopProviderError::EmptySecret);
        }
        Ok(ValidatedProviderCredential {
            provider_id: provider.provider_id,
            kind,
            secret_material,
            account_label: normalize_optional(input.account_label),
            source: normalize_optional(input.source),
        })
    }

    fn finish_provider_credential_change(
        &self,
        provider_id: String,
        descriptor: CredentialDescriptorView,
    ) -> DesktopCredentialView {
        let revision = self
            .inner
            .provider_revision
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        let view = desktop_credential_view(descriptor);
        self.emit_event(DesktopEventKind::CredentialChanged {
            provider_id: provider_id.clone(),
            credential_id: view.credential_id.clone(),
            revision,
        });
        self.emit_event(DesktopEventKind::ProviderChanged {
            provider_id: Some(provider_id),
            revision,
        });
        view
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::Mutex;

    use lilia_feature_provider::PROVIDER_RUNTIME_SETTINGS_KEY;
    use lilia_service::ServiceAuthority;
    use lilia_storage::SqliteAgentRuntimeStateStore;
    use mutsuki_agent_contracts::OPENAI_CREDENTIAL_PROVIDER_ID;

    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };

    #[derive(Debug)]
    struct TestHost;

    impl DesktopHost for TestHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    #[derive(Default)]
    struct MemoryCredentialHost {
        secrets: Mutex<BTreeMap<(String, String), Vec<u8>>>,
    }

    impl DesktopHost for MemoryCredentialHost {
        fn execute(
            &self,
            context: &DesktopHostContext,
            action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            match action {
                DesktopHostAction::Credential(DesktopCredentialAction::Write { key, secret }) => {
                    self.secrets.lock().unwrap().insert(
                        (context.instance_identity.clone(), key),
                        secret.into_inner(),
                    );
                    Ok(DesktopHostResult::Completed)
                }
                DesktopHostAction::Credential(DesktopCredentialAction::Read { key }) => {
                    Ok(DesktopHostResult::Credential(
                        self.secrets
                            .lock()
                            .unwrap()
                            .get(&(context.instance_identity.clone(), key))
                            .cloned()
                            .map(DesktopSecret::new),
                    ))
                }
                DesktopHostAction::Credential(DesktopCredentialAction::Delete { key }) => {
                    self.secrets
                        .lock()
                        .unwrap()
                        .remove(&(context.instance_identity.clone(), key));
                    Ok(DesktopHostResult::Completed)
                }
                _ => Err(DesktopHostError::new(
                    "unexpected_test_host_action",
                    "test host only supports credential actions",
                    false,
                )),
            }
        }
    }

    fn application() -> DesktopApplication {
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("provider-test:{}", uuid::Uuid::new_v4()),
            "provider-test",
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new("C:/lilia/provider-test", "provider-test").unwrap(),
            authority,
            Arc::new(TestHost),
        )
        .unwrap()
    }

    #[test]
    fn login_and_revoke_refresh_the_typed_provider_snapshot() {
        let application = application();
        let events = application.subscribe_events();
        let before = application.provider_snapshot();
        assert!(before.broker_ready);
        assert!(before.credentials.is_empty());
        assert!(matches!(
            before.remote_quota,
            DesktopRemoteQuotaState::Unavailable { .. }
        ));

        let credential = application
            .login_provider_credential(DesktopProviderCredentialInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.to_owned(),
                kind: DesktopCredentialKind::ApiKey,
                secret: DesktopSecret::new(b"sk-test-openai-api-key-0123456789abcdef".to_vec()),
                account_label: Some("  LiliaCode  ".to_owned()),
                source: Some("settings".to_owned()),
            })
            .unwrap();
        assert_eq!(credential.status, DesktopCredentialStatus::Active);
        assert_eq!(credential.account_label.as_deref(), Some("LiliaCode"));
        let credential_event = events.recv().unwrap();
        assert!(matches!(
            credential_event.kind,
            DesktopEventKind::CredentialChanged { .. }
        ));
        assert!(matches!(
            events.recv().unwrap().kind,
            DesktopEventKind::ProviderChanged { .. }
        ));

        let active = application.provider_snapshot();
        assert!(active.revision > before.revision);
        assert!(active.runtime.profile_has_credential_refs);
        assert!(active.runtime.live_model_adapter_drives_turn);
        assert_eq!(active.credentials, vec![credential.clone()]);

        let refreshed = application
            .refresh_provider_runtime(Some(OPENAI_CREDENTIAL_PROVIDER_ID.to_owned()))
            .unwrap();
        assert!(refreshed.revision > active.revision);
        assert!(matches!(
            events.recv().unwrap().kind,
            DesktopEventKind::ProviderChanged { .. }
        ));

        let revoked = application
            .revoke_provider_credential(
                credential.credential_id,
                credential.revision,
                Some("test cleanup".to_owned()),
            )
            .unwrap();
        assert_eq!(revoked.status, DesktopCredentialStatus::Revoked);
        assert!(
            !application
                .provider_snapshot()
                .runtime
                .profile_has_credential_refs
        );
    }

    #[test]
    fn provider_input_debug_output_redacts_secret() {
        let input = DesktopProviderCredentialInput {
            provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.to_owned(),
            kind: DesktopCredentialKind::ApiKey,
            secret: DesktopSecret::new(b"never-print-this-secret".to_vec()),
            account_label: None,
            source: None,
        };
        let debug = format!("{input:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("never-print-this-secret"));
    }

    #[test]
    fn invalid_provider_or_secret_never_mutates_broker_state() {
        let application = application();
        let error = application
            .login_provider_credential(DesktopProviderCredentialInput {
                provider_id: "missing-provider".to_owned(),
                kind: DesktopCredentialKind::ApiKey,
                secret: DesktopSecret::new(b"secret".to_vec()),
                account_label: None,
                source: None,
            })
            .unwrap_err();
        assert!(matches!(
            error,
            DesktopApplicationError::Provider(DesktopProviderError::UnknownProvider(_))
        ));
        assert!(application.provider_snapshot().credentials.is_empty());
    }

    #[test]
    fn runtime_settings_validate_apply_and_reject_stale_revisions() {
        let application = application();
        let events = application.subscribe_events();
        let initial = application.provider_runtime_settings().unwrap();

        let saved = application
            .save_provider_runtime_settings(DesktopAgentRuntimeSettingsUpdate {
                expected_revision: initial.revision,
                openai_endpoint: Some(
                    "  https://models.example.test/v1/chat/completions  ".to_owned(),
                ),
                anthropic_endpoint: Some("https://anthropic.example.test/v1/messages".to_owned()),
                model: Some("gpt-4.1".to_owned()),
            })
            .unwrap();
        assert_eq!(
            saved.openai_endpoint.as_deref(),
            Some("https://models.example.test/v1/chat/completions")
        );
        assert_eq!(saved.model.as_deref(), Some("gpt-4.1"));
        assert!(matches!(
            events.recv().unwrap().kind,
            DesktopEventKind::ProviderChanged {
                provider_id: None,
                ..
            }
        ));
        let applied = application
            .authority()
            .shared_runtime()
            .inner()
            .model_runtime_configuration()
            .unwrap();
        assert_eq!(
            applied.openai_endpoint_override.as_deref(),
            saved.openai_endpoint.as_deref()
        );
        assert_eq!(
            applied.anthropic_endpoint_override.as_deref(),
            saved.anthropic_endpoint.as_deref()
        );
        assert_eq!(applied.model_override.as_deref(), saved.model.as_deref());

        let stale = application
            .save_provider_runtime_settings(DesktopAgentRuntimeSettingsUpdate {
                expected_revision: initial.revision,
                openai_endpoint: None,
                anthropic_endpoint: None,
                model: None,
            })
            .unwrap_err();
        assert!(matches!(
            stale,
            DesktopApplicationError::Provider(
                DesktopProviderError::SettingsRevisionConflict { .. }
            )
        ));
        assert_eq!(application.provider_runtime_settings().unwrap(), saved);

        let invalid = application
            .save_provider_runtime_settings(DesktopAgentRuntimeSettingsUpdate {
                expected_revision: saved.revision,
                openai_endpoint: Some("file:///tmp/model".to_owned()),
                anthropic_endpoint: None,
                model: Some("invalid model".to_owned()),
            })
            .unwrap_err();
        assert!(matches!(
            invalid,
            DesktopApplicationError::Provider(DesktopProviderError::InvalidEndpoint { .. })
        ));
        assert_eq!(application.provider_runtime_settings().unwrap(), saved);
    }

    #[test]
    fn corrupt_or_unsafe_persisted_runtime_settings_fail_closed() {
        let store = SqliteAgentRuntimeStateStore::open_in_memory().unwrap();
        store
            .put_setting(
                PROVIDER_RUNTIME_SETTINGS_KEY,
                &serde_json::json!({
                    "schemaVersion": 1,
                    "revision": 2,
                    "openaiEndpoint": "file:///tmp/model",
                    "anthropicEndpoint": null,
                    "model": "gpt-4.1"
                }),
            )
            .unwrap();

        assert!(matches!(
            DesktopAgentRuntimeSettingsState::open(store),
            Err(DesktopProviderError::InvalidEndpoint { .. })
        ));
    }

    #[test]
    fn desktop_bootstrap_restores_non_secret_runtime_settings_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let config =
            DesktopApplicationConfig::new(root.path(), "lilia.provider-runtime-settings-test")
                .unwrap();
        let host = Arc::new(MemoryCredentialHost::default());
        let application = DesktopApplication::bootstrap(config.clone(), host.clone()).unwrap();
        let saved = application
            .save_provider_runtime_settings(DesktopAgentRuntimeSettingsUpdate {
                expected_revision: 1,
                openai_endpoint: Some("https://models.example.test/v1".to_owned()),
                anthropic_endpoint: None,
                model: Some("persisted-model".to_owned()),
            })
            .unwrap();
        drop(application);

        let restarted = DesktopApplication::bootstrap(config, host).unwrap();
        assert_eq!(restarted.provider_runtime_settings().unwrap(), saved);
        let applied = restarted
            .authority()
            .shared_runtime()
            .inner()
            .model_runtime_configuration()
            .unwrap();
        assert_eq!(
            applied.openai_endpoint_override.as_deref(),
            Some("https://models.example.test/v1")
        );
        assert_eq!(applied.model_override.as_deref(), Some("persisted-model"));
    }

    #[test]
    fn desktop_bootstrap_restores_keyring_backed_credentials_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let config = DesktopApplicationConfig::new(root.path(), "lilia.provider-test").unwrap();
        let host = Arc::new(MemoryCredentialHost::default());
        let secret = "sk-persisted-lilia-secret-0123456789abcdef";

        let application = DesktopApplication::bootstrap(config.clone(), host.clone()).unwrap();
        let credential = application
            .login_provider_credential(DesktopProviderCredentialInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.to_owned(),
                kind: DesktopCredentialKind::ApiKey,
                secret: DesktopSecret::new(secret.as_bytes().to_vec()),
                account_label: Some("restart test".to_owned()),
                source: Some("native settings".to_owned()),
            })
            .unwrap();
        drop(application);

        let restarted = DesktopApplication::bootstrap(config.clone(), host.clone()).unwrap();
        let restored = restarted.provider_snapshot();
        assert_eq!(restored.credentials.len(), 1);
        assert_eq!(
            restored.credentials[0].credential_id,
            credential.credential_id
        );
        assert_eq!(
            restored.credentials[0].status,
            DesktopCredentialStatus::Active
        );
        assert!(restored.runtime.profile_has_credential_refs);
        restarted
            .revoke_provider_credential(
                credential.credential_id.clone(),
                credential.revision,
                Some("restart test".to_owned()),
            )
            .unwrap();
        drop(restarted);

        let revoked_restart = DesktopApplication::bootstrap(config.clone(), host.clone()).unwrap();
        let revoked = revoked_restart.provider_snapshot();
        assert_eq!(revoked.credentials.len(), 1);
        assert_eq!(
            revoked.credentials[0].status,
            DesktopCredentialStatus::Revoked
        );
        assert!(!revoked.runtime.profile_has_credential_refs);
        assert!(host.secrets.lock().unwrap().is_empty());

        let registry_bytes = std::fs::read(config.data_paths().agent_runtime_db()).unwrap();
        assert!(!String::from_utf8_lossy(&registry_bytes).contains(secret));
    }
}

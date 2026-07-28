//! Product Credential Broker bridge (#50).
//!
//! Official login / API key material enters only through AgentKit Credential Broker.
//! Provider instances and profiles store CredentialRef only — never secret material.
//! Credential health is diagnosed independently from Native Runtime health.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use mutsuki_agent_contracts::{
    official_credential_providers, CredentialCapability, CredentialDescriptor,
    CredentialImportRequest, CredentialKind, CredentialLoginRequest, CredentialMaterialOrigin,
    CredentialProviderDescriptor, CredentialRef, CredentialRefreshPolicy, CredentialRevokeRequest,
    CredentialStatus, CredentialStatusRequest, ANTHROPIC_CREDENTIAL_PROVIDER_ID,
    CREDENTIAL_UNSUPPORTED_FOR_CUSTOM_RUNTIME, OPENAI_CREDENTIAL_PROVIDER_ID,
};
use mutsuki_agent_runtime::{CredentialBrokerService, InMemorySecretStore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::NativeRuntimeError;

/// Product-facing credential login request (secret accepted only on this boundary).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCredentialLoginInput {
    pub provider_id: String,
    pub kind: CredentialKind,
    pub secret_material: String,
    #[serde(default)]
    pub account_label: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

/// Product-facing import for official-login-generated API keys.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCredentialImportInput {
    pub provider_id: String,
    pub kind: CredentialKind,
    pub secret_material: String,
    #[serde(default)]
    pub account_label: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub permissions_summary: Option<String>,
    #[serde(default)]
    pub independent_revoke_uri: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialHealthSnapshot {
    pub broker_ready: bool,
    pub provider_count: usize,
    pub credential_count: usize,
    pub active_count: usize,
    pub unavailable_count: usize,
    /// True when at least one usable API credential can bind a Provider instance.
    pub has_usable_model_credential: bool,
    pub credentials: Vec<CredentialDescriptorView>,
}

/// Public credential view — never includes secret material.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialDescriptorView {
    pub credential_id: String,
    pub revision: u64,
    pub provider_id: String,
    pub kind: CredentialKind,
    pub status: CredentialStatus,
    pub account_label: Option<String>,
    pub source: Option<String>,
    pub model_inference: bool,
}

impl From<&CredentialDescriptor> for CredentialDescriptorView {
    fn from(value: &CredentialDescriptor) -> Self {
        Self {
            credential_id: value.credential.credential_id.clone(),
            revision: value.credential.revision,
            provider_id: value.provider_id.clone(),
            kind: value.kind,
            status: value.status,
            account_label: value.account_label.clone(),
            source: value.source.clone(),
            model_inference: value.capability.model_inference,
        }
    }
}

/// Separates credential diagnosis from Runtime capability diagnosis (#50 / #121 DoD).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndependentDiagnostics {
    pub credential: CredentialHealthSnapshot,
    pub runtime_backend: String,
    pub runtime_ready: bool,
    pub official_agent_server: bool,
    pub node_runner_default: bool,
    pub profile_id: Option<String>,
    pub profile_has_credential_refs: bool,
    pub credential_and_runtime_independent: bool,
    /// Honest: product turns use protocol HTTP Model Adapter when openai-compatible
    /// or anthropic-messages CredentialRef is bound (otherwise reference coding path).
    pub live_model_adapter_drives_turn: bool,
}

#[derive(Clone)]
pub struct ProductCredentialBridge {
    broker: CredentialBrokerService,
    /// Track known descriptors so product can list/diagnose without a Broker list API.
    known: Arc<Mutex<BTreeMap<String, CredentialDescriptor>>>,
}

impl Default for ProductCredentialBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl ProductCredentialBridge {
    pub fn new() -> Self {
        Self {
            broker: CredentialBrokerService::new(Arc::new(InMemorySecretStore::default())),
            known: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn broker(&self) -> &CredentialBrokerService {
        &self.broker
    }

    pub fn providers(&self) -> Vec<CredentialProviderDescriptor> {
        let mut providers = self.broker.providers();
        if providers.is_empty() {
            providers = official_credential_providers();
        }
        providers
    }

    pub fn login(
        &self,
        input: ProductCredentialLoginInput,
    ) -> Result<CredentialDescriptorView, NativeRuntimeError> {
        let result = self
            .broker
            .login(CredentialLoginRequest {
                provider_id: input.provider_id,
                kind: input.kind,
                secret_material: input.secret_material,
                account_label: input.account_label,
                source: input.source,
                capability: CredentialCapability::default(),
                refresh_policy: CredentialRefreshPolicy::default(),
                expires_at_unix_ms: None,
                metadata: Value::Null,
            })
            .map_err(map_broker_error)?;
        self.remember(&result.descriptor);
        Ok(CredentialDescriptorView::from(&result.descriptor))
    }

    pub fn import_generated_api_key(
        &self,
        input: ProductCredentialImportInput,
    ) -> Result<CredentialDescriptorView, NativeRuntimeError> {
        let result = self
            .broker
            .import(CredentialImportRequest {
                provider_id: input.provider_id,
                kind: input.kind,
                secret_material: input.secret_material,
                origin: CredentialMaterialOrigin::OfficialLoginGenerated,
                account_label: input.account_label,
                source: input.source,
                permissions_summary: input.permissions_summary,
                independent_revoke_uri: input.independent_revoke_uri,
                capability: CredentialCapability::default(),
                refresh_policy: CredentialRefreshPolicy::default(),
                expires_at_unix_ms: None,
                metadata: json!({}),
            })
            .map_err(map_broker_error)?;
        self.remember(&result.descriptor);
        Ok(CredentialDescriptorView::from(&result.descriptor))
    }

    pub fn status(
        &self,
        credential: CredentialRef,
    ) -> Result<CredentialDescriptorView, NativeRuntimeError> {
        let result = self
            .broker
            .status(CredentialStatusRequest { credential })
            .map_err(map_broker_error)?;
        self.remember(&result.descriptor);
        Ok(CredentialDescriptorView::from(&result.descriptor))
    }

    pub fn revoke(
        &self,
        credential: CredentialRef,
        reason: Option<String>,
    ) -> Result<CredentialDescriptorView, NativeRuntimeError> {
        let result = self
            .broker
            .revoke(CredentialRevokeRequest { credential, reason })
            .map_err(map_broker_error)?;
        self.remember(&result.descriptor);
        Ok(CredentialDescriptorView::from(&result.descriptor))
    }

    /// Prove Adapter path can resolve without leaking secret into descriptors.
    pub fn resolve_for_adapter(
        &self,
        credential: &CredentialRef,
    ) -> Result<(), NativeRuntimeError> {
        let _secret = self
            .broker
            .resolve_secret(credential)
            .map_err(map_broker_error)?;
        Ok(())
    }

    pub fn health(&self) -> CredentialHealthSnapshot {
        let known = self.known.lock().expect("credential known lock");
        let credentials: Vec<_> = known.values().map(CredentialDescriptorView::from).collect();
        let active_count = credentials
            .iter()
            .filter(|c| c.status == CredentialStatus::Active)
            .count();
        let unavailable_count = credentials.len().saturating_sub(active_count);
        let has_usable_model_credential = credentials.iter().any(|c| {
            c.status == CredentialStatus::Active
                && c.model_inference
                && (c.provider_id == OPENAI_CREDENTIAL_PROVIDER_ID
                    || c.provider_id == ANTHROPIC_CREDENTIAL_PROVIDER_ID)
        });
        CredentialHealthSnapshot {
            broker_ready: true,
            provider_count: self.providers().len(),
            credential_count: credentials.len(),
            active_count,
            unavailable_count,
            has_usable_model_credential,
            credentials,
        }
    }

    pub fn primary_usable_credential(&self) -> Option<CredentialRef> {
        let known = self.known.lock().expect("credential known lock");
        known.values().find_map(|descriptor| {
            if descriptor.status == CredentialStatus::Active
                && descriptor.capability.model_inference
            {
                Some(descriptor.credential.clone())
            } else {
                None
            }
        })
    }

    pub fn openai_compatible_bindings(&self) -> Vec<(String, CredentialRef)> {
        let known = self.known.lock().expect("credential known lock");
        known
            .values()
            .filter(|descriptor| {
                descriptor.status == CredentialStatus::Active
                    && descriptor.provider_id == OPENAI_CREDENTIAL_PROVIDER_ID
            })
            .enumerate()
            .map(|(index, descriptor)| {
                (
                    format!("openai-compatible-{}", index + 1),
                    descriptor.credential.clone(),
                )
            })
            .collect()
    }

    pub fn primary_anthropic_credential(&self) -> Option<CredentialRef> {
        let known = self.known.lock().expect("credential known lock");
        known.values().find_map(|descriptor| {
            if descriptor.status == CredentialStatus::Active
                && descriptor.capability.model_inference
                && descriptor.provider_id == ANTHROPIC_CREDENTIAL_PROVIDER_ID
            {
                Some(descriptor.credential.clone())
            } else {
                None
            }
        })
    }

    fn remember(&self, descriptor: &CredentialDescriptor) {
        self.known.lock().expect("credential known lock").insert(
            descriptor.credential.credential_id.clone(),
            descriptor.clone(),
        );
    }
}

fn map_broker_error(err: mutsuki_agent_contracts::AgentError) -> NativeRuntimeError {
    if err.code == CREDENTIAL_UNSUPPORTED_FOR_CUSTOM_RUNTIME {
        NativeRuntimeError::Agent(format!(
            "{CREDENTIAL_UNSUPPORTED_FOR_CUSTOM_RUNTIME}: {}",
            err.message
        ))
    } else {
        NativeRuntimeError::Agent(format!("{}: {}", err.code, err.message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_and_anthropic_api_keys_login_and_diagnose_independently() {
        let bridge = ProductCredentialBridge::new();
        let openai = bridge
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: Some("openai".into()),
                source: Some("user_api_key".into()),
            })
            .unwrap();
        let anthropic = bridge
            .login(ProductCredentialLoginInput {
                provider_id: ANTHROPIC_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-ant-api03-console-key-0123456789abcdef".into(),
                account_label: None,
                source: Some("anthropic_console".into()),
            })
            .unwrap();
        let health = bridge.health();
        assert!(health.broker_ready);
        assert!(health.has_usable_model_credential);
        assert_eq!(health.active_count, 2);
        assert!(!serde_json::to_string(&openai).unwrap().contains("sk-test"));
        assert!(!serde_json::to_string(&anthropic)
            .unwrap()
            .contains("sk-ant"));
        bridge
            .resolve_for_adapter(&CredentialRef {
                credential_id: openai.credential_id.clone(),
                revision: openai.revision,
            })
            .unwrap();
    }

    #[test]
    fn claude_subscription_credential_is_rejected() {
        let bridge = ProductCredentialBridge::new();
        let err = bridge
            .import_generated_api_key(ProductCredentialImportInput {
                provider_id: ANTHROPIC_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-ant-sid-claude-code-subscription-token".into(),
                account_label: None,
                source: Some("claude_code".into()),
                permissions_summary: None,
                independent_revoke_uri: None,
            })
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(CREDENTIAL_UNSUPPORTED_FOR_CUSTOM_RUNTIME));
        assert!(!bridge.health().has_usable_model_credential);
    }
}

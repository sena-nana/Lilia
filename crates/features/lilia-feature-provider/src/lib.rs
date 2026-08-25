//! Provider domain feature.
//!
//! Owns the provider and credential vocabulary the settings surface renders,
//! plus the revisioned agent runtime settings (endpoints and model) persisted in
//! the agent runtime state store. Credential material itself never lives here —
//! the host's credential store keeps it and this crate only names it.

use std::sync::Arc;

use lilia_contracts::Secret;
use lilia_kernel::{
    Feature, FeatureContext, FeatureId, JobContext, JobProtocol, JobSlot, KernelError,
};
use lilia_storage::SqliteAgentRuntimeStateStore;
use mutsuki_agent_contracts::{CredentialKind, CredentialStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CREDENTIAL_PROTOCOL: &str = "lilia.provider/credential@1";

/// Payload of [`CREDENTIAL_PROTOCOL`].
///
/// An API key save names only its provider: the key itself is staged with the
/// host and claimed on the worker thread, so no secret reaches a job payload or
/// a journal record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase")]
pub enum CredentialRequest {
    SaveApiKey {
        provider_id: String,
    },
    Revoke {
        credential_id: String,
        revision: u64,
    },
    Refresh {
        provider_id: Option<String>,
    },
}

/// Applies one credential operation. Every call reaches the OS credential store
/// or a provider endpoint, which is why none of them may run on the UI thread.
pub trait CredentialPort: Send + Sync + 'static {
    /// Saves the API key the host staged for `provider_id`.
    fn save_api_key(&self, provider_id: &str) -> Result<(), String>;
    fn revoke(&self, credential_id: &str, revision: u64) -> Result<(), String>;
    fn refresh(&self, provider_id: Option<&str>) -> Result<(), String>;
}

/// Single-flight lane: the provider surface shows one credential operation at
/// a time, and two overlapping edits would race on the same revision.
pub fn credential_slot() -> JobSlot {
    JobSlot::new("lilia.provider.credential").expect("the credential slot name is not blank")
}

pub const ASSISTANT_PROBE_PROTOCOL: &str = "lilia.provider/assistant-probe@1";

/// Which question the probe asks the auxiliary model endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssistantProbeKind {
    /// Lists the models the endpoint offers.
    Models,
    /// Checks the endpoint answers and that the configured model exists.
    Connection,
}

/// Payload of [`ASSISTANT_PROBE_PROTOCOL`].
///
/// The probe runs against a base URL, model and API key the user is still
/// editing — none of which are saved yet. They stay staged with the host under
/// `ticket` and are claimed on the worker thread, so an unsaved API key never
/// reaches a job payload or a journal record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantProbeRequest {
    pub ticket: u64,
    pub kind: AssistantProbeKind,
}

impl AssistantProbeRequest {
    pub fn new(ticket: u64, kind: AssistantProbeKind) -> Self {
        Self { ticket, kind }
    }
}

/// Runs one probe against the draft configuration the host staged.
pub trait AssistantProbePort: Send + Sync + 'static {
    fn probe(&self, ticket: u64, kind: AssistantProbeKind) -> Result<Value, String>;
}

/// Single-flight lane: the settings surface shows one probe result at a time,
/// so a second probe replaces the first rather than racing it.
pub fn assistant_probe_slot() -> JobSlot {
    JobSlot::new("lilia.provider.assistant-probe")
        .expect("the assistant probe slot name is not blank")
}

pub struct ProviderFeature {
    port: Arc<dyn CredentialPort>,
    assistant: Arc<dyn AssistantProbePort>,
}

impl ProviderFeature {
    pub fn new(port: Arc<dyn CredentialPort>, assistant: Arc<dyn AssistantProbePort>) -> Self {
        Self { port, assistant }
    }
}

impl Feature for ProviderFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.provider").expect("the provider feature id is not blank")
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        let port = Arc::clone(&self.port);
        let assistant = Arc::clone(&self.assistant);
        vec![
            JobProtocol::new(
                CREDENTIAL_PROTOCOL,
                Arc::new(move |payload, _context: &JobContext| {
                    run_credential_job(payload, port.as_ref())
                }),
            ),
            JobProtocol::new(
                ASSISTANT_PROBE_PROTOCOL,
                Arc::new(move |payload, _context: &JobContext| {
                    run_assistant_probe_job(payload, assistant.as_ref())
                }),
            ),
        ]
    }

    fn mount(&self, _cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        Ok(())
    }
}

fn run_credential_job(payload: Value, port: &dyn CredentialPort) -> Result<Value, String> {
    let request: CredentialRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid credential request: {error}"))?;
    match request {
        CredentialRequest::SaveApiKey { provider_id } => port.save_api_key(&provider_id),
        CredentialRequest::Revoke {
            credential_id,
            revision,
        } => port.revoke(&credential_id, revision),
        CredentialRequest::Refresh { provider_id } => port.refresh(provider_id.as_deref()),
    }?;
    Ok(Value::Null)
}

fn run_assistant_probe_job(
    payload: Value,
    port: &dyn AssistantProbePort,
) -> Result<Value, String> {
    let request: AssistantProbeRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid assistant probe request: {error}"))?;
    port.probe(request.ticket, request.kind)
}

pub const PROVIDER_RUNTIME_SETTINGS_KEY: &str = "provider.runtime.v1";
const PROVIDER_RUNTIME_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCredentialKind {
    ApiKey,
    OAuthGrant,
    GeneratedApiKey,
    CloudIdentity,
}

impl From<CredentialKind> for ProviderCredentialKind {
    fn from(value: CredentialKind) -> Self {
        match value {
            CredentialKind::ApiKey => Self::ApiKey,
            CredentialKind::OAuthGrant => Self::OAuthGrant,
            CredentialKind::GeneratedApiKey => Self::GeneratedApiKey,
            CredentialKind::CloudIdentity => Self::CloudIdentity,
        }
    }
}

impl From<ProviderCredentialKind> for CredentialKind {
    fn from(value: ProviderCredentialKind) -> Self {
        match value {
            ProviderCredentialKind::ApiKey => Self::ApiKey,
            ProviderCredentialKind::OAuthGrant => Self::OAuthGrant,
            ProviderCredentialKind::GeneratedApiKey => Self::GeneratedApiKey,
            ProviderCredentialKind::CloudIdentity => Self::CloudIdentity,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCredentialStatus {
    Active,
    Expired,
    Revoked,
    InsufficientScope,
    AccountDisabled,
    UnsupportedForCustomRuntime,
    PendingRefresh,
}

impl From<CredentialStatus> for ProviderCredentialStatus {
    fn from(value: CredentialStatus) -> Self {
        match value {
            CredentialStatus::Active => Self::Active,
            CredentialStatus::Expired => Self::Expired,
            CredentialStatus::Revoked => Self::Revoked,
            CredentialStatus::InsufficientScope => Self::InsufficientScope,
            CredentialStatus::AccountDisabled => Self::AccountDisabled,
            CredentialStatus::UnsupportedForCustomRuntime => Self::UnsupportedForCustomRuntime,
            CredentialStatus::PendingRefresh => Self::PendingRefresh,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub provider_id: String,
    pub display_name: String,
    pub protocol_families: Vec<String>,
    pub supported_kinds: Vec<ProviderCredentialKind>,
    pub supports_browser_login: bool,
    pub enterprise_identity: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCredentialView {
    pub credential_id: String,
    pub revision: u64,
    pub provider_id: String,
    pub kind: ProviderCredentialKind,
    pub status: ProviderCredentialStatus,
    pub account_label: Option<String>,
    pub source: Option<String>,
    pub model_inference: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRuntimeState {
    pub backend: String,
    pub runtime_ready: bool,
    pub profile_id: Option<String>,
    pub profile_has_credential_refs: bool,
    pub live_model_adapter_drives_turn: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeSettings {
    pub revision: u64,
    pub openai_endpoint: Option<String>,
    pub anthropic_endpoint: Option<String>,
    pub model: Option<String>,
}

impl Default for AgentRuntimeSettings {
    fn default() -> Self {
        Self {
            revision: 1,
            openai_endpoint: None,
            anthropic_endpoint: None,
            model: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeSettingsUpdate {
    pub expected_revision: u64,
    pub openai_endpoint: Option<String>,
    pub anthropic_endpoint: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAgentRuntimeSettings {
    schema_version: u32,
    revision: u64,
    openai_endpoint: Option<String>,
    anthropic_endpoint: Option<String>,
    model: Option<String>,
}

impl From<AgentRuntimeSettings> for StoredAgentRuntimeSettings {
    fn from(value: AgentRuntimeSettings) -> Self {
        Self {
            schema_version: PROVIDER_RUNTIME_SETTINGS_SCHEMA_VERSION,
            revision: value.revision,
            openai_endpoint: value.openai_endpoint,
            anthropic_endpoint: value.anthropic_endpoint,
            model: value.model,
        }
    }
}

impl TryFrom<StoredAgentRuntimeSettings> for AgentRuntimeSettings {
    type Error = ProviderError;

    fn try_from(value: StoredAgentRuntimeSettings) -> Result<Self, Self::Error> {
        if value.schema_version != PROVIDER_RUNTIME_SETTINGS_SCHEMA_VERSION {
            return Err(ProviderError::UnsupportedSettingsSchema(
                value.schema_version,
            ));
        }
        if value.revision == 0 {
            return Err(ProviderError::CorruptSettings(
                "revision must be positive".to_owned(),
            ));
        }
        Ok(Self {
            revision: value.revision,
            openai_endpoint: validate_endpoint("openaiEndpoint", value.openai_endpoint)?,
            anthropic_endpoint: validate_endpoint("anthropicEndpoint", value.anthropic_endpoint)?,
            model: validate_model(value.model)?,
        })
    }
}

pub struct AgentRuntimeSettingsState {
    store: SqliteAgentRuntimeStateStore,
    current: AgentRuntimeSettings,
}

impl AgentRuntimeSettingsState {
    pub fn open(store: SqliteAgentRuntimeStateStore) -> Result<Self, ProviderError> {
        let current = match store
            .setting(PROVIDER_RUNTIME_SETTINGS_KEY)
            .map_err(|error| ProviderError::Persistence(error.to_string()))?
        {
            Some(payload) => {
                let stored: StoredAgentRuntimeSettings = serde_json::from_value(payload)
                    .map_err(|error| ProviderError::CorruptSettings(error.to_string()))?;
                AgentRuntimeSettings::try_from(stored)?
            }
            None => AgentRuntimeSettings::default(),
        };
        Ok(Self { store, current })
    }

    pub fn current(&self) -> AgentRuntimeSettings {
        self.current.clone()
    }

    /// Adopt settings that are already persisted and accepted by the runtime.
    pub fn commit(&mut self, settings: AgentRuntimeSettings) {
        self.current = settings;
    }

    pub fn prepare_update(
        &self,
        update: AgentRuntimeSettingsUpdate,
    ) -> Result<AgentRuntimeSettings, ProviderError> {
        if update.expected_revision != self.current.revision {
            return Err(ProviderError::SettingsRevisionConflict {
                expected: update.expected_revision,
                actual: self.current.revision,
            });
        }
        Ok(AgentRuntimeSettings {
            revision: self
                .current
                .revision
                .checked_add(1)
                .ok_or(ProviderError::SettingsRevisionOverflow)?,
            openai_endpoint: validate_endpoint("openaiEndpoint", update.openai_endpoint)?,
            anthropic_endpoint: validate_endpoint("anthropicEndpoint", update.anthropic_endpoint)?,
            model: validate_model(update.model)?,
        })
    }

    pub fn persist(&self, settings: &AgentRuntimeSettings) -> Result<(), ProviderError> {
        let payload =
            serde_json::to_value(StoredAgentRuntimeSettings::from(settings.clone()))
                .map_err(|error| ProviderError::Persistence(error.to_string()))?;
        self.store
            .put_setting(PROVIDER_RUNTIME_SETTINGS_KEY, &payload)
            .map_err(|error| ProviderError::Persistence(error.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "availability", rename_all = "snake_case")]
pub enum RemoteQuotaState {
    Unavailable { note: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityLimit {
    pub kind: String,
    pub label: String,
    pub value: Option<u64>,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilityView {
    pub provider_id: String,
    pub display_name: String,
    pub adapter_id: Option<String>,
    pub credential_health: String,
    pub has_usable_credential: bool,
    pub known_limits: Vec<CapabilityLimit>,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub revision: u64,
    pub providers: Vec<ProviderView>,
    pub credentials: Vec<ProviderCredentialView>,
    pub broker_ready: bool,
    pub broker_degraded: bool,
    pub credential_recovery_issue_count: usize,
    pub runtime: ProviderRuntimeState,
    pub remote_quota: RemoteQuotaState,
    pub capability_limits: Vec<ProviderCapabilityView>,
    pub subscription_not_equated_to_api_quota: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderCredentialInput {
    pub provider_id: String,
    pub kind: ProviderCredentialKind,
    pub secret: Secret,
    pub account_label: Option<String>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderCredentialImportInput {
    pub credential: ProviderCredentialInput,
    pub permissions_summary: Option<String>,
    pub independent_revoke_uri: Option<String>,
}


pub fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn validate_endpoint(
    field: &'static str,
    value: Option<String>,
) -> Result<Option<String>, ProviderError> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    if value.len() > 2_048 || value.chars().any(char::is_control) {
        return Err(ProviderError::InvalidEndpoint {
            field,
            message: "endpoint is too long or contains control characters".to_owned(),
        });
    }
    let remainder = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .ok_or_else(|| ProviderError::InvalidEndpoint {
            field,
            message: "endpoint must be an absolute HTTP or HTTPS URL".to_owned(),
        })?;
    let authority_end = remainder.find(['/', '?']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_end];
    if authority.is_empty()
        || authority.starts_with(':')
        || authority.contains('@')
        || value.contains('#')
        || value.chars().any(char::is_whitespace)
    {
        return Err(ProviderError::InvalidEndpoint {
            field,
            message: "endpoint must contain a host and no credentials, whitespace, or fragment"
                .to_owned(),
        });
    }
    Ok(Some(value))
}

fn validate_model(value: Option<String>) -> Result<Option<String>, ProviderError> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    if value.len() > 256
        || value.chars().any(char::is_control)
        || value.chars().any(char::is_whitespace)
    {
        return Err(ProviderError::InvalidModel);
    }
    Ok(Some(value))
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    #[error("unknown provider `{0}`")]
    UnknownProvider(String),
    #[error("provider `{provider_id}` does not support credential kind `{kind:?}`")]
    UnsupportedCredentialKind {
        provider_id: String,
        kind: ProviderCredentialKind,
    },
    #[error("provider secret must not be empty")]
    EmptySecret,
    #[error("provider secret must be valid UTF-8 text")]
    InvalidSecretEncoding,
    #[error("credential id must not be empty")]
    InvalidCredentialId,
    #[error("invalid provider runtime endpoint `{field}`: {message}")]
    InvalidEndpoint {
        field: &'static str,
        message: String,
    },
    #[error("provider runtime model must be a non-empty model id without whitespace")]
    InvalidModel,
    #[error("provider runtime settings revision conflict: expected {expected}, actual {actual}")]
    SettingsRevisionConflict { expected: u64, actual: u64 },
    #[error("provider runtime settings revision overflowed")]
    SettingsRevisionOverflow,
    #[error("provider runtime settings state is unavailable")]
    SettingsStateUnavailable,
    #[error("provider runtime settings use unsupported schema version {0}")]
    UnsupportedSettingsSchema(u32),
    #[error("provider runtime settings are corrupt: {0}")]
    CorruptSettings(String),
    #[error(
        "provider runtime settings could not be applied: {message}{rollback}",
        rollback = rollback_failed
            .as_ref()
            .map(|value| format!("; rollback failed: {value}"))
            .unwrap_or_default()
    )]
    RuntimeSettingsApply {
        message: String,
        rollback_failed: Option<String>,
    },
    #[error("provider credential persistence failed: {0}")]
    Persistence(String),
    #[error("provider runtime operation failed: {0}")]
    Runtime(String),
}


#[cfg(test)]
mod assistant_probe_tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        probes: Mutex<Vec<(u64, AssistantProbeKind)>>,
        failure: Option<String>,
    }

    impl AssistantProbePort for RecordingPort {
        fn probe(&self, ticket: u64, kind: AssistantProbeKind) -> Result<Value, String> {
            self.probes.lock().unwrap().push((ticket, kind));
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(serde_json::json!({ "ok": true })),
            }
        }
    }

    #[test]
    fn the_job_claims_the_ticket_the_host_staged() {
        let port = RecordingPort::default();

        let output = run_assistant_probe_job(
            serde_json::to_value(AssistantProbeRequest::new(4, AssistantProbeKind::Models))
                .unwrap(),
            &port,
        )
        .unwrap();

        assert_eq!(output, serde_json::json!({ "ok": true }));
        assert_eq!(
            port.probes.lock().unwrap().as_slice(),
            [(4, AssistantProbeKind::Models)]
        );
    }

    #[test]
    fn a_probe_never_puts_the_draft_api_key_in_the_payload() {
        let payload =
            serde_json::to_value(AssistantProbeRequest::new(9, AssistantProbeKind::Connection))
                .unwrap();

        assert_eq!(
            payload,
            serde_json::json!({ "ticket": 9, "kind": "connection" })
        );
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_assistant_probe_job(
            serde_json::json!({ "ticket": 1, "kind": "explode" }),
            &RecordingPort::default(),
        )
        .expect_err("an unknown probe cannot run");

        assert!(
            error.contains("invalid assistant probe request"),
            "{error}"
        );
    }

    #[test]
    fn credential_edits_and_probes_do_not_share_a_lane() {
        assert_ne!(credential_slot(), assistant_probe_slot());
    }
}

//! Product provider selection for the AgentKit-owned Model effect Runner.

use std::collections::BTreeMap;
use std::sync::Arc;

use mutsuki_agent_adapter_api::{CredentialBroker, CredentialFuture, CredentialValue};
use mutsuki_agent_contracts::{
    AgentRuntimeProfile, CredentialRef, ModelCapability, ModelProtocolAdapterDescriptor,
    ProtocolError, ProtocolErrorClass, ProviderInstanceDescriptor,
};
use mutsuki_agent_runtime::CredentialBrokerService;
use serde_json::json;

use crate::anthropic_adapter::{
    anthropic_provider_descriptor, resolve_anthropic_endpoint, ANTHROPIC_MESSAGES_ADAPTER_ID,
    DEFAULT_ANTHROPIC_MODEL,
};
use crate::profile::{ANTHROPIC_MESSAGES_PROTOCOL_FAMILY, OPENAI_COMPATIBLE_ADAPTER_ID};

pub const DEFAULT_OPENAI_COMPATIBLE_ENDPOINT: &str = "https://api.openai.com/v1";
pub const ENV_MODEL_ENDPOINT: &str = "LILIA_MODEL_ENDPOINT";
pub const ENV_MODEL_ID: &str = "LILIA_MODEL_ID";
const PRODUCT_DEFAULT_MODEL_PLACEHOLDER: &str = "product-default";
const DEFAULT_OPENAI_MODEL: &str = "gpt-4.1-mini";

#[derive(Clone)]
struct ProductAdapterCredentialBroker {
    service: CredentialBrokerService,
}

impl CredentialBroker for ProductAdapterCredentialBroker {
    fn resolve(&self, credential: CredentialRef) -> CredentialFuture {
        let service = self.service.clone();
        Box::pin(async move {
            match service.resolve_secret(&credential) {
                Ok(secret) => CredentialValue::new(secret).map_err(|error| ProtocolError {
                    code: error.code.clone(),
                    class: ProtocolErrorClass::Authentication,
                    message: error.message,
                    retry_after_ms: None,
                }),
                Err(error) => Err(ProtocolError {
                    code: if error.code.is_empty() {
                        "agent.credential.unavailable".into()
                    } else {
                        error.code
                    },
                    class: ProtocolErrorClass::Authentication,
                    message: error.message,
                    retry_after_ms: None,
                }),
            }
        })
    }
}

pub fn adapter_credential_broker(service: CredentialBrokerService) -> Arc<dyn CredentialBroker> {
    Arc::new(ProductAdapterCredentialBroker { service })
}

#[derive(Clone, Debug)]
pub struct LiveModelTurnPlan {
    pub provider: ProviderInstanceDescriptor,
    pub model: String,
    pub driver: LiveModelDriver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveModelDriver {
    OpenAiCompatible,
    AnthropicMessages,
}

impl LiveModelDriver {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai-compatible",
            Self::AnthropicMessages => "anthropic-messages",
        }
    }
}

pub fn resolve_model_endpoint(override_endpoint: Option<&str>) -> String {
    override_endpoint
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var(ENV_MODEL_ENDPOINT)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| DEFAULT_OPENAI_COMPATIBLE_ENDPOINT.to_string())
}

pub fn openai_live_eligible(profile: &AgentRuntimeProfile) -> bool {
    profile.providers.iter().any(|provider| {
        provider.adapter_id == OPENAI_COMPATIBLE_ADAPTER_ID && provider.credential_ref.is_some()
    })
}

pub fn anthropic_live_eligible(profile: &AgentRuntimeProfile) -> bool {
    profile.providers.iter().any(|provider| {
        provider.adapter_id == ANTHROPIC_MESSAGES_ADAPTER_ID && provider.credential_ref.is_some()
    })
}

pub fn live_model_adapter_eligible(profile: &AgentRuntimeProfile) -> bool {
    openai_live_eligible(profile) || anthropic_live_eligible(profile)
}

pub fn build_live_turn_plan(
    profile: &AgentRuntimeProfile,
    openai_endpoint: &str,
    anthropic_endpoint: Option<&str>,
) -> Option<LiveModelTurnPlan> {
    if openai_live_eligible(profile) {
        return build_openai_turn_plan(profile, openai_endpoint);
    }
    if anthropic_live_eligible(profile) {
        return build_anthropic_turn_plan(profile, anthropic_endpoint);
    }
    None
}

fn build_openai_turn_plan(
    profile: &AgentRuntimeProfile,
    endpoint: &str,
) -> Option<LiveModelTurnPlan> {
    let selection = profile
        .adapters
        .iter()
        .find(|adapter| adapter.adapter_id == OPENAI_COMPATIBLE_ADAPTER_ID)?;
    let provider = profile
        .providers
        .iter()
        .find(|provider| provider.instance_id == selection.provider_instance_id)
        .filter(|provider| provider.credential_ref.is_some())
        .or_else(|| {
            profile.providers.iter().find(|provider| {
                provider.adapter_id == OPENAI_COMPATIBLE_ADAPTER_ID
                    && provider.credential_ref.is_some()
            })
        })?;
    let credential = provider.credential_ref.clone()?;
    let model = resolve_model_id(&selection.model, DEFAULT_OPENAI_MODEL);
    Some(LiveModelTurnPlan {
        provider: ProviderInstanceDescriptor {
            provider_id: provider.instance_id.clone(),
            adapter_id: OPENAI_COMPATIBLE_ADAPTER_ID.into(),
            endpoint: endpoint.to_string(),
            credential,
            models: BTreeMap::from([(
                model.clone(),
                ModelCapability {
                    context_window: 128_000,
                    streaming: true,
                    tools: true,
                    structured_output: true,
                    ..ModelCapability::default()
                },
            )]),
            headers: BTreeMap::new(),
            compatibility: BTreeMap::from([
                ("timeout_ms".into(), json!(30_000)),
                ("max_retries".into(), json!(1)),
            ]),
            remote_execution_allowed: true,
        },
        model,
        driver: LiveModelDriver::OpenAiCompatible,
    })
}

fn build_anthropic_turn_plan(
    profile: &AgentRuntimeProfile,
    endpoint_override: Option<&str>,
) -> Option<LiveModelTurnPlan> {
    let selection = profile.adapters.iter().find(|adapter| {
        adapter.adapter_id == ANTHROPIC_MESSAGES_ADAPTER_ID
            || adapter.protocol_family == ANTHROPIC_MESSAGES_PROTOCOL_FAMILY
    });
    let provider = selection
        .and_then(|selection| {
            profile
                .providers
                .iter()
                .find(|provider| provider.instance_id == selection.provider_instance_id)
                .filter(|provider| provider.credential_ref.is_some())
        })
        .or_else(|| {
            profile.providers.iter().find(|provider| {
                provider.adapter_id == ANTHROPIC_MESSAGES_ADAPTER_ID
                    && provider.credential_ref.is_some()
            })
        })?;
    let credential = provider.credential_ref.clone()?;
    let model = selection
        .map(|selection| resolve_model_id(&selection.model, DEFAULT_ANTHROPIC_MODEL))
        .unwrap_or_else(|| DEFAULT_ANTHROPIC_MODEL.to_string());
    Some(LiveModelTurnPlan {
        provider: anthropic_provider_descriptor(
            provider.instance_id.clone(),
            &resolve_anthropic_endpoint(endpoint_override),
            credential,
            &model,
        ),
        model,
        driver: LiveModelDriver::AnthropicMessages,
    })
}

fn resolve_model_id(profile_model: &str, fallback: &str) -> String {
    if let Ok(value) = std::env::var(ENV_MODEL_ID) {
        let value = value.trim();
        if !value.is_empty() {
            return value.to_string();
        }
    }
    if profile_model.trim().is_empty() || profile_model == PRODUCT_DEFAULT_MODEL_PLACEHOLDER {
        fallback.to_string()
    } else {
        profile_model.to_string()
    }
}

pub(crate) fn openai_adapter_descriptor() -> ModelProtocolAdapterDescriptor {
    ModelProtocolAdapterDescriptor {
        adapter_id: OPENAI_COMPATIBLE_ADAPTER_ID.into(),
        protocol: "openai.chat-completions".into(),
        version: "1".into(),
        runner_id: "agent.adapter.openai-compatible".into(),
        capability: ModelCapability {
            context_window: 128_000,
            streaming: true,
            tools: true,
            structured_output: true,
            ..ModelCapability::default()
        },
    }
}

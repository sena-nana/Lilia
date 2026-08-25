//! Anthropic Messages Adapter — thin product re-export.
//!
//! Owned implementation lives in AgentKit `mutsuki-agent-adapter-anthropic`.
//! Product only resolves Lilia-specific endpoint env overrides.

pub use mutsuki_agent_adapter_anthropic::{
    provider_descriptor as anthropic_provider_descriptor, resolve_endpoint,
    AnthropicMessagesAdapter, ADAPTER_ID as ANTHROPIC_MESSAGES_ADAPTER_ID,
    DEFAULT_ENDPOINT as DEFAULT_ANTHROPIC_ENDPOINT, DEFAULT_MODEL as DEFAULT_ANTHROPIC_MODEL,
};

/// Product env override (not read inside AgentKit Adapter).
pub const ENV_ANTHROPIC_ENDPOINT: &str = "LILIA_ANTHROPIC_ENDPOINT";

pub fn resolve_anthropic_endpoint(override_endpoint: Option<&str>) -> String {
    override_endpoint
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var(ENV_ANTHROPIC_ENDPOINT)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| resolve_endpoint(None))
}

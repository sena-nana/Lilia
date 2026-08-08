mod agent_interaction_defaults_contract;
mod assistant_ai;
#[cfg(test)]
mod command_contract;
mod commands;
mod config;
mod config_contract;
mod connection;
mod credentials;
mod subagents;
mod types;

pub use commands::*;

/// Native-only readiness check (official Claude/Codex servers removed).
pub(crate) fn validate_backend_ready_for_send(backend: &str) -> Result<(), String> {
    if backend == "native-agentkit" || backend == "claude" || backend == "codex" {
        // Brand backends are no longer execution paths; turns use native-agentkit.
        return Ok(());
    }
    if backend.trim().is_empty() {
        return Err("未选择 Agent 后端".to_string());
    }
    Ok(())
}

pub(crate) use config::{
    assistant_ai_secret, backend_api_key_env, backend_direct_url, load_active_backend,
    load_agent_interaction_settings, load_assistant_ai_config, load_model_feature_settings,
    normalize_permission_mode,
};
pub(crate) use connection::resolve_connection_for;
pub(crate) use types::{
    AssistantAIConfig, AutoTurnDecisionSettings, BackendConnectionPlan, CodexProfileSettings,
    ConnectionMode, ModelFeatureSettings,
};

#[cfg(test)]
pub(crate) use types::*;

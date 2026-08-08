use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::agent_interaction_defaults_contract;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentInteractionSettings {
    #[serde(default)]
    pub(crate) non_interrupt_mode: bool,
    #[serde(default)]
    pub(crate) debug: bool,
    #[serde(default = "default_permission_mode")]
    pub(crate) permission_mode: String,
    #[serde(default = "default_permission_mode_availability")]
    pub(crate) permission_mode_availability: HashMap<String, bool>,
    #[serde(default = "default_main_agent_prompt_mode")]
    pub(crate) main_agent_prompt_mode: String,
    #[serde(default)]
    pub(crate) main_agent_custom_prompt: String,
    #[serde(default)]
    pub(crate) codex_profile: CodexProfileSettings,
    #[serde(default)]
    pub(crate) subagent_mode: SubagentModeSettings,
    #[serde(default)]
    pub(crate) auto_turn_decision: AutoTurnDecisionSettings,
}

impl Default for AgentInteractionSettings {
    fn default() -> Self {
        agent_interaction_defaults_contract::agent_interaction_settings()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubagentBackendSettings {
    #[serde(default = "default_codex_subagent_enabled")]
    pub(crate) enabled: bool,
}

impl Default for SubagentBackendSettings {
    fn default() -> Self {
        agent_interaction_defaults_contract::subagent_backend_settings()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeSubagentModeSettings {
    #[serde(default = "default_claude_subagent_enabled")]
    pub(crate) enabled: bool,
    #[serde(default = "default_claude_forward_subagent_text")]
    pub(crate) forward_subagent_text: bool,
    #[serde(default = "default_claude_agent_progress_summaries")]
    pub(crate) agent_progress_summaries: bool,
}

impl Default for ClaudeSubagentModeSettings {
    fn default() -> Self {
        agent_interaction_defaults_contract::claude_subagent_mode_settings()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubagentModeSettings {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) codex: SubagentBackendSettings,
    #[serde(default)]
    pub(crate) claude: ClaudeSubagentModeSettings,
}

impl Default for SubagentModeSettings {
    fn default() -> Self {
        agent_interaction_defaults_contract::subagent_mode_settings()
    }
}

fn default_codex_subagent_enabled() -> bool {
    agent_interaction_defaults_contract::codex_subagent_enabled()
}

fn default_claude_subagent_enabled() -> bool {
    agent_interaction_defaults_contract::claude_subagent_enabled()
}

fn default_claude_forward_subagent_text() -> bool {
    agent_interaction_defaults_contract::claude_forward_subagent_text()
}

fn default_claude_agent_progress_summaries() -> bool {
    agent_interaction_defaults_contract::claude_agent_progress_summaries()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CustomSubagentDefinition {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) instruction: String,
    #[serde(default)]
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutoTurnDecisionSettings {
    #[serde(default = "default_auto_turn_decision_enabled")]
    pub(crate) enabled: bool,
    #[serde(default = "default_auto_turn_decision_allow_model_tier")]
    pub(crate) allow_model_tier: bool,
    #[serde(default = "default_auto_turn_decision_allow_reasoning_effort")]
    pub(crate) allow_reasoning_effort: bool,
    #[serde(default = "default_auto_turn_decision_allow_plan_mode")]
    pub(crate) allow_plan_mode: bool,
    #[serde(default = "default_auto_turn_decision_allow_goal_mode")]
    pub(crate) allow_goal_mode: bool,
    #[serde(default = "default_auto_turn_decision_allow_session_fork")]
    pub(crate) allow_session_fork: bool,
}

impl Default for AutoTurnDecisionSettings {
    fn default() -> Self {
        agent_interaction_defaults_contract::auto_turn_decision_settings()
    }
}

fn default_auto_turn_decision_enabled() -> bool {
    agent_interaction_defaults_contract::auto_turn_decision_enabled()
}

fn default_auto_turn_decision_allow_model_tier() -> bool {
    agent_interaction_defaults_contract::auto_turn_decision_allow_model_tier()
}

fn default_auto_turn_decision_allow_reasoning_effort() -> bool {
    agent_interaction_defaults_contract::auto_turn_decision_allow_reasoning_effort()
}

fn default_auto_turn_decision_allow_plan_mode() -> bool {
    agent_interaction_defaults_contract::auto_turn_decision_allow_plan_mode()
}

fn default_auto_turn_decision_allow_goal_mode() -> bool {
    agent_interaction_defaults_contract::auto_turn_decision_allow_goal_mode()
}

fn default_auto_turn_decision_allow_session_fork() -> bool {
    agent_interaction_defaults_contract::auto_turn_decision_allow_session_fork()
}

fn default_permission_mode() -> String {
    agent_interaction_defaults_contract::permission_mode()
}

fn default_permission_mode_availability() -> HashMap<String, bool> {
    agent_interaction_defaults_contract::permission_mode_availability()
}

fn default_main_agent_prompt_mode() -> String {
    agent_interaction_defaults_contract::main_agent_prompt_mode()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexProfileSettings {
    #[serde(default = "default_codex_profile_name")]
    pub(crate) profile: String,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) reasoning_effort: Option<String>,
    #[serde(default)]
    pub(crate) runtime_workspace_roots: Vec<String>,
    #[serde(default)]
    pub(crate) responses_api_client_metadata: Option<JsonValue>,
    #[serde(default)]
    pub(crate) additional_context: Option<String>,
    #[serde(default)]
    pub(crate) persist_extended_history: Option<bool>,
    #[serde(default)]
    pub(crate) initial_turns_page: Option<JsonValue>,
    #[serde(default)]
    pub(crate) exclude_turns: Vec<String>,
}

impl Default for CodexProfileSettings {
    fn default() -> Self {
        agent_interaction_defaults_contract::codex_profile_settings()
    }
}

fn default_codex_profile_name() -> String {
    agent_interaction_defaults_contract::codex_profile_name()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderConfig {
    pub(crate) backend: String,
    pub(crate) base_url: Option<String>,
    pub(crate) api_key: Option<String>,
    #[serde(default)]
    pub(crate) has_api_key: bool,
    #[serde(default)]
    pub(crate) clear_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssistantAIConfig {
    pub(crate) base_url: Option<String>,
    pub(crate) api_key: Option<String>,
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) model_pool: Vec<AssistantAIModelPoolItem>,
    #[serde(default)]
    pub(crate) codex_account_spark_enabled: bool,
    #[serde(default)]
    pub(crate) has_api_key: bool,
    #[serde(default)]
    pub(crate) clear_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssistantAIModelPoolItem {
    pub(crate) id: String,
    pub(crate) label: String,
    #[serde(default = "default_model_pool_source")]
    pub(crate) source: String,
    #[serde(default = "default_model_pool_backend")]
    pub(crate) backend: String,
}

fn default_model_pool_source() -> String {
    "remote".to_string()
}

fn default_model_pool_backend() -> String {
    "codex".to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssistantAITestResult {
    pub(crate) ok: bool,
    pub(crate) error: Option<String>,
    pub(crate) models: Option<Vec<String>>,
    pub(crate) model_matched: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssistantAIModelsResult {
    pub(crate) ok: bool,
    pub(crate) error: Option<String>,
    pub(crate) models: Vec<AssistantAIModelPoolItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelFeatureChatSettings {
    #[serde(default)]
    pub(crate) light: Option<String>,
    #[serde(default)]
    pub(crate) normal: Option<String>,
    #[serde(default)]
    pub(crate) deep: Option<String>,
}

impl Default for ModelFeatureChatSettings {
    fn default() -> Self {
        Self {
            light: None,
            normal: None,
            deep: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelFeatureSettings {
    #[serde(default)]
    pub(crate) chat: ModelFeatureChatSettings,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) suggestion: Option<String>,
    #[serde(default)]
    pub(crate) prompt_router: Option<String>,
    #[serde(default)]
    pub(crate) prompt_optimize: Option<String>,
    #[serde(default)]
    pub(crate) auto_turn_decision: Option<String>,
}

impl Default for ModelFeatureSettings {
    fn default() -> Self {
        Self {
            chat: ModelFeatureChatSettings::default(),
            title: None,
            suggestion: None,
            prompt_router: None,
            prompt_optimize: None,
            auto_turn_decision: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackendEnvStatus {
    pub(crate) backend: String,
    pub(crate) has_api_key: bool,
    pub(crate) connection_mode: ConnectionMode,
    pub(crate) effective_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexAppServerStatus {
    pub(crate) version: Option<String>,
    pub(crate) install_path: Option<String>,
    pub(crate) managed: bool,
    pub(crate) available: bool,
    pub(crate) supports_required_protocol: bool,
    pub(crate) failure_kind: Option<String>,
    pub(crate) issues: Vec<String>,
    pub(crate) latest_version: Option<String>,
    pub(crate) update_available: bool,
    pub(crate) release_notes: Vec<String>,
    pub(crate) update_error: Option<String>,
    pub(crate) update_state: String,
    pub(crate) prepared_version: Option<String>,
    pub(crate) update_progress_percent: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EnvStatusReport {
    pub(crate) node_available: bool,
    pub(crate) codex_cli_available: bool,
    pub(crate) codex_app_server: CodexAppServerStatus,
    pub(crate) router_modes: HashMap<String, String>,
    pub(crate) backends: HashMap<String, BackendEnvStatus>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum ConnectionMode {
    #[serde(rename = "api")]
    Api,
    #[serde(rename = "custom")]
    CustomBaseUrl,
    #[serde(rename = "codex-account")]
    CodexAccount,
    #[serde(rename = "unconfigured")]
    Unconfigured,
}

impl ConnectionMode {
    pub(crate) fn as_contract_value(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::CustomBaseUrl => "custom",
            Self::CodexAccount => "codex-account",
            Self::Unconfigured => "unconfigured",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BackendConnectionPlan {
    pub(crate) mode: ConnectionMode,
    pub(crate) base_url: Option<String>,
    pub(crate) api_key: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_interaction_defaults_load_from_contracts_manifest() {
        let settings = AgentInteractionSettings::default();

        assert!(!settings.non_interrupt_mode);
        assert!(!settings.debug);
        assert_eq!(settings.permission_mode, "ask");
        assert_eq!(settings.main_agent_prompt_mode, "conservative");
        assert!(settings.main_agent_custom_prompt.is_empty());
        assert_eq!(settings.codex_profile.profile, "default");
        assert!(settings.codex_profile.model.is_none());
        assert!(settings.codex_profile.runtime_workspace_roots.is_empty());
        assert!(!settings.subagent_mode.enabled);
        assert!(settings.subagent_mode.codex.enabled);
        assert!(settings.subagent_mode.claude.enabled);
        assert!(settings.subagent_mode.claude.forward_subagent_text);
        assert!(settings.subagent_mode.claude.agent_progress_summaries);
        assert!(settings.auto_turn_decision.enabled);
        assert!(settings.auto_turn_decision.allow_model_tier);
        assert!(settings.auto_turn_decision.allow_reasoning_effort);
        assert!(settings.auto_turn_decision.allow_plan_mode);
        assert!(settings.auto_turn_decision.allow_goal_mode);
        assert!(settings.auto_turn_decision.allow_session_fork);
    }

    #[test]
    fn missing_agent_interaction_fields_deserialize_from_contracts_defaults() {
        let settings: AgentInteractionSettings = serde_json::from_value(json!({})).unwrap();

        assert_eq!(settings.permission_mode, default_permission_mode());
        assert_eq!(
            settings.main_agent_prompt_mode,
            default_main_agent_prompt_mode()
        );
        assert!(settings.main_agent_custom_prompt.is_empty());
        assert_eq!(settings.codex_profile, CodexProfileSettings::default());
        assert_eq!(
            settings.subagent_mode.enabled,
            SubagentModeSettings::default().enabled
        );
        assert_eq!(
            settings.subagent_mode.codex.enabled,
            SubagentModeSettings::default().codex.enabled
        );
        assert_eq!(
            settings.auto_turn_decision.allow_session_fork,
            AutoTurnDecisionSettings::default().allow_session_fork
        );
    }

    #[test]
    fn missing_nested_agent_interaction_fields_deserialize_from_contracts_defaults() {
        let settings: AgentInteractionSettings = serde_json::from_value(json!({
            "codexProfile": {},
            "subagentMode": {
                "codex": {},
                "claude": {}
            },
            "autoTurnDecision": {}
        }))
        .unwrap();

        assert_eq!(settings.codex_profile, CodexProfileSettings::default());
        assert_eq!(settings.subagent_mode.codex.enabled, true);
        assert_eq!(settings.subagent_mode.claude.enabled, true);
        assert_eq!(settings.subagent_mode.claude.forward_subagent_text, true);
        assert_eq!(
            settings.auto_turn_decision,
            AutoTurnDecisionSettings::default()
        );
    }

    #[test]
    fn backend_env_status_serializes_connection_mode_contract_value() {
        let mode: ConnectionMode = serde_json::from_value(json!("codex-account")).unwrap();
        assert_eq!(mode, ConnectionMode::CodexAccount);
        assert_eq!(mode.as_contract_value(), "codex-account");

        let value = serde_json::to_value(BackendEnvStatus {
            backend: "codex".to_string(),
            has_api_key: false,
            connection_mode: ConnectionMode::CodexAccount,
            effective_url: None,
        })
        .unwrap();

        assert_eq!(value["connectionMode"], json!("codex-account"));
    }
}

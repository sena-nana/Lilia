use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use super::types::{
    AgentInteractionSettings, AutoTurnDecisionSettings, ClaudeSubagentModeSettings,
    CodexProfileSettings, SubagentBackendSettings, SubagentModeSettings,
};

const AGENT_INTERACTION_DEFAULTS_JSON: &str =
    include_str!("../../../../../packages/contracts/src/agent-interaction-defaults.json");

static AGENT_INTERACTION_DEFAULTS: OnceLock<AgentInteractionSettings> = OnceLock::new();

/// Wire shape of packages/contracts agent-interaction-defaults.json (flat subagentMode).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAgentInteractionDefaults {
    #[serde(default)]
    non_interrupt_mode: bool,
    #[serde(default)]
    debug: bool,
    #[serde(default = "default_permission_mode")]
    permission_mode: String,
    #[serde(default = "default_permission_availability")]
    permission_mode_availability: HashMap<String, bool>,
    #[serde(default = "default_prompt_mode")]
    main_agent_prompt_mode: String,
    #[serde(default)]
    main_agent_custom_prompt: String,
    #[serde(default)]
    subagent_mode: RawSubagentMode,
    #[serde(default)]
    auto_turn_decision: RawAutoTurnDecision,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSubagentMode {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_true")]
    forward_subagent_text: bool,
    #[serde(default = "default_true")]
    agent_progress_summaries: bool,
}

impl Default for RawSubagentMode {
    fn default() -> Self {
        Self {
            enabled: false,
            forward_subagent_text: true,
            agent_progress_summaries: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAutoTurnDecision {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_true", alias = "allowModelTiers")]
    allow_model_tier: bool,
    #[serde(default = "default_true")]
    allow_reasoning_effort: bool,
    #[serde(default = "default_true")]
    allow_plan_mode: bool,
    #[serde(default = "default_true")]
    allow_goal_mode: bool,
    #[serde(default = "default_true")]
    allow_session_fork: bool,
}

impl Default for RawAutoTurnDecision {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_model_tier: true,
            allow_reasoning_effort: true,
            allow_plan_mode: true,
            allow_goal_mode: true,
            allow_session_fork: true,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_permission_mode() -> String {
    "ask".to_string()
}

fn default_prompt_mode() -> String {
    "conservative".to_string()
}

fn default_permission_availability() -> HashMap<String, bool> {
    HashMap::from([
        ("full".to_string(), true),
        ("ask".to_string(), true),
        ("readonly".to_string(), true),
        ("free".to_string(), true),
    ])
}

pub(super) fn agent_interaction_settings() -> AgentInteractionSettings {
    AGENT_INTERACTION_DEFAULTS
        .get_or_init(|| {
            let raw: RawAgentInteractionDefaults = crate::contract_manifest::parse_contract_json(
                AGENT_INTERACTION_DEFAULTS_JSON,
                "agent-interaction-defaults.json",
            );
            AgentInteractionSettings {
                non_interrupt_mode: raw.non_interrupt_mode,
                debug: raw.debug,
                permission_mode: raw.permission_mode,
                permission_mode_availability: raw.permission_mode_availability,
                main_agent_prompt_mode: raw.main_agent_prompt_mode,
                main_agent_custom_prompt: raw.main_agent_custom_prompt,
                codex_profile: CodexProfileSettings::default_non_reentrant(),
                subagent_mode: SubagentModeSettings {
                    enabled: raw.subagent_mode.enabled,
                    codex: SubagentBackendSettings { enabled: true },
                    claude: ClaudeSubagentModeSettings {
                        enabled: true,
                        forward_subagent_text: raw.subagent_mode.forward_subagent_text,
                        agent_progress_summaries: raw.subagent_mode.agent_progress_summaries,
                    },
                },
                auto_turn_decision: AutoTurnDecisionSettings {
                    enabled: raw.auto_turn_decision.enabled,
                    allow_model_tier: raw.auto_turn_decision.allow_model_tier,
                    allow_reasoning_effort: raw.auto_turn_decision.allow_reasoning_effort,
                    allow_plan_mode: raw.auto_turn_decision.allow_plan_mode,
                    allow_goal_mode: raw.auto_turn_decision.allow_goal_mode,
                    allow_session_fork: raw.auto_turn_decision.allow_session_fork,
                },
            }
        })
        .clone()
}

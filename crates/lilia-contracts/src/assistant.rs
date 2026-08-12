use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

const PROMPT_TEXT_JSON: &str = include_str!("../../../packages/contracts/src/prompt-text.json");
const MODEL_SELECTION_DEFAULTS_JSON: &str =
    include_str!("../../../packages/contracts/src/model-selection-defaults.json");

static PROMPTS: OnceLock<PromptContract> = OnceLock::new();
static MODEL_SELECTION: OnceLock<ModelSelectionContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptContract {
    assistant: AssistantPrompts,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssistantPrompts {
    auto_turn_decision: AutoTurnDecisionPrompts,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutoTurnDecisionPrompts {
    system_instruction: String,
    request_instruction: String,
    tier_policy: AutoTurnDecisionTierPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoTurnDecisionTierPolicy {
    pub light: String,
    pub normal: String,
    pub deep: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelSelectionContract {
    auto_model_families: BTreeMap<String, BTreeMap<String, String>>,
    auto_reasoning_efforts: BTreeMap<String, String>,
}

pub fn auto_turn_decision_system_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .auto_turn_decision
        .system_instruction
}

pub fn auto_turn_decision_request_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .auto_turn_decision
        .request_instruction
}

pub fn auto_turn_decision_tier_policy() -> &'static AutoTurnDecisionTierPolicy {
    &prompt_contract().assistant.auto_turn_decision.tier_policy
}

pub fn auto_model_for_provider_family_tier(family: &str, tier: &str) -> Option<&'static str> {
    model_selection_contract()
        .auto_model_families
        .get(family)
        .and_then(|models| models.get(tier))
        .map(String::as_str)
}

pub fn auto_reasoning_effort_for_tier(tier: &str) -> Option<&'static str> {
    model_selection_contract()
        .auto_reasoning_efforts
        .get(tier)
        .map(String::as_str)
}

fn prompt_contract() -> &'static PromptContract {
    PROMPTS.get_or_init(|| {
        serde_json::from_str(PROMPT_TEXT_JSON)
            .expect("packages/contracts prompt-text.json must match the Rust contract")
    })
}

fn model_selection_contract() -> &'static ModelSelectionContract {
    MODEL_SELECTION.get_or_init(|| {
        serde_json::from_str(MODEL_SELECTION_DEFAULTS_JSON)
            .expect("packages/contracts model-selection-defaults.json must match the Rust contract")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_auto_turn_contract_exposes_prompts_and_provider_family_models() {
        assert!(!auto_turn_decision_system_instruction().trim().is_empty());
        assert!(!auto_turn_decision_request_instruction().trim().is_empty());
        assert_eq!(
            auto_model_for_provider_family_tier("openai", "deep"),
            Some("gpt-5.5")
        );
        assert_eq!(
            auto_model_for_provider_family_tier("anthropic", "light"),
            Some("claude-haiku-4-5")
        );
        assert_eq!(auto_reasoning_effort_for_tier("normal"), Some("medium"));
    }
}

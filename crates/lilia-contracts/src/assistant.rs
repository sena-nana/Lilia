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
    main_agent: MainAgentPrompts,
    assistant: AssistantPrompts,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MainAgentPrompts {
    base_prompt: String,
    tools_prompt: String,
    workflow_types: BTreeMap<String, MainAgentWorkflowPrompt>,
    modes: MainAgentModePrompts,
    workflow_order: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MainAgentWorkflowPrompt {
    title: String,
    summary: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct MainAgentModePrompts {
    conservative: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssistantPrompts {
    prompt_router: PromptRouterPrompts,
    prompt_optimize: PromptOptimizePrompts,
    auto_turn_decision: AutoTurnDecisionPrompts,
    context_compaction: ContextCompactionPrompts,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptRouterPrompts {
    system_instruction: String,
    request_instruction: String,
    requirements: Vec<String>,
    scenarios: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptOptimizePrompts {
    system_instruction: String,
    request_instruction: String,
    requirements: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutoTurnDecisionPrompts {
    system_instruction: String,
    request_instruction: String,
    tier_policy: AutoTurnDecisionTierPolicy,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContextCompactionPrompts {
    system_instruction: String,
    request_instruction: String,
    success_message: String,
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
    auto_preset_reasoning_efforts: BTreeMap<String, String>,
    preset_tier_map: BTreeMap<String, String>,
    builtin_preset_labels: BTreeMap<String, String>,
    auto_preset_rules: AutoPresetRules,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutoPresetRules {
    plan_mode_preset: String,
    workflow_presets: BTreeMap<String, Vec<String>>,
    context_scale_presets: BTreeMap<String, String>,
    context_thresholds: BTreeMap<String, ModelSelectionContextThresholds>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelectionContextThresholds {
    pub context_usage_percent: f64,
    pub prompt_length: usize,
    pub attachment_count: usize,
    pub conversation_reference_count: usize,
    #[serde(default)]
    pub directory_file_count: Option<u64>,
    #[serde(default)]
    pub directory_total_size: Option<u64>,
}

pub fn auto_turn_decision_system_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .auto_turn_decision
        .system_instruction
}

pub fn prompt_router_system_instruction() -> &'static str {
    &prompt_contract().assistant.prompt_router.system_instruction
}

pub fn prompt_router_request_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .prompt_router
        .request_instruction
}

pub fn prompt_router_requirements() -> &'static [String] {
    &prompt_contract().assistant.prompt_router.requirements
}

pub fn prompt_router_scenarios() -> &'static [String] {
    &prompt_contract().assistant.prompt_router.scenarios
}

pub fn prompt_optimize_system_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .prompt_optimize
        .system_instruction
}

pub fn prompt_optimize_request_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .prompt_optimize
        .request_instruction
}

pub fn prompt_optimize_requirements() -> &'static [String] {
    &prompt_contract().assistant.prompt_optimize.requirements
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

pub fn context_compaction_system_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .context_compaction
        .system_instruction
}

pub fn context_compaction_request_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .context_compaction
        .request_instruction
}

pub fn context_compaction_success_message() -> &'static str {
    &prompt_contract()
        .assistant
        .context_compaction
        .success_message
}

pub fn main_agent_system_instruction() -> String {
    let prompts = &prompt_contract().main_agent;
    let workflows = prompts
        .workflow_order
        .iter()
        .filter_map(|key| prompts.workflow_types.get(key))
        .map(|workflow| {
            format!(
                "## {}\n{}\n\n{}",
                workflow.title.trim(),
                workflow.summary.trim(),
                workflow.prompt.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    [
        prompts.base_prompt.trim(),
        prompts.modes.conservative.trim(),
        prompts.tools_prompt.trim(),
        workflows.trim(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n")
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

pub fn auto_reasoning_effort_for_preset(preset: &str) -> Option<&'static str> {
    model_selection_contract()
        .auto_preset_reasoning_efforts
        .get(preset)
        .map(String::as_str)
}

pub fn tier_for_preset(preset: &str) -> Option<&'static str> {
    model_selection_contract()
        .preset_tier_map
        .get(preset)
        .map(String::as_str)
}

pub fn builtin_preset_label(preset: &str) -> Option<&'static str> {
    model_selection_contract()
        .builtin_preset_labels
        .get(preset)
        .map(String::as_str)
}

pub fn plan_mode_preset() -> &'static str {
    &model_selection_contract()
        .auto_preset_rules
        .plan_mode_preset
}

pub fn auto_preset_for_workflow_type(workflow: &str) -> Option<&'static str> {
    model_selection_contract()
        .auto_preset_rules
        .workflow_presets
        .iter()
        .find_map(|(preset, workflows)| {
            workflows
                .iter()
                .any(|candidate| candidate == workflow)
                .then_some(preset.as_str())
        })
}

pub fn auto_preset_for_context_scale(scale: &str) -> Option<&'static str> {
    model_selection_contract()
        .auto_preset_rules
        .context_scale_presets
        .get(scale)
        .map(String::as_str)
}

pub fn auto_context_thresholds_for_scale(
    scale: &str,
) -> Option<&'static ModelSelectionContextThresholds> {
    model_selection_contract()
        .auto_preset_rules
        .context_thresholds
        .get(scale)
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
        assert_eq!(auto_reasoning_effort_for_preset("review"), Some("high"));
        assert_eq!(tier_for_preset("plan"), Some("deep"));
        assert_eq!(builtin_preset_label("fast"), Some("Fast"));
        assert_eq!(plan_mode_preset(), "plan");
        assert_eq!(
            auto_preset_for_workflow_type("lilia_review"),
            Some("review")
        );
        assert_eq!(auto_preset_for_context_scale("medium"), Some("default"));
        let large = auto_context_thresholds_for_scale("large").unwrap();
        assert_eq!(large.prompt_length, 8_000);
        assert_eq!(large.directory_file_count, Some(200));
        assert!(!prompt_router_system_instruction().trim().is_empty());
        assert!(!prompt_router_request_instruction().trim().is_empty());
        assert!(!prompt_router_requirements().is_empty());
        assert!(!prompt_router_scenarios().is_empty());
        assert!(!prompt_optimize_system_instruction().trim().is_empty());
        assert!(!prompt_optimize_request_instruction().trim().is_empty());
        assert!(!prompt_optimize_requirements().is_empty());
        assert!(!context_compaction_system_instruction().trim().is_empty());
        assert!(!context_compaction_request_instruction().trim().is_empty());
        assert!(!context_compaction_success_message().trim().is_empty());
    }

    #[test]
    fn shared_main_agent_instruction_assembles_declared_workflows_in_contract_order() {
        let prompts = &prompt_contract().main_agent;
        let instruction = main_agent_system_instruction();

        let positions = prompts
            .workflow_order
            .iter()
            .map(|key| {
                let workflow = prompts.workflow_types.get(key).unwrap();
                instruction.find(&workflow.title).unwrap()
            })
            .collect::<Vec<_>>();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(instruction.starts_with(prompts.base_prompt.trim()));
        assert!(instruction.contains(prompts.tools_prompt.trim()));
    }
}

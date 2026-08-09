use std::sync::OnceLock;

use serde::Deserialize;

const PROMPT_TEXT_JSON: &str = include_str!("../../../../packages/contracts/src/prompt-text.json");

static PROMPT_CONTRACT: OnceLock<PromptContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptContract {
    #[allow(dead_code)] // tests / native pack consumers
    main_agent: MainAgentPrompts,
    /// Mutsuki / Lilia native prompt pack (subagents, plan, workflows).
    #[allow(dead_code)]
    native: NativePrompts,
    assistant: AssistantPrompts,
    suggestion: SuggestionPrompts,
    title: TitlePrompts,
    automation: AutomationPrompts,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MainAgentPrompts {
    pub(crate) base_prompt: String,
    pub(crate) tools_prompt: String,
    pub(crate) workflow_types: std::collections::BTreeMap<String, MainAgentWorkflowPrompt>,
    pub(crate) modes: MainAgentModePrompts,
    pub(crate) workflow_order: Vec<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MainAgentWorkflowPrompt {
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) prompt: String,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MainAgentModePrompts {
    pub(crate) conservative: String,
    pub(crate) aggressive: String,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativePrompts {
    subagents: NativeSubagentPrompts,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSubagentPrompts {
    pub(crate) delegation_header: String,
    pub(crate) baseline_intro: String,
    pub(crate) baseline_prompt: String,
    pub(crate) baseline_label: String,
    pub(crate) available_label: String,
    pub(crate) agent_specific_instruction_label: String,
    pub(crate) agent_role_label: String,
    pub(crate) delegation_rules_label: String,
    pub(crate) delegation_rules: Vec<String>,
}



#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssistantPrompts {
    prompt_router: PromptRouterPrompts,
    prompt_optimize: PromptOptimizePrompts,
    auto_turn_decision: AutoTurnDecisionPrompts,
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
pub(crate) struct AutoTurnDecisionTierPolicy {
    pub(crate) light: String,
    pub(crate) normal: String,
    pub(crate) deep: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SuggestionPrompts {
    system_instruction: String,
    generation_rules: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TitlePrompts {
    system_instruction: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutomationPrompts {
    default_agent_prompt: String,
    default_human_prompt: String,
}

fn prompt_contract() -> &'static PromptContract {
    PROMPT_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(PROMPT_TEXT_JSON, "prompt-text.json")
    })
}

pub(crate) fn prompt_optimize_system_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .prompt_optimize
        .system_instruction
}

#[cfg(test)]
pub(crate) fn main_agent_prompts() -> &'static MainAgentPrompts {
    &prompt_contract().main_agent
}

#[cfg(test)]
pub(crate) fn main_agent_prompt_mode(mode: &str) -> &'static str {
    match mode {
        "aggressive" => &prompt_contract().main_agent.modes.aggressive,
        _ => &prompt_contract().main_agent.modes.conservative,
    }
}

#[cfg(test)]
pub(crate) fn build_main_agent_prompt(mode: &str, custom_prompt: Option<&str>) -> String {
    let prompts = main_agent_prompts();
    let strategy_prompt = match mode {
        "custom" => custom_prompt
            .map(str::trim)
            .filter(|prompt| !prompt.is_empty())
            .unwrap_or_else(|| main_agent_prompt_mode("conservative")),
        _ => main_agent_prompt_mode(mode),
    };
    let mut parts = vec![
        prompts.base_prompt.trim().to_string(),
        strategy_prompt.trim().to_string(),
        prompts.tools_prompt.trim().to_string(),
    ];
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
    if !workflows.trim().is_empty() {
        parts.push(workflows);
    }
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn prompt_optimize_request_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .prompt_optimize
        .request_instruction
}

pub(crate) fn prompt_optimize_requirements() -> &'static [String] {
    &prompt_contract().assistant.prompt_optimize.requirements
}

pub(crate) fn prompt_router_system_instruction() -> &'static str {
    &prompt_contract().assistant.prompt_router.system_instruction
}

pub(crate) fn prompt_router_request_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .prompt_router
        .request_instruction
}

pub(crate) fn prompt_router_requirements() -> &'static [String] {
    &prompt_contract().assistant.prompt_router.requirements
}

pub(crate) fn prompt_router_scenarios() -> &'static [String] {
    &prompt_contract().assistant.prompt_router.scenarios
}

pub(crate) fn auto_turn_decision_system_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .auto_turn_decision
        .system_instruction
}

pub(crate) fn auto_turn_decision_request_instruction() -> &'static str {
    &prompt_contract()
        .assistant
        .auto_turn_decision
        .request_instruction
}

pub(crate) fn auto_turn_decision_tier_policy() -> &'static AutoTurnDecisionTierPolicy {
    &prompt_contract().assistant.auto_turn_decision.tier_policy
}

#[cfg(test)]
pub(crate) fn native_subagent_prompts() -> &'static NativeSubagentPrompts {
    &prompt_contract().native.subagents
}

#[cfg(test)]
pub(crate) fn codex_subagent_prompts() -> &'static NativeSubagentPrompts {
    native_subagent_prompts()
}

pub(crate) fn suggestion_system_instruction() -> &'static str {
    &prompt_contract().suggestion.system_instruction
}

pub(crate) fn suggestion_generation_rules() -> &'static [String] {
    &prompt_contract().suggestion.generation_rules
}

pub(crate) fn title_system_instruction() -> &'static str {
    &prompt_contract().title.system_instruction
}

pub(crate) fn default_automation_agent_prompt() -> &'static str {
    &prompt_contract().automation.default_agent_prompt
}

pub(crate) fn default_automation_human_prompt() -> &'static str {
    &prompt_contract().automation.default_human_prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_contract_loads_shared_defaults() {
        assert!(!default_automation_agent_prompt().trim().is_empty());
        assert!(!default_automation_human_prompt().trim().is_empty());
        let main_agent = main_agent_prompts();
        assert!(!main_agent.base_prompt.trim().is_empty());
        assert!(!main_agent.tools_prompt.trim().is_empty());
        assert!(!main_agent.workflow_types.is_empty());
        assert!(!main_agent.workflow_order.is_empty());
        assert!(!main_agent_prompt_mode("conservative").trim().is_empty());
        assert!(!main_agent_prompt_mode("aggressive").trim().is_empty());
        assert_eq!(
            main_agent.workflow_order.len(),
            main_agent.workflow_types.len(),
            "mainAgent.workflowOrder must list every configured workflow type"
        );
        let built_main_agent_prompt = build_main_agent_prompt("aggressive", None);
        assert!(built_main_agent_prompt.len() > main_agent.base_prompt.len());
        assert!(
            built_main_agent_prompt.contains("不替代 Mutsuki AgentKit / 模型协议 adapter 的系统提示"),
            "mainAgent prompt must preserve AgentKit / adapter system prompt precedence"
        );
        for key in main_agent.workflow_types.keys() {
            assert!(main_agent.workflow_order.contains(key));
        }
        for key in &main_agent.workflow_order {
            let workflow = main_agent.workflow_types.get(key).unwrap_or_else(|| {
                panic!("mainAgent.workflowOrder references missing workflow type: {key}")
            });
            assert!(
                !workflow.title.trim().is_empty(),
                "mainAgent workflow {key} must have a title"
            );
            assert!(
                !workflow.summary.trim().is_empty(),
                "mainAgent workflow {key} must have a summary"
            );
            assert!(
                !workflow.prompt.trim().is_empty(),
                "mainAgent workflow {key} must have a prompt"
            );
            assert!(
                built_main_agent_prompt.contains(&format!("## {}", workflow.title.trim())),
                "mainAgent prompt must include workflow title: {}",
                workflow.title
            );
        }
        assert!(!title_system_instruction().trim().is_empty());
        assert!(!prompt_router_system_instruction().trim().is_empty());
        assert!(!prompt_router_request_instruction().trim().is_empty());
        assert!(!prompt_router_requirements().is_empty());
        assert!(!prompt_router_scenarios().is_empty());
        assert!(!prompt_optimize_system_instruction().trim().is_empty());
        assert!(!prompt_optimize_request_instruction().trim().is_empty());
        assert!(!prompt_optimize_requirements().is_empty());
        assert!(!auto_turn_decision_system_instruction().trim().is_empty());
        assert!(!auto_turn_decision_request_instruction().trim().is_empty());
        assert!(!suggestion_system_instruction().trim().is_empty());
        assert!(!suggestion_generation_rules().is_empty());

        let subagent_prompts = codex_subagent_prompts();
        assert!(!subagent_prompts.baseline_prompt.trim().is_empty());
        assert!(!subagent_prompts
            .agent_specific_instruction_label
            .trim()
            .is_empty());
        assert!(!subagent_prompts.delegation_rules.is_empty());
    }

    #[test]
    fn custom_main_agent_prompt_replaces_strategy_segment_only() {
        let main_agent = main_agent_prompts();
        let built = build_main_agent_prompt("custom", Some("Custom strategy segment."));

        assert!(built.contains(main_agent.base_prompt.trim()));
        assert!(built.contains(main_agent.tools_prompt.trim()));
        assert!(built.contains("Custom strategy segment."));
        assert!(!built.contains(main_agent_prompt_mode("conservative").trim()));
        assert!(!built.contains(main_agent_prompt_mode("aggressive").trim()));
        for key in &main_agent.workflow_order {
            let workflow = main_agent.workflow_types.get(key).unwrap();
            assert!(built.contains(&format!("## {}", workflow.title.trim())));
        }
    }

    #[test]
    fn empty_custom_main_agent_prompt_uses_conservative_strategy() {
        let built = build_main_agent_prompt("custom", Some("   "));

        assert!(built.contains(main_agent_prompt_mode("conservative").trim()));
        assert!(!built.contains(main_agent_prompt_mode("aggressive").trim()));
    }
}

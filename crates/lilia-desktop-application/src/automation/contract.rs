use std::sync::OnceLock;

use serde::Deserialize;

const CONTRACT_JSON: &str =
    include_str!("../../../../packages/contracts/src/automation-contract.json");

static CONTRACT: OnceLock<AutomationContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutomationContract {
    automation_scope_event_kinds: Vec<String>,
    automation_scope_task_statuses: Vec<String>,
    default_automation_logic_kind: String,
    default_automation_logic_path: String,
    default_automation_agent_prompt: String,
    default_automation_human_prompt: String,
    default_automation_tool_action: String,
    default_automation_tool_priority: String,
}

fn contract() -> &'static AutomationContract {
    CONTRACT.get_or_init(|| {
        serde_json::from_str(CONTRACT_JSON)
            .expect("the checked-in automation contract must remain valid JSON")
    })
}

pub(super) fn scope_event_kinds() -> &'static [String] {
    &contract().automation_scope_event_kinds
}

pub(super) fn scope_task_statuses() -> &'static [String] {
    &contract().automation_scope_task_statuses
}

pub(super) fn default_logic_kind() -> &'static str {
    &contract().default_automation_logic_kind
}

pub(super) fn default_logic_path() -> &'static str {
    &contract().default_automation_logic_path
}

pub(super) fn default_agent_prompt() -> &'static str {
    &contract().default_automation_agent_prompt
}

pub(super) fn default_human_prompt() -> &'static str {
    &contract().default_automation_human_prompt
}

pub(super) fn default_tool_action() -> &'static str {
    &contract().default_automation_tool_action
}

pub(super) fn default_tool_priority() -> &'static str {
    &contract().default_automation_tool_priority
}

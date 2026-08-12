use std::sync::OnceLock;

use serde::Deserialize;

const AUTOMATION_CONTRACT_JSON: &str =
    include_str!("../../../../../packages/contracts/src/automation-contract.json");

static AUTOMATION_CONTRACT: OnceLock<AutomationContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutomationContract {
    automation_trigger_kinds: Vec<String>,
    default_automation_trigger_kind: String,
    default_automation_logic_kind: String,
    default_automation_logic_path: String,
    #[cfg(test)]
    automation_run_statuses: Vec<String>,
    automation_changed_event_name: String,
    automation_run_started_event_name: String,
    automation_run_updated_event_name: String,
    automation_run_finished_event_name: String,
    #[allow(dead_code)]
    automation_list_workflows_command: String,
    #[allow(dead_code)]
    automation_save_draft_command: String,
    #[allow(dead_code)]
    automation_publish_command: String,
    #[allow(dead_code)]
    automation_delete_workflow_command: String,
    #[allow(dead_code)]
    automation_set_enabled_command: String,
    #[allow(dead_code)]
    automation_run_once_command: String,
    #[allow(dead_code)]
    automation_resume_run_command: String,
    #[allow(dead_code)]
    automation_list_runs_command: String,
    #[allow(dead_code)]
    automation_get_run_command: String,
    #[cfg(test)]
    default_automation_run_status: String,
    automation_scope_event_kinds: Vec<String>,
    automation_scope_task_statuses: Vec<String>,
    default_automation_tool_action: String,
}

fn automation_contract() -> &'static AutomationContract {
    AUTOMATION_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            AUTOMATION_CONTRACT_JSON,
            "automation-contract.json",
        )
    })
}

fn required_trigger_kind(kind: &str) -> &'static str {
    automation_contract()
        .automation_trigger_kinds
        .iter()
        .find(|value| value.as_str() == kind)
        .map(String::as_str)
        .unwrap_or_else(|| panic!("automation-contract.json missing trigger kind {kind}"))
}

fn required_scope_event_kind(kind: &str) -> &'static str {
    automation_contract()
        .automation_scope_event_kinds
        .iter()
        .find(|value| value.as_str() == kind)
        .map(String::as_str)
        .unwrap_or_else(|| panic!("automation-contract.json missing scope event kind {kind}"))
}

fn required_scope_task_status(status: &str) -> &'static str {
    automation_contract()
        .automation_scope_task_statuses
        .iter()
        .find(|value| value.as_str() == status)
        .map(String::as_str)
        .unwrap_or_else(|| panic!("automation-contract.json missing scope task status {status}"))
}

pub(crate) fn default_trigger_kind() -> &'static str {
    &automation_contract().default_automation_trigger_kind
}

pub(crate) fn task_changed_trigger_kind() -> &'static str {
    required_trigger_kind("task_changed")
}

pub(crate) fn timeline_trigger_kind() -> &'static str {
    required_trigger_kind("timeline_event")
}

pub(crate) fn todo_trigger_kind() -> &'static str {
    required_trigger_kind("todo_changed")
}

pub(crate) fn interaction_trigger_kind() -> &'static str {
    required_trigger_kind("interaction_request")
}

pub(crate) fn default_logic_kind() -> &'static str {
    &automation_contract().default_automation_logic_kind
}

pub(crate) fn default_logic_path() -> &'static str {
    &automation_contract().default_automation_logic_path
}

pub(crate) fn default_agent_prompt() -> &'static str {
    crate::prompt_contract::default_automation_agent_prompt()
}

pub(crate) fn default_human_prompt() -> &'static str {
    crate::prompt_contract::default_automation_human_prompt()
}

#[cfg(test)]
pub(crate) fn scope_event_kinds() -> &'static [String] {
    &automation_contract().automation_scope_event_kinds
}

#[cfg(test)]
pub(crate) fn run_statuses() -> &'static [String] {
    &automation_contract().automation_run_statuses
}

pub(crate) fn changed_event_name() -> &'static str {
    &automation_contract().automation_changed_event_name
}

pub(crate) fn run_started_event_name() -> &'static str {
    &automation_contract().automation_run_started_event_name
}

pub(crate) fn run_updated_event_name() -> &'static str {
    &automation_contract().automation_run_updated_event_name
}

pub(crate) fn run_finished_event_name() -> &'static str {
    &automation_contract().automation_run_finished_event_name
}

#[cfg(test)]
pub(crate) fn default_run_status() -> &'static str {
    &automation_contract().default_automation_run_status
}

pub(crate) fn task_created_event_kind() -> &'static str {
    required_scope_event_kind("task_created")
}

pub(crate) fn task_status_changed_event_kind() -> &'static str {
    required_scope_event_kind("task_status_changed")
}

pub(crate) fn task_updated_event_kind() -> &'static str {
    required_scope_event_kind("task_updated")
}

pub(crate) fn waiting_task_status() -> &'static str {
    required_scope_task_status("waiting")
}

pub(crate) fn running_task_status() -> &'static str {
    required_scope_task_status("running")
}

pub(crate) fn default_tool_action() -> &'static str {
    &automation_contract().default_automation_tool_action
}

#[cfg(test)]
mod tests {
    use super::automation_contract;
    use crate::automation::commands::{
        automation_delete_workflow, automation_get_run, automation_list_runs,
        automation_list_workflows, automation_publish, automation_resume_run, automation_run_once,
        automation_save_draft, automation_set_enabled,
    };

    #[test]
    fn automation_command_contract_matches_rust_dispatch_names() {
        let _ = automation_list_workflows;
        let _ = automation_save_draft::<tauri::Wry>;
        let _ = automation_publish::<tauri::Wry>;
        let _ = automation_delete_workflow::<tauri::Wry>;
        let _ = automation_set_enabled::<tauri::Wry>;
        let _ = automation_run_once::<tauri::Wry>;
        let _ = automation_resume_run::<tauri::Wry>;
        let _ = automation_list_runs;
        let _ = automation_get_run;

        let contract = automation_contract();
        assert_eq!(
            contract.automation_list_workflows_command,
            stringify!(automation_list_workflows)
        );
        assert_eq!(
            contract.automation_save_draft_command,
            stringify!(automation_save_draft)
        );
        assert_eq!(
            contract.automation_publish_command,
            stringify!(automation_publish)
        );
        assert_eq!(
            contract.automation_delete_workflow_command,
            stringify!(automation_delete_workflow)
        );
        assert_eq!(
            contract.automation_set_enabled_command,
            stringify!(automation_set_enabled)
        );
        assert_eq!(
            contract.automation_run_once_command,
            stringify!(automation_run_once)
        );
        assert_eq!(
            contract.automation_resume_run_command,
            stringify!(automation_resume_run)
        );
        assert_eq!(
            contract.automation_list_runs_command,
            stringify!(automation_list_runs)
        );
        assert_eq!(
            contract.automation_get_run_command,
            stringify!(automation_get_run)
        );
    }
}

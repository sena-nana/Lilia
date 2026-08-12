use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

const TASK_STATUS_MANIFEST_JSON: &str =
    include_str!("../../../../packages/contracts/src/task-statuses.json");
const TASK_EVENTS_CONTRACT_JSON: &str =
    include_str!("../../../../packages/contracts/src/task-events-contract.json");

static TASK_STATUS_MANIFEST: OnceLock<TaskStatusManifest> = OnceLock::new();
static TASK_EVENTS_CONTRACT: OnceLock<TaskEventsContract> = OnceLock::new();

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg(test)]
pub(crate) struct MemorySettingsDefaults {
    pub(crate) enabled: bool,
    pub(crate) baseline_injection_enabled: bool,
    pub(crate) cooldown_turns: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskStatusManifest {
    task_statuses: Vec<String>,
    project_dashboard_active_statuses: Vec<String>,
    project_dashboard_blocked_statuses: Vec<String>,
    milestone_statuses: Vec<String>,
    #[cfg(test)]
    default_milestone_status: String,
    #[cfg(test)]
    default_memory_settings: MemorySettingsDefaults,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskEventsContract {
    tasks_changed_event_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TasksChangedEventPayload<'a> {
    project_id: Option<&'a str>,
}

fn task_status_manifest() -> &'static TaskStatusManifest {
    TASK_STATUS_MANIFEST.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            TASK_STATUS_MANIFEST_JSON,
            "task-statuses.json",
        )
    })
}

fn task_events_contract() -> &'static TaskEventsContract {
    TASK_EVENTS_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            TASK_EVENTS_CONTRACT_JSON,
            "task-events-contract.json",
        )
    })
}

pub(crate) fn task_statuses() -> &'static [String] {
    &task_status_manifest().task_statuses
}

pub(crate) fn tasks_changed_event_name() -> &'static str {
    &task_events_contract().tasks_changed_event_name
}

pub(crate) fn tasks_changed_event_payload(project_id: Option<&str>) -> JsonValue {
    serde_json::to_value(TasksChangedEventPayload { project_id })
        .expect("TasksChangedEventPayload must serialize")
}

pub(crate) fn milestone_statuses() -> &'static [String] {
    &task_status_manifest().milestone_statuses
}

pub(crate) fn project_dashboard_active_statuses() -> &'static [String] {
    &task_status_manifest().project_dashboard_active_statuses
}

pub(crate) fn project_dashboard_blocked_statuses() -> &'static [String] {
    &task_status_manifest().project_dashboard_blocked_statuses
}

#[cfg(test)]
pub(crate) fn default_milestone_status() -> &'static str {
    &task_status_manifest().default_milestone_status
}

#[cfg(test)]
pub(crate) fn default_memory_settings() -> MemorySettingsDefaults {
    task_status_manifest().default_memory_settings
}

fn required_task_status(status: &str) -> &'static str {
    task_statuses()
        .iter()
        .find(|value| value.as_str() == status)
        .map(String::as_str)
        .expect("task-statuses.json missing required task status")
}

pub(crate) fn running_task_status() -> &'static str {
    required_task_status("running")
}

pub(crate) fn waiting_task_status() -> &'static str {
    required_task_status("waiting")
}

#[cfg(test)]
fn validate_project_dashboard_statuses(statuses: &[String]) {
    for status in statuses {
        required_task_status(status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_milestone_and_memory_defaults_load_from_contract_manifest() {
        let task_statuses: Vec<&str> = task_statuses().iter().map(String::as_str).collect();
        assert_eq!(
            task_statuses,
            vec![
                "draft",
                "waiting",
                "running",
                "blocked",
                "done",
                "cancelled"
            ]
        );
        assert_eq!(running_task_status(), "running");
        assert_eq!(tasks_changed_event_name(), "tasks:changed");
        assert_eq!(
            tasks_changed_event_payload(Some("project-1")),
            serde_json::json!({ "projectId": "project-1" })
        );
        assert_eq!(
            tasks_changed_event_payload(None),
            serde_json::json!({ "projectId": null })
        );
        validate_project_dashboard_statuses(project_dashboard_active_statuses());
        validate_project_dashboard_statuses(project_dashboard_blocked_statuses());
        assert_eq!(
            project_dashboard_active_statuses()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["waiting", "running"]
        );
        assert_eq!(
            project_dashboard_blocked_statuses()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["blocked"]
        );

        let milestone_statuses: Vec<&str> =
            milestone_statuses().iter().map(String::as_str).collect();
        assert_eq!(
            milestone_statuses,
            vec!["upcoming", "in-progress", "done", "abandoned"]
        );
        assert_eq!(default_milestone_status(), "upcoming");

        let memory = default_memory_settings();
        assert!(memory.enabled);
        assert!(memory.baseline_injection_enabled);
        assert_eq!(memory.cooldown_turns, 5);
    }
}

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

const TODO_CONTRACT_JSON: &str =
    include_str!("../../../../../packages/contracts/src/todo-contract.json");

static TODO_CONTRACT: OnceLock<TodoContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TodoContract {
    #[cfg(test)]
    commands: TodoCommandsContract,
    task_todo_priorities: Vec<String>,
    default_task_todo_priority: String,
    task_todo_guide_statuses: Vec<String>,
    pending_task_todo_guide_status: String,
    todo_changed_event_name: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TodoCommandsContract {
    pub(crate) list: String,
    pub(crate) create: String,
    pub(crate) submit_guide: String,
    pub(crate) update: String,
    pub(crate) delete: String,
    pub(crate) apply_agent_event: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TodoChangedEventPayload<'a> {
    task_id: &'a str,
}

fn todo_contract() -> &'static TodoContract {
    TODO_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(TODO_CONTRACT_JSON, "todo-contract.json")
    })
}

#[cfg(test)]
pub(crate) fn commands() -> &'static TodoCommandsContract {
    &todo_contract().commands
}

pub(crate) fn priorities() -> &'static [String] {
    &todo_contract().task_todo_priorities
}

pub(crate) fn default_priority() -> &'static str {
    &todo_contract().default_task_todo_priority
}

pub(crate) fn guide_statuses() -> &'static [String] {
    &todo_contract().task_todo_guide_statuses
}

pub(crate) fn pending_guide_status() -> &'static str {
    &todo_contract().pending_task_todo_guide_status
}

pub(crate) fn changed_event_name() -> &'static str {
    &todo_contract().todo_changed_event_name
}

pub(crate) fn changed_event_payload(task_id: &str) -> JsonValue {
    serde_json::to_value(TodoChangedEventPayload { task_id })
        .expect("TodoChangedEventPayload must serialize")
}

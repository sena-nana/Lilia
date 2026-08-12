use lilia_contracts::ChatConversationReference;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::contract;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskTodo {
    pub id: String,
    pub task_id: String,
    pub text: String,
    pub done: bool,
    pub order: i64,
    pub source: String,
    pub priority: String,
    pub guide_status: Option<String>,
    pub attachments: Vec<JsonValue>,
    pub conversation_references: Vec<ChatConversationReference>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTodoItem {
    #[serde(alias = "text", alias = "title", alias = "description")]
    pub content: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub completed: Option<bool>,
    #[serde(default)]
    pub done: Option<bool>,
    #[serde(default)]
    pub priority: Option<String>,
}

impl AgentTodoItem {
    pub(super) fn is_done(&self) -> bool {
        self.completed.unwrap_or(false)
            || self.done.unwrap_or(false)
            || self.status.eq_ignore_ascii_case("completed")
    }

    pub(super) fn normalized_priority(&self) -> String {
        normalize_priority(self.priority.as_deref())
    }
}

pub(super) fn normalize_priority(value: Option<&str>) -> String {
    let value = value.unwrap_or("").trim().to_ascii_lowercase();
    contract::priorities()
        .iter()
        .find(|priority| priority.as_str() == value)
        .map(ToString::to_string)
        .unwrap_or_else(|| contract::default_priority().to_string())
}

pub(super) fn normalize_guide_status(value: Option<&str>) -> Option<String> {
    let value = value.unwrap_or("").trim().to_ascii_lowercase();
    contract::guide_statuses()
        .iter()
        .find(|status| status.as_str() == value)
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todo_contract_matches_rust_normalization() {
        assert_eq!(
            contract::priorities()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["high", "normal", "low"]
        );
        assert_eq!(contract::default_priority(), "normal");
        assert_eq!(normalize_priority(Some(" high ")), "high");
        assert_eq!(normalize_priority(Some("LOW")), "low");
        assert_eq!(normalize_priority(Some("urgent")), "normal");
        assert_eq!(normalize_priority(None), "normal");

        assert_eq!(
            contract::guide_statuses()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["pending", "queued", "sent"]
        );
        assert_eq!(contract::pending_guide_status(), "pending");
        assert_eq!(contract::changed_event_name(), "todo-changed");
        assert_eq!(
            contract::changed_event_payload("task-1"),
            serde_json::json!({ "taskId": "task-1" })
        );
        assert_eq!(
            normalize_guide_status(Some(" queued ")).as_deref(),
            Some("queued")
        );
        assert_eq!(
            normalize_guide_status(Some("SENT")).as_deref(),
            Some("sent")
        );
        assert_eq!(normalize_guide_status(Some("done")), None);
        assert_eq!(normalize_guide_status(None), None);
    }
}

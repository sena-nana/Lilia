use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[cfg(test)]
pub(crate) use lilia_contracts::LiliaReviewTarget;
pub(crate) use lilia_contracts::{
    ChatAttachment, ChatContextSearchResult, ChatContextUsage, ChatConversationReference,
    LiliaAgentWorkflow as ChatWorkflow,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatMessage {
    pub(crate) id: String,
    pub(crate) task_id: String,
    pub(crate) role: String, // "user" | "assistant" | "system"
    pub(crate) content: String,
    pub(crate) attachments: Vec<ChatAttachment>,
    #[serde(default)]
    pub(crate) conversation_references: Vec<ChatConversationReference>,
    pub(crate) created_at: u64,
}

pub(crate) fn conversation_references_payload(
    conversation_references: &[ChatConversationReference],
) -> JsonValue {
    JsonValue::Array(
        conversation_references
            .iter()
            .map(|reference| {
                serde_json::to_value(reference).expect("ChatConversationReference must serialize")
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn conversation_reference_payload_uses_struct_serialization() {
        let reference = ChatConversationReference {
            task_id: "task-1".to_string(),
            title: "Title".to_string(),
            route: "/task/task-1".to_string(),
            project_id: Some("project-1".to_string()),
            project_name: None,
        };

        assert_eq!(
            serde_json::to_value(reference).unwrap(),
            json!({
                "taskId": "task-1",
                "title": "Title",
                "route": "/task/task-1",
                "projectId": "project-1",
                "projectName": null,
            })
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ChatSlashCommandSource {
    #[serde(rename = "native")]
    Native,
    #[serde(rename = "project")]
    Project,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatSlashCommandParameter {
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) required: bool,
    #[serde(default)]
    pub(crate) hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatSlashCommand {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) source: ChatSlashCommandSource,
    pub(crate) parameters: Vec<ChatSlashCommandParameter>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatSlashCommandSearchResult {
    pub(crate) command: ChatSlashCommand,
    pub(crate) matched_by: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClipboardImageInput {
    pub(crate) mime: Option<String>,
    pub(crate) bytes_base64: String,
    pub(crate) name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClipboardTextInput {
    pub(crate) text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatSendResult {
    pub(crate) message: ChatMessage,
    /// "started" | "queued"
    pub(crate) dispatch: String,
    pub(crate) queued_count: usize,
    pub(crate) turn_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeSettingsCommon {
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) permission: Option<String>,
    #[serde(default)]
    pub(crate) reasoning_effort: Option<String>,
    #[serde(default)]
    pub(crate) runtime_workspace_roots: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) model_selection: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeSettingsCodex {
    #[serde(default)]
    pub(crate) profile: Option<String>,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) reasoning_effort: Option<String>,
    #[serde(default)]
    pub(crate) permission_profile: Option<String>,
    #[serde(default)]
    pub(crate) runtime_workspace_roots: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) additional_context: Option<String>,
    #[serde(default)]
    pub(crate) persist_extended_history: Option<bool>,
    #[serde(default)]
    pub(crate) initial_turns_page: Option<JsonValue>,
    #[serde(default)]
    pub(crate) exclude_turns: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) environments: Option<Vec<JsonValue>>,
    #[serde(default)]
    pub(crate) experimental_raw_events: Option<bool>,
    #[serde(default)]
    pub(crate) responses_api_client_metadata: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeSettingsClaude {
    #[serde(default)]
    pub(crate) reasoning_effort: Option<String>,
    #[serde(default)]
    pub(crate) thinking: Option<JsonValue>,
    #[serde(default)]
    pub(crate) allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) disallowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) additional_directories: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) additional_context: Option<String>,
    #[serde(default)]
    pub(crate) max_turns: Option<u64>,
    #[serde(default)]
    pub(crate) max_budget_usd: Option<f64>,
    #[serde(default)]
    pub(crate) tools: Option<JsonValue>,
    #[serde(default)]
    pub(crate) permission_prompt_tool_name: Option<String>,
    #[serde(default)]
    pub(crate) settings: Option<JsonValue>,
    #[serde(default)]
    pub(crate) managed_settings: Option<JsonValue>,
    #[serde(default)]
    pub(crate) setting_sources: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) sandbox: Option<JsonValue>,
    #[serde(default)]
    pub(crate) output_format: Option<JsonValue>,
    #[serde(default)]
    pub(crate) include_hook_events: Option<bool>,
    #[serde(default)]
    pub(crate) forward_subagent_text: Option<bool>,
    #[serde(default)]
    pub(crate) agent_progress_summaries: Option<bool>,
    #[serde(default, rename = "continue")]
    pub(crate) continue_session: Option<bool>,
    #[serde(default)]
    pub(crate) resume_session_at: Option<String>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) abort_after_ms: Option<u64>,
    #[serde(default)]
    pub(crate) session_store: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub(crate) enum ChatRuntimeCommand {
    #[serde(rename = "session_fork")]
    SessionFork {
        #[serde(default)]
        exclude_turns: Option<bool>,
        #[serde(default)]
        source_turn_id: Option<String>,
        #[serde(default)]
        mode: Option<String>,
    },
    #[serde(rename = "session_management")]
    SessionManagement {
        action: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        tag: Option<String>,
        #[serde(default)]
        archived: Option<bool>,
        #[serde(default)]
        limit: Option<u64>,
        #[serde(default)]
        cursor: Option<String>,
        #[serde(default)]
        search_term: Option<String>,
        #[serde(default)]
        include_system_messages: Option<bool>,
    },
    #[serde(rename = "runtime_settings")]
    RuntimeSettings { action: String },
    #[serde(rename = "remote_environment")]
    RemoteEnvironment {
        action: String,
        #[serde(default)]
        environment_id: Option<String>,
        #[serde(default)]
        environment: Option<JsonValue>,
    },
    #[serde(rename = "sandbox_diagnostics")]
    SandboxDiagnostics {
        #[serde(default)]
        include_details: Option<bool>,
    },
    #[serde(rename = "process_session")]
    ProcessSession {
        action: String,
        #[serde(default)]
        process_id: Option<String>,
        #[serde(default)]
        command: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        stdin: Option<String>,
        #[serde(default)]
        rows: Option<u64>,
        #[serde(default)]
        cols: Option<u64>,
        #[serde(default)]
        env: Option<JsonValue>,
        #[serde(default)]
        tty: Option<bool>,
        #[serde(default)]
        permission_profile: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderRuntimeOptions {
    #[serde(default)]
    pub(crate) common: Option<RuntimeSettingsCommon>,
    #[serde(default)]
    pub(crate) provider: Option<ProviderRuntimeOptionsProvider>,
    #[serde(default)]
    pub(crate) experimental_provider_options: Option<Vec<ExperimentalProviderOptions>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderRuntimeOptionsProvider {
    #[serde(default)]
    pub(crate) codex: Option<RuntimeSettingsCodex>,
    #[serde(default)]
    pub(crate) claude: Option<RuntimeSettingsClaude>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExperimentalProviderOptions {
    pub(crate) provider: String,
    pub(crate) capability: String,
    pub(crate) payload: JsonValue,
    pub(crate) fallback: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatInterruptResult {
    pub(crate) rolled_back: bool,
    pub(crate) restored_content: String,
    pub(crate) restored_attachments: Vec<ChatAttachment>,
    #[serde(default)]
    pub(crate) restored_conversation_references: Vec<ChatConversationReference>,
    pub(crate) removed_event_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatRollbackResult {
    pub(crate) rolled_back: bool,
    pub(crate) restored_content: String,
    pub(crate) restored_attachments: Vec<ChatAttachment>,
    #[serde(default)]
    pub(crate) restored_conversation_references: Vec<ChatConversationReference>,
    pub(crate) removed_event_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatRuntimeSnapshot {
    pub(crate) task_id: String,
    pub(crate) phase: String,
    pub(crate) backend: Option<String>,
    pub(crate) turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) process_session_id: Option<String>,
    pub(crate) queued_count: usize,
    pub(crate) pending_rollback: bool,
    pub(crate) pending_reset_cleanup: bool,
    pub(crate) context_usage: Option<ChatContextUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rollback: Option<ChatRollbackResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatComposerState {
    pub(crate) task_id: String,
    /// "claude" | "codex"
    pub(crate) backend: String,
    pub(crate) model: String,
    #[serde(default = "default_model_selection_mode")]
    pub(crate) model_selection_mode: String,
    #[serde(default)]
    pub(crate) reasoning_effort: Option<String>,
    #[serde(default)]
    pub(crate) plan_mode: bool,
    #[serde(default)]
    pub(crate) goal_mode: bool,
    /// "full" | "ask" | "readonly"
    pub(crate) permission: String,
    #[serde(default)]
    pub(crate) codex_settings: CodexComposerSettings,
}

fn default_model_selection_mode() -> String {
    "auto".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexComposerSettings {
    #[serde(default)]
    pub(crate) profile: Option<String>,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) reasoning_effort: Option<String>,
    #[serde(default)]
    pub(crate) runtime_workspace_roots: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) responses_api_client_metadata: Option<JsonValue>,
    #[serde(default)]
    pub(crate) additional_context: Option<String>,
    #[serde(default)]
    pub(crate) persist_extended_history: Option<bool>,
    #[serde(default)]
    pub(crate) initial_turns_page: Option<JsonValue>,
    #[serde(default)]
    pub(crate) exclude_turns: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatModelOption {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) backend: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoneEvent {
    pub(crate) task_id: String,
    pub(crate) session_id: Option<String>,
    pub(crate) subtype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rollback: Option<ChatRollbackResult>,
}

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::agent_interaction_contract;
use crate::runner_protocol_contract::{self, RunnerControlMessageTypes, RunnerRuntimeEventTypes};

fn runner_runtime_event_types() -> &'static RunnerRuntimeEventTypes {
    runner_protocol_contract::runner_runtime_event_types()
}

fn runner_control_message_types() -> &'static RunnerControlMessageTypes {
    runner_protocol_contract::runner_control_message_types()
}

pub(crate) fn runner_interaction_response_control_type() -> &'static str {
    runner_control_message_types().interaction_response.as_str()
}

pub(crate) fn runner_settings_update_control_type() -> &'static str {
    runner_control_message_types().settings_update.as_str()
}

pub(crate) fn runner_interrupt_turn_control_type() -> &'static str {
    runner_control_message_types().interrupt_turn.as_str()
}

#[cfg(test)]
pub(crate) fn runner_quota_usage_result_control_type() -> &'static str {
    runner_control_message_types().quota_usage_result.as_str()
}

pub(crate) fn runner_lilia_iab_result_control_type() -> &'static str {
    runner_control_message_types().lilia_iab_result.as_str()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTurnContext {
    pub task_id: String,
    pub backend: String,
    pub turn_id: String,
    #[serde(default)]
    pub automation_run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentRuntimeEvent {
    ToolUse {
        name: String,
        #[serde(default)]
        input: JsonValue,
    },
    TodoList {
        #[serde(default)]
        items: Vec<JsonValue>,
    },
    Timeline {
        #[serde(default)]
        event: JsonValue,
    },
    InteractionRequest {
        id: String,
        kind: String,
        #[serde(default)]
        backend: Option<String>,
        #[serde(default)]
        payload: JsonValue,
    },
    QuotaUsageRequest {
        id: String,
        #[serde(default)]
        payload: JsonValue,
    },
    ContextUsage {
        used_tokens: u64,
        #[serde(default)]
        limit_tokens: Option<u64>,
        #[serde(default)]
        used_percent: Option<f64>,
        #[serde(default)]
        source: Option<String>,
        #[serde(default)]
        unavailable_reason: Option<String>,
    },
    Done {
        session_id: Option<String>,
        subtype: Option<String>,
    },
    PromptSuggestion {
        suggestion: String,
        uuid: Option<String>,
    },
    Error {
        message: String,
    },
}

impl AgentRuntimeEvent {
    pub fn from_runner_json(value: &JsonValue) -> Option<Self> {
        let ty = value.get("type").and_then(|v| v.as_str())?;
        let types = runner_runtime_event_types();
        match ty {
            ty if ty == types.tool_use.as_str() => {
                let name = value
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let input = value.get("input").cloned().unwrap_or(JsonValue::Null);
                Some(Self::ToolUse { name, input })
            }
            ty if ty == types.todo_list.as_str() => {
                let items = value
                    .get("items")
                    .or_else(|| value.get("todos"))
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                Some(Self::TodoList { items })
            }
            ty if ty == types.timeline.as_str() => value
                .get("event")
                .cloned()
                .map(|event| Self::Timeline { event }),
            ty if ty == types.interaction_request.as_str() => {
                let id = value.get("id").and_then(|v| v.as_str())?.to_string();
                let kind = value
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| agent_interaction_contract::ask_user_interaction_kind())
                    .to_string();
                let backend = value
                    .get("backend")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                let payload = value.get("payload").cloned().unwrap_or(JsonValue::Null);
                Some(Self::InteractionRequest {
                    id,
                    kind,
                    backend,
                    payload,
                })
            }
            ty if ty == types.quota_usage_request.as_str() => {
                let id = value.get("id").and_then(|v| v.as_str())?.to_string();
                let payload = value.get("payload").cloned().unwrap_or(JsonValue::Null);
                Some(Self::QuotaUsageRequest { id, payload })
            }
            ty if ty == types.context_usage.as_str() => {
                let used_tokens = json_u64_field(value, &["usedTokens", "used_tokens"])?;
                let limit_tokens = json_u64_field(value, &["limitTokens", "limit_tokens"]);
                let used_percent = json_f64_field(value, &["usedPercent", "used_percent"]);
                let source = value
                    .get("source")
                    .and_then(|v| v.as_str())
                    .map(|source| source.trim().to_string())
                    .filter(|source| !source.is_empty());
                let unavailable_reason = value
                    .get("unavailableReason")
                    .or_else(|| value.get("unavailable_reason"))
                    .and_then(|v| v.as_str())
                    .map(|reason| reason.trim().to_string())
                    .filter(|reason| !reason.is_empty());
                Some(Self::ContextUsage {
                    used_tokens,
                    limit_tokens,
                    used_percent,
                    source,
                    unavailable_reason,
                })
            }
            ty if ty == types.done.as_str() => {
                let session_id = value
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .map(|sid| sid.to_string());
                let subtype = value
                    .get("subtype")
                    .and_then(|v| v.as_str())
                    .map(|subtype| subtype.to_string());
                Some(Self::Done {
                    session_id,
                    subtype,
                })
            }
            ty if ty == types.prompt_suggestion.as_str() => {
                let suggestion = value
                    .get("suggestion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if suggestion.trim().is_empty() {
                    return None;
                }
                let uuid = value
                    .get("uuid")
                    .and_then(|v| v.as_str())
                    .map(|uuid| uuid.to_string());
                Some(Self::PromptSuggestion { suggestion, uuid })
            }
            ty if ty == types.error.as_str() => {
                let message = value
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知错误")
                    .to_string();
                Some(Self::Error { message })
            }
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn event_type(&self) -> &'static str {
        let types = runner_runtime_event_types();
        match self {
            Self::ToolUse { .. } => &types.tool_use,
            Self::TodoList { .. } => &types.todo_list,
            Self::Timeline { .. } => &types.timeline,
            Self::InteractionRequest { .. } => &types.interaction_request,
            Self::QuotaUsageRequest { .. } => &types.quota_usage_request,
            Self::ContextUsage { .. } => &types.context_usage,
            Self::Done { .. } => &types.done,
            Self::PromptSuggestion { .. } => &types.prompt_suggestion,
            Self::Error { .. } => &types.error,
        }
    }
}

fn json_u64_field(value: &JsonValue, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_i64().and_then(|n| u64::try_from(n).ok()))
        })
    })
}

fn json_f64_field(value: &JsonValue, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|v| v.as_f64()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn runner_json_is_normalized_to_runtime_events() {
        let types = runner_runtime_event_types();
        assert_eq!(
            AgentRuntimeEvent::from_runner_json(
                &json!({ "type": types.tool_use, "name": "Read", "input": { "file": "a.md" } })
            ),
            Some(AgentRuntimeEvent::ToolUse {
                name: "Read".to_string(),
                input: json!({ "file": "a.md" }),
            })
        );
        assert_eq!(
            AgentRuntimeEvent::from_runner_json(
                &json!({ "type": types.timeline, "event": { "kind": "tool" } })
            ),
            Some(AgentRuntimeEvent::Timeline {
                event: json!({ "kind": "tool" }),
            })
        );
        assert_eq!(
            AgentRuntimeEvent::from_runner_json(&json!({
                "type": types.todo_list,
                "items": [
                    { "text": "Mirror provider todo", "completed": true },
                    { "content": "Keep Claude compatibility", "status": "pending" }
                ]
            })),
            Some(AgentRuntimeEvent::TodoList {
                items: vec![
                    json!({ "text": "Mirror provider todo", "completed": true }),
                    json!({ "content": "Keep Claude compatibility", "status": "pending" })
                ],
            })
        );
        assert_eq!(
            AgentRuntimeEvent::from_runner_json(&json!({
                "type": types.interaction_request,
                "id": "ask-1",
                "kind": agent_interaction_contract::ask_user_interaction_kind(),
                "backend": "codex",
                "payload": {
                    "title": "Codex 想确认一下",
                    "questions": []
                }
            })),
            Some(AgentRuntimeEvent::InteractionRequest {
                id: "ask-1".to_string(),
                kind: agent_interaction_contract::ask_user_interaction_kind().to_string(),
                backend: Some("codex".to_string()),
                payload: json!({
                    "title": "Codex 想确认一下",
                    "questions": []
                }),
            })
        );
        assert_eq!(
            AgentRuntimeEvent::from_runner_json(&json!({
                "type": types.quota_usage_request,
                "id": "quota-1",
                "payload": { "days": 7, "scope": "tools" }
            })),
            Some(AgentRuntimeEvent::QuotaUsageRequest {
                id: "quota-1".to_string(),
                payload: json!({ "days": 7, "scope": "tools" }),
            })
        );
        assert_eq!(
            AgentRuntimeEvent::from_runner_json(&json!({
                "type": types.context_usage,
                "usedTokens": 4096,
                "limitTokens": 8192,
                "usedPercent": 50.0,
                "source": "runtime"
            })),
            Some(AgentRuntimeEvent::ContextUsage {
                used_tokens: 4096,
                limit_tokens: Some(8192),
                used_percent: Some(50.0),
                source: Some("runtime".to_string()),
                unavailable_reason: None,
            })
        );
        assert_eq!(
            AgentRuntimeEvent::from_runner_json(
                &json!({ "type": types.done, "sessionId": "s1", "subtype": "success" })
            ),
            Some(AgentRuntimeEvent::Done {
                session_id: Some("s1".to_string()),
                subtype: Some("success".to_string()),
            })
        );
        assert_eq!(
            AgentRuntimeEvent::from_runner_json(&json!({
                "type": types.prompt_suggestion,
                "suggestion": "请继续检查 Claude 原生建议展示。",
                "uuid": "suggestion-1"
            })),
            Some(AgentRuntimeEvent::PromptSuggestion {
                suggestion: "请继续检查 Claude 原生建议展示。".to_string(),
                uuid: Some("suggestion-1".to_string()),
            })
        );
        assert!(AgentRuntimeEvent::from_runner_json(&json!({
            "type": types.prompt_suggestion,
            "suggestion": " "
        }))
        .is_none());
        assert_eq!(
            AgentRuntimeEvent::from_runner_json(
                &json!({ "type": types.error, "message": "failed" })
            ),
            Some(AgentRuntimeEvent::Error {
                message: "failed".to_string(),
            })
        );
        // 未知/历史 type 直接降级为 None，runner 端如果发了 `chunk`/`assistant_done`
        // 这类已淘汰的帧也不会让主循环 panic。
        assert!(
            AgentRuntimeEvent::from_runner_json(&json!({ "type": "chunk", "text": "hi" }))
                .is_none()
        );
    }

    #[test]
    fn runner_control_message_types_are_loaded_from_protocol_contract() {
        let types = runner_control_message_types();

        assert_eq!(
            runner_interaction_response_control_type(),
            types.interaction_response.as_str()
        );
        assert_eq!(
            runner_settings_update_control_type(),
            types.settings_update.as_str()
        );
        assert_eq!(
            runner_interrupt_turn_control_type(),
            types.interrupt_turn.as_str()
        );
        assert_eq!(
            runner_quota_usage_result_control_type(),
            types.quota_usage_result.as_str()
        );
        assert_eq!(
            runner_lilia_iab_result_control_type(),
            types.lilia_iab_result.as_str()
        );
    }
}

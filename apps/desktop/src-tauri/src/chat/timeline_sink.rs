use serde_json::Value as JsonValue;
use tauri::{AppHandle, Emitter, Manager, Runtime};

#[cfg(test)]
use crate::agent_events::{AgentRuntimeEvent, AgentTurnContext};
use crate::agent_timeline;
use crate::agent_timeline::AgentTimelineEventInput;
use crate::chat::contract;
use crate::chat::types::{conversation_references_payload, ChatMessage};
use crate::store::LiliaStore;
use crate::util::now_millis;

#[cfg(test)]
pub(crate) fn timeline_input_from_runtime_event(
    ctx: &AgentTurnContext,
    event: &AgentRuntimeEvent,
) -> Option<AgentTimelineEventInput> {
    let AgentRuntimeEvent::Timeline { event } = event else {
        return None;
    };
    let Some(obj) = event.as_object() else {
        return None;
    };

    let kind = obj
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("tool")
        .to_string();
    let status = obj
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("info")
        .to_string();
    let title = obj
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(kind.as_str())
        .to_string();
    let summary = obj
        .get("summary")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let mut payload = obj.get("payload").cloned().unwrap_or(JsonValue::Null);
    if let Some(automation_run_id) = &ctx.automation_run_id {
        let mut payload_obj = payload.as_object().cloned().unwrap_or_default();
        payload_obj.insert(
            "automationRunId".to_string(),
            JsonValue::String(automation_run_id.clone()),
        );
        payload = JsonValue::Object(payload_obj);
    }
    let source_id = obj.get("sourceId").and_then(|v| v.as_str());
    let turn_id = obj
        .get("turnIdOverride")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| ctx.turn_id.clone());
    let id = source_id.map(|sid| format!("{}:{}:{sid}", ctx.task_id, turn_id));
    let created_at = obj.get("createdAt").and_then(|v| v.as_i64());
    let updated_at = obj.get("updatedAt").and_then(|v| v.as_i64());

    Some(AgentTimelineEventInput {
        id,
        task_id: ctx.task_id.clone(),
        turn_id: Some(turn_id),
        backend: ctx.backend.clone(),
        kind,
        status,
        title,
        summary,
        payload,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
pub(crate) fn assistant_error_text(input: &AgentTimelineEventInput) -> Option<String> {
    if input.kind != "message" || !matches!(input.status.as_str(), "error" | "failed") {
        return None;
    }
    let obj = input.payload.as_object()?;
    if obj.get("role").and_then(|v| v.as_str()) != Some("assistant") {
        return None;
    }
    let text = normalize_timeline_text(obj.get("content")?.as_str()?);
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
pub(crate) fn normalize_timeline_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 直接落库 + emit 一条 timeline 输入，不做节流。
/// 任何调用方（throttle、用户消息、错误事件）都共用同一条物理路径，
/// 保证「emit 的 payload = DB 写入的快照」始终成立。
pub(crate) fn persist_and_emit_input<R: Runtime>(
    app_handle: &AppHandle<R>,
    input: AgentTimelineEventInput,
) {
    let store = app_handle.state::<LiliaStore>();
    match store.conn().and_then(|conn| {
        let saved = agent_timeline::insert(&conn, input)?;
        if let Err(err) = crate::quota_usage::record_from_timeline_event(&conn, &saved) {
            eprintln!("[quota-usage] persist failed: {err}");
        }
        Ok(saved)
    }) {
        Ok(saved) => {
            let _ = app_handle.emit(contract::agent_timeline_event_name(), &saved);
            crate::automation::emit_timeline_signal(app_handle, &saved);
        }
        Err(err) => {
            eprintln!("[agent-timeline] persist failed: {err}");
        }
    }
}

pub(crate) fn persist_and_emit_message_timeline_event<R: Runtime>(
    app_handle: &AppHandle<R>,
    message: &ChatMessage,
    backend: &str,
    turn_id: &str,
    queued: bool,
    automation_run_id: Option<&str>,
) {
    let mut payload = serde_json::json!({
        "role": message.role,
        "content": message.content,
        "attachments": message.attachments,
        "conversationReferences": conversation_references_payload(&message.conversation_references),
        "queued": queued,
    });
    if let Some(automation_run_id) = automation_run_id {
        payload["automationRunId"] = JsonValue::String(automation_run_id.to_string());
    }
    let input = AgentTimelineEventInput {
        id: Some(message.id.clone()),
        task_id: message.task_id.clone(),
        // user message 与它触发的 agent turn 共享 turn_id，所以两者会被分到同一个
        // turn_seq，user 消息天然落在 turn 内部第一位（intra_turn_order=0）。
        turn_id: Some(turn_id.to_string()),
        backend: backend.to_string(),
        kind: "message".to_string(),
        status: if queued { "pending" } else { "success" }.to_string(),
        title: "用户输入".to_string(),
        summary: Some(message.content.clone()),
        payload,
        created_at: Some(message.created_at as i64),
        updated_at: Some(now_millis() as i64),
    };

    persist_and_emit_input(app_handle, input);
}

pub(crate) fn persist_and_emit_model_selection_timeline_event<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: &str,
    backend: &str,
    turn_id: &str,
    model_selection: Option<&JsonValue>,
) {
    let Some(model_selection) = model_selection else {
        return;
    };
    if !model_selection.is_object() {
        return;
    }
    let now = now_millis();
    let summary = model_selection
        .get("summary")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("已自动选择本轮模型")
        .to_string();
    let input = AgentTimelineEventInput {
        id: Some(format!("{task_id}:{turn_id}:model-selection:{turn_id}")),
        task_id: task_id.to_string(),
        turn_id: Some(turn_id.to_string()),
        backend: backend.to_string(),
        kind: "diagnostic".to_string(),
        status: "info".to_string(),
        title: "模型选择".to_string(),
        summary: Some(summary),
        payload: serde_json::json!({
            "backend": backend,
            "sourceId": format!("model-selection:{turn_id}"),
            "subkind": "model_selection",
            "selection": model_selection,
        }),
        created_at: Some(now as i64),
        updated_at: Some(now as i64),
    };

    persist_and_emit_input(app_handle, input);
}

pub(crate) fn persist_and_emit_error_timeline_event<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: &str,
    backend: &str,
    turn_id: Option<&str>,
    message: String,
) {
    let now = now_millis();
    let input = AgentTimelineEventInput {
        id: None,
        task_id: task_id.to_string(),
        turn_id: turn_id.map(|id| id.to_string()),
        backend: backend.to_string(),
        kind: "error".to_string(),
        status: "error".to_string(),
        title: "错误".to_string(),
        summary: Some(message.clone()),
        payload: serde_json::json!({ "message": message }),
        created_at: Some(now as i64),
        updated_at: Some(now as i64),
    };

    persist_and_emit_input(app_handle, input);
}

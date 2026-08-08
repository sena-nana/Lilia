//! Desktop product side effects on the Mutsuki projection stream
//! (todo checklist + context-usage ring).

use mutsuki_agent_contracts::{AgentEvent, AgentEventEnvelope, TodoItem, TodoItemStatus};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::chat::contract::context_usage_event_name;
use crate::chat::state::{set_context_usage, ChatStore};
use crate::chat::types::ChatContextUsage;
use crate::native_agent::BACKEND_NATIVE_AGENTKIT;
use crate::store::LiliaStore;
use crate::todos;
use crate::util::now_millis;

pub(crate) fn apply_projection_side_effects<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
    events: &[AgentEventEnvelope],
) {
    if events.is_empty() {
        return;
    }

    let mut latest_todos: Option<&[TodoItem]> = None;
    let mut context_usage: Option<ChatContextUsage> = None;
    let mut usage_fallback: Option<ChatContextUsage> = None;

    for envelope in events {
        match &envelope.event {
            AgentEvent::TodoUpdated { todo, .. } => {
                latest_todos = Some(todo.items.as_slice());
            }
            AgentEvent::ContextUsageUpdated { usage, .. } => {
                let used = usage.input_tokens.saturating_add(usage.reserved_tokens);
                let limit = (usage.limit_tokens > 0).then_some(usage.limit_tokens);
                context_usage = Some(ChatContextUsage {
                    task_id: task_id.to_string(),
                    backend: BACKEND_NATIVE_AGENTKIT.to_string(),
                    used_tokens: used,
                    limit_tokens: limit,
                    used_percent: limit
                        .map(|l| ((used as f64 / l as f64) * 100.0).clamp(0.0, 100.0)),
                    source: "native-agentkit".into(),
                    updated_at: now_millis() as u64,
                    unavailable_reason: None,
                });
            }
            AgentEvent::Usage { usage, .. } if context_usage.is_none() => {
                let used = if usage.total_tokens > 0 {
                    usage.total_tokens
                } else {
                    usage.input_tokens.saturating_add(usage.output_tokens)
                };
                usage_fallback = Some(ChatContextUsage {
                    task_id: task_id.to_string(),
                    backend: BACKEND_NATIVE_AGENTKIT.to_string(),
                    used_tokens: used,
                    limit_tokens: None,
                    used_percent: None,
                    source: "native-agentkit-usage".into(),
                    updated_at: now_millis() as u64,
                    unavailable_reason: None,
                });
            }
            _ => {}
        }
    }

    if let Some(items) = latest_todos {
        if let Err(err) = apply_todo_items(app, task_id, items) {
            eprintln!("[native-projection] todo mirror failed for {task_id}: {err}");
        }
    }

    if let Some(usage) = context_usage.or(usage_fallback) {
        if let Some(store) = app.try_state::<ChatStore>() {
            set_context_usage(&store, usage.clone());
        }
        let _ = app.emit(context_usage_event_name(), usage);
    }
}

/// Rebuild path: sync latest product todo projection into `task_todos`.
pub(crate) fn mirror_product_todos_for_task<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
) -> Result<(), String> {
    let runtime = crate::native_agent::native_runtime()?;
    let task = lilia_contracts::TaskId::new(task_id.to_string()).map_err(|e| e.to_string())?;
    let todos_proj = runtime.product_todos_for_task(&task);
    let Some(latest) = todos_proj.iter().max_by_key(|t| t.sequence) else {
        return Ok(());
    };
    let items: Vec<TodoItem> = serde_json::from_value(latest.items.clone()).unwrap_or_default();
    apply_todo_items(app, task_id, &items)
}

fn apply_todo_items<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
    items: &[TodoItem],
) -> Result<(), String> {
    if items.is_empty() {
        return Ok(());
    }
    let values: Vec<_> = items
        .iter()
        .filter(|item| !item.title.trim().is_empty())
        .map(|item| {
            let done = matches!(
                item.status,
                TodoItemStatus::Completed | TodoItemStatus::Cancelled
            );
            json!({
                "content": item.title.trim(),
                "status": if done { "completed" } else { "pending" },
                "priority": match item.priority {
                    p if p >= 2 => "high",
                    p if p < 0 => "low",
                    _ => "normal",
                },
            })
        })
        .collect();
    if values.is_empty() {
        return Ok(());
    }

    let store = app
        .try_state::<LiliaStore>()
        .ok_or_else(|| "LiliaStore is not available".to_string())?;
    let conn = store.conn()?;
    let parsed = todos::parse_agent_todo_items(&values);
    todos::apply_agent_event_impl(&conn, task_id, &parsed)?;
    let _ = app.emit(
        todos::contract::changed_event_name(),
        todos::contract::changed_event_payload(task_id),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_completed_high_priority_todo() {
        let values = [json!({
            "content": "Ship",
            "status": "completed",
            "priority": "high",
        })];
        let parsed = todos::parse_agent_todo_items(&values);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].content, "Ship");
        assert_eq!(parsed[0].status, "completed");
    }
}

use lilia_contracts::{ChatConversationReference, TaskId};
use lilia_desktop_application::{
    DesktopApplication, DesktopTaskTodo, DesktopTodoCreate, DesktopTodoGuideStatus,
    DesktopTodoPriority, DesktopTodoSource, DesktopTodoUpdate,
};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Runtime, State};

use crate::store::LiliaStore;

use super::agent_sync::apply_agent_event_impl;
use super::repository::emit_todo_changed;
use super::types::{AgentTodoItem, TaskTodo};

fn parse_task_id(value: String) -> Result<TaskId, String> {
    TaskId::new(value).map_err(|error| error.to_string())
}

fn desktop_priority(value: Option<&str>) -> DesktopTodoPriority {
    match value
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "high" => DesktopTodoPriority::High,
        "low" => DesktopTodoPriority::Low,
        _ => DesktopTodoPriority::Normal,
    }
}

fn desktop_guide_status(value: &str) -> Result<DesktopTodoGuideStatus, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "pending" => Ok(DesktopTodoGuideStatus::Pending),
        "queued" => Ok(DesktopTodoGuideStatus::Queued),
        "sent" => Ok(DesktopTodoGuideStatus::Sent),
        _ => Err(format!("todo_update(guideStatus): 无效状态 {value}")),
    }
}

fn task_todo(todo: DesktopTaskTodo) -> TaskTodo {
    TaskTodo {
        id: todo.id,
        task_id: todo.task_id.into_inner(),
        text: todo.text,
        done: todo.done,
        order: todo.order,
        source: match todo.source {
            DesktopTodoSource::Lilia => "lilia",
            DesktopTodoSource::Agent => "agent",
        }
        .to_owned(),
        priority: match todo.priority {
            DesktopTodoPriority::High => "high",
            DesktopTodoPriority::Normal => "normal",
            DesktopTodoPriority::Low => "low",
        }
        .to_owned(),
        guide_status: todo
            .guide_status
            .map(|status| match status {
                DesktopTodoGuideStatus::Pending => "pending",
                DesktopTodoGuideStatus::Queued => "queued",
                DesktopTodoGuideStatus::Sent => "sent",
            })
            .map(ToOwned::to_owned),
        attachments: todo.attachments,
        conversation_references: todo.conversation_references,
        created_at: todo.created_at,
        updated_at: todo.updated_at,
    }
}

#[tauri::command]
pub fn todo_list(
    task_id: String,
    desktop: State<'_, DesktopApplication>,
) -> Result<Vec<TaskTodo>, String> {
    desktop
        .list_task_todos(&parse_task_id(task_id)?)
        .map(|todos| todos.into_iter().map(task_todo).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn todo_create(
    task_id: String,
    text: String,
    priority: Option<String>,
    attachments: Option<Vec<JsonValue>>,
    conversation_references: Option<Vec<ChatConversationReference>>,
    desktop: State<'_, DesktopApplication>,
) -> Result<TaskTodo, String> {
    let input = DesktopTodoCreate {
        task_id: parse_task_id(task_id)?,
        text,
        priority: desktop_priority(priority.as_deref()),
        attachments: attachments.unwrap_or_default(),
        conversation_references: conversation_references.unwrap_or_default(),
        workflow: None,
    };
    desktop
        .create_task_todo(input)
        .map(task_todo)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn todo_submit_guide(
    task_id: String,
    expected_composer_revision: u64,
    text: String,
    priority: Option<String>,
    attachments: Option<Vec<JsonValue>>,
    conversation_references: Option<Vec<ChatConversationReference>>,
    desktop: State<'_, DesktopApplication>,
) -> Result<TaskTodo, String> {
    let input = DesktopTodoCreate {
        task_id: parse_task_id(task_id)?,
        text,
        priority: desktop_priority(priority.as_deref()),
        attachments: attachments.unwrap_or_default(),
        conversation_references: conversation_references.unwrap_or_default(),
        workflow: None,
    };
    desktop
        .submit_composer_guide(expected_composer_revision, input)
        .map(task_todo)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn todo_update(
    id: String,
    text: Option<String>,
    done: Option<bool>,
    order: Option<i64>,
    priority: Option<String>,
    guide_status: Option<String>,
    desktop: State<'_, DesktopApplication>,
) -> Result<(), String> {
    if text.is_none()
        && done.is_none()
        && order.is_none()
        && priority.is_none()
        && guide_status.is_none()
    {
        return Ok(());
    }
    let update = DesktopTodoUpdate {
        text,
        done,
        order,
        priority: priority
            .as_deref()
            .map(|value| desktop_priority(Some(value))),
        guide_status: guide_status
            .as_deref()
            .map(desktop_guide_status)
            .transpose()?,
    };
    desktop
        .update_task_todo(&id, update)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn todo_delete(id: String, desktop: State<'_, DesktopApplication>) -> Result<(), String> {
    desktop
        .delete_task_todo(&id)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn todo_apply_agent_event<R: Runtime>(
    task_id: String,
    todos: Vec<AgentTodoItem>,
    app: AppHandle<R>,
    store: State<'_, LiliaStore>,
) -> Result<Vec<TaskTodo>, String> {
    let conn = store.conn()?;
    let updated = apply_agent_event_impl(&conn, &task_id, &todos)?;
    emit_todo_changed(&app, &task_id)?;
    Ok(updated)
}

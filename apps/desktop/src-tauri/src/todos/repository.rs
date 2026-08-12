use lilia_contracts::ChatConversationReference;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Emitter, Runtime};

use crate::store::LiliaStore;
use crate::util::now_millis;

use super::contract;
use super::types::{normalize_guide_status, TaskTodo};

fn parse_attachments_json(value: String) -> Vec<JsonValue> {
    serde_json::from_str::<Vec<JsonValue>>(&value).unwrap_or_default()
}

fn parse_conversation_references_json(value: String) -> Vec<ChatConversationReference> {
    serde_json::from_str::<Vec<ChatConversationReference>>(&value).unwrap_or_default()
}

fn row_to_todo(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskTodo> {
    Ok(TaskTodo {
        id: row.get(0)?,
        task_id: row.get(1)?,
        text: row.get(2)?,
        done: row.get::<_, i64>(3)? != 0,
        order: row.get(4)?,
        source: row.get(5)?,
        priority: row.get(6)?,
        guide_status: row.get(7)?,
        attachments: parse_attachments_json(row.get(8)?),
        conversation_references: parse_conversation_references_json(row.get(9)?),
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

pub(super) fn select_by_task(conn: &Connection, task_id: &str) -> Result<Vec<TaskTodo>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT id, task_id, text, done, "order", source, priority, guide_status, attachments_json,
                      conversation_references_json, created_at, updated_at
               FROM task_todos WHERE task_id = ?1 ORDER BY "order" ASC, created_at ASC"#,
        )
        .map_err(|e| format!("todo_list: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map(params![task_id], row_to_todo)
        .map_err(|e| format!("todo_list: query 失败：{e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("todo_list: 行解析失败：{e}"))?);
    }
    Ok(out)
}

pub(super) fn emit_todo_changed<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
) -> Result<(), String> {
    app.emit(
        contract::changed_event_name(),
        contract::changed_event_payload(task_id),
    )
    .map_err(|err| format!("{} emit failed: {err}", contract::changed_event_name()))?;
    crate::automation::emit_todo_signal(app, task_id.to_string(), None);
    Ok(())
}

pub(crate) fn set_lilia_guide_status<R: Runtime>(
    app: &AppHandle<R>,
    store: &LiliaStore,
    id: &str,
    status: &str,
) -> Result<(), String> {
    let conn = store.conn()?;
    let normalized = normalize_guide_status(Some(status))
        .ok_or_else(|| format!("todo guide status: 无效状态 {status}"))?;
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT task_id, guide_status FROM task_todos WHERE id = ?1 AND source = 'lilia'",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("todo guide status: 查询失败：{e}"))?;
    let Some((task_id, current)) = row else {
        return Ok(());
    };
    if current.as_deref() == Some(normalized.as_str()) {
        return Ok(());
    }
    conn.execute(
        "UPDATE task_todos SET guide_status = ?1, updated_at = ?2 WHERE id = ?3 AND source = 'lilia'",
        params![normalized, now_millis(), id],
    )
    .map_err(|e| format!("todo guide status: 更新失败：{e}"))?;
    emit_todo_changed(app, &task_id)
}

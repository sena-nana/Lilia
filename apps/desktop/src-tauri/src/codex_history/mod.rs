pub(crate) mod bulk;
pub(crate) mod preview;
mod types;
pub(crate) mod utility;

pub use types::{
    CodexThreadAttachInput, CodexThreadAttachResult, CodexThreadPreview, CodexThreadPreviewInput,
    CodexThreadPreviewMessage, CodexThreadRuntimeState, CodexThreadSearchInput,
    CodexThreadSearchResult, CodexThreadSummary,
};

use rusqlite::{params, OptionalExtension};
use tauri::{AppHandle, Manager, Runtime};
use uuid::Uuid;

use crate::agent_timeline::AgentTimelineEventInput;
use crate::chat::state::{
    count_pending_turns, default_composer, load_persisted_resume_session_id, load_runtime_state,
    persisted_runtime_state_is_active, remember_agent_session, ChatStore,
};
use crate::chat::timeline_sink::persist_and_emit_input;
use crate::projects_tasks::events::emit_tasks_changed;
use crate::projects_tasks::TaskRow;
use crate::store::LiliaStore;
use crate::util::now_millis;
use crate::BACKEND_CODEX;

use self::bulk::persist_history_events_batch;
use self::preview::preview_events_from_inputs;
use self::utility::run_codex_history_utility;

pub(crate) fn codex_thread_search_blocking(
    app: AppHandle,
    input: CodexThreadSearchInput,
) -> Result<CodexThreadSearchResult, String> {
    let result = run_codex_history_utility(
        &app,
        serde_json::json!({
            "action": "search",
            "input": input,
        }),
    )?;
    Ok(CodexThreadSearchResult {
        threads: result.threads,
        next_cursor: result.next_cursor,
    })
}

pub(crate) fn query_codex_thread_runtime_states(
    conn: &rusqlite::Connection,
    chat_store: &ChatStore,
) -> Result<Vec<CodexThreadRuntimeState>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT s.session_id, t.id, t.title, t.project_id
               FROM task_agent_sessions s
               JOIN tasks t ON t.id = s.task_id
               JOIN (
                 SELECT task_id, MAX(updated_at) AS max_updated_at
                 FROM task_agent_sessions
                 WHERE backend = ?1
                 GROUP BY task_id
               ) latest ON latest.task_id = s.task_id AND latest.max_updated_at = s.updated_at
               WHERE s.backend = ?1 AND t.archived = 0
               ORDER BY s.updated_at DESC"#,
        )
        .map_err(|e| format!("Codex thread runtime states: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map(params![BACKEND_CODEX], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|e| format!("Codex thread runtime states: query 失败：{e}"))?;

    let running = chat_store.running_tasks.lock().unwrap();
    let running_turns = chat_store.running_turns.lock().unwrap();
    let pending_turns = chat_store.pending_turns.lock().unwrap();
    let mut out = Vec::new();
    for row in rows {
        let (thread_id, task_id, task_title, project_id) =
            row.map_err(|e| format!("Codex thread runtime states: row 失败：{e}"))?;
        let in_memory_queued_count = pending_turns
            .get(&task_id)
            .map(|queue| queue.len())
            .unwrap_or(0);
        let persisted_queued_count = count_pending_turns(conn, &task_id).unwrap_or(0);
        let queued_count = in_memory_queued_count + persisted_queued_count;
        let persisted_running = load_runtime_state(conn, chat_store, &task_id)
            .ok()
            .flatten()
            .is_some_and(|state| {
                state.turn.backend == BACKEND_CODEX
                    && persisted_runtime_state_is_active(&state, chat_store)
            });
        let is_running = running.get(&task_id).copied().unwrap_or(false)
            || running_turns
                .get(&task_id)
                .is_some_and(|turn| turn.backend == BACKEND_CODEX)
            || persisted_running;
        out.push(CodexThreadRuntimeState {
            thread_id,
            task_id,
            task_title,
            project_id,
            running: is_running,
            queued: queued_count > 0,
            pending: is_running || queued_count > 0,
            queued_count,
        });
    }
    Ok(out)
}

pub(crate) fn clean_background_terminals_payload(
    thread_id: &str,
) -> Result<serde_json::Value, String> {
    let thread_id = thread_id.trim();
    if thread_id.is_empty() {
        return Err("Codex threadId 不能为空".to_string());
    }
    Ok(serde_json::json!({
        "action": "cleanBackgroundTerminals",
        "threadId": thread_id,
    }))
}

pub(crate) fn clean_codex_thread_background_terminals_blocking(
    app: &AppHandle,
    thread_id: &str,
) -> Result<(), String> {
    let payload = clean_background_terminals_payload(thread_id)?;
    run_codex_history_utility(app, payload).map(|_| ())
}

pub(crate) fn archive_thread_payload(thread_id: &str) -> Result<serde_json::Value, String> {
    let thread_id = thread_id.trim();
    if thread_id.is_empty() {
        return Err("Codex threadId 不能为空".to_string());
    }
    Ok(serde_json::json!({
        "action": "archiveThread",
        "threadId": thread_id,
    }))
}

fn rename_thread_payload(thread_id: &str, name: &str) -> Result<serde_json::Value, String> {
    let thread_id = thread_id.trim();
    let name = name.trim();
    if thread_id.is_empty() {
        return Err("Codex threadId 不能为空".to_string());
    }
    if name.is_empty() {
        return Err("Codex thread 名称不能为空".to_string());
    }
    Ok(serde_json::json!({
        "action": "renameThread",
        "threadId": thread_id,
        "name": name,
    }))
}

pub(crate) fn sync_thread_title_blocking<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
    title: &str,
) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Ok(());
    }
    let Some(store) = app.try_state::<LiliaStore>() else {
        return Ok(());
    };
    let conn = store.conn()?;
    let Some(thread_id) = load_persisted_resume_session_id(&conn, task_id, BACKEND_CODEX) else {
        return Ok(());
    };
    let payload = rename_thread_payload(&thread_id, title)?;
    let result = run_codex_history_utility(app, payload).map(|_| ());
    persist_and_emit_input(
        app,
        thread_rename_diagnostic_input(task_id, &thread_id, title, result.as_ref().err().cloned()),
    );
    result
}

pub(crate) fn sync_thread_archive_blocking<R: Runtime>(
    app: &AppHandle<R>,
    thread_id: &str,
) -> Result<(), String> {
    let payload = archive_thread_payload(thread_id)?;
    run_codex_history_utility(app, payload).map(|_| ())
}

pub(crate) fn spawn_codex_thread_archive_sync<R: Runtime>(
    app: AppHandle<R>,
    thread_ids: Vec<String>,
) {
    let thread_ids = normalize_thread_ids(thread_ids);
    if thread_ids.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        run_codex_thread_archive_sync(thread_ids, |thread_id| {
            sync_thread_archive_blocking(&app, thread_id)
        });
    });
}

fn normalize_thread_ids(thread_ids: Vec<String>) -> Vec<String> {
    thread_ids
        .into_iter()
        .map(|thread_id| thread_id.trim().to_string())
        .filter(|thread_id| !thread_id.is_empty())
        .collect()
}

fn run_codex_thread_archive_sync<F>(thread_ids: Vec<String>, run: F)
where
    F: FnMut(&str) -> Result<(), String>,
{
    let mut run = run;
    for thread_id in thread_ids {
        if let Err(err) = run(&thread_id) {
            eprintln!(
                "[codex-history] best-effort thread archive sync failed for {}: {}",
                thread_id, err
            );
        }
    }
}

pub(crate) fn codex_thread_preview_blocking(
    app: AppHandle,
    input: CodexThreadPreviewInput,
) -> Result<CodexThreadPreview, String> {
    let detail = input.detail.as_deref().unwrap_or("lite");
    let result = run_codex_history_utility(
        &app,
        serde_json::json!({
            "action": "preview",
            "threadId": input.thread_id,
            "detail": detail,
        }),
    )?;
    let thread = result
        .thread
        .ok_or_else(|| "Codex thread preview 缺少 thread 信息".to_string())?;
    let is_full = detail == "full";
    let events = if is_full {
        preview_events_from_inputs(result.events)?
    } else {
        Vec::new()
    };
    Ok(CodexThreadPreview {
        thread,
        event_count: if is_full {
            events.len()
        } else {
            result.event_count.unwrap_or(result.messages.len())
        },
        events,
        messages: result.messages,
        has_full_preview: result.has_full_preview,
    })
}

fn task_row_by_id(conn: &rusqlite::Connection, task_id: &str) -> Result<Option<TaskRow>, String> {
    conn.query_row(
        r#"SELECT id, project_id, session_id, title, title_source, status, created_at, parent_id, sort_order, pinned
           FROM tasks
           WHERE id = ?1 AND archived = 0"#,
        params![task_id],
        |row| {
            let pinned: i64 = row.get(9)?;
            Ok(TaskRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                session_id: row.get(2)?,
                title: row.get(3)?,
                title_source: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
                parent_id: row.get(7)?,
                depends_on: Vec::new(),
                sort_order: row.get(8)?,
                pinned: pinned != 0,
            })
        },
    )
    .optional()
    .map_err(|e| format!("查询任务失败：{e}"))
}

fn next_task_sort_order(
    conn: &rusqlite::Connection,
    project_id: Option<&str>,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) FROM tasks WHERE (project_id = ?1 OR (project_id IS NULL AND ?1 IS NULL)) AND archived = 0",
        params![project_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|value| value + 1)
    .map_err(|e| format!("Codex history: 查询 sort_order 失败：{e}"))
}

fn create_task_for_thread(
    app: &AppHandle,
    conn: &rusqlite::Connection,
    project_id: Option<String>,
    thread: Option<&CodexThreadSummary>,
) -> Result<TaskRow, String> {
    let id = Uuid::new_v4().to_string();
    let title = thread
        .map(|thread| thread.title.trim())
        .filter(|title| !title.is_empty())
        .unwrap_or("Codex 历史对话")
        .to_string();
    let now = now_millis() as i64;
    let sort_order = next_task_sort_order(conn, project_id.as_deref())?;
    conn.execute(
        r#"INSERT INTO tasks (id, project_id, session_id, title, status, created_at, sort_order)
           VALUES (?1, ?2, ?3, ?4, 'waiting', ?5, ?6)"#,
        params![id.as_str(), project_id, id.as_str(), title, now, sort_order],
    )
    .map_err(|e| format!("创建 Codex 历史对话失败：{e}"))?;
    emit_tasks_changed(app, project_id.clone());
    task_row_by_id(conn, &id)?.ok_or_else(|| "创建 Codex 历史对话后读取失败".to_string())
}

fn insert_session_anchor(app: &AppHandle, task_id: &str, thread_id: &str) {
    persist_and_emit_input(
        app,
        session_anchor_input(task_id, thread_id, now_millis() as i64),
    );
}

fn session_anchor_input(task_id: &str, thread_id: &str, now: i64) -> AgentTimelineEventInput {
    AgentTimelineEventInput {
        id: Some(format!("{task_id}:codex-thread-attach:{thread_id}")),
        task_id: task_id.to_string(),
        turn_id: Some(format!("codex-thread-attach:{thread_id}")),
        backend: BACKEND_CODEX.to_string(),
        kind: "turn".to_string(),
        status: "success".to_string(),
        title: "Codex thread attached".to_string(),
        summary: Some("已接入 Codex thread".to_string()),
        payload: serde_json::json!({
            "backend": "codex",
            "sessionId": thread_id,
            "subkind": "thread_attach",
        }),
        created_at: Some(now),
        updated_at: Some(now),
    }
}

fn remember_codex_thread_session(
    conn: &rusqlite::Connection,
    chat_store: &ChatStore,
    task_id: &str,
    thread_id: &str,
) {
    remember_agent_session(
        conn,
        chat_store,
        task_id,
        BACKEND_CODEX,
        thread_id,
        "Codex attach",
    );
}

fn history_sync_error_input(
    task_id: &str,
    thread_id: &str,
    message: String,
) -> AgentTimelineEventInput {
    let now = now_millis() as i64;
    AgentTimelineEventInput {
        id: Some(format!("{task_id}:codex-history-sync-error:{thread_id}")),
        task_id: task_id.to_string(),
        turn_id: Some(format!("codex-history:{thread_id}")),
        backend: BACKEND_CODEX.to_string(),
        kind: "diagnostic".to_string(),
        status: "error".to_string(),
        title: "Codex history sync failed".to_string(),
        summary: Some("Codex 历史后台同步失败，已保留 thread 接入。".to_string()),
        payload: serde_json::json!({
            "backend": "codex",
            "subkind": "history_sync",
            "threadId": thread_id,
            "error": message,
        }),
        created_at: Some(now),
        updated_at: Some(now),
    }
}

const HISTORY_SYNC_LIMIT: i64 = 50;

fn normalize_next_cursor(cursor: Option<String>) -> Option<String> {
    cursor.and_then(|cursor| {
        let trimmed = cursor.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn history_sync_success_input(
    task_id: &str,
    thread_id: &str,
    event_count: usize,
    page_count: usize,
) -> AgentTimelineEventInput {
    let now = now_millis() as i64;
    AgentTimelineEventInput {
        id: Some(format!("{task_id}:codex-history-sync:{thread_id}")),
        task_id: task_id.to_string(),
        turn_id: Some(format!("codex-history:{thread_id}")),
        backend: BACKEND_CODEX.to_string(),
        kind: "diagnostic".to_string(),
        status: "success".to_string(),
        title: "Codex history synced".to_string(),
        summary: Some(if event_count > 0 {
            format!("已同步 {event_count} 条 Codex 历史事件")
        } else {
            "没有需要同步的 Codex 历史事件".to_string()
        }),
        payload: serde_json::json!({
            "backend": "codex",
            "subkind": "history_sync",
            "threadId": thread_id,
            "eventCount": event_count,
            "pageCount": page_count,
        }),
        created_at: Some(now),
        updated_at: Some(now),
    }
}

fn thread_rename_diagnostic_input(
    task_id: &str,
    thread_id: &str,
    name: &str,
    error: Option<String>,
) -> AgentTimelineEventInput {
    let now = now_millis() as i64;
    let (status, summary, error_message) = match error.as_deref() {
        Some(error) => (
            "error",
            "Codex thread 名称同步失败，Lilia 本地标题已保留。",
            Some(error),
        ),
        None => ("success", "已同步 Codex thread 名称", None),
    };
    let mut payload = serde_json::json!({
        "backend": "codex",
        "subkind": "thread_rename",
        "method": "thread/name/set",
        "threadId": thread_id,
        "name": name,
    });
    if let Some(error) = error_message {
        payload["error"] = serde_json::json!(error);
    }
    AgentTimelineEventInput {
        id: Some(format!("{task_id}:codex-thread-rename:{thread_id}")),
        task_id: task_id.to_string(),
        turn_id: Some(format!("codex-thread-rename:{thread_id}")),
        backend: BACKEND_CODEX.to_string(),
        kind: "diagnostic".to_string(),
        status: status.to_string(),
        title: "Codex thread renamed".to_string(),
        summary: Some(summary.to_string()),
        payload,
        created_at: Some(now),
        updated_at: Some(now),
    }
}

fn spawn_codex_history_sync(app: AppHandle, task_id: String, thread_id: String) {
    tauri::async_runtime::spawn_blocking(move || {
        let mut next_cursor: Option<String> = None;
        let mut page_count = 0usize;
        let mut event_count = 0usize;
        loop {
            let previous_cursor = next_cursor.clone();
            let result = run_codex_history_utility(
                &app,
                serde_json::json!({
                    "action": "sync",
                    "taskId": task_id,
                    "threadId": thread_id,
                    "limit": HISTORY_SYNC_LIMIT,
                    "cursor": next_cursor,
                }),
            );
            let sync = match result {
                Ok(sync) => sync,
                Err(err) => {
                    persist_and_emit_input(
                        &app,
                        history_sync_error_input(&task_id, &thread_id, err),
                    );
                    return;
                }
            };
            let page_event_count = persist_history_events_batch(&app, &task_id, sync.events);
            event_count += page_event_count;
            page_count += 1;
            next_cursor = normalize_next_cursor(sync.next_cursor);
            if next_cursor.is_none() {
                persist_and_emit_input(
                    &app,
                    history_sync_success_input(&task_id, &thread_id, event_count, page_count),
                );
                return;
            }
            if next_cursor == previous_cursor {
                persist_and_emit_input(
                    &app,
                    history_sync_error_input(
                        &task_id,
                        &thread_id,
                        "Codex history sync 返回了重复 cursor，已停止后台同步。".to_string(),
                    ),
                );
                return;
            }
        }
    });
}

pub(crate) fn codex_thread_attach_blocking(
    app: AppHandle,
    input: CodexThreadAttachInput,
) -> Result<CodexThreadAttachResult, String> {
    let thread_id = input.thread_id.trim().to_string();
    if thread_id.is_empty() {
        return Err("Codex threadId 不能为空".to_string());
    }
    let mode = input.mode.as_str();
    if mode != "current" && mode != "new" {
        return Err(format!("未知 Codex thread attach mode: {}", input.mode));
    }

    let store = app.state::<LiliaStore>();
    let chat_store = app.state::<ChatStore>();
    let conn = store.conn()?;
    let task = if mode == "new" {
        Some(create_task_for_thread(
            &app,
            &conn,
            input.project_id.clone(),
            input.thread.as_ref(),
        )?)
    } else {
        let task_id = input
            .task_id
            .as_ref()
            .ok_or_else(|| "接入当前对话需要 taskId".to_string())?;
        Some(task_row_by_id(&conn, task_id)?.ok_or_else(|| format!("未找到任务：{task_id}"))?)
    };
    let task = task.expect("task is always set");
    remember_codex_thread_session(&conn, &chat_store, &task.id, &thread_id);
    {
        let mut composers = chat_store.composers.lock().unwrap();
        let composer = composers
            .entry(task.id.clone())
            .or_insert_with(|| default_composer(&task.id));
        composer.backend = BACKEND_CODEX.to_string();
    }

    insert_session_anchor(&app, &task.id, &thread_id);
    spawn_codex_history_sync(app.clone(), task.id.clone(), thread_id.clone());
    Ok(CodexThreadAttachResult {
        task_id: task.id.clone(),
        project_id: task.project_id.clone(),
        thread_id,
        task: Some(task),
        event_count: 0,
        history_sync: Some("queued".to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::state::{persist_runtime_state, session_key, RunningTurn};

    fn create_task_agent_sessions_schema(conn: &rusqlite::Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE tasks (
              id           TEXT PRIMARY KEY,
              project_id   TEXT,
              session_id   TEXT NOT NULL,
              title        TEXT NOT NULL,
              status       TEXT NOT NULL,
              created_at   INTEGER NOT NULL,
              archived     INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE task_agent_sessions (
              task_id         TEXT NOT NULL,
              backend         TEXT NOT NULL CHECK (backend IN ('claude','codex')),
              session_id      TEXT NOT NULL,
              updated_at      INTEGER NOT NULL,
              PRIMARY KEY (task_id, backend)
            );
            CREATE TABLE task_runtime_states (
              task_id         TEXT PRIMARY KEY,
              turn_id         TEXT NOT NULL,
              backend         TEXT NOT NULL CHECK (backend IN ('claude','codex')),
              phase           TEXT NOT NULL CHECK (phase IN
                                ('running','interrupted_pending_finish','reset_pending_finish')),
              process_session_id TEXT,
              runtime_epoch   TEXT NOT NULL,
              context_json    TEXT,
              updated_at      INTEGER NOT NULL
            );
            CREATE TABLE task_pending_turns (
              id              INTEGER PRIMARY KEY AUTOINCREMENT,
              task_id         TEXT NOT NULL,
              content         TEXT NOT NULL,
              composer_json   TEXT NOT NULL,
              project_cwd     TEXT NOT NULL,
              attachments_json TEXT NOT NULL DEFAULT '[]',
              workflow_json   TEXT,
              runtime_command_json TEXT,
              message_json    TEXT NOT NULL,
              turn_id         TEXT NOT NULL,
              guide_id        TEXT,
              created_at      INTEGER NOT NULL
            );
            "#,
        )
        .unwrap();
    }

    fn assert_agent_session_checkpoint(
        conn: &rusqlite::Connection,
        store: &ChatStore,
        task_id: &str,
        backend: &str,
        expected: &str,
    ) {
        assert_eq!(
            store
                .sdk_sessions
                .lock()
                .unwrap()
                .get(&session_key(backend, task_id))
                .cloned(),
            Some(expected.to_string())
        );
        let session_id: String = conn
            .query_row(
                "SELECT session_id FROM task_agent_sessions WHERE task_id = ?1 AND backend = ?2",
                params![task_id, backend],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(session_id, expected);
    }

    #[test]
    fn session_anchor_records_codex_resume_thread_id() {
        let input = session_anchor_input("task-1", "thread-1", 1234);

        assert_eq!(
            input.id.as_deref(),
            Some("task-1:codex-thread-attach:thread-1")
        );
        assert_eq!(input.kind, "turn");
        assert_eq!(input.status, "success");
        assert_eq!(
            input
                .payload
                .get("sessionId")
                .and_then(|value| value.as_str()),
            Some("thread-1")
        );
        assert_eq!(
            input
                .payload
                .get("subkind")
                .and_then(|value| value.as_str()),
            Some("thread_attach")
        );
    }

    #[test]
    fn remember_codex_thread_session_updates_chat_store() {
        let store = ChatStore::default();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_task_agent_sessions_schema(&conn);

        remember_codex_thread_session(&conn, &store, "task-1", "thread-1");

        assert_agent_session_checkpoint(&conn, &store, "task-1", BACKEND_CODEX, "thread-1");
    }

    #[test]
    fn history_sync_error_input_records_thread_and_stable_id() {
        let input = history_sync_error_input("task-1", "thread-1", "network failed".to_string());

        assert_eq!(
            input.id.as_deref(),
            Some("task-1:codex-history-sync-error:thread-1")
        );
        assert_eq!(input.kind, "diagnostic");
        assert_eq!(input.status, "error");
        assert_eq!(
            input
                .payload
                .get("threadId")
                .and_then(|value| value.as_str()),
            Some("thread-1")
        );
        assert_eq!(
            input.payload.get("error").and_then(|value| value.as_str()),
            Some("network failed")
        );
    }

    #[test]
    fn normalize_next_cursor_trims_and_drops_empty_values() {
        assert_eq!(
            normalize_next_cursor(Some(" cursor-2 ".to_string())).as_deref(),
            Some("cursor-2")
        );
        assert_eq!(normalize_next_cursor(Some("".to_string())), None);
        assert_eq!(normalize_next_cursor(None), None);
    }

    #[test]
    fn runtime_states_join_codex_sessions_with_running_queue_state() {
        let store = ChatStore::default();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_task_agent_sessions_schema(&conn);
        conn.execute(
            r#"INSERT INTO tasks
               (id, project_id, session_id, title, status, created_at)
               VALUES ('task-1', 'project-1', 'task-1', 'Codex task', 'running', 1)"#,
            [],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO task_agent_sessions
               (task_id, backend, session_id, updated_at)
               VALUES ('task-1', 'codex', 'thread-1', 2)"#,
            [],
        )
        .unwrap();
        persist_runtime_state(
            &conn,
            &store,
            "task-1",
            &RunningTurn {
                turn_id: "turn-builtin".to_string(),
                backend: BACKEND_CODEX.to_string(),
                native_approval_pause: None,
            },
            "running",
            None,
            None,
        )
        .unwrap();
        store.pending_turns.lock().unwrap().insert(
            "task-1".to_string(),
            std::collections::VecDeque::from([crate::chat::state::PendingChatTurn {
                content: "queued".to_string(),
                composer: default_composer("task-1"),
                project_cwd: "D:/repo".to_string(),
                attachments: Vec::new(),
                conversation_references: Vec::new(),
                workflow: None,
                runtime_command: None,
                runtime_options: None,
                message: crate::chat::types::ChatMessage {
                    id: "msg-1".to_string(),
                    task_id: "task-1".to_string(),
                    role: "user".to_string(),
                    content: "queued".to_string(),
                    attachments: Vec::new(),
                    conversation_references: Vec::new(),
                    created_at: 1,
                },
                turn_id: "turn-1".to_string(),
                guide_id: None,
            }]),
        );

        let states = query_codex_thread_runtime_states(&conn, &store).unwrap();

        assert_eq!(states.len(), 1);
        assert_eq!(states[0].thread_id, "thread-1");
        assert_eq!(states[0].task_title, "Codex task");
        assert!(states[0].running);
        assert!(states[0].queued);
        assert_eq!(states[0].queued_count, 1);
    }

    #[test]
    fn clean_background_terminals_payload_trims_and_validates_thread_id() {
        let payload = clean_background_terminals_payload(" thread-1 ").unwrap();

        assert_eq!(payload["action"], "cleanBackgroundTerminals");
        assert_eq!(payload["threadId"], "thread-1");
        assert!(clean_background_terminals_payload("  ").is_err());
    }

    #[test]
    fn archive_thread_payload_trims_and_selects_action() {
        let archive = archive_thread_payload(" thread-1 ").unwrap();

        assert_eq!(archive["action"], "archiveThread");
        assert_eq!(archive["threadId"], "thread-1");
        assert!(archive_thread_payload("  ").is_err());
    }

    #[test]
    fn archive_sync_normalizes_thread_ids() {
        assert_eq!(
            normalize_thread_ids(vec![
                " thread-1 ".to_string(),
                " ".to_string(),
                "thread-2".to_string(),
            ]),
            vec!["thread-1", "thread-2"]
        );
    }

    #[test]
    fn archive_sync_runner_errors_are_best_effort() {
        let mut called = Vec::new();

        run_codex_thread_archive_sync(
            vec!["thread-1".to_string(), "thread-2".to_string()],
            |thread_id| {
                called.push(thread_id.to_string());
                if thread_id == "thread-1" {
                    Err("Codex app-server unavailable".to_string())
                } else {
                    Ok(())
                }
            },
        );

        assert_eq!(called, vec!["thread-1", "thread-2"]);
    }

    #[test]
    fn rename_thread_payload_trims_and_validates_inputs() {
        let payload = rename_thread_payload(" thread-1 ", " 新标题 ").unwrap();

        assert_eq!(payload["action"], "renameThread");
        assert_eq!(payload["threadId"], "thread-1");
        assert_eq!(payload["name"], "新标题");
        assert!(rename_thread_payload("  ", "新标题").is_err());
        assert!(rename_thread_payload("thread-1", "  ").is_err());
    }

    #[test]
    fn thread_rename_diagnostic_records_method_and_error() {
        let success = thread_rename_diagnostic_input("task-1", "thread-1", "新标题", None);

        assert_eq!(success.kind, "diagnostic");
        assert_eq!(success.status, "success");
        assert_eq!(
            success
                .payload
                .get("subkind")
                .and_then(|value| value.as_str()),
            Some("thread_rename")
        );
        assert_eq!(
            success
                .payload
                .get("method")
                .and_then(|value| value.as_str()),
            Some("thread/name/set")
        );
        assert_eq!(
            success.payload.get("name").and_then(|value| value.as_str()),
            Some("新标题")
        );

        let error = thread_rename_diagnostic_input(
            "task-1",
            "thread-1",
            "新标题",
            Some("network failed".to_string()),
        );

        assert_eq!(error.status, "error");
        assert_eq!(
            error.payload.get("error").and_then(|value| value.as_str()),
            Some("network failed")
        );
    }

    #[test]
    fn history_sync_success_input_records_total_events_and_pages() {
        let input = history_sync_success_input("task-1", "thread-1", 5, 2);

        assert_eq!(
            input.id.as_deref(),
            Some("task-1:codex-history-sync:thread-1")
        );
        assert_eq!(input.title, "Codex history synced");
        assert_eq!(input.summary.as_deref(), Some("已同步 5 条 Codex 历史事件"));
        assert_eq!(
            input
                .payload
                .get("eventCount")
                .and_then(|value| value.as_u64()),
            Some(5)
        );
        assert_eq!(
            input
                .payload
                .get("pageCount")
                .and_then(|value| value.as_u64()),
            Some(2)
        );
    }
}

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use tauri::{AppHandle, Manager, Runtime};
use uuid::Uuid;

use crate::agent_timeline::AgentTimelineEventInput;
use crate::chat::timeline_sink::persist_and_emit_input;
use crate::chat::types::{
    ChatAttachment, ChatComposerState, ChatContextUsage, ChatConversationReference, ChatMessage,
    ChatModelOption, ChatRollbackResult, ChatRuntimeCommand, ChatRuntimeSnapshot, ChatWorkflow,
    ProviderRuntimeOptions,
};
use crate::chat_backends_contract::chat_backends_contract;
use crate::provider::normalize_permission_mode;
use crate::store::LiliaStore;
use crate::{agent_timeline, todos};

#[derive(Debug, Clone)]
pub(crate) struct PendingChatTurn {
    pub(crate) content: String,
    pub(crate) composer: ChatComposerState,
    pub(crate) project_cwd: String,
    pub(crate) attachments: Vec<ChatAttachment>,
    pub(crate) conversation_references: Vec<ChatConversationReference>,
    pub(crate) workflow: Option<ChatWorkflow>,
    pub(crate) runtime_command: Option<ChatRuntimeCommand>,
    pub(crate) runtime_options: Option<ProviderRuntimeOptions>,
    pub(crate) message: ChatMessage,
    /// queue 时就分配好 turn_id，user message + agent turn 共享同一个 turn_id
    /// → 同一个 turn_seq；这是把"按 turn 隔离"的排序契约推到入口的关键。
    pub(crate) turn_id: String,
    pub(crate) guide_id: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct RunningTurn {
    pub(crate) turn_id: String,
    pub(crate) backend: String,
    pub(crate) native_approval_pause: Option<NativeApprovalPause>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct NativeApprovalPause {
    pub(crate) automation_run_id: Option<String>,
    pub(crate) last_session_id: Option<String>,
}

// ---------- 进程内状态 ----------

#[derive(Default)]
pub(crate) struct ChatStore {
    pub(crate) composers: Mutex<HashMap<String, ChatComposerState>>,
    /// SDK session id：key = "{backend}:{task_id}"，第一次发送为空，done 后写入用于 resume。
    pub(crate) sdk_sessions: Mutex<HashMap<String, String>>,
    pub(crate) running_tasks: Mutex<HashMap<String, bool>>,
    pub(crate) pending_turns: Mutex<HashMap<String, VecDeque<PendingChatTurn>>>,
    pub(crate) running_turns: Mutex<HashMap<String, RunningTurn>>,
    pub(crate) running_process_sessions: Mutex<HashMap<String, String>>,
    pub(crate) interrupted_turns: Mutex<HashMap<String, RunningTurn>>,
    pub(crate) reset_turns: Mutex<HashMap<String, RunningTurn>>,
    pub(crate) pending_rollbacks: Mutex<HashMap<String, ChatRollbackResult>>,
    pub(crate) pending_reset_cleanups: Mutex<HashSet<String>>,
    pub(crate) context_usage: Mutex<HashMap<String, ChatContextUsage>>,
    pub(crate) runtime_epoch: Mutex<Option<String>>,
}

pub(crate) fn session_key(backend: &str, task_id: &str) -> String {
    format!("{backend}:{task_id}")
}

pub(crate) fn register_running_turn(
    store: &ChatStore,
    task_id: impl Into<String>,
    turn_id: impl Into<String>,
    backend: impl Into<String>,
) {
    store.running_turns.lock().unwrap().insert(
        task_id.into(),
        RunningTurn {
            turn_id: turn_id.into(),
            backend: backend.into(),
            native_approval_pause: None,
        },
    );
}

pub(crate) fn pause_native_running_turn(
    store: &ChatStore,
    task_id: &str,
    turn_id: &str,
    automation_run_id: Option<String>,
    last_session_id: Option<String>,
) -> bool {
    let mut turns = store.running_turns.lock().unwrap();
    let Some(turn) = turns.get_mut(task_id).filter(|turn| {
        turn.turn_id == turn_id && turn.backend == crate::native_agent::BACKEND_NATIVE_AGENTKIT
    }) else {
        return false;
    };
    turn.native_approval_pause = Some(NativeApprovalPause {
        automation_run_id,
        last_session_id,
    });
    true
}

pub(crate) fn turn_uses_legacy_process_runtime(turn: &RunningTurn) -> bool {
    turn.backend != crate::native_agent::BACKEND_NATIVE_AGENTKIT
}

pub(crate) fn set_context_usage(store: &ChatStore, usage: ChatContextUsage) {
    let key = session_key(&usage.backend, &usage.task_id);
    store.context_usage.lock().unwrap().insert(key, usage);
}

pub(crate) fn latest_context_usage(
    store: &ChatStore,
    task_id: &str,
    backend: Option<&str>,
) -> Option<ChatContextUsage> {
    let usages = store.context_usage.lock().unwrap();
    if let Some(backend) = backend {
        if let Some(usage) = usages.get(&session_key(backend, task_id)) {
            return Some(usage.clone());
        }
    }
    usages
        .values()
        .filter(|usage| usage.task_id == task_id)
        .max_by_key(|usage| usage.updated_at)
        .cloned()
}

pub(crate) fn runtime_epoch(store: &ChatStore) -> String {
    let mut epoch = store.runtime_epoch.lock().unwrap();
    if let Some(epoch) = epoch.as_ref() {
        return epoch.clone();
    }
    let next = Uuid::new_v4().to_string();
    *epoch = Some(next.clone());
    next
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PersistedRuntimeState {
    pub(crate) task_id: String,
    pub(crate) turn: RunningTurn,
    pub(crate) phase: String,
    pub(crate) process_session_id: Option<String>,
    pub(crate) runtime_epoch: String,
    pub(crate) context_json: Option<String>,
}

pub(crate) fn persist_runtime_state(
    conn: &Connection,
    chat_store: &ChatStore,
    task_id: &str,
    turn: &RunningTurn,
    phase: &str,
    process_session_id: Option<&str>,
    context_json: Option<&str>,
) -> Result<(), String> {
    let phase = match phase {
        "running" | "interrupted_pending_finish" | "reset_pending_finish" => phase,
        _ => return Err(format!("未知 runtime phase: {phase}")),
    };
    conn.execute(
        r#"INSERT INTO task_runtime_states
           (task_id, turn_id, backend, phase, process_session_id, runtime_epoch, context_json, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
           ON CONFLICT(task_id) DO UPDATE SET
             turn_id = excluded.turn_id,
             backend = excluded.backend,
             phase = excluded.phase,
             process_session_id = COALESCE(excluded.process_session_id, task_runtime_states.process_session_id),
             runtime_epoch = excluded.runtime_epoch,
             context_json = COALESCE(excluded.context_json, task_runtime_states.context_json),
             updated_at = excluded.updated_at"#,
        params![
            task_id,
            turn.turn_id,
            normalize_backend(&turn.backend),
            phase,
            process_session_id.map(str::trim).filter(|value| !value.is_empty()),
            runtime_epoch(chat_store),
            context_json.map(str::trim).filter(|value| !value.is_empty()),
            now_millis() as i64
        ],
    )
    .map(|_| ())
    .map_err(|e| format!("写入 runtime state 失败：{e}"))
}

pub(crate) fn clear_runtime_state(conn: &Connection, task_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM task_runtime_states WHERE task_id = ?1",
        params![task_id],
    )
    .map(|_| ())
    .map_err(|e| format!("清理 runtime state 失败：{e}"))
}

pub(crate) fn clear_runtime_finalization(conn: &Connection, task_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM task_runtime_finalizations WHERE task_id = ?1",
        params![task_id],
    )
    .map(|_| ())
    .map_err(|e| format!("清理 runtime finalization 失败：{e}"))
}

pub(crate) fn load_runtime_state(
    conn: &Connection,
    chat_store: &ChatStore,
    task_id: &str,
) -> Result<Option<PersistedRuntimeState>, String> {
    let epoch = runtime_epoch(chat_store);
    conn.query_row(
        r#"SELECT task_id, turn_id, backend, phase, process_session_id, runtime_epoch
                  , context_json
           FROM task_runtime_states
           WHERE task_id = ?1 AND runtime_epoch = ?2"#,
        params![task_id, epoch],
        |row| {
            Ok(PersistedRuntimeState {
                task_id: row.get(0)?,
                turn: RunningTurn {
                    turn_id: row.get(1)?,
                    backend: row.get(2)?,
                    native_approval_pause: None,
                },
                phase: row.get(3)?,
                process_session_id: row.get(4)?,
                runtime_epoch: row.get(5)?,
                context_json: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("读取 runtime state 失败：{e}"))
}

pub(crate) fn load_any_runtime_state(
    conn: &Connection,
    task_id: &str,
) -> Result<Option<PersistedRuntimeState>, String> {
    conn.query_row(
        r#"SELECT task_id, turn_id, backend, phase, process_session_id, runtime_epoch
                  , context_json
           FROM task_runtime_states
           WHERE task_id = ?1
           ORDER BY updated_at DESC
           LIMIT 1"#,
        params![task_id],
        |row| {
            Ok(PersistedRuntimeState {
                task_id: row.get(0)?,
                turn: RunningTurn {
                    turn_id: row.get(1)?,
                    backend: row.get(2)?,
                    native_approval_pause: None,
                },
                phase: row.get(3)?,
                process_session_id: row.get(4)?,
                runtime_epoch: row.get(5)?,
                context_json: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("读取 runtime state 失败：{e}"))
}

pub(crate) fn list_runtime_states(conn: &Connection) -> Result<Vec<PersistedRuntimeState>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT task_id, turn_id, backend, phase, process_session_id, runtime_epoch
                      , context_json
               FROM task_runtime_states
               ORDER BY updated_at DESC"#,
        )
        .map_err(|e| format!("读取 runtime states 失败：{e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PersistedRuntimeState {
                task_id: row.get(0)?,
                turn: RunningTurn {
                    turn_id: row.get(1)?,
                    backend: row.get(2)?,
                    native_approval_pause: None,
                },
                phase: row.get(3)?,
                process_session_id: row.get(4)?,
                runtime_epoch: row.get(5)?,
                context_json: row.get(6)?,
            })
        })
        .map_err(|e| format!("读取 runtime states 失败：{e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取 runtime states 失败：{e}"))
}

pub(crate) fn persist_runtime_state_for_app<R: Runtime>(
    app: &AppHandle<R>,
    chat_store: &ChatStore,
    task_id: &str,
    turn: &RunningTurn,
    phase: &str,
    process_session_id: Option<&str>,
    context_json: Option<&str>,
) {
    let Some(store) = app.try_state::<LiliaStore>() else {
        return;
    };
    if let Err(err) = store.conn().and_then(|conn| {
        persist_runtime_state(
            &conn,
            chat_store,
            task_id,
            turn,
            phase,
            process_session_id,
            context_json,
        )
    }) {
        eprintln!("[chat-runtime] persist runtime state failed: {err}");
    }
}

pub(crate) fn clear_runtime_state_for_app<R: Runtime>(app: &AppHandle<R>, task_id: &str) {
    let Some(store) = app.try_state::<LiliaStore>() else {
        return;
    };
    if let Err(err) = store
        .conn()
        .and_then(|conn| clear_runtime_state(&conn, task_id))
    {
        eprintln!("[chat-runtime] clear runtime state failed: {err}");
    }
}

pub(crate) fn persisted_runtime_state_is_active(
    persisted: &PersistedRuntimeState,
    store: &ChatStore,
) -> bool {
    if let Some(process_session_id) = persisted.process_session_id.as_deref() {
        return super::runner::process_session_is_active(process_session_id);
    }
    load_runtime_state_shares_current_epoch(persisted, store)
}

fn load_runtime_state_shares_current_epoch(
    persisted: &PersistedRuntimeState,
    store: &ChatStore,
) -> bool {
    persisted.runtime_epoch == runtime_epoch(store)
}

pub(crate) fn restore_active_runtime_sessions(
    conn: &Connection,
    store: &ChatStore,
) -> Vec<PersistedRuntimeState> {
    let Ok(states) = list_runtime_states(conn) else {
        return Vec::new();
    };
    let mut restored = Vec::new();
    for persisted in states {
        if !persisted_runtime_state_is_active(&persisted, store) {
            continue;
        }
        let Some(process_session_id) = persisted.process_session_id.clone() else {
            continue;
        };
        store
            .running_tasks
            .lock()
            .unwrap()
            .insert(persisted.task_id.clone(), true);
        store
            .running_turns
            .lock()
            .unwrap()
            .insert(persisted.task_id.clone(), persisted.turn.clone());
        store
            .running_process_sessions
            .lock()
            .unwrap()
            .insert(persisted.task_id.clone(), process_session_id);
        restored.push(persisted);
    }
    restored
}

pub(crate) fn prepare_pending_turn_recovery(
    conn: &Connection,
    store: &ChatStore,
    task_id: &str,
) -> Result<bool, String> {
    let Some(persisted) = load_any_runtime_state(conn, task_id)? else {
        return Ok(true);
    };
    if persisted_runtime_state_is_active(&persisted, store) {
        return Ok(false);
    }
    clear_runtime_state(conn, task_id)?;
    Ok(true)
}

pub(crate) fn take_next_recoverable_pending_turn(
    conn: &Connection,
    store: &ChatStore,
    task_id: &str,
) -> Result<Option<PendingChatTurn>, String> {
    if !prepare_pending_turn_recovery(conn, store, task_id)? {
        return Ok(None);
    }
    take_next_persisted_pending_turn(conn, task_id)
}

pub(crate) fn load_persisted_resume_session_id(
    conn: &Connection,
    task_id: &str,
    backend: &str,
) -> Option<String> {
    if let Ok(Some(session_id)) = load_task_agent_session_id(conn, task_id, backend) {
        return Some(session_id);
    }
    agent_timeline::latest_session_id(conn, task_id, backend)
        .ok()
        .flatten()
}

fn load_task_agent_session_id(
    conn: &Connection,
    task_id: &str,
    backend: &str,
) -> Result<Option<String>, String> {
    let session_id = conn
        .query_row(
            r#"SELECT session_id
               FROM task_agent_sessions
               WHERE task_id = ?1 AND backend = ?2"#,
            params![task_id, backend],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| format!("读取 agent session checkpoint 失败：{e}"))?;
    Ok(session_id.and_then(|sid| {
        let trimmed = sid.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }))
}

pub(crate) fn persist_agent_session_id(
    conn: &Connection,
    task_id: &str,
    backend: &str,
    session_id: &str,
) -> Result<(), String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    conn.execute(
        r#"INSERT INTO task_agent_sessions
           (task_id, backend, session_id, updated_at)
           VALUES (?1, ?2, ?3, ?4)
           ON CONFLICT(task_id, backend) DO UPDATE SET
             session_id = excluded.session_id,
             updated_at = excluded.updated_at"#,
        params![task_id, backend, trimmed, now_millis() as i64],
    )
    .map(|_| ())
    .map_err(|e| format!("写入 agent session checkpoint 失败：{e}"))
}

pub(crate) fn remember_agent_session(
    conn: &Connection,
    chat_store: &ChatStore,
    task_id: &str,
    backend: &str,
    session_id: &str,
    log_context: &str,
) {
    chat_store
        .sdk_sessions
        .lock()
        .unwrap()
        .insert(session_key(backend, task_id), session_id.to_string());
    if let Err(err) = persist_agent_session_id(conn, task_id, backend, session_id) {
        eprintln!("[agent-session] persist {log_context} checkpoint failed: {err}");
    }
}

pub(crate) fn clear_agent_sessions_for_task(
    conn: &Connection,
    task_id: &str,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM task_agent_sessions WHERE task_id = ?1",
        params![task_id],
    )
    .map(|_| ())
    .map_err(|e| format!("清理 agent session checkpoint 失败：{e}"))
}

pub(crate) fn clear_task_runtime_state_for_reset<R: Runtime>(
    app: &AppHandle<R>,
    chat_store: &ChatStore,
    task_id: &str,
) {
    let mut sessions = chat_store.sdk_sessions.lock().unwrap();
    for backend in chat_backends() {
        sessions.remove(&session_key(backend, task_id));
    }
    drop(sessions);

    if let Some(store) = app.try_state::<LiliaStore>() {
        if let Err(err) = store.conn().and_then(|conn| {
            clear_agent_sessions_for_task(&conn, task_id)?;
            clear_runtime_state(&conn, task_id)?;
            clear_runtime_finalization(&conn, task_id)?;
            agent_timeline::clear(&conn, task_id).map(|_| ())
        }) {
            eprintln!("[agent-timeline] clear on reset failed: {err}");
        }
    }
}

pub(crate) fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn new_chat_message_id() -> String {
    format!("u-{}", Uuid::new_v4())
}

pub(crate) fn normalize_backend(value: &str) -> &'static str {
    try_normalize_backend(value).unwrap_or_else(default_backend)
}

pub(crate) fn chat_backends() -> &'static [String] {
    &chat_backends_contract().chat_backends
}

pub(crate) fn chat_backend_supported(value: &str) -> bool {
    try_normalize_backend(value).is_some()
}

pub(crate) fn try_normalize_backend(value: &str) -> Option<&'static str> {
    let value = value.trim();
    chat_backends()
        .iter()
        .find(|backend| backend.as_str() == value)
        .map(String::as_str)
}

pub(crate) fn default_backend() -> &'static str {
    chat_backends_contract().default_backend.as_str()
}

pub(crate) fn default_model_for_backend(backend: &str) -> &'static str {
    let backend = normalize_backend(backend);
    chat_backends_contract()
        .default_models
        .get(backend)
        .map(String::as_str)
        .expect("chat-backends.json missing defaultModels backend")
}

pub(crate) fn model_options_for_backend(backend: &str) -> Vec<ChatModelOption> {
    let backend = normalize_backend(backend);
    chat_backends_contract()
        .backend_models
        .get(backend)
        .expect("chat-backends.json missing backendModels backend")
        .iter()
        .map(|option| ChatModelOption {
            id: option.id.clone(),
            label: option.label.clone(),
            backend: backend.to_string(),
        })
        .collect()
}

fn model_belongs_to_backend(model: &str, backend: &str) -> bool {
    let model = model.trim();
    if model.is_empty() {
        return false;
    }
    model_options_for_backend(backend)
        .iter()
        .any(|option| option.id == model)
        || chat_backends_contract()
            .allowed_model_prefixes
            .get(backend)
            .map(|prefixes| prefixes.iter().any(|prefix| model.starts_with(prefix)))
            .unwrap_or(false)
}

pub(crate) fn normalize_model_for_backend(model: &str, backend: &str) -> String {
    let backend = normalize_backend(backend);
    let model = model.trim();
    if model_belongs_to_backend(model, backend) {
        model.to_string()
    } else {
        default_model_for_backend(backend).to_string()
    }
}

pub(crate) fn normalize_model_selection_mode(value: &str) -> String {
    if value == "manual" {
        "manual".to_string()
    } else {
        "auto".to_string()
    }
}

pub(crate) fn normalize_reasoning_effort_for_backend(
    effort: Option<String>,
    backend: &str,
) -> Option<String> {
    let value = effort.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })?;
    let backend = normalize_backend(backend);
    let manifest = chat_backends_contract();
    let supported_efforts = manifest.backend_reasoning_efforts.get(backend)?;
    let value_index = manifest
        .reasoning_efforts
        .iter()
        .position(|item| item == &value)?;
    if supported_efforts.iter().any(|item| item == &value) {
        return Some(value);
    }
    manifest
        .reasoning_efforts
        .iter()
        .take(value_index)
        .rev()
        .find(|candidate| supported_efforts.iter().any(|item| item == *candidate))
        .cloned()
        .or_else(|| supported_efforts.first().cloned())
}

pub(crate) fn normalize_composer_for_backend(
    mut composer: ChatComposerState,
    task_id: &str,
    backend: &str,
) -> ChatComposerState {
    let backend = normalize_backend(backend);
    composer.task_id = task_id.to_string();
    composer.backend = backend.to_string();
    composer.model = normalize_model_for_backend(&composer.model, backend);
    composer.model_selection_mode = normalize_model_selection_mode(&composer.model_selection_mode);
    composer.reasoning_effort =
        normalize_reasoning_effort_for_backend(composer.reasoning_effort, backend);
    composer.permission = normalize_permission_mode(&composer.permission);
    composer
}

pub(crate) fn default_composer(task_id: &str) -> ChatComposerState {
    let backend = default_backend();
    ChatComposerState {
        task_id: task_id.to_string(),
        backend: backend.to_string(),
        model: default_model_for_backend(backend).to_string(),
        model_selection_mode: "auto".to_string(),
        reasoning_effort: None,
        plan_mode: false,
        goal_mode: false,
        permission: normalize_permission_mode(""),
        codex_settings: Default::default(),
    }
}

fn serialize_pending_turn_field<T: serde::Serialize>(
    value: &T,
    field: &str,
) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("pending turn {field} 序列化失败：{e}"))
}

fn deserialize_pending_turn_field<T: serde::de::DeserializeOwned>(
    text: &str,
    field: &str,
) -> Result<T, String> {
    serde_json::from_str(text).map_err(|e| format!("pending turn {field} 解析失败：{e}"))
}

pub(crate) fn persist_pending_turn(
    conn: &Connection,
    task_id: &str,
    turn: &PendingChatTurn,
) -> Result<(), String> {
    let composer_json = serialize_pending_turn_field(&turn.composer, "composer")?;
    let attachments_json = serialize_pending_turn_field(&turn.attachments, "attachments")?;
    let workflow_json = turn
        .workflow
        .as_ref()
        .map(|workflow| serialize_pending_turn_field(workflow, "workflow"))
        .transpose()?;
    let runtime_command_json = turn
        .runtime_command
        .as_ref()
        .map(|command| serialize_pending_turn_field(command, "runtime_command"))
        .transpose()?;
    let runtime_options_json = turn
        .runtime_options
        .as_ref()
        .map(|options| serialize_pending_turn_field(options, "runtime_options"))
        .transpose()?;
    let message_json = serialize_pending_turn_field(&turn.message, "message")?;
    conn.execute(
        r#"INSERT INTO task_pending_turns
           (task_id, content, composer_json, project_cwd, attachments_json, workflow_json,
            runtime_command_json, runtime_options_json, message_json, turn_id, guide_id, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"#,
        params![
            task_id,
            turn.content,
            composer_json,
            turn.project_cwd,
            attachments_json,
            workflow_json,
            runtime_command_json,
            runtime_options_json,
            message_json,
            turn.turn_id,
            turn.guide_id,
            now_millis() as i64
        ],
    )
    .map(|_| ())
    .map_err(|e| format!("写入 pending turn 失败：{e}"))
}

pub(crate) fn count_pending_turns(conn: &Connection, task_id: &str) -> Result<usize, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM task_pending_turns WHERE task_id = ?1",
        params![task_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count.max(0) as usize)
    .map_err(|e| format!("统计 pending turns 失败：{e}"))
}

fn row_to_pending_turn(row: &rusqlite::Row<'_>) -> Result<(i64, PendingChatTurn), String> {
    let id: i64 = row
        .get(0)
        .map_err(|e| format!("读取 pending turn id 失败：{e}"))?;
    let content: String = row
        .get(1)
        .map_err(|e| format!("读取 pending turn content 失败：{e}"))?;
    let composer_json: String = row
        .get(2)
        .map_err(|e| format!("读取 pending turn composer 失败：{e}"))?;
    let project_cwd: String = row
        .get(3)
        .map_err(|e| format!("读取 pending turn cwd 失败：{e}"))?;
    let attachments_json: String = row
        .get(4)
        .map_err(|e| format!("读取 pending turn attachments 失败：{e}"))?;
    let workflow_json: Option<String> = row
        .get(5)
        .map_err(|e| format!("读取 pending turn workflow 失败：{e}"))?;
    let runtime_command_json: Option<String> = row
        .get(6)
        .map_err(|e| format!("读取 pending turn runtime_command 失败：{e}"))?;
    let runtime_options_json: Option<String> = row
        .get(7)
        .map_err(|e| format!("读取 pending turn runtime_options 失败：{e}"))?;
    let message_json: String = row
        .get(8)
        .map_err(|e| format!("读取 pending turn message 失败：{e}"))?;
    let turn_id: String = row
        .get(9)
        .map_err(|e| format!("读取 pending turn turn_id 失败：{e}"))?;
    let guide_id: Option<String> = row
        .get(10)
        .map_err(|e| format!("读取 pending turn guide_id 失败：{e}"))?;
    let message: ChatMessage = deserialize_pending_turn_field(&message_json, "message")?;
    let conversation_references = message.conversation_references.clone();
    Ok((
        id,
        PendingChatTurn {
            content,
            composer: deserialize_pending_turn_field(&composer_json, "composer")?,
            project_cwd,
            attachments: deserialize_pending_turn_field(&attachments_json, "attachments")?,
            conversation_references,
            workflow: workflow_json
                .as_deref()
                .map(|text| deserialize_pending_turn_field(text, "workflow"))
                .transpose()?,
            runtime_command: runtime_command_json
                .as_deref()
                .map(|text| deserialize_pending_turn_field(text, "runtime_command"))
                .transpose()?,
            runtime_options: runtime_options_json
                .as_deref()
                .map(|text| deserialize_pending_turn_field(text, "runtime_options"))
                .transpose()?,
            message,
            turn_id,
            guide_id,
        },
    ))
}

pub(crate) fn take_next_persisted_pending_turn(
    conn: &Connection,
    task_id: &str,
) -> Result<Option<PendingChatTurn>, String> {
    let row = conn
        .query_row(
            r#"SELECT id, content, composer_json, project_cwd, attachments_json, workflow_json,
                      runtime_command_json, runtime_options_json, message_json, turn_id, guide_id
               FROM task_pending_turns
               WHERE task_id = ?1
               ORDER BY id ASC
               LIMIT 1"#,
            params![task_id],
            |row| {
                row_to_pending_turn(row).map_err(|err| rusqlite::Error::InvalidParameterName(err))
            },
        )
        .optional()
        .map_err(|e| format!("读取 pending turn 失败：{e}"))?;
    let Some((id, turn)) = row else {
        return Ok(None);
    };
    conn.execute("DELETE FROM task_pending_turns WHERE id = ?1", params![id])
        .map_err(|e| format!("删除 pending turn 失败：{e}"))?;
    Ok(Some(turn))
}

pub(crate) fn clear_persisted_pending_turns(
    conn: &Connection,
    task_id: &str,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT guide_id
               FROM task_pending_turns
               WHERE task_id = ?1
               ORDER BY id ASC"#,
        )
        .map_err(|e| format!("prepare pending turn guide ids 失败：{e}"))?;
    let rows = stmt
        .query_map(params![task_id], |row| row.get::<_, Option<String>>(0))
        .map_err(|e| format!("query pending turn guide ids 失败：{e}"))?;
    let mut guide_ids = Vec::new();
    for row in rows {
        if let Some(guide_id) = row.map_err(|e| format!("读取 pending turn guide_id 失败：{e}"))?
        {
            guide_ids.push(guide_id);
        }
    }
    conn.execute(
        "DELETE FROM task_pending_turns WHERE task_id = ?1",
        params![task_id],
    )
    .map_err(|e| format!("清理 pending turns 失败：{e}"))?;
    Ok(guide_ids)
}

pub(crate) fn queue_pending_turn_for_app<R: Runtime>(
    app: &AppHandle<R>,
    store: &ChatStore,
    task_id: &str,
    content: String,
    composer: ChatComposerState,
    project_cwd: String,
    attachments: Vec<ChatAttachment>,
    conversation_references: Vec<ChatConversationReference>,
    workflow: Option<ChatWorkflow>,
    runtime_command: Option<ChatRuntimeCommand>,
    runtime_options: Option<ProviderRuntimeOptions>,
    message: ChatMessage,
    turn_id: String,
    guide_id: Option<String>,
) -> usize {
    let turn = PendingChatTurn {
        content,
        composer,
        project_cwd,
        attachments,
        conversation_references,
        workflow,
        runtime_command,
        runtime_options,
        message,
        turn_id,
        guide_id,
    };
    if let Some(lilia_store) = app.try_state::<LiliaStore>() {
        if let Ok(conn) = lilia_store.conn() {
            match persist_pending_turn(&conn, task_id, &turn)
                .and_then(|_| count_pending_turns(&conn, task_id))
            {
                Ok(count) => return count,
                Err(err) => eprintln!("[chat-runtime] persist pending turn failed: {err}"),
            }
        }
    }
    let mut pending = store.pending_turns.lock().unwrap();
    let queue = pending.entry(task_id.to_string()).or_default();
    queue.push_back(turn);
    queue.len()
}

pub(crate) fn should_persist_user_message(
    content: &str,
    workflow: &Option<ChatWorkflow>,
    runtime_command: &Option<ChatRuntimeCommand>,
) -> bool {
    let empty = content.trim().is_empty();
    if !empty {
        return true;
    }
    if runtime_command.is_some() {
        return false;
    }
    !matches!(
        workflow,
        Some(ChatWorkflow::LiliaReview { .. })
            | Some(ChatWorkflow::LiliaFixSuggestion { .. })
            | Some(ChatWorkflow::LiliaBatchApply { .. })
            | Some(ChatWorkflow::LiliaTaskWorkflow { .. })
            | Some(ChatWorkflow::LiliaGoal { .. })
            | Some(ChatWorkflow::LiliaCompact)
            | Some(ChatWorkflow::LiliaBackgroundTerminalsClean)
            | Some(ChatWorkflow::LiliaMemoryMode { .. })
            | Some(ChatWorkflow::LiliaMemoryReset)
            | Some(ChatWorkflow::LiliaConfigDiagnostics { .. })
            | Some(ChatWorkflow::SlashCommand { .. })
            | Some(ChatWorkflow::Automation { .. })
    )
}

pub(crate) fn clear_pending_turns(store: &ChatStore, task_id: &str) -> Vec<String> {
    store
        .pending_turns
        .lock()
        .unwrap()
        .remove(task_id)
        .map(|queue| queue.into_iter().filter_map(|turn| turn.guide_id).collect())
        .unwrap_or_default()
}

pub(crate) fn clear_persisted_pending_turns_for_app<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
) -> Vec<String> {
    let Some(lilia_store) = app.try_state::<LiliaStore>() else {
        return Vec::new();
    };
    let Ok(conn) = lilia_store.conn() else {
        return Vec::new();
    };
    match clear_persisted_pending_turns(&conn, task_id) {
        Ok(guide_ids) => guide_ids,
        Err(err) => {
            eprintln!("[chat-runtime] clear persisted pending turns failed: {err}");
            Vec::new()
        }
    }
}

pub(crate) fn list_pending_turn_task_ids(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT DISTINCT task_id
               FROM task_pending_turns
               ORDER BY task_id ASC"#,
        )
        .map_err(|e| format!("prepare pending turn task ids 失败：{e}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("query pending turn task ids 失败：{e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取 pending turn task ids 失败：{e}"))
}

pub(crate) fn set_guide_status_for_app<R: Runtime>(
    app: &AppHandle<R>,
    guide_id: Option<&str>,
    status: &str,
) -> Result<(), String> {
    let Some(guide_id) = guide_id else {
        return Ok(());
    };
    let Some(store) = app.try_state::<LiliaStore>() else {
        return Ok(());
    };
    todos::set_lilia_guide_status(app, &store, guide_id, status)
}

pub(crate) fn reset_cleared_guide_queue<R: Runtime>(app: &AppHandle<R>, guide_ids: Vec<String>) {
    for guide_id in guide_ids {
        if let Err(err) = set_guide_status_for_app(app, Some(&guide_id), "pending") {
            eprintln!("[todo-guides] reset queued guide failed: {err}");
        }
    }
}

pub(crate) struct PreparedTurnStop {
    pub(crate) running_turn: RunningTurn,
    pub(crate) guide_ids: Vec<String>,
}

pub(crate) struct FinishedRunningTurn {
    pub(crate) interrupted: bool,
    pub(crate) reset: bool,
}

pub(crate) fn clear_running_handles(store: &ChatStore, task_id: &str) -> Option<RunningTurn> {
    store
        .running_process_sessions
        .lock()
        .unwrap()
        .remove(task_id);
    store.running_turns.lock().unwrap().remove(task_id)
}

#[cfg(test)]
pub(crate) fn set_pending_rollback(store: &ChatStore, task_id: &str, rollback: ChatRollbackResult) {
    store
        .pending_rollbacks
        .lock()
        .unwrap()
        .insert(task_id.to_string(), rollback);
}

#[cfg(test)]
pub(crate) fn persist_pending_rollback(
    conn: &Connection,
    task_id: &str,
    rollback: &ChatRollbackResult,
) -> Result<(), String> {
    let rollback_json =
        serde_json::to_string(rollback).map_err(|e| format!("runtime rollback 序列化失败：{e}"))?;
    conn.execute(
        r#"INSERT INTO task_runtime_finalizations
           (task_id, pending_reset_cleanup, rollback_json, updated_at)
           VALUES (?1, 0, ?2, ?3)
           ON CONFLICT(task_id) DO UPDATE SET
             rollback_json = excluded.rollback_json,
             updated_at = excluded.updated_at"#,
        params![task_id, rollback_json, now_millis() as i64],
    )
    .map(|_| ())
    .map_err(|e| format!("写入 runtime rollback 失败：{e}"))
}

pub(crate) fn take_pending_rollback(
    store: &ChatStore,
    task_id: &str,
) -> Option<ChatRollbackResult> {
    store.pending_rollbacks.lock().unwrap().remove(task_id)
}

pub(crate) fn take_persisted_pending_rollback(
    conn: &Connection,
    task_id: &str,
) -> Result<Option<ChatRollbackResult>, String> {
    let rollback = read_persisted_pending_rollback(conn, task_id)?;
    clear_persisted_pending_rollback(conn, task_id)?;
    Ok(rollback)
}

fn read_persisted_pending_rollback(
    conn: &Connection,
    task_id: &str,
) -> Result<Option<ChatRollbackResult>, String> {
    let rollback_json: Option<String> = conn
        .query_row(
            r#"SELECT rollback_json
           FROM task_runtime_finalizations
           WHERE task_id = ?1"#,
            params![task_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("读取 runtime rollback 失败：{e}"))?
        .flatten();
    rollback_json
        .map(|text| {
            serde_json::from_str::<ChatRollbackResult>(&text)
                .map_err(|e| format!("runtime rollback 解析失败：{e}"))
        })
        .transpose()
}

pub(crate) fn peek_persisted_pending_rollback(
    conn: &Connection,
    task_id: &str,
) -> Result<Option<ChatRollbackResult>, String> {
    read_persisted_pending_rollback(conn, task_id)
}

pub(crate) fn clear_persisted_pending_rollback(
    conn: &Connection,
    task_id: &str,
) -> Result<(), String> {
    conn.execute(
        r#"UPDATE task_runtime_finalizations
           SET rollback_json = NULL, updated_at = ?2
           WHERE task_id = ?1"#,
        params![task_id, now_millis() as i64],
    )
    .map(|_| ())
    .map_err(|e| format!("清理 runtime rollback 失败：{e}"))
}

#[cfg(test)]
pub(crate) fn mark_pending_reset_cleanup(store: &ChatStore, task_id: &str) {
    store
        .pending_reset_cleanups
        .lock()
        .unwrap()
        .insert(task_id.to_string());
}

#[cfg(test)]
pub(crate) fn persist_pending_reset_cleanup(
    conn: &Connection,
    task_id: &str,
) -> Result<(), String> {
    conn.execute(
        r#"INSERT INTO task_runtime_finalizations
           (task_id, pending_reset_cleanup, rollback_json, updated_at)
           VALUES (?1, 1, NULL, ?2)
           ON CONFLICT(task_id) DO UPDATE SET
             pending_reset_cleanup = 1,
             updated_at = excluded.updated_at"#,
        params![task_id, now_millis() as i64],
    )
    .map(|_| ())
    .map_err(|e| format!("写入 runtime reset cleanup 失败：{e}"))
}

pub(crate) fn take_pending_reset_cleanup(store: &ChatStore, task_id: &str) -> bool {
    store.pending_reset_cleanups.lock().unwrap().remove(task_id)
}

pub(crate) fn take_persisted_pending_reset_cleanup(
    conn: &Connection,
    task_id: &str,
) -> Result<bool, String> {
    let pending: Option<i64> = conn
        .query_row(
            r#"SELECT pending_reset_cleanup
               FROM task_runtime_finalizations
               WHERE task_id = ?1"#,
            params![task_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("读取 runtime reset cleanup 失败：{e}"))?;
    conn.execute(
        r#"UPDATE task_runtime_finalizations
           SET pending_reset_cleanup = 0, updated_at = ?2
           WHERE task_id = ?1"#,
        params![task_id, now_millis() as i64],
    )
    .map_err(|e| format!("清理 runtime reset cleanup 失败：{e}"))?;
    Ok(pending.unwrap_or(0) != 0)
}

pub(crate) fn take_pending_finalization_for_app<R: Runtime>(
    app: &AppHandle<R>,
    store: &ChatStore,
    task_id: &str,
) -> (Option<ChatRollbackResult>, bool) {
    let mut rollback = take_pending_rollback(store, task_id);
    let mut reset_cleanup = take_pending_reset_cleanup(store, task_id);
    if let Some(lilia_store) = app.try_state::<LiliaStore>() {
        if let Ok(conn) = lilia_store.conn() {
            match take_persisted_pending_rollback(&conn, task_id) {
                Ok(persisted) => {
                    if rollback.is_none() {
                        rollback = persisted;
                    }
                }
                Err(err) => eprintln!("[chat-runtime] take persisted rollback failed: {err}"),
            }
            match take_persisted_pending_reset_cleanup(&conn, task_id) {
                Ok(persisted) => reset_cleanup = reset_cleanup || persisted,
                Err(err) => {
                    eprintln!("[chat-runtime] take persisted reset cleanup failed: {err}")
                }
            }
        }
    }
    (rollback, reset_cleanup)
}

pub(crate) fn chat_runtime_snapshot(store: &ChatStore, task_id: &str) -> ChatRuntimeSnapshot {
    let running_turn = store.running_turns.lock().unwrap().get(task_id).cloned();
    let queued_count = store
        .pending_turns
        .lock()
        .unwrap()
        .get(task_id)
        .map(|queue| queue.len())
        .unwrap_or(0);
    let pending_rollback = store
        .pending_rollbacks
        .lock()
        .unwrap()
        .contains_key(task_id);
    let pending_reset_cleanup = store
        .pending_reset_cleanups
        .lock()
        .unwrap()
        .contains(task_id);
    let interrupted_pending_finish = store
        .interrupted_turns
        .lock()
        .unwrap()
        .contains_key(task_id);
    let reset_pending_finish = store.reset_turns.lock().unwrap().contains_key(task_id);
    let phase = if reset_pending_finish {
        "reset_pending_finish"
    } else if interrupted_pending_finish {
        "interrupted_pending_finish"
    } else if running_turn.is_some() && queued_count > 0 {
        "running_and_queued"
    } else if running_turn.is_some() {
        "running"
    } else if queued_count > 0 {
        "queued"
    } else {
        "idle"
    };

    let backend = running_turn.as_ref().map(|turn| turn.backend.clone());
    let turn_id = running_turn.map(|turn| turn.turn_id);
    let process_session_id = store
        .running_process_sessions
        .lock()
        .unwrap()
        .get(task_id)
        .cloned()
        .filter(|process_session_id| super::runner::process_session_is_active(process_session_id));
    let context_usage = latest_context_usage(store, task_id, backend.as_deref());

    ChatRuntimeSnapshot {
        task_id: task_id.to_string(),
        phase: phase.to_string(),
        backend,
        turn_id,
        process_session_id,
        queued_count,
        pending_rollback,
        pending_reset_cleanup,
        context_usage,
        rollback: None,
    }
}

pub(crate) fn chat_runtime_snapshot_with_persisted(
    conn: Option<&Connection>,
    store: &ChatStore,
    task_id: &str,
) -> ChatRuntimeSnapshot {
    let mut snapshot = chat_runtime_snapshot(store, task_id);
    if let Some(conn) = conn {
        if let Ok(count) = count_pending_turns(conn, task_id) {
            snapshot.queued_count += count;
        }
        if !snapshot.pending_rollback {
            if let Ok(rollback) = peek_persisted_pending_rollback(conn, task_id) {
                snapshot.pending_rollback = rollback.is_some();
                snapshot.rollback = rollback;
            }
        }
    }
    snapshot.phase = match snapshot.phase.as_str() {
        "running" if snapshot.queued_count > 0 => "running_and_queued".to_string(),
        "idle" if snapshot.queued_count > 0 => "queued".to_string(),
        _ => snapshot.phase.clone(),
    };
    if snapshot.phase != "idle" && snapshot.phase != "queued" {
        return snapshot;
    }
    let Some(conn) = conn else {
        return snapshot;
    };
    let Ok(Some(persisted)) = load_runtime_state(conn, store, task_id) else {
        if let Ok(Some(abandoned)) = load_any_runtime_state(conn, task_id) {
            if persisted_runtime_state_is_active(&abandoned, store) {
                snapshot.phase = if abandoned.phase == "running" && snapshot.queued_count > 0 {
                    "running_and_queued".to_string()
                } else {
                    abandoned.phase.clone()
                };
                snapshot.process_session_id =
                    abandoned.process_session_id.filter(|process_session_id| {
                        super::runner::process_session_is_active(process_session_id)
                    });
            } else {
                snapshot.phase = "abandoned".to_string();
                snapshot.process_session_id = None;
            }
            snapshot.backend = Some(abandoned.turn.backend);
            snapshot.turn_id = Some(abandoned.turn.turn_id);
            snapshot.context_usage =
                latest_context_usage(store, task_id, snapshot.backend.as_deref());
        }
        return snapshot;
    };
    let queued_count = snapshot.queued_count;
    snapshot.phase = if persisted.phase == "running" && queued_count > 0 {
        "running_and_queued".to_string()
    } else {
        persisted.phase
    };
    snapshot.backend = Some(persisted.turn.backend);
    snapshot.turn_id = Some(persisted.turn.turn_id);
    snapshot.process_session_id = persisted
        .process_session_id
        .filter(|process_session_id| super::runner::process_session_is_active(process_session_id));
    snapshot.context_usage = latest_context_usage(store, task_id, snapshot.backend.as_deref());
    snapshot
}

pub(crate) fn prepare_running_turn_stop(
    store: &ChatStore,
    task_id: &str,
    mark_interrupted: bool,
    mark_reset: bool,
) -> Option<PreparedTurnStop> {
    let running_turn = store.running_turns.lock().unwrap().remove(task_id)?;

    let guide_ids = clear_pending_turns(store, task_id);
    if mark_interrupted {
        store
            .interrupted_turns
            .lock()
            .unwrap()
            .insert(task_id.to_string(), running_turn.clone());
    }
    if mark_reset {
        store
            .reset_turns
            .lock()
            .unwrap()
            .insert(task_id.to_string(), running_turn.clone());
    }

    Some(PreparedTurnStop {
        running_turn,
        guide_ids,
    })
}

pub(crate) fn is_turn_marked_reset(
    store: &ChatStore,
    task_id: &str,
    turn_id: &str,
    backend: &str,
) -> bool {
    store
        .reset_turns
        .lock()
        .unwrap()
        .get(task_id)
        .is_some_and(|turn| turn.turn_id == turn_id && turn.backend == backend)
}

pub(crate) fn take_turn_stop_marks(
    store: &ChatStore,
    task_id: &str,
    turn_id: &str,
    backend: &str,
) -> (bool, bool) {
    let interrupted = store
        .interrupted_turns
        .lock()
        .unwrap()
        .remove(task_id)
        .is_some_and(|turn| turn.turn_id == turn_id && turn.backend == backend);
    let reset = store
        .reset_turns
        .lock()
        .unwrap()
        .remove(task_id)
        .is_some_and(|turn| turn.turn_id == turn_id && turn.backend == backend);
    (interrupted, reset)
}

pub(crate) fn finish_running_turn_handles(
    store: &ChatStore,
    task_id: &str,
    turn_id: &str,
    backend: &str,
) -> FinishedRunningTurn {
    clear_running_handles(store, task_id);
    let (interrupted, reset) = take_turn_stop_marks(store, task_id, turn_id, backend);
    FinishedRunningTurn { interrupted, reset }
}

pub(crate) fn stop_running_turn<R: Runtime>(
    app: &AppHandle<R>,
    store: &ChatStore,
    task_id: &str,
    mark_interrupted: bool,
    mark_reset: bool,
) -> Result<Option<RunningTurn>, String> {
    let Some(prepared) = prepare_running_turn_stop(store, task_id, mark_interrupted, mark_reset)
    else {
        return Ok(None);
    };
    let phase = if mark_reset {
        Some("reset_pending_finish")
    } else if mark_interrupted {
        Some("interrupted_pending_finish")
    } else {
        None
    };
    let uses_legacy_process_runtime = turn_uses_legacy_process_runtime(&prepared.running_turn);
    if uses_legacy_process_runtime {
        if let Some(phase) = phase {
            persist_runtime_state_for_app(
                app,
                store,
                task_id,
                &prepared.running_turn,
                phase,
                None,
                None,
            );
        }
    }
    let mut guide_ids = prepared.guide_ids;
    guide_ids.append(&mut clear_persisted_pending_turns_for_app(app, task_id));
    reset_cleared_guide_queue(app, guide_ids);
    if uses_legacy_process_runtime {
        super::runner::terminate_runner_process_session(store, task_id)?;
    }
    clear_running_handles(store, task_id);
    Ok(Some(prepared.running_turn))
}

pub(crate) fn take_next_pending_turn(
    store: &ChatStore,
    task_id: &str,
    advance_queue: bool,
) -> Option<PendingChatTurn> {
    let mut running = store.running_tasks.lock().unwrap();
    if !advance_queue {
        running.remove(task_id);
        return None;
    }

    let mut pending = store.pending_turns.lock().unwrap();
    let mut should_remove_queue = false;
    let next = if let Some(queue) = pending.get_mut(task_id) {
        let turn = queue.pop_front();
        should_remove_queue = queue.is_empty();
        turn
    } else {
        None
    };
    if should_remove_queue {
        pending.remove(task_id);
    }
    if next.is_none() {
        running.remove(task_id);
    }
    next
}

pub(crate) fn take_next_pending_turn_for_app<R: Runtime>(
    app: &AppHandle<R>,
    store: &ChatStore,
    task_id: &str,
    advance_queue: bool,
) -> Option<PendingChatTurn> {
    if !advance_queue {
        return take_next_pending_turn(store, task_id, false);
    }

    if let Some(lilia_store) = app.try_state::<LiliaStore>() {
        if let Ok(conn) = lilia_store.conn() {
            match take_next_persisted_pending_turn(&conn, task_id) {
                Ok(Some(turn)) => return Some(turn),
                Ok(None) => {}
                Err(err) => eprintln!("[chat-runtime] take persisted pending turn failed: {err}"),
            }
        }
    }

    take_next_pending_turn(store, task_id, true)
}

pub(crate) fn should_emit_runner_exit_error(
    interrupted: bool,
    nonzero: bool,
    stderr_text: &str,
) -> bool {
    !interrupted && nonzero && !stderr_text.trim().is_empty()
}

pub(crate) fn persist_and_emit_interrupted_timeline_event<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: &str,
    backend: &str,
    turn_id: &str,
) {
    let now = now_millis() as i64;
    let message = "用户打断了当前 Agent 运行";
    persist_and_emit_input(
        app_handle,
        AgentTimelineEventInput {
            id: Some(format!("{turn_id}:interrupted")),
            task_id: task_id.to_string(),
            turn_id: Some(turn_id.to_string()),
            backend: backend.to_string(),
            kind: "error".to_string(),
            status: "error".to_string(),
            title: "Agent 已打断".to_string(),
            summary: Some(message.to_string()),
            payload: serde_json::json!({
                "backend": backend,
                "interrupted": true,
                "message": message,
            }),
            created_at: Some(now),
            updated_at: Some(now),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BACKEND_CLAUDE, BACKEND_CODEX};

    #[test]
    fn reasoning_effort_normalization_uses_contract_manifest() {
        let manifest = chat_backends_contract();
        assert_eq!(
            manifest.reasoning_efforts,
            vec![
                "low".to_string(),
                "medium".to_string(),
                "high".to_string(),
                "xhigh".to_string(),
                "max".to_string()
            ]
        );
        assert_eq!(
            manifest.backend_reasoning_efforts.get(BACKEND_CODEX),
            Some(&vec![
                "low".to_string(),
                "medium".to_string(),
                "high".to_string(),
                "xhigh".to_string()
            ])
        );
        assert_eq!(
            normalize_reasoning_effort_for_backend(Some(" xhigh ".to_string()), BACKEND_CODEX)
                .as_deref(),
            Some("xhigh")
        );
        assert_eq!(
            normalize_reasoning_effort_for_backend(Some("max".to_string()), BACKEND_CODEX)
                .as_deref(),
            Some("xhigh")
        );
        assert_eq!(
            normalize_reasoning_effort_for_backend(Some("max".to_string()), BACKEND_CLAUDE)
                .as_deref(),
            Some("max")
        );
    }

    #[test]
    fn model_options_are_loaded_from_contract_manifest() {
        assert_eq!(
            chat_backends(),
            &[BACKEND_CLAUDE.to_string(), BACKEND_CODEX.to_string()]
        );
        assert!(chat_backend_supported(BACKEND_CLAUDE));
        assert!(!chat_backend_supported("unknown"));
        assert_eq!(
            try_normalize_backend(&format!(" {BACKEND_CODEX} ")),
            Some(BACKEND_CODEX)
        );
        assert_eq!(try_normalize_backend("unknown"), None);
        assert_eq!(default_backend(), BACKEND_CLAUDE);
        assert_eq!(normalize_backend("unknown"), default_backend());
        assert_eq!(
            default_model_for_backend(BACKEND_CLAUDE),
            "claude-sonnet-4-6"
        );
        assert_eq!(default_model_for_backend(BACKEND_CODEX), "gpt-5.5");

        let claude_models = model_options_for_backend(BACKEND_CLAUDE);
        assert_eq!(claude_models.len(), 3);
        assert_eq!(claude_models[0].id, "claude-opus-4-7");
        assert_eq!(claude_models[1].label, "Sonnet 4.6");
        assert!(claude_models
            .iter()
            .all(|option| option.backend == BACKEND_CLAUDE));

        let codex_models = model_options_for_backend(BACKEND_CODEX);
        assert_eq!(codex_models[0].id, "gpt-5.5");
        assert_eq!(
            normalize_model_for_backend(" gpt-6-preview ", BACKEND_CODEX),
            "gpt-6-preview"
        );
        assert_eq!(
            normalize_model_for_backend("claude-sonnet-4-6", BACKEND_CODEX),
            "gpt-5.5"
        );
        assert_eq!(
            normalize_model_for_backend("unknown", BACKEND_CODEX),
            "gpt-5.5"
        );
        assert_eq!(
            default_composer("task-1").permission,
            normalize_permission_mode("")
        );
        let mut composer = default_composer("task-1");
        composer.permission = "danger".to_string();
        assert_eq!(
            normalize_composer_for_backend(composer, "task-1", BACKEND_CODEX).permission,
            normalize_permission_mode("")
        );
    }

    #[test]
    fn runtime_snapshot_includes_latest_context_usage() {
        let store = ChatStore::default();
        set_context_usage(
            &store,
            ChatContextUsage {
                task_id: "task-1".to_string(),
                backend: BACKEND_CODEX.to_string(),
                used_tokens: 4096,
                limit_tokens: Some(8192),
                used_percent: Some(50.0),
                source: "runtime".to_string(),
                updated_at: 100,
                unavailable_reason: None,
            },
        );

        let snapshot = chat_runtime_snapshot_with_persisted(None, &store, "task-1");

        let usage = snapshot.context_usage.expect("context usage");
        assert_eq!(usage.task_id, "task-1");
        assert_eq!(usage.backend, BACKEND_CODEX);
        assert_eq!(usage.used_tokens, 4096);
        assert_eq!(usage.limit_tokens, Some(8192));
        assert_eq!(usage.used_percent, Some(50.0));
    }
}

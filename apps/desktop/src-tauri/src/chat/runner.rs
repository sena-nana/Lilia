use std::collections::HashSet;
use std::io::Write;
use std::sync::OnceLock;
use std::thread;

use rusqlite::{params, OptionalExtension};
#[cfg(test)]
use serde::Serialize;
#[cfg(test)]
use serde_json::Map as JsonMap;
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Emitter, Manager, Runtime};

#[cfg(test)]
use crate::agent_events::AgentRuntimeEvent;
use crate::chat::auto_turn_decision::resolve_resume_session_id;
use crate::chat::contract;
use crate::chat::process_registry::JsonlProcessRegistry;
#[cfg(test)]
use crate::chat::state::take_next_pending_turn;
use crate::chat::state::{
    clear_task_runtime_state_for_reset, finish_running_turn_handles, persist_agent_session_id,
    session_key, set_guide_status_for_app, should_persist_user_message,
    take_next_pending_turn_for_app, take_next_recoverable_pending_turn,
    take_pending_finalization_for_app, ChatStore,
};
use crate::chat::timeline_sink::{
    persist_and_emit_error_timeline_event, persist_and_emit_message_timeline_event,
};
#[cfg(test)]
use crate::chat::types::conversation_references_payload;
use crate::chat::types::{
    ChatAttachment, ChatComposerState, ChatConversationReference, ChatRollbackResult,
    ChatRuntimeCommand, ChatWorkflow, DoneEvent, ProviderRuntimeOptions,
};
use crate::chat::workflow::automation_run_id;
#[cfg(test)]
use crate::chat::workflow::workflow_kind;
#[cfg(test)]
use crate::runner_protocol_contract;
use crate::store::LiliaStore;
#[cfg(test)]
use crate::{BACKEND_CLAUDE, BACKEND_CODEX};

pub(crate) struct RunnerInvocation {
    pub(crate) task_id: String,
    pub(crate) content: String,
    pub(crate) composer: ChatComposerState,
    pub(crate) project_cwd: String,
    pub(crate) attachments: Vec<ChatAttachment>,
    pub(crate) conversation_references: Vec<ChatConversationReference>,
    pub(crate) workflow: Option<ChatWorkflow>,
    pub(crate) runtime_command: Option<ChatRuntimeCommand>,
    pub(crate) runtime_options: Option<ProviderRuntimeOptions>,
    pub(crate) turn_id: String,
    pub(crate) resume_session_id: Option<String>,
    pub(crate) queued_count: usize,
}

#[derive(Default)]
pub(crate) struct RunnerOutput {
    pub(crate) last_session_id: Option<String>,
    pub(crate) interrupted: bool,
    pub(crate) reset: bool,
    pub(crate) waiting_approval: bool,
    pub(crate) terminal_failed: bool,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunnerLifecycleEvent {
    pub(crate) stage: &'static str,
    pub(crate) detail: JsonValue,
}

#[cfg(test)]
pub(crate) trait RunnerLifecycleObserver {
    fn record(&mut self, event: RunnerLifecycleEvent);
}

#[cfg(test)]
fn record_runner_lifecycle(
    observer: &mut dyn RunnerLifecycleObserver,
    stage: &'static str,
    detail: JsonValue,
) {
    observer.record(RunnerLifecycleEvent { stage, detail });
}

fn process_registry() -> &'static JsonlProcessRegistry {
    static REGISTRY: OnceLock<JsonlProcessRegistry> = OnceLock::new();
    REGISTRY.get_or_init(JsonlProcessRegistry::new)
}

pub(crate) fn running_process_session_id(store: &ChatStore, task_id: &str) -> Option<String> {
    store
        .running_process_sessions
        .lock()
        .unwrap()
        .get(task_id)
        .cloned()
}

pub(crate) fn write_runner_stdin_payload(
    process_session_id: &str,
    payload: JsonValue,
) -> Result<bool, String> {
    let Some(handle) = process_registry().stdin_handle(process_session_id) else {
        return Ok(false);
    };
    let mut line = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    line.push('\n');
    let mut stdin = handle.lock().map_err(|e| e.to_string())?;
    stdin
        .write_all(line.as_bytes())
        .map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())?;
    Ok(true)
}

pub(crate) fn write_runner_stdin_for_task(
    store: &ChatStore,
    task_id: &str,
    payload: JsonValue,
) -> Result<bool, String> {
    let Some(process_session_id) = running_process_session_id(store, task_id) else {
        return Ok(false);
    };
    write_runner_stdin_payload(&process_session_id, payload)
}

pub(crate) fn terminate_runner_process_session(
    store: &ChatStore,
    task_id: &str,
) -> Result<bool, String> {
    let Some(process_session_id) = running_process_session_id(store, task_id) else {
        return Ok(false);
    };
    process_registry().terminate(&process_session_id)
}

pub(crate) fn process_session_is_active(process_session_id: &str) -> bool {
    process_registry().is_active(process_session_id)
}

#[cfg(test)]
pub(crate) fn start_test_process_session(
    child: std::process::Child,
    initial_payload: &JsonValue,
) -> Result<String, String> {
    process_registry()
        .start(child, initial_payload)
        .map_err(|err| err.to_string())
}

#[cfg(test)]
pub(crate) fn remove_test_process_session(process_session_id: &str) {
    let _ = process_registry().remove(process_session_id);
}

pub(crate) fn spawn_agent_turn<R: Runtime>(
    app: AppHandle<R>,
    task_id: String,
    content: String,
    composer: ChatComposerState,
    project_cwd: String,
    attachments: Vec<ChatAttachment>,
    conversation_references: Vec<ChatConversationReference>,
    workflow: Option<ChatWorkflow>,
    runtime_command: Option<ChatRuntimeCommand>,
    runtime_options: Option<ProviderRuntimeOptions>,
    turn_id: String,
) {
    let backend = composer.backend.clone();
    let resume_session_id = resolve_resume_session_id(&app, &task_id, &backend);

    // Native AgentKit is the only Desktop execution backend.
    let app_handle = app.clone();
    let task_id_for_thread = task_id.clone();
    let invocation = RunnerInvocation {
        task_id,
        content,
        composer,
        project_cwd,
        attachments,
        conversation_references,
        workflow,
        runtime_command,
        runtime_options,
        turn_id,
        resume_session_id,
        queued_count: 0,
    };

    thread::spawn(move || {
        let queued_count = {
            let store = app_handle.state::<ChatStore>();
            let in_memory = store
                .pending_turns
                .lock()
                .unwrap()
                .get(&task_id_for_thread)
                .map(|q| q.len())
                .unwrap_or(0);
            let persisted = app_handle
                .try_state::<LiliaStore>()
                .and_then(|store| store.conn().ok())
                .and_then(|conn| {
                    crate::chat::state::count_pending_turns(&conn, &task_id_for_thread).ok()
                })
                .unwrap_or(0);
            in_memory + persisted
        };
        let mut invocation = invocation;
        invocation.queued_count = queued_count;

        let turn_id_for_finish = invocation.turn_id.clone();
        let automation_run_id_for_finish = automation_run_id(invocation.workflow.as_ref());
        let finish_backend = crate::native_agent::BACKEND_NATIVE_AGENTKIT.to_string();
        let result = crate::native_agent::run_native_agent_turn(&app_handle, invocation);
        let mut runner_ok = true;
        let mut output = match result {
            Ok(output) => output,
            Err(err) => {
                runner_ok = false;
                persist_and_emit_error_timeline_event(
                    &app_handle,
                    &task_id_for_thread,
                    &finish_backend,
                    Some(&turn_id_for_finish),
                    err,
                );
                RunnerOutput::default()
            }
        };
        if output.waiting_approval {
            return;
        }
        let finished = {
            let store = app_handle.state::<ChatStore>();
            finish_running_turn_handles(
                &store,
                &task_id_for_thread,
                &turn_id_for_finish,
                &finish_backend,
            )
        };
        output.interrupted |= finished.interrupted;
        output.reset |= finished.reset;
        let agent_success =
            runner_ok && !output.interrupted && !output.reset && !output.terminal_failed;
        {
            let store = app_handle.state::<ChatStore>();
            crate::automation::automation_complete_agent_turn(
                &app_handle,
                &store,
                automation_run_id_for_finish,
                &turn_id_for_finish,
                agent_success,
            );
        }
        finish_agent_turn(
            app_handle,
            task_id_for_thread,
            finish_backend,
            output.last_session_id,
            agent_success,
            None,
        );
    });
}

pub(crate) fn resume_or_dispatch_persisted_pending_turn<R: Runtime>(
    app: AppHandle<R>,
    task_id: String,
) -> Result<bool, String> {
    let store = app.state::<ChatStore>();
    if store.running_tasks.lock().unwrap().contains_key(&task_id) {
        return Ok(false);
    }
    let Some(lilia_store) = app.try_state::<LiliaStore>() else {
        return Ok(false);
    };
    let conn = lilia_store.conn()?;
    if let Err(err) = ensure_task_ready_for_agent_turn_with_conn(&conn, &task_id) {
        eprintln!(
            "[chat-runtime] skip persisted queued turn for blocked task {}: {}",
            task_id, err
        );
        return Ok(false);
    }
    let Some(turn) = take_next_recoverable_pending_turn(&conn, &store, &task_id)? else {
        return Ok(false);
    };
    store
        .running_tasks
        .lock()
        .unwrap()
        .insert(task_id.clone(), true);
    if let Err(err) = set_guide_status_for_app(&app, turn.guide_id.as_deref(), "sent") {
        eprintln!("[todo-guides] mark recovered queued guide sent failed: {err}");
    }
    if should_persist_user_message(&turn.content, &turn.workflow, &turn.runtime_command) {
        persist_and_emit_message_timeline_event(
            &app,
            &turn.message,
            &turn.composer.backend,
            &turn.turn_id,
            false,
            automation_run_id(turn.workflow.as_ref()).as_deref(),
        );
    }
    spawn_agent_turn(
        app,
        task_id,
        turn.content,
        turn.composer,
        turn.project_cwd,
        turn.attachments,
        turn.conversation_references,
        turn.workflow,
        turn.runtime_command,
        turn.runtime_options,
        turn.turn_id,
    );
    Ok(true)
}

pub(crate) fn ensure_task_ready_for_agent_turn<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
) -> Result<(), String> {
    let Some(lilia_store) = app.try_state::<LiliaStore>() else {
        return Ok(());
    };
    let conn = lilia_store.conn()?;
    ensure_task_ready_for_agent_turn_with_conn(&conn, task_id)
}

#[cfg(test)]
fn runner_event_kind(event: &AgentRuntimeEvent) -> &'static str {
    event.event_type()
}

#[cfg(test)]
pub(crate) fn build_runner_stdin_payload<T: Serialize>(
    backend: &str,
    project_cwd: &str,
    prompt: &str,
    attachments: &[ChatAttachment],
    conversation_references: &[ChatConversationReference],
    workflow: Option<&ChatWorkflow>,
    runtime_command: Option<&ChatRuntimeCommand>,
    runtime_options: Option<&JsonValue>,
    composer: &ChatComposerState,
    resume_session_id: Option<&str>,
    extensions: &T,
) -> JsonValue {
    let turn_keys = runner_protocol_contract::runner_stdin_turn_keys();
    let payload_keys = runner_protocol_contract::runner_stdin_payload_keys();

    let mut turn = JsonMap::new();
    turn.insert(turn_keys.cwd.clone(), serde_json::json!(project_cwd));
    turn.insert(turn_keys.prompt.clone(), serde_json::json!(prompt));
    turn.insert(
        turn_keys.attachments.clone(),
        serde_json::json!(attachments),
    );
    turn.insert(
        turn_keys.conversation_references.clone(),
        serde_json::json!(conversation_references_payload(conversation_references)),
    );
    turn.insert(turn_keys.model.clone(), serde_json::json!(composer.model));
    turn.insert(
        turn_keys.resume_session_id.clone(),
        serde_json::json!(resume_session_id),
    );
    turn.insert(
        turn_keys.plan_mode.clone(),
        serde_json::json!(composer.plan_mode),
    );
    turn.insert(
        turn_keys.goal_mode.clone(),
        serde_json::json!(composer.goal_mode),
    );
    turn.insert(
        turn_keys.permission.clone(),
        serde_json::json!(composer.permission),
    );

    let mut payload = JsonMap::new();
    payload.insert(payload_keys.backend.clone(), serde_json::json!(backend));
    payload.insert(payload_keys.turn.clone(), JsonValue::Object(turn));
    payload.insert(payload_keys.workflow.clone(), serde_json::json!(workflow));
    payload.insert(
        payload_keys.runtime_command.clone(),
        serde_json::json!(runtime_command),
    );
    payload.insert(
        payload_keys.runtime_options.clone(),
        serde_json::json!(runtime_options),
    );
    payload.insert(
        payload_keys.extensions.clone(),
        serde_json::json!(extensions),
    );
    JsonValue::Object(payload)
}

#[cfg(feature = "runtime-domain-reference")]
pub(crate) fn runtime_reference_agent_payload(payload: &JsonValue) -> Result<JsonValue, String> {
    let task_id = payload
        .get("taskId")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "reference Agent payload 缺少 taskId".to_string())?;
    let profile_id = payload
        .get("profileId")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "reference Agent payload 缺少 profileId".to_string())?;
    let project_cwd = payload
        .get("cwd")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "reference Agent payload 缺少 cwd".to_string())?;
    let prompt = payload
        .get("prompt")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "reference Agent payload 缺少 prompt".to_string())?;
    let iterations = payload
        .get("iterations")
        .and_then(JsonValue::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1);
    if iterations == 0 {
        return Err("reference Agent iterations 必须大于零".to_string());
    }
    let mut request = mutsuki_agent_contracts::AgentRunRequest::new(
        profile_id,
        vec![mutsuki_agent_contracts::AgentMessage::user(prompt)],
    );
    request.session_id = payload
        .get("sessionId")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    request.model = payload
        .get("model")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    request.stream = true;
    request.metadata = Some(serde_json::json!({
        "taskId": task_id,
        "workspace": project_cwd,
        "source": "lilia.runtime-domain-reference",
    }));
    let mut output = JsonValue::Null;
    for _ in 0..iterations {
        output = serde_json::to_value(&request)
            .map_err(|error| format!("serialize AgentKit AgentRunRequest: {error}"))?;
    }
    Ok(output)
}

pub(crate) fn finish_agent_turn<R: Runtime>(
    app_handle: AppHandle<R>,
    task_id: String,
    backend: String,
    last_session_id: Option<String>,
    advance_queue: bool,
    rollback: Option<ChatRollbackResult>,
) {
    // 记下 session id 供下一轮 resume。
    if let Some(sid) = last_session_id.clone() {
        let store = app_handle.state::<ChatStore>();
        store
            .sdk_sessions
            .lock()
            .unwrap()
            .insert(session_key(&backend, &task_id), sid.clone());
        if let Some(store) = app_handle.try_state::<LiliaStore>() {
            match store
                .conn()
                .and_then(|conn| persist_agent_session_id(&conn, &task_id, &backend, &sid))
            {
                Ok(()) => {}
                Err(err) => eprintln!("[agent-session] persist checkpoint failed: {err}"),
            }
        }
    }

    let (pending_rollback, pending_reset_cleanup) = {
        let store = app_handle.state::<ChatStore>();
        take_pending_finalization_for_app(&app_handle, &store, &task_id)
    };
    let completion = build_turn_completion(
        task_id.clone(),
        last_session_id.clone(),
        rollback,
        pending_rollback,
        pending_reset_cleanup,
    );

    let _ = app_handle.emit(contract::done_event_name(), completion.done_event);

    if completion.reset_cleanup_requested {
        let store = app_handle.state::<ChatStore>();
        clear_task_runtime_state_for_reset(&app_handle, &store, &task_id);
    }

    let next_dispatch = {
        let store = app_handle.state::<ChatStore>();
        plan_next_turn_dispatch(&app_handle, &store, &task_id, advance_queue)
    };
    if let Some(turn) = next_dispatch.next_turn {
        if let Err(err) = set_guide_status_for_app(&app_handle, turn.guide_id.as_deref(), "sent") {
            eprintln!("[todo-guides] mark queued guide sent failed: {err}");
        }
        if should_persist_user_message(&turn.content, &turn.workflow, &turn.runtime_command) {
            persist_and_emit_message_timeline_event(
                &app_handle,
                &turn.message,
                &turn.composer.backend,
                &turn.turn_id,
                false,
                None,
            );
        }
        spawn_agent_turn(
            app_handle,
            task_id,
            turn.content,
            turn.composer,
            turn.project_cwd,
            turn.attachments,
            turn.conversation_references,
            turn.workflow,
            turn.runtime_command,
            turn.runtime_options,
            turn.turn_id,
        );
    }
}

#[derive(Debug)]
struct TurnCompletion {
    done_event: DoneEvent,
    reset_cleanup_requested: bool,
}

struct NextTurnDispatch {
    next_turn: Option<crate::chat::state::PendingChatTurn>,
}

fn build_turn_completion(
    task_id: String,
    session_id: Option<String>,
    explicit_rollback: Option<ChatRollbackResult>,
    pending_rollback: Option<ChatRollbackResult>,
    pending_reset_cleanup: bool,
) -> TurnCompletion {
    TurnCompletion {
        done_event: DoneEvent {
            task_id,
            session_id,
            subtype: None,
            rollback: explicit_rollback.or(pending_rollback),
        },
        reset_cleanup_requested: pending_reset_cleanup,
    }
}

fn plan_next_turn_dispatch<R: Runtime>(
    app: &AppHandle<R>,
    store: &ChatStore,
    task_id: &str,
    advance_queue: bool,
) -> NextTurnDispatch {
    NextTurnDispatch {
        next_turn: take_next_pending_turn_for_app(app, store, task_id, advance_queue),
    }
}

const CONVERSATION_CONTEXT_TASK_LIMIT: i64 = 24;
const CONVERSATION_CONTEXT_MESSAGE_LIMIT: i64 = 24;
const CONVERSATION_CONTEXT_TEXT_LIMIT: usize = 2_000;
#[cfg(test)]
const DEPENDENCY_CONTEXT_MESSAGE_SCAN_LIMIT: i64 = 64;

struct DependencyTaskRow {
    id: String,
    title: String,
    status: String,
}

#[cfg(test)]
struct DependencyContextItem {
    task_id: String,
    title: String,
    status: String,
    summary: String,
}

fn task_status_label(status: &str) -> &str {
    match status {
        "draft" => "草稿",
        "waiting" => "等待中",
        "running" => "运行中",
        "blocked" => "阻塞",
        "done" => "完成",
        "cancelled" => "已取消",
        _ => status,
    }
}

fn load_task_run_gate_row(
    conn: &rusqlite::Connection,
    task_id: &str,
) -> Result<Option<DependencyTaskRow>, String> {
    conn.query_row(
        r#"SELECT id, title, status
           FROM tasks
           WHERE id = ?1 AND archived = 0"#,
        params![task_id],
        |row| {
            Ok(DependencyTaskRow {
                id: row.get(0)?,
                title: row.get(1)?,
                status: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("task run gate: 查询任务失败：{e}"))
}

fn ensure_task_ready_for_agent_turn_with_conn(
    conn: &rusqlite::Connection,
    task_id: &str,
) -> Result<(), String> {
    let Some(task) = load_task_run_gate_row(conn, task_id)? else {
        return Ok(());
    };
    if task.status == "blocked" {
        return Err(format!("任务已标记为阻塞，暂不能启动会话：{}", task.title));
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    ensure_dependency_chain_done(conn, &task.id, &mut visiting, &mut visited)
}

fn ensure_dependency_chain_done(
    conn: &rusqlite::Connection,
    task_id: &str,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) -> Result<(), String> {
    if visited.contains(task_id) {
        return Ok(());
    }
    if !visiting.insert(task_id.to_string()) {
        return Err("任务依赖存在循环，暂不能启动会话".to_string());
    }

    for dependency in load_dependency_tasks(conn, task_id)? {
        if visiting.contains(&dependency.id) {
            return Err("任务依赖存在循环，暂不能启动会话".to_string());
        }
        if dependency.status != "done" {
            return Err(format!(
                "任务依赖未完成，暂不能启动会话：{}（{}）",
                dependency.title,
                task_status_label(&dependency.status)
            ));
        }
        ensure_dependency_chain_done(conn, &dependency.id, visiting, visited)?;
    }

    visiting.remove(task_id);
    visited.insert(task_id.to_string());
    Ok(())
}

#[cfg(test)]
fn append_main_agent_prompt_to_runtime_options(
    backend: &str,
    runtime_options: Option<JsonValue>,
    mode: &str,
    custom_prompt: &str,
) -> Option<JsonValue> {
    crate::memory::append_context_to_runtime_options(
        backend,
        runtime_options,
        &crate::prompt_contract::build_main_agent_prompt(mode, Some(custom_prompt)),
    )
}

#[cfg(test)]
fn build_dependency_context_core(
    conn: &rusqlite::Connection,
    task_id: &str,
) -> Result<Option<String>, String> {
    let mut items = Vec::new();
    for task in load_dependency_tasks(conn, task_id)? {
        let Some(summary) = load_dependency_final_summary(conn, &task.id)? else {
            continue;
        };
        items.push(DependencyContextItem {
            task_id: task.id,
            title: task.title,
            status: task.status,
            summary,
        });
    }
    Ok(format_dependency_context(&items))
}

#[cfg(test)]
mod main_agent_prompt_tests {
    use super::*;

    #[test]
    fn main_agent_prompt_appends_to_existing_codex_context() {
        let value = append_main_agent_prompt_to_runtime_options(
            BACKEND_CODEX,
            Some(serde_json::json!({
                "provider": {
                    "codex": {
                        "additionalContext": "existing context"
                    }
                }
            })),
            "aggressive",
            "",
        )
        .unwrap();
        let context = value["provider"]["codex"]["additionalContext"]
            .as_str()
            .unwrap();
        let aggressive = crate::prompt_contract::main_agent_prompt_mode("aggressive");
        let first_workflow_title = crate::prompt_contract::main_agent_prompts()
            .workflow_order
            .first()
            .and_then(|key| {
                crate::prompt_contract::main_agent_prompts()
                    .workflow_types
                    .get(key)
            })
            .map(|workflow| workflow.title.as_str())
            .unwrap();

        assert!(context.starts_with("existing context\n\n"));
        assert!(context.contains(aggressive));
        assert!(context.contains(first_workflow_title));
    }

    #[test]
    fn main_agent_prompt_unknown_mode_uses_conservative_context() {
        let value =
            append_main_agent_prompt_to_runtime_options(BACKEND_CLAUDE, None, "unknown", "")
                .unwrap();
        let context = value["provider"]["claude"]["additionalContext"]
            .as_str()
            .unwrap();
        let conservative = crate::prompt_contract::main_agent_prompt_mode("conservative");
        let aggressive = crate::prompt_contract::main_agent_prompt_mode("aggressive");

        assert!(context.contains(conservative));
        assert!(!context.contains(aggressive));
    }

    #[test]
    fn main_agent_prompt_custom_mode_uses_custom_strategy_context() {
        let value = append_main_agent_prompt_to_runtime_options(
            BACKEND_CODEX,
            None,
            "custom",
            "Custom strategy for this workspace.",
        )
        .unwrap();
        let context = value["provider"]["codex"]["additionalContext"]
            .as_str()
            .unwrap();
        let first_workflow_title = crate::prompt_contract::main_agent_prompts()
            .workflow_order
            .first()
            .and_then(|key| {
                crate::prompt_contract::main_agent_prompts()
                    .workflow_types
                    .get(key)
            })
            .map(|workflow| workflow.title.as_str())
            .unwrap();

        assert!(context.contains("Custom strategy for this workspace."));
        assert!(
            !context.contains(crate::prompt_contract::main_agent_prompt_mode(
                "conservative"
            ))
        );
        assert!(context.contains(first_workflow_title));
    }
}

fn load_dependency_tasks(
    conn: &rusqlite::Connection,
    task_id: &str,
) -> Result<Vec<DependencyTaskRow>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT t.id, t.title, t.status
               FROM task_dependencies d
               INNER JOIN tasks t ON t.id = d.depends_on_id
               WHERE d.task_id = ?1 AND t.archived = 0
               ORDER BY t.created_at ASC, t.id ASC"#,
        )
        .map_err(|e| format!("dependency context: prepare dependencies failed: {e}"))?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            Ok(DependencyTaskRow {
                id: row.get(0)?,
                title: row.get(1)?,
                status: row.get(2)?,
            })
        })
        .map_err(|e| format!("dependency context: query dependencies failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("dependency context: dependency row failed: {e}"))?);
    }
    Ok(out)
}

#[cfg(test)]
fn load_dependency_final_summary(
    conn: &rusqlite::Connection,
    task_id: &str,
) -> Result<Option<String>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT summary, payload
               FROM agent_timeline_events
               WHERE task_id = ?1 AND kind = 'message' AND status = 'success'
               ORDER BY turn_seq DESC, intra_turn_order DESC, created_at DESC
               LIMIT ?2"#,
        )
        .map_err(|e| format!("dependency context: prepare messages failed: {e}"))?;
    let rows = stmt
        .query_map(
            params![task_id, DEPENDENCY_CONTEXT_MESSAGE_SCAN_LIMIT],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|e| format!("dependency context: query messages failed: {e}"))?;
    for row in rows {
        let (summary, payload_text) =
            row.map_err(|e| format!("dependency context: message row failed: {e}"))?;
        let payload = serde_json::from_str::<JsonValue>(&payload_text).unwrap_or(JsonValue::Null);
        let role = payload
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("assistant");
        if role != "assistant" {
            continue;
        }
        let content = payload
            .get("content")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .or_else(|| {
                summary
                    .as_deref()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
            });
        let Some(content) = content else {
            continue;
        };
        return Ok(Some(clip_context_text(content)));
    }
    Ok(None)
}

#[cfg(test)]
fn format_dependency_context(items: &[DependencyContextItem]) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    let mut lines = vec![
        "[Lilia Dependency Context]".to_string(),
        "These are final summaries from user-declared prerequisite tasks. Treat them as default context for this new session; inspect the full conversations if you need details.".to_string(),
        String::new(),
    ];
    for item in items {
        lines.push(format!(
            "- {} (status: {}, task: {}): {}",
            item.title.trim(),
            item.status.trim(),
            item.task_id,
            compact_context_line(&item.summary)
        ));
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
fn compact_context_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn build_runner_conversation_context<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
) -> Option<JsonValue> {
    let store = app.try_state::<LiliaStore>()?;
    let conn = store.conn().ok()?;
    let current = load_context_task_row(&conn, task_id).ok().flatten()?;
    let project_id = current.project_id.clone();
    if project_id.is_none() && current.parent_id.is_none() {
        return None;
    }
    let parent_task_id = current.parent_id.clone();
    let mut tasks = Vec::new();
    let mut seen = std::collections::HashSet::new();

    seen.insert(task_id.to_string());
    if let Ok(task) = context_task_json(&conn, current, true) {
        tasks.push(task);
    }

    if let Some(parent_task_id) = parent_task_id.as_deref() {
        if seen.insert(parent_task_id.to_string()) {
            if let Ok(Some(task)) = load_context_task(&conn, parent_task_id, true) {
                tasks.push(task);
            }
        }
    }

    if let Ok(mut related) = load_related_context_tasks(
        &conn,
        project_id.as_deref(),
        CONVERSATION_CONTEXT_TASK_LIMIT,
    ) {
        for task in related.drain(..) {
            let Some(id) = task.get("taskId").and_then(|value| value.as_str()) else {
                continue;
            };
            if seen.insert(id.to_string()) {
                tasks.push(task);
            }
        }
    }

    Some(serde_json::json!({
        "currentTaskId": task_id,
        "parentTaskId": parent_task_id,
        "projectId": project_id,
        "tasks": tasks,
    }))
}

struct ContextTaskRow {
    id: String,
    project_id: Option<String>,
    title: String,
    status: String,
    created_at: i64,
    parent_id: Option<String>,
}

fn load_context_task_row(
    conn: &rusqlite::Connection,
    task_id: &str,
) -> Result<Option<ContextTaskRow>, String> {
    conn.query_row(
        r#"SELECT id, project_id, title, status, created_at, parent_id
           FROM tasks
           WHERE id = ?1 AND archived = 0"#,
        params![task_id],
        |row| {
            Ok(ContextTaskRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                title: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
                parent_id: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("conversation context: 查询任务失败：{e}"))
}

fn load_context_task(
    conn: &rusqlite::Connection,
    task_id: &str,
    include_messages: bool,
) -> Result<Option<JsonValue>, String> {
    let Some(row) = load_context_task_row(conn, task_id)? else {
        return Ok(None);
    };
    Ok(Some(context_task_json(conn, row, include_messages)?))
}

fn load_related_context_tasks(
    conn: &rusqlite::Connection,
    project_id: Option<&str>,
    limit: i64,
) -> Result<Vec<JsonValue>, String> {
    let rows = if let Some(project_id) = project_id {
        let mut stmt = conn
            .prepare(
                r#"SELECT id, project_id, title, status, created_at, parent_id
                   FROM tasks
                   WHERE project_id = ?1 AND archived = 0
                   ORDER BY pinned DESC, sort_order ASC
                   LIMIT ?2"#,
            )
            .map_err(|e| format!("conversation context: prepare 失败：{e}"))?;
        let mapped = stmt
            .query_map(params![project_id, limit], context_task_row_from_sql)
            .map_err(|e| format!("conversation context: query 失败：{e}"))?;
        collect_context_task_rows(mapped)?
    } else {
        let mut stmt = conn
            .prepare(
                r#"SELECT id, project_id, title, status, created_at, parent_id
                   FROM tasks
                   WHERE project_id IS NULL AND archived = 0
                   ORDER BY pinned DESC, sort_order ASC
                   LIMIT ?1"#,
            )
            .map_err(|e| format!("conversation context: prepare 失败：{e}"))?;
        let mapped = stmt
            .query_map(params![limit], context_task_row_from_sql)
            .map_err(|e| format!("conversation context: query 失败：{e}"))?;
        collect_context_task_rows(mapped)?
    };

    rows.into_iter()
        .map(|row| context_task_json(conn, row, false))
        .collect()
}

fn context_task_row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContextTaskRow> {
    Ok(ContextTaskRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
        parent_id: row.get(5)?,
    })
}

fn collect_context_task_rows<I>(rows: I) -> Result<Vec<ContextTaskRow>, String>
where
    I: IntoIterator<Item = rusqlite::Result<ContextTaskRow>>,
{
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("conversation context: row 失败：{e}"))?);
    }
    Ok(out)
}

fn context_task_json(
    conn: &rusqlite::Connection,
    task: ContextTaskRow,
    include_messages: bool,
) -> Result<JsonValue, String> {
    let messages = if include_messages {
        load_context_messages(conn, &task.id)?
    } else {
        Vec::new()
    };
    let truncated = include_messages && messages.len() as i64 >= CONVERSATION_CONTEXT_MESSAGE_LIMIT;
    Ok(serde_json::json!({
        "taskId": task.id,
        "projectId": task.project_id,
        "title": task.title,
        "status": task.status,
        "createdAt": task.created_at,
        "parentId": task.parent_id,
        "messages": messages,
        "truncated": truncated,
    }))
}

fn load_context_messages(
    conn: &rusqlite::Connection,
    task_id: &str,
) -> Result<Vec<JsonValue>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT summary, payload, created_at
               FROM agent_timeline_events
               WHERE task_id = ?1 AND kind = 'message'
               ORDER BY turn_seq ASC, intra_turn_order ASC, created_at ASC
               LIMIT ?2"#,
        )
        .map_err(|e| format!("conversation context: prepare messages 失败：{e}"))?;
    let rows = stmt
        .query_map(
            params![task_id, CONVERSATION_CONTEXT_MESSAGE_LIMIT],
            |row| {
                let summary: Option<String> = row.get(0)?;
                let payload_text: String = row.get(1)?;
                let created_at: i64 = row.get(2)?;
                Ok((summary, payload_text, created_at))
            },
        )
        .map_err(|e| format!("conversation context: query messages 失败：{e}"))?;

    let mut out = Vec::new();
    for row in rows {
        let (summary, payload_text, created_at) =
            row.map_err(|e| format!("conversation context: message row 失败：{e}"))?;
        let payload = serde_json::from_str::<JsonValue>(&payload_text).unwrap_or(JsonValue::Null);
        let role = payload
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("assistant");
        let content = payload
            .get("content")
            .and_then(|value| value.as_str())
            .or(summary.as_deref())
            .unwrap_or("");
        if content.trim().is_empty() {
            continue;
        }
        out.push(serde_json::json!({
            "role": role,
            "content": clip_context_text(content),
            "createdAt": created_at,
        }));
    }
    Ok(out)
}

fn clip_context_text(text: &str) -> String {
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= CONVERSATION_CONTEXT_TEXT_LIMIT {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_timeline;

    #[derive(Default)]
    struct CollectingLifecycleObserver {
        events: Vec<RunnerLifecycleEvent>,
    }

    impl RunnerLifecycleObserver for CollectingLifecycleObserver {
        fn record(&mut self, event: RunnerLifecycleEvent) {
            self.events.push(event);
        }
    }

    fn create_dependency_context_schema(conn: &rusqlite::Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE tasks (
              id TEXT PRIMARY KEY,
              title TEXT NOT NULL,
              status TEXT NOT NULL,
              created_at INTEGER NOT NULL,
              archived INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE task_dependencies (
              task_id TEXT NOT NULL,
              depends_on_id TEXT NOT NULL,
              PRIMARY KEY (task_id, depends_on_id)
            );
            "#,
        )
        .unwrap();
        agent_timeline::create_timeline_schema(conn).unwrap();
    }

    fn insert_dependency_task(
        conn: &rusqlite::Connection,
        id: &str,
        title: &str,
        status: &str,
        created_at: i64,
        archived: bool,
    ) {
        conn.execute(
            r#"INSERT INTO tasks (id, title, status, created_at, archived)
               VALUES (?1, ?2, ?3, ?4, ?5)"#,
            params![id, title, status, created_at, if archived { 1 } else { 0 }],
        )
        .unwrap();
    }

    fn insert_dependency_link(conn: &rusqlite::Connection, task_id: &str, depends_on_id: &str) {
        conn.execute(
            "INSERT INTO task_dependencies (task_id, depends_on_id) VALUES (?1, ?2)",
            params![task_id, depends_on_id],
        )
        .unwrap();
    }

    fn insert_dependency_message(
        conn: &rusqlite::Connection,
        task_id: &str,
        id: &str,
        role: &str,
        content: &str,
        summary: Option<&str>,
        turn_seq: i64,
        order: i64,
    ) {
        conn.execute(
            r#"INSERT INTO agent_timeline_events
               (id, task_id, turn_id, backend, kind, status, title, summary, payload,
                created_at, updated_at, turn_seq, intra_turn_order)
               VALUES (?1, ?2, 'turn-1', 'codex', 'message', 'success', 'Message', ?3, ?4,
                       ?5, ?5, ?6, ?7)"#,
            params![
                id,
                task_id,
                summary,
                serde_json::json!({ "role": role, "content": content }).to_string(),
                1_000 + turn_seq,
                turn_seq,
                order
            ],
        )
        .unwrap();
    }

    #[test]
    fn dependency_context_uses_latest_successful_assistant_summary() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_dependency_context_schema(&conn);
        insert_dependency_task(&conn, "task-1", "Current", "running", 1, false);
        insert_dependency_task(&conn, "dep-1", "Design pass", "done", 2, false);
        insert_dependency_link(&conn, "task-1", "dep-1");
        insert_dependency_message(
            &conn,
            "dep-1",
            "dep-1-old",
            "assistant",
            "Old conclusion",
            None,
            1,
            1,
        );
        insert_dependency_message(
            &conn,
            "dep-1",
            "dep-1-final",
            "assistant",
            "Final conclusion\nwith detail",
            None,
            2,
            1,
        );
        insert_dependency_message(
            &conn,
            "dep-1",
            "dep-1-user",
            "user",
            "User follow-up should not be injected",
            None,
            3,
            1,
        );

        let context = build_dependency_context_core(&conn, "task-1")
            .unwrap()
            .expect("dependency context");

        assert!(context.contains("[Lilia Dependency Context]"));
        assert!(context.contains("Design pass"));
        assert!(context.contains("status: done"));
        assert!(context.contains("task: dep-1"));
        assert!(context.contains("Final conclusion with detail"));
        assert!(!context.contains("Old conclusion"));
        assert!(!context.contains("User follow-up"));
    }

    #[test]
    fn dependency_context_skips_archived_and_empty_dependencies() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_dependency_context_schema(&conn);
        insert_dependency_task(&conn, "task-1", "Current", "running", 1, false);
        insert_dependency_task(&conn, "archived", "Archived", "done", 2, true);
        insert_dependency_task(&conn, "empty", "Empty", "done", 3, false);
        insert_dependency_link(&conn, "task-1", "archived");
        insert_dependency_link(&conn, "task-1", "empty");
        insert_dependency_message(
            &conn,
            "archived",
            "archived-final",
            "assistant",
            "Archived summary",
            None,
            1,
            1,
        );
        insert_dependency_message(
            &conn,
            "empty",
            "empty-final",
            "assistant",
            "   ",
            None,
            1,
            1,
        );

        let context = build_dependency_context_core(&conn, "task-1").unwrap();

        assert!(context.is_none());
    }

    #[test]
    fn dependency_context_falls_back_to_timeline_summary() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_dependency_context_schema(&conn);
        insert_dependency_task(&conn, "task-1", "Current", "running", 1, false);
        insert_dependency_task(&conn, "dep-1", "Summary fallback", "done", 2, false);
        insert_dependency_link(&conn, "task-1", "dep-1");
        insert_dependency_message(
            &conn,
            "dep-1",
            "dep-1-final",
            "assistant",
            "",
            Some("Summary fallback text"),
            1,
            1,
        );

        let context = build_dependency_context_core(&conn, "task-1")
            .unwrap()
            .expect("dependency context");

        assert!(context.contains("Summary fallback text"));
    }

    #[test]
    fn task_run_gate_rejects_blocked_current_task() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_dependency_context_schema(&conn);
        insert_dependency_task(&conn, "task-1", "Blocked task", "blocked", 1, false);

        let err = ensure_task_ready_for_agent_turn_with_conn(&conn, "task-1").unwrap_err();

        assert!(err.contains("任务已标记为阻塞"));
        assert!(err.contains("Blocked task"));
    }

    #[test]
    fn task_run_gate_rejects_direct_unfinished_dependency() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_dependency_context_schema(&conn);
        insert_dependency_task(&conn, "task-1", "Current", "waiting", 1, false);
        insert_dependency_task(&conn, "dep-1", "Design pass", "waiting", 2, false);
        insert_dependency_link(&conn, "task-1", "dep-1");

        let err = ensure_task_ready_for_agent_turn_with_conn(&conn, "task-1").unwrap_err();

        assert!(err.contains("任务依赖未完成"));
        assert!(err.contains("Design pass"));
        assert!(err.contains("等待中"));
    }

    #[test]
    fn task_run_gate_rejects_transitive_unfinished_dependency() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_dependency_context_schema(&conn);
        insert_dependency_task(&conn, "task-1", "Current", "waiting", 1, false);
        insert_dependency_task(&conn, "dep-1", "Implementation", "done", 2, false);
        insert_dependency_task(&conn, "dep-2", "Design pass", "running", 3, false);
        insert_dependency_link(&conn, "task-1", "dep-1");
        insert_dependency_link(&conn, "dep-1", "dep-2");

        let err = ensure_task_ready_for_agent_turn_with_conn(&conn, "task-1").unwrap_err();

        assert!(err.contains("Design pass"));
        assert!(err.contains("运行中"));
    }

    #[test]
    fn task_run_gate_allows_completed_dependency_chain() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_dependency_context_schema(&conn);
        insert_dependency_task(&conn, "task-1", "Current", "running", 1, false);
        insert_dependency_task(&conn, "dep-1", "Implementation", "done", 2, false);
        insert_dependency_task(&conn, "dep-2", "Design pass", "done", 3, false);
        insert_dependency_link(&conn, "task-1", "dep-1");
        insert_dependency_link(&conn, "dep-1", "dep-2");

        ensure_task_ready_for_agent_turn_with_conn(&conn, "task-1").unwrap();
    }

    #[test]
    fn task_run_gate_rejects_dependency_cycles() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_dependency_context_schema(&conn);
        insert_dependency_task(&conn, "task-1", "Current", "waiting", 1, false);
        insert_dependency_task(&conn, "dep-1", "Dependency", "done", 2, false);
        insert_dependency_link(&conn, "task-1", "dep-1");
        insert_dependency_link(&conn, "dep-1", "task-1");

        let err = ensure_task_ready_for_agent_turn_with_conn(&conn, "task-1").unwrap_err();

        assert!(err.contains("依赖存在循环"));
    }

    #[test]
    fn runner_lifecycle_observer_records_stable_stage_and_detail() {
        let mut observer = CollectingLifecycleObserver::default();

        record_runner_lifecycle(
            &mut observer,
            "process_spawned",
            serde_json::json!({ "pid": 42 }),
        );

        assert_eq!(observer.events.len(), 1);
        assert_eq!(observer.events[0].stage, "process_spawned");
        assert_eq!(observer.events[0].detail["pid"], serde_json::json!(42));
    }

    #[test]
    fn runner_lifecycle_classifies_workflow_and_runtime_events() {
        let workflow = ChatWorkflow::LiliaMemoryReset;
        assert_eq!(
            workflow_kind(Some(&workflow)).as_deref(),
            Some("lilia_memory_reset")
        );
        assert_eq!(workflow_kind(None), None);

        assert_eq!(
            runner_event_kind(&AgentRuntimeEvent::InteractionRequest {
                id: "ask-1".to_string(),
                kind: crate::agent_interaction_contract::tool_consent_interaction_kind()
                    .to_string(),
                backend: Some(BACKEND_CODEX.to_string()),
                payload: JsonValue::Null,
            }),
            "interaction_request"
        );
        assert_eq!(
            runner_event_kind(&AgentRuntimeEvent::Done {
                session_id: Some("thread-1".to_string()),
                subtype: None,
            }),
            "done"
        );
        assert_eq!(
            runner_event_kind(&AgentRuntimeEvent::ContextUsage {
                used_tokens: 4096,
                limit_tokens: Some(8192),
                used_percent: Some(50.0),
                source: Some("runtime".to_string()),
                unavailable_reason: None,
            }),
            "context_usage"
        );
    }

    #[test]
    fn turn_completion_prefers_explicit_rollback_over_pending() {
        let explicit = ChatRollbackResult {
            rolled_back: true,
            restored_content: "explicit".to_string(),
            restored_attachments: Vec::new(),
            restored_conversation_references: Vec::new(),
            removed_event_ids: vec!["evt-explicit".to_string()],
        };
        let pending = ChatRollbackResult {
            rolled_back: true,
            restored_content: "pending".to_string(),
            restored_attachments: Vec::new(),
            restored_conversation_references: Vec::new(),
            removed_event_ids: vec!["evt-pending".to_string()],
        };

        let completion = build_turn_completion(
            "task-1".to_string(),
            Some("session-1".to_string()),
            Some(explicit),
            Some(pending),
            true,
        );

        assert_eq!(completion.done_event.task_id, "task-1");
        assert_eq!(
            completion.done_event.session_id.as_deref(),
            Some("session-1")
        );
        let rollback = completion.done_event.rollback.expect("rollback");
        assert_eq!(rollback.restored_content, "explicit");
        assert_eq!(rollback.removed_event_ids, vec!["evt-explicit".to_string()]);
        assert!(completion.reset_cleanup_requested);
    }

    #[test]
    fn turn_completion_falls_back_to_pending_rollback() {
        let pending = ChatRollbackResult {
            rolled_back: true,
            restored_content: "pending".to_string(),
            restored_attachments: Vec::new(),
            restored_conversation_references: Vec::new(),
            removed_event_ids: vec!["evt-pending".to_string()],
        };

        let completion =
            build_turn_completion("task-2".to_string(), None, None, Some(pending), false);

        assert_eq!(completion.done_event.task_id, "task-2");
        assert!(completion.done_event.session_id.is_none());
        let rollback = completion.done_event.rollback.expect("rollback");
        assert_eq!(rollback.restored_content, "pending");
        assert_eq!(rollback.removed_event_ids, vec!["evt-pending".to_string()]);
        assert!(!completion.reset_cleanup_requested);
    }

    #[test]
    fn next_turn_dispatch_advances_queue_when_allowed() {
        let store = ChatStore::default();
        store
            .running_tasks
            .lock()
            .unwrap()
            .insert("task-1".to_string(), true);
        store
            .pending_turns
            .lock()
            .unwrap()
            .entry("task-1".to_string())
            .or_default()
            .push_back(crate::chat::state::PendingChatTurn {
                content: "next".to_string(),
                composer: crate::chat::state::default_composer("task-1"),
                project_cwd: "C:\\repo".to_string(),
                attachments: Vec::new(),
                conversation_references: Vec::new(),
                workflow: None,
                runtime_command: None,
                runtime_options: None,
                message: crate::chat::types::ChatMessage {
                    id: "u-next".to_string(),
                    task_id: "task-1".to_string(),
                    role: "user".to_string(),
                    content: "next".to_string(),
                    attachments: Vec::new(),
                    conversation_references: Vec::new(),
                    created_at: 1,
                },
                turn_id: "turn-next".to_string(),
                guide_id: Some("guide-next".to_string()),
            });

        let dispatch = NextTurnDispatch {
            next_turn: take_next_pending_turn(&store, "task-1", true),
        };

        let turn = dispatch.next_turn.expect("next turn");
        assert_eq!(turn.turn_id, "turn-next");
        assert!(store.pending_turns.lock().unwrap().get("task-1").is_none());
        assert!(store.running_tasks.lock().unwrap().get("task-1").is_some());
    }

    #[test]
    fn next_turn_dispatch_keeps_queue_when_not_advancing() {
        let store = ChatStore::default();
        store
            .running_tasks
            .lock()
            .unwrap()
            .insert("task-1".to_string(), true);
        store
            .pending_turns
            .lock()
            .unwrap()
            .entry("task-1".to_string())
            .or_default()
            .push_back(crate::chat::state::PendingChatTurn {
                content: "queued".to_string(),
                composer: crate::chat::state::default_composer("task-1"),
                project_cwd: "C:\\repo".to_string(),
                attachments: Vec::new(),
                conversation_references: Vec::new(),
                workflow: None,
                runtime_command: None,
                runtime_options: None,
                message: crate::chat::types::ChatMessage {
                    id: "u-queued".to_string(),
                    task_id: "task-1".to_string(),
                    role: "user".to_string(),
                    content: "queued".to_string(),
                    attachments: Vec::new(),
                    conversation_references: Vec::new(),
                    created_at: 1,
                },
                turn_id: "turn-queued".to_string(),
                guide_id: Some("guide-queued".to_string()),
            });

        let dispatch = NextTurnDispatch {
            next_turn: take_next_pending_turn(&store, "task-1", false),
        };

        assert!(dispatch.next_turn.is_none());
        assert_eq!(
            store
                .pending_turns
                .lock()
                .unwrap()
                .get("task-1")
                .map(|queue| queue.len()),
            Some(1)
        );
        assert!(store.running_tasks.lock().unwrap().get("task-1").is_none());
    }
}

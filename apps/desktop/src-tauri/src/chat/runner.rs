use std::collections::HashSet;
use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::{Map as JsonMap, Value as JsonValue};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::agent_events::{
    runner_quota_usage_result_control_type, AgentEventHost, AgentRuntimeEvent, AgentTurnContext,
};
use crate::agent_extensions::TodoMirrorExtension;
use crate::chat::auto_turn_decision::{prepare_turn_for_start, resolve_resume_session_id};
use crate::chat::contract;
use crate::chat::process_registry::{
    JsonlProcessPoll, JsonlProcessRegistry, JsonlProcessStdinStatus,
};
#[cfg(test)]
use crate::chat::state::take_next_pending_turn;
use crate::chat::state::{
    clear_runtime_state_for_app, clear_task_runtime_state_for_reset, finish_running_turn_handles,
    is_turn_marked_reset, persist_agent_session_id, persist_and_emit_interrupted_timeline_event,
    persist_runtime_state_for_app, session_key, set_context_usage, set_guide_status_for_app,
    should_emit_runner_exit_error, should_persist_user_message, take_next_pending_turn_for_app,
    take_next_recoverable_pending_turn, take_pending_finalization_for_app, ChatStore,
    PersistedRuntimeState, RunningTurn,
};
use crate::chat::timeline_sink::{
    assistant_error_text, log_agent_event_effect, normalize_timeline_text,
    persist_and_emit_error_timeline_event, persist_and_emit_message_timeline_event,
    persist_and_emit_model_selection_timeline_event, timeline_input_from_runtime_event,
    TimelineThrottle,
};
use crate::chat::title_update::spawn_title_update;
use crate::chat::types::{
    conversation_references_payload, AgentInteractionRequestEvent, ChatAttachment,
    ChatComposerState, ChatContextUsage, ChatConversationReference, ChatRollbackResult,
    ChatRuntimeCommand, ChatWorkflow, CodexComposerSettings, DoneEvent, ProviderRuntimeOptions,
    TurnStartedEvent,
};
use crate::chat::workflow::{automation_run_id, runtime_command_kind, workflow_kind};
use crate::process_command::hide_console_window;
use crate::provider::{
    build_effective_claude_settings, build_effective_codex_subagent_settings,
    load_agent_interaction_settings, normalize_codex_settings_profile, normalize_json_object,
    normalize_optional_string, normalize_reasoning_effort, normalize_runtime_workspace_roots,
    normalize_string_list, resolve_connection_for, CodexProfileSettings, ConnectionMode,
};
use crate::runner_protocol_contract;
use crate::store::LiliaStore;
use crate::{plugins, BACKEND_CLAUDE, BACKEND_CODEX};

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
    pub(crate) script_path: PathBuf,
}

#[derive(Default)]
pub(crate) struct RunnerOutput {
    pub(crate) last_session_id: Option<String>,
    pub(crate) interrupted: bool,
    pub(crate) reset: bool,
}

pub(crate) enum RunnerSessionPoll {
    Running,
    Completed(RunnerOutput),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunnerLifecycleEvent {
    pub(crate) stage: &'static str,
    pub(crate) detail: JsonValue,
}

pub(crate) trait RunnerLifecycleObserver {
    fn record(&mut self, event: RunnerLifecycleEvent);
}

struct NoopRunnerLifecycleObserver;

impl RunnerLifecycleObserver for NoopRunnerLifecycleObserver {
    fn record(&mut self, _event: RunnerLifecycleEvent) {}
}

fn record_runner_lifecycle(
    observer: &mut dyn RunnerLifecycleObserver,
    stage: &'static str,
    detail: JsonValue,
) {
    observer.record(RunnerLifecycleEvent { stage, detail });
}

pub(crate) struct RunnerSession {
    task_id: String,
    turn_id: String,
    backend: String,
    pub(crate) process_session_id: String,
    event_ctx: AgentTurnContext,
    event_host: AgentEventHost,
    timeline_throttle: TimelineThrottle,
    last_assistant_error_text: Option<String>,
    last_session_id: Option<String>,
    reset_observed: bool,
    finished: bool,
}

pub(crate) fn reattach_runner_session<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: String,
    turn_id: String,
    backend: String,
    process_session_id: String,
) -> Result<RunnerSession, String> {
    if !process_registry().is_active(&process_session_id) {
        return Err("runner session 已结束，无法恢复接管".to_string());
    }
    let mut event_host = AgentEventHost::new();
    event_host.register(Box::new(TodoMirrorExtension::new(app_handle.clone())));
    Ok(RunnerSession {
        task_id: task_id.clone(),
        turn_id: turn_id.clone(),
        backend: backend.clone(),
        process_session_id,
        event_ctx: AgentTurnContext {
            task_id,
            backend,
            turn_id,
            automation_run_id: None,
        },
        event_host,
        timeline_throttle: TimelineThrottle::new(),
        last_assistant_error_text: None,
        last_session_id: None,
        reset_observed: false,
        finished: false,
    })
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

// ---------- 子进程定位 ----------

/// 找到 agent-runner.mjs 的实际路径。
///
/// 开发态：cargo 编出来的二进制位于 `apps/desktop/src-tauri/target/{debug|release}/`，
/// 而脚本位于 `apps/desktop/agent-runner.mjs`，相对路径回退 3 层。
/// 按候选顺序找第一个存在的文件；找不到就返回最后一个候选让上层报错更直观。
pub(crate) fn locate_agent_runner<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1) 与 binary 同目录 → 适合未来 sidecar/资源拷贝场景
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("agent-runner.mjs"));
            // 2) 开发态：target/debug → 回退 3 层到 apps/desktop
            candidates.push(dir.join("../../../agent-runner.mjs"));
        }
    }

    // 3) Tauri resource_dir 兜底
    if let Ok(res) = app.path().resource_dir() {
        candidates.push(res.join("agent-runner.mjs"));
    }

    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    candidates
        .into_iter()
        .last()
        .unwrap_or_else(|| PathBuf::from("agent-runner.mjs"))
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

    let script_path = locate_agent_runner(&app);
    let app_handle = app.clone();
    let task_id_for_thread = task_id.clone();
    let backend_for_thread = backend.clone();
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
        script_path,
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
        let result = run_node_agent_runner(&app_handle, invocation);
        let mut runner_ok = true;
        let output = match result {
            Ok(output) => output,
            Err(err) => {
                runner_ok = false;
                persist_and_emit_error_timeline_event(
                    &app_handle,
                    &task_id_for_thread,
                    &backend_for_thread,
                    Some(&turn_id_for_finish),
                    err,
                );
                RunnerOutput::default()
            }
        };
        let agent_success = runner_ok && !output.interrupted && !output.reset;
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
            backend_for_thread,
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

pub(crate) fn run_node_agent_runner<R: Runtime>(
    app_handle: &AppHandle<R>,
    invocation: RunnerInvocation,
) -> Result<RunnerOutput, String> {
    let mut observer = NoopRunnerLifecycleObserver;
    run_node_agent_runner_with_observer(app_handle, invocation, &mut observer)
}

pub(crate) fn start_runner_session<R: Runtime>(
    app_handle: &AppHandle<R>,
    invocation: RunnerInvocation,
    observer: &mut dyn RunnerLifecycleObserver,
) -> Result<RunnerSession, String> {
    let task_id_for_thread = invocation.task_id;
    let mut composer_for_thread = invocation.composer;
    let prompt_for_thread = invocation.content;
    let project_cwd = invocation.project_cwd;
    let attachments_for_thread = invocation.attachments;
    let conversation_references_for_thread = invocation.conversation_references;
    let workflow_for_thread = invocation.workflow;
    let runtime_command_for_thread = invocation.runtime_command;
    let mut runtime_options_for_thread = invocation.runtime_options;
    let automation_run_id_for_thread = automation_run_id(workflow_for_thread.as_ref());
    let turn_id_for_thread = invocation.turn_id;
    let resume_session_id = invocation.resume_session_id;
    let is_new_session = resume_session_id.is_none();
    let queued_count = invocation.queued_count;
    let script_path = invocation.script_path;
    let prepared = prepare_turn_for_start(
        app_handle,
        &task_id_for_thread,
        &prompt_for_thread,
        composer_for_thread,
        &project_cwd,
        &attachments_for_thread,
        &conversation_references_for_thread,
        workflow_for_thread.as_ref(),
        runtime_command_for_thread.as_ref(),
        runtime_options_for_thread,
        resume_session_id.as_deref(),
    )?;
    composer_for_thread = prepared.composer;
    runtime_options_for_thread = prepared.runtime_options;
    let backend_for_thread = composer_for_thread.backend.clone();
    persist_and_emit_model_selection_timeline_event(
        app_handle,
        &task_id_for_thread,
        &backend_for_thread,
        &turn_id_for_thread,
        runtime_options_for_thread
            .as_ref()
            .and_then(|options| options.common.as_ref())
            .and_then(|common| common.model_selection.as_ref()),
    );
    let connection = resolve_connection_for(app_handle, &backend_for_thread);
    let extensions = plugins::runtime_extensions(app_handle, Some(&project_cwd));
    let runtime_options = build_provider_runtime_options(
        app_handle,
        &backend_for_thread,
        &composer_for_thread,
        runtime_command_for_thread.as_ref(),
        runtime_options_for_thread.as_ref(),
    );
    let runtime_options = apply_main_agent_prompt_to_runtime_options(
        app_handle,
        &backend_for_thread,
        runtime_options,
    );
    let runtime_options = crate::memory::apply_memory_baseline_to_runtime_options(
        app_handle,
        &task_id_for_thread,
        &project_cwd,
        &backend_for_thread,
        runtime_options,
    );
    let runtime_options = apply_dependency_context_to_runtime_options(
        app_handle,
        &task_id_for_thread,
        &backend_for_thread,
        runtime_options,
        is_new_session,
    );
    let mut stdin_payload = build_runner_stdin_payload(
        &backend_for_thread,
        &project_cwd,
        &prompt_for_thread,
        &attachments_for_thread,
        &conversation_references_for_thread,
        workflow_for_thread.as_ref(),
        runtime_command_for_thread.as_ref(),
        runtime_options.as_ref(),
        &composer_for_thread,
        resume_session_id.as_deref(),
        &extensions,
    );
    if let Some(context) = build_runner_conversation_context(app_handle, &task_id_for_thread) {
        stdin_payload["conversationContext"] = context;
    }
    record_runner_lifecycle(
        observer,
        "payload_prepared",
        serde_json::json!({
            "backend": backend_for_thread,
            "cwd": project_cwd,
            "resumeSessionId": resume_session_id,
            "queuedCount": queued_count,
            "attachmentCount": attachments_for_thread.len(),
            "workflowType": workflow_kind(workflow_for_thread.as_ref()),
            "runtimeCommandType": runtime_command_kind(runtime_command_for_thread.as_ref()),
            "hasRuntimeOptions": stdin_payload.get("runtimeOptions").is_some(),
            "codexRuntimeWorkspaceRootCount": stdin_payload
                .get("runtimeOptions")
                .and_then(|options| options.get("provider"))
                .and_then(|provider| provider.get("codex"))
                .and_then(|settings| settings.get("runtimeWorkspaceRoots"))
                .and_then(|roots| roots.as_array())
                .map(|roots| roots.len()),
            "hasConversationContext": stdin_payload.get("conversationContext").is_some(),
        }),
    );

    let mut cmd = Command::new("node");
    hide_console_window(&mut cmd);
    cmd.arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let (base_key, key_key) = match backend_for_thread.as_str() {
        BACKEND_CODEX => ("OPENAI_BASE_URL", "OPENAI_API_KEY"),
        _ => ("ANTHROPIC_BASE_URL", "ANTHROPIC_API_KEY"),
    };
    if let Some(url) = &connection.base_url {
        cmd.env(base_key, url);
    }
    if let Some(key) = &connection.api_key {
        cmd.env(key_key, key);
    }
    if backend_for_thread == BACKEND_CODEX {
        if connection.mode == ConnectionMode::CodexAccount {
            cmd.env_remove("OPENAI_BASE_URL");
            cmd.env_remove("OPENAI_API_KEY");
            cmd.env_remove("CODEX_API_KEY");
        }
    }
    record_runner_lifecycle(
        observer,
        "process_configured",
        serde_json::json!({
            "scriptPath": script_path.to_string_lossy(),
            "backend": backend_for_thread,
            "hasBaseUrl": connection.base_url.is_some(),
            "hasApiKey": connection.api_key.is_some(),
        }),
    );

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(err) => {
            let msg = format!("无法启动 node 子进程（请确保已安装 Node 18+ 并在 PATH 中）：{err}");
            record_runner_lifecycle(
                observer,
                "process_spawn_failed",
                serde_json::json!({ "message": msg }),
            );
            return Err(msg);
        }
    };
    record_runner_lifecycle(observer, "process_spawned", serde_json::json!({}));

    let process_session_id = process_registry()
        .start(child, &stdin_payload)
        .map_err(|err| format!("无法启动 runner session：{err}"))?;
    {
        let store = app_handle.state::<ChatStore>();
        let running_turn = RunningTurn {
            turn_id: turn_id_for_thread.clone(),
            backend: backend_for_thread.clone(),
        };
        store
            .running_process_sessions
            .lock()
            .unwrap()
            .insert(task_id_for_thread.clone(), process_session_id.clone());
        store
            .running_turns
            .lock()
            .unwrap()
            .insert(task_id_for_thread.clone(), running_turn.clone());
        let context_json = runtime_state_context_json(
            &project_cwd,
            &prompt_for_thread,
            &attachments_for_thread,
            &conversation_references_for_thread,
            workflow_for_thread.as_ref(),
            runtime_command_for_thread.as_ref(),
            &composer_for_thread,
            resume_session_id.as_deref(),
        );
        persist_runtime_state_for_app(
            app_handle,
            &store,
            &task_id_for_thread,
            &running_turn,
            "running",
            Some(&process_session_id),
            Some(&context_json),
        );
    }
    record_runner_lifecycle(
        observer,
        "running_turn_registered",
        serde_json::json!({
            "taskId": task_id_for_thread,
            "turnId": turn_id_for_thread,
            "backend": backend_for_thread,
        }),
    );

    let _ = app_handle.emit(
        contract::turn_started_event_name(),
        TurnStartedEvent {
            task_id: task_id_for_thread.clone(),
            queued_count,
        },
    );
    record_runner_lifecycle(
        observer,
        "turn_started_emitted",
        serde_json::json!({ "queuedCount": queued_count }),
    );
    match process_registry().stdin_status(&process_session_id) {
        Some(JsonlProcessStdinStatus::Ready { bytes }) => {
            record_runner_lifecycle(
                observer,
                "stdin_ready",
                serde_json::json!({ "bytes": bytes }),
            );
        }
        Some(JsonlProcessStdinStatus::Unavailable) => {
            record_runner_lifecycle(observer, "stdin_unavailable", serde_json::json!({}));
        }
        Some(JsonlProcessStdinStatus::WriteFailed { bytes }) => {
            record_runner_lifecycle(
                observer,
                "stdin_write_failed",
                serde_json::json!({ "bytes": bytes }),
            );
        }
        None => {
            record_runner_lifecycle(observer, "stdin_unavailable", serde_json::json!({}));
        }
    }
    if process_registry().stdout_available(&process_session_id) {
        record_runner_lifecycle(observer, "stdout_loop_started", serde_json::json!({}));
    } else {
        record_runner_lifecycle(observer, "stdout_unavailable", serde_json::json!({}));
    }

    let mut event_host = AgentEventHost::new();
    event_host.register(Box::new(TodoMirrorExtension::new(app_handle.clone())));
    Ok(RunnerSession {
        task_id: task_id_for_thread.clone(),
        turn_id: turn_id_for_thread.clone(),
        backend: backend_for_thread.clone(),
        process_session_id,
        event_ctx: AgentTurnContext {
            task_id: task_id_for_thread,
            backend: backend_for_thread,
            turn_id: turn_id_for_thread,
            automation_run_id: automation_run_id_for_thread,
        },
        event_host,
        timeline_throttle: TimelineThrottle::new(),
        last_assistant_error_text: None,
        last_session_id: None,
        reset_observed: false,
        finished: false,
    })
}

pub(crate) fn poll_runner_session<R: Runtime>(
    app_handle: &AppHandle<R>,
    session: &mut RunnerSession,
    observer: &mut dyn RunnerLifecycleObserver,
) -> Result<RunnerSessionPoll, String> {
    if session.finished {
        return Ok(RunnerSessionPoll::Completed(RunnerOutput {
            last_session_id: session.last_session_id.clone(),
            interrupted: false,
            reset: session.reset_observed,
        }));
    }

    loop {
        if is_turn_marked_reset(
            &app_handle.state::<ChatStore>(),
            &session.task_id,
            &session.turn_id,
            &session.backend,
        ) && !session.reset_observed
        {
            session.reset_observed = true;
            observer.record(RunnerLifecycleEvent {
                stage: "reset_mark_observed",
                detail: serde_json::json!({
                    "taskId": session.task_id,
                    "turnId": session.turn_id,
                }),
            });
        }
        let Some(poll) = process_registry().poll(&session.process_session_id) else {
            return Err("runner session 不存在或已被释放".to_string());
        };
        match poll {
            JsonlProcessPoll::Pending => return Ok(RunnerSessionPoll::Running),
            JsonlProcessPoll::StdoutLine(line) => {
                let value: JsonValue = match serde_json::from_str(&line) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let Some(event) = AgentRuntimeEvent::from_runner_json(&value) else {
                    continue;
                };
                observer.record(RunnerLifecycleEvent {
                    stage: "runner_event",
                    detail: serde_json::json!({ "kind": runner_event_kind(&event) }),
                });
                handle_runner_runtime_event(app_handle, session, observer, event);
            }
            JsonlProcessPoll::Exited(exit) => {
                if process_registry().stdout_available(&session.process_session_id) {
                    observer.record(RunnerLifecycleEvent {
                        stage: "stdout_loop_finished",
                        detail: serde_json::json!({}),
                    });
                }
                let output = finalize_runner_session(
                    app_handle,
                    session,
                    observer,
                    exit.success,
                    &exit.stderr_text,
                );
                session.finished = true;
                return Ok(RunnerSessionPoll::Completed(output));
            }
        }
    }
}

fn handle_runner_runtime_event<R: Runtime>(
    app_handle: &AppHandle<R>,
    session: &mut RunnerSession,
    observer: &mut dyn RunnerLifecycleObserver,
    event: AgentRuntimeEvent,
) {
    log_agent_event_effect(session.event_host.dispatch(&session.event_ctx, &event));
    match &event {
        AgentRuntimeEvent::ToolUse { .. } | AgentRuntimeEvent::TodoList { .. } => {}
        AgentRuntimeEvent::Timeline { .. } => {
            if let Some(input) = timeline_input_from_runtime_event(&session.event_ctx, &event) {
                if let Some(text) = assistant_error_text(&input) {
                    session.last_assistant_error_text = Some(text);
                }
                session.timeline_throttle.submit(app_handle, input);
            }
        }
        AgentRuntimeEvent::InteractionRequest {
            id,
            kind,
            backend,
            payload,
        } => {
            record_runner_lifecycle(
                observer,
                "interaction_requested",
                serde_json::json!({
                    "requestId": id,
                    "kind": kind,
                    "backend": backend.clone().unwrap_or_else(|| session.backend.clone()),
                }),
            );
            let _ = app_handle.emit(
                contract::agent_interaction_request_event_name(),
                AgentInteractionRequestEvent {
                    task_id: session.task_id.clone(),
                    turn_id: session.turn_id.clone(),
                    backend: backend.clone().unwrap_or_else(|| session.backend.clone()),
                    request_id: id.clone(),
                    kind: kind.clone(),
                    payload: payload.clone(),
                },
            );
            crate::remote_control::record_pending_interaction(
                session.task_id.clone(),
                session.turn_id.clone(),
                backend.clone().unwrap_or_else(|| session.backend.clone()),
                id.clone(),
                kind.clone(),
                payload.clone(),
            );
            crate::automation::emit_interaction_signal(
                app_handle,
                session.task_id.clone(),
                session.turn_id.clone(),
                backend.clone().unwrap_or_else(|| session.backend.clone()),
                id.clone(),
                kind.clone(),
                payload.clone(),
                session.event_ctx.automation_run_id.clone(),
            );
        }
        AgentRuntimeEvent::QuotaUsageRequest { id, payload } => {
            record_runner_lifecycle(
                observer,
                "quota_usage_requested",
                serde_json::json!({
                    "requestId": id,
                }),
            );
            let result = handle_quota_usage_request(app_handle, payload.clone());
            let response = match result {
                Ok(value) => serde_json::json!({
                    "type": runner_quota_usage_result_control_type(),
                    "id": id,
                    "ok": true,
                    "result": value,
                }),
                Err(err) => serde_json::json!({
                    "type": runner_quota_usage_result_control_type(),
                    "id": id,
                    "ok": false,
                    "error": err,
                }),
            };
            if let Err(err) = write_runner_stdin_payload(&session.process_session_id, response) {
                record_runner_lifecycle(
                    observer,
                    "quota_usage_response_failed",
                    serde_json::json!({
                        "requestId": id,
                        "error": err,
                    }),
                );
            }
        }
        AgentRuntimeEvent::ContextUsage {
            used_tokens,
            limit_tokens,
            used_percent,
            source,
            unavailable_reason,
        } => {
            let computed_percent = limit_tokens.and_then(|limit| {
                if limit == 0 {
                    None
                } else {
                    Some(((*used_tokens as f64 / limit as f64) * 100.0).clamp(0.0, 100.0))
                }
            });
            let usage = ChatContextUsage {
                task_id: session.task_id.clone(),
                backend: session.backend.clone(),
                used_tokens: *used_tokens,
                limit_tokens: *limit_tokens,
                used_percent: used_percent
                    .or(computed_percent)
                    .map(|percent| percent.clamp(0.0, 100.0)),
                source: source.clone().unwrap_or_else(|| "runtime".to_string()),
                updated_at: crate::util::now_millis() as u64,
                unavailable_reason: unavailable_reason.clone(),
            };
            record_runner_lifecycle(
                observer,
                "context_usage_updated",
                serde_json::json!({
                    "usedTokens": usage.used_tokens,
                    "hasLimit": usage.limit_tokens.is_some(),
                    "hasPercent": usage.used_percent.is_some(),
                }),
            );
            let store = app_handle.state::<ChatStore>();
            set_context_usage(&store, usage.clone());
            let _ = app_handle.emit(contract::context_usage_event_name(), usage);
        }
        AgentRuntimeEvent::Done { session_id, .. } => {
            if let Some(sid) = session_id {
                session.last_session_id = Some(sid.clone());
            }
            record_runner_lifecycle(
                observer,
                "runner_done",
                serde_json::json!({ "hasSessionId": session_id.is_some() }),
            );
        }
        AgentRuntimeEvent::PromptSuggestion { suggestion, uuid } => {
            if session.backend == BACKEND_CLAUDE {
                if let Err(err) = crate::conversation_suggestions::save_claude_prompt_suggestion(
                    app_handle,
                    &session.task_id,
                    suggestion,
                    uuid.as_deref(),
                ) {
                    eprintln!(
                        "[conversation-suggestions] save Claude prompt suggestion failed: {err}"
                    );
                }
            }
        }
        AgentRuntimeEvent::Error { message } => {
            record_runner_lifecycle(
                observer,
                "runner_error_event",
                serde_json::json!({ "message": message }),
            );
            session.timeline_throttle.flush_all(app_handle);
            if session
                .last_assistant_error_text
                .as_deref()
                .is_some_and(|text| normalize_timeline_text(message).contains(text))
            {
                return;
            }
            persist_and_emit_error_timeline_event(
                app_handle,
                &session.task_id,
                &session.backend,
                Some(&session.turn_id),
                message.clone(),
            );
        }
    }
}

fn finalize_runner_session<R: Runtime>(
    app_handle: &AppHandle<R>,
    session: &mut RunnerSession,
    observer: &mut dyn RunnerLifecycleObserver,
    process_success: bool,
    stderr_text: &str,
) -> RunnerOutput {
    drop(process_registry().take_stdin_handle(&session.process_session_id));
    process_registry().release_child_handle(&session.process_session_id);
    let _ = process_registry().remove(&session.process_session_id);
    record_runner_lifecycle(observer, "stdin_closed", serde_json::json!({}));
    let finished = {
        let store = app_handle.state::<ChatStore>();
        finish_running_turn_handles(&store, &session.task_id, &session.turn_id, &session.backend)
    };
    clear_runtime_state_for_app(app_handle, &session.task_id);
    record_runner_lifecycle(
        observer,
        "stop_marks_consumed",
        serde_json::json!({
            "interrupted": finished.interrupted,
            "reset": finished.reset,
        }),
    );

    if finished.reset {
        session.timeline_throttle.pending.clear();
        record_runner_lifecycle(
            observer,
            "timeline_discarded_for_reset",
            serde_json::json!({}),
        );
    } else {
        session.timeline_throttle.flush_all(app_handle);
        record_runner_lifecycle(observer, "timeline_flushed", serde_json::json!({}));
    }

    record_runner_lifecycle(
        observer,
        "process_waited",
        serde_json::json!({
            "success": process_success,
            "stderrLength": stderr_text.len(),
        }),
    );

    if finished.interrupted && !finished.reset {
        record_runner_lifecycle(
            observer,
            "interrupted_event_persisted",
            serde_json::json!({}),
        );
        persist_and_emit_interrupted_timeline_event(
            app_handle,
            &session.task_id,
            &session.backend,
            &session.turn_id,
        );
    }

    if !finished.reset
        && should_emit_runner_exit_error(finished.interrupted, !process_success, stderr_text)
    {
        record_runner_lifecycle(
            observer,
            "process_exit_error_emitted",
            serde_json::json!({ "stderrLength": stderr_text.trim().len() }),
        );
        persist_and_emit_error_timeline_event(
            app_handle,
            &session.task_id,
            &session.backend,
            Some(&session.turn_id),
            format!("agent 进程异常退出：{}", stderr_text.trim()),
        );
    }

    if !finished.interrupted && !finished.reset {
        record_runner_lifecycle(observer, "title_update_spawned", serde_json::json!({}));
        spawn_title_update(
            app_handle.clone(),
            session.task_id.clone(),
            session.backend.clone(),
            Some(session.turn_id.clone()),
        );
    }

    RunnerOutput {
        last_session_id: session.last_session_id.clone(),
        interrupted: finished.interrupted,
        reset: finished.reset,
    }
}

pub(crate) fn run_node_agent_runner_with_observer<R: Runtime>(
    app_handle: &AppHandle<R>,
    invocation: RunnerInvocation,
    observer: &mut dyn RunnerLifecycleObserver,
) -> Result<RunnerOutput, String> {
    let mut session = start_runner_session(app_handle, invocation, observer)?;
    loop {
        match poll_runner_session(app_handle, &mut session, observer)? {
            RunnerSessionPoll::Running => thread::sleep(Duration::from_millis(16)),
            RunnerSessionPoll::Completed(output) => return Ok(output),
        }
    }
}

pub(crate) fn resume_node_agent_runner_with_observer<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: String,
    turn_id: String,
    backend: String,
    process_session_id: String,
    observer: &mut dyn RunnerLifecycleObserver,
) -> Result<RunnerOutput, String> {
    let mut session =
        reattach_runner_session(app_handle, task_id, turn_id, backend, process_session_id)?;
    loop {
        match poll_runner_session(app_handle, &mut session, observer)? {
            RunnerSessionPoll::Running => thread::sleep(Duration::from_millis(16)),
            RunnerSessionPoll::Completed(output) => return Ok(output),
        }
    }
}

pub(crate) fn resume_node_agent_runner<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: String,
    turn_id: String,
    backend: String,
    process_session_id: String,
) -> Result<RunnerOutput, String> {
    let mut observer = NoopRunnerLifecycleObserver;
    resume_node_agent_runner_with_observer(
        app_handle,
        task_id,
        turn_id,
        backend,
        process_session_id,
        &mut observer,
    )
}

pub(crate) fn resume_persisted_node_agent_runner<R: Runtime>(
    app_handle: AppHandle<R>,
    persisted: PersistedRuntimeState,
) -> Result<(), String> {
    let process_session_id = persisted
        .process_session_id
        .clone()
        .ok_or_else(|| "persisted runner session 缺少 process_session_id".to_string())?;
    let task_id = persisted.task_id.clone();
    let turn_id = persisted.turn.turn_id.clone();
    let backend = persisted.turn.backend.clone();
    let output = resume_node_agent_runner(
        &app_handle,
        task_id.clone(),
        turn_id,
        backend.clone(),
        process_session_id,
    )?;
    finish_agent_turn(
        app_handle,
        task_id,
        backend,
        output.last_session_id,
        !output.interrupted && !output.reset,
        None,
    );
    Ok(())
}

fn handle_quota_usage_request<R: Runtime>(
    app_handle: &AppHandle<R>,
    payload: JsonValue,
) -> Result<JsonValue, String> {
    let input: crate::quota_usage::QuotaUsageQueryInput =
        serde_json::from_value(payload).map_err(|e| e.to_string())?;
    let store = app_handle
        .try_state::<LiliaStore>()
        .ok_or_else(|| "LiliaStore is not available".to_string())?;
    let conn = store.conn()?;
    crate::quota_usage::query_usage(&conn, input, crate::util::now_millis())
}

fn runner_event_kind(event: &AgentRuntimeEvent) -> &'static str {
    event.event_type()
}

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
    let backend = payload
        .get("backend")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(BACKEND_CODEX);
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
    let mut composer = crate::chat::state::default_composer(task_id);
    composer.backend = backend.to_string();
    let mut output = JsonValue::Null;
    for _ in 0..iterations {
        output = build_runner_stdin_payload(
            backend,
            project_cwd,
            prompt,
            &[],
            &[],
            None,
            None,
            None,
            &composer,
            None,
            &serde_json::json!({}),
        );
    }
    Ok(output)
}

fn runtime_state_context_json(
    project_cwd: &str,
    prompt: &str,
    attachments: &[ChatAttachment],
    conversation_references: &[ChatConversationReference],
    workflow: Option<&ChatWorkflow>,
    runtime_command: Option<&ChatRuntimeCommand>,
    composer: &ChatComposerState,
    resume_session_id: Option<&str>,
) -> String {
    serde_json::json!({
        "projectCwd": project_cwd,
        "promptLength": prompt.chars().count(),
        "attachmentCount": attachments.len(),
        "conversationReferenceCount": conversation_references.len(),
        "workflowType": workflow_kind(workflow),
        "runtimeCommandType": runtime_command_kind(runtime_command),
        "automationRunId": automation_run_id(workflow),
        "resumeSessionId": resume_session_id,
        "permission": composer.permission,
        "composerRuntimeWorkspaceRoots": composer
            .codex_settings
            .runtime_workspace_roots
            .clone()
            .unwrap_or_default(),
    })
    .to_string()
}

fn build_provider_runtime_options<R: Runtime>(
    app: &AppHandle<R>,
    backend: &str,
    composer: &ChatComposerState,
    runtime_command: Option<&ChatRuntimeCommand>,
    runtime_options: Option<&ProviderRuntimeOptions>,
) -> Option<JsonValue> {
    let mut provider = serde_json::Map::new();
    if backend == BACKEND_CODEX {
        let mut codex = build_effective_codex_settings(app, composer);
        if let Some(subagents) = build_effective_codex_subagent_settings(app) {
            if let (Some(target), Some(source)) = (codex.as_object_mut(), subagents.as_object()) {
                for (key, value) in source {
                    if !target.contains_key(key) || target[key].is_null() {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        provider.insert("codex".to_string(), codex);
    }
    if backend == BACKEND_CLAUDE {
        if let Some(claude) = build_effective_claude_settings(app) {
            provider.insert("claude".to_string(), claude);
        }
    }
    if let Some(options) = runtime_options {
        if let Ok(value) = serde_json::to_value(options) {
            return Some(merge_runtime_provider_defaults(value, provider));
        }
    }
    if matches!(
        runtime_command,
        Some(ChatRuntimeCommand::RuntimeSettings { .. })
    ) {
        return Some(merge_runtime_provider_defaults(
            serde_json::json!({}),
            provider,
        ));
    }
    if provider.is_empty() {
        None
    } else {
        Some(JsonValue::Object(
            [("provider".to_string(), JsonValue::Object(provider))]
                .into_iter()
                .collect(),
        ))
    }
}

fn merge_runtime_provider_defaults(
    mut value: JsonValue,
    defaults: serde_json::Map<String, JsonValue>,
) -> JsonValue {
    if defaults.is_empty() {
        return value;
    }
    if !value.is_object() {
        value = serde_json::json!({});
    }
    if !value
        .get("provider")
        .is_some_and(|provider| provider.is_object())
    {
        value["provider"] = serde_json::json!({});
    }
    for (key, default_value) in defaults {
        let existing = value["provider"].get(&key).cloned();
        match (existing, default_value.as_object()) {
            (Some(JsonValue::Object(mut current)), Some(default_map)) => {
                for (inner_key, inner_value) in default_map {
                    if current.get(inner_key).is_none() || current[inner_key].is_null() {
                        current.insert(inner_key.clone(), inner_value.clone());
                    }
                }
                value["provider"][&key] = JsonValue::Object(current);
            }
            (Some(current), _) if !current.is_null() => {}
            _ => {
                value["provider"][key] = default_value;
            }
        }
    }
    value
}

fn build_effective_codex_settings<R: Runtime>(
    app: &AppHandle<R>,
    composer: &ChatComposerState,
) -> JsonValue {
    let global = load_agent_interaction_settings(app).codex_profile;
    let project = crate::project_shell::load_project_settings(app)
        .codex_defaults
        .unwrap_or_default();
    let local = &composer.codex_settings;
    let fallback_model = composer.model.trim();
    let default_model = crate::chat::state::default_model_for_backend(BACKEND_CODEX);
    let model = normalize_optional_string(local.model.clone())
        .or_else(|| {
            if !fallback_model.is_empty() && fallback_model != default_model {
                Some(fallback_model.to_string())
            } else {
                normalize_optional_string(project.model.clone())
                    .or_else(|| normalize_optional_string(global.model.clone()))
            }
        })
        .or_else(|| normalize_optional_string(Some(fallback_model.to_string())));
    let reasoning_effort = normalize_reasoning_effort(local.reasoning_effort.clone())
        .or_else(|| normalize_reasoning_effort(project.reasoning_effort.clone()))
        .or_else(|| normalize_reasoning_effort(global.reasoning_effort.clone()));
    let runtime_workspace_roots = effective_runtime_workspace_roots(local, &project, &global);
    let responses_api_client_metadata =
        normalize_json_object(local.responses_api_client_metadata.clone())
            .or_else(|| normalize_json_object(project.responses_api_client_metadata.clone()))
            .or_else(|| normalize_json_object(global.responses_api_client_metadata.clone()));
    let additional_context = normalize_optional_string(local.additional_context.clone())
        .or_else(|| normalize_optional_string(project.additional_context.clone()))
        .or_else(|| normalize_optional_string(global.additional_context.clone()));
    let persist_extended_history = local
        .persist_extended_history
        .or(project.persist_extended_history)
        .or(global.persist_extended_history);
    let initial_turns_page = normalize_json_object(local.initial_turns_page.clone())
        .or_else(|| normalize_json_object(project.initial_turns_page.clone()))
        .or_else(|| normalize_json_object(global.initial_turns_page.clone()));
    let exclude_turns = effective_string_list(
        local.exclude_turns.clone(),
        &project.exclude_turns,
        &global.exclude_turns,
    );
    let profile = normalize_codex_settings_profile(local.profile.clone())
        .or_else(|| {
            let project_profile = normalize_codex_settings_profile(Some(project.profile));
            project_profile.filter(|value| value != "default")
        })
        .unwrap_or_else(|| normalize_codex_settings_profile(Some(global.profile)).unwrap());

    serde_json::json!({
        "profile": profile,
        "model": model,
        "reasoningEffort": reasoning_effort,
        "runtimeWorkspaceRoots": runtime_workspace_roots,
        "responsesApiClientMetadata": responses_api_client_metadata,
        "additionalContext": additional_context,
        "persistExtendedHistory": persist_extended_history,
        "initialTurnsPage": initial_turns_page,
        "excludeTurns": exclude_turns,
    })
}

fn effective_runtime_workspace_roots(
    local: &CodexComposerSettings,
    project: &CodexProfileSettings,
    global: &CodexProfileSettings,
) -> Vec<String> {
    match local.runtime_workspace_roots.clone() {
        Some(roots) => normalize_runtime_workspace_roots(roots),
        None if !project.runtime_workspace_roots.is_empty() => {
            normalize_runtime_workspace_roots(project.runtime_workspace_roots.clone())
        }
        None => normalize_runtime_workspace_roots(global.runtime_workspace_roots.clone()),
    }
}

fn effective_string_list(
    local: Option<Vec<String>>,
    project: &[String],
    global: &[String],
) -> Vec<String> {
    match local {
        Some(values) => normalize_string_list(values),
        None if !project.is_empty() => normalize_string_list(project.to_vec()),
        None => normalize_string_list(global.to_vec()),
    }
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
const DEPENDENCY_CONTEXT_MESSAGE_SCAN_LIMIT: i64 = 64;

struct DependencyTaskRow {
    id: String,
    title: String,
    status: String,
}

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

fn apply_main_agent_prompt_to_runtime_options<R: Runtime>(
    app: &AppHandle<R>,
    backend: &str,
    runtime_options: Option<JsonValue>,
) -> Option<JsonValue> {
    let settings = load_agent_interaction_settings(app);
    append_main_agent_prompt_to_runtime_options(
        backend,
        runtime_options,
        &settings.main_agent_prompt_mode,
        &settings.main_agent_custom_prompt,
    )
}

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

fn apply_dependency_context_to_runtime_options<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
    backend: &str,
    runtime_options: Option<JsonValue>,
    is_new_session: bool,
) -> Option<JsonValue> {
    if !is_new_session {
        return runtime_options;
    }
    let Some(store) = app.try_state::<LiliaStore>() else {
        return runtime_options;
    };
    let Ok(conn) = store.conn() else {
        return runtime_options;
    };
    match build_dependency_context_core(&conn, task_id) {
        Ok(Some(context)) => {
            crate::memory::append_context_to_runtime_options(backend, runtime_options, &context)
        }
        Ok(None) => runtime_options,
        Err(err) => {
            eprintln!("[dependency-context] skipped: {err}");
            runtime_options
        }
    }
}

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

fn compact_context_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_runner_conversation_context<R: Runtime>(
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

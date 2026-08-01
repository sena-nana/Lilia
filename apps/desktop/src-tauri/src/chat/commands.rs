use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use tauri::{AppHandle, Manager, State};

use crate::agent_events::{
    runner_interaction_response_control_type, runner_interrupt_turn_control_type,
    runner_settings_update_control_type,
};
use crate::chat::auto_turn_decision::{prepare_turn_for_start, resolve_resume_session_id};
use crate::chat::runner::{
    ensure_task_ready_for_agent_turn, spawn_agent_turn, write_runner_stdin_for_task,
};
use crate::chat::slash_commands::{
    emit_slash_command_done, execute_slash_command, persist_and_emit_slash_command_result,
};
#[cfg(test)]
use crate::chat::state::{clear_pending_turns, RunningTurn};
use crate::chat::state::{
    clear_persisted_pending_rollback, default_composer, model_options_for_backend,
    new_chat_message_id, normalize_composer_for_backend, now_millis, queue_pending_turn_for_app,
    set_guide_status_for_app, should_persist_user_message, stop_running_turn, ChatStore,
};
use crate::chat::timeline_sink::{
    persist_and_emit_message_timeline_event, persist_and_emit_model_selection_timeline_event,
};
use crate::chat::types::{
    ChatAttachment, ChatComposerState, ChatConversationReference, ChatInterruptResult, ChatMessage,
    ChatModelOption, ChatRuntimeCommand, ChatRuntimeSnapshot, ChatSendResult, ChatWorkflow,
    ProviderRuntimeOptions,
};
use crate::provider::{
    load_active_backend, load_assistant_ai_config, validate_backend_ready_for_send,
};
use crate::store::LiliaStore;

#[cfg(test)]
#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct ResetSessionPlan {
    pub(crate) cleared_guide_ids: Vec<String>,
    pub(crate) stopped_running: bool,
    pub(crate) immediate_cleanup: bool,
}

fn model_selection_from_runtime_options(
    runtime_options: Option<&ProviderRuntimeOptions>,
) -> Option<&JsonValue> {
    runtime_options
        .and_then(|options| options.common.as_ref())
        .and_then(|common| common.model_selection.as_ref())
}

#[tauri::command]
pub fn chat_send_message(
    app: AppHandle,
    task_id: String,
    content: String,
    composer: ChatComposerState,
    project_cwd: String,
    attachments: Vec<ChatAttachment>,
    conversation_references: Vec<ChatConversationReference>,
    guide_id: Option<String>,
    workflow: Option<ChatWorkflow>,
    runtime_command: Option<ChatRuntimeCommand>,
    runtime_options: Option<ProviderRuntimeOptions>,
    store: State<'_, ChatStore>,
) -> Result<ChatSendResult, String> {
    let active_backend = load_active_backend(&app);
    validate_backend_ready_for_send(&active_backend)?;
    let mut composer = normalize_composer_for_backend(composer, &task_id, &active_backend);
    let mut runtime_options = runtime_options;
    ensure_task_ready_for_agent_turn(&app, &task_id)?;
    let slash_command_id = match &workflow {
        Some(ChatWorkflow::SlashCommand { command_id, .. }) => Some(command_id.clone()),
        _ => None,
    };
    // 1) 写入 user 消息并立即返回，给前端一个乐观渲染的锚点。
    let user_msg = ChatMessage {
        id: new_chat_message_id(),
        task_id: task_id.clone(),
        role: "user".to_string(),
        content: content.clone(),
        attachments: attachments.clone(),
        conversation_references: conversation_references.clone(),
        created_at: now_millis(),
    };
    // turn_id 在 user 消息入库前就分配，并与 agent turn 共享 —— 让两者落到同一个
    // turn_seq，user 消息天然占据 turn 内 intra_turn_order=0 的位置。
    let turn_id = format!("turn-{}", now_millis());
    let persist_user_message = should_persist_user_message(&content, &workflow, &runtime_command);
    store
        .composers
        .lock()
        .unwrap()
        .insert(task_id.clone(), composer.clone());

    if let Some(command_id) = slash_command_id {
        let execution = execute_slash_command(&command_id, &project_cwd, &composer.backend)?;
        persist_and_emit_slash_command_result(
            &app,
            &task_id,
            &turn_id,
            &composer.backend,
            &execution,
        );
        emit_slash_command_done(&app, task_id.clone());
        return Ok(ChatSendResult {
            message: user_msg,
            dispatch: "started".to_string(),
            queued_count: 0,
            turn_id,
        });
    }

    {
        let mut running = store.running_tasks.lock().unwrap();
        if running.contains_key(&task_id) {
            drop(running);
            set_guide_status_for_app(&app, guide_id.as_deref(), "queued")?;
            let model_selection =
                model_selection_from_runtime_options(runtime_options.as_ref()).cloned();
            let queued_count = queue_pending_turn_for_app(
                &app,
                &store,
                &task_id,
                content,
                composer.clone(),
                project_cwd,
                attachments,
                conversation_references,
                workflow,
                runtime_command,
                runtime_options,
                user_msg.clone(),
                turn_id.clone(),
                guide_id.clone(),
            );
            if persist_user_message {
                persist_and_emit_message_timeline_event(
                    &app,
                    &user_msg,
                    &composer.backend,
                    &turn_id,
                    true,
                    None,
                );
            }
            persist_and_emit_model_selection_timeline_event(
                &app,
                &task_id,
                &composer.backend,
                &turn_id,
                model_selection.as_ref(),
            );
            return Ok(ChatSendResult {
                message: user_msg,
                dispatch: "queued".to_string(),
                queued_count,
                turn_id,
            });
        }
        running.insert(task_id.clone(), true);
    }

    let resume_session_id = resolve_resume_session_id(&app, &task_id, &composer.backend);
    match prepare_turn_for_start(
        &app,
        &task_id,
        &content,
        composer,
        &project_cwd,
        &attachments,
        &conversation_references,
        workflow.as_ref(),
        runtime_command.as_ref(),
        runtime_options,
        resume_session_id.as_deref(),
    ) {
        Ok(prepared) => {
            composer = prepared.composer;
            runtime_options = prepared.runtime_options;
        }
        Err(err) => {
            store.running_tasks.lock().unwrap().remove(&task_id);
            return Err(err);
        }
    }

    set_guide_status_for_app(&app, guide_id.as_deref(), "sent")?;
    if persist_user_message {
        persist_and_emit_message_timeline_event(
            &app,
            &user_msg,
            &composer.backend,
            &turn_id,
            false,
            None,
        );
    }
    persist_and_emit_model_selection_timeline_event(
        &app,
        &task_id,
        &composer.backend,
        &turn_id,
        model_selection_from_runtime_options(runtime_options.as_ref()),
    );

    spawn_agent_turn(
        app,
        task_id,
        content,
        composer,
        project_cwd,
        attachments,
        conversation_references,
        workflow,
        runtime_command,
        runtime_options,
        turn_id.clone(),
    );

    Ok(ChatSendResult {
        message: user_msg,
        dispatch: "started".to_string(),
        queued_count: 0,
        turn_id,
    })
}

#[tauri::command]
pub fn chat_interrupt_turn(
    task_id: String,
    app: AppHandle,
    store: State<'_, ChatStore>,
) -> Result<ChatInterruptResult, String> {
    let running_turn = store.running_turns.lock().unwrap().get(&task_id).cloned();
    let Some(running_turn) = running_turn else {
        return Ok(ChatInterruptResult::default());
    };

    let native_cancellation = if running_turn.backend
        == crate::native_agent::BACKEND_NATIVE_AGENTKIT
    {
        Some(crate::native_agent::cancel_product_agent_turn(
            &app,
            &task_id,
            &running_turn.turn_id,
        )?)
    } else if let Err(err) = write_runner_stdin(&store, &task_id, interrupt_turn_control_payload())
    {
        eprintln!("[chat-runtime] send interrupt control message failed: {err}");
        None
    } else {
        None
    };
    let stopped = stop_running_turn(&app, &store, &task_id, true, false)?;
    if native_cancellation
        == Some(lilia_agent_integration::TurnCancellationDisposition::PausedApproval)
    {
        if let Some(turn) = stopped.filter(|turn| turn.native_approval_pause.is_some()) {
            crate::native_agent::finish_cancelled_native_approval_turn(&app, &task_id, turn);
        }
    }
    Ok(ChatInterruptResult::default())
}

fn write_runner_stdin(
    store: &ChatStore,
    task_id: &str,
    payload: JsonValue,
) -> Result<bool, String> {
    write_runner_stdin_for_task(store, task_id, payload)
}

pub(crate) fn interrupt_turn_control_payload() -> JsonValue {
    serde_json::json!({ "type": runner_interrupt_turn_control_type() })
}

pub(crate) fn process_session_command_payload(command: ChatRuntimeCommand) -> JsonValue {
    serde_json::json!({
        "type": "process_session_command",
        "runtimeCommand": command,
    })
}

pub(crate) fn agent_interaction_response_payload(
    request_id: String,
    kind: String,
    result: JsonValue,
) -> JsonValue {
    serde_json::json!({
        "type": runner_interaction_response_control_type(),
        "id": request_id,
        "kind": kind,
        "result": result,
    })
}

pub(crate) fn composer_runtime_settings_update_payload(
    previous: Option<&ChatComposerState>,
    next: &ChatComposerState,
) -> Option<JsonValue> {
    let Some(previous) = previous else {
        return None;
    };
    let mut payload = serde_json::Map::from_iter([(
        "type".to_string(),
        JsonValue::String(runner_settings_update_control_type().to_string()),
    )]);
    if previous.permission != next.permission {
        match next.permission.as_str() {
            "full" | "readonly" | "ask" => {
                payload.insert(
                    "permission".to_string(),
                    JsonValue::String(next.permission.clone()),
                );
            }
            _ => {}
        }
    }
    if previous.model != next.model {
        let model = next.model.trim();
        if !model.is_empty() {
            payload.insert("model".to_string(), JsonValue::String(model.to_string()));
        }
    }
    if payload.len() == 1 {
        None
    } else {
        Some(JsonValue::Object(payload))
    }
}

fn composer_runtime_settings_update_attributes(payload: &JsonValue) -> BTreeMap<String, String> {
    let mut attributes = BTreeMap::new();
    if let Some(permission) = payload.get("permission").and_then(JsonValue::as_str) {
        attributes.insert("permission".to_string(), permission.to_string());
    }
    if let Some(model) = payload.get("model").and_then(JsonValue::as_str) {
        attributes.insert("model".to_string(), model.to_string());
    }
    attributes
}

pub(crate) fn control_event_attributes(
    pairs: impl IntoIterator<Item = (&'static str, String)>,
) -> BTreeMap<String, String> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

#[cfg(test)]
pub(crate) fn plan_reset_session(
    store: &ChatStore,
    task_id: &str,
    running_turn: Option<&RunningTurn>,
) -> ResetSessionPlan {
    match running_turn {
        None => ResetSessionPlan {
            cleared_guide_ids: clear_pending_turns(store, task_id),
            stopped_running: false,
            immediate_cleanup: true,
        },
        Some(_) => ResetSessionPlan::default(),
    }
}

pub(crate) fn attach_stdin_delivery(
    attributes: &mut BTreeMap<String, String>,
    result: &Result<bool, String>,
) {
    match result {
        Ok(forwarded) => {
            attributes.insert("stdinForwarded".to_string(), forwarded.to_string());
        }
        Err(err) => {
            attributes.insert("stdinForwarded".to_string(), "false".to_string());
            attributes.insert("stdinError".to_string(), err.clone());
        }
    }
}

/// 把用户对统一 Agent interaction 的响应写回 runner 的 stdin。
#[tauri::command]
pub fn chat_respond_agent_interaction(
    task_id: String,
    request_id: String,
    kind: String,
    result: JsonValue,
    _app: AppHandle,
    store: State<'_, ChatStore>,
) -> Result<(), String> {
    let payload = agent_interaction_response_payload(request_id.clone(), kind.clone(), result);
    let mut attributes = control_event_attributes([("requestId", request_id), ("kind", kind)]);
    let write_result = write_runner_stdin(&store, &task_id, payload);
    attach_stdin_delivery(&mut attributes, &write_result);
    write_result.map(|_| ())
}

#[tauri::command]
pub fn chat_send_process_session_command(
    task_id: String,
    command: ChatRuntimeCommand,
    store: State<'_, ChatStore>,
) -> Result<(), String> {
    match command {
        ChatRuntimeCommand::ProcessSession { .. } => {
            let payload = process_session_command_payload(command);
            write_runner_stdin(&store, &task_id, payload).map(|_| ())
        }
        _ => Err(
            "chat_send_process_session_command 只接受 process_session runtime command".to_string(),
        ),
    }
}

#[tauri::command]
pub fn chat_list_models(app: AppHandle, backend: String) -> Vec<ChatModelOption> {
    let normalized_backend = crate::chat::state::normalize_backend(&backend).to_string();
    let mut options = model_options_for_backend(&normalized_backend);
    if normalized_backend == crate::BACKEND_CODEX {
        let mut seen = options
            .iter()
            .map(|option| option.id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        for item in load_assistant_ai_config(&app).model_pool {
            if item.backend != crate::BACKEND_CODEX {
                continue;
            }
            let id = item.id.trim();
            if id.is_empty() || !seen.insert(id.to_string()) {
                continue;
            }
            let label = item.label.trim();
            options.push(ChatModelOption {
                id: id.to_string(),
                label: if label.is_empty() {
                    id.to_string()
                } else {
                    label.to_string()
                },
                backend: normalized_backend.clone(),
            });
        }
    }
    options
}

#[tauri::command]
pub fn chat_get_composer_state(task_id: String, store: State<'_, ChatStore>) -> ChatComposerState {
    store
        .composers
        .lock()
        .unwrap()
        .get(&task_id)
        .cloned()
        .unwrap_or_else(|| default_composer(&task_id))
}

#[tauri::command]
pub fn chat_get_runtime_snapshot(
    task_id: String,
    app: AppHandle,
    store: State<'_, ChatStore>,
) -> ChatRuntimeSnapshot {
    let conn = app
        .try_state::<LiliaStore>()
        .and_then(|store| store.conn().ok());
    crate::chat::state::chat_runtime_snapshot_with_persisted(conn.as_deref(), &store, &task_id)
}

#[tauri::command]
pub fn chat_ack_restored_rollback(task_id: String, app: AppHandle) -> Result<(), String> {
    let Some(store) = app.try_state::<LiliaStore>() else {
        return Ok(());
    };
    let conn = store.conn()?;
    clear_persisted_pending_rollback(&conn, &task_id)
}

#[tauri::command]
pub fn chat_set_composer_state(
    state: ChatComposerState,
    _app: AppHandle,
    store: State<'_, ChatStore>,
) {
    let payload = {
        let mut composers = store.composers.lock().unwrap();
        let previous = composers.get(&state.task_id);
        let payload = composer_runtime_settings_update_payload(previous, &state);
        composers.insert(state.task_id.clone(), state.clone());
        payload
    };
    let Some(payload) = payload else {
        return;
    };
    let mut attributes = composer_runtime_settings_update_attributes(&payload);
    let write_result = write_runner_stdin(&store, &state.task_id, payload);
    attach_stdin_delivery(&mut attributes, &write_result);
    if let Err(err) = write_result {
        eprintln!("[chat] runtime settings update failed: {err}");
    }
}

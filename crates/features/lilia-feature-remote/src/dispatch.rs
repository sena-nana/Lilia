//! Remote request dispatch, chat / process parsing and JSON projection.
//!
//! The host supplies product, terminal and AgentKit I/O through [`RemoteHost`].

use std::collections::BTreeMap;

use lilia_contracts::{
    ChatAttachment, PendingProjection, PendingProjectionStatus, ProductTask, ProductTaskStatus,
    TaskId, TimelineProjectionEvent,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::service::{
    now_millis, remote_capabilities, DesktopRemoteControlError,
};
use crate::types::{
    RemoteRequestEnvelope, REMOTE_MIN_PROTOCOL_VERSION, REMOTE_PROTOCOL_VERSION,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteSessionForkCommand {
    pub source_turn_id: String,
    pub mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteProcessSessionCommand {
    Spawn {
        command: String,
        environment: BTreeMap<String, String>,
        requested_cwd: Option<String>,
        rows: u16,
        columns: u16,
    },
    WriteStdin {
        process_id: Option<String>,
        input: String,
    },
    Kill {
        process_id: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteChatPermission {
    Full,
    Readonly,
    Ask,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteChatSpec {
    pub task_id: TaskId,
    pub content: String,
    pub attachments: Vec<ChatAttachment>,
    pub workspace_path: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub permission: RemoteChatPermission,
    pub plan_mode: bool,
    pub goal_mode: bool,
    pub session_fork: Option<RemoteSessionForkCommand>,
}

/// Host I/O for remote dispatch: product reads, AgentKit, terminals.
pub trait RemoteHost: Send + Sync {
    fn status(&self) -> Result<crate::types::RemoteControlStatus, DesktopRemoteControlError>;
    fn pair_device(
        &self,
        input: crate::types::RemotePairDeviceInput,
    ) -> Result<crate::types::RemotePeerSummary, DesktopRemoteControlError>;
    fn authorize(
        &self,
        device_id: &str,
        request_type: &str,
    ) -> Result<(), DesktopRemoteControlError>;
    fn resume_peer(
        &self,
        device_id: &str,
    ) -> Result<Option<crate::types::RemotePeerSummary>, DesktopRemoteControlError>;
    fn record_activity(&self) -> Result<(), DesktopRemoteControlError>;
    fn sync_wake(&self) -> Result<(), DesktopRemoteControlError>;
    fn list_tasks(&self) -> Result<Vec<(ProductTask, Option<String>)>, DesktopRemoteControlError>;
    fn load_task(
        &self,
        task_id: &TaskId,
    ) -> Result<(ProductTask, Option<String>), DesktopRemoteControlError>;
    fn task_runtime(&self, task_id: &TaskId) -> Result<Value, DesktopRemoteControlError>;
    fn timeline(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<TimelineProjectionEvent>, DesktopRemoteControlError>;
    fn open_session(
        &self,
        task_id: &TaskId,
    ) -> Result<(Value, Option<String>), DesktopRemoteControlError>;
    fn dispatch_wire(&self, envelope: Value) -> Result<Value, DesktopRemoteControlError>;
    fn start_chat(&self, spec: RemoteChatSpec) -> Result<Value, DesktopRemoteControlError>;
    fn interrupt(&self, task_id: &TaskId) -> Result<Value, DesktopRemoteControlError>;
    fn retry_event(
        &self,
        task_id: &TaskId,
        event_id: Option<&str>,
    ) -> Result<Value, DesktopRemoteControlError>;
    fn pending(
        &self,
        task_id: Option<&TaskId>,
    ) -> Result<Vec<PendingProjection>, DesktopRemoteControlError>;
    fn respond_approval(
        &self,
        task_id: &TaskId,
        request_id: &str,
        approved: bool,
    ) -> Result<(), DesktopRemoteControlError>;
    fn respond_interaction(
        &self,
        task_id: &TaskId,
        request_id: &str,
        accepted: bool,
        result: Value,
    ) -> Result<(), DesktopRemoteControlError>;
    fn respond_architecture(
        &self,
        task_id: &TaskId,
        request_id: &str,
        allow: bool,
    ) -> Result<(), DesktopRemoteControlError>;
    fn provider_status(&self) -> Result<Value, DesktopRemoteControlError>;
    fn live_process(&self, task_id: &TaskId) -> Result<Option<String>, DesktopRemoteControlError>;
    fn task_blocked(&self, task_id: &TaskId) -> Result<Option<String>, DesktopRemoteControlError>;
    fn resolve_process_cwd(
        &self,
        task_id: &TaskId,
        requested: Option<&str>,
    ) -> Result<(), DesktopRemoteControlError>;
    fn spawn_process(
        &self,
        task_id: &TaskId,
        command: String,
        environment: BTreeMap<String, String>,
        rows: u16,
        columns: u16,
    ) -> Result<Value, DesktopRemoteControlError>;
    fn write_process(
        &self,
        session_id: &str,
        input: &str,
    ) -> Result<Value, DesktopRemoteControlError>;
    fn kill_process(&self, session_id: &str) -> Result<Value, DesktopRemoteControlError>;
    fn remember_process(&self, task_id: TaskId, session_id: String);
    fn forget_process(&self, task_id: &TaskId);
}

pub fn dispatch_remote_request(host: &dyn RemoteHost, envelope: RemoteRequestEnvelope) -> Value {
    match dispatch_remote_payload(host, &envelope) {
        Ok(payload) => response_ok(&envelope, payload),
        Err(error) => response_error(&envelope, error),
    }
}

pub fn dispatch_remote_payload(
    host: &dyn RemoteHost,
    envelope: &RemoteRequestEnvelope,
) -> Result<Value, DesktopRemoteControlError> {
    if envelope.protocol_version < REMOTE_MIN_PROTOCOL_VERSION {
        return Err(DesktopRemoteControlError::unsupported(
            "remote protocol version is too old",
        ));
    }
    let request_type = string_field(&envelope.request, "type")?;
    host.authorize(&envelope.device_id, &request_type)?;
    match request_type.as_str() {
        "connection.capabilities.read" => Ok(json!({
            "type": "connection.capabilities",
            "capabilities": remote_capabilities(),
        })),
        "connection.resume" => {
            let peer = host.resume_peer(&envelope.device_id)?;
            if peer.is_some() {
                host.record_activity()?;
            } else {
                host.sync_wake()?;
            }
            Ok(json!({
                "type": "connection.resume",
                "accepted": peer.is_some(),
                "peer": peer,
            }))
        }
        "tasks.list" => {
            let limit = positive_limit(&envelope.request, 80, 200);
            Ok(json!({
                "type": "tasks.list",
                "tasks": remote_task_list(host, limit)?,
            }))
        }
        "tasks.get" => {
            let task_id = parse_task_id(&envelope.request)?;
            let (task, project_name) = host.load_task(&task_id)?;
            Ok(json!({
                "type": "tasks.get",
                "task": remote_task_detail(&task, project_name),
                "runtime": remote_task_runtime(host, &task_id)?,
            }))
        }
        "timeline.snapshot" => remote_timeline_snapshot(host, &envelope.request),
        "timeline.subscribe" => remote_timeline_subscribe(host, &envelope.request),
        "agent.session.open" => remote_session_open(host, &envelope.request),
        "agent.wire" => {
            let raw = envelope.request.get("envelope").cloned().ok_or_else(|| {
                DesktopRemoteControlError::invalid("agent wire envelope is missing")
            })?;
            let response = host.dispatch_wire(raw)?;
            Ok(json!({ "type": "agent.wire", "envelope": response }))
        }
        "chat.send" => remote_chat_send(host, &envelope.request),
        "chat.interrupt" => {
            let task_id = parse_task_id(&envelope.request)?;
            let result = host.interrupt(&task_id)?;
            Ok(json!({ "type": "chat.interrupt", "result": result }))
        }
        "chat.retry" => remote_chat_retry(host, &envelope.request),
        "interaction.pending.read" => {
            let task_id = optional_task_id(&envelope.request)?;
            let interactions = host
                .pending(task_id.as_ref())?
                .iter()
                .filter_map(remote_pending_interaction)
                .collect::<Vec<_>>();
            Ok(json!({ "type": "interaction.pending", "interactions": interactions }))
        }
        "interaction.respond" => remote_interaction_respond(host, &envelope.request),
        "provider.status.read" => host.provider_status(),
        _ => Err(DesktopRemoteControlError::unsupported(format!(
            "unsupported remote request: {request_type}"
        ))),
    }
}

pub fn remote_timeline_snapshot(
    host: &dyn RemoteHost,
    request: &Value,
) -> Result<Value, DesktopRemoteControlError> {
    let task_id = parse_task_id(request)?;
    let events = host.timeline(&task_id)?;
    let limit = positive_limit(request, events.len().max(1), 500);
    let direction = request
        .get("direction")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let (page, has_more_before, has_more_after) = if direction == "before" {
        let cursor = string_field(request, "cursor")?;
        let end = events
            .iter()
            .position(|event| event.id.as_str() == cursor)
            .unwrap_or(events.len());
        let start = end.saturating_sub(limit);
        (events[start..end].to_vec(), start > 0, end < events.len())
    } else {
        let start = events.len().saturating_sub(limit);
        (events[start..].to_vec(), start > 0, false)
    };
    let before_cursor = page.first().map(|event| event.id.as_str().to_owned());
    let after_cursor = page.last().map(|event| event.id.as_str().to_owned());
    Ok(json!({
        "type": "timeline.snapshot",
        "taskId": task_id.as_str(),
        "events": page,
        "page": {
            "beforeCursor": before_cursor,
            "afterCursor": after_cursor,
            "hasMoreBefore": has_more_before,
            "hasMoreAfter": has_more_after,
        },
    }))
}

fn remote_timeline_subscribe(
    host: &dyn RemoteHost,
    request: &Value,
) -> Result<Value, DesktopRemoteControlError> {
    let task_id = parse_task_id(request)?;
    let events = host.timeline(&task_id)?;
    let cursor = request
        .get("afterCursor")
        .or_else(|| request.get("afterEventId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let start = cursor
        .and_then(|cursor| {
            events
                .iter()
                .position(|event| event.id.as_str() == cursor)
                .map(|index| index + 1)
        })
        .unwrap_or(0);
    let limit = positive_limit(request, 500, 500);
    let page = events.into_iter().skip(start).take(limit).collect::<Vec<_>>();
    Ok(json!({
        "type": "timeline.subscribe",
        "taskId": task_id.as_str(),
        "events": page,
    }))
}

fn remote_session_open(
    host: &dyn RemoteHost,
    request: &Value,
) -> Result<Value, DesktopRemoteControlError> {
    let task_id = parse_task_id(request)?;
    let (session, workspace_path) = host.open_session(&task_id)?;
    let generation = u64::try_from(now_millis()).unwrap_or_default();
    let folders = workspace_path.into_iter().collect::<Vec<_>>();
    let workspace = json!({
        "workspace_id": format!("lilia.task:{}", task_id.as_str()),
        "folders": folders,
        "metadata": {
            "productTaskId": task_id.as_str(),
            "source": "lilia-android-remote",
        },
    });
    Ok(json!({
        "type": "agent.session.open",
        "taskId": task_id.as_str(),
        "session": session,
        "context": {
            "workspace": workspace,
            "editorContext": {
                "snapshot_id": format!("lilia:{}:remote:{generation}", task_id.as_str()),
                "workspace": workspace,
                "generation": generation,
                "active_document": null,
                "documents": [],
                "supports_workspace_edit_preview": false,
                "supports_workspace_edit_apply": false,
            },
        },
    }))
}

fn remote_task_list(
    host: &dyn RemoteHost,
    limit: usize,
) -> Result<Vec<Value>, DesktopRemoteControlError> {
    let mut tasks = host.list_tasks()?;
    tasks.sort_by(|(left, _), (right, _)| {
        task_sort_rank(left.status)
            .cmp(&task_sort_rank(right.status))
            .then_with(|| right.pinned.cmp(&left.pinned))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| right.created_at.cmp(&left.created_at))
    });
    Ok(tasks
        .into_iter()
        .take(limit)
        .map(|(task, project_name)| remote_task_summary(&task, project_name))
        .collect())
}

fn remote_task_runtime(
    host: &dyn RemoteHost,
    task_id: &TaskId,
) -> Result<Value, DesktopRemoteControlError> {
    let mut runtime = host.task_runtime(task_id)?;
    let process_session_id = host.live_process(task_id)?;
    let object = runtime.as_object_mut().ok_or_else(|| {
        DesktopRemoteControlError::internal("task runtime snapshot is not an object")
    })?;
    object.insert("processSessionId".to_owned(), json!(process_session_id));
    Ok(runtime)
}

fn remote_chat_send(
    host: &dyn RemoteHost,
    request: &Value,
) -> Result<Value, DesktopRemoteControlError> {
    let task_id = parse_task_id(request)?;
    if let Some(command) = remote_process_session_command(request)? {
        return remote_process_session(host, &task_id, request, command);
    }
    let session_fork = remote_session_fork_command(request)?;
    let content = string_field(request, "content")?;
    let attachments = request
        .get("attachments")
        .cloned()
        .filter(|value| !value.is_null())
        .map(serde_json::from_value::<Vec<ChatAttachment>>)
        .transpose()
        .map_err(|error| DesktopRemoteControlError::invalid(error.to_string()))?
        .unwrap_or_default();
    let composer = request.get("composer").cloned().unwrap_or(Value::Null);
    let runtime_options = request
        .get("runtimeOptions")
        .cloned()
        .unwrap_or(Value::Null);
    let spec = RemoteChatSpec {
        task_id,
        content,
        attachments,
        workspace_path: request
            .get("projectCwd")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        model: composer
            .get("model")
            .and_then(Value::as_str)
            .or_else(|| {
                runtime_options
                    .pointer("/common/model")
                    .and_then(Value::as_str)
            })
            .map(str::to_owned),
        reasoning_effort: composer
            .get("reasoningEffort")
            .and_then(Value::as_str)
            .or_else(|| {
                runtime_options
                    .pointer("/common/reasoningEffort")
                    .and_then(Value::as_str)
            })
            .map(str::to_owned),
        // Keep `free` on Ask so high-risk process/network tools are not auto-approved
        // via the remote composer path (mirrors native_runtime permission_mode).
        permission: match composer.get("permission").and_then(Value::as_str) {
            Some("full") => RemoteChatPermission::Full,
            Some("readonly") => RemoteChatPermission::Readonly,
            _ => RemoteChatPermission::Ask,
        },
        plan_mode: composer
            .get("planMode")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        goal_mode: composer
            .get("goalMode")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        session_fork,
    };
    host.start_chat(spec)
}

fn remote_process_session(
    host: &dyn RemoteHost,
    task_id: &TaskId,
    request: &Value,
    command: RemoteProcessSessionCommand,
) -> Result<Value, DesktopRemoteControlError> {
    let has_content = request
        .get("content")
        .and_then(Value::as_str)
        .is_some_and(|content| !content.trim().is_empty());
    let has_attachments = request
        .get("attachments")
        .and_then(Value::as_array)
        .is_some_and(|attachments| !attachments.is_empty());
    if has_content || has_attachments {
        return Err(DesktopRemoteControlError::invalid(
            "process session commands cannot also submit a chat message",
        ));
    }

    let existing = host.live_process(task_id)?;
    match command {
        RemoteProcessSessionCommand::Spawn {
            command,
            environment,
            requested_cwd,
            rows,
            columns,
        } => {
            if let Some(block) = host.task_blocked(task_id)? {
                return Err(DesktopRemoteControlError::new("conflict", block, false));
            }
            host.resolve_process_cwd(task_id, requested_cwd.as_deref())?;
            if existing.is_some() {
                return Err(DesktopRemoteControlError::new(
                    "conflict",
                    "task already has a running process session",
                    false,
                ));
            }
            let snapshot = host.spawn_process(task_id, command, environment, rows, columns)?;
            if let Some(session_id) = snapshot
                .get("processSessionId")
                .and_then(Value::as_str)
                .map(str::to_owned)
            {
                host.remember_process(task_id.clone(), session_id);
            }
            Ok(json!({
                "type": "chat.send",
                "result": { "accepted": true },
                "processSession": snapshot,
            }))
        }
        RemoteProcessSessionCommand::WriteStdin { process_id, input } => {
            let session_id = require_remote_process_session(existing, process_id.as_deref())?;
            let snapshot = host.write_process(&session_id, &input)?;
            Ok(json!({
                "type": "chat.send",
                "result": { "accepted": true },
                "processSession": snapshot,
            }))
        }
        RemoteProcessSessionCommand::Kill { process_id } => {
            let session_id = require_remote_process_session(existing, process_id.as_deref())?;
            let snapshot = host.kill_process(&session_id)?;
            host.forget_process(task_id);
            Ok(json!({
                "type": "chat.send",
                "result": { "accepted": true },
                "processSession": snapshot,
            }))
        }
    }
}

fn remote_chat_retry(
    host: &dyn RemoteHost,
    request: &Value,
) -> Result<Value, DesktopRemoteControlError> {
    let task_id = parse_task_id(request)?;
    let event_id = request.get("eventId").and_then(Value::as_str);
    let result = host.retry_event(&task_id, event_id)?;
    Ok(json!({ "type": "chat.retry", "result": result }))
}

pub fn remote_interaction_respond(
    host: &dyn RemoteHost,
    request: &Value,
) -> Result<Value, DesktopRemoteControlError> {
    let response = request
        .get("response")
        .ok_or_else(|| DesktopRemoteControlError::invalid("interaction response is missing"))?;
    let task_id = parse_task_id(response)?;
    let request_id = string_field(response, "requestId")?;
    let kind = string_field(response, "kind")?;
    let result = response
        .get("result")
        .cloned()
        .ok_or_else(|| DesktopRemoteControlError::invalid("interaction result is missing"))?;
    if kind == "permission_approval" {
        let approved = result.get("action").and_then(Value::as_str) == Some("approve");
        host.respond_approval(&task_id, &request_id, approved)?;
    } else if matches!(kind.as_str(), "ask_user" | "plan_approval") {
        let accepted = !result
            .get("cancelled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        host.respond_interaction(&task_id, &request_id, accepted, result)?;
    } else if kind == "mcp_elicitation" {
        let accepted = result.get("action").and_then(Value::as_str) == Some("accept");
        host.respond_interaction(&task_id, &request_id, accepted, result)?;
    } else if kind == "architecture_change" {
        let allow = match result.get("decision").and_then(Value::as_str) {
            Some("allow") => true,
            Some("deny") => false,
            _ => {
                return Err(DesktopRemoteControlError::invalid(
                    "architecture interaction decision must be allow or deny",
                ));
            }
        };
        host.respond_architecture(&task_id, &request_id, allow)?;
    } else {
        return Err(DesktopRemoteControlError::unsupported(format!(
            "unsupported remote interaction: {kind}"
        )));
    }
    Ok(json!({
        "type": "interaction.respond",
        "accepted": true,
        "backend": "native-agentkit",
    }))
}

pub fn remote_process_session_command(
    request: &Value,
) -> Result<Option<RemoteProcessSessionCommand>, DesktopRemoteControlError> {
    let Some(command) = request
        .get("runtimeCommand")
        .filter(|value| !value.is_null())
    else {
        return Ok(None);
    };
    if command.get("type").and_then(Value::as_str) != Some("process_session") {
        return Ok(None);
    }
    let action = string_field(command, "action")?;
    let process_id = command
        .get("processId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    match action.as_str() {
        "spawn" => {
            let command_text = string_field(command, "command")?.to_owned();
            if command_text.chars().any(char::is_control) {
                return Err(DesktopRemoteControlError::invalid(
                    "process session command must be a single line",
                ));
            }
            if command.get("tty").and_then(Value::as_bool) == Some(false) {
                return Err(DesktopRemoteControlError::unsupported(
                    "Native process sessions always use a PTY",
                ));
            }
            if let Some(permission) = command
                .get("permissionProfile")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if permission != ":workspace" {
                    return Err(DesktopRemoteControlError::unsupported(
                        "Native process sessions only support the workspace permission profile",
                    ));
                }
            }
            let environment = command
                .get("env")
                .filter(|value| !value.is_null())
                .map(remote_process_environment)
                .transpose()?
                .unwrap_or_default();
            Ok(Some(RemoteProcessSessionCommand::Spawn {
                command: command_text,
                environment,
                requested_cwd: command
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
                rows: remote_terminal_dimension(command, "rows", 24)?,
                columns: remote_terminal_dimension(command, "cols", 80)?,
            }))
        }
        "write_stdin" => {
            let input = command
                .get("stdin")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    DesktopRemoteControlError::invalid("process session stdin is missing")
                })?
                .to_owned();
            Ok(Some(RemoteProcessSessionCommand::WriteStdin {
                process_id,
                input,
            }))
        }
        "kill" => Ok(Some(RemoteProcessSessionCommand::Kill { process_id })),
        _ => Err(DesktopRemoteControlError::unsupported(format!(
            "unsupported process session action: {action}"
        ))),
    }
}

fn remote_process_environment(
    value: &Value,
) -> Result<BTreeMap<String, String>, DesktopRemoteControlError> {
    let object = value.as_object().ok_or_else(|| {
        DesktopRemoteControlError::invalid("process session env must be an object")
    })?;
    object
        .iter()
        .map(|(key, value)| {
            let value = value.as_str().ok_or_else(|| {
                DesktopRemoteControlError::invalid(format!(
                    "process session env `{key}` must be a string"
                ))
            })?;
            Ok((key.clone(), value.to_owned()))
        })
        .collect()
}

fn remote_terminal_dimension(
    command: &Value,
    field: &'static str,
    default: u16,
) -> Result<u16, DesktopRemoteControlError> {
    command
        .get(field)
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| u16::try_from(value).ok())
                .ok_or_else(|| {
                    DesktopRemoteControlError::invalid(format!(
                        "process session {field} must be an unsigned 16-bit integer"
                    ))
                })
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn require_remote_process_session(
    existing: Option<String>,
    requested: Option<&str>,
) -> Result<String, DesktopRemoteControlError> {
    let existing = existing.ok_or_else(|| {
        DesktopRemoteControlError::new(
            "conflict",
            "task does not have a running process session",
            false,
        )
    })?;
    if requested.is_some_and(|requested| requested != existing) {
        return Err(DesktopRemoteControlError::new(
            "conflict",
            "process session id does not match the task's active session",
            false,
        ));
    }
    Ok(existing)
}

pub fn remote_session_fork_command(
    request: &Value,
) -> Result<Option<RemoteSessionForkCommand>, DesktopRemoteControlError> {
    let Some(command) = request
        .get("runtimeCommand")
        .filter(|value| !value.is_null())
    else {
        return Ok(None);
    };
    let kind = string_field(command, "type")?;
    if kind != "session_fork" {
        return Err(DesktopRemoteControlError::unsupported(format!(
            "unsupported Native remote runtime command: {kind}"
        )));
    }
    if !command
        .get("excludeTurns")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        return Err(DesktopRemoteControlError::unsupported(
            "Native session fork requires excluding turns after the selected turn",
        ));
    }
    let source_turn_id = string_field(command, "sourceTurnId")?;
    let mode = command
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("fork")
        .trim();
    if !matches!(mode, "continue" | "fork") {
        return Err(DesktopRemoteControlError::invalid(
            "session fork mode must be `continue` or `fork`",
        ));
    }
    Ok(Some(RemoteSessionForkCommand {
        source_turn_id,
        mode: mode.to_owned(),
    }))
}

fn remote_task_summary(task: &ProductTask, project_name: Option<String>) -> Value {
    json!({
        "taskId": task.id.as_str(),
        "projectId": task.project_id.as_ref().map(|id| id.as_str()),
        "projectName": project_name,
        "title": task.title,
        "status": task_status(task.status),
        "dependsOn": task.depends_on.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "createdAt": task.created_at,
        "pinned": task.pinned,
        "route": task.project_id.as_ref()
            .map(|project_id| format!("/projects/{}/tasks/{}", project_id.as_str(), task.id.as_str()))
            .unwrap_or_else(|| format!("/chats/{}", task.id.as_str())),
    })
}

fn remote_task_detail(task: &ProductTask, project_name: Option<String>) -> Value {
    let mut value = remote_task_summary(task, project_name);
    if let Some(object) = value.as_object_mut() {
        object.insert("id".to_owned(), json!(task.id.as_str()));
        object.insert(
            "parentId".to_owned(),
            json!(task.parent_id.as_ref().map(|id| id.as_str())),
        );
    }
    value
}

fn task_status(status: ProductTaskStatus) -> &'static str {
    match status {
        ProductTaskStatus::Draft => "draft",
        ProductTaskStatus::Waiting => "waiting",
        ProductTaskStatus::Running => "running",
        ProductTaskStatus::Blocked => "blocked",
        ProductTaskStatus::Done => "done",
        ProductTaskStatus::Cancelled => "cancelled",
    }
}

fn task_sort_rank(status: ProductTaskStatus) -> u8 {
    match status {
        ProductTaskStatus::Running => 0,
        ProductTaskStatus::Blocked => 1,
        ProductTaskStatus::Waiting => 2,
        ProductTaskStatus::Draft => 3,
        ProductTaskStatus::Done => 4,
        ProductTaskStatus::Cancelled => 5,
    }
}

fn remote_pending_interaction(pending: &PendingProjection) -> Option<Value> {
    if pending.status != PendingProjectionStatus::Open {
        return None;
    }
    let payload = match pending.kind.as_str() {
        "ask_user" | "plan_approval" => pending
            .payload
            .get("spec")
            .cloned()
            .unwrap_or_else(|| pending.payload.clone()),
        "permission_approval" => json!({
            "title": "Native 审批",
            "body": pending.prompt,
            "toolName": pending.payload.get("tool"),
            "requestedAccess": {
                "tool": pending.payload.get("tool"),
                "sideEffect": pending.payload.get("sideEffect"),
            },
            "scopeSuggestion": "turn",
            "providerContext": pending.payload.get("providerContext"),
        }),
        _ => pending.payload.clone(),
    };
    Some(json!({
        "taskId": pending.task_id.as_str(),
        "turnId": pending.turn_id,
        "backend": "native-agentkit",
        "requestId": pending.request_id,
        "kind": pending.kind,
        "payload": payload,
    }))
}

pub fn response_ok(envelope: &RemoteRequestEnvelope, payload: Value) -> Value {
    json!({
        "id": format!("remote-response-{}", Uuid::new_v4()),
        "requestId": envelope.id,
        "protocolVersion": REMOTE_PROTOCOL_VERSION,
        "sentAt": now_millis(),
        "ok": true,
        "payload": payload,
    })
}

pub fn response_error(envelope: &RemoteRequestEnvelope, error: DesktopRemoteControlError) -> Value {
    json!({
        "id": format!("remote-response-{}", Uuid::new_v4()),
        "requestId": envelope.id,
        "protocolVersion": REMOTE_PROTOCOL_VERSION,
        "sentAt": now_millis(),
        "ok": false,
        "error": {
            "code": error.code,
            "message": error.message,
            "retryable": error.retryable,
        },
    })
}

fn string_field(value: &Value, key: &str) -> Result<String, DesktopRemoteControlError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| DesktopRemoteControlError::invalid(format!("{key} is missing")))
}

fn parse_task_id(value: &Value) -> Result<TaskId, DesktopRemoteControlError> {
    TaskId::new(string_field(value, "taskId")?)
        .map_err(|error| DesktopRemoteControlError::invalid(error.to_string()))
}

fn optional_task_id(value: &Value) -> Result<Option<TaskId>, DesktopRemoteControlError> {
    value
        .get("taskId")
        .and_then(Value::as_str)
        .map(|value| {
            TaskId::new(value.to_owned())
                .map_err(|error| DesktopRemoteControlError::invalid(error.to_string()))
        })
        .transpose()
}

fn positive_limit(value: &Value, default: usize, maximum: usize) -> usize {
    value
        .get("limit")
        .and_then(Value::as_u64)
        .filter(|limit| *limit > 0)
        .and_then(|limit| usize::try_from(limit).ok())
        .map(|limit| limit.min(maximum))
        .unwrap_or(default.min(maximum))
}

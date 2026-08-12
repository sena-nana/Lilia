use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lilia_contracts::{
    ChatAttachment, PendingProjection, PendingProjectionStatus, ProductTask, ProductTaskStatus,
    TaskId, TimelineProjectionEvent,
};
use mutsuki_agent_contracts::AgentWireRequestEnvelope;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::application::DesktopApplicationInner;
use crate::{
    DesktopApplication, DesktopApplicationError, DesktopArchitectureInteractionDecision,
    DesktopExecutionPermission, DesktopHost, DesktopHostAction, DesktopHostContext,
    DesktopTurnRequest, ProjectQuery, TaskQuery,
};

pub const REMOTE_PROTOCOL_VERSION: i64 = 1;
pub const REMOTE_MIN_PROTOCOL_VERSION: i64 = 1;
pub const REMOTE_ALPN: &str = "lilia.remote-control.v1";

const HOST_ENABLED_KEY: &str = "host_enabled";
const PC_NAME_KEY: &str = "pc_name";
const ENDPOINT_ID_KEY: &str = "endpoint_id";
const KEEP_AWAKE_ENABLED_KEY: &str = "keep_awake_enabled";
const PAIRING_TTL_MS: i64 = 10 * 60 * 1000;
const RECENT_ANDROID_SEEN_MS: i64 = 2 * 60 * 1000;
const DEFAULT_HTTP_BRIDGE_PORT: u16 = 41478;
const REMOTE_WAKE_MONITOR_IDLE_MS: u64 = 30_000;
const MAX_HTTP_REQUEST_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEndpointAddress {
    pub endpoint_id: String,
    #[serde(default)]
    pub relay_url: Option<String>,
    #[serde(default)]
    pub direct_addresses: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCapabilitySet {
    pub protocol_version: i64,
    pub min_protocol_version: i64,
    pub alpn: String,
    pub supports_pairing: bool,
    pub supports_task_inbox: bool,
    pub supports_timeline_subscription: bool,
    pub supports_timeline_pagination: bool,
    pub supports_chat_send: bool,
    pub supports_interaction_response: bool,
    pub supports_interrupt: bool,
    pub supports_agent_wire: bool,
    pub supports_session_fork: bool,
    pub supports_process_session: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemoteSessionForkCommand {
    source_turn_id: String,
    mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePeerSummary {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub endpoint_id: String,
    pub protocol_version: i64,
    pub trusted: bool,
    pub first_paired_at: i64,
    pub last_seen_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePairingTicket {
    pub id: String,
    pub pc_name: String,
    pub pc_endpoint: RemoteEndpointAddress,
    pub protocol_version: i64,
    pub challenge: String,
    pub expires_at: i64,
    pub pairing_uri: String,
    pub bridge_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteControlStatus {
    pub host_enabled: bool,
    pub state: String,
    pub pc_name: String,
    pub keep_awake_enabled: bool,
    pub endpoint: Option<RemoteEndpointAddress>,
    pub active_ticket: Option<RemotePairingTicket>,
    pub trusted_devices: Vec<RemotePeerSummary>,
    pub capabilities: RemoteCapabilitySet,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePairDeviceInput {
    pub ticket_id: String,
    pub challenge: String,
    pub device_name: String,
    pub android_endpoint: RemoteEndpointAddress,
    pub protocol_version: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRequestEnvelope {
    pub id: String,
    pub protocol_version: i64,
    #[serde(default)]
    pub sent_at: Option<i64>,
    pub device_id: String,
    pub request: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct DesktopRemoteControlError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl DesktopRemoteControlError {
    fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalidRequest", message, false)
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self::new("unauthorized", message, false)
    }

    fn unsupported(message: impl Into<String>) -> Self {
        Self::new("unsupported", message, false)
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self::new("unavailable", message, true)
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message, false)
    }
}

impl From<DesktopApplicationError> for DesktopRemoteControlError {
    fn from(value: DesktopApplicationError) -> Self {
        Self::unavailable(value.to_string())
    }
}

#[derive(Clone)]
pub struct DesktopRemoteControlService {
    inner: Arc<RemoteServiceInner>,
}

struct RemoteServiceInner {
    connection: Mutex<Connection>,
    bridge: Mutex<Option<RemoteHttpBridge>>,
    wake: Arc<RemoteWakeController>,
}

#[derive(Clone, Copy, Debug)]
struct RemoteHttpBridge {
    port: u16,
}

struct RemoteWakeController {
    state: Mutex<RemoteWakeRuntime>,
    changed: Condvar,
    host: Arc<dyn DesktopHost>,
    context: DesktopHostContext,
}

#[derive(Clone, Debug)]
struct RemoteWakeRuntime {
    configured: bool,
    active_until_ms: Option<i64>,
    platform_active: bool,
}

impl DesktopRemoteControlService {
    pub fn open(
        path: impl AsRef<Path>,
        host: Arc<dyn DesktopHost>,
        context: DesktopHostContext,
    ) -> Result<Self, DesktopRemoteControlError> {
        let connection = Connection::open(path)
            .map_err(|error| DesktopRemoteControlError::internal(error.to_string()))?;
        Self::from_connection(connection, host, context)
    }

    pub fn in_memory(
        host: Arc<dyn DesktopHost>,
        context: DesktopHostContext,
    ) -> Result<Self, DesktopRemoteControlError> {
        let connection = Connection::open_in_memory()
            .map_err(|error| DesktopRemoteControlError::internal(error.to_string()))?;
        Self::from_connection(connection, host, context)
    }

    fn from_connection(
        connection: Connection,
        host: Arc<dyn DesktopHost>,
        context: DesktopHostContext,
    ) -> Result<Self, DesktopRemoteControlError> {
        initialize_schema(&connection)?;
        let wake = Arc::new(RemoteWakeController {
            state: Mutex::new(RemoteWakeRuntime {
                configured: false,
                active_until_ms: None,
                platform_active: false,
            }),
            changed: Condvar::new(),
            host,
            context,
        });
        let monitor = wake.clone();
        thread::Builder::new()
            .name("lilia-remote-wake".to_owned())
            .spawn(move || remote_wake_monitor(monitor))
            .map_err(|error| DesktopRemoteControlError::internal(error.to_string()))?;
        Ok(Self {
            inner: Arc::new(RemoteServiceInner {
                connection: Mutex::new(connection),
                bridge: Mutex::new(None),
                wake,
            }),
        })
    }

    fn with_connection<T>(
        &self,
        action: impl FnOnce(&Connection) -> Result<T, DesktopRemoteControlError>,
    ) -> Result<T, DesktopRemoteControlError> {
        let connection =
            self.inner.connection.lock().map_err(|_| {
                DesktopRemoteControlError::internal("remote database lock poisoned")
            })?;
        action(&connection)
    }

    fn bridge_url(&self) -> Result<Option<String>, DesktopRemoteControlError> {
        Ok(self
            .inner
            .bridge
            .lock()
            .map_err(|_| DesktopRemoteControlError::internal("remote bridge lock poisoned"))?
            .as_ref()
            .map(|bridge| advertised_bridge_url(bridge.port)))
    }

    fn sync_wake(&self) -> Result<(), DesktopRemoteControlError> {
        let (configured, active) = self.with_connection(|connection| {
            let configured = host_enabled(connection)? && keep_awake_enabled(connection)?;
            let active = configured && has_recent_connected_device(connection)?;
            Ok((configured, active))
        })?;
        self.inner.wake.set_target(
            configured,
            active.then(|| now_millis() + RECENT_ANDROID_SEEN_MS),
        );
        Ok(())
    }

    fn record_activity(&self) -> Result<(), DesktopRemoteControlError> {
        let configured = self.with_connection(|connection| {
            Ok(host_enabled(connection)? && keep_awake_enabled(connection)?)
        })?;
        self.inner.wake.set_target(
            configured,
            configured.then(|| now_millis() + RECENT_ANDROID_SEEN_MS),
        );
        Ok(())
    }
}

impl RemoteWakeController {
    fn set_target(&self, configured: bool, active_until_ms: Option<i64>) {
        if let Ok(mut state) = self.state.lock() {
            state.configured = configured;
            state.active_until_ms = configured.then_some(active_until_ms).flatten();
            self.changed.notify_one();
        }
    }
}

fn remote_wake_monitor(controller: Arc<RemoteWakeController>) {
    loop {
        let mut state = match controller.state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        let now = now_millis();
        if state
            .active_until_ms
            .is_some_and(|active_until| active_until <= now)
        {
            state.active_until_ms = None;
        }
        let target = state.configured
            && state
                .active_until_ms
                .is_some_and(|active_until| active_until > now);
        if state.platform_active != target {
            let result = controller.host.execute(
                &controller.context,
                DesktopHostAction::SetSystemAwake {
                    active: target,
                    reason: "remote_control".to_owned(),
                },
            );
            if result.is_ok() {
                state.platform_active = target;
            }
        }
        let wait = state
            .active_until_ms
            .filter(|active_until| *active_until > now)
            .map(|active_until| Duration::from_millis((active_until - now) as u64))
            .unwrap_or_else(|| Duration::from_millis(REMOTE_WAKE_MONITOR_IDLE_MS));
        match controller.changed.wait_timeout(state, wait) {
            Ok(_) => {}
            Err(_) => return,
        }
    }
}

impl DesktopApplication {
    pub fn remote_control_status(&self) -> Result<RemoteControlStatus, DesktopRemoteControlError> {
        let enabled = self.inner.remote.with_connection(host_enabled)?;
        let bridge_url = if enabled {
            Some(self.ensure_remote_http_bridge()?)
        } else {
            None
        };
        self.inner
            .remote
            .with_connection(|connection| remote_status(connection, bridge_url.as_deref()))
    }

    pub fn restore_remote_control(&self) -> Result<RemoteControlStatus, DesktopRemoteControlError> {
        let status = self.remote_control_status()?;
        self.inner.remote.sync_wake()?;
        Ok(status)
    }

    pub fn set_remote_control_enabled(
        &self,
        enabled: bool,
    ) -> Result<RemoteControlStatus, DesktopRemoteControlError> {
        let bridge_url = enabled
            .then(|| self.ensure_remote_http_bridge())
            .transpose()?;
        self.inner.remote.with_connection(|connection| {
            set_setting(
                connection,
                HOST_ENABLED_KEY,
                if enabled { "true" } else { "false" },
            )?;
            let _ = endpoint_id(connection)?;
            if !enabled {
                cancel_pairing(connection)?;
            }
            remote_status(connection, bridge_url.as_deref())
        })?;
        self.inner.remote.sync_wake()?;
        self.remote_control_status()
    }

    pub fn set_remote_control_pc_name(
        &self,
        name: impl Into<String>,
    ) -> Result<RemoteControlStatus, DesktopRemoteControlError> {
        let name = name.into();
        let bridge_url = self.inner.remote.bridge_url()?;
        self.inner.remote.with_connection(|connection| {
            let name = name.trim();
            set_setting(
                connection,
                PC_NAME_KEY,
                if name.is_empty() {
                    "Lilia 电脑"
                } else {
                    name
                },
            )?;
            remote_status(connection, bridge_url.as_deref())
        })
    }

    pub fn set_remote_control_keep_awake(
        &self,
        enabled: bool,
    ) -> Result<RemoteControlStatus, DesktopRemoteControlError> {
        self.inner.remote.with_connection(|connection| {
            set_setting(
                connection,
                KEEP_AWAKE_ENABLED_KEY,
                if enabled { "true" } else { "false" },
            )
        })?;
        self.inner.remote.sync_wake()?;
        self.remote_control_status()
    }

    pub fn start_remote_pairing(&self) -> Result<RemotePairingTicket, DesktopRemoteControlError> {
        let bridge_url = self.ensure_remote_http_bridge()?;
        self.inner.remote.with_connection(|connection| {
            set_setting(connection, HOST_ENABLED_KEY, "true")?;
            cancel_pairing(connection)?;
            let name = pc_name(connection)?;
            let endpoint = endpoint(connection)?;
            let id = Uuid::new_v4().to_string();
            let challenge = Uuid::new_v4().to_string();
            let expires_at = now_millis() + PAIRING_TTL_MS;
            let pairing_uri = format!(
                "lilia-remote://pair?v={}&ticket={}&challenge={}&endpoint={}&name={}&bridge={}",
                REMOTE_PROTOCOL_VERSION,
                url_encode(&id),
                url_encode(&challenge),
                url_encode(&endpoint.endpoint_id),
                url_encode(&name),
                url_encode(&bridge_url),
            );
            connection
                .execute(
                    r#"INSERT INTO remote_control_pairing_tickets
                       (id, challenge, pc_name, endpoint_id, pairing_uri, expires_at, consumed_at, created_at)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)"#,
                    params![
                        id,
                        challenge,
                        name,
                        endpoint.endpoint_id,
                        pairing_uri,
                        expires_at,
                        now_millis()
                    ],
                )
                .map_err(database_error)?;
            active_ticket(connection)?.ok_or_else(|| {
                DesktopRemoteControlError::internal("pairing ticket was not persisted")
            })
        })
    }

    pub fn cancel_remote_pairing(&self) -> Result<(), DesktopRemoteControlError> {
        self.inner.remote.with_connection(cancel_pairing)
    }

    pub fn pair_remote_device(
        &self,
        input: RemotePairDeviceInput,
    ) -> Result<RemotePeerSummary, DesktopRemoteControlError> {
        let peer = self
            .inner
            .remote
            .with_connection(|connection| pair_device(connection, input))?;
        self.inner.remote.record_activity()?;
        Ok(peer)
    }

    pub fn revoke_remote_device(
        &self,
        device_id: &str,
    ) -> Result<RemoteControlStatus, DesktopRemoteControlError> {
        self.inner.remote.with_connection(|connection| {
            connection
                .execute(
                    r#"UPDATE remote_control_trusted_devices
                       SET trusted = 0, revoked_at = ?1 WHERE id = ?2"#,
                    params![now_millis(), device_id],
                )
                .map_err(database_error)?;
            Ok(())
        })?;
        self.inner.remote.sync_wake()?;
        self.remote_control_status()
    }

    pub fn dispatch_remote_request(&self, envelope: RemoteRequestEnvelope) -> Value {
        match self.dispatch_remote_payload(&envelope) {
            Ok(payload) => response_ok(&envelope, payload),
            Err(error) => response_error(&envelope, error),
        }
    }

    fn ensure_remote_http_bridge(&self) -> Result<String, DesktopRemoteControlError> {
        let mut bridge = self
            .inner
            .remote
            .inner
            .bridge
            .lock()
            .map_err(|_| DesktopRemoteControlError::internal("remote bridge lock poisoned"))?;
        if let Some(existing) = bridge.as_ref() {
            return Ok(advertised_bridge_url(existing.port));
        }
        let listener = TcpListener::bind(("0.0.0.0", DEFAULT_HTTP_BRIDGE_PORT))
            .or_else(|_| TcpListener::bind("0.0.0.0:0"))
            .map_err(|error| DesktopRemoteControlError::unavailable(error.to_string()))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| DesktopRemoteControlError::unavailable(error.to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|error| DesktopRemoteControlError::unavailable(error.to_string()))?
            .port();
        let application = Arc::downgrade(&self.inner);
        thread::Builder::new()
            .name("lilia-remote-http".to_owned())
            .spawn(move || serve_http_bridge(listener, application))
            .map_err(|error| DesktopRemoteControlError::unavailable(error.to_string()))?;
        *bridge = Some(RemoteHttpBridge { port });
        Ok(advertised_bridge_url(port))
    }

    fn dispatch_remote_payload(
        &self,
        envelope: &RemoteRequestEnvelope,
    ) -> Result<Value, DesktopRemoteControlError> {
        if envelope.protocol_version < REMOTE_MIN_PROTOCOL_VERSION {
            return Err(DesktopRemoteControlError::unsupported(
                "remote protocol version is too old",
            ));
        }
        let request_type = string_field(&envelope.request, "type")?;
        self.authorize_remote_request(envelope, &request_type)?;
        match request_type.as_str() {
            "connection.capabilities.read" => Ok(json!({
                "type": "connection.capabilities",
                "capabilities": remote_capabilities(),
            })),
            "connection.resume" => {
                let peer = self.inner.remote.with_connection(|connection| {
                    refresh_trusted_peer_seen(connection, &envelope.device_id)
                })?;
                if peer.is_some() {
                    self.inner.remote.record_activity()?;
                } else {
                    self.inner.remote.sync_wake()?;
                }
                Ok(json!({
                    "type": "connection.resume",
                    "accepted": peer.is_some(),
                    "peer": peer,
                }))
            }
            "tasks.list" => {
                let limit = positive_limit(&envelope.request, 80, 200);
                Ok(json!({ "type": "tasks.list", "tasks": self.remote_task_list(limit)? }))
            }
            "tasks.get" => {
                let task_id = parse_task_id(&envelope.request)?;
                let task = self.get_task(&task_id)?;
                let project_name = task
                    .project_id
                    .as_ref()
                    .and_then(|project_id| self.get_project(project_id).ok())
                    .map(|project| project.name);
                Ok(json!({
                    "type": "tasks.get",
                    "task": remote_task_detail(&task, project_name),
                    "runtime": self.task_runtime_snapshot(&task_id),
                }))
            }
            "timeline.snapshot" => self.remote_timeline_snapshot(&envelope.request),
            "timeline.subscribe" => self.remote_timeline_subscribe(&envelope.request),
            "agent.session.open" => {
                let task_id = parse_task_id(&envelope.request)?;
                let task = self.get_task(&task_id)?;
                let session = self.open_task_agent_wire_session(&task_id)?;
                let generation = u64::try_from(now_millis()).unwrap_or_default();
                let folders = task
                    .project_id
                    .as_ref()
                    .and_then(|project_id| self.get_project(project_id).ok())
                    .and_then(|project| project.workspace_path)
                    .into_iter()
                    .collect::<Vec<_>>();
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
            "agent.wire" => {
                let raw = envelope.request.get("envelope").cloned().ok_or_else(|| {
                    DesktopRemoteControlError::invalid("agent wire envelope is missing")
                })?;
                let request: AgentWireRequestEnvelope = serde_json::from_value(raw)
                    .map_err(|error| DesktopRemoteControlError::invalid(error.to_string()))?;
                let response = self.dispatch_agent_wire(request).map_err(|error| {
                    DesktopRemoteControlError::new(error.code, error.message, error.retryable)
                })?;
                Ok(json!({ "type": "agent.wire", "envelope": response }))
            }
            "chat.send" => self.remote_chat_send(&envelope.request),
            "chat.interrupt" => {
                let task_id = parse_task_id(&envelope.request)?;
                let result = self
                    .interrupt_task_turn(&task_id)
                    .or_else(|_| self.interrupt_projected_task_turn(&task_id))?;
                Ok(json!({ "type": "chat.interrupt", "result": result }))
            }
            "chat.retry" => self.remote_chat_retry(&envelope.request),
            "interaction.pending.read" => {
                let interactions = if let Some(task_id) = optional_task_id(&envelope.request)? {
                    self.task_session_snapshot(&task_id)?
                        .pending
                        .iter()
                        .filter_map(remote_pending_interaction)
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                Ok(json!({ "type": "interaction.pending", "interactions": interactions }))
            }
            "interaction.respond" => self.remote_interaction_respond(&envelope.request),
            "provider.status.read" => {
                let provider = self.provider_snapshot();
                let ready = provider.runtime.runtime_ready
                    && provider.credentials.iter().any(|credential| {
                        credential.status == crate::DesktopCredentialStatus::Active
                    });
                Ok(json!({
                    "type": "provider.status",
                    "backend": provider.runtime.backend,
                    "ready": ready,
                    "report": {
                        "brokerReady": provider.broker_ready,
                        "brokerDegraded": provider.broker_degraded,
                    },
                }))
            }
            _ => Err(DesktopRemoteControlError::unsupported(format!(
                "unsupported remote request: {request_type}"
            ))),
        }
    }

    fn authorize_remote_request(
        &self,
        envelope: &RemoteRequestEnvelope,
        request_type: &str,
    ) -> Result<(), DesktopRemoteControlError> {
        if request_type == "connection.capabilities.read" {
            return Ok(());
        }
        self.inner.remote.with_connection(|connection| {
            if !host_enabled(connection)? {
                return Err(DesktopRemoteControlError::new(
                    "unavailable",
                    "remote control host is disabled",
                    false,
                ));
            }
            if request_type.starts_with("connection.") {
                return Ok(());
            }
            let trusted = connection
                .query_row(
                    r#"SELECT trusted FROM remote_control_trusted_devices
                       WHERE endpoint_id = ?1 AND revoked_at IS NULL"#,
                    params![envelope.device_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(database_error)?;
            if trusted != Some(1) {
                return Err(DesktopRemoteControlError::unauthorized(
                    "Android device is not paired or has been revoked",
                ));
            }
            connection
                .execute(
                    "UPDATE remote_control_trusted_devices SET last_seen_at = ?1 WHERE endpoint_id = ?2",
                    params![now_millis(), envelope.device_id],
                )
                .map_err(database_error)?;
            Ok(())
        })?;
        self.inner.remote.record_activity()
    }

    fn remote_task_list(&self, limit: usize) -> Result<Vec<Value>, DesktopRemoteControlError> {
        let projects = self
            .query_projects(ProjectQuery {
                include_archived: false,
            })?
            .into_iter()
            .map(|project| (project.id.as_str().to_owned(), project.name))
            .collect::<HashMap<_, _>>();
        let mut tasks = self.query_tasks(TaskQuery::default())?;
        tasks.sort_by(|left, right| {
            task_sort_rank(left.status)
                .cmp(&task_sort_rank(right.status))
                .then_with(|| right.pinned.cmp(&left.pinned))
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| right.created_at.cmp(&left.created_at))
        });
        Ok(tasks
            .into_iter()
            .take(limit)
            .map(|task| {
                let project_name = task
                    .project_id
                    .as_ref()
                    .and_then(|project_id| projects.get(project_id.as_str()))
                    .cloned();
                remote_task_summary(&task, project_name)
            })
            .collect())
    }

    fn remote_timeline_snapshot(
        &self,
        request: &Value,
    ) -> Result<Value, DesktopRemoteControlError> {
        let task_id = parse_task_id(request)?;
        let events = self.task_session_snapshot(&task_id)?.timeline;
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
        &self,
        request: &Value,
    ) -> Result<Value, DesktopRemoteControlError> {
        let task_id = parse_task_id(request)?;
        let events = self.task_session_snapshot(&task_id)?.timeline;
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
        let page = events
            .into_iter()
            .skip(start)
            .take(limit)
            .collect::<Vec<_>>();
        Ok(json!({
            "type": "timeline.subscribe",
            "taskId": task_id.as_str(),
            "events": page,
        }))
    }

    fn remote_chat_send(&self, request: &Value) -> Result<Value, DesktopRemoteControlError> {
        let task_id = parse_task_id(request)?;
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
        let mut turn = DesktopTurnRequest::new(task_id, content).with_attachments(attachments);
        turn.workspace_path = request
            .get("projectCwd")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        turn.model = composer
            .get("model")
            .and_then(Value::as_str)
            .or_else(|| {
                runtime_options
                    .pointer("/common/model")
                    .and_then(Value::as_str)
            })
            .map(str::to_owned);
        turn.reasoning_effort = composer
            .get("reasoningEffort")
            .and_then(Value::as_str)
            .or_else(|| {
                runtime_options
                    .pointer("/common/reasoningEffort")
                    .and_then(Value::as_str)
            })
            .map(str::to_owned);
        turn.permission = match composer.get("permission").and_then(Value::as_str) {
            Some("full" | "free") => DesktopExecutionPermission::Full,
            Some("readonly") => DesktopExecutionPermission::Readonly,
            _ => DesktopExecutionPermission::Ask,
        };
        turn.plan_mode = composer
            .get("planMode")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        turn.goal_mode = composer
            .get("goalMode")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let forked_session_id = session_fork
            .as_ref()
            .map(|command| {
                self.fork_task_agent_session_through_turn(&turn.task_id, &command.source_turn_id)
            })
            .transpose()?;
        let result = self.start_task_turn(turn)?;
        Ok(json!({
            "type": "chat.send",
            "result": result,
            "sessionFork": session_fork.zip(forked_session_id).map(|(command, session_id)| json!({
                "sessionId": session_id,
                "sourceTurnId": command.source_turn_id,
                "mode": command.mode,
            })),
        }))
    }

    fn remote_chat_retry(&self, request: &Value) -> Result<Value, DesktopRemoteControlError> {
        let task_id = parse_task_id(request)?;
        let events = self.task_session_snapshot(&task_id)?.timeline;
        let selected = request
            .get("eventId")
            .and_then(Value::as_str)
            .and_then(|event_id| events.iter().find(|event| event.id.as_str() == event_id));
        let error = selected.or_else(|| events.iter().rev().find(|event| event.kind == "error"));
        let context = error
            .and_then(|event| retry_context(event, &events))
            .ok_or_else(|| {
                DesktopRemoteControlError::new("conflict", "no retryable remote message", false)
            })?;
        let mut turn = DesktopTurnRequest::new(task_id, context.content);
        turn.attachments = context.attachments;
        let result = self.start_task_turn(turn)?;
        Ok(json!({ "type": "chat.retry", "result": result }))
    }

    fn remote_interaction_respond(
        &self,
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
            self.respond_task_approval(&task_id, &request_id, approved)
                .or_else(|_| {
                    self.respond_projected_task_approval(&task_id, &request_id, approved)
                })?;
        } else if matches!(kind.as_str(), "ask_user" | "plan_approval") {
            let accepted = !result
                .get("cancelled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            self.respond_task_interaction(&task_id, &request_id, accepted, result.clone())
                .or_else(|_| {
                    self.respond_projected_task_interaction(&task_id, &request_id, accepted, result)
                })?;
        } else if kind == "mcp_elicitation" {
            let accepted = result.get("action").and_then(Value::as_str) == Some("accept");
            self.respond_task_interaction(&task_id, &request_id, accepted, result.clone())
                .or_else(|_| {
                    self.respond_projected_task_interaction(&task_id, &request_id, accepted, result)
                })?;
        } else if kind == "architecture_change" {
            let decision = match result.get("decision").and_then(Value::as_str) {
                Some("allow") => DesktopArchitectureInteractionDecision::Allow,
                Some("deny") => DesktopArchitectureInteractionDecision::Deny,
                _ => {
                    return Err(DesktopRemoteControlError::invalid(
                        "architecture interaction decision must be allow or deny",
                    ));
                }
            };
            self.respond_task_architecture_interaction(&task_id, &request_id, decision)?;
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
}

fn initialize_schema(connection: &Connection) -> Result<(), DesktopRemoteControlError> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS remote_control_settings (
              key        TEXT PRIMARY KEY,
              value      TEXT NOT NULL,
              updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS remote_control_trusted_devices (
              id                TEXT PRIMARY KEY,
              display_name      TEXT NOT NULL,
              endpoint_id       TEXT NOT NULL UNIQUE,
              protocol_version  INTEGER NOT NULL,
              trusted           INTEGER NOT NULL DEFAULT 1 CHECK (trusted IN (0, 1)),
              first_paired_at   INTEGER NOT NULL,
              last_seen_at      INTEGER,
              revoked_at        INTEGER
            );
            CREATE TABLE IF NOT EXISTS remote_control_pairing_tickets (
              id             TEXT PRIMARY KEY,
              challenge      TEXT NOT NULL,
              pc_name        TEXT NOT NULL,
              endpoint_id    TEXT NOT NULL,
              pairing_uri    TEXT NOT NULL,
              expires_at     INTEGER NOT NULL,
              consumed_at    INTEGER,
              created_at     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_remote_control_pairing_tickets_active
              ON remote_control_pairing_tickets(expires_at, consumed_at);
            "#,
        )
        .map_err(database_error)
}

fn setting(
    connection: &Connection,
    key: &str,
) -> Result<Option<String>, DesktopRemoteControlError> {
    connection
        .query_row(
            "SELECT value FROM remote_control_settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(database_error)
}

fn set_setting(
    connection: &Connection,
    key: &str,
    value: &str,
) -> Result<(), DesktopRemoteControlError> {
    connection
        .execute(
            r#"INSERT INTO remote_control_settings (key, value, updated_at)
               VALUES (?1, ?2, ?3)
               ON CONFLICT(key) DO UPDATE SET
                 value = excluded.value,
                 updated_at = excluded.updated_at"#,
            params![key, value, now_millis()],
        )
        .map(|_| ())
        .map_err(database_error)
}

fn host_enabled(connection: &Connection) -> Result<bool, DesktopRemoteControlError> {
    Ok(setting(connection, HOST_ENABLED_KEY)?.as_deref() == Some("true"))
}

fn keep_awake_enabled(connection: &Connection) -> Result<bool, DesktopRemoteControlError> {
    Ok(setting(connection, KEEP_AWAKE_ENABLED_KEY)?.as_deref() != Some("false"))
}

fn pc_name(connection: &Connection) -> Result<String, DesktopRemoteControlError> {
    Ok(setting(connection, PC_NAME_KEY)?
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Lilia 电脑".to_owned()))
}

fn endpoint_id(connection: &Connection) -> Result<String, DesktopRemoteControlError> {
    if let Some(id) = setting(connection, ENDPOINT_ID_KEY)?
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        return Ok(id);
    }
    let id = format!("pc-{}", Uuid::new_v4());
    set_setting(connection, ENDPOINT_ID_KEY, &id)?;
    Ok(id)
}

fn endpoint(connection: &Connection) -> Result<RemoteEndpointAddress, DesktopRemoteControlError> {
    Ok(RemoteEndpointAddress {
        endpoint_id: endpoint_id(connection)?,
        relay_url: None,
        direct_addresses: Vec::new(),
    })
}

fn active_ticket(
    connection: &Connection,
) -> Result<Option<RemotePairingTicket>, DesktopRemoteControlError> {
    connection
        .query_row(
            r#"SELECT id, challenge, pc_name, endpoint_id, pairing_uri, expires_at
               FROM remote_control_pairing_tickets
               WHERE consumed_at IS NULL AND expires_at > ?1
               ORDER BY created_at DESC LIMIT 1"#,
            params![now_millis()],
            |row| {
                let pairing_uri: String = row.get(4)?;
                Ok(RemotePairingTicket {
                    id: row.get(0)?,
                    challenge: row.get(1)?,
                    pc_name: row.get(2)?,
                    pc_endpoint: RemoteEndpointAddress {
                        endpoint_id: row.get(3)?,
                        relay_url: None,
                        direct_addresses: Vec::new(),
                    },
                    protocol_version: REMOTE_PROTOCOL_VERSION,
                    expires_at: row.get(5)?,
                    bridge_url: bridge_url_from_pairing_uri(&pairing_uri),
                    pairing_uri,
                })
            },
        )
        .optional()
        .map_err(database_error)
}

fn trusted_devices(
    connection: &Connection,
) -> Result<Vec<RemotePeerSummary>, DesktopRemoteControlError> {
    let mut statement = connection
        .prepare(
            r#"SELECT id, display_name, endpoint_id, protocol_version, trusted,
                      first_paired_at, last_seen_at, revoked_at
               FROM remote_control_trusted_devices
               ORDER BY revoked_at IS NOT NULL ASC, last_seen_at DESC, first_paired_at DESC"#,
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([], peer_from_row)
        .map_err(database_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

fn peer_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RemotePeerSummary> {
    Ok(RemotePeerSummary {
        id: row.get(0)?,
        kind: "android".to_owned(),
        display_name: row.get(1)?,
        endpoint_id: row.get(2)?,
        protocol_version: row.get(3)?,
        trusted: row.get::<_, i64>(4)? != 0,
        first_paired_at: row.get(5)?,
        last_seen_at: row.get(6)?,
        revoked_at: row.get(7)?,
    })
}

fn remote_status(
    connection: &Connection,
    bridge_url: Option<&str>,
) -> Result<RemoteControlStatus, DesktopRemoteControlError> {
    let enabled = host_enabled(connection)?;
    let mut ticket = enabled
        .then(|| active_ticket(connection))
        .transpose()?
        .flatten();
    if let (Some(ticket), Some(bridge_url)) = (ticket.as_mut(), bridge_url) {
        ticket.bridge_url = Some(bridge_url.to_owned());
        ticket.pairing_uri = pairing_uri_with_bridge_url(&ticket.pairing_uri, bridge_url);
    }
    let connected = enabled && has_recent_connected_device(connection)?;
    let state = if !enabled {
        "disabled"
    } else if ticket.is_some() {
        "pairing"
    } else if connected {
        "connected"
    } else {
        "listening"
    };
    Ok(RemoteControlStatus {
        host_enabled: enabled,
        state: state.to_owned(),
        pc_name: pc_name(connection)?,
        keep_awake_enabled: keep_awake_enabled(connection)?,
        endpoint: enabled.then(|| endpoint(connection)).transpose()?,
        active_ticket: ticket,
        trusted_devices: trusted_devices(connection)?,
        capabilities: remote_capabilities(),
    })
}

fn remote_capabilities() -> RemoteCapabilitySet {
    RemoteCapabilitySet {
        protocol_version: REMOTE_PROTOCOL_VERSION,
        min_protocol_version: REMOTE_MIN_PROTOCOL_VERSION,
        alpn: REMOTE_ALPN.to_owned(),
        supports_pairing: true,
        supports_task_inbox: true,
        supports_timeline_subscription: true,
        supports_timeline_pagination: true,
        supports_chat_send: true,
        supports_interaction_response: true,
        supports_interrupt: true,
        supports_agent_wire: true,
        supports_session_fork: true,
        supports_process_session: false,
    }
}

fn remote_session_fork_command(
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

fn cancel_pairing(connection: &Connection) -> Result<(), DesktopRemoteControlError> {
    connection
        .execute(
            "UPDATE remote_control_pairing_tickets SET consumed_at = ?1 WHERE consumed_at IS NULL",
            params![now_millis()],
        )
        .map(|_| ())
        .map_err(database_error)
}

fn pair_device(
    connection: &Connection,
    input: RemotePairDeviceInput,
) -> Result<RemotePeerSummary, DesktopRemoteControlError> {
    if input.protocol_version < REMOTE_MIN_PROTOCOL_VERSION {
        return Err(DesktopRemoteControlError::unsupported(
            "Android protocol version is too old",
        ));
    }
    if !host_enabled(connection)? {
        return Err(DesktopRemoteControlError::unavailable(
            "remote control host is disabled",
        ));
    }
    let ticket = connection
        .query_row(
            r#"SELECT challenge, expires_at FROM remote_control_pairing_tickets
               WHERE id = ?1 AND consumed_at IS NULL"#,
            params![input.ticket_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| {
            DesktopRemoteControlError::invalid("pairing ticket is missing or consumed")
        })?;
    if ticket.1 <= now_millis() {
        return Err(DesktopRemoteControlError::invalid(
            "pairing ticket has expired",
        ));
    }
    if ticket.0 != input.challenge {
        return Err(DesktopRemoteControlError::invalid(
            "pairing challenge does not match",
        ));
    }
    let now = now_millis();
    let display_name = input.device_name.trim();
    let display_name = if display_name.is_empty() {
        "Android 设备"
    } else {
        display_name
    };
    connection
        .execute(
            r#"INSERT INTO remote_control_trusted_devices
               (id, display_name, endpoint_id, protocol_version, trusted, first_paired_at, last_seen_at, revoked_at)
               VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5, NULL)
               ON CONFLICT(endpoint_id) DO UPDATE SET
                 display_name = excluded.display_name,
                 protocol_version = excluded.protocol_version,
                 trusted = 1,
                 last_seen_at = excluded.last_seen_at,
                 revoked_at = NULL"#,
            params![
                format!("android-{}", Uuid::new_v4()),
                display_name,
                input.android_endpoint.endpoint_id,
                input.protocol_version,
                now
            ],
        )
        .map_err(database_error)?;
    connection
        .execute(
            "UPDATE remote_control_pairing_tickets SET consumed_at = ?1 WHERE id = ?2",
            params![now, input.ticket_id],
        )
        .map_err(database_error)?;
    peer_for_endpoint(connection, &input.android_endpoint.endpoint_id)?
        .ok_or_else(|| DesktopRemoteControlError::internal("paired device could not be reloaded"))
}

fn peer_for_endpoint(
    connection: &Connection,
    endpoint_id: &str,
) -> Result<Option<RemotePeerSummary>, DesktopRemoteControlError> {
    connection
        .query_row(
            r#"SELECT id, display_name, endpoint_id, protocol_version, trusted,
                      first_paired_at, last_seen_at, revoked_at
               FROM remote_control_trusted_devices WHERE endpoint_id = ?1"#,
            params![endpoint_id],
            peer_from_row,
        )
        .optional()
        .map_err(database_error)
}

fn refresh_trusted_peer_seen(
    connection: &Connection,
    endpoint_id: &str,
) -> Result<Option<RemotePeerSummary>, DesktopRemoteControlError> {
    connection
        .execute(
            r#"UPDATE remote_control_trusted_devices SET last_seen_at = ?1
               WHERE endpoint_id = ?2 AND trusted = 1 AND revoked_at IS NULL"#,
            params![now_millis(), endpoint_id],
        )
        .map_err(database_error)?;
    peer_for_endpoint(connection, endpoint_id)
        .map(|peer| peer.filter(|peer| peer.trusted && peer.revoked_at.is_none()))
}

fn has_recent_connected_device(connection: &Connection) -> Result<bool, DesktopRemoteControlError> {
    connection
        .query_row(
            r#"SELECT EXISTS(
                 SELECT 1 FROM remote_control_trusted_devices
                 WHERE trusted = 1 AND revoked_at IS NULL
                   AND last_seen_at IS NOT NULL AND last_seen_at >= ?1
               )"#,
            params![now_millis() - RECENT_ANDROID_SEEN_MS],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(database_error)
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

struct RetryContext {
    content: String,
    attachments: Vec<ChatAttachment>,
}

fn retry_context(
    error: &TimelineProjectionEvent,
    events: &[TimelineProjectionEvent],
) -> Option<RetryContext> {
    if error.kind != "error" {
        return None;
    }
    if let Some(context) = retry_context_value(error.payload.get("retryContext")) {
        return Some(context);
    }
    let turn_id = error.turn_id.as_deref()?;
    let source = events.iter().find(|event| {
        event.kind == "message"
            && event.turn_id.as_deref() == Some(turn_id)
            && event.payload.get("role").and_then(Value::as_str) == Some("user")
    })?;
    retry_context_value(Some(&source.payload))
}

fn retry_context_value(value: Option<&Value>) -> Option<RetryContext> {
    let value = value?;
    let content = value
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let attachments: Vec<ChatAttachment> = value
        .get("attachments")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    (!content.trim().is_empty() || !attachments.is_empty()).then_some(RetryContext {
        content,
        attachments,
    })
}

fn serve_http_bridge(listener: TcpListener, application: Weak<DesktopApplicationInner>) {
    loop {
        if application.strong_count() == 0 {
            return;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let Some(inner) = application.upgrade() else {
                    return;
                };
                let application = DesktopApplication { inner };
                let _ = thread::Builder::new()
                    .name("lilia-remote-request".to_owned())
                    .spawn(move || handle_http_stream(application, stream));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(40));
            }
            Err(_) => return,
        }
    }
}

fn handle_http_stream(application: DesktopApplication, mut stream: TcpStream) {
    let response = match read_http_request(&mut stream) {
        Ok(request) => handle_http_request(&application, request),
        Err(error) => http_json_response(400, http_error_payload("invalidRequest", error, false)),
    };
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("connection closed before request completed".to_owned());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break index;
        }
        if buffer.len() > 64 * 1024 {
            return Err("request headers are too large".to_owned());
        }
    };
    let header = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header.lines();
    let mut request_line = lines
        .next()
        .ok_or_else(|| "request line is missing".to_owned())?
        .split_whitespace();
    let method = request_line.next().unwrap_or_default().to_owned();
    let path = request_line.next().unwrap_or_default().to_owned();
    if method.is_empty() || path.is_empty() {
        return Err("request line is invalid".to_owned());
    }
    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find_map(|(name, value)| {
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    if content_length > MAX_HTTP_REQUEST_BYTES {
        return Err("request body is too large".to_owned());
    }
    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    if buffer.len() < body_start + content_length {
        return Err("request body is incomplete".to_owned());
    }
    Ok(HttpRequest {
        method,
        path,
        body: buffer[body_start..body_start + content_length].to_vec(),
    })
}

fn handle_http_request(application: &DesktopApplication, request: HttpRequest) -> String {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/status") => match application.remote_control_status() {
            Ok(status) => http_json_response(200, json!({ "ok": true, "status": status })),
            Err(error) => http_json_response(
                500,
                http_error_payload(&error.code, error.message, error.retryable),
            ),
        },
        ("POST", "/pair") => {
            let input = match serde_json::from_slice::<RemotePairDeviceInput>(&request.body) {
                Ok(input) => input,
                Err(error) => {
                    return http_json_response(
                        400,
                        http_error_payload("invalidRequest", error.to_string(), false),
                    );
                }
            };
            match application.pair_remote_device(input) {
                Ok(peer) => http_json_response(200, json!({ "ok": true, "peer": peer })),
                Err(error) => http_json_response(
                    403,
                    http_error_payload(&error.code, error.message, error.retryable),
                ),
            }
        }
        ("POST", "/dispatch") => {
            let envelope = match serde_json::from_slice::<RemoteRequestEnvelope>(&request.body) {
                Ok(envelope) => envelope,
                Err(error) => {
                    return http_json_response(
                        400,
                        http_error_payload("invalidRequest", error.to_string(), false),
                    );
                }
            };
            http_json_response(200, application.dispatch_remote_request(envelope))
        }
        _ => http_json_response(
            404,
            http_error_payload("unsupported", "unknown remote route", false),
        ),
    }
}

fn http_json_response(status: u16, payload: Value) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned());
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n{body}",
        body.len()
    )
}

fn response_ok(envelope: &RemoteRequestEnvelope, payload: Value) -> Value {
    json!({
        "id": format!("remote-response-{}", Uuid::new_v4()),
        "requestId": envelope.id,
        "protocolVersion": REMOTE_PROTOCOL_VERSION,
        "sentAt": now_millis(),
        "ok": true,
        "payload": payload,
    })
}

fn response_error(envelope: &RemoteRequestEnvelope, error: DesktopRemoteControlError) -> Value {
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

fn http_error_payload(code: &str, message: impl Into<String>, retryable: bool) -> Value {
    json!({
        "ok": false,
        "error": { "code": code, "message": message.into(), "retryable": retryable },
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

fn local_lan_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            let _ = socket.connect("8.8.8.8:80");
            socket.local_addr()
        })
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_owned())
}

fn advertised_bridge_url(port: u16) -> String {
    format!("http://{}:{port}", local_lan_ip())
}

fn pairing_uri_with_bridge_url(pairing_uri: &str, bridge_url: &str) -> String {
    let encoded = url_encode(bridge_url);
    match pairing_uri.split_once('?') {
        Some((base, query)) => {
            let mut parts = query
                .split('&')
                .filter(|part| part.split_once('=').is_none_or(|(key, _)| key != "bridge"))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            parts.push(format!("bridge={encoded}"));
            format!("{base}?{}", parts.join("&"))
        }
        None => format!("{pairing_uri}?bridge={encoded}"),
    }
}

fn bridge_url_from_pairing_uri(pairing_uri: &str) -> Option<String> {
    pairing_uri
        .split_once('?')?
        .1
        .split('&')
        .filter_map(|part| part.split_once('='))
        .find_map(|(key, value)| (key == "bridge").then(|| url_decode(value)))
}

fn url_encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(char::from(byte));
            }
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

fn url_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                output.push(byte);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn database_error(error: rusqlite::Error) -> DesktopRemoteControlError {
    DesktopRemoteControlError::internal(format!("remote database: {error}"))
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    use lilia_agent_integration::ProductCredentialLoginInput;
    use lilia_service::ServiceAuthority;
    use mutsuki_agent_contracts::{
        AgentPermissionMode, CredentialKind, InteractionKind, InteractionRequest,
        OPENAI_CREDENTIAL_PROVIDER_ID,
    };

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopHostError, DesktopHostResult, DesktopProjectCreate,
        DesktopTaskCreate,
    };

    #[derive(Default)]
    struct TestHost {
        awake: AtomicBool,
    }

    impl DesktopHost for TestHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            if let DesktopHostAction::SetSystemAwake { active, .. } = action {
                self.awake.store(active, Ordering::Release);
            }
            Ok(DesktopHostResult::Completed)
        }
    }

    fn application() -> DesktopApplication {
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("remote-test:{}", Uuid::new_v4()),
            "remote-test",
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new("C:/remote-test", "lilia.remote-test").unwrap(),
            authority,
            Arc::new(TestHost::default()),
        )
        .unwrap()
    }

    fn wait_for_task_idle(application: &DesktopApplication, task_id: &TaskId) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if application.task_runtime_snapshot(task_id).phase == "idle" {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("task `{task_id}` did not become idle");
    }

    #[test]
    fn pairing_is_single_use_and_authorizes_product_task_reads() {
        let application = application();
        let project = application
            .create_project(DesktopProjectCreate::new("Remote"))
            .unwrap();
        let task = application
            .create_task(DesktopTaskCreate::new(
                Some(project.id.clone()),
                "Inspect remotely",
            ))
            .unwrap();
        let ticket = application.start_remote_pairing().unwrap();
        let peer = application
            .pair_remote_device(RemotePairDeviceInput {
                ticket_id: ticket.id.clone(),
                challenge: ticket.challenge.clone(),
                device_name: "Phone".to_owned(),
                android_endpoint: RemoteEndpointAddress {
                    endpoint_id: "android-test".to_owned(),
                    relay_url: None,
                    direct_addresses: Vec::new(),
                },
                protocol_version: 1,
            })
            .unwrap();
        assert_eq!(peer.endpoint_id, "android-test");
        assert!(application
            .pair_remote_device(RemotePairDeviceInput {
                ticket_id: ticket.id,
                challenge: ticket.challenge,
                device_name: "Phone".to_owned(),
                android_endpoint: RemoteEndpointAddress {
                    endpoint_id: "android-test".to_owned(),
                    relay_url: None,
                    direct_addresses: Vec::new(),
                },
                protocol_version: 1,
            })
            .is_err());

        let response = application.dispatch_remote_request(RemoteRequestEnvelope {
            id: "request-1".to_owned(),
            protocol_version: 1,
            sent_at: None,
            device_id: "android-test".to_owned(),
            request: json!({ "type": "tasks.list" }),
        });
        assert_eq!(response["ok"], true);
        assert_eq!(response["payload"]["tasks"][0]["taskId"], task.id.as_str());
    }

    #[test]
    fn revoked_or_unpaired_devices_cannot_dispatch_product_requests() {
        let application = application();
        application.set_remote_control_enabled(true).unwrap();
        let response = application.dispatch_remote_request(RemoteRequestEnvelope {
            id: "request-denied".to_owned(),
            protocol_version: 1,
            sent_at: None,
            device_id: "unknown".to_owned(),
            request: json!({ "type": "tasks.list" }),
        });
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "unauthorized");
    }

    #[test]
    fn timeline_pagination_uses_projection_event_ids_as_cursors() {
        let application = application();
        let task = application
            .authority()
            .client()
            .unwrap()
            .create_task(TaskId::new("remote-timeline").unwrap(), None, "Timeline")
            .unwrap();
        let session = lilia_contracts::AgentSessionRef::new("remote-session").unwrap();
        for sequence in 1..=3 {
            application
                .authority()
                .apply_projection(
                    lilia_contracts::TimelineProjectionCommand::UpsertTimelineEvent {
                        event: TimelineProjectionEvent {
                            id: lilia_contracts::ProjectionEventId::from_session_sequence(
                                session.as_str(),
                                sequence,
                            ),
                            task_id: task.id.clone(),
                            agent_session: session.clone(),
                            sequence,
                            turn_id: Some(format!("turn-{sequence}")),
                            kind: "message".to_owned(),
                            status: "success".to_owned(),
                            title: format!("Event {sequence}"),
                            summary: None,
                            payload: json!({ "role": "assistant" }),
                            projected: true,
                        },
                    },
                )
                .unwrap();
        }
        let payload = application
            .remote_timeline_snapshot(&json!({
                "type": "timeline.snapshot",
                "taskId": task.id.as_str(),
                "limit": 2,
                "direction": "latest",
            }))
            .unwrap();
        assert_eq!(payload["events"].as_array().unwrap().len(), 2);
        assert_eq!(payload["page"]["hasMoreBefore"], true);
        assert_eq!(payload["page"]["afterCursor"], "remote-session:3");
    }

    #[test]
    fn native_remote_session_fork_requires_a_durable_turn_cut() {
        let parsed = remote_session_fork_command(&json!({
            "runtimeCommand": {
                "type": "session_fork",
                "excludeTurns": true,
                "sourceTurnId": " turn-2 ",
                "mode": "continue",
            }
        }))
        .unwrap()
        .unwrap();
        assert_eq!(parsed.source_turn_id, "turn-2");
        assert_eq!(parsed.mode, "continue");

        let process = remote_session_fork_command(&json!({
            "runtimeCommand": { "type": "process_session", "action": "spawn" }
        }))
        .unwrap_err();
        assert_eq!(process.code, "unsupported");

        let unbounded = remote_session_fork_command(&json!({
            "runtimeCommand": {
                "type": "session_fork",
                "excludeTurns": false,
                "sourceTurnId": "turn-2",
            }
        }))
        .unwrap_err();
        assert_eq!(unbounded.code, "unsupported");
    }

    #[test]
    fn native_remote_capabilities_only_advertise_runtime_commands_that_are_real() {
        let capabilities = remote_capabilities();
        assert!(capabilities.supports_session_fork);
        assert!(!capabilities.supports_process_session);
    }

    #[test]
    fn remote_architecture_decision_uses_the_atomic_application_interaction() {
        let application = application();
        let project = application
            .create_project(DesktopProjectCreate::new("Remote architecture"))
            .unwrap();
        let task = application
            .create_task(DesktopTaskCreate::new(
                Some(project.id.clone()),
                "Remote architecture approval",
            ))
            .unwrap();
        application
            .authority()
            .shared_runtime()
            .inner()
            .seed_debug_interaction(
                &task.id,
                "remote-architecture-session",
                "remote-architecture-turn",
                InteractionRequest {
                    session_id: "remote-architecture-session".to_owned(),
                    turn_id: "remote-architecture-turn".to_owned(),
                    version: 1,
                    interaction_id: "remote-architecture-request".to_owned(),
                    kind: InteractionKind::Custom,
                    source_tool: Some("update_project_architecture".to_owned()),
                    permission_mode: AgentPermissionMode::Ask,
                    prompt: "Apply remote architecture change".to_owned(),
                    options: json!({
                        "reason": "Remote approval keeps one application authority",
                        "changes": [{
                            "type": "set_summary",
                            "summary": "Approved from a trusted remote client"
                        }]
                    }),
                    context: Some(json!({
                        "productTaskId": task.id.as_str(),
                        "productProjectId": project.id.as_str(),
                        "projectArchitectureVersion": 0
                    })),
                    details: None,
                },
            )
            .unwrap();
        application
            .restore_task_runtime_from_projection(&task.id)
            .unwrap();

        let response = application
            .remote_interaction_respond(&json!({
                "response": {
                    "taskId": task.id.as_str(),
                    "requestId": "remote-architecture-request",
                    "kind": "architecture_change",
                    "result": { "decision": "allow" }
                }
            }))
            .unwrap();

        assert_eq!(response["accepted"], true);
        let graph = application.project_architecture(&project.id).unwrap();
        assert_eq!(graph.version, 1);
        assert_eq!(graph.summary, "Approved from a trusted remote client");
    }

    #[test]
    fn paired_remote_session_fork_continues_from_the_selected_turn_only() {
        let application = application();
        let task = application
            .create_task(DesktopTaskCreate::new(None, "Remote session fork"))
            .unwrap();
        let runtime = application.authority().shared_runtime();
        runtime
            .inner()
            .credentials()
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: None,
                source: Some("user_api_key".into()),
            })
            .unwrap();
        runtime.inner().refresh_product_profile(None).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for index in 1..=3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0_u8; 16_384];
                let _ = stream.read(&mut request).unwrap();
                let body = json!({
                    "choices": [{
                        "finish_reason": "stop",
                        "message": {"role": "assistant", "content": format!("done-{index}")}
                    }],
                    "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
                })
                .to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        runtime
            .inner()
            .set_model_endpoint_override(Some(format!("http://{address}/v1/chat/completions")));

        let first = application
            .start_task_turn(DesktopTurnRequest::new(task.id.clone(), "first"))
            .unwrap();
        wait_for_task_idle(&application, &task.id);
        let second = application
            .start_task_turn(DesktopTurnRequest::new(task.id.clone(), "second"))
            .unwrap();
        wait_for_task_idle(&application, &task.id);
        let source_session_id = application
            .authority()
            .list_session_bindings(&task.id)
            .unwrap()[0]
            .agent_session
            .as_str()
            .to_owned();

        let ticket = application.start_remote_pairing().unwrap();
        application
            .pair_remote_device(RemotePairDeviceInput {
                ticket_id: ticket.id,
                challenge: ticket.challenge,
                device_name: "Phone".to_owned(),
                android_endpoint: RemoteEndpointAddress {
                    endpoint_id: "android-fork".to_owned(),
                    relay_url: None,
                    direct_addresses: Vec::new(),
                },
                protocol_version: 1,
            })
            .unwrap();
        let response = application.dispatch_remote_request(RemoteRequestEnvelope {
            id: "request-fork".to_owned(),
            protocol_version: 1,
            sent_at: None,
            device_id: "android-fork".to_owned(),
            request: json!({
                "type": "chat.send",
                "taskId": task.id.as_str(),
                "content": "third",
                "runtimeCommand": {
                    "type": "session_fork",
                    "excludeTurns": true,
                    "sourceTurnId": first.turn_id,
                    "mode": "fork",
                }
            }),
        });
        assert_eq!(response["ok"], true, "{response}");
        wait_for_task_idle(&application, &task.id);
        server.join().unwrap();

        let target_session_id = response["payload"]["sessionFork"]["sessionId"]
            .as_str()
            .unwrap();
        assert_ne!(target_session_id, source_session_id);
        assert_eq!(
            application
                .authority()
                .list_session_bindings(&task.id)
                .unwrap()[0]
                .agent_session
                .as_str(),
            target_session_id
        );
        let source = runtime
            .inner()
            .session_snapshot(&source_session_id)
            .unwrap();
        assert!(source
            .events
            .iter()
            .any(|event| event.meta.turn_id.as_deref() == Some(second.turn_id.as_str())));
        let target = runtime.inner().session_snapshot(target_session_id).unwrap();
        assert!(target
            .messages
            .iter()
            .any(|message| message.content == "first"));
        assert!(target
            .messages
            .iter()
            .any(|message| message.content == "third"));
        assert!(!target
            .messages
            .iter()
            .any(|message| message.content == "second"));
        assert!(!target
            .events
            .iter()
            .any(|event| event.meta.turn_id.as_deref() == Some(second.turn_id.as_str())));
    }
}

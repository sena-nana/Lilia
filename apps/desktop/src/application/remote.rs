use std::collections::{BTreeMap, HashMap};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

use lilia_contracts::{PendingProjection, ProductTask, TaskId, TimelineProjectionEvent};
use mutsuki_agent_contracts::AgentWireRequestEnvelope;
use serde_json::{json, Value};

use crate::application::{
    timeline_retry_context, DesktopApplication, DesktopApplicationError,
    DesktopArchitectureInteractionDecision, DesktopExecutionPermission, DesktopHost,
    DesktopHostAction, DesktopHostContext, DesktopTerminalCommand, DesktopTerminalLaunch,
    DesktopTerminalProcessState, DesktopTerminalScope, DesktopTerminalSessionId,
    DesktopTurnRequest, ProjectQuery, TaskQuery,
};

pub use lilia_feature_remote::{
    advertised_bridge_url, cancel_pairing, database_error, endpoint_id, host_enabled, now_millis,
    pair_device, refresh_trusted_peer_seen, remote_status, set_setting, DesktopRemoteControlError,
    DesktopRemoteControlService, RemoteCapabilitySet, RemoteChatPermission, RemoteChatSpec,
    RemoteControlStatus, RemoteEndpointAddress, RemoteHost, RemotePairDeviceInput,
    RemotePairingTicket, RemotePeerSummary, RemoteRequestEnvelope, RemoteWakeHost,
    DEFAULT_HTTP_BRIDGE_PORT, HOST_ENABLED_KEY, KEEP_AWAKE_ENABLED_KEY, PC_NAME_KEY, REMOTE_ALPN,
    REMOTE_MIN_PROTOCOL_VERSION, REMOTE_PROTOCOL_VERSION,
};

#[cfg(test)]
pub use lilia_feature_remote::{
    remote_capabilities, remote_process_session_command, remote_session_fork_command,
    RemoteProcessSessionCommand,
};

pub(crate) struct DesktopRemoteWakeHost {
    host: Arc<dyn DesktopHost>,
    context: DesktopHostContext,
}

impl DesktopRemoteWakeHost {
    pub(crate) fn from_host(host: Arc<dyn DesktopHost>, context: DesktopHostContext) -> Self {
        Self { host, context }
    }
}

impl RemoteWakeHost for DesktopRemoteWakeHost {
    fn set_system_awake(&self, active: bool) -> Result<(), String> {
        self.host
            .execute(
                &self.context,
                DesktopHostAction::SetSystemAwake {
                    active,
                    reason: "remote_control".to_owned(),
                },
            )
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

impl From<DesktopApplicationError> for DesktopRemoteControlError {
    fn from(value: DesktopApplicationError) -> Self {
        Self::unavailable(value.to_string())
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
        self.inner.remote.start_pairing(&bridge_url)
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
                    rusqlite::params![now_millis(), device_id],
                )
                .map_err(database_error)?;
            Ok(())
        })?;
        self.inner.remote.sync_wake()?;
        self.remote_control_status()
    }

    pub fn dispatch_remote_request(&self, envelope: RemoteRequestEnvelope) -> Value {
        lilia_feature_remote::dispatch_remote_request(self, envelope)
    }

    fn ensure_remote_http_bridge(&self) -> Result<String, DesktopRemoteControlError> {
        if let Some(existing) = self.inner.remote.bridge_url()? {
            return Ok(existing);
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
            .spawn(move || {
                lilia_feature_remote::serve_http_bridge(listener, move || {
                    application
                        .upgrade()
                        .map(|inner| DesktopApplication { inner })
                })
            })
            .map_err(|error| DesktopRemoteControlError::unavailable(error.to_string()))?;
        self.inner.remote.store_bridge_port(port)?;
        Ok(advertised_bridge_url(port))
    }
}

impl RemoteHost for DesktopApplication {
    fn status(&self) -> Result<RemoteControlStatus, DesktopRemoteControlError> {
        self.remote_control_status()
    }

    fn pair_device(
        &self,
        input: RemotePairDeviceInput,
    ) -> Result<RemotePeerSummary, DesktopRemoteControlError> {
        self.pair_remote_device(input)
    }

    fn authorize(
        &self,
        device_id: &str,
        request_type: &str,
    ) -> Result<(), DesktopRemoteControlError> {
        if request_type == "connection.capabilities.read" {
            return Ok(());
        }
        self.inner
            .remote
            .with_connection(|connection| {
                lilia_feature_remote::authorize_request(connection, device_id, request_type)
            })?;
        self.inner.remote.record_activity()
    }

    fn resume_peer(
        &self,
        device_id: &str,
    ) -> Result<Option<RemotePeerSummary>, DesktopRemoteControlError> {
        self.inner
            .remote
            .with_connection(|connection| refresh_trusted_peer_seen(connection, device_id))
    }

    fn record_activity(&self) -> Result<(), DesktopRemoteControlError> {
        self.inner.remote.record_activity()
    }

    fn sync_wake(&self) -> Result<(), DesktopRemoteControlError> {
        self.inner.remote.sync_wake()
    }

    fn list_tasks(&self) -> Result<Vec<(ProductTask, Option<String>)>, DesktopRemoteControlError> {
        let projects = self
            .query_projects(ProjectQuery {
                include_archived: false,
            })?
            .into_iter()
            .map(|project| (project.id.as_str().to_owned(), project.name))
            .collect::<HashMap<_, _>>();
        Ok(self
            .query_tasks(TaskQuery::default())?
            .into_iter()
            .map(|task| {
                let project_name = task
                    .project_id
                    .as_ref()
                    .and_then(|project_id| projects.get(project_id.as_str()))
                    .cloned();
                (task, project_name)
            })
            .collect())
    }

    fn load_task(
        &self,
        task_id: &TaskId,
    ) -> Result<(ProductTask, Option<String>), DesktopRemoteControlError> {
        let task = self.get_task(task_id)?;
        let project_name = task
            .project_id
            .as_ref()
            .and_then(|project_id| self.get_project(project_id).ok())
            .map(|project| project.name);
        Ok((task, project_name))
    }

    fn task_runtime(&self, task_id: &TaskId) -> Result<Value, DesktopRemoteControlError> {
        serde_json::to_value(self.task_runtime_snapshot(task_id))
            .map_err(|error| DesktopRemoteControlError::internal(error.to_string()))
    }

    fn timeline(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<TimelineProjectionEvent>, DesktopRemoteControlError> {
        Ok(self.task_session_snapshot(task_id)?.timeline)
    }

    fn open_session(
        &self,
        task_id: &TaskId,
    ) -> Result<(Value, Option<String>), DesktopRemoteControlError> {
        let task = self.get_task(task_id)?;
        let session = self.open_task_agent_wire_session(task_id)?;
        let folder = task
            .project_id
            .as_ref()
            .and_then(|project_id| self.get_project(project_id).ok())
            .and_then(|project| project.workspace_path);
        serde_json::to_value(session)
            .map(|session| (session, folder))
            .map_err(|error| DesktopRemoteControlError::internal(error.to_string()))
    }

    fn dispatch_wire(&self, envelope: Value) -> Result<Value, DesktopRemoteControlError> {
        let request: AgentWireRequestEnvelope = serde_json::from_value(envelope)
            .map_err(|error| DesktopRemoteControlError::invalid(error.to_string()))?;
        let response = self.dispatch_agent_wire(request).map_err(|error| {
            DesktopRemoteControlError::new(error.code, error.message, error.retryable)
        })?;
        serde_json::to_value(response)
            .map_err(|error| DesktopRemoteControlError::internal(error.to_string()))
    }

    fn start_chat(&self, spec: RemoteChatSpec) -> Result<Value, DesktopRemoteControlError> {
        let mut turn = DesktopTurnRequest::new(spec.task_id.clone(), spec.content)
            .with_attachments(spec.attachments);
        turn.workspace_path = spec.workspace_path;
        turn.model = spec.model;
        turn.reasoning_effort = spec.reasoning_effort;
        turn.permission = match spec.permission {
            RemoteChatPermission::Full => DesktopExecutionPermission::Full,
            RemoteChatPermission::Readonly => DesktopExecutionPermission::Readonly,
            RemoteChatPermission::Ask => DesktopExecutionPermission::Ask,
        };
        turn.plan_mode = spec.plan_mode;
        turn.goal_mode = spec.goal_mode;
        let forked_session_id = spec
            .session_fork
            .as_ref()
            .map(|command| {
                self.fork_task_agent_session_through_turn(&turn.task_id, &command.source_turn_id)
            })
            .transpose()?;
        let result = self.start_task_turn(turn)?;
        Ok(json!({
            "type": "chat.send",
            "result": result,
            "sessionFork": spec.session_fork.zip(forked_session_id).map(|(command, session_id)| json!({
                "sessionId": session_id,
                "sourceTurnId": command.source_turn_id,
                "mode": command.mode,
            })),
        }))
    }

    fn interrupt(&self, task_id: &TaskId) -> Result<Value, DesktopRemoteControlError> {
        let result = self
            .interrupt_task_turn(task_id)
            .or_else(|_| self.interrupt_projected_task_turn(task_id))?;
        serde_json::to_value(result)
            .map_err(|error| DesktopRemoteControlError::internal(error.to_string()))
    }

    fn retry_event(
        &self,
        task_id: &TaskId,
        event_id: Option<&str>,
    ) -> Result<Value, DesktopRemoteControlError> {
        let events = self.task_session_snapshot(task_id)?.timeline;
        let selected = event_id.and_then(|event_id| {
            events.iter().find(|event| event.id.as_str() == event_id)
        });
        let error = selected.or_else(|| events.iter().rev().find(|event| event.kind == "error"));
        let error = error
            .filter(|event| timeline_retry_context(event, &events).is_some())
            .ok_or_else(|| {
                DesktopRemoteControlError::new("conflict", "no retryable remote message", false)
            })?;
        let result = self.retry_task_timeline_event(task_id, error.id.as_str())?;
        serde_json::to_value(result)
            .map_err(|error| DesktopRemoteControlError::internal(error.to_string()))
    }

    fn pending(
        &self,
        task_id: Option<&TaskId>,
    ) -> Result<Vec<PendingProjection>, DesktopRemoteControlError> {
        let Some(task_id) = task_id else {
            return Ok(Vec::new());
        };
        Ok(self.task_session_snapshot(task_id)?.pending)
    }

    fn respond_approval(
        &self,
        task_id: &TaskId,
        request_id: &str,
        approved: bool,
    ) -> Result<(), DesktopRemoteControlError> {
        self.respond_task_approval(task_id, request_id, approved)
            .or_else(|_| self.respond_projected_task_approval(task_id, request_id, approved))
            .map(|_| ())
            .map_err(DesktopRemoteControlError::from)
    }

    fn respond_interaction(
        &self,
        task_id: &TaskId,
        request_id: &str,
        accepted: bool,
        result: Value,
    ) -> Result<(), DesktopRemoteControlError> {
        self.respond_task_interaction(task_id, request_id, accepted, result.clone())
            .or_else(|_| {
                self.respond_projected_task_interaction(task_id, request_id, accepted, result)
            })
            .map(|_| ())
            .map_err(DesktopRemoteControlError::from)
    }

    fn respond_architecture(
        &self,
        task_id: &TaskId,
        request_id: &str,
        allow: bool,
    ) -> Result<(), DesktopRemoteControlError> {
        let decision = if allow {
            DesktopArchitectureInteractionDecision::Allow
        } else {
            DesktopArchitectureInteractionDecision::Deny
        };
        self.respond_task_architecture_interaction(task_id, request_id, decision)
            .map(|_| ())
            .map_err(DesktopRemoteControlError::from)
    }

    fn provider_status(&self) -> Result<Value, DesktopRemoteControlError> {
        let provider = self.provider_snapshot();
        let ready = provider.runtime.runtime_ready
            && provider.credentials.iter().any(|credential| {
                credential.status == crate::application::DesktopCredentialStatus::Active
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

    fn live_process(&self, task_id: &TaskId) -> Result<Option<String>, DesktopRemoteControlError> {
        let Some(session_id) = self.inner.remote.process_session(task_id) else {
            return Ok(None);
        };
        let id = DesktopTerminalSessionId::from_stored(session_id.clone());
        match self.terminal_snapshot(&id, 0) {
            Ok(snapshot) if matches!(snapshot.process, DesktopTerminalProcessState::Running) => {
                Ok(Some(session_id))
            }
            Ok(snapshot) => {
                self.inner.remote.forget_process_session(task_id);
                if !snapshot.process.is_running() {
                    let _ = self.forget_terminal(&id);
                }
                Ok(None)
            }
            Err(DesktopApplicationError::Terminal(
                crate::application::DesktopTerminalError::SessionNotFound(_),
            )) => {
                self.inner.remote.forget_process_session(task_id);
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn task_blocked(&self, task_id: &TaskId) -> Result<Option<String>, DesktopRemoteControlError> {
        Ok(self.task_run_block(task_id)?.map(|block| block.to_string()))
    }

    fn resolve_process_cwd(
        &self,
        task_id: &TaskId,
        requested: Option<&str>,
    ) -> Result<(), DesktopRemoteControlError> {
        let Some(requested) = requested else {
            return Ok(());
        };
        let requested_cwd = std::fs::canonicalize(requested).map_err(|error| {
            DesktopRemoteControlError::invalid(format!(
                "process session cwd `{requested}` cannot be resolved: {error}"
            ))
        })?;
        let workspace_root =
            self.terminal_workspace_root(&DesktopTerminalScope::Task(task_id.clone()))?;
        if requested_cwd != workspace_root {
            return Err(DesktopRemoteControlError::invalid(
                "process session cwd must match the task workspace",
            ));
        }
        Ok(())
    }

    fn spawn_process(
        &self,
        task_id: &TaskId,
        command: String,
        environment: BTreeMap<String, String>,
        rows: u16,
        columns: u16,
    ) -> Result<Value, DesktopRemoteControlError> {
        let snapshot = self.launch_terminal(DesktopTerminalLaunch {
            scope: DesktopTerminalScope::Task(task_id.clone()),
            command: Some(remote_shell_command(command, environment)),
            rows,
            columns,
        })?;
        Ok(remote_process_snapshot(&snapshot))
    }

    fn write_process(
        &self,
        session_id: &str,
        input: &str,
    ) -> Result<Value, DesktopRemoteControlError> {
        let session_id = DesktopTerminalSessionId::from_stored(session_id);
        self.write_terminal(&session_id, input.as_bytes())?;
        Ok(remote_process_snapshot(&self.terminal_snapshot(&session_id, 0)?))
    }

    fn kill_process(&self, session_id: &str) -> Result<Value, DesktopRemoteControlError> {
        let session_id = DesktopTerminalSessionId::from_stored(session_id);
        self.terminate_terminal(&session_id)?;
        Ok(remote_process_snapshot(&self.terminal_snapshot(&session_id, 0)?))
    }

    fn remember_process(&self, task_id: TaskId, session_id: String) {
        self.inner.remote.remember_process_session(task_id, session_id);
    }

    fn forget_process(&self, task_id: &TaskId) {
        self.inner.remote.forget_process_session(task_id);
    }
}

fn remote_shell_command(
    command: String,
    environment: BTreeMap<String, String>,
) -> DesktopTerminalCommand {
    #[cfg(windows)]
    let mut specification = DesktopTerminalCommand::new("cmd.exe");
    #[cfg(windows)]
    {
        specification.arguments = vec![
            "/D".to_owned(),
            "/S".to_owned(),
            "/C".to_owned(),
            command.clone(),
        ];
    }
    #[cfg(not(windows))]
    let mut specification = DesktopTerminalCommand::new("/bin/sh");
    #[cfg(not(windows))]
    {
        specification.arguments = vec!["-lc".to_owned(), command.clone()];
    }
    specification.environment = environment;
    specification.label = Some(command);
    specification
}

fn remote_process_snapshot(snapshot: &crate::application::DesktopTerminalSnapshot) -> Value {
    let status = match snapshot.process {
        DesktopTerminalProcessState::Running => "running",
        DesktopTerminalProcessState::Terminating => "terminating",
        DesktopTerminalProcessState::Exited { .. } => "exited",
        DesktopTerminalProcessState::Failed { .. } => "failed",
        DesktopTerminalProcessState::Restored => "restored",
    };
    json!({
        "processSessionId": snapshot.id.as_str(),
        "status": status,
        "processId": snapshot.process_id,
        "cwd": snapshot.cwd,
        "command": snapshot.command_label,
        "rows": snapshot.rows,
        "cols": snapshot.columns,
        "revision": snapshot.revision,
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use lilia_agent::ProductCredentialLoginInput;
    use lilia_service::ServiceAuthority;
    use mutsuki_agent_contracts::{
        AgentPermissionMode, CredentialKind, InteractionKind, InteractionRequest,
        OPENAI_CREDENTIAL_PROVIDER_ID,
    };

    use super::*;
    use uuid::Uuid;
    use crate::application::{
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
        let payload = lilia_feature_remote::remote_timeline_snapshot(
            &application,
            &json!({
                "type": "timeline.snapshot",
                "taskId": task.id.as_str(),
                "limit": 2,
                "direction": "latest",
            }),
        )
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
        assert!(capabilities.supports_process_session);
        assert!(matches!(
            remote_process_session_command(&json!({
                "runtimeCommand": {
                    "type": "process_session",
                    "action": "spawn",
                    "command": "cargo test",
                    "rows": 30,
                    "cols": 100,
                    "tty": true,
                    "permissionProfile": ":workspace",
                    "env": { "CI": "1" },
                }
            }))
            .unwrap(),
            Some(RemoteProcessSessionCommand::Spawn {
                rows: 30,
                columns: 100,
                ..
            })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn remote_process_session_runs_in_task_workspace_and_supports_input_and_kill() {
        let root = tempfile::tempdir().unwrap();
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("remote-process-test:{}", Uuid::new_v4()),
            "remote-process-test",
        )
        .unwrap();
        let application = DesktopApplication::from_authority(
            DesktopApplicationConfig::new(root.path().join("home"), "lilia.remote-process-test")
                .unwrap(),
            authority,
            Arc::new(TestHost::default()),
        )
        .unwrap();
        let project = application
            .create_project(DesktopProjectCreate {
                workspace_path: Some(root.path().display().to_string()),
                ..DesktopProjectCreate::new("Remote process")
            })
            .unwrap();
        let task = application
            .create_task(DesktopTaskCreate::new(Some(project.id), "Run remotely"))
            .unwrap();
        let ticket = application.start_remote_pairing().unwrap();
        application
            .pair_remote_device(RemotePairDeviceInput {
                ticket_id: ticket.id,
                challenge: ticket.challenge,
                device_name: "Phone".to_owned(),
                android_endpoint: RemoteEndpointAddress {
                    endpoint_id: "android-process".to_owned(),
                    relay_url: None,
                    direct_addresses: Vec::new(),
                },
                protocol_version: 1,
            })
            .unwrap();

        application
            .update_task(
                &task.id,
                DesktopTaskPatch {
                    status: Some(ProductTaskStatus::Blocked),
                    ..DesktopTaskPatch::default()
                },
            )
            .unwrap();
        let blocked = application.dispatch_remote_request(RemoteRequestEnvelope {
            id: "process-blocked".to_owned(),
            protocol_version: 1,
            sent_at: None,
            device_id: "android-process".to_owned(),
            request: json!({
                "type": "chat.send",
                "taskId": task.id.as_str(),
                "content": "",
                "runtimeCommand": {
                    "type": "process_session",
                    "action": "spawn",
                    "command": "echo should-not-run",
                },
            }),
        });
        assert_eq!(blocked["ok"], false, "{blocked}");
        assert_eq!(blocked["error"]["code"], "conflict");
        assert!(application.list_terminal_sessions().unwrap().is_empty());
        application
            .update_task(
                &task.id,
                DesktopTaskPatch {
                    status: Some(ProductTaskStatus::Waiting),
                    ..DesktopTaskPatch::default()
                },
            )
            .unwrap();

        let spawn = application.dispatch_remote_request(RemoteRequestEnvelope {
            id: "process-spawn".to_owned(),
            protocol_version: 1,
            sent_at: None,
            device_id: "android-process".to_owned(),
            request: json!({
                "type": "chat.send",
                "taskId": task.id.as_str(),
                "content": "",
                "runtimeCommand": {
                    "type": "process_session",
                    "action": "spawn",
                    "command": "printf 'ready:%s\\n' \"$PWD\"; read line; printf 'input:%s\\n' \"$line\"; sleep 30",
                    "cwd": root.path(),
                    "env": { "CI": "1" },
                    "tty": true,
                    "rows": 12,
                    "cols": 90,
                    "permissionProfile": ":workspace",
                },
            }),
        });
        assert_eq!(spawn["ok"], true, "{spawn}");
        let session_id = spawn["payload"]["processSession"]["processSessionId"]
            .as_str()
            .unwrap()
            .to_owned();
        let detail = application.dispatch_remote_request(RemoteRequestEnvelope {
            id: "process-detail".to_owned(),
            protocol_version: 1,
            sent_at: None,
            device_id: "android-process".to_owned(),
            request: json!({ "type": "tasks.get", "taskId": task.id.as_str() }),
        });
        assert_eq!(detail["payload"]["runtime"]["processSessionId"], session_id);

        let stdin = application.dispatch_remote_request(RemoteRequestEnvelope {
            id: "process-stdin".to_owned(),
            protocol_version: 1,
            sent_at: None,
            device_id: "android-process".to_owned(),
            request: json!({
                "type": "chat.send",
                "taskId": task.id.as_str(),
                "content": "",
                "runtimeCommand": {
                    "type": "process_session",
                    "action": "write_stdin",
                    "processId": session_id,
                    "stdin": "hello\r",
                },
            }),
        });
        assert_eq!(stdin["ok"], true, "{stdin}");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let snapshot = application
                .list_terminal_sessions()
                .unwrap()
                .into_iter()
                .find(|snapshot| snapshot.id.as_str() == session_id)
                .unwrap();
            let screen = snapshot
                .screen
                .iter()
                .map(|row| row.text.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            if screen.contains("input:hello") {
                assert!(screen.contains(&format!(
                    "ready:{}",
                    std::fs::canonicalize(root.path()).unwrap().display()
                )));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "remote process output did not converge"
            );
            std::thread::sleep(Duration::from_millis(20));
        }

        let kill = application.dispatch_remote_request(RemoteRequestEnvelope {
            id: "process-kill".to_owned(),
            protocol_version: 1,
            sent_at: None,
            device_id: "android-process".to_owned(),
            request: json!({
                "type": "chat.send",
                "taskId": task.id.as_str(),
                "content": "",
                "runtimeCommand": {
                    "type": "process_session",
                    "action": "kill",
                    "processId": session_id,
                },
            }),
        });
        assert_eq!(kill["ok"], true, "{kill}");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let detail = application.dispatch_remote_request(RemoteRequestEnvelope {
                id: "process-detail-after-kill".to_owned(),
                protocol_version: 1,
                sent_at: None,
                device_id: "android-process".to_owned(),
                request: json!({ "type": "tasks.get", "taskId": task.id.as_str() }),
            });
            if detail["payload"]["runtime"]["processSessionId"].is_null() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "remote process did not terminate"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
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

        let response = lilia_feature_remote::remote_interaction_respond(
            &application,
            &json!({
                "response": {
                    "taskId": task.id.as_str(),
                    "requestId": "remote-architecture-request",
                    "kind": "architecture_change",
                    "result": { "decision": "allow" }
                }
            }),
        )
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

use std::collections::BTreeMap;
use std::sync::Arc;

use lilia_contracts::{AgentSessionRef, ProductApprovalDecision, TaskId};
use lilia_core::AgentKitClientPort;
use mutsuki_agent_client::{
    wire_error, AgentWireAuthority, AgentWireRuntime, AgentWireStateStore, AgentWireTurnOutput,
    InProcessAgentService,
};
use mutsuki_agent_contracts::{
    AgentEventEnvelope, AgentMessage, AgentSession, AgentSessionCreateRequest, AgentWireError,
    AgentWireRequestEnvelope, AgentWireResponseEnvelope, InteractionResolution, PermissionDecision,
    PermissionDecisionKind, ResourceRef,
};
use serde_json::Value;

use crate::{NativeAgentKitRuntime, NativeTurnStreamPage, SharedNativeAgentKitRuntime};

#[derive(Clone, Debug)]
pub struct NativeWireTurnResult {
    pub version: mutsuki_agent_contracts::SessionVersion,
    pub page: NativeTurnStreamPage,
}

#[derive(Clone)]
struct NativeWireRuntime {
    runtime: Arc<NativeAgentKitRuntime>,
}

impl NativeWireRuntime {
    fn ensure_binding(&self, session_id: &str) -> Result<AgentSessionRef, AgentWireError> {
        self.runtime
            .session_snapshot(session_id)
            .map_err(port_error)?;
        AgentSessionRef::new(session_id.to_string())
            .map_err(|error| wire_error("agent.session.invalid_id", error.to_string(), false))
    }
}

impl AgentWireRuntime for NativeWireRuntime {
    fn start_session(
        &self,
        session_id: &str,
        request: AgentSessionCreateRequest,
    ) -> Result<AgentSession, AgentWireError> {
        let task_id = wire_task_id(session_id)?;
        self.runtime
            .open_bound_session(&task_id, session_id, Some(&request.profile_id))
            .map_err(port_error)?;
        let mut session = self
            .runtime
            .session_snapshot(session_id)
            .map_err(port_error)?;
        session.title = request.title;
        Ok(session)
    }

    fn submit_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        messages: &[AgentMessage],
    ) -> Result<AgentWireTurnOutput, AgentWireError> {
        let session = self.ensure_binding(session_id)?;
        let message = messages
            .iter()
            .rev()
            .find(|message| !message.content.trim().is_empty())
            .ok_or_else(|| {
                wire_error(
                    "agent.turn.message_required",
                    "turn must contain a non-empty message",
                    false,
                )
            })?;
        let page = self
            .runtime
            .submit_turn_with_context_streaming(
                &session,
                &message.content,
                turn_id,
                message.metadata.clone(),
            )
            .map_err(port_error)?;
        page_output(page)
    }

    fn cancel_turn(&self, session_id: &str, turn_id: &str) -> Result<(), AgentWireError> {
        self.ensure_binding(session_id)?;
        self.runtime
            .cancel_session_turn(session_id, turn_id)
            .map(|_| ())
            .map_err(port_error)
    }

    fn apply_permission(
        &self,
        decision: &PermissionDecision,
    ) -> Result<AgentWireTurnOutput, AgentWireError> {
        let session = self.ensure_binding(&decision.session_id)?;
        let page = self
            .runtime
            .respond_approval_streaming(
                &session,
                &ProductApprovalDecision {
                    session_id: decision.session_id.clone(),
                    turn_id: decision.turn_id.clone(),
                    action_id: decision.action_id.clone(),
                    version: decision.version,
                    approved: decision.decision == PermissionDecisionKind::Approved,
                },
            )
            .map_err(port_error)?;
        page_output(page)
    }

    fn apply_interactions(
        &self,
        resolutions: &[InteractionResolution],
    ) -> Result<AgentWireTurnOutput, AgentWireError> {
        let first = resolutions.first().ok_or_else(|| {
            wire_error(
                "agent.interaction.resolution_required",
                "at least one interaction resolution is required",
                false,
            )
        })?;
        let session = self.ensure_binding(&first.session_id)?;
        let page = self
            .runtime
            .respond_interactions_streaming(&session, resolutions.to_vec())
            .map_err(port_error)?;
        page_output(page)
    }

    fn events_after(
        &self,
        session_id: &str,
        after_sequence: u64,
    ) -> Result<Vec<AgentEventEnvelope>, AgentWireError> {
        let session = self.ensure_binding(session_id)?;
        self.runtime
            .events_after(&session, after_sequence)
            .map_err(port_error)
    }

    fn fork_session(
        &self,
        source_session_id: &str,
        target_session_id: &str,
    ) -> Result<AgentSession, AgentWireError> {
        self.ensure_binding(source_session_id)?;
        self.runtime
            .fork_session_state(source_session_id, target_session_id)
            .map_err(port_error)?;
        self.runtime
            .session_snapshot(target_session_id)
            .map_err(port_error)
    }

    fn read_resource(
        &self,
        resource: &ResourceRef,
        offset: u64,
        length: u32,
    ) -> Result<(Vec<u8>, bool), AgentWireError> {
        let value = self
            .runtime
            .bootstrap()
            .bundle()
            .resources
            .read_json(resource)
            .map_err(agent_error)?;
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| wire_error("agent.resource.encode", error.to_string(), false))?;
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let end = start.saturating_add(length as usize).min(bytes.len());
        Ok((bytes[start..end].to_vec(), end == bytes.len()))
    }

    fn capabilities(&self) -> Result<BTreeMap<String, String>, AgentWireError> {
        let caps = self.runtime.capabilities().map_err(port_error)?;
        Ok(BTreeMap::from([
            ("backend".into(), caps.backend),
            ("bundle_id".into(), caps.bundle_id),
            (
                "official_agent_server".into(),
                caps.official_agent_server.to_string(),
            ),
            (
                "node_runner_default".into(),
                caps.node_runner_default.to_string(),
            ),
            ("event_resume".into(), caps.supports_resume.to_string()),
        ]))
    }
}

#[derive(Clone)]
struct NativeWirePersistence {
    runtime: Arc<NativeAgentKitRuntime>,
}

impl AgentWireStateStore for NativeWirePersistence {
    fn load(&self) -> Result<Vec<(String, Value)>, AgentWireError> {
        self.runtime.persisted_wire_sessions().map_err(port_error)
    }

    fn store(&self, session_id: &str, state: &Value) -> Result<(), AgentWireError> {
        self.runtime
            .persist_wire_session(session_id, state)
            .map_err(port_error)
    }
}

type NativeAuthority = AgentWireAuthority<NativeWireRuntime, NativeWirePersistence>;

/// Thin product Host adapter around the AgentKit-owned Wire authority.
pub struct NativeAgentWireService {
    runtime: Arc<NativeAgentKitRuntime>,
    authority: NativeAuthority,
    next_product_session: u64,
}

impl NativeAgentWireService {
    pub fn new(runtime: SharedNativeAgentKitRuntime) -> Self {
        Self::try_new(runtime).expect("restore AgentKit Wire authority")
    }

    pub fn try_new(runtime: SharedNativeAgentKitRuntime) -> Result<Self, AgentWireError> {
        let runtime = runtime.0;
        let authority = AgentWireAuthority::new(
            NativeWireRuntime {
                runtime: runtime.clone(),
            },
            NativeWirePersistence {
                runtime: runtime.clone(),
            },
        )?;
        Ok(Self {
            runtime,
            authority,
            next_product_session: 1,
        })
    }

    pub fn runtime(&self) -> &NativeAgentKitRuntime {
        self.runtime.as_ref()
    }

    pub fn open_task_session(
        &mut self,
        task_id: &TaskId,
        requested_session_id: Option<&str>,
        profile_id: &str,
        title: Option<String>,
    ) -> Result<AgentSession, AgentWireError> {
        let session_id = requested_session_id.map(str::to_string).unwrap_or_else(|| {
            let id = format!("native-{}-{}", task_id.as_str(), self.next_product_session);
            self.next_product_session = self.next_product_session.saturating_add(1);
            id
        });
        self.runtime
            .open_bound_session(task_id, &session_id, Some(profile_id))
            .map_err(port_error)?;
        let mut session = self
            .runtime
            .session_snapshot(&session_id)
            .map_err(port_error)?;
        session.title = title;
        self.authority.attach_session(session)
    }

    pub fn submit_task_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
        messages: Vec<AgentMessage>,
        idempotency_key: &str,
    ) -> Result<NativeWireTurnResult, AgentWireError> {
        let expected = self.authority.current_version(session_id)?;
        let (version, output) =
            self.authority
                .submit(session_id, expected, turn_id, messages, idempotency_key)?;
        Ok(NativeWireTurnResult {
            version,
            page: output_page(output)?,
        })
    }

    pub fn fork_task_session(
        &mut self,
        source_session_id: &str,
        target_session_id: &str,
    ) -> Result<AgentSession, AgentWireError> {
        self.fork_task_session_from(source_session_id, target_session_id, None)
    }

    pub fn fork_task_session_through_turn(
        &mut self,
        source_session_id: &str,
        target_session_id: &str,
        through_turn_id: &str,
    ) -> Result<AgentSession, AgentWireError> {
        self.fork_task_session_from(source_session_id, target_session_id, Some(through_turn_id))
    }

    fn fork_task_session_from(
        &mut self,
        source_session_id: &str,
        target_session_id: &str,
        through_turn_id: Option<&str>,
    ) -> Result<AgentSession, AgentWireError> {
        let session = self
            .runtime
            .fork_session_state_through_turn(source_session_id, target_session_id, through_turn_id)
            .map_err(port_error)?;
        self.authority.attach_session(session)
    }

    pub fn submit_task_turn_observed<O>(
        &mut self,
        session_id: &str,
        turn_id: &str,
        messages: Vec<AgentMessage>,
        idempotency_key: &str,
        observer: O,
    ) -> Result<NativeWireTurnResult, AgentWireError>
    where
        O: Fn(&[AgentEventEnvelope]) + Send + Sync + 'static,
    {
        let runtime = self.runtime.clone();
        runtime
            .with_turn_event_observer(session_id, turn_id, observer, || {
                self.submit_task_turn(session_id, turn_id, messages, idempotency_key)
            })
            .map_err(port_error)?
    }

    pub fn respond_task_approval(
        &mut self,
        decision: ProductApprovalDecision,
    ) -> Result<NativeWireTurnResult, AgentWireError> {
        let (version, output) = self.authority.apply_permission(PermissionDecision {
            session_id: decision.session_id,
            turn_id: decision.turn_id,
            action_id: decision.action_id,
            version: decision.version,
            decision: if decision.approved {
                PermissionDecisionKind::Approved
            } else {
                PermissionDecisionKind::Rejected
            },
        })?;
        Ok(NativeWireTurnResult {
            version,
            page: output_page(output)?,
        })
    }

    pub fn respond_task_approval_observed<O>(
        &mut self,
        decision: ProductApprovalDecision,
        observer: O,
    ) -> Result<NativeWireTurnResult, AgentWireError>
    where
        O: Fn(&[AgentEventEnvelope]) + Send + Sync + 'static,
    {
        let runtime = self.runtime.clone();
        let session_id = decision.session_id.clone();
        let turn_id = decision.turn_id.clone();
        runtime
            .with_turn_event_observer(&session_id, &turn_id, observer, || {
                self.respond_task_approval(decision)
            })
            .map_err(port_error)?
    }

    pub fn respond_task_interaction_observed<O>(
        &mut self,
        resolution: InteractionResolution,
        observer: O,
    ) -> Result<NativeWireTurnResult, AgentWireError>
    where
        O: Fn(&[AgentEventEnvelope]) + Send + Sync + 'static,
    {
        let runtime = self.runtime.clone();
        let session_id = resolution.session_id.clone();
        let turn_id = resolution.turn_id.clone();
        runtime
            .with_turn_event_observer(&session_id, &turn_id, observer, || {
                let (version, output) = self.authority.apply_interaction(resolution)?;
                Ok(NativeWireTurnResult {
                    version,
                    page: output_page(output)?,
                })
            })
            .map_err(port_error)?
    }
}

impl InProcessAgentService for NativeAgentWireService {
    fn dispatch(
        &mut self,
        request: AgentWireRequestEnvelope,
    ) -> Result<AgentWireResponseEnvelope, AgentWireError> {
        self.authority.dispatch(request)
    }
}

fn page_output(page: NativeTurnStreamPage) -> Result<AgentWireTurnOutput, AgentWireError> {
    Ok(AgentWireTurnOutput {
        events: page.events.clone(),
        next_sequence: page.next_sequence,
        payload: serde_json::to_value(page)
            .map_err(|error| wire_error("agent.turn.page_encode", error.to_string(), false))?,
    })
}

fn output_page(output: AgentWireTurnOutput) -> Result<NativeTurnStreamPage, AgentWireError> {
    serde_json::from_value(output.payload)
        .map_err(|error| wire_error("agent.turn.page_decode", error.to_string(), false))
}

fn wire_task_id(session_id: &str) -> Result<TaskId, AgentWireError> {
    TaskId::new(format!("agent-wire-task-{session_id}"))
        .map_err(|error| wire_error("agent.session.invalid_task", error.to_string(), false))
}

fn port_error(error: lilia_core::AgentKitPortError) -> AgentWireError {
    wire_error("agent.runtime.unavailable", error.to_string(), true)
}

fn agent_error(error: AgentError) -> AgentWireError {
    wire_error(error.code, error.message, false)
}

use mutsuki_agent_contracts::AgentError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NativeRuntimeBootstrap, ProductCredentialLoginInput};
    use mutsuki_agent_client::{AgentClient, InProcessAgentClient};
    use mutsuki_agent_contracts::{CredentialKind, SessionVersion, OPENAI_CREDENTIAL_PROVIDER_ID};
    use serde_json::json;
    use std::io::{Read, Write};
    use std::sync::mpsc;
    use std::time::Duration;

    fn runtime_with_final_models(
        response_count: usize,
    ) -> (SharedNativeAgentKitRuntime, std::thread::JoinHandle<()>) {
        let runtime = SharedNativeAgentKitRuntime::new(
            NativeRuntimeBootstrap::embedded_reference()
                .unwrap()
                .into_runtime(),
        );
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
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for index in 0..response_count {
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
        (runtime, server)
    }

    fn runtime_with_final_model() -> (SharedNativeAgentKitRuntime, std::thread::JoinHandle<()>) {
        runtime_with_final_models(1)
    }

    #[test]
    fn wire_authority_owns_version_idempotency_and_event_resume() {
        let (runtime, server) = runtime_with_final_model();
        let mut client = AgentClient::new(InProcessAgentClient::new(NativeAgentWireService::new(
            runtime,
        )));
        let session = client
            .start_session(AgentSessionCreateRequest {
                session_id: None,
                profile_id: "mutsuki.reference.coding-agent".into(),
                title: Some("wire".into()),
            })
            .unwrap();
        let version = client
            .submit_turn(
                &session.session_id,
                SessionVersion(1),
                "turn-1",
                vec![AgentMessage::user("finish")],
                "turn-1-key",
            )
            .unwrap();
        server.join().unwrap();
        assert_eq!(version, SessionVersion(2));
        assert_eq!(
            client
                .submit_turn(
                    &session.session_id,
                    SessionVersion(1),
                    "turn-1",
                    vec![AgentMessage::user("finish")],
                    "turn-1-key",
                )
                .unwrap(),
            version
        );
        assert!(!client
            .resume_session_events(&session.session_id, 0)
            .unwrap()
            .events
            .is_empty());
    }

    #[test]
    fn task_session_fork_stops_at_the_selected_durable_turn() {
        let (runtime, server) = runtime_with_final_models(2);
        let mut service = NativeAgentWireService::new(runtime);
        let source = service
            .open_task_session(
                &TaskId::new("task-fork-cut").unwrap(),
                Some("session-fork-source"),
                "mutsuki.reference.coding-agent",
                Some("Fork source".into()),
            )
            .unwrap();
        service
            .submit_task_turn(
                &source.session_id,
                "turn-1",
                vec![AgentMessage::user("first")],
                "turn-1-key",
            )
            .unwrap();
        service
            .submit_task_turn(
                &source.session_id,
                "turn-2",
                vec![AgentMessage::user("second")],
                "turn-2-key",
            )
            .unwrap();
        server.join().unwrap();

        let fork = service
            .fork_task_session_through_turn(&source.session_id, "session-fork-target", "turn-1")
            .unwrap();
        assert_eq!(fork.turn_count, 1);
        assert!(fork
            .messages
            .iter()
            .any(|message| message.content == "first"));
        assert!(!fork
            .messages
            .iter()
            .any(|message| message.content == "second"));
        assert!(fork
            .events
            .iter()
            .all(|event| event.meta.turn_id.as_deref() != Some("turn-2")));
        assert!(service
            .runtime()
            .session_snapshot(&source.session_id)
            .unwrap()
            .events
            .iter()
            .any(|event| event.meta.turn_id.as_deref() == Some("turn-2")));
    }

    #[test]
    fn observed_wire_turn_publishes_running_events_before_model_completion() {
        let runtime = SharedNativeAgentKitRuntime::new(
            NativeRuntimeBootstrap::embedded_reference()
                .unwrap()
                .into_runtime(),
        );
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
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (request_started_tx, request_started_rx) = mpsc::channel();
        let (release_response_tx, release_response_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 16_384];
            let _ = stream.read(&mut request).unwrap();
            request_started_tx.send(()).unwrap();
            release_response_rx.recv().unwrap();
            let body = json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {"role": "assistant", "content": "done"}
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
        });
        runtime
            .inner()
            .set_model_endpoint_override(Some(format!("http://{address}/v1/chat/completions")));
        let mut service = NativeAgentWireService::new(runtime);
        let session = service
            .open_task_session(
                &TaskId::new("task-stream").unwrap(),
                None,
                "mutsuki.reference.coding-agent",
                None,
            )
            .unwrap();
        let (events_tx, events_rx) = mpsc::channel();
        let running = std::thread::spawn(move || {
            service.submit_task_turn_observed(
                &session.session_id,
                "turn-stream",
                vec![AgentMessage::user("stream before completion")],
                "turn-stream-key",
                move |events| events_tx.send(events.to_vec()).unwrap(),
            )
        });

        let observed = match events_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(events) => events,
            Err(error) => {
                let result = running.join().unwrap();
                panic!("observer ended before streaming: {error:?}; turn={result:?}");
            }
        };
        request_started_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(observed.iter().any(|event| matches!(
            &event.event,
            mutsuki_agent_contracts::AgentEvent::TurnState { status, .. }
                if status == "running"
        )));
        assert!(!observed.iter().any(|event| matches!(
            event.event,
            mutsuki_agent_contracts::AgentEvent::FinalResponse { .. }
        )));
        release_response_tx.send(()).unwrap();
        let completed = running.join().unwrap().unwrap();
        server.join().unwrap();
        assert!(completed.page.events.iter().any(|event| matches!(
            event.event,
            mutsuki_agent_contracts::AgentEvent::FinalResponse { .. }
        )));
    }
}

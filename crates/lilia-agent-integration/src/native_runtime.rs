use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use lilia_contracts::{AgentSessionRef, ProductApprovalDecision, TaskId, TimelineProjectionEvent};
use lilia_core::{AgentKitClientPort, AgentKitPortError, NativeAgentCapabilitySnapshot};
use lilia_storage::{SqliteTimelineProjectionStore, TimelineProjectionRepository};
use mutsuki_agent_bundle::{
    run_fix_golden_path, NativeCodingAgentBundle, NativeCodingBackends, NATIVE_CODING_BUNDLE_ID,
};
use mutsuki_agent_contracts::{
    AgentError, AgentEvent, AgentEventEnvelope, AgentEventMeta, AgentRuntimeProfile,
    CodingCommandRef,
};
use mutsuki_agent_plugin_git::CliGitBackend;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::credential::{IndependentDiagnostics, ProductCredentialBridge};
use crate::model_turn::{
    build_live_turn_plan, drive_live_model_turn, live_model_adapter_eligible,
    resolve_model_endpoint, PendingToolApproval,
};
use crate::profile::{build_product_coding_profile, profile_has_credential_refs};
use crate::projection::project_agent_events;

/// Embedded (in-process) vs future Service-connected mode. Both share Client port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeRuntimeMode {
    Embedded,
    Service,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NativeRuntimeError {
    #[error("{0}")]
    Agent(String),
    #[error("session `{0}` not found")]
    SessionNotFound(String),
}

impl From<AgentError> for NativeRuntimeError {
    fn from(value: AgentError) -> Self {
        Self::Agent(value.to_string())
    }
}

impl From<NativeRuntimeError> for AgentKitPortError {
    fn from(value: NativeRuntimeError) -> Self {
        match value {
            NativeRuntimeError::SessionNotFound(id) => AgentKitPortError::NotFound(id),
            NativeRuntimeError::Agent(message) => AgentKitPortError::Unavailable(message),
        }
    }
}

#[derive(Clone, Debug)]
struct SessionRecord {
    task_id: String,
    #[allow(dead_code)]
    profile_id: String,
    turns: Vec<String>,
    cancelled: bool,
    next_sequence: u64,
    events: Vec<AgentEventEnvelope>,
    /// Pending tool approvals awaiting product decision (approve → execute).
    pending_approvals: Vec<SessionPendingApproval>,
}

#[derive(Clone, Debug)]
struct SessionPendingApproval {
    turn_id: String,
    call: PendingToolApproval,
    /// Original user prompt that produced the tool call (for future multi-step continue).
    #[allow(dead_code)]
    prompt: String,
}

/// Host-neutral bootstrap factory. No Tauri / Vue dependency.
#[derive(Clone)]
pub struct NativeRuntimeBootstrap {
    mode: NativeRuntimeMode,
    bundle: NativeCodingAgentBundle,
    credentials: ProductCredentialBridge,
}

impl NativeRuntimeBootstrap {
    pub fn embedded_reference() -> Result<Self, NativeRuntimeError> {
        Self::reference_with_mode(NativeRuntimeMode::Embedded)
    }

    /// Service-mode bootstrap: same Native Coding Agent bundle, Host-neutral authority.
    pub fn service_reference() -> Result<Self, NativeRuntimeError> {
        Self::reference_with_mode(NativeRuntimeMode::Service)
    }

    fn reference_with_mode(mode: NativeRuntimeMode) -> Result<Self, NativeRuntimeError> {
        // Product Git UI and Agent tools share one CliGitBackend-backed service.
        let mut backends = NativeCodingBackends::default();
        backends.git = Arc::new(CliGitBackend::default());
        let bundle = NativeCodingAgentBundle::reference(backends);
        bundle.assert_shared_service_identity()?;
        bundle.assert_no_official_agent_server_dependency()?;
        Ok(Self {
            mode,
            bundle,
            credentials: ProductCredentialBridge::new(),
        })
    }

    pub fn mode(&self) -> NativeRuntimeMode {
        self.mode
    }

    pub fn bundle(&self) -> &NativeCodingAgentBundle {
        &self.bundle
    }

    pub fn credentials(&self) -> &ProductCredentialBridge {
        &self.credentials
    }

    pub fn into_runtime(self) -> NativeAgentKitRuntime {
        NativeAgentKitRuntime::from_bootstrap(self)
    }

    pub fn into_runtime_with_projection_store(
        self,
        projections: SqliteTimelineProjectionStore,
    ) -> NativeAgentKitRuntime {
        NativeAgentKitRuntime::from_bootstrap_with_projections(self, projections)
    }

    /// Reference golden path used by migration smoke (no official Agent Server).
    pub fn run_reference_fix_smoke(&self) -> Result<serde_json::Value, NativeRuntimeError> {
        run_fix_golden_path(&self.bundle).map_err(Into::into)
    }

    pub fn product_profile(
        &self,
        workflow_kind: Option<&str>,
    ) -> Result<AgentRuntimeProfile, NativeRuntimeError> {
        build_product_coding_profile(&self.credentials, workflow_kind)
    }
}

/// Streamable turn result: session events after submit (session/tool/stream surface).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTurnStreamPage {
    pub session_id: String,
    pub turn_id: String,
    pub events: Vec<AgentEventEnvelope>,
    pub next_sequence: u64,
    pub tool_summary: Option<serde_json::Value>,
    pub official_agent_server: bool,
    /// True when turn gated through Credential Broker resolve (secret not in events).
    pub credential_bound: bool,
    /// True when this turn was driven by protocol HTTP Model Adapter (not reference-only).
    pub live_model_adapter_drives_turn: bool,
    pub profile_id: String,
}

/// In-process Native AgentKit runtime/client used by Embedded Desktop and tests.
pub struct NativeAgentKitRuntime {
    bootstrap: NativeRuntimeBootstrap,
    sessions: Mutex<BTreeMap<String, SessionRecord>>,
    /// Product projection store (not Agent Runtime fact source). Durable SQLite.
    projections: SqliteTimelineProjectionStore,
    /// Last built product profile revision snapshot for diagnostics.
    product_profile: Mutex<Option<AgentRuntimeProfile>>,
    /// Optional openai-compatible endpoint override (tests / local gateway).
    model_endpoint_override: Mutex<Option<String>>,
    /// Optional Anthropic Messages endpoint override (tests / local gateway).
    anthropic_endpoint_override: Mutex<Option<String>>,
}

impl NativeAgentKitRuntime {
    pub fn from_bootstrap(bootstrap: NativeRuntimeBootstrap) -> Self {
        let projections = SqliteTimelineProjectionStore::open_in_memory()
            .expect("in-memory product projection store");
        Self::from_bootstrap_with_projections(bootstrap, projections)
    }

    pub fn from_bootstrap_with_projections(
        bootstrap: NativeRuntimeBootstrap,
        projections: SqliteTimelineProjectionStore,
    ) -> Self {
        let profile = build_product_coding_profile(bootstrap.credentials(), None).ok();
        Self {
            bootstrap,
            sessions: Mutex::new(BTreeMap::new()),
            projections,
            product_profile: Mutex::new(profile),
            model_endpoint_override: Mutex::new(None),
            anthropic_endpoint_override: Mutex::new(None),
        }
    }

    /// Override Chat Completions endpoint (loopback for recorded/fake Adapter tests).
    pub fn set_model_endpoint_override(&self, endpoint: Option<String>) {
        if let Ok(mut guard) = self.model_endpoint_override.lock() {
            *guard = endpoint;
        }
    }

    /// Override Anthropic Messages endpoint (loopback for recorded/fake Adapter tests).
    pub fn set_anthropic_endpoint_override(&self, endpoint: Option<String>) {
        if let Ok(mut guard) = self.anthropic_endpoint_override.lock() {
            *guard = endpoint;
        }
    }

    fn model_endpoint(&self) -> String {
        let override_endpoint = self
            .model_endpoint_override
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        resolve_model_endpoint(override_endpoint.as_deref())
    }

    fn anthropic_endpoint(&self) -> Option<String> {
        self.anthropic_endpoint_override
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    pub fn bootstrap(&self) -> &NativeRuntimeBootstrap {
        &self.bootstrap
    }

    pub fn credentials(&self) -> &ProductCredentialBridge {
        self.bootstrap.credentials()
    }

    pub fn projections(&self) -> &SqliteTimelineProjectionStore {
        &self.projections
    }

    pub fn product_artifacts_for_task(
        &self,
        task_id: &TaskId,
    ) -> Vec<lilia_contracts::ArtifactProjection> {
        self.projections.list_artifacts_for_task(task_id)
    }

    pub fn product_todos_for_task(&self, task_id: &TaskId) -> Vec<lilia_contracts::TodoProjection> {
        self.projections.list_todos_for_task(task_id)
    }

    pub fn product_pending_for_task(
        &self,
        task_id: &TaskId,
    ) -> Vec<lilia_contracts::PendingProjection> {
        self.projections.list_pending_for_task(task_id)
    }

    /// Product timeline rows from `lilia-storage` (not Desktop SQLite).
    pub fn product_timeline_for_task(&self, task_id: &TaskId) -> Vec<TimelineProjectionEvent> {
        self.projections.list_for_task(task_id)
    }

    /// Rebuild product timeline for a session from stored AgentKit envelopes.
    pub fn rebuild_product_timeline_for_session(
        &self,
        session: &AgentSessionRef,
    ) -> Result<usize, AgentKitPortError> {
        let (task_id, events) = self.with_session_mut(session.as_str(), |record| {
            Ok((record.task_id.clone(), record.events.clone()))
        })?;
        let task =
            TaskId::new(task_id).map_err(|err| AgentKitPortError::InvalidInput(err.to_string()))?;
        let commands = project_agent_events(&task, &events);
        self.projections
            .rebuild_session(session, commands)
            .map_err(|err| AgentKitPortError::Unavailable(err.to_string()))
    }

    pub fn refresh_product_profile(
        &self,
        workflow_kind: Option<&str>,
    ) -> Result<AgentRuntimeProfile, NativeRuntimeError> {
        let profile = self.bootstrap.product_profile(workflow_kind)?;
        *self
            .product_profile
            .lock()
            .map_err(|_| NativeRuntimeError::Agent("product profile lock poisoned".into()))? =
            Some(profile.clone());
        Ok(profile)
    }

    pub fn current_product_profile(&self) -> Option<AgentRuntimeProfile> {
        self.product_profile
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    pub fn independent_diagnostics(&self) -> IndependentDiagnostics {
        let caps = self.capabilities_inner();
        let profile = self.current_product_profile();
        let live = profile
            .as_ref()
            .map(live_model_adapter_eligible)
            .unwrap_or(false);
        IndependentDiagnostics {
            credential: self.credentials().health(),
            runtime_backend: caps.backend,
            runtime_ready: caps.supports_session && caps.supports_stream,
            official_agent_server: caps.official_agent_server,
            node_runner_default: caps.node_runner_default,
            profile_id: profile
                .as_ref()
                .map(|p| p.profile_id.clone())
                .or(caps.profile_id),
            profile_has_credential_refs: profile
                .as_ref()
                .map(profile_has_credential_refs)
                .unwrap_or(false),
            credential_and_runtime_independent: true,
            live_model_adapter_drives_turn: live,
        }
    }

    /// Credential Broker quota / limits surface (#50). Never fabricates remote remaining quota.
    pub fn native_quota_surface(&self) -> crate::NativeQuotaSurface {
        crate::NativeQuotaSurface::from_credential_health(self.credentials().health())
    }

    pub fn arc(self) -> Arc<Self> {
        Arc::new(self)
    }

    fn capabilities_inner(&self) -> NativeAgentCapabilitySnapshot {
        let profile_id = self
            .current_product_profile()
            .map(|p| p.profile_id)
            .unwrap_or_else(|| self.bootstrap.bundle.profile.profile_id.clone());
        NativeAgentCapabilitySnapshot {
            backend: "native-agentkit".into(),
            bundle_id: NATIVE_CODING_BUNDLE_ID.into(),
            official_agent_server: false,
            node_runner_default: false,
            supports_session: true,
            supports_stream: true,
            supports_approval: true,
            supports_cancel: true,
            supports_resume: true,
            profile_id: Some(profile_id),
        }
    }

    fn with_session_mut<R>(
        &self,
        session_id: &str,
        f: impl FnOnce(&mut SessionRecord) -> Result<R, AgentKitPortError>,
    ) -> Result<R, AgentKitPortError> {
        let mut sessions = self.sessions.lock().map_err(|_| {
            AgentKitPortError::Unavailable("native runtime session lock poisoned".into())
        })?;
        let record = sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentKitPortError::NotFound(session_id.to_string()))?;
        f(record)
    }

    fn append_event(
        &self,
        session_id: &str,
        meta: AgentEventMeta,
        event: AgentEvent,
    ) -> Result<AgentEventEnvelope, AgentKitPortError> {
        self.with_session_mut(session_id, |record| {
            record.next_sequence += 1;
            let envelope = AgentEventEnvelope {
                session_id: session_id.to_string(),
                sequence: record.next_sequence,
                meta,
                event,
            };
            record.events.push(envelope.clone());
            Ok(envelope)
        })
    }

    /// Ensure any bound CredentialRef resolves through Broker before turn tools run.
    /// Does not call official Agent Server. Secret material stays in Broker.
    fn gate_credentials_for_turn(&self) -> Result<bool, AgentKitPortError> {
        let profile = self
            .refresh_product_profile(None)
            .map_err(|err| AgentKitPortError::Unavailable(err.to_string()))?;
        let mut bound = false;
        for provider in &profile.providers {
            if let Some(credential) = &provider.credential_ref {
                self.credentials()
                    .resolve_for_adapter(credential)
                    .map_err(|err| AgentKitPortError::Unavailable(err.to_string()))?;
                bound = true;
            }
        }
        Ok(bound)
    }

    /// Submit a turn and return the streamed AgentKit events for that turn.
    ///
    /// When the product profile binds a usable openai-compatible CredentialRef,
    /// the turn is driven by the protocol HTTP Model Adapter (Credential resolve →
    /// Adapter generate/stream). Otherwise the reference Native Coding tool path
    /// (`run_fix_golden_path`) is retained for credential-free / smoke environments.
    pub fn submit_turn_streaming(
        &self,
        session: &AgentSessionRef,
        prompt: &str,
        turn_id: &str,
    ) -> Result<NativeTurnStreamPage, AgentKitPortError> {
        let session_id = session.as_str().to_string();
        let after = self.with_session_mut(&session_id, |record| {
            if record.cancelled {
                return Err(AgentKitPortError::InvalidInput(
                    "cannot submit turn on cancelled session".into(),
                ));
            }
            if prompt.trim().is_empty() {
                return Err(AgentKitPortError::InvalidInput(
                    "prompt must not be empty".into(),
                ));
            }
            Ok(record.next_sequence)
        })?;

        self.bootstrap
            .bundle
            .assert_no_official_agent_server_dependency()
            .map_err(|err| AgentKitPortError::Unavailable(err.to_string()))?;

        let credential_bound = self.gate_credentials_for_turn()?;
        let profile = self
            .current_product_profile()
            .ok_or_else(|| AgentKitPortError::Unavailable("product profile missing".into()))?;
        let profile_id = profile.profile_id.clone();
        let live_plan = if live_model_adapter_eligible(&profile) {
            build_live_turn_plan(
                &profile,
                &self.model_endpoint(),
                self.anthropic_endpoint().as_deref(),
            )
        } else {
            None
        };

        self.with_session_mut(&session_id, |record| {
            record.turns.push(prompt.to_string());
            Ok(())
        })?;

        self.append_event(
            &session_id,
            AgentEventMeta::new(format!("evt-turn-{turn_id}"), "turn started").with_turn(turn_id),
            AgentEvent::TurnState {
                turn_id: turn_id.into(),
                status: "running".into(),
            },
        )?;

        let (tool_summary, live_model_adapter_drives_turn) = if let Some(plan) = live_plan.as_ref()
        {
            let live = drive_live_model_turn(
                self.credentials().broker(),
                plan,
                &session_id,
                turn_id,
                prompt,
            )
            .map_err(AgentKitPortError::Unavailable)?;
            for (meta, event) in live.events {
                self.append_event(&session_id, meta, event)?;
            }
            if live.waiting_approval {
                self.with_session_mut(&session_id, |record| {
                    for call in live.pending_approvals {
                        record.pending_approvals.push(SessionPendingApproval {
                            turn_id: turn_id.to_string(),
                            call,
                            prompt: prompt.to_string(),
                        });
                    }
                    Ok(())
                })?;
            }
            let mut summary = live.tool_summary;
            if let Some(object) = summary.as_object_mut() {
                object.insert("waiting_approval".into(), json!(live.waiting_approval));
            }
            (Some(summary), true)
        } else {
            self.append_event(
                &session_id,
                AgentEventMeta::new(format!("evt-delta-{turn_id}"), "model delta")
                    .with_turn(turn_id),
                AgentEvent::ModelDelta {
                    turn_id: turn_id.into(),
                    text: format!(
                        "Native AgentKit reference path: {} (credential_bound={credential_bound})",
                        prompt.trim()
                    ),
                },
            )?;
            self.append_event(
                &session_id,
                AgentEventMeta::new(format!("evt-tool-{turn_id}"), "tool call").with_turn(turn_id),
                AgentEvent::ToolCallStarted {
                    turn_id: turn_id.into(),
                    call_id: format!("tool-{turn_id}"),
                    name: "native.coding.fix".into(),
                    input: json!({ "prompt": prompt }),
                },
            )?;

            let tool_summary = run_fix_golden_path(&self.bootstrap.bundle)
                .map_err(|err| AgentKitPortError::Unavailable(err.to_string()))?;
            if tool_summary
                .get("official_servers")
                .and_then(|v| v.as_u64())
                != Some(0)
            {
                return Err(AgentKitPortError::Unavailable(
                    "native path must not depend on official agent servers".into(),
                ));
            }

            self.append_event(
                &session_id,
                AgentEventMeta::new(format!("evt-cmd-{turn_id}"), "command started")
                    .with_turn(turn_id),
                AgentEvent::CommandStarted {
                    turn_id: turn_id.into(),
                    command: CodingCommandRef {
                        command_id: format!("cmd-{turn_id}"),
                        command: "native.coding.fix".into(),
                        args: vec![],
                        cwd: None,
                    },
                },
            )?;
            self.append_event(
                &session_id,
                AgentEventMeta::new(format!("evt-tool-done-{turn_id}"), "tool completed")
                    .with_turn(turn_id),
                AgentEvent::ToolCallCompleted {
                    turn_id: turn_id.into(),
                    call_id: format!("tool-{turn_id}"),
                    summary: tool_summary
                        .get("patched")
                        .and_then(|v| v.as_str())
                        .unwrap_or("ok")
                        .to_string(),
                    details: None,
                },
            )?;
            self.append_event(
                    &session_id,
                    AgentEventMeta::new(format!("evt-final-{turn_id}"), "final response")
                        .with_turn(turn_id),
                    AgentEvent::FinalResponse {
                        turn_id: turn_id.into(),
                        summary: format!(
                            "Native AgentKit reference turn complete (official_servers=0, credential_bound={credential_bound}, patched={})",
                            tool_summary
                                .get("patched")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?")
                        ),
                        result: None,
                    },
                )?;
            self.append_event(
                &session_id,
                AgentEventMeta::new(format!("evt-turn-done-{turn_id}"), "turn completed")
                    .with_turn(turn_id),
                AgentEvent::TurnState {
                    turn_id: turn_id.into(),
                    status: "completed".into(),
                },
            )?;
            (Some(tool_summary), false)
        };

        let events = self.with_session_mut(&session_id, |record| {
            Ok(record
                .events
                .iter()
                .filter(|event| event.sequence > after)
                .cloned()
                .collect::<Vec<_>>())
        })?;
        let next_sequence = events.last().map(|e| e.sequence).unwrap_or(after);

        // Project AgentKit events into product projection store (not execution source).
        if let Ok(task_id) =
            TaskId::new(self.with_session_mut(&session_id, |record| Ok(record.task_id.clone()))?)
        {
            for command in project_agent_events(&task_id, &events) {
                let _ = self.projections.apply(command);
            }
        }

        Ok(NativeTurnStreamPage {
            session_id,
            turn_id: turn_id.to_string(),
            events,
            next_sequence,
            tool_summary,
            official_agent_server: false,
            credential_bound,
            live_model_adapter_drives_turn,
            profile_id,
        })
    }

    /// Apply product approval decision and execute/deny the pending tool.
    pub fn respond_approval_streaming(
        &self,
        session: &AgentSessionRef,
        decision: &ProductApprovalDecision,
    ) -> Result<NativeTurnStreamPage, AgentKitPortError> {
        let session_id = session.as_str().to_string();
        if decision.session_id != session_id {
            return Err(AgentKitPortError::InvalidInput(
                "approval session_id does not match target session".into(),
            ));
        }
        let after = self.with_session_mut(&session_id, |record| Ok(record.next_sequence))?;

        let pending = self.with_session_mut(&session_id, |record| {
            let index = record.pending_approvals.iter().position(|item| {
                item.turn_id == decision.turn_id
                    && item.call.call_id == decision.action_id
                    && item.call.version == decision.version
            });
            let Some(index) = index else {
                return Err(AgentKitPortError::NotFound(format!(
                    "pending approval `{}`@v{}",
                    decision.action_id, decision.version
                )));
            };
            Ok(record.pending_approvals.remove(index))
        })?;

        let turn_id = pending.turn_id.clone();
        let call_id = pending.call.call_id.clone();
        let tool_name = pending.call.name.clone();

        self.append_event(
            &session_id,
            AgentEventMeta::new(
                format!("evt-approval-decision-{call_id}"),
                "approval decision",
            )
            .with_turn(&turn_id),
            AgentEvent::TurnState {
                turn_id: turn_id.clone(),
                status: if decision.approved {
                    "approval_granted".into()
                } else {
                    "approval_denied".into()
                },
            },
        )?;

        let tool_summary = if decision.approved {
            self.execute_approved_tool(
                &session_id,
                &turn_id,
                &call_id,
                &tool_name,
                &pending.call.input,
            )?
        } else {
            self.append_event(
                &session_id,
                AgentEventMeta::new(format!("evt-tool-denied-{call_id}"), "tool denied")
                    .with_turn(&turn_id),
                AgentEvent::ToolCallCompleted {
                    turn_id: turn_id.clone(),
                    call_id: call_id.clone(),
                    summary: format!("Tool `{tool_name}` denied by product approval"),
                    details: None,
                },
            )?;
            self.append_event(
                &session_id,
                AgentEventMeta::new(format!("evt-final-{turn_id}-denied"), "final response")
                    .with_turn(&turn_id),
                AgentEvent::FinalResponse {
                    turn_id: turn_id.clone(),
                    summary: format!("Turn stopped: tool `{tool_name}` was denied"),
                    result: None,
                },
            )?;
            self.append_event(
                &session_id,
                AgentEventMeta::new(format!("evt-turn-done-{turn_id}-denied"), "turn completed")
                    .with_turn(&turn_id),
                AgentEvent::TurnState {
                    turn_id: turn_id.clone(),
                    status: "completed".into(),
                },
            )?;
            Some(json!({
                "driver": "approval",
                "official_servers": 0,
                "approved": false,
                "tool": tool_name,
            }))
        };

        let events = self.with_session_mut(&session_id, |record| {
            Ok(record
                .events
                .iter()
                .filter(|event| event.sequence > after)
                .cloned()
                .collect::<Vec<_>>())
        })?;
        let next_sequence = events.last().map(|e| e.sequence).unwrap_or(after);
        let profile_id = self
            .current_product_profile()
            .map(|p| p.profile_id)
            .unwrap_or_default();

        if let Ok(task_id) =
            TaskId::new(self.with_session_mut(&session_id, |record| Ok(record.task_id.clone()))?)
        {
            for command in project_agent_events(&task_id, &events) {
                let _ = self.projections.apply(command);
            }
        }

        Ok(NativeTurnStreamPage {
            session_id,
            turn_id,
            events,
            next_sequence,
            tool_summary,
            official_agent_server: false,
            credential_bound: true,
            live_model_adapter_drives_turn: true,
            profile_id,
        })
    }

    fn execute_approved_tool(
        &self,
        session_id: &str,
        turn_id: &str,
        call_id: &str,
        tool_name: &str,
        input: &Value,
    ) -> Result<Option<Value>, AgentKitPortError> {
        self.append_event(
            session_id,
            AgentEventMeta::new(format!("evt-cmd-{call_id}"), "command started").with_turn(turn_id),
            AgentEvent::CommandStarted {
                turn_id: turn_id.into(),
                command: CodingCommandRef {
                    command_id: format!("cmd-{call_id}"),
                    command: tool_name.into(),
                    args: vec![],
                    cwd: None,
                },
            },
        )?;

        let tool_summary = if tool_name == "native.coding.fix" {
            run_fix_golden_path(&self.bootstrap.bundle)
                .map_err(|err| AgentKitPortError::Unavailable(err.to_string()))?
        } else {
            json!({
                "official_servers": 0,
                "executed": true,
                "tool": tool_name,
                "input": input,
                "note": "generic tool execution stub in product native runtime",
            })
        };
        if tool_summary
            .get("official_servers")
            .and_then(|v| v.as_u64())
            != Some(0)
        {
            return Err(AgentKitPortError::Unavailable(
                "native path must not depend on official agent servers".into(),
            ));
        }

        let summary = tool_summary
            .get("patched")
            .and_then(|v| v.as_str())
            .map(|value| format!("executed `{tool_name}`: {value}"))
            .unwrap_or_else(|| format!("executed `{tool_name}`"));

        self.append_event(
            session_id,
            AgentEventMeta::new(format!("evt-tool-done-{call_id}"), "tool completed")
                .with_turn(turn_id),
            AgentEvent::ToolCallCompleted {
                turn_id: turn_id.into(),
                call_id: call_id.into(),
                summary: summary.clone(),
                details: None,
            },
        )?;
        self.append_event(
            session_id,
            AgentEventMeta::new(format!("evt-final-{turn_id}-approved"), "final response")
                .with_turn(turn_id),
            AgentEvent::FinalResponse {
                turn_id: turn_id.into(),
                summary: format!("Native tool completed after approval ({summary})"),
                result: None,
            },
        )?;
        self.append_event(
            session_id,
            AgentEventMeta::new(
                format!("evt-turn-done-{turn_id}-approved"),
                "turn completed",
            )
            .with_turn(turn_id),
            AgentEvent::TurnState {
                turn_id: turn_id.into(),
                status: "completed".into(),
            },
        )?;

        let mut summary_value = tool_summary;
        if let Some(object) = summary_value.as_object_mut() {
            object.insert("approved".into(), json!(true));
            object.insert("tool".into(), json!(tool_name));
            object.insert("official_servers".into(), json!(0));
        }
        Ok(Some(summary_value))
    }

    pub fn events_after(
        &self,
        session: &AgentSessionRef,
        after_sequence: u64,
    ) -> Result<Vec<AgentEventEnvelope>, AgentKitPortError> {
        self.with_session_mut(session.as_str(), |record| {
            Ok(record
                .events
                .iter()
                .filter(|event| event.sequence > after_sequence)
                .cloned()
                .collect())
        })
    }

    /// Open (or idempotently resume) a Live Runtime session for a product binding.
    ///
    /// Used after migration: binding already holds a deterministic AgentKit session id
    /// (`agentkit-from-legacy:…`); first Native turn must attach that id rather than
    /// forging a new one.
    pub fn open_bound_session(
        &self,
        task_id: &TaskId,
        session_id: &str,
        profile_id: Option<&str>,
    ) -> Result<AgentSessionRef, AgentKitPortError> {
        let profile = match profile_id {
            Some(id) => id.to_string(),
            None => self
                .refresh_product_profile(None)
                .map_err(|err| AgentKitPortError::Unavailable(err.to_string()))?
                .profile_id,
        };
        let mut sessions = self.sessions.lock().map_err(|_| {
            AgentKitPortError::Unavailable("native runtime session lock poisoned".into())
        })?;
        if let Some(existing) = sessions.get(session_id) {
            if existing.task_id != task_id.as_str() {
                return Err(AgentKitPortError::InvalidInput(format!(
                    "session `{session_id}` already bound to task `{}`",
                    existing.task_id
                )));
            }
            return AgentSessionRef::new(session_id.to_string())
                .map_err(|err| AgentKitPortError::InvalidInput(err.to_string()));
        }
        sessions.insert(
            session_id.to_string(),
            SessionRecord {
                task_id: task_id.as_str().to_string(),
                profile_id: profile,
                turns: Vec::new(),
                cancelled: false,
                next_sequence: 0,
                events: Vec::new(),
                pending_approvals: Vec::new(),
            },
        );
        AgentSessionRef::new(session_id.to_string())
            .map_err(|err| AgentKitPortError::InvalidInput(err.to_string()))
    }
}

impl AgentKitClientPort for NativeAgentKitRuntime {
    fn capabilities(&self) -> Result<NativeAgentCapabilitySnapshot, AgentKitPortError> {
        Ok(self.capabilities_inner())
    }

    fn start_session_for_task(
        &self,
        task_id: &TaskId,
        profile_id: Option<&str>,
    ) -> Result<AgentSessionRef, AgentKitPortError> {
        let profile = self
            .refresh_product_profile(None)
            .map_err(|err| AgentKitPortError::Unavailable(err.to_string()))?;
        let profile = profile_id
            .map(str::to_string)
            .unwrap_or_else(|| profile.profile_id.clone());
        let session_id = {
            let sessions = self.sessions.lock().map_err(|_| {
                AgentKitPortError::Unavailable("native runtime session lock poisoned".into())
            })?;
            format!("native-{}-{}", task_id.as_str(), sessions.len())
        };
        self.open_bound_session(task_id, &session_id, Some(&profile))
    }

    fn submit_turn(
        &self,
        session: &AgentSessionRef,
        prompt: &str,
    ) -> Result<(), AgentKitPortError> {
        let turn_id = format!("turn-{}", uuid_like_turn_id(session.as_str(), prompt));
        self.submit_turn_streaming(session, prompt, &turn_id)?;
        Ok(())
    }

    fn cancel_turn(&self, session: &AgentSessionRef) -> Result<(), AgentKitPortError> {
        let mut sessions = self.sessions.lock().map_err(|_| {
            AgentKitPortError::Unavailable("native runtime session lock poisoned".into())
        })?;
        let record = sessions
            .get_mut(session.as_str())
            .ok_or_else(|| AgentKitPortError::NotFound(session.as_str().to_string()))?;
        record.cancelled = true;
        self.bootstrap
            .bundle
            .subagents
            .cancel_parent(session.as_str());
        Ok(())
    }

    fn respond_approval(
        &self,
        session: &AgentSessionRef,
        decision: &ProductApprovalDecision,
    ) -> Result<(), AgentKitPortError> {
        self.respond_approval_streaming(session, decision)?;
        Ok(())
    }
}

fn uuid_like_turn_id(session_id: &str, prompt: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    session_id.hash(&mut hasher);
    prompt.hash(&mut hasher);
    hasher.finish()
}

/// Newtype so `Arc` can implement the product AgentKit port without orphan rules.
#[derive(Clone)]
pub struct SharedNativeAgentKitRuntime(pub Arc<NativeAgentKitRuntime>);

impl SharedNativeAgentKitRuntime {
    pub fn new(runtime: NativeAgentKitRuntime) -> Self {
        Self(Arc::new(runtime))
    }

    pub fn inner(&self) -> &NativeAgentKitRuntime {
        self.0.as_ref()
    }
}

impl AgentKitClientPort for SharedNativeAgentKitRuntime {
    fn capabilities(&self) -> Result<NativeAgentCapabilitySnapshot, AgentKitPortError> {
        self.0.capabilities()
    }

    fn start_session_for_task(
        &self,
        task_id: &TaskId,
        profile_id: Option<&str>,
    ) -> Result<AgentSessionRef, AgentKitPortError> {
        self.0.start_session_for_task(task_id, profile_id)
    }

    fn submit_turn(
        &self,
        session: &AgentSessionRef,
        prompt: &str,
    ) -> Result<(), AgentKitPortError> {
        self.0.submit_turn(session, prompt)
    }

    fn cancel_turn(&self, session: &AgentSessionRef) -> Result<(), AgentKitPortError> {
        self.0.cancel_turn(session)
    }

    fn respond_approval(
        &self,
        session: &AgentSessionRef,
        decision: &ProductApprovalDecision,
    ) -> Result<(), AgentKitPortError> {
        self.0.respond_approval_streaming(session, decision)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::ProductCredentialLoginInput;
    use crate::live_model_adapter_eligible;
    use crate::profile::PRODUCT_NATIVE_CODING_PROFILE_HINT;
    use lilia_client::LiliaClient;
    use lilia_contracts::{BindingId, ProductApprovalDecision, ProjectId};
    use lilia_core::SessionBindingService;
    use lilia_storage::ProjectionApplyResult;
    use mutsuki_agent_contracts::{
        AgentRuntimeMode, CredentialKind, OPENAI_CREDENTIAL_PROVIDER_ID,
    };
    use mutsuki_agent_testkit::{emit_deterministic_coding_run, CodingEventLog};

    #[test]
    fn embedded_bootstrap_rejects_official_servers_and_binds_product_session() {
        let bootstrap = NativeRuntimeBootstrap::embedded_reference().unwrap();
        assert_eq!(bootstrap.mode(), NativeRuntimeMode::Embedded);
        assert_eq!(bootstrap.bundle().profile.mode, AgentRuntimeMode::Test);
        let smoke = bootstrap.run_reference_fix_smoke().unwrap();
        assert_eq!(smoke["official_servers"], 0);

        let runtime = SharedNativeAgentKitRuntime::new(bootstrap.into_runtime());
        let caps = runtime.capabilities().unwrap();
        assert_eq!(caps.backend, "native-agentkit");
        assert!(!caps.official_agent_server);
        assert!(!caps.node_runner_default);
        assert!(caps.supports_approval);
        assert_eq!(caps.bundle_id, NATIVE_CODING_BUNDLE_ID);

        let client = LiliaClient::with_agent(runtime.clone());
        let project = client
            .create_project(ProjectId::new("p-native").unwrap(), "Native")
            .unwrap();
        let task = client
            .create_task(
                TaskId::new("t-native").unwrap(),
                Some(project.id),
                "Native path",
            )
            .unwrap();
        let binding = client
            .bind_agent_session(
                &task.id,
                None,
                Some("mutsuki.reference.coding-agent"),
                BindingId::new("bind-1").unwrap(),
            )
            .unwrap();
        assert_eq!(client.list_bindings(&task.id).len(), 1);
        SessionBindingService::new(client.products(), &runtime)
            .submit_prompt(&binding.agent_session, "fix the failing test")
            .unwrap();
    }

    #[test]
    fn native_client_session_submit_stream_cancel_and_tool_events() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let task = TaskId::new("task-42").unwrap();
        let session = runtime.start_session_for_task(&task, None).unwrap();
        let page = runtime
            .submit_turn_streaming(&session, "implement fix", "turn-1")
            .unwrap();
        assert!(!page.events.is_empty());
        assert!(!page.official_agent_server);
        assert!(!page.credential_bound);
        assert!(!page.live_model_adapter_drives_turn);
        assert_eq!(
            page.tool_summary
                .as_ref()
                .and_then(|v| v.get("official_servers"))
                .and_then(|v| v.as_u64()),
            Some(0)
        );
        assert!(page.events.iter().any(|envelope| {
            matches!(
                envelope.event,
                AgentEvent::ToolCallStarted { .. }
                    | AgentEvent::ToolCallCompleted { .. }
                    | AgentEvent::FinalResponse { .. }
                    | AgentEvent::ModelDelta { .. }
            )
        }));
        assert!(!runtime.projections().list_for_task(&task).is_empty());

        runtime.cancel_turn(&session).unwrap();
        let err = runtime
            .submit_turn_streaming(&session, "should fail", "turn-2")
            .unwrap_err();
        assert!(matches!(err, AgentKitPortError::InvalidInput(_)));

        let events = CodingEventLog::new(session.as_str());
        emit_deterministic_coding_run(&events, "product-projection");
        let page = events.page(0);
        assert!(!page.events.is_empty());
    }

    #[test]
    fn credential_login_binds_profile_and_drives_live_model_adapter_turn() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let before = runtime.independent_diagnostics();
        assert!(before.credential_and_runtime_independent);
        assert!(before.runtime_ready);
        assert!(!before.credential.has_usable_model_credential);
        assert!(!before.live_model_adapter_drives_turn);

        runtime
            .credentials()
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: Some("openai".into()),
                source: Some("user_api_key".into()),
            })
            .unwrap();
        let _ = runtime.refresh_product_profile(None).unwrap();
        assert!(
            runtime
                .independent_diagnostics()
                .live_model_adapter_drives_turn
        );

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 16_384];
            let _ = stream.read(&mut bytes).unwrap();
            let payload = r#"{"choices":[{"message":{"role":"assistant","content":"credential live reply"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3}}"#;
            let body = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
            stream.write_all(body.as_bytes()).unwrap();
        });
        runtime.set_model_endpoint_override(Some(format!("http://{address}/v1/chat/completions")));

        let task = TaskId::new("task-cred").unwrap();
        let session = runtime.start_session_for_task(&task, None).unwrap();
        let page = runtime
            .submit_turn_streaming(&session, "with credential", "turn-c1")
            .unwrap();
        assert!(page.credential_bound);
        assert!(page.live_model_adapter_drives_turn);
        assert!(page
            .profile_id
            .starts_with(PRODUCT_NATIVE_CODING_PROFILE_HINT));
        assert!(!page.official_agent_server);
        assert!(page.events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                AgentEvent::ModelDelta { text, .. } if text.contains("credential live reply")
            )
        }));
        assert_eq!(
            page.tool_summary
                .as_ref()
                .and_then(|v| v.get("driver"))
                .and_then(|v| v.as_str()),
            Some("openai-compatible")
        );

        let after = runtime.independent_diagnostics();
        assert!(after.credential.has_usable_model_credential);
        assert!(after.profile_has_credential_refs);
        assert!(after.live_model_adapter_drives_turn);
        assert!(after.runtime_ready);
        assert!(!after.official_agent_server);

        let projected = runtime.projections().list_for_task(&task);
        assert!(!projected.is_empty());
        for command in crate::project_agent_event(&task, &page.events[0]) {
            let again = runtime.projections().apply(command);
            assert!(matches!(
                again.unwrap(),
                ProjectionApplyResult::DuplicateIgnored | ProjectionApplyResult::Updated
            ));
        }
        server.join().unwrap();
    }

    #[test]
    fn live_adapter_tool_call_emits_approval_request() {
        use std::io::{Read, Write};

        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        runtime
            .credentials()
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: None,
                source: Some("user_api_key".into()),
            })
            .unwrap();
        let _ = runtime.refresh_product_profile(None).unwrap();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 16_384];
            let _ = stream.read(&mut bytes).unwrap();
            let body = json!({
                "choices": [{
                    "finish_reason": "tool_calls",
                    "message": {
                        "content": null,
                        "tool_calls": [{
                            "id": "call-approve-1",
                            "type": "function",
                            "function": {
                                "name": "native.coding.fix",
                                "arguments": "{\"prompt\":\"fix\"}"
                            }
                        }]
                    }
                }],
                "usage": {"prompt_tokens": 2, "completion_tokens": 3, "total_tokens": 5}
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        runtime.set_model_endpoint_override(Some(format!("http://{address}/v1/chat/completions")));

        let task = TaskId::new("task-approval").unwrap();
        let session = runtime.start_session_for_task(&task, None).unwrap();
        let page = runtime
            .submit_turn_streaming(&session, "please fix", "turn-a1")
            .unwrap();
        assert!(page.live_model_adapter_drives_turn);
        assert!(page
            .events
            .iter()
            .any(|envelope| { matches!(envelope.event, AgentEvent::ApprovalRequest { .. }) }));
        assert!(page.events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                AgentEvent::TurnState { status, .. } if status == "waiting_approval"
            )
        }));
        assert!(!runtime.projections().list_for_task(&task).is_empty());
        server.join().unwrap();

        let approved = runtime
            .respond_approval_streaming(
                &session,
                &ProductApprovalDecision {
                    session_id: session.as_str().to_string(),
                    turn_id: "turn-a1".into(),
                    action_id: "call-approve-1".into(),
                    version: 1,
                    approved: true,
                },
            )
            .unwrap();
        assert!(approved.events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                AgentEvent::ToolCallCompleted { summary, .. }
                    if summary.contains("native.coding.fix")
            )
        }));
        assert!(approved.events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                AgentEvent::TurnState { status, .. } if status == "completed"
            )
        }));
        assert_eq!(
            approved
                .tool_summary
                .as_ref()
                .and_then(|v| v.get("official_servers"))
                .and_then(|v| v.as_u64()),
            Some(0)
        );
        let timeline = runtime.product_timeline_for_task(&task);
        assert!(timeline.iter().any(|event| event.kind == "tool"));
        assert!(timeline.iter().any(|event| {
            event.payload.get("productProjectionStore")
                == Some(&json!(lilia_contracts::PRODUCT_TIMELINE_STORE_ID))
        }));
        assert!(
            !runtime.product_pending_for_task(&task).is_empty(),
            "approval should project into pending surface"
        );
        let rebuilt = runtime
            .rebuild_product_timeline_for_session(&session)
            .unwrap();
        assert!(rebuilt >= timeline.len());
        let again = runtime
            .rebuild_product_timeline_for_session(&session)
            .unwrap();
        assert_eq!(again, rebuilt);
        assert_eq!(
            runtime.product_timeline_for_task(&task).len(),
            timeline.len()
        );
        assert!(!runtime.product_pending_for_task(&task).is_empty());
    }

    #[test]
    fn approval_deny_stops_tool_without_official_server() {
        use std::io::{Read, Write};

        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        runtime
            .credentials()
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: None,
                source: Some("user_api_key".into()),
            })
            .unwrap();
        let _ = runtime.refresh_product_profile(None).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 16_384];
            let _ = stream.read(&mut bytes).unwrap();
            let body = json!({
                "choices": [{
                    "finish_reason": "tool_calls",
                    "message": {
                        "content": null,
                        "tool_calls": [{
                            "id": "call-deny-1",
                            "type": "function",
                            "function": {
                                "name": "native.coding.fix",
                                "arguments": "{\"prompt\":\"fix\"}"
                            }
                        }]
                    }
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        runtime.set_model_endpoint_override(Some(format!("http://{address}/v1/chat/completions")));
        let task = TaskId::new("task-deny").unwrap();
        let session = runtime.start_session_for_task(&task, None).unwrap();
        runtime
            .submit_turn_streaming(&session, "deny me", "turn-d1")
            .unwrap();
        server.join().unwrap();
        let denied = runtime
            .respond_approval_streaming(
                &session,
                &ProductApprovalDecision {
                    session_id: session.as_str().to_string(),
                    turn_id: "turn-d1".into(),
                    action_id: "call-deny-1".into(),
                    version: 1,
                    approved: false,
                },
            )
            .unwrap();
        assert!(denied.events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                AgentEvent::ToolCallCompleted { summary, .. } if summary.contains("denied")
            )
        }));
        assert!(!denied.official_agent_server);
    }

    #[test]
    fn anthropic_credential_drives_live_messages_adapter_turn() {
        use mutsuki_agent_contracts::ANTHROPIC_CREDENTIAL_PROVIDER_ID;
        use std::io::{Read, Write};

        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        runtime
            .credentials()
            .login(ProductCredentialLoginInput {
                provider_id: ANTHROPIC_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-ant-api03-console-key-0123456789abcdef".into(),
                account_label: None,
                source: Some("anthropic_console".into()),
            })
            .unwrap();
        let profile = runtime.refresh_product_profile(None).unwrap();
        assert!(live_model_adapter_eligible(&profile));
        assert!(
            runtime
                .independent_diagnostics()
                .live_model_adapter_drives_turn
        );

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 16_384];
            let n = stream.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..n]);
            assert!(request.contains("x-api-key"));
            let payload = r#"{"content":[{"type":"text","text":"anthropic live reply"}],"stop_reason":"end_turn","usage":{"input_tokens":2,"output_tokens":3}}"#;
            let body = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
            stream.write_all(body.as_bytes()).unwrap();
        });
        runtime.set_anthropic_endpoint_override(Some(format!("http://{address}")));
        let task = TaskId::new("task-anthropic").unwrap();
        let session = runtime.start_session_for_task(&task, None).unwrap();
        let page = runtime
            .submit_turn_streaming(&session, "hello anthropic", "turn-ant-1")
            .unwrap();
        assert!(page.live_model_adapter_drives_turn);
        assert_eq!(
            page.tool_summary
                .as_ref()
                .and_then(|v| v.get("driver"))
                .and_then(|v| v.as_str()),
            Some("anthropic-messages")
        );
        assert!(page.events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                AgentEvent::ModelDelta { text, .. } if text.contains("anthropic live reply")
            )
        }));
        server.join().unwrap();
    }
}

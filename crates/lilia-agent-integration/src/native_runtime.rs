use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lilia_contracts::{
    AgentSessionRef, PendingProjectionStatus, ProductApprovalDecision, TaskId,
    TimelineProjectionCommand, TimelineProjectionEvent,
};
use lilia_core::{AgentKitClientPort, AgentKitPortError, NativeAgentCapabilitySnapshot};
use lilia_storage::{
    SqliteAgentRuntimeStateStore, SqliteTimelineProjectionStore, TimelineProjectionRepository,
};
use mutsuki_agent_bundle::{
    run_fix_golden_path, NativeCodingAgentBundle, NativeCodingBackends, NativeCodingRunContext,
    SessionStore, NATIVE_CODING_BUNDLE_ID,
};
use mutsuki_agent_contracts::{
    AgentError, AgentEvent, AgentEventEnvelope, AgentEventMeta, AgentMessage, AgentPermissionMode,
    AgentRole, AgentRunRequest, AgentRunResult, AgentRunStatus, AgentRuntimeProfile, AgentSession,
    AgentSessionAppendRequest, AgentSessionCreateRequest, AgentSessionForkRequest,
    AgentSessionGetRequest, AgentWorkspaceRef, PermissionDecision, PermissionDecisionKind,
    AGENT_RUN_PROTOCOL, AGENT_SESSION_APPEND_PROTOCOL, AGENT_SESSION_CREATE_PROTOCOL,
    AGENT_SESSION_FORK_PROTOCOL, AGENT_SESSION_GET_PROTOCOL,
};
use mutsuki_agent_runtime::{SessionEventSubscription, SessionPersistence};
use mutsuki_runtime_contracts::TaskHandle;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::agentkit_host::AgentKitHost;
use crate::credential::{IndependentDiagnostics, ProductCredentialBridge};
use crate::model_turn::{
    adapter_credential_broker, build_live_turn_plan, live_model_adapter_eligible,
    resolve_model_endpoint, LiveModelTurnPlan,
};
use crate::profile::{build_product_coding_profile, profile_has_credential_refs};
use crate::projection::project_agent_events;

const AGENTKIT_SESSION_PREFIX: &str = "agentkit-session/";
const WIRE_SESSION_PREFIX: &str = "wire-session/";
const TASK_TIMEOUT: Duration = Duration::from_secs(90);
const STREAM_WAIT_INTERVAL: Duration = Duration::from_millis(50);

type TurnEventObserver = Arc<dyn Fn(&[AgentEventEnvelope]) + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeRuntimeMode {
    Embedded,
    Service,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnCancellationDisposition {
    ActiveRun,
    PausedApproval,
    PendingRegistration,
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

    pub fn service_reference() -> Result<Self, NativeRuntimeError> {
        Self::reference_with_mode(NativeRuntimeMode::Service)
    }

    fn reference_with_mode(mode: NativeRuntimeMode) -> Result<Self, NativeRuntimeError> {
        let bundle =
            NativeCodingAgentBundle::reference(crate::host_backends::native_coding_backends());
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

    pub fn into_runtime_with_stores(
        self,
        projections: SqliteTimelineProjectionStore,
        runtime_state: SqliteAgentRuntimeStateStore,
    ) -> NativeAgentKitRuntime {
        NativeAgentKitRuntime::from_bootstrap_with_stores(self, projections, runtime_state)
    }

    pub fn run_reference_fix_smoke(&self) -> Result<Value, NativeRuntimeError> {
        let isolated = NativeCodingAgentBundle::reference(NativeCodingBackends::default());
        run_fix_golden_path(&isolated).map_err(Into::into)
    }

    pub fn product_profile(
        &self,
        workflow_kind: Option<&str>,
    ) -> Result<AgentRuntimeProfile, NativeRuntimeError> {
        build_product_coding_profile(&self.credentials, workflow_kind)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeTurnStreamPage {
    pub session_id: String,
    pub turn_id: String,
    pub events: Vec<AgentEventEnvelope>,
    pub next_sequence: u64,
    pub waiting_approval: bool,
    pub completed: bool,
    pub tool_summary: Option<Value>,
    pub official_agent_server: bool,
    pub credential_bound: bool,
    pub live_model_adapter_drives_turn: bool,
    pub profile_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ProductSessionBinding {
    task_id: String,
    profile_id: String,
}

#[derive(Clone)]
struct ActiveRun {
    session_id: String,
    turn_id: Option<String>,
    host: Arc<AgentKitHost>,
    handle: TaskHandle,
}

struct CachedHost {
    key: String,
    host: Arc<AgentKitHost>,
}

struct TurnEventObserverGuard<'a> {
    observers: &'a Mutex<BTreeMap<(String, String), TurnEventObserver>>,
    key: (String, String),
}

impl Drop for TurnEventObserverGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut observers) = self.observers.lock() {
            observers.remove(&self.key);
        }
    }
}

struct SqliteSessionPersistence {
    store: Arc<SqliteAgentRuntimeStateStore>,
}

impl SessionPersistence for SqliteSessionPersistence {
    fn load(&self) -> Result<Vec<AgentSession>, AgentError> {
        self.store
            .list_sessions()
            .map_err(product_store_error)?
            .into_iter()
            .filter_map(|(key, value)| {
                key.strip_prefix(AGENTKIT_SESSION_PREFIX)
                    .map(|_| serde_json::from_value(value).map_err(decode_session_error))
            })
            .collect()
    }

    fn store(&self, session: &AgentSession) -> Result<(), AgentError> {
        self.store
            .put_session(
                &format!("{AGENTKIT_SESSION_PREFIX}{}", session.session_id),
                &serde_json::to_value(session).map_err(decode_session_error)?,
            )
            .map_err(product_store_error)
    }
}

pub struct NativeAgentKitRuntime {
    bootstrap: NativeRuntimeBootstrap,
    bindings: Mutex<BTreeMap<String, ProductSessionBinding>>,
    projections: SqliteTimelineProjectionStore,
    runtime_state: Arc<SqliteAgentRuntimeStateStore>,
    product_profile: Mutex<Option<AgentRuntimeProfile>>,
    model_endpoint_override: Mutex<Option<String>>,
    anthropic_endpoint_override: Mutex<Option<String>>,
    host: Mutex<Option<CachedHost>>,
    active_runs: Mutex<BTreeMap<String, ActiveRun>>,
    pending_turn_cancellations: Mutex<BTreeSet<(String, String)>>,
    turn_event_observers: Mutex<BTreeMap<(String, String), TurnEventObserver>>,
    next_session: AtomicU64,
}

impl NativeAgentKitRuntime {
    pub fn from_bootstrap(bootstrap: NativeRuntimeBootstrap) -> Self {
        Self::from_bootstrap_with_stores(
            bootstrap,
            SqliteTimelineProjectionStore::open_in_memory()
                .expect("in-memory product projection store"),
            SqliteAgentRuntimeStateStore::open_in_memory().expect("in-memory AgentKit state store"),
        )
    }

    pub fn from_bootstrap_with_projections(
        bootstrap: NativeRuntimeBootstrap,
        projections: SqliteTimelineProjectionStore,
    ) -> Self {
        Self::from_bootstrap_with_stores(
            bootstrap,
            projections,
            SqliteAgentRuntimeStateStore::open_in_memory().expect("in-memory AgentKit state store"),
        )
    }

    pub fn from_bootstrap_with_stores(
        mut bootstrap: NativeRuntimeBootstrap,
        projections: SqliteTimelineProjectionStore,
        runtime_state: SqliteAgentRuntimeStateStore,
    ) -> Self {
        let runtime_state = Arc::new(runtime_state);
        bootstrap.bundle.core.sessions =
            SessionStore::with_persistence(Arc::new(SqliteSessionPersistence {
                store: runtime_state.clone(),
            }))
            .expect("restore AgentKit-owned durable sessions");
        let profile = build_product_coding_profile(bootstrap.credentials(), None).ok();
        Self {
            bootstrap,
            bindings: Mutex::new(BTreeMap::new()),
            projections,
            runtime_state,
            product_profile: Mutex::new(profile),
            model_endpoint_override: Mutex::new(None),
            anthropic_endpoint_override: Mutex::new(None),
            host: Mutex::new(None),
            active_runs: Mutex::new(BTreeMap::new()),
            pending_turn_cancellations: Mutex::new(BTreeSet::new()),
            turn_event_observers: Mutex::new(BTreeMap::new()),
            next_session: AtomicU64::new(1),
        }
    }

    pub fn set_model_endpoint_override(&self, endpoint: Option<String>) {
        if let Ok(mut guard) = self.model_endpoint_override.lock() {
            *guard = endpoint;
        }
        self.invalidate_host();
    }

    pub fn set_anthropic_endpoint_override(&self, endpoint: Option<String>) {
        if let Ok(mut guard) = self.anthropic_endpoint_override.lock() {
            *guard = endpoint;
        }
        self.invalidate_host();
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

    pub fn product_timeline_for_task(&self, task_id: &TaskId) -> Vec<TimelineProjectionEvent> {
        self.projections.list_for_task(task_id)
    }

    pub fn task_for_session(&self, session_id: &str) -> Result<TaskId, AgentKitPortError> {
        TaskId::new(self.binding(session_id)?.task_id)
            .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))
    }

    pub fn rebuild_product_timeline_for_session(
        &self,
        session: &AgentSessionRef,
    ) -> Result<usize, AgentKitPortError> {
        let snapshot = self.session_snapshot(session.as_str())?;
        let task_id = self.binding(session.as_str())?.task_id;
        let task = TaskId::new(task_id)
            .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))?;
        self.projections
            .rebuild_session(session, project_agent_events(&task, &snapshot.events))
            .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))
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
        IndependentDiagnostics {
            credential: self.credentials().health(),
            runtime_backend: caps.backend,
            runtime_ready: caps.supports_session && caps.supports_stream,
            official_agent_server: caps.official_agent_server,
            node_runner_default: caps.node_runner_default,
            profile_id: profile
                .as_ref()
                .map(|profile| profile.profile_id.clone())
                .or(caps.profile_id),
            profile_has_credential_refs: profile
                .as_ref()
                .map(profile_has_credential_refs)
                .unwrap_or(false),
            credential_and_runtime_independent: true,
            live_model_adapter_drives_turn: profile
                .as_ref()
                .map(live_model_adapter_eligible)
                .unwrap_or(false),
        }
    }

    pub fn native_quota_surface(&self) -> crate::NativeQuotaSurface {
        crate::NativeQuotaSurface::from_credential_health(self.credentials().health())
    }

    pub fn arc(self) -> Arc<Self> {
        Arc::new(self)
    }

    pub fn submit_turn_streaming(
        &self,
        session: &AgentSessionRef,
        prompt: &str,
        turn_id: &str,
    ) -> Result<NativeTurnStreamPage, AgentKitPortError> {
        self.submit_turn_with_context_streaming(session, prompt, turn_id, None)
    }

    pub fn with_turn_event_observer<T, O, F>(
        &self,
        session_id: &str,
        turn_id: &str,
        observer: O,
        action: F,
    ) -> Result<T, AgentKitPortError>
    where
        O: Fn(&[AgentEventEnvelope]) + Send + Sync + 'static,
        F: FnOnce() -> T,
    {
        let key = (session_id.to_string(), turn_id.to_string());
        {
            let mut observers = self.turn_event_observers.lock().map_err(|_| {
                AgentKitPortError::Unavailable("turn event observer lock poisoned".into())
            })?;
            if observers.insert(key.clone(), Arc::new(observer)).is_some() {
                return Err(AgentKitPortError::Unavailable(format!(
                    "turn event observer already registered for `{session_id}` / `{turn_id}`"
                )));
            }
        }
        let guard = TurnEventObserverGuard {
            observers: &self.turn_event_observers,
            key,
        };
        let result = action();
        drop(guard);
        Ok(result)
    }

    pub fn submit_turn_with_context_streaming(
        &self,
        session: &AgentSessionRef,
        prompt: &str,
        turn_id: &str,
        context: Option<Value>,
    ) -> Result<NativeTurnStreamPage, AgentKitPortError> {
        if prompt.trim().is_empty() || turn_id.trim().is_empty() {
            return Err(AgentKitPortError::InvalidInput(
                "prompt and turn_id are required".into(),
            ));
        }
        let binding = self.binding(session.as_str())?;
        self.gate_credentials_for_turn()?;
        let (plan, workspace) = self.turn_plan(context.as_ref())?;
        if let Some(workspace) = &workspace {
            self.prepare_native_coding_workspace(&workspace.root)
                .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))?;
        }
        let host = self.host_for_plan(Some(&plan), workspace.is_some())?;
        let mut messages = Vec::new();
        if let Some(context) = &context {
            messages.push(AgentMessage::system(format!(
                "Product-provided workspace and turn context (authoritative for this turn): {context}"
            )));
        }
        let mut user = AgentMessage::user(prompt);
        user.metadata = context.clone();
        messages.push(user);
        let mut request = AgentRunRequest::new(binding.profile_id.clone(), messages);
        request.session_id = Some(session.as_str().to_string());
        request.turn_id = Some(turn_id.to_string());
        request.model = Some(plan.model.clone());
        request.provider_hint = Some(plan.provider.provider_id.clone());
        request.permission_mode = permission_mode(context.as_ref());
        request.metadata = workspace.map(|workspace| {
            serde_json::to_value(NativeCodingRunContext {
                workspace,
                turn_id: turn_id.to_string(),
            })
            .expect("Native Coding run context serializes")
        });
        let result = self.run_agent(host, session.as_str(), request)?;
        self.page_from_result(session.as_str(), turn_id, &binding, &plan, true, result)
    }

    pub fn respond_approval_streaming(
        &self,
        session: &AgentSessionRef,
        decision: &ProductApprovalDecision,
    ) -> Result<NativeTurnStreamPage, AgentKitPortError> {
        if decision.session_id != session.as_str() {
            return Err(AgentKitPortError::InvalidInput(
                "approval session_id does not match target session".into(),
            ));
        }
        let binding = self.binding(session.as_str())?;
        self.gate_credentials_for_turn()?;
        let snapshot = self.session_snapshot(session.as_str())?;
        let context = snapshot
            .messages
            .iter()
            .rev()
            .find(|message| message.role == AgentRole::User)
            .and_then(|message| message.metadata.clone());
        let (plan, workspace) = self.turn_plan(context.as_ref())?;
        let host = self.host_for_plan(Some(&plan), workspace.is_some())?;
        let mut request = AgentRunRequest::new(binding.profile_id.clone(), Vec::new());
        request.session_id = Some(session.as_str().to_string());
        request.turn_id = Some(decision.turn_id.clone());
        request.model = Some(plan.model.clone());
        request.provider_hint = Some(plan.provider.provider_id.clone());
        request.permission_mode = permission_mode(context.as_ref());
        request.metadata = workspace.map(|workspace| {
            serde_json::to_value(NativeCodingRunContext {
                workspace,
                turn_id: decision.turn_id.clone(),
            })
            .expect("Native Coding run context serializes")
        });
        request.permission_decisions = vec![PermissionDecision {
            session_id: decision.session_id.clone(),
            turn_id: decision.turn_id.clone(),
            action_id: decision.action_id.clone(),
            version: decision.version,
            decision: if decision.approved {
                PermissionDecisionKind::Approved
            } else {
                PermissionDecisionKind::Rejected
            },
        }];
        let result = self.run_agent(host, session.as_str(), request)?;
        let page = self.page_from_result(
            session.as_str(),
            &decision.turn_id,
            &binding,
            &plan,
            true,
            result,
        )?;
        self.resolve_product_approval(&binding, decision, page.next_sequence)?;
        Ok(page)
    }

    pub fn events_after(
        &self,
        session: &AgentSessionRef,
        after_sequence: u64,
    ) -> Result<Vec<AgentEventEnvelope>, AgentKitPortError> {
        Ok(self
            .session_snapshot(session.as_str())?
            .events
            .into_iter()
            .filter(|event| event.sequence > after_sequence)
            .collect())
    }

    pub fn open_bound_session(
        &self,
        task_id: &TaskId,
        session_id: &str,
        profile_id: Option<&str>,
    ) -> Result<AgentSessionRef, AgentKitPortError> {
        if session_id.trim().is_empty() {
            return Err(AgentKitPortError::InvalidInput(
                "session_id is required".into(),
            ));
        }
        let profile_id = profile_id.map(str::to_string).unwrap_or_else(|| {
            self.current_product_profile()
                .map(|profile| profile.profile_id)
                .unwrap_or_else(|| self.bootstrap.bundle.profile.profile_id.clone())
        });
        {
            let bindings = self.bindings.lock().map_err(|_| {
                AgentKitPortError::Unavailable("session binding lock poisoned".into())
            })?;
            if let Some(existing) = bindings.get(session_id) {
                if existing.task_id != task_id.as_str() || existing.profile_id != profile_id {
                    return Err(AgentKitPortError::InvalidInput(format!(
                        "session `{session_id}` is already bound to a different task or profile"
                    )));
                }
                return AgentSessionRef::new(session_id.to_string())
                    .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()));
            }
        }
        let host = self.host_for_plan(None, false)?;
        let session: AgentSession = self.call(
            &host,
            "session-create",
            AGENT_SESSION_CREATE_PROTOCOL,
            AgentSessionCreateRequest {
                session_id: Some(session_id.to_string()),
                profile_id: profile_id.clone(),
                title: None,
            },
        )?;
        if session.session_id != session_id {
            return Err(AgentKitPortError::Unavailable(
                "AgentKit returned a mismatched session id".into(),
            ));
        }
        self.bindings
            .lock()
            .map_err(|_| AgentKitPortError::Unavailable("session binding lock poisoned".into()))?
            .insert(
                session_id.to_string(),
                ProductSessionBinding {
                    task_id: task_id.as_str().to_string(),
                    profile_id,
                },
            );
        AgentSessionRef::new(session_id.to_string())
            .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))
    }

    pub(crate) fn fork_session_state(
        &self,
        source_session_id: &str,
        target_session_id: &str,
    ) -> Result<(), AgentKitPortError> {
        let source = self.binding(source_session_id)?;
        let host = self.host_for_plan(None, false)?;
        let _: AgentSession = self.call(
            &host,
            "session-fork",
            AGENT_SESSION_FORK_PROTOCOL,
            AgentSessionForkRequest {
                source_session_id: source_session_id.to_string(),
                target_session_id: target_session_id.to_string(),
                title: None,
            },
        )?;
        self.bindings
            .lock()
            .map_err(|_| AgentKitPortError::Unavailable("session binding lock poisoned".into()))?
            .insert(target_session_id.to_string(), source);
        Ok(())
    }

    pub(crate) fn persist_wire_session(
        &self,
        session_id: &str,
        state: &Value,
    ) -> Result<(), AgentKitPortError> {
        self.runtime_state
            .put_session(&format!("{WIRE_SESSION_PREFIX}{session_id}"), state)
            .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))
    }

    pub(crate) fn persisted_wire_sessions(
        &self,
    ) -> Result<Vec<(String, Value)>, AgentKitPortError> {
        self.runtime_state
            .list_sessions()
            .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))
            .map(|rows| {
                rows.into_iter()
                    .filter_map(|(key, value)| {
                        key.strip_prefix(WIRE_SESSION_PREFIX)
                            .map(|session_id| (session_id.to_string(), value))
                    })
                    .collect()
            })
    }

    fn binding(&self, session_id: &str) -> Result<ProductSessionBinding, AgentKitPortError> {
        self.bindings
            .lock()
            .map_err(|_| AgentKitPortError::Unavailable("session binding lock poisoned".into()))?
            .get(session_id)
            .cloned()
            .ok_or_else(|| AgentKitPortError::NotFound(session_id.to_string()))
    }

    pub(crate) fn session_snapshot(
        &self,
        session_id: &str,
    ) -> Result<AgentSession, AgentKitPortError> {
        let host = self.host_for_plan(None, false)?;
        self.session_snapshot_on_host(&host, session_id)
    }

    fn session_snapshot_on_host(
        &self,
        host: &Arc<AgentKitHost>,
        session_id: &str,
    ) -> Result<AgentSession, AgentKitPortError> {
        self.call(
            host,
            "session-get",
            AGENT_SESSION_GET_PROTOCOL,
            AgentSessionGetRequest {
                session_id: session_id.to_string(),
            },
        )
    }

    fn call<T: for<'de> Deserialize<'de>>(
        &self,
        host: &Arc<AgentKitHost>,
        label: &str,
        protocol_id: &str,
        request: impl Serialize,
    ) -> Result<T, AgentKitPortError> {
        let payload = serde_json::to_value(request)
            .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))?;
        let handle = host
            .submit(label, protocol_id, payload)
            .map_err(agent_port_error)?;
        let output = host.wait(&handle, TASK_TIMEOUT).map_err(agent_port_error)?;
        serde_json::from_value(output)
            .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))
    }

    fn run_agent(
        &self,
        host: Arc<AgentKitHost>,
        session_id: &str,
        request: AgentRunRequest,
    ) -> Result<AgentRunResult, AgentKitPortError> {
        let turn_id = request.turn_id.clone();
        let mut observed_sequence = self
            .session_snapshot_on_host(&host, session_id)?
            .next_event_sequence;
        let subscription = self
            .bootstrap
            .bundle()
            .core
            .sessions
            .subscribe_events(session_id, observed_sequence)
            .map_err(agent_port_error)?;
        let handle = host
            .submit(
                "agent-run",
                AGENT_RUN_PROTOCOL,
                serde_json::to_value(request)
                    .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))?,
            )
            .map_err(agent_port_error)?;
        let task_id = handle.task_id.to_string();
        {
            let mut active = self
                .active_runs
                .lock()
                .map_err(|_| AgentKitPortError::Unavailable("active run lock poisoned".into()))?;
            active.insert(
                task_id.clone(),
                ActiveRun {
                    session_id: session_id.to_string(),
                    turn_id: turn_id.clone(),
                    host: host.clone(),
                    handle: handle.clone(),
                },
            );
        }
        let cancellation_result = if let Some(turn_id) = turn_id.as_deref() {
            let cancel_requested = self.take_pending_turn_cancellation(session_id, turn_id)?;
            if cancel_requested {
                self.cancel_active_runs(session_id, Some(turn_id))
                    .map(|_| ())
            } else {
                Ok(())
            }
        } else {
            Ok(())
        };
        let output = cancellation_result.and_then(|()| {
            self.wait_agent_and_publish_events(
                &host,
                &handle,
                session_id,
                turn_id.as_deref().unwrap_or_default(),
                &mut observed_sequence,
                &subscription,
            )
        });
        self.active_runs
            .lock()
            .map_err(|_| AgentKitPortError::Unavailable("active run lock poisoned".into()))?
            .remove(&task_id);
        if let Some(turn_id) = turn_id.as_deref() {
            self.take_pending_turn_cancellation(session_id, turn_id)?;
        }
        serde_json::from_value(output?)
            .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))
    }

    fn wait_agent_and_publish_events(
        &self,
        host: &Arc<AgentKitHost>,
        handle: &TaskHandle,
        session_id: &str,
        turn_id: &str,
        observed_sequence: &mut u64,
        subscription: &SessionEventSubscription,
    ) -> Result<Value, AgentKitPortError> {
        let deadline = Instant::now() + TASK_TIMEOUT;
        loop {
            match host.try_output(handle) {
                Ok(Some(output)) => {
                    self.drain_turn_events(session_id, turn_id, observed_sequence, subscription)?;
                    return Ok(output);
                }
                Ok(None) => {}
                Err(error) => {
                    self.drain_turn_events(session_id, turn_id, observed_sequence, subscription)?;
                    return Err(agent_port_error(error));
                }
            }
            if Instant::now() >= deadline {
                return Err(AgentKitPortError::Unavailable(
                    "AgentKit task did not reach a terminal state before the deadline".into(),
                ));
            }
            if let Some(events) = subscription
                .next_timeout(STREAM_WAIT_INTERVAL)
                .map_err(agent_port_error)?
            {
                self.publish_turn_event_batch(session_id, turn_id, observed_sequence, events)?;
            }
        }
    }

    fn drain_turn_events(
        &self,
        session_id: &str,
        turn_id: &str,
        observed_sequence: &mut u64,
        subscription: &SessionEventSubscription,
    ) -> Result<(), AgentKitPortError> {
        while let Some(events) = subscription
            .next_timeout(Duration::ZERO)
            .map_err(agent_port_error)?
        {
            self.publish_turn_event_batch(session_id, turn_id, observed_sequence, events)?;
        }
        Ok(())
    }

    fn publish_turn_event_batch(
        &self,
        session_id: &str,
        turn_id: &str,
        observed_sequence: &mut u64,
        events: Vec<AgentEventEnvelope>,
    ) -> Result<(), AgentKitPortError> {
        let events = events
            .into_iter()
            .filter(|event| event.sequence > *observed_sequence)
            .collect::<Vec<_>>();
        let Some(last_sequence) = events.last().map(|event| event.sequence) else {
            return Ok(());
        };
        let task_id = TaskId::new(self.binding(session_id)?.task_id)
            .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))?;
        for command in project_agent_events(&task_id, &events) {
            self.projections
                .apply(command)
                .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))?;
        }
        let observer = self
            .turn_event_observers
            .lock()
            .map_err(|_| {
                AgentKitPortError::Unavailable("turn event observer lock poisoned".into())
            })?
            .get(&(session_id.to_string(), turn_id.to_string()))
            .cloned();
        *observed_sequence = last_sequence;
        if let Some(observer) = observer {
            observer(&events);
        }
        Ok(())
    }

    fn cancel_active_runs(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
    ) -> Result<bool, AgentKitPortError> {
        let active = self
            .active_runs
            .lock()
            .map_err(|_| AgentKitPortError::Unavailable("active run lock poisoned".into()))?;
        let runs = active
            .values()
            .filter(|run| {
                run.session_id == session_id
                    && turn_id.is_none_or(|turn_id| run.turn_id.as_deref() == Some(turn_id))
            })
            .cloned()
            .collect::<Vec<_>>();
        drop(active);
        for run in &runs {
            run.host.cancel(&run.handle).map_err(agent_port_error)?;
            if let Some(turn_id) = run.turn_id.as_deref() {
                self.append_cancelled_turn_event(&run.host, &run.session_id, turn_id)?;
            }
        }
        Ok(!runs.is_empty())
    }

    fn request_turn_cancellation(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<(), AgentKitPortError> {
        self.pending_turn_cancellations
            .lock()
            .map_err(|_| {
                AgentKitPortError::Unavailable("pending turn cancellation lock poisoned".into())
            })?
            .insert((session_id.to_string(), turn_id.to_string()));
        Ok(())
    }

    fn take_pending_turn_cancellation(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<bool, AgentKitPortError> {
        self.pending_turn_cancellations
            .lock()
            .map_err(|_| {
                AgentKitPortError::Unavailable("pending turn cancellation lock poisoned".into())
            })
            .map(|mut pending| pending.remove(&(session_id.to_string(), turn_id.to_string())))
    }

    fn append_cancelled_turn_event(
        &self,
        host: &Arc<AgentKitHost>,
        session_id: &str,
        turn_id: &str,
    ) -> Result<Option<AgentEventEnvelope>, AgentKitPortError> {
        let snapshot = self.session_snapshot_on_host(host, session_id)?;
        if snapshot.events.iter().any(|event| {
            matches!(
                &event.event,
                AgentEvent::TurnState {
                    turn_id: event_turn_id,
                    status,
                } if event_turn_id == turn_id && status == "cancelled"
            )
        }) {
            return Ok(None);
        }
        let sequence = snapshot.next_event_sequence.saturating_add(1);
        let event = AgentEventEnvelope {
            session_id: session_id.to_string(),
            sequence,
            meta: AgentEventMeta::new(format!("{turn_id}:{sequence}"), "turn cancelled")
                .with_turn(turn_id),
            event: AgentEvent::TurnState {
                turn_id: turn_id.to_string(),
                status: "cancelled".into(),
            },
        };
        let _: AgentSession = self.call(
            host,
            "session-cancel-event",
            AGENT_SESSION_APPEND_PROTOCOL,
            AgentSessionAppendRequest {
                session_id: session_id.to_string(),
                messages: Vec::new(),
                events: vec![event.clone()],
                advance_turn: false,
            },
        )?;
        Ok(Some(event))
    }

    pub fn cancel_session_turn(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<TurnCancellationDisposition, AgentKitPortError> {
        let snapshot = self.session_snapshot(session_id)?;
        self.request_turn_cancellation(session_id, turn_id)?;
        if self.cancel_active_runs(session_id, Some(turn_id))? {
            self.take_pending_turn_cancellation(session_id, turn_id)?;
            return Ok(TurnCancellationDisposition::ActiveRun);
        }
        if turn_is_waiting_approval(&snapshot, turn_id) {
            let host = self.host_for_plan(None, false)?;
            if let Some(event) = self.append_cancelled_turn_event(&host, session_id, turn_id)? {
                self.project_and_observe_turn_events(
                    session_id,
                    turn_id,
                    std::slice::from_ref(&event),
                )?;
                self.resolve_open_pending_for_turn(
                    session_id,
                    turn_id,
                    PendingProjectionStatus::Cancelled,
                    event.sequence,
                    json!({ "cancelled": true }),
                )?;
            }
            self.take_pending_turn_cancellation(session_id, turn_id)?;
            return Ok(TurnCancellationDisposition::PausedApproval);
        }
        Ok(TurnCancellationDisposition::PendingRegistration)
    }

    fn project_and_observe_turn_events(
        &self,
        session_id: &str,
        turn_id: &str,
        events: &[AgentEventEnvelope],
    ) -> Result<(), AgentKitPortError> {
        let task_id = TaskId::new(self.binding(session_id)?.task_id)
            .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))?;
        for command in project_agent_events(&task_id, events) {
            self.projections
                .apply(command)
                .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))?;
        }
        let observer = self
            .turn_event_observers
            .lock()
            .map_err(|_| {
                AgentKitPortError::Unavailable("turn event observer lock poisoned".into())
            })?
            .get(&(session_id.to_string(), turn_id.to_string()))
            .cloned();
        if let Some(observer) = observer {
            observer(events);
        }
        Ok(())
    }

    fn resolve_product_approval(
        &self,
        binding: &ProductSessionBinding,
        decision: &ProductApprovalDecision,
        sequence: u64,
    ) -> Result<(), AgentKitPortError> {
        self.projections
            .apply(TimelineProjectionCommand::ResolvePending {
                session_id: decision.session_id.clone(),
                request_id: decision.action_id.clone(),
                status: if decision.approved {
                    PendingProjectionStatus::Resolved
                } else {
                    PendingProjectionStatus::Cancelled
                },
                sequence,
                response: json!({
                    "approved": decision.approved,
                    "turnId": decision.turn_id,
                    "actionRevision": decision.version,
                    "taskId": binding.task_id,
                }),
            })
            .map(|_| ())
            .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))
    }

    fn resolve_open_pending_for_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        status: PendingProjectionStatus,
        sequence: u64,
        response: Value,
    ) -> Result<(), AgentKitPortError> {
        let task_id = TaskId::new(self.binding(session_id)?.task_id)
            .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))?;
        for pending in self
            .projections
            .list_pending_for_task(&task_id)
            .into_iter()
            .filter(|pending| {
                pending.agent_session.as_str() == session_id
                    && pending.turn_id.as_deref() == Some(turn_id)
                    && pending.status == PendingProjectionStatus::Open
            })
        {
            self.projections
                .apply(TimelineProjectionCommand::ResolvePending {
                    session_id: session_id.to_string(),
                    request_id: pending.request_id,
                    status: status.clone(),
                    sequence,
                    response: response.clone(),
                })
                .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))?;
        }
        Ok(())
    }

    fn page_from_result(
        &self,
        session_id: &str,
        turn_id: &str,
        binding: &ProductSessionBinding,
        plan: &LiveModelTurnPlan,
        credential_bound: bool,
        result: AgentRunResult,
    ) -> Result<NativeTurnStreamPage, AgentKitPortError> {
        let task_id = TaskId::new(binding.task_id.clone())
            .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))?;
        for command in project_agent_events(&task_id, &result.events) {
            self.projections
                .apply(command)
                .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))?;
        }
        let next_sequence = result
            .events
            .last()
            .map(|event| event.sequence)
            .unwrap_or_else(|| {
                self.session_snapshot(session_id)
                    .map(|session| session.next_event_sequence)
                    .unwrap_or_default()
            });
        let executed = result
            .steps
            .iter()
            .filter(|step| step.kind == "tool_execute")
            .count();
        let model_steps = result
            .steps
            .iter()
            .filter(|step| step.kind.starts_with("model_"))
            .count();
        let blocked = result
            .steps
            .iter()
            .filter(|step| step.kind == "tool_blocked")
            .count();
        Ok(NativeTurnStreamPage {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            events: result.events,
            next_sequence,
            waiting_approval: result.status == AgentRunStatus::WaitingApproval,
            completed: result.status == AgentRunStatus::Completed,
            tool_summary: Some(json!({
                "driver": plan.driver.as_str(),
                "official_servers": 0,
                "waiting_approval": result.status == AgentRunStatus::WaitingApproval,
                "auto_executed": executed,
                "blocked": blocked,
                "model_steps": model_steps,
            })),
            official_agent_server: false,
            credential_bound,
            live_model_adapter_drives_turn: true,
            profile_id: binding.profile_id.clone(),
        })
    }

    fn turn_plan(
        &self,
        context: Option<&Value>,
    ) -> Result<(LiveModelTurnPlan, Option<AgentWorkspaceRef>), AgentKitPortError> {
        let profile = self
            .current_product_profile()
            .ok_or_else(|| AgentKitPortError::Unavailable("product profile missing".into()))?;
        let mut plan = build_live_turn_plan(
            &profile,
            &self.model_endpoint(),
            self.anthropic_endpoint().as_deref(),
        )
        .ok_or_else(|| {
            AgentKitPortError::Unavailable(
                "Native Coding turn requires a configured model provider credential".into(),
            )
        })?;
        if let Some(model) = context
            .and_then(|value| value.get("model"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|model| !model.is_empty() && *model != "auto")
        {
            plan.model = model.to_string();
        }
        let workspace = workspace_cwd(context).map(|root| AgentWorkspaceRef {
            workspace_id: root.clone(),
            root,
        });
        Ok((plan, workspace))
    }

    fn host_for_plan(
        &self,
        plan: Option<&LiveModelTurnPlan>,
        enable_tools: bool,
    ) -> Result<Arc<AgentKitHost>, AgentKitPortError> {
        let key = match plan {
            Some(plan) => format!(
                "{}:{enable_tools}:{}:{}",
                plan.driver.as_str(),
                plan.model,
                serde_json::to_string(&plan.provider)
                    .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))?
            ),
            None => "control".into(),
        };
        let mut cached = self
            .host
            .lock()
            .map_err(|_| AgentKitPortError::Unavailable("AgentKit Host lock poisoned".into()))?;
        if let Some(cached) = cached.as_ref().filter(|cached| cached.key == key) {
            return Ok(cached.host.clone());
        }
        let host = Arc::new(
            AgentKitHost::build(
                self.bootstrap.bundle.clone(),
                plan,
                adapter_credential_broker(self.credentials().broker().clone()),
                enable_tools,
            )
            .map_err(agent_port_error)?,
        );
        *cached = Some(CachedHost {
            key,
            host: host.clone(),
        });
        Ok(host)
    }

    fn invalidate_host(&self) {
        if let Ok(mut host) = self.host.lock() {
            *host = None;
        }
    }

    fn model_endpoint(&self) -> String {
        resolve_model_endpoint(
            self.model_endpoint_override
                .lock()
                .ok()
                .and_then(|value| value.clone())
                .as_deref(),
        )
    }

    fn anthropic_endpoint(&self) -> Option<String> {
        self.anthropic_endpoint_override
            .lock()
            .ok()
            .and_then(|value| value.clone())
    }

    fn gate_credentials_for_turn(&self) -> Result<bool, AgentKitPortError> {
        let profile = self
            .refresh_product_profile(None)
            .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))?;
        let mut bound = false;
        for provider in &profile.providers {
            if let Some(credential) = &provider.credential_ref {
                self.credentials()
                    .resolve_for_adapter(credential)
                    .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))?;
                bound = true;
            }
        }
        Ok(bound)
    }

    fn capabilities_inner(&self) -> NativeAgentCapabilitySnapshot {
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
            profile_id: self
                .current_product_profile()
                .map(|profile| profile.profile_id),
        }
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
        let ordinal = self.next_session.fetch_add(1, Ordering::Relaxed);
        self.open_bound_session(
            task_id,
            &format!("native-{}-{ordinal}", task_id.as_str()),
            profile_id,
        )
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
        if self.cancel_active_runs(session.as_str(), None)? {
            return Ok(());
        }
        self.session_snapshot(session.as_str()).map(|_| ())
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
        self.0.respond_approval(session, decision)
    }
}

fn workspace_cwd(context: Option<&Value>) -> Option<String> {
    context
        .and_then(|value| value.get("workspace"))
        .and_then(|value| value.get("folders"))
        .and_then(Value::as_array)
        .and_then(|folders| folders.first())
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn permission_mode(context: Option<&Value>) -> AgentPermissionMode {
    match context
        .and_then(|value| value.get("permission"))
        .and_then(Value::as_str)
    {
        Some("full" | "free") => AgentPermissionMode::Full,
        Some("readonly") => AgentPermissionMode::ReadOnly,
        _ => AgentPermissionMode::Ask,
    }
}

fn turn_is_waiting_approval(session: &AgentSession, turn_id: &str) -> bool {
    session
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.event {
            AgentEvent::TurnState {
                turn_id: event_turn_id,
                status,
            } if event_turn_id == turn_id => Some(status.as_str()),
            _ => None,
        })
        == Some("waiting_approval")
}

fn uuid_like_turn_id(session_id: &str, prompt: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    session_id.hash(&mut hasher);
    prompt.hash(&mut hasher);
    hasher.finish()
}

fn agent_port_error(error: AgentError) -> AgentKitPortError {
    if error.code.contains("not_found") {
        AgentKitPortError::NotFound(error.message)
    } else if error.code.contains("invalid") || error.code.contains("approval") {
        AgentKitPortError::InvalidInput(error.message)
    } else {
        AgentKitPortError::Unavailable(error.to_string())
    }
}

fn product_store_error(error: lilia_contracts::ProductError) -> AgentError {
    AgentError::new("agent.session.persistence", error.to_string())
}

fn decode_session_error(error: serde_json::Error) -> AgentError {
    AgentError::new("agent.session.persistence_decode", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::ProductCredentialLoginInput;
    use mutsuki_agent_contracts::{CredentialKind, OPENAI_CREDENTIAL_PROVIDER_ID};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestWorkspace(PathBuf);

    impl TestWorkspace {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "lilia-agentkit-{label}-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn configured_runtime(
        responses: Vec<Value>,
    ) -> (NativeAgentKitRuntime, std::thread::JoinHandle<()>) {
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
        runtime.refresh_product_profile(None).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut bytes = [0_u8; 32_768];
                let _ = stream.read(&mut bytes).unwrap();
                let body = response.to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        runtime.set_model_endpoint_override(Some(format!("http://{address}/v1/chat/completions")));
        (runtime, server)
    }

    fn write_call() -> Value {
        json!({
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "write-1",
                        "type": "function",
                        "function": {
                            "name": "computer.fs.write",
                            "arguments": "{\"path\":\"created.txt\",\"content\":\"agentkit\",\"create\":true}"
                        }
                    }]
                }
            }],
            "usage": {"prompt_tokens": 2, "completion_tokens": 1, "total_tokens": 3}
        })
    }

    fn final_response() -> Value {
        json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {"role": "assistant", "content": "done"}
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 1, "total_tokens": 4}
        })
    }

    fn session(runtime: &NativeAgentKitRuntime, suffix: &str) -> AgentSessionRef {
        runtime
            .open_bound_session(
                &TaskId::new(format!("task-{suffix}")).unwrap(),
                &format!("session-{suffix}"),
                Some("mutsuki.reference.coding-agent"),
            )
            .unwrap()
    }

    #[test]
    fn full_permission_runs_model_and_native_tool_only_through_agentkit_host() {
        let workspace = TestWorkspace::new("full");
        let (runtime, server) = configured_runtime(vec![write_call(), final_response()]);
        let session = session(&runtime, "full");
        let page = runtime
            .submit_turn_with_context_streaming(
                &session,
                "write the fixture",
                "turn-full",
                Some(json!({
                    "workspace": {"folders": [workspace.0.to_string_lossy()]},
                    "permission": "full"
                })),
            )
            .unwrap();
        server.join().unwrap();
        assert_eq!(
            std::fs::read_to_string(workspace.0.join("created.txt")).unwrap(),
            "agentkit"
        );
        assert_eq!(page.tool_summary.as_ref().unwrap()["auto_executed"], 1);
        assert_eq!(
            page.tool_summary.as_ref().unwrap()["waiting_approval"],
            false
        );
        assert!(page.events.iter().any(|event| matches!(
            event.event,
            mutsuki_agent_contracts::AgentEvent::FinalResponse { .. }
        )));
    }

    #[test]
    fn readonly_permission_returns_tool_error_to_model_without_side_effect() {
        let workspace = TestWorkspace::new("readonly");
        let (runtime, server) = configured_runtime(vec![write_call(), final_response()]);
        let session = session(&runtime, "readonly");
        let page = runtime
            .submit_turn_with_context_streaming(
                &session,
                "inspect without writing",
                "turn-readonly",
                Some(json!({
                    "workspace": {"folders": [workspace.0.to_string_lossy()]},
                    "permission": "readonly"
                })),
            )
            .unwrap();
        server.join().unwrap();
        assert!(!workspace.0.join("created.txt").exists());
        assert_eq!(page.tool_summary.as_ref().unwrap()["blocked"], 1);
        assert_eq!(
            page.tool_summary.as_ref().unwrap()["waiting_approval"],
            false
        );
    }

    #[test]
    fn ask_permission_pauses_and_resumes_same_agentkit_session() {
        let workspace = TestWorkspace::new("ask");
        let (runtime, server) = configured_runtime(vec![write_call(), final_response()]);
        let session = session(&runtime, "ask");
        let waiting = runtime
            .submit_turn_with_context_streaming(
                &session,
                "ask before writing",
                "turn-ask",
                Some(json!({
                    "workspace": {"folders": [workspace.0.to_string_lossy()]},
                    "permission": "ask"
                })),
            )
            .unwrap();
        assert_eq!(
            waiting.tool_summary.as_ref().unwrap()["waiting_approval"],
            true
        );
        let task = TaskId::new("task-ask").unwrap();
        assert!(runtime
            .product_pending_for_task(&task)
            .iter()
            .any(|pending| pending.status == PendingProjectionStatus::Open));
        let approval = waiting
            .events
            .iter()
            .find_map(|event| match &event.event {
                mutsuki_agent_contracts::AgentEvent::ApprovalRequest { request } => {
                    Some(request.clone())
                }
                _ => None,
            })
            .unwrap();
        assert!(!workspace.0.join("created.txt").exists());
        let resumed = runtime
            .respond_approval_streaming(
                &session,
                &ProductApprovalDecision {
                    session_id: approval.session_id,
                    turn_id: approval.turn_id,
                    action_id: approval.action_id,
                    version: approval.version,
                    approved: true,
                },
            )
            .unwrap();
        server.join().unwrap();
        assert_eq!(
            resumed.tool_summary.as_ref().unwrap()["waiting_approval"],
            false
        );
        assert!(resumed.completed);
        assert!(runtime
            .product_pending_for_task(&task)
            .iter()
            .any(|pending| pending.status == PendingProjectionStatus::Resolved));
        assert_eq!(
            std::fs::read_to_string(workspace.0.join("created.txt")).unwrap(),
            "agentkit"
        );
        let snapshot = runtime.session_snapshot(session.as_str()).unwrap();
        assert_eq!(snapshot.turn_count, 1);
        assert!(snapshot.next_event_sequence >= resumed.next_sequence);
    }

    #[test]
    fn cancelling_paused_approval_is_terminal_in_session_and_product_projection() {
        let workspace = TestWorkspace::new("cancel-paused-approval");
        let (runtime, server) = configured_runtime(vec![write_call()]);
        let session = session(&runtime, "cancel-paused-approval");
        let waiting = runtime
            .submit_turn_with_context_streaming(
                &session,
                "ask before writing",
                "turn-cancel-paused",
                Some(json!({
                    "workspace": {"folders": [workspace.0.to_string_lossy()]},
                    "permission": "ask"
                })),
            )
            .unwrap();
        server.join().unwrap();
        assert!(waiting.waiting_approval);
        let waiting_turn_count = runtime
            .session_snapshot(session.as_str())
            .unwrap()
            .turn_count;

        assert_eq!(
            runtime
                .cancel_session_turn(session.as_str(), "turn-cancel-paused")
                .unwrap(),
            TurnCancellationDisposition::PausedApproval
        );

        let snapshot = runtime.session_snapshot(session.as_str()).unwrap();
        assert_eq!(snapshot.turn_count, waiting_turn_count);
        assert!(snapshot.events.iter().any(|event| matches!(
            &event.event,
            AgentEvent::TurnState { turn_id, status }
                if turn_id == "turn-cancel-paused" && status == "cancelled"
        )));
        let task = TaskId::new("task-cancel-paused-approval").unwrap();
        assert!(runtime
            .product_pending_for_task(&task)
            .iter()
            .any(|pending| {
                pending.turn_id.as_deref() == Some("turn-cancel-paused")
                    && pending.status == PendingProjectionStatus::Cancelled
            }));
        assert!(runtime
            .product_timeline_for_task(&task)
            .iter()
            .any(|event| {
                event.turn_id.as_deref() == Some("turn-cancel-paused")
                    && event.payload.get("turnStatus").and_then(Value::as_str) == Some("cancelled")
            }));
        assert!(!workspace.0.join("created.txt").exists());
    }

    #[test]
    fn cancellation_before_host_task_registration_is_retained_once() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let session = session(&runtime, "cancel-before-registration");

        assert_eq!(
            runtime
                .cancel_session_turn(session.as_str(), "turn-before-registration")
                .unwrap(),
            TurnCancellationDisposition::PendingRegistration
        );
        assert_eq!(
            runtime
                .cancel_session_turn(session.as_str(), "turn-before-registration")
                .unwrap(),
            TurnCancellationDisposition::PendingRegistration
        );

        assert!(runtime
            .take_pending_turn_cancellation(session.as_str(), "turn-before-registration")
            .unwrap());
        assert!(!runtime
            .take_pending_turn_cancellation(session.as_str(), "turn-before-registration")
            .unwrap());
    }

    #[test]
    fn exact_turn_cancellation_stops_the_host_task_and_releases_the_session() {
        let runtime = Arc::new(
            NativeRuntimeBootstrap::embedded_reference()
                .unwrap()
                .into_runtime(),
        );
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
        runtime.refresh_product_profile(None).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (request_started_tx, request_started_rx) = mpsc::channel();
        let (release_response_tx, release_response_rx) = mpsc::channel();
        let (streamed_events_tx, streamed_events_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            for index in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut bytes = [0_u8; 32_768];
                let _ = stream.read(&mut bytes).unwrap();
                if index == 0 {
                    request_started_tx.send(()).unwrap();
                    release_response_rx.recv().unwrap();
                }
                let body = final_response().to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        runtime.set_model_endpoint_override(Some(format!("http://{address}/v1/chat/completions")));
        let session = session(&runtime, "cancel");
        let running_runtime = Arc::clone(&runtime);
        let running_session = session.clone();
        let running = std::thread::spawn(move || {
            running_runtime
                .with_turn_event_observer(
                    running_session.as_str(),
                    "turn-cancel",
                    move |events| {
                        streamed_events_tx.send(events.to_vec()).unwrap();
                    },
                    || {
                        running_runtime.submit_turn_with_context_streaming(
                            &running_session,
                            "wait for cancellation",
                            "turn-cancel",
                            Some(json!({"permission": "full"})),
                        )
                    },
                )
                .and_then(|result| result)
        });

        request_started_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let observed = streamed_events_rx
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
        let streamed = runtime.events_after(&session, 0).unwrap();
        assert!(streamed.iter().any(|event| matches!(
            &event.event,
            mutsuki_agent_contracts::AgentEvent::TurnState { status, .. }
                if status == "running"
        )));
        assert!(streamed.iter().any(|event| matches!(
            &event.event,
            mutsuki_agent_contracts::AgentEvent::StepState { status, .. }
                if status == "model_started"
        )));
        assert!(!streamed.iter().any(|event| matches!(
            event.event,
            mutsuki_agent_contracts::AgentEvent::FinalResponse { .. }
        )));
        let running_sequence = streamed
            .iter()
            .find_map(|event| match &event.event {
                mutsuki_agent_contracts::AgentEvent::TurnState { status, .. }
                    if status == "running" =>
                {
                    Some(event.sequence)
                }
                _ => None,
            })
            .unwrap();
        let projected_sequences = runtime
            .product_timeline_for_task(&TaskId::new("task-cancel").unwrap())
            .into_iter()
            .map(|event| event.sequence)
            .collect::<std::collections::BTreeSet<_>>();
        assert!(projected_sequences.contains(&running_sequence));
        assert_eq!(
            runtime
                .cancel_session_turn(session.as_str(), "turn-cancel")
                .unwrap(),
            TurnCancellationDisposition::ActiveRun
        );
        release_response_tx.send(()).unwrap();
        assert!(running.join().unwrap().is_err());
        let cancelled_events = runtime.events_after(&session, 0).unwrap();
        assert!(cancelled_events.iter().any(|event| {
            matches!(
                &event.event,
                mutsuki_agent_contracts::AgentEvent::TurnState {
                    turn_id,
                    status,
                } if turn_id == "turn-cancel" && status == "cancelled"
            )
        }));
        let cancelled_sequence = cancelled_events
            .iter()
            .find_map(|event| match &event.event {
                mutsuki_agent_contracts::AgentEvent::TurnState { turn_id, status }
                    if turn_id == "turn-cancel" && status == "cancelled" =>
                {
                    Some(event.sequence)
                }
                _ => None,
            })
            .unwrap();
        assert!(runtime
            .product_timeline_for_task(&TaskId::new("task-cancel").unwrap())
            .iter()
            .any(|event| event.sequence == cancelled_sequence));
        assert!(!cancelled_events.iter().any(|event| {
            matches!(
                &event.event,
                mutsuki_agent_contracts::AgentEvent::FinalResponse { turn_id, .. }
                    if turn_id == "turn-cancel"
            )
        }));

        let completed = runtime
            .submit_turn_with_context_streaming(
                &session,
                "run after cancellation",
                "turn-after-cancel",
                Some(json!({"permission": "full"})),
            )
            .unwrap();
        server.join().unwrap();
        assert!(completed.events.iter().any(|event| matches!(
            event.event,
            mutsuki_agent_contracts::AgentEvent::FinalResponse { .. }
        )));
        assert!(runtime.active_runs.lock().unwrap().is_empty());
    }
}

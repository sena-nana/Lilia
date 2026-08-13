use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::{
    AgentSessionRef, PendingProjectionStatus, ProductApprovalDecision, ProductResult, TaskId,
    TimelineProjectionCommand, TimelineProjectionCursor, TimelineProjectionEvent,
    TimelineProjectionPage,
};
use lilia_core::{AgentKitClientPort, AgentKitPortError, NativeAgentCapabilitySnapshot};
use lilia_storage::{
    SqliteAgentRuntimeStateStore, SqliteTimelineProjectionStore, TimelineProjectionRepository,
};
use mutsuki_agent_adapter_anthropic::AnthropicMessagesAdapter as MutsukiAnthropicMessagesAdapter;
use mutsuki_agent_adapter_api::ModelProtocolAdapter;
use mutsuki_agent_adapter_openai::OpenAiCompatibleAdapter;
use mutsuki_agent_bundle::{
    run_fix_golden_path, NativeCodingAgentBundle, NativeCodingBackends, NativeCodingRunContext,
    SessionStore, NATIVE_CODING_BUNDLE_ID,
};
use mutsuki_agent_contracts::{
    AgentError, AgentEvent, AgentEventEnvelope, AgentEventMeta, AgentMessage,
    AgentModelGenerateRequest, AgentPermissionMode, AgentRole, AgentRunRequest, AgentRunResult,
    AgentRunStatus, AgentRuntimeProfile, AgentSession, AgentSessionAppendRequest,
    AgentSessionCreateRequest, AgentSessionForkRequest, AgentSessionGetRequest, AgentToolCall,
    AgentToolResultMetadata, AgentUsage, AgentWorkspaceRef, InteractionRequest,
    InteractionResolution, ModelGenerateRequest, PermissionDecision, PermissionDecisionKind,
    AGENT_RUN_PROTOCOL, AGENT_SESSION_APPEND_PROTOCOL, AGENT_SESSION_CREATE_PROTOCOL,
    AGENT_SESSION_FORK_PROTOCOL, AGENT_SESSION_GET_PROTOCOL,
};
use mutsuki_agent_runtime::{
    SessionEventSubscription, SessionPersistence, TranscriptContextDisposition,
    TranscriptContextWindow,
};
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
use crate::subagent::NativeSubagentDefinition;

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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeModelRuntimeConfiguration {
    pub openai_endpoint_override: Option<String>,
    pub anthropic_endpoint_override: Option<String>,
    pub model_override: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeControlModelRequest {
    pub system_instruction: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default = "default_control_model_output_tokens")]
    pub max_output_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeControlModelResult {
    pub provider_id: String,
    pub model: String,
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeContextCompactionSource {
    pub source_session_id: String,
    pub profile_id: String,
    pub title: Option<String>,
    pub messages: Vec<AgentMessage>,
    pub source_message_count: usize,
    pub omitted_message_count: usize,
    pub estimated_tokens: u64,
    pub budget_satisfied: bool,
}

const fn default_control_model_output_tokens() -> u64 {
    600
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnCancellationDisposition {
    ActiveRun,
    PausedAction,
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

    pub fn embedded_reference_with_credentials(
        credentials: ProductCredentialBridge,
    ) -> Result<Self, NativeRuntimeError> {
        Self::reference_with_mode_and_credentials(NativeRuntimeMode::Embedded, credentials)
    }

    pub fn service_reference() -> Result<Self, NativeRuntimeError> {
        Self::reference_with_mode(NativeRuntimeMode::Service)
    }

    pub fn service_reference_with_credentials(
        credentials: ProductCredentialBridge,
    ) -> Result<Self, NativeRuntimeError> {
        Self::reference_with_mode_and_credentials(NativeRuntimeMode::Service, credentials)
    }

    fn reference_with_mode(mode: NativeRuntimeMode) -> Result<Self, NativeRuntimeError> {
        Self::reference_with_mode_and_credentials(mode, ProductCredentialBridge::new())
    }

    fn reference_with_mode_and_credentials(
        mode: NativeRuntimeMode,
        credentials: ProductCredentialBridge,
    ) -> Result<Self, NativeRuntimeError> {
        let bundle =
            NativeCodingAgentBundle::reference(crate::host_backends::native_coding_backends());
        bundle.assert_shared_service_identity()?;
        bundle.assert_no_official_agent_server_dependency()?;
        Ok(Self {
            mode,
            bundle,
            credentials,
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
    pub waiting_interaction: bool,
    pub completed: bool,
    pub cancelled: bool,
    pub tool_summary: Option<Value>,
    pub official_agent_server: bool,
    pub credential_bound: bool,
    pub live_model_adapter_drives_turn: bool,
    pub profile_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    model_runtime_configuration: Mutex<NativeModelRuntimeConfiguration>,
    subagent_configuration: Mutex<Vec<NativeSubagentDefinition>>,
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
            model_runtime_configuration: Mutex::new(NativeModelRuntimeConfiguration::default()),
            subagent_configuration: Mutex::new(Vec::new()),
            host: Mutex::new(None),
            active_runs: Mutex::new(BTreeMap::new()),
            pending_turn_cancellations: Mutex::new(BTreeSet::new()),
            turn_event_observers: Mutex::new(BTreeMap::new()),
            next_session: AtomicU64::new(1),
        }
    }

    pub fn set_model_endpoint_override(&self, endpoint: Option<String>) {
        if let Ok(mut configuration) = self.model_runtime_configuration.lock() {
            configuration.openai_endpoint_override = endpoint;
        }
        self.invalidate_host();
    }

    pub fn set_anthropic_endpoint_override(&self, endpoint: Option<String>) {
        if let Ok(mut configuration) = self.model_runtime_configuration.lock() {
            configuration.anthropic_endpoint_override = endpoint;
        }
        self.invalidate_host();
    }

    pub fn set_model_override(&self, model: Option<String>) {
        if let Ok(mut configuration) = self.model_runtime_configuration.lock() {
            configuration.model_override = model;
        }
        self.invalidate_host();
    }

    pub fn configure_model_runtime(
        &self,
        configuration: NativeModelRuntimeConfiguration,
    ) -> Result<(), NativeRuntimeError> {
        *self.model_runtime_configuration.lock().map_err(|_| {
            NativeRuntimeError::Agent("model runtime configuration lock poisoned".into())
        })? = configuration;
        self.invalidate_host();
        Ok(())
    }

    pub fn model_runtime_configuration(
        &self,
    ) -> Result<NativeModelRuntimeConfiguration, NativeRuntimeError> {
        self.model_runtime_configuration
            .lock()
            .map(|configuration| configuration.clone())
            .map_err(|_| {
                NativeRuntimeError::Agent("model runtime configuration lock poisoned".into())
            })
    }

    pub fn generate_control_text(
        &self,
        request: NativeControlModelRequest,
    ) -> Result<NativeControlModelResult, NativeRuntimeError> {
        if request.system_instruction.trim().is_empty() || request.prompt.trim().is_empty() {
            return Err(NativeRuntimeError::Agent(
                "control model system instruction and prompt are required".into(),
            ));
        }
        let turn_context = request
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(|model| json!({ "model": model }));
        let (mut plan, _) = self
            .turn_plan(turn_context.as_ref())
            .map_err(|error| NativeRuntimeError::Agent(error.to_string()))?;
        plan.provider
            .compatibility
            .insert("timeout_ms".into(), json!(12_000));
        plan.provider
            .compatibility
            .insert("max_retries".into(), json!(0));
        let credentials = adapter_credential_broker(self.credentials().broker().clone());
        let adapter: Arc<dyn ModelProtocolAdapter> = match plan.driver {
            crate::model_turn::LiveModelDriver::OpenAiCompatible => Arc::new(
                OpenAiCompatibleAdapter::new(
                    OpenAiCompatibleAdapter::default_descriptor(),
                    credentials,
                )
                .map_err(|error| NativeRuntimeError::Agent(protocol_error_message(&error)))?,
            ),
            crate::model_turn::LiveModelDriver::AnthropicMessages => Arc::new(
                MutsukiAnthropicMessagesAdapter::new(
                    MutsukiAnthropicMessagesAdapter::default_descriptor(),
                    credentials,
                )
                .map_err(|error| NativeRuntimeError::Agent(protocol_error_message(&error)))?,
            ),
        };
        let provider_id = plan.provider.provider_id.clone();
        let model = plan.model.clone();
        let generate = ModelGenerateRequest {
            request: AgentModelGenerateRequest {
                model: model.clone(),
                messages: vec![
                    AgentMessage::system(request.system_instruction),
                    AgentMessage::user(request.prompt),
                ],
                temperature: Some(0.1),
                max_output_tokens: Some(request.max_output_tokens.clamp(1, 4_096)),
                provider_hint: Some(provider_id.clone()),
                metadata: Some(json!({"purpose": "lilia-control-model"})),
                result_protocol_id: None,
                result_context: None,
                session_id: None,
            },
            tools: Vec::new(),
            structured_output: None,
            reasoning: request.reasoning.map(Value::String),
        };
        let provider = plan.provider;
        let worker = std::thread::Builder::new()
            .name("lilia-agentkit-control-model".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| error.to_string())?;
                runtime
                    .block_on(adapter.generate(provider, generate))
                    .map_err(|error| protocol_error_message(&error))
            })
            .map_err(|error| NativeRuntimeError::Agent(error.to_string()))?;
        let generated = worker
            .join()
            .map_err(|_| NativeRuntimeError::Agent("control model worker panicked".into()))?
            .map_err(NativeRuntimeError::Agent)?;
        if generated.message.content.trim().is_empty() {
            return Err(NativeRuntimeError::Agent(
                "control model response did not contain text".into(),
            ));
        }
        Ok(NativeControlModelResult {
            provider_id,
            model,
            text: generated.message.content,
            input_tokens: generated.usage.input_tokens,
            output_tokens: generated.usage.output_tokens,
        })
    }

    pub fn prepare_product_session_compaction(
        &self,
        source_session_id: &str,
        max_context_tokens: u64,
    ) -> Result<NativeContextCompactionSource, AgentKitPortError> {
        if max_context_tokens == 0 {
            return Err(AgentKitPortError::InvalidInput(
                "context compaction token budget must be greater than zero".into(),
            ));
        }
        self.binding(source_session_id)?;
        let snapshot = self.session_snapshot(source_session_id)?;
        let profile_prompt = self
            .current_product_profile()
            .filter(|profile| profile.profile_id == snapshot.profile_id)
            .map(|profile| render_profile_prompt(&profile))
            .unwrap_or_default();
        let source_messages = snapshot
            .messages
            .iter()
            .filter(|message| {
                !(message.role == AgentRole::System
                    && !profile_prompt.is_empty()
                    && message.content == profile_prompt)
            })
            .cloned()
            .collect::<Vec<_>>();
        if source_messages.is_empty() {
            return Err(AgentKitPortError::InvalidInput(
                "Agent session has no conversation context to compact".into(),
            ));
        }
        let source_message_count = source_messages.len();
        let prepared = TranscriptContextWindow.prepare(&source_messages, Some(max_context_tokens));
        let omitted_message_count = match prepared.disposition {
            TranscriptContextDisposition::Unchanged => 0,
            TranscriptContextDisposition::Compacted { dropped_messages } => dropped_messages,
        };
        Ok(NativeContextCompactionSource {
            source_session_id: source_session_id.to_owned(),
            profile_id: snapshot.profile_id,
            title: snapshot.title,
            messages: prepared.messages,
            source_message_count,
            omitted_message_count,
            estimated_tokens: prepared.estimated_tokens,
            budget_satisfied: prepared.budget_satisfied,
        })
    }

    pub fn create_compacted_product_session(
        &self,
        task_id: &TaskId,
        source: &NativeContextCompactionSource,
        target_session_id: &str,
        turn_id: &str,
        generated: &NativeControlModelResult,
        confirmation: &str,
    ) -> Result<AgentSession, AgentKitPortError> {
        if target_session_id.trim().is_empty() || target_session_id == source.source_session_id {
            return Err(AgentKitPortError::InvalidInput(
                "context compaction requires a distinct target session id".into(),
            ));
        }
        if turn_id.trim().is_empty()
            || generated.text.trim().is_empty()
            || confirmation.trim().is_empty()
        {
            return Err(AgentKitPortError::InvalidInput(
                "context compaction turn, summary and confirmation are required".into(),
            ));
        }
        let source_binding = self.binding(&source.source_session_id)?;
        if source_binding.task_id != task_id.as_str()
            || source_binding.profile_id != source.profile_id
        {
            return Err(AgentKitPortError::InvalidInput(
                "context compaction source does not match its product binding".into(),
            ));
        }
        self.open_bound_session_with_title(
            task_id,
            target_session_id,
            Some(&source.profile_id),
            source.title.clone(),
        )?;
        let host = self.host_for_plan(None, false)?;
        let target = self.session_snapshot_on_host(&host, target_session_id)?;
        let mut messages = Vec::new();
        if let Some(prompt) = self
            .current_product_profile()
            .filter(|profile| profile.profile_id == source.profile_id)
            .map(|profile| render_profile_prompt(&profile))
            .filter(|prompt| !prompt.is_empty())
        {
            messages.push(AgentMessage::system(prompt));
        }
        let mut summary = AgentMessage::system(generated.text.trim());
        summary.metadata = Some(json!({
            "context_compaction": {
                "strategy": "lilia_model_summary_v1",
                "source_session_id": source.source_session_id,
                "source_message_count": source.source_message_count,
                "omitted_message_count": source.omitted_message_count,
                "provider_id": generated.provider_id,
                "model": generated.model,
                "input_tokens": generated.input_tokens,
                "output_tokens": generated.output_tokens,
            }
        }));
        messages.push(summary);
        messages.push(AgentMessage::assistant(confirmation.trim()));

        let mut sequence = target.next_event_sequence;
        let mut next_event = |summary: &str, event| {
            sequence = sequence.saturating_add(1);
            AgentEventEnvelope {
                session_id: target_session_id.to_owned(),
                sequence,
                meta: timestamped_event_meta(
                    format!("{turn_id}:context-compaction:{sequence}"),
                    summary,
                    turn_id,
                ),
                event,
            }
        };
        let events = vec![
            next_event(
                "context compaction started",
                AgentEvent::TurnState {
                    turn_id: turn_id.to_owned(),
                    status: "running".into(),
                },
            ),
            next_event(
                "context compaction usage",
                AgentEvent::Usage {
                    turn_id: turn_id.to_owned(),
                    usage: AgentUsage {
                        input_tokens: generated.input_tokens,
                        output_tokens: generated.output_tokens,
                        total_tokens: generated
                            .input_tokens
                            .saturating_add(generated.output_tokens),
                    },
                },
            ),
            next_event(
                "context compaction completed",
                AgentEvent::FinalResponse {
                    turn_id: turn_id.to_owned(),
                    summary: confirmation.trim().to_owned(),
                    result: None,
                },
            ),
            next_event(
                "context compaction turn completed",
                AgentEvent::TurnState {
                    turn_id: turn_id.to_owned(),
                    status: "completed".into(),
                },
            ),
        ];
        let compacted: AgentSession = self.call(
            &host,
            "session-context-compaction",
            AGENT_SESSION_APPEND_PROTOCOL,
            AgentSessionAppendRequest {
                session_id: target_session_id.to_owned(),
                messages,
                events: events.clone(),
                advance_turn: true,
            },
        )?;
        self.project_and_observe_turn_events(target_session_id, turn_id, &events)?;
        Ok(compacted)
    }

    pub fn configure_subagents(
        &self,
        definitions: Vec<NativeSubagentDefinition>,
    ) -> Result<(), NativeRuntimeError> {
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for definition in &definitions {
            definition
                .validate()
                .map_err(|message| NativeRuntimeError::Agent(message.into()))?;
            if !ids.insert(definition.id.trim().to_owned()) {
                return Err(NativeRuntimeError::Agent(format!(
                    "duplicate custom subagent id `{}`",
                    definition.id
                )));
            }
            let normalized_name = definition.name.trim().to_lowercase();
            if !names.insert(normalized_name) {
                return Err(NativeRuntimeError::Agent(format!(
                    "duplicate custom subagent name `{}`",
                    definition.name
                )));
            }
        }
        *self.subagent_configuration.lock().map_err(|_| {
            NativeRuntimeError::Agent("subagent configuration lock poisoned".into())
        })? = definitions;
        self.invalidate_host();
        Ok(())
    }

    pub fn subagent_configuration(
        &self,
    ) -> Result<Vec<NativeSubagentDefinition>, NativeRuntimeError> {
        self.subagent_configuration
            .lock()
            .map(|definitions| definitions.clone())
            .map_err(|_| NativeRuntimeError::Agent("subagent configuration lock poisoned".into()))
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

    pub fn product_open_pending(&self) -> Vec<lilia_contracts::PendingProjection> {
        self.projections.list_open_pending()
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

    pub fn product_timeline_page_before(
        &self,
        task_id: &TaskId,
        before: Option<&TimelineProjectionCursor>,
        limit: usize,
    ) -> ProductResult<TimelineProjectionPage> {
        self.projections
            .list_task_page_before(task_id, before, limit)
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
        {
            *self
                .product_profile
                .lock()
                .map_err(|_| NativeRuntimeError::Agent("product profile lock poisoned".into()))? =
                Some(profile.clone());
        }
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
        let credential_bound = self.gate_credentials_for_turn()?;
        let (plan, workspace) = self.turn_plan(context.as_ref())?;
        if let Some(workspace) = &workspace {
            self.prepare_native_coding_workspace(&workspace.root)
                .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))?;
        }
        let host = self.host_for_plan(Some(&plan), workspace.is_some())?;
        let snapshot = self.session_snapshot_on_host(&host, session.as_str())?;
        if let Some((persisted_prompt, persisted_context)) =
            persisted_turn_user_request(&snapshot, turn_id)
        {
            if persisted_prompt != prompt || persisted_context != context.as_ref() {
                return Err(AgentKitPortError::InvalidInput(format!(
                    "turn_id `{turn_id}` is already bound to a different request"
                )));
            }
        }
        if let Some(status) = persisted_turn_status(&snapshot, turn_id) {
            if matches!(
                status,
                "completed"
                    | "cancelled"
                    | "failed"
                    | "budget_exceeded"
                    | "waiting_approval"
                    | "waiting_interaction"
            ) {
                return Ok(self.page_from_persisted_turn(
                    &snapshot,
                    turn_id,
                    &binding,
                    &plan,
                    credential_bound,
                    status,
                ));
            }
        }
        let resume_without_prompt = persisted_turn_has_durable_progress(&snapshot, turn_id);
        let mut messages = Vec::new();
        if !resume_without_prompt {
            let system_prompt = self
                .current_product_profile()
                .as_ref()
                .map(render_profile_prompt)
                .unwrap_or_default();
            if !system_prompt.is_empty()
                && !snapshot.messages.iter().any(|message| {
                    message.role == AgentRole::System && message.content == system_prompt
                })
            {
                messages.push(AgentMessage::system(system_prompt));
            }
            if let Some(context) = &context {
                messages.push(AgentMessage::system(format!(
                    "Product-provided workspace and turn context (authoritative for this turn): {context}"
                )));
            }
            let mut user = AgentMessage::user(prompt);
            user.metadata = context.clone();
            messages.push(user);
        }
        let mut request = AgentRunRequest::new(binding.profile_id.clone(), messages);
        request.session_id = Some(session.as_str().to_string());
        request.turn_id = Some(turn_id.to_string());
        request.model = Some(plan.model.clone());
        request.provider_hint = Some(plan.provider.provider_id.clone());
        request.budget.max_context_tokens = plan.input_context_token_budget();
        request.permission_mode = permission_mode(context.as_ref());
        request.metadata = agent_run_metadata(workspace, turn_id, context.as_ref());
        let result = self.run_agent(host, session.as_str(), request)?;
        self.page_from_result(
            session.as_str(),
            turn_id,
            &binding,
            &plan,
            credential_bound,
            result,
        )
    }

    fn page_from_persisted_turn(
        &self,
        session: &AgentSession,
        turn_id: &str,
        binding: &ProductSessionBinding,
        plan: &LiveModelTurnPlan,
        credential_bound: bool,
        status: &str,
    ) -> NativeTurnStreamPage {
        let events = session
            .events
            .iter()
            .filter(|event| event.meta.turn_id.as_deref() == Some(turn_id))
            .cloned()
            .collect::<Vec<_>>();
        let completed_tools = events
            .iter()
            .filter(|event| matches!(event.event, AgentEvent::ToolCallCompleted { .. }))
            .count();
        NativeTurnStreamPage {
            session_id: session.session_id.clone(),
            turn_id: turn_id.to_owned(),
            events,
            next_sequence: session.next_event_sequence,
            waiting_approval: status == "waiting_approval",
            waiting_interaction: status == "waiting_interaction",
            completed: status == "completed",
            cancelled: status == "cancelled",
            tool_summary: Some(json!({
                "driver": plan.driver.as_str(),
                "official_servers": 0,
                "waiting_approval": status == "waiting_approval",
                "waiting_interaction": status == "waiting_interaction",
                "auto_executed": completed_tools,
                "blocked": 0,
                "model_steps": 0,
                "replayed": true,
            })),
            official_agent_server: false,
            credential_bound,
            live_model_adapter_drives_turn: credential_bound,
            profile_id: binding.profile_id.clone(),
        }
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
        let credential_bound = self.gate_credentials_for_turn()?;
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
        request.budget.max_context_tokens = plan.input_context_token_budget();
        request.permission_mode = permission_mode(context.as_ref());
        request.metadata = agent_run_metadata(workspace, &decision.turn_id, context.as_ref());
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
        let result = self.run_agent(Arc::clone(&host), session.as_str(), request)?;
        let mut page = self.page_from_result(
            session.as_str(),
            &decision.turn_id,
            &binding,
            &plan,
            credential_bound,
            result,
        )?;
        if !decision.approved {
            if let Some(event) =
                self.append_cancelled_turn_event(&host, session.as_str(), &decision.turn_id)?
            {
                self.project_and_observe_turn_events(
                    session.as_str(),
                    &decision.turn_id,
                    std::slice::from_ref(&event),
                )?;
                self.resolve_open_pending_for_turn(
                    session.as_str(),
                    &decision.turn_id,
                    PendingProjectionStatus::Cancelled,
                    event.sequence,
                    json!({ "approved": false, "cancelled": true }),
                )?;
                page.next_sequence = event.sequence;
                page.events.push(event);
            }
            page.waiting_approval = false;
            page.waiting_interaction = false;
            page.completed = false;
            page.cancelled = true;
        }
        self.resolve_product_approval(&binding, decision, page.next_sequence)?;
        Ok(page)
    }

    pub fn respond_interaction_streaming(
        &self,
        session: &AgentSessionRef,
        resolution: InteractionResolution,
    ) -> Result<NativeTurnStreamPage, AgentKitPortError> {
        self.respond_interactions_streaming(session, vec![resolution])
    }

    pub fn respond_interactions_streaming(
        &self,
        session: &AgentSessionRef,
        resolutions: Vec<InteractionResolution>,
    ) -> Result<NativeTurnStreamPage, AgentKitPortError> {
        let first = resolutions.first().ok_or_else(|| {
            AgentKitPortError::InvalidInput(
                "at least one interaction resolution is required".into(),
            )
        })?;
        let turn_id = first.turn_id.clone();
        if resolutions
            .iter()
            .any(|resolution| resolution.session_id != session.as_str())
        {
            return Err(AgentKitPortError::InvalidInput(
                "interaction session_id does not match target session".into(),
            ));
        }
        if first.turn_id.trim().is_empty()
            || resolutions.iter().any(|resolution| {
                resolution.turn_id != first.turn_id || resolution.interaction_id.trim().is_empty()
            })
        {
            return Err(AgentKitPortError::InvalidInput(
                "interaction resolutions must have one non-empty turn_id and interaction_id".into(),
            ));
        }
        let binding = self.binding(session.as_str())?;
        let credential_bound = self.gate_credentials_for_turn()?;
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
        request.session_id = Some(session.as_str().to_owned());
        request.turn_id = Some(turn_id.clone());
        request.model = Some(plan.model.clone());
        request.provider_hint = Some(plan.provider.provider_id.clone());
        request.budget.max_context_tokens = plan.input_context_token_budget();
        request.permission_mode = permission_mode(context.as_ref());
        request.metadata = agent_run_metadata(workspace, &turn_id, context.as_ref());
        request.interaction_resolutions = resolutions;
        let result = self.run_agent(host, session.as_str(), request)?;
        self.page_from_result(
            session.as_str(),
            &turn_id,
            &binding,
            &plan,
            credential_bound,
            result,
        )
    }

    pub fn request_interaction(
        &self,
        session: &AgentSessionRef,
        turn_id: &str,
        interaction: InteractionRequest,
    ) -> Result<AgentEventEnvelope, AgentKitPortError> {
        if interaction.interaction_id.trim().is_empty() {
            return Err(AgentKitPortError::InvalidInput(
                "interaction_id is required".into(),
            ));
        }
        if interaction.prompt.trim().is_empty() {
            return Err(AgentKitPortError::InvalidInput(
                "interaction prompt is required".into(),
            ));
        }
        self.binding(session.as_str())?;
        let host = self.host_for_plan(None, false)?;
        let snapshot = self.session_snapshot_on_host(&host, session.as_str())?;
        if let Some(existing) = snapshot.events.iter().find(|event| {
            matches!(
                &event.event,
                AgentEvent::InteractionRequested {
                    turn_id: event_turn_id,
                    interaction: existing,
                } if event_turn_id == turn_id
                    && existing.interaction_id == interaction.interaction_id
            )
        }) {
            if matches!(
                &existing.event,
                AgentEvent::InteractionRequested {
                    interaction: existing,
                    ..
                } if existing == &interaction
            ) {
                return Ok(existing.clone());
            }
            return Err(AgentKitPortError::InvalidInput(format!(
                "interaction `{}` was already requested with different content",
                interaction.interaction_id
            )));
        }
        self.append_interaction_event(
            &host,
            session.as_str(),
            turn_id,
            "interaction requested",
            AgentEvent::InteractionRequested {
                turn_id: turn_id.to_owned(),
                interaction,
            },
        )
    }

    pub fn respond_interaction(
        &self,
        session: &AgentSessionRef,
        turn_id: &str,
        resolution: InteractionResolution,
    ) -> Result<AgentEventEnvelope, AgentKitPortError> {
        if resolution.interaction_id.trim().is_empty() {
            return Err(AgentKitPortError::InvalidInput(
                "interaction_id is required".into(),
            ));
        }
        self.binding(session.as_str())?;
        let host = self.host_for_plan(None, false)?;
        let snapshot = self.session_snapshot_on_host(&host, session.as_str())?;
        let requested = snapshot.events.iter().any(|event| {
            matches!(
                &event.event,
                AgentEvent::InteractionRequested {
                    turn_id: event_turn_id,
                    interaction,
                } if event_turn_id == turn_id
                    && interaction.interaction_id == resolution.interaction_id
            )
        });
        if !requested {
            return Err(AgentKitPortError::NotFound(format!(
                "interaction `{}` for turn `{turn_id}`",
                resolution.interaction_id
            )));
        }
        if let Some(existing) = snapshot.events.iter().find(|event| {
            matches!(
                &event.event,
                AgentEvent::InteractionResolved {
                    turn_id: event_turn_id,
                    resolution: existing,
                } if event_turn_id == turn_id
                    && existing.interaction_id == resolution.interaction_id
            )
        }) {
            if matches!(
                &existing.event,
                AgentEvent::InteractionResolved {
                    resolution: existing,
                    ..
                } if existing == &resolution
            ) {
                return Ok(existing.clone());
            }
            return Err(AgentKitPortError::InvalidInput(format!(
                "interaction `{}` was already resolved with a different response",
                resolution.interaction_id
            )));
        }
        self.append_interaction_event(
            &host,
            session.as_str(),
            turn_id,
            "interaction resolved",
            AgentEvent::InteractionResolved {
                turn_id: turn_id.to_owned(),
                resolution,
            },
        )
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

    #[cfg(debug_assertions)]
    pub fn seed_debug_interaction(
        &self,
        task_id: &TaskId,
        session_id: &str,
        turn_id: &str,
        interaction: InteractionRequest,
    ) -> Result<AgentEventEnvelope, AgentKitPortError> {
        if interaction.session_id != session_id
            || interaction.turn_id != turn_id
            || interaction.interaction_id.trim().is_empty()
        {
            return Err(AgentKitPortError::InvalidInput(
                "debug interaction identity does not match its session and turn".into(),
            ));
        }
        match self.session_snapshot(session_id) {
            Ok(_) => {
                self.restore_product_session_binding(task_id, session_id, None)?;
            }
            Err(AgentKitPortError::NotFound(_)) => {
                self.open_bound_session(task_id, session_id, None)?;
            }
            Err(error) => return Err(error),
        }
        if let Some(existing) =
            self.session_snapshot(session_id)?
                .events
                .into_iter()
                .find(|event| {
                    matches!(
                        &event.event,
                        AgentEvent::InteractionRequested {
                            turn_id: event_turn_id,
                            interaction: existing,
                        } if event_turn_id == turn_id
                            && existing.interaction_id == interaction.interaction_id
                    )
                })
        {
            return Ok(existing);
        }
        let host = self.host_for_plan(None, false)?;
        self.append_interaction_event(
            &host,
            session_id,
            turn_id,
            "debug interaction requested",
            AgentEvent::InteractionRequested {
                turn_id: turn_id.to_owned(),
                interaction,
            },
        )
    }

    #[cfg(debug_assertions)]
    pub fn seed_interrupted_tool_for_debug(
        &self,
        task_id: &TaskId,
        session_id: &str,
        turn_id: &str,
        user_message: AgentMessage,
        tool_call: AgentToolCall,
    ) -> Result<Vec<AgentEventEnvelope>, AgentKitPortError> {
        if turn_id.trim().is_empty()
            || tool_call.call_id.trim().is_empty()
            || user_message.role != AgentRole::User
            || user_message.content.trim().is_empty()
        {
            return Err(AgentKitPortError::InvalidInput(
                "debug interrupted tool requires a turn, call and user message".into(),
            ));
        }
        self.restore_product_session_binding(task_id, session_id, None)?;
        let host = self.host_for_plan(None, false)?;
        let snapshot = self.session_snapshot_on_host(&host, session_id)?;
        if let Some(existing) = snapshot.events.iter().find(|event| {
            matches!(
                &event.event,
                AgentEvent::ToolCallStarted {
                    turn_id: event_turn_id,
                    call_id,
                    name,
                    input,
                } if event_turn_id == turn_id
                    && call_id == &tool_call.call_id
                    && name == &tool_call.name
                    && input == &tool_call.input
            )
        }) {
            return Ok(vec![existing.clone()]);
        }
        if snapshot
            .events
            .iter()
            .any(|event| event.meta.turn_id.as_deref() == Some(turn_id))
        {
            return Err(AgentKitPortError::InvalidInput(format!(
                "debug turn `{turn_id}` already has different durable state"
            )));
        }
        let defaults = AgentRunRequest::new(snapshot.profile_id.clone(), Vec::new());
        let mut assistant = AgentMessage::assistant(String::new());
        assistant.metadata = Some(json!({
            "tool_calls": [tool_call.clone()],
            "run_continuation": {
                "next_step_index": 1,
                "max_steps": defaults.max_steps,
                "budget": defaults.budget,
                "usage": mutsuki_agent_contracts::AgentUsage::default(),
                "cost_microunits": 0
            }
        }));
        let mut sequence = snapshot.next_event_sequence;
        let mut next_event = |summary: &str, event| {
            sequence = sequence.saturating_add(1);
            AgentEventEnvelope {
                session_id: session_id.to_owned(),
                sequence,
                meta: timestamped_event_meta(format!("{turn_id}:{sequence}"), summary, turn_id),
                event,
            }
        };
        let events = vec![
            next_event(
                "turn started",
                AgentEvent::TurnState {
                    turn_id: turn_id.to_owned(),
                    status: "running".into(),
                },
            ),
            next_event(
                "user message",
                AgentEvent::UserMessage {
                    turn_id: turn_id.to_owned(),
                    content: user_message.content.clone(),
                    metadata: user_message.metadata.clone(),
                },
            ),
            next_event(
                "tool call started",
                AgentEvent::ToolCallStarted {
                    turn_id: turn_id.to_owned(),
                    call_id: tool_call.call_id,
                    name: tool_call.name,
                    input: tool_call.input,
                },
            ),
        ];
        let _: AgentSession = self.call(
            &host,
            "session-seed-interrupted-tool",
            AGENT_SESSION_APPEND_PROTOCOL,
            AgentSessionAppendRequest {
                session_id: session_id.to_owned(),
                messages: vec![user_message, assistant],
                events: events.clone(),
                advance_turn: false,
            },
        )?;
        self.project_and_observe_turn_events(session_id, turn_id, &events)?;
        Ok(events)
    }

    pub fn open_bound_session(
        &self,
        task_id: &TaskId,
        session_id: &str,
        profile_id: Option<&str>,
    ) -> Result<AgentSessionRef, AgentKitPortError> {
        self.open_bound_session_with_title(task_id, session_id, profile_id, None)
    }

    fn open_bound_session_with_title(
        &self,
        task_id: &TaskId,
        session_id: &str,
        profile_id: Option<&str>,
        title: Option<String>,
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
                title,
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

    pub fn restore_product_session_binding(
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
        let session = self.session_snapshot(session_id)?;
        let profile_id = profile_id.unwrap_or(session.profile_id.as_str());
        if session.profile_id != profile_id {
            return Err(AgentKitPortError::InvalidInput(format!(
                "session `{session_id}` profile does not match its product binding"
            )));
        }
        let binding = ProductSessionBinding {
            task_id: task_id.as_str().to_owned(),
            profile_id: profile_id.to_owned(),
        };
        let mut bindings = self
            .bindings
            .lock()
            .map_err(|_| AgentKitPortError::Unavailable("session binding lock poisoned".into()))?;
        if let Some(existing) = bindings.get(session_id) {
            if existing != &binding {
                return Err(AgentKitPortError::InvalidInput(format!(
                    "session `{session_id}` is already bound to a different task or profile"
                )));
            }
        } else {
            bindings.insert(session_id.to_owned(), binding);
        }
        AgentSessionRef::new(session_id.to_owned())
            .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))
    }

    pub(crate) fn fork_session_state(
        &self,
        source_session_id: &str,
        target_session_id: &str,
    ) -> Result<(), AgentKitPortError> {
        self.fork_session_state_through_turn(source_session_id, target_session_id, None)
            .map(|_| ())
    }

    pub(crate) fn fork_session_state_through_turn(
        &self,
        source_session_id: &str,
        target_session_id: &str,
        through_turn_id: Option<&str>,
    ) -> Result<AgentSession, AgentKitPortError> {
        let source = self.binding(source_session_id)?;
        let host = self.host_for_plan(None, false)?;
        let forked: AgentSession = self.call(
            &host,
            "session-fork",
            AGENT_SESSION_FORK_PROTOCOL,
            AgentSessionForkRequest {
                source_session_id: source_session_id.to_string(),
                target_session_id: target_session_id.to_string(),
                title: None,
                through_turn_id: through_turn_id.map(str::to_owned),
            },
        )?;
        self.bindings
            .lock()
            .map_err(|_| AgentKitPortError::Unavailable("session binding lock poisoned".into()))?
            .insert(target_session_id.to_string(), source);
        Ok(forked)
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

    pub fn session_snapshot(&self, session_id: &str) -> Result<AgentSession, AgentKitPortError> {
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
            run.host
                .cancel_subagents(&run.session_id)
                .map_err(agent_port_error)?;
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
        let already_cancelled = snapshot.events.iter().any(|event| {
            matches!(
                &event.event,
                AgentEvent::TurnState {
                    turn_id: event_turn_id,
                    status,
                } if event_turn_id == turn_id && status == "cancelled"
            )
        });
        let messages = cancelled_pending_tool_messages(&snapshot, turn_id)?;
        if already_cancelled && messages.is_empty() {
            return Ok(None);
        }
        let event = (!already_cancelled).then(|| {
            let sequence = snapshot.next_event_sequence.saturating_add(1);
            AgentEventEnvelope {
                session_id: session_id.to_string(),
                sequence,
                meta: timestamped_event_meta(
                    format!("{turn_id}:{sequence}"),
                    "turn cancelled",
                    turn_id,
                ),
                event: AgentEvent::TurnState {
                    turn_id: turn_id.to_string(),
                    status: "cancelled".into(),
                },
            }
        });
        let _: AgentSession = self.call(
            host,
            "session-cancel-event",
            AGENT_SESSION_APPEND_PROTOCOL,
            AgentSessionAppendRequest {
                session_id: session_id.to_string(),
                messages,
                events: event.iter().cloned().collect(),
                advance_turn: false,
            },
        )?;
        Ok(event)
    }

    fn append_interaction_event(
        &self,
        host: &Arc<AgentKitHost>,
        session_id: &str,
        turn_id: &str,
        summary: &str,
        event: AgentEvent,
    ) -> Result<AgentEventEnvelope, AgentKitPortError> {
        let snapshot = self.session_snapshot_on_host(host, session_id)?;
        let sequence = snapshot.next_event_sequence.saturating_add(1);
        let event = AgentEventEnvelope {
            session_id: session_id.to_owned(),
            sequence,
            meta: timestamped_event_meta(
                format!("{turn_id}:interaction:{sequence}"),
                summary.to_owned(),
                turn_id,
            ),
            event,
        };
        let _: AgentSession = self.call(
            host,
            "session-interaction-event",
            AGENT_SESSION_APPEND_PROTOCOL,
            AgentSessionAppendRequest {
                session_id: session_id.to_owned(),
                messages: Vec::new(),
                events: vec![event.clone()],
                advance_turn: false,
            },
        )?;
        self.project_and_observe_turn_events(session_id, turn_id, std::slice::from_ref(&event))?;
        Ok(event)
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
        if turn_is_waiting_for_action(&snapshot, turn_id) {
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
            return Ok(TurnCancellationDisposition::PausedAction);
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
            waiting_interaction: result.status == AgentRunStatus::WaitingInteraction,
            completed: result.status == AgentRunStatus::Completed,
            cancelled: result.status == AgentRunStatus::Cancelled,
            tool_summary: Some(json!({
                "driver": plan.driver.as_str(),
                "official_servers": 0,
                "waiting_approval": result.status == AgentRunStatus::WaitingApproval,
                "waiting_interaction": result.status == AgentRunStatus::WaitingInteraction,
                "auto_executed": executed,
                "blocked": blocked,
                "model_steps": model_steps,
            })),
            official_agent_server: false,
            credential_bound,
            // Plan only builds when a live adapter CredentialRef is bound.
            live_model_adapter_drives_turn: credential_bound,
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
        let configuration = self
            .model_runtime_configuration()
            .map_err(|error| AgentKitPortError::Unavailable(error.to_string()))?;
        let mut plan = build_live_turn_plan(
            &profile,
            &resolve_model_endpoint(configuration.openai_endpoint_override.as_deref()),
            configuration.anthropic_endpoint_override.as_deref(),
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
            .or(configuration.model_override.as_deref())
        {
            plan.select_model(model);
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
        enable_workspace_tools: bool,
    ) -> Result<Arc<AgentKitHost>, AgentKitPortError> {
        let key = match plan {
            Some(plan) => format!(
                "{}:{enable_workspace_tools}:{}:{}",
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
        let subagents = self.subagent_configuration().map_err(|error| {
            AgentKitPortError::Unavailable(format!(
                "custom subagent configuration is unavailable: {error}"
            ))
        })?;
        let host = Arc::new(
            AgentKitHost::build(
                self.bootstrap.bundle.clone(),
                plan,
                adapter_credential_broker(self.credentials().broker().clone()),
                enable_workspace_tools,
                &subagents,
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

fn timestamped_event_meta(
    event_id: impl Into<String>,
    summary: impl Into<String>,
    turn_id: &str,
) -> AgentEventMeta {
    let mut meta = AgentEventMeta::new(event_id, summary).with_turn(turn_id);
    meta.timestamp_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    meta
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

fn render_profile_prompt(profile: &AgentRuntimeProfile) -> String {
    profile
        .system_instructions
        .iter()
        .map(String::as_str)
        .chain(
            profile
                .prompt_fragments
                .iter()
                .map(|fragment| fragment.content.as_str()),
        )
        .map(str::trim)
        .filter(|fragment| !fragment.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn agent_run_metadata(
    workspace: Option<AgentWorkspaceRef>,
    turn_id: &str,
    context: Option<&Value>,
) -> Option<Value> {
    let mut metadata = workspace
        .map(|workspace| {
            serde_json::to_value(NativeCodingRunContext {
                workspace,
                turn_id: turn_id.to_owned(),
            })
            .expect("Native Coding run context serializes")
        })
        .unwrap_or_else(|| json!({}));
    if let Some(reasoning_effort) = context
        .and_then(|value| value.get("reasoningEffort"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        metadata
            .as_object_mut()
            .expect("Native Agent run metadata is an object")
            .insert(
                "reasoningEffort".into(),
                Value::String(reasoning_effort.to_owned()),
            );
    }
    for (field, value) in [
        (
            "productTaskId",
            context.and_then(|value| value.pointer("/workspace/metadata/productTaskId")),
        ),
        (
            "productProjectId",
            context.and_then(|value| value.pointer("/workspace/metadata/productProjectId")),
        ),
        (
            "projectArchitectureVersion",
            context.and_then(|value| value.pointer("/projectArchitecture/version")),
        ),
    ] {
        if let Some(value) = value.filter(|value| !value.is_null()) {
            metadata
                .as_object_mut()
                .expect("Native Agent run metadata is an object")
                .insert(field.to_owned(), value.clone());
        }
    }
    metadata
        .as_object()
        .is_some_and(|metadata| !metadata.is_empty())
        .then_some(metadata)
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

fn cancelled_pending_tool_messages(
    session: &AgentSession,
    turn_id: &str,
) -> Result<Vec<AgentMessage>, AgentKitPortError> {
    let Some(metadata) = session
        .messages
        .last()
        .filter(|message| message.role == AgentRole::Assistant)
        .and_then(|message| message.metadata.as_ref())
    else {
        return Ok(Vec::new());
    };
    let pending_matches_turn = ["pending_approvals", "pending_interactions"]
        .into_iter()
        .filter_map(|key| metadata.get(key).and_then(Value::as_array))
        .flatten()
        .any(|pending| pending.get("turn_id").and_then(Value::as_str) == Some(turn_id));
    if !pending_matches_turn {
        return Ok(Vec::new());
    }
    let calls = metadata
        .get("tool_calls")
        .cloned()
        .map(serde_json::from_value::<Vec<AgentToolCall>>)
        .transpose()
        .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))?
        .unwrap_or_default();
    calls
        .into_iter()
        .map(|call| {
            let error = AgentError::new(
                "agent.tool.cancelled",
                "the user cancelled the pending action",
            );
            Ok(AgentMessage {
                role: AgentRole::Tool,
                content: json!({ "cancelled": true }).to_string(),
                name: Some(call.name),
                metadata: Some(
                    serde_json::to_value(AgentToolResultMetadata {
                        call_id: call.call_id,
                        output_ref: None,
                        is_error: true,
                        error: Some(error),
                    })
                    .map_err(|error| AgentKitPortError::InvalidInput(error.to_string()))?,
                ),
                parts: Vec::new(),
            })
        })
        .collect()
}

fn turn_is_waiting_for_action(session: &AgentSession, turn_id: &str) -> bool {
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
        .is_some_and(|status| matches!(status, "waiting_approval" | "waiting_interaction"))
}

fn persisted_turn_status<'a>(session: &'a AgentSession, turn_id: &str) -> Option<&'a str> {
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
}

fn persisted_turn_user_request<'a>(
    session: &'a AgentSession,
    turn_id: &str,
) -> Option<(&'a str, Option<&'a Value>)> {
    session.events.iter().find_map(|event| match &event.event {
        AgentEvent::UserMessage {
            turn_id: event_turn_id,
            content,
            metadata,
        } if event_turn_id == turn_id => Some((content.as_str(), metadata.as_ref())),
        _ => None,
    })
}

fn persisted_turn_has_durable_progress(session: &AgentSession, turn_id: &str) -> bool {
    if session.events.iter().any(|event| {
        event.meta.turn_id.as_deref() == Some(turn_id)
            && matches!(
                event.event,
                AgentEvent::ToolCallStarted { .. } | AgentEvent::ToolCallCompleted { .. }
            )
    }) {
        return true;
    }
    session
        .messages
        .iter()
        .rev()
        .find(|message| message.role == AgentRole::Assistant)
        .and_then(|message| message.metadata.as_ref())
        .is_some_and(|metadata| {
            ["pending_approvals", "pending_interactions"]
                .into_iter()
                .filter_map(|key| metadata.get(key).and_then(Value::as_array))
                .flatten()
                .any(|pending| pending.get("turn_id").and_then(Value::as_str) == Some(turn_id))
        })
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

fn protocol_error_message(error: &mutsuki_agent_contracts::ProtocolError) -> String {
    format!("{}: {}", error.code, error.message)
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

    #[test]
    fn configured_model_runtime_drives_default_plan_but_task_context_can_override_model() {
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
                source: Some("runtime-settings-test".into()),
            })
            .unwrap();
        runtime.refresh_product_profile(None).unwrap();
        runtime
            .configure_model_runtime(NativeModelRuntimeConfiguration {
                openai_endpoint_override: Some(
                    "https://models.example.test/v1/chat/completions".into(),
                ),
                anthropic_endpoint_override: Some(
                    "https://anthropic.example.test/v1/messages".into(),
                ),
                model_override: Some("configured-model".into()),
            })
            .unwrap();

        let (configured, _) = runtime.turn_plan(None).unwrap();
        assert_eq!(
            configured.provider.endpoint,
            "https://models.example.test/v1/chat/completions"
        );
        assert_eq!(configured.model, "configured-model");
        assert!(configured.provider.models.contains_key("configured-model"));

        let (task_override, _) = runtime
            .turn_plan(Some(&json!({ "model": "task-model" })))
            .unwrap();
        assert_eq!(task_override.model, "task-model");
        assert!(task_override.provider.models.contains_key("task-model"));
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

    fn plan_confirmation_call() -> Value {
        json!({
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "confirm-plan-1",
                        "type": "function",
                        "function": {
                            "name": "confirm_plan",
                            "arguments": "{\"plan\":\"Inspect, implement, and verify\",\"question\":\"Run this plan?\"}"
                        }
                    }]
                }
            }],
            "usage": {"prompt_tokens": 2, "completion_tokens": 1, "total_tokens": 3}
        })
    }

    fn delegate_agent_call() -> Value {
        json!({
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "delegate-1",
                        "type": "function",
                        "function": {
                            "name": "delegate_agent",
                            "arguments": "{\"agentId\":\"reviewer\",\"task\":\"Review the current design\"}"
                        }
                    }]
                }
            }],
            "usage": {"prompt_tokens": 2, "completion_tokens": 1, "total_tokens": 3}
        })
    }

    fn text_response(content: &str) -> Value {
        json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {"role": "assistant", "content": content}
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 1, "total_tokens": 4}
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
    fn product_session_binding_restores_from_the_durable_agentkit_session() {
        let workspace = TestWorkspace::new("binding-restart");
        let projection_path = workspace.0.join("projections.db");
        let runtime_path = workspace.0.join("runtime.db");
        let task_id = TaskId::new("task-binding-restart").unwrap();
        let session_id = "session-binding-restart";
        {
            let runtime = NativeRuntimeBootstrap::embedded_reference()
                .unwrap()
                .into_runtime_with_stores(
                    SqliteTimelineProjectionStore::open(&projection_path).unwrap(),
                    SqliteAgentRuntimeStateStore::open(&runtime_path).unwrap(),
                );
            runtime
                .open_bound_session(&task_id, session_id, Some("mutsuki.reference.coding-agent"))
                .unwrap();
        }

        let recovered = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime_with_stores(
                SqliteTimelineProjectionStore::open(&projection_path).unwrap(),
                SqliteAgentRuntimeStateStore::open(&runtime_path).unwrap(),
            );
        assert!(matches!(
            recovered.binding(session_id),
            Err(AgentKitPortError::NotFound(_))
        ));
        recovered
            .restore_product_session_binding(
                &task_id,
                session_id,
                Some("mutsuki.reference.coding-agent"),
            )
            .unwrap();
        assert_eq!(
            recovered.binding(session_id).unwrap(),
            ProductSessionBinding {
                task_id: task_id.as_str().to_owned(),
                profile_id: "mutsuki.reference.coding-agent".to_owned(),
            }
        );
    }

    #[test]
    fn interaction_response_is_appended_to_the_same_session_and_resolves_projection() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let session = session(&runtime, "interaction-response");
        let task = TaskId::new("task-interaction-response").unwrap();
        let request = InteractionRequest {
            session_id: session.as_str().to_owned(),
            turn_id: "turn-question".into(),
            version: 1,
            interaction_id: "question-1".into(),
            kind: mutsuki_agent_contracts::InteractionKind::Clarification,
            source_tool: Some("ask_user_question".into()),
            permission_mode: mutsuki_agent_contracts::AgentPermissionMode::Ask,
            prompt: "Which target?".into(),
            options: json!({"choices": ["A", "B"]}),
            context: None,
            details: None,
        };

        let requested = runtime
            .request_interaction(&session, "turn-question", request.clone())
            .unwrap();
        assert!(matches!(
            requested.event,
            AgentEvent::InteractionRequested { .. }
        ));
        assert!(runtime
            .product_pending_for_task(&task)
            .iter()
            .any(|pending| {
                pending.request_id == "question-1"
                    && pending.kind == "ask_user"
                    && pending.status == PendingProjectionStatus::Open
            }));

        let resolution = InteractionResolution {
            session_id: session.as_str().to_owned(),
            turn_id: "turn-question".into(),
            version: 1,
            interaction_id: "question-1".into(),
            accepted: true,
            response: json!({"answer": "A"}),
        };
        let resolved = runtime
            .respond_interaction(&session, "turn-question", resolution.clone())
            .unwrap();
        assert_eq!(resolved.sequence, requested.sequence + 1);
        assert!(runtime
            .product_pending_for_task(&task)
            .iter()
            .any(|pending| {
                pending.request_id == "question-1"
                    && pending.status == PendingProjectionStatus::Resolved
            }));
        assert_eq!(
            runtime
                .respond_interaction(&session, "turn-question", resolution)
                .unwrap()
                .sequence,
            resolved.sequence
        );
        let snapshot = runtime.session_snapshot(session.as_str()).unwrap();
        assert!(snapshot.events.iter().any(|event| event == &resolved));
    }

    #[test]
    fn debug_mcp_interaction_seed_is_idempotent_and_projects_the_native_contract() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let task = TaskId::new("task-debug-mcp-seed").unwrap();
        let request = InteractionRequest {
            session_id: "session-debug-mcp-seed".into(),
            turn_id: "turn-debug-mcp-seed".into(),
            version: 1,
            interaction_id: "request-debug-mcp-seed".into(),
            kind: mutsuki_agent_contracts::InteractionKind::Custom,
            source_tool: None,
            permission_mode: mutsuki_agent_contracts::AgentPermissionMode::Ask,
            prompt: "Choose a project".into(),
            options: json!({
                "interaction": "mcp_elicitation",
                "threadId": "task-debug-mcp-seed",
                "serverName": "debug-mcp",
                "mode": "form",
                "requestedSchema": {
                    "type": "object",
                    "required": ["project"],
                    "properties": {
                        "project": {"type": "string", "enum": ["A", "B"]}
                    }
                }
            }),
            context: None,
            details: None,
        };

        let first = runtime
            .seed_debug_interaction(
                &task,
                "session-debug-mcp-seed",
                "turn-debug-mcp-seed",
                request.clone(),
            )
            .unwrap();
        let repeated = runtime
            .seed_debug_interaction(
                &task,
                "session-debug-mcp-seed",
                "turn-debug-mcp-seed",
                request,
            )
            .unwrap();

        assert_eq!(first.sequence, repeated.sequence);
        assert!(runtime
            .product_pending_for_task(&task)
            .iter()
            .any(|pending| {
                pending.request_id == "request-debug-mcp-seed"
                    && pending.kind == "mcp_elicitation"
                    && pending.status == PendingProjectionStatus::Open
            }));
    }

    #[test]
    fn live_model_diagnostics_flip_true_after_login_and_false_after_revoke() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let cold = runtime.independent_diagnostics();
        assert!(!cold.live_model_adapter_drives_turn);
        assert!(!cold.profile_has_credential_refs);

        let view = runtime
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
        let live = runtime.independent_diagnostics();
        assert!(live.profile_has_credential_refs);
        assert!(live.live_model_adapter_drives_turn);
        assert!(live.credential.has_usable_model_credential);
        assert!(live.runtime_ready);

        let credential = runtime
            .credentials()
            .primary_usable_credential()
            .expect("login must expose usable credential ref");
        assert_eq!(credential.credential_id, view.credential_id);
        runtime
            .credentials()
            .revoke(credential, Some("test-revoke".into()))
            .unwrap();
        runtime.refresh_product_profile(None).unwrap();
        let revoked = runtime.independent_diagnostics();
        assert!(!revoked.live_model_adapter_drives_turn);
        assert!(!revoked.profile_has_credential_refs);
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
        assert!(page.credential_bound);
        assert!(page.live_model_adapter_drives_turn);
        assert_eq!(
            page.tool_summary.as_ref().unwrap()["driver"],
            "openai-compatible"
        );
        assert!(page.events.iter().any(|event| matches!(
            event.event,
            mutsuki_agent_contracts::AgentEvent::FinalResponse { .. }
        )));
        assert!(page
            .events
            .iter()
            .any(|event| matches!(event.event, AgentEvent::ToolCallStarted { .. })));
        assert!(page
            .events
            .iter()
            .any(|event| matches!(event.event, AgentEvent::ToolCallCompleted { .. })));
        assert!(page
            .events
            .iter()
            .all(|event| event.meta.timestamp_unix_ms > 0));
        let snapshot = runtime.session_snapshot(session.as_str()).unwrap();
        assert!(snapshot.messages.iter().any(|message| {
            message.role == AgentRole::Tool
                && message
                    .metadata
                    .as_ref()
                    .is_some_and(|metadata| metadata.get("tool_execution_resume_receipt").is_some())
        }));
        let replayed = runtime
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
        assert!(replayed.completed);
        assert_eq!(replayed.tool_summary.as_ref().unwrap()["replayed"], true);
        assert_eq!(
            std::fs::read_to_string(workspace.0.join("created.txt")).unwrap(),
            "agentkit"
        );
        assert!(matches!(
            runtime.submit_turn_with_context_streaming(
                &session,
                "different request",
                "turn-full",
                Some(json!({
                    "workspace": {"folders": [workspace.0.to_string_lossy()]},
                    "permission": "full"
                })),
            ),
            Err(AgentKitPortError::InvalidInput(_))
        ));
        let usage = runtime
            .product_timeline_for_task(&TaskId::new("task-full").unwrap())
            .into_iter()
            .find(|event| event.kind == "usage")
            .expect("usage projection");
        assert!(usage.payload["createdAt"].as_u64().unwrap_or_default() > 0);
    }

    #[test]
    fn product_profile_prompt_is_persisted_once_across_session_turns() {
        let (runtime, server) = configured_runtime(vec![final_response(), final_response()]);
        let session = session(&runtime, "profile-prompt");

        for turn in ["turn-one", "turn-two"] {
            runtime
                .submit_turn_with_context_streaming(&session, turn, turn, None)
                .unwrap();
        }
        server.join().unwrap();

        let profile = runtime.current_product_profile().unwrap();
        let expected = render_profile_prompt(&profile);
        let snapshot = runtime.session_snapshot(session.as_str()).unwrap();
        assert_eq!(
            snapshot
                .messages
                .iter()
                .filter(|message| {
                    message.role == AgentRole::System && message.content == expected
                })
                .count(),
            1
        );
    }

    #[test]
    fn context_compaction_creates_a_new_durable_session_without_mutating_the_source() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let task_id = TaskId::new("task-context-compaction").unwrap();
        let source_session_id = "session-context-compaction-source";
        runtime
            .open_bound_session(
                &task_id,
                source_session_id,
                Some("profile-context-compaction"),
            )
            .unwrap();
        let host = runtime.host_for_plan(None, false).unwrap();
        let source_before: AgentSession = runtime
            .call(
                &host,
                "session-context-compaction-source",
                AGENT_SESSION_APPEND_PROTOCOL,
                AgentSessionAppendRequest {
                    session_id: source_session_id.into(),
                    messages: vec![
                        AgentMessage::user("keep the workspace changes"),
                        AgentMessage::assistant("the document editor is complete"),
                        AgentMessage::user("continue with diagnostics"),
                    ],
                    events: Vec::new(),
                    advance_turn: true,
                },
            )
            .unwrap();
        let source = runtime
            .prepare_product_session_compaction(source_session_id, 8_000)
            .unwrap();
        assert!(source.budget_satisfied);
        assert_eq!(source.source_message_count, 3);

        let target = runtime
            .create_compacted_product_session(
                &task_id,
                &source,
                "session-context-compaction-target",
                "turn-context-compaction",
                &NativeControlModelResult {
                    provider_id: "provider-test".into(),
                    model: "model-test".into(),
                    text: "目标：保留工作区改动。当前：文档编辑器已完成。下一步：继续诊断。".into(),
                    input_tokens: 30,
                    output_tokens: 12,
                },
                "上下文已压缩，可以继续对话。",
            )
            .unwrap();

        assert_eq!(
            runtime.session_snapshot(source_session_id).unwrap(),
            source_before
        );
        assert!(target.messages.iter().any(|message| {
            message.role == AgentRole::System
                && message
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("context_compaction"))
                    .is_some()
        }));
        assert!(target.events.iter().any(|event| matches!(
            &event.event,
            AgentEvent::FinalResponse { turn_id, summary, .. }
                if turn_id == "turn-context-compaction"
                    && summary == "上下文已压缩，可以继续对话。"
        )));
        assert!(target.events.iter().any(|event| matches!(
            &event.event,
            AgentEvent::TurnState { turn_id, status }
                if turn_id == "turn-context-compaction" && status == "completed"
        )));
    }

    #[test]
    fn interrupted_tool_start_requires_user_recovery_without_reexecuting_side_effect() {
        let workspace = TestWorkspace::new("tool-recovery");
        let (runtime, server) = configured_runtime(Vec::new());
        server.join().unwrap();
        let session = session(&runtime, "tool-recovery");
        let host = runtime.host_for_plan(None, false).unwrap();
        let snapshot = runtime
            .session_snapshot_on_host(&host, session.as_str())
            .unwrap();
        let tool_call = AgentToolCall {
            call_id: "write-crash-1".into(),
            name: "computer.fs.write".into(),
            input: json!({
                "path": "created.txt",
                "content": "must-not-run-without-confirmation"
            }),
        };
        let mut assistant = AgentMessage::assistant(String::new());
        assistant.metadata = Some(json!({
            "tool_calls": [tool_call.clone()],
            "run_continuation": {
                "next_step_index": 1,
                "max_steps": 8,
                "budget": mutsuki_agent_contracts::AgentRunBudget::default(),
                "usage": mutsuki_agent_contracts::AgentUsage::default(),
                "cost_microunits": 0
            }
        }));
        let sequence = snapshot.next_event_sequence.saturating_add(1);
        let _: AgentSession = runtime
            .call(
                &host,
                "session-seed-interrupted-tool",
                AGENT_SESSION_APPEND_PROTOCOL,
                AgentSessionAppendRequest {
                    session_id: session.as_str().to_owned(),
                    messages: vec![AgentMessage::user("write once"), assistant],
                    events: vec![AgentEventEnvelope {
                        session_id: session.as_str().to_owned(),
                        sequence,
                        meta: timestamped_event_meta(
                            format!("turn-tool-recovery:{sequence}"),
                            "tool call started",
                            "turn-tool-recovery",
                        ),
                        event: AgentEvent::ToolCallStarted {
                            turn_id: "turn-tool-recovery".into(),
                            call_id: tool_call.call_id.clone(),
                            name: tool_call.name.clone(),
                            input: tool_call.input.clone(),
                        },
                    }],
                    advance_turn: false,
                },
            )
            .unwrap();

        let page = runtime
            .submit_turn_with_context_streaming(
                &session,
                "write once",
                "turn-tool-recovery",
                Some(json!({
                    "workspace": {"folders": [workspace.0.to_string_lossy()]},
                    "permission": "full"
                })),
            )
            .unwrap();
        assert!(page.waiting_interaction);
        assert!(!workspace.0.join("created.txt").exists());
        let recovery = page
            .events
            .iter()
            .find_map(|event| match &event.event {
                AgentEvent::InteractionRequested { interaction, .. }
                    if interaction.interaction_id == tool_call.call_id =>
                {
                    Some(interaction.clone())
                }
                _ => None,
            })
            .expect("interrupted tool must project a recovery interaction");
        assert_eq!(recovery.options["recovery"], "ambiguous_tool_execution");
        assert_eq!(recovery.options["choices"].as_array().unwrap().len(), 2);

        let cancelled = runtime
            .respond_interaction_streaming(
                &session,
                InteractionResolution {
                    session_id: recovery.session_id,
                    turn_id: recovery.turn_id,
                    version: recovery.version,
                    interaction_id: recovery.interaction_id,
                    accepted: false,
                    response: json!({"cancelled": true}),
                },
            )
            .unwrap();
        assert!(cancelled.cancelled);
        assert!(!workspace.0.join("created.txt").exists());
        let snapshot = runtime.session_snapshot(session.as_str()).unwrap();
        assert!(snapshot.events.iter().any(|event| matches!(
            &event.event,
            AgentEvent::ToolCallCompleted { call_id, .. } if call_id == "write-crash-1"
        )));
    }

    #[test]
    fn configured_custom_subagent_executes_as_a_real_readonly_agent_tool() {
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
                source: Some("subagent-test".into()),
            })
            .unwrap();
        runtime.refresh_product_profile(None).unwrap();
        runtime
            .configure_subagents(vec![NativeSubagentDefinition {
                id: "reviewer".into(),
                name: "Reviewer".into(),
                description: "Review architecture decisions".into(),
                instruction: "Find correctness and ownership risks.".into(),
                enabled: true,
            }])
            .unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (requests_tx, requests_rx) = mpsc::channel();
        let responses = vec![
            delegate_agent_call(),
            text_response("child finding: keep the service boundary typed"),
            text_response("parent incorporated the child finding"),
        ];
        let server = std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut responses = responses.into_iter();
            while let Some(response) = responses.next() {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break Some(stream),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if Instant::now() >= deadline {
                                break None;
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("subagent test listener failed: {error}"),
                    }
                };
                let Some(mut stream) = stream.take() else {
                    break;
                };
                stream.set_nonblocking(false).unwrap();
                let mut bytes = [0_u8; 65_536];
                let read = stream.read(&mut bytes).unwrap();
                requests_tx
                    .send(String::from_utf8_lossy(&bytes[..read]).to_string())
                    .unwrap();
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
        let session = session(&runtime, "custom-subagent");
        let page = runtime
            .submit_turn_with_context_streaming(
                &session,
                "Ask the configured reviewer",
                "turn-custom-subagent",
                Some(json!({"permission": "full"})),
            )
            .unwrap();
        server.join().unwrap();
        let requests = requests_rx.try_iter().collect::<Vec<_>>();
        assert_eq!(
            requests.len(),
            3,
            "unexpected model request count: {requests:#?}"
        );
        assert!(requests[0].contains("delegate_agent"));
        assert!(requests[1].contains("Find correctness and ownership risks."));
        assert!(!requests[1].contains("computer.fs.write"));
        assert!(!requests[1].contains("delegate_agent"));
        assert!(requests[2].contains("child finding: keep the service boundary typed"));
        assert!(page.events.iter().any(|event| matches!(
            &event.event,
            AgentEvent::ToolCallCompleted { summary, .. } if summary == "delegate_agent"
        )));
        assert!(page.events.iter().any(|event| matches!(
            &event.event,
            AgentEvent::FinalResponse { summary, .. }
                if summary == "parent incorporated the child finding"
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
    fn rejected_approval_is_terminal_and_allows_a_new_turn() {
        let workspace = TestWorkspace::new("reject-approval");
        let (runtime, server) = configured_runtime(vec![write_call(), final_response()]);
        let session = session(&runtime, "reject-approval");
        let waiting = runtime
            .submit_turn_with_context_streaming(
                &session,
                "ask before writing",
                "turn-reject-approval",
                Some(json!({
                    "workspace": {"folders": [workspace.0.to_string_lossy()]},
                    "permission": "ask"
                })),
            )
            .unwrap();
        let approval = waiting
            .events
            .iter()
            .find_map(|event| match &event.event {
                AgentEvent::ApprovalRequest { request } => Some(request.clone()),
                _ => None,
            })
            .unwrap();

        let rejected = runtime
            .respond_approval_streaming(
                &session,
                &ProductApprovalDecision {
                    session_id: approval.session_id,
                    turn_id: approval.turn_id,
                    action_id: approval.action_id,
                    version: approval.version,
                    approved: false,
                },
            )
            .unwrap();
        assert!(rejected.cancelled);
        assert!(!rejected.waiting_approval);
        assert!(runtime
            .session_snapshot(session.as_str())
            .unwrap()
            .events
            .iter()
            .any(|event| matches!(
                &event.event,
                AgentEvent::TurnState { turn_id, status }
                    if turn_id == "turn-reject-approval" && status == "cancelled"
            )));
        assert!(!workspace.0.join("created.txt").exists());

        let next = runtime
            .submit_turn_with_context_streaming(
                &session,
                "continue after rejection",
                "turn-after-rejection",
                Some(json!({ "permission": "full" })),
            )
            .unwrap();
        server.join().unwrap();
        assert!(next.completed);
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
            TurnCancellationDisposition::PausedAction
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
    fn cancelling_paused_plan_is_terminal_in_session_and_product_projection() {
        let (runtime, server) = configured_runtime(vec![plan_confirmation_call()]);
        let session = session(&runtime, "cancel-paused-plan");
        let task = TaskId::new("task-cancel-paused-plan").unwrap();
        let waiting = runtime
            .submit_turn_with_context_streaming(
                &session,
                "prepare a plan",
                "turn-cancel-paused-plan",
                Some(json!({"permission": "full", "planMode": true})),
            )
            .unwrap();
        server.join().unwrap();
        assert!(waiting.waiting_interaction);
        assert!(runtime
            .product_pending_for_task(&task)
            .iter()
            .any(|pending| {
                pending.kind == "plan_approval" && pending.status == PendingProjectionStatus::Open
            }));

        assert_eq!(
            runtime
                .cancel_session_turn(session.as_str(), "turn-cancel-paused-plan")
                .unwrap(),
            TurnCancellationDisposition::PausedAction
        );
        assert!(runtime
            .product_pending_for_task(&task)
            .iter()
            .any(|pending| {
                pending.kind == "plan_approval"
                    && pending.status == PendingProjectionStatus::Cancelled
            }));
        assert!(runtime
            .session_snapshot(session.as_str())
            .unwrap()
            .events
            .iter()
            .any(|event| matches!(
                &event.event,
                AgentEvent::TurnState { turn_id, status }
                    if turn_id == "turn-cancel-paused-plan" && status == "cancelled"
            )));
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
                let response = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                if index != 0 {
                    response.unwrap();
                }
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

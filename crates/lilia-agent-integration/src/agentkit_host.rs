use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use mutsuki_agent_adapter_api::{CredentialBroker, ModelProtocolAdapter};
use mutsuki_agent_adapter_openai::OpenAiCompatibleAdapter;
use mutsuki_agent_bundle::{
    native_coding_tool_plugin, AdapterBackedModelProvider, AgentLoop, AgentRuntimeRunner,
    ModelGateway, NativeCodingAgentBundle, ToolRegistry,
};
use mutsuki_agent_contracts::{
    AgentError, AgentMessage, AgentPermissionMode, AgentResult, AgentRunRequest, AgentRunResult,
    AgentRunStatus, AgentSessionCreateRequest, AgentSessionGetRequest, AgentToolDescriptor,
    AgentToolExecuteRequest, AgentToolExecution, InteractionKind, ToolSideEffect,
    ToolTargetPayloadMode, AGENT_RUN_PROTOCOL, AGENT_SESSION_CREATE_PROTOCOL,
    AGENT_SESSION_GET_PROTOCOL,
};
use mutsuki_runtime_contracts::{
    PluginDeploymentKind, RuntimeProfile, RuntimeProfileMode, Task, TaskBatch, TaskHandle,
    TaskOutcome, TaskStatus,
};
use mutsuki_runtime_host::{
    HostRuntime, HostRuntimeConfig, RuntimeBootstrapper, TokioAsyncExecutor,
};
use mutsuki_runtime_sdk::{
    contracts::RunnerResult, HostRuntime as _, PluginBuilder, ProtocolSpec, RuntimeClient,
    RuntimeClientRef, RuntimeFailure, RuntimeResult, SdkProtocol, TaskAwaitRunnerAdapter,
    TaskSubmitterRuntimeClient,
};
use serde_json::{json, Value};

use crate::anthropic_adapter::AnthropicMessagesAdapter;
use crate::model_turn::{openai_adapter_descriptor, LiveModelDriver, LiveModelTurnPlan};
use crate::subagent::NativeSubagentDefinition;

const HOST_PROFILE_ID: &str = "lilia.native-coding.agentkit-host";
const SUBAGENT_TOOL_NAME: &str = "delegate_agent";
const SUBAGENT_TOOL_PLUGIN_ID: &str = "lilia.plugin.agent.custom-subagent";
const SUBAGENT_TOOL_RUNNER_ID: &str = "lilia.agent.custom-subagent.runner";
const SUBAGENT_TOOL_PROTOCOL: &str = "lilia.agent.custom-subagent.tool@1";
const SUBAGENT_TASK_TIMEOUT: Duration = Duration::from_secs(90);
const PROJECT_ARCHITECTURE_TOOL_NAME: &str = "update_project_architecture";
const PROJECT_ARCHITECTURE_CONTRACT_JSON: &str =
    include_str!("../../lilia-contracts/contracts/architecture-contract.json");

#[derive(Clone, Debug)]
struct NativeSubagentToolProtocol;

impl SdkProtocol for NativeSubagentToolProtocol {
    const PROTOCOL_ID: &'static str = SUBAGENT_TOOL_PROTOCOL;
}

impl ProtocolSpec for NativeSubagentToolProtocol {}

#[derive(Default)]
struct DeferredRuntimeClient {
    client: OnceLock<RuntimeClientRef>,
}

impl DeferredRuntimeClient {
    fn bind(&self, runtime: &HostRuntime) -> AgentResult<()> {
        let submitter = runtime.host_context().task_submitter_ref();
        self.client
            .set(TaskSubmitterRuntimeClient::new(submitter).into_runtime_client())
            .map_err(|_| {
                AgentError::new("agent.host.already_bound", "runtime client already bound")
            })
    }

    fn client(&self) -> RuntimeResult<RuntimeClientRef> {
        self.client.get().cloned().ok_or_else(|| {
            RuntimeFailure::new(mutsuki_runtime_contracts::RuntimeError::new(
                "agent.host.not_bound",
                HOST_PROFILE_ID,
                "runtime_client.not_bound",
            ))
        })
    }
}

impl RuntimeClient for DeferredRuntimeClient {
    fn submit_batch(&self, batch: TaskBatch) -> RuntimeResult<Vec<TaskHandle>> {
        self.client()?.submit_batch(batch)
    }

    fn task_outcome(&self, handle: &TaskHandle) -> RuntimeResult<Option<TaskOutcome>> {
        self.client()?.task_outcome(handle)
    }
}

/// Product Host lifecycle wrapper. Agent/session/model/tool facts stay in the
/// AgentKit runners registered in this Host; Lilia only submits typed tasks.
pub(crate) struct AgentKitHost {
    runtime: Arc<HostRuntime>,
    next_task: AtomicU64,
    subagents: Option<Arc<LiveSubagentToolRuntime>>,
}

impl AgentKitHost {
    pub(crate) fn build(
        bundle: NativeCodingAgentBundle,
        plan: Option<&LiveModelTurnPlan>,
        credentials: Arc<dyn CredentialBroker>,
        enable_workspace_tools: bool,
        subagents: &[NativeSubagentDefinition],
    ) -> AgentResult<Self> {
        let enabled_subagents = subagents
            .iter()
            .filter(|subagent| subagent.enabled)
            .cloned()
            .collect::<Vec<_>>();
        let subagent_runtime = if enabled_subagents.is_empty() || plan.is_none() {
            None
        } else {
            let child_host = Arc::new(Self::build_host(
                bundle.clone(),
                plan,
                Arc::clone(&credentials),
                enable_workspace_tools,
                ToolAccess::ReadOnly,
                None,
            )?);
            Some(Arc::new(LiveSubagentToolRuntime::new(
                child_host,
                enabled_subagents,
            )))
        };
        Self::build_host(
            bundle,
            plan,
            credentials,
            enable_workspace_tools,
            ToolAccess::Full,
            subagent_runtime,
        )
    }

    fn build_host(
        mut bundle: NativeCodingAgentBundle,
        plan: Option<&LiveModelTurnPlan>,
        credentials: Arc<dyn CredentialBroker>,
        enable_workspace_tools: bool,
        tool_access: ToolAccess,
        subagents: Option<Arc<LiveSubagentToolRuntime>>,
    ) -> AgentResult<Self> {
        let mut product_tools = bundle.routed_model_tools();
        if !product_tools
            .iter()
            .any(|descriptor| descriptor.name == PROJECT_ARCHITECTURE_TOOL_NAME)
        {
            product_tools.push(project_architecture_tool_descriptor());
        }
        let routed_tools = product_tools
            .into_iter()
            .filter(|descriptor| {
                tool_access.allows(descriptor)
                    && (enable_workspace_tools
                        || matches!(
                            &descriptor.execution,
                            AgentToolExecution::Interaction { .. }
                        ))
            })
            .chain(subagents.iter().map(|runtime| runtime.tool_descriptor()))
            .collect::<Vec<_>>();
        let tools = ToolRegistry::default();
        for descriptor in routed_tools.iter().cloned() {
            tools.register(descriptor)?;
        }
        bundle.core.tools = tools;
        bundle.core.context.set_tools(routed_tools.clone());

        if let Some(plan) = plan {
            let adapter: Arc<dyn ModelProtocolAdapter> = match plan.driver {
                LiveModelDriver::OpenAiCompatible => Arc::new(
                    OpenAiCompatibleAdapter::new(openai_adapter_descriptor(), credentials)
                        .map_err(protocol_error)?,
                ),
                LiveModelDriver::AnthropicMessages => Arc::new(
                    AnthropicMessagesAdapter::new(
                        AnthropicMessagesAdapter::default_descriptor(),
                        credentials,
                    )
                    .map_err(protocol_error)?,
                ),
            };
            let model = ModelGateway::with_default_provider(plan.provider.provider_id.clone());
            model.register(Arc::new(AdapterBackedModelProvider::new(
                plan.provider.clone(),
                adapter,
                routed_tools,
            )?));
            bundle.core.model = model;
            bundle.core.agent_loop = AgentLoop::default().with_default_model(plan.model.clone());
        }

        let deferred = Arc::new(DeferredRuntimeClient::default());
        let client: RuntimeClientRef = deferred.clone();
        let mut manifests = bundle.core.manifests();
        let mut native_tools = native_coding_tool_plugin(client.clone(), bundle.clone()).build();
        manifests.push(native_tools.manifest.clone());
        let mut subagent_tools = subagents.as_ref().map(|runtime| {
            native_subagent_tool_plugin(client.clone(), Arc::clone(runtime)).build()
        });
        if let Some(plugin) = subagent_tools.as_ref() {
            manifests.push(plugin.manifest.clone());
        }

        let mut bootstrapper = RuntimeBootstrapper::new();
        for manifest in &manifests {
            bootstrapper.register_manifest(manifest.clone());
        }
        for kind in AgentRuntimeRunner::ALL {
            bootstrapper.register_builtin_runner(bundle.core.runtime_runner(kind, client.clone()));
        }
        bootstrapper.register_async_handler(bundle.core.model_async_handler());
        for runner in native_tools.runners.drain(..) {
            bootstrapper.register_builtin_runner(runner);
        }
        if let Some(plugin) = subagent_tools.as_mut() {
            for runner in plugin.runners.drain(..) {
                bootstrapper.register_builtin_runner(runner);
            }
        }

        let enabled_plugins = manifests
            .iter()
            .map(|manifest| manifest.plugin_id.clone())
            .collect::<Vec<_>>();
        let profile = RuntimeProfile {
            profile_id: HOST_PROFILE_ID.into(),
            mode: RuntimeProfileMode::FullDev,
            enabled_plugins: enabled_plugins.clone(),
            bindings: BTreeMap::new(),
            plugin_deployments: enabled_plugins
                .into_iter()
                .map(|plugin_id| (plugin_id, PluginDeploymentKind::Builtin))
                .collect(),
            observability: Default::default(),
            allow_dynamic_registration: false,
            allow_hot_reload: false,
        };
        let runtime = Arc::new(
            bootstrapper
                .into_host_runtime_with_config(
                    profile,
                    HostRuntimeConfig {
                        event_driven: true,
                        async_executor: Some(Arc::new(
                            TokioAsyncExecutor::new(2, 64, 64, 4 * 1024 * 1024)
                                .map_err(runtime_error)?,
                        )),
                        ..HostRuntimeConfig::default()
                    },
                )
                .map_err(runtime_error)?,
        );
        deferred.bind(&runtime)?;
        Ok(Self {
            runtime,
            next_task: AtomicU64::new(1),
            subagents,
        })
    }

    pub(crate) fn submit(
        &self,
        label: &str,
        protocol_id: &str,
        payload: serde_json::Value,
    ) -> AgentResult<TaskHandle> {
        let id = self.next_task.fetch_add(1, Ordering::Relaxed);
        self.runtime
            .submit_task(Task::new(
                format!("lilia:{label}:{id}"),
                protocol_id,
                payload,
            ))
            .map_err(runtime_error)
    }

    pub(crate) fn wait(
        &self,
        handle: &TaskHandle,
        timeout: Duration,
    ) -> AgentResult<serde_json::Value> {
        let states = self
            .runtime
            .wait_task_states(vec![handle.clone()], timeout)
            .map_err(runtime_error)?;
        if !states.first().is_some_and(|state| {
            matches!(
                state.status,
                Some(
                    TaskStatus::Completed
                        | TaskStatus::Failed
                        | TaskStatus::Cancelled
                        | TaskStatus::Expired
                        | TaskStatus::DeadLetter
                )
            )
        }) {
            return Err(AgentError::new(
                "agent.host.timeout",
                "AgentKit task did not reach a terminal state before the deadline",
            ));
        }
        let outcome = self
            .runtime
            .task_outcome(handle)
            .map_err(runtime_error)?
            .ok_or_else(|| {
                AgentError::new("agent.host.timeout", "AgentKit task outcome is unavailable")
            })?;
        task_output(outcome)
    }

    pub(crate) fn try_output(&self, handle: &TaskHandle) -> AgentResult<Option<serde_json::Value>> {
        self.runtime
            .task_outcome(handle)
            .map_err(runtime_error)?
            .map(task_output)
            .transpose()
    }

    pub(crate) fn cancel(&self, handle: &TaskHandle) -> AgentResult<()> {
        self.runtime.cancel_task(handle).map_err(runtime_error)
    }

    pub(crate) fn cancel_subagents(&self, parent_session_id: &str) -> AgentResult<usize> {
        self.subagents
            .as_ref()
            .map_or(Ok(0), |runtime| runtime.cancel_parent(parent_session_id))
    }
}

fn project_architecture_tool_descriptor() -> AgentToolDescriptor {
    let contract: Value = serde_json::from_str(PROJECT_ARCHITECTURE_CONTRACT_JSON)
        .expect("architecture-contract.json must be valid JSON");
    let mut descriptor = AgentToolDescriptor::new(
        PROJECT_ARCHITECTURE_TOOL_NAME,
        AGENT_RUN_PROTOCOL,
        "Propose typed changes to the current Lilia project architecture graph. Use the authoritative project architecture snapshot in the turn context, explain the reason, and submit only changes supported by the schema. The host applies or rejects the proposal according to the current execution permission.",
    );
    descriptor.input_schema = contract["updateProjectArchitectureInputSchema"].clone();
    descriptor.execution = AgentToolExecution::Interaction {
        interaction_kind: InteractionKind::Custom,
    };
    descriptor
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolAccess {
    Full,
    ReadOnly,
}

impl ToolAccess {
    fn allows(self, descriptor: &AgentToolDescriptor) -> bool {
        match self {
            Self::Full => true,
            Self::ReadOnly => {
                matches!(&descriptor.execution, AgentToolExecution::Routed)
                    && !descriptor.requires_approval
                    && matches!(
                        descriptor.side_effect,
                        ToolSideEffect::None
                            | ToolSideEffect::WorkspaceRead
                            | ToolSideEffect::ExternalRead
                    )
            }
        }
    }
}

struct LiveSubagentToolRuntime {
    child_host: Arc<AgentKitHost>,
    definitions: BTreeMap<String, NativeSubagentDefinition>,
    results: Mutex<BTreeMap<String, Value>>,
    active: Mutex<BTreeMap<String, Vec<TaskHandle>>>,
    gates: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
}

impl LiveSubagentToolRuntime {
    fn new(child_host: Arc<AgentKitHost>, definitions: Vec<NativeSubagentDefinition>) -> Self {
        Self {
            child_host,
            definitions: definitions
                .into_iter()
                .map(|definition| (definition.id.clone(), definition))
                .collect(),
            results: Mutex::new(BTreeMap::new()),
            active: Mutex::new(BTreeMap::new()),
            gates: Mutex::new(BTreeMap::new()),
        }
    }

    fn tool_descriptor(&self) -> AgentToolDescriptor {
        let agent_ids = self.definitions.keys().cloned().collect::<Vec<_>>();
        let catalog = self
            .definitions
            .values()
            .map(|definition| {
                let summary = if definition.description.is_empty() {
                    definition.name.as_str()
                } else {
                    definition.description.as_str()
                };
                format!("{} ({}): {}", definition.name, definition.id, summary)
            })
            .collect::<Vec<_>>()
            .join("; ");
        let mut descriptor = AgentToolDescriptor::new(
            SUBAGENT_TOOL_NAME,
            SUBAGENT_TOOL_PROTOCOL,
            format!(
                "Delegate a bounded read-only research or review task to one configured Agent. Available Agents: {catalog}"
            ),
        );
        descriptor.input_schema = json!({
            "type": "object",
            "required": ["agentId", "task"],
            "properties": {
                "agentId": {"type": "string", "enum": agent_ids},
                "task": {"type": "string", "minLength": 1, "maxLength": 16000}
            },
            "additionalProperties": false
        });
        descriptor.output_schema = json!({
            "type": "object",
            "required": ["agentId", "agentName", "status", "summary"],
            "properties": {
                "agentId": {"type": "string"},
                "agentName": {"type": "string"},
                "status": {"type": "string"},
                "summary": {"type": "string"}
            }
        });
        descriptor.side_effect = ToolSideEffect::WorkspaceRead;
        descriptor.target_payload_mode = ToolTargetPayloadMode::ExecutionRequest;
        descriptor
    }

    fn execute(&self, request: AgentToolExecuteRequest) -> AgentResult<Value> {
        if request.name != SUBAGENT_TOOL_NAME {
            return Err(AgentError::not_found(format!(
                "custom subagent tool `{}` is not registered",
                request.name
            )));
        }
        let parent_session_id = request
            .session_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AgentError::invalid_input("subagent parent session_id is required"))?
            .to_owned();
        if parent_session_id.contains(":subagent:") {
            return Err(AgentError::new(
                "agent.subagent.depth_exceeded",
                "custom subagents cannot recursively delegate",
            ));
        }
        let context = request.context.as_ref().and_then(Value::as_object);
        let fallback_turn_id = request
            .call_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("turn");
        let parent_turn_id = context
            .and_then(|value| value.get("turn_id").or_else(|| value.get("turnId")))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_turn_id)
            .to_owned();
        let input = request
            .input
            .as_object()
            .ok_or_else(|| AgentError::invalid_input("subagent tool input must be an object"))?;
        let agent_id = required_input_string(input, "agentId")?;
        let task = required_input_string(input, "task")?;
        if task.chars().count() > 16_000 {
            return Err(AgentError::invalid_input(
                "subagent task exceeds 16000 characters",
            ));
        }
        let definition = self.definitions.get(&agent_id).cloned().ok_or_else(|| {
            AgentError::not_found(format!("custom subagent `{agent_id}` is not enabled"))
        })?;
        let call_id = request
            .call_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{}:{}:{}", parent_turn_id, definition.id, task));
        let idempotency_key = format!("{parent_session_id}:{parent_turn_id}:{call_id}");
        let gate = {
            let mut gates = self
                .gates
                .lock()
                .map_err(|_| AgentError::provider_unavailable("subagent gate state unavailable"))?;
            Arc::clone(
                gates
                    .entry(idempotency_key.clone())
                    .or_insert_with(|| Arc::new(Mutex::new(()))),
            )
        };
        let _guard = gate
            .lock()
            .map_err(|_| AgentError::provider_unavailable("subagent call gate unavailable"))?;
        if let Some(result) = self
            .results
            .lock()
            .map_err(|_| AgentError::provider_unavailable("subagent result state unavailable"))?
            .get(&idempotency_key)
            .cloned()
        {
            return Ok(result);
        }

        let child_session_id = format!(
            "{}:subagent:{}:{}",
            parent_session_id,
            definition.id,
            compact_identifier(&call_id)
        );
        let child_turn_id = format!("{parent_turn_id}:subagent:{}", compact_identifier(&call_id));
        let system_prompt = format!(
            "You are the configured Lilia Agent named `{}`.\n\n{}\n\nWork as a bounded read-only subagent. Use only the read tools exposed to you. Do not ask the user, modify files, or delegate again. Return concise findings and concrete evidence to the parent Agent.",
            definition.name, definition.instruction
        );
        let mut child_request = AgentRunRequest::new(
            format!("lilia.custom-subagent.{}", definition.id),
            vec![
                AgentMessage::system(system_prompt),
                AgentMessage::user(task),
            ],
        );
        self.ensure_child_session(
            &child_session_id,
            &child_request.profile_id,
            &definition.name,
        )?;
        child_request.session_id = Some(child_session_id);
        child_request.turn_id = Some(child_turn_id);
        child_request.permission_mode = AgentPermissionMode::ReadOnly;
        child_request.max_steps = 8;
        child_request.metadata = Some(json!({
            "parentSessionId": parent_session_id,
            "parentTurnId": parent_turn_id,
            "subagentId": definition.id,
            "subagentName": definition.name,
            "callId": call_id,
        }));
        let handle = self.child_host.submit(
            "custom-subagent",
            AGENT_RUN_PROTOCOL,
            serde_json::to_value(child_request)
                .map_err(|error| AgentError::invalid_input(error.to_string()))?,
        )?;
        self.active
            .lock()
            .map_err(|_| AgentError::provider_unavailable("subagent active state unavailable"))?
            .entry(parent_session_id.clone())
            .or_default()
            .push(handle.clone());
        let output = self.child_host.wait(&handle, SUBAGENT_TASK_TIMEOUT);
        self.remove_active(&parent_session_id, &handle)?;
        let run: AgentRunResult = serde_json::from_value(output?)
            .map_err(|error| AgentError::new("agent.subagent.result_invalid", error.to_string()))?;
        let summary = run
            .messages
            .iter()
            .rev()
            .find(|message| message.role == mutsuki_agent_contracts::AgentRole::Assistant)
            .map(|message| message.content.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("{} finished without a text result", definition.name));
        let status = match run.status {
            AgentRunStatus::Completed => "completed",
            AgentRunStatus::Cancelled => "cancelled",
            AgentRunStatus::BudgetExceeded => "budget_exceeded",
            AgentRunStatus::Failed => "failed",
            AgentRunStatus::WaitingApproval | AgentRunStatus::WaitingInteraction => {
                return Err(AgentError::new(
                    "agent.subagent.unexpected_interaction",
                    "read-only custom subagent requested an unsupported interaction",
                ));
            }
        };
        let result = json!({
            "agentId": definition.id,
            "agentName": definition.name,
            "status": status,
            "summary": summary,
            "usage": run.usage,
        });
        self.results
            .lock()
            .map_err(|_| AgentError::provider_unavailable("subagent result state unavailable"))?
            .insert(idempotency_key, result.clone());
        Ok(result)
    }

    fn ensure_child_session(
        &self,
        session_id: &str,
        profile_id: &str,
        title: &str,
    ) -> AgentResult<()> {
        let get = self.child_host.submit(
            "custom-subagent-session-get",
            AGENT_SESSION_GET_PROTOCOL,
            serde_json::to_value(AgentSessionGetRequest {
                session_id: session_id.to_owned(),
            })
            .map_err(|error| AgentError::invalid_input(error.to_string()))?,
        )?;
        match self.child_host.wait(&get, SUBAGENT_TASK_TIMEOUT) {
            Ok(_) => return Ok(()),
            Err(error) if error.code.contains("not_found") => {}
            Err(error) => return Err(error),
        }
        let create = self.child_host.submit(
            "custom-subagent-session-create",
            AGENT_SESSION_CREATE_PROTOCOL,
            serde_json::to_value(AgentSessionCreateRequest {
                session_id: Some(session_id.to_owned()),
                profile_id: profile_id.to_owned(),
                title: Some(title.to_owned()),
            })
            .map_err(|error| AgentError::invalid_input(error.to_string()))?,
        )?;
        self.child_host
            .wait(&create, SUBAGENT_TASK_TIMEOUT)
            .map(|_| ())
    }

    fn remove_active(&self, parent_session_id: &str, handle: &TaskHandle) -> AgentResult<()> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| AgentError::provider_unavailable("subagent active state unavailable"))?;
        if let Some(handles) = active.get_mut(parent_session_id) {
            handles.retain(|candidate| candidate.task_id != handle.task_id);
            if handles.is_empty() {
                active.remove(parent_session_id);
            }
        }
        Ok(())
    }

    fn cancel_parent(&self, parent_session_id: &str) -> AgentResult<usize> {
        let handles = self
            .active
            .lock()
            .map_err(|_| AgentError::provider_unavailable("subagent active state unavailable"))?
            .remove(parent_session_id)
            .unwrap_or_default();
        for handle in &handles {
            self.child_host.cancel(handle)?;
        }
        Ok(handles.len())
    }
}

fn native_subagent_tool_plugin(
    client: RuntimeClientRef,
    runtime: Arc<LiveSubagentToolRuntime>,
) -> PluginBuilder {
    let descriptor =
        mutsuki_agent_sdk::orchestration_runner(SUBAGENT_TOOL_RUNNER_ID, SUBAGENT_TOOL_PLUGIN_ID)
            .accepts::<NativeSubagentToolProtocol>()
            .build();
    PluginBuilder::new(SUBAGENT_TOOL_PLUGIN_ID)
        .protocol::<NativeSubagentToolProtocol>()
        .runner(Box::new(TaskAwaitRunnerAdapter::new(
            descriptor,
            client,
            Box::new(move |_context, task| {
                let runtime = Arc::clone(&runtime);
                Box::pin(async move { run_native_subagent_tool(runtime, task) })
            }),
        )))
}

fn run_native_subagent_tool(
    runtime: Arc<LiveSubagentToolRuntime>,
    task: Task,
) -> RuntimeResult<RunnerResult> {
    let request: AgentToolExecuteRequest = serde_json::from_value(task.payload.clone().into())
        .map_err(|error| {
            mutsuki_agent_sdk::runtime_failure(
                SUBAGENT_TOOL_PLUGIN_ID,
                &task.task_id,
                AgentError::invalid_input(error.to_string()),
            )
        })?;
    let output = runtime.execute(request).map_err(|error| {
        mutsuki_agent_sdk::runtime_failure(SUBAGENT_TOOL_PLUGIN_ID, &task.task_id, error)
    })?;
    let mut result = RunnerResult::completed(task.task_id);
    result.output = Some(output);
    Ok(result)
}

fn required_input_string(
    input: &serde_json::Map<String, Value>,
    field: &'static str,
) -> AgentResult<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| AgentError::invalid_input(format!("subagent tool requires `{field}`")))
}

fn compact_identifier(value: &str) -> String {
    let compact = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(64)
        .collect::<String>();
    if compact.is_empty() {
        "call".to_owned()
    } else {
        compact
    }
}

fn task_output(outcome: TaskOutcome) -> AgentResult<serde_json::Value> {
    match outcome {
        TaskOutcome::Completed {
            output: Some(output),
            ..
        } => Ok(output),
        TaskOutcome::Completed { .. } => Err(AgentError::new(
            "agent.result_missing",
            "AgentKit task completed without a typed result",
        )),
        TaskOutcome::Failed { error, .. } => Err(AgentError::new(
            error.code,
            error
                .evidence
                .get("message")
                .and_then(|value| match value {
                    mutsuki_runtime_contracts::ScalarValue::String(message) => {
                        Some(message.clone())
                    }
                    _ => None,
                })
                .unwrap_or(error.route),
        )),
        other => Err(AgentError::new(
            "agent.host.task_failed",
            format!("AgentKit task did not complete: {other:?}"),
        )),
    }
}

fn protocol_error(error: mutsuki_agent_contracts::ProtocolError) -> AgentError {
    AgentError::new(error.code, error.message)
}

fn runtime_error(error: RuntimeFailure) -> AgentError {
    let runtime = error.error();
    AgentError::new(
        runtime.code.clone(),
        runtime
            .evidence
            .get("message")
            .and_then(|value| match value {
                mutsuki_runtime_contracts::ScalarValue::String(message) => Some(message.clone()),
                _ => None,
            })
            .unwrap_or_else(|| runtime.route.clone()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn architecture_tool_is_a_typed_model_visible_interaction() {
        let descriptor = project_architecture_tool_descriptor();
        assert_eq!(descriptor.name, PROJECT_ARCHITECTURE_TOOL_NAME);
        assert_eq!(descriptor.target_protocol_id, AGENT_RUN_PROTOCOL);
        assert!(matches!(
            descriptor.execution,
            AgentToolExecution::Interaction {
                interaction_kind: InteractionKind::Custom
            }
        ));
        assert_eq!(descriptor.input_schema["type"], "object");
        assert!(descriptor.input_schema["properties"]["changes"].is_object());
    }
}

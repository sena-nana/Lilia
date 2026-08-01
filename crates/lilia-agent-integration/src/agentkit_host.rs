use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use mutsuki_agent_adapter_api::{CredentialBroker, ModelProtocolAdapter};
use mutsuki_agent_adapter_openai::OpenAiCompatibleAdapter;
use mutsuki_agent_bundle::{
    native_coding_tool_plugin, AdapterBackedModelProvider, AgentLoop, AgentRuntimeRunner,
    ModelGateway, NativeCodingAgentBundle, ToolRegistry,
};
use mutsuki_agent_contracts::{AgentError, AgentResult};
use mutsuki_runtime_contracts::{
    PluginDeploymentKind, RuntimeProfile, RuntimeProfileMode, Task, TaskBatch, TaskHandle,
    TaskOutcome, TaskStatus,
};
use mutsuki_runtime_host::{
    HostRuntime, HostRuntimeConfig, RuntimeBootstrapper, TokioAsyncExecutor,
};
use mutsuki_runtime_sdk::{
    HostRuntime as _, RuntimeClient, RuntimeClientRef, RuntimeFailure, RuntimeResult,
    TaskSubmitterRuntimeClient,
};

use crate::anthropic_adapter::AnthropicMessagesAdapter;
use crate::model_turn::{openai_adapter_descriptor, LiveModelDriver, LiveModelTurnPlan};

const HOST_PROFILE_ID: &str = "lilia.native-coding.agentkit-host";

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
}

impl AgentKitHost {
    pub(crate) fn build(
        mut bundle: NativeCodingAgentBundle,
        plan: Option<&LiveModelTurnPlan>,
        credentials: Arc<dyn CredentialBroker>,
        enable_tools: bool,
    ) -> AgentResult<Self> {
        let routed_tools = if enable_tools {
            bundle.routed_model_tools()
        } else {
            Vec::new()
        };
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

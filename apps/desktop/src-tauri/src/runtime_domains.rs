use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use mutsuki_runtime_contracts::{
    CrossDomainTaskRequest, DispatchLane, DomainTaskHandle, ExecutionClass, ObservabilityProfile,
    RunnerDescriptor, RunnerPurity, RunnerResult, RuntimeDomainId, RuntimeError, RuntimeProfile,
    RuntimeProfileMode, Task, TaskOutcome,
};
use mutsuki_runtime_core::RuntimeFailure;
use mutsuki_runtime_host::{
    runner_manifest, ExecutionDomainConfig, HostRuntime, HostRuntimeConfig, NativeRunner,
    RuntimeBootstrapper, RuntimeGroupHost,
};
use mutsuki_runtime_sdk::HostServiceRegistry;
use serde_json::Value;

pub const PRODUCT_DOMAIN_ID: &str = "lilia-product-domain";
pub const AGENT_DOMAIN_ID: &str = "lilia-agent-domain";
pub const WORKSPACE_DOMAIN_ID: &str = "lilia-workspace-domain";
pub const SHARED_DOMAIN_ID: &str = "lilia-shared-domain";

pub const PRODUCT_COMMAND_PROTOCOL: &str = "lilia.reference.product.command.v1";
pub const AGENT_EVENT_PROTOCOL: &str = "lilia.reference.agent.event.v1";
pub const AGENT_COMPLETION_PROTOCOL: &str = "lilia.reference.agent.completion.v1";
pub const WORKSPACE_SCAN_PROTOCOL: &str = "lilia.reference.workspace.scan.v1";
pub const WORKSPACE_INDEX_PROTOCOL: &str = "lilia.reference.workspace.index.v1";

const REFERENCE_PLUGIN_ID: &str = "lilia.runtime-domains.reference";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiliaRuntimeTopology {
    SingleDomain,
    ProductAgentWorkspace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiliaWorkload {
    ProductCommand,
    AgentEvent,
    AgentCompletion,
    WorkspaceScan,
    WorkspaceIndex,
}

impl LiliaWorkload {
    fn protocol(self) -> &'static str {
        match self {
            Self::ProductCommand => PRODUCT_COMMAND_PROTOCOL,
            Self::AgentEvent => AGENT_EVENT_PROTOCOL,
            Self::AgentCompletion => AGENT_COMPLETION_PROTOCOL,
            Self::WorkspaceScan => WORKSPACE_SCAN_PROTOCOL,
            Self::WorkspaceIndex => WORKSPACE_INDEX_PROTOCOL,
        }
    }

    fn dispatch_lane(self) -> DispatchLane {
        match self {
            Self::ProductCommand => DispatchLane::Interactive,
            Self::AgentEvent | Self::AgentCompletion => DispatchLane::Normal,
            Self::WorkspaceScan => DispatchLane::Background,
            Self::WorkspaceIndex => DispatchLane::Bulk,
        }
    }

    fn execution_class(self) -> ExecutionClass {
        match self {
            Self::ProductCommand => ExecutionClass::Orchestration,
            Self::AgentEvent | Self::AgentCompletion => ExecutionClass::Script,
            Self::WorkspaceScan => ExecutionClass::Blocking,
            Self::WorkspaceIndex => ExecutionClass::Cpu,
        }
    }
}

pub struct LiliaRuntimeDomainReference {
    topology: LiliaRuntimeTopology,
    group: RuntimeGroupHost,
}

impl LiliaRuntimeDomainReference {
    pub fn start(topology: LiliaRuntimeTopology) -> Result<Self, String> {
        let shared_services = Arc::new(HostServiceRegistry::new());
        shared_services.freeze();
        let mut group = RuntimeGroupHost::with_defaults(shared_services.clone());

        match topology {
            LiliaRuntimeTopology::SingleDomain => {
                group
                    .insert_domain(
                        domain_id(SHARED_DOMAIN_ID)?,
                        build_runtime(
                            shared_services,
                            SHARED_DOMAIN_ID,
                            &[
                                LiliaWorkload::ProductCommand,
                                LiliaWorkload::AgentEvent,
                                LiliaWorkload::AgentCompletion,
                                LiliaWorkload::WorkspaceScan,
                                LiliaWorkload::WorkspaceIndex,
                            ],
                            vec![ExecutionDomainConfig::new(
                                "lilia-shared",
                                vec![
                                    ExecutionClass::Orchestration,
                                    ExecutionClass::Io,
                                    ExecutionClass::Cpu,
                                    ExecutionClass::Blocking,
                                    ExecutionClass::Script,
                                ],
                                3,
                            )],
                        )?,
                    )
                    .map_err(|error| error.to_string())?;
            }
            LiliaRuntimeTopology::ProductAgentWorkspace => {
                group
                    .insert_domain(
                        domain_id(PRODUCT_DOMAIN_ID)?,
                        build_runtime(
                            shared_services.clone(),
                            PRODUCT_DOMAIN_ID,
                            &[LiliaWorkload::ProductCommand],
                            vec![ExecutionDomainConfig::new(
                                "product-interactive",
                                all_execution_classes(),
                                1,
                            )],
                        )?,
                    )
                    .map_err(|error| error.to_string())?;
                group
                    .insert_domain(
                        domain_id(AGENT_DOMAIN_ID)?,
                        build_runtime(
                            shared_services.clone(),
                            AGENT_DOMAIN_ID,
                            &[LiliaWorkload::AgentEvent, LiliaWorkload::AgentCompletion],
                            vec![ExecutionDomainConfig::new(
                                "agent-script",
                                all_execution_classes(),
                                1,
                            )],
                        )?,
                    )
                    .map_err(|error| error.to_string())?;
                group
                    .insert_domain(
                        domain_id(WORKSPACE_DOMAIN_ID)?,
                        build_runtime(
                            shared_services,
                            WORKSPACE_DOMAIN_ID,
                            &[LiliaWorkload::WorkspaceScan, LiliaWorkload::WorkspaceIndex],
                            vec![ExecutionDomainConfig::new(
                                "workspace-services",
                                all_execution_classes(),
                                1,
                            )],
                        )?,
                    )
                    .map_err(|error| error.to_string())?;
            }
        }

        Ok(Self { topology, group })
    }

    pub fn submit(
        &self,
        request_id: impl Into<String>,
        workload: LiliaWorkload,
        payload: Value,
    ) -> Result<DomainTaskHandle, String> {
        let request_id = request_id.into();
        let source_domain = self.route(LiliaWorkload::ProductCommand)?;
        let target_domain = self.route(workload)?;
        let mut task = Task::new(request_id.clone(), workload.protocol(), payload);
        task.dispatch_lane = workload.dispatch_lane();
        self.group
            .submit_cross_domain(CrossDomainTaskRequest {
                request_id: request_id.clone(),
                source_domain,
                target_domain,
                task,
                timeout_ms: 5_000,
                idempotency_key: format!("{request_id}:{}", workload.protocol()),
                max_attempts: 1,
            })
            .map_err(|error| error.to_string())
    }

    pub fn wait_outcome(
        &self,
        handle: &DomainTaskHandle,
        timeout: Duration,
    ) -> Result<Option<TaskOutcome>, String> {
        self.group
            .wait_outcome(handle, timeout)
            .map_err(|error| error.to_string())
    }

    pub fn route(&self, workload: LiliaWorkload) -> Result<RuntimeDomainId, String> {
        let id = match self.topology {
            LiliaRuntimeTopology::SingleDomain => SHARED_DOMAIN_ID,
            LiliaRuntimeTopology::ProductAgentWorkspace => match workload {
                LiliaWorkload::ProductCommand => PRODUCT_DOMAIN_ID,
                LiliaWorkload::AgentEvent | LiliaWorkload::AgentCompletion => AGENT_DOMAIN_ID,
                LiliaWorkload::WorkspaceScan | LiliaWorkload::WorkspaceIndex => WORKSPACE_DOMAIN_ID,
            },
        };
        domain_id(id)
    }

    pub fn group(&self) -> &RuntimeGroupHost {
        &self.group
    }
}

fn build_runtime(
    shared_services: Arc<HostServiceRegistry>,
    profile_id: &str,
    workloads: &[LiliaWorkload],
    execution_domains: Vec<ExecutionDomainConfig>,
) -> Result<HostRuntime, String> {
    let descriptors = workloads
        .iter()
        .copied()
        .map(descriptor)
        .collect::<Vec<_>>();
    let mut bootstrapper = RuntimeBootstrapper::new();
    bootstrapper.register_manifest(runner_manifest(REFERENCE_PLUGIN_ID, descriptors.clone()));
    bootstrapper
        .use_shared_services(shared_services)
        .map_err(|error| error.to_string())?;

    for (workload, descriptor) in workloads.iter().copied().zip(descriptors) {
        bootstrapper.register_runner(Box::new(NativeRunner::new(
            descriptor,
            move |_context, task| {
                let task_id = task.task_id.clone();
                let payload: Value = task.payload.into();
                let output = execute_reference_workload(workload, &payload)
                    .map_err(|message| reference_failure(workload, message))?;
                let mut result = RunnerResult::completed(task_id);
                result.output = Some(output);
                Ok(result)
            },
        )));
    }

    bootstrapper
        .into_host_runtime_with_config(
            profile(profile_id),
            HostRuntimeConfig {
                event_driven: true,
                execution_domains,
                ..HostRuntimeConfig::default()
            },
        )
        .map_err(|error| error.to_string())
}

fn execute_reference_workload(workload: LiliaWorkload, payload: &Value) -> Result<Value, String> {
    match workload {
        LiliaWorkload::ProductCommand => {
            crate::task_handoff::runtime_reference_prepare_handoff(payload)
        }
        LiliaWorkload::AgentEvent | LiliaWorkload::AgentCompletion => {
            crate::chat::runner::runtime_reference_agent_payload(payload)
        }
        LiliaWorkload::WorkspaceScan | LiliaWorkload::WorkspaceIndex => {
            let path = payload
                .get("path")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "reference workspace payload 缺少 path".to_string())?;
            let iterations = payload
                .get("iterations")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value > 0)
                .ok_or_else(|| "reference workspace payload 缺少有效 iterations".to_string())?;
            crate::worktrees::runtime_reference_inspect_worktrees(Path::new(path), iterations)
        }
    }
}

fn reference_failure(workload: LiliaWorkload, message: String) -> RuntimeFailure {
    RuntimeFailure::new(RuntimeError::new(
        "lilia.reference.invalid_input",
        "lilia.runtime-domain-reference",
        format!("{}.{}", workload.protocol(), message),
    ))
}

fn descriptor(workload: LiliaWorkload) -> RunnerDescriptor {
    RunnerDescriptor {
        runner_id: format!("{}.runner", workload.protocol()),
        plugin_id: REFERENCE_PLUGIN_ID.into(),
        plugin_generation: 1,
        accepted_protocol_ids: vec![workload.protocol().into()],
        purity: RunnerPurity::Pure,
        execution_class: workload.execution_class(),
        invocation_mode: Default::default(),
        concurrency: Default::default(),
        input_schema: serde_json::json!({}),
        output_schema: serde_json::json!({}),
        batch: Default::default(),
        payload: Default::default(),
        resources: Default::default(),
        ordering: Default::default(),
        control: Default::default(),
        metadata: BTreeMap::new(),
        contract_surfaces: vec![format!("runner:{}", workload.protocol())],
    }
}

fn profile(profile_id: &str) -> RuntimeProfile {
    RuntimeProfile {
        profile_id: format!("lilia-reference-{profile_id}"),
        mode: RuntimeProfileMode::FullDev,
        enabled_plugins: vec![REFERENCE_PLUGIN_ID.into()],
        bindings: BTreeMap::new(),
        plugin_deployments: BTreeMap::new(),
        observability: ObservabilityProfile::default(),
        allow_dynamic_registration: false,
        allow_hot_reload: false,
    }
}

fn all_execution_classes() -> Vec<ExecutionClass> {
    vec![
        ExecutionClass::Orchestration,
        ExecutionClass::Io,
        ExecutionClass::Cpu,
        ExecutionClass::Blocking,
        ExecutionClass::Script,
    ]
}

fn domain_id(value: &str) -> Result<RuntimeDomainId, String> {
    RuntimeDomainId::new(value).map_err(|error| format!("{error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn handoff_payload() -> Value {
        json!({
            "protocol": "lilia-code-task-handoff",
            "version": 1,
            "id": "issue43-reference",
            "createdAt": "2026-07-26T00:00:00Z",
            "title": "验证三运行域",
            "kind": "repository",
            "repository": {
                "fullName": "sena-nana/LiliaCode",
                "worktreePath": ".",
                "branch": "main",
                "remoteUrl": "https://github.com/sena-nana/LiliaCode.git"
            },
            "source": {
                "application": "LiliaGithub",
                "route": "/repos/sena-nana/LiliaCode",
                "objectUrl": null
            },
            "problem": "在后台扫描和 Agent 事件满载时推进产品 projection。",
            "relatedFiles": [],
            "logSummary": null,
            "acceptanceCriteria": ["产品命令保持低尾延迟"],
            "pullRequest": null,
            "workflow": null
        })
    }

    fn payload(workload: LiliaWorkload) -> Value {
        match workload {
            LiliaWorkload::ProductCommand => handoff_payload(),
            LiliaWorkload::AgentEvent | LiliaWorkload::AgentCompletion => json!({
                "taskId": "issue43-agent",
                "backend": "codex",
                "cwd": ".",
                "prompt": "检查 RuntimeDomain reference profile"
            }),
            LiliaWorkload::WorkspaceScan | LiliaWorkload::WorkspaceIndex => json!({
                "path": ".",
                "iterations": 1
            }),
        }
    }

    #[test]
    fn three_domain_reference_runs_real_work_and_isolates_workspace_abort() {
        let reference =
            LiliaRuntimeDomainReference::start(LiliaRuntimeTopology::ProductAgentWorkspace)
                .unwrap();
        assert_eq!(
            reference
                .group()
                .snapshots()
                .unwrap()
                .into_iter()
                .map(|snapshot| snapshot.domain_id.to_string())
                .collect::<Vec<_>>(),
            vec![AGENT_DOMAIN_ID, PRODUCT_DOMAIN_ID, WORKSPACE_DOMAIN_ID]
        );

        for (index, workload) in [
            LiliaWorkload::ProductCommand,
            LiliaWorkload::AgentEvent,
            LiliaWorkload::AgentCompletion,
            LiliaWorkload::WorkspaceScan,
            LiliaWorkload::WorkspaceIndex,
        ]
        .into_iter()
        .enumerate()
        {
            let handle = reference
                .submit(format!("reference-{index}"), workload, payload(workload))
                .unwrap();
            assert!(matches!(
                reference
                    .wait_outcome(&handle, Duration::from_secs(2))
                    .unwrap(),
                Some(TaskOutcome::Completed {
                    output: Some(_),
                    ..
                })
            ));
        }

        reference
            .group()
            .abort_domain(
                &domain_id(WORKSPACE_DOMAIN_ID).unwrap(),
                "test.workspace.abort",
            )
            .unwrap();
        for (request_id, workload) in [
            (
                "product-after-workspace-abort",
                LiliaWorkload::ProductCommand,
            ),
            ("agent-after-workspace-abort", LiliaWorkload::AgentEvent),
        ] {
            let handle = reference
                .submit(request_id, workload, payload(workload))
                .unwrap();
            assert!(matches!(
                reference
                    .wait_outcome(&handle, Duration::from_secs(2))
                    .unwrap(),
                Some(TaskOutcome::Completed { .. })
            ));
        }
    }

    #[test]
    fn reference_rejects_invalid_business_inputs_as_failed_tasks() {
        let reference =
            LiliaRuntimeDomainReference::start(LiliaRuntimeTopology::ProductAgentWorkspace)
                .unwrap();
        for (index, workload) in [
            LiliaWorkload::ProductCommand,
            LiliaWorkload::AgentEvent,
            LiliaWorkload::WorkspaceScan,
        ]
        .into_iter()
        .enumerate()
        {
            let handle = reference
                .submit(format!("invalid-{index}"), workload, json!({}))
                .unwrap();
            assert!(matches!(
                reference
                    .wait_outcome(&handle, Duration::from_secs(2))
                    .unwrap(),
                Some(TaskOutcome::Failed { .. })
            ));
        }
    }

    #[test]
    fn single_domain_reference_keeps_the_same_protocols_and_worker_budget() {
        let reference =
            LiliaRuntimeDomainReference::start(LiliaRuntimeTopology::SingleDomain).unwrap();
        let snapshots = reference.group().snapshots().unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].domain_id.to_string(), SHARED_DOMAIN_ID);
        assert_eq!(
            snapshots[0]
                .execution_domains
                .iter()
                .map(|domain| domain.configured_threads)
                .sum::<usize>(),
            3
        );
    }
}

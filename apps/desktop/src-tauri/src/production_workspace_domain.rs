//! Production workspace RuntimeDomain slice (#43 → #44/#52/#60 phase-1).
//!
//! Hosts only `lilia-workspace-domain` with a real git worktree list runner.
//! Product SQLite and AgentKit turns stay on Embedded/Service bootstrap — no
//! empty product/agent domain injection and no second fact source.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};
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
use serde_json::{json, Value};

pub const WORKSPACE_DOMAIN_ID: &str = "lilia-workspace-domain";
pub const WORKSPACE_LIST_PROTOCOL: &str = "lilia.workspace.worktree.list.v1";
const WORKSPACE_PLUGIN_ID: &str = "lilia.runtime-domains.workspace";

pub struct ProductionWorkspaceDomain {
    group: RuntimeGroupHost,
    domain_id: RuntimeDomainId,
}

impl ProductionWorkspaceDomain {
    pub fn start() -> Result<Self, String> {
        let shared_services = Arc::new(HostServiceRegistry::new());
        shared_services.freeze();
        let mut group = RuntimeGroupHost::with_defaults(shared_services.clone());
        let domain_id = RuntimeDomainId::new(WORKSPACE_DOMAIN_ID).map_err(|error| {
            format!(
                "workspace domain id: {}:{}:{}",
                error.code, error.source, error.route
            )
        })?;
        group
            .insert_domain(domain_id.clone(), build_workspace_runtime(shared_services)?)
            .map_err(|error| error.to_string())?;
        Ok(Self { group, domain_id })
    }

    pub fn list_worktrees(&self, base_repo_path: &Path) -> Result<Value, String> {
        let request_id = format!(
            "workspace-list-{}-{}",
            std::process::id(),
            crate::util::now_millis()
        );
        let mut task = Task::new(
            request_id.clone(),
            WORKSPACE_LIST_PROTOCOL,
            json!({ "path": base_repo_path.to_string_lossy() }),
        );
        task.dispatch_lane = DispatchLane::Background;
        let handle = self
            .group
            .submit_cross_domain(CrossDomainTaskRequest {
                request_id: request_id.clone(),
                source_domain: self.domain_id.clone(),
                target_domain: self.domain_id.clone(),
                task,
                timeout_ms: 15_000,
                idempotency_key: format!("{request_id}:{WORKSPACE_LIST_PROTOCOL}"),
                max_attempts: 1,
            })
            .map_err(|error| error.to_string())?;
        wait_completed(&self.group, &handle)
    }
}

fn global_workspace_domain() -> Result<&'static ProductionWorkspaceDomain, String> {
    static DOMAIN: OnceLock<ProductionWorkspaceDomain> = OnceLock::new();
    if let Some(domain) = DOMAIN.get() {
        return Ok(domain);
    }
    // Cache only successful starts so a transient failure can retry.
    let started = ProductionWorkspaceDomain::start()?;
    Ok(DOMAIN.get_or_init(|| started))
}

pub fn list_worktrees_via_domain(
    base_repo_path: &Path,
) -> Result<Vec<crate::worktrees::GitWorktree>, String> {
    match global_workspace_domain() {
        Ok(domain) => {
            let output = domain.list_worktrees(base_repo_path)?;
            let worktrees = output
                .get("worktrees")
                .cloned()
                .ok_or_else(|| "workspace domain response missing worktrees".to_string())?;
            serde_json::from_value(worktrees)
                .map_err(|error| format!("workspace domain worktrees decode failed: {error}"))
        }
        Err(error) => {
            eprintln!(
                "[workspace-domain] init failed, falling back to direct git worktree list: {error}"
            );
            crate::worktrees::list_git_worktrees(base_repo_path)
        }
    }
}

fn build_workspace_runtime(
    shared_services: Arc<HostServiceRegistry>,
) -> Result<HostRuntime, String> {
    let descriptor = RunnerDescriptor {
        runner_id: format!("{WORKSPACE_LIST_PROTOCOL}.runner"),
        plugin_id: WORKSPACE_PLUGIN_ID.into(),
        plugin_generation: 1,
        accepted_protocol_ids: vec![WORKSPACE_LIST_PROTOCOL.into()],
        purity: RunnerPurity::Pure,
        execution_class: ExecutionClass::Blocking,
        invocation_mode: Default::default(),
        concurrency: Default::default(),
        input_schema: json!({}),
        output_schema: json!({}),
        batch: Default::default(),
        payload: Default::default(),
        resources: Default::default(),
        ordering: Default::default(),
        control: Default::default(),
        metadata: BTreeMap::new(),
        contract_surfaces: vec![format!("runner:{WORKSPACE_LIST_PROTOCOL}")],
    };
    let mut bootstrapper = RuntimeBootstrapper::new();
    bootstrapper.register_manifest(runner_manifest(
        WORKSPACE_PLUGIN_ID,
        vec![descriptor.clone()],
    ));
    bootstrapper
        .use_shared_services(shared_services)
        .map_err(|error| error.to_string())?;
    bootstrapper.register_runner(Box::new(NativeRunner::new(descriptor, move |_context, task| {
        let task_id = task.task_id.clone();
        let payload: Value = task.payload.into();
        let path = payload
            .get("path")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                RuntimeFailure::new(RuntimeError::new(
                    "lilia.workspace.invalid_input",
                    WORKSPACE_PLUGIN_ID,
                    "workspace worktree list payload 缺少 path",
                ))
            })?;
        let worktrees = crate::worktrees::list_git_worktrees(Path::new(path)).map_err(|message| {
            RuntimeFailure::new(RuntimeError::new(
                "lilia.workspace.failed",
                WORKSPACE_PLUGIN_ID,
                message,
            ))
        })?;
        let mut result = RunnerResult::completed(task_id);
        result.output = Some(json!({ "worktrees": worktrees }));
        Ok(result)
    })));
    bootstrapper
        .into_host_runtime_with_config(
            RuntimeProfile {
                profile_id: "lilia-production-workspace".into(),
                mode: RuntimeProfileMode::FullDev,
                enabled_plugins: vec![WORKSPACE_PLUGIN_ID.into()],
                bindings: BTreeMap::new(),
                plugin_deployments: BTreeMap::new(),
                observability: ObservabilityProfile::default(),
                allow_dynamic_registration: false,
                allow_hot_reload: false,
            },
            HostRuntimeConfig {
                event_driven: true,
                execution_domains: vec![ExecutionDomainConfig::new(
                    "workspace-services",
                    vec![
                        ExecutionClass::Orchestration,
                        ExecutionClass::Io,
                        ExecutionClass::Cpu,
                        ExecutionClass::Blocking,
                        ExecutionClass::Script,
                    ],
                    1,
                )],
                ..HostRuntimeConfig::default()
            },
        )
        .map_err(|error| error.to_string())
}

fn wait_completed(
    group: &RuntimeGroupHost,
    handle: &DomainTaskHandle,
) -> Result<Value, String> {
    let outcome = group
        .wait_outcome(handle, Duration::from_secs(20))
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "workspace domain task timed out".to_string())?;
    match outcome {
        TaskOutcome::Completed { output, .. } => output
            .ok_or_else(|| "workspace domain completed without output".to_string()),
        TaskOutcome::Failed { error, .. } => Err(format!(
            "{}:{}:{}",
            error.code, error.source, error.route
        )),
        other => Err(format!("workspace domain unexpected outcome: {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn production_workspace_domain_lists_real_worktrees() {
        let root = std::env::temp_dir().join(format!(
            "lilia-workspace-domain-{}-{}",
            std::process::id(),
            crate::util::now_millis()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let status = Command::new("git")
            .args(["init"])
            .current_dir(&root)
            .status()
            .expect("git available");
        assert!(status.success());
        let domain = ProductionWorkspaceDomain::start().unwrap();
        assert_eq!(domain.domain_id.as_str(), WORKSPACE_DOMAIN_ID);
        let output = domain.list_worktrees(&root).unwrap();
        let worktrees = output
            .get("worktrees")
            .and_then(Value::as_array)
            .expect("worktrees array");
        assert!(!worktrees.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
}

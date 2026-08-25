//! Kernel [`Jobs`](lilia_kernel::Jobs) backed by a Mutsuki `HostRuntime`.
//!
//! Every long LiliaCode operation is a protocol with a runner rather than a
//! thread: the runtime supplies idempotency keys, cancellation, leases and
//! terminal outcomes, so callers stop maintaining per-operation sequence
//! counters to discard stale results.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use lilia_kernel::{
    JobContext, JobError, JobHandler, JobProtocol, TaskProgress, TaskRuntime, TaskSpec, TaskTicket,
};
use mutsuki_runtime_contracts::{
    PluginDeploymentKind, ProtocolClass, RuntimeProfile, RuntimeProfileMode, Task, TaskHandle,
    TaskOutcome,
};
use mutsuki_runtime_host::{HostRuntime, HostRuntimeConfig, RuntimeBootstrapper};
use mutsuki_runtime_contracts::{
    BatchEntry, CompletionBatch, EntryCompletion, RunnerDescriptor, RunnerResult, RuntimeError,
    ScalarValue, WorkBatch,
};
use mutsuki_runtime_core::{Runner, RunnerContext, RuntimeResult};
use mutsuki_runtime_sdk::{HostRuntime as _, PluginBuilder, ProtocolDescriptorBuilder};
use serde_json::Value;

const PLUGIN_ID: &str = "lilia.plugin.jobs";
const PROFILE_ID: &str = "lilia.jobs";

/// Maps a runtime task id to the kernel [`JobContext`] its handler writes to.
/// Both the runtime port and the runner hold the same registry, so progress and
/// cancellation cross the task pool boundary without a second event stream.
#[derive(Default)]
struct ContextRegistry {
    contexts: Mutex<HashMap<String, JobContext>>,
}

impl ContextRegistry {
    fn open(&self, task_id: &str) -> JobContext {
        self.locked()
            .entry(task_id.to_owned())
            .or_default()
            .clone()
    }

    fn get(&self, task_id: &str) -> Option<JobContext> {
        self.locked().get(task_id).cloned()
    }

    fn close(&self, task_id: &str) {
        self.locked().remove(task_id);
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, HashMap<String, JobContext>> {
        self.contexts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JobRuntimeError {
    #[error("job protocol {0} is registered twice")]
    DuplicateProtocol(String),

    #[error("job runtime failed to start: {0}")]
    Bootstrap(String),
}

#[derive(Default)]
pub struct LiliaJobRuntimeBuilder {
    handlers: BTreeMap<String, JobHandler>,
}

impl LiliaJobRuntimeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Binds one protocol id, such as `lilia.project/clone@1`, to its handler.
    pub fn protocol(
        mut self,
        protocol: impl Into<String>,
        handler: JobHandler,
    ) -> Result<Self, JobRuntimeError> {
        let protocol = protocol.into();
        if self.handlers.contains_key(&protocol) {
            return Err(JobRuntimeError::DuplicateProtocol(protocol));
        }
        self.handlers.insert(protocol, handler);
        Ok(self)
    }

    /// Binds every protocol a feature declared through
    /// [`lilia_kernel::Feature::protocols`].
    pub fn protocols(
        self,
        protocols: impl IntoIterator<Item = JobProtocol>,
    ) -> Result<Self, JobRuntimeError> {
        protocols
            .into_iter()
            .try_fold(self, |builder, protocol| {
                builder.protocol(protocol.id, protocol.handler)
            })
    }

    pub fn build(self) -> Result<LiliaJobRuntime, JobRuntimeError> {
        let contexts = Arc::new(ContextRegistry::default());
        let mut plugin = PluginBuilder::new(PLUGIN_ID);
        for (index, (protocol, handler)) in self.handlers.into_iter().enumerate() {
            let descriptor = mutsuki_agent_sdk::effectful_runner(
                format!("lilia.jobs.runner.{index}"),
                PLUGIN_ID,
            )
            .accepted_protocol(protocol.clone())
            .build();
            plugin = plugin
                .protocol_descriptor(
                    ProtocolDescriptorBuilder::new(protocol.clone())
                        .input_schema(serde_json::json!({ "type": ["object", "array", "string", "number", "boolean", "null"] }))
                        .output_schema(serde_json::json!({ "type": ["object", "array", "string", "number", "boolean", "null"] }))
                        .error_schema(serde_json::json!({ "type": "object" }))
                        .build(),
                )
                .protocol_class(protocol, ProtocolClass::Effect)
                .runner(Box::new(JobRunner {
                    descriptor,
                    handler,
                    contexts: Arc::clone(&contexts),
                }));
        }

        let mut built = plugin.build();
        let manifest = built.manifest.clone();
        let mut bootstrapper = RuntimeBootstrapper::new();
        bootstrapper.register_manifest(manifest.clone());
        for runner in built.runners.drain(..) {
            bootstrapper.register_builtin_runner(runner);
        }
        let runtime = bootstrapper
            .into_host_runtime_with_config(
                job_profile(&manifest.plugin_id),
                HostRuntimeConfig {
                    event_driven: true,
                    ..HostRuntimeConfig::default()
                },
            )
            .map_err(|error| JobRuntimeError::Bootstrap(error.to_string()))?;
        Ok(LiliaJobRuntime {
            runtime: Arc::new(runtime),
            handles: Mutex::new(HashMap::new()),
            next_task: AtomicU64::new(1),
            contexts,
        })
    }
}

/// The kernel's [`TaskRuntime`] port, implemented on Mutsuki's task pool.
pub struct LiliaJobRuntime {
    runtime: Arc<HostRuntime>,
    handles: Mutex<HashMap<String, TaskHandle>>,
    next_task: AtomicU64,
    contexts: Arc<ContextRegistry>,
}

impl LiliaJobRuntime {
    pub fn builder() -> LiliaJobRuntimeBuilder {
        LiliaJobRuntimeBuilder::new()
    }

    fn handle(&self, ticket: &TaskTicket) -> Result<TaskHandle, JobError> {
        self.locked()
            .get(ticket.as_str())
            .cloned()
            .ok_or_else(|| JobError::Rejected {
                protocol: String::new(),
                message: format!("task {} is unknown to the job runtime", ticket.as_str()),
            })
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, HashMap<String, TaskHandle>> {
        self.handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl TaskRuntime for LiliaJobRuntime {
    fn submit(&self, spec: &TaskSpec) -> Result<TaskTicket, JobError> {
        let sequence = self.next_task.fetch_add(1, Ordering::Relaxed);
        let task_id = format!("lilia.job.{sequence}");
        let mut task = Task::new(task_id.clone(), spec.protocol.clone(), spec.payload.clone());
        task.idempotency_key = Some(spec.idempotency_key.clone());
        self.contexts.open(&task_id);
        let handle = self
            .runtime
            .submit_task(task)
            .map_err(|error| JobError::Rejected {
                protocol: spec.protocol.clone(),
                message: error.to_string(),
            })?;
        self.locked().insert(task_id.clone(), handle);
        Ok(TaskTicket::new(task_id))
    }

    fn cancel(&self, ticket: &TaskTicket) -> Result<(), JobError> {
        let handle = self.handle(ticket)?;
        if let Some(context) = self.contexts.get(ticket.as_str()) {
            context.request_cancel();
        }
        self.runtime
            .cancel_task(&handle)
            .map_err(|error| JobError::Rejected {
                protocol: handle.protocol_id.clone(),
                message: error.to_string(),
            })
    }

    fn poll(&self, ticket: &TaskTicket) -> Result<TaskProgress, JobError> {
        let handle = self.handle(ticket)?;
        let outcome = self
            .runtime
            .task_outcome(&handle)
            .map_err(|error| JobError::Rejected {
                protocol: handle.protocol_id.clone(),
                message: error.to_string(),
            })?;
        let context = self.contexts.get(ticket.as_str());
        let cancelled = context
            .as_ref()
            .is_some_and(|context| context.is_cancelled());
        let progress = match outcome {
            None if cancelled => TaskProgress::Cancelled,
            None => TaskProgress::Running(context.map_or(Value::Null, |context| context.progress())),
            Some(_) if cancelled => TaskProgress::Cancelled,
            Some(TaskOutcome::Completed { output, .. }) => {
                TaskProgress::Completed(output.unwrap_or(Value::Null))
            }
            Some(TaskOutcome::Failed { error, .. }) => TaskProgress::Failed(runtime_message(error)),
            Some(TaskOutcome::Cancelled { .. }) => TaskProgress::Cancelled,
            Some(other) => TaskProgress::Failed(format!("job did not complete: {other:?}")),
        };
        if matches!(
            progress,
            TaskProgress::Completed(_) | TaskProgress::Failed(_) | TaskProgress::Cancelled
        ) {
            // The kernel drops a settled job from its inflight set and never
            // polls this ticket again, so both entries can go. Keeping them
            // would grow one context and one handle per job for the life of
            // the process.
            self.contexts.close(ticket.as_str());
            self.locked().remove(ticket.as_str());
        }
        Ok(progress)
    }
}

struct JobRunner {
    descriptor: RunnerDescriptor,
    handler: JobHandler,
    contexts: Arc<ContextRegistry>,
}

impl Runner for JobRunner {
    fn descriptor(&self) -> &RunnerDescriptor {
        &self.descriptor
    }

    fn run_batch(&mut self, _ctx: RunnerContext, batch: WorkBatch) -> RuntimeResult<CompletionBatch> {
        let mut results = Vec::with_capacity(batch.entries.len());
        for entry in &batch.entries {
            let completion = match batch.payload_task(entry.payload_index) {
                Ok(task) if task.task_id == entry.task_id => {
                    let payload: Value = task.payload.clone().into();
                    let context = self.contexts.open(&entry.task_id);
                    match run_handler(&self.handler, payload, &context) {
                        Ok(output) => {
                            let mut result = RunnerResult::completed(entry.task_id.clone());
                            result.output = Some(output);
                            EntryCompletion {
                                entry_id: entry.entry_id.clone(),
                                task_id: entry.task_id.clone(),
                                result: Some(result),
                                error: None,
                            }
                        }
                        Err(message) => failed_entry(entry, "lilia.job.failed", message),
                    }
                }
                Ok(_) | Err(_) => failed_entry(
                    entry,
                    mutsuki_runtime_contracts::ERR_TASK_CLAIM_CONFLICT,
                    format!("batch entry {} does not match its payload", entry.entry_id),
                ),
            };
            results.push(completion);
        }
        Ok(CompletionBatch::from_results(&batch, results))
    }
}

/// Runs one job handler, turning a panic into an ordinary job failure.
///
/// Without this, a panicking handler unwinds out of `run_batch`: the task never
/// reaches a terminal outcome, so the kernel keeps polling it forever and the
/// surface that submitted it stays busy until the process restarts. Worse, a
/// batch carries several entries, so one panicking job would strand every
/// sibling job batched with it. Catching here keeps the blast radius at the one
/// operation that panicked, which is what a thread-per-operation host gave us.
///
/// `AssertUnwindSafe` is sound here because nothing reads state back out of a
/// panicked handler: the caller reports `Failed`, and the shell discards
/// whatever the operation had parked.
fn run_handler(
    handler: &JobHandler,
    payload: Value,
    context: &JobContext,
) -> Result<Value, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(payload, context)))
        .unwrap_or_else(|panic| Err(format!("job handler panicked: {}", panic_message(&panic))))
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        return (*message).to_owned();
    }
    if let Some(message) = panic.downcast_ref::<String>() {
        return message.clone();
    }
    "no panic message".to_owned()
}

fn failed_entry(
    entry: &BatchEntry,
    code: &str,
    message: String,
) -> EntryCompletion {
    let mut error = RuntimeError::new(code, PLUGIN_ID, format!("task.{}", entry.task_id));
    error
        .evidence
        .insert("message".to_owned(), ScalarValue::String(message));
    EntryCompletion {
        entry_id: entry.entry_id.clone(),
        task_id: entry.task_id.clone(),
        result: None,
        error: Some(error),
    }
}

fn runtime_message(error: RuntimeError) -> String {
    match error.evidence.get("message") {
        Some(ScalarValue::String(message)) => message.clone(),
        _ => error.route,
    }
}

fn job_profile(plugin_id: &str) -> RuntimeProfile {
    RuntimeProfile {
        profile_id: PROFILE_ID.into(),
        mode: RuntimeProfileMode::FullDev,
        enabled_plugins: vec![plugin_id.to_owned()],
        bindings: BTreeMap::new(),
        surface_bindings: BTreeMap::new(),
        supported_extensions: Vec::new(),
        plugin_deployments: [(plugin_id.to_owned(), PluginDeploymentKind::Builtin)]
            .into_iter()
            .collect(),
        observability: Default::default(),
        allow_dynamic_registration: false,
        allow_hot_reload: false,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use lilia_kernel::{JobRequest, JobSlot, JobState, Jobs};
    use serde_json::json;

    use super::*;

    const ECHO: &str = "lilia.test/echo@1";
    const BLOCK: &str = "lilia.test/block@1";

    fn settled(jobs: &Jobs, id: lilia_kernel::JobId) -> JobState {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let state = jobs.state(id).expect("job is tracked");
            if state.is_terminal() {
                return state;
            }
            assert!(Instant::now() < deadline, "job {id:?} never settled");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_submitted_job_runs_on_the_task_pool_and_returns_its_typed_output() {
        let runtime = LiliaJobRuntime::builder()
            .protocol(
                ECHO,
                Arc::new(|payload: Value, _: &JobContext| Ok(json!({ "echoed": payload }))),
            )
            .unwrap()
            .build()
            .unwrap();
        let jobs = Jobs::new(lilia_kernel::EventBus::new(), lilia_kernel::Journal::new());
        jobs.install_runtime(Arc::new(runtime));

        let handle = jobs
            .submit(JobRequest::new(ECHO, json!({ "value": 7 })))
            .unwrap();

        assert_eq!(
            settled(&jobs, handle.id()),
            JobState::Completed {
                output: json!({ "echoed": { "value": 7 } })
            }
        );
        jobs.shutdown();
    }

    #[test]
    fn a_failing_handler_settles_the_job_as_failed_rather_than_hanging() {
        let runtime = LiliaJobRuntime::builder()
            .protocol(
                ECHO,
                Arc::new(|_, _: &JobContext| Err("handler refused".to_owned())),
            )
            .unwrap()
            .build()
            .unwrap();
        let jobs = Jobs::new(lilia_kernel::EventBus::new(), lilia_kernel::Journal::new());
        jobs.install_runtime(Arc::new(runtime));

        let handle = jobs.submit(JobRequest::new(ECHO, Value::Null)).unwrap();

        let JobState::Failed { message } = settled(&jobs, handle.id()) else {
            panic!("expected a failed job");
        };
        assert!(message.contains("handler refused"), "{message}");
        jobs.shutdown();
    }

    #[test]
    fn a_panicking_handler_fails_its_own_job_instead_of_stranding_the_lane() {
        let runtime = LiliaJobRuntime::builder()
            .protocol(
                ECHO,
                Arc::new(|_, _: &JobContext| panic!("the registry vanished")),
            )
            .unwrap()
            .build()
            .unwrap();
        let jobs = Jobs::new(lilia_kernel::EventBus::new(), lilia_kernel::Journal::new());
        jobs.install_runtime(Arc::new(runtime));

        let handle = jobs.submit(JobRequest::new(ECHO, Value::Null)).unwrap();

        let JobState::Failed { message } = settled(&jobs, handle.id()) else {
            panic!("a panicking handler must settle as failed, not hang");
        };
        assert!(message.contains("the registry vanished"), "{message}");
        jobs.shutdown();
    }

    #[test]
    fn a_panicking_job_leaves_a_later_job_on_the_same_runtime_runnable() {
        let runtime = LiliaJobRuntime::builder()
            .protocol(
                ECHO,
                Arc::new(|payload: Value, _: &JobContext| {
                    if payload == json!("explode") {
                        panic!("boom");
                    }
                    Ok(payload)
                }),
            )
            .unwrap()
            .build()
            .unwrap();
        let jobs = Jobs::new(lilia_kernel::EventBus::new(), lilia_kernel::Journal::new());
        jobs.install_runtime(Arc::new(runtime));

        let exploding = jobs.submit(JobRequest::new(ECHO, json!("explode"))).unwrap();
        assert!(matches!(
            settled(&jobs, exploding.id()),
            JobState::Failed { .. }
        ));

        let survivor = jobs.submit(JobRequest::new(ECHO, json!("intact"))).unwrap();

        assert_eq!(
            settled(&jobs, survivor.id()),
            JobState::Completed {
                output: json!("intact")
            }
        );
        jobs.shutdown();
    }

    #[test]
    fn a_settled_job_leaves_no_handle_or_context_behind() {
        let runtime = Arc::new(
            LiliaJobRuntime::builder()
                .protocol(ECHO, Arc::new(|payload: Value, _: &JobContext| Ok(payload)))
                .unwrap()
                .build()
                .unwrap(),
        );
        let jobs = Jobs::new(lilia_kernel::EventBus::new(), lilia_kernel::Journal::new());
        jobs.install_runtime(Arc::clone(&runtime) as Arc<dyn lilia_kernel::TaskRuntime>);

        for value in 0..3 {
            let handle = jobs.submit(JobRequest::new(ECHO, json!(value))).unwrap();
            settled(&jobs, handle.id());
        }

        assert!(
            runtime.locked().is_empty(),
            "settled jobs must not retain their task handles"
        );
        assert!(
            runtime.contexts.locked().is_empty(),
            "settled jobs must not retain their progress contexts"
        );
        jobs.shutdown();
    }

    #[test]
    fn a_second_submission_in_one_slot_supersedes_the_first() {
        let (release_sender, release_receiver) = mpsc::channel::<()>();
        let release_receiver = Arc::new(Mutex::new(release_receiver));
        let runtime = LiliaJobRuntime::builder()
            .protocol(
                BLOCK,
                Arc::new(move |payload: Value, _: &JobContext| {
                    let _ = release_receiver
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10));
                    Ok(payload)
                }),
            )
            .unwrap()
            .build()
            .unwrap();
        let jobs = Jobs::new(lilia_kernel::EventBus::new(), lilia_kernel::Journal::new());
        jobs.install_runtime(Arc::new(runtime));
        let slot = JobSlot::new("lilia.test.slot").unwrap();

        let first = jobs
            .submit(JobRequest::new(BLOCK, json!(1)).in_slot(slot.clone()))
            .unwrap();
        let second = jobs
            .submit(JobRequest::new(BLOCK, json!(2)).in_slot(slot))
            .unwrap();

        assert_eq!(jobs.state(first.id()), Some(JobState::Superseded));
        let _ = release_sender.send(());
        let _ = release_sender.send(());
        assert_eq!(settled(&jobs, second.id()), JobState::Completed { output: json!(2) });
        jobs.shutdown();
    }

    #[test]
    fn a_handler_reports_progress_and_observes_cancellation_at_its_own_safe_point() {
        let (started_sender, started_receiver) = mpsc::channel::<()>();
        let runtime = LiliaJobRuntime::builder()
            .protocol(
                BLOCK,
                Arc::new(move |_: Value, context: &JobContext| {
                    context.report(json!({ "percent": 25 }));
                    let _ = started_sender.send(());
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while !context.is_cancelled() {
                        assert!(Instant::now() < deadline, "cancellation never arrived");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err("cancelled".to_owned())
                }),
            )
            .unwrap()
            .build()
            .unwrap();
        let jobs = Jobs::new(lilia_kernel::EventBus::new(), lilia_kernel::Journal::new());
        jobs.install_runtime(Arc::new(runtime));

        let handle = jobs.submit(JobRequest::new(BLOCK, Value::Null)).unwrap();
        started_receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("the handler starts");

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if matches!(
                jobs.state(handle.id()),
                Some(JobState::Running { ref progress }) if *progress == json!({ "percent": 25 })
            ) {
                break;
            }
            assert!(Instant::now() < deadline, "progress never reached the kernel");
            std::thread::sleep(Duration::from_millis(10));
        }

        handle.cancel();
        assert_eq!(settled(&jobs, handle.id()), JobState::Cancelled);
        jobs.shutdown();
    }

    #[test]
    fn one_protocol_cannot_be_registered_twice() {
        let error = LiliaJobRuntime::builder()
            .protocol(ECHO, Arc::new(|payload, _: &JobContext| Ok(payload)))
            .unwrap()
            .protocol(ECHO, Arc::new(|payload, _: &JobContext| Ok(payload)))
            .err()
            .expect("a duplicate protocol is rejected");

        assert!(matches!(error, JobRuntimeError::DuplicateProtocol(protocol) if protocol == ECHO));
    }
}

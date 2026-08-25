use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock, Weak};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Event, EventBus, JobId, JobSlot, Journal, RecordKind};

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(20);
const TERMINAL_RETENTION: usize = 512;

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("no task runtime is installed; long work cannot be submitted")]
    RuntimeUnavailable,

    #[error("job protocol must not be blank")]
    InvalidProtocol,

    #[error("task runtime rejected protocol {protocol}: {message}")]
    Rejected { protocol: String, message: String },

    #[error("job {0} is unknown")]
    UnknownJob(JobId),
}

/// Handle produced by the task runtime for one submitted task.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskTicket(String);

impl TaskTicket {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskSpec {
    pub protocol: String,
    pub payload: Value,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TaskProgress {
    Pending,
    /// Still executing. The payload is the runner's latest progress report, or
    /// `Value::Null` when the runner reports none.
    Running(Value),
    Completed(Value),
    Failed(String),
    Cancelled,
}

/// Progress and cancellation channel for one running job. Handlers report
/// intermediate state here instead of pushing UI messages, and poll
/// [`JobContext::is_cancelled`] at their own safe points. The task runtime
/// mirrors the reported progress into [`JobState::Running`] and flips the
/// cancellation flag when the kernel cancels or supersedes the job.
#[derive(Clone, Default)]
pub struct JobContext {
    inner: Arc<JobContextInner>,
}

#[derive(Default)]
struct JobContextInner {
    progress: Mutex<Value>,
    cancelled: std::sync::atomic::AtomicBool,
}

impl JobContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn report(&self, progress: Value) {
        *self
            .inner
            .progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = progress;
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Relaxed)
    }

    /// Called by the task runtime, not by handlers.
    pub fn request_cancel(&self) {
        self.inner.cancelled.store(true, Ordering::Relaxed);
    }

    /// Called by the task runtime, not by handlers.
    pub fn progress(&self) -> Value {
        self.inner
            .progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl std::fmt::Debug for JobContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JobContext")
            .field("cancelled", &self.is_cancelled())
            .finish_non_exhaustive()
    }
}

/// Executes one job on a task runtime thread. Errors become
/// [`JobState::Failed`]; the handler never touches UI state directly.
pub type JobHandler = Arc<dyn Fn(Value, &JobContext) -> Result<Value, String> + Send + Sync>;

/// One protocol a feature declares before the task runtime is built. Declared
/// separately from [`crate::Feature::mount`] because the runtime binds its
/// handler set once at startup.
#[derive(Clone)]
pub struct JobProtocol {
    pub id: String,
    pub handler: JobHandler,
}

impl JobProtocol {
    pub fn new(id: impl Into<String>, handler: JobHandler) -> Self {
        Self {
            id: id.into(),
            handler,
        }
    }
}

impl std::fmt::Debug for JobProtocol {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JobProtocol")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// Port onto the process task runtime. Backed by Mutsuki's `HostRuntime` in the
/// desktop build so long work inherits idempotency keys, cancellation, leases
/// and child tasks instead of a bespoke thread per operation.
pub trait TaskRuntime: Send + Sync + 'static {
    fn submit(&self, spec: &TaskSpec) -> Result<TaskTicket, JobError>;
    fn cancel(&self, ticket: &TaskTicket) -> Result<(), JobError>;
    fn poll(&self, ticket: &TaskTicket) -> Result<TaskProgress, JobError>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobState {
    Pending,
    Running { progress: Value },
    Completed { output: Value },
    Failed { message: String },
    Cancelled,
    /// A newer submission took over this job's slot; the result is discarded.
    Superseded,
}

impl JobState {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Pending | Self::Running { .. })
    }

    fn topic(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running { .. } => "running",
            Self::Completed { .. } => "completed",
            Self::Failed { .. } => "failed",
            Self::Cancelled => "cancelled",
            Self::Superseded => "superseded",
        }
    }
}

/// Published on every job transition. Consumers match on `job_id`, which
/// removes the need for per-operation sequence fields to discard stale results.
#[derive(Clone, Debug, PartialEq)]
pub struct JobEvent {
    pub job_id: JobId,
    pub protocol: String,
    pub slot: Option<JobSlot>,
    pub state: JobState,
}

impl Event for JobEvent {
    const NAME: &'static str = "lilia.kernel.job";
}

#[derive(Clone, Debug, PartialEq)]
pub struct JobRequest {
    pub protocol: String,
    pub payload: Value,
    /// Deduplicates concurrent submissions. Defaults to a per-submission unique
    /// key when omitted.
    pub idempotency_key: Option<String>,
    /// Single-flight lane. A new submission cancels the slot's previous job.
    pub slot: Option<JobSlot>,
}

impl JobRequest {
    pub fn new(protocol: impl Into<String>, payload: Value) -> Self {
        Self {
            protocol: protocol.into(),
            payload,
            idempotency_key: None,
            slot: None,
        }
    }

    pub fn idempotent(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    pub fn in_slot(mut self, slot: JobSlot) -> Self {
        self.slot = Some(slot);
        self
    }
}

#[derive(Clone)]
pub struct JobHandle {
    id: JobId,
    protocol: String,
    jobs: Jobs,
}

impl JobHandle {
    pub fn id(&self) -> JobId {
        self.id
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    pub fn state(&self) -> Option<JobState> {
        self.jobs.state(self.id)
    }

    pub fn cancel(&self) {
        self.jobs.cancel(self.id);
    }
}

impl std::fmt::Debug for JobHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JobHandle")
            .field("id", &self.id)
            .field("protocol", &self.protocol)
            .finish_non_exhaustive()
    }
}

struct JobRecord {
    id: JobId,
    protocol: String,
    slot: Option<JobSlot>,
    idempotency_key: String,
    ticket: TaskTicket,
    state: JobState,
}

struct JobsState {
    inflight: Vec<JobRecord>,
    terminal: HashMap<JobId, JobState>,
    terminal_order: VecDeque<JobId>,
    worker_started: bool,
    shutdown: bool,
}

struct JobsInner {
    runtime: RwLock<Option<Arc<dyn TaskRuntime>>>,
    state: Mutex<JobsState>,
    wake: Condvar,
    next_id: AtomicU64,
    events: EventBus,
    journal: Journal,
    poll_interval: Duration,
}

/// Single entry point for long work. Features submit a protocol and payload and
/// react to [`JobEvent`]; no feature spawns threads or tracks its own operation
/// sequence numbers.
#[derive(Clone)]
pub struct Jobs {
    inner: Arc<JobsInner>,
}

impl Jobs {
    pub fn new(events: EventBus, journal: Journal) -> Self {
        Self::with_poll_interval(events, journal, DEFAULT_POLL_INTERVAL)
    }

    pub fn with_poll_interval(
        events: EventBus,
        journal: Journal,
        poll_interval: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(JobsInner {
                runtime: RwLock::new(None),
                state: Mutex::new(JobsState {
                    inflight: Vec::new(),
                    terminal: HashMap::new(),
                    terminal_order: VecDeque::new(),
                    worker_started: false,
                    shutdown: false,
                }),
                wake: Condvar::new(),
                next_id: AtomicU64::new(0),
                events,
                journal,
                poll_interval: poll_interval.max(Duration::from_millis(1)),
            }),
        }
    }

    pub fn install_runtime(&self, runtime: Arc<dyn TaskRuntime>) {
        *self
            .inner
            .runtime
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(runtime);
    }

    pub fn has_runtime(&self) -> bool {
        self.runtime().is_some()
    }

    pub fn submit(&self, request: JobRequest) -> Result<JobHandle, JobError> {
        if request.protocol.trim().is_empty() {
            return Err(JobError::InvalidProtocol);
        }
        let runtime = self.runtime().ok_or(JobError::RuntimeUnavailable)?;

        if let Some(existing) = request
            .idempotency_key
            .as_deref()
            .and_then(|key| self.find_inflight(&request.protocol, key))
        {
            return Ok(existing);
        }

        let superseded = request
            .slot
            .as_ref()
            .and_then(|slot| self.slot_occupant(slot));
        if let Some((id, ticket)) = superseded {
            // Settle first: once the record leaves the in-flight set the poll
            // loop cannot reclassify the supersede as a plain cancellation.
            self.settle(id, JobState::Superseded);
            let _ = runtime.cancel(&ticket);
        }

        let id = JobId::new(self.inner.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        let idempotency_key = request
            .idempotency_key
            .clone()
            .unwrap_or_else(|| format!("{}#{}", request.protocol, id.get()));
        let spec = TaskSpec {
            protocol: request.protocol.clone(),
            payload: request.payload,
            idempotency_key: idempotency_key.clone(),
        };
        let ticket = runtime.submit(&spec)?;

        {
            let mut state = self.locked();
            state.inflight.push(JobRecord {
                id,
                protocol: request.protocol.clone(),
                slot: request.slot.clone(),
                idempotency_key,
                ticket,
                state: JobState::Pending,
            });
        }
        self.announce(id, &request.protocol, request.slot.as_ref(), JobState::Pending);
        self.ensure_worker();
        self.inner.wake.notify_all();

        Ok(JobHandle {
            id,
            protocol: request.protocol,
            jobs: self.clone(),
        })
    }

    pub fn cancel(&self, id: JobId) {
        let target = {
            let state = self.locked();
            state
                .inflight
                .iter()
                .find(|record| record.id == id)
                .map(|record| record.ticket.clone())
        };
        let Some(ticket) = target else {
            return;
        };
        if let Some(runtime) = self.runtime() {
            let _ = runtime.cancel(&ticket);
        }
        self.settle(id, JobState::Cancelled);
    }

    /// Cancels the job currently occupying `slot`, if any.
    pub fn cancel_slot(&self, slot: &JobSlot) {
        if let Some((id, _)) = self.slot_occupant(slot) {
            self.cancel(id);
        }
    }

    pub fn state(&self, id: JobId) -> Option<JobState> {
        let state = self.locked();
        state
            .inflight
            .iter()
            .find(|record| record.id == id)
            .map(|record| record.state.clone())
            .or_else(|| state.terminal.get(&id).cloned())
    }

    pub fn inflight_count(&self) -> usize {
        self.locked().inflight.len()
    }

    pub fn shutdown(&self) {
        let tickets = {
            let mut state = self.locked();
            state.shutdown = true;
            state
                .inflight
                .iter()
                .map(|record| record.ticket.clone())
                .collect::<Vec<_>>()
        };
        if let Some(runtime) = self.runtime() {
            for ticket in &tickets {
                let _ = runtime.cancel(ticket);
            }
        }
        self.inner.wake.notify_all();
    }

    /// Drives one poll pass on the calling thread. The worker uses it, and tests
    /// use it to advance jobs deterministically.
    pub fn poll_once(&self) {
        let Some(runtime) = self.runtime() else {
            return;
        };
        let pending = {
            let state = self.locked();
            state
                .inflight
                .iter()
                .map(|record| (record.id, record.ticket.clone(), record.state.clone()))
                .collect::<Vec<_>>()
        };
        for (id, ticket, previous) in pending {
            let progress = match runtime.poll(&ticket) {
                Ok(progress) => progress,
                Err(error) => TaskProgress::Failed(error.to_string()),
            };
            let next = match progress {
                TaskProgress::Pending => JobState::Pending,
                TaskProgress::Running(progress) => JobState::Running { progress },
                TaskProgress::Completed(output) => JobState::Completed { output },
                TaskProgress::Failed(message) => JobState::Failed { message },
                TaskProgress::Cancelled => JobState::Cancelled,
            };
            if next == previous {
                continue;
            }
            if next.is_terminal() {
                self.settle(id, next);
            } else {
                self.advance(id, next);
            }
        }
    }

    fn ensure_worker(&self) {
        {
            let mut state = self.locked();
            if state.worker_started || state.shutdown {
                return;
            }
            state.worker_started = true;
        }
        let weak = Arc::downgrade(&self.inner);
        let _ = std::thread::Builder::new()
            .name("lilia-kernel-jobs".to_owned())
            .spawn(move || worker_loop(weak));
    }

    fn advance(&self, id: JobId, next: JobState) {
        let announcement = {
            let mut state = self.locked();
            state
                .inflight
                .iter_mut()
                .find(|record| record.id == id)
                .map(|record| {
                    record.state = next.clone();
                    (record.protocol.clone(), record.slot.clone())
                })
        };
        if let Some((protocol, slot)) = announcement {
            self.announce(id, &protocol, slot.as_ref(), next);
        }
    }

    fn settle(&self, id: JobId, next: JobState) {
        let announcement = {
            let mut state = self.locked();
            state
                .inflight
                .iter()
                .position(|record| record.id == id)
                .map(|position| {
                    let record = state.inflight.remove(position);
                    state.terminal.insert(id, next.clone());
                    state.terminal_order.push_back(id);
                    while state.terminal_order.len() > TERMINAL_RETENTION {
                        if let Some(evicted) = state.terminal_order.pop_front() {
                            state.terminal.remove(&evicted);
                        }
                    }
                    (record.protocol, record.slot)
                })
        };
        if let Some((protocol, slot)) = announcement {
            self.announce(id, &protocol, slot.as_ref(), next);
        }
    }

    fn announce(&self, id: JobId, protocol: &str, slot: Option<&JobSlot>, state: JobState) {
        self.inner.journal.append(
            RecordKind::Job,
            format!("job.{}", state.topic()),
            Some(id.to_string()),
            serde_json::json!({
                "protocol": protocol,
                "slot": slot.map(JobSlot::as_str),
                "state": state,
            }),
        );
        self.inner.events.publish(JobEvent {
            job_id: id,
            protocol: protocol.to_owned(),
            slot: slot.cloned(),
            state,
        });
    }

    fn find_inflight(&self, protocol: &str, idempotency_key: &str) -> Option<JobHandle> {
        let state = self.locked();
        state
            .inflight
            .iter()
            .find(|record| {
                record.protocol == protocol && record.idempotency_key == idempotency_key
            })
            .map(|record| JobHandle {
                id: record.id,
                protocol: record.protocol.clone(),
                jobs: self.clone(),
            })
    }

    fn slot_occupant(&self, slot: &JobSlot) -> Option<(JobId, TaskTicket)> {
        let state = self.locked();
        state
            .inflight
            .iter()
            .find(|record| record.slot.as_ref() == Some(slot))
            .map(|record| (record.id, record.ticket.clone()))
    }

    fn runtime(&self) -> Option<Arc<dyn TaskRuntime>> {
        self.inner
            .runtime
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn locked(&self) -> MutexGuard<'_, JobsState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Parks while idle and polls in-flight tickets otherwise. The loop holds only a
/// [`Weak`] between iterations, so it exits once the last [`Jobs`] handle drops.
fn worker_loop(weak: Weak<JobsInner>) {
    const IDLE_PARK: Duration = Duration::from_millis(100);

    loop {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        let jobs = Jobs { inner };
        let interval = jobs.inner.poll_interval;
        let state = jobs.locked();
        if state.shutdown {
            return;
        }
        if state.inflight.is_empty() {
            let (guard, _) = jobs
                .inner
                .wake
                .wait_timeout(state, IDLE_PARK)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            drop(guard);
            drop(jobs);
            continue;
        }
        drop(state);
        jobs.poll_once();
        drop(jobs);
        std::thread::sleep(interval);
    }
}

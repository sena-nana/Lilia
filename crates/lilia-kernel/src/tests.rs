use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;

use crate::{
    Contribution, Event, EventBus, Feature, FeatureContext, FeatureId, JobError, JobEvent,
    JobRequest, JobSlot, JobState, Jobs, Journal, Kernel, KernelError, RecordKind, ServiceKey,
    ServiceRef, TaskProgress, TaskRuntime, TaskSpec, TaskTicket,
};

trait Clock: Send + Sync {
    fn now(&self) -> u64;
}

impl ServiceKey for dyn Clock {
    type Value = Arc<dyn Clock>;
    const NAME: &'static str = "test.clock";
}

trait Ledger: Send + Sync {
    fn stamped(&self) -> u64;
}

impl ServiceKey for dyn Ledger {
    type Value = Arc<dyn Ledger>;
    const NAME: &'static str = "test.ledger";
}

struct FixedClock(u64);

impl Clock for FixedClock {
    fn now(&self) -> u64 {
        self.0
    }
}

struct ClockFeature(u64);

impl Feature for ClockFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("test.clock").unwrap()
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<dyn Clock>()]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        cx.provide::<dyn Clock>(Arc::new(FixedClock(self.0)))
    }
}

struct StampedLedger(u64);

impl Ledger for StampedLedger {
    fn stamped(&self) -> u64 {
        self.0
    }
}

/// Depends on `Clock` so the kernel must mount `ClockFeature` first regardless of
/// the order the caller supplies.
struct LedgerFeature {
    mount_order: Arc<AtomicUsize>,
}

impl Feature for LedgerFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("test.ledger").unwrap()
    }

    fn requires(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<dyn Clock>()]
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<dyn Ledger>()]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        self.mount_order.fetch_add(1, Ordering::Relaxed);
        let clock = cx.require::<dyn Clock>()?;
        cx.provide::<dyn Ledger>(Arc::new(StampedLedger(clock.now())))
    }
}

#[test]
fn mount_all_orders_features_by_declared_dependencies() {
    let kernel = Kernel::new();
    let order = Arc::new(AtomicUsize::new(0));

    kernel
        .mount_all([
            Arc::new(LedgerFeature {
                mount_order: Arc::clone(&order),
            }) as Arc<dyn Feature>,
            Arc::new(ClockFeature(1234)) as Arc<dyn Feature>,
        ])
        .expect("dependency order is resolvable");

    assert_eq!(
        kernel.service::<dyn Ledger>().unwrap().stamped(),
        1234,
        "ledger must observe the clock provided by its dependency"
    );
    assert_eq!(order.load(Ordering::Relaxed), 1);
}

#[test]
fn unsatisfied_requirement_fails_before_any_feature_runs() {
    let kernel = Kernel::new();
    let order = Arc::new(AtomicUsize::new(0));

    let error = kernel
        .mount_all([Arc::new(LedgerFeature {
            mount_order: Arc::clone(&order),
        }) as Arc<dyn Feature>])
        .expect_err("ledger cannot mount without a clock");

    assert!(matches!(error, KernelError::UnsatisfiedRequirement { .. }));
    assert_eq!(
        order.load(Ordering::Relaxed),
        0,
        "ordering must fail before feature code executes"
    );
    assert!(kernel.mounted_features().is_empty());
}

#[test]
fn dependency_cycle_is_rejected() {
    struct Cyclic {
        id: &'static str,
        requires: ServiceRef,
        provides: ServiceRef,
    }

    impl Feature for Cyclic {
        fn id(&self) -> FeatureId {
            FeatureId::new(self.id).unwrap()
        }

        fn requires(&self) -> Vec<ServiceRef> {
            vec![self.requires]
        }

        fn provides(&self) -> Vec<ServiceRef> {
            vec![self.provides]
        }

        fn mount(&self, _cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
            Ok(())
        }
    }

    let kernel = Kernel::new();
    let error = kernel
        .mount_all([
            Arc::new(Cyclic {
                id: "test.a",
                requires: ServiceRef::of::<dyn Ledger>(),
                provides: ServiceRef::of::<dyn Clock>(),
            }) as Arc<dyn Feature>,
            Arc::new(Cyclic {
                id: "test.b",
                requires: ServiceRef::of::<dyn Clock>(),
                provides: ServiceRef::of::<dyn Ledger>(),
            }) as Arc<dyn Feature>,
        ])
        .expect_err("a cycle has no valid mount order");

    assert!(matches!(error, KernelError::DependencyCycle(_)));
}

#[derive(Clone, Debug, PartialEq)]
struct Renamed {
    name: String,
}

impl Event for Renamed {
    const NAME: &'static str = "test.renamed";

    fn subject(&self) -> Option<String> {
        Some(self.name.clone())
    }

    fn detail(&self) -> serde_json::Value {
        json!({ "name": self.name })
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Removed;

impl Event for Removed {
    const NAME: &'static str = "test.removed";
}

struct Commands;

impl Contribution for Commands {
    type Item = &'static str;
    const NAME: &'static str = "test.commands";
}

struct ObservingFeature {
    seen: Arc<Mutex<Vec<String>>>,
}

impl Feature for ObservingFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("test.observer").unwrap()
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<dyn Clock>()]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        let seen = Arc::clone(&self.seen);
        cx.on::<Renamed, _>(move |event| {
            seen.lock().unwrap().push(event.name.clone());
        });
        cx.contribute::<Commands>("test.command");
        cx.provide::<dyn Clock>(Arc::new(FixedClock(7)))
    }
}

#[test]
fn unmount_reverses_services_subscriptions_and_contributions() {
    let kernel = Kernel::new();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let feature = Arc::new(ObservingFeature {
        seen: Arc::clone(&seen),
    });
    let id = feature.id();

    kernel.mount(feature as Arc<dyn Feature>).unwrap();
    kernel.events().publish(Renamed {
        name: "before".to_owned(),
    });
    assert_eq!(seen.lock().unwrap().as_slice(), ["before"]);
    assert!(kernel.has_service(ServiceRef::of::<dyn Clock>()));
    assert_eq!(kernel.contributions().count::<Commands>(), 1);

    kernel.unmount(&id).unwrap();

    kernel.events().publish(Renamed {
        name: "after".to_owned(),
    });
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        ["before"],
        "handlers must not run after their feature unmounts"
    );
    assert!(!kernel.has_service(ServiceRef::of::<dyn Clock>()));
    assert_eq!(kernel.contributions().count::<Commands>(), 0);
    assert!(kernel.mounted_features().is_empty());
}

#[test]
fn typed_topics_isolate_consumers_and_share_one_sequence() {
    let bus = EventBus::new();
    let renamed = Arc::new(AtomicUsize::new(0));
    let removed = Arc::new(AtomicUsize::new(0));

    let renamed_counter = Arc::clone(&renamed);
    bus.on::<Renamed, _>(None, move |_| {
        renamed_counter.fetch_add(1, Ordering::Relaxed);
    });
    let removed_counter = Arc::clone(&removed);
    bus.on::<Removed, _>(None, move |_| {
        removed_counter.fetch_add(1, Ordering::Relaxed);
    });

    let first = bus.publish(Renamed {
        name: "a".to_owned(),
    });
    let second = bus.publish(Removed);

    assert_eq!(renamed.load(Ordering::Relaxed), 1);
    assert_eq!(removed.load(Ordering::Relaxed), 1);
    assert_eq!(second.sequence(), first.sequence() + 1);
}

#[test]
fn observers_receive_every_topic_and_can_downcast() {
    let bus = EventBus::new();
    let names = Arc::new(Mutex::new(Vec::new()));
    let collected = Arc::clone(&names);
    bus.observe(None, move |envelope| {
        let renamed = envelope
            .downcast::<Renamed>()
            .map(|event| event.name.clone());
        collected
            .lock()
            .unwrap()
            .push((envelope.name(), renamed, envelope.is::<Removed>()));
    });

    bus.publish(Renamed {
        name: "one".to_owned(),
    });
    bus.publish(Removed);

    let names = names.lock().unwrap();
    assert_eq!(names.len(), 2);
    assert_eq!(names[0].1.as_deref(), Some("one"));
    assert!(names[1].2);
}

#[test]
fn a_handler_may_publish_without_deadlocking() {
    let bus = EventBus::new();
    let republished = Arc::new(AtomicUsize::new(0));

    let inner = bus.clone();
    bus.on::<Renamed, _>(None, move |_| {
        inner.publish(Removed);
    });
    let counter = Arc::clone(&republished);
    bus.on::<Removed, _>(None, move |_| {
        counter.fetch_add(1, Ordering::Relaxed);
    });

    bus.publish(Renamed {
        name: "chain".to_owned(),
    });

    assert_eq!(republished.load(Ordering::Relaxed), 1);
}

#[test]
fn journal_sequences_records_and_evicts_the_oldest_beyond_capacity() {
    let journal = Journal::with_capacity(2);

    journal.append(RecordKind::Mutation, "first", None, json!({}));
    journal.append(RecordKind::Mutation, "second", None, json!({}));
    journal.append(RecordKind::Mutation, "third", None, json!({}));

    assert_eq!(journal.sequence(), 3);
    assert_eq!(journal.earliest_sequence(), Some(2));
    let tail = journal.records_after(1, 10);
    assert_eq!(
        tail.iter().map(|record| record.topic.as_str()).collect::<Vec<_>>(),
        ["second", "third"]
    );
}

#[test]
fn journal_sink_observes_every_appended_record() {
    struct CountingSink(Arc<AtomicUsize>);

    impl crate::JournalSink for CountingSink {
        fn write(&self, _record: &crate::JournalRecord) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    let journal = Journal::new();
    let writes = Arc::new(AtomicUsize::new(0));
    journal.set_sink(Arc::new(CountingSink(Arc::clone(&writes))));

    journal.append(RecordKind::Event, "topic", Some("subject".into()), json!({}));

    assert_eq!(writes.load(Ordering::Relaxed), 1);
}

/// The journal is the only place a reader can reconstruct why a fact changed, so
/// a topic that fans out without leaving a record breaks post-mortem causality.
#[test]
fn a_published_event_is_recorded_with_its_subject_and_detail() {
    let journal = Journal::new();
    let bus = EventBus::with_journal(journal.clone());

    bus.publish(Renamed {
        name: "alpha".to_owned(),
    });
    bus.publish(Removed);

    let records = journal.records_after(0, 10);
    assert_eq!(
        records
            .iter()
            .map(|record| (record.kind, record.topic.as_str()))
            .collect::<Vec<_>>(),
        [
            (RecordKind::Event, "test.renamed"),
            (RecordKind::Event, "test.removed")
        ]
    );
    assert_eq!(records[0].subject.as_deref(), Some("alpha"));
    assert_eq!(records[0].payload, json!({ "name": "alpha" }));
    assert_eq!(records[1].subject, None);
}

/// `Jobs` writes its own record before publishing, so a second record per
/// transition would double every job line in the log.
#[test]
fn a_job_transition_leaves_exactly_one_record() {
    let journal = Journal::new();
    let jobs = Jobs::with_poll_interval(
        EventBus::with_journal(journal.clone()),
        journal.clone(),
        Duration::from_millis(2),
    );
    jobs.install_runtime(Arc::new(FakeTaskRuntime::default()));

    jobs.submit(JobRequest {
        protocol: "test.protocol".to_owned(),
        payload: json!({}),
        slot: None,
        idempotency_key: Some("once".to_owned()),
    })
    .expect("the job is accepted");

    let records = journal.records_after(0, 10);
    assert_eq!(
        records
            .iter()
            .map(|record| (record.kind, record.topic.as_str()))
            .collect::<Vec<_>>(),
        [(RecordKind::Job, "job.pending")]
    );
}

#[derive(Default)]
struct FakeTaskRuntime {
    state: Mutex<FakeState>,
}

#[derive(Default)]
struct FakeState {
    next: u64,
    progress: HashMap<String, TaskProgress>,
    submitted: Vec<TaskSpec>,
    cancelled: Vec<String>,
}

impl FakeTaskRuntime {
    fn settle(&self, ticket: &TaskTicket, progress: TaskProgress) {
        self.state
            .lock()
            .unwrap()
            .progress
            .insert(ticket.as_str().to_owned(), progress);
    }

    fn cancelled(&self) -> Vec<String> {
        self.state.lock().unwrap().cancelled.clone()
    }

    fn submitted_keys(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .submitted
            .iter()
            .map(|spec| spec.idempotency_key.clone())
            .collect()
    }
}

impl TaskRuntime for FakeTaskRuntime {
    fn submit(&self, spec: &TaskSpec) -> Result<TaskTicket, JobError> {
        let mut state = self.state.lock().unwrap();
        state.next += 1;
        let ticket = TaskTicket::new(format!("ticket-{}", state.next));
        state
            .progress
            .insert(ticket.as_str().to_owned(), TaskProgress::Pending);
        state.submitted.push(spec.clone());
        Ok(ticket)
    }

    fn cancel(&self, ticket: &TaskTicket) -> Result<(), JobError> {
        let mut state = self.state.lock().unwrap();
        state.cancelled.push(ticket.as_str().to_owned());
        state
            .progress
            .insert(ticket.as_str().to_owned(), TaskProgress::Cancelled);
        Ok(())
    }

    fn poll(&self, ticket: &TaskTicket) -> Result<TaskProgress, JobError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .progress
            .get(ticket.as_str())
            .cloned()
            .unwrap_or(TaskProgress::Pending))
    }
}

fn job_harness() -> (Jobs, Arc<FakeTaskRuntime>, Arc<Mutex<Vec<JobEvent>>>) {
    let events = EventBus::new();
    let jobs = Jobs::with_poll_interval(
        events.clone(),
        Journal::new(),
        Duration::from_millis(2),
    );
    let runtime = Arc::new(FakeTaskRuntime::default());
    jobs.install_runtime(Arc::clone(&runtime) as Arc<dyn TaskRuntime>);

    let observed = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&observed);
    events.on::<JobEvent, _>(None, move |event| {
        sink.lock().unwrap().push(event.clone());
    });
    (jobs, runtime, observed)
}

fn wait_for(mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

/// A job's recorded state becomes visible before its journal entry and its
/// event, so a test that asserts on either must wait on that observable rather
/// than on [`JobHandle::state`].
fn wait_for_event(
    observed: &Arc<Mutex<Vec<JobEvent>>>,
    mut predicate: impl FnMut(&JobEvent) -> bool,
) -> JobEvent {
    let mut found = None;
    let matched = wait_for(|| {
        found = observed
            .lock()
            .unwrap()
            .iter()
            .find(|event| predicate(event))
            .cloned();
        found.is_some()
    });
    assert!(matched, "the expected job event was never published");
    found.expect("the matched event is present")
}

#[test]
fn submitting_without_a_runtime_is_refused() {
    let jobs = Jobs::new(EventBus::new(), Journal::new());
    let error = jobs
        .submit(JobRequest::new("lilia.test/noop@1", json!({})))
        .expect_err("no runtime is installed");
    assert!(matches!(error, JobError::RuntimeUnavailable));
}

#[test]
fn a_completed_task_publishes_its_output_and_clears_the_inflight_set() {
    let (jobs, runtime, observed) = job_harness();

    let handle = jobs
        .submit(JobRequest::new("lilia.test/search@1", json!({ "q": "x" })))
        .unwrap();
    let ticket = TaskTicket::new("ticket-1");
    runtime.settle(&ticket, TaskProgress::Completed(json!({ "hits": 2 })));

    let terminal = wait_for_event(&observed, |event| {
        matches!(event.state, JobState::Completed { .. })
    });
    assert_eq!(terminal.job_id, handle.id());
    assert!(matches!(
        &terminal.state,
        JobState::Completed { output } if output["hits"] == 2
    ));
    assert_eq!(jobs.inflight_count(), 0);
}

#[test]
fn an_idempotency_key_collapses_concurrent_submissions() {
    let (jobs, runtime, _observed) = job_harness();

    let first = jobs
        .submit(JobRequest::new("lilia.test/clone@1", json!({})).idempotent("repo-a"))
        .unwrap();
    let second = jobs
        .submit(JobRequest::new("lilia.test/clone@1", json!({})).idempotent("repo-a"))
        .unwrap();

    assert_eq!(first.id(), second.id());
    assert_eq!(runtime.submitted_keys(), ["repo-a"]);
    assert_eq!(jobs.inflight_count(), 1);
}

#[test]
fn a_slot_supersedes_its_previous_job_so_stale_results_are_discarded() {
    let (jobs, runtime, observed) = job_harness();
    let slot = JobSlot::new("composer.suggestions").unwrap();

    let stale = jobs
        .submit(
            JobRequest::new("lilia.test/suggest@1", json!({ "n": 1 }))
                .idempotent("n1")
                .in_slot(slot.clone()),
        )
        .unwrap();
    let fresh = jobs
        .submit(
            JobRequest::new("lilia.test/suggest@1", json!({ "n": 2 }))
                .idempotent("n2")
                .in_slot(slot),
        )
        .unwrap();

    assert_ne!(stale.id(), fresh.id());
    assert!(matches!(stale.state(), Some(JobState::Superseded)));
    assert_eq!(runtime.cancelled(), ["ticket-1"]);
    wait_for_event(&observed, |event| {
        event.job_id == stale.id() && event.state == JobState::Superseded
    });
    assert!(wait_for(|| jobs.inflight_count() == 1));
}

#[test]
fn cancelling_a_job_cancels_its_task_and_settles_it() {
    let (jobs, runtime, _observed) = job_harness();

    let handle = jobs
        .submit(JobRequest::new("lilia.test/index@1", json!({})))
        .unwrap();
    handle.cancel();

    assert!(matches!(handle.state(), Some(JobState::Cancelled)));
    assert_eq!(runtime.cancelled(), ["ticket-1"]);
    assert_eq!(jobs.inflight_count(), 0);
}

#[test]
fn a_failing_task_surfaces_its_message() {
    let (jobs, runtime, _observed) = job_harness();

    let handle = jobs
        .submit(JobRequest::new("lilia.test/probe@1", json!({})))
        .unwrap();
    runtime.settle(
        &TaskTicket::new("ticket-1"),
        TaskProgress::Failed("endpoint unreachable".to_owned()),
    );

    assert!(wait_for(|| matches!(handle.state(), Some(JobState::Failed { .. }))));
    assert!(matches!(
        handle.state(),
        Some(JobState::Failed { message }) if message.contains("unreachable")
    ));
}

#[test]
fn job_transitions_are_journalled_in_order() {
    let events = EventBus::new();
    let journal = Journal::new();
    let jobs = Jobs::with_poll_interval(events, journal.clone(), Duration::from_millis(2));
    let runtime = Arc::new(FakeTaskRuntime::default());
    jobs.install_runtime(Arc::clone(&runtime) as Arc<dyn TaskRuntime>);

    let handle = jobs
        .submit(JobRequest::new("lilia.test/build@1", json!({})))
        .unwrap();
    runtime.settle(
        &TaskTicket::new("ticket-1"),
        TaskProgress::Running(serde_json::json!({ "percent": 40 })),
    );
    assert!(wait_for(|| matches!(
        handle.state(),
        Some(JobState::Running { progress }) if progress == serde_json::json!({ "percent": 40 })
    )));
    runtime.settle(&TaskTicket::new("ticket-1"), TaskProgress::Completed(json!(1)));

    let job_topics = || {
        journal
            .records_after(0, 100)
            .into_iter()
            .filter(|record| record.kind == RecordKind::Job)
            .map(|record| record.topic)
            .collect::<Vec<_>>()
    };
    assert!(wait_for(|| job_topics().len() == 3));
    assert_eq!(job_topics(), ["job.pending", "job.running", "job.completed"]);
}

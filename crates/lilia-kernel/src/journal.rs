use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_CAPACITY: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    /// A durable fact changed.
    Mutation,
    /// A job reached a new state.
    Job,
    /// A topic was published.
    Event,
    /// A feature was mounted or unmounted.
    Lifecycle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalRecord {
    pub sequence: u64,
    pub kind: RecordKind,
    pub topic: String,
    pub subject: Option<String>,
    pub payload: Value,
}

/// Durability port for the journal. The in-memory ring stays authoritative for
/// the running process; a sink lets a host persist records for post-mortem
/// replay without the kernel knowing about storage.
pub trait JournalSink: Send + Sync + 'static {
    fn write(&self, record: &JournalRecord);
}

/// Append-only, monotonically sequenced record of everything the kernel did.
///
/// One ordered log is what makes replay, agent-debug capture and cross-feature
/// causality tractable: a reader reconstructs why a fact changed without asking
/// each feature for its private state.
#[derive(Clone)]
pub struct Journal {
    inner: Arc<JournalInner>,
}

struct JournalInner {
    state: Mutex<JournalState>,
    sink: Mutex<Option<Arc<dyn JournalSink>>>,
}

struct JournalState {
    next_sequence: u64,
    capacity: usize,
    records: VecDeque<JournalRecord>,
}

impl Default for Journal {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}

impl Journal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(JournalInner {
                state: Mutex::new(JournalState {
                    next_sequence: 1,
                    capacity: capacity.max(1),
                    records: VecDeque::new(),
                }),
                sink: Mutex::new(None),
            }),
        }
    }

    pub fn set_sink(&self, sink: Arc<dyn JournalSink>) {
        *self
            .inner
            .sink
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sink);
    }

    pub fn append(
        &self,
        kind: RecordKind,
        topic: impl Into<String>,
        subject: Option<String>,
        payload: Value,
    ) -> JournalRecord {
        let record = {
            let mut state = self.state();
            let record = JournalRecord {
                sequence: state.next_sequence,
                kind,
                topic: topic.into(),
                subject,
                payload,
            };
            state.next_sequence = state.next_sequence.saturating_add(1);
            state.records.push_back(record.clone());
            while state.records.len() > state.capacity {
                state.records.pop_front();
            }
            record
        };
        let sink = self
            .inner
            .sink
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(sink) = sink {
            sink.write(&record);
        }
        record
    }

    /// Records with a sequence strictly greater than `after`, oldest first.
    pub fn records_after(&self, after: u64, limit: usize) -> Vec<JournalRecord> {
        self.state()
            .records
            .iter()
            .filter(|record| record.sequence > after)
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn sequence(&self) -> u64 {
        self.state().next_sequence.saturating_sub(1)
    }

    /// Sequence of the oldest retained record, or `None` when empty. A reader
    /// whose cursor is below this value has missed records.
    pub fn earliest_sequence(&self) -> Option<u64> {
        self.state().records.front().map(|record| record.sequence)
    }

    fn state(&self) -> MutexGuard<'_, JournalState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

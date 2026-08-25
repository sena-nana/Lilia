use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use lilia_contracts::{ProjectId, TaskId};
use serde::{Deserialize, Serialize};

use lilia_feature_automation::AutomationRunStatus;

const DEFAULT_SUBSCRIBER_CAPACITY: usize = 256;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEvent {
    pub sequence: u64,
    pub source_instance: String,
    pub kind: DesktopEventKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DesktopEventKind {
    ProjectsChanged,
    TasksChanged {
        project_id: Option<ProjectId>,
        task_id: Option<TaskId>,
    },
    TimelineChanged {
        task_id: TaskId,
        cursor: Option<u64>,
    },
    ComposerChanged {
        task_id: TaskId,
        revision: u64,
    },
    TodosChanged {
        task_id: TaskId,
    },
    GoalChanged {
        task_id: TaskId,
    },
    WorktreeChanged {
        task_id: TaskId,
    },
    WorktreeOperationFailed {
        task_id: TaskId,
        message: String,
    },
    WorktreeOperationCompleted {
        task_id: TaskId,
    },
    ProviderChanged {
        provider_id: Option<String>,
        revision: u64,
    },
    CredentialChanged {
        provider_id: String,
        credential_id: String,
        revision: u64,
    },
    GitHubBindingChanged {
        login: Option<String>,
    },
    AgentInteractionChanged {
        revision: u64,
    },
    TurnStateChanged {
        task_id: TaskId,
        turn_id: String,
        state: DesktopTurnState,
    },
    TurnRecoveryIssue {
        task_id: Option<TaskId>,
        turn_id: String,
        reason: String,
    },
    ApprovalChanged {
        task_id: TaskId,
        request_id: String,
        state: DesktopApprovalState,
    },
    InteractionChanged {
        task_id: TaskId,
        request_id: String,
        state: DesktopInteractionState,
    },
    AutomationChanged {
        automation_id: Option<String>,
    },
    AutomationRunChanged {
        automation_id: String,
        run_id: String,
        status: AutomationRunStatus,
    },
    MemoryChanged {
        memory_id: Option<String>,
        project_id: Option<ProjectId>,
    },
    MemorySettingsChanged,
    MemoryInjectionChanged {
        task_id: TaskId,
    },
    RoadmapChanged {
        project_id: ProjectId,
        milestone_id: Option<String>,
    },
    ArchitectureChanged {
        project_id: ProjectId,
        version: i64,
    },
    ProjectFilesChanged {
        project_id: ProjectId,
    },
    TerminalChanged {
        session_id: crate::application::DesktopTerminalSessionId,
        revision: u64,
    },
    ProjectSettingsChanged,
    ConversationSuggestionSettingsChanged,
    ConversationSuggestionsChanged {
        project_id: Option<String>,
    },
    PopupWindowSettingsChanged,
    ModelFeatureSettingsChanged {
        revision: u64,
    },
    AssistantAiSettingsChanged {
        revision: u64,
    },
    RouterModeSettingsChanged {
        revision: u64,
    },
    HooksRegistryChanged,
    SkillsRegistryChanged,
    McpRegistryChanged,
    PluginsRegistryChanged,
    NavigationRequested {
        target: DesktopNavigationTarget,
    },
    UpdateStateChanged {
        state: DesktopUpdateState,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DesktopTurnState {
    Queued {
        position: usize,
    },
    Starting,
    Running,
    WaitingApproval {
        request_id: Option<String>,
        error: Option<String>,
    },
    ResolvingApproval,
    WaitingInteraction {
        request_id: Option<String>,
        kind: Option<String>,
        error: Option<String>,
    },
    ResolvingInteraction,
    Completed,
    Cancelled,
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum DesktopNavigationTarget {
    Project(ProjectId),
    Task(TaskId),
    Automations,
    Settings,
    Route(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopApprovalState {
    Requested,
    Approved,
    Denied,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopInteractionState {
    Accepted,
    Declined,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DesktopUpdateState {
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        notes: Option<String>,
    },
    Downloading {
        version: String,
        progress: Option<f32>,
    },
    Installing {
        version: String,
    },
    Restarting {
        version: String,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone)]
pub struct DesktopEventBus {
    inner: Arc<DesktopEventBusInner>,
}

struct DesktopEventBusInner {
    state: Mutex<DesktopEventBusState>,
    dropped_events: AtomicU64,
}

struct DesktopEventBusState {
    next_sequence: u64,
    subscriber_capacity: usize,
    subscribers: Vec<mpsc::SyncSender<DesktopEvent>>,
}

impl Default for DesktopEventBus {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_SUBSCRIBER_CAPACITY)
    }
}

impl DesktopEventBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a bus with a bounded queue for every subscriber.
    ///
    /// A zero capacity is clamped to one so publishing can never become a
    /// rendezvous operation.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(DesktopEventBusInner {
                state: Mutex::new(DesktopEventBusState {
                    next_sequence: 1,
                    subscriber_capacity: capacity.max(1),
                    subscribers: Vec::new(),
                }),
                dropped_events: AtomicU64::new(0),
            }),
        }
    }

    pub fn subscribe(&self) -> DesktopEventSubscription {
        let mut state = self.state();
        let (sender, receiver) = mpsc::sync_channel(state.subscriber_capacity);
        state.subscribers.push(sender);
        DesktopEventSubscription { receiver }
    }

    /// Returns the number of subscriber deliveries dropped because their
    /// bounded queues were full.
    pub fn dropped_events(&self) -> u64 {
        self.inner.dropped_events.load(Ordering::Relaxed)
    }

    pub fn publish(
        &self,
        source_instance: impl Into<String>,
        kind: DesktopEventKind,
    ) -> DesktopEvent {
        let mut state = self.state();
        let event = DesktopEvent {
            sequence: state.next_sequence,
            source_instance: source_instance.into(),
            kind,
        };
        state.next_sequence = state.next_sequence.saturating_add(1);
        let dropped_events = &self.inner.dropped_events;
        state
            .subscribers
            .retain(|subscriber| match subscriber.try_send(event.clone()) {
                Ok(()) => true,
                Err(mpsc::TrySendError::Full(_)) => {
                    let _ = dropped_events.fetch_update(
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                        |current| Some(current.saturating_add(1)),
                    );
                    true
                }
                Err(mpsc::TrySendError::Disconnected(_)) => false,
            });
        event
    }

    fn state(&self) -> std::sync::MutexGuard<'_, DesktopEventBusState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub struct DesktopEventSubscription {
    receiver: mpsc::Receiver<DesktopEvent>,
}

impl DesktopEventSubscription {
    pub fn recv(&self) -> Result<DesktopEvent, mpsc::RecvError> {
        self.receiver.recv()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<DesktopEvent, mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    pub fn try_recv(&self) -> Result<DesktopEvent, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_live_subscriber_receives_the_same_ordered_events() {
        let bus = DesktopEventBus::new();
        let first = bus.subscribe();
        let second = bus.subscribe();

        let published = bus.publish("lilia", DesktopEventKind::ProjectsChanged);
        assert_eq!(published.sequence, 1);
        assert_eq!(first.recv().unwrap(), published);
        assert_eq!(second.recv().unwrap(), published);

        drop(first);
        let next = bus.publish(
            "lilia",
            DesktopEventKind::NavigationRequested {
                target: DesktopNavigationTarget::Settings,
            },
        );
        assert_eq!(next.sequence, 2);
        assert_eq!(second.recv().unwrap(), next);
    }

    #[test]
    fn full_subscriber_queue_drops_without_blocking_and_counts_delivery() {
        let bus = DesktopEventBus::with_capacity(1);
        let slow = bus.subscribe();

        bus.publish("lilia", DesktopEventKind::ProjectsChanged);
        let publisher = bus.clone();
        let (completed_tx, completed_rx) = std::sync::mpsc::channel();
        let publisher_thread = std::thread::spawn(move || {
            let dropped = publisher.publish("lilia", DesktopEventKind::ProjectsChanged);
            completed_tx.send(dropped).unwrap();
        });
        let dropped = match completed_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(dropped) => dropped,
            Err(error) => {
                drop(slow);
                publisher_thread.join().unwrap();
                panic!("publish blocked on a full subscriber queue: {error}");
            }
        };
        publisher_thread.join().unwrap();

        assert_eq!(dropped.sequence, 2);
        assert_eq!(bus.dropped_events(), 1);
    }

    #[test]
    fn subscriber_receives_later_events_after_consuming_from_a_full_queue() {
        let bus = DesktopEventBus::with_capacity(1);
        let subscription = bus.subscribe();

        let first = bus.publish("lilia", DesktopEventKind::ProjectsChanged);
        let dropped = bus.publish("lilia", DesktopEventKind::ProjectsChanged);
        assert_eq!(subscription.recv().unwrap(), first);

        let resumed = bus.publish("lilia", DesktopEventKind::ProjectsChanged);
        assert_eq!(resumed.sequence, dropped.sequence + 1);
        assert_eq!(subscription.recv().unwrap(), resumed);
        assert_eq!(bus.dropped_events(), 1);
    }

    #[test]
    fn disconnected_subscriber_is_removed_on_the_next_publish() {
        let bus = DesktopEventBus::with_capacity(1);
        let subscription = bus.subscribe();
        assert_eq!(bus.state().subscribers.len(), 1);

        drop(subscription);
        bus.publish("lilia", DesktopEventKind::ProjectsChanged);

        assert!(bus.state().subscribers.is_empty());
        assert_eq!(bus.dropped_events(), 0);
    }

    #[test]
    fn zero_capacity_is_clamped_to_a_nonblocking_queue() {
        let bus = DesktopEventBus::with_capacity(0);
        let subscription = bus.subscribe();

        let published = bus.publish("lilia", DesktopEventKind::ProjectsChanged);

        assert_eq!(subscription.try_recv().unwrap(), published);
    }
}

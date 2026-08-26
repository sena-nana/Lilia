use std::sync::mpsc;
use std::time::Duration;

use lilia_contracts::{ProjectId, TaskId};
use lilia_kernel::{Event, EventBus, EventEnvelope, SubscriptionId};
use serde::{Deserialize, Serialize};

pub use lilia_feature_agent_session::{GoalChanged, TodosChanged};
pub use lilia_feature_architecture::ArchitectureChanged;
pub use lilia_feature_automation::{AutomationChanged, AutomationRunChanged};
pub use lilia_feature_composer::ComposerChanged;
pub use lilia_feature_extensions::{
    HooksRegistryChanged, McpRegistryChanged, PluginsRegistryChanged, SkillsRegistryChanged,
};
pub use lilia_feature_memory::{MemoryChanged, MemoryInjectionChanged, MemorySettingsChanged};
pub use lilia_feature_roadmap::RoadmapChanged;
pub use lilia_feature_task::{ProjectsChanged, TasksChanged};
pub use lilia_feature_terminal::TerminalChanged;
pub use lilia_feature_timeline::TimelineChanged;

const DEFAULT_SUBSCRIBER_CAPACITY: usize = 256;

/// Host delivery of one typed kernel event. Sequence comes from the bus.
#[derive(Clone, Debug)]
pub struct DesktopEvent {
    envelope: EventEnvelope,
}

impl DesktopEvent {
    pub fn from_envelope(envelope: EventEnvelope) -> Self {
        Self { envelope }
    }

    pub fn sequence(&self) -> u64 {
        self.envelope.sequence()
    }

    pub fn name(&self) -> &'static str {
        self.envelope.name()
    }

    pub fn is<E: Event>(&self) -> bool {
        self.envelope.is::<E>()
    }

    pub fn downcast<E: Event>(&self) -> Option<&E> {
        self.envelope.downcast()
    }

    pub fn envelope(&self) -> &EventEnvelope {
        &self.envelope
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeChanged {
    pub task_id: TaskId,
}

impl Event for WorktreeChanged {
    const NAME: &'static str = "lilia.worktree.changed";

    fn subject(&self) -> Option<String> {
        Some(self.task_id.as_str().to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeOperationCompleted {
    pub task_id: TaskId,
}

impl Event for WorktreeOperationCompleted {
    const NAME: &'static str = "lilia.worktree.completed";

    fn subject(&self) -> Option<String> {
        Some(self.task_id.as_str().to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeOperationFailed {
    pub task_id: TaskId,
    pub message: String,
}

impl Event for WorktreeOperationFailed {
    const NAME: &'static str = "lilia.worktree.failed";

    fn subject(&self) -> Option<String> {
        Some(self.task_id.as_str().to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderChanged {
    pub provider_id: Option<String>,
    pub revision: u64,
}

impl Event for ProviderChanged {
    const NAME: &'static str = "lilia.provider.changed";
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialChanged {
    pub provider_id: String,
    pub credential_id: String,
    pub revision: u64,
}

impl Event for CredentialChanged {
    const NAME: &'static str = "lilia.provider.credential_changed";
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubBindingChanged {
    pub login: Option<String>,
}

impl Event for GitHubBindingChanged {
    const NAME: &'static str = "lilia.github.binding_changed";
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentInteractionChanged {
    pub revision: u64,
}

impl Event for AgentInteractionChanged {
    const NAME: &'static str = "lilia.agent.interaction_changed";
}

#[derive(Clone, Debug, PartialEq)]
pub struct TurnStateChanged {
    pub task_id: TaskId,
    pub turn_id: String,
    pub state: DesktopTurnState,
}

impl Event for TurnStateChanged {
    const NAME: &'static str = "lilia.agent.turn_state_changed";

    fn subject(&self) -> Option<String> {
        Some(self.task_id.as_str().to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnRecoveryIssue {
    pub task_id: Option<TaskId>,
    pub turn_id: String,
    pub reason: String,
}

impl Event for TurnRecoveryIssue {
    const NAME: &'static str = "lilia.agent.turn_recovery_issue";
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalChanged {
    pub task_id: TaskId,
    pub request_id: String,
    pub state: DesktopApprovalState,
}

impl Event for ApprovalChanged {
    const NAME: &'static str = "lilia.agent.approval_changed";

    fn subject(&self) -> Option<String> {
        Some(self.task_id.as_str().to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionChanged {
    pub task_id: TaskId,
    pub request_id: String,
    pub state: DesktopInteractionState,
}

impl Event for InteractionChanged {
    const NAME: &'static str = "lilia.agent.interaction_changed";

    fn subject(&self) -> Option<String> {
        Some(self.task_id.as_str().to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectFilesChanged {
    pub project_id: ProjectId,
}

impl Event for ProjectFilesChanged {
    const NAME: &'static str = "lilia.project.files_changed";

    fn subject(&self) -> Option<String> {
        Some(self.project_id.as_str().to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSettingsChanged;

impl Event for ProjectSettingsChanged {
    const NAME: &'static str = "lilia.project.settings_changed";
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationSuggestionSettingsChanged;

impl Event for ConversationSuggestionSettingsChanged {
    const NAME: &'static str = "lilia.suggestions.settings_changed";
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationSuggestionsChanged {
    pub project_id: Option<String>,
}

impl Event for ConversationSuggestionsChanged {
    const NAME: &'static str = "lilia.suggestions.changed";
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PopupWindowSettingsChanged;

impl Event for PopupWindowSettingsChanged {
    const NAME: &'static str = "lilia.shell.popup_settings_changed";
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelFeatureSettingsChanged {
    pub revision: u64,
}

impl Event for ModelFeatureSettingsChanged {
    const NAME: &'static str = "lilia.provider.model_feature_settings_changed";
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssistantAiSettingsChanged {
    pub revision: u64,
}

impl Event for AssistantAiSettingsChanged {
    const NAME: &'static str = "lilia.provider.assistant_ai_settings_changed";
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouterModeSettingsChanged {
    pub revision: u64,
}

impl Event for RouterModeSettingsChanged {
    const NAME: &'static str = "lilia.provider.router_mode_settings_changed";
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationRequested {
    pub target: DesktopNavigationTarget,
}

impl Event for NavigationRequested {
    const NAME: &'static str = "lilia.shell.navigation_requested";
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpdateStateChanged {
    pub state: DesktopUpdateState,
}

impl Event for UpdateStateChanged {
    const NAME: &'static str = "lilia.update.state_changed";
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

/// Host wrapper around the kernel bus so tests can still subscribe with a channel.
#[derive(Clone)]
pub struct DesktopEventBus {
    inner: EventBus,
}

impl Default for DesktopEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopEventBus {
    pub fn new() -> Self {
        Self {
            inner: EventBus::new(),
        }
    }

    pub fn from_bus(inner: EventBus) -> Self {
        Self { inner }
    }

    pub fn bus(&self) -> &EventBus {
        &self.inner
    }

    pub fn subscribe(&self) -> DesktopEventSubscription {
        let (sender, receiver) = mpsc::sync_channel(DEFAULT_SUBSCRIBER_CAPACITY);
        let id = self.inner.observe(None, move |envelope| {
            let _ = sender.try_send(envelope.clone());
        });
        DesktopEventSubscription {
            receiver,
            bus: self.inner.clone(),
            id,
        }
    }

    pub fn publish<E: Event>(&self, event: E) -> DesktopEvent {
        DesktopEvent::from_envelope(self.inner.publish(event))
    }
}

pub struct DesktopEventSubscription {
    receiver: mpsc::Receiver<EventEnvelope>,
    bus: EventBus,
    id: SubscriptionId,
}

impl DesktopEventSubscription {
    pub fn recv(&self) -> Result<DesktopEvent, mpsc::RecvError> {
        self.receiver.recv().map(DesktopEvent::from_envelope)
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<DesktopEvent, mpsc::RecvTimeoutError> {
        self.receiver
            .recv_timeout(timeout)
            .map(DesktopEvent::from_envelope)
    }

    pub fn try_recv(&self) -> Result<DesktopEvent, mpsc::TryRecvError> {
        self.receiver.try_recv().map(DesktopEvent::from_envelope)
    }
}

impl Drop for DesktopEventSubscription {
    fn drop(&mut self) {
        self.bus.unsubscribe(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn every_live_subscriber_receives_the_same_ordered_events() {
        let bus = DesktopEventBus::new();
        let first = bus.subscribe();
        let second = bus.subscribe();

        let published = bus.publish(ProjectsChanged);
        assert_eq!(published.sequence(), 1);
        let received = first.recv().unwrap();
        assert!(received.is::<ProjectsChanged>());
        assert_eq!(received.sequence(), published.sequence());
        assert!(second.recv().unwrap().is::<ProjectsChanged>());
    }

    #[test]
    fn a_typed_topic_does_not_wake_an_unrelated_consumer() {
        let bus = DesktopEventBus::new();
        let woken = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = Arc::clone(&woken);
        let _id = bus.bus().on::<TimelineChanged, _>(None, move |_| {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        });

        bus.publish(ProjectsChanged);
        assert!(!woken.load(std::sync::atomic::Ordering::Relaxed));
    }
}

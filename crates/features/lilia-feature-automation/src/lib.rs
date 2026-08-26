//! Automation domain feature.
//!
//! Owns automation workflows, their published versions and the runs executed
//! against them. The ports a run needs (tasks, guides, agent turns, timeline)
//! are traits the host implements.

mod contract;
mod execution;
mod graph;
mod service;
mod sqlite;
mod store;
mod template;
mod types;

pub use execution::{
    AutomationAddTodoRequest, AutomationAgentActivation, AutomationAgentDispatch,
    AutomationAgentPort, AutomationAgentTarget, AutomationCompleteAgentInput,
    AutomationCreateTaskRequest, AutomationExecutionEngine, AutomationExecutionError,
    AutomationExecutionPorts, AutomationExecutionRepository, AutomationExecutionResult,
    AutomationGuidePort, AutomationIdempotencyKey, AutomationPortContext, AutomationPortError,
    AutomationRecordTimelineRequest, AutomationSendGuideRequest, AutomationStartAgentRequest,
    AutomationTaskPort, AutomationTimelinePort, AutomationTodoPort,
    AutomationUpdateTaskStatusRequest,
};
pub use graph::{
    automation_active_outgoing_edges, automation_initial_active_nodes,
    automation_selected_output_handles, automation_topological_order, validate_automation_graph,
    AutomationGraphError,
};
pub use service::{DesktopAutomationError, DesktopAutomationService};
pub use sqlite::SqliteAutomationStore;
pub use store::{
    AutomationActiveRunConflict, AutomationExecutionTransition, AutomationNodeStateUpdate,
    AutomationRecordKind, AutomationRunStateUpdate, AutomationStore, AutomationStoreError,
};
pub use template::{automation_json_path, render_automation_template};
pub use types::{
    AutomationBeginRunInput, AutomationDraft, AutomationEdge, AutomationNode,
    AutomationNodePosition, AutomationResumeRunInput, AutomationRun, AutomationRunDetail,
    AutomationRunNodeState, AutomationRunOnceInput, AutomationRunStatus, AutomationRunSummary,
    AutomationSaveDraftInput, AutomationScopeFilter, AutomationSignalEnvelope, AutomationWorkflow,
    AutomationWorkflowVersion, GraphExecution,
};

use lilia_kernel::{Event, Feature, FeatureContext, FeatureId, KernelError, ServiceKey, ServiceRef};

/// Where the automation service reports that a workflow or run changed.
pub trait AutomationEvents: Send + Sync + 'static {
    fn workflow_changed(&self, automation_id: Option<&str>);
    fn run_changed(&self, automation_id: &str, run_id: &str, status: AutomationRunStatus);
}

/// Drops every automation notification.
pub struct SilentAutomationEvents;

impl AutomationEvents for SilentAutomationEvents {
    fn workflow_changed(&self, _automation_id: Option<&str>) {}
    fn run_changed(&self, _automation_id: &str, _run_id: &str, _status: AutomationRunStatus) {}
}

/// One notification the service published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutomationNotification {
    WorkflowChanged {
        automation_id: Option<String>,
    },
    RunChanged {
        automation_id: String,
        run_id: String,
        status: AutomationRunStatus,
    },
}

/// Records notifications in order so tests can assert what a mutation
/// announced.
#[derive(Default)]
pub struct RecordingAutomationEvents {
    recorded: std::sync::Mutex<std::collections::VecDeque<AutomationNotification>>,
}

impl RecordingAutomationEvents {
    /// Removes and returns the oldest notification.
    pub fn take(&self) -> Option<AutomationNotification> {
        self.recorded
            .lock()
            .expect("the recording lock is never poisoned")
            .pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.recorded
            .lock()
            .expect("the recording lock is never poisoned")
            .is_empty()
    }

    fn record(&self, notification: AutomationNotification) {
        self.recorded
            .lock()
            .expect("the recording lock is never poisoned")
            .push_back(notification);
    }
}

impl AutomationEvents for RecordingAutomationEvents {
    fn workflow_changed(&self, automation_id: Option<&str>) {
        self.record(AutomationNotification::WorkflowChanged {
            automation_id: automation_id.map(str::to_owned),
        });
    }

    fn run_changed(&self, automation_id: &str, run_id: &str, status: AutomationRunStatus) {
        self.record(AutomationNotification::RunChanged {
            automation_id: automation_id.to_owned(),
            run_id: run_id.to_owned(),
            status,
        });
    }
}

/// Service slot for [`DesktopAutomationService`].
pub enum AutomationServiceKey {}

impl ServiceKey for AutomationServiceKey {
    type Value = DesktopAutomationService;

    const NAME: &'static str = "lilia.automation";
}

pub struct AutomationFeature {
    service: DesktopAutomationService,
}

impl AutomationFeature {
    pub fn new(service: DesktopAutomationService) -> Self {
        Self { service }
    }
}

impl Feature for AutomationFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.automation").expect("the automation feature id is not blank")
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<AutomationServiceKey>()]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        cx.provide::<AutomationServiceKey>(self.service.clone())
    }
}

/// A workflow was created, saved, published or deleted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomationChanged {
    pub automation_id: Option<String>,
}

impl Event for AutomationChanged {
    const NAME: &'static str = "lilia.automation.changed";

    fn subject(&self) -> Option<String> {
        self.automation_id.clone()
    }
}

/// A run advanced to a new status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomationRunChanged {
    pub automation_id: String,
    pub run_id: String,
    pub status: AutomationRunStatus,
}

impl Event for AutomationRunChanged {
    const NAME: &'static str = "lilia.automation.run_changed";

    fn subject(&self) -> Option<String> {
        Some(self.run_id.clone())
    }
}

/// Publishes automation notifications onto the kernel bus.
pub struct KernelAutomationEvents {
    events: lilia_kernel::EventBus,
}

impl KernelAutomationEvents {
    pub fn new(events: lilia_kernel::EventBus) -> Self {
        Self { events }
    }
}

impl AutomationEvents for KernelAutomationEvents {
    fn workflow_changed(&self, automation_id: Option<&str>) {
        self.events.publish(AutomationChanged {
            automation_id: automation_id.map(str::to_owned),
        });
    }

    fn run_changed(&self, automation_id: &str, run_id: &str, status: AutomationRunStatus) {
        self.events.publish(AutomationRunChanged {
            automation_id: automation_id.to_owned(),
            run_id: run_id.to_owned(),
            status,
        });
    }
}

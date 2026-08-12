mod application_ports;
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

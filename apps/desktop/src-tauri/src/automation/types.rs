use serde::Serialize;

pub use lilia_desktop_application::{
    AutomationDraft, AutomationEdge, AutomationNode, AutomationResumeRunInput, AutomationRun,
    AutomationRunDetail, AutomationRunNodeState, AutomationRunOnceInput, AutomationRunStatus,
    AutomationRunSummary, AutomationSaveDraftInput, AutomationScopeFilter,
    AutomationSignalEnvelope, AutomationWorkflow, AutomationWorkflowVersion, GraphExecution,
};

#[cfg(test)]
pub use lilia_desktop_application::AutomationNodePosition;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationChangedEvent {
    pub(crate) workflow_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationRunEvent {
    pub(crate) run: AutomationRun,
}

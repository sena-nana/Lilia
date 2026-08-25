use std::fmt;

use super::{
    AutomationBeginRunInput, AutomationGraphError, AutomationRunDetail, AutomationRunStatus,
    AutomationRunSummary, AutomationSaveDraftInput, AutomationWorkflow, AutomationWorkflowVersion,
};
use serde_json::Value as JsonValue;

pub trait AutomationStore: Send {
    fn list_workflows(&self) -> Result<Vec<AutomationWorkflow>, AutomationStoreError>;

    fn workflow(
        &self,
        workflow_id: &str,
    ) -> Result<Option<AutomationWorkflow>, AutomationStoreError>;

    fn save_draft(
        &mut self,
        input: AutomationSaveDraftInput,
    ) -> Result<AutomationWorkflow, AutomationStoreError>;

    fn publish(
        &mut self,
        workflow_id: &str,
    ) -> Result<AutomationWorkflowVersion, AutomationStoreError>;

    fn set_enabled(
        &mut self,
        workflow_id: &str,
        enabled: bool,
    ) -> Result<AutomationWorkflow, AutomationStoreError>;

    fn delete_workflow(&mut self, workflow_id: &str) -> Result<(), AutomationStoreError>;

    fn version(
        &self,
        version_id: &str,
    ) -> Result<Option<AutomationWorkflowVersion>, AutomationStoreError>;

    fn try_begin_run(
        &mut self,
        input: AutomationBeginRunInput,
    ) -> Result<AutomationRunDetail, AutomationStoreError>;

    fn list_runs(
        &self,
        workflow_id: Option<&str>,
    ) -> Result<Vec<AutomationRunSummary>, AutomationStoreError>;

    fn run_detail(&self, run_id: &str)
        -> Result<Option<AutomationRunDetail>, AutomationStoreError>;

    fn apply_execution_transition(
        &mut self,
        transition: AutomationExecutionTransition,
    ) -> Result<AutomationRunDetail, AutomationStoreError>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationExecutionTransition {
    pub run_id: String,
    pub run: AutomationRunStateUpdate,
    pub nodes: Vec<AutomationNodeStateUpdate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomationRunStateUpdate {
    pub expected_statuses: Vec<AutomationRunStatus>,
    pub status: AutomationRunStatus,
    pub error: Option<String>,
    pub finished: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationNodeStateUpdate {
    pub node_id: String,
    pub expected_statuses: Vec<AutomationRunStatus>,
    pub status: AutomationRunStatus,
    pub input: JsonValue,
    pub output: Option<JsonValue>,
    pub error: Option<String>,
    pub mark_started: bool,
    pub finished: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomationActiveRunConflict {
    pub workflow_id: String,
    pub run_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomationRecordKind {
    Workflow,
    WorkflowVersion,
    Run,
    RunNode,
}

impl fmt::Display for AutomationRecordKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Workflow => "workflow",
            Self::WorkflowVersion => "workflow version",
            Self::Run => "run",
            Self::RunNode => "run node",
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AutomationStoreError {
    #[error(transparent)]
    Graph(#[from] AutomationGraphError),
    #[error("automation workflow name must not be empty")]
    InvalidWorkflowName,
    #[error("automation workflow does not exist: {workflow_id}")]
    WorkflowNotFound { workflow_id: String },
    #[error(
        "automation workflow must be published before it can run or be enabled: {workflow_id}"
    )]
    PublishedVersionRequired { workflow_id: String },
    #[error("automation workflow version does not exist: {version_id}")]
    VersionNotFound { version_id: String },
    #[error(
        "automation workflow version {version_id} belongs to {actual_workflow_id}, not {expected_workflow_id}"
    )]
    VersionWorkflowMismatch {
        version_id: String,
        expected_workflow_id: String,
        actual_workflow_id: String,
    },
    #[error("automation workflow {workflow_id} already has active run {run_id}")]
    ActiveRunExists { workflow_id: String, run_id: String },
    #[error("automation run does not exist: {run_id}")]
    RunNotFound { run_id: String },
    #[error("automation run node does not exist: {run_id}/{node_id}")]
    RunNodeNotFound { run_id: String, node_id: String },
    #[error("automation transition contains duplicate node {node_id}")]
    DuplicateNodeTransition { node_id: String },
    #[error(
        "stored {record_kind} {record_id} cannot transition from {actual:?}; expected one of {expected:?}"
    )]
    InvalidStateTransition {
        record_kind: AutomationRecordKind,
        record_id: String,
        expected: Vec<AutomationRunStatus>,
        actual: AutomationRunStatus,
    },
    #[error("existing automation data has multiple active runs: {conflicts:?}")]
    ExistingActiveRunConflict {
        conflicts: Vec<AutomationActiveRunConflict>,
    },
    #[error("stored {record_kind} {record_id} has invalid JSON in {field}: {message}")]
    CorruptJson {
        record_kind: AutomationRecordKind,
        record_id: String,
        field: &'static str,
        message: String,
    },
    #[error("stored {record_kind} {record_id} has invalid status {status}")]
    InvalidStoredStatus {
        record_kind: AutomationRecordKind,
        record_id: String,
        status: String,
    },
    #[error("automation serialization failed for {field}: {message}")]
    Serialization {
        field: &'static str,
        message: String,
    },
    #[error("automation schema invariant failed: {message}")]
    SchemaInvariant { message: String },
    #[error("automation storage operation {operation} failed: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },
}

impl AutomationStoreError {
    pub(crate) fn storage(operation: &'static str, error: impl fmt::Display) -> Self {
        Self::Storage {
            operation,
            message: error.to_string(),
        }
    }
}

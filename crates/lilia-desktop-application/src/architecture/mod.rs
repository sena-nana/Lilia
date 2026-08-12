mod service;
mod sqlite;
mod types;

#[cfg(test)]
mod application_tests;

use std::fmt;

pub use service::DesktopArchitectureService;
pub use sqlite::SqliteArchitectureStore;
pub use types::{
    ArchitectureBackend, ArchitectureChangeStatus, ArchitecturePermission,
    ProjectArchitectureApplyInput, ProjectArchitectureApplyResult, ProjectArchitectureChange,
    ProjectArchitectureChangeEvent, ProjectArchitectureChangeRecord, ProjectArchitectureEdge,
    ProjectArchitectureGraph, ProjectArchitectureNode, ProjectArchitectureQuarantineRecord,
    ProjectArchitectureRejectInput, ProjectArchitectureRollbackResult,
};

pub trait ArchitectureStore: Send {
    fn graph(
        &mut self,
        project_id: &str,
    ) -> Result<ProjectArchitectureGraph, DesktopArchitectureError>;
    fn list_changes(
        &mut self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<ProjectArchitectureChangeRecord>, DesktopArchitectureError>;
    fn list_quarantine(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectArchitectureQuarantineRecord>, DesktopArchitectureError>;
    fn apply(
        &mut self,
        input: ProjectArchitectureApplyInput,
    ) -> Result<ProjectArchitectureApplyResult, DesktopArchitectureError>;
    fn reject(
        &mut self,
        input: ProjectArchitectureRejectInput,
    ) -> Result<ProjectArchitectureChangeEvent, DesktopArchitectureError>;
    fn rollback(
        &mut self,
        project_id: &str,
        task_id: &str,
        backend: ArchitectureBackend,
    ) -> Result<ProjectArchitectureRollbackResult, DesktopArchitectureError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopArchitectureError {
    #[error("architecture project id must not be empty")]
    EmptyProjectId,
    #[error("architecture task id must not be empty")]
    EmptyTaskId,
    #[error("architecture changes must not be empty")]
    EmptyChanges,
    #[error("architecture node id must not be empty")]
    EmptyNodeId,
    #[error("architecture edge id, source and target must not be empty")]
    InvalidEdge,
    #[error("architecture edge {edge_id} references missing node {node_id}")]
    MissingEdgeNode { edge_id: String, node_id: String },
    #[error("architecture version conflict: expected {expected}, current {current}")]
    VersionConflict { expected: i64, current: i64 },
    #[error("architecture request id {request_id} was already used with different input")]
    IdempotencyConflict { request_id: String },
    #[error("stored architecture event {event_id} has invalid backend {backend}")]
    InvalidStoredBackend { event_id: String, backend: String },
    #[error("stored architecture event {event_id} has invalid permission {permission}")]
    InvalidStoredPermission {
        event_id: String,
        permission: String,
    },
    #[error("stored architecture event {event_id} has invalid status {status}")]
    InvalidStoredStatus { event_id: String, status: String },
    #[error("architecture state is unavailable")]
    StateUnavailable,
    #[error("architecture storage operation {operation} failed: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },
}

impl DesktopArchitectureError {
    pub(crate) fn storage(operation: &'static str, error: impl fmt::Display) -> Self {
        Self::Storage {
            operation,
            message: error.to_string(),
        }
    }
}

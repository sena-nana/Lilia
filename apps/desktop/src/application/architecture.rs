pub use lilia_feature_architecture::{
    ArchitectureBackend, ArchitectureChangeStatus, ArchitecturePermission, ArchitectureStore,
    DesktopArchitectureError, DesktopArchitectureService, ProjectArchitectureApplyInput,
    ProjectArchitectureApplyResult, ProjectArchitectureChange, ProjectArchitectureChangeEvent,
    ProjectArchitectureChangeRecord, ProjectArchitectureEdge, ProjectArchitectureGraph,
    ProjectArchitectureNode, ProjectArchitectureQuarantineRecord, ProjectArchitectureRejectInput,
    ProjectArchitectureRollbackResult, SqliteArchitectureStore,
};

#[cfg(test)]
mod tests;

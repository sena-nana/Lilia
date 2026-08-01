//! Lilia product contracts.
//!
//! This crate owns product-facing IDs, revisioned commands, structured errors, and
//! AgentKit *reference* types (session / profile / artifact ids). It does not own
//! Agent Runtime wire payloads or Host UI types.

mod artifact;
mod binding;
mod command;
mod conversation;
mod entity;
mod error;
mod frontend_contract;
mod ids;
mod milestone;
mod project;
mod projection;
mod revision;
mod task;
mod workflow;

pub use artifact::{
    ArtifactMaterializationStatus, ArtifactRetention, ProductArtifact, ProjectAsset,
    ProjectAssetKind, ProjectAssetProposalStatus,
};
pub use binding::{AgentSessionBinding, AgentSessionRef};
pub use command::{
    IdempotencyKey, Page, PageRequest, ProductCommandMeta, ProductCommandResult, ProductEvent,
    ProductEventSequence, SortDirection,
};
pub use conversation::{ProductConversation, ProductConversationStatus};
pub use entity::{ProductEntity, ProductEntityKind};
pub use error::{ConflictKind, ProductError, ProductResult};
pub use frontend_contract::{product_event_name, PRODUCT_CORE_FRONTEND_CONTRACT_JSON};
pub use ids::{
    ArtifactId, AssignmentId, BindingId, ConversationId, MilestoneId, ProjectAssetId, ProjectId,
    TaskId, WorkflowId, WorkflowRunId,
};
pub use milestone::{ProductMilestone, ProductMilestoneStatus};
pub use project::{GitWorkspaceRef, Project, ProjectArchiveState, ProjectSettings};
pub use projection::{
    ArtifactProjection, PendingProjection, PendingProjectionStatus, ProductApprovalDecision,
    ProjectionEventId, TimelineProjectionCommand, TimelineProjectionEvent, TodoProjection,
    PRODUCT_TIMELINE_STORE_ID, TIMELINE_UI_CACHE_KIND,
};
pub use revision::{ExpectedRevision, ProductRevision};
pub use task::{
    ProductTask, ProductTaskPriority, ProductTaskStatus, TaskDependencyGraph, TaskDependencyRule,
    AGENT_TODO_PROMOTION_REQUIRED,
};
pub use workflow::{
    AssignmentStatus, ProductAssignment, ProductWorkflow, ProductWorkflowRun,
    ProductWorkflowRunStatus, ProductWorkflowStatus,
};

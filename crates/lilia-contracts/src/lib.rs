//! Lilia product contracts.
//!
//! This crate owns product-facing IDs, revisioned commands, structured errors, and
//! AgentKit *reference* types (session / profile / artifact ids). It does not own
//! Agent Runtime wire payloads or Host UI types.

mod binding;
mod error;
mod ids;
mod project;
mod projection;
mod revision;
mod task;

pub use binding::{AgentSessionBinding, AgentSessionRef};
pub use error::{ConflictKind, ProductError, ProductResult};
pub use ids::{BindingId, ConversationId, MilestoneId, ProjectId, TaskId, WorkflowId};
pub use project::{Project, ProjectArchiveState};
pub use projection::{
    ArtifactProjection, PendingProjection, PendingProjectionStatus, ProductApprovalDecision,
    ProjectionEventId, TimelineProjectionCommand, TimelineProjectionEvent, TodoProjection,
    PRODUCT_TIMELINE_STORE_ID, TIMELINE_UI_CACHE_KIND,
};
pub use revision::{ExpectedRevision, ProductRevision};
pub use task::{
    ProductTask, ProductTaskStatus, TaskDependencyGraph, TaskDependencyRule,
    AGENT_TODO_PROMOTION_REQUIRED,
};

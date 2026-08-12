//! Lilia product contracts.
//!
//! This crate owns product-facing IDs, revisioned commands, structured errors, and
//! AgentKit *reference* types (session / profile / artifact ids). It does not own
//! Agent Runtime wire payloads or Host UI types.

mod artifact;
mod assistant;
mod attachment;
mod binding;
mod command;
mod context;
mod conversation;
mod entity;
mod error;
mod frontend_contract;
mod handoff;
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
pub use assistant::{
    auto_model_for_provider_family_tier, auto_reasoning_effort_for_tier,
    auto_turn_decision_request_instruction, auto_turn_decision_system_instruction,
    auto_turn_decision_tier_policy, AutoTurnDecisionTierPolicy,
};
pub use attachment::{
    ChatAttachment, ChatAttachmentDirectoryMeta, ChatAttachmentKind, ChatContextSearchMatch,
    ChatContextSearchResult,
};
pub use binding::{AgentSessionBinding, AgentSessionRef};
pub use command::{
    IdempotencyKey, Page, PageRequest, ProductCommandMeta, ProductCommandResult, ProductEvent,
    ProductEventSequence, SortDirection,
};
pub use context::ChatContextUsage;
pub use conversation::{ChatConversationReference, ProductConversation, ProductConversationStatus};
pub use entity::{ProductEntity, ProductEntityKind};
pub use error::{ConflictKind, ProductError, ProductResult};
pub use frontend_contract::{product_event_name, PRODUCT_CORE_FRONTEND_CONTRACT_JSON};
pub use handoff::{
    LiliaCodeTaskHandoff, LiliaCodeTaskHandoffKind, ProductTaskHandoffImport,
    ProductTaskHandoffRecord, PullRequestHandoffContext, TaskHandoffRepository, TaskHandoffSource,
    WorkflowHandoffContext, LILIA_CODE_TASK_HANDOFF_PROTOCOL, LILIA_CODE_TASK_HANDOFF_VERSION,
};
pub use ids::{
    ArtifactId, AssignmentId, BindingId, ConversationId, MilestoneId, ProjectAssetId, ProjectId,
    TaskId, WorkflowId, WorkflowRunId,
};
pub use milestone::{ProductMilestone, ProductMilestoneStatus};
pub use project::{
    GitWorkspaceRef, ProductProjectRemovalOutcome, ProductProjectReorderEntry,
    ProductProjectReorderOutcome, Project, ProjectArchiveState, ProjectSettings,
};
pub use projection::{
    ArtifactProjection, PendingProjection, PendingProjectionStatus, ProductApprovalDecision,
    ProjectionEventId, TimelineProjectionCommand, TimelineProjectionCursor,
    TimelineProjectionEvent, TimelineProjectionPage, TodoProjection, PRODUCT_TIMELINE_STORE_ID,
    TIMELINE_UI_CACHE_KIND,
};
pub use revision::{ExpectedRevision, ProductRevision};
pub use task::{
    ProductTask, ProductTaskMoveInput, ProductTaskMoveOutcome, ProductTaskPriority,
    ProductTaskReorderEntry, ProductTaskReorderOutcome, ProductTaskStatus, TaskDependencyGraph,
    TaskDependencyRule, AGENT_TODO_PROMOTION_REQUIRED,
};
pub use workflow::{
    AssignmentStatus, ProductAssignment, ProductWorkflow, ProductWorkflowRun,
    ProductWorkflowRunStatus, ProductWorkflowStatus,
};

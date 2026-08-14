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
mod sidebar;
mod task;
mod workflow;

pub use artifact::{
    ArtifactMaterializationStatus, ArtifactRetention, ProductArtifact, ProjectAsset,
    ProjectAssetKind, ProjectAssetProposalStatus,
};
pub use assistant::{
    auto_context_thresholds_for_scale, auto_model_for_provider_family_tier,
    auto_preset_for_context_scale, auto_preset_for_workflow_type, auto_reasoning_effort_for_preset,
    auto_reasoning_effort_for_tier, auto_turn_decision_request_instruction,
    auto_turn_decision_system_instruction, auto_turn_decision_tier_policy, builtin_preset_label,
    context_compaction_request_instruction, context_compaction_success_message,
    context_compaction_system_instruction, main_agent_system_instruction, plan_mode_preset,
    prompt_optimize_request_instruction, prompt_optimize_requirements,
    prompt_optimize_system_instruction, prompt_router_request_instruction,
    prompt_router_requirements, prompt_router_scenarios, prompt_router_system_instruction,
    tier_for_preset, AutoTurnDecisionTierPolicy, ModelSelectionContextThresholds,
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
    GitWorkspaceRef, ProductProjectArchiveConversationEntry, ProductProjectArchiveInput,
    ProductProjectArchiveOutcome, ProductProjectArchiveTaskEntry, ProductProjectRemovalOutcome,
    ProductProjectReorderEntry, ProductProjectReorderOutcome, Project, ProjectArchiveState,
    ProjectSettings,
};
pub use projection::{
    ArtifactProjection, PendingProjection, PendingProjectionStatus, ProductApprovalDecision,
    ProjectionEventId, TimelineProjectionCommand, TimelineProjectionCursor,
    TimelineProjectionEvent, TimelineProjectionPage, TodoProjection, PRODUCT_TIMELINE_STORE_ID,
    TIMELINE_UI_CACHE_KIND,
};
pub use revision::{ExpectedRevision, ProductRevision};
pub use sidebar::{
    SidebarNavigationContribution, SidebarNavigationContributionError,
    SidebarNavigationContributionSet, SidebarNavigationIcon, SidebarNavigationTarget,
    SIDEBAR_NAVIGATION_EXTENSION_ID, SIDEBAR_NAVIGATION_SCHEMA_VERSION,
};
pub use task::{
    ProductTask, ProductTaskArchiveConversationEntry, ProductTaskArchiveInput,
    ProductTaskArchiveOutcome, ProductTaskMoveInput, ProductTaskMoveOutcome, ProductTaskPriority,
    ProductTaskReorderEntry, ProductTaskReorderOutcome, ProductTaskStatus, TaskDependencyGraph,
    TaskDependencyRule, AGENT_TODO_PROMOTION_REQUIRED,
};
pub use workflow::{
    AssignmentStatus, LiliaAgentWorkflow, LiliaReviewTarget, ProductAssignment, ProductWorkflow,
    ProductWorkflowRun, ProductWorkflowRunStatus, ProductWorkflowStatus,
};

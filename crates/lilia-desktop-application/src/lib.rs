//! Host-neutral desktop application boundary for LiliaCode.
//!
//! Tauri and native UI hosts depend on this crate instead of owning product,
//! persistence, or AgentKit state themselves.

mod agent;
mod agent_interaction;
mod application;
mod architecture;
mod attachment;
mod auto_turn;
mod automation;
mod buffer;
mod cli;
mod coding_services;
mod command;
mod composer;
mod config;
mod context_search;
mod context_usage;
mod conversation_reference;
mod document;
mod domain_services;
#[cfg(debug_assertions)]
mod equivalence;
mod events;
mod extensions;
mod github;
mod goal;
mod handoff;
mod hooks;
mod host;
mod import;
mod language;
mod legacy_database;
mod mcp_elicitation;
mod memory;
mod panel;
mod plugins;
mod product_management;
mod project;
mod project_clone;
mod provider;
mod query;
mod remote;
mod roadmap;
mod slash_command;
mod submission;
mod todo;
mod turn_queue;
mod update;
mod usage;
mod workspace;
mod workspace_item;
mod worktree;

pub use agent::{
    DesktopApprovalResponse, DesktopArchitectureInteractionDecision,
    DesktopArchitectureInteractionResponse, DesktopAutomaticTurnSelection,
    DesktopAutomationTurnCorrelation, DesktopExecutionPermission, DesktopInteractionResponse,
    DesktopInterruptResult, DesktopTaskRuntimeSnapshot, DesktopTurnDispatch,
    DesktopTurnDispatchKind, DesktopTurnRequest,
};
#[cfg(debug_assertions)]
pub use agent::{DesktopDurableTurnDebugSnapshot, DesktopQuarantinedTurnDebugSnapshot};
pub use agent_interaction::{
    DesktopAgentInteractionError, DesktopAgentInteractionSettings,
    DesktopAgentInteractionSettingsUpdate, DesktopAutoTurnDecisionSettings,
    DesktopCustomSubagentCatalog, DesktopCustomSubagentDefinition, DesktopCustomSubagentUpsert,
    DesktopSubagentModeSettings,
};
pub use application::{DesktopApplication, DesktopApplicationError, DesktopTaskSessionSnapshot};
pub use architecture::{
    ArchitectureBackend, ArchitectureChangeStatus, ArchitecturePermission, ArchitectureStore,
    DesktopArchitectureError, DesktopArchitectureService, ProjectArchitectureApplyInput,
    ProjectArchitectureApplyResult, ProjectArchitectureChange, ProjectArchitectureChangeEvent,
    ProjectArchitectureChangeRecord, ProjectArchitectureEdge, ProjectArchitectureGraph,
    ProjectArchitectureNode, ProjectArchitectureQuarantineRecord, ProjectArchitectureRejectInput,
    ProjectArchitectureRollbackResult, SqliteArchitectureStore,
};
pub use attachment::{
    describe_attachment_path, describe_attachment_paths, save_clipboard_image_attachment,
    DesktopAttachmentError,
};
pub use auto_turn::DesktopAutoTurnDecisionError;
pub use automation::{
    automation_active_outgoing_edges, automation_initial_active_nodes, automation_json_path,
    automation_selected_output_handles, automation_topological_order, render_automation_template,
    validate_automation_graph, AutomationActiveRunConflict, AutomationAddTodoRequest,
    AutomationAgentActivation, AutomationAgentDispatch, AutomationAgentPort, AutomationAgentTarget,
    AutomationBeginRunInput, AutomationCompleteAgentInput, AutomationCreateTaskRequest,
    AutomationDraft, AutomationEdge, AutomationExecutionEngine, AutomationExecutionError,
    AutomationExecutionPorts, AutomationExecutionRepository, AutomationExecutionResult,
    AutomationExecutionTransition, AutomationGraphError, AutomationGuidePort,
    AutomationIdempotencyKey, AutomationNode, AutomationNodePosition, AutomationNodeStateUpdate,
    AutomationPortContext, AutomationPortError, AutomationRecordKind,
    AutomationRecordTimelineRequest, AutomationResumeRunInput, AutomationRun, AutomationRunDetail,
    AutomationRunNodeState, AutomationRunOnceInput, AutomationRunStateUpdate, AutomationRunStatus,
    AutomationRunSummary, AutomationSaveDraftInput, AutomationScopeFilter,
    AutomationSendGuideRequest, AutomationSignalEnvelope, AutomationStartAgentRequest,
    AutomationStore, AutomationStoreError, AutomationTaskPort, AutomationTimelinePort,
    AutomationTodoPort, AutomationUpdateTaskStatusRequest, AutomationWorkflow,
    AutomationWorkflowVersion, DesktopAutomationError, DesktopAutomationService, GraphExecution,
    SqliteAutomationStore,
};
pub use buffer::{
    BufferError, BufferId, BufferRevision, BufferSnapshot, BufferStore, TextBuffer, TextEdit,
};
pub use coding_services::{
    DesktopCodeSearchHit, DesktopCodeSearchResult, DesktopCodingServicesSnapshot, DesktopGitChange,
    DesktopGitFileStatus, DesktopGitStatus, DesktopWorkspaceEntry, DesktopWorkspaceListing,
};
pub use command::{DesktopCommand, DesktopCommandOutcome};
pub use composer::{
    DesktopComposerCommand, DesktopComposerError, DesktopComposerState, DesktopComposerSubmission,
};
pub use config::{DesktopApplicationConfig, DesktopApplicationConfigError, DesktopDomainDatabase};
pub use context_search::search_context_attachments;
pub use document::{DocumentError, DocumentId, DocumentSnapshot, DocumentStore};
#[cfg(debug_assertions)]
pub use equivalence::{
    DesktopEquivalenceComposerFact, DesktopEquivalenceConversationFact,
    DesktopEquivalenceHookHandlerFact, DesktopEquivalenceHookSourceFact,
    DesktopEquivalenceMcpCredentialFact, DesktopEquivalenceMcpServerFact,
    DesktopEquivalenceProjectFact, DesktopEquivalenceSkillFact, DesktopEquivalenceSnapshot,
    DesktopEquivalenceTaskFact, DesktopEquivalenceTimelineFact,
};
pub use events::{
    DesktopApprovalState, DesktopEvent, DesktopEventBus, DesktopEventKind,
    DesktopEventSubscription, DesktopInteractionState, DesktopNavigationTarget, DesktopTurnState,
    DesktopUpdateState,
};
pub use extensions::{
    DesktopExtensionsSnapshot, DesktopMcpActivationReport, DesktopMcpActivationResult,
    DesktopMcpCredentialKind, DesktopMcpCredentialView, DesktopMcpPromptArgumentView,
    DesktopMcpPromptFragmentView, DesktopMcpPromptGetView, DesktopMcpPromptView,
    DesktopMcpResourceContentView, DesktopMcpResourceReadView, DesktopMcpResourceView,
    DesktopMcpServerUpsert, DesktopMcpServerView, DesktopMcpToolView, DesktopMcpTransport,
    DesktopRuntimeServiceView, DesktopSkillCreate, DesktopSkillPackageView, DesktopSkillScope,
};
pub use github::{
    DesktopGitHubBindingMetadata, DesktopGitHubBindingStatus, DesktopGitHubClientIdSource,
    DesktopGitHubDeviceFlowPollResult, DesktopGitHubDeviceFlowStart, DesktopGitHubError,
    DesktopGitHubRepoPage, DesktopGitHubRepoSummary,
};
pub use goal::{DesktopGoalSnapshot, DesktopGoalStatus};
pub use handoff::{
    describe_task_handoff, prepare_task_handoff_reference, DesktopImportedTaskHandoff,
    DesktopTaskHandoffOpen,
};
pub use hooks::{
    DesktopHookDocumentUpdate, DesktopHookDocumentView, DesktopHookError, DesktopHookHandlerUpdate,
    DesktopHookHandlerView, DesktopHookScope, DesktopHookSourceView, DesktopHooksOverview,
};
pub use host::{
    DesktopCliRequest, DesktopCliResult, DesktopClipboardImage, DesktopCredentialAction,
    DesktopFileDialogRequest, DesktopFileFilter, DesktopHost, DesktopHostAction,
    DesktopHostContext, DesktopHostError, DesktopHostResult, DesktopSecret, DesktopShortcutAction,
    DesktopSingleInstanceRequest, DesktopTrayAction, DesktopTrayItem, DesktopUpdateAction,
    DesktopUpdateResult, DesktopWindowAction, HostCredentialImportResult,
};
pub use import::{
    CredentialImportDecision, DesktopDataImportService, DesktopDatabaseKind, DesktopImportError,
    DesktopImportErrorCode, DesktopImportExecutionOptions, DesktopImportFile,
    DesktopImportFileMetadata, DesktopImportFileRole, DesktopImportItemError,
    DesktopImportItemKind, DesktopImportPlan, DesktopImportPlanItem, DesktopImportPlanItemStatus,
    DesktopImportPlanStatus, DesktopImportReport, DesktopImportReportItem,
    DesktopImportReportItemStatus, DesktopImportReportStatus,
};
pub use language::{LanguageDefinition, LanguageId, LanguageRegistry, LanguageRegistryError};
pub use legacy_database::DesktopLegacyDatabaseError;
pub use lilia_contracts::{
    ChatAttachment, ChatAttachmentDirectoryMeta, ChatAttachmentKind, ChatContextSearchMatch,
    ChatContextSearchResult, ChatContextUsage, ChatConversationReference,
    ProductProjectRemovalOutcome,
};
pub use mcp_elicitation::{
    DesktopMcpElicitation, DesktopMcpElicitationAction, DesktopMcpElicitationError,
    DesktopMcpElicitationMode, DesktopMcpFormField, DesktopMcpFormFieldKind, DesktopMcpFormOption,
    MCP_ELICITATION_INTERACTION_KIND,
};
pub use memory::{
    DesktopMemory, DesktopMemoryError, DesktopMemoryService, InMemoryMemorySettingsStore,
    MemoryInjectionState, MemoryScope, MemorySettings, MemorySettingsStore, MemoryStore,
    MemoryStoreError, MemoryUpsertInput, SqliteMemoryStore, MEMORY_SETTINGS_KEY,
};
pub use panel::{
    default_panel_states, DockSlot, PaneId, PaneNode, PanelId, PanelLayoutError,
    PanelLayoutSnapshot, PanelState, SplitAxis, WorkspaceItemId, CODING_TOOLS_PANEL_ID,
    DIAGNOSTICS_PANEL_ID, RESOURCES_PANEL_ID, TASK_INSPECTOR_PANEL_ID,
};
pub use plugins::{DesktopPluginInstall, DesktopPluginPackageView};
pub use product_management::{
    DesktopOptionalTextUpdate, DesktopProjectCreate, DesktopProjectPatch,
    DesktopProjectRemovalPreview, DesktopTaskCreate, DesktopTaskMove, DesktopTaskPatch,
};
pub use project::{ProjectContext, ProjectContextError};
pub use project_clone::{
    DesktopProjectCloneError, DesktopProjectCloneOperation, DesktopProjectClonePhase,
    DesktopProjectCloneRequest, DesktopProjectCloneResult, DesktopProjectCloneSnapshot,
};
pub use provider::{
    DesktopAgentRuntimeSettings, DesktopAgentRuntimeSettingsUpdate, DesktopCapabilityLimit,
    DesktopCredentialKind, DesktopCredentialStatus, DesktopCredentialView,
    DesktopProviderCapabilityView, DesktopProviderCredentialImportInput,
    DesktopProviderCredentialInput, DesktopProviderError, DesktopProviderRuntimeState,
    DesktopProviderSnapshot, DesktopProviderView, DesktopRemoteQuotaState,
};
pub use query::{DesktopTaskScope, ProjectQuery, TaskQuery};
pub use remote::{
    DesktopRemoteControlError, DesktopRemoteControlService, RemoteCapabilitySet,
    RemoteControlStatus, RemoteEndpointAddress, RemotePairDeviceInput, RemotePairingTicket,
    RemotePeerSummary, RemoteRequestEnvelope, REMOTE_ALPN, REMOTE_MIN_PROTOCOL_VERSION,
    REMOTE_PROTOCOL_VERSION,
};
pub use roadmap::{
    DesktopRoadmapService, Milestone, MilestoneDueDateUpdate, MilestoneStatus,
    MilestoneUpdatePatch, ProjectRoadmap, RoadmapStore, RoadmapStoreError, SqliteRoadmapStore,
    TaskMilestoneLink,
};
pub use slash_command::{
    DesktopSlashCommand, DesktopSlashCommandExecution, DesktopSlashCommandSearchResult,
    DesktopSlashCommandSource,
};
pub use submission::DesktopSubmissionError;
pub use todo::{
    DesktopGuideDispatchResult, DesktopGuideDispatchWindow, DesktopTaskTodo, DesktopTodoCreate,
    DesktopTodoError, DesktopTodoGuideStatus, DesktopTodoPriority, DesktopTodoSource,
    DesktopTodoUpdate,
};
pub use turn_queue::DesktopTurnQueueError;
pub use usage::{
    QuotaUsageBackendSummary, QuotaUsageConversationSummary, QuotaUsageCostCoverage,
    QuotaUsageDailyBucket, QuotaUsageProjectSummary, QuotaUsageRecentRecord, QuotaUsageStats,
    QuotaUsageStatsInput, QuotaUsageTokenTotals, QuotaUsageToolSummary,
};
pub use workspace::{
    DesktopWorkspaceProject, DesktopWorkspaceSession, DesktopWorkspaceSessionId,
    DesktopWorkspaceSessionIdError, DesktopWorkspaceSessionState,
    DesktopWorkspaceSessionStateError, DesktopWorkspaceSnapshot, DesktopWorkspaceTask,
    DesktopWorkspaceTransferOutcome,
};
pub use workspace_item::{
    ApplicationWorkspaceSurface, ProjectWorkspaceSurface, WorkspaceFocusTarget, WorkspaceItem,
    WorkspaceItemCapabilities, WorkspaceItemError, WorkspaceItemKind, WorkspaceItemRestoration,
    WorkspaceResourceId, ARCHITECTURE_WORKSPACE_ITEM_KIND, AUTOMATION_WORKSPACE_ITEM_KIND,
    MEMORY_WORKSPACE_ITEM_KIND, ROADMAP_WORKSPACE_ITEM_KIND, SETTINGS_WORKSPACE_ITEM_KIND,
    TASK_WORKSPACE_ITEM_KIND,
};
pub use worktree::{
    DesktopTaskWorktree, DesktopWorktreeError, DesktopWorktreeListItem, DesktopWorktreeMergeResult,
    DesktopWorktreeStatus,
};

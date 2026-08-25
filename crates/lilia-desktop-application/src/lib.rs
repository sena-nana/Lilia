//! Host-neutral desktop application boundary for LiliaCode.
//!
//! Desktop UI hosts depend on this crate instead of owning product,
//! persistence, or AgentKit state themselves.

mod agent;
mod agent_interaction;
mod application;
mod architecture;
mod assistant_ai_probe;
mod attachment;
mod auto_turn;
mod automation;
mod automation_ports;
mod auxiliary_model;
mod change_feed;
mod cli;
mod coding_services;
mod command;
mod composer;
mod config;
mod context_compaction;
mod context_search;
mod context_usage;
mod contributions;
mod conversation_reference;
mod conversation_suggestions;
mod diagnostics;
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
mod iab;
mod import;
mod language;
mod language_service;
mod mcp_elicitation;
mod panel;
mod plugins;
mod popup_settings;
mod product_management;
mod project;
mod project_files;
mod project_settings;
mod project_tasks;
mod prompt_optimize;
mod provider;
mod provider_ui_settings;
mod registry_watch;
mod remote;
mod session_search;
mod slash_command;
mod submission;
mod terminal;
mod timeline_retry;
mod title_update;
mod todo;
mod tool_consent;
mod update;
mod usage;
mod workspace;
mod workspace_item;
mod worktree;

pub use agent::{
    DesktopApprovalResponse, DesktopArchitectureInteractionDecision,
    DesktopArchitectureInteractionResponse, DesktopAutomaticTurnSelection,
    DesktopAutomationTurnCorrelation, DesktopExecutionPermission, DesktopInteractionResponse,
    DesktopInterruptResult, DesktopSessionBranchAnchor, DesktopSessionBranchMode,
    DesktopTaskRuntimeSnapshot, DesktopTurnDispatch, DesktopTurnDispatchKind, DesktopTurnRequest,
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
pub use assistant_ai_probe::{
    DesktopAssistantAiModelsResult, DesktopAssistantAiProbeInput, DesktopAssistantAiTestResult,
};
pub use attachment::{
    clipboard_text_should_be_attachment, describe_attachment_path, describe_attachment_paths,
    save_clipboard_image_attachment, save_clipboard_text_attachment,
    save_encoded_clipboard_image_attachment, DesktopAttachmentError, DesktopClipboardEncodedImage,
    LONG_CLIPBOARD_TEXT_ATTACHMENT_THRESHOLD, MAX_CLIPBOARD_TEXT_ATTACHMENT_BYTES,
};
pub use auto_turn::{preview_automatic_turn_selection, DesktopAutoTurnDecisionError};
pub use lilia_feature_automation::{
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
pub use lilia_feature_document::{
    BufferError, BufferId, BufferRevision, BufferSnapshot, BufferStore, TextBuffer, TextEdit,
};
pub use change_feed::PRODUCT_CHANGE_FEED_SOURCE;
pub use coding_services::{
    DesktopCodeSearchHit, DesktopCodeSearchMode, DesktopCodeSearchResult, DesktopCodeSearchScope,
    DesktopCodingServicesSnapshot, DesktopGitChange, DesktopGitDiff, DesktopGitDiffScope,
    DesktopGitFileStatus, DesktopGitStatus, DesktopWorkspaceCodeSearchFailure,
    DesktopWorkspaceCodeSearchHit, DesktopWorkspaceCodeSearchResult, DesktopWorkspaceEntry,
    DesktopWorkspaceListing,
};
pub use command::{DesktopCommand, DesktopCommandOutcome};
pub use composer::{
    DesktopComposerCommand, DesktopComposerError, DesktopComposerState, DesktopComposerSubmission,
    DesktopComposerTurnRequest,
};
pub use config::{DesktopApplicationConfig, DesktopApplicationConfigError};
pub use context_compaction::DesktopContextCompactionResult;
pub use context_search::search_context_attachments;
pub use conversation_suggestions::{
    request_model_completion, ConversationSuggestionModelPort,
    DesktopApplicationSuggestionModelPort, DesktopConversationSuggestionError,
    DesktopConversationSuggestionSettings, DesktopConversationSuggestionSource,
    DesktopSuggestionGitHubActivityRef, DesktopSuggestionItem, DesktopSuggestionItemSource,
    DesktopSuggestionLocalGitContextRef, DesktopSuggestionLocalGitProbe,
    DesktopSuggestionModelRequest, DesktopSuggestionSessionThreadRef, DesktopSuggestionSourceProbe,
    CONVERSATION_SUGGESTION_SETTINGS_KEY,
};
pub use diagnostics::{Diagnostic, DiagnosticSeverity, DiagnosticStore};
pub use document::{
    document_resource_key, path_from_document_resource_key, DocumentError, DocumentId,
    DocumentSavePlan, DocumentSnapshot, DocumentStore,
};
#[cfg(debug_assertions)]
pub use equivalence::{
    DesktopEquivalenceComposerFact, DesktopEquivalenceConversationFact,
    DesktopEquivalenceConversationSuggestionSettingsFact, DesktopEquivalenceHookHandlerFact,
    DesktopEquivalenceHookSourceFact, DesktopEquivalenceMcpCredentialFact,
    DesktopEquivalenceMcpServerFact, DesktopEquivalenceProjectFact, DesktopEquivalenceSkillFact,
    DesktopEquivalenceSnapshot, DesktopEquivalenceTaskFact, DesktopEquivalenceTimelineFact,
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
    DesktopCredentialImportEntry, DesktopFileDialogRequest, DesktopFileFilter, DesktopHost,
    DesktopHostAction, DesktopHostContext, DesktopHostError, DesktopHostResult, DesktopSecret,
    DesktopShortcutAction, DesktopSingleInstanceRequest, DesktopTrayAction, DesktopTrayItem,
    DesktopUpdateAction, DesktopUpdateResult, DesktopWindowAction, HostCredentialImportResult,
};
pub use iab::{
    DesktopIabSnapshot, DesktopIabSnapshotInput, DesktopIabSnapshotStatus, DesktopIabSubmission,
};
pub use import::{
    CredentialImportDecision, DesktopDataImportService, DesktopDatabaseKind, DesktopImportError,
    DesktopImportErrorCode, DesktopImportExecutionOptions, DesktopImportFile,
    DesktopImportFileMetadata, DesktopImportFileRole, DesktopImportItemError,
    DesktopImportItemKind, DesktopImportPlan, DesktopImportPlanItem, DesktopImportPlanItemStatus,
    DesktopImportPlanStatus, DesktopImportReport, DesktopImportReportItem,
    DesktopImportReportItemStatus, DesktopImportReportStatus, DesktopLegacyConfigurationImport,
};
pub use language::{LanguageDefinition, LanguageId, LanguageRegistry, LanguageRegistryError};
pub use language_service::{
    DesktopDocumentDefinitionResult, DesktopDocumentDefinitionTarget,
    DesktopDocumentDiagnosticsSnapshot, DesktopDocumentDiagnosticsState,
};
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
pub use lilia_feature_memory::{
    DesktopMemory, DesktopMemoryError, DesktopMemoryService, InMemoryMemorySettingsStore,
    MemoryInjectionState, MemoryScope, MemorySettings, MemorySettingsStore, MemoryStore,
    MemoryStoreError, MemoryUpsertInput, SqliteMemoryStore, MEMORY_SETTINGS_KEY,
};
pub use panel::{
    default_panel_states, DockSlot, PaneId, PaneNode, PanelId, PanelLayoutError,
    PanelLayoutSnapshot, PanelState, SplitAxis, WorkspaceItemId, CODING_TOOLS_PANEL_ID,
    DIAGNOSTICS_PANEL_ID, IAB_PANEL_ID, RESOURCES_PANEL_ID, TASK_INSPECTOR_PANEL_ID,
};
pub use plugins::{DesktopPluginInstall, DesktopPluginPackageView};
pub use popup_settings::{
    DesktopPopupSettingsError, DesktopPopupWindowSettings, POPUP_LAST_PROJECT_KEY,
    POPUP_WINDOW_SETTINGS_KEY,
};
pub use lilia_feature_task::{
    DesktopOptionalTextUpdate, DesktopProjectCreate, DesktopProjectPatch,
    DesktopProjectRemovalPreview, DesktopTaskCreate, DesktopTaskMove, DesktopTaskPatch,
    DesktopTaskRunBlock,
};
pub use project::{ProjectContext, ProjectContextError};
pub use project_files::{
    ProjectFileEntry, ProjectFileKind, ProjectFilesError, ProjectFilesSnapshot,
    ProjectFilesViewState,
};
pub use project_settings::{
    default_worktree_auto_instructions, DesktopProjectSettings, DesktopProjectSettingsError,
    DesktopWorktreeSelectionMode, DesktopWorktreeSettings, PROJECT_SETTINGS_KEY,
};
pub use project_tasks::{
    DesktopProjectTaskCatalog, DesktopProjectTaskError, DesktopProjectTaskLaunch,
    DesktopProjectTaskView,
};
pub use prompt_optimize::{
    DesktopPromptOptimizeInput, DesktopPromptOptimizeResult, DesktopPromptRoute,
};
pub use provider::{
    DesktopAgentRuntimeSettings, DesktopAgentRuntimeSettingsUpdate, DesktopCapabilityLimit,
    DesktopCredentialKind, DesktopCredentialStatus, DesktopCredentialView,
    DesktopProviderCapabilityView, DesktopProviderCredentialImportInput,
    DesktopProviderCredentialInput, DesktopProviderError, DesktopProviderRuntimeState,
    DesktopProviderSnapshot, DesktopProviderView, DesktopRemoteQuotaState,
};
pub use provider_ui_settings::{
    normalize_assistant_ai_settings, normalize_model_feature_settings, normalize_model_pool,
    normalize_router_mode_settings, DesktopAssistantAiConfigurationUpdate,
    DesktopAssistantAiModelPoolItem, DesktopAssistantAiSecretUpdate, DesktopAssistantAiSettings,
    DesktopAssistantAiSettingsUpdate, DesktopModelFeatureChatSettings, DesktopModelFeatureSettings,
    DesktopModelFeatureSettingsUpdate, DesktopModelPresetGroup, DesktopProviderUiSettingsError,
    DesktopRouterModeSettings, DesktopRouterModeSettingsUpdate, ASSISTANT_AI_CREDENTIAL_KEY,
    ASSISTANT_AI_SETTINGS_KEY, MODEL_FEATURE_SETTINGS_KEY, ROUTER_MODE_SETTINGS_KEY,
};
pub use lilia_feature_task::{DesktopTaskScope, ProjectQuery, TaskQuery};
pub use registry_watch::REGISTRY_WATCH_SOURCE;
pub use remote::{
    DesktopRemoteControlError, DesktopRemoteControlService, RemoteCapabilitySet,
    RemoteControlStatus, RemoteEndpointAddress, RemotePairDeviceInput, RemotePairingTicket,
    RemotePeerSummary, RemoteRequestEnvelope, REMOTE_ALPN, REMOTE_MIN_PROTOCOL_VERSION,
    REMOTE_PROTOCOL_VERSION,
};
pub use lilia_feature_roadmap::{
    DesktopRoadmapService, Milestone, MilestoneDueDateUpdate, MilestoneStatus,
    MilestoneUpdatePatch, ProjectRoadmap, RoadmapStore, RoadmapStoreError, SqliteRoadmapStore,
    TaskMilestoneLink,
};
pub use session_search::{DesktopSessionSearchKind, DesktopSessionSearchResult};
pub use slash_command::{
    DesktopSlashCommand, DesktopSlashCommandAction, DesktopSlashCommandExecution,
    DesktopSlashCommandSearchResult, DesktopSlashCommandSource,
};
pub use submission::DesktopSubmissionError;
pub use terminal::{
    DesktopTerminalColor, DesktopTerminalCommand, DesktopTerminalError, DesktopTerminalLaunch,
    DesktopTerminalProcessState, DesktopTerminalRestoration, DesktopTerminalRow,
    DesktopTerminalScope, DesktopTerminalSessionId, DesktopTerminalSnapshot, DesktopTerminalStyle,
    DesktopTerminalStyleSpan,
};
pub use timeline_retry::{timeline_retry_context, DesktopTimelineRetryContext};
pub use title_update::{
    normalize_title, title_event_id, title_system_instruction, DesktopTaskTitleSource,
    DesktopTaskTitleState, DesktopTimelineUpperBound, DesktopTitleUpdateCoordinator,
    DesktopTitleUpdateDecision, DesktopTitleUpdateJob, DesktopTitleUpdateReview,
    DesktopTitleUpdateScheduler, TITLE_MAX_CHARS, TITLE_MIN_CHARS, TITLE_UPDATE_ACTION_KIND,
};
pub use todo::{
    DesktopGuideDispatchResult, DesktopGuideDispatchWindow, DesktopTaskTodo, DesktopTodoCreate,
    DesktopTodoError, DesktopTodoGuideStatus, DesktopTodoPriority, DesktopTodoSource,
    DesktopTodoUpdate,
};
pub use tool_consent::{
    DesktopToolConsent, DesktopToolConsentDecision, DesktopToolConsentError,
    TOOL_CONSENT_INTERACTION_KIND,
};
pub use lilia_feature_agent_session::DesktopTurnQueueError;
pub use usage::{
    DesktopProjectDashboardSummary, DesktopProjectTaskStatusCounts, QuotaUsageBackendSummary,
    QuotaUsageConversationSummary, QuotaUsageCostCoverage, QuotaUsageDailyBucket,
    QuotaUsageProjectSummary, QuotaUsageRecentRecord, QuotaUsageStats, QuotaUsageStatsInput,
    QuotaUsageTokenTotals, QuotaUsageToolSummary,
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
    DOCUMENT_WORKSPACE_ITEM_KIND, MEMORY_WORKSPACE_ITEM_KIND, PROJECT_FILES_WORKSPACE_ITEM_KIND,
    ROADMAP_WORKSPACE_ITEM_KIND, SETTINGS_WORKSPACE_ITEM_KIND, TASK_WORKSPACE_ITEM_KIND,
    TERMINAL_WORKSPACE_ITEM_KIND,
};
pub use worktree::{
    DesktopInitialWorktreeSelection, DesktopTaskWorktree, DesktopWorktreeError,
    DesktopWorktreeListItem, DesktopWorktreeMergeResult, DesktopWorktreeStatus,
};

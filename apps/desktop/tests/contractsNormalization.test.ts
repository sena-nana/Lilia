import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  RUNNER_CONTROL_MESSAGE_TYPES,
  RUNNER_CONTEXT_USAGE_EVENT_TYPE,
  RUNNER_DONE_EVENT_TYPE,
  RUNNER_ERROR_EVENT_TYPE,
  RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
  RUNNER_INTERACTION_REQUEST_EVENT_TYPE,
  RUNNER_INTERRUPT_TURN_CONTROL_TYPE,
  RUNNER_LILIA_IAB_RESULT_CONTROL_TYPE,
  RUNNER_PROMPT_SUGGESTION_EVENT_TYPE,
  RUNNER_QUOTA_USAGE_RESULT_CONTROL_TYPE,
  RUNNER_QUOTA_USAGE_REQUEST_EVENT_TYPE,
  RUNNER_RUNTIME_EVENT_TYPES,
  RUNNER_SETTINGS_UPDATE_CONTROL_TYPE,
  RUNNER_STDIN_PAYLOAD_KEYS,
  RUNNER_STDIN_TURN_KEYS,
  RUNNER_TIMELINE_EVENT_TYPE,
  RUNNER_TODO_LIST_EVENT_TYPE,
  RUNNER_TOOL_USE_EVENT_TYPE,
} from "@lilia/contracts/runnerProtocolContract.mjs";
import {
  AGENT_INTERACTION_KINDS,
  AGENT_INTERACTION_DELETE_SUBAGENT_COMMAND,
  AGENT_INTERACTION_GET_SETTINGS_COMMAND,
  AGENT_INTERACTION_LIST_SUBAGENTS_COMMAND,
  AGENT_INTERACTION_SET_SETTINGS_COMMAND,
  AGENT_INTERACTION_UPSERT_SUBAGENT_COMMAND,
  AGENT_TIMELINE_ACTION_KIND_BY_EVENT_KIND,
  AGENT_TIMELINE_PENDING_INTERACTIONS,
  AGENT_TIMELINE_RUNNING_STATUSES,
  AGENT_TIMELINE_TERMINAL_STATUSES,
  AGENT_TIMELINE_TOOL_WINDOW_KINDS,
  ALLOWED_MODEL_PREFIXES_BY_BACKEND,
  ARCHITECTURE_INTERACTION_KIND,
  ASK_USER_CANCEL_ANSWER_VALUE,
  ASK_USER_CONFIRM_ANSWER_VALUE,
  ASK_USER_INTERACTION_KINDS,
  ASK_USER_INTERACTION_KIND,
  ASK_USER_INTENTS,
  ASK_USER_MODES,
  ASK_USER_CLAUDE_TOOL_NAME,
  ASK_USER_MCP_TOOL_NAME,
  ASK_USER_MULTI_SELECT_MODE,
  ASK_USER_QUESTION_INPUT_SCHEMA,
  ASK_USER_SINGLE_SELECT_MODE,
  ASK_USER_TOOL_NAME,
  ASK_USER_TOOL_NAMES,
  CLI_PROJECT_OPEN_EVENT_NAME,
  CLI_PROJECT_OPEN_CONSUME_PENDING_COMMAND,
  MAX_ASK_USER_HEADER_TEXT,
  MAX_ASK_USER_QUESTION_TEXT,
  MAX_ASK_USER_TOOL_OPTIONS,
  MAX_ASK_USER_TOOL_QUESTIONS,
  MAX_CODEX_REQUEST_USER_INPUT_OPTIONS,
  MAX_CONVERSATION_CONTEXT_MESSAGES,
  MAX_CONVERSATION_CONTEXT_TEXT,
  MIN_ASK_USER_TOOL_OPTIONS,
  AUTO_CONTEXT_THRESHOLDS,
  AUTO_TURN_DECISION_PERMISSION_OPTIONS,
  API_KEY_ENV_BY_BACKEND,
  PROVIDER_STORE_KEY_BY_BACKEND,
  AUTOMATION_DELETE_WORKFLOW_COMMAND,
  AUTOMATION_GET_RUN_COMMAND,
  AUTOMATION_LIST_RUNS_COMMAND,
  AUTOMATION_LIST_WORKFLOWS_COMMAND,
  AUTOMATION_LOGIC_KINDS,
  AUTOMATION_LOGIC_KIND_LABELS,
  AUTOMATION_NODE_KIND_LABELS,
  AUTOMATION_CHANGED_EVENT_NAME,
  AUTOMATION_RUN_EVENT_NAMES,
  AUTOMATION_RUN_FINISHED_EVENT_NAME,
  AUTOMATION_PUBLISH_COMMAND,
  AUTOMATION_RESUME_RUN_COMMAND,
  AUTOMATION_RUN_ONCE_COMMAND,
  AUTOMATION_RUN_STARTED_EVENT_NAME,
  AUTOMATION_RUN_STATUSES,
  AUTOMATION_RUN_STATUS_TONES,
  AUTOMATION_RUN_UPDATED_EVENT_NAME,
  AUTOMATION_WAITING_USER_STATUS,
  AUTOMATION_SCOPE_EVENT_KINDS,
  AUTOMATION_SCOPE_TASK_STATUSES,
  AUTOMATION_TOOL_ACTIONS,
  AUTOMATION_TOOL_ACTION_FIELDS,
  AUTOMATION_TOOL_ACTION_LABELS,
  AUTOMATION_TOOL_PRIORITIES,
  AUTOMATION_TRIGGER_KINDS,
  AUTOMATION_SAVE_DRAFT_COMMAND,
  AUTOMATION_SET_ENABLED_COMMAND,
  AUTOMATION_WORKFLOW_TYPE,
  AUTO_MODEL_BY_BACKEND_AND_TIER,
  AUTO_REASONING_EFFORT_BY_TIER,
  AUTO_RUNTIME_COMMAND_SIGNAL_LABELS,
  AUTO_RUNTIME_COMMAND_TYPES_BY_TIER,
  AUTO_WORKFLOW_TYPES_BY_TIER,
  ASSISTANT_AI_FETCH_MODELS_COMMAND,
  ASSISTANT_AI_GET_CONFIG_COMMAND,
  MODEL_FEATURE_GET_SETTINGS_COMMAND,
  MODEL_FEATURE_LIST_MODEL_OPTIONS_COMMAND,
  MODEL_FEATURE_SET_SETTINGS_COMMAND,
  ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND,
  ASSISTANT_AI_SET_CONFIG_COMMAND,
  ASSISTANT_AI_TEST_CONNECTION_COMMAND,
  BACKEND_REASONING_EFFORTS,
  API_DESCRIPTION_BY_BACKEND,
  AGENT_TIMELINE_BATCH_EVENT_NAME,
  AGENT_TIMELINE_EVENT_NAME,
  CHAT_BACKENDS,
  CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME,
  AGENT_TIMELINE_CLEAR_TASK_COMMAND,
  AGENT_TIMELINE_LIST_COMMAND,
  CHAT_BACKEND_LABELS,
  CHAT_ACK_RESTORED_ROLLBACK_COMMAND,
  CHAT_CHECK_ENV_COMMAND,
  CHAT_CONTEXT_USAGE_EVENT_NAME,
  CHAT_DESCRIBE_ATTACHMENTS_COMMAND,
  CHAT_DONE_EVENT_NAME,
  CHAT_GET_COMPOSER_STATE_COMMAND,
  CHAT_GET_RUNTIME_SNAPSHOT_COMMAND,
  CHAT_INTERRUPT_TURN_COMMAND,
  CHAT_LIST_MODELS_COMMAND,
  CHAT_READ_CLIPBOARD_FILE_PATHS_COMMAND,
  CHAT_RESPOND_AGENT_INTERACTION_COMMAND,
  CHAT_RESPOND_TITLE_UPDATE_COMMAND,
  CHAT_SAVE_CLIPBOARD_IMAGE_COMMAND,
  CHAT_SAVE_CLIPBOARD_TEXT_COMMAND,
  CHAT_SEARCH_CONTEXT_ATTACHMENTS_COMMAND,
  CHAT_SEARCH_SLASH_COMMANDS_COMMAND,
  CHAT_SEND_MESSAGE_COMMAND,
  CHAT_SET_COMPOSER_STATE_COMMAND,
  CHAT_TURN_STARTED_EVENT_NAME,
  CONNECTION_MODES,
  CONNECTION_MODES_USING_API_KEY,
  CONNECTION_MODES_USING_CUSTOM_URL,
  CONNECTION_MODES_USING_DEFAULT_API,
  CHAT_WORKFLOW_SLASH_COMMANDS,
  CHAT_SLASH_COMMAND_WORKFLOW_TYPE,
  LILIA_IAB_OPEN_COMMAND,
  CONVERSATION_CONTEXT_CLAUDE_TOOL_NAME,
  CONVERSATION_CONTEXT_MCP_TOOL_NAME,
  CONVERSATION_CONTEXT_TOOL_NAME,
  CONVERSATION_CONTEXT_TOOL_NAMES,
  CODEX_REASONING_EFFORTS,
  CODEX_RATE_LIMIT_RESET_CREDIT_CONSUME_OUTCOMES,
  CODEX_RATE_LIMIT_RESET_CREDIT_CONSUME_OUTCOME_LABELS,
  CODEX_SETTINGS_PROFILES,
  DIRECT_DEFAULT_URLS,
  DEFAULT_AGENT_INTERACTION_SETTINGS,
  DEFAULT_PERMISSION_MODE_AVAILABILITY,
  DEFAULT_ASK_USER_MODE,
  DEFAULT_ASK_USER_MODE_WITH_OPTIONS,
  DEFAULT_AUTOMATION_AGENT_PROMPT,
  DEFAULT_AUTOMATION_HUMAN_PROMPT,
  DEFAULT_CHAT_BACKEND,
  MAIN_NAVIGATE_EVENT_NAME,
  DEFAULT_CONVERSATION_CONTEXT_INCLUDE_MESSAGES,
  DEFAULT_AUTOMATION_LOGIC_KIND,
  DEFAULT_AUTOMATION_LOGIC_PATH,
  DEFAULT_AUTOMATION_RUN_STATUS,
  DEFAULT_AUTOMATION_TRIGGER_KIND,
  DEFAULT_LILIA_GOAL_STATUS,
  DEFAULT_LILIA_CONFIG_DIAGNOSTICS_INCLUDE_LAYERS,
  DEFAULT_LILIA_FIX_SUGGESTION_MODE,
  DEFAULT_LILIA_REVIEW_DELIVERY,
  DEFAULT_MEMORY_SETTINGS,
  DEFAULT_MEMORY_SCOPE,
  DEFAULT_MODEL_BY_BACKEND,
  DEFAULT_MILESTONE_STATUS,
  DEFAULT_CODEX_RATE_LIMIT_RESET_CREDIT_CONSUME_OUTCOME_LABEL,
  DEFAULT_QUOTA_USAGE_QUERY_SCOPE,
  DEFAULT_ROUTER_MODE_BY_BACKEND,
  DEFAULT_QUOTA_USAGE_STATS_DAYS,
  DEFAULT_SESSION_FORK_EXCLUDE_TURNS,
  DEFAULT_SUGGESTION_SOURCE,
  DEFAULT_SUGGESTION_STATUS,
  CONVERSATION_SUGGESTIONS_GET_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_SETTINGS_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND,
  CONVERSATION_SUGGESTIONS_SET_SETTINGS_COMMAND,
  DEFAULT_SESSION_MANAGEMENT_ARCHIVED,
  DEFAULT_SESSION_MANAGEMENT_LIMIT,
  GIT_CLONE_REPO_COMMAND,
  GITHUB_CLONE_REPO_COMMAND,
  GITHUB_GET_BINDING_STATUS_COMMAND,
  GITHUB_LIST_REPOS_COMMAND,
  GITHUB_POLL_DEVICE_FLOW_COMMAND,
  GITHUB_START_DEVICE_FLOW_COMMAND,
  GITHUB_UNBIND_COMMAND,
  HOOK_BACKENDS,
  HOOK_SCOPES,
  HOOK_SOURCE_EDIT_LABELS,
  HOOK_SOURCE_FORMATS,
  HOOK_SOURCE_STATE_LABELS,
  HOOK_TRUST_STATES,
  LILIA_GOAL_ACTIONS,
  LILIA_GOAL_STATUSES,
  LILIA_GOAL_WORKFLOW_TYPE,
  LILIA_QUERY_WORKFLOW_TYPES,
  LILIA_BACKGROUND_TERMINALS_CLEAN_WORKFLOW_TYPE,
  LILIA_COMPACT_WORKFLOW_TYPE,
  LILIA_CONFIG_DIAGNOSTICS_WORKFLOW_TYPE,
  LILIA_BATCH_APPLY_SOURCE_KINDS,
  LILIA_BATCH_APPLY_WORKFLOW_TYPE,
  LILIA_FIX_SUGGESTION_MODES,
  LILIA_FIX_SUGGESTION_WORKFLOW_TYPE,
  LILIA_MEMORY_MODES,
  LILIA_MEMORY_MODE_WORKFLOW_TYPE,
  LILIA_MEMORY_RESET_WORKFLOW_TYPE,
  LILIA_IAB_SUBMIT_COMMAND,
  LILIA_REVIEW_DELIVERIES,
  LILIA_REVIEW_TARGET_TYPES,
  LILIA_REVIEW_WORKFLOW_TYPE,
  LILIA_TASK_WORKFLOW_KINDS,
  LILIA_TASK_WORKFLOW_TYPE,
  MCP_ELICITATION_INTERACTION_KIND,
  MILESTONE_CREATE_COMMAND,
  MILESTONE_DELETE_COMMAND,
  MILESTONE_LIST_COMMAND,
  MILESTONE_REORDER_COMMAND,
  MILESTONE_SET_TASKS_COMMAND,
  MILESTONE_STATUSES,
  MILESTONE_UPDATE_COMMAND,
  MEMORY_DELETE_COMMAND,
  MEMORY_GET_INJECTION_STATE_COMMAND,
  MEMORY_GET_SETTINGS_COMMAND,
  MEMORY_LIST_COMMAND,
  MEMORY_RESET_TASK_COOLDOWN_COMMAND,
  MEMORY_SCOPES,
  MEMORY_SET_ENABLED_COMMAND,
  MEMORY_SET_SETTINGS_COMMAND,
  MEMORY_SET_TASK_ENABLED_COMMAND,
  MEMORY_UPSERT_COMMAND,
  MEMORY_SCOPE_DISPLAY_SPECS,
  MODEL_OPTIONS_BY_BACKEND,
  enabledPermissionModes,
  PERMISSION_MODE_DISPLAY,
  PERMISSION_MODE_DISPLAY_ORDER,
  PERMISSION_MODES,
  LOCKED_PERMISSION_MODES,
  PENDING_AUTO_DECISION_KINDS,
  PENDING_AUTO_DECISION_LABELS,
  PLAN_APPROVAL_INTERACTION_KIND,
  PLAN_APPROVAL_REVISION_REQUEST_ANSWER_VALUE,
  PLAN_APPROVAL_SPEC_DEFAULTS,
  PLUGIN_BACKENDS,
  PLUGINS_CREATE_HOOK_SOURCE_COMMAND,
  PLUGINS_CREATE_MCP_SERVER_COMMAND,
  PLUGINS_CREATE_SKILL_COMMAND,
  PLUGINS_DELETE_HOOK_SOURCE_COMMAND,
  PLUGINS_DELETE_MCP_SERVER_COMMAND,
  PLUGINS_DELETE_SKILL_COMMAND,
  PLUGINS_HOOKS_OVERVIEW_COMMAND,
  PLUGINS_OPEN_HOOK_CONFIG_COMMAND,
  PLUGINS_OPEN_MCP_CONFIG_COMMAND,
  PLUGINS_OVERVIEW_COMMAND,
  PLUGINS_READ_HOOK_SOURCE_COMMAND,
  PLUGINS_SET_HOOK_SOURCE_ENABLED_COMMAND,
  PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND,
  PLUGINS_SET_PACKAGE_ENABLED_COMMAND,
  PLUGINS_SET_SKILL_ENABLED_COMMAND,
  PLUGINS_UPDATE_HOOK_SOURCE_COMMAND,
  PLUGINS_UPDATE_MCP_SERVER_COMMAND,
  POPUP_FOCUS_MAIN_COMMAND,
  POPUP_GET_WINDOW_SETTINGS_COMMAND,
  POPUP_OPEN_CHILD_QUESTION_COMMAND,
  POPUP_OPEN_CONVERSATION_STATUS_COMMAND,
  POPUP_OPEN_NEW_CHAT_COMMAND,
  POPUP_OPEN_TASK_COMMAND,
  POPUP_REMEMBER_LAST_PROJECT_COMMAND,
  POPUP_SET_WINDOW_SETTINGS_COMMAND,
  POPUP_TOGGLE_CONVERSATION_STATUS_COMMAND,
  POPUP_NAVIGATE_EVENT_NAME,
  PLUGIN_MCP_TRANSPORTS,
  PLUGIN_SCOPES,
  PROVIDER_GET_ACTIVE_BACKEND_COMMAND,
  PROVIDER_GET_CONFIG_COMMAND,
  PROVIDER_SET_ACTIVE_BACKEND_COMMAND,
  PROVIDER_SET_CONFIG_COMMAND,
  PROJECT_ARCHITECTURE_APPLIED_INTERACTION_STATUS,
  PROJECT_CREATE_COMMAND,
  PROJECT_DASHBOARD_LIST_COMMAND,
  PROJECT_ENSURE_FOLDERS_COMMAND,
  PROJECT_DASHBOARD_STATUS_ORDER,
  PROJECT_GET_COMMAND,
  PROJECT_GET_SETTINGS_COMMAND,
  PROJECT_LIST_COMMAND,
  PROJECT_REMOVE_COMMAND,
  PROJECT_RENAME_COMMAND,
  PROJECT_REORDER_COMMAND,
  PROJECT_SET_SETTINGS_COMMAND,
  PROJECT_TOGGLE_PIN_COMMAND,
  PROJECT_ARCHITECTURE_BACKENDS,
  PROJECT_ARCHITECTURE_CHANGE_STATUSES,
  PROJECT_ARCHITECTURE_DEFAULT_CHANGE_STATUS,
  PROJECT_ARCHITECTURE_DEFAULT_EDGE_TYPE,
  PROJECT_ARCHITECTURE_DEFAULT_INTERACTION_STATUS,
  PROJECT_ARCHITECTURE_DEFAULT_NODE_TYPE,
  PROJECT_ARCHITECTURE_DEFAULT_PERMISSION,
  PROJECT_ARCHITECTURE_APPLY_COMMAND,
  PROJECT_ARCHITECTURE_GET_COMMAND,
  PROJECT_ARCHITECTURE_LIST_CHANGES_COMMAND,
  PROJECT_ARCHITECTURE_PERMISSIONS,
  PROJECT_ARCHITECTURE_READONLY_INTERACTION_STATUS,
  PROJECT_ARCHITECTURE_REJECT_COMMAND,
  PROJECT_ARCHITECTURE_ROLLBACK_COMMAND,
  PROJECT_ARCHITECTURE_ROLLBACK_PERMISSION,
  PROJECT_ARCHITECTURE_RUNTIME_PERMISSION_MAP,
  ARCHITECTURE_CLAUDE_TOOL_NAME,
  ARCHITECTURE_MCP_TOOL_NAME,
  ARCHITECTURE_TOOL_NAME,
  ARCHITECTURE_TOOL_NAMES,
  UPDATE_PROJECT_ARCHITECTURE_INPUT_SCHEMA,
  PROJECT_DASHBOARD_ACTIVE_STATUSES,
  PROJECT_DASHBOARD_BLOCKED_STATUSES,
  PROJECT_ROADMAP_STATUS_ORDER,
  PERMISSION_APPROVAL_INTERACTION_KIND,
  QUOTA_USAGE_STATS_BACKEND_EXTRA_FILTERS,
  QUOTA_USAGE_STATS_BACKEND_FILTERS,
  QUOTA_USAGE_STATS_BACKEND_FILTER_LABELS,
  QUOTA_USAGE_QUERY_SCOPES,
  QUOTA_USAGE_STATS_DAYS,
  QUOTA_USAGE_CLAUDE_TOOL_NAME,
  QUOTA_USAGE_GET_STATS_COMMAND,
  QUOTA_USAGE_MCP_TOOL_NAME,
  QUOTA_USAGE_TOOL_NAME,
  QUOTA_USAGE_TOOL_NAMES,
  QUERY_CONVERSATION_CONTEXT_INPUT_SCHEMA,
  QUERY_QUOTA_USAGE_INPUT_SCHEMA,
  REASONING_EFFORTS,
  ROUTER_GET_MODE_COMMAND,
  ROUTER_MODES_BY_BACKEND,
  ROUTER_STORE_KEY_BY_BACKEND,
  ROUTER_MODE_LABELS,
  ROUTER_MODES_USING_API_CONFIG,
  ROUTER_SET_MODE_COMMAND,
  RUNTIME_INTERACTION_KINDS,
  RUNTIME_SETTINGS_ACTIONS,
  RUNTIME_SETTINGS_COMMAND_TYPE,
  REMOTE_ENVIRONMENT_ACTIONS,
  REMOTE_ENVIRONMENT_COMMAND_TYPE,
  MAX_SESSION_MANAGEMENT_LIMIT,
  SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
  SESSION_MANAGEMENT_ACTIONS,
  SESSION_FORK_COMMAND_TYPE,
  SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE,
  SUGGESTION_ITEM_SOURCES,
  SUGGESTION_LOADING_SOURCE_LABELS,
  SUGGESTION_LOCAL_GIT_LOADING_LABELS,
  SUGGESTION_SOURCE_LABELS,
  SUGGESTION_SOURCE_SETTING_ORDER,
  SUGGESTION_SOURCES,
  SUGGESTION_STATUSES,
  SYSTEM_OPEN_IN_VSCODE_COMMAND,
  SYSTEM_OPEN_PATH_COMMAND,
  SYSTEM_OPEN_URL_COMMAND,
  TASK_ARCHIVE_COMMAND,
  TASK_ARCHIVE_PROJECT_COMMAND,
  TASK_CREATE_COMMAND,
  TASK_DELETE_COMMAND,
  TASK_GET_COMMAND,
  TASK_LIST_COMMAND,
  TASK_LIST_SIDEBAR_CONVERSATIONS_COMMAND,
  TASK_PROMOTE_COMMAND,
  TASK_REORDER_COMMAND,
  TASK_REPARENT_COMMAND,
  TASKS_CHANGED_EVENT_NAME,
  TASK_STATUSES,
  TASK_TOGGLE_PIN_COMMAND,
  TASK_UPDATE_COMMAND,
  TIMELINE_DISPLAY_ALLOWED_PROMPT_TEXT_LIMIT,
  TIMELINE_DISPLAY_ASK_USER_HEADER_TEXT_LIMIT,
  TIMELINE_DISPLAY_ASK_USER_QUESTION_PREVIEW_TEXT_LIMIT,
  TIMELINE_DISPLAY_CLAUDE_PLAN_TEXT_LIMIT,
  TIMELINE_DISPLAY_COMMAND_INLINE_THRESHOLD,
  TIMELINE_DISPLAY_DETAIL_TEXT_LIMIT,
  TIMELINE_DISPLAY_ERROR_SUMMARY_TEXT_LIMIT,
  TIMELINE_DISPLAY_FILE_CHANGE_PATH_TEXT_LIMIT,
  TIMELINE_DISPLAY_INLINE_TEXT_LIMIT,
  TIMELINE_DISPLAY_ONE_LINE_TEXT_LIMIT,
  TIMELINE_DISPLAY_PATH_PREVIEW_TEXT_LIMIT,
  TIMELINE_DISPLAY_SHORT_TEXT_LIMIT,
  TIMELINE_DISPLAY_SUMMARY_TEXT_LIMIT,
  TIMELINE_DISPLAY_TEXT_LIMITS,
  TIMELINE_DISPLAY_TITLE_TEXT_LIMIT,
  TIMELINE_DISPLAY_TINY_TEXT_LIMIT,
  TIMELINE_DISPLAY_TODO_ITEM_TEXT_LIMIT,
  TIMELINE_DISPLAY_TODO_STEP_TEXT_LIMIT,
  TITLE_UPDATE_ACTION_KIND,
  TIMELINE_PAYLOAD_MAX_DEPTH,
  TIMELINE_PAYLOAD_RESERVED_KEYS,
  TIMELINE_STATUS_ALIASES,
  TODO_APPLY_AGENT_EVENT_COMMAND,
  TODO_CHANGED_EVENT_NAME,
  TODO_CREATE_COMMAND,
  TODO_DELETE_COMMAND,
  TODO_LIST_COMMAND,
  TODO_UPDATE_COMMAND,
  TASK_TODO_GUIDE_STATUSES,
  TASK_TODO_PRIORITIES,
  TOOL_CONSENT_INTERACTION_KIND,
  UNCONFIGURED_CONNECTION_MODES,
  DEFAULT_TASK_TODO_PRIORITY,
  DEFAULT_TIMELINE_STATUS,
  PENDING_TASK_TODO_GUIDE_STATUS,
  PLAN_APPROVAL_INTENT,
  QUEUED_TASK_TODO_GUIDE_STATUS,
  SENT_TASK_TODO_GUIDE_STATUS,
  agentTimelineActionDescriptor,
  agentTimelineActionMatches,
  agentTimelineActionRequestId,
  agentTimelineActionRequestIds,
  agentTimelineActionKind,
  agentTimelineEventRequiresAction,
  agentTimelineEventRequestId,
  agentTimelinePendingInteraction,
  agentTimelinePlanAllowedPromptRows,
  agentTimelinePlanArchitectureImpactRows,
  agentTimelinePayloadRecord,
  agentTimelinePayloadValueRecord,
  aggregateTimelineStatus,
  allowedModelPrefixesForBackend,
  apiKeyEnvForBackend,
  apiDescriptionForBackend,
  autoContextThresholdsForScale,
  automationLogicKindLabel,
  automationNodeKindLabel,
  automationRunStatusTone,
  askUserInteractionKindForSpec,
  askUserSpecKey,
  createPlanApprovalAskUserSpec,
  automationToolActionLabel,
  automationToolActionUsesField,
  automationTriggerKindLabel,
  autoModelForBackendTier,
  autoModelForBackendPreset,
  autoPresetForWorkflowType,
  autoReasoningEffortForPreset,
  autoReasoningEffortForTier,
  autoRuntimeCommandSignalLabel,
  autoTierForRuntimeCommandType,
  autoTierForWorkflowType,
  normalizeModelFeatureSettings,
  PLAN_MODE_PRESET,
  chatAttachmentReferenceLabel,
  chatAttachmentReferencePattern,
  chatAttachmentDirectorySummaryLabel,
  chatAttachmentMetaLabel,
  chatAttachmentSizeLabel,
  chatBackendLabel,
  chatSlashCommandSourceLabel,
  chatWorkflowSlashKindLabel,
  chatAttachmentsToPayload,
  clampPercent,
  codexAccountQuotaCreditsLabel,
  codexAccountQuotaPercentLabel,
  codexAccountQuotaWindowRemainingLabel,
  codexAccountQuotaWindowRemainingLine,
  codexAccountQuotaWindowRemainingPercent,
  codexAccountQuotaWindowShortLabel,
  codexRateLimitResetCreditConsumeOutcomeLabel,
  createLiliaCodeCoreCodexQuotaUnavailableStatus,
  connectionDiagnostic,
  connectionModeUsesApiKey,
  connectionModeUsesCodexAccount,
  connectionModeUsesCustomUrl,
  connectionModeUsesDefaultApi,
  connectionModeIsUnconfigured,
  conversationReferencePattern,
  conversationReferencesToPayload,
  createAutomationChangedEvent,
  createAutomationRunEvent,
  createChatBackendRecord,
  createAppNavigateEvent,
  createChatDoneEvent,
  createChatTurnStartedEvent,
  createCliProjectOpenEvent,
  countProjectTaskStatuses,
  deriveProjectDashboardCounts,
  createAutomationRunWorkflow,
  createChatSlashCommandWorkflow,
  createUpdatedToolConsentCommandInput,
  createEmptyProjectTaskStatusCounts,
  createMcpElicitationResult,
  createLiliaBatchApplyWorkflow,
  createLiliaCompactWorkflow,
  createLiliaFixSuggestionWorkflow,
  createLiliaGoalWorkflow,
  createLiliaReviewWorkflow,
  createLiliaTaskWorkflow,
  createMemoryUpsertInput,
  createPermissionApprovalResult,
  createRemoteEnvironmentCommand,
  createRuntimeSettingsCommand,
  createSandboxDiagnosticsCommand,
  createSessionManagementRuntimeCommand,
  createSessionForkCommand,
  createTasksChangedEvent,
  createTodoChangedEvent,
  defaultModelForBackend,
  defaultRouterModeForBackend,
  deriveChatMessageDisplay,
  directDefaultUrlForBackend,
  hookScopeLabel,
  hookSourceEditLabel,
  hookSourceFormatLabel,
  hookSourceStateLabel,
  hookTrustStateLabel,
  isAgentInteractionKind,
  isAskUserInteractionKind,
  isAgentTimelineRunningStatus,
  isAgentTimelineTerminalStatus,
  isAgentTimelineToolWindowKind,
  isAgentTimelinePendingInteraction,
  isAutomationLogicKind,
  isAutomationRunStatus,
  isAskUserIntent,
  isAskUserMode,
  isChatBackendKind,
  isChatAttachment,
  isChatImageAttachment,
  isChatSlashCommandSource,
  isLargeChatAttachmentDirectory,
  isCodexReasoningEffort,
  isCodexRateLimitResetCreditConsumeOutcome,
  isCodexSettingsProfile,
  isConnectionMode,
  isHookBackendKind,
  isHookScope,
  isHookSourceFormat,
  isHookTrustState,
  isLiliaBackgroundTerminalsCleanWorkflow,
  isLiliaCompactWorkflow,
  isLiliaBatchApplySourceKind,
  isLiliaFixSuggestionMode,
  isLiliaGoalAction,
  isLiliaGoalStatus,
  isLiliaMemoryMode,
  isLiliaMemoryResetWorkflow,
  isLiliaQueryWorkflowType,
  isLiliaReviewDelivery,
  isLiliaReviewTargetType,
  isLiliaTaskWorkflowKind,
  isLiliaAskUserTool,
  isLiliaArchitectureTool,
  isLiliaConversationContextTool,
  isLiliaQuotaTool,
  isMilestoneStatus,
  isMemoryScope,
  isPendingAutoDecisionKind,
  isPermissionMode,
  isPluginBackendKind,
  isPluginScope,
  isQuotaUsageStatsBackendExtraFilter,
  isQuotaUsageStatsBackendFilter,
  isQuotaUsageStatsDays,
  isReasoningEffort,
  isRouterModeForBackend,
  isSuggestionItemSource,
  isSuggestionSource,
  isSuggestionStatus,
  isSessionManagementAction,
  isRuntimeSettingsAction,
  isRemoteEnvironmentAction,
  isTaskStatus,
  isTaskTodoGuideStatus,
  isTaskTodoPriority,
  latestLiliaGoalFromTimeline,
  normalizeAgentInteractionSettings,
  normalizeAgentInteractionRequest,
  normalizeAskUserIntent,
  normalizeAskUserMode,
  normalizeAskUserSpec,
  normalizeAutomationLogicKind,
  normalizeAutomationRunStatus,
  normalizeAutomationScope,
  normalizeAutomationToolAction,
  normalizeAutomationToolPriority,
  normalizeAutomationRunWorkflow,
  normalizeChatBackendKind,
  normalizeChatSlashCommandWorkflow,
  normalizeCodexProfileSettings,
  normalizeLiliaCodeCoreCodexProfileSettings,
  normalizeLiliaCodeCoreCodexQuotaStatus,
  normalizeLiliaConfigDiagnosticsWorkflow,
  normalizeLiliaBatchApplyWorkflow,
  normalizeLiliaFixSuggestionMode,
  normalizeLiliaFixSuggestionWorkflow,
  normalizeLiliaGoalStatus,
  normalizeLiliaGoalWorkflow,
  normalizeLiliaMemoryModeWorkflow,
  normalizeLiliaReviewDelivery,
  normalizeLiliaReviewTarget,
  normalizeLiliaReviewWorkflow,
  normalizeLiliaTaskWorkflow,
  normalizeMemoryScope,
  normalizeMemorySettings,
  normalizeMemoryTags,
  normalizeMcpElicitationPayload,
  normalizeMcpElicitationResult,
  normalizeMilestoneStatus,
  normalizePermissionApprovalPayload,
  normalizePermissionApprovalResult,
  normalizeAvailablePermissionMode,
  normalizePermissionModeAvailability,
  normalizePermissionMode,
  normalizeQuotaUsageQueryScope,
  normalizeReasoningEffort,
  normalizeReasoningEffortForBackend,
  normalizeRouterModeForBackend,
  normalizeRemoteEnvironmentCommand,
  normalizeRuntimeInteractionResult,
  normalizeRuntimeSettingsCommand,
  normalizeSandboxDiagnosticsCommand,
  normalizeSessionManagementRuntimeCommand,
  normalizeSessionForkCommand,
  normalizeSuggestionSource,
  normalizeSuggestionStatus,
  normalizeTimelineStatus,
  pluginBackendLabel,
  pluginEnabledLabel,
  pluginMcpBackendLabel,
  pluginMcpTransportLabel,
  pluginToggleActionLabel,
  quotaUsageStatsBackendFilterLabel,
  quotaUsageStatsDaysLabel,
  routerModesForBackend,
  routerModeLabel,
  routerModeUsesCodexAccount,
  routerModeUsesApiConfig,
  suggestionSourceSettingLabel,
  reasoningEffortsForBackend,
  normalizeProjectArchitectureChange,
  normalizeProjectArchitectureChangeStatus,
  normalizeProjectArchitectureInteractionPayload,
  normalizeProjectArchitecturePermission,
  projectArchitecturePermissionFromRuntimePermission,
  projectArchitectureChangeText,
  projectArchitectureChangeTextFromPayload,
  projectArchitectureApplyInputFromInteraction,
  projectArchitectureRejectInputFromInteraction,
  isProjectArchitectureChangeStatus,
  isProjectArchitecturePermission,
  isPlanApprovalConfirmAskUserSpec,
  isPlanApprovalAskUserSpec,
  isHiddenTimelineEvent,
  isTimelineErrorReply,
  isTimelineFinalReply,
  isTimelineFinalReplyStreaming,
  isTimelineInterruptEvent,
  isTimelineMessageEvent,
  isTimelineProcessAnchor,
  isTimelineUserMessage,
  liliaGoalStatusLabel,
  memoryScopeEmptyLabel,
  memoryScopeLabel,
  modelBelongsToBackend,
  mergeModelSelectionRuntimeOptions,
  modelOptionsForBackend,
  normalizeToolConsentRequestFromInteraction,
  normalizeTaskTodoGuideStatus,
  normalizeTaskTodoPriority,
  pendingAutoDecisionLabel,
  readAgentTimelinePayloadString,
  readChatAttachmentPayloadPaths,
  readChatAttachments,
  readConversationReferences,
  readEditableToolConsentCommand,
  readTimelineFinalText,
  resolveConversationReferenceMatch,
  runtimeOptionsModelForBackend,
  runtimeOptionsReasoningEffortForBackend,
  runtimeDiagnostic,
  resolveChatAttachmentReferenceMatch,
  serializeConversationReference,
  serializeChatAttachmentReference,
  stripSerializedConversationReferences,
  conversationSuggestionLoadingText,
  suggestionGitHubActivityAnchor,
  suggestionSourceLabel,
  suggestionStatusDisplayText,
  stringifyCodexToolCommand,
  milestoneStatusLabel,
  taskStatusLabel,
  taskTodoPriorityLabel,
  timelineActionStatusLabel,
  timelineEventScopedKey,
  timelineFinalReplyBatchApplyInput,
  timelineChatMessageFromEvent,
  timelineDeclaredGroupUnit,
  timelineDeclaredGroupUnitFromDisplay,
  timelineEventLabelFromDisplay,
  timelineGroupLabelFromDisplay,
  timelineLatestEventTimeMs,
  timelinePlanStatusKind,
  timelinePlanStatusLabel,
  timelineProcessEventsSummary,
  timelineRunningSubagentLabel,
  timelineThinkingDurationLabel,
  titleUpdateTimelinePayload,
} from "@lilia/contracts";

const repoRoot = join(process.cwd(), "../..");

function collectContractCommandStrings(
  value: unknown,
  out = new Set<string>(),
  path: string[] = [],
): Set<string> {
  if (typeof value === "string") {
    const key = path.at(-1) ?? "";
    const isCommandField = path.includes("commands") || key.endsWith("Command");
    if (isCommandField && /^[a-z][a-z0-9_]+$/.test(value)) out.add(value);
    return out;
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) => collectContractCommandStrings(item, out, [...path, String(index)]));
    return out;
  }
  if (value && typeof value === "object") {
    Object.entries(value).forEach(([key, item]) => collectContractCommandStrings(item, out, [...path, key]));
  }
  return out;
}

function registeredTauriCommandNames(): string[] {
  const lib = readFileSync(join(repoRoot, "apps/desktop/src-tauri/src/lib.rs"), "utf8");
  return [...lib.matchAll(/^\s+([a-zA-Z_][a-zA-Z0-9_:]*),\s*$/gm)]
    .map((match) => match[1].split("::").at(-1) ?? "")
    .filter((name) => name && name !== "app" && name !== "event" && name !== "ping")
    .sort();
}

function contractCommandValues(): Set<string> {
  const contractsDir = join(repoRoot, "packages/contracts/src");
  const values = new Set<string>();
  for (const file of readdirSync(contractsDir)) {
    if (!file.endsWith("contract.json")) continue;
    collectContractCommandStrings(
      JSON.parse(readFileSync(join(contractsDir, file), "utf8")) as unknown,
      values,
    );
  }
  return values;
}

function sourceFilePaths(relativeDir: string, extensions: Set<string>, out: string[] = []): string[] {
  const dir = join(repoRoot, relativeDir);
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === "dist" || entry.name === "target" || entry.name === "node_modules") {
      continue;
    }
    const fullPath = join(dir, entry.name);
    if (entry.isDirectory()) {
      sourceFilePaths(`${relativeDir}/${entry.name}`, extensions, out);
      continue;
    }
    if (extensions.has(entry.name.split(".").at(-1) ?? "")) out.push(fullPath);
  }
  return out;
}

function lineNumberAt(text: string, index: number): number {
  return text.slice(0, index).split(/\r?\n/).length;
}

function relativeSourcePath(file: string): string {
  return file.slice(repoRoot.length + 1).replace(/\\/g, "/");
}

function tauriEventLiteralCallSites(): string[] {
  const tsEventCallPattern = /\b(?:listen|emitTauriEvent)\s*\(\s*["']([^"']+)["']/g;
  const rustEventCallPattern = /\.(?:emit|emit_all|emit_to|emit_filter)\s*\(\s*["']([^"']+)["']/g;
  const callSites: string[] = [];

  for (const file of [
    ...sourceFilePaths("apps/desktop/src", new Set(["ts", "tsx", "vue"])),
    ...sourceFilePaths("apps/desktop/tests", new Set(["ts", "tsx"])),
  ]) {
    const text = readFileSync(file, "utf8");
    for (const match of text.matchAll(tsEventCallPattern)) {
      callSites.push(`${relativeSourcePath(file)}:${lineNumberAt(text, match.index ?? 0)}:${match[1]}`);
    }
  }

  for (const file of sourceFilePaths("apps/desktop/src-tauri/src", new Set(["rs"]))) {
    const text = readFileSync(file, "utf8");
    for (const match of text.matchAll(rustEventCallPattern)) {
      callSites.push(`${relativeSourcePath(file)}:${lineNumberAt(text, match.index ?? 0)}:${match[1]}`);
    }
  }

  return callSites.sort();
}

function tauriCommandLiteralCallSites(contractValues: Set<string>): string[] {
  const patterns = [
    /\binvoke(?:<[^>]+>)?\s*\(\s*["']([^"']+)["']/g,
    /\bmockInvoke\s*\(\s*["']([^"']+)["']/g,
    /\btoHaveBeenCalledWith\s*\(\s*["']([^"']+)["']/g,
    /\bcase\s*["']([^"']+)["']\s*:/g,
  ];
  const callSites: string[] = [];

  for (const file of [
    ...sourceFilePaths("apps/desktop/src", new Set(["ts", "tsx", "vue"])),
    ...sourceFilePaths("apps/desktop/tests", new Set(["ts", "tsx"])),
  ]) {
    const text = readFileSync(file, "utf8");
    for (const pattern of patterns) {
      for (const match of text.matchAll(pattern)) {
        const command = match[1];
        if (!contractValues.has(command)) continue;
        callSites.push(`${relativeSourcePath(file)}:${lineNumberAt(text, match.index ?? 0)}:${command}`);
      }
    }
  }

  return callSites.sort();
}

function collectJsonStrings(value: unknown, path: string[] = [], out: string[] = []): string[] {
  if (typeof value === "string") {
    out.push(`${path.join(".") || "$"}=${value}`);
    return out;
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) => collectJsonStrings(item, [...path, String(index)], out));
    return out;
  }
  if (value && typeof value === "object") {
    Object.entries(value).forEach(([key, item]) => collectJsonStrings(item, [...path, key], out));
  }
  return out;
}

function nonContractManifestBoundaryLeaks(commandValues: Set<string>): string[] {
  const contractsDir = join(repoRoot, "packages/contracts/src");
  const eventNamePattern = /^[a-z][a-z0-9_-]*(?::[a-z0-9_-]+)+$/;
  const leaks: string[] = [];

  for (const file of readdirSync(contractsDir)) {
    if (!file.endsWith(".json") || file.endsWith("contract.json")) continue;
    const entries = collectJsonStrings(
      JSON.parse(readFileSync(join(contractsDir, file), "utf8")) as unknown,
    );
    for (const entry of entries) {
      const value = entry.slice(entry.indexOf("=") + 1);
      if (commandValues.has(value) || eventNamePattern.test(value)) {
        leaks.push(`${file}:${entry}`);
      }
    }
  }

  return leaks.sort();
}

function contractTsJsonImportSites(): string[] {
  const importJsonPattern = /\bimport\s+[\s\S]*?\s+from\s+["']\.\/[^"']+\.json["']/g;
  const sites: string[] = [];

  for (const file of sourceFilePaths("packages/contracts/src", new Set(["ts"]))) {
    const text = readFileSync(file, "utf8");
    for (const match of text.matchAll(importJsonPattern)) {
      sites.push(`${relativeSourcePath(file)}:${lineNumberAt(text, match.index ?? 0)}`);
    }
  }

  return sites.sort();
}

function rustContractJsonIncludeSites(): string[] {
  const includeContractJsonPattern = /\binclude_str!\s*\(\s*["'][^"']*packages\/contracts\/src\/[^"']+\.json["']/g;
  const sites: string[] = [];

  for (const file of sourceFilePaths("apps/desktop/src-tauri/src", new Set(["rs"]))) {
    const relative = relativeSourcePath(file);
    const moduleName = relative.split("/").at(-1) ?? "";
    if (moduleName.includes("contract")) continue;
    const text = readFileSync(file, "utf8");
    for (const match of text.matchAll(includeContractJsonPattern)) {
      sites.push(`${relative}:${lineNumberAt(text, match.index ?? 0)}`);
    }
  }

  return sites.sort();
}

function rustContractJsonDirectParserSites(): string[] {
  const directParserPattern = /\bserde_json::from_str\s*\(/g;
  const sites: string[] = [];

  for (const file of sourceFilePaths("apps/desktop/src-tauri/src", new Set(["rs"]))) {
    const text = readFileSync(file, "utf8");
    if (!text.includes("packages/contracts/src")) continue;
    for (const match of text.matchAll(directParserPattern)) {
      sites.push(`${relativeSourcePath(file)}:${lineNumberAt(text, match.index ?? 0)}`);
    }
  }

  return sites.sort();
}

function packageMjsExportIssues(): string[] {
  const packageJson = JSON.parse(
    readFileSync(join(repoRoot, "packages/contracts/package.json"), "utf8"),
  ) as { exports: Record<string, string> };
  const issues: string[] = [];

  for (const [exportName, target] of Object.entries(packageJson.exports)) {
    if (!exportName.endsWith(".mjs") || !target.endsWith(".mjs")) continue;
    const sourcePath = join(repoRoot, "packages/contracts", target);
    if (!existsSync(sourcePath)) {
      issues.push(`${exportName}: missing ${target}`);
      continue;
    }
    const declarationPath = sourcePath.replace(/\.mjs$/, ".d.mts");
    if (!existsSync(declarationPath)) {
      issues.push(`${exportName}: missing ${relativeSourcePath(declarationPath)}`);
    }
  }

  return issues.sort();
}

describe("contracts normalization helpers", () => {
  it("keeps registered Tauri commands covered by contract manifests", () => {
    const contractValues = contractCommandValues();
    const missing = registeredTauriCommandNames().filter((name) => !contractValues.has(name));
    expect(missing).toEqual([]);
  });

  it("keeps contract JSON reads behind runtime contract modules", () => {
    expect(contractTsJsonImportSites()).toEqual([]);
    expect(rustContractJsonIncludeSites()).toEqual([]);
    expect(rustContractJsonDirectParserSites()).toEqual([]);
  });

  it("keeps public contract mjs exports backed by source and declaration files", () => {
    expect(packageMjsExportIssues()).toEqual([]);
  });

  it("keeps Tauri event call sites on named contract constants", () => {
    expect(tauriEventLiteralCallSites()).toEqual([]);
  });

  it("keeps Tauri command call sites on named contract constants", () => {
    expect(tauriCommandLiteralCallSites(contractCommandValues())).toEqual([]);
  });

  it("keeps command and event names out of non-contract manifests", () => {
    expect(nonContractManifestBoundaryLeaks(contractCommandValues())).toEqual([]);
  });

  it("exports runner stdout event types from the protocol contract", () => {
    expect(RUNNER_RUNTIME_EVENT_TYPES).toEqual({
      toolUse: "tool_use",
      todoList: "todo_list",
      timeline: "timeline",
      interactionRequest: "interaction_request",
      quotaUsageRequest: "quota_usage_request",
      contextUsage: "context_usage",
      done: "done",
      promptSuggestion: "prompt_suggestion",
      error: "error",
    });
    expect(RUNNER_TOOL_USE_EVENT_TYPE).toBe("tool_use");
    expect(RUNNER_TODO_LIST_EVENT_TYPE).toBe("todo_list");
    expect(RUNNER_TIMELINE_EVENT_TYPE).toBe("timeline");
    expect(RUNNER_INTERACTION_REQUEST_EVENT_TYPE).toBe("interaction_request");
    expect(RUNNER_QUOTA_USAGE_REQUEST_EVENT_TYPE).toBe("quota_usage_request");
    expect(RUNNER_CONTEXT_USAGE_EVENT_TYPE).toBe("context_usage");
    expect(RUNNER_DONE_EVENT_TYPE).toBe("done");
    expect(RUNNER_PROMPT_SUGGESTION_EVENT_TYPE).toBe("prompt_suggestion");
    expect(RUNNER_ERROR_EVENT_TYPE).toBe("error");
    expect(RUNNER_CONTROL_MESSAGE_TYPES).toEqual({
      interactionResponse: "interaction_response",
      settingsUpdate: "settings_update",
      interruptTurn: "interrupt_turn",
      quotaUsageResult: "quota_usage_result",
      liliaIabResult: "lilia_iab_result",
    });
    expect(RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE).toBe("interaction_response");
    expect(RUNNER_SETTINGS_UPDATE_CONTROL_TYPE).toBe("settings_update");
    expect(RUNNER_INTERRUPT_TURN_CONTROL_TYPE).toBe("interrupt_turn");
    expect(RUNNER_QUOTA_USAGE_RESULT_CONTROL_TYPE).toBe("quota_usage_result");
    expect(RUNNER_LILIA_IAB_RESULT_CONTROL_TYPE).toBe("lilia_iab_result");
    expect(RUNNER_STDIN_PAYLOAD_KEYS).toEqual({
      backend: "backend",
      turn: "turn",
      workflow: "workflow",
      runtimeCommand: "runtimeCommand",
      runtimeOptions: "runtimeOptions",
      extensions: "extensions",
    });
    expect(RUNNER_STDIN_TURN_KEYS).toEqual({
      cwd: "cwd",
      prompt: "prompt",
      attachments: "attachments",
      conversationReferences: "conversationReferences",
      model: "model",
      resumeSessionId: "resumeSessionId",
      planMode: "planMode",
      goalMode: "goalMode",
      permission: "permission",
    });
  });

  it.skip("normalizes chat backends", () => {
    expect(CHAT_BACKENDS).toEqual(["native-agentkit"]);
    expect(DEFAULT_CHAT_BACKEND).toBe("native-agentkit");
    expect(CHAT_BACKEND_LABELS).toEqual({
      "native-agentkit": "Native AgentKit",
    });
    expect(isChatBackendKind("native-agentkit")).toBe(true);
    expect(isChatBackendKind("openai")).toBe(false);
    expect(normalizeChatBackendKind("openai")).toBe("native-agentkit");
    expect(normalizeChatBackendKind("openai", "native-agentkit")).toBe("native-agentkit");
    expect(chatBackendLabel("native-agentkit")).toBe("Native AgentKit");
    expect(DEFAULT_MODEL_BY_BACKEND).toEqual({
      "native-agentkit": "gpt-5.4",
    });
    expect(createChatBackendRecord((backend) => `${backend}:ok`)).toEqual({
      "native-agentkit": "native-agentkit:ok",
    });
    expect(defaultModelForBackend("native-agentkit")).toBe("gpt-5.4");
    expect(modelOptionsForBackend("native-agentkit")).toEqual(
      MODEL_OPTIONS_BY_BACKEND["native-agentkit"],
    );
    expect(ALLOWED_MODEL_PREFIXES_BY_BACKEND).toEqual({
      "native-agentkit": ["gpt-", "o", "claude-"],
    });
    expect(allowedModelPrefixesForBackend("native-agentkit")).toEqual([
      "gpt-",
      "o",
      "claude-",
    ]);
    expect(modelBelongsToBackend("native-agentkit", "gpt-6-preview")).toBe(true);
    expect(modelBelongsToBackend("native-agentkit", "claude-sonnet-4-6")).toBe(true);
    expect(MODEL_OPTIONS_BY_BACKEND["native-agentkit"].map((option) => option.id)).toEqual([
      "gpt-5.5",
      "gpt-5.4",
      "gpt-5.4-mini",
      "claude-opus-4-7",
      "claude-sonnet-4-6",
      "claude-haiku-4-5",
    ]);
    expect(MAX_CONVERSATION_CONTEXT_MESSAGES).toBe(12);
    expect(MAX_CONVERSATION_CONTEXT_TEXT).toBe(1200);
    expect(DEFAULT_CONVERSATION_CONTEXT_INCLUDE_MESSAGES).toBe(true);
    expect(CONVERSATION_CONTEXT_TOOL_NAMES).toEqual([
      "QueryConversationContext",
      "query_conversation_context",
      "mcp__lilia__query_conversation_context",
    ]);
    expect(CONVERSATION_CONTEXT_TOOL_NAME).toBe(CONVERSATION_CONTEXT_TOOL_NAMES[0]);
    expect(CONVERSATION_CONTEXT_CLAUDE_TOOL_NAME).toBe(CONVERSATION_CONTEXT_TOOL_NAMES[1]);
    expect(CONVERSATION_CONTEXT_MCP_TOOL_NAME).toBe(CONVERSATION_CONTEXT_TOOL_NAMES[2]);
    expect(isLiliaConversationContextTool("QueryConversationContext")).toBe(true);
    expect(isLiliaConversationContextTool("query_conversation_context")).toBe(true);
    expect(isLiliaConversationContextTool("mcp__lilia__query_conversation_context")).toBe(true);
    expect(isLiliaConversationContextTool("conversation_context")).toBe(false);
    expect(QUERY_CONVERSATION_CONTEXT_INPUT_SCHEMA).toMatchObject({
      type: "object",
      additionalProperties: false,
      properties: {
        taskId: {
          type: "string",
          description: "Conversation task id to query. Defaults to the parent conversation.",
        },
        includeMessages: {
          type: "boolean",
          default: true,
          description: "Whether to include clipped readable timeline messages.",
        },
      },
    });
    expect(QUOTA_USAGE_STATS_DAYS).toEqual([7, 30]);
    expect(DEFAULT_QUOTA_USAGE_STATS_DAYS).toBe(7);
    expect(quotaUsageStatsDaysLabel(30)).toBe("30 天");
    expect(isQuotaUsageStatsDays(7)).toBe(true);
    expect(isQuotaUsageStatsDays(14)).toBe(false);
    expect(QUOTA_USAGE_STATS_BACKEND_EXTRA_FILTERS).toEqual(["all"]);
    expect(QUOTA_USAGE_STATS_BACKEND_FILTERS).toEqual(["all", "native-agentkit"]);
    expect(QUOTA_USAGE_STATS_BACKEND_FILTER_LABELS).toEqual({ all: "全部" });
    expect(isQuotaUsageStatsBackendExtraFilter("all")).toBe(true);
    expect(isQuotaUsageStatsBackendExtraFilter("codex")).toBe(false);
    expect(isQuotaUsageStatsBackendFilter("all")).toBe(true);
    expect(isQuotaUsageStatsBackendFilter("claude")).toBe(true);
    expect(isQuotaUsageStatsBackendFilter("local")).toBe(false);
    expect(quotaUsageStatsBackendFilterLabel("all")).toBe("全部");
    expect(quotaUsageStatsBackendFilterLabel("claude")).toBe("Claude");
    expect(quotaUsageStatsBackendFilterLabel("codex")).toBe("Codex");
    expect(QUOTA_USAGE_QUERY_SCOPES).toEqual(["summary", "projects", "conversations", "tools", "all"]);
    expect(DEFAULT_QUOTA_USAGE_QUERY_SCOPE).toBe("all");
    expect(QUOTA_USAGE_TOOL_NAMES).toEqual([
      "QueryQuotaUsage",
      "query_quota_usage",
      "mcp__lilia__query_quota_usage",
    ]);
    expect(QUOTA_USAGE_TOOL_NAME).toBe(QUOTA_USAGE_TOOL_NAMES[0]);
    expect(QUOTA_USAGE_CLAUDE_TOOL_NAME).toBe(QUOTA_USAGE_TOOL_NAMES[1]);
    expect(QUOTA_USAGE_MCP_TOOL_NAME).toBe(QUOTA_USAGE_TOOL_NAMES[2]);
    expect(QUOTA_USAGE_GET_STATS_COMMAND).toBe("quota_usage_get_stats");
    expect(QUOTA_USAGE_GET_CODEX_ACCOUNT_STATUS_COMMAND).toBe(
      "quota_usage_get_codex_account_status",
    );
    expect(QUOTA_USAGE_CONSUME_CODEX_RATE_LIMIT_RESET_CREDIT_COMMAND).toBe(
      "quota_usage_consume_codex_rate_limit_reset_credit",
    );
    expect(isLiliaQuotaTool("QueryQuotaUsage")).toBe(true);
    expect(isLiliaQuotaTool("query_quota_usage")).toBe(true);
    expect(isLiliaQuotaTool("mcp__lilia__query_quota_usage")).toBe(true);
    expect(isLiliaQuotaTool("quota_usage")).toBe(false);
    expect(normalizeQuotaUsageQueryScope("tools")).toBe("tools");
    expect(normalizeQuotaUsageQueryScope("unknown")).toBe("all");
    expect(QUERY_QUOTA_USAGE_INPUT_SCHEMA).toMatchObject({
      type: "object",
      additionalProperties: false,
      properties: {
        days: { type: "integer", enum: [7, 30], default: 7 },
        backend: { type: "string", enum: ["all", "native-agentkit"], default: "all" },
        scope: {
          type: "string",
          enum: ["summary", "projects", "conversations", "tools", "all"],
          default: "all",
        },
      },
    });
    expect(codexAccountQuotaCreditsLabel(null)).toBe("暂无 credit 数据");
    expect(codexAccountQuotaCreditsLabel({ hasCredits: true, unlimited: true, balance: null })).toBe("不限");
    expect(codexAccountQuotaCreditsLabel({ hasCredits: true, unlimited: false, balance: "$5" })).toBe("剩余 $5");
    expect(codexAccountQuotaCreditsLabel({ hasCredits: true, unlimited: false, balance: null })).toBe("可用");
    expect(codexAccountQuotaWindowShortLabel(null)).toBe("额度");
    expect(codexAccountQuotaWindowShortLabel({ windowDurationMins: 300 })).toBe("5h");
    expect(codexAccountQuotaWindowShortLabel({ windowDurationMins: 10080 })).toBe("7d");
    expect(codexAccountQuotaWindowShortLabel({ windowDurationMins: 45 })).toBe("45m");
    expect(codexAccountQuotaPercentLabel(42.4)).toBe("42%");
    expect(codexAccountQuotaPercentLabel(7.25)).toBe("7.3%");
    expect(codexAccountQuotaPercentLabel(142)).toBe("100%");
    expect(clampPercent(-12)).toBe(0);
    expect(clampPercent(42.4)).toBe(42.4);
    expect(clampPercent(142)).toBe(100);
    expect(codexAccountQuotaWindowRemainingPercent({ usedPercent: 42.4 })).toBe(57.6);
    expect(codexAccountQuotaWindowRemainingLabel({ usedPercent: 42.4 })).toBe("剩余 58%");
    expect(codexAccountQuotaWindowRemainingLabel(null)).toBe("暂无数据");
    expect(codexAccountQuotaWindowRemainingLine({
      usedPercent: 42.4,
      windowDurationMins: 300,
    })).toBe("5h · 剩余 58%");
    expect(codexAccountQuotaWindowRemainingLine({
      usedPercent: 92,
      windowDurationMins: 10080,
    }, "Plus")).toBe("7d · 剩余 8% · Plus");
    expect(normalizeLiliaCodeCoreCodexQuotaStatus({
      available: true,
      connectionMode: "codex-account",
      limitId: " codex ",
      fiveHour: {
        usedPercent: 142,
        windowDurationMins: 300.8,
        resetsAt: -1,
      },
      credits: {
        hasCredits: true,
        unlimited: false,
        balance: "  $5  ",
      },
      rateLimitResetCredits: {
        availableCount: 2.8,
      },
      accountUsage: {
        summary: {
          lifetimeTokens: 12.8,
          peakDailyTokens: -1,
        },
        dailyUsageBuckets: [
          { startDate: "2026-06-27", tokens: 5.9 },
          { startDate: "", tokens: 8 },
        ],
      },
      fetchedAt: 42,
    })).toMatchObject({
      available: true,
      connectionMode: "codex-account",
      limitId: "codex",
      fiveHour: {
        usedPercent: 100,
        windowDurationMins: 300,
        resetsAt: null,
      },
      credits: {
        hasCredits: true,
        unlimited: false,
        balance: "$5",
      },
      rateLimitResetCredits: {
        availableCount: 2,
      },
      accountUsage: {
        summary: {
          lifetimeTokens: 12,
          peakDailyTokens: null,
        },
        dailyUsageBuckets: [{ startDate: "2026-06-27", tokens: 5 }],
      },
      fetchedAt: 42,
      error: null,
    });
    expect(createLiliaCodeCoreCodexQuotaUnavailableStatus({
      error: new Error("offline"),
      fetchedAt: 7,
    })).toMatchObject({
      available: false,
      connectionMode: "codex-account",
      fetchedAt: 7,
      error: "Error: offline",
    });
    expect(CODEX_RATE_LIMIT_RESET_CREDIT_CONSUME_OUTCOMES).toEqual([
      "reset",
      "alreadyRedeemed",
      "nothingToReset",
      "noCredit",
    ]);
    expect(CODEX_RATE_LIMIT_RESET_CREDIT_CONSUME_OUTCOME_LABELS.reset).toBe("已使用 1 次重置次数，官方额度已刷新");
    expect(DEFAULT_CODEX_RATE_LIMIT_RESET_CREDIT_CONSUME_OUTCOME_LABEL).toBe("重置次数请求已完成");
    expect(isCodexRateLimitResetCreditConsumeOutcome("reset")).toBe(true);
    expect(isCodexRateLimitResetCreditConsumeOutcome("unknown")).toBe(false);
    expect(codexRateLimitResetCreditConsumeOutcomeLabel("reset")).toBe("已使用 1 次重置次数，官方额度已刷新");
    expect(codexRateLimitResetCreditConsumeOutcomeLabel("alreadyRedeemed")).toBe("本次重置请求已处理，官方额度已刷新");
    expect(codexRateLimitResetCreditConsumeOutcomeLabel("nothingToReset")).toBe("当前没有可重置的额度窗口");
    expect(codexRateLimitResetCreditConsumeOutcomeLabel("noCredit")).toBe("没有可用的重置次数");
    expect(codexRateLimitResetCreditConsumeOutcomeLabel("unknown")).toBe("重置次数请求已完成");
    expect(isPermissionMode("free")).toBe(true);
    expect(isPermissionMode("danger")).toBe(false);
    expect(normalizePermissionMode("danger", "readonly")).toBe("readonly");
    expect(REASONING_EFFORTS).toEqual(["low", "medium", "high", "xhigh", "max"]);
    expect(BACKEND_REASONING_EFFORTS).toEqual({
      claude: ["low", "medium", "high", "xhigh", "max"],
      codex: ["low", "medium", "high", "xhigh"],
    });
    expect(reasoningEffortsForBackend("codex")).toEqual(["low", "medium", "high", "xhigh"]);
    expect(normalizeReasoningEffortForBackend("codex", "max")).toBe("xhigh");
    expect(normalizeReasoningEffortForBackend("claude", "max")).toBe("max");
    expect(PERMISSION_MODES).toEqual(["full", "ask", "readonly", "free"]);
    expect(PERMISSION_MODE_DISPLAY_ORDER).toEqual(["ask", "readonly", "full", "free"]);
    expect(LOCKED_PERMISSION_MODES).toEqual(["ask", "readonly"]);
    expect(DEFAULT_PERMISSION_MODE_AVAILABILITY).toEqual({
      full: true,
      ask: true,
      readonly: true,
      free: true,
    });
    expect(PERMISSION_MODE_DISPLAY.ask).toEqual({
      label: "询问",
      description: "敏感操作前等待用户确认。",
    });
    expect(PERMISSION_MODE_DISPLAY.free.description).toContain("8 秒");
    expect(AUTO_TURN_DECISION_PERMISSION_OPTIONS.map((option) => option.key)).toEqual([
      "allowModelTier",
      "allowReasoningEffort",
      "allowPlanMode",
      "allowGoalMode",
      "allowSessionFork",
    ]);
    expect(AUTO_TURN_DECISION_PERMISSION_OPTIONS[0].label).toBe("模型层级");
    expect(isReasoningEffort("max")).toBe(true);
    expect(isReasoningEffort("extreme")).toBe(false);
    expect(normalizeReasoningEffort("xhigh")).toBe("xhigh");
    expect(normalizeReasoningEffort("extreme")).toBeNull();
    expect(normalizeReasoningEffortForBackend("claude", "max")).toBe("max");
    expect(normalizeReasoningEffortForBackend("codex", "max")).toBe("xhigh");
    expect(normalizeReasoningEffortForBackend("codex", "extreme")).toBeNull();
  });

  it("loads chat event contracts", () => {
    expect(CHAT_TURN_STARTED_EVENT_NAME).toBe("chat:turn-started");
    expect(CHAT_DONE_EVENT_NAME).toBe("chat:done");
    expect(CHAT_CONTEXT_USAGE_EVENT_NAME).toBe("chat:context-usage");
    expect(CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME).toBe("chat:agent-interaction-request");
    expect(AGENT_TIMELINE_EVENT_NAME).toBe("agent:timeline");
    expect(AGENT_TIMELINE_BATCH_EVENT_NAME).toBe("agent:timeline-batch");
    expect(createChatTurnStartedEvent("task-1", 2)).toEqual({
      taskId: "task-1",
      queuedCount: 2,
    });
    expect(createChatDoneEvent({ taskId: "task-1" })).toEqual({
      taskId: "task-1",
      sessionId: null,
      subtype: null,
      rollback: null,
    });
  });

  it("loads chat command contracts", () => {
    expect(AGENT_TIMELINE_LIST_COMMAND).toBe("agent_timeline_list");
    expect(AGENT_TIMELINE_CLEAR_TASK_COMMAND).toBe("agent_timeline_clear_task");
    expect(CHAT_SEND_MESSAGE_COMMAND).toBe("chat_send_message");
    expect(CHAT_INTERRUPT_TURN_COMMAND).toBe("chat_interrupt_turn");
    expect(CHAT_DESCRIBE_ATTACHMENTS_COMMAND).toBe("chat_describe_attachments");
    expect(CHAT_SEARCH_CONTEXT_ATTACHMENTS_COMMAND).toBe("chat_search_context_attachments");
    expect(CHAT_SEARCH_SLASH_COMMANDS_COMMAND).toBe("chat_search_slash_commands");
    expect(CHAT_READ_CLIPBOARD_FILE_PATHS_COMMAND).toBe("chat_read_clipboard_file_paths");
    expect(CHAT_SAVE_CLIPBOARD_IMAGE_COMMAND).toBe("chat_save_clipboard_image");
    expect(CHAT_SAVE_CLIPBOARD_TEXT_COMMAND).toBe("chat_save_clipboard_text");
    expect(CHAT_GET_COMPOSER_STATE_COMMAND).toBe("chat_get_composer_state");
    expect(CHAT_LIST_MODELS_COMMAND).toBe("chat_list_models");
    expect(CHAT_GET_RUNTIME_SNAPSHOT_COMMAND).toBe("chat_get_runtime_snapshot");
    expect(CHAT_SET_COMPOSER_STATE_COMMAND).toBe("chat_set_composer_state");
    expect(CHAT_ACK_RESTORED_ROLLBACK_COMMAND).toBe("chat_ack_restored_rollback");
    expect(CHAT_RESPOND_AGENT_INTERACTION_COMMAND).toBe("chat_respond_agent_interaction");
    expect(CHAT_RESPOND_TITLE_UPDATE_COMMAND).toBe("chat_respond_title_update");
    expect(LILIA_IAB_OPEN_COMMAND).toBe("lilia_iab_open");
    expect(LILIA_IAB_SUBMIT_COMMAND).toBe("lilia_iab_submit");
  });

  it("loads automation command contracts", () => {
    expect(AUTOMATION_LIST_WORKFLOWS_COMMAND).toBe("automation_list_workflows");
    expect(AUTOMATION_SAVE_DRAFT_COMMAND).toBe("automation_save_draft");
    expect(AUTOMATION_PUBLISH_COMMAND).toBe("automation_publish");
    expect(AUTOMATION_DELETE_WORKFLOW_COMMAND).toBe("automation_delete_workflow");
    expect(AUTOMATION_SET_ENABLED_COMMAND).toBe("automation_set_enabled");
    expect(AUTOMATION_RUN_ONCE_COMMAND).toBe("automation_run_once");
    expect(AUTOMATION_RESUME_RUN_COMMAND).toBe("automation_resume_run");
    expect(AUTOMATION_LIST_RUNS_COMMAND).toBe("automation_list_runs");
    expect(AUTOMATION_GET_RUN_COMMAND).toBe("automation_get_run");
  });

  it("loads provider command contracts", () => {
    expect(CHAT_CHECK_ENV_COMMAND).toBe("chat_check_env");
    expect(PROVIDER_GET_CONFIG_COMMAND).toBe("provider_get_config");
    expect(PROVIDER_SET_CONFIG_COMMAND).toBe("provider_set_config");
    expect(PROVIDER_GET_ACTIVE_BACKEND_COMMAND).toBe("provider_get_active_backend");
    expect(PROVIDER_SET_ACTIVE_BACKEND_COMMAND).toBe("provider_set_active_backend");
    expect(ROUTER_GET_MODE_COMMAND).toBe("router_get_mode");
    expect(ROUTER_SET_MODE_COMMAND).toBe("router_set_mode");
    expect(ASSISTANT_AI_GET_CONFIG_COMMAND).toBe("assistant_ai_get_config");
    expect(ASSISTANT_AI_SET_CONFIG_COMMAND).toBe("assistant_ai_set_config");
    expect(ASSISTANT_AI_FETCH_MODELS_COMMAND).toBe("assistant_ai_fetch_models");
    expect(MODEL_FEATURE_LIST_MODEL_OPTIONS_COMMAND).toBe("model_feature_list_model_options");
    expect(MODEL_FEATURE_GET_SETTINGS_COMMAND).toBe("model_feature_get_settings");
    expect(MODEL_FEATURE_SET_SETTINGS_COMMAND).toBe("model_feature_set_settings");
    expect(ASSISTANT_AI_TEST_CONNECTION_COMMAND).toBe("assistant_ai_test_connection");
    expect(ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND).toBe("assistant_ai_optimize_prompt");
  });

  it("loads app event contracts", () => {
    expect(MAIN_NAVIGATE_EVENT_NAME).toBe("lilia:main:navigate");
    expect(POPUP_NAVIGATE_EVENT_NAME).toBe("lilia:popup:navigate");
    expect(CLI_PROJECT_OPEN_EVENT_NAME).toBe("lilia:cli-project-open");
    expect(CLI_PROJECT_OPEN_CONSUME_PENDING_COMMAND).toBe("cli_project_open_consume_pending");
    expect(POPUP_GET_WINDOW_SETTINGS_COMMAND).toBe("popup_get_window_settings");
    expect(POPUP_SET_WINDOW_SETTINGS_COMMAND).toBe("popup_set_window_settings");
    expect(POPUP_REMEMBER_LAST_PROJECT_COMMAND).toBe("popup_remember_last_project");
    expect(POPUP_OPEN_NEW_CHAT_COMMAND).toBe("popup_open_new_chat");
    expect(POPUP_OPEN_TASK_COMMAND).toBe("popup_open_task");
    expect(POPUP_OPEN_CHILD_QUESTION_COMMAND).toBe("popup_open_child_question");
    expect(POPUP_OPEN_CONVERSATION_STATUS_COMMAND).toBe("popup_open_conversation_status");
    expect(POPUP_TOGGLE_CONVERSATION_STATUS_COMMAND).toBe("popup_toggle_conversation_status");
    expect(POPUP_FOCUS_MAIN_COMMAND).toBe("popup_focus_main");
    expect(createAppNavigateEvent("/projects/lilia")).toEqual({
      route: "/projects/lilia",
    });
    expect(createCliProjectOpenEvent("lilia", "D:\\PROJECT\\workspace\\Lilia")).toEqual({
      projectId: "lilia",
      cwd: "D:\\PROJECT\\workspace\\Lilia",
    });
  });

  it.skip("derives provider diagnostics", () => {
    expect(DIRECT_DEFAULT_URLS).toMatchObject({
      claude: "https://api.anthropic.com",
      codex: "https://api.openai.com/v1",
    });
    expect(directDefaultUrlForBackend("codex")).toBe("https://api.openai.com/v1");
    expect(API_KEY_ENV_BY_BACKEND).toEqual({
      claude: "ANTHROPIC_API_KEY",
      codex: "OPENAI_API_KEY",
    });
    expect(apiKeyEnvForBackend("claude")).toBe("ANTHROPIC_API_KEY");
    expect(PROVIDER_STORE_KEY_BY_BACKEND).toEqual({
      claude: "provider.claude",
      codex: "provider.codex",
    });
    expect(ROUTER_STORE_KEY_BY_BACKEND).toEqual({
      claude: "router.claude",
      codex: "router.codex",
    });
    expect(ROUTER_MODES_BY_BACKEND).toEqual({
      claude: ["api"],
      codex: ["api", "codex-account"],
    });
    expect(DEFAULT_ROUTER_MODE_BY_BACKEND).toEqual({
      claude: "api",
      codex: "codex-account",
    });
    expect(routerModesForBackend("codex")).toEqual(["api", "codex-account"]);
    expect(defaultRouterModeForBackend("claude")).toBe("api");
    expect(isRouterModeForBackend("codex", "codex-account")).toBe(true);
    expect(isRouterModeForBackend("claude", "codex-account")).toBe(false);
    expect(normalizeRouterModeForBackend("claude", "codex-account")).toBe("api");
    expect(normalizeRouterModeForBackend("codex", "codex-account")).toBe("codex-account");
    expect(ROUTER_MODE_LABELS).toEqual({
      api: "API",
      "codex-account": "官方账号",
    });
    expect(routerModeLabel("codex-account")).toBe("官方账号");
    expect(ROUTER_MODES_USING_API_CONFIG).toEqual(["api"]);
    expect(ROUTER_MODES_USING_CODEX_ACCOUNT).toEqual(["codex-account"]);
    expect(routerModeUsesApiConfig("api")).toBe(true);
    expect(routerModeUsesApiConfig("codex-account")).toBe(false);
    expect(routerModeUsesCodexAccount("api")).toBe(false);
    expect(routerModeUsesCodexAccount("codex-account")).toBe(true);
    expect(CONNECTION_MODES).toEqual(["api", "custom", "codex-account", "unconfigured"]);
    expect(CONNECTION_MODES_USING_API_KEY).toEqual(["api", "custom"]);
    expect(CONNECTION_MODES_USING_DEFAULT_API).toEqual(["api"]);
    expect(CONNECTION_MODES_USING_CUSTOM_URL).toEqual(["custom"]);
    expect(CONNECTION_MODES_USING_CODEX_ACCOUNT).toEqual(["codex-account"]);
    expect(UNCONFIGURED_CONNECTION_MODES).toEqual(["unconfigured"]);
    expect(isConnectionMode("custom")).toBe(true);
    expect(isConnectionMode("direct")).toBe(false);
    expect(connectionModeUsesApiKey("api")).toBe(true);
    expect(connectionModeUsesApiKey("custom")).toBe(true);
    expect(connectionModeUsesApiKey("codex-account")).toBe(false);
    expect(connectionModeUsesDefaultApi("api")).toBe(true);
    expect(connectionModeUsesDefaultApi("custom")).toBe(false);
    expect(connectionModeUsesCustomUrl("custom")).toBe(true);
    expect(connectionModeUsesCustomUrl("unconfigured")).toBe(false);
    expect(connectionModeUsesCodexAccount("codex-account")).toBe(true);
    expect(connectionModeUsesCodexAccount("api")).toBe(false);
    expect(connectionModeIsUnconfigured("unconfigured")).toBe(true);
    expect(connectionModeIsUnconfigured("api")).toBe(false);
    expect(API_DESCRIPTION_BY_BACKEND.claude).toContain("Anthropic API");
    expect(apiDescriptionForBackend("codex")).toContain("OpenAI API");
    expect(codexRuntimeIssue({
      version: null,
      installPath: null,
      managed: true,
      available: false,
      supportsRequiredProtocol: false,
      failureKind: "providerIncompatible",
      issues: ["模型不可用"],
      latestVersion: null,
      updateAvailable: false,
      releaseNotes: [],
      updateError: null,
      updateState: "idle",
      preparedVersion: null,
      updateProgressPercent: null,
    })).toBe("模型不可用 请确认当前 API 来源支持 OpenAI Responses API 与 Codex 模型白名单。");
    expect(runtimeDiagnostic("codex", {
      nodeAvailable: true,
      codexCliAvailable: false,
      codexAppServer: {
        version: null,
        installPath: null,
        managed: true,
        available: false,
        supportsRequiredProtocol: false,
        failureKind: "missingCli",
        issues: [],
        latestVersion: null,
        updateAvailable: false,
        releaseNotes: [],
        updateError: null,
        updateState: "idle",
        preparedVersion: null,
        updateProgressPercent: null,
      },
      routerModes: { claude: "api", codex: "codex-account" },
      backends: {
        claude: { backend: "native-agentkit", hasApiKey: true, connectionMode: "api", effectiveUrl: null },
        codex: { backend: "native-agentkit", hasApiKey: false, connectionMode: "codex-account", effectiveUrl: null },
      },
    })).toMatchObject({
      tone: "err",
      title: "Codex app-server 缺失",
    });
    expect(connectionDiagnostic("claude", {
      backend: "native-agentkit",
      hasApiKey: false,
      connectionMode: "api",
      effectiveUrl: null,
    }, "api")).toMatchObject({
      tone: "warn",
      title: "Claude API 缺少 API key",
    });
    expect(connectionDiagnostic("codex", {
      backend: "native-agentkit",
      hasApiKey: false,
      connectionMode: "codex-account",
      effectiveUrl: null,
    }, "codex-account")).toBeNull();
    expect(connectionDiagnostic("codex", {
      backend: "native-agentkit",
      hasApiKey: false,
      connectionMode: "custom",
      effectiveUrl: "http://localhost:11434/v1",
    }, "api")).toMatchObject({
      tone: "warn",
      title: "Codex 自定义 API 来源",
    });
    expect(connectionDiagnostic("codex", {
      backend: "native-agentkit",
      hasApiKey: false,
      connectionMode: "unconfigured",
      effectiveUrl: null,
    }, "codex-account")).toMatchObject({
      tone: "warn",
      title: "Codex 官方账号未就绪",
    });
    expect(connectionDiagnostic("claude", {
      backend: "native-agentkit",
      hasApiKey: false,
      connectionMode: "unconfigured",
      effectiveUrl: null,
    }, "api")).toMatchObject({
      tone: "warn",
      title: "Claude API 未配置",
    });
  });

  it.skip("labels history import providers", () => {
    expect(HISTORY_IMPORT_PROVIDERS).toEqual(["native-agentkit"]);
    expect(HISTORY_IMPORT_DEFAULT_SEARCH_LIMIT).toBe(20);
    expect(HISTORY_IMPORT_MAX_SEARCH_LIMIT).toBe(50);
    expect(HISTORY_IMPORT_MAX_SESSION_MANAGEMENT_LIMIT).toBe(100);
    expect(HISTORY_IMPORT_DEFAULT_SYNC_LIMIT).toBe(120);
    expect(HISTORY_IMPORT_MAX_SYNC_LIMIT).toBe(500);
    expect(CODEX_HISTORY_DEFAULT_TURN_LIMIT).toBe(50);
    expect(CODEX_HISTORY_PREVIEW_TURN_LIMIT).toBe(8);
    expect(HISTORY_IMPORT_PREVIEW_MESSAGE_LIMIT).toBe(5);
    expect(HISTORY_IMPORT_TITLE_TEXT_LIMIT).toBe(160);
    expect(HISTORY_IMPORT_PREVIEW_TEXT_LIMIT).toBe(240);
    expect(HISTORY_IMPORT_INLINE_PREVIEW_TEXT_LIMIT).toBe(180);
    expect(HISTORY_IMPORT_MESSAGE_SUMMARY_TEXT_LIMIT).toBe(1200);
    expect(HISTORY_IMPORT_ERROR_SUMMARY_TEXT_LIMIT).toBe(400);
    expect(HISTORY_IMPORT_SEARCH_COMMAND).toBe("history_import_search");
    expect(HISTORY_IMPORT_PREVIEW_COMMAND).toBe("history_import_preview");
    expect(HISTORY_IMPORT_ATTACH_COMMAND).toBe("history_import_attach");
    expect(HISTORY_IMPORT_RUNTIME_STATES_COMMAND).toBe("history_import_runtime_states");
    expect(HISTORY_IMPORT_CLEAN_BACKGROUND_TERMINALS_COMMAND).toBe(
      "history_import_clean_background_terminals",
    );
    expect(isHistoryImportProvider("codex")).toBe(true);
    expect(isHistoryImportProvider("openai")).toBe(false);
    expect(historyImportProviderDisplay("codex")).toEqual({
      name: "Codex",
      entity: "Codex thread",
      supportsArchived: true,
    });
    expect(historyImportProviderDisplay("claude")).toEqual({
      name: "Claude",
      entity: "Claude session",
      supportsArchived: false,
    });
    expect(historyImportProviderUiLabels("codex")).toEqual({
      list: "Codex thread 列表",
      preview: "Codex thread 预览",
      searchPlaceholder: "搜索 Codex thread",
      loading: "正在读取 Codex 历史",
      emptyList: "没有找到 Codex thread",
      emptyPreview: "这个 Codex thread 暂无可预览事件。",
      choosePreview: "选择一个 Codex thread 后查看摘要并导入。",
    });
    expect(historyImportProviderUiLabels("claude").loading).toBe("正在读取 Claude 历史");
  });

  it.skip("normalizes session management runtime commands", () => {
    expect(SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE).toBe("session_management");
    expect(SESSION_MANAGEMENT_ACTIONS).toEqual([
      "list",
      "info",
      "messages",
      "rename",
      "tag",
      "delete",
      "archive",
    ]);
    expect(DEFAULT_SESSION_MANAGEMENT_LIMIT).toBe(20);
    expect(MAX_SESSION_MANAGEMENT_LIMIT).toBe(100);
    expect(DEFAULT_SESSION_MANAGEMENT_ARCHIVED).toBe(false);
    expect(isSessionManagementAction("archive")).toBe(true);
    expect(isSessionManagementAction("fork")).toBe(false);
    expect(normalizeSessionManagementRuntimeCommand(null)).toBeNull();
    expect(normalizeSessionManagementRuntimeCommand({
      type: "session_management",
      action: "list",
      limit: 999,
      cursor: " 5 ",
      searchTerm: " fix ",
      includeSystemMessages: true,
    })).toEqual({
      action: "list",
      sessionId: "",
      title: "",
      tag: null,
      archived: false,
      limit: DEFAULT_SESSION_MANAGEMENT_LIMIT,
      cursor: "5",
      searchTerm: "fix",
      includeSystemMessages: true,
    });
    expect(normalizeSessionManagementRuntimeCommand({
      type: "session_management",
      action: "rename",
      sessionId: " thread-1 ",
      title: " 新标题 ",
      archived: true,
    })).toMatchObject({
      action: "rename",
      sessionId: "thread-1",
      title: "新标题",
      archived: true,
    });
    expect(createSessionManagementRuntimeCommand("rename", {
      sessionId: " thread-1 ",
      title: " 新标题 ",
      archived: true,
    })).toEqual({
      type: "session_management",
      action: "rename",
      sessionId: "thread-1",
      title: "新标题",
      archived: true,
    });
    expect(createSessionManagementRuntimeCommand("list", {
      limit: 999,
      cursor: " 5 ",
      searchTerm: " fix ",
      includeSystemMessages: true,
    })).toEqual({
      type: "session_management",
      action: "list",
      limit: DEFAULT_SESSION_MANAGEMENT_LIMIT,
      cursor: "5",
      searchTerm: "fix",
      includeSystemMessages: true,
    });
    expect(() => normalizeSessionManagementRuntimeCommand({
      type: "session_management",
      action: "messages",
    })).toThrow("missing sessionId");
    expect(() => normalizeSessionManagementRuntimeCommand({
      type: "session_management",
      action: "tag",
      sessionId: "thread-1",
    })).toThrow("requires tag");
  });

  it("normalizes lilia workflow contracts", () => {
    expect(LILIA_GOAL_WORKFLOW_TYPE).toBe("lilia_goal");
    expect(LILIA_GOAL_ACTIONS).toEqual(["set", "refresh", "clear"]);
    expect(LILIA_GOAL_STATUSES).toEqual([
      "active",
      "paused",
      "blocked",
      "usageLimited",
      "budgetLimited",
      "complete",
    ]);
    expect(DEFAULT_LILIA_GOAL_STATUS).toBe("active");
    expect(LILIA_MEMORY_MODE_WORKFLOW_TYPE).toBe("lilia_memory_mode");
    expect(LILIA_MEMORY_MODES).toEqual(["enabled", "disabled"]);
    expect(LILIA_MEMORY_RESET_WORKFLOW_TYPE).toBe("lilia_memory_reset");
    expect(LILIA_COMPACT_WORKFLOW_TYPE).toBe("lilia_compact");
    expect(LILIA_BACKGROUND_TERMINALS_CLEAN_WORKFLOW_TYPE).toBe(
      "lilia_background_terminals_clean",
    );
    expect(LILIA_CONFIG_DIAGNOSTICS_WORKFLOW_TYPE).toBe("lilia_config_diagnostics");
    expect(DEFAULT_LILIA_CONFIG_DIAGNOSTICS_INCLUDE_LAYERS).toBe(true);
    expect(AUTOMATION_WORKFLOW_TYPE).toBe("automation");
    expect(CHAT_SLASH_COMMAND_WORKFLOW_TYPE).toBe("slash_command");
    expect(createAutomationRunWorkflow(" run-1 ")).toEqual({
      type: "automation",
      automationRunId: "run-1",
    });
    expect(normalizeAutomationRunWorkflow({ type: "unknown" })).toBeNull();
    expect(() => createAutomationRunWorkflow(" ")).toThrow("automationRunId");
    expect(LILIA_REVIEW_WORKFLOW_TYPE).toBe("lilia_review");
    expect(LILIA_QUERY_WORKFLOW_TYPES).toEqual([
      "lilia_review",
      "lilia_fix_suggestion",
      "lilia_batch_apply",
      "lilia_task_workflow",
      "lilia_compact",
    ]);
    expect(isLiliaQueryWorkflowType("lilia_review")).toBe(true);
    expect(isLiliaQueryWorkflowType("lilia_task_workflow")).toBe(true);
    expect(isLiliaQueryWorkflowType("lilia_compact")).toBe(true);
    expect(isLiliaQueryWorkflowType("lilia_goal")).toBe(false);
    expect(LILIA_TASK_WORKFLOW_TYPE).toBe("lilia_task_workflow");
    expect(LILIA_TASK_WORKFLOW_KINDS).toContain("frontend");
    expect(isLiliaTaskWorkflowKind("frontend")).toBe(true);
    expect(isLiliaTaskWorkflowKind("unknown")).toBe(false);
    expect(LILIA_REVIEW_TARGET_TYPES).toEqual([
      "uncommittedChanges",
      "baseBranch",
      "commit",
    ]);
    expect(LILIA_REVIEW_DELIVERIES).toEqual(["inline", "detached"]);
    expect(DEFAULT_LILIA_REVIEW_DELIVERY).toBe("inline");
    expect(LILIA_FIX_SUGGESTION_WORKFLOW_TYPE).toBe("lilia_fix_suggestion");
    expect(LILIA_FIX_SUGGESTION_MODES).toEqual(["suggest", "apply"]);
    expect(DEFAULT_LILIA_FIX_SUGGESTION_MODE).toBe("suggest");
    expect(LILIA_BATCH_APPLY_WORKFLOW_TYPE).toBe("lilia_batch_apply");
    expect(LILIA_BATCH_APPLY_SOURCE_KINDS).toEqual(["review", "fix_suggestion"]);
    expect(isLiliaReviewTargetType("baseBranch")).toBe(true);
    expect(isLiliaReviewTargetType("branch")).toBe(false);
    expect(isLiliaReviewDelivery("detached")).toBe(true);
    expect(isLiliaReviewDelivery("popup")).toBe(false);
    expect(isLiliaFixSuggestionMode("apply")).toBe(true);
    expect(isLiliaFixSuggestionMode("patch")).toBe(false);
    expect(isLiliaBatchApplySourceKind("fix_suggestion")).toBe(true);
    expect(isLiliaBatchApplySourceKind("goal")).toBe(false);
    expect(normalizeLiliaReviewTarget({ type: "baseBranch", branch: " main " })).toEqual({
      type: "baseBranch",
      branch: "main",
    });
    expect(normalizeLiliaReviewTarget({ type: "commit", sha: " abc " })).toEqual({
      type: "commit",
      sha: "abc",
    });
    expect(normalizeLiliaReviewTarget({ type: "baseBranch" })).toBeNull();
    expect(normalizeLiliaReviewDelivery("popup")).toBe("inline");
    expect(normalizeLiliaFixSuggestionMode("patch")).toBe("suggest");
    expect(createLiliaReviewWorkflow(
      { type: "uncommittedChanges" },
      { instructions: " check ", delivery: "detached" },
    )).toEqual({
      type: "lilia_review",
      target: { type: "uncommittedChanges" },
      instructions: "check",
      delivery: "detached",
    });
    expect(createLiliaFixSuggestionWorkflow(
      { type: "baseBranch", branch: " main " },
      { instructions: " fix ", mode: "apply" },
    )).toEqual({
      type: "lilia_fix_suggestion",
      target: { type: "baseBranch", branch: "main" },
      instructions: "fix",
      mode: "apply",
    });
    expect(createLiliaBatchApplyWorkflow({
      sourceTurnId: " turn-1 ",
      sourceKind: "review",
      sourceSummary: " summary ",
    })).toEqual({
      type: "lilia_batch_apply",
      sourceTurnId: "turn-1",
      sourceKind: "review",
      sourceSummary: "summary",
    });
    expect(createLiliaTaskWorkflow("frontend", { instructions: " ui " })).toEqual({
      type: "lilia_task_workflow",
      kind: "frontend",
      instructions: "ui",
    });
    expect(normalizeLiliaReviewWorkflow({
      type: "lilia_review",
      target: { type: "commit", sha: " abc " },
    })).toEqual({
      target: { type: "commit", sha: "abc" },
      instructions: "",
      delivery: "inline",
    });
    expect(normalizeLiliaFixSuggestionWorkflow({
      type: "lilia_fix_suggestion",
      target: { type: "uncommittedChanges" },
      mode: "apply",
    })).toEqual({
      target: { type: "uncommittedChanges" },
      instructions: "",
      mode: "apply",
    });
    expect(normalizeLiliaBatchApplyWorkflow({
      type: "lilia_batch_apply",
      sourceTurnId: " turn-1 ",
      sourceKind: "fix_suggestion",
      sourceSummary: " summary ",
      instructions: " apply ",
    })).toEqual({
      sourceTurnId: "turn-1",
      sourceKind: "fix_suggestion",
      sourceSummary: "summary",
      instructions: "apply",
    });
    expect(normalizeLiliaTaskWorkflow({
      type: "lilia_task_workflow",
      kind: "bugLocalization",
      instructions: " trace ",
    })).toEqual({
      kind: "bugLocalization",
      instructions: "trace",
    });
    expect(() => normalizeLiliaTaskWorkflow({
      type: "lilia_task_workflow",
      kind: "missing",
    })).toThrow("valid kind");
    expect(() => normalizeLiliaBatchApplyWorkflow({
      type: "lilia_batch_apply",
      sourceKind: "review",
      sourceSummary: "summary",
    })).toThrow("missing sourceTurnId");
    expect(isLiliaGoalAction("set")).toBe(true);
    expect(isLiliaGoalAction("delete")).toBe(false);
    expect(isLiliaGoalStatus("budgetLimited")).toBe(true);
    expect(isLiliaGoalStatus("waiting")).toBe(false);
    expect(isLiliaMemoryMode("enabled")).toBe(true);
    expect(isLiliaMemoryMode("auto")).toBe(false);
    expect(normalizeLiliaGoalStatus("complete")).toBe("complete");
    expect(normalizeLiliaGoalStatus("waiting")).toBe("active");
    expect(createLiliaGoalWorkflow("set", { objective: " ship it " })).toEqual({
      type: "lilia_goal",
      action: "set",
      objective: "ship it",
      status: "active",
      tokenBudget: null,
    });
    expect(createLiliaGoalWorkflow("refresh")).toEqual({
      type: "lilia_goal",
      action: "refresh",
    });
    expect(createLiliaGoalWorkflow("clear")).toEqual({
      type: "lilia_goal",
      action: "clear",
    });
    expect(() => createLiliaGoalWorkflow("set")).toThrow("missing objective");
    expect(createLiliaCompactWorkflow()).toEqual({ type: "lilia_compact" });
    expect(normalizeLiliaGoalWorkflow(null)).toBeNull();
    expect(normalizeLiliaGoalWorkflow({
      type: "lilia_goal",
      action: "set",
      objective: " ship it ",
      status: "paused",
      tokenBudget: 5000,
    })).toEqual({
      action: "set",
      objective: "ship it",
      status: "paused",
      tokenBudget: 5000,
    });
    expect(normalizeLiliaGoalWorkflow({
      type: "lilia_goal",
      action: "clear",
      status: "unknown",
      tokenBudget: Number.NaN,
    })).toEqual({
      action: "clear",
      objective: "",
      status: "active",
      tokenBudget: null,
    });
    expect(() => normalizeLiliaGoalWorkflow({
      type: "lilia_goal",
      action: "delete",
    })).toThrow("missing a valid action");
    expect(() => normalizeLiliaGoalWorkflow({
      type: "lilia_goal",
      action: "set",
    })).toThrow("missing objective");
    expect(normalizeLiliaMemoryModeWorkflow({
      type: "lilia_memory_mode",
      mode: "disabled",
    })).toEqual({ mode: "disabled" });
    expect(() => normalizeLiliaMemoryModeWorkflow({
      type: "lilia_memory_mode",
      mode: "auto",
    })).toThrow("missing a valid mode");
    expect(isLiliaMemoryResetWorkflow({ type: "lilia_memory_reset" })).toBe(true);
    expect(isLiliaMemoryResetWorkflow({ type: "lilia_memory_mode" })).toBe(false);
    expect(isLiliaCompactWorkflow({ type: "lilia_compact" })).toBe(true);
    expect(isLiliaCompactWorkflow({ type: "lilia_goal" })).toBe(false);
    expect(isLiliaBackgroundTerminalsCleanWorkflow({
      type: "lilia_background_terminals_clean",
    })).toBe(true);
    expect(normalizeLiliaConfigDiagnosticsWorkflow({
      type: "lilia_config_diagnostics",
    })).toEqual({ includeLayers: true });
    expect(normalizeLiliaConfigDiagnosticsWorkflow({
      type: "lilia_config_diagnostics",
      includeLayers: false,
    })).toEqual({ includeLayers: false });
    expect(normalizeLiliaConfigDiagnosticsWorkflow({ type: "lilia_compact" })).toBeNull();
  });

  it("normalizes provider runtime command contracts", () => {
    expect(RUNTIME_SETTINGS_COMMAND_TYPE).toBe("runtime_settings");
    expect(RUNTIME_SETTINGS_ACTIONS).toEqual(["diagnose", "update"]);
    expect(REMOTE_ENVIRONMENT_COMMAND_TYPE).toBe("remote_environment");
    expect(REMOTE_ENVIRONMENT_ACTIONS).toEqual(["diagnose", "add", "select"]);
    expect(SANDBOX_DIAGNOSTICS_COMMAND_TYPE).toBe("sandbox_diagnostics");
    expect(SESSION_FORK_COMMAND_TYPE).toBe("session_fork");
    expect(DEFAULT_SESSION_FORK_EXCLUDE_TURNS).toBe(true);
    expect(isRuntimeSettingsAction("diagnose")).toBe(true);
    expect(isRuntimeSettingsAction("delete")).toBe(false);
    expect(isRemoteEnvironmentAction("select")).toBe(true);
    expect(isRemoteEnvironmentAction("delete")).toBe(false);
    expect(normalizeRuntimeSettingsCommand(null)).toBeNull();
    expect(normalizeRuntimeSettingsCommand({
      type: "runtime_settings",
      action: "diagnose",
    })).toEqual({ action: "diagnose" });
    expect(createRuntimeSettingsCommand("update")).toEqual({
      type: "runtime_settings",
      action: "update",
    });
    expect(() => normalizeRuntimeSettingsCommand({
      type: "runtime_settings",
      action: "delete",
    })).toThrow("runtime settings command missing a valid action");
    expect(normalizeRemoteEnvironmentCommand({
      type: "remote_environment",
      action: "diagnose",
    })).toEqual({
      action: "diagnose",
      environmentId: "",
      environment: null,
    });
    expect(normalizeRemoteEnvironmentCommand({
      type: "remote_environment",
      action: "add",
      environmentId: " env-1 ",
      environment: { name: "Windows VM" },
    })).toEqual({
      action: "add",
      environmentId: "env-1",
      environment: { id: "env-1", name: "Windows VM" },
    });
    expect(createRemoteEnvironmentCommand("add", {
      environmentId: " env-2 ",
      environment: { name: "Linux VM" },
    })).toEqual({
      type: "remote_environment",
      action: "add",
      environmentId: "env-2",
      environment: { id: "env-2", name: "Linux VM" },
    });
    expect(normalizeRemoteEnvironmentCommand({
      type: "remote_environment",
      action: "select",
      environmentId: " env-1 ",
    })).toEqual({
      action: "select",
      environmentId: "env-1",
      environment: null,
    });
    expect(() => normalizeRemoteEnvironmentCommand({
      type: "remote_environment",
      action: "add",
    })).toThrow("add requires environment");
    expect(() => normalizeRemoteEnvironmentCommand({
      type: "remote_environment",
      action: "select",
    })).toThrow("select requires environmentId");
    expect(normalizeSandboxDiagnosticsCommand({
      type: "sandbox_diagnostics",
      includeDetails: true,
    })).toEqual({ includeDetails: true });
    expect(createSandboxDiagnosticsCommand()).toEqual({
      type: "sandbox_diagnostics",
      includeDetails: false,
    });
    expect(createSandboxDiagnosticsCommand({ includeDetails: true })).toEqual({
      type: "sandbox_diagnostics",
      includeDetails: true,
    });
    expect(normalizeSandboxDiagnosticsCommand({ type: "other" })).toBeNull();
    expect(normalizeSessionForkCommand({ type: "session_fork" })).toEqual({
      excludeTurns: true,
      mode: "fork",
      sourceTurnId: "",
    });
    expect(createSessionForkCommand()).toEqual({
      type: "session_fork",
      excludeTurns: true,
      mode: "fork",
    });
    expect(createSessionForkCommand({ excludeTurns: false })).toEqual({
      type: "session_fork",
      excludeTurns: false,
      mode: "fork",
    });
    expect(normalizeSessionForkCommand({
      type: "session_fork",
      excludeTurns: false,
    })).toEqual({ excludeTurns: false, mode: "fork", sourceTurnId: "" });
    expect(normalizeSessionForkCommand({ type: "other" })).toBeNull();
  });

  it("normalizes model selection defaults and preset feature settings", () => {
    expect(AUTO_MODEL_BY_BACKEND_AND_TIER["native-agentkit"]).toEqual({
      light: "gpt-5.4-mini",
      normal: "gpt-5.4",
      deep: "gpt-5.5",
    });
    expect(autoModelForBackendTier("native-agentkit", "deep")).toBe("gpt-5.5");
    expect(autoModelForBackendPreset("native-agentkit", "fast")).toBe("gpt-5.4-mini");
    expect(autoModelForBackendPreset("native-agentkit", "plan")).toBe("gpt-5.5");
    expect(PLAN_MODE_PRESET).toBe("plan");
    expect(autoReasoningEffortForPreset("review")).toBe("high");
    expect(AUTO_REASONING_EFFORT_BY_TIER).toEqual({
      light: "low",
      normal: "medium",
      deep: "high",
    });
    expect(autoReasoningEffortForTier("normal")).toBe("medium");
    expect(AUTO_WORKFLOW_TYPES_BY_TIER.light).toContain("lilia_compact");
    expect(AUTO_WORKFLOW_TYPES_BY_TIER.deep).toContain("lilia_batch_apply");
    expect(autoTierForWorkflowType("lilia_review")).toBe("deep");
    expect(autoPresetForWorkflowType("lilia_review")).toBe("review");
    expect(autoPresetForWorkflowType("lilia_compact")).toBe("fast");
    expect(autoTierForWorkflowType("unknown")).toBeNull();
    expect(AUTO_RUNTIME_COMMAND_TYPES_BY_TIER.light).toEqual([
      "runtime_settings",
      "remote_environment",
      "session_management",
    ]);
    expect(AUTO_RUNTIME_COMMAND_SIGNAL_LABELS.remote_environment).toBe("远程环境管理");
    expect(autoTierForRuntimeCommandType("session_management")).toBe("light");
    expect(autoRuntimeCommandSignalLabel("runtime_settings")).toBe("运行时诊断/设置");
    expect(AUTO_CONTEXT_THRESHOLDS.medium).toMatchObject({
      contextUsagePercent: 35,
      promptLength: 2000,
      attachmentCount: 2,
      conversationReferenceCount: 1,
    });
    expect(autoContextThresholdsForScale("large")).toMatchObject({
      contextUsagePercent: 70,
      promptLength: 8000,
      directoryFileCount: 200,
      directoryTotalSize: 20971520,
    });
    expect(runtimeOptionsModelForBackend("native-agentkit", {
      common: { model: " gpt-5.4 " },
      provider: { "native-agentkit": { model: "gpt-5.5" } },
    })).toBe("gpt-5.4");
    expect(runtimeOptionsModelForBackend("native-agentkit", {
      provider: { "native-agentkit": { model: " gpt-5.5 " } },
    })).toBe("gpt-5.5");
    expect(runtimeOptionsReasoningEffortForBackend("native-agentkit", {
      common: { reasoningEffort: "low" },
      provider: { "native-agentkit": { reasoningEffort: "max" } },
    })).toBe("max");

    const migrated = normalizeModelFeatureSettings({
      chat: { light: "gpt-5.4-mini", normal: null, deep: "gpt-5.5" },
    });
    expect(migrated.presets.map((preset) => preset.id)).toEqual([
      "fast",
      "default",
      "plan",
      "review",
    ]);
    expect(migrated.presets.find((preset) => preset.id === "fast")?.model).toBe("gpt-5.4-mini");

    const explanation = {
      mode: "auto" as const,
      model: "gpt-5.5",
      reasoningEffort: "high" as const,
      presetId: "plan",
      presetLabel: "Plan",
      source: "auto" as const,
      summary: "自动选择 [Plan] gpt-5.5",
      signals: ["计划模式"],
    };
    const nativeRuntimeOptions = mergeModelSelectionRuntimeOptions(
      "native-agentkit",
      { provider: { "native-agentkit": { maxTurns: 12 } } },
      "gpt-5.5",
      "high",
      explanation,
    );
    expect(nativeRuntimeOptions.common).toMatchObject({
      model: "gpt-5.5",
      reasoningEffort: "high",
      modelSelection: explanation,
    });
    expect(nativeRuntimeOptions.provider?.["native-agentkit"]).toMatchObject({
      maxTurns: 12,
      model: "gpt-5.5",
      reasoningEffort: "high",
      thinking: { type: "adaptive" },
    });
  });

  it("normalizes chat attachment payloads", () => {
    const attachment = {
      id: "a-1",
      name: "README.md",
      path: "D:/PROJECT/workspace/Lilia/README.md",
      kind: "file" as const,
      size: 120,
      exists: true,
      mime: "text/markdown",
      directory: null,
    };
    const imageAttachment = {
      ...attachment,
      id: "a-2",
      name: "shot.png",
      path: "D:/PROJECT/workspace/Lilia/shot.png",
      mime: "image/png",
    };
    const directoryAttachment = {
      ...attachment,
      id: "a-3",
      name: "src",
      path: "D:/PROJECT/workspace/Lilia/src",
      kind: "directory" as const,
      mime: null,
      directory: {
        fileCount: 12,
        directoryCount: 3,
        totalSize: 1536,
        truncated: false,
        unreadableCount: 1,
      },
    };
    const largeDirectoryAttachment = {
      ...directoryAttachment,
      directory: {
        fileCount: 200,
        directoryCount: 8,
        totalSize: 21 * 1024 * 1024,
        truncated: true,
        unreadableCount: 0,
      },
    };
    expect(chatAttachmentsToPayload([attachment])).toEqual([{
      id: "a-1",
      name: "README.md",
      path: "D:/PROJECT/workspace/Lilia/README.md",
      kind: "file",
      size: 120,
      exists: true,
      mime: "text/markdown",
      directory: null,
    }]);
    expect(chatAttachmentsToPayload([directoryAttachment])[0].directory).toEqual({
      fileCount: 12,
      directoryCount: 3,
      totalSize: 1536,
      truncated: false,
      unreadableCount: 1,
    });
    expect(isChatAttachment(attachment)).toBe(true);
    expect(isChatAttachment({ ...attachment, kind: "url" })).toBe(false);
    expect(isChatAttachment({ ...attachment, exists: "yes" })).toBe(false);
    expect(readChatAttachments([attachment, { ...attachment, size: "120" }, null])).toEqual([
      attachment,
    ]);
    expect(readChatAttachments([{
      ...attachment,
      exists: "yes",
      mime: 42,
      directory: { fileCount: "12" },
    }])).toEqual([{
      id: "a-1",
      name: "README.md",
      path: "D:/PROJECT/workspace/Lilia/README.md",
      kind: "file",
      size: 120,
    }]);
    expect(readChatAttachments([directoryAttachment])).toEqual([directoryAttachment]);
    expect(readChatAttachmentPayloadPaths([
      { path: "D:/PROJECT/workspace/Lilia/README.md" },
      { path: 42 },
      "invalid",
    ])).toEqual(["D:/PROJECT/workspace/Lilia/README.md"]);
    expect(isChatImageAttachment(imageAttachment)).toBe(true);
    expect(isChatImageAttachment({ ...imageAttachment, exists: false })).toBe(false);
    expect(chatAttachmentSizeLabel(1536)).toBe("2 KB");
    expect(chatAttachmentSizeLabel(null)).toBe("大小未知");
    expect(chatAttachmentDirectorySummaryLabel(directoryAttachment))
      .toBe("12 个文件 · 3 个目录 · 2 KB · 1 处不可读");
    expect(isLargeChatAttachmentDirectory(directoryAttachment)).toBe(false);
    expect(isLargeChatAttachmentDirectory(largeDirectoryAttachment)).toBe(true);
    expect(chatAttachmentMetaLabel(attachment)).toBe("120 B");
    expect(chatAttachmentMetaLabel(imageAttachment)).toBe("图片");
    expect(chatAttachmentMetaLabel({ ...attachment, exists: false })).toBe("路径不存在");
    expect(chatAttachmentMetaLabel(directoryAttachment))
      .toBe("12 个文件 · 3 个目录 · 2 KB · 1 处不可读");
    expect(chatAttachmentMetaLabel(largeDirectoryAttachment))
      .toBe("目录较大 · 200 个文件 · 8 个目录 · 21 MB · 未完全统计");
    expect(chatAttachmentReferenceLabel(attachment)).toBe("文件引用");
    expect(chatAttachmentReferenceLabel(directoryAttachment)).toBe("目录引用");
    expect(chatAttachmentReferenceLabel(imageAttachment)).toBe("图片引用");
    const imageLabel = serializeChatAttachmentReference(imageAttachment);
    expect(imageLabel).toBe("[图片引用: shot.png | D:/PROJECT/workspace/Lilia/shot.png]");
    const imageMatch = Array.from(imageLabel.matchAll(chatAttachmentReferencePattern()))[0];
    expect(resolveChatAttachmentReferenceMatch(imageMatch, [imageAttachment])).toEqual(imageAttachment);
    const fallbackMatch = Array.from(
      "[目录引用: docs | D:/PROJECT/workspace/Lilia/docs]".matchAll(chatAttachmentReferencePattern()),
    )[0];
    expect(resolveChatAttachmentReferenceMatch(fallbackMatch, [])).toEqual({
      id: "inline-D:/PROJECT/workspace/Lilia/docs",
      name: "docs",
      path: "D:/PROJECT/workspace/Lilia/docs",
      kind: "directory",
      size: null,
      exists: true,
      mime: null,
      directory: null,
    });
    const reference = {
      taskId: "task-1",
      title: "旧对话",
      route: "/tasks/task-1",
    };
    expect(deriveChatMessageDisplay(
      `看 ${serializeChatAttachmentReference(attachment)} 和 ${serializeConversationReference(reference)}，再看未引用图片`,
      [attachment, imageAttachment, directoryAttachment],
      [reference],
    )).toEqual({
      segments: [
        { type: "text", text: "看 " },
        { type: "attachment", attachment },
        { type: "text", text: " 和 " },
        { type: "conversationReference", reference },
        { type: "text", text: "，再看未引用图片" },
      ],
      previewAttachments: [imageAttachment],
      unreferencedAttachments: [directoryAttachment],
    });
  });

  it("normalizes conversation reference labels and payloads", () => {
    const reference = {
      taskId: "task-1",
      title: "复盘上下文",
      route: "/tasks/task-1",
      projectId: "project-1",
      projectName: "Lilia",
    };
    const label = serializeConversationReference(reference);
    expect(label).toBe("[对话引用: 复盘上下文 | task-1]");
    const match = Array.from(label.matchAll(conversationReferencePattern()))[0];
    expect(resolveConversationReferenceMatch(match, [reference])).toEqual(reference);
    expect(resolveConversationReferenceMatch(match, [])).toEqual({
      taskId: "task-1",
      title: "复盘上下文",
      route: "",
    });
    expect(conversationReferencesToPayload([reference])).toEqual([{
      taskId: "task-1",
      title: "复盘上下文",
      route: "/tasks/task-1",
      projectId: "project-1",
      projectName: "Lilia",
    }]);
    expect(readConversationReferences([
      { ...reference, projectId: null, projectName: null },
      { taskId: "bad", title: "missing route" },
      "invalid",
    ])).toEqual([{
      taskId: "task-1",
      title: "复盘上下文",
      route: "/tasks/task-1",
      projectId: undefined,
      projectName: undefined,
    }]);
    expect(stripSerializedConversationReferences(`继续 ${label} 处理`, [reference])).toBe("继续  处理");
  });

  it("labels chat slash command sources", () => {
    expect(chatSlashCommandSourceLabel("native")).toBe("内置");
    expect(chatSlashCommandSourceLabel("project")).toBe("项目");
    expect(isChatSlashCommandSource("native")).toBe(true);
    expect(isChatSlashCommandSource("remote")).toBe(false);
    expect(CHAT_WORKFLOW_SLASH_COMMANDS.map((item) => item.kind)).toEqual([
      "review",
      "fix_suggestion",
      "task:generalTask",
      "task:bugLocalization",
      "task:frontend",
      "task:refactor",
      "task:testAndVerification",
      "task:docsAndPrompt",
      "task:gitAndRelease",
      "task:architectureAndMemory",
    ]);
    expect(CHAT_WORKFLOW_SLASH_COMMANDS.map((item) => item.command.name)).toEqual([
      "review",
      "fix",
      "task",
      "debug",
      "frontend",
      "refactor",
      "verify",
      "docs",
      "git",
      "architecture",
    ]);
    expect(CHAT_WORKFLOW_SLASH_COMMANDS.map((item) => item.command.id)).toEqual([
      `workflow:${LILIA_REVIEW_WORKFLOW_TYPE}`,
      `workflow:${LILIA_FIX_SUGGESTION_WORKFLOW_TYPE}`,
      `workflow:${LILIA_TASK_WORKFLOW_TYPE}:generalTask`,
      `workflow:${LILIA_TASK_WORKFLOW_TYPE}:bugLocalization`,
      `workflow:${LILIA_TASK_WORKFLOW_TYPE}:frontend`,
      `workflow:${LILIA_TASK_WORKFLOW_TYPE}:refactor`,
      `workflow:${LILIA_TASK_WORKFLOW_TYPE}:testAndVerification`,
      `workflow:${LILIA_TASK_WORKFLOW_TYPE}:docsAndPrompt`,
      `workflow:${LILIA_TASK_WORKFLOW_TYPE}:gitAndRelease`,
      `workflow:${LILIA_TASK_WORKFLOW_TYPE}:architectureAndMemory`,
    ]);
    expect(createChatSlashCommandWorkflow({
      commandId: " native:help ",
      source: "native",
      arguments: { topic: "usage" },
    })).toEqual({
      type: "slash_command",
      commandId: "native:help",
      source: "native",
      arguments: { topic: "usage" },
    });
    expect(normalizeChatSlashCommandWorkflow({
      type: "slash_command",
      commandId: "project:release",
      source: "project",
      arguments: { count: 1, dryRun: true, ignored: null },
    })).toEqual({
      commandId: "project:release",
      source: "project",
      arguments: { count: "1", dryRun: "true" },
    });
    expect(() =>
      createChatSlashCommandWorkflow({ commandId: "help", source: "remote", arguments: {} } as never)
    ).toThrow("valid source");
    expect(chatWorkflowSlashKindLabel("review")).toBe("代码审查");
    expect(chatWorkflowSlashKindLabel("fix_suggestion")).toBe("修复建议");
    expect(chatWorkflowSlashKindLabel(null)).toBe("工作流");
  });

  it("labels suggestion sources", () => {
    const githubActivity = {
      id: "gh-1",
      repoFullName: "openai/lilia",
      kind: "pull_request",
      title: "Fix contracts #42",
      url: null,
    };
    expect(SUGGESTION_SOURCES).toEqual(["provider", "assistant-ai"]);
    expect(DEFAULT_SUGGESTION_SOURCE).toBe("assistant-ai");
    expect(SUGGESTION_SOURCE_LABELS).toEqual({
      provider: "当前 Provider",
      "assistant-ai": "辅助模型",
    });
    expect(SUGGESTION_SOURCE_SETTING_ORDER).toEqual(["assistant-ai", "provider"]);
    expect(CONVERSATION_SUGGESTIONS_GET_COMMAND).toBe("conversation_suggestions_get");
    expect(CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND).toBe(
      "conversation_suggestions_get_sources",
    );
    expect(CONVERSATION_SUGGESTIONS_GET_SETTINGS_COMMAND).toBe(
      "conversation_suggestions_get_settings",
    );
    expect(CONVERSATION_SUGGESTIONS_SET_SETTINGS_COMMAND).toBe(
      "conversation_suggestions_set_settings",
    );
    expect(suggestionSourceSettingLabel("assistant-ai")).toBe("辅助模型");
    expect(SUGGESTION_ITEM_SOURCES).toEqual(["task", "github", "local-git", "session-thread", "provider"]);
    expect(SUGGESTION_LOADING_SOURCE_LABELS).toMatchObject({
      task: "历史任务",
      github: "GitHub 活动",
      "session-thread": "会话线程",
      provider: "Provider 建议",
    });
    expect(SUGGESTION_LOCAL_GIT_LOADING_LABELS.default).toBe("本地 Git 上下文");
    expect(SUGGESTION_STATUSES).toEqual(["idle", "loading", "empty", "error"]);
    expect(DEFAULT_SUGGESTION_STATUS).toBe("idle");
    expect(isSuggestionSource("assistant-ai")).toBe(true);
    expect(isSuggestionSource("local")).toBe(false);
    expect(isSuggestionItemSource("local-git")).toBe(true);
    expect(isSuggestionItemSource("remote")).toBe(false);
    expect(isSuggestionStatus("loading")).toBe(true);
    expect(isSuggestionStatus("done")).toBe(false);
    expect(normalizeSuggestionSource("assistant-ai")).toBe("assistant-ai");
    expect(normalizeSuggestionSource("local")).toBe("assistant-ai");
    expect(normalizeSuggestionStatus("error")).toBe("error");
    expect(normalizeSuggestionStatus("done")).toBe("idle");
    expect(suggestionGitHubActivityAnchor(githubActivity)).toBe("PR #42");
    expect(suggestionGitHubActivityAnchor({ ...githubActivity, kind: "issue", title: "Bug #7" }))
      .toBe("Issue #7");
    expect(suggestionGitHubActivityAnchor({ ...githubActivity, kind: "push", title: "Push main: update" }))
      .toBe("Push main");
    expect(suggestionSourceLabel({
      githubActivities: [
        githubActivity,
        { ...githubActivity, id: "gh-2", title: "Bug #7" },
      ],
      localGitContexts: [],
      codexThreads: [],
    })).toBe("openai/lilia · PR #42 +1");
    expect(suggestionSourceLabel({
      githubActivities: [],
      localGitContexts: [{
        id: "git-1",
        branch: " feature/contracts ",
        status: "",
        changedFiles: [],
        recentCommits: [],
      }],
      codexThreads: [],
    })).toBe("本地 Git · feature/contracts");
    expect(suggestionSourceLabel({
      githubActivities: [],
      localGitContexts: [],
      codexThreads: [{ id: "thread-1", title: " 补齐建议 ", updatedAt: 1, preview: "预览" }],
    })).toBe("会话线程 · 补齐建议");
    expect(suggestionSourceLabel({ githubActivities: [], localGitContexts: [], codexThreads: [] })).toBe("");
    expect(conversationSuggestionLoadingText({ sources: ["provider"] })).toBe("正在读取 Provider 建议");
    expect(conversationSuggestionLoadingText({
      sources: ["task", "github", "local-git", "session-thread"],
      localGit: { hasRecentCommits: true, hasChangedFiles: true },
    })).toBe("正在检查历史任务、GitHub 活动、本地提交和未提交变更和会话线程");
    expect(conversationSuggestionLoadingText({ sources: [] })).toBe("正在寻找灵感");
    expect(suggestionStatusDisplayText({
      hasSuggestions: false,
      status: "loading",
      loadingText: " 自定义加载 ",
    })).toBe("自定义加载");
    expect(suggestionStatusDisplayText({ hasSuggestions: false, status: "error" })).toBe("建议暂时不可用");
    expect(suggestionStatusDisplayText({ hasSuggestions: true, status: "error" })).toBe("");
  });

  it.skip("normalizes agent interaction requests and pending payloads", () => {
    expect(AGENT_INTERACTION_KINDS).toContain("mcp_elicitation");
    expect(ASK_USER_INTERACTION_KIND).toBe("ask_user");
    expect(PLAN_APPROVAL_INTERACTION_KIND).toBe("plan_approval");
    expect(ASK_USER_INTERACTION_KINDS).toEqual([
      ASK_USER_INTERACTION_KIND,
      PLAN_APPROVAL_INTERACTION_KIND,
    ]);
    expect(askUserInteractionKindForSpec({ intent: undefined })).toBe(ASK_USER_INTERACTION_KIND);
    expect(askUserInteractionKindForSpec({ intent: PLAN_APPROVAL_INTENT })).toBe(PLAN_APPROVAL_INTERACTION_KIND);
    expect(TOOL_CONSENT_INTERACTION_KIND).toBe("tool_consent");
    expect(ARCHITECTURE_INTERACTION_KIND).toBe("architecture_change");
    expect(MCP_ELICITATION_INTERACTION_KIND).toBe("mcp_elicitation");
    expect(PERMISSION_APPROVAL_INTERACTION_KIND).toBe("permission_approval");
    expect(AGENT_INTERACTION_GET_SETTINGS_COMMAND).toBe("agent_interaction_get_settings");
    expect(AGENT_INTERACTION_SET_SETTINGS_COMMAND).toBe("agent_interaction_set_settings");
    expect(AGENT_INTERACTION_LIST_SUBAGENTS_COMMAND).toBe("agent_interaction_list_subagents");
    expect(AGENT_INTERACTION_UPSERT_SUBAGENT_COMMAND).toBe("agent_interaction_upsert_subagent");
    expect(AGENT_INTERACTION_DELETE_SUBAGENT_COMMAND).toBe("agent_interaction_delete_subagent");
    expect(RUNTIME_INTERACTION_KINDS).toEqual([
      MCP_ELICITATION_INTERACTION_KIND,
      PERMISSION_APPROVAL_INTERACTION_KIND,
    ]);
    expect(isAgentInteractionKind("permission_approval")).toBe(true);
    expect(isAgentInteractionKind("unknown")).toBe(false);
    expect(isAskUserInteractionKind("ask_user")).toBe(true);
    expect(isAskUserInteractionKind("plan_approval")).toBe(true);
    expect(isAskUserInteractionKind("tool_consent")).toBe(false);
    expect(PENDING_AUTO_DECISION_KINDS).toEqual([
      "ask_user",
      "plan_approval",
      "tool_consent",
      "title_update",
      "mcp_elicitation",
      "architecture_change",
      "permission_approval",
    ]);
    expect(PENDING_AUTO_DECISION_LABELS.title_update).toBe("同意标题");
    expect(isPendingAutoDecisionKind("architecture_change")).toBe(true);
    expect(isPendingAutoDecisionKind("unknown")).toBe(false);
    expect(pendingAutoDecisionLabel("mcp_elicitation")).toBe("提交 MCP 表单");
    expect(PLAN_APPROVAL_SPEC_DEFAULTS).toMatchObject({
      questionId: "approve-plan",
      questionHeader: "计划确认",
      confirmLabel: "按计划执行",
      cancelLabel: "先不执行",
      dismissable: true,
      titleTemplate: "确认 {backend} 计划",
      sourceTemplate: "{backend} Plan",
    });
    expect(normalizeMcpElicitationResult({
      action: "accept",
      content: { repo: "lilia" },
      _meta: { id: "mcp-1" },
    })).toEqual({
      action: "accept",
      content: { repo: "lilia" },
      _meta: { id: "mcp-1" },
    });
    expect(normalizeMcpElicitationResult({ action: "unknown" })).toEqual({
      action: "cancel",
      content: null,
      _meta: null,
    });
    expect(normalizePermissionApprovalResult({
      grantedAccess: { network: true },
      scope: "session",
    })).toEqual({
      action: "approve",
      grantedAccess: { network: true },
      scope: "session",
    });
    expect(normalizeRuntimeInteractionResult(PERMISSION_APPROVAL_INTERACTION_KIND, {
      strictAutoReview: true,
    })).toEqual({
      action: "cancel",
      grantedAccess: {},
      scope: "turn",
      strictAutoReview: true,
    });

    expect(normalizeAgentInteractionRequest({
      taskId: "t-1",
      turnId: "turn-1",
      backend: "native-agentkit",
      requestId: "mcp-1",
      kind: "mcp_elicitation",
      payload: {
        threadId: "thread-1",
        turnId: "turn-1",
        serverName: "github",
        mode: "form",
        message: "Pick a repo",
        requestedSchema: { type: "object" },
        url: 123,
        elicitationId: "elicit-1",
      },
    })).toMatchObject({
      taskId: "t-1",
      turnId: "turn-1",
      backend: "native-agentkit",
      requestId: "mcp-1",
      kind: "mcp_elicitation",
      payload: {
        threadId: "thread-1",
        turnId: "turn-1",
        serverName: "github",
        mode: "form",
        message: "Pick a repo",
        elicitationId: "elicit-1",
      },
    });
    expect(normalizeAgentInteractionRequest({
      taskId: "t-1",
      requestId: "bad-1",
      kind: "mcp_elicitation",
      payload: { threadId: "thread-1", serverName: "github", mode: "prompt" },
    })).toBeNull();
    expect(normalizeAgentInteractionRequest({
      taskId: "t-1",
      requestId: "ask-1",
      backend: "unknown",
      kind: "ask_user",
      payload: {
        title: "继续？",
        questions: [{ id: "confirm", question: "", mode: "confirm" }],
      },
    })).toMatchObject({
      backend: "native-agentkit",
      kind: "ask_user",
      payload: {
        title: "继续？",
        questions: [{ id: "confirm", question: "", mode: "confirm" }],
      },
    });
    expect(normalizeAgentInteractionRequest({
      taskId: "t-1",
      requestId: "bad-2",
      kind: "ask_user",
      payload: "not object",
    })).toBeNull();
    expect(normalizeAgentInteractionRequest({
      taskId: "t-1",
      requestId: "bad-3",
      kind: "ask_user",
      payload: { title: "missing questions", questions: [] },
    })).toBeNull();
    expect(normalizeMcpElicitationPayload({
      threadId: "thread-1",
      serverName: "linear",
      mode: "url",
      message: "Open Linear",
      url: "https://linear.app",
      _meta: { source: "test" },
    })).toEqual({
      threadId: "thread-1",
      turnId: null,
      serverName: "linear",
      mode: "url",
      message: "Open Linear",
      requestedSchema: undefined,
      url: "https://linear.app",
      elicitationId: undefined,
      _meta: { source: "test" },
    });
    expect(normalizePermissionApprovalPayload({
      reason: 123,
      requestedAccess: null,
      scopeSuggestion: { mode: "workspace" },
      providerContext: ["invalid"],
    })).toEqual({
      reason: null,
      requestedAccess: {},
      scopeSuggestion: { mode: "workspace" },
      providerContext: undefined,
    });
    const permissionPayload = normalizePermissionApprovalPayload({
      requestedAccess: { network: true },
      scopeSuggestion: "turn",
      providerContext: { codex: { threadId: "thread-1" } },
    })!;
    expect(createPermissionApprovalResult(permissionPayload, "allow")).toEqual({
      action: "approve",
      grantedAccess: { network: true },
      scope: "turn",
      providerContext: { codex: { threadId: "thread-1" } },
    });
    expect(createPermissionApprovalResult(permissionPayload, "deny")).toEqual({
      action: "decline",
      grantedAccess: {},
      scope: "turn",
      strictAutoReview: true,
      providerContext: { codex: { threadId: "thread-1" } },
    });
    expect(createMcpElicitationResult("accept", { repo: "Lilia" })).toEqual({
      action: "accept",
      content: { repo: "Lilia" },
    });
    expect(createMcpElicitationResult("cancel")).toEqual({ action: "cancel" });
    const toolRequest = normalizeToolConsentRequestFromInteraction(normalizeAgentInteractionRequest({
      taskId: "t-1",
      turnId: "turn-1",
      backend: "native-agentkit",
      requestId: "tool-1",
      kind: "tool_consent",
      payload: {
        toolName: 123,
        input: ["invalid"],
        title: "Run command",
        toolUseID: "codex-tool-1",
        availableDecisions: ["accept", 42, "decline"],
        cwd: "D:/PROJECT/workspace/Lilia",
      },
    })!);
    expect(toolRequest).toMatchObject({
      taskId: "t-1",
      turnId: "turn-1",
      backend: "native-agentkit",
      requestId: "tool-1",
      toolName: "tool",
      input: {},
      title: "Run command",
      toolUseId: "codex-tool-1",
      availableDecisions: ["accept", "decline"],
      cwd: "D:/PROJECT/workspace/Lilia",
    });
  });

  it.skip("derives editable tool consent command inputs", () => {
    expect(stringifyCodexToolCommand([
      { text: "yarn" },
      { value: "test" },
      "--runInBand",
    ])).toBe("yarn test --runInBand");

    const bashRequest = {
      taskId: "task-1",
      turnId: "turn-1",
      backend: "native-agentkit" as const,
      requestId: "bash-1",
      toolName: "Bash",
      input: { command: "pwd" },
      title: null,
      displayName: null,
      description: null,
      blockedPath: null,
      decisionReason: null,
      toolUseId: null,
    };
    expect(readEditableToolConsentCommand(bashRequest)).toBe("pwd");
    expect(createUpdatedToolConsentCommandInput(bashRequest, "pwd && echo ok")).toEqual({
      command: "pwd && echo ok",
    });
    expect(createUpdatedToolConsentCommandInput(bashRequest, "pwd")).toBeUndefined();
    expect(createUpdatedToolConsentCommandInput(bashRequest, "   ")).toBeUndefined();

    const codexRequest = {
      ...bashRequest,
      backend: "native-agentkit" as const,
      requestId: "codex-1",
      toolName: "item/commandExecution/requestApproval",
      input: { parsedCmd: [{ text: "pnpm" }, { arg: "test" }] },
      commandActions: [{ text: "ignored" }],
    };
    expect(readEditableToolConsentCommand(codexRequest)).toBe("pnpm test");
    expect(createUpdatedToolConsentCommandInput(codexRequest, "pnpm test --changed")).toEqual({
      parsedCmd: [{ text: "pnpm" }, { arg: "test" }],
      command: "pnpm test --changed",
    });
    expect(readEditableToolConsentCommand({
      ...codexRequest,
      toolName: "Read",
    })).toBe("");
  });

  it("normalizes AskUser specs", () => {
    expect(DEFAULT_ASK_USER_MODE).toBe("confirm");
    expect(DEFAULT_ASK_USER_MODE_WITH_OPTIONS).toBe("single");
    expect(ASK_USER_MODES).toEqual(["confirm", "single", "multi"]);
    expect(ASK_USER_INTENTS).toEqual([PLAN_APPROVAL_INTENT]);
    expect(ASK_USER_SINGLE_SELECT_MODE).toBe("single");
    expect(ASK_USER_MULTI_SELECT_MODE).toBe("multi");
    expect(MAX_ASK_USER_TOOL_QUESTIONS).toBe(4);
    expect(MIN_ASK_USER_TOOL_OPTIONS).toBe(2);
    expect(MAX_ASK_USER_TOOL_OPTIONS).toBe(4);
    expect(MAX_CODEX_REQUEST_USER_INPUT_OPTIONS).toBe(10);
    expect(MAX_ASK_USER_HEADER_TEXT).toBe(12);
    expect(MAX_ASK_USER_QUESTION_TEXT).toBe(1200);
    expect(ASK_USER_TOOL_NAMES).toEqual([
      "AskUserQuestion",
      "ask_user_question",
      "mcp__lilia__ask_user_question",
    ]);
    expect(ASK_USER_TOOL_NAME).toBe(ASK_USER_TOOL_NAMES[0]);
    expect(ASK_USER_CLAUDE_TOOL_NAME).toBe(ASK_USER_TOOL_NAMES[1]);
    expect(ASK_USER_MCP_TOOL_NAME).toBe(ASK_USER_TOOL_NAMES[2]);
    expect(isLiliaAskUserTool("AskUserQuestion")).toBe(true);
    expect(isLiliaAskUserTool("ask_user_question")).toBe(true);
    expect(isLiliaAskUserTool("mcp__lilia__ask_user_question")).toBe(true);
    expect(isLiliaAskUserTool("AskUser")).toBe(false);
    expect(ASK_USER_QUESTION_INPUT_SCHEMA).toMatchObject({
      type: "object",
      additionalProperties: false,
      required: ["questions"],
      properties: {
        questions: {
          type: "array",
          minItems: 1,
          maxItems: MAX_ASK_USER_TOOL_QUESTIONS,
          items: {
            required: ["question", "header", "options"],
            additionalProperties: false,
            properties: {
              options: {
                minItems: MIN_ASK_USER_TOOL_OPTIONS,
                maxItems: MAX_ASK_USER_TOOL_OPTIONS,
              },
              multiSelect: { type: "boolean", default: false },
            },
          },
        },
      },
    });
    expect(PLAN_APPROVAL_INTENT).toBe("plan_approval");
    expect(isAskUserMode("multi")).toBe(true);
    expect(isAskUserMode("prompt")).toBe(false);
    expect(isAskUserIntent(PLAN_APPROVAL_INTENT)).toBe(true);
    expect(isAskUserIntent("follow_up")).toBe(false);
    expect(normalizeAskUserMode("prompt")).toBe(DEFAULT_ASK_USER_MODE);
    expect(normalizeAskUserMode("prompt", "single")).toBe("single");
    expect(normalizeAskUserIntent(PLAN_APPROVAL_INTENT)).toBe(PLAN_APPROVAL_INTENT);
    expect(normalizeAskUserIntent("follow_up")).toBeUndefined();
    expect(ASK_USER_CONFIRM_ANSWER_VALUE).toBe("yes");
    expect(ASK_USER_CANCEL_ANSWER_VALUE).toBe("no");
    expect(PLAN_APPROVAL_REVISION_REQUEST_ANSWER_VALUE).toBe("revision_request");
    expect(isPlanApprovalAskUserSpec({ intent: PLAN_APPROVAL_INTENT })).toBe(true);
    expect(isPlanApprovalAskUserSpec({ intent: undefined })).toBe(false);
    const planApprovalSpec = createPlanApprovalAskUserSpec({
      title: "确认 Codex 计划",
      source: "Codex",
    });
    expect(planApprovalSpec).toEqual({
      title: "确认 Codex 计划",
      source: "Codex",
      intent: PLAN_APPROVAL_INTENT,
      dismissable: PLAN_APPROVAL_SPEC_DEFAULTS.dismissable,
      questions: [{
        id: PLAN_APPROVAL_SPEC_DEFAULTS.questionId,
        header: PLAN_APPROVAL_SPEC_DEFAULTS.questionHeader,
        question: "",
        mode: DEFAULT_ASK_USER_MODE,
        confirmLabel: PLAN_APPROVAL_SPEC_DEFAULTS.confirmLabel,
        cancelLabel: PLAN_APPROVAL_SPEC_DEFAULTS.cancelLabel,
      }],
    });
    expect(isPlanApprovalConfirmAskUserSpec(planApprovalSpec)).toBe(true);
    expect(isPlanApprovalConfirmAskUserSpec({
      ...planApprovalSpec,
      questions: [
        { ...planApprovalSpec.questions[0]!, mode: ASK_USER_SINGLE_SELECT_MODE },
      ],
    })).toBe(false);
    expect(isPlanApprovalConfirmAskUserSpec({
      ...planApprovalSpec,
      questions: [
        planApprovalSpec.questions[0]!,
        { ...planApprovalSpec.questions[0]!, id: "extra-plan-question" },
      ],
    })).toBe(false);
    expect(askUserSpecKey({
      title: "确认",
      questions: [{
        id: "q-1",
        question: "继续？",
        mode: "confirm",
      }],
    })).toBe(askUserSpecKey({
      questions: [{
        mode: "confirm",
        question: "继续？",
        id: "q-1",
      }],
      title: "确认",
    }));
    expect(normalizeAskUserSpec({
      questions: [{
        question: "带选项时默认单选。",
        mode: "prompt",
        options: [{ label: "保留" }],
      }],
    })?.questions[0]?.mode).toBe(DEFAULT_ASK_USER_MODE_WITH_OPTIONS);
    expect(normalizeAskUserSpec({
      title: "Lilia 想确认一下",
      source: 42,
      intent: "plan_approval",
      dismissable: true,
      questions: [
        {
          id: "q-1",
          header: "入口",
          question: "选一个入口。",
          mode: "single",
          options: [
            { id: "alpha", label: " Alpha ", recommended: true },
            { id: "bad" },
          ],
          minSelections: "1",
          maxSelections: 2.9,
        },
        {
          question: "无 id 时使用稳定 fallback。",
          mode: "multi",
          options: [{ label: "保留" }],
        },
      ],
    })).toEqual({
      title: "Lilia 想确认一下",
      source: undefined,
      intent: "plan_approval",
      dismissable: true,
      questions: [
        {
          id: "q-1",
          header: "入口",
          question: "选一个入口。",
          mode: "single",
          options: [{
            id: "alpha",
            label: " Alpha ",
            description: undefined,
            preview: undefined,
            icon: undefined,
            recommended: true,
            danger: undefined,
          }],
          confirmLabel: undefined,
          cancelLabel: undefined,
          danger: undefined,
          skippable: undefined,
          allowOther: undefined,
          minSelections: 1,
          maxSelections: 2,
        },
        {
          id: "question-2",
          header: undefined,
          question: "无 id 时使用稳定 fallback。",
          mode: "multi",
          options: [{
            id: undefined,
            label: "保留",
            description: undefined,
            preview: undefined,
            icon: undefined,
            recommended: undefined,
            danger: undefined,
          }],
          confirmLabel: undefined,
          cancelLabel: undefined,
          danger: undefined,
          skippable: undefined,
          allowOther: undefined,
          minSelections: undefined,
          maxSelections: undefined,
        },
      ],
    });
    expect(normalizeAskUserSpec({ title: "empty", questions: [] })).toBeNull();
  });

  it.skip("normalizes actionable timeline payloads", () => {
    const titleEvent = {
      id: "title-update:task-1:req-1",
      taskId: "task-1",
      turnId: "turn-1",
      backend: "native-agentkit" as const,
      kind: "title_update",
      status: "requires_action",
      title: "标题已更新",
      summary: "新标题",
      payload: {
        requestId: "req-1",
        proposedTitle: "新标题",
        previousTitle: "旧标题",
      },
      createdAt: 1,
      updatedAt: 1,
      turnSeq: 1,
      intraTurnOrder: 1,
    };
    expect(AGENT_TIMELINE_PENDING_INTERACTIONS).toEqual([
      "tool_consent",
      "mcp_elicitation",
      "permission_approval",
    ]);
    expect(AGENT_TIMELINE_ACTION_KIND_BY_EVENT_KIND).toMatchObject({
      title_update: "title_update",
      plan: "plan_approval",
      ask_user: "ask_user",
      architecture_change: "architecture_change",
    });
    expect(TITLE_UPDATE_ACTION_KIND).toBe(
      AGENT_TIMELINE_ACTION_KIND_BY_EVENT_KIND.title_update,
    );
    expect(isAgentTimelinePendingInteraction("tool_consent")).toBe(true);
    expect(isAgentTimelinePendingInteraction("title_update")).toBe(false);
    expect(readAgentTimelinePayloadString(titleEvent, "proposedTitle")).toBe("新标题");
    expect(agentTimelinePayloadRecord(titleEvent)).toMatchObject({
      requestId: "req-1",
      proposedTitle: "新标题",
    });
    expect(agentTimelinePayloadRecord({
      ...titleEvent,
      payload: "not record",
    })).toBeNull();
    expect(agentTimelinePayloadValueRecord({ path: "src/main.ts" })).toEqual({
      path: "src/main.ts",
    });
    expect(agentTimelinePayloadValueRecord(["not", "record"])).toBeNull();
    expect(timelineEventScopedKey(titleEvent)).toBe("task-1:title-update:task-1:req-1");
    expect(timelineEventScopedKey({ ...titleEvent, taskId: "task-2" })).toBe(
      "task-2:title-update:task-1:req-1",
    );
    expect(titleUpdateTimelinePayload(titleEvent)).toEqual({
      requestId: "req-1",
      proposedTitle: "新标题",
      previousTitle: "旧标题",
    });
    expect(agentTimelineEventRequiresAction(titleEvent)).toBe(true);
    expect(agentTimelineActionKind(titleEvent)).toBe("title_update");
    expect(agentTimelineActionDescriptor(titleEvent)).toEqual({
      kind: "title_update",
      requestId: "req-1",
    });
    expect(agentTimelineActionRequestId(titleEvent)).toBe("req-1");
    expect(agentTimelineEventRequestId({
      ...titleEvent,
      payload: { proposedTitle: "新标题" },
    }, "remembered-1")).toBe("remembered-1");
    expect(agentTimelineEventRequestId({
      ...titleEvent,
      id: "event-without-request",
      payload: { proposedTitle: "新标题" },
    })).toBe("timeline:event-without-request");
    expect(agentTimelineActionRequestId({
      ...titleEvent,
      kind: "plan",
      id: "plan-without-request",
      payload: { approved: null },
    })).toBeNull();
    expect(agentTimelineActionDescriptor({
      ...titleEvent,
      kind: "plan",
      id: "plan-without-request",
      payload: { approved: null },
    })).toBeNull();
    expect(agentTimelineEventRequiresAction({
      ...titleEvent,
      payload: { requestId: "req-1" },
    })).toBe(false);
    expect(agentTimelineActionKind({
      ...titleEvent,
      payload: { requestId: "req-1" },
    })).toBeNull();
    expect(agentTimelineActionKind({
      ...titleEvent,
      kind: "plan",
      payload: { approved: null },
    })).toBe("plan_approval");
    const planEvent = {
      ...titleEvent,
      kind: "plan",
      payload: { approved: null },
    };
    expect(agentTimelineActionMatches(planEvent, {
      kind: "plan_approval",
      requestId: null,
      turnId: titleEvent.turnId,
    })).toBe(true);
    expect(agentTimelineActionMatches({
      ...planEvent,
      payload: { requestId: "other-plan" },
    }, {
      kind: "plan_approval",
      requestId: "plan-1",
      turnId: titleEvent.turnId,
    })).toBe(false);
    expect(agentTimelineActionMatches({
      ...planEvent,
      payload: { requestId: "plan-1" },
    }, {
      kind: "plan_approval",
      requestId: "plan-1",
      turnId: "other-turn",
    })).toBe(true);
    expect(agentTimelineActionMatches(planEvent, {
      kind: "plan_approval",
      requestId: null,
      turnId: "other-turn",
    })).toBe(false);
    expect(agentTimelineActionMatches(titleEvent, {
      kind: "title_update",
      requestId: "req-1",
      turnId: null,
    })).toBe(true);
    expect(agentTimelineActionMatches(titleEvent, {
      kind: "title_update",
      requestId: "other-req",
      turnId: null,
    })).toBe(false);
    expect(timelinePlanStatusKind({
      ...titleEvent,
      kind: "plan",
      payload: { approved: null },
    })).toBe("pending");
    expect(timelinePlanStatusKind({
      ...titleEvent,
      kind: "plan",
      status: "success",
      payload: { approved: true },
    })).toBe("approved");
    expect(timelinePlanStatusKind({
      ...titleEvent,
      kind: "plan",
      status: "success",
      payload: { approved: false },
    })).toBe("rejected");
    expect(timelinePlanStatusKind({
      ...titleEvent,
      kind: "plan",
      status: "success",
      payload: { revisionRequest: " 修改 " },
    })).toBe("revision");
    expect(timelinePlanStatusKind({
      ...titleEvent,
      kind: "plan",
      status: "cancelled",
      payload: {},
    })).toBe("cancelled");
    expect(timelinePlanStatusKind({
      ...titleEvent,
      kind: "plan",
      status: "success",
      payload: {},
    }, true)).toBe("expired");
    expect(timelinePlanStatusLabel("revision")).toBe("修改要求");
    expect(agentTimelineActionKind({
      ...titleEvent,
      kind: "ask_user",
      payload: { requestId: "ask-1" },
    })).toBe("ask_user");
    expect(agentTimelineActionKind({
      ...titleEvent,
      kind: "architecture_change",
      payload: { requestId: "arch-1" },
    })).toBe("architecture_change");
    const mcpEvent = {
      ...titleEvent,
      id: "mcp:req-1",
      kind: "mcp",
      payload: { interaction: "mcp_elicitation", requestId: "mcp-1" },
    };
    expect(agentTimelinePendingInteraction(mcpEvent)).toBe("mcp_elicitation");
    expect(agentTimelineActionKind(mcpEvent)).toBe("mcp_elicitation");
    expect(agentTimelineEventRequiresAction(mcpEvent)).toBe(true);
    expect(agentTimelineActionRequestIds([titleEvent, mcpEvent, mcpEvent])).toEqual([
      "req-1",
      "mcp-1",
    ]);
    expect(agentTimelineEventRequiresAction({
      ...mcpEvent,
      payload: { interaction: "unknown", requestId: "mcp-1" },
    })).toBe(false);
    const finalReplyEvent = {
      ...titleEvent,
      kind: "message",
      status: "success",
      payload: {
        role: "assistant",
        content: "建议修复权限边界",
        workflowSource: { sourceKind: "fix_suggestion" },
      },
    };
    expect(timelineFinalReplyBatchApplyInput(finalReplyEvent, "建议修复权限边界")).toEqual({
      sourceTurnId: "turn-1",
      sourceKind: "fix_suggestion",
      sourceSummary: "建议修复权限边界",
    });
    expect(timelineFinalReplyBatchApplyInput({
      ...finalReplyEvent,
      turnId: null,
    }, "建议修复权限边界")).toBeNull();
    expect(timelineFinalReplyBatchApplyInput({
      ...finalReplyEvent,
      payload: { workflowSource: { sourceKind: "unsupported" } },
    }, "建议修复权限边界")).toBeNull();
    expect(timelineFinalReplyBatchApplyInput(finalReplyEvent, "   ")).toBeNull();
    expect(timelineChatMessageFromEvent({
      ...titleEvent,
      id: "message-1",
      kind: "message",
      summary: "fallback content",
      payload: {
        role: "system",
        content: "系统提示",
        queued: true,
        attachments: [{
          id: "att-1",
          name: "README.md",
          path: "D:/PROJECT/workspace/Lilia/README.md",
          kind: "file",
          size: 120,
        }],
        conversationReferences: [{
          taskId: "task-old",
          title: "旧对话",
          route: "/tasks/task-old",
        }],
      },
    })).toEqual({
      id: "message-1",
      taskId: "task-1",
      role: "system",
      content: "系统提示",
      attachments: [{
        id: "att-1",
        name: "README.md",
        path: "D:/PROJECT/workspace/Lilia/README.md",
        kind: "file",
        size: 120,
      }],
      conversationReferences: [{
        taskId: "task-old",
        title: "旧对话",
        route: "/tasks/task-old",
        projectId: undefined,
        projectName: undefined,
      }],
      createdAt: 1,
      queued: true,
    });
    const mcpProcessEvent = {
      ...titleEvent,
      id: "mcp-process-1",
      kind: "mcp",
      status: "success" as const,
      title: "docs / search",
      summary: "docs / search",
      payload: { server: "docs", tool: "search" },
      createdAt: 1000,
      updatedAt: 2500,
    };
    const toolProcessEvent = {
      ...titleEvent,
      id: "tool-process-1",
      kind: "tool",
      status: "success" as const,
      title: "Read",
      summary: "src/App.vue",
      payload: { toolName: "Read" },
      createdAt: 3000,
      updatedAt: 3500,
    };
    expect(timelineDeclaredGroupUnit(mcpProcessEvent)).toEqual({
      key: "mcp",
      count: 1,
      unit: "次 MCP",
    });
    expect(AGENT_TIMELINE_TOOL_WINDOW_KINDS).toEqual([
      "tool",
      "command",
      "mcp",
      "search",
      "web_search",
      "web_fetch",
      "file_read",
      "file_change",
      "todo_list",
      ARCHITECTURE_INTERACTION_KIND,
      "subagent",
    ]);
    expect(isAgentTimelineToolWindowKind("tool")).toBe(true);
    expect(isAgentTimelineToolWindowKind(ARCHITECTURE_INTERACTION_KIND)).toBe(true);
    expect(isAgentTimelineToolWindowKind("message")).toBe(false);
    expect(timelineDeclaredGroupUnitFromDisplay({
      group: { key: "tool:Read", bucket: "tool", unit: "个工具", count: 1 },
    })).toEqual({
      key: "tool",
      count: 1,
      unit: "个工具",
    });
    expect(timelineActionStatusLabel("running", "调用 MCP")).toBe("正在调用 MCP");
    expect(timelineActionStatusLabel("failed", "调用 MCP")).toBe("调用 MCP失败");
    expect(timelineEventLabelFromDisplay({
      action: "调用 MCP",
      object: "docs/search",
      objectInLabel: true,
    }, "success")).toBe("已调用 MCP docs/search");
    expect(timelineGroupLabelFromDisplay({
      action: "调用 MCP",
      group: { key: "mcp:docs", bucket: "mcp", unit: "次 MCP", count: 1 },
    }, 2, "running")).toBe("正在调用 MCP 2 次 MCP");
    expect(timelineProcessEventsSummary([
      mcpProcessEvent,
      toolProcessEvent,
    ], {
      createdAt: 5000,
    })).toBe("MCP 调用、工具调用 · 3 秒");
    expect(timelineLatestEventTimeMs([mcpProcessEvent, {
      ...toolProcessEvent,
      updatedAt: 7000,
    }])).toBe(7000);
    expect(timelineThinkingDurationLabel(null, 10_000)).toBe("思考中");
    expect(timelineThinkingDurationLabel(0, 128_000)).toBe("思考中 2 分 8 秒");
    expect(timelineThinkingDurationLabel(1000, 1500)).toBe("思考中 1 秒");
    expect(timelineRunningSubagentLabel([{
      ...titleEvent,
      id: "subagent-1",
      kind: "subagent",
      status: "running" as const,
      title: "explorer",
      summary: "Read",
      payload: {
        subagentType: "explorer",
        lastToolName: "Read",
      },
    }])).toBe("子代理explorer正在调用工具");
    expect(AGENT_TIMELINE_RUNNING_STATUSES).toEqual([
      "pending",
      "started",
      "running",
      "in_progress",
    ]);
    expect(AGENT_TIMELINE_TERMINAL_STATUSES).toEqual([
      "success",
      "completed",
      "done",
      "error",
      "failed",
      "cancelled",
    ]);
    expect(DEFAULT_TIMELINE_STATUS).toBe("info");
    expect(TIMELINE_STATUS_ALIASES).toMatchObject({
      failed: "error",
      completed: "success",
      in_progress: "running",
    });
    expect(normalizeTimelineStatus(null)).toBe("info");
    expect(normalizeTimelineStatus("failed")).toBe("error");
    expect(normalizeTimelineStatus("completed")).toBe("success");
    expect(normalizeTimelineStatus("in_progress")).toBe("running");
    expect(normalizeTimelineStatus("requires_action")).toBe("requires_action");
    expect(TIMELINE_DISPLAY_TEXT_LIMITS).toMatchObject({
      title: 200,
      summary: 1200,
      detail: 6000,
      inline: 600,
      short: 200,
      tiny: 80,
      oneLine: 400,
      errorSummary: 400,
      todoItem: 120,
      todoStep: 160,
      pathPreview: 240,
      askUserHeader: 40,
      askUserQuestionPreview: 240,
      allowedPrompt: 400,
      claudePlan: 12000,
      commandInlineThreshold: 180,
      fileChangePath: 600,
    });
    expect(TIMELINE_DISPLAY_TITLE_TEXT_LIMIT).toBe(200);
    expect(TIMELINE_DISPLAY_SUMMARY_TEXT_LIMIT).toBe(1200);
    expect(TIMELINE_DISPLAY_DETAIL_TEXT_LIMIT).toBe(6000);
    expect(TIMELINE_DISPLAY_INLINE_TEXT_LIMIT).toBe(600);
    expect(TIMELINE_DISPLAY_SHORT_TEXT_LIMIT).toBe(200);
    expect(TIMELINE_DISPLAY_TINY_TEXT_LIMIT).toBe(80);
    expect(TIMELINE_DISPLAY_ONE_LINE_TEXT_LIMIT).toBe(400);
    expect(TIMELINE_DISPLAY_ERROR_SUMMARY_TEXT_LIMIT).toBe(400);
    expect(TIMELINE_DISPLAY_TODO_ITEM_TEXT_LIMIT).toBe(120);
    expect(TIMELINE_DISPLAY_TODO_STEP_TEXT_LIMIT).toBe(160);
    expect(TIMELINE_DISPLAY_PATH_PREVIEW_TEXT_LIMIT).toBe(240);
    expect(TIMELINE_DISPLAY_ASK_USER_HEADER_TEXT_LIMIT).toBe(40);
    expect(TIMELINE_DISPLAY_ASK_USER_QUESTION_PREVIEW_TEXT_LIMIT).toBe(240);
    expect(TIMELINE_DISPLAY_ALLOWED_PROMPT_TEXT_LIMIT).toBe(400);
    expect(TIMELINE_DISPLAY_CLAUDE_PLAN_TEXT_LIMIT).toBe(12000);
    expect(TIMELINE_DISPLAY_COMMAND_INLINE_THRESHOLD).toBe(180);
    expect(TIMELINE_DISPLAY_FILE_CHANGE_PATH_TEXT_LIMIT).toBe(600);
    expect(TIMELINE_PAYLOAD_MAX_DEPTH).toBe(5);
    expect(TIMELINE_PAYLOAD_RESERVED_KEYS).toEqual([
      "taskId",
      "task_id",
      "turnId",
      "turn_id",
      "order",
      "thinking",
      "redacted_thinking",
      "signature",
    ]);
    expect(isAgentTimelineRunningStatus("running")).toBe(true);
    expect(isAgentTimelineRunningStatus("success")).toBe(false);
    expect(isAgentTimelineTerminalStatus("success")).toBe(true);
    expect(isAgentTimelineTerminalStatus("running")).toBe(false);
    expect(isTimelineMessageEvent(finalReplyEvent)).toBe(true);
    expect(isTimelineMessageEvent({ ...titleEvent, kind: "tool" })).toBe(false);
    expect(isTimelineFinalReply(finalReplyEvent)).toBe(true);
    expect(isTimelineUserMessage(finalReplyEvent)).toBe(false);
    expect(isTimelineUserMessage({
      ...finalReplyEvent,
      payload: { role: "user", content: "请修复" },
    })).toBe(true);
    expect(isTimelineUserMessage({
      ...finalReplyEvent,
      payload: { role: "system", content: "系统提示" },
    })).toBe(true);
    expect(readTimelineFinalText(finalReplyEvent)).toBe("建议修复权限边界");
    expect(isTimelineFinalReplyStreaming({
      ...finalReplyEvent,
      status: "running",
    })).toBe(true);
    const failedEvent = {
      ...titleEvent,
      kind: "error",
      status: "failed",
      title: "运行失败",
      summary: "",
      payload: { stderr: "权限不足" },
    };
    expect(isTimelineErrorReply(failedEvent)).toBe(true);
    expect(readTimelineFinalText(failedEvent)).toBe("权限不足");
    expect(isTimelineInterruptEvent({
      ...failedEvent,
      payload: { interrupted: true, stderr: "用户中断" },
    })).toBe(true);
    expect(isTimelineProcessAnchor(finalReplyEvent)).toBe(true);
    expect(isTimelineErrorReply({
      ...failedEvent,
      payload: { interrupted: true, stderr: "用户中断" },
    })).toBe(false);
    expect(isTimelineProcessAnchor({
      ...failedEvent,
      payload: { interrupted: true, stderr: "用户中断" },
    })).toBe(true);
    expect(isTimelineProcessAnchor({ ...titleEvent, kind: "tool" })).toBe(false);
    expect(isHiddenTimelineEvent({ ...titleEvent, kind: "reasoning" })).toBe(true);
    expect(aggregateTimelineStatus([
      { status: "success" },
      { status: "running" },
    ])).toBe("running");
    expect(aggregateTimelineStatus([
      { status: "success" },
      { status: "cancelled" },
    ])).toBe("failed");
    expect(aggregateTimelineStatus([{ status: "success" }])).toBe("completed");
    const earlierGoal = {
      ...titleEvent,
      id: "goal-1",
      kind: "goal",
      updatedAt: 10,
      payload: {
        goal: {
          objective: "清理协议边界",
          status: "active",
          tokenUsage: null,
          tokenBudget: null,
        },
      },
    };
    const laterGoal = {
      ...earlierGoal,
      id: "goal-2",
      updatedAt: 20,
      payload: {
        goal: {
          objective: "继续重构",
          status: "active",
          tokenUsage: null,
          tokenBudget: null,
        },
      },
    };
    expect(latestLiliaGoalFromTimeline([earlierGoal, laterGoal])).toEqual({
      objective: "继续重构",
      status: "active",
      tokenUsage: null,
      tokenBudget: null,
    });
    expect(latestLiliaGoalFromTimeline([
      earlierGoal,
      { ...laterGoal, payload: { cleared: true } },
    ])).toBeNull();
  });

  it.skip("normalizes project architecture interaction payloads", () => {
    expect(PROJECT_ARCHITECTURE_BACKENDS).toEqual(["native-agentkit"]);
    expect(PROJECT_ARCHITECTURE_PERMISSIONS).toEqual(["ask", "full", "readonly"]);
    expect(PROJECT_ARCHITECTURE_DEFAULT_PERMISSION).toBe("ask");
    expect(PROJECT_ARCHITECTURE_ROLLBACK_PERMISSION).toBe("full");
    expect(PROJECT_ARCHITECTURE_RUNTIME_PERMISSION_MAP).toMatchObject({
      full: "full",
      free: "full",
      readonly: "readonly",
    });
    expect(PROJECT_ARCHITECTURE_CHANGE_STATUSES).toEqual([
      "proposed",
      "pending",
      "applied",
      "rejected",
      "rolled_back",
    ]);
    expect(PROJECT_ARCHITECTURE_DEFAULT_CHANGE_STATUS).toBe("pending");
    expect(PROJECT_ARCHITECTURE_DEFAULT_INTERACTION_STATUS).toBe("pending");
    expect(PROJECT_ARCHITECTURE_READONLY_INTERACTION_STATUS).toBe("proposed");
    expect(PROJECT_ARCHITECTURE_APPLIED_INTERACTION_STATUS).toBe("applied");
    expect(PROJECT_ARCHITECTURE_DEFAULT_NODE_TYPE).toBe("module");
    expect(PROJECT_ARCHITECTURE_DEFAULT_EDGE_TYPE).toBe("depends_on");
    expect(PROJECT_ARCHITECTURE_GET_COMMAND).toBe("project_architecture_get");
    expect(PROJECT_ARCHITECTURE_LIST_CHANGES_COMMAND).toBe(
      "project_architecture_list_changes",
    );
    expect(PROJECT_ARCHITECTURE_APPLY_COMMAND).toBe("project_architecture_apply");
    expect(PROJECT_ARCHITECTURE_REJECT_COMMAND).toBe("project_architecture_reject");
    expect(PROJECT_ARCHITECTURE_ROLLBACK_COMMAND).toBe(
      "project_architecture_rollback",
    );
    expect(UPDATE_PROJECT_ARCHITECTURE_INPUT_SCHEMA).toMatchObject({
      type: "object",
      additionalProperties: false,
      required: ["changes"],
      properties: {
        changes: {
          type: "array",
          minItems: 1,
          items: {
            oneOf: expect.arrayContaining([
              expect.objectContaining({
                required: ["type", "node"],
                properties: expect.objectContaining({
                  type: { const: "upsert_node" },
                }),
              }),
              expect.objectContaining({
                required: ["type", "edge"],
                properties: expect.objectContaining({
                  type: { const: "upsert_edge" },
                }),
              }),
            ]),
          },
        },
      },
    });
    expect(isProjectArchitecturePermission("full")).toBe(true);
    expect(isProjectArchitecturePermission("free")).toBe(false);
    expect(ARCHITECTURE_TOOL_NAMES).toEqual([
      "UpdateProjectArchitecture",
      "update_project_architecture",
      "mcp__lilia__update_project_architecture",
    ]);
    expect(ARCHITECTURE_TOOL_NAME).toBe(ARCHITECTURE_TOOL_NAMES[0]);
    expect(ARCHITECTURE_CLAUDE_TOOL_NAME).toBe(ARCHITECTURE_TOOL_NAMES[1]);
    expect(ARCHITECTURE_MCP_TOOL_NAME).toBe(ARCHITECTURE_TOOL_NAMES[2]);
    expect(isLiliaArchitectureTool("UpdateProjectArchitecture")).toBe(true);
    expect(isLiliaArchitectureTool("update_project_architecture")).toBe(true);
    expect(isLiliaArchitectureTool("mcp__lilia__update_project_architecture")).toBe(true);
    expect(isLiliaArchitectureTool("project_architecture")).toBe(false);
    expect(normalizeProjectArchitecturePermission("free")).toBe("ask");
    expect(projectArchitecturePermissionFromRuntimePermission("free")).toBe("full");
    expect(projectArchitecturePermissionFromRuntimePermission("readonly")).toBe("readonly");
    expect(projectArchitecturePermissionFromRuntimePermission("danger")).toBe("ask");
    expect(isProjectArchitectureChangeStatus("rolled_back")).toBe(true);
    expect(isProjectArchitectureChangeStatus("queued")).toBe(false);
    expect(normalizeProjectArchitectureChangeStatus("queued")).toBe("pending");
    expect(normalizeProjectArchitectureChange({
      type: "upsert_node",
      node: {
        id: "node-1",
        label: "Runtime",
        type: 42,
        paths: ["src/runtime.ts", 123],
        tags: ["chat", null],
      },
    })).toEqual({
      type: "upsert_node",
      node: {
        id: "node-1",
        label: "Runtime",
        type: "module",
        summary: "",
        paths: ["src/runtime.ts"],
        tags: ["chat"],
      },
    });
    expect(normalizeProjectArchitectureChange({
      type: "remove_edge",
      edgeId: "",
    })).toBeNull();
    expect(projectArchitectureChangeText({
      type: "upsert_node",
      node: {
        id: "runtime",
        label: "Runtime",
        type: "module",
        summary: "",
        paths: [],
        tags: [],
      },
    })).toBe("更新节点：Runtime");
    expect(projectArchitectureChangeTextFromPayload({
      type: "upsert_edge",
      edge: {
        id: "edge-1",
        from: "runtime",
        to: "ui",
        type: "calls",
        label: "",
        summary: "",
      },
    })).toBe("更新关系：edge-1");

    expect(normalizeProjectArchitectureInteractionPayload({
      projectId: "project-1",
      taskId: 42,
      turnId: "turn-1",
      backend: "native-agentkit",
      permission: "free",
      reason: 123,
      status: "invalid",
      requiresConfirmation: false,
      changes: [
        { type: "set_summary", summary: "分层更清晰" },
        { type: "remove_node", nodeId: "" },
      ],
    })).toEqual({
      projectId: "project-1",
      taskId: "",
      turnId: "turn-1",
      backend: "native-agentkit",
      permission: "ask",
      reason: "",
      changes: [{ type: "set_summary", summary: "分层更清晰" }],
      requestId: null,
      status: "pending",
      requiresConfirmation: false,
    });
    expect(normalizeProjectArchitectureInteractionPayload({
      projectId: "project-1",
      changes: [{ type: "remove_node", nodeId: "" }],
    })).toBeNull();
    expect(normalizeAgentInteractionRequest({
      taskId: "task-1",
      turnId: "turn-1",
      requestId: "arch-1",
      kind: "architecture_change",
      payload: {
        projectId: "project-1",
        backend: "unknown",
        permission: "full",
        changes: [{ type: "remove_node", nodeId: "node-1" }],
      },
    })).toMatchObject({
      kind: "architecture_change",
      backend: "native-agentkit",
      payload: {
        projectId: "project-1",
        backend: "native-agentkit",
        permission: "full",
        status: "pending",
        requiresConfirmation: true,
      },
    });
    const architecturePayload = normalizeProjectArchitectureInteractionPayload({
      projectId: "project-1",
      taskId: "",
      turnId: null,
      backend: "native-agentkit",
      permission: "readonly",
      reason: "sync graph",
      changes: [{ type: "remove_node", nodeId: "node-1" }],
      requestId: "payload-request",
      requiresConfirmation: true,
    })!;
    const architectureEnvelope = {
      taskId: "task-1",
      turnId: "turn-1",
      backend: "native-agentkit" as const,
      requestId: "arch-1",
      payload: architecturePayload,
    };
    expect(projectArchitectureApplyInputFromInteraction(architectureEnvelope)).toEqual({
      projectId: "project-1",
      taskId: "task-1",
      turnId: "turn-1",
      backend: "native-agentkit",
      permission: "readonly",
      reason: "sync graph",
      changes: [{ type: "remove_node", nodeId: "node-1" }],
      requestId: "arch-1",
    });
    expect(projectArchitectureRejectInputFromInteraction(architectureEnvelope)).toEqual(
      projectArchitectureApplyInputFromInteraction(architectureEnvelope),
    );
  });

  it.skip("derives plan payload rows for timeline rendering", () => {
    const payload = {
      allowedPrompts: [
        { tool: "Shell", prompt: "运行 yarn verify:contracts" },
        { tool: 42, prompt: "   " },
      ],
      architectureImpacts: [
        {
          reason: "更新边界",
          changes: [
            {
              type: "remove_node",
              nodeId: "old-runtime",
            },
            {
              type: "upsert_edge",
              edge: {
                id: "runtime-ui",
                from: "runtime",
                to: "ui",
                type: "calls",
                label: "渲染事件",
                summary: "",
              },
            },
            { type: "unknown" },
          ],
        },
      ],
    };
    expect(agentTimelinePlanAllowedPromptRows(payload)).toEqual([
      { index: 0, text: "Shell：运行 yarn verify:contracts" },
    ]);
    expect(agentTimelinePlanArchitectureImpactRows(payload)).toEqual([
      { impactIndex: 0, rowIndex: 0, text: "更新边界" },
      { impactIndex: 0, rowIndex: 1, text: "移除节点：old-runtime" },
      { impactIndex: 0, rowIndex: 2, text: "更新关系：渲染事件" },
      { impactIndex: 0, rowIndex: 3, text: "unknown" },
    ]);
  });

  it.skip("normalizes Codex profile settings", () => {
    expect(CODEX_REASONING_EFFORTS).toEqual(["low", "medium", "high", "xhigh"]);
    expect(CODEX_SETTINGS_PROFILES).toEqual(["default", "fast", "balanced", "deep"]);
    expect(isCodexReasoningEffort("xhigh")).toBe(true);
    expect(isCodexReasoningEffort("extreme")).toBe(false);
    expect(isCodexSettingsProfile("deep")).toBe(true);
    expect(isCodexSettingsProfile("custom")).toBe(false);

    expect(normalizeCodexProfileSettings({
      profile: "deep",
      model: " gpt-5.5 ",
      reasoningEffort: "extreme" as never,
      runtimeWorkspaceRoots: [" D:/PROJECT/workspace/Lilia ", "", "D:/PROJECT/workspace/Lilia"],
      responsesApiClientMetadata: { app: "lilia" },
      additionalContext: "  use contracts ",
      persistExtendedHistory: true,
      initialTurnsPage: ["invalid"] as never,
      excludeTurns: [" turn-1 ", "turn-1"],
    })).toEqual({
      profile: "deep",
      model: "gpt-5.5",
      reasoningEffort: null,
      runtimeWorkspaceRoots: ["D:/PROJECT/workspace/Lilia"],
      responsesApiClientMetadata: { app: "lilia" },
      additionalContext: "use contracts",
      persistExtendedHistory: true,
      initialTurnsPage: null,
      excludeTurns: ["turn-1"],
    });
    expect(normalizeLiliaCodeCoreCodexProfileSettings({
      profile: "custom" as never,
      runtimeWorkspaceRoots: [" C:/repo ", "C:/repo"],
    })).toMatchObject({
      profile: "default",
      runtimeWorkspaceRoots: ["C:/repo"],
    });
  });

  it.skip("normalizes agent interaction settings", () => {
    expect(normalizeAgentInteractionSettings(null)).toEqual(DEFAULT_AGENT_INTERACTION_SETTINGS);
    expect(normalizeAgentInteractionSettings({
      nonInterruptMode: true,
      debug: true,
      permissionMode: "free",
      permissionModeAvailability: {
        ask: false,
        readonly: false,
        full: true,
        free: true,
      },
      mainAgentPromptMode: "custom",
      mainAgentCustomPrompt: "  custom strategy\nwith details  ",
      codexProfile: {
        profile: "fast",
        model: " gpt-5.4 ",
        reasoningEffort: "high",
        runtimeWorkspaceRoots: [" D:/repo "],
        responsesApiClientMetadata: null,
        additionalContext: "",
        persistExtendedHistory: false,
        initialTurnsPage: { cursor: "abc" },
        excludeTurns: [" t-1 ", "t-1"],
      },
      subagentMode: {
        enabled: true,
        codex: { enabled: false },
        claude: {
          enabled: false,
          forwardSubagentText: false,
          agentProgressSummaries: true,
        },
      },
      autoTurnDecision: {
        enabled: false,
        allowModelTier: false,
        allowReasoningEffort: true,
        allowPlanMode: false,
        allowGoalMode: true,
        allowSessionFork: false,
      },
    })).toMatchObject({
      nonInterruptMode: true,
      debug: true,
      permissionMode: "free",
      permissionModeAvailability: {
        ask: true,
        readonly: true,
        full: true,
        free: true,
      },
      mainAgentPromptMode: "custom",
      mainAgentCustomPrompt: "custom strategy\nwith details",
      codexProfile: {
        profile: "fast",
        model: "gpt-5.4",
        reasoningEffort: "high",
        runtimeWorkspaceRoots: ["D:/repo"],
        additionalContext: null,
        persistExtendedHistory: false,
        initialTurnsPage: { cursor: "abc" },
        excludeTurns: ["t-1"],
      },
      subagentMode: {
        enabled: true,
        codex: { enabled: false },
        claude: {
          enabled: false,
          forwardSubagentText: false,
          agentProgressSummaries: true,
        },
      },
      autoTurnDecision: {
        enabled: false,
        allowModelTier: false,
        allowReasoningEffort: true,
        allowPlanMode: false,
        allowGoalMode: true,
        allowSessionFork: false,
      },
    });
    expect(normalizeAgentInteractionSettings({
      permissionMode: "free",
      permissionModeAvailability: {
        ask: false,
        readonly: false,
        full: false,
        free: false,
      },
    })).toMatchObject({
      permissionMode: "ask",
      permissionModeAvailability: {
        ask: true,
        readonly: true,
        full: false,
        free: false,
      },
    });
    expect(normalizePermissionModeAvailability({
      ask: false,
      readonly: false,
      full: false,
      free: true,
    })).toEqual({
      ask: true,
      readonly: true,
      full: false,
      free: true,
    });
    expect(enabledPermissionModes({
      ask: true,
      readonly: true,
      full: false,
      free: true,
    })).toEqual(["ask", "readonly", "free"]);
    expect(normalizeAvailablePermissionMode("free", {
      ask: true,
      readonly: true,
      full: true,
      free: false,
    })).toBe("ask");
    expect(normalizeAgentInteractionSettings({
      nonInterruptMode: "yes" as never,
      debug: 1 as never,
      mainAgentPromptMode: "deep" as never,
      mainAgentCustomPrompt: 42 as never,
      subagentMode: {
        enabled: "true" as never,
        codex: { enabled: 0 as never },
        claude: {
          enabled: null as never,
          forwardSubagentText: "false" as never,
          agentProgressSummaries: false,
        },
      },
      autoTurnDecision: {
        enabled: "false" as never,
        allowModelTier: false,
        allowReasoningEffort: null as never,
        allowPlanMode: undefined as never,
        allowGoalMode: true,
        allowSessionFork: 1 as never,
      },
    })).toMatchObject({
      nonInterruptMode: false,
      debug: false,
      mainAgentPromptMode: "conservative",
      mainAgentCustomPrompt: "",
      subagentMode: {
        enabled: false,
        codex: { enabled: true },
        claude: {
          enabled: true,
          forwardSubagentText: true,
          agentProgressSummaries: false,
        },
      },
      autoTurnDecision: {
        enabled: true,
        allowModelTier: false,
        allowReasoningEffort: true,
        allowPlanMode: true,
        allowGoalMode: true,
        allowSessionFork: true,
      },
    });
  });

  it("normalizes memory settings and tags", () => {
    expect(DEFAULT_MEMORY_SETTINGS).toEqual({
      enabled: true,
      baselineInjectionEnabled: true,
      cooldownTurns: 5,
    });
    expect(normalizeMemorySettings({
      enabled: false,
      baselineInjectionEnabled: true,
      cooldownTurns: 2.8,
    })).toEqual({
      enabled: false,
      baselineInjectionEnabled: true,
      cooldownTurns: 2,
    });
    expect(normalizeMemorySettings({ cooldownTurns: 0 })).toMatchObject({ cooldownTurns: 5 });
    expect(normalizeMemoryTags(" release, check, release ,, ")).toEqual(["release", "check"]);
    expect(MEMORY_SCOPES).toEqual(["user", "project"]);
    expect(DEFAULT_MEMORY_SCOPE).toBe("project");
    expect(MEMORY_LIST_COMMAND).toBe("memory_list");
    expect(MEMORY_UPSERT_COMMAND).toBe("memory_upsert");
    expect(MEMORY_SET_ENABLED_COMMAND).toBe("memory_set_enabled");
    expect(MEMORY_DELETE_COMMAND).toBe("memory_delete");
    expect(MEMORY_GET_SETTINGS_COMMAND).toBe("memory_get_settings");
    expect(MEMORY_SET_SETTINGS_COMMAND).toBe("memory_set_settings");
    expect(MEMORY_GET_INJECTION_STATE_COMMAND).toBe("memory_get_injection_state");
    expect(MEMORY_SET_TASK_ENABLED_COMMAND).toBe("memory_set_task_enabled");
    expect(MEMORY_RESET_TASK_COOLDOWN_COMMAND).toBe("memory_reset_task_cooldown");
    expect(isMemoryScope("user")).toBe(true);
    expect(isMemoryScope("workspace")).toBe(false);
    expect(normalizeMemoryScope("workspace")).toBe(DEFAULT_MEMORY_SCOPE);
    expect(normalizeMemoryScope("workspace", "user")).toBe("user");
    expect(memoryScopeLabel("project")).toBe("项目级");
    expect(memoryScopeEmptyLabel("user")).toBe("暂无用户级记忆");
    expect(MEMORY_SCOPE_DISPLAY_SPECS).toEqual([
      { scope: "user", title: "用户级", empty: "暂无用户级记忆" },
      { scope: "project", title: "项目级", empty: "暂无项目级记忆" },
    ]);
    expect(createMemoryUpsertInput({
      id: " memory-1 ",
      scope: "project",
      projectId: " lilia ",
      title: " 发布约束 ",
      body: " 发布前先跑 contracts。 ",
      tags: " release, check, release ",
      enabled: undefined,
      sourceTaskId: " t-001 ",
    })).toEqual({
      id: "memory-1",
      scope: "project",
      projectId: "lilia",
      title: "发布约束",
      body: "发布前先跑 contracts。",
      tags: ["release", "check"],
      enabled: true,
      sourceTaskId: "t-001",
    });
    expect(createMemoryUpsertInput({
      scope: "user",
      projectId: "lilia",
      title: "全局偏好",
      body: "不要 emoji。",
    })).toMatchObject({
      scope: "user",
      projectId: null,
      tags: [],
      enabled: true,
      sourceTaskId: null,
    });
  });

  it.skip("normalizes automation scope filters", () => {
    expect(AUTOMATION_NODE_KIND_LABELS).toEqual({
      trigger: "事件触发",
      agent: "Agent 调用",
      logic: "逻辑",
      tool: "工具",
      human: "人工确认",
    });
    expect(automationNodeKindLabel("agent")).toBe("Agent 调用");

    const looseScope = {
      projectIds: ["lilia", "", "lilia"],
      includeInbox: false,
      taskStatuses: ["running", "done", "draft", "cancelled", "unknown", "running"],
      backends: ["claude", "unknown", "codex"],
      eventKinds: ["task_created", "unknown", "task_created", "timeline_event"],
    } as unknown as Parameters<typeof normalizeAutomationScope>[0];

    expect(normalizeAutomationScope(looseScope)).toEqual({
      projectIds: ["lilia"],
      includeInbox: false,
      taskStatuses: ["running", "done"],
      backends: ["native-agentkit"],
      eventKinds: ["task_created", "timeline_event"],
    });
    expect(AUTOMATION_SCOPE_EVENT_KINDS).toEqual([
      "task_created",
      "task_status_changed",
      "task_updated",
      "timeline_event",
      "todo_changed",
      "interaction_request",
    ]);
    expect(AUTOMATION_SCOPE_TASK_STATUSES).toEqual(["waiting", "running", "blocked", "done"]);
    expect(AUTOMATION_TRIGGER_KINDS).toEqual([
      "manual",
      "task_changed",
      "timeline_event",
      "todo_changed",
      "interaction_request",
    ]);
    expect(automationTriggerKindLabel("interaction_request")).toBe("Agent 交互请求");
    expect(AUTOMATION_LOGIC_KINDS).toEqual(["condition", "switch", "stop"]);
    expect(DEFAULT_AUTOMATION_TRIGGER_KIND).toBe("manual");
    expect(AUTOMATION_LOGIC_KIND_LABELS.stop).toBe("停止运行");
    expect(DEFAULT_AUTOMATION_LOGIC_KIND).toBe("condition");
    expect(DEFAULT_AUTOMATION_LOGIC_PATH).toBe("trigger.kind");
    expect(DEFAULT_AUTOMATION_AGENT_PROMPT.trim().length).toBeGreaterThan(0);
    expect(DEFAULT_AUTOMATION_HUMAN_PROMPT.trim().length).toBeGreaterThan(0);
    expect(isAutomationLogicKind("switch")).toBe(true);
    expect(isAutomationLogicKind("branch")).toBe(false);
    expect(normalizeAutomationLogicKind("switch")).toBe("switch");
    expect(normalizeAutomationLogicKind("branch")).toBe("condition");
    expect(automationLogicKindLabel("condition")).toBe("条件");
    expect(AUTOMATION_RUN_STATUSES).toEqual([
      "pending",
      "running",
      "succeeded",
      "failed",
      "skipped",
      "waiting_user",
    ]);
    expect(AUTOMATION_CHANGED_EVENT_NAME).toBe("automation:changed");
    expect(AUTOMATION_RUN_STARTED_EVENT_NAME).toBe("automation:run-started");
    expect(AUTOMATION_RUN_UPDATED_EVENT_NAME).toBe("automation:run-updated");
    expect(AUTOMATION_RUN_FINISHED_EVENT_NAME).toBe("automation:run-finished");
    expect(AUTOMATION_RUN_EVENT_NAMES).toEqual([
      "automation:run-started",
      "automation:run-updated",
      "automation:run-finished",
    ]);
    expect(createAutomationChangedEvent("wf-1")).toEqual({ workflowId: "wf-1" });
    expect(createAutomationChangedEvent(null)).toEqual({ workflowId: null });
    const automationRun = {
      id: "run-1",
      workflowId: "wf-1",
      workflowVersionId: "wv-1",
      status: "running" as const,
      trigger: {
        id: "signal-1",
        kind: "manual" as const,
        payload: {},
        createdAt: 1,
      },
      scope: {
        projectIds: [],
        includeInbox: true,
        taskStatuses: [],
        backends: [],
        eventKinds: [],
      },
      startedAt: 1,
      finishedAt: null,
      error: null,
    };
    expect(createAutomationRunEvent(automationRun)).toEqual({ run: automationRun });
    expect(DEFAULT_AUTOMATION_RUN_STATUS).toBe("pending");
    expect(AUTOMATION_WAITING_USER_STATUS).toBe("waiting_user");
    expect(AUTOMATION_RUN_STATUS_TONES).toMatchObject({
      pending: "muted",
      running: "accent",
      succeeded: "ok",
      failed: "err",
      skipped: "muted",
      waiting_user: "accent",
    });
    expect(isAutomationRunStatus("waiting_user")).toBe(true);
    expect(isAutomationRunStatus("waiting")).toBe(false);
    expect(normalizeAutomationRunStatus("failed")).toBe("failed");
    expect(normalizeAutomationRunStatus("waiting")).toBe("pending");
    expect(automationRunStatusTone("succeeded")).toBe("ok");
    expect(AUTOMATION_TOOL_ACTIONS).toEqual([
      "record_timeline",
      "create_task",
      "update_task_status",
      "add_todo",
      "send_guide",
    ]);
    expect(AUTOMATION_TOOL_ACTION_LABELS.send_guide).toBe("发送引导");
    expect(AUTOMATION_TOOL_ACTION_FIELDS).toMatchObject({
      record_timeline: ["taskId", "title", "summary", "status", "backend"],
      create_task: ["projectId", "title", "status"],
      send_guide: ["taskId", "text", "priority"],
    });
    expect(automationToolActionLabel("record_timeline")).toBe("记录运行");
    expect(automationToolActionUsesField("send_guide", "priority")).toBe(true);
    expect(automationToolActionUsesField("add_todo", "priority")).toBe(false);
    expect(normalizeAutomationToolAction("send_guide")).toBe("send_guide");
    expect(normalizeAutomationToolAction("unknown")).toBe("record_timeline");
    expect(AUTOMATION_TOOL_PRIORITIES).toEqual(["low", "normal", "high"]);
    expect(normalizeAutomationToolPriority("high")).toBe("high");
    expect(normalizeAutomationToolPriority("urgent")).toBe("normal");
  });

  it("provides task status helpers for project summaries", () => {
    expect(TASK_STATUSES).toEqual(["draft", "waiting", "running", "blocked", "done", "cancelled"]);
    expect(TASKS_CHANGED_EVENT_NAME).toBe("tasks:changed");
    expect(TASK_LIST_COMMAND).toBe("task_list");
    expect(TASK_LIST_SIDEBAR_CONVERSATIONS_COMMAND).toBe("task_list_sidebar_conversations");
    expect(TASK_GET_COMMAND).toBe("task_get");
    expect(TASK_CREATE_COMMAND).toBe("task_create");
    expect(TASK_UPDATE_COMMAND).toBe("task_update");
    expect(TASK_DELETE_COMMAND).toBe("task_delete");
    expect(TASK_PROMOTE_COMMAND).toBe("task_promote");
    expect(TASK_ARCHIVE_PROJECT_COMMAND).toBe("task_archive_project");
    expect(TASK_ARCHIVE_COMMAND).toBe("task_archive");
    expect(TASK_TOGGLE_PIN_COMMAND).toBe("task_toggle_pin");
    expect(TASK_REORDER_COMMAND).toBe("task_reorder");
    expect(TASK_REPARENT_COMMAND).toBe("task_reparent");
    expect(createTasksChangedEvent("project-1")).toEqual({ projectId: "project-1" });
    expect(createTasksChangedEvent(null)).toEqual({ projectId: null });
    expect(PROJECT_LIST_COMMAND).toBe("project_list");
    expect(PROJECT_DASHBOARD_LIST_COMMAND).toBe("project_dashboard_list");
    expect(PROJECT_GET_COMMAND).toBe("project_get");
    expect(PROJECT_CREATE_COMMAND).toBe("project_create");
    expect(PROJECT_ENSURE_FOLDERS_COMMAND).toBe("project_ensure_folders");
    expect(PROJECT_RENAME_COMMAND).toBe("project_rename");
    expect(PROJECT_REMOVE_COMMAND).toBe("project_remove");
    expect(PROJECT_TOGGLE_PIN_COMMAND).toBe("project_toggle_pin");
    expect(PROJECT_REORDER_COMMAND).toBe("project_reorder");
    expect(PROJECT_GET_SETTINGS_COMMAND).toBe("project_get_settings");
    expect(PROJECT_SET_SETTINGS_COMMAND).toBe("project_set_settings");
    expect(GIT_CLONE_REPO_COMMAND).toBe("git_clone_repo");
    expect(GITHUB_GET_BINDING_STATUS_COMMAND).toBe("github_get_binding_status");
    expect(GITHUB_START_DEVICE_FLOW_COMMAND).toBe("github_start_device_flow");
    expect(GITHUB_POLL_DEVICE_FLOW_COMMAND).toBe("github_poll_device_flow");
    expect(GITHUB_UNBIND_COMMAND).toBe("github_unbind");
    expect(GITHUB_LIST_REPOS_COMMAND).toBe("github_list_repos");
    expect(GITHUB_CLONE_REPO_COMMAND).toBe("github_clone_repo");
    expect(SYSTEM_OPEN_PATH_COMMAND).toBe("system_open_path");
    expect(SYSTEM_OPEN_URL_COMMAND).toBe("system_open_url");
    expect(SYSTEM_OPEN_IN_VSCODE_COMMAND).toBe("system_open_in_vscode");
    expect(PROJECT_DASHBOARD_STATUS_ORDER).toEqual([
      "blocked",
      "running",
      "waiting",
      "draft",
      "done",
      "cancelled",
    ]);
    expect(PROJECT_DASHBOARD_ACTIVE_STATUSES).toEqual(["waiting", "running"]);
    expect(PROJECT_DASHBOARD_BLOCKED_STATUSES).toEqual(["blocked"]);
    expect(PROJECT_ROADMAP_STATUS_ORDER).toEqual([
      "running",
      "waiting",
      "blocked",
      "done",
      "cancelled",
    ]);
    expect(createEmptyProjectTaskStatusCounts()).toEqual({
      draft: 0,
      waiting: 0,
      running: 0,
      blocked: 0,
      done: 0,
      cancelled: 0,
    });
    expect(isTaskStatus("running")).toBe(true);
    expect(isTaskStatus("todo")).toBe(false);
    expect(taskStatusLabel("running")).toBe("运行");
    expect(taskStatusLabel("blocked")).toBe("阻塞");
    expect(taskStatusLabel("running", "state")).toBe("运行中");
    expect(taskStatusLabel("done", "state")).toBe("已完成");
    expect(TASK_TODO_PRIORITIES).toEqual(["high", "normal", "low"]);
    expect(DEFAULT_TASK_TODO_PRIORITY).toBe("normal");
    expect(taskTodoPriorityLabel("high")).toBe("高");
    expect(taskTodoPriorityLabel("normal")).toBe("中");
    expect(taskTodoPriorityLabel("low")).toBe("低");
    expect(isTaskTodoPriority("high")).toBe(true);
    expect(isTaskTodoPriority("urgent")).toBe(false);
    expect(normalizeTaskTodoPriority("low")).toBe("low");
    expect(normalizeTaskTodoPriority("urgent")).toBe("normal");
    expect(TASK_TODO_GUIDE_STATUSES).toEqual(["pending", "queued", "sent"]);
    expect(PENDING_TASK_TODO_GUIDE_STATUS).toBe("pending");
    expect(QUEUED_TASK_TODO_GUIDE_STATUS).toBe("queued");
    expect(SENT_TASK_TODO_GUIDE_STATUS).toBe("sent");
    expect(TODO_LIST_COMMAND).toBe("todo_list");
    expect(TODO_CREATE_COMMAND).toBe("todo_create");
    expect(TODO_UPDATE_COMMAND).toBe("todo_update");
    expect(TODO_DELETE_COMMAND).toBe("todo_delete");
    expect(TODO_APPLY_AGENT_EVENT_COMMAND).toBe("todo_apply_agent_event");
    expect(TODO_CHANGED_EVENT_NAME).toBe("todo-changed");
    expect(createTodoChangedEvent("task-1")).toEqual({ taskId: "task-1" });
    expect(isTaskTodoGuideStatus("queued")).toBe(true);
    expect(isTaskTodoGuideStatus("done")).toBe(false);
    expect(normalizeTaskTodoGuideStatus("sent")).toBe("sent");
    expect(normalizeTaskTodoGuideStatus("done")).toBeNull();
    expect(liliaGoalStatusLabel("active")).toBe("进行中");
    expect(liliaGoalStatusLabel("usageLimited")).toBe("用量受限");
    const projectCounts = countProjectTaskStatuses([
      { status: "running" },
      { status: "done" },
      { status: "todo" },
      { status: null },
    ]);
    expect(projectCounts).toMatchObject({
      running: 1,
      done: 1,
      waiting: 0,
    });
    expect(deriveProjectDashboardCounts({
      ...projectCounts,
      waiting: 2,
      blocked: 3,
    })).toEqual({ activeCount: 3, blockedCount: 3 });
  });

  it("normalizes milestone statuses", () => {
    expect(MILESTONE_STATUSES).toEqual(["upcoming", "in-progress", "done", "abandoned"]);
    expect(DEFAULT_MILESTONE_STATUS).toBe("upcoming");
    expect(MILESTONE_LIST_COMMAND).toBe("milestone_list");
    expect(MILESTONE_CREATE_COMMAND).toBe("milestone_create");
    expect(MILESTONE_UPDATE_COMMAND).toBe("milestone_update");
    expect(MILESTONE_DELETE_COMMAND).toBe("milestone_delete");
    expect(MILESTONE_REORDER_COMMAND).toBe("milestone_reorder");
    expect(MILESTONE_SET_TASKS_COMMAND).toBe("milestone_set_tasks");
    expect(isMilestoneStatus("in-progress")).toBe(true);
    expect(isMilestoneStatus("running")).toBe(false);
    expect(normalizeMilestoneStatus("done")).toBe("done");
    expect(normalizeMilestoneStatus("running")).toBe(DEFAULT_MILESTONE_STATUS);
    expect(normalizeMilestoneStatus("running", "abandoned")).toBe("abandoned");
    expect(milestoneStatusLabel("in-progress")).toBe("进行中");
    expect(milestoneStatusLabel("abandoned")).toBe("已放弃");
  });

  it.skip("labels hook source protocol fields", () => {
    expect(HOOK_BACKENDS).toEqual(["native-agentkit"]);
    expect(HOOK_SCOPES).toEqual(["managed", "user", "project", "local", "plugin", "system"]);
    expect(HOOK_SOURCE_FORMATS).toEqual([
      "claude_settings_json",
      "codex_hooks_json",
      "codex_config_toml",
      "managed_settings",
      "requirements_toml",
      "plugin_manifest",
    ]);
    expect(HOOK_TRUST_STATES).toEqual(["unknown", "required", "managed", "n_a"]);
    expect(isHookBackendKind("codex")).toBe(true);
    expect(isHookBackendKind("openai")).toBe(false);
    expect(isHookScope("plugin")).toBe(true);
    expect(isHookScope("workspace")).toBe(false);
    expect(isHookSourceFormat("requirements_toml")).toBe(true);
    expect(isHookSourceFormat("yaml")).toBe(false);
    expect(isHookTrustState("required")).toBe(true);
    expect(isHookTrustState("trusted")).toBe(false);
    expect(hookScopeLabel("user")).toBe("全局");
    expect(hookScopeLabel("managed")).toBe("Managed");
    expect(hookSourceFormatLabel("codex_config_toml")).toBe("config.toml [hooks]");
    expect(hookSourceFormatLabel("plugin_manifest")).toBe("plugin.json");
    expect(hookTrustStateLabel("required")).toBe("需要 review/trust");
    expect(hookTrustStateLabel("n_a")).toBe("不适用");
    expect(HOOK_SOURCE_STATE_LABELS).toEqual({
      missing: "未创建",
      empty: "空来源",
      handlerCount: "{count} 条 Handler",
    });
    expect(HOOK_SOURCE_EDIT_LABELS).toEqual({
      readonly: "只读",
      writable: "可写",
      creatable: "可创建",
    });
    expect(hookSourceStateLabel({ exists: false, handlerCount: 0 })).toBe("未创建");
    expect(hookSourceStateLabel({ exists: true, handlerCount: 0 })).toBe("空来源");
    expect(hookSourceStateLabel({ exists: true, handlerCount: 2 })).toBe("2 条 Handler");
    expect(hookSourceEditLabel({ editable: false, exists: true })).toBe("只读");
    expect(hookSourceEditLabel({ editable: true, exists: false })).toBe("可创建");
    expect(hookSourceEditLabel({ editable: true, exists: true })).toBe("可写");
  });

  it.skip("labels plugin protocol fields", () => {
    expect(PLUGIN_SCOPES).toEqual(["user", "project"]);
    expect(PLUGIN_BACKENDS).toEqual(["native-agentkit"]);
    expect(PLUGIN_MCP_TRANSPORTS).toEqual(["stdio", "http", "oauth", "unknown"]);
    expect(PLUGINS_OVERVIEW_COMMAND).toBe("plugins_overview");
    expect(PLUGINS_HOOKS_OVERVIEW_COMMAND).toBe("plugins_hooks_overview");
    expect(PLUGINS_CREATE_SKILL_COMMAND).toBe("plugins_create_skill");
    expect(PLUGINS_DELETE_SKILL_COMMAND).toBe("plugins_delete_skill");
    expect(PLUGINS_SET_SKILL_ENABLED_COMMAND).toBe("plugins_set_skill_enabled");
    expect(PLUGINS_SET_PACKAGE_ENABLED_COMMAND).toBe("plugins_set_package_enabled");
    expect(PLUGINS_CREATE_MCP_SERVER_COMMAND).toBe("plugins_create_mcp_server");
    expect(PLUGINS_UPDATE_MCP_SERVER_COMMAND).toBe("plugins_update_mcp_server");
    expect(PLUGINS_DELETE_MCP_SERVER_COMMAND).toBe("plugins_delete_mcp_server");
    expect(PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND).toBe("plugins_set_mcp_server_enabled");
    expect(PLUGINS_OPEN_MCP_CONFIG_COMMAND).toBe("plugins_open_mcp_config");
    expect(PLUGINS_READ_HOOK_SOURCE_COMMAND).toBe("plugins_read_hook_source");
    expect(PLUGINS_UPDATE_HOOK_SOURCE_COMMAND).toBe("plugins_update_hook_source");
    expect(PLUGINS_CREATE_HOOK_SOURCE_COMMAND).toBe("plugins_create_hook_source");
    expect(PLUGINS_DELETE_HOOK_SOURCE_COMMAND).toBe("plugins_delete_hook_source");
    expect(PLUGINS_SET_HOOK_SOURCE_ENABLED_COMMAND).toBe("plugins_set_hook_source_enabled");
    expect(PLUGINS_OPEN_HOOK_CONFIG_COMMAND).toBe("plugins_open_hook_config");
    expect(isPluginScope("project")).toBe(true);
    expect(isPluginScope("workspace")).toBe(false);
    expect(isPluginBackendKind("codex")).toBe(true);
    expect(isPluginBackendKind("openai")).toBe(false);
    expect(pluginEnabledLabel(true)).toBe("已启用");
    expect(pluginEnabledLabel(false)).toBe("已停用");
    expect(pluginToggleActionLabel(true)).toBe("停用");
    expect(pluginToggleActionLabel(false)).toBe("启用");
    expect(pluginBackendLabel("claude")).toBe("Claude");
    expect(pluginMcpBackendLabel("codex")).toBe("Codex MCP");
    expect(pluginMcpTransportLabel("stdio")).toBe("stdio");
    expect(pluginMcpTransportLabel("http")).toBe("HTTP");
    expect(pluginMcpTransportLabel("oauth")).toBe("OAuth");
    expect(pluginMcpTransportLabel("streamable-http")).toBe("未知");
  });
});


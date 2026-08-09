/**
 * Chat 服务层：把 Tauri command/event 包成 typed 函数。
 * 输入/输出形状走 @lilia/contracts，跨端共享；Rust 侧 `#[serde(rename_all = "camelCase")]`
 * 已对齐字段名，前端不需再做 key 映射。
 */

import { invoke } from "../tauri/runtime";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { normalizeAgentInteractionRequest } from "@lilia/contracts/agentInteraction.mjs";
import {
  AGENT_INTERACTION_DELETE_SUBAGENT_COMMAND,
  AGENT_INTERACTION_GET_SETTINGS_COMMAND,
  AGENT_INTERACTION_LIST_SUBAGENTS_COMMAND,
  AGENT_INTERACTION_SET_SETTINGS_COMMAND,
  AGENT_INTERACTION_UPSERT_SUBAGENT_COMMAND,
} from "@lilia/contracts/agentInteractionContract.mjs";
import {
  AGENT_TIMELINE_BATCH_EVENT_NAME,
  AGENT_TIMELINE_EVENT_NAME,
  CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME,
  CHAT_CONTEXT_USAGE_EVENT_NAME,
  CHAT_DONE_EVENT_NAME,
  CHAT_TURN_STARTED_EVENT_NAME,
} from "@lilia/contracts/chatEventsContract.mjs";
import {
  CHAT_ACK_RESTORED_ROLLBACK_COMMAND,
  CHAT_DESCRIBE_ATTACHMENTS_COMMAND,
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
  AGENT_TIMELINE_LIST_COMMAND,
} from "@lilia/contracts/chatCommandsContract.mjs";
import {
  CONVERSATION_SUGGESTIONS_GET_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_SETTINGS_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND,
  CONVERSATION_SUGGESTIONS_SET_SETTINGS_COMMAND,
} from "@lilia/contracts/suggestionsContract.mjs";
import {
  ASSISTANT_AI_FETCH_MODELS_COMMAND,
  ASSISTANT_AI_GET_CONFIG_COMMAND,
  ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND,
  ASSISTANT_AI_SET_CONFIG_COMMAND,
  ASSISTANT_AI_TEST_CONNECTION_COMMAND,
  CHAT_CHECK_ENV_COMMAND,
  MODEL_FEATURE_GET_SETTINGS_COMMAND,
  MODEL_FEATURE_LIST_MODEL_OPTIONS_COMMAND,
  MODEL_FEATURE_SET_SETTINGS_COMMAND,
  PROVIDER_GET_ACTIVE_BACKEND_COMMAND,
  PROVIDER_GET_CONFIG_COMMAND,
  PROVIDER_SET_ACTIVE_BACKEND_COMMAND,
  PROVIDER_SET_CONFIG_COMMAND,
  ROUTER_GET_MODE_COMMAND,
  ROUTER_SET_MODE_COMMAND,
} from "@lilia/contracts/providerCommandsContract.mjs";
import {
  PROJECT_ARCHITECTURE_APPLY_COMMAND,
  PROJECT_ARCHITECTURE_GET_COMMAND,
  PROJECT_ARCHITECTURE_LIST_CHANGES_COMMAND,
  PROJECT_ARCHITECTURE_REJECT_COMMAND,
  PROJECT_ARCHITECTURE_ROLLBACK_COMMAND,
} from "@lilia/contracts/architectureContract.mjs";
import {
  QUOTA_USAGE_GET_STATS_COMMAND,
} from "@lilia/contracts/quotaContract.mjs";
import {
  REMOTE_CONTROL_CANCEL_PAIRING_COMMAND,
  REMOTE_CONTROL_REVOKE_DEVICE_COMMAND,
  REMOTE_CONTROL_SET_KEEP_AWAKE_ENABLED_COMMAND,
  REMOTE_CONTROL_SET_HOST_ENABLED_COMMAND,
  REMOTE_CONTROL_SET_PC_NAME_COMMAND,
  REMOTE_CONTROL_START_PAIRING_COMMAND,
  REMOTE_CONTROL_STATUS_COMMAND,
} from "@lilia/contracts/remoteControlCommandsContract.mjs";
import { TAURI_PLUGIN_DIALOG_OPEN_COMMAND } from "../tauri/pluginCommands";
import { installCombinedUnlisten } from "@lilia/ui/utils/eventListeners";
import {
  normalizeModelFeatureSettings,
  type AgentInteractionSettings,
  type AgentInteractionKind,
  type AgentInteractionRequest,
  type AgentInteractionResponse,
  type AgentTimelineBatchEvent,
  type AssistantAIConfig,
  type AssistantAIModelsResult,
  type AssistantAITestResult,
  type BackendEnvStatus,
  type ChatBackendKind,
  type ChatAttachment,
  type ChatContextSearchResult,
  type ChatConversationReference,
  type ChatSlashCommandSearchResult,
  type ChatComposerState,
  type ChatContextUsage,
  type ChatModelOption,
  type ChatRuntimeCommand,
  type ChatRuntimeSnapshot,
  type ChatWorkflow,
  type ProviderRuntimeOptions,
  type ChatInterruptResult,
  type AgentTimelineEvent,
  type AssistantAIModelPoolItem,
  type ChatSendResult,
  type ConnectionMode,
  type EnvStatusReport,
  type ModelFeatureSettings,
  type ProviderConfig,
  type RouterMode,
  type SuggestionItem,
  type SuggestionSettings,
  type ToolConsentDecision,
  type ToolConsentRequest,
  type ToolConsentUpdatedInput,
  type McpElicitationPayload,
  type McpElicitationResult,
  type PermissionApprovalPayload,
  type PermissionApprovalResult,
  type LiliaIabSnapshot,
  type ChatRollbackResult,
  type ChatDoneEvent,
  type ChatTurnStartedEvent,
  type ConversationSuggestionSources,
  type ConversationSuggestionSourceKind,
  type ProjectArchitectureApplyInput,
  type ProjectArchitectureApplyResult,
  type ProjectArchitectureChangeEvent,
  type ProjectArchitectureChangeRecord,
  type ProjectArchitectureGraph,
  type ProjectArchitectureRejectInput,
  type ProjectArchitectureRollbackResult,
  type CustomSubagentDefinition,
  type CustomSubagentUpsertInput,
  type QuotaUsageStats,
  type QuotaUsageStatsInput,
  type RemoteControlStatus,
  type RemotePairingTicket,
} from "@lilia/contracts";

export type {
  AgentInteractionSettings,
  AgentInteractionKind,
  AgentInteractionRequest,
  AgentInteractionResponse,
  AgentTimelineBatchEvent,
  AssistantAIConfig,
  AssistantAIModelsResult,
  AssistantAITestResult,
  ChatAttachment,
  ChatInterruptResult,
  ChatContextSearchResult,
  ChatConversationReference,
  ChatContextUsage,
  ChatModelOption,
  ChatSlashCommandSearchResult,
  ChatWorkflow,
  ChatRuntimeCommand,
  ChatRuntimeSnapshot,
  ProviderRuntimeOptions,
  ModelFeatureSettings,
  ConnectionMode,
  BackendEnvStatus,
  EnvStatusReport,
  ProviderConfig,
  RouterMode,
  SuggestionItem,
  SuggestionSettings,
  ToolConsentDecision,
  ToolConsentRequest,
  ToolConsentUpdatedInput,
  McpElicitationPayload,
  McpElicitationResult,
  PermissionApprovalPayload,
  PermissionApprovalResult,
  LiliaIabSnapshot,
  ChatRollbackResult,
  ConversationSuggestionSources,
  ConversationSuggestionSourceKind,
  ProjectArchitectureApplyInput,
  ProjectArchitectureApplyResult,
  ProjectArchitectureChangeEvent,
  ProjectArchitectureChangeRecord,
  ProjectArchitectureGraph,
  ProjectArchitectureRejectInput,
  ProjectArchitectureRollbackResult,
  QuotaUsageStats,
  QuotaUsageStatsInput,
  RemoteControlStatus,
  RemotePairingTicket,
};

export type TurnStartedEvent = ChatTurnStartedEvent;
export type DoneEvent = ChatDoneEvent;

export function listAgentTimeline(taskId: string): Promise<AgentTimelineEvent[]> {
  return invoke<AgentTimelineEvent[]>(AGENT_TIMELINE_LIST_COMMAND, { taskId });
}

/**
 * 发起一轮对话。返回值是 user 那条消息本身（用于乐观渲染）；
 * Agent 的过程与最终回复通过 agent timeline 异步推回。
 * projectCwd 决定 agent 能看到的文件树。
 */
export interface SendMessageInput {
  taskId: string;
  turn: {
    content: string;
    composer: ChatComposerState;
    projectCwd: string;
    attachments?: ChatAttachment[];
    conversationReferences?: ChatConversationReference[];
    guideId?: string | null;
  };
  workflow?: ChatWorkflow | null;
  runtimeCommand?: ChatRuntimeCommand | null;
  runtimeOptions?: ProviderRuntimeOptions | null;
}

export function sendMessage(input: SendMessageInput): Promise<ChatSendResult> {
  return invoke<ChatSendResult>(CHAT_SEND_MESSAGE_COMMAND, {
    taskId: input.taskId,
    content: input.turn.content,
    composer: input.turn.composer,
    projectCwd: input.turn.projectCwd,
    attachments: input.turn.attachments ?? [],
    conversationReferences: input.turn.conversationReferences ?? [],
    guideId: input.turn.guideId ?? null,
    workflow: input.workflow ?? null,
    runtimeCommand: input.runtimeCommand ?? null,
    runtimeOptions: input.runtimeOptions ?? null,
  });
}

export function interruptTurn(taskId: string): Promise<ChatInterruptResult> {
  return invoke<ChatInterruptResult>(CHAT_INTERRUPT_TURN_COMMAND, { taskId });
}

export function describeAttachments(paths: string[]): Promise<ChatAttachment[]> {
  return invoke<ChatAttachment[]>(CHAT_DESCRIBE_ATTACHMENTS_COMMAND, { paths });
}

export function searchContextAttachments(
  projectCwd: string,
  query: string,
  limit = 12,
): Promise<ChatContextSearchResult[]> {
  return invoke<ChatContextSearchResult[]>(CHAT_SEARCH_CONTEXT_ATTACHMENTS_COMMAND, {
    projectCwd,
    query,
    limit,
  });
}

export function searchSlashCommands(
  projectCwd: string,
  query: string,
  limit = 12,
): Promise<ChatSlashCommandSearchResult[]> {
  return invoke<ChatSlashCommandSearchResult[]>(CHAT_SEARCH_SLASH_COMMANDS_COMMAND, {
    projectCwd,
    query,
    limit,
  });
}

export function readClipboardFilePaths(): Promise<string[]> {
  return invoke<string[]>(CHAT_READ_CLIPBOARD_FILE_PATHS_COMMAND);
}

export function saveClipboardImage(input: {
  mime: string | null;
  bytesBase64: string;
  name?: string | null;
}): Promise<ChatAttachment> {
  return invoke<ChatAttachment>(CHAT_SAVE_CLIPBOARD_IMAGE_COMMAND, { input });
}

export function saveClipboardText(input: { text: string }): Promise<ChatAttachment> {
  return invoke<ChatAttachment>(CHAT_SAVE_CLIPBOARD_TEXT_COMMAND, { input });
}

export async function pickAttachmentFiles(): Promise<string[]> {
  const picked = await invoke<string | string[] | null>(TAURI_PLUGIN_DIALOG_OPEN_COMMAND, {
    options: {
      directory: false,
      multiple: true,
      title: "选择附件",
    },
  });
  if (!picked) return [];
  return Array.isArray(picked) ? picked : [picked];
}

export function getComposerState(taskId: string): Promise<ChatComposerState> {
  return invoke<ChatComposerState>(CHAT_GET_COMPOSER_STATE_COMMAND, { taskId });
}

export function listModels(backend: ChatBackendKind): Promise<ChatModelOption[]> {
  return invoke<ChatModelOption[]>(CHAT_LIST_MODELS_COMMAND, { backend });
}

export function getRuntimeSnapshot(taskId: string): Promise<ChatRuntimeSnapshot> {
  return invoke<ChatRuntimeSnapshot>(CHAT_GET_RUNTIME_SNAPSHOT_COMMAND, { taskId });
}

export function setComposerState(state: ChatComposerState): Promise<void> {
  return invoke<void>(CHAT_SET_COMPOSER_STATE_COMMAND, { state });
}

export function getAgentInteractionSettings(): Promise<AgentInteractionSettings> {
  return invoke<AgentInteractionSettings>(AGENT_INTERACTION_GET_SETTINGS_COMMAND);
}

export function setAgentInteractionSettings(
  settings: Partial<AgentInteractionSettings>,
): Promise<void> {
  return invoke<void>(AGENT_INTERACTION_SET_SETTINGS_COMMAND, { settings });
}

export function listCustomSubagents(): Promise<CustomSubagentDefinition[]> {
  return invoke<CustomSubagentDefinition[]>(AGENT_INTERACTION_LIST_SUBAGENTS_COMMAND);
}

export function upsertCustomSubagent(
  input: CustomSubagentUpsertInput,
): Promise<CustomSubagentDefinition> {
  return invoke<CustomSubagentDefinition>(AGENT_INTERACTION_UPSERT_SUBAGENT_COMMAND, { input });
}

export function deleteCustomSubagent(id: string): Promise<void> {
  return invoke<void>(AGENT_INTERACTION_DELETE_SUBAGENT_COMMAND, { id });
}

export function ackRestoredRollback(taskId: string): Promise<void> {
  return invoke<void>(CHAT_ACK_RESTORED_ROLLBACK_COMMAND, { taskId });
}

/** 健康检查：node 运行时与 native-agentkit 连接模式。 */
export function checkEnv(options: { forceRefresh?: boolean } = {}): Promise<EnvStatusReport> {
  return invoke<EnvStatusReport>(CHAT_CHECK_ENV_COMMAND, {
    forceRefresh: options.forceRefresh ?? false,
  });
}

export function getProviderConfig(backend: ChatBackendKind): Promise<ProviderConfig> {
  return invoke<ProviderConfig>(PROVIDER_GET_CONFIG_COMMAND, { backend });
}

export function setProviderConfig(config: ProviderConfig): Promise<void> {
  return invoke<void>(PROVIDER_SET_CONFIG_COMMAND, { config });
}

export function getActiveBackend(): Promise<ChatBackendKind> {
  return invoke<ChatBackendKind>(PROVIDER_GET_ACTIVE_BACKEND_COMMAND);
}

export function setActiveBackend(backend: ChatBackendKind): Promise<void> {
  return invoke<void>(PROVIDER_SET_ACTIVE_BACKEND_COMMAND, { backend });
}

export function getRouterMode(backend: ChatBackendKind): Promise<RouterMode> {
  return invoke<RouterMode>(ROUTER_GET_MODE_COMMAND, { backend });
}

export function setRouterMode(backend: ChatBackendKind, mode: RouterMode): Promise<void> {
  return invoke<void>(ROUTER_SET_MODE_COMMAND, { backend, mode });
}

// ---- 辅助模型（Assistant AI） ----
// 与 Provider 平级、独立配置，不参与 Agent 主循环；供 Memory 助手等周边模块消费。

export function getAssistantAIConfig(): Promise<AssistantAIConfig> {
  return invoke<AssistantAIConfig>(ASSISTANT_AI_GET_CONFIG_COMMAND);
}

export function setAssistantAIConfig(config: AssistantAIConfig): Promise<void> {
  return invoke<void>(ASSISTANT_AI_SET_CONFIG_COMMAND, { config });
}

export function fetchAssistantAIModels(
  config: AssistantAIConfig,
): Promise<AssistantAIModelsResult> {
  return invoke<AssistantAIModelsResult>(ASSISTANT_AI_FETCH_MODELS_COMMAND, { config });
}

export function listModelFeatureOptions(): Promise<AssistantAIModelPoolItem[]> {
  return invoke<AssistantAIModelPoolItem[]>(MODEL_FEATURE_LIST_MODEL_OPTIONS_COMMAND);
}

export async function getModelFeatureSettings(): Promise<ModelFeatureSettings> {
  const settings = await invoke<ModelFeatureSettings>(MODEL_FEATURE_GET_SETTINGS_COMMAND);
  return normalizeModelFeatureSettings(settings);
}

type ModelFeatureSettingsListener = (settings: ModelFeatureSettings) => void;

const modelFeatureSettingsListeners = new Set<ModelFeatureSettingsListener>();

export function onModelFeatureSettingsChanged(listener: ModelFeatureSettingsListener): () => void {
  modelFeatureSettingsListeners.add(listener);
  return () => modelFeatureSettingsListeners.delete(listener);
}

export async function setModelFeatureSettings(settings: ModelFeatureSettings): Promise<void> {
  const normalized = normalizeModelFeatureSettings(settings);
  await invoke<void>(MODEL_FEATURE_SET_SETTINGS_COMMAND, { settings: normalized });
  for (const listener of modelFeatureSettingsListeners) {
    listener(normalized);
  }
}

export function testAssistantAIConnection(
  config: AssistantAIConfig,
): Promise<AssistantAITestResult> {
  return invoke<AssistantAITestResult>(ASSISTANT_AI_TEST_CONNECTION_COMMAND, { config });
}

export interface PromptOptimizeInput {
  prompt: string;
  attachments?: ChatAttachment[];
  conversationReferences?: ChatConversationReference[];
  projectCwd?: string | null;
  taskId?: string | null;
}

export type PromptOptimizeScenario =
  | "review"
  | "fix_suggestion"
  | "batch_apply"
  | "bug_localization"
  | "context_compact"
  | "config_diagnostics"
  | "goal_update"
  | "general_task_optimize";

export interface PromptOptimizeRoute {
  scenario: PromptOptimizeScenario;
  workflow: ChatWorkflow | null;
  confidence: number;
  reason: string;
  signals: string[];
}

export interface PromptOptimizeResult {
  optimizedPrompt: string;
  route: PromptOptimizeRoute;
}

export function optimizePrompt(input: PromptOptimizeInput): Promise<PromptOptimizeResult> {
  return invoke<PromptOptimizeResult>(ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND, { input });
}

export function getConversationSuggestions(
  projectId?: string | null,
  forceRefresh = false,
): Promise<SuggestionItem[]> {
  return invoke<SuggestionItem[]>(CONVERSATION_SUGGESTIONS_GET_COMMAND, {
    projectId: projectId ?? null,
    forceRefresh,
  });
}

export function getConversationSuggestionSources(
  projectId?: string | null,
  forceRefresh = false,
): Promise<ConversationSuggestionSources> {
  return invoke<ConversationSuggestionSources>(CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND, {
    projectId: projectId ?? null,
    forceRefresh,
  });
}

export function getConversationSuggestionSettings(): Promise<SuggestionSettings> {
  return invoke<SuggestionSettings>(CONVERSATION_SUGGESTIONS_GET_SETTINGS_COMMAND);
}

export function setConversationSuggestionSettings(
  settings: SuggestionSettings,
): Promise<void> {
  return invoke<void>(CONVERSATION_SUGGESTIONS_SET_SETTINGS_COMMAND, { settings });
}

export function getQuotaUsageStats(
  input: QuotaUsageStatsInput = {},
): Promise<QuotaUsageStats> {
  return invoke<QuotaUsageStats>(QUOTA_USAGE_GET_STATS_COMMAND, { input });
}

export function getRemoteControlStatus(): Promise<RemoteControlStatus> {
  return invoke<RemoteControlStatus>(REMOTE_CONTROL_STATUS_COMMAND);
}

export function setRemoteControlHostEnabled(enabled: boolean): Promise<RemoteControlStatus> {
  return invoke<RemoteControlStatus>(REMOTE_CONTROL_SET_HOST_ENABLED_COMMAND, { enabled });
}

export function setRemoteControlPcName(name: string): Promise<RemoteControlStatus> {
  return invoke<RemoteControlStatus>(REMOTE_CONTROL_SET_PC_NAME_COMMAND, { name });
}

export function setRemoteControlKeepAwakeEnabled(enabled: boolean): Promise<RemoteControlStatus> {
  return invoke<RemoteControlStatus>(REMOTE_CONTROL_SET_KEEP_AWAKE_ENABLED_COMMAND, { enabled });
}

export function startRemoteControlPairing(): Promise<RemotePairingTicket> {
  return invoke<RemotePairingTicket>(REMOTE_CONTROL_START_PAIRING_COMMAND);
}

export function cancelRemoteControlPairing(): Promise<void> {
  return invoke<void>(REMOTE_CONTROL_CANCEL_PAIRING_COMMAND);
}

export function revokeRemoteControlDevice(deviceId: string): Promise<RemoteControlStatus> {
  return invoke<RemoteControlStatus>(REMOTE_CONTROL_REVOKE_DEVICE_COMMAND, { deviceId });
}

export function getProjectArchitecture(projectId: string): Promise<ProjectArchitectureGraph> {
  return invoke<ProjectArchitectureGraph>(PROJECT_ARCHITECTURE_GET_COMMAND, { projectId });
}

export function listProjectArchitectureChanges(
  projectId: string,
  limit = 20,
): Promise<ProjectArchitectureChangeRecord[]> {
  return invoke<ProjectArchitectureChangeRecord[]>(PROJECT_ARCHITECTURE_LIST_CHANGES_COMMAND, {
    projectId,
    limit,
  });
}

export function applyProjectArchitecture(
  input: ProjectArchitectureApplyInput,
): Promise<ProjectArchitectureApplyResult> {
  return invoke<ProjectArchitectureApplyResult>(PROJECT_ARCHITECTURE_APPLY_COMMAND, { input });
}

export function rejectProjectArchitecture(
  input: ProjectArchitectureRejectInput,
): Promise<ProjectArchitectureChangeEvent> {
  return invoke<ProjectArchitectureChangeEvent>(PROJECT_ARCHITECTURE_REJECT_COMMAND, { input });
}

export function rollbackProjectArchitecture(
  projectId: string,
  taskId: string,
  backend: ChatBackendKind,
): Promise<ProjectArchitectureRollbackResult> {
  return invoke<ProjectArchitectureRollbackResult>(PROJECT_ARCHITECTURE_ROLLBACK_COMMAND, {
    projectId,
    taskId,
    backend,
  });
}

// ---- 事件订阅 ----
// UI 只订阅 turn 状态和 agent timeline；文本、工具、错误都归入 timeline 事件。

export function onTurnStarted(handler: (e: TurnStartedEvent) => void): Promise<UnlistenFn> {
  return listen<TurnStartedEvent>(CHAT_TURN_STARTED_EVENT_NAME, (event) => handler(event.payload));
}

export function onDone(handler: (e: DoneEvent) => void): Promise<UnlistenFn> {
  return listen<DoneEvent>(CHAT_DONE_EVENT_NAME, (event) => handler(event.payload));
}

export function onContextUsage(handler: (e: ChatContextUsage) => void): Promise<UnlistenFn> {
  return listen<ChatContextUsage>(CHAT_CONTEXT_USAGE_EVENT_NAME, (event) => handler(event.payload));
}

export function onAgentTimeline(
  handler: (e: AgentTimelineEvent) => void,
): Promise<UnlistenFn> {
  return listen<AgentTimelineEvent>(AGENT_TIMELINE_EVENT_NAME, (event) => handler(event.payload));
}

export function onAgentTimelineBatch(
  handler: (e: AgentTimelineBatchEvent) => void,
): Promise<UnlistenFn> {
  return listen<AgentTimelineBatchEvent>(AGENT_TIMELINE_BATCH_EVENT_NAME, (event) => handler(event.payload));
}

export type AgentTimelineEventsSource = "single" | "batch";

export async function onAgentTimelineEvents(
  handler: (events: AgentTimelineEvent[], source: AgentTimelineEventsSource) => void,
): Promise<UnlistenFn> {
  return await installCombinedUnlisten([
    () => onAgentTimeline((event) => handler([event], "single")),
    () => onAgentTimelineBatch((event) => handler(event.events, "batch")),
  ]);
}

export function onAgentInteractionRequest(
  handler: (e: AgentInteractionRequest) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME, (event) => {
    const req = normalizeAgentInteractionRequest(event.payload);
    if (req) handler(req);
  });
}

export function respondAgentInteraction(response: AgentInteractionResponse): Promise<void> {
  return invoke<void>(CHAT_RESPOND_AGENT_INTERACTION_COMMAND, {
    taskId: response.taskId,
    requestId: response.requestId,
    kind: response.kind,
    result: response.result,
  });
}

export function respondTitleUpdate(
  taskId: string,
  requestId: string,
  decision: "accept" | "decline",
): Promise<void> {
  return invoke<void>(CHAT_RESPOND_TITLE_UPDATE_COMMAND, { taskId, requestId, decision });
}

import {
  ALLOWED_MODEL_PREFIXES_BY_BACKEND,
  BACKEND_REASONING_EFFORTS,
  CHAT_BACKENDS,
  CHAT_BACKEND_LABELS,
  DEFAULT_CHAT_BACKEND,
  DEFAULT_MODEL_BY_BACKEND,
  MODEL_OPTIONS_BY_BACKEND,
  REASONING_EFFORTS,
} from "./chatBackendsContract.mjs";
import {
  DEFAULT_PERMISSION_MODE,
  isRuntimePermissionMode as isPermissionModeImpl,
  normalizeRuntimePermissionMode as normalizePermissionModeImpl,
  PERMISSION_MODE_DISPLAY,
  PERMISSION_MODE_DISPLAY_ORDER,
  PERMISSION_MODES,
  type PermissionModeDisplay as ContractPermissionModeDisplay,
  type RuntimePermissionMode as ContractPermissionMode,
} from "./permissionModes.mjs";
import {
  AGENT_TIMELINE_BATCH_EVENT_NAME,
  AGENT_TIMELINE_EVENT_NAME,
  CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME,
  CHAT_CONTEXT_USAGE_EVENT_NAME,
  CHAT_DONE_EVENT_NAME,
  CHAT_TURN_STARTED_EVENT_NAME,
} from "./chatEventsContract.mjs";
import {
  AGENT_TIMELINE_CLEAR_TASK_COMMAND,
  AGENT_TIMELINE_LIST_COMMAND,
  CHAT_ACK_RESTORED_ROLLBACK_COMMAND,
  CHAT_DESCRIBE_ATTACHMENTS_COMMAND,
  CHAT_GET_COMPOSER_DRAFT_COMMAND,
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
  CHAT_SEND_PROCESS_SESSION_COMMAND,
  CHAT_SET_COMPOSER_DRAFT_COMMAND,
  CHAT_SET_COMPOSER_STATE_COMMAND,
  LILIA_IAB_OPEN_COMMAND,
  LILIA_IAB_SUBMIT_COMMAND,
} from "./chatCommandsContract.mjs";
import {
  AUTOMATION_WORKFLOW_TYPE,
  CHAT_SLASH_COMMAND_WORKFLOW_TYPE,
  createLiliaBatchApplyWorkflow as createLiliaBatchApplyWorkflowImpl,
  createLiliaCompactWorkflow as createLiliaCompactWorkflowImpl,
  createLiliaFixSuggestionWorkflow as createLiliaFixSuggestionWorkflowImpl,
  createLiliaGoalWorkflow as createLiliaGoalWorkflowImpl,
  createLiliaReviewWorkflow as createLiliaReviewWorkflowImpl,
  createLiliaTaskWorkflow as createLiliaTaskWorkflowImpl,
  DEFAULT_LILIA_CONFIG_DIAGNOSTICS_INCLUDE_LAYERS,
  DEFAULT_LILIA_FIX_SUGGESTION_MODE,
  DEFAULT_LILIA_GOAL_STATUS,
  DEFAULT_LILIA_REVIEW_DELIVERY,
  isLiliaBackgroundTerminalsCleanWorkflow as isLiliaBackgroundTerminalsCleanWorkflowImpl,
  isLiliaBatchApplySourceKind as isLiliaBatchApplySourceKindImpl,
  isLiliaCompactWorkflow as isLiliaCompactWorkflowImpl,
  isLiliaFixSuggestionMode as isLiliaFixSuggestionModeImpl,
  isLiliaGoalAction as isLiliaGoalActionImpl,
  isLiliaGoalStatus as isLiliaGoalStatusImpl,
  isLiliaMemoryMode as isLiliaMemoryModeImpl,
  isLiliaMemoryResetWorkflow as isLiliaMemoryResetWorkflowImpl,
  isLiliaQueryWorkflowType as isLiliaQueryWorkflowTypeImpl,
  isLiliaReviewDelivery as isLiliaReviewDeliveryImpl,
  isLiliaReviewTargetType as isLiliaReviewTargetTypeImpl,
  isLiliaTaskWorkflowKind as isLiliaTaskWorkflowKindImpl,
  LILIA_BACKGROUND_TERMINALS_CLEAN_WORKFLOW_TYPE,
  LILIA_BATCH_APPLY_SOURCE_KINDS,
  LILIA_BATCH_APPLY_WORKFLOW_TYPE,
  LILIA_COMPACT_WORKFLOW_TYPE,
  LILIA_CONFIG_DIAGNOSTICS_WORKFLOW_TYPE,
  LILIA_FIX_SUGGESTION_MODES,
  LILIA_FIX_SUGGESTION_WORKFLOW_TYPE,
  LILIA_GOAL_ACTIONS,
  LILIA_GOAL_STATUSES,
  LILIA_GOAL_WORKFLOW_TYPE,
  LILIA_MEMORY_MODE_WORKFLOW_TYPE,
  LILIA_MEMORY_MODES,
  LILIA_MEMORY_RESET_WORKFLOW_TYPE,
  LILIA_QUERY_WORKFLOW_TYPES,
  LILIA_REVIEW_DELIVERIES,
  LILIA_REVIEW_TARGET_TYPES,
  LILIA_REVIEW_WORKFLOW_TYPE,
  LILIA_TASK_WORKFLOW_KINDS,
  LILIA_TASK_WORKFLOW_TYPE,
  normalizeLiliaBatchApplyWorkflow as normalizeLiliaBatchApplyWorkflowImpl,
  normalizeLiliaConfigDiagnosticsWorkflow as normalizeLiliaConfigDiagnosticsWorkflowImpl,
  normalizeLiliaFixSuggestionMode as normalizeLiliaFixSuggestionModeImpl,
  normalizeLiliaFixSuggestionWorkflow as normalizeLiliaFixSuggestionWorkflowImpl,
  normalizeLiliaGoalStatus as normalizeLiliaGoalStatusImpl,
  normalizeLiliaGoalWorkflow as normalizeLiliaGoalWorkflowImpl,
  normalizeLiliaMemoryModeWorkflow as normalizeLiliaMemoryModeWorkflowImpl,
  normalizeLiliaReviewDelivery as normalizeLiliaReviewDeliveryImpl,
  normalizeLiliaReviewTarget as normalizeLiliaReviewTargetImpl,
  normalizeLiliaReviewWorkflow as normalizeLiliaReviewWorkflowImpl,
  normalizeLiliaTaskWorkflow as normalizeLiliaTaskWorkflowImpl,
  type CreateLiliaBatchApplyWorkflowOptions as ContractCreateLiliaBatchApplyWorkflowOptions,
  type CreateLiliaFixSuggestionWorkflowOptions as ContractCreateLiliaFixSuggestionWorkflowOptions,
  type CreateLiliaGoalWorkflowOptions as ContractCreateLiliaGoalWorkflowOptions,
  type CreateLiliaReviewWorkflowOptions as ContractCreateLiliaReviewWorkflowOptions,
  type CreateLiliaTaskWorkflowOptions as ContractCreateLiliaTaskWorkflowOptions,
  type LiliaBatchApplySourceKind as ContractLiliaBatchApplySourceKind,
  type LiliaFixSuggestionMode as ContractLiliaFixSuggestionMode,
  type LiliaGoalAction as ContractLiliaGoalAction,
  type LiliaGoalStatus as ContractLiliaGoalStatus,
  type LiliaMemoryMode as ContractLiliaMemoryMode,
  type LiliaQueryWorkflowType as ContractLiliaQueryWorkflowType,
  type LiliaReviewDelivery as ContractLiliaReviewDelivery,
  type LiliaReviewTarget as ContractLiliaReviewTarget,
  type LiliaReviewTargetType as ContractLiliaReviewTargetType,
  type LiliaTaskWorkflowKind as ContractLiliaTaskWorkflowKind,
  type NormalizedLiliaBatchApplyWorkflow as ContractNormalizedLiliaBatchApplyWorkflow,
  type NormalizedLiliaConfigDiagnosticsWorkflow as ContractNormalizedLiliaConfigDiagnosticsWorkflow,
  type NormalizedLiliaFixSuggestionWorkflow as ContractNormalizedLiliaFixSuggestionWorkflow,
  type NormalizedLiliaGoalWorkflow as ContractNormalizedLiliaGoalWorkflow,
  type NormalizedLiliaMemoryModeWorkflow as ContractNormalizedLiliaMemoryModeWorkflow,
  type NormalizedLiliaReviewWorkflow as ContractNormalizedLiliaReviewWorkflow,
  type NormalizedLiliaTaskWorkflow as ContractNormalizedLiliaTaskWorkflow,
} from "./liliaWorkflowContract.mjs";
import {
  AUTO_CONTEXT_SCALE_PRESETS,
  AUTO_CONTEXT_THRESHOLDS,
  AUTO_MODEL_BY_BACKEND_AND_TIER,
  AUTO_PRESET_REASONING_EFFORT,
  AUTO_REASONING_EFFORT_BY_TIER,
  AUTO_RUNTIME_COMMAND_SIGNAL_LABELS,
  AUTO_RUNTIME_COMMAND_TYPES_BY_PRESET,
  AUTO_RUNTIME_COMMAND_TYPES_BY_TIER,
  AUTO_WORKFLOW_TYPES_BY_PRESET,
  AUTO_WORKFLOW_TYPES_BY_TIER,
  BUILTIN_MODEL_PRESET_IDS,
  BUILTIN_PRESET_LABELS,
  PLAN_MODE_PRESET,
  PRESET_TIER_MAP,
  autoContextThresholdsForScale as autoContextThresholdsForScaleImpl,
  autoModelForBackendPreset as autoModelForBackendPresetImpl,
  autoModelForBackendTier as autoModelForBackendTierImpl,
  autoPresetForContextScale as autoPresetForContextScaleImpl,
  autoPresetForRuntimeCommandType as autoPresetForRuntimeCommandTypeImpl,
  autoPresetForWorkflowType as autoPresetForWorkflowTypeImpl,
  autoReasoningEffortForPreset as autoReasoningEffortForPresetImpl,
  autoReasoningEffortForTier as autoReasoningEffortForTierImpl,
  autoRuntimeCommandSignalLabel as autoRuntimeCommandSignalLabelImpl,
  autoTierForRuntimeCommandType as autoTierForRuntimeCommandTypeImpl,
  autoTierForWorkflowType as autoTierForWorkflowTypeImpl,
  builtinPresetLabel as builtinPresetLabelImpl,
  createDefaultBuiltinPresets as createDefaultBuiltinPresetsImpl,
  isBuiltinModelPresetId as isBuiltinModelPresetIdImpl,
  MODEL_SELECTION_TIERS,
  normalizeModelFeatureSettings as normalizeModelFeatureSettingsImpl,
  tierForPreset as tierForPresetImpl,
  type BuiltinModelPresetId as ContractBuiltinModelPresetId,
  type ModelSelectionContextScale as ContractModelSelectionContextScale,
  type ModelSelectionContextThresholds as ContractModelSelectionContextThresholds,
  type ModelTier as ContractModelTier,
} from "./modelSelectionDefaults.mjs";
import {
  createProcessSessionCommand as createProcessSessionCommandImpl,
  createRemoteEnvironmentCommand as createRemoteEnvironmentCommandImpl,
  createRuntimeSettingsCommand as createRuntimeSettingsCommandImpl,
  createSandboxDiagnosticsCommand as createSandboxDiagnosticsCommandImpl,
  createSessionForkCommand as createSessionForkCommandImpl,
  DEFAULT_SESSION_FORK_EXCLUDE_TURNS,
  DEFAULT_SESSION_FORK_MODE,
  isProcessSessionAction as isProcessSessionActionImpl,
  isRemoteEnvironmentAction as isRemoteEnvironmentActionImpl,
  isRuntimeSettingsAction as isRuntimeSettingsActionImpl,
  isSessionForkMode as isSessionForkModeImpl,
  normalizeProcessSessionCommand as normalizeProcessSessionCommandImpl,
  normalizeRemoteEnvironmentCommand as normalizeRemoteEnvironmentCommandImpl,
  normalizeRuntimeSettingsCommand as normalizeRuntimeSettingsCommandImpl,
  normalizeSandboxDiagnosticsCommand as normalizeSandboxDiagnosticsCommandImpl,
  normalizeSessionForkCommand as normalizeSessionForkCommandImpl,
  PROCESS_SESSION_ACTIONS,
  PROCESS_SESSION_COMMAND_TYPE,
  REMOTE_ENVIRONMENT_ACTIONS,
  REMOTE_ENVIRONMENT_COMMAND_TYPE,
  RUNTIME_SETTINGS_ACTIONS,
  RUNTIME_SETTINGS_COMMAND_TYPE,
  SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
  SESSION_FORK_COMMAND_TYPE,
  SESSION_FORK_MODES,
  type NormalizedProcessSessionCommand as RuntimeContractNormalizedProcessSessionCommand,
  type NormalizedRemoteEnvironmentCommand as RuntimeContractNormalizedRemoteEnvironmentCommand,
  type NormalizedRuntimeSettingsCommand as RuntimeContractNormalizedRuntimeSettingsCommand,
  type NormalizedSandboxDiagnosticsCommand as RuntimeContractNormalizedSandboxDiagnosticsCommand,
  type NormalizedSessionForkCommand as RuntimeContractNormalizedSessionForkCommand,
  type ProcessSessionAction as RuntimeContractProcessSessionAction,
  type RemoteEnvironmentAction as RuntimeContractRemoteEnvironmentAction,
  type RuntimeSettingsAction as RuntimeContractRuntimeSettingsAction,
  type SessionForkMode as RuntimeContractSessionForkMode,
} from "./runtimeCommandContract.mjs";
import {
  createSessionManagementRuntimeCommand as createSessionManagementRuntimeCommandImpl,
  DEFAULT_SESSION_MANAGEMENT_ARCHIVED,
  DEFAULT_SESSION_MANAGEMENT_LIMIT,
  isSessionManagementAction as isSessionManagementActionImpl,
  MAX_SESSION_MANAGEMENT_LIMIT,
  normalizeSessionManagementRuntimeCommand as normalizeSessionManagementRuntimeCommandImpl,
  SESSION_MANAGEMENT_ACTIONS,
  SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE,
  type NormalizedSessionManagementRuntimeCommand as SessionContractNormalizedSessionManagementRuntimeCommand,
  type SessionManagementAction as SessionContractSessionManagementAction,
} from "./sessionManagementContract.mjs";
import {
  AUTO_TURN_DECISION_PERMISSION_KEYS,
  AUTO_TURN_DECISION_PERMISSION_OPTIONS,
  DEFAULT_AGENT_INTERACTION_SETTINGS,
  DEFAULT_AGENT_SUBAGENT_MODE_SETTINGS,
  DEFAULT_AUTO_TURN_DECISION_SETTINGS,
  DEFAULT_PERMISSION_MODE_AVAILABILITY,
  LOCKED_PERMISSION_MODES,
  enabledPermissionModes as enabledPermissionModesImpl,
  normalizeAgentInteractionSettings as normalizeAgentInteractionSettingsImpl,
  normalizeAgentSubagentModeSettings as normalizeAgentSubagentModeSettingsImpl,
  normalizeAutoTurnDecisionSettings as normalizeAutoTurnDecisionSettingsImpl,
  normalizeAvailablePermissionMode as normalizeAvailablePermissionModeImpl,
  type AgentInteractionSettings as ContractAgentInteractionSettings,
  type AgentSubagentBackendSettings as ContractAgentSubagentBackendSettings,
  type AgentSubagentModeSettings as ContractAgentSubagentModeSettings,
  type AutoTurnDecisionPermissionKey as ContractAutoTurnDecisionPermissionKey,
  type AutoTurnDecisionPermissionOption as ContractAutoTurnDecisionPermissionOption,
  type AutoTurnDecisionSettings as ContractAutoTurnDecisionSettings,
  type ClaudeSubagentModeSettings as ContractClaudeSubagentModeSettings,
  type MainAgentPromptMode as ContractMainAgentPromptMode,
  type PermissionModeAvailability as ContractPermissionModeAvailability,
  normalizePermissionModeAvailability as normalizePermissionModeAvailabilityImpl,
} from "./agentInteractionDefaults.mjs";

export type ChatBackendKind = (typeof CHAT_BACKENDS)[number];
export type ReasoningEffort = (typeof REASONING_EFFORTS)[number];

export {
  AGENT_TIMELINE_BATCH_EVENT_NAME,
  AGENT_TIMELINE_CLEAR_TASK_COMMAND,
  AGENT_TIMELINE_EVENT_NAME,
  AGENT_TIMELINE_LIST_COMMAND,
  ALLOWED_MODEL_PREFIXES_BY_BACKEND,
  BACKEND_REASONING_EFFORTS,
  CHAT_ACK_RESTORED_ROLLBACK_COMMAND,
  CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME,
  CHAT_BACKENDS,
  CHAT_BACKEND_LABELS,
  CHAT_CONTEXT_USAGE_EVENT_NAME,
  CHAT_DESCRIBE_ATTACHMENTS_COMMAND,
  CHAT_DONE_EVENT_NAME,
  CHAT_GET_COMPOSER_DRAFT_COMMAND,
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
  CHAT_SEND_PROCESS_SESSION_COMMAND,
  CHAT_SET_COMPOSER_DRAFT_COMMAND,
  CHAT_SET_COMPOSER_STATE_COMMAND,
  CHAT_TURN_STARTED_EVENT_NAME,
  DEFAULT_CHAT_BACKEND,
  DEFAULT_MODEL_BY_BACKEND,
  LILIA_IAB_OPEN_COMMAND,
  LILIA_IAB_SUBMIT_COMMAND,
  MODEL_OPTIONS_BY_BACKEND,
  REASONING_EFFORTS,
};

export function createChatBackendRecord<T>(
  factory: (backend: ChatBackendKind) => T,
): Record<ChatBackendKind, T> {
  return Object.fromEntries(
    CHAT_BACKENDS.map((backend) => [backend, factory(backend)]),
  ) as Record<ChatBackendKind, T>;
}

function resolveKnownBackend(backend: string): ChatBackendKind {
  if ((CHAT_BACKENDS as readonly string[]).includes(backend)) {
    return backend as ChatBackendKind;
  }
  return DEFAULT_CHAT_BACKEND;
}

export function defaultModelForBackend(backend: ChatBackendKind): string {
  return DEFAULT_MODEL_BY_BACKEND[resolveKnownBackend(backend)];
}

export function chatBackendLabel(backend: ChatBackendKind): string {
  return CHAT_BACKEND_LABELS[resolveKnownBackend(backend)] ?? String(backend);
}

export function modelOptionsForBackend(backend: ChatBackendKind): readonly ChatModelOption[] {
  return MODEL_OPTIONS_BY_BACKEND[resolveKnownBackend(backend)] ?? [];
}

export function allowedModelPrefixesForBackend(backend: ChatBackendKind): readonly string[] {
  return ALLOWED_MODEL_PREFIXES_BY_BACKEND[resolveKnownBackend(backend)] ?? [];
}

export function modelBelongsToBackend(backend: ChatBackendKind, model: string): boolean {
  const trimmed = model.trim();
  if (!trimmed) return false;
  return (
    modelOptionsForBackend(backend).some((option) => option.id === trimmed) ||
    allowedModelPrefixesForBackend(backend).some((prefix) => trimmed.startsWith(prefix))
  );
}

export function reasoningEffortsForBackend(backend: ChatBackendKind): readonly ReasoningEffort[] {
  return BACKEND_REASONING_EFFORTS[resolveKnownBackend(backend)] ?? REASONING_EFFORTS;
}

export type ChatRole = "user" | "assistant" | "system";

export type ChatAttachmentKind = "file" | "directory" | "unknown";

export interface ChatAttachmentDirectoryMeta {
  fileCount: number;
  directoryCount: number;
  totalSize: number;
  truncated: boolean;
  unreadableCount: number;
}

export interface ChatAttachment {
  id: string;
  name: string;
  path: string;
  kind: ChatAttachmentKind;
  size: number | null;
  exists?: boolean;
  mime?: string | null;
  directory?: ChatAttachmentDirectoryMeta | null;
}

export type ChatSerializablePayload =
  | null
  | boolean
  | number
  | string
  | ChatSerializablePayload[]
  | { [key: string]: ChatSerializablePayload };

export type ChatAttachmentPayload = Record<string, ChatSerializablePayload>;

export function chatAttachmentToPayload(attachment: ChatAttachment): ChatAttachmentPayload {
  return {
    id: attachment.id,
    name: attachment.name,
    path: attachment.path,
    kind: attachment.kind,
    size: attachment.size,
    exists: attachment.exists ?? null,
    mime: attachment.mime ?? null,
    directory: attachment.directory ? { ...attachment.directory } : null,
  };
}

export function chatAttachmentsToPayload(
  attachments: readonly ChatAttachment[],
): ChatAttachmentPayload[] {
  return attachments.map(chatAttachmentToPayload);
}

function readChatAttachmentKind(value: unknown): ChatAttachmentKind | null {
  return value === "file" || value === "directory" || value === "unknown" ? value : null;
}

function isChatAttachmentDirectoryMeta(value: unknown): value is ChatAttachmentDirectoryMeta {
  const row = recordValue(value);
  return !!row &&
    typeof row.fileCount === "number" &&
    typeof row.directoryCount === "number" &&
    typeof row.totalSize === "number" &&
    typeof row.truncated === "boolean" &&
    typeof row.unreadableCount === "number";
}

function readChatAttachmentDirectoryMeta(value: unknown): ChatAttachmentDirectoryMeta | null {
  if (!isChatAttachmentDirectoryMeta(value)) return null;
  return {
    fileCount: value.fileCount,
    directoryCount: value.directoryCount,
    totalSize: value.totalSize,
    truncated: value.truncated,
    unreadableCount: value.unreadableCount,
  };
}

function normalizeChatAttachment(value: unknown): ChatAttachment | null {
  const row = recordValue(value);
  const kind = readChatAttachmentKind(row?.kind);
  if (
    !row ||
    typeof row.id !== "string" ||
    typeof row.name !== "string" ||
    typeof row.path !== "string" ||
    !kind ||
    !(typeof row.size === "number" || row.size === null)
  ) {
    return null;
  }

  const attachment: ChatAttachment = {
    id: row.id,
    name: row.name,
    path: row.path,
    kind,
    size: row.size,
  };
  if (typeof row.exists === "boolean") {
    attachment.exists = row.exists;
  }
  if (typeof row.mime === "string" || row.mime === null) {
    attachment.mime = row.mime;
  }
  if (row.directory === null) {
    attachment.directory = null;
  } else {
    const directory = readChatAttachmentDirectoryMeta(row.directory);
    if (directory) {
      attachment.directory = directory;
    }
  }
  return attachment;
}

export function isChatAttachment(value: unknown): value is ChatAttachment {
  const row = recordValue(value);
  return !!row &&
    typeof row.id === "string" &&
    typeof row.name === "string" &&
    typeof row.path === "string" &&
    !!readChatAttachmentKind(row.kind) &&
    (typeof row.size === "number" || row.size === null) &&
    (row.exists === undefined || typeof row.exists === "boolean") &&
    (row.mime === undefined || typeof row.mime === "string" || row.mime === null) &&
    (
      row.directory === undefined ||
      row.directory === null ||
      isChatAttachmentDirectoryMeta(row.directory)
    );
}

export function readChatAttachments(value: unknown): ChatAttachment[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    const attachment = normalizeChatAttachment(item);
    return attachment ? [attachment] : [];
  });
}

export function readChatAttachmentPayloadPaths(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value
    .map((item) => {
      const row = recordValue(item);
      return typeof row?.path === "string" ? row.path : "";
    })
    .filter(Boolean);
}

export const CHAT_ATTACHMENT_REFERENCE_LABELS = {
  file: "文件引用",
  directory: "目录引用",
  image: "图片引用",
} as const;

export type ChatAttachmentReferenceLabel =
  (typeof CHAT_ATTACHMENT_REFERENCE_LABELS)[keyof typeof CHAT_ATTACHMENT_REFERENCE_LABELS];

const CHAT_ATTACHMENT_REFERENCE_LABEL_PATTERN = /\[(文件引用|目录引用|图片引用): ([^\]\n|]+?) \| ([^\]\n]+?)\]/g;

export function chatAttachmentReferencePattern(): RegExp {
  return new RegExp(CHAT_ATTACHMENT_REFERENCE_LABEL_PATTERN);
}

export function isChatImageAttachment(attachment: ChatAttachment): boolean {
  return attachment.exists !== false && !!attachment.mime?.startsWith("image/");
}

export const LARGE_CHAT_ATTACHMENT_DIRECTORY_FILE_COUNT = 200;
export const LARGE_CHAT_ATTACHMENT_DIRECTORY_TOTAL_SIZE = 20 * 1024 * 1024;

export function chatAttachmentSizeLabel(size: number | null | undefined): string {
  if (typeof size !== "number" || !Number.isFinite(size)) return "大小未知";
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${Math.round(size / 1024)} KB`;
  return `${Math.round(size / (1024 * 1024))} MB`;
}

export function isLargeChatAttachmentDirectory(attachment: ChatAttachment): boolean {
  const directory = attachment.directory;
  if (!directory) return false;
  return directory.fileCount >= LARGE_CHAT_ATTACHMENT_DIRECTORY_FILE_COUNT ||
    directory.totalSize >= LARGE_CHAT_ATTACHMENT_DIRECTORY_TOTAL_SIZE ||
    directory.truncated;
}

export function chatAttachmentDirectorySummaryLabel(attachment: ChatAttachment): string | null {
  const directory = attachment.directory;
  if (!directory) return null;
  const parts = [
    `${directory.fileCount} 个文件`,
    `${directory.directoryCount} 个目录`,
    chatAttachmentSizeLabel(directory.totalSize),
  ];
  if (directory.truncated) parts.push("未完全统计");
  if (directory.unreadableCount > 0) parts.push(`${directory.unreadableCount} 处不可读`);
  return parts.join(" · ");
}

export function chatAttachmentMetaLabel(attachment: ChatAttachment): string {
  if (attachment.exists === false) return "路径不存在";
  if (attachment.kind === "directory") {
    const summary = chatAttachmentDirectorySummaryLabel(attachment);
    return isLargeChatAttachmentDirectory(attachment)
      ? `目录较大${summary ? ` · ${summary}` : ""}`
      : summary ?? "目录";
  }
  if (isChatImageAttachment(attachment)) return "图片";
  if (attachment.kind === "file") return chatAttachmentSizeLabel(attachment.size);
  return "未知路径";
}

export function chatAttachmentReferenceLabel(
  attachment: ChatAttachment,
): ChatAttachmentReferenceLabel {
  if (isChatImageAttachment(attachment)) return CHAT_ATTACHMENT_REFERENCE_LABELS.image;
  if (attachment.kind === "directory") return CHAT_ATTACHMENT_REFERENCE_LABELS.directory;
  return CHAT_ATTACHMENT_REFERENCE_LABELS.file;
}

export function serializeChatAttachmentReference(attachment: ChatAttachment): string {
  return `[${chatAttachmentReferenceLabel(attachment)}: ${attachment.name} | ${attachment.path}]`;
}

export function resolveChatAttachmentReferenceMatch(
  match: RegExpMatchArray,
  attachments: readonly ChatAttachment[],
): ChatAttachment {
  const [, label, rawName, rawPath] = match;
  const name = rawName.trim();
  const path = rawPath.trim();
  return attachments.find((attachment) => attachment.path === path) ??
    fallbackChatAttachmentFromReference(label, name, path);
}

function fallbackChatAttachmentFromReference(
  label: string,
  name: string,
  path: string,
): ChatAttachment {
  return {
    id: `inline-${path}`,
    name: name || path,
    path,
    kind: label === CHAT_ATTACHMENT_REFERENCE_LABELS.directory ? "directory" : "file",
    size: null,
    exists: true,
    mime: label === CHAT_ATTACHMENT_REFERENCE_LABELS.image ? "image/*" : null,
    directory: null,
  };
}

export interface ChatConversationReference {
  taskId: string;
  title: string;
  route: string;
  projectId?: string;
  projectName?: string;
}

export type ChatConversationReferencePayload = Record<string, string | null>;

const CONVERSATION_REFERENCE_LABEL_PATTERN = /\[对话引用: ([^\]\n|]+?) \| ([^\]\n]+?)\]/g;

export function conversationReferencePattern(): RegExp {
  return new RegExp(CONVERSATION_REFERENCE_LABEL_PATTERN);
}

export function serializeConversationReference(reference: ChatConversationReference): string {
  return `[对话引用: ${reference.title} | ${reference.taskId}]`;
}

export function conversationReferenceToPayload(
  reference: ChatConversationReference,
): ChatConversationReferencePayload {
  return {
    taskId: reference.taskId,
    title: reference.title,
    route: reference.route,
    projectId: reference.projectId ?? null,
    projectName: reference.projectName ?? null,
  };
}

export function conversationReferencesToPayload(
  conversationReferences: readonly ChatConversationReference[],
): ChatConversationReferencePayload[] {
  return conversationReferences.map(conversationReferenceToPayload);
}

export function isChatConversationReference(value: unknown): value is ChatConversationReference {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const row = value as Record<string, unknown>;
  return typeof row.taskId === "string" &&
    typeof row.title === "string" &&
    typeof row.route === "string" &&
    (typeof row.projectId === "string" || typeof row.projectId === "undefined" || row.projectId === null) &&
    (typeof row.projectName === "string" || typeof row.projectName === "undefined" || row.projectName === null);
}

export function readConversationReferences(value: unknown): ChatConversationReference[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter(isChatConversationReference)
    .map((reference) => ({
      taskId: reference.taskId,
      title: reference.title,
      route: reference.route,
      projectId: reference.projectId ?? undefined,
      projectName: reference.projectName ?? undefined,
    }));
}

export function resolveConversationReferenceMatch(
  match: RegExpMatchArray,
  conversationReferences: readonly ChatConversationReference[],
): ChatConversationReference {
  const [, rawTitle, rawTaskId] = match;
  const title = rawTitle.trim();
  const taskId = rawTaskId.trim();
  return conversationReferences.find((reference) => reference.taskId === taskId) ?? {
    taskId,
    title: title || taskId,
    route: "",
  };
}

export function stripSerializedConversationReferences(
  content: string,
  conversationReferences: readonly ChatConversationReference[],
): string {
  let next = content;
  for (const reference of conversationReferences) {
    next = next.split(serializeConversationReference(reference)).join("");
  }
  return next;
}

export interface ChatContextSearchResult {
  attachment: ChatAttachment;
  relativePath: string;
  matchedBy: "name" | "path";
}

export type ChatSlashCommandSource = "native" | "project";

export const CHAT_SLASH_COMMAND_SOURCE_LABELS: Record<ChatSlashCommandSource, string> = {
  native: "内置",
  project: "项目",
};

const CHAT_SLASH_COMMAND_SOURCE_SET = new Set<string>(
  Object.keys(CHAT_SLASH_COMMAND_SOURCE_LABELS),
);

export function isChatSlashCommandSource(value: unknown): value is ChatSlashCommandSource {
  return typeof value === "string" && CHAT_SLASH_COMMAND_SOURCE_SET.has(value);
}

export function chatSlashCommandSourceLabel(source: ChatSlashCommandSource): string {
  return CHAT_SLASH_COMMAND_SOURCE_LABELS[source];
}

export interface ChatSlashCommandParameter {
  name: string;
  label: string;
  required: boolean;
  hint?: string | null;
}

export interface ChatSlashCommand {
  id: string;
  name: string;
  title: string;
  description: string;
  source: ChatSlashCommandSource;
  parameters: ChatSlashCommandParameter[];
}

export interface ChatSlashCommandSearchResult {
  command: ChatSlashCommand;
  matchedBy: "name" | "title" | "description";
}

export type ChatWorkflowSlashKind =
  | "review"
  | "fix_suggestion"
  | "task:generalTask"
  | "task:bugLocalization"
  | "task:frontend"
  | "task:refactor"
  | "task:testAndVerification"
  | "task:docsAndPrompt"
  | "task:gitAndRelease"
  | "task:architectureAndMemory";

export const CHAT_WORKFLOW_SLASH_COMMANDS: ReadonlyArray<{
  kind: ChatWorkflowSlashKind;
  command: ChatSlashCommand;
}> = [
  {
    kind: "review",
    command: {
      id: `workflow:${LILIA_REVIEW_WORKFLOW_TYPE}`,
      name: "review",
      title: "代码审查",
      description: "对指定代码范围做审查。",
      source: "native",
      parameters: [],
    },
  },
  {
    kind: "fix_suggestion",
    command: {
      id: `workflow:${LILIA_FIX_SUGGESTION_WORKFLOW_TYPE}`,
      name: "fix",
      title: "修复建议",
      description: "生成修复建议。",
      source: "native",
      parameters: [],
    },
  },
  {
    kind: "task:generalTask",
    command: {
      id: `workflow:${LILIA_TASK_WORKFLOW_TYPE}:generalTask`,
      name: "task",
      title: "通用实现任务",
      description: "按通用实现任务工作流发送。",
      source: "native",
      parameters: [],
    },
  },
  {
    kind: "task:bugLocalization",
    command: {
      id: `workflow:${LILIA_TASK_WORKFLOW_TYPE}:bugLocalization`,
      name: "debug",
      title: "问题定位",
      description: "按问题定位工作流发送。",
      source: "native",
      parameters: [],
    },
  },
  {
    kind: "task:frontend",
    command: {
      id: `workflow:${LILIA_TASK_WORKFLOW_TYPE}:frontend`,
      name: "frontend",
      title: "前端与交互",
      description: "按前端与交互工作流发送。",
      source: "native",
      parameters: [],
    },
  },
  {
    kind: "task:refactor",
    command: {
      id: `workflow:${LILIA_TASK_WORKFLOW_TYPE}:refactor`,
      name: "refactor",
      title: "重构与结构调整",
      description: "按重构与结构调整工作流发送。",
      source: "native",
      parameters: [],
    },
  },
  {
    kind: "task:testAndVerification",
    command: {
      id: `workflow:${LILIA_TASK_WORKFLOW_TYPE}:testAndVerification`,
      name: "verify",
      title: "测试与验证",
      description: "按测试与验证工作流发送。",
      source: "native",
      parameters: [],
    },
  },
  {
    kind: "task:docsAndPrompt",
    command: {
      id: `workflow:${LILIA_TASK_WORKFLOW_TYPE}:docsAndPrompt`,
      name: "docs",
      title: "文档与提示词",
      description: "按文档与提示词工作流发送。",
      source: "native",
      parameters: [],
    },
  },
  {
    kind: "task:gitAndRelease",
    command: {
      id: `workflow:${LILIA_TASK_WORKFLOW_TYPE}:gitAndRelease`,
      name: "git",
      title: "Git 与发布",
      description: "按 Git 与发布工作流发送。",
      source: "native",
      parameters: [],
    },
  },
  {
    kind: "task:architectureAndMemory",
    command: {
      id: `workflow:${LILIA_TASK_WORKFLOW_TYPE}:architectureAndMemory`,
      name: "architecture",
      title: "架构图与记忆",
      description: "按架构图与记忆工作流发送。",
      source: "native",
      parameters: [],
    },
  },
];

export function chatWorkflowSlashKindLabel(kind: ChatWorkflowSlashKind | null | undefined): string {
  return CHAT_WORKFLOW_SLASH_COMMANDS.find((item) => item.kind === kind)?.command.title ?? "工作流";
}

export interface ChatMessage {
  id: string;
  taskId: string;
  role: ChatRole;
  content: string;
  attachments: ChatAttachment[];
  conversationReferences?: ChatConversationReference[];
  createdAt: number;
}

export type ChatMessageSegment =
  | { type: "text"; text: string }
  | { type: "attachment"; attachment: ChatAttachment }
  | { type: "conversationReference"; reference: ChatConversationReference };

export type ChatMessageDisplay = {
  segments: ChatMessageSegment[];
  previewAttachments: ChatAttachment[];
  unreferencedAttachments: ChatAttachment[];
};

export function deriveChatMessageDisplay(
  content: string,
  attachments: readonly ChatAttachment[],
  conversationReferences: readonly ChatConversationReference[] = [],
): ChatMessageDisplay {
  const previewAttachments: ChatAttachment[] = [];
  for (const attachment of attachments) {
    if (isChatImageAttachment(attachment)) previewAttachments.push(attachment);
  }

  const segments: ChatMessageSegment[] = [];
  const referencedAttachmentPaths = new Set<string>();
  let cursor = 0;
  const matches = [
    ...Array.from(content.matchAll(chatAttachmentReferencePattern()), (match) => ({
      kind: "attachment" as const,
      match,
    })),
    ...Array.from(content.matchAll(conversationReferencePattern()), (match) => ({
      kind: "conversationReference" as const,
      match,
    })),
  ].sort((a, b) => (a.match.index ?? 0) - (b.match.index ?? 0));

  for (const entry of matches) {
    const start = entry.match.index ?? 0;
    if (start > cursor) segments.push({ type: "text", text: content.slice(cursor, start) });
    if (entry.kind === "attachment") {
      const attachment = resolveChatAttachmentReferenceMatch(entry.match, attachments);
      referencedAttachmentPaths.add(attachment.path);
      segments.push({
        type: "attachment",
        attachment,
      });
    } else {
      segments.push({
        type: "conversationReference",
        reference: resolveConversationReferenceMatch(entry.match, conversationReferences),
      });
    }
    cursor = start + entry.match[0].length;
  }

  if (cursor < content.length) segments.push({ type: "text", text: content.slice(cursor) });
  const normalizedSegments = segments.length ? segments : [{ type: "text" as const, text: content }];
  const unreferencedAttachments = attachments.filter((attachment) =>
    !referencedAttachmentPaths.has(attachment.path) && !isChatImageAttachment(attachment)
  );

  return {
    segments: normalizedSegments,
    previewAttachments,
    unreferencedAttachments,
  };
}

export type ChatSendDispatch = "started" | "queued";

export interface ChatSendResult {
  message: ChatMessage;
  dispatch: ChatSendDispatch;
  queuedCount: number;
  turnId: string;
}

export {
  AUTOMATION_WORKFLOW_TYPE,
  CHAT_SLASH_COMMAND_WORKFLOW_TYPE,
  DEFAULT_LILIA_CONFIG_DIAGNOSTICS_INCLUDE_LAYERS,
  DEFAULT_LILIA_FIX_SUGGESTION_MODE,
  DEFAULT_LILIA_GOAL_STATUS,
  DEFAULT_LILIA_REVIEW_DELIVERY,
  LILIA_BACKGROUND_TERMINALS_CLEAN_WORKFLOW_TYPE,
  LILIA_BATCH_APPLY_SOURCE_KINDS,
  LILIA_BATCH_APPLY_WORKFLOW_TYPE,
  LILIA_COMPACT_WORKFLOW_TYPE,
  LILIA_CONFIG_DIAGNOSTICS_WORKFLOW_TYPE,
  LILIA_FIX_SUGGESTION_MODES,
  LILIA_FIX_SUGGESTION_WORKFLOW_TYPE,
  LILIA_GOAL_ACTIONS,
  LILIA_GOAL_STATUSES,
  LILIA_GOAL_WORKFLOW_TYPE,
  LILIA_MEMORY_MODE_WORKFLOW_TYPE,
  LILIA_MEMORY_MODES,
  LILIA_MEMORY_RESET_WORKFLOW_TYPE,
  LILIA_QUERY_WORKFLOW_TYPES,
  LILIA_REVIEW_DELIVERIES,
  LILIA_REVIEW_TARGET_TYPES,
  LILIA_REVIEW_WORKFLOW_TYPE,
  LILIA_TASK_WORKFLOW_KINDS,
  LILIA_TASK_WORKFLOW_TYPE,
};

export type LiliaQueryWorkflowType = ContractLiliaQueryWorkflowType;
export type LiliaTaskWorkflowKind = ContractLiliaTaskWorkflowKind;
export type LiliaReviewTargetType = ContractLiliaReviewTargetType;
export type LiliaReviewDelivery = ContractLiliaReviewDelivery;
export type LiliaFixSuggestionMode = ContractLiliaFixSuggestionMode;
export type LiliaBatchApplySourceKind = ContractLiliaBatchApplySourceKind;
export type LiliaReviewTarget = ContractLiliaReviewTarget;

export interface LiliaReviewWorkflow {
  type: typeof LILIA_REVIEW_WORKFLOW_TYPE;
  target: LiliaReviewTarget;
  instructions?: string;
  delivery?: LiliaReviewDelivery;
}

export interface LiliaFixSuggestionWorkflow {
  type: typeof LILIA_FIX_SUGGESTION_WORKFLOW_TYPE;
  target: LiliaReviewTarget;
  instructions?: string;
  mode?: LiliaFixSuggestionMode;
}

export interface LiliaBatchApplyWorkflow {
  type: typeof LILIA_BATCH_APPLY_WORKFLOW_TYPE;
  sourceTurnId: string;
  sourceKind: LiliaBatchApplySourceKind;
  sourceSummary: string;
  instructions?: string;
}

export interface LiliaTaskWorkflow {
  type: typeof LILIA_TASK_WORKFLOW_TYPE;
  kind: LiliaTaskWorkflowKind;
  instructions?: string;
}

export type LiliaBatchApplyInput = Pick<
  LiliaBatchApplyWorkflow,
  "sourceTurnId" | "sourceKind" | "sourceSummary"
>;

export type LiliaGoalAction = ContractLiliaGoalAction;
export type LiliaGoalStatus = ContractLiliaGoalStatus;

export const LILIA_GOAL_STATUS_LABELS: Record<LiliaGoalStatus, string> = {
  active: "进行中",
  paused: "已暂停",
  blocked: "受阻",
  usageLimited: "用量受限",
  budgetLimited: "预算受限",
  complete: "已完成",
};

export function liliaGoalStatusLabel(status: LiliaGoalStatus): string {
  return LILIA_GOAL_STATUS_LABELS[status] ?? status;
}

export interface LiliaGoalWorkflow {
  type: typeof LILIA_GOAL_WORKFLOW_TYPE;
  action: LiliaGoalAction;
  objective?: string;
  status?: LiliaGoalStatus;
  tokenBudget?: number | null;
}

export interface LiliaCompactWorkflow {
  type: typeof LILIA_COMPACT_WORKFLOW_TYPE;
}

export interface LiliaBackgroundTerminalsCleanWorkflow {
  type: typeof LILIA_BACKGROUND_TERMINALS_CLEAN_WORKFLOW_TYPE;
}

export type LiliaMemoryMode = ContractLiliaMemoryMode;

export interface LiliaMemoryModeWorkflow {
  type: typeof LILIA_MEMORY_MODE_WORKFLOW_TYPE;
  mode: LiliaMemoryMode;
}

export interface LiliaMemoryResetWorkflow {
  type: typeof LILIA_MEMORY_RESET_WORKFLOW_TYPE;
}

export type NormalizedLiliaGoalWorkflow = ContractNormalizedLiliaGoalWorkflow;
export type NormalizedLiliaMemoryModeWorkflow =
  ContractNormalizedLiliaMemoryModeWorkflow;
export type NormalizedLiliaConfigDiagnosticsWorkflow =
  ContractNormalizedLiliaConfigDiagnosticsWorkflow;
export type NormalizedLiliaReviewWorkflow = ContractNormalizedLiliaReviewWorkflow;
export type NormalizedLiliaTaskWorkflow = ContractNormalizedLiliaTaskWorkflow;
export type NormalizedLiliaFixSuggestionWorkflow =
  ContractNormalizedLiliaFixSuggestionWorkflow;
export type NormalizedLiliaBatchApplyWorkflow =
  ContractNormalizedLiliaBatchApplyWorkflow;

export interface NormalizedAutomationRunWorkflow {
  automationRunId: string;
}

export interface NormalizedChatSlashCommandWorkflow {
  commandId: string;
  source: ChatSlashCommandSource;
  arguments: Record<string, string>;
}

export type CreateLiliaGoalWorkflowOptions = ContractCreateLiliaGoalWorkflowOptions;
export type CreateLiliaReviewWorkflowOptions =
  ContractCreateLiliaReviewWorkflowOptions;
export type CreateLiliaTaskWorkflowOptions =
  ContractCreateLiliaTaskWorkflowOptions;
export type CreateLiliaFixSuggestionWorkflowOptions =
  ContractCreateLiliaFixSuggestionWorkflowOptions;
export type CreateLiliaBatchApplyWorkflowOptions =
  ContractCreateLiliaBatchApplyWorkflowOptions;

export interface CreateChatSlashCommandWorkflowInput {
  commandId: string;
  source: ChatSlashCommandSource;
  arguments?: Record<string, string>;
}

export const isLiliaReviewTargetType = isLiliaReviewTargetTypeImpl;
export const isLiliaQueryWorkflowType = isLiliaQueryWorkflowTypeImpl;
export const isLiliaTaskWorkflowKind = isLiliaTaskWorkflowKindImpl;
export const isLiliaReviewDelivery = isLiliaReviewDeliveryImpl;
export const isLiliaFixSuggestionMode = isLiliaFixSuggestionModeImpl;
export const isLiliaBatchApplySourceKind = isLiliaBatchApplySourceKindImpl;
export const isLiliaGoalAction = isLiliaGoalActionImpl;
export const isLiliaGoalStatus = isLiliaGoalStatusImpl;
export const isLiliaMemoryMode = isLiliaMemoryModeImpl;
export const normalizeLiliaGoalStatus = normalizeLiliaGoalStatusImpl;
export const normalizeLiliaReviewTarget = normalizeLiliaReviewTargetImpl;
export const normalizeLiliaReviewDelivery = normalizeLiliaReviewDeliveryImpl;
export const normalizeLiliaFixSuggestionMode = normalizeLiliaFixSuggestionModeImpl;
export const normalizeLiliaReviewWorkflow = normalizeLiliaReviewWorkflowImpl;
export const normalizeLiliaTaskWorkflow = normalizeLiliaTaskWorkflowImpl;
export const normalizeLiliaFixSuggestionWorkflow =
  normalizeLiliaFixSuggestionWorkflowImpl;
export const normalizeLiliaBatchApplyWorkflow = normalizeLiliaBatchApplyWorkflowImpl;
export const createLiliaReviewWorkflow = createLiliaReviewWorkflowImpl;
export const createLiliaTaskWorkflow = createLiliaTaskWorkflowImpl;
export const createLiliaFixSuggestionWorkflow = createLiliaFixSuggestionWorkflowImpl;
export const createLiliaBatchApplyWorkflow = createLiliaBatchApplyWorkflowImpl;
export const normalizeLiliaGoalWorkflow = normalizeLiliaGoalWorkflowImpl;
export const createLiliaGoalWorkflow = createLiliaGoalWorkflowImpl;
export const createLiliaCompactWorkflow = createLiliaCompactWorkflowImpl;
export const normalizeLiliaMemoryModeWorkflow =
  normalizeLiliaMemoryModeWorkflowImpl;
export const isLiliaMemoryResetWorkflow = isLiliaMemoryResetWorkflowImpl;
export const isLiliaCompactWorkflow = isLiliaCompactWorkflowImpl;
export const isLiliaBackgroundTerminalsCleanWorkflow =
  isLiliaBackgroundTerminalsCleanWorkflowImpl;
export const normalizeLiliaConfigDiagnosticsWorkflow =
  normalizeLiliaConfigDiagnosticsWorkflowImpl;

export function normalizeAutomationRunWorkflow(
  value: unknown,
): NormalizedAutomationRunWorkflow | null {
  const workflow = chatRecordOrNull(value);
  if (workflow?.type !== AUTOMATION_WORKFLOW_TYPE) return null;
  const automationRunId = chatStringOrNull(workflow.automationRunId)?.trim() || "";
  if (!automationRunId) throw new Error("Automation workflow missing automationRunId");
  return { automationRunId };
}

export function createAutomationRunWorkflow(automationRunId: string): AutomationRunWorkflow {
  const normalized = normalizeAutomationRunWorkflow({
    type: AUTOMATION_WORKFLOW_TYPE,
    automationRunId,
  });
  if (!normalized) throw new Error("Automation workflow missing automationRunId");
  return {
    type: AUTOMATION_WORKFLOW_TYPE,
    automationRunId: normalized.automationRunId,
  };
}

export function normalizeChatSlashCommandWorkflow(
  value: unknown,
): NormalizedChatSlashCommandWorkflow | null {
  const workflow = chatRecordOrNull(value);
  if (workflow?.type !== CHAT_SLASH_COMMAND_WORKFLOW_TYPE) return null;
  const commandId = chatStringOrNull(workflow.commandId)?.trim() || "";
  if (!commandId) throw new Error("Chat slash command workflow missing commandId");
  const source = chatStringOrNull(workflow.source);
  if (!isChatSlashCommandSource(source)) {
    throw new Error("Chat slash command workflow missing a valid source");
  }
  return {
    commandId,
    source,
    arguments: chatStringRecord(workflow.arguments),
  };
}

export function createChatSlashCommandWorkflow(
  input: CreateChatSlashCommandWorkflowInput,
): ChatSlashCommandWorkflow {
  const normalized = normalizeChatSlashCommandWorkflow({
    type: CHAT_SLASH_COMMAND_WORKFLOW_TYPE,
    ...input,
    arguments: input.arguments ?? {},
  });
  if (!normalized) throw new Error("Chat slash command workflow missing commandId");
  return {
    type: CHAT_SLASH_COMMAND_WORKFLOW_TYPE,
    commandId: normalized.commandId,
    source: normalized.source,
    arguments: normalized.arguments,
  };
}

export {
  DEFAULT_SESSION_MANAGEMENT_ARCHIVED,
  DEFAULT_SESSION_MANAGEMENT_LIMIT,
  MAX_SESSION_MANAGEMENT_LIMIT,
  SESSION_MANAGEMENT_ACTIONS,
  SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE,
};

export type SessionManagementAction = SessionContractSessionManagementAction;
export type NormalizedSessionManagementRuntimeCommand =
  SessionContractNormalizedSessionManagementRuntimeCommand;

export interface SessionManagementRuntimeCommand {
  type: typeof SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE;
  action: SessionManagementAction;
  sessionId?: string;
  title?: string;
  tag?: string | null;
  archived?: boolean;
  limit?: number;
  cursor?: string;
  searchTerm?: string;
  includeSystemMessages?: boolean;
}

export type CreateSessionManagementRuntimeCommandOptions = Partial<
  Omit<SessionManagementRuntimeCommand, "type" | "action">
>;

export const isSessionManagementAction = isSessionManagementActionImpl as (
  value: unknown,
) => value is SessionManagementAction;

export const normalizeSessionManagementRuntimeCommand =
  normalizeSessionManagementRuntimeCommandImpl as (
    value: unknown,
  ) => NormalizedSessionManagementRuntimeCommand | null;

export const createSessionManagementRuntimeCommand =
  createSessionManagementRuntimeCommandImpl as (
    action: SessionManagementAction,
    options?: CreateSessionManagementRuntimeCommandOptions,
  ) => SessionManagementRuntimeCommand;

function chatRecordOrNull(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function chatStringOrNull(value: unknown): string | null {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return null;
}

function chatStringRecord(value: unknown): Record<string, string> {
  const row = chatRecordOrNull(value);
  if (!row) return {};
  const out: Record<string, string> = {};
  for (const [key, item] of Object.entries(row)) {
    const normalized = chatStringOrNull(item);
    if (normalized !== null) out[key] = normalized;
  }
  return out;
}

export interface LiliaConfigDiagnosticsWorkflow {
  type: typeof LILIA_CONFIG_DIAGNOSTICS_WORKFLOW_TYPE;
  includeLayers?: boolean;
}

export interface SessionForkRuntimeCommand {
  type: typeof SESSION_FORK_COMMAND_TYPE;
  excludeTurns?: boolean;
  sourceTurnId?: string;
  mode?: SessionForkMode;
}

export interface ChatBranchAnchor {
  sourceTurnId: string;
  mode: SessionForkMode;
}

export interface LiliaRuntimeSettingsCommon {
  model?: string;
  permission?: PermissionMode;
  reasoningEffort?: ReasoningEffort;
  runtimeWorkspaceRoots?: string[];
  modelSelection?: ModelSelectionExplanation;
}

const REASONING_EFFORT_SET = new Set<string>(REASONING_EFFORTS);

export function isReasoningEffort(value: unknown): value is ReasoningEffort {
  return typeof value === "string" && REASONING_EFFORT_SET.has(value);
}

export function normalizeReasoningEffort(value: unknown): ReasoningEffort | null {
  return isReasoningEffort(value) ? value : null;
}

export function normalizeReasoningEffortForBackend(
  backend: ChatBackendKind,
  value: unknown,
): ReasoningEffort | null {
  const effort = normalizeReasoningEffort(value);
  if (!effort) return null;
  const supportedEfforts = reasoningEffortsForBackend(backend);
  if (supportedEfforts.includes(effort)) return effort;
  const effortIndex = REASONING_EFFORTS.indexOf(effort);
  for (let index = effortIndex - 1; index >= 0; index -= 1) {
    const candidate = REASONING_EFFORTS[index];
    if (supportedEfforts.includes(candidate)) return candidate;
  }
  return supportedEfforts[0] ?? null;
}

export type ModelSelectionMode = "auto" | "manual";

export {
  AUTO_CONTEXT_SCALE_PRESETS,
  AUTO_CONTEXT_THRESHOLDS,
  AUTO_MODEL_BY_BACKEND_AND_TIER,
  AUTO_PRESET_REASONING_EFFORT,
  AUTO_REASONING_EFFORT_BY_TIER,
  AUTO_RUNTIME_COMMAND_SIGNAL_LABELS,
  AUTO_RUNTIME_COMMAND_TYPES_BY_PRESET,
  AUTO_RUNTIME_COMMAND_TYPES_BY_TIER,
  AUTO_WORKFLOW_TYPES_BY_PRESET,
  AUTO_WORKFLOW_TYPES_BY_TIER,
  BUILTIN_MODEL_PRESET_IDS,
  BUILTIN_PRESET_LABELS,
  MODEL_SELECTION_TIERS,
  PLAN_MODE_PRESET,
  PRESET_TIER_MAP,
};

export type ModelTier = ContractModelTier;
export type BuiltinModelPresetId = ContractBuiltinModelPresetId;
export type ModelSelectionContextScale = ContractModelSelectionContextScale;

export type ModelSelectionContextThresholds =
  ContractModelSelectionContextThresholds;

export interface ModelSelectionExplanation {
  mode: ModelSelectionMode;
  model: string;
  reasoningEffort?: ReasoningEffort | null;
  /** @deprecated Prefer presetId; retained for older diagnostics. */
  tier?: ModelTier;
  /** Role preset used for auto selection (default/plan/fast/review or custom id). */
  presetId?: string;
  presetLabel?: string;
  planMode?: boolean;
  goalMode?: boolean;
  sessionFork?: boolean;
  source: "auto" | "manual" | "runtimeOptions";
  summary: string;
  signals: string[];
}

export const autoModelForBackendTier = autoModelForBackendTierImpl;
export const autoModelForBackendPreset = autoModelForBackendPresetImpl;
export const autoReasoningEffortForTier = autoReasoningEffortForTierImpl;
export const autoReasoningEffortForPreset = autoReasoningEffortForPresetImpl;
export const autoTierForWorkflowType = autoTierForWorkflowTypeImpl;
export const autoTierForRuntimeCommandType = autoTierForRuntimeCommandTypeImpl;
export const autoPresetForWorkflowType = autoPresetForWorkflowTypeImpl;
export const autoPresetForRuntimeCommandType = autoPresetForRuntimeCommandTypeImpl;
export const autoPresetForContextScale = autoPresetForContextScaleImpl;
export const autoRuntimeCommandSignalLabel = autoRuntimeCommandSignalLabelImpl;
export const autoContextThresholdsForScale = autoContextThresholdsForScaleImpl;
export const tierForPreset = tierForPresetImpl;
export const isBuiltinModelPresetId = isBuiltinModelPresetIdImpl;
export const builtinPresetLabel = builtinPresetLabelImpl;
export const createDefaultBuiltinPresets = createDefaultBuiltinPresetsImpl;
export const normalizeModelFeatureSettings = normalizeModelFeatureSettingsImpl;

/** Anthropic-compatible thinking config (LLM provider concept, not a chat backend). */
export type ClaudeThinkingConfig =
  | { type: "adaptive" }
  | { type: "enabled"; budgetTokens: number }
  | { type: "disabled" };

export type ThinkingConfig = ClaudeThinkingConfig;

export interface ProviderRuntimeOptionsNative {
  model?: string | null;
  reasoningEffort?: ReasoningEffort;
  thinking?: ThinkingConfig;
  allowedTools?: string[];
  disallowedTools?: string[];
  additionalDirectories?: string[];
  additionalContext?: string | null;
  maxTurns?: number;
  maxBudgetUsd?: number;
  tools?: string[] | { type: "preset"; preset: string };
  permissionPromptToolName?: string;
  settings?: string | Record<string, unknown>;
  managedSettings?: Record<string, unknown>;
  settingSources?: string[];
  sandbox?: Record<string, unknown>;
  outputFormat?: Record<string, unknown>;
  includeHookEvents?: boolean;
  forwardSubagentText?: boolean;
  agentProgressSummaries?: boolean;
  continue?: boolean;
  resumeSessionAt?: string;
  sessionId?: string;
  abortAfterMs?: number;
  sessionStore?: Record<string, unknown>;
  runtimeWorkspaceRoots?: string[];
  excludeTurns?: string[];
  environments?: unknown[];
  experimentalRawEvents?: boolean;
  responsesApiClientMetadata?: Record<string, unknown>;
  profile?: string;
  persistExtendedHistory?: boolean;
  initialTurnsPage?: Record<string, unknown> | null;
}

/** @deprecated Use ProviderRuntimeOptionsNative. */
export type ProviderRuntimeOptionsCodex = ProviderRuntimeOptionsNative;
/** @deprecated Use ProviderRuntimeOptionsNative. */
export type ProviderRuntimeOptionsClaude = ProviderRuntimeOptionsNative;

export interface ExperimentalProviderOptions {
  provider: ChatBackendKind;
  capability: string;
  payload: Record<string, unknown>;
  fallback: "diagnostic" | "unsupported" | "ignore";
}

export interface ProviderRuntimeOptions {
  common?: LiliaRuntimeSettingsCommon;
  provider?: {
    "native-agentkit"?: ProviderRuntimeOptionsNative;
  };
  experimentalProviderOptions?: ExperimentalProviderOptions[];
}

export function runtimeOptionsModelForBackend(
  _backend: ChatBackendKind,
  runtimeOptions: ProviderRuntimeOptions | null | undefined,
): string | null {
  const commonModel = runtimeOptions?.common?.model?.trim();
  if (commonModel) return commonModel;
  const nativeModel = runtimeOptions?.provider?.["native-agentkit"]?.model?.trim();
  if (nativeModel) return nativeModel;
  return null;
}

export function runtimeOptionsReasoningEffortForBackend(
  backend: ChatBackendKind,
  runtimeOptions: ProviderRuntimeOptions | null | undefined,
): ReasoningEffort | null {
  const providerEffort = normalizeReasoningEffort(
    runtimeOptions?.provider?.["native-agentkit"]?.reasoningEffort,
  );
  return normalizeReasoningEffortForBackend(
    backend,
    providerEffort ?? runtimeOptions?.common?.reasoningEffort,
  );
}

export function mergeModelSelectionRuntimeOptions(
  backend: ChatBackendKind,
  runtimeOptions: ProviderRuntimeOptions | null | undefined,
  model: string,
  effort: ReasoningEffort | null,
  explanation: ModelSelectionExplanation,
): ProviderRuntimeOptions {
  const nextEffort = normalizeReasoningEffortForBackend(backend, effort);
  return {
    ...(runtimeOptions ?? {}),
    common: {
      ...(runtimeOptions?.common ?? {}),
      model,
      reasoningEffort: nextEffort ?? undefined,
      modelSelection: explanation,
    },
    provider: {
      ...(runtimeOptions?.provider ?? {}),
      "native-agentkit": {
        ...(runtimeOptions?.provider?.["native-agentkit"] ?? {}),
        model,
        reasoningEffort: nextEffort ?? undefined,
        thinking:
          runtimeOptions?.provider?.["native-agentkit"]?.thinking ??
          (nextEffort ? { type: "adaptive" } : undefined),
      },
    },
  };
}

export {
  DEFAULT_SESSION_FORK_EXCLUDE_TURNS,
  DEFAULT_SESSION_FORK_MODE,
  PROCESS_SESSION_ACTIONS,
  PROCESS_SESSION_COMMAND_TYPE,
  REMOTE_ENVIRONMENT_ACTIONS,
  REMOTE_ENVIRONMENT_COMMAND_TYPE,
  RUNTIME_SETTINGS_ACTIONS,
  RUNTIME_SETTINGS_COMMAND_TYPE,
  SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
  SESSION_FORK_COMMAND_TYPE,
  SESSION_FORK_MODES,
};

export type RuntimeSettingsAction = RuntimeContractRuntimeSettingsAction;
export type RemoteEnvironmentAction = RuntimeContractRemoteEnvironmentAction;
export type SessionForkMode = RuntimeContractSessionForkMode;
export type ProcessSessionAction = RuntimeContractProcessSessionAction;

export interface RuntimeSettingsCommand {
  type: typeof RUNTIME_SETTINGS_COMMAND_TYPE;
  action: RuntimeSettingsAction;
  common?: never;
  runtimeOptions?: never;
}

export interface RemoteEnvironmentRuntimeCommand {
  type: typeof REMOTE_ENVIRONMENT_COMMAND_TYPE;
  action: RemoteEnvironmentAction;
  environmentId?: string;
  environment?: Record<string, unknown>;
}

export interface SandboxDiagnosticsRuntimeCommand {
  type: typeof SANDBOX_DIAGNOSTICS_COMMAND_TYPE;
  includeDetails?: boolean;
}

export interface ProcessSessionRuntimeCommand {
  type: typeof PROCESS_SESSION_COMMAND_TYPE;
  action: ProcessSessionAction;
  processId?: string;
  command?: string;
  cwd?: string;
  stdin?: string;
  rows?: number;
  cols?: number;
  env?: Record<string, string>;
  tty?: boolean;
  permissionProfile?: string;
}

export type CreateRemoteEnvironmentCommandOptions = Partial<
  Omit<RemoteEnvironmentRuntimeCommand, "type" | "action">
>;
export type CreateProcessSessionCommandOptions = Partial<
  Omit<ProcessSessionRuntimeCommand, "type" | "action">
>;

export type NormalizedRuntimeSettingsCommand =
  RuntimeContractNormalizedRuntimeSettingsCommand;
export type NormalizedRemoteEnvironmentCommand =
  RuntimeContractNormalizedRemoteEnvironmentCommand;
export type NormalizedSandboxDiagnosticsCommand =
  RuntimeContractNormalizedSandboxDiagnosticsCommand;
export type NormalizedSessionForkCommand =
  RuntimeContractNormalizedSessionForkCommand;
export type NormalizedProcessSessionCommand =
  RuntimeContractNormalizedProcessSessionCommand;

export const isRuntimeSettingsAction = isRuntimeSettingsActionImpl as (
  value: unknown,
) => value is RuntimeSettingsAction;

export const isRemoteEnvironmentAction = isRemoteEnvironmentActionImpl as (
  value: unknown,
) => value is RemoteEnvironmentAction;

export const isSessionForkMode = isSessionForkModeImpl as (
  value: unknown,
) => value is SessionForkMode;

export const isProcessSessionAction = isProcessSessionActionImpl as (
  value: unknown,
) => value is ProcessSessionAction;

export const normalizeRuntimeSettingsCommand =
  normalizeRuntimeSettingsCommandImpl as (
    value: unknown,
  ) => NormalizedRuntimeSettingsCommand | null;

export const createRuntimeSettingsCommand = createRuntimeSettingsCommandImpl as (
  action: RuntimeSettingsAction,
) => RuntimeSettingsCommand;

export const normalizeRemoteEnvironmentCommand =
  normalizeRemoteEnvironmentCommandImpl as (
    value: unknown,
  ) => NormalizedRemoteEnvironmentCommand | null;

export const createRemoteEnvironmentCommand =
  createRemoteEnvironmentCommandImpl as (
    action: RemoteEnvironmentAction,
    options?: CreateRemoteEnvironmentCommandOptions,
  ) => RemoteEnvironmentRuntimeCommand;

export const normalizeSandboxDiagnosticsCommand =
  normalizeSandboxDiagnosticsCommandImpl as (
    value: unknown,
  ) => NormalizedSandboxDiagnosticsCommand | null;

export const createSandboxDiagnosticsCommand =
  createSandboxDiagnosticsCommandImpl as (
    options?: Partial<NormalizedSandboxDiagnosticsCommand>,
  ) => SandboxDiagnosticsRuntimeCommand;

export const normalizeSessionForkCommand = normalizeSessionForkCommandImpl as (
  value: unknown,
) => NormalizedSessionForkCommand | null;

export const createSessionForkCommand = createSessionForkCommandImpl as (
  options?: Partial<NormalizedSessionForkCommand>,
) => SessionForkRuntimeCommand;

export const normalizeProcessSessionCommand =
  normalizeProcessSessionCommandImpl as (
    value: unknown,
  ) => NormalizedProcessSessionCommand | null;

export const createProcessSessionCommand =
  createProcessSessionCommandImpl as (
    action: ProcessSessionAction,
    options?: CreateProcessSessionCommandOptions,
  ) => ProcessSessionRuntimeCommand;

export interface AutomationRunWorkflow {
  type: typeof AUTOMATION_WORKFLOW_TYPE;
  automationRunId: string;
}

export interface ChatSlashCommandWorkflow {
  type: typeof CHAT_SLASH_COMMAND_WORKFLOW_TYPE;
  commandId: string;
  source: ChatSlashCommandSource;
  arguments: Record<string, string>;
}

export interface LiliaThreadGoal {
  threadId: string;
  objective: string;
  status: LiliaGoalStatus;
  tokenBudget: number | null;
  tokensUsed: number;
  timeUsedSeconds: number;
  createdAt: number;
  updatedAt: number;
}

export type ChatWorkflow =
  | LiliaReviewWorkflow
  | LiliaFixSuggestionWorkflow
  | LiliaBatchApplyWorkflow
  | LiliaTaskWorkflow
  | LiliaGoalWorkflow
  | LiliaCompactWorkflow
  | LiliaBackgroundTerminalsCleanWorkflow
  | LiliaMemoryModeWorkflow
  | LiliaMemoryResetWorkflow
  | LiliaConfigDiagnosticsWorkflow
  | AutomationRunWorkflow
  | ChatSlashCommandWorkflow;

export type ChatRuntimeCommand =
  | SessionForkRuntimeCommand
  | SessionManagementRuntimeCommand
  | RuntimeSettingsCommand
  | RemoteEnvironmentRuntimeCommand
  | SandboxDiagnosticsRuntimeCommand
  | ProcessSessionRuntimeCommand;

export interface ChatInterruptResult {
  rolledBack: boolean;
  restoredContent: string;
  restoredAttachments: ChatAttachment[];
  restoredConversationReferences?: ChatConversationReference[];
  removedEventIds: string[];
}

export interface ChatRollbackResult {
  rolledBack: boolean;
  restoredContent: string;
  restoredAttachments: ChatAttachment[];
  restoredConversationReferences?: ChatConversationReference[];
  removedEventIds: string[];
}

export interface ChatTurnStartedEvent {
  taskId: string;
  queuedCount: number;
}

export interface ChatDoneEvent {
  taskId: string;
  sessionId: string | null;
  subtype: string | null;
  rollback?: ChatRollbackResult | null;
}

export function createChatTurnStartedEvent(
  taskId: string,
  queuedCount: number,
): ChatTurnStartedEvent {
  return { taskId, queuedCount };
}

export function createChatDoneEvent(input: {
  taskId: string;
  sessionId?: string | null;
  subtype?: string | null;
  rollback?: ChatRollbackResult | null;
}): ChatDoneEvent {
  return {
    taskId: input.taskId,
    sessionId: input.sessionId ?? null,
    subtype: input.subtype ?? null,
    rollback: input.rollback ?? null,
  };
}

export type ChatRuntimePhase =
  | "idle"
  | "running"
  | "queued"
  | "running_and_queued"
  | "interrupted_pending_finish"
  | "reset_pending_finish"
  | "abandoned";

export interface ChatContextUsage {
  taskId: string;
  backend: ChatBackendKind;
  usedTokens: number;
  limitTokens: number | null;
  usedPercent: number | null;
  source: string;
  updatedAt: number;
  unavailableReason?: string | null;
}

export interface ChatRuntimeSnapshot {
  taskId: string;
  phase: ChatRuntimePhase;
  backend: ChatBackendKind | null;
  turnId: string | null;
  processSessionId?: string | null;
  queuedCount: number;
  pendingRollback: boolean;
  pendingResetCleanup: boolean;
  contextUsage: ChatContextUsage | null;
  rollback?: ChatRollbackResult | null;
}

export type PermissionMode = ContractPermissionMode;
export type PermissionModeDisplay = ContractPermissionModeDisplay;
export type PermissionModeAvailability = ContractPermissionModeAvailability;

export {
  DEFAULT_PERMISSION_MODE,
  DEFAULT_PERMISSION_MODE_AVAILABILITY,
  LOCKED_PERMISSION_MODES,
  PERMISSION_MODE_DISPLAY,
  PERMISSION_MODE_DISPLAY_ORDER,
  PERMISSION_MODES,
};

export const isPermissionMode = isPermissionModeImpl as (
  value: unknown,
) => value is PermissionMode;

export const normalizePermissionMode = normalizePermissionModeImpl as (
  value: unknown,
  fallback?: PermissionMode,
) => PermissionMode;

export const normalizePermissionModeAvailability = normalizePermissionModeAvailabilityImpl as (
  input: Partial<PermissionModeAvailability> | null | undefined,
  base?: PermissionModeAvailability,
) => PermissionModeAvailability;

export const enabledPermissionModes = enabledPermissionModesImpl as (
  availability?: Partial<PermissionModeAvailability> | null,
) => PermissionMode[];

export const normalizeAvailablePermissionMode = normalizeAvailablePermissionModeImpl as (
  value: unknown,
  availability?: Partial<PermissionModeAvailability> | null,
  fallback?: PermissionMode,
) => PermissionMode;

const CHAT_BACKEND_SET = new Set<string>(CHAT_BACKENDS);

export function isChatBackendKind(value: unknown): value is ChatBackendKind {
  return typeof value === "string" && CHAT_BACKEND_SET.has(value);
}

export function normalizeChatBackendKind(
  value: unknown,
  fallback: ChatBackendKind = DEFAULT_CHAT_BACKEND,
): ChatBackendKind {
  return isChatBackendKind(value) ? value : fallback;
}

export interface ChatComposerState {
  taskId: string;
  backend: ChatBackendKind;
  model: string;
  modelSelectionMode?: ModelSelectionMode;
  reasoningEffort?: ReasoningEffort | null;
  planMode: boolean;
  goalMode: boolean;
  permission: PermissionMode;
}

export interface ChatModelOption {
  id: string;
  label: string;
  backend: ChatBackendKind;
}

export type ToolConsentDecision = "allow" | "deny";
/** Provider-neutral extended tool consent decisions (Mutsuki / Lilia interaction). */
export type ToolConsentDecisionKind = "accept" | "decline" | "cancel" | (string & {});
/** @deprecated Use ToolConsentDecisionKind. */
export type CodexToolConsentDecision = ToolConsentDecisionKind;
export type ToolConsentUpdatedInput = Record<string, unknown>;

export interface ToolConsentRequest {
  taskId: string;
  turnId: string;
  backend: ChatBackendKind;
  requestId: string;
  toolName: string;
  input: ToolConsentUpdatedInput;
  title: string | null;
  displayName: string | null;
  description: string | null;
  blockedPath: string | null;
  decisionReason: string | null;
  toolUseId: string | null;
  additionalPermissions?: unknown;
  availableDecisions?: ToolConsentDecisionKind[];
  proposedExecpolicyAmendment?: unknown;
  proposedNetworkPolicyAmendments?: unknown;
  networkApprovalContext?: unknown;
  cwd?: string | null;
  reason?: string | null;
  commandActions?: unknown;
}

export interface ToolConsentResponsePayload {
  taskId: string;
  requestId: string;
  decision: ToolConsentDecision;
  message: string | null;
  updatedInput?: ToolConsentUpdatedInput;
  /** Extended decision for native tool consent round-trip. */
  decisionKind?: ToolConsentDecisionKind;
  /** @deprecated Use decisionKind. */
  codexDecision?: ToolConsentDecisionKind;
}

function stringValue(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function recordValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

export function stringifyCodexToolCommand(value: unknown): string {
  if (Array.isArray(value)) {
    return value
      .map((part) => {
        if (typeof part === "string") return part;
        const row = recordValue(part);
        if (row) {
          return stringValue(row.text) ||
            stringValue(row.value) ||
            stringValue(row.arg) ||
            stringValue(row.command) ||
            "";
        }
        return stringValue(part) || "";
      })
      .filter(Boolean)
      .join(" ");
  }
  if (typeof value === "string") return value;
  const row = recordValue(value);
  if (row) return stringifyCodexToolCommand(row.parsedCmd || row.command || row.cmd || row.args);
  return "";
}

export function readEditableToolConsentCommand(
  request: ToolConsentRequest | null | undefined,
): string {
  if (!request) return "";
  if (typeof request.input.command === "string") {
    return request.input.command;
  }
  return stringifyCodexToolCommand(
    request.input.parsedCmd ||
      request.input.command ||
      request.input.cmd ||
      request.input.commandActions ||
      request.commandActions,
  );
}

export function createUpdatedToolConsentCommandInput(
  request: ToolConsentRequest | null | undefined,
  command: string,
): ToolConsentUpdatedInput | undefined {
  const originalCommand = readEditableToolConsentCommand(request);
  if (!request || !originalCommand || command === originalCommand || command.trim().length === 0) {
    return undefined;
  }
  return { ...request.input, command };
}

export {
  AUTO_TURN_DECISION_PERMISSION_KEYS,
  AUTO_TURN_DECISION_PERMISSION_OPTIONS,
  DEFAULT_AGENT_SUBAGENT_MODE_SETTINGS,
  DEFAULT_AUTO_TURN_DECISION_SETTINGS,
};

export type AgentSubagentBackendSettings = ContractAgentSubagentBackendSettings;
export type ClaudeSubagentModeSettings = ContractClaudeSubagentModeSettings;
export type AgentSubagentModeSettings = ContractAgentSubagentModeSettings;
export type AutoTurnDecisionSettings = ContractAutoTurnDecisionSettings;
export type AutoTurnDecisionPermissionKey = ContractAutoTurnDecisionPermissionKey;
export type AutoTurnDecisionPermissionOption =
  ContractAutoTurnDecisionPermissionOption;

export const normalizeAgentSubagentModeSettings =
  normalizeAgentSubagentModeSettingsImpl as (
    input: Partial<AgentSubagentModeSettings> | null | undefined,
    base?: AgentSubagentModeSettings,
  ) => AgentSubagentModeSettings;

export const normalizeAutoTurnDecisionSettings =
  normalizeAutoTurnDecisionSettingsImpl as (
    input: Partial<AutoTurnDecisionSettings> | null | undefined,
    base?: AutoTurnDecisionSettings,
  ) => AutoTurnDecisionSettings;

export interface CustomSubagentDefinition {
  id: string;
  name: string;
  description: string;
  instruction: string;
  enabled: boolean;
}

export interface CustomSubagentUpsertInput {
  id?: string | null;
  name: string;
  description?: string | null;
  instruction: string;
  enabled?: boolean;
}

export { DEFAULT_AGENT_INTERACTION_SETTINGS };

export type AgentInteractionSettings = ContractAgentInteractionSettings;
export type MainAgentPromptMode = ContractMainAgentPromptMode;

export const normalizeAgentInteractionSettings =
  normalizeAgentInteractionSettingsImpl as (
    input: Partial<AgentInteractionSettings> | null | undefined,
    base?: AgentInteractionSettings,
  ) => AgentInteractionSettings;

import {
  isPlanApprovalAskUserSpec,
  normalizeAskUserSpec,
  type AskUserResult,
  type AskUserSpec,
} from "./ask-user";
import {
  AGENT_INTERACTION_DELETE_SUBAGENT_COMMAND,
  AGENT_INTERACTION_GET_SETTINGS_COMMAND,
  AGENT_INTERACTION_KINDS,
  AGENT_INTERACTION_LIST_SUBAGENTS_COMMAND,
  AGENT_INTERACTION_SET_SETTINGS_COMMAND,
  AGENT_INTERACTION_UPSERT_SUBAGENT_COMMAND,
  AGENT_TIMELINE_PENDING_INTERACTIONS,
  ARCHITECTURE_INTERACTION_KIND,
  ASK_USER_INTERACTION_KINDS,
  ASK_USER_INTERACTION_KIND,
  isAgentInteractionKind as isAgentInteractionKindImpl,
  isAskUserInteractionKind as isAskUserInteractionKindImpl,
  isPendingAutoDecisionKind as isPendingAutoDecisionKindImpl,
  MCP_ELICITATION_INTERACTION_KIND,
  normalizeMcpElicitationResult as normalizeMcpElicitationResultImpl,
  normalizePermissionApprovalResult as normalizePermissionApprovalResultImpl,
  normalizeRuntimeInteractionResult as normalizeRuntimeInteractionResultImpl,
  pendingAutoDecisionLabel as pendingAutoDecisionLabelImpl,
  PENDING_AUTO_DECISION_KINDS,
  PENDING_AUTO_DECISION_LABELS,
  PERMISSION_APPROVAL_INTERACTION_KIND,
  PLAN_APPROVAL_INTERACTION_KIND,
  PLAN_APPROVAL_SPEC_DEFAULTS,
  RUNTIME_INTERACTION_KINDS,
  TOOL_CONSENT_INTERACTION_KIND,
  type AskUserInteractionKind as ContractAskUserInteractionKind,
  type AgentInteractionKind as ContractAgentInteractionKind,
  type PendingAutoDecisionKind as ContractPendingAutoDecisionKind,
  type PlanApprovalSpecDefaults as ContractPlanApprovalSpecDefaults,
  type RuntimeInteractionKind,
} from "./agentInteractionContract.mjs";
import {
  normalizeProjectArchitectureInteractionPayload,
  type ProjectArchitectureInteractionPayload,
  type ProjectArchitectureInteractionResult,
} from "./architecture";
import {
  normalizeChatBackendKind,
  type ChatBackendKind,
  type ToolConsentDecisionKind,
  type ToolConsentResponsePayload,
  type ToolConsentRequest,
  type ToolConsentUpdatedInput,
} from "./chat";

export {
  AGENT_INTERACTION_DELETE_SUBAGENT_COMMAND,
  AGENT_INTERACTION_GET_SETTINGS_COMMAND,
  AGENT_INTERACTION_KINDS,
  AGENT_INTERACTION_LIST_SUBAGENTS_COMMAND,
  AGENT_INTERACTION_SET_SETTINGS_COMMAND,
  AGENT_INTERACTION_UPSERT_SUBAGENT_COMMAND,
  AGENT_TIMELINE_PENDING_INTERACTIONS,
  ARCHITECTURE_INTERACTION_KIND,
  ASK_USER_INTERACTION_KINDS,
  ASK_USER_INTERACTION_KIND,
  MCP_ELICITATION_INTERACTION_KIND,
  PENDING_AUTO_DECISION_KINDS,
  PENDING_AUTO_DECISION_LABELS,
  PERMISSION_APPROVAL_INTERACTION_KIND,
  PLAN_APPROVAL_INTERACTION_KIND,
  PLAN_APPROVAL_SPEC_DEFAULTS,
  RUNTIME_INTERACTION_KINDS,
  TOOL_CONSENT_INTERACTION_KIND,
};

export type PlanApprovalSpecDefaults = ContractPlanApprovalSpecDefaults;
export type AgentInteractionKind = ContractAgentInteractionKind;
export type AskUserInteractionKind = ContractAskUserInteractionKind;
export type AgentTimelinePendingInteraction =
  (typeof AGENT_TIMELINE_PENDING_INTERACTIONS)[number];
export type PendingAutoDecisionKind = ContractPendingAutoDecisionKind;

export const isAgentInteractionKind = isAgentInteractionKindImpl as (
  value: unknown,
) => value is AgentInteractionKind;

export const isAskUserInteractionKind = isAskUserInteractionKindImpl as (
  value: unknown,
) => value is AskUserInteractionKind;

export const isPendingAutoDecisionKind = isPendingAutoDecisionKindImpl as (
  value: unknown,
) => value is PendingAutoDecisionKind;

export const pendingAutoDecisionLabel = pendingAutoDecisionLabelImpl as (
  kind: PendingAutoDecisionKind,
) => string;

export function askUserInteractionKindForSpec(
  spec: Pick<AskUserSpec, "intent">,
): AskUserInteractionKind {
  return isPlanApprovalAskUserSpec(spec)
    ? PLAN_APPROVAL_INTERACTION_KIND
    : ASK_USER_INTERACTION_KIND;
}

export interface ToolConsentInteractionPayload {
  toolName: string;
  input: ToolConsentUpdatedInput;
  title?: string | null;
  displayName?: string | null;
  description?: string | null;
  blockedPath?: string | null;
  decisionReason?: string | null;
  toolUseId?: string | null;
  toolUseID?: string | null;
  backend?: ChatBackendKind;
  additionalPermissions?: unknown;
  availableDecisions?: ToolConsentDecisionKind[];
  proposedExecpolicyAmendment?: unknown;
  proposedNetworkPolicyAmendments?: unknown;
  networkApprovalContext?: unknown;
  cwd?: string | null;
  reason?: string | null;
  commandActions?: unknown;
}

export type McpElicitationAction = "accept" | "decline" | "cancel";

export interface McpElicitationPayload {
  threadId: string;
  turnId: string | null;
  serverName: string;
  mode: "form" | "url";
  message: string;
  requestedSchema?: unknown;
  url?: string;
  elicitationId?: string;
  _meta?: unknown;
}

export interface McpElicitationResult {
  action: McpElicitationAction;
  content?: Record<string, unknown> | null;
  _meta?: unknown;
}

export interface PermissionApprovalPayload {
  reason: string | null;
  requestedAccess: unknown;
  scopeSuggestion?: unknown;
  providerContext?: {
    [provider: string]: unknown;
  };
}

export interface PermissionApprovalResult {
  action: "approve" | "decline" | "cancel";
  grantedAccess?: unknown;
  scope?: unknown;
  strictAutoReview?: boolean;
  providerContext?: unknown;
}

export type AgentInteractionPayloadByKind = {
  [PLAN_APPROVAL_INTERACTION_KIND]: AskUserSpec;
  [ASK_USER_INTERACTION_KIND]: AskUserSpec;
  [ARCHITECTURE_INTERACTION_KIND]: ProjectArchitectureInteractionPayload;
  [TOOL_CONSENT_INTERACTION_KIND]: ToolConsentInteractionPayload;
  [MCP_ELICITATION_INTERACTION_KIND]: McpElicitationPayload;
  [PERMISSION_APPROVAL_INTERACTION_KIND]: PermissionApprovalPayload;
};

export type AgentInteractionResultByKind = {
  [PLAN_APPROVAL_INTERACTION_KIND]: AskUserResult;
  [ASK_USER_INTERACTION_KIND]: AskUserResult;
  [ARCHITECTURE_INTERACTION_KIND]: ProjectArchitectureInteractionResult;
  [TOOL_CONSENT_INTERACTION_KIND]: ToolConsentResponsePayload;
  [MCP_ELICITATION_INTERACTION_KIND]: McpElicitationResult;
  [PERMISSION_APPROVAL_INTERACTION_KIND]: PermissionApprovalResult;
};

export type AgentInteractionRequest = {
  [K in AgentInteractionKind]: AgentInteractionRequestOf<K>;
}[AgentInteractionKind];

export type AgentInteractionResponse = {
  [K in AgentInteractionKind]: AgentInteractionResponseOf<K>;
}[AgentInteractionKind];

export interface AgentInteractionRequestOf<K extends AgentInteractionKind> {
  taskId: string;
  turnId: string;
  backend: ChatBackendKind;
  requestId: string;
  kind: K;
  payload: AgentInteractionPayloadByKind[K];
}

export interface AgentInteractionResponseOf<K extends AgentInteractionKind> {
  taskId: string;
  requestId: string;
  kind: K;
  result: AgentInteractionResultByKind[K];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function stringField(row: Record<string, unknown>, key: string): string {
  const value = row[key];
  return typeof value === "string" ? value : "";
}

function stringOrNull(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function recordOrEmpty(value: unknown): Record<string, unknown> {
  return isRecord(value) ? value : {};
}

function stringListOrUndefined(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value.filter((item): item is string => typeof item === "string");
}

export function normalizeMcpElicitationPayload(value: unknown): McpElicitationPayload | null {
  if (!isRecord(value)) return null;
  const mode = value.mode === "url" ? "url" : value.mode === "form" ? "form" : null;
  const threadId = stringField(value, "threadId");
  const serverName = stringField(value, "serverName");
  const message = stringField(value, "message");
  if (!mode || !threadId || !serverName) return null;
  return {
    threadId,
    turnId: typeof value.turnId === "string" ? value.turnId : null,
    serverName,
    mode,
    message,
    requestedSchema: value.requestedSchema,
    url: typeof value.url === "string" ? value.url : undefined,
    elicitationId: typeof value.elicitationId === "string" ? value.elicitationId : undefined,
    _meta: value._meta,
  };
}

export function normalizePermissionApprovalPayload(value: unknown): PermissionApprovalPayload | null {
  if (!isRecord(value)) return null;
  const providerContext = isRecord(value.providerContext)
    ? value.providerContext as PermissionApprovalPayload["providerContext"]
    : undefined;
  return {
    reason: typeof value.reason === "string" ? value.reason : null,
    requestedAccess: value.requestedAccess ?? {},
    scopeSuggestion: value.scopeSuggestion,
    providerContext,
  };
}

export function createMcpElicitationResult(
  action: McpElicitationAction,
  content?: Record<string, unknown>,
): McpElicitationResult {
  return content ? { action, content } : { action };
}

export const normalizeMcpElicitationResult =
  normalizeMcpElicitationResultImpl as (
    value: unknown,
  ) => McpElicitationResult;

export const normalizePermissionApprovalResult =
  normalizePermissionApprovalResultImpl as (
    value: unknown,
  ) => PermissionApprovalResult;

export const normalizeRuntimeInteractionResult =
  normalizeRuntimeInteractionResultImpl as (
    kind: RuntimeInteractionKind,
    result: unknown,
  ) => McpElicitationResult | PermissionApprovalResult;

export function createPermissionApprovalResult(
  payload: PermissionApprovalPayload,
  decision: "allow" | "deny",
): PermissionApprovalResult {
  const scope = payload.scopeSuggestion ?? "turn";
  if (decision === "allow") {
    return {
      action: "approve",
      grantedAccess: payload.requestedAccess,
      scope,
      providerContext: payload.providerContext,
    };
  }
  return {
    action: "decline",
    grantedAccess: {},
    scope,
    strictAutoReview: true,
    providerContext: payload.providerContext,
  };
}

function normalizeAgentInteractionPayload<K extends AgentInteractionKind>(
  kind: K,
  value: unknown,
): AgentInteractionPayloadByKind[K] | null {
  if (kind === MCP_ELICITATION_INTERACTION_KIND) {
    return normalizeMcpElicitationPayload(value) as AgentInteractionPayloadByKind[K] | null;
  }
  if (kind === PERMISSION_APPROVAL_INTERACTION_KIND) {
    return normalizePermissionApprovalPayload(value) as AgentInteractionPayloadByKind[K] | null;
  }
  if (isAskUserInteractionKind(kind)) {
    return normalizeAskUserSpec(value) as AgentInteractionPayloadByKind[K] | null;
  }
  if (kind === ARCHITECTURE_INTERACTION_KIND) {
    return normalizeProjectArchitectureInteractionPayload(value) as AgentInteractionPayloadByKind[K] | null;
  }
  return isRecord(value) ? value as unknown as AgentInteractionPayloadByKind[K] : null;
}

export function normalizeAgentInteractionRequest(value: unknown): AgentInteractionRequest | null {
  if (!isRecord(value)) return null;
  const taskId = stringField(value, "taskId");
  const turnId = stringField(value, "turnId");
  const requestId = stringField(value, "requestId");
  const kind = stringField(value, "kind");
  if (!taskId || !requestId || !isAgentInteractionKind(kind)) return null;
  const payload = normalizeAgentInteractionPayload(kind, value.payload);
  if (!payload) return null;
  return {
    taskId,
    turnId,
    backend: normalizeChatBackendKind(value.backend),
    requestId,
    kind,
    payload,
  } as AgentInteractionRequest;
}

export function normalizeToolConsentRequestFromInteraction(
  req: AgentInteractionRequest,
): ToolConsentRequest | null {
  if (req.kind !== TOOL_CONSENT_INTERACTION_KIND) return null;
  const payload = recordOrEmpty(req.payload);
  return {
    taskId: req.taskId,
    turnId: req.turnId,
    backend: req.backend,
    requestId: req.requestId,
    toolName: stringOrNull(payload.toolName) || "tool",
    input: recordOrEmpty(payload.input),
    title: stringOrNull(payload.title),
    displayName: stringOrNull(payload.displayName),
    description: stringOrNull(payload.description),
    blockedPath: stringOrNull(payload.blockedPath),
    decisionReason: stringOrNull(payload.decisionReason),
    toolUseId: stringOrNull(payload.toolUseId) || stringOrNull(payload.toolUseID),
    additionalPermissions: payload.additionalPermissions,
    availableDecisions: stringListOrUndefined(payload.availableDecisions),
    proposedExecpolicyAmendment: payload.proposedExecpolicyAmendment,
    proposedNetworkPolicyAmendments: payload.proposedNetworkPolicyAmendments,
    networkApprovalContext: payload.networkApprovalContext,
    cwd: stringOrNull(payload.cwd),
    reason: stringOrNull(payload.reason),
    commandActions: payload.commandActions,
  };
}

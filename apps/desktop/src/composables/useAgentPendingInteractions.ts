import { computed, reactive, type ComputedRef } from "vue";
import {
  MCP_ELICITATION_INTERACTION_KIND,
  normalizeMcpElicitationPayload,
  normalizePermissionApprovalPayload,
  PERMISSION_APPROVAL_INTERACTION_KIND,
  type AgentInteractionRequest,
  type AgentInteractionResponse,
  type McpElicitationPayload,
  type McpElicitationResult,
  type PermissionApprovalPayload,
  type PermissionApprovalResult,
} from "@lilia/contracts";
import { respondAgentInteraction } from "../services/chat";
import {
  productApprovalDecisionFromPermissionResult,
  respondNativeApproval,
} from "../services/nativeAgent";
import {
  clearConversationRequiresAction,
  markConversationRequiresAction,
} from "./useConversationActivity";
import {
  shouldClearPendingInteraction,
  type ClearPendingInteractionsOptions,
} from "./pendingInteractionClearOptions";

interface PendingMcpElicitation {
  kind: typeof MCP_ELICITATION_INTERACTION_KIND;
  taskId: string;
  turnId: string | null;
  requestId: string;
  payload: McpElicitationPayload;
}

interface PendingPermissionApproval {
  kind: typeof PERMISSION_APPROVAL_INTERACTION_KIND;
  taskId: string;
  turnId: string | null;
  requestId: string;
  payload: PermissionApprovalPayload;
}

export type PendingAgentInteraction =
  | PendingMcpElicitation
  | PendingPermissionApproval;

const pending = reactive<Record<string, Record<string, PendingAgentInteraction>>>({});

type TaskIdSource = string | (() => string);

function readTaskId(source: TaskIdSource): string {
  return typeof source === "function" ? source() : source;
}

function taskBucket(taskId: string): Record<string, PendingAgentInteraction> {
  if (!pending[taskId]) pending[taskId] = {};
  return pending[taskId];
}

function clearPending(taskId: string, requestId: string) {
  if (pending[taskId]?.[requestId]) delete pending[taskId][requestId];
  clearConversationRequiresAction(taskId, requestId);
}

export function handleAgentPendingInteractionRequest(req: AgentInteractionRequest): boolean {
  if (req.kind === MCP_ELICITATION_INTERACTION_KIND) {
    const payload = normalizeMcpElicitationPayload(req.payload);
    if (!payload) return false;
    hydrateAgentPendingInteraction({
      kind: req.kind,
      taskId: req.taskId,
      turnId: req.turnId || null,
      requestId: req.requestId,
      payload,
    });
    return true;
  }
  if (req.kind !== PERMISSION_APPROVAL_INTERACTION_KIND) return false;
  const payload = normalizePermissionApprovalPayload(req.payload);
  if (!payload) return false;
  hydrateAgentPendingInteraction({
    kind: req.kind,
    taskId: req.taskId,
    turnId: req.turnId || null,
    requestId: req.requestId,
    payload,
  });
  return true;
}

export function hydrateAgentPendingInteraction(interaction: PendingAgentInteraction) {
  taskBucket(interaction.taskId)[interaction.requestId] = interaction;
  markConversationRequiresAction(interaction.taskId, interaction.requestId);
}

export function clearAgentPendingInteractionsForTask(
  taskId: string,
  options: ClearPendingInteractionsOptions = {},
) {
  const bucket = pending[taskId];
  if (!bucket) return;
  for (const [requestId, interaction] of Object.entries(bucket)) {
    if (!shouldClearPendingInteraction(interaction, taskId, options)) continue;
    delete bucket[requestId];
    clearConversationRequiresAction(taskId, requestId);
  }
  if (Object.keys(bucket).length === 0) delete pending[taskId];
}

export function usePendingAgentInteractionsForTask(
  taskId: TaskIdSource,
): ComputedRef<PendingAgentInteraction[]> {
  return computed(() => {
    const bucket = pending[readTaskId(taskId)];
    return bucket ? Object.values(bucket) : [];
  });
}

export async function respondMcpElicitation(
  taskId: string,
  requestId: string,
  result: McpElicitationResult,
): Promise<void> {
  await respondAgentInteraction({
    taskId,
    requestId,
    kind: MCP_ELICITATION_INTERACTION_KIND,
    result,
  } satisfies AgentInteractionResponse);
  clearPending(taskId, requestId);
}

export async function respondPermissionApproval(
  taskId: string,
  requestId: string,
  result: PermissionApprovalResult,
): Promise<void> {
  const nativeDecision = productApprovalDecisionFromPermissionResult(requestId, result);
  if (nativeDecision) {
    await respondNativeApproval(taskId, nativeDecision);
  } else {
    // Missing providerContext.native: backend refuses Node stdin on Native turns.
    await respondAgentInteraction({
      taskId,
      requestId,
      kind: PERMISSION_APPROVAL_INTERACTION_KIND,
      result,
    } satisfies AgentInteractionResponse);
  }
  clearPending(taskId, requestId);
}

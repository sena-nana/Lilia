import { reactive, computed, type ComputedRef } from "vue";
import {
  respondAgentInteraction,
  type ToolConsentDecision,
  type ToolConsentRequest,
  type ToolConsentUpdatedInput,
} from "../services/chat";
import {
  TOOL_CONSENT_INTERACTION_KIND,
  type AgentInteractionResponse,
} from "@lilia/contracts";
import {
  clearConversationRequiresAction,
  markConversationRequiresAction,
} from "./useConversationActivity";
import {
  shouldClearPendingInteraction,
  type ClearPendingInteractionsOptions,
} from "./pendingInteractionClearOptions";

const pending = reactive<Record<string, ToolConsentRequest>>({});
const localResolvers = new Map<
  string,
  (
    decision: ToolConsentDecision,
    message?: string,
    updatedInput?: ToolConsentUpdatedInput,
  ) => void
>();

type TaskIdSource = string | (() => string);

function readTaskId(source: TaskIdSource): string {
  return typeof source === "function" ? source() : source;
}

function setPendingToolConsent(request: ToolConsentRequest) {
  const previous = pending[request.taskId];
  if (previous && previous.requestId !== request.requestId) {
    localResolvers.delete(previous.requestId);
    clearConversationRequiresAction(request.taskId, previous.requestId);
  }
  pending[request.taskId] = request;
  markConversationRequiresAction(request.taskId, request.requestId);
}

export function useToolConsentForTask(
  taskId: TaskIdSource,
): ComputedRef<ToolConsentRequest | null> {
  return computed(() => pending[readTaskId(taskId)] ?? null);
}

export function usePendingToolConsentsForTask(
  taskId: TaskIdSource,
): ComputedRef<ToolConsentRequest[]> {
  return computed(() => {
    const request = pending[readTaskId(taskId)];
    return request ? [request] : [];
  });
}

export function requestLocalToolConsent(
  request: ToolConsentRequest,
): Promise<{
  decision: ToolConsentDecision;
  message?: string;
  updatedInput?: ToolConsentUpdatedInput;
}> {
  setPendingToolConsent(request);
  return new Promise((resolve) => {
    localResolvers.set(request.requestId, (decision, message, updatedInput) => {
      resolve({ decision, message, updatedInput });
    });
  });
}

export function handleToolConsentRequest(
  request: ToolConsentRequest,
) {
  setPendingToolConsent(request);
  localResolvers.delete(request.requestId);
}

export function hydrateToolConsentRequest(request: ToolConsentRequest) {
  setPendingToolConsent(request);
  localResolvers.delete(request.requestId);
}

export function clearToolConsentForTask(
  taskId: string,
  options: ClearPendingInteractionsOptions = {},
) {
  const request = pending[taskId];
  if (!request) return;
  if (!shouldClearPendingInteraction(request, taskId, options)) return;
  delete pending[taskId];
  localResolvers.delete(request.requestId);
  clearConversationRequiresAction(taskId, request.requestId);
}

export async function respondConsent(
  taskId: string,
  requestId: string,
  decision: ToolConsentDecision,
  message?: string,
  updatedInput?: ToolConsentUpdatedInput,
  decisionKind?: string,
): Promise<void> {
  const localResolve = localResolvers.get(requestId);
  if (localResolve) {
    if (pending[taskId]?.requestId === requestId) {
      delete pending[taskId];
    }
    clearConversationRequiresAction(taskId, requestId);
    localResolvers.delete(requestId);
    localResolve(decision, message, updatedInput);
    return;
  }
  await respondAgentInteraction({
    taskId,
    requestId,
    kind: TOOL_CONSENT_INTERACTION_KIND,
    result: {
      taskId,
      requestId,
      decision,
      message: message ?? null,
      ...(updatedInput ? { updatedInput } : {}),
      ...(decisionKind
        ? { decisionKind, codexDecision: decisionKind }
        : {}),
    },
  } satisfies AgentInteractionResponse);
  if (pending[taskId]?.requestId === requestId) {
    delete pending[taskId];
  }
  clearConversationRequiresAction(taskId, requestId);
}


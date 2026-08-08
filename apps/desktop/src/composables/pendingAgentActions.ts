import {
  agentTimelineActionMatches,
  ARCHITECTURE_INTERACTION_KIND,
  askUserInteractionKindForSpec,
  isAskUserInteractionKind,
  MCP_ELICITATION_INTERACTION_KIND,
  pendingAutoDecisionLabel,
  PERMISSION_APPROVAL_INTERACTION_KIND,
  TITLE_UPDATE_ACTION_KIND,
  titleUpdateTimelinePayload,
  TOOL_CONSENT_INTERACTION_KIND,
  type AgentTimelineEvent,
  type AskUserInteractionKind,
  type AskUserResult,
} from "@lilia/contracts";
import { pendingAskInteractionKey, type PendingAsk } from "./useAskUser";
import type { PendingAgentInteraction } from "./useAgentPendingInteractions";
import type { PendingArchitectureChange } from "./useProjectArchitectureInteractions";
import type {
  ToolConsentDecision,
  ToolConsentRequest,
  ToolConsentUpdatedInput,
} from "../services/chat";

type PendingAskUserAgentAction = {
  [K in AskUserInteractionKind]: {
    kind: K;
    taskId: string | null;
    turnId: string | null;
    requestId: string | null;
    ask: PendingAsk;
  };
}[AskUserInteractionKind];

type PendingAskUserAgentActionResolution = {
  [K in AskUserInteractionKind]: {
    kind: K;
    requestId: string | null;
    askId: number;
    result: AskUserResult;
  };
}[AskUserInteractionKind];

export type PendingAgentAction =
  | {
      kind: typeof TOOL_CONSENT_INTERACTION_KIND;
      taskId: string;
      turnId: string | null;
      requestId: string;
      request: ToolConsentRequest;
    }
  | PendingAskUserAgentAction
  | PendingAgentInteraction
  | PendingArchitectureChange
  | {
      kind: typeof TITLE_UPDATE_ACTION_KIND;
      taskId: string;
      turnId: string | null;
      requestId: string;
      proposedTitle: string;
      previousTitle: string | null;
    };

export type PendingAgentActionResolution =
  | {
      kind: typeof TOOL_CONSENT_INTERACTION_KIND;
      requestId: string;
      decision: ToolConsentDecision;
      message?: string;
      updatedInput?: ToolConsentUpdatedInput;
    }
  | PendingAskUserAgentActionResolution
  | {
      kind: typeof MCP_ELICITATION_INTERACTION_KIND;
      requestId: string;
      action: "accept" | "decline" | "cancel";
      content?: Record<string, unknown>;
    }
  | {
      kind: typeof PERMISSION_APPROVAL_INTERACTION_KIND;
      requestId: string;
      decision: "allow" | "deny";
    }
  | {
      kind: typeof ARCHITECTURE_INTERACTION_KIND;
      requestId: string;
      decision: "allow" | "deny";
    }
  | {
      kind: typeof TITLE_UPDATE_ACTION_KIND;
      requestId: string;
      decision: "accept" | "decline";
    };

export type PendingAgentActionSubmittingTarget = "tool" | "native" | null;

export interface PendingAgentActionResolutionInput {
  architectureDecision?: "allow" | "deny" | null;
  askResult?: AskUserResult | null;
  mcpContent?: Record<string, unknown>;
  mcpDecision?: "accept" | "decline" | "cancel" | null;
  permissionDecision?: "allow" | "deny" | null;
  titleDecision?: "accept" | "decline" | null;
  toolDecision?: ToolConsentDecision | null;
  toolMessage?: string;
  toolUpdatedInput?: ToolConsentUpdatedInput;
}

export interface PendingAgentActionSources {
  asks: readonly PendingAsk[];
  toolConsents: readonly ToolConsentRequest[];
  agentInteractions?: readonly PendingAgentInteraction[];
  timelineEvents?: readonly AgentTimelineEvent[];
  architectureChanges?: readonly PendingArchitectureChange[];
}

export function isPendingAskUserAgentAction(
  action: PendingAgentAction,
): action is PendingAskUserAgentAction {
  return isAskUserInteractionKind(action.kind);
}

export function pendingAskAgentAction(ask: PendingAsk): PendingAskUserAgentAction {
  return {
    kind: askUserInteractionKindForSpec(ask.spec),
    taskId: ask.taskId,
    turnId: ask.turnId,
    requestId: ask.requestId ?? null,
    ask,
  };
}

export function toolConsentAgentAction(request: ToolConsentRequest): PendingAgentAction {
  return {
    kind: TOOL_CONSENT_INTERACTION_KIND,
    taskId: request.taskId,
    turnId: request.turnId,
    requestId: request.requestId,
    request,
  };
}

export function pendingAgentActionsForTask(
  sources: PendingAgentActionSources,
): PendingAgentAction[] {
  return uniquePendingAgentActions([
    ...sources.asks.map(pendingAskAgentAction),
    ...sources.toolConsents.map(toolConsentAgentAction),
    ...(sources.agentInteractions ?? []),
    ...(sources.architectureChanges ?? []),
    ...(sources.timelineEvents ?? []).flatMap(titleUpdateActionForEvent),
  ]);
}

export function pendingAgentActionKey(action: PendingAgentAction): string {
  switch (action.kind) {
    case TOOL_CONSENT_INTERACTION_KIND:
      return `tool:${action.requestId}`;
    case TITLE_UPDATE_ACTION_KIND:
      return `title:${action.requestId}`;
    case MCP_ELICITATION_INTERACTION_KIND:
      return `mcp:${action.requestId}`;
    case PERMISSION_APPROVAL_INTERACTION_KIND:
      return `permission:${action.requestId}`;
    case ARCHITECTURE_INTERACTION_KIND:
      return `architecture:${action.requestId}`;
    default:
      if (action.requestId) return `${action.kind}:${action.requestId}`;
      return pendingAskInteractionKey(action.ask);
  }
}

export function uniquePendingAgentActions(
  actions: readonly PendingAgentAction[],
): PendingAgentAction[] {
  const byKey = new Map<string, PendingAgentAction>();
  for (const action of actions) {
    byKey.set(pendingAgentActionKey(action), action);
  }
  return [...byKey.values()];
}

export function pendingAgentActionResolution(
  action: PendingAgentAction,
  input: PendingAgentActionResolutionInput,
): PendingAgentActionResolution | null {
  if (isPendingAskUserAgentAction(action)) {
    if (!input.askResult) return null;
    return {
      kind: action.kind,
      requestId: action.requestId,
      askId: action.ask.id,
      result: input.askResult,
    };
  }
  switch (action.kind) {
    case TOOL_CONSENT_INTERACTION_KIND:
      if (!input.toolDecision) return null;
      return {
        kind: TOOL_CONSENT_INTERACTION_KIND,
        requestId: action.requestId,
        decision: input.toolDecision,
        message: input.toolMessage,
        ...(input.toolUpdatedInput ? { updatedInput: input.toolUpdatedInput } : {}),
      };
    case TITLE_UPDATE_ACTION_KIND:
      if (!input.titleDecision) return null;
      return {
        kind: TITLE_UPDATE_ACTION_KIND,
        requestId: action.requestId,
        decision: input.titleDecision,
      };
    case MCP_ELICITATION_INTERACTION_KIND:
      if (!input.mcpDecision) return null;
      return {
        kind: MCP_ELICITATION_INTERACTION_KIND,
        requestId: action.requestId,
        action: input.mcpDecision,
        ...(input.mcpDecision === "accept" && action.payload.mode === "form"
          ? { content: input.mcpContent ?? {} }
          : {}),
      };
    case PERMISSION_APPROVAL_INTERACTION_KIND:
      if (!input.permissionDecision) return null;
      return {
        kind: PERMISSION_APPROVAL_INTERACTION_KIND,
        requestId: action.requestId,
        decision: input.permissionDecision,
      };
    case ARCHITECTURE_INTERACTION_KIND:
      if (!input.architectureDecision) return null;
      return {
        kind: ARCHITECTURE_INTERACTION_KIND,
        requestId: action.requestId,
        decision: input.architectureDecision,
      };
    default:
      return null;
  }
}

export function pendingAgentActionResolutionSubmittingTarget(
  resolution: PendingAgentActionResolution,
): PendingAgentActionSubmittingTarget {
  switch (resolution.kind) {
    case TOOL_CONSENT_INTERACTION_KIND:
      return "tool";
    case MCP_ELICITATION_INTERACTION_KIND:
    case PERMISSION_APPROVAL_INTERACTION_KIND:
    case ARCHITECTURE_INTERACTION_KIND:
      return "native";
    default:
      return null;
  }
}

export interface PendingAgentActionAutoDecisionState {
  askHasRecommendedResult?: boolean;
  askQuestionId?: string | null;
  editingToolCommand?: boolean;
  mcpCanSubmit?: boolean;
  submitting?: boolean;
  toolCommandIsEmpty?: boolean;
  toolDanger?: boolean;
  toolSubmitting?: boolean;
}

export interface PendingAgentActionAutoResolutionInput
  extends PendingAgentActionAutoDecisionState {
  askResult?: AskUserResult | null;
  mcpContent?: Record<string, unknown>;
  toolUpdatedInput?: ToolConsentUpdatedInput;
}

function pendingAskAutoDecisionKey(
  action: PendingAskUserAgentAction,
  state: Pick<PendingAgentActionAutoDecisionState, "askHasRecommendedResult" | "askQuestionId">,
): string {
  if (!state.askHasRecommendedResult) return "";
  return `${pendingAgentActionKey(action)}:${state.askQuestionId ?? ""}`;
}

function pendingAskAutoDecisionLabel(ask: Pick<PendingAsk, "spec">): string {
  return pendingAutoDecisionLabel(askUserInteractionKindForSpec(ask.spec));
}

function toolConsentAutoDecisionKey(
  request: Pick<ToolConsentRequest, "requestId">,
  state: Pick<
    PendingAgentActionAutoDecisionState,
    "editingToolCommand" | "toolCommandIsEmpty" | "toolDanger" | "toolSubmitting"
  >,
): string {
  return !state.toolDanger &&
      !state.toolSubmitting &&
      !state.editingToolCommand &&
      !state.toolCommandIsEmpty
    ? `tool:${request.requestId}`
    : "";
}

function toolConsentAutoDecisionLabel(): string {
  return pendingAutoDecisionLabel(TOOL_CONSENT_INTERACTION_KIND);
}

export function pendingAgentActionAutoDecisionKey(
  action: PendingAgentAction,
  state: PendingAgentActionAutoDecisionState,
): string {
  if (isPendingAskUserAgentAction(action)) {
    return pendingAskAutoDecisionKey(action, state);
  }
  if (action.kind === TOOL_CONSENT_INTERACTION_KIND) {
    return toolConsentAutoDecisionKey(action, state);
  }
  if (action.kind === TITLE_UPDATE_ACTION_KIND) return pendingAgentActionKey(action);
  if (action.kind === MCP_ELICITATION_INTERACTION_KIND) {
    return action.payload.mode === "form" && state.mcpCanSubmit && !state.submitting
      ? pendingAgentActionKey(action)
      : "";
  }
  if (
    action.kind === ARCHITECTURE_INTERACTION_KIND ||
    action.kind === PERMISSION_APPROVAL_INTERACTION_KIND
  ) {
    return state.submitting ? "" : pendingAgentActionKey(action);
  }
  return "";
}

export function pendingAgentActionAutoDecisionLabel(action: PendingAgentAction): string {
  if (isPendingAskUserAgentAction(action)) return pendingAskAutoDecisionLabel(action.ask);
  if (action.kind === TOOL_CONSENT_INTERACTION_KIND) return toolConsentAutoDecisionLabel();
  return pendingAutoDecisionLabel(action.kind);
}

export function pendingAgentActionAutoResolution(
  action: PendingAgentAction,
  input: PendingAgentActionAutoResolutionInput,
): PendingAgentActionResolution | null {
  if (!pendingAgentActionAutoDecisionKey(action, input)) return null;
  if (isPendingAskUserAgentAction(action)) {
    return pendingAgentActionResolution(action, { askResult: input.askResult });
  }
  switch (action.kind) {
    case TOOL_CONSENT_INTERACTION_KIND:
      return pendingAgentActionResolution(action, {
        toolDecision: "allow",
        toolUpdatedInput: input.toolUpdatedInput,
      });
    case TITLE_UPDATE_ACTION_KIND:
      return pendingAgentActionResolution(action, { titleDecision: "accept" });
    case MCP_ELICITATION_INTERACTION_KIND:
      return pendingAgentActionResolution(action, {
        mcpDecision: "accept",
        mcpContent: input.mcpContent,
      });
    case ARCHITECTURE_INTERACTION_KIND:
      return pendingAgentActionResolution(action, { architectureDecision: "allow" });
    case PERMISSION_APPROVAL_INTERACTION_KIND:
      return pendingAgentActionResolution(action, { permissionDecision: "allow" });
    default:
      return null;
  }
}

export interface PendingAgentActionTraits {
  blocksComposer: boolean;
  visibleInTimelineWithoutInterrupt: boolean;
}

export interface PendingAgentActionBuckets {
  blocking: PendingAgentAction[];
  visible: PendingAgentAction[];
}

export function pendingAgentActionTraits(action: PendingAgentAction): PendingAgentActionTraits {
  switch (action.kind) {
    case TITLE_UPDATE_ACTION_KIND:
      return {
        blocksComposer: false,
        visibleInTimelineWithoutInterrupt: true,
      };
    case MCP_ELICITATION_INTERACTION_KIND:
    case PERMISSION_APPROVAL_INTERACTION_KIND:
    case ARCHITECTURE_INTERACTION_KIND:
      return {
        blocksComposer: true,
        visibleInTimelineWithoutInterrupt: true,
      };
    default:
      return {
        blocksComposer: true,
        visibleInTimelineWithoutInterrupt: false,
      };
  }
}

export function pendingAgentActionBuckets(
  actions: readonly PendingAgentAction[],
  options: { nonInterruptMode: boolean },
): PendingAgentActionBuckets {
  const visible: PendingAgentAction[] = [];
  const blocking: PendingAgentAction[] = [];
  for (const action of actions) {
    const traits = pendingAgentActionTraits(action);
    if (options.nonInterruptMode || traits.visibleInTimelineWithoutInterrupt) {
      visible.push(action);
    }
    if (traits.blocksComposer) {
      blocking.push(action);
    }
  }
  return { blocking, visible };
}

export function titleUpdateActionForEvent(event: AgentTimelineEvent): PendingAgentAction[] {
  const payload = titleUpdateTimelinePayload(event);
  if (!payload) return [];
  return [{
    kind: TITLE_UPDATE_ACTION_KIND,
    taskId: event.taskId,
    turnId: event.turnId,
    requestId: payload.requestId,
    proposedTitle: payload.proposedTitle,
    previousTitle: payload.previousTitle,
  }];
}

export function pendingActionForTimelineEvent(
  event: AgentTimelineEvent,
  actions: readonly PendingAgentAction[],
): PendingAgentAction | null {
  for (const action of actions) {
    if (agentTimelineActionMatches(event, action)) return action;
  }
  return null;
}


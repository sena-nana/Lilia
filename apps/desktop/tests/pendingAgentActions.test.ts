import { describe, expect, it } from "vitest";
import {
  ARCHITECTURE_INTERACTION_KIND,
  MCP_ELICITATION_INTERACTION_KIND,
  PLAN_APPROVAL_INTERACTION_KIND,
  TITLE_UPDATE_ACTION_KIND,
  TOOL_CONSENT_INTERACTION_KIND,
  agentTimelineEventRequiresAction,
} from "@lilia/contracts";
import {
  isPendingAskUserAgentAction,
  pendingAskAgentAction,
  pendingAgentActionBuckets,
  pendingAgentActionAutoDecisionLabel,
  pendingAgentActionAutoDecisionKey,
  pendingAgentActionAutoResolution,
  pendingAgentActionsForTask,
  pendingAgentActionKey,
  pendingAgentActionResolution,
  pendingAgentActionResolutionSubmittingTarget,
  pendingAgentActionTraits,
  pendingActionForTimelineEvent,
  toolConsentAgentAction,
  uniquePendingAgentActions,
  type PendingAgentAction,
} from "../src/composables/pendingAgentActions";
import { usePendingAgentActionsForTask } from "../src/composables/usePendingAgentActions";
import { computed, ref } from "vue";
import { toolConsentRequestFixture as toolConsentRequest } from "./interactionTestHelpers";
import { pendingPlanTimelineEvent as timelineEvent } from "./timelineTestHelpers";

function askFixture(overrides: Record<string, unknown> = {}) {
  return {
    id: 11,
    taskId: "task-1",
    turnId: "turn-1",
    requestId: "ask-1",
    spec: {
      title: "确认",
      questions: [{ id: "q-1", question: "继续？", mode: "confirm" as const }],
    },
    resolve: () => {},
    ...overrides,
  };
}

function createAskAction(): PendingAgentAction {
  const ask = askFixture();
  return {
    kind: "ask_user",
    taskId: ask.taskId,
    turnId: ask.turnId,
    requestId: ask.requestId,
    ask,
  };
}

function createToolAction(): PendingAgentAction {
  const request = toolConsentRequest({ requestId: "consent-1" });
  return {
    kind: TOOL_CONSENT_INTERACTION_KIND,
    taskId: request.taskId,
    turnId: request.turnId,
    requestId: request.requestId,
    request,
  };
}

function createMcpAction(): PendingAgentAction {
  return {
    kind: MCP_ELICITATION_INTERACTION_KIND,
    taskId: "task-1",
    turnId: "turn-1",
    requestId: "mcp-1",
    payload: {
      threadId: "thread-1",
      turnId: "turn-1",
      serverName: "linear",
      mode: "form",
      message: "选择项目",
    },
  };
}

function createTitleAction(): PendingAgentAction {
  return {
    kind: TITLE_UPDATE_ACTION_KIND,
    taskId: "task-1",
    turnId: "turn-1",
    requestId: "title-1",
    proposedTitle: "新标题",
    previousTitle: "旧标题",
  };
}

function createPermissionAction(): PendingAgentAction {
  return {
    kind: "permission_approval",
    taskId: "task-1",
    turnId: "turn-1",
    requestId: "permission-1",
    payload: {
      threadId: "thread-1",
      turnId: "turn-1",
      itemId: "item-1",
      startedAtMs: 1,
      cwd: "C:/repo",
      reason: "need network",
      permissions: { network: { domains: [{ domain: "example.com" }] } },
    },
  };
}

function createArchitectureAction(): PendingAgentAction {
  return {
    kind: ARCHITECTURE_INTERACTION_KIND,
    taskId: "task-1",
    turnId: "turn-1",
    requestId: "architecture-1",
    id: "architecture-1",
    title: "更新架构",
    summary: "改架构文档",
    draft: "draft",
    response: { status: "pending" },
  };
}

function createPendingActionFixtures() {
  return {
    askAction: createAskAction(),
    toolAction: createToolAction(),
    mcpAction: createMcpAction(),
    titleAction: createTitleAction(),
    permissionAction: createPermissionAction(),
    architectureAction: createArchitectureAction(),
  };
}

describe("pending agent actions", () => {
  it("combines pending sources into the unified action model", () => {
    const ask = askFixture();
    const planAsk = askFixture({
      id: 12,
      turnId: "turn-2",
      requestId: "plan-1",
      spec: {
        title: "确认计划",
        intent: "plan_approval" as const,
        questions: [{ id: "approve-plan", question: "", mode: "confirm" as const }],
      },
    });
    const toolConsent = toolConsentRequest({ requestId: "tool-1" });
    const mcpAction = createMcpAction();
    const architectureAction = createArchitectureAction();
    const titleEvent = timelineEvent({
      id: "title-update:task-1:title-1",
      kind: TITLE_UPDATE_ACTION_KIND,
      status: "requires_action",
      payload: {
        requestId: "title-1",
        proposedTitle: "新标题",
        previousTitle: "旧标题",
      },
    });

    const actions = pendingAgentActionsForTask({
      asks: [ask, planAsk],
      toolConsents: [toolConsent],
      agentInteractions: [mcpAction],
      architectureChanges: [architectureAction],
      timelineEvents: [titleEvent],
    });

    expect(actions.map((action) => action.kind)).toEqual([
      "ask_user",
      PLAN_APPROVAL_INTERACTION_KIND,
      TOOL_CONSENT_INTERACTION_KIND,
      MCP_ELICITATION_INTERACTION_KIND,
      ARCHITECTURE_INTERACTION_KIND,
      TITLE_UPDATE_ACTION_KIND,
    ]);
    expect(actions[0]).toMatchObject({ requestId: "ask-1", ask });
    expect(actions[1]).toMatchObject({ requestId: "plan-1", ask: planAsk });
    expect(actions[2]).toMatchObject({ requestId: "tool-1", request: toolConsent });
    expect(actions[5]).toMatchObject({
      requestId: "title-1",
      proposedTitle: "新标题",
      previousTitle: "旧标题",
    });
  });

  it("only attaches plan approval controls to actionable plan events", () => {
    const ask = askFixture({
      id: 1,
      spec: {
        title: "确认 Codex 计划",
        intent: "plan_approval",
        questions: [{ id: "approve-plan", question: "", mode: "confirm" }],
      },
    });
    const action: PendingAgentAction = {
      kind: "plan_approval",
      taskId: ask.taskId,
      turnId: ask.turnId,
      requestId: ask.requestId,
      ask,
    };

    expect(pendingActionForTimelineEvent(
      timelineEvent({ status: "requires_action" }),
      [action],
    )).toBe(action);
    expect(pendingActionForTimelineEvent(
      timelineEvent({
        status: "requires_action",
        payload: { requestId: "other-plan", approved: null },
      }),
      [action],
    )).toBeNull();
    expect(pendingActionForTimelineEvent(
      timelineEvent({
        status: "requires_action",
        payload: { requestId: "ask-1", approved: null },
      }),
      [action],
    )).toBe(action);
    expect(pendingActionForTimelineEvent(
      timelineEvent({ status: "success", payload: { backend: "codex", approved: true } }),
      [action],
    )).toBeNull();
    expect(pendingActionForTimelineEvent(
      timelineEvent({ status: "cancelled", payload: { backend: "codex", approved: false } }),
      [action],
    )).toBeNull();
  });

  it("does not mark completed tool consent timeline events as pending", () => {
    const action = createToolAction();
    const completed = timelineEvent({
      kind: "command",
      status: "success",
      payload: { interaction: "tool_consent", requestId: "consent-1" },
    });

    expect(agentTimelineEventRequiresAction(completed)).toBe(false);
    expect(pendingActionForTimelineEvent(completed, [action])).toBeNull();
  });

  it("classifies visible and blocking pending actions", () => {
    const askAction = createAskAction();
    const titleAction = createTitleAction();
    const toolAction = createToolAction();
    const mcpAction = createMcpAction();

    expect(isPendingAskUserAgentAction(askAction)).toBe(true);
    expect(isPendingAskUserAgentAction(toolAction)).toBe(false);
    for (const [action, key, label] of [
      [askAction, "ask_user:ask-1", "选择推荐项"],
      [titleAction, "title:title-1", "同意标题"],
      [toolAction, "tool:consent-1", "同意工具调用"],
      [mcpAction, "mcp:mcp-1", "提交 MCP 表单"],
    ] as const) {
      expect(pendingAgentActionKey(action)).toBe(key);
      expect(pendingAgentActionAutoDecisionLabel(action)).toBe(label);
    }
    for (const [action, traits] of [
      [titleAction, { blocksComposer: false, visibleInTimelineWithoutInterrupt: true }],
      [mcpAction, { blocksComposer: true, visibleInTimelineWithoutInterrupt: true }],
      [toolAction, { blocksComposer: true, visibleInTimelineWithoutInterrupt: false }],
    ] as const) {
      expect(pendingAgentActionTraits(action)).toEqual(traits);
    }
    expect(pendingAgentActionBuckets([askAction, titleAction, toolAction, mcpAction], {
      nonInterruptMode: false,
    })).toEqual({
      visible: [titleAction, mcpAction],
      blocking: [askAction, toolAction, mcpAction],
    });
    expect(pendingAgentActionBuckets([askAction, titleAction, toolAction, mcpAction], {
      nonInterruptMode: true,
    })).toEqual({
      visible: [askAction, titleAction, toolAction, mcpAction],
      blocking: [askAction, toolAction, mcpAction],
    });
  });

  it("derives free-mode auto-decision keys from eligible pending actions", () => {
    const { askAction, toolAction, mcpAction, titleAction, permissionAction, architectureAction } =
      createPendingActionFixtures();

    for (const [action, input, key] of [
      [askAction, { askHasRecommendedResult: true, askQuestionId: "q-1" }, "ask_user:ask-1:q-1"],
      [askAction, { askHasRecommendedResult: false, askQuestionId: "q-1" }, ""],
      [toolAction, { toolCommandIsEmpty: false }, "tool:consent-1"],
      [toolAction, { editingToolCommand: true, toolCommandIsEmpty: false }, ""],
      [titleAction, {}, "title:title-1"],
      [mcpAction, { mcpCanSubmit: true }, "mcp:mcp-1"],
      [mcpAction, { mcpCanSubmit: false }, ""],
      [permissionAction, {}, "permission:permission-1"],
      [permissionAction, { submitting: true }, ""],
      [architectureAction, {}, "architecture:architecture-1"],
    ] as const) {
      expect(pendingAgentActionAutoDecisionKey(action, input)).toBe(key);
    }
    expect(pendingAskAgentAction(askAction.ask)).toMatchObject({
      kind: "ask_user",
      requestId: "ask-1",
      ask: askAction.ask,
    });
    expect(pendingAgentActionAutoDecisionKey({
      ...askAction,
      requestId: null,
      ask: { ...askAction.ask, requestId: null },
    }, {
      askHasRecommendedResult: true,
      askQuestionId: "q-1",
    })).toBe("ask:11:q-1");
    expect(toolConsentAgentAction(toolAction.request)).toMatchObject({
      kind: "tool_consent",
      requestId: "consent-1",
      request: toolAction.request,
    });
  });

  it("builds typed resolutions from pending actions", () => {
    const { askAction, toolAction, mcpAction, titleAction, permissionAction, architectureAction } =
      createPendingActionFixtures();

    expect(pendingAgentActionResolution(askAction, {
      askResult: { answers: {}, cancelled: false },
    })).toMatchObject({
      kind: "ask_user",
      requestId: "ask-1",
      askId: 11,
      result: { cancelled: false },
    });
    expect(pendingAgentActionResolution(toolAction, {
      toolDecision: "deny",
      toolMessage: "no",
    })).toMatchObject({
      kind: "tool_consent",
      requestId: "consent-1",
      decision: "deny",
      message: "no",
    });
    expect(pendingAgentActionResolution(mcpAction, {
      mcpDecision: "accept",
      mcpContent: { project: "lilia" },
    })).toMatchObject({
      kind: "mcp_elicitation",
      requestId: "mcp-1",
      action: "accept",
      content: { project: "lilia" },
    });
    expect(pendingAgentActionResolution(titleAction, {
      titleDecision: "accept",
    })).toEqual({
      kind: "title_update",
      requestId: "title-1",
      decision: "accept",
    });
    expect(pendingAgentActionResolution(permissionAction, {
      permissionDecision: "allow",
    })).toEqual({
      kind: "permission_approval",
      requestId: "permission-1",
      decision: "allow",
    });
    expect(pendingAgentActionResolution(architectureAction, {
      architectureDecision: "deny",
    })).toEqual({
      kind: "architecture_change",
      requestId: "architecture-1",
      decision: "deny",
    });
    expect(pendingAgentActionResolution(toolAction, {})).toBeNull();
  });

  it("classifies which resolutions should enter a submitting state", () => {
    for (const [resolution, target] of [
      [{ kind: TOOL_CONSENT_INTERACTION_KIND, requestId: "tool-1", decision: "allow" }, "tool"],
      [{ kind: MCP_ELICITATION_INTERACTION_KIND, requestId: "mcp-1", action: "accept", content: {} }, "native"],
      [{ kind: ARCHITECTURE_INTERACTION_KIND, requestId: "architecture-1", decision: "allow" }, "native"],
      [{ kind: TITLE_UPDATE_ACTION_KIND, requestId: "title-1", decision: "accept" }, null],
    ] as const) {
      expect(pendingAgentActionResolutionSubmittingTarget(resolution)).toBe(target);
    }
  });

  it("builds auto resolutions only when the action is eligible", () => {
    const { askAction, toolAction, mcpAction, titleAction, permissionAction, architectureAction } =
      createPendingActionFixtures();

    expect(pendingAgentActionAutoResolution(askAction, {
      askHasRecommendedResult: true,
      askQuestionId: "q-1",
      askResult: { answers: {}, cancelled: false },
    })).toMatchObject({
      kind: "ask_user",
      requestId: "ask-1",
      askId: 11,
    });
    expect(pendingAgentActionAutoResolution(askAction, {
      askHasRecommendedResult: false,
      askResult: { answers: {}, cancelled: false },
    })).toBeNull();
    expect(pendingAgentActionAutoResolution(toolAction, {
      toolCommandIsEmpty: false,
      toolUpdatedInput: { command: "pwd" },
    })).toMatchObject({
      kind: "tool_consent",
      requestId: "consent-1",
      decision: "allow",
      updatedInput: { command: "pwd" },
    });
    expect(pendingAgentActionAutoResolution(toolAction, {
      toolCommandIsEmpty: false,
      toolDanger: true,
    })).toBeNull();
    expect(pendingAgentActionAutoResolution(titleAction, {})).toEqual({
      kind: "title_update",
      requestId: "title-1",
      decision: "accept",
    });
    expect(pendingAgentActionAutoResolution(mcpAction, {
      mcpCanSubmit: true,
      mcpContent: { project: "lilia" },
    })).toEqual({
      kind: "mcp_elicitation",
      requestId: "mcp-1",
      action: "accept",
      content: { project: "lilia" },
    });
    expect(pendingAgentActionAutoResolution(permissionAction, {})).toEqual({
      kind: "permission_approval",
      requestId: "permission-1",
      decision: "allow",
    });
    expect(pendingAgentActionAutoResolution(architectureAction, {
      submitting: true,
    })).toBeNull();
  });

  it("derives nonblocking title update actions from actionable timeline events", () => {
    const event = timelineEvent({
      id: "title-update:task-1:req-1",
      kind: "title_update",
      status: "requires_action",
      title: "标题已更新",
      summary: "标题事件化",
      payload: {
        requestId: "req-1",
        proposedTitle: "标题事件化",
        previousTitle: "旧标题",
      },
    });
    const actions = usePendingAgentActionsForTask(
      computed(() => []),
      computed(() => []),
      computed(() => []),
      ref([event]),
    );

    expect(actions.value).toHaveLength(1);
    expect(actions.value[0]).toMatchObject({
      kind: "title_update",
      requestId: "req-1",
      proposedTitle: "标题事件化",
      previousTitle: "旧标题",
    });
    expect(agentTimelineEventRequiresAction(event)).toBe(true);
    expect(pendingActionForTimelineEvent(event, actions.value)).toBe(actions.value[0]);
  });

  it("deduplicates request-keyed pending actions and keeps the latest title payload", () => {
    const firstTitle = timelineEvent({
      id: "title-update:task-1:req-1:first",
      kind: TITLE_UPDATE_ACTION_KIND,
      status: "requires_action",
      payload: {
        requestId: "req-1",
        proposedTitle: "旧建议",
        previousTitle: "原标题",
      },
    });
    const latestTitle = timelineEvent({
      id: "title-update:task-1:req-1:latest",
      kind: TITLE_UPDATE_ACTION_KIND,
      status: "requires_action",
      payload: {
        requestId: "req-1",
        proposedTitle: "新建议",
        previousTitle: "原标题",
      },
    });

    const actions = pendingAgentActionsForTask({
      asks: [],
      toolConsents: [],
      timelineEvents: [firstTitle, latestTitle],
    });

    expect(actions).toHaveLength(1);
    expect(actions[0]).toMatchObject({
      kind: TITLE_UPDATE_ACTION_KIND,
      requestId: "req-1",
      proposedTitle: "新建议",
    });
    expect(uniquePendingAgentActions([actions[0]!, actions[0]!])).toHaveLength(1);
  });

  it("deduplicates ask actions by request id when a backend request is rehydrated", () => {
    const firstAsk = askFixture();
    const latestAsk = askFixture({
      id: 12,
      spec: {
        title: "确认更新",
        questions: [{ id: "q-1", question: "继续执行？", mode: "confirm" as const }],
      },
    });

    const actions = pendingAgentActionsForTask({
      asks: [firstAsk, latestAsk],
      toolConsents: [],
    });

    expect(actions).toHaveLength(1);
    expect(actions[0]).toMatchObject({
      kind: "ask_user",
      requestId: "ask-1",
      ask: latestAsk,
    });
  });

  it("attaches MCP and permission actions to matching timeline events", () => {
    const actions = usePendingAgentActionsForTask(
      computed(() => []),
      computed(() => []),
      ref([createMcpAction(), createPermissionAction()]),
      ref([]),
    );
    const mcpEvent = timelineEvent({
      kind: "mcp",
      status: "requires_action",
      payload: { interaction: "mcp_elicitation", requestId: "mcp-1" },
    });
    const permissionEvent = timelineEvent({
      kind: "diagnostic",
      status: "requires_action",
      payload: { interaction: "permission_approval", requestId: "permission-1" },
    });

    expect(actions.value).toHaveLength(2);
    expect(agentTimelineEventRequiresAction(mcpEvent)).toBe(true);
    expect(agentTimelineEventRequiresAction(permissionEvent)).toBe(true);
    expect(pendingActionForTimelineEvent(mcpEvent, actions.value)).toBe(actions.value[0]);
    expect(pendingActionForTimelineEvent(permissionEvent, actions.value)).toBe(actions.value[1]);
  });
});


import { describe, expect, it, vi } from "vitest";
import {
  CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME,
  CHAT_RESPOND_AGENT_INTERACTION_COMMAND,
  PROJECT_ARCHITECTURE_APPLY_COMMAND,
} from "@lilia/contracts";
import {
  emitTauriEvent,
  failNextMockListen,
  mockInvoke,
  mockListenerCount,
} from "./tauriMock";
import {
  getAgentInteractionSettings,
  onAgentInteractionRequest,
  respondAgentInteraction,
  setAgentInteractionSettings,
  type AgentInteractionRequest,
} from "../src/services/chat";
import { installAgentInteractionBridge } from "../src/composables/useAgentInteractionBridge";
import {
  respondConsent,
  useToolConsentForTask,
} from "../src/composables/useToolConsentBridge";
import { usePendingProjectArchitectureChangesForTask } from "../src/composables/useProjectArchitectureInteractions";

describe("chat AskUser bridge service", () => {
  it("bridge listener registration failure does not poison future installs", async () => {
    failNextMockListen(CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME, "interaction listener failed");

    await expect(installAgentInteractionBridge()).rejects.toThrow("interaction listener failed");
    expect(mockListenerCount(CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME)).toBe(0);

    const unlisten = await installAgentInteractionBridge();
    try {
      expect(mockListenerCount(CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME)).toBe(1);
    } finally {
      unlisten();
    }
    expect(mockListenerCount(CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME)).toBe(0);
  });

  it("订阅统一 Agent interaction 并把响应写回 runner", async () => {
    const handler = vi.fn<(event: AgentInteractionRequest) => void>();
    await onAgentInteractionRequest(handler);

    emitTauriEvent(CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME, {
      taskId: "task-1",
      turnId: "turn-1",
      backend: "native-agentkit",
      requestId: "ask-1",
      kind: "plan_approval",
      payload: {
        title: "确认 Codex 计划",
        intent: "plan_approval",
        questions: [
          {
            id: "approve-plan",
            question: "",
            mode: "confirm",
          },
        ],
      },
    });

    expect(handler).toHaveBeenCalledWith(
      expect.objectContaining({
        backend: "native-agentkit",
        kind: "plan_approval",
        payload: expect.objectContaining({ title: "确认 Codex 计划" }),
      }),
    );

    await respondAgentInteraction({
      taskId: "task-1",
      requestId: "ask-1",
      kind: "plan_approval",
      result: {
        cancelled: false,
        answers: {
          "approve-plan": { questionId: "approve-plan", value: "yes" },
        },
      },
    });

    expect(mockInvoke).toHaveBeenCalledWith(CHAT_RESPOND_AGENT_INTERACTION_COMMAND, {
      taskId: "task-1",
      requestId: "ask-1",
      kind: "plan_approval",
      result: {
        cancelled: false,
        answers: {
          "approve-plan": { questionId: "approve-plan", value: "yes" },
        },
      },
    }, undefined);
  });

  it("统一 Agent interaction bridge 把工具授权事件转入 pending 并写回 runner", async () => {
    const unlisten = await installAgentInteractionBridge();
    try {
      emitTauriEvent(CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME, {
        taskId: "task-1",
        turnId: "turn-1",
        backend: "native-agentkit",
        requestId: "tool-1",
        kind: "tool_consent",
        payload: {
          toolName: "commandExecution",
          input: { command: "yarn test" },
          title: "Run command",
          displayName: "命令执行",
          toolUseID: "codex-tool-1",
          availableDecisions: ["accept", "decline", 42],
          cwd: "D:/PROJECT/workspace/Lilia",
          reason: "run tests",
          commandActions: [{ text: "yarn test" }],
        },
      });

      expect(useToolConsentForTask("task-1").value).toMatchObject({
        taskId: "task-1",
        backend: "native-agentkit",
        requestId: "tool-1",
        toolName: "commandExecution",
      });

      await respondConsent(
        "task-1",
        "tool-1",
        "deny",
        "先不执行",
        { command: "echo skipped" },
        "decline",
      );

      expect(useToolConsentForTask("task-1").value).toBeNull();
      expect(mockInvoke).toHaveBeenCalledWith(CHAT_RESPOND_AGENT_INTERACTION_COMMAND, {
        taskId: "task-1",
        requestId: "tool-1",
        kind: "tool_consent",
        result: {
          taskId: "task-1",
          requestId: "tool-1",
          decision: "deny",
          message: "先不执行",
          decisionKind: "decline",
          codexDecision: "decline",
          updatedInput: { command: "echo skipped" },
        },
      }, undefined);
    } finally {
      unlisten();
    }
  });

  it("架构图自动应用请求不进入 pending，并把应用结果写回 runner", async () => {
    const unlisten = await installAgentInteractionBridge();
    try {
      emitTauriEvent(CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME, {
        taskId: "task-1",
        turnId: "turn-1",
        backend: "claude",
        requestId: "architecture-1",
        kind: "architecture_change",
        payload: {
          projectId: "lilia",
          taskId: "task-1",
          turnId: "turn-1",
          backend: "claude",
          permission: "full",
          status: "applied",
          reason: "计划内补全架构图",
          changes: [{
            type: "upsert_node",
            node: {
              id: "desktop",
              label: "Desktop",
              type: "module",
              summary: "",
              paths: ["apps/desktop"],
              tags: [],
            },
          }],
          requiresConfirmation: false,
        },
      });

      await vi.waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith(
          PROJECT_ARCHITECTURE_APPLY_COMMAND,
          expect.objectContaining({
            input: expect.objectContaining({
              projectId: "lilia",
              requestId: "architecture-1",
              permission: "full",
            }),
          }),
          undefined,
        );
        expect(mockInvoke).toHaveBeenCalledWith(
          CHAT_RESPOND_AGENT_INTERACTION_COMMAND,
          expect.objectContaining({
            taskId: "task-1",
            requestId: "architecture-1",
            kind: "architecture_change",
            result: expect.objectContaining({
              decision: "allow",
              message: "架构图已更新",
            }),
          }),
          undefined,
        );
      });
      expect(usePendingProjectArchitectureChangesForTask("task-1").value).toEqual([]);
    } finally {
      unlisten();
    }
  });

  it.skip("Agent 交互设置默认关闭，并能保存非打断模式", async () => {
    await expect(getAgentInteractionSettings()).resolves.toMatchObject({
      nonInterruptMode: false,
      debug: false,
      permissionMode: "ask",
      codexProfile: {
        profile: "default",
        model: null,
        reasoningEffort: null,
        runtimeWorkspaceRoots: [],
      },
    });

    await setAgentInteractionSettings({
      nonInterruptMode: true,
      permissionMode: "free",
    });

    await expect(getAgentInteractionSettings()).resolves.toMatchObject({
      nonInterruptMode: true,
      debug: false,
      permissionMode: "free",
      codexProfile: {
        profile: "default",
        model: null,
        reasoningEffort: null,
        runtimeWorkspaceRoots: [],
      },
    });
  });
});


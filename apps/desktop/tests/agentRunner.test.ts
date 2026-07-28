import { mkdir, rm, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { EventEmitter } from "node:events";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { PassThrough } from "node:stream";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  PROJECT_ARCHITECTURE_APPLIED_INTERACTION_STATUS,
} from "@lilia/contracts/architectureContract.mjs";
import { ASK_USER_SINGLE_SELECT_MODE } from "@lilia/contracts/askUserContract.mjs";
import {
  LILIA_BACKGROUND_TERMINALS_CLEAN_WORKFLOW_TYPE,
  LILIA_BATCH_APPLY_WORKFLOW_TYPE,
  LILIA_COMPACT_WORKFLOW_TYPE,
  LILIA_CONFIG_DIAGNOSTICS_WORKFLOW_TYPE,
  LILIA_FIX_SUGGESTION_WORKFLOW_TYPE,
  LILIA_GOAL_WORKFLOW_TYPE,
  LILIA_REVIEW_WORKFLOW_TYPE,
  REMOTE_ENVIRONMENT_COMMAND_TYPE,
  RUNTIME_SETTINGS_COMMAND_TYPE,
  SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
  SESSION_FORK_COMMAND_TYPE,
  SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE,
} from "@lilia/contracts";
import {
  RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
  RUNNER_INTERRUPT_TURN_CONTROL_TYPE,
  RUNNER_SETTINGS_UPDATE_CONTROL_TYPE,
} from "@lilia/contracts/runnerProtocolContract.mjs";
import { runAgentTurn } from "../legacy/agent-runner/core.mjs";
import { createInteractionBroker } from "../legacy/agent-runner/interactions.mjs";
import { createProtocolEmitter } from "../legacy/agent-runner/protocol.mjs";
import { createArchitectureChangeHandler } from "../legacy/agent-runner/architecture.mjs";
import { normalizeRuntimePermission } from "../legacy/agent-runner/runtimeSettings.mjs";
import {
  applyClaudeRuntimePermission,
  mapClaudeInitialPermission,
} from "../legacy/agent-runner/claude/permissions.mjs";
import {
  createLiliaAskUserServer,
  runClaude,
} from "../legacy/agent-runner/claude/runClaude.mjs";
import {
  createConversationContextHandler,
} from "../legacy/agent-runner/conversationContext.mjs";
import {
  codexPermissionProfileIdForMode,
  codexLegacyPermissionRuntimeParams,
  codexPermissionRuntimeParams,
  mapCodexApprovalPolicy,
  mapCodexSandboxMode,
  maybeHandleCodexApprovalRequest,
} from "../legacy/agent-runner/codex/permissions.mjs";
import {
  codexHistoryTimelineEvents,
  createCodexRunContext,
  mapCodexEventToNdjson,
  normalizeCodexAppServerEvent,
  normalizeCodexPlanSteps,
} from "../legacy/agent-runner/codex/timeline.mjs";
import {
  runCodex,
  runCodexAppServer,
  maybeHandleCodexServerRequest,
  startCodexAppServerThread,
  syncCodexThreadHistory,
  updateCodexThreadSettings,
  applyCodexRuntimeSettings,
  flushCodexRuntimeSettings,
  handleLiliaIabResult,
  startCodexAppServerTurn,
  withCodexElicitation,
} from "../legacy/agent-runner/codex/runCodex.mjs";
import {
  codexAppServerSpawnCommand,
  codexAppServerBinary,
  createCodexAppServer,
  resolveWindowsCommandScript,
} from "../legacy/agent-runner/codex/appServer.mjs";
import {
  consumeCodexRateLimitResetCredit,
  readCodexAccountQuotaStatus,
} from "../legacy/agent-runner/codex/accountQuota.mjs";
import {
  runCodexSparkPromptCommand,
  updateCodexSparkPromptState,
} from "../legacy/agent-runner/codex/sparkPrompt.mjs";
import {
  archiveCodexThread,
  cleanCodexThreadBackgroundTerminals,
  codexHistoryTimelineInputs,
  previewCodexThread,
  previewCodexThreadLite,
  readCodexThreadTurns,
  renameCodexThread,
  searchCodexThreads,
  syncCodexThreadHistoryForTask,
} from "../legacy/agent-runner/codex/history.mjs";
import {
  searchClaudeSessions,
  syncClaudeSessionHistoryForTask,
} from "../legacy/agent-runner/claude/history.mjs";
import {
  runClaudeSessionManagementRuntimeCommand,
} from "../legacy/agent-runner/sessionManagement.mjs";

const testsDir = dirname(fileURLToPath(import.meta.url));
const codexAccountQuotaUtility = join(testsDir, "..", "legacy", "codex-account-quota.mjs");

function captureProtocol() {
  const lines: string[] = [];
  const protocol = createProtocolEmitter({ write: (line) => lines.push(line.trimEnd()) });
  return {
    protocol,
    lines,
    json: () => lines.map((line) => JSON.parse(line)),
  };
}

async function waitUntil(predicate: () => boolean, timeoutMs = 1000) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  throw new Error("timed out waiting for condition");
}

function trackedElicitation(calls: any[]) {
  return async (kind: string, fn: () => Promise<unknown>) => {
    calls.push(["increment", kind]);
    try {
      return await fn();
    } finally {
      calls.push(["decrement", kind]);
    }
  };
}

function runnerTurn(prompt = "", extra: Record<string, unknown> = {}) {
  return { prompt, ...extra };
}

function approvePlanAnswers(value = "yes", notes?: string) {
  return {
    "approve-plan": {
      questionId: "approve-plan",
      value,
      ...(notes ? { notes } : {}),
    },
  };
}

function approvedPlanResponse(value = "yes", notes?: string) {
  return {
    cancelled: false,
    answers: approvePlanAnswers(value, notes),
  };
}

function approvedPlanInteractions(value = "yes", notes?: string) {
  return {
    requestAskUser: async () => approvedPlanResponse(value, notes),
  };
}

async function cancelledAskUser() {
  return { cancelled: true, answers: {} };
}

function createTestInteractionBroker(protocol: any) {
  return createInteractionBroker({
    protocol,
    emitToolConsentTimeline: () => {},
    emitAskUserTimeline: () => {},
  });
}

function codexAppServerStub(
  request: (method: string, params: any) => Promise<unknown>,
  overrides: Record<string, unknown> = {},
) {
  return {
    request,
    notify: () => {},
    respond: () => {},
    drainNotifications: () => [],
    close: () => {},
    ...overrides,
  };
}

function createCodexTurnTestServer(
  calls: any[],
  turn: Record<string, unknown> = { status: "completed" },
) {
  return codexAppServerStub(
    async (method: string, params: any) => {
      calls.push({ method, params });
      if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
      if (method === "turn/start") return { turn: { id: "fix-turn-1" } };
      return {};
    },
    {
      drainNotifications: () => [{
        method: "turn/completed",
        params: { threadId: "thread-1", turn },
      }],
    },
  );
}

function codexAppServerTestContext(
  protocol: any,
  server: any,
  overrides: Record<string, unknown> = {},
) {
  return {
    protocol,
    interactions: { requestAskUser: cancelledAskUser },
    emitToolConsentTimeline: () => {},
    createCodexAppServer: () => server,
    env: {},
    cwd: () => "C:/repo",
    ...overrides,
  };
}

async function runCodexAppServerTestTurn(
  protocol: any,
  server: any,
  cmd: Record<string, unknown>,
  overrides: Record<string, unknown> = {},
) {
  await runCodexAppServer(cmd, { mcpServers: [], warnings: [] }, codexAppServerTestContext(protocol, server, overrides));
}

describe("runner core", () => {
  it("缺失或空 prompt 时输出 error 且不进入 backend", async () => {
    const { protocol, json } = captureProtocol();
    const result = await runAgentTurn({}, {
      protocol,
      env: {},
      runClaude: async () => {
        throw new Error("should not run");
      },
    });

    expect(result).toEqual({ ok: false, exitCode: 1 });
    expect(json()).toEqual([{ type: "error", message: "missing prompt" }]);
  });

  it("未知 workflow kind 被拒绝", async () => {
    const { protocol, json } = captureProtocol();
    const result = await runAgentTurn({
      backend: "codex",
      turn: runnerTurn(""),
      workflow: {
        type: "lilia_unknown",
      },
    }, {
      protocol,
      env: {},
      runCodex: async () => {
        throw new Error("should not run");
      },
      runClaude: async () => {
        throw new Error("wrong backend");
      },
    });

    expect(result).toEqual({ ok: false, exitCode: 1 });
    expect(json()).toEqual([{ type: "error", message: "unknown workflow: lilia_unknown" }]);
  });

  it("Codex review workflow 允许空 prompt 进入 Codex 后端", async () => {
    const { protocol, json } = captureProtocol();
    const result = await runAgentTurn({
      backend: "codex",
      turn: runnerTurn(""),
      workflow: {
        type: LILIA_REVIEW_WORKFLOW_TYPE,
        target: { type: "uncommittedChanges" },
      },
    }, {
      protocol,
      env: {},
      runCodex: async (cmd: any) => {
        protocol.emit({ type: "done", sessionId: "thread-review", workflow: cmd.workflow });
      },
      runClaude: async () => {
        throw new Error("wrong backend");
      },
    });

    expect(result).toEqual({ ok: true, exitCode: 0 });
    expect(json()[0]).toMatchObject({
      type: "done",
      sessionId: "thread-review",
      workflow: {
        type: LILIA_REVIEW_WORKFLOW_TYPE,
        target: { type: "uncommittedChanges" },
      },
    });
  });

  it("Codex fix suggestion workflow 允许空 prompt 进入 Codex 后端", async () => {
    const { protocol, json } = captureProtocol();
    const result = await runAgentTurn({
      backend: "codex",
      turn: runnerTurn(""),
      workflow: {
        type: LILIA_FIX_SUGGESTION_WORKFLOW_TYPE,
        target: { type: "uncommittedChanges" },
        mode: "suggest",
      },
    }, {
      protocol,
      env: {},
      runCodex: async (cmd: any) => {
        protocol.emit({ type: "done", sessionId: "thread-fix", workflow: cmd.workflow });
      },
      runClaude: async () => {
        throw new Error("wrong backend");
      },
    });

    expect(result).toEqual({ ok: true, exitCode: 0 });
    expect(json()[0]).toMatchObject({
      type: "done",
      sessionId: "thread-fix",
      workflow: {
        type: LILIA_FIX_SUGGESTION_WORKFLOW_TYPE,
        target: { type: "uncommittedChanges" },
        mode: "suggest",
      },
    });
  });

  it("Codex batch apply workflow 允许空 prompt 进入 Codex 后端", async () => {
    const { protocol, json } = captureProtocol();
    const result = await runAgentTurn({
      backend: "codex",
      turn: runnerTurn(""),
      workflow: {
        type: LILIA_BATCH_APPLY_WORKFLOW_TYPE,
        sourceTurnId: "turn-source",
        sourceKind: "fix_suggestion",
        sourceSummary: "建议修复权限边界",
      },
    }, {
      protocol,
      env: {},
      runCodex: async (cmd: any) => {
        protocol.emit({ type: "done", sessionId: "thread-batch", workflow: cmd.workflow });
      },
      runClaude: async () => {
        throw new Error("wrong backend");
      },
    });

    expect(result).toEqual({ ok: true, exitCode: 0 });
    expect(json()[0]).toMatchObject({
      type: "done",
      sessionId: "thread-batch",
      workflow: {
        type: LILIA_BATCH_APPLY_WORKFLOW_TYPE,
        sourceTurnId: "turn-source",
        sourceKind: "fix_suggestion",
      },
    });
  });

  it("Codex goal workflow 允许空 prompt 进入 Codex 后端", async () => {
    const { protocol, json } = captureProtocol();
    const result = await runAgentTurn({
      backend: "codex",
      turn: runnerTurn(""),
      workflow: {
        type: LILIA_GOAL_WORKFLOW_TYPE,
        action: "set",
        objective: "完成 Thread Goal 接入",
      },
    }, {
      protocol,
      env: {},
      runCodex: async (cmd: any) => {
        protocol.emit({ type: "done", sessionId: "thread-goal", workflow: cmd.workflow });
      },
      runClaude: async () => {
        throw new Error("wrong backend");
      },
    });

    expect(result).toEqual({ ok: true, exitCode: 0 });
    expect(json()[0]).toMatchObject({
      type: "done",
      sessionId: "thread-goal",
      workflow: {
        type: LILIA_GOAL_WORKFLOW_TYPE,
        action: "set",
        objective: "完成 Thread Goal 接入",
      },
    });
  });

  it("Codex compact workflow 允许空 prompt 进入 Codex 后端", async () => {
    const { protocol, json } = captureProtocol();
    const result = await runAgentTurn({
      backend: "codex",
      turn: runnerTurn(""),
      workflow: { type: LILIA_COMPACT_WORKFLOW_TYPE },
    }, {
      protocol,
      env: {},
      runCodex: async (cmd: any) => {
        protocol.emit({ type: "done", sessionId: "thread-compact", workflow: cmd.workflow });
      },
      runClaude: async () => {
        throw new Error("wrong backend");
      },
    });

    expect(result).toEqual({ ok: true, exitCode: 0 });
    expect(json()[0]).toMatchObject({
      type: "done",
      sessionId: "thread-compact",
      workflow: { type: LILIA_COMPACT_WORKFLOW_TYPE },
    });
  });

  it("Codex background terminals clean workflow 允许空 prompt 进入 Codex 后端", async () => {
    const { protocol, json } = captureProtocol();
    const result = await runAgentTurn({
      backend: "codex",
      turn: runnerTurn(""),
      workflow: { type: LILIA_BACKGROUND_TERMINALS_CLEAN_WORKFLOW_TYPE },
    }, {
      protocol,
      env: {},
      runCodex: async (cmd: any) => {
        protocol.emit({ type: "done", sessionId: "thread-clean", workflow: cmd.workflow });
      },
      runClaude: async () => {
        throw new Error("wrong backend");
      },
    });

    expect(result).toEqual({ ok: true, exitCode: 0 });
    expect(json()[0]).toMatchObject({
      type: "done",
      sessionId: "thread-clean",
      workflow: { type: LILIA_BACKGROUND_TERMINALS_CLEAN_WORKFLOW_TYPE },
    });
  });

  it("Codex native runtime command 允许空 prompt 进入 Codex 后端", async () => {
    for (const runtimeCommand of [
      { type: SESSION_FORK_COMMAND_TYPE },
      { type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE, action: "list" },
      { type: RUNTIME_SETTINGS_COMMAND_TYPE, action: "diagnose" },
      { type: SANDBOX_DIAGNOSTICS_COMMAND_TYPE, includeDetails: true },
    ]) {
      const { protocol, json } = captureProtocol();
      const result = await runAgentTurn({
        backend: "codex",
        turn: runnerTurn(""),
        runtimeCommand,
      }, {
        protocol,
        env: {},
        runCodex: async (cmd: any) => {
          protocol.emit({ type: "done", sessionId: "thread-native", runtimeCommand: cmd.runtimeCommand });
        },
        runClaude: async () => {
          throw new Error("wrong backend");
        },
      });

      expect(result).toEqual({ ok: true, exitCode: 0 });
      expect(json()[0]).toMatchObject({
        type: "done",
        sessionId: "thread-native",
        runtimeCommand,
      });
    }
  });

  it("model selection session fork metadata enables runner auto session fork", async () => {
    const { protocol } = captureProtocol();
    let seenCmd: any = null;
    const result = await runAgentTurn({
      backend: "codex",
      turn: runnerTurn("继续实现", {
        resumeSessionId: "thread-source",
      }),
      runtimeOptions: {
        common: {
          modelSelection: {
            source: "auto",
            sessionFork: true,
          },
        },
      },
    }, {
      protocol,
      env: {},
      runCodex: async (cmd: any) => {
        seenCmd = cmd;
        protocol.emit({ type: "done", sessionId: "thread-fork" });
      },
      runClaude: async () => {
        throw new Error("wrong backend");
      },
    });

    expect(result).toEqual({ ok: true, exitCode: 0 });
    expect(seenCmd).toMatchObject({
      prompt: "继续实现",
      resumeSessionId: "thread-source",
      autoSessionFork: true,
    });
  });

  it("Claude session fork runtime command 允许空 prompt 进入 Claude 后端", async () => {
    const { protocol, json } = captureProtocol();
    const result = await runAgentTurn({
      backend: "claude",
      turn: runnerTurn(""),
      runtimeCommand: { type: SESSION_FORK_COMMAND_TYPE },
      }, {
      protocol,
      env: {},
      runClaude: async (cmd: any) => {
        protocol.emit({ type: "done", sessionId: "claude-fork", runtimeCommand: cmd.runtimeCommand });
      },
      runCodex: async () => {
        throw new Error("wrong backend");
      },
    });

    expect(result).toEqual({ ok: true, exitCode: 0 });
    expect(json()[0]).toMatchObject({
      type: "done",
      sessionId: "claude-fork",
      runtimeCommand: { type: SESSION_FORK_COMMAND_TYPE },
    });
  });

  it("按 backend 路由，并把附件路径注入 prompt", async () => {
    const { protocol } = captureProtocol();
    let seen: any = null;
    const result = await runAgentTurn({
      backend: "codex",
      turn: runnerTurn("请读附件", {
        attachments: [{
          path: "C:/tmp/a.txt",
          name: "a.txt",
          kind: "file",
          mime: "text/plain",
          size: 2048,
        }, {
          path: "C:/tmp/src",
          name: "src",
          kind: "directory",
          size: null,
          directory: { fileCount: 3, directoryCount: 1, totalSize: 100, truncated: true },
        }],
      }),
    }, {
      protocol,
      env: {},
      runCodex: async (cmd: any) => {
        seen = cmd;
      },
      runClaude: async () => {
        throw new Error("wrong backend");
      },
    });

    expect(result.ok).toBe(true);
    expect(seen.prompt).toContain("C:/tmp/a.txt");
    expect(seen.prompt).toContain("file, text/plain, 2 KB");
    expect(seen.prompt).toContain("directory, unknown size, 3 files, 1 dirs, truncated");
  });

  it("dry-run 分支不启动真实后端", async () => {
    const { protocol, json } = captureProtocol();
    const result = await runAgentTurn({ backend: "claude", turn: runnerTurn("hi") }, {
      protocol,
      env: { LILIA_AGENT_DRY_RUN: "1" },
      runDryRun: async (cmd: any, context: any) => {
        context.protocol.emit({ type: "done", sessionId: `dry-${cmd.backend}` });
      },
      runClaude: async () => {
        throw new Error("should not run");
      },
    });

    expect(result.ok).toBe(true);
    expect(json()).toEqual([{ type: "done", sessionId: "dry-claude" }]);
  });
});

describe("protocol emitter", () => {
  it("timeline payload 只保留事实字段并能安全 JSON 化", () => {
    const { protocol, json } = captureProtocol();
    const circular: any = { ok: true, taskId: "drop", nested: { turn_id: "drop" } };
    circular.self = circular;
    protocol.emitTimeline({
      kind: "tool",
      status: "running",
      title: "Tool",
      summary: "summary",
      payload: {
        circular,
        value: 1n,
        error: new Error("boom"),
      },
      sourceId: "s1",
    });

    const event = json()[0].event;
    expect(event).toMatchObject({
      kind: "tool",
      status: "running",
      title: "Tool",
      summary: "summary",
      sourceId: "s1",
    });
    expect(event.payload.circular.taskId).toBeUndefined();
    expect(event.payload.circular.nested.turn_id).toBeUndefined();
    expect(event.payload.circular.self).toBe("[Circular]");
    expect(event.payload.value).toBe("1");
    expect(event.payload.error.message).toBe("boom");
  });
});

describe("interaction broker", () => {
  it("关联 consent/ask-user 请求与 stdin 响应", async () => {
    const { protocol, json } = captureProtocol();
    const timelineCalls: any[] = [];
    const broker = createInteractionBroker({
      protocol,
      emitToolConsentTimeline: (...args: any[]) => timelineCalls.push(["consent", ...args]),
      emitAskUserTimeline: (...args: any[]) => timelineCalls.push(["ask", ...args]),
    });

    const consent = broker.requestUserConsent({ toolName: "Bash", input: { command: "pwd" } });
    expect(json()[0]).toMatchObject({
      type: "interaction_request",
      id: "consent-1",
      kind: "tool_consent",
      payload: {
        toolName: "Bash",
      },
    });
    broker.handleControlLine(JSON.stringify({
      type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
      id: "consent-1",
      kind: "tool_consent",
      result: {
      decision: "allow",
      message: "ok",
      updatedInput: { command: "ls" },
      },
    }));
    expect(await consent).toMatchObject({
      id: "consent-1",
      decision: "allow",
      updatedInput: { command: "ls" },
    });

    const ask = broker.requestAskUser({
      title: "Confirm",
      questions: [{ id: "q1", question: "Go?", options: [] }],
    });
    broker.handleControlLine("not json");
    broker.handleControlLine(JSON.stringify({
      type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
      id: "missing",
    }));
    broker.handleControlLine(JSON.stringify({
      type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
      id: "ask-1",
      kind: "ask_user",
      result: { answers: { q1: { value: "yes" } } },
    }));
    expect(await ask).toMatchObject({
      cancelled: false,
      answers: { q1: { value: "yes" } },
    });
    expect(timelineCalls.map((call) => call[0])).toEqual(["consent", "ask", "ask"]);
  });

  it("计划确认通过统一 interaction_request 发出", async () => {
    const { protocol, json } = captureProtocol();
    const broker = createInteractionBroker({
      protocol,
      emitToolConsentTimeline: () => {},
      emitAskUserTimeline: () => {},
    });

    const ask = broker.requestAskUser({
      intent: "plan_approval",
      title: "确认 Codex 计划",
      questions: [{ id: "approve-plan", question: "", mode: "confirm" }],
    }, { backend: "codex", emitTimelineEvent: false });

    expect(json()[0]).toMatchObject({
      type: "interaction_request",
      id: "ask-1",
      kind: "plan_approval",
      backend: "codex",
      payload: {
        title: "确认 Codex 计划",
      },
    });

    broker.handleControlLine(JSON.stringify({
      type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
      id: "ask-1",
      kind: "plan_approval",
      result: approvedPlanResponse(),
    }));

    await expect(ask).resolves.toMatchObject({ cancelled: false });
  });

  it("settings_update control lines are dispatched without disturbing pending interactions", async () => {
    const { protocol } = captureProtocol();
    const updates: any[] = [];
    const broker = createInteractionBroker({
      protocol,
      emitToolConsentTimeline: () => {},
      emitAskUserTimeline: () => {},
    });
    broker.handleSettingsUpdate((msg: any) => updates.push(msg));
    const consent = broker.requestUserConsent({ toolName: "Bash", input: { command: "pwd" } });

    broker.handleControlLine(JSON.stringify({
      type: RUNNER_SETTINGS_UPDATE_CONTROL_TYPE,
      permission: "readonly",
    }));
    expect(updates).toEqual([{
      type: RUNNER_SETTINGS_UPDATE_CONTROL_TYPE,
      permission: "readonly",
    }]);
    expect(broker.pendingCounts().consent).toBe(1);

    broker.handleControlLine(JSON.stringify({
      type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
      id: "consent-1",
      kind: "tool_consent",
      result: { decision: "allow" },
    }));
    await expect(consent).resolves.toMatchObject({ decision: "allow" });
  });

  it("interrupt_turn control lines are dispatched to registered handlers", () => {
    const { protocol } = captureProtocol();
    const calls: string[] = [];
    const broker = createInteractionBroker({
      protocol,
      emitToolConsentTimeline: () => {},
      emitAskUserTimeline: () => {},
    });
    const unregister = broker.handleInterruptTurn(() => calls.push("interrupt"));

    broker.handleControlLine(JSON.stringify({ type: RUNNER_INTERRUPT_TURN_CONTROL_TYPE }));
    unregister();
    broker.handleControlLine(JSON.stringify({ type: RUNNER_INTERRUPT_TURN_CONTROL_TYPE }));

    expect(calls).toEqual(["interrupt"]);
  });

  it("MCP elicitation broker preserves the Claude backend through request/response", async () => {
    const { protocol, json } = captureProtocol();
    const broker = createTestInteractionBroker(protocol);

    const result = broker.requestMcpElicitation({
      threadId: "claude-session-1",
      turnId: null,
      serverName: "linear",
      mode: "url",
      message: "完成授权",
      url: "https://linear.app/oauth",
      elicitationId: "elicit-url-1",
    }, { backend: "claude" });

    expect(json()).toContainEqual(expect.objectContaining({
      type: "interaction_request",
      id: "codex-1",
      kind: "mcp_elicitation",
      backend: "claude",
      payload: expect.objectContaining({
        threadId: "claude-session-1",
        serverName: "linear",
        mode: "url",
      }),
    }));

    broker.handleControlLine(JSON.stringify({
      type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
      id: "codex-1",
      kind: "mcp_elicitation",
      result: { action: "decline" },
    }));

    await expect(result).resolves.toEqual({
      action: "decline",
      content: null,
      _meta: null,
    });
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "mcp",
        status: "cancelled",
        title: "Claude MCP elicitation",
        payload: expect.objectContaining({
          backend: "claude",
          interaction: "mcp_elicitation",
          requestId: "codex-1",
          result: expect.objectContaining({ action: "decline" }),
        }),
      }),
    }));
  });
});

describe("Claude helpers", () => {
  function emptyClaudeQuery() {
    return (async function* () {})();
  }

  function emptyClaudeQueryProbe(onQuery: () => void) {
    return () => {
      onQuery();
      return emptyClaudeQuery();
    };
  }

  function claudeRunnerContext(protocol: any, overrides: Record<string, unknown> = {}) {
    return {
      protocol,
      platform: "win32",
      interactions: {
        requestAskUser: cancelledAskUser,
        handleSettingsUpdate: () => {},
      },
      emitToolConsentTimeline: () => {},
      ...overrides,
    } as any;
  }

  function claudeRunnerContextWithTools(protocol: any, overrides: Record<string, unknown> = {}) {
    return claudeRunnerContext(protocol, {
      createSdkMcpServer: (config: any) => config,
      createClaudeTool: (name: string) => ({ name }),
      ...overrides,
    });
  }

  function claudeMcpElicitationContext(protocol: any, broker: any, createClaudeQuery: any) {
    return claudeRunnerContextWithTools(protocol, {
      interactions: broker,
      createClaudeQuery,
    });
  }

  it("子对话才会注册 Claude 查询对话上下文工具", () => {
    const createdTools: any[] = [];
    const createTool = (...args: any[]) => {
      createdTools.push(args);
      return { name: args[0] };
    };
    const createServer = (config: any) => config;

    const normal = createLiliaAskUserServer({
      createServer,
      createTool,
      requestAskUser: cancelledAskUser,
    });
    expect(normal.tools.map((tool: any) => tool.name)).toEqual([
      "ask_user_question",
      "query_quota_usage",
    ]);

    const child = createLiliaAskUserServer({
      createServer,
      createTool,
      requestAskUser: cancelledAskUser,
      conversationContext: {
        currentTaskId: "child-1",
        parentTaskId: "parent-1",
        tasks: [],
      },
    });

    expect(child.tools.map((tool: any) => tool.name)).toEqual([
      "ask_user_question",
      "query_quota_usage",
      "query_conversation_context",
    ]);
    expect(createdTools.some((args) => args[0] === "query_conversation_context")).toBe(true);
  });

  it("查询对话上下文工具默认返回父对话消息摘要", async () => {
    const handler = createConversationContextHandler({
      currentTaskId: "child-1",
      parentTaskId: "parent-1",
      tasks: [{
        taskId: "parent-1",
        projectId: "lilia",
        title: "父对话",
        status: "done",
        parentId: null,
        createdAt: 1000,
        messages: [
          { role: "user", content: "父对话里的问题", createdAt: 1001 },
          { role: "assistant", content: "父对话里的回答", createdAt: 1002 },
        ],
      }],
    });

    await expect(handler({})).resolves.toMatchObject({
      ok: true,
      currentTaskId: "child-1",
      parentTaskId: "parent-1",
      requestedTaskId: "parent-1",
      task: {
        taskId: "parent-1",
        title: "父对话",
        messages: [
          { role: "user", content: "父对话里的问题" },
          { role: "assistant", content: "父对话里的回答" },
        ],
      },
    });
  });

  it("Claude turn enables SDK promptSuggestions and emits native suggestion frames", async () => {
    const { protocol, json } = captureProtocol();
    let seenOptions: any = null;

    await runClaude({
      cwd: "C:/repo",
      prompt: "继续实现",
      model: "claude-sonnet-4-6",
      permission: "ask",
    }, claudeRunnerContextWithTools(protocol, {
      createClaudeQuery: ({ options }: any) => {
        seenOptions = options;
        return (async function* () {
          yield {
            type: "result",
            is_error: false,
            subtype: "success",
            session_id: "claude-session-1",
            uuid: "result-1",
            usage: {
              input_tokens: 1200,
              cache_creation_input_tokens: 300,
              cache_read_input_tokens: 500,
              output_tokens: 50,
            },
          };
          yield {
            type: "prompt_suggestion",
            suggestion: "请继续检查 Claude 原生建议展示。",
            session_id: "claude-session-1",
            uuid: "suggestion-1",
          };
        })();
      },
    }));

    expect(seenOptions).toMatchObject({ promptSuggestions: true });
    expect(seenOptions.toolAliases.QueryQuotaUsage).toBe("mcp__lilia__query_quota_usage");
    expect(json()).toContainEqual({
      type: "prompt_suggestion",
      suggestion: "请继续检查 Claude 原生建议展示。",
      uuid: "suggestion-1",
    });
    expect(json()).toContainEqual({
      type: "context_usage",
      usedTokens: 2000,
      source: "claude",
    });
  });

  it("Claude turn appends provider additionalContext to system prompt", async () => {
    const { protocol } = captureProtocol();
    let seenOptions: any = null;

    await runClaude({
      cwd: "C:/repo",
      prompt: "继续实现",
      model: "claude-sonnet-4-6",
      permission: "ask",
      runtimeOptions: {
        provider: {
          claude: {
            additionalContext: "[Lilia Memory Baseline]\nUser constraints:\n- PR: no emoji",
          },
        },
      },
    }, claudeRunnerContextWithTools(protocol, {
      createClaudeQuery: ({ options }: any) => {
        seenOptions = options;
        return (async function* () {
          yield {
            type: "result",
            is_error: false,
            subtype: "success",
            session_id: "claude-session-1",
          };
        })();
      },
    }));

    expect(seenOptions.systemPrompt).toMatchObject({
      type: "preset",
      preset: "claude_code",
    });
    expect(seenOptions.systemPrompt.append).toContain(
      "[Lilia Memory Baseline]\nUser constraints:\n- PR: no emoji",
    );
    expect(seenOptions).not.toHaveProperty("additionalContext");
  });

  it("Claude onElicitation routes form accept through Lilia MCP interaction", async () => {
    const { protocol, json } = captureProtocol();
    const broker = createTestInteractionBroker(protocol);
    let elicitationResult: any = null;

    const run = runClaude({
      cwd: "C:/repo",
      prompt: "需要 MCP 表单",
      model: "claude-sonnet-4-6",
      permission: "ask",
    }, claudeMcpElicitationContext(protocol, broker, ({ options }: any) => (async function* () {
      yield {
        type: "system",
        subtype: "status",
        session_id: "claude-session-1",
        uuid: "status-1",
      };
      elicitationResult = await options.onElicitation({
        serverName: "linear",
        mode: "form",
        message: "选择项目",
        requestedSchema: {
          type: "object",
          properties: { project: { type: "string", enum: ["A", "B"] } },
          required: ["project"],
        },
      }, { signal: new AbortController().signal });
      yield {
        type: "result",
        is_error: false,
        subtype: "success",
        session_id: "claude-session-1",
        uuid: "result-1",
      };
    })()));

    await waitUntil(() => json().some((line) => line.type === "interaction_request"));
    expect(json()).toContainEqual(expect.objectContaining({
      type: "interaction_request",
      id: "codex-1",
      kind: "mcp_elicitation",
      backend: "claude",
      payload: expect.objectContaining({
        threadId: "claude-session-1",
        serverName: "linear",
        mode: "form",
        message: "选择项目",
      }),
    }));

    broker.handleControlLine(JSON.stringify({
      type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
      id: "codex-1",
      kind: "mcp_elicitation",
      result: {
        action: "accept",
        content: { project: "B" },
      },
    }));

    await run;
    expect(elicitationResult).toEqual({
      action: "accept",
      content: { project: "B" },
    });
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "mcp",
        status: "success",
        title: "Claude MCP elicitation",
        payload: expect.objectContaining({
          backend: "claude",
          interaction: "mcp_elicitation",
          result: expect.objectContaining({ action: "accept" }),
        }),
      }),
    }));
  });

  it("Claude onElicitation routes URL decline and cancel through Lilia MCP interaction", async () => {
    for (const action of ["decline", "cancel"] as const) {
      const { protocol, json } = captureProtocol();
      const broker = createTestInteractionBroker(protocol);
      let elicitationResult: any = null;

      const run = runClaude({
        cwd: "C:/repo",
        prompt: "需要 MCP URL",
        model: "claude-sonnet-4-6",
        permission: "ask",
      }, claudeMcpElicitationContext(protocol, broker, ({ options }: any) => (async function* () {
        yield {
          type: "system",
          subtype: "status",
          session_id: `claude-session-${action}`,
          uuid: `status-${action}`,
        };
        elicitationResult = await options.onElicitation({
          serverName: "github",
          mode: "url",
          message: "打开授权链接",
          url: "https://github.com/login/oauth",
          elicitationId: `elicit-${action}`,
        }, { signal: new AbortController().signal });
        yield {
          type: "result",
          is_error: false,
          subtype: "success",
          session_id: `claude-session-${action}`,
          uuid: `result-${action}`,
        };
      })()));

      await waitUntil(() => json().some((line) => line.type === "interaction_request"));
      expect(json()).toContainEqual(expect.objectContaining({
        type: "interaction_request",
        id: "codex-1",
        kind: "mcp_elicitation",
        backend: "claude",
        payload: expect.objectContaining({
          threadId: `claude-session-${action}`,
          serverName: "github",
          mode: "url",
          url: "https://github.com/login/oauth",
          elicitationId: `elicit-${action}`,
        }),
      }));

      broker.handleControlLine(JSON.stringify({
        type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
        id: "codex-1",
        kind: "mcp_elicitation",
        result: { action },
      }));

      await run;
      expect(elicitationResult).toEqual({ action });
    }
  });

  it("Claude session fork runtime command calls forkSession and emits the forked session id", async () => {
    const { protocol, json } = captureProtocol();
    let forkInput: any = null;
    let queryCalled = false;

    await runClaude({
      cwd: "C:/repo",
      prompt: "",
      resumeSessionId: "claude-source",
      runtimeCommand: { type: SESSION_FORK_COMMAND_TYPE },
    }, claudeRunnerContext(protocol, {
      forkClaudeSession: async (sessionId: string, options: any) => {
        forkInput = { sessionId, options };
        return { sessionId: "claude-forked" };
      },
      createClaudeQuery: emptyClaudeQueryProbe(() => {
        queryCalled = true;
      }),
    }));

    expect(forkInput).toEqual({
      sessionId: "claude-source",
      options: { dir: "C:/repo" },
    });
    expect(queryCalled).toBe(false);
    expect(json()).toContainEqual({
      type: "done",
      sessionId: "claude-forked",
      subtype: "success",
    });
  });

  it("Claude auto session fork continues the current prompt in the forked session", async () => {
    const { protocol, json } = captureProtocol();
    let forkInput: any = null;
    let seenOptions: any = null;

    await runClaude({
      cwd: "C:/repo",
      prompt: "继续实现",
      resumeSessionId: "claude-source",
      autoSessionFork: true,
    }, claudeRunnerContext(protocol, {
      forkClaudeSession: async (sessionId: string, options: any) => {
        forkInput = { sessionId, options };
        return { sessionId: "claude-forked" };
      },
      createClaudeQuery: ({ options }: any) => {
        seenOptions = options;
        return (async function* () {
          yield {
            type: "result",
            is_error: false,
            subtype: "success",
            session_id: "claude-forked",
            uuid: "result-1",
            usage: {
              input_tokens: 10,
              output_tokens: 2,
            },
          };
        })();
      },
    }));

    expect(forkInput).toEqual({
      sessionId: "claude-source",
      options: { dir: "C:/repo" },
    });
    expect(seenOptions).toMatchObject({
      cwd: "C:/repo",
      resume: "claude-forked",
    });
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "success",
        title: "Claude session fork completed",
        payload: expect.objectContaining({
          autoTurnDecision: true,
          sourceSessionId: "claude-source",
          sessionId: "claude-forked",
        }),
      }),
    }));
    expect(json()).toContainEqual({
      type: "done",
      sessionId: "claude-forked",
      subtype: "success",
    });
  });

  it("Claude session fork continue mode resumes at the selected turn for the next prompt", async () => {
    const { protocol } = captureProtocol();
    let seenOptions: any = null;
    let forkCalled = false;

    await runClaude({
      cwd: "C:/repo",
      prompt: "从这里继续",
      resumeSessionId: "claude-source",
      runtimeCommand: {
        type: "session_fork",
        sourceTurnId: "message-uuid",
        mode: "continue",
      },
    }, claudeRunnerContext(protocol, {
      forkClaudeSession: async () => {
        forkCalled = true;
        return { sessionId: "claude-forked" };
      },
      createClaudeQuery: ({ options }: any) => {
        seenOptions = options;
        return (async function* () {
          yield {
            type: "result",
            is_error: false,
            subtype: "success",
            session_id: "claude-source",
            uuid: "result-1",
            usage: { input_tokens: 10, output_tokens: 2 },
          };
        })();
      },
    }));

    expect(forkCalled).toBe(false);
    expect(seenOptions).toMatchObject({
      resume: "claude-source",
      resumeSessionAt: "message-uuid",
    });
  });

  it("Claude session fork mode forks from the selected turn before continuing the prompt", async () => {
    const { protocol } = captureProtocol();
    let forkInput: any = null;
    let seenOptions: any = null;

    await runClaude({
      cwd: "C:/repo",
      prompt: "从分叉继续",
      resumeSessionId: "claude-source",
      runtimeCommand: {
        type: "session_fork",
        sourceTurnId: "message-uuid",
        mode: "fork",
      },
    }, claudeRunnerContext(protocol, {
      forkClaudeSession: async (sessionId: string, options: any) => {
        forkInput = { sessionId, options };
        return { sessionId: "claude-forked" };
      },
      createClaudeQuery: ({ options }: any) => {
        seenOptions = options;
        return (async function* () {
          yield {
            type: "result",
            is_error: false,
            subtype: "success",
            session_id: "claude-forked",
            uuid: "result-1",
            usage: { input_tokens: 10, output_tokens: 2 },
          };
        })();
      },
    }));

    expect(forkInput).toEqual({
      sessionId: "claude-source",
      options: { dir: "C:/repo", resumeSessionAt: "message-uuid" },
    });
    expect(seenOptions).toMatchObject({
      resume: "claude-forked",
      resumeSessionAt: "message-uuid",
    });
  });

  it("Claude session fork runtime command requires an existing session checkpoint", async () => {
    const { protocol, json } = captureProtocol();
    let forkCalled = false;
    let queryCalled = false;

    await expect(runClaude({
      cwd: "C:/repo",
      prompt: "",
      runtimeCommand: { type: SESSION_FORK_COMMAND_TYPE },
    }, claudeRunnerContext(protocol, {
      forkClaudeSession: async () => {
        forkCalled = true;
        return { sessionId: "claude-forked" };
      },
      createClaudeQuery: emptyClaudeQueryProbe(() => {
        queryCalled = true;
      }),
    }))).rejects.toThrow("当前 Claude task 没有可 fork 的 session");

    expect(forkCalled).toBe(false);
    expect(queryCalled).toBe(false);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "error",
        title: "Claude session fork failed",
      }),
    }));
  });

  it("handles Claude session management through SDK session APIs", async () => {
    for (const runtimeCommand of [
      { type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE, action: "list", limit: 10, cursor: "5", searchTerm: "fix" },
      { type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE, action: "info", sessionId: "claude-session-1" },
      {
        type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE,
        action: "messages",
        sessionId: "claude-session-1",
        limit: 3,
        cursor: "2",
        includeSystemMessages: true,
      },
      { type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE, action: "tag", sessionId: "claude-session-1", tag: "release" },
      { type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE, action: "delete", sessionId: "claude-session-1" },
    ]) {
      const { protocol, json } = captureProtocol();
      const calls: any[] = [];

      await runClaudeSessionManagementRuntimeCommand({
        cwd: "C:/repo",
        prompt: "",
        resumeSessionId: "claude-current",
        runtimeCommand,
      }, {
        protocol,
        listClaudeSessions: async (options: any) => {
          calls.push({ method: "listSessions", options });
          return [{
            sessionId: "claude-session-1",
            summary: "Fix flow",
            lastModified: 10,
            cwd: "C:/repo",
          }];
        },
        getClaudeSessionInfo: async (sessionId: string, options: any) => {
          calls.push({ method: "getSessionInfo", sessionId, options });
          return { sessionId, summary: "Session info", lastModified: 11 };
        },
        getClaudeSessionMessages: async (sessionId: string, options: any) => {
          calls.push({ method: "getSessionMessages", sessionId, options });
          return [{
            type: "user",
            uuid: "msg-1",
            message: { content: "用户问题" },
          }];
        },
        renameClaudeSession: async (sessionId: string, title: string, options: any) => {
          calls.push({ method: "renameSession", sessionId, title, options });
        },
        tagClaudeSession: async (sessionId: string, tag: string | null, options: any) => {
          calls.push({ method: "tagSession", sessionId, tag, options });
        },
        deleteClaudeSession: async (sessionId: string, options: any) => {
          calls.push({ method: "deleteSession", sessionId, options });
        },
      } as any, "C:/repo");

      expect(calls[0]).toMatchObject({
        method: runtimeCommand.action === "list"
          ? "listSessions"
          : runtimeCommand.action === "info"
            ? "getSessionInfo"
            : runtimeCommand.action === "messages"
              ? "getSessionMessages"
              : runtimeCommand.action === "tag"
                ? "tagSession"
                : "deleteSession",
      });
      expect(json()).toContainEqual(expect.objectContaining({
        type: "timeline",
        event: expect.objectContaining({
          kind: "diagnostic",
          status: "success",
          payload: expect.objectContaining({
            backend: "claude",
            subkind: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE,
            action: runtimeCommand.action,
            native: true,
          }),
        }),
      }));
      expect(json().some((line) =>
        line.type === "done" &&
        line.sessionId === (runtimeCommand.sessionId || "claude-current")
      )).toBe(true);
    }
  });

  it("reports unsupported diagnostic for Claude session archive", async () => {
    const { protocol, json } = captureProtocol();

    await expect(runClaudeSessionManagementRuntimeCommand({
      cwd: "C:/repo",
      prompt: "",
      runtimeCommand: { type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE, action: "archive", sessionId: "claude-session-1" },
    }, { protocol } as any, "C:/repo")).resolves.toBe(true);

    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "success",
        payload: expect.objectContaining({
          backend: "claude",
          subkind: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE,
          action: "archive",
          native: false,
          result: expect.objectContaining({ unsupported: true }),
        }),
      }),
    }));
  });

  it("reports unsupported diagnostic when Claude session rename API is unavailable", async () => {
    const { protocol, json } = captureProtocol();

    await expect(runClaudeSessionManagementRuntimeCommand({
      cwd: "C:/repo",
      prompt: "",
      runtimeCommand: {
        type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE,
        action: "rename",
        sessionId: "claude-session-1",
        title: "新标题",
      },
    }, {
      protocol,
      renameClaudeSession: null,
    } as any, "C:/repo")).rejects.toThrow("Claude SDK renameSession is not available");

    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "error",
        payload: expect.objectContaining({
          backend: "claude",
          subkind: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE,
          action: "rename",
          native: false,
          error: "Claude SDK renameSession is not available",
        }),
      }),
    }));
  });

  it("Claude compact workflow sends native /compact and emits done", async () => {
    const { protocol, json } = captureProtocol();
    let seenPrompt = "";
    let seenOptions: any = null;

    await runClaude({
      cwd: "C:/repo",
      prompt: "",
      resumeSessionId: "claude-source",
      workflow: { type: LILIA_COMPACT_WORKFLOW_TYPE },
    }, claudeRunnerContextWithTools(protocol, {
      createClaudeQuery: ({ prompt, options }: any) => {
        seenOptions = options;
        return (async function* () {
          for await (const msg of prompt) {
            seenPrompt = msg.message.content[0].text;
          }
          yield {
            type: "system",
            subtype: "status",
            status: "compacting",
            session_id: "claude-source",
            uuid: "compact-status-1",
          };
          yield {
            type: "system",
            subtype: "compact_boundary",
            session_id: "claude-source",
            uuid: "compact-boundary-1",
            compact_metadata: {
              trigger: "manual",
              pre_tokens: 12000,
              post_tokens: 3000,
              duration_ms: 42,
            },
          };
          yield {
            type: "system",
            subtype: "status",
            compact_result: "success",
            session_id: "claude-source",
            uuid: "compact-status-2",
          };
          yield {
            type: "result",
            is_error: false,
            subtype: "success",
            session_id: "claude-source",
            uuid: "compact-result",
          };
        })();
      },
    }));

    expect(seenPrompt).toBe("/compact");
    expect(seenOptions).toMatchObject({
      resume: "claude-source",
      promptSuggestions: false,
    });
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.title === "Lilia workflow handled locally"
    )).toBe(false);
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.title === "Claude compact boundary" &&
      line.event.payload.preTokens === 12000 &&
      line.event.payload.postTokens === 3000
    )).toBe(true);
    expect(json()).toContainEqual({
      type: "done",
      sessionId: "claude-source",
      subtype: "success",
    });
  });

  it("Claude compact workflow requires an existing session checkpoint", async () => {
    const { protocol, json } = captureProtocol();
    let queryCalled = false;

    await expect(runClaude({
      cwd: "C:/repo",
      prompt: "",
      workflow: { type: LILIA_COMPACT_WORKFLOW_TYPE },
    }, claudeRunnerContext(protocol, {
      createClaudeQuery: emptyClaudeQueryProbe(() => {
        queryCalled = true;
      }),
    }))).rejects.toThrow("当前 Claude task 没有可 compact 的 session");

    expect(queryCalled).toBe(false);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "error",
        title: "Claude compact failed",
      }),
    }));
  });

  it("Claude compact workflow fails when native compact reports failed", async () => {
    const { protocol, json } = captureProtocol();

    await expect(runClaude({
      cwd: "C:/repo",
      prompt: "",
      resumeSessionId: "claude-source",
      workflow: { type: LILIA_COMPACT_WORKFLOW_TYPE },
    }, claudeRunnerContextWithTools(protocol, {
      createClaudeQuery: () => (async function* () {
        yield {
          type: "system",
          subtype: "status",
          compact_result: "failed",
          compact_error: "native compact failed",
          session_id: "claude-source",
          uuid: "compact-status-failed",
        };
        yield {
          type: "result",
          is_error: false,
          subtype: "success",
          session_id: "claude-source",
          uuid: "compact-result-failed",
        };
      })(),
    }))).rejects.toThrow("native compact failed");

    expect(json().some((line) =>
      line.type === "done" &&
      line.sessionId === "claude-source"
    )).toBe(false);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "error",
        title: "Claude compact failed",
      }),
    }));
  });

  it.each([
    {
      name: "memory mode",
      workflow: { type: "lilia_memory_mode", mode: "enabled" },
      title: "Claude memory mode recorded",
      payload: { subkind: "memory_mode", mode: "enabled", native: false },
    },
    {
      name: "memory reset",
      workflow: { type: "lilia_memory_reset" },
      title: "Claude memory reset recorded",
      payload: { subkind: "memory_reset", native: false },
    },
    {
      name: "background terminals clean",
      workflow: { type: LILIA_BACKGROUND_TERMINALS_CLEAN_WORKFLOW_TYPE },
      title: "Claude background terminals clean completed",
      payload: { subkind: "background_terminals_clean", cleanedCount: 0 },
    },
  ])("Claude $name workflow is handled by Lilia without SDK query", async ({ workflow, title, payload }) => {
    const { protocol, json } = captureProtocol();
    let queryCalled = false;

    await runClaude({
      cwd: "C:/repo",
      prompt: "",
      resumeSessionId: "claude-source",
      workflow,
    }, claudeRunnerContext(protocol, {
      createClaudeQuery: emptyClaudeQueryProbe(() => {
        queryCalled = true;
      }),
    }));

    expect(queryCalled).toBe(false);
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.title === "Lilia workflow handled locally"
    )).toBe(false);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "success",
        title,
        payload: expect.objectContaining({
          backend: "claude",
          source: "lilia",
          ...payload,
        }),
      }),
    }));
    expect(json()).toContainEqual({ type: "done", sessionId: null, subtype: "success" });
  });

  it("Claude memory mode workflow rejects invalid mode before SDK query", async () => {
    const { protocol } = captureProtocol();
    let queryCalled = false;

    await expect(runClaude({
      cwd: "C:/repo",
      prompt: "",
      workflow: { type: "lilia_memory_mode", mode: "maybe" },
    }, claudeRunnerContext(protocol, {
      createClaudeQuery: emptyClaudeQueryProbe(() => {
        queryCalled = true;
      }),
    }))).rejects.toThrow("Lilia memory mode workflow missing a valid mode");

    expect(queryCalled).toBe(false);
  });

  it("Claude provider settings passes advanced fields to SDK query", async () => {
    const { protocol, json } = captureProtocol();
    let seenOptions: any = null;

    await runClaude({
      cwd: "C:/repo",
      prompt: "",
      model: "claude-sonnet-4-6",
      permission: "ask",
      runtimeCommand: {
        type: RUNTIME_SETTINGS_COMMAND_TYPE,
        action: "update",
      },
      runtimeOptions: {
        common: { model: "claude-opus-4-5", permission: "readonly", reasoningEffort: "medium" },
        provider: {
          claude: {
          allowedTools: ["Read"],
          disallowedTools: ["Bash"],
          additionalDirectories: ["D:/shared"],
          reasoningEffort: "xhigh",
          maxTurns: 4,
          maxBudgetUsd: 1.5,
          tools: { type: "preset", preset: "claude_code" },
          permissionPromptToolName: "mcp__lilia__permission_prompt",
          settings: { model: "claude-opus-4-5" },
          managedSettings: {
            sandbox: { enabled: true },
            agents: {
              "lilia-reviewer": {
                description: "检查风险与回归",
                prompt: "Review code changes and summarize risks.",
              },
            },
          },
          settingSources: ["user", "project"],
          sandbox: { enabled: true },
          outputFormat: { type: "json" },
          includeHookEvents: true,
          forwardSubagentText: true,
          agentProgressSummaries: true,
          continue: true,
          resumeSessionAt: "message-uuid",
          sessionId: "00000000-0000-4000-8000-000000000001",
          abortAfterMs: 3000,
          sessionStore: { explicit: true },
          },
        },
      },
    }, claudeRunnerContextWithTools(protocol, {
      createClaudeQuery: ({ options }: any) => {
        seenOptions = options;
        return (async function* () {
          yield {
            type: "result",
            is_error: false,
            subtype: "success",
            session_id: "claude-provider-settings",
            uuid: "provider-settings-result",
          };
        })();
      },
    }));

    expect(seenOptions).toMatchObject({
      model: "claude-opus-4-5",
      permissionMode: "default",
      allowedTools: ["Read"],
      disallowedTools: ["Bash"],
      additionalDirectories: ["D:/shared"],
      effort: "xhigh",
      thinking: { type: "adaptive" },
      maxTurns: 4,
      maxBudgetUsd: 1.5,
      tools: { type: "preset", preset: "claude_code" },
      permissionPromptToolName: "mcp__lilia__permission_prompt",
      settings: { model: "claude-opus-4-5" },
      managedSettings: {
        sandbox: { enabled: true },
        agents: {
          "lilia-reviewer": {
            description: "检查风险与回归",
            prompt: "Review code changes and summarize risks.",
          },
        },
      },
      settingSources: ["user", "project"],
      sandbox: { enabled: true },
      outputFormat: { type: "json" },
      includeHookEvents: true,
      forwardSubagentText: true,
      agentProgressSummaries: true,
      continue: true,
      resumeSessionAt: "message-uuid",
      sessionId: "00000000-0000-4000-8000-000000000001",
      sessionStore: { explicit: true },
      agents: {
        "lilia-reviewer": {
          description: "检查风险与回归",
          prompt: "Review code changes and summarize risks.",
        },
      },
    });
    expect(seenOptions.abortController).toBeInstanceOf(AbortController);
    expect(seenOptions.abortAfterMs).toBeUndefined();
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "success",
        title: "Claude provider settings applied",
        payload: expect.objectContaining({
          backend: "claude",
          subkind: "provider_settings",
          action: "update",
          settingsKeys: expect.arrayContaining(["model", "permission", "allowedTools", "effort", "thinking"]),
          optionKeys: expect.arrayContaining(["allowedTools", "disallowedTools", "effort", "thinking"]),
        }),
      }),
    }));
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.payload?.unsupportedKeys
    )).toBe(false);
    expect(json()).toContainEqual({
      type: "done",
      sessionId: "claude-provider-settings",
      subtype: "success",
    });
  });

  it("Claude interrupt control aborts the active SDK query", async () => {
    const { protocol, json } = captureProtocol();
    const broker = createTestInteractionBroker(protocol);
    let seenController: AbortController | null = null;

    const run = runClaude({
      cwd: "C:/repo",
      prompt: "long turn",
      backend: "claude",
    }, {
      protocol,
      interactions: broker,
      emitToolConsentTimeline: () => {},
      createClaudeQuery: ({ options }: any) => {
        seenController = options.abortController;
        return (async function* () {
          await new Promise((_resolve, reject) => {
            options.abortController.signal.addEventListener("abort", () => {
              const err = new Error("aborted");
              err.name = "AbortError";
              reject(err);
            });
          });
        })();
      },
    });

    await waitUntil(() => seenController !== null);
    broker.handleControlLine(JSON.stringify({ type: RUNNER_INTERRUPT_TURN_CONTROL_TYPE }));
    await run;

    expect(seenController?.signal.aborted).toBe(true);
    expect(json()).toContainEqual({
      type: "done",
      sessionId: null,
      subtype: "interrupted",
    });
    expect(json().some((line) => line.type === "error")).toBe(false);
  });

  it("Claude provider settings diagnose emits diagnostics without SDK query", async () => {
    const { protocol, json } = captureProtocol();
    let queryCalled = false;

    await runClaude({
      cwd: "C:/repo",
      prompt: "",
      resumeSessionId: "claude-existing",
      runtimeCommand: {
        type: RUNTIME_SETTINGS_COMMAND_TYPE,
        action: "diagnose",
      },
      runtimeOptions: {
        common: { model: "claude-opus-4-5", permission: "readonly" },
        provider: {
          claude: {
            allowedTools: ["Read"],
            unknownClaudeOption: true,
          },
          codex: {
            reasoningEffort: "high",
          },
        },
      },
    }, claudeRunnerContext(protocol, {
      createClaudeQuery: emptyClaudeQueryProbe(() => {
        queryCalled = true;
      }),
    }));

    expect(queryCalled).toBe(false);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "info",
        title: "Claude provider settings diagnostics",
        payload: expect.objectContaining({
          backend: "claude",
          subkind: "provider_settings",
          action: "diagnose",
          settingsKeys: expect.arrayContaining(["model", "permission", "allowedTools"]),
          ignoredProviderKeys: ["reasoningEffort"],
          unsupportedKeys: ["unknownClaudeOption"],
        }),
      }),
    }));
    expect(json()).toContainEqual({
      type: "done",
      sessionId: "claude-existing",
      subtype: "success",
    });
  });

  it("Claude sandbox diagnostics emits unsupported diagnostic without SDK query", async () => {
    const { protocol, json } = captureProtocol();
    let queryCalled = false;

    await runClaude({
      cwd: "C:/repo",
      prompt: "",
      resumeSessionId: "claude-existing",
      runtimeCommand: {
        type: SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
        includeDetails: true,
      },
    }, claudeRunnerContext(protocol, {
      createClaudeQuery: emptyClaudeQueryProbe(() => {
        queryCalled = true;
      }),
    }));

    expect(queryCalled).toBe(false);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "info",
        title: "Claude sandbox diagnostics unsupported",
        payload: expect.objectContaining({
          backend: "claude",
          subkind: SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
          includeDetails: true,
          native: false,
          unsupported: true,
        }),
      }),
    }));
    expect(json()).toContainEqual({
      type: "done",
      sessionId: "claude-existing",
      subtype: "success",
    });
  });

  it("Claude remote environment emits unsupported diagnostic without SDK query", async () => {
    const { protocol, json } = captureProtocol();
    let queryCalled = false;

    await runClaude({
      cwd: "C:/repo",
      prompt: "",
      resumeSessionId: "claude-existing",
      runtimeCommand: {
        type: REMOTE_ENVIRONMENT_COMMAND_TYPE,
        action: "select",
        environmentId: "env-1",
      },
    }, claudeRunnerContext(protocol, {
      createClaudeQuery: emptyClaudeQueryProbe(() => {
        queryCalled = true;
      }),
    }));

    expect(queryCalled).toBe(false);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "info",
        title: "Claude remote environment unsupported",
        payload: expect.objectContaining({
          backend: "claude",
          subkind: REMOTE_ENVIRONMENT_COMMAND_TYPE,
          action: "select",
          environmentId: "env-1",
          native: false,
          unsupported: true,
        }),
      }),
    }));
    expect(json()).toContainEqual({
      type: "done",
      sessionId: "claude-existing",
      subtype: "success",
    });
  });

  it("Claude process session emits unsupported diagnostic without SDK query", async () => {
    const { protocol, json } = captureProtocol();
    let queryCalled = false;

    await runClaude({
      cwd: "C:/repo",
      prompt: "",
      resumeSessionId: "claude-existing",
      runtimeCommand: {
        type: "process_session",
        action: "spawn",
        command: "npm test",
      },
    }, claudeRunnerContext(protocol, {
      createClaudeQuery: emptyClaudeQueryProbe(() => {
        queryCalled = true;
      }),
    }));

    expect(queryCalled).toBe(false);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "info",
        title: "Claude process session unsupported",
        payload: expect.objectContaining({
          backend: "claude",
          subkind: "process_session",
          action: "spawn",
          native: false,
          unsupported: true,
        }),
      }),
    }));
    expect(json()).toContainEqual({
      type: "done",
      sessionId: "claude-existing",
      subtype: "success",
    });
  });

  it("Claude experimental provider options emit diagnostics or stop by fallback", async () => {
    const diagnostic = captureProtocol();
    let queryCalled = false;

    await runClaude({
      cwd: "C:/repo",
      prompt: "hello",
      runtimeOptions: {
        experimentalProviderOptions: [{
          provider: "claude",
          capability: "future-output-mode",
          payload: { enabled: true },
          fallback: "diagnostic",
        }],
      },
    }, claudeRunnerContextWithTools(diagnostic.protocol, {
      createClaudeQuery: () => {
        queryCalled = true;
        return (async function* () {
          yield {
            type: "result",
            is_error: false,
            subtype: "success",
            session_id: "claude-experimental-diagnostic",
            uuid: "claude-experimental-diagnostic-result",
          };
        })();
      },
    }));

    expect(queryCalled).toBe(true);
    expect(diagnostic.json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "info",
        title: "Ignored experimental provider option",
        payload: expect.objectContaining({
          backend: "claude",
          subkind: "experimental_provider_option",
          capability: "future-output-mode",
          fallback: "diagnostic",
          payloadKeys: ["enabled"],
        }),
      }),
    }));

    const unsupported = captureProtocol();
    let unsupportedQueryCalled = false;
    await expect(runClaude({
      cwd: "C:/repo",
      prompt: "hello",
      runtimeOptions: {
        experimentalProviderOptions: [{
          provider: "claude",
          capability: "future-session-store",
          payload: {},
          fallback: "unsupported",
        }],
      },
    }, claudeRunnerContext(unsupported.protocol, {
      createClaudeQuery: () => {
        unsupportedQueryCalled = true;
        return emptyClaudeQuery();
      },
    }))).rejects.toThrow("claude experimental provider capability is unsupported: future-session-store");

    expect(unsupportedQueryCalled).toBe(false);
    expect(unsupported.json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "error",
        title: "Unsupported experimental provider option",
        payload: expect.objectContaining({
          backend: "claude",
          capability: "future-session-store",
          fallback: "unsupported",
        }),
      }),
    }));
  });

  it("Claude config diagnostics reports safe Lilia and runtime facts without SDK query", async () => {
    const { protocol, json } = captureProtocol();
    let queryCalled = false;

    await runClaude({
      cwd: "C:/repo",
      prompt: "",
      model: "claude-sonnet-4-6",
      permission: "full",
      planMode: true,
      resumeSessionId: "claude-source",
      workflow: { type: LILIA_CONFIG_DIAGNOSTICS_WORKFLOW_TYPE },
      extensions: {
        claude: {
          skills: ["reviewer"],
          plugins: [{ type: "local", path: "C:/plugins/one" }],
          warnings: ["skip unsafe setting"],
          mcpServers: {
            lilia: { type: "stdio", command: "ignored" },
            safeServer: {
              type: "stdio",
              command: "node",
              args: ["server.mjs"],
              env: { API_KEY: "secret-value" },
            },
          },
        },
      },
    }, claudeRunnerContext(protocol, {
      createClaudeQuery: emptyClaudeQueryProbe(() => {
        queryCalled = true;
      }),
    }));

    expect(queryCalled).toBe(false);
    const diagnostic = json().find((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.title === "Claude config diagnostics"
    );
    expect(diagnostic).toMatchObject({
      event: {
        status: "success",
        payload: {
          backend: "claude",
          source: "lilia",
          subkind: "config_diagnostics",
          cwd: "C:/repo",
          model: "claude-sonnet-4-6",
          permission: "full",
          planMode: true,
          hasResumeSession: true,
          runtimeExtensions: {
            mcpServerCount: 1,
            mcpServers: ["safeServer"],
            skillCount: 1,
            skills: ["reviewer"],
            pluginCount: 1,
            warningCount: 2,
          },
        },
      },
    });
    expect(JSON.stringify(diagnostic)).not.toContain("secret-value");
    expect(JSON.stringify(diagnostic)).not.toContain("API_KEY");
  });

  it("plan mode 初始进入 Claude plan，确认后恢复原执行权限映射", () => {
    expect(mapClaudeInitialPermission("ask", true).permissionMode).toBe("plan");
    expect(mapClaudeInitialPermission("readonly", false).permissionMode).toBe("default");
    expect(mapClaudeInitialPermission("full", false)).toMatchObject({
      permissionMode: "bypassPermissions",
      allowDangerouslySkipPermissions: true,
    });
    expect(mapClaudeInitialPermission("free", false)).toMatchObject({
      permissionMode: "bypassPermissions",
      allowDangerouslySkipPermissions: true,
    });
    expect(mapClaudeInitialPermission("unknown", false).permissionMode).toBe("default");
  });

  it("runtime permission update changes Claude SDK mode and Lilia gate", async () => {
    const { protocol, json } = captureProtocol();
    const modeCalls: string[] = [];
    const ctx: any = {
      protocol,
      executionPermission: "ask",
      query: {
        setPermissionMode: async (mode: string) => {
          modeCalls.push(mode);
        },
      },
    };

    expect(applyClaudeRuntimePermission(ctx, "readonly")).toBe(true);
    expect(ctx.executionPermission).toBe("readonly");
    expect(modeCalls).toEqual(["default"]);

    expect(applyClaudeRuntimePermission(ctx, "full")).toBe(true);
    expect(ctx.executionPermission).toBe("full");
    expect(modeCalls).toEqual(["default", "bypassPermissions"]);

    expect(applyClaudeRuntimePermission(ctx, "free")).toBe(true);
    expect(ctx.executionPermission).toBe("free");
    expect(modeCalls).toEqual(["default", "bypassPermissions", "bypassPermissions"]);
    expect(json().filter((line) => line.type === "timeline")).toHaveLength(3);
    expect(applyClaudeRuntimePermission(ctx, "unknown")).toBe(false);
    expect(normalizeRuntimePermission("unknown")).toBeNull();
  });

  it("maps free runtime permission to full architecture permission", async () => {
    const change = { type: "set_summary", summary: "更新架构摘要" };
    const requests: any[] = [];
    const handler = createArchitectureChangeHandler({
      cmd: {
        conversationContext: { projectId: "project-1" },
        permission: "free",
        taskId: "task-1",
        turnId: "turn-1",
      },
      ctx: {
        currentTurnId: "turn-1",
        approvedArchitectureImpacts: [{ changes: [change] }],
        interactions: {
          requestArchitectureChange: async (payload: any, options: any) => {
            requests.push({ payload, options });
            return { decision: "allow", graph: { version: 2 } };
          },
        },
      },
      backend: "codex",
    });

    const result = await handler({ reason: "更新架构", changes: [change] });

    expect(result).toMatchObject({ ok: true, applied: true, version: 2 });
    expect(requests).toHaveLength(1);
    expect(requests[0].payload).toMatchObject({
      permission: "full",
      status: PROJECT_ARCHITECTURE_APPLIED_INTERACTION_STATUS,
      requiresConfirmation: false,
    });
    expect(requests[0].options).toMatchObject({ autoApply: true });
  });
});

describe("Codex app-server mapping", () => {
  it("derives Codex permission profile from Lilia permission mode", () => {
    expect(mapCodexApprovalPolicy("full")).toBe("never");
    expect(mapCodexSandboxMode("full")).toBe("danger-full-access");
    expect(codexPermissionProfileIdForMode("full")).toBe(":danger-no-sandbox");

    expect(mapCodexApprovalPolicy("free")).toBe("never");
    expect(mapCodexSandboxMode("free")).toBe("danger-full-access");
    expect(codexPermissionProfileIdForMode("free")).toBe(":danger-no-sandbox");

    expect(mapCodexApprovalPolicy("ask")).toBe("on-request");
    expect(mapCodexSandboxMode("ask")).toBe("read-only");
    expect(codexPermissionProfileIdForMode("ask")).toBe(":read-only");

    expect(mapCodexApprovalPolicy("readonly")).toBe("on-request");
    expect(mapCodexSandboxMode("readonly")).toBe("read-only");
    expect(codexPermissionProfileIdForMode("readonly")).toBe(":read-only");
    expect(codexPermissionRuntimeParams("readonly")).toEqual(codexPermissionRuntimeParams("ask"));

    expect(mapCodexApprovalPolicy("unknown")).toBe("on-request");
    expect(mapCodexSandboxMode("unknown")).toBe("read-only");
    expect(codexPermissionProfileIdForMode("unknown")).toBe(":read-only");
    expect(codexPermissionRuntimeParams("ask")).toEqual({
      approvalPolicy: "on-request",
      permissions: ":read-only",
      sandboxPolicy: { type: "readOnly" },
    });
    expect(codexLegacyPermissionRuntimeParams("readonly")).toEqual({
      approvalPolicy: "on-request",
      sandbox: "read-only",
    });
  });

  it("normalizes Codex timeline execution permission through contracts defaults", () => {
    const { protocol } = captureProtocol();

    expect(createCodexRunContext({ permission: "free" }, protocol).executionPermission).toBe("free");
    expect(createCodexRunContext({ permission: "unknown" }, protocol).executionPermission).toBe("ask");
  });

  it("resolves managed Codex CLI path from Lilia home", () => {
    const expected = join("C:/Lilia", "runtime", "codex", "bin", "codex.cmd");
    expect(codexAppServerBinary({
      env: { LILIA_HOME: "C:/Lilia" } as any,
      platform: "win32",
      fileExists: (path: string) => path === expected,
    })).toBe(expected);
  });

  it("follows Lilia home redirect for managed Codex CLI path", () => {
    const homeDir = join("C:/Users", "me");
    const defaultHome = join(homeDir, ".lilia");
    const redirectedHome = join("D:/Data", "Lilia");
    const redirectFile = join(defaultHome, ".redirect");
    const expected = join(redirectedHome, "runtime", "codex", "bin", "codex");

    expect(codexAppServerBinary({
      env: {} as any,
      platform: "linux",
      homeDir,
      fileExists: (path: string) => path === redirectFile || path === expected,
      readTextFile: (path: string) => {
        expect(path).toBe(redirectFile);
        return ` ${redirectedHome} \n`;
      },
    })).toBe(expected);
  });

  it("keeps a clear error when managed Codex CLI is missing", () => {
    expect(() => codexAppServerBinary({
      env: {} as any,
      homeDir: "C:/Users/me",
      fileExists: () => false,
    })).toThrow("Lilia 内置 Codex CLI 未安装或不可用");
  });

  it("resolves Windows npm extensionless Codex shim to cmd script", () => {
    expect(resolveWindowsCommandScript("C:/Users/me/AppData/Roaming/npm/codex", {
      platform: "win32",
      fileExists: (path: string) => path === "C:/Users/me/AppData/Roaming/npm/codex.cmd",
    })).toBe("C:/Users/me/AppData/Roaming/npm/codex.cmd");
    expect(resolveWindowsCommandScript("C:/bin/codex.exe", {
      platform: "win32",
      fileExists: () => true,
    })).toBeNull();
    expect(resolveWindowsCommandScript("C:/Users/me/AppData/Roaming/npm/codex", {
      platform: "linux",
      fileExists: () => true,
    })).toBeNull();
  });

  it("runs Windows command scripts through cmd.exe", () => {
    expect(codexAppServerSpawnCommand("C:/Program Files/node/codex.cmd", {
      env: { ComSpec: "C:/Windows/System32/cmd.exe" } as any,
      platform: "win32",
    })).toEqual({
      command: "C:/Windows/System32/cmd.exe",
      args: [
        "/d",
        "/s",
        "/c",
        "\"C:/Program Files/node/codex.cmd\" app-server",
      ],
    });
  });

  it("serializes Codex app-server request params, including null for no-arg methods", async () => {
    const writes: string[] = [];
    const stdout = new PassThrough();
    const child: any = new EventEmitter();
    child.stdout = stdout;
    child.stderr = new PassThrough();
    child.stdin = {
      write: (line: string) => {
        writes.push(line);
        const payload = JSON.parse(line);
        queueMicrotask(() => {
          stdout.write(`${JSON.stringify({ id: payload.id, result: { ok: true } })}\n`);
        });
        return true;
      },
    };
    child.kill = () => {};
    const server = createCodexAppServer({
      env: {},
      resolveBinary: () => "codex",
      spawnServer: () => child,
    });

    await expect(server.request("memory/reset")).resolves.toEqual({ ok: true });
    await expect(server.request("config/read", { cwd: "C:/repo" })).resolves.toEqual({ ok: true });
    server.close({ forceKillMs: 0 });

    expect(JSON.parse(writes[0])).toEqual({
      id: 1,
      method: "memory/reset",
      params: null,
    });
    expect(JSON.parse(writes[1])).toEqual({
      id: 2,
      method: "config/read",
      params: { cwd: "C:/repo" },
    });
  });

  it("collects Codex Spark assistant delta text from app-server notifications", async () => {
    const calls: any[] = [];
    let drained = false;
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "initialize") return {};
        if (method === "thread/start") return { thread: { id: "thread-spark" } };
        if (method === "turn/start") return { turn: { id: "turn-spark" } };
        return {};
      },
      {
        notify: () => {},
        drainNotifications: () => {
          if (drained) return [];
          drained = true;
          return [
            { method: "item/agentMessage/delta", params: { itemId: "msg-1", delta: "{\"items\"" } },
            { method: "item/agentMessage/delta", params: { itemId: "msg-1", delta: ":[]}" } },
            { method: "turn/completed", params: { threadId: "thread-spark", turn: { status: "completed" } } },
          ];
        },
        close: () => calls.push({ method: "close" }),
      },
    );

    const result = await runCodexSparkPromptCommand({
      kind: "codex_spark_prompt",
      prompt: "suggest",
      instruction: "json only",
      timeoutMs: 100,
    }, {
      createCodexAppServer: () => server,
      cwd: () => "C:/repo",
    });

    expect(result).toEqual({ ok: true, text: "{\"items\":[]}", error: null });
    expect(calls.find((call) => call.method === "thread/start")?.params).toMatchObject({
      model: "gpt-5.3-codex-spark",
      approvalPolicy: "never",
      sandbox: "read-only",
      dynamicTools: [],
    });
    expect(calls.find((call) => call.method === "turn/start")?.params.collaborationMode.settings)
      .toMatchObject({
        model: "gpt-5.3-codex-spark",
        developer_instructions: "json only",
      });
  });

  it("collects Codex Spark assistant text from completed agentMessage items", () => {
    const state = {
      deltaText: "",
      snapshotText: "",
      completed: false,
      error: null,
    };

    updateCodexSparkPromptState(state, {
      method: "item/completed",
      params: {
        item: {
          type: "agentMessage",
          content: [{ type: "output_text", text: "完成标题" }],
        },
      },
    });
    updateCodexSparkPromptState(state, {
      method: "turn/completed",
      params: { turn: { status: "completed" } },
    });

    expect(state.snapshotText).toBe("完成标题");
    expect(state.completed).toBe(true);
    expect(state.error).toBeNull();
  });

  it("returns a handled failure JSON when Codex Spark turn fails", async () => {
    let drained = false;
    const server = {
      request: async (method: string) => {
        if (method === "thread/start") return { thread: { id: "thread-spark" } };
        return {};
      },
      notify: () => {},
      drainNotifications: () => {
        if (drained) return [];
        drained = true;
        return [
          {
            method: "turn/completed",
            params: {
              turn: { status: "failed", error: { message: "spark unavailable" } },
            },
          },
        ];
      },
      close: () => {},
    };

    const result = await runCodexSparkPromptCommand({
      kind: "codex_spark_prompt",
      prompt: "suggest",
      timeoutMs: 100,
    }, {
      createCodexAppServer: () => server,
      cwd: () => "C:/repo",
    });

    expect(result).toEqual({ ok: false, text: null, error: "spark unavailable" });
  });

  it("keeps app-server exit errors actionable when stderr is empty", async () => {
    const child: any = new EventEmitter();
    child.stdout = new PassThrough();
    child.stderr = new PassThrough();
    child.stdin = { write: () => true };
    child.kill = () => {};
    const server = createCodexAppServer({
      env: {},
      resolveBinary: () => "codex",
      spawnServer: () => child,
    });

    const request = server.request("thread/list", {});
    child.emit("exit", 1, null);

    await expect(request).rejects.toThrow(
      "Codex app-server exited (code 1)，但没有输出 stderr；请检查 Codex CLI 配置或认证状态。",
    );
  });

  it("includes app-server stderr when pending requests fail on exit", async () => {
    const child: any = new EventEmitter();
    child.stdout = new PassThrough();
    child.stderr = new PassThrough();
    child.stdin = { write: () => true };
    child.kill = () => {};
    const server = createCodexAppServer({
      env: {},
      resolveBinary: () => "codex",
      spawnServer: () => child,
    });

    const request = server.request("thread/list", {});
    child.stderr.write("auth failed\n");
    child.emit("exit", 2, null);

    await expect(request).rejects.toThrow("Codex app-server exited (code 2): auth failed");
  });

  it("does not surface SIGTERM as an app-server failure after Lilia force-closes the server", async () => {
    const child: any = new EventEmitter();
    child.stdout = new PassThrough();
    child.stderr = new PassThrough();
    child.stdin = { write: () => true, end: () => {} };
    child.kill = () => {
      queueMicrotask(() => {
        child.emit("exit", null, "SIGTERM");
        child.emit("close");
      });
      return true;
    };
    const server = createCodexAppServer({
      env: {},
      resolveBinary: () => "codex",
      spawnServer: () => child,
    });

    const request = server.request("thread/list", {});
    server.close({ forceKillMs: 0 });

    await expect(request).rejects.toThrow(
      "Codex app-server request was cancelled because Lilia closed the app-server.",
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
  });

  it("allows an in-flight app-server response to settle when close is requested", async () => {
    const stdout = new PassThrough();
    const child: any = new EventEmitter();
    child.stdout = stdout;
    child.stderr = new PassThrough();
    let stdinEnded = false;
    child.stdin = {
      write: (line: string) => {
        const payload = JSON.parse(line);
        queueMicrotask(() => {
          stdout.write(`${JSON.stringify({ id: payload.id, result: { threads: [] } })}\n`);
        });
        return true;
      },
      end: () => {
        stdinEnded = true;
        setTimeout(() => {
          child.emit("exit", 0, null);
          child.emit("close");
        }, 0);
      },
    };
    child.kill = () => {
      child.emit("exit", null, "SIGTERM");
      return true;
    };
    const server = createCodexAppServer({
      env: {},
      resolveBinary: () => "codex",
      spawnServer: () => child,
    });

    const request = server.request("thread/list", {});
    server.close();
    expect(stdinEnded).toBe(false);

    await expect(request).resolves.toEqual({ threads: [] });
    expect(stdinEnded).toBe(true);
  });

  it("keeps stdin open while a large app-server response is still pending", async () => {
    const stdout = new PassThrough();
    const child: any = new EventEmitter();
    let stdinEnded = false;
    let resolveResponse: (() => void) | null = null;
    child.stdout = stdout;
    child.stderr = new PassThrough();
    child.stdin = {
      write: (line: string) => {
        const payload = JSON.parse(line);
        resolveResponse = () => {
          stdout.write(`${JSON.stringify({
            id: payload.id,
            result: { data: Array.from({ length: 500 }, (_, index) => ({ id: `thread-${index}` })) },
          })}\n`);
        };
        return true;
      },
      end: () => {
        stdinEnded = true;
        child.emit("exit", 0, null);
        child.emit("close");
      },
    };
    child.kill = () => {
      child.emit("exit", null, "SIGTERM");
      child.emit("close");
      return true;
    };
    const server = createCodexAppServer({
      env: {},
      resolveBinary: () => "codex",
      spawnServer: () => child,
    });

    const request = server.request("thread/list", {});
    server.close();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(stdinEnded).toBe(false);
    resolveResponse?.();
    const result = await request;

    expect(result.data).toHaveLength(500);
    expect(stdinEnded).toBe(true);
  });

  it("keeps pending requests until stdout is drained after process exit", async () => {
    const stdout = new PassThrough();
    const child: any = new EventEmitter();
    child.stdout = stdout;
    child.stderr = new PassThrough();
    child.stdin = {
      write: (line: string) => {
        const payload = JSON.parse(line);
        queueMicrotask(() => {
          child.emit("exit", 0, null);
          stdout.write(`${JSON.stringify({ id: payload.id, result: { threads: ["large"] } })}\n`);
          child.emit("close");
        });
        return true;
      },
      end: () => {},
    };
    child.kill = () => {
      child.emit("exit", null, "SIGTERM");
      child.emit("close");
      return true;
    };
    const server = createCodexAppServer({
      env: {},
      resolveBinary: () => "codex",
      spawnServer: () => child,
    });

    const request = server.request("thread/list", {});
    server.close();

    await expect(request).resolves.toEqual({ threads: ["large"] });
  });

  it("normalizes app-server turn and plan events", () => {
    expect(normalizeCodexAppServerEvent({
      method: "turn/completed",
      params: { turn: { status: "failed", error: { message: "bad" } } },
    })).toMatchObject({
      type: "turn.failed",
      error: { message: "bad" },
    });
    expect(normalizeCodexPlanSteps([
      { step: "读代码", status: "completed" },
      { text: "写测试", status: "pending" },
    ])).toEqual([
      { text: "读代码", completed: true, status: "completed" },
      { text: "写测试", completed: false, status: "pending" },
    ]);
  });

  it("maps Codex item events to stable NDJSON/timeline facts", () => {
    const { protocol, json } = captureProtocol();
    const ctx: any = createCodexRunContext({ permission: "ask" }, protocol, "thread-1");

    mapCodexEventToNdjson({
      type: "item.started",
      item: {
        id: "cmd-1",
        type: "commandExecution",
        command: "yarn test",
        cwd: "C:/repo",
        stdout: "ok",
        stderr: "",
        aggregatedOutput: "ok",
        exitCode: 0,
        durationMs: 123,
        approvalId: "approval-1",
      },
    }, ctx);

    const lines = json();
    expect(lines).toEqual([
      {
        type: "timeline",
        event: expect.objectContaining({
          kind: "command",
          status: "started",
          title: "yarn test",
          summary: "yarn test",
          sourceId: "cmd-1",
        }),
      },
      {
        type: "tool_use",
        name: "commandExecution",
        input: expect.objectContaining({ id: "cmd-1", command: "yarn test" }),
      },
    ]);
    expect(lines[0].event.payload).toMatchObject({
      command: "yarn test",
      cwd: "C:/repo",
      stdout: "ok",
      aggregatedOutput: "ok",
      exitCode: 0,
      durationMs: 123,
      approvalId: "approval-1",
    });
  });

  it("maps Codex file and MCP item payload fields without dropping tool facts", () => {
    const { protocol, json } = captureProtocol();
    const ctx: any = createCodexRunContext({ permission: "ask" }, protocol, "thread-1");

    mapCodexEventToNdjson({
      type: "item.completed",
      item: {
        id: "file-1",
        type: "fileChange",
        path: "src/App.vue",
        changes: [{ kind: "update", path: "src/App.vue" }],
        grantRoot: "C:/repo",
        output: "patched",
        error: "warning",
      },
    }, ctx);
    mapCodexEventToNdjson({
      type: "item.completed",
      item: {
        id: "mcp-1",
        type: "mcpToolCall",
        server: "docs",
        tool: "search",
        arguments: { query: "timeline" },
        input: { query: "timeline" },
        result: { hits: 1 },
        error: null,
      },
    }, ctx);

    const events = json().filter((line) => line.type === "timeline").map((line) => line.event);
    expect(events[0]).toMatchObject({
      kind: "file_change",
      payload: {
        path: "src/App.vue",
        changes: [{ kind: "update", path: "src/App.vue" }],
        grantRoot: "C:/repo",
        output: "patched",
        error: "warning",
      },
    });
    expect(events[1]).toMatchObject({
      kind: "mcp",
      payload: {
        server: "docs",
        tool: "search",
        arguments: { query: "timeline" },
        input: { query: "timeline" },
        result: { hits: 1 },
        error: null,
      },
    });
  });

  it("ignores malformed Codex approval requests without throwing", async () => {
    const calls: any[] = [];
    const handled = await maybeHandleCodexApprovalRequest(
      {
        respond: (...args: any[]) => calls.push(args),
      },
      { id: "bad-request", params: {} },
      {
        interactions: {
          requestUserConsent: async () => {
            throw new Error("should not ask");
          },
        },
        emitToolConsentTimeline: () => {},
      },
    );

    expect(handled).toBe(false);
    expect(calls).toEqual([]);
  });

  it("tracks Codex elicitation around UI waits and cleans up on errors", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ method, params });
        return {};
      },
    };

    await expect(withCodexElicitation(
      server as any,
      "thread-1",
      { protocol } as any,
      "ask_user",
      async () => "ok",
    )).resolves.toBe("ok");

    await expect(withCodexElicitation(
      server as any,
      "thread-1",
      { protocol } as any,
      "plan_approval",
      async () => {
        throw new Error("user flow failed");
      },
    )).rejects.toThrow("user flow failed");

    expect(calls).toEqual([
      { method: "thread/increment_elicitation", params: { threadId: "thread-1" } },
      { method: "thread/decrement_elicitation", params: { threadId: "thread-1" } },
      { method: "thread/increment_elicitation", params: { threadId: "thread-1" } },
      { method: "thread/decrement_elicitation", params: { threadId: "thread-1" } },
    ]);
    expect(json().filter((line) => line.type === "timeline")).toEqual([]);
  });

  it("continues Codex elicitation when increment fails and skips decrement", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/increment_elicitation") {
          throw new Error("unsupported");
        }
        return {};
      },
    };

    await expect(withCodexElicitation(
      server as any,
      "thread-1",
      { protocol } as any,
      "tool_consent",
      async () => "continued",
    )).resolves.toBe("continued");

    expect(calls).toEqual([
      { method: "thread/increment_elicitation", params: { threadId: "thread-1" } },
    ]);
    expect(json()).toEqual([
      {
        type: "timeline",
        event: expect.objectContaining({
          kind: "diagnostic",
          status: "error",
          payload: expect.objectContaining({
            backend: "codex",
            method: "thread/increment_elicitation",
            interactionKind: "tool_consent",
            error: "unsupported",
          }),
        }),
      },
    ]);
  });

  it("Codex Agent 提问通过统一 interaction_request/response 往返", async () => {
    const { protocol, json } = captureProtocol();
    const broker = createInteractionBroker({
      protocol,
      emitToolConsentTimeline: () => {},
      emitAskUserTimeline: () => {},
    });
    const calls: any[] = [];
    const elicitationCalls: any[] = [];

    const handled = maybeHandleCodexServerRequest(
      {
        respond: (...args: any[]) => calls.push(["respond", ...args]),
      } as any,
      {
        id: "request-user-input-1",
        method: "item/tool/requestUserInput",
        params: {
          questions: [{
            id: "choice",
            header: "方案",
            question: "选哪个方案？",
            options: [
              { label: "A", description: "先改合约" },
              { label: "B", description: "先改 UI" },
            ],
          }],
        },
      },
      {
        interactions: broker,
        withCodexElicitation: trackedElicitation(elicitationCalls),
      } as any,
    );

    await waitUntil(() => json().some((line) => line.type === "interaction_request"));
    expect(json()).toEqual([
      {
        type: "interaction_request",
        id: "ask-1",
        kind: "ask_user",
        backend: "codex",
        payload: expect.objectContaining({
          title: "Codex 想确认一下",
          source: "Codex",
          questions: [expect.objectContaining({ id: "choice" })],
        }),
      },
    ]);

    broker.handleControlLine(JSON.stringify({
      type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
      id: "ask-1",
      kind: "ask_user",
      result: {
        cancelled: false,
        answers: {
          choice: { questionId: "choice", value: "B" },
        },
      },
    }));

    await expect(handled).resolves.toBe(true);
    expect(elicitationCalls).toEqual([
      ["increment", "ask_user"],
      ["decrement", "ask_user"],
    ]);
    expect(calls).toEqual([
      ["respond", "request-user-input-1", { answers: { choice: { answers: ["B"] } } }],
    ]);
  });

  it("Codex dynamic AskUser tool also marks elicitation", async () => {
    const calls: any[] = [];
    const elicitationCalls: any[] = [];
    let requestedAskUserSpec: any = null;
    const handled = await maybeHandleCodexServerRequest(
      {
        respond: (...args: any[]) => calls.push(["respond", ...args]),
      } as any,
      {
        id: "ask-tool-1",
        method: "item/tool/call",
        params: {
          tool: "AskUserQuestion",
          arguments: {
            questions: [{
              id: "choice",
              header: "方案",
              question: "选哪个方案？",
              options: [
                { label: "A", description: "先改合约" },
                { label: "B", description: "先改 UI" },
              ],
            }],
          },
        },
      },
      {
        interactions: {
          requestAskUser: async (spec: any) => {
            requestedAskUserSpec = spec;
            return {
              cancelled: false,
              answers: {
                "q-1": { questionId: "q-1", value: "o-2" },
              },
            };
          },
        },
        withCodexElicitation: trackedElicitation(elicitationCalls),
      } as any,
    );

    expect(handled).toBe(true);
    expect(elicitationCalls).toEqual([
      ["increment", "ask_user"],
      ["decrement", "ask_user"],
    ]);
    expect(requestedAskUserSpec?.questions[0]?.mode).toBe(ASK_USER_SINGLE_SELECT_MODE);
    expect(calls[0][1]).toBe("ask-tool-1");
    expect(calls[0][2]).toMatchObject({ success: true });
    const output = JSON.parse(calls[0][2].contentItems[0].text);
    expect(output).toMatchObject({
      cancelled: false,
      answers: {
        "选哪个方案？": "B",
      },
    });
  });

  it("Codex quota tool delegates to Lilia internal quota plugin", async () => {
    const calls: any[] = [];
    const quotaCalls: any[] = [];
    const handled = await maybeHandleCodexServerRequest(
      {
        respond: (...args: any[]) => calls.push(["respond", ...args]),
      } as any,
      {
        id: "quota-1",
        method: "item/tool/call",
        params: {
          tool: "QueryQuotaUsage",
          arguments: {
            days: 30,
            backend: "codex",
            scope: "tools",
          },
        },
      },
      {
        interactions: {
          requestQuotaUsage: async (payload: any) => {
            quotaCalls.push(payload);
            return {
              ok: true,
              result: {
                days: payload.days,
                backend: payload.backend,
                tools: [{ key: "command::", label: "命令", callCount: 2 }],
              },
            };
          },
        },
      } as any,
    );

    expect(handled).toBe(true);
    expect(quotaCalls).toEqual([{ days: 30, backend: "codex", scope: "tools" }]);
    expect(calls[0][1]).toBe("quota-1");
    expect(calls[0][2]).toMatchObject({ success: true });
    const output = JSON.parse(calls[0][2].contentItems[0].text);
    expect(output.tools[0]).toMatchObject({ label: "命令", callCount: 2 });
  });

  function mockQuotaServer(
    read: (attempt: number) => unknown,
    {
      usage = () => ({
        summary: {
          lifetimeTokens: 123456,
          peakDailyTokens: 4567,
          longestRunningTurnSec: 540,
          currentStreakDays: 8,
          longestStreakDays: 14,
        },
        dailyUsageBuckets: [{ startDate: "2026-06-18", tokens: 1234 }],
      }),
      consume = () => ({ outcome: "reset" }),
    }: {
      usage?: () => unknown;
      consume?: () => unknown;
    } = {},
  ): { server: any; requests: string[] } {
    const requests: string[] = [];
    let readAttempts = 0;
    return {
      requests,
      server: {
        request: async (method: string) => {
          requests.push(method);
          if (method === "initialize") return {};
          if (method === "account/rateLimits/read") {
            readAttempts += 1;
            return read(readAttempts);
          }
          if (method === "account/usage/read") return usage();
          if (method === "account/rateLimitResetCredit/consume") return consume();
          throw new Error(`unexpected method ${method}`);
        },
        notify: () => {},
        close: () => {},
      },
    };
  }

  it("Codex account quota reads Spark quota from named Codex rate limit", async () => {
    const { server, requests } = mockQuotaServer(() => ({
      rateLimitsByLimitId: {
        codex: {
          limitId: "codex",
          planType: "pro",
          credits: { hasCredits: true, unlimited: false, balance: "3" },
          primary: { usedPercent: 14, windowDurationMins: 300, resetsAt: 10 },
          secondary: { usedPercent: 14, windowDurationMins: 10080, resetsAt: 20 },
        },
        codex_bengalfox: {
          limitId: "codex_bengalfox",
          limitName: "GPT-5.3-Codex-Spark",
          planType: "pro",
          credits: { hasCredits: true, unlimited: true, balance: null },
          primary: { usedPercent: 0, windowDurationMins: 300, resetsAt: 30 },
          secondary: { usedPercent: 18, windowDurationMins: 10080, resetsAt: 40 },
        },
      },
    }));

    const status = await readCodexAccountQuotaStatus({
      createServer: () => server as any,
    });

    expect(requests).toEqual(["initialize", "account/rateLimits/read", "account/usage/read"]);
    expect(status.fiveHour?.usedPercent).toBe(14);
    expect(status.weekly?.usedPercent).toBe(14);
    expect(status.sparkFiveHour?.usedPercent).toBe(0);
    expect(status.sparkWeekly?.usedPercent).toBe(18);
    expect(status.credits).toEqual({ hasCredits: true, unlimited: false, balance: "3" });
    expect(status.sparkCredits).toEqual({ hasCredits: true, unlimited: true, balance: null });
    expect(status.accountUsage?.summary.currentStreakDays).toBe(8);
  });

  it("Codex account quota utility returns structured JSON when Codex CLI is missing", async () => {
    const liliaHome = join(tmpdir(), `lilia-quota-utility-${process.pid}-${Date.now()}`);
    await mkdir(liliaHome, { recursive: true });
    try {
      const result = spawnSync(process.execPath, [codexAccountQuotaUtility], {
        cwd: join(testsDir, ".."),
        env: {
          ...process.env,
          LILIA_HOME: liliaHome,
          OPENAI_BASE_URL: "",
          OPENAI_API_KEY: "",
          CODEX_API_KEY: "",
        },
        encoding: "utf8",
        windowsHide: true,
      });
      expect(result.stdout.trim()).not.toBe("");
      const payload = JSON.parse(result.stdout.trim().split(/\r?\n/)[0]);
      expect(payload).toMatchObject({
        available: false,
        connectionMode: "codex-account",
        fiveHour: null,
        weekly: null,
      });
      expect(payload.error).toContain("Codex app-server");
    } finally {
      await rm(liliaHome, { recursive: true, force: true });
    }
  });

  it("Codex account quota retries transient wham usage fetch failures", async () => {
    const { server, requests } = mockQuotaServer((attempt) => {
      if (attempt === 1) {
        throw new Error(
          "failed to fetch codex rate limits: error sending request for url (https://chatgpt.com/backend-api/wham/usage)",
        );
      }
      return {
        rateLimitsByLimitId: {
          codex: {
            limitId: "codex",
            planType: "pro",
            rateLimitResetCredits: { availableCount: 99 },
            primary: { usedPercent: 12, windowDurationMins: 300, resetsAt: 10 },
            secondary: { usedPercent: 34, windowDurationMins: 10080, resetsAt: 20 },
          },
        },
        rateLimitResetCredits: { availableCount: 2 },
      };
    });

    const status = await readCodexAccountQuotaStatus({
      createServer: () => server as any,
      retries: 1,
      retryDelayMs: 0,
    });

    expect(requests).toEqual([
      "initialize",
      "account/rateLimits/read",
      "account/rateLimits/read",
      "account/usage/read",
    ]);
    expect(status.error).toBeNull();
    expect(status.fiveHour?.usedPercent).toBe(12);
    expect(status.weekly?.usedPercent).toBe(34);
    expect(status.credits).toBeNull();
    expect(status.sparkCredits).toBeNull();
    expect(status.rateLimitResetCredits).toEqual({ availableCount: 2 });
  });

  it("Codex account quota keeps rate limits when account usage is unavailable", async () => {
    const { server, requests } = mockQuotaServer(
      () => ({
        rateLimitsByLimitId: {
          codex: {
            limitId: "codex",
            planType: "pro",
            primary: { usedPercent: 12, windowDurationMins: 300, resetsAt: 10 },
          },
        },
      }),
      {
        usage: () => {
          throw new Error("usage unsupported");
        },
      },
    );

    const status = await readCodexAccountQuotaStatus({
      createServer: () => server as any,
    });

    expect(requests).toEqual(["initialize", "account/rateLimits/read", "account/usage/read"]);
    expect(status.available).toBe(true);
    expect(status.accountUsage).toBeNull();
    expect(status.usageError).toBe("usage unsupported");
  });

  for (const outcome of ["reset", "alreadyRedeemed", "nothingToReset", "noCredit"]) {
    it(`Codex account quota consume reset credit handles ${outcome}`, async () => {
      const { server, requests } = mockQuotaServer(
        () => ({
          rateLimitsByLimitId: {
            codex: {
              limitId: "codex",
              planType: "pro",
              primary: { usedPercent: 1, windowDurationMins: 300, resetsAt: 10 },
            },
          },
          rateLimitResetCredits: { availableCount: 1 },
        }),
        {
          consume: () => ({ outcome }),
        },
      );

      const result = await consumeCodexRateLimitResetCredit("key-1", {
        createServer: () => server as any,
      });

      expect(requests).toEqual([
        "initialize",
        "account/rateLimitResetCredit/consume",
        "account/rateLimits/read",
        "account/usage/read",
      ]);
      expect(result.outcome).toBe(outcome);
      expect(result.status.fiveHour?.usedPercent).toBe(1);
      expect(result.status.rateLimitResetCredits).toEqual({ availableCount: 1 });
    });
  }

  it("Codex 子对话工具调用可查询父对话上下文", async () => {
    const calls: any[] = [];
    const handled = await maybeHandleCodexServerRequest(
      {
        respond: (...args: any[]) => calls.push(["respond", ...args]),
      } as any,
      {
        id: "query-context-1",
        method: "item/tool/call",
        params: {
          tool: "QueryConversationContext",
          arguments: {},
        },
      },
      {
        cmd: {
          conversationContext: {
            currentTaskId: "child-1",
            parentTaskId: "parent-1",
            tasks: [{
              taskId: "parent-1",
              title: "父对话",
              messages: [{ role: "user", content: "父问题", createdAt: 1 }],
            }],
          },
        },
        interactions: {
          requestAskUser: async () => {
            throw new Error("should not ask");
          },
        },
      } as any,
    );

    expect(handled).toBe(true);
    expect(calls).toHaveLength(1);
    expect(calls[0][1]).toBe("query-context-1");
    expect(calls[0][2]).toMatchObject({
      success: true,
      contentItems: [expect.objectContaining({ type: "inputText" })],
    });
    const output = JSON.parse(calls[0][2].contentItems[0].text);
    expect(output).toMatchObject({
      ok: true,
      currentTaskId: "child-1",
      parentTaskId: "parent-1",
      task: {
        taskId: "parent-1",
        messages: [{ role: "user", content: "父问题" }],
      },
    });
  });

  it("Codex MCP elicitation 通过统一 interaction_request/response 往返", async () => {
    const { protocol, json } = captureProtocol();
    const broker = createInteractionBroker({
      protocol,
      emitToolConsentTimeline: () => {},
      emitAskUserTimeline: () => {},
    });
    const calls: any[] = [];
    const elicitationCalls: any[] = [];

    const handled = maybeHandleCodexServerRequest(
      {
        respond: (...args: any[]) => calls.push(["respond", ...args]),
      } as any,
      {
        id: "mcp-elicit-1",
        method: "mcpServer/elicitation/request",
        params: {
          threadId: "thread-1",
          turnId: "turn-1",
          serverName: "linear",
          mode: "form",
          message: "选择项目",
          requestedSchema: {
            type: "object",
            properties: {
              project: {
                type: "string",
                title: "项目",
                enum: ["A", "B"],
              },
            },
            required: ["project"],
          },
          _meta: { source: "test" },
        },
      },
      {
        protocol,
        interactions: broker,
        withCodexElicitation: trackedElicitation(elicitationCalls),
      } as any,
    );

    await waitUntil(() => json().some((line) => line.type === "interaction_request"));
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "mcp",
        status: "requires_action",
        payload: expect.objectContaining({
          interaction: "mcp_elicitation",
          requestId: "codex-1",
          serverName: "linear",
          mode: "form",
        }),
      }),
    }));
    expect(json()).toContainEqual(expect.objectContaining({
      type: "interaction_request",
      id: "codex-1",
      kind: "mcp_elicitation",
      backend: "codex",
      payload: expect.objectContaining({
        threadId: "thread-1",
        turnId: "turn-1",
        serverName: "linear",
        mode: "form",
        message: "选择项目",
      }),
    }));

    broker.handleControlLine(JSON.stringify({
      type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
      id: "codex-1",
      kind: "mcp_elicitation",
      result: {
        action: "accept",
        content: { project: "B" },
      },
    }));

    await expect(handled).resolves.toBe(true);
    expect(elicitationCalls).toEqual([
      ["increment", "mcp_elicitation"],
      ["decrement", "mcp_elicitation"],
    ]);
    expect(calls).toEqual([
      ["respond", "mcp-elicit-1", {
        action: "accept",
        content: { project: "B" },
        _meta: null,
      }],
    ]);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "mcp",
        status: "success",
        payload: expect.objectContaining({
          interaction: "mcp_elicitation",
          requestId: "codex-1",
          result: expect.objectContaining({ action: "accept" }),
        }),
      }),
    }));
  });

  it("Codex permission approval 通过统一 interaction_request/response 往返", async () => {
    const { protocol, json } = captureProtocol();
    const broker = createInteractionBroker({
      protocol,
      emitToolConsentTimeline: () => {},
      emitAskUserTimeline: () => {},
    });
    const calls: any[] = [];
    const elicitationCalls: any[] = [];

    const handled = maybeHandleCodexServerRequest(
      {
        respond: (...args: any[]) => calls.push(["respond", ...args]),
      } as any,
      {
        id: "permissions-1",
        method: "item/permissions/requestApproval",
        params: {
          threadId: "thread-1",
          turnId: "turn-1",
          itemId: "item-1",
          startedAtMs: 123,
          cwd: "C:/repo",
          reason: "need network",
          permissions: {
            network: { domains: [{ domain: "example.com" }] },
            fileSystem: null,
          },
        },
      },
      {
        protocol,
        interactions: broker,
        withCodexElicitation: trackedElicitation(elicitationCalls),
      } as any,
    );

    await waitUntil(() => json().some((line) => line.type === "interaction_request"));
    expect(json()).toContainEqual(expect.objectContaining({
      type: "interaction_request",
      id: "codex-1",
      kind: "permission_approval",
      backend: "codex",
      payload: expect.objectContaining({
        reason: "need network",
        requestedAccess: expect.objectContaining({
          network: { domains: [{ domain: "example.com" }] },
        }),
        providerContext: expect.objectContaining({
          codex: expect.objectContaining({
            threadId: "thread-1",
            turnId: "turn-1",
            itemId: "item-1",
            cwd: "C:/repo",
          }),
        }),
      }),
    }));

    broker.handleControlLine(JSON.stringify({
      type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
      id: "codex-1",
      kind: "permission_approval",
      result: {
        grantedAccess: {
          network: { domains: [{ domain: "example.com" }] },
        },
        scope: "session",
      },
    }));

    await expect(handled).resolves.toBe(true);
    expect(elicitationCalls).toEqual([
      ["increment", "permission_approval"],
      ["decrement", "permission_approval"],
    ]);
    expect(calls).toEqual([
      ["respond", "permissions-1", {
        permissions: {
          network: { domains: [{ domain: "example.com" }] },
        },
        scope: "session",
      }],
    ]);
  });

  it("Codex 工具确认通过统一 interaction_request/response 往返", async () => {
    const { protocol, json } = captureProtocol();
    const broker = createInteractionBroker({
      protocol,
      emitToolConsentTimeline: () => {},
      emitAskUserTimeline: () => {},
    });
    const calls: any[] = [];
    const elicitationCalls: any[] = [];

    const handled = maybeHandleCodexApprovalRequest(
      {
        respond: (...args: any[]) => calls.push(["respond", ...args]),
      },
      {
        id: "approval-rpc-unified",
        method: "item/commandExecution/requestApproval",
        params: {
          approvalId: "approval-unified",
          command: "yarn test",
          cwd: "C:/repo",
          availableDecisions: ["accept", "decline", "cancel"],
        },
      },
      {
        protocol,
        interactions: broker,
        emitToolConsentTimeline: () => {},
        withCodexElicitation: trackedElicitation(elicitationCalls),
      },
    );

    await waitUntil(() => json().some((line) => line.type === "interaction_request"));
    expect(json()).toEqual([
      {
        type: "interaction_request",
        id: "consent-1",
        kind: "tool_consent",
        backend: "codex",
        payload: expect.objectContaining({
          backend: "codex",
          toolName: "item/commandExecution/requestApproval",
          input: expect.objectContaining({ command: "yarn test" }),
          toolUseID: "approval-unified",
        }),
      },
    ]);

    broker.handleControlLine(JSON.stringify({
      type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
      id: "consent-1",
      kind: "tool_consent",
      result: {
        decision: "allow",
        message: "",
      },
    }));

    await expect(handled).resolves.toBe(true);
    expect(elicitationCalls).toEqual([
      ["increment", "tool_consent"],
      ["decrement", "tool_consent"],
    ]);
    expect(calls).toEqual([
      ["respond", "approval-rpc-unified", { decision: "accept" }],
    ]);
  });

  it("passes Codex approval fields through for readonly runs and honors codexDecision first", async () => {
    const calls: any[] = [];
    let seenPayload: any = null;
    const handled = await maybeHandleCodexApprovalRequest(
      {
        respond: (...args: any[]) => calls.push(["respond", ...args]),
      },
      {
        id: "approval-rpc-1",
        method: "item/fileChange/requestApproval",
        params: {
          approvalId: "approval-1",
          grantRoot: "C:/repo",
          additionalPermissions: [{ permission: "filesystem" }],
          availableDecisions: ["accept", "decline", "cancel"],
          proposedExecpolicyAmendment: { sandbox: "workspace-write" },
          proposedNetworkPolicyAmendments: [{ host: "example.com" }],
          networkApprovalContext: { host: "example.com" },
          cwd: "C:/repo",
          reason: "needs write",
          commandActions: [{ text: "ignored" }],
        },
      },
      {
        executionPermission: "readonly",
        interactions: {
          requestUserConsent: async (payload: any) => {
            seenPayload = payload;
            return {
              id: "consent-1",
              decision: "deny",
              message: "use explicit",
              codexDecision: "cancel",
            };
          },
        },
        emitToolConsentTimeline: (...args: any[]) => calls.push(["timeline", ...args]),
      },
    );

    expect(handled).toBe(true);
    expect(seenPayload).toMatchObject({
      backend: "codex",
      toolName: "item/fileChange/requestApproval",
      additionalPermissions: [{ permission: "filesystem" }],
      availableDecisions: ["accept", "decline", "cancel"],
      proposedExecpolicyAmendment: { sandbox: "workspace-write" },
      proposedNetworkPolicyAmendments: [{ host: "example.com" }],
      networkApprovalContext: { host: "example.com" },
      cwd: "C:/repo",
      reason: "needs write",
      commandActions: [{ text: "ignored" }],
    });
    expect(calls[0]).toEqual(["respond", "approval-rpc-1", { decision: "cancel" }]);
    expect(calls[1]).toMatchObject([
      "timeline",
      "consent-1",
      seenPayload,
      "cancelled",
      "use explicit",
    ]);
  });

  it("uses Codex cancel when a denied approval cannot decline", async () => {
    const calls: any[] = [];
    const seenTimeline: any[] = [];
    const handled = await maybeHandleCodexApprovalRequest(
      {
        respond: (...args: any[]) => calls.push(["respond", ...args]),
      },
      {
        id: "approval-rpc-cancel-only",
        method: "item/fileChange/requestApproval",
        params: {
          approvalId: "approval-cancel-only",
          grantRoot: "C:/repo",
          availableDecisions: ["accept", "cancel"],
        },
      },
      {
        interactions: {
          requestUserConsent: async () => ({
            id: "consent-cancel-only",
            decision: "deny",
            message: "暂不授权",
          }),
        },
        emitToolConsentTimeline: (...args: any[]) => seenTimeline.push(args),
      },
    );

    expect(handled).toBe(true);
    expect(calls).toEqual([
      ["respond", "approval-rpc-cancel-only", { decision: "cancel" }],
    ]);
    expect(seenTimeline[0]).toMatchObject([
      "consent-cancel-only",
      expect.objectContaining({ availableDecisions: ["accept", "cancel"] }),
      "cancelled",
      "暂不授权",
    ]);
  });

  it("runs edited Codex command approvals through Lilia and steers the result back", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = {
      request: async (method: string, params: any) => {
        calls.push(["request", method, params]);
        if (method === "command/exec") {
          return { exitCode: 0, stdout: "tests passed\n", stderr: "" };
        }
        if (method === "turn/steer") return {};
        throw new Error(`unexpected request ${method}`);
      },
      respond: (...args: any[]) => calls.push(["respond", ...args]),
      drainNotifications: () => [],
    };
    const consentTimeline: any[] = [];

    const handled = await maybeHandleCodexApprovalRequest(
      server as any,
      {
        id: "approval-rpc-2",
        method: "item/commandExecution/requestApproval",
        params: {
          approvalId: "approval-2",
          command: "yarn test",
          cwd: "C:/repo",
          availableDecisions: ["accept", "decline", "cancel"],
          proposedExecpolicyAmendment: { sandbox: "workspace-write" },
        },
      },
      {
        protocol,
        threadId: "thread-1",
        currentTurnId: "turn-1",
        executionPermission: "ask",
        interactions: {
          requestUserConsent: async () => ({
            id: "consent-2",
            decision: "allow",
            message: "",
            updatedInput: { command: "yarn test --runInBand" },
          }),
        },
        emitToolConsentTimeline: (...args: any[]) => consentTimeline.push(args),
      },
    );

    expect(handled).toBe(true);
    expect(calls).toEqual([
      ["request", "command/exec", {
        command: "yarn test --runInBand",
        cwd: "C:/repo",
        permissionProfile: ":workspace",
      }],
      ["respond", "approval-rpc-2", { decision: "cancel" }],
      ["request", "turn/steer", expect.objectContaining({
        threadId: "thread-1",
        turnId: "turn-1",
        additionalContext: expect.stringContaining("yarn test --runInBand"),
      })],
    ]);
    expect(calls[2][2].additionalContext).toContain("退出码：0");
    expect(calls[2][2].additionalContext).toContain("tests passed");
    expect(json()).toEqual([
      {
        type: "timeline",
        event: expect.objectContaining({
          kind: "command",
          status: "success",
          title: "yarn test --runInBand",
          payload: expect.objectContaining({
            executionOwner: "lilia",
            subkind: "lilia_edit_exec",
            originalCommand: "yarn test",
            modifiedCommand: "yarn test --runInBand",
            exitCode: 0,
          }),
        }),
      },
    ]);
    expect(consentTimeline[0]).toMatchObject([
      "consent-2",
      expect.objectContaining({
        commandEdited: true,
        originalCommand: "yarn test",
        modifiedCommand: "yarn test --runInBand",
      }),
      "success",
      "Lilia 已执行用户修改后的 Codex 命令",
    ]);
  });

  it("emits an error timeline and declines the original Codex command when Lilia cannot execute the edit", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const consentTimeline: any[] = [];
    const server = {
      request: async (method: string, params: any) => {
        calls.push(["request", method, params]);
        throw new Error(`${method} unavailable`);
      },
      respond: (...args: any[]) => calls.push(["respond", ...args]),
      drainNotifications: () => [],
    };

    await maybeHandleCodexApprovalRequest(
      server as any,
      {
        id: "approval-rpc-4",
        method: "item/commandExecution/requestApproval",
        params: {
          approvalId: "approval-4",
          command: "pnpm test",
          availableDecisions: ["accept", "decline"],
        },
      },
      {
        protocol,
        threadId: "thread-1",
        currentTurnId: "turn-1",
        interactions: {
          requestUserConsent: async () => ({
            id: "consent-4",
            decision: "allow",
            updatedInput: { command: "pnpm test --changed" },
          }),
        },
        emitToolConsentTimeline: (...args: any[]) => consentTimeline.push(args),
      },
    );

    expect(calls).toEqual([
      ["request", "command/exec", {
        command: "pnpm test --changed",
        cwd: undefined,
        permissionProfile: ":read-only",
      }],
      ["request", "process/spawn", {
        command: "pnpm test --changed",
        cwd: undefined,
        permissionProfile: ":read-only",
      }],
      ["respond", "approval-rpc-4", { decision: "decline" }],
    ]);
    expect(json()).toEqual([
      {
        type: "timeline",
        event: expect.objectContaining({
          kind: "command",
          status: "error",
          payload: expect.objectContaining({
            executionOwner: "lilia",
            subkind: "lilia_edit_exec",
            modifiedCommand: "pnpm test --changed",
          }),
        }),
      },
    ]);
    expect(consentTimeline).toEqual([]);
  });

  it("steers Lilia IAB snapshots back into the active Codex turn", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "turn/steer") return {};
        throw new Error(`unexpected request ${method}`);
      },
    };
    const ctx: any = {
      protocol,
      server,
      threadId: "thread-1",
      currentTurnId: "turn-1",
    };

    await handleLiliaIabResult(ctx, {
      taskId: "task-1",
      url: "https://example.com/debug",
      title: "Debug Page",
      note: "button broken",
      capturedAt: 1,
      screenshotPath: "C:/shot.png",
      status: "captured",
    });

    expect(json()).toEqual([
      {
        type: "timeline",
        event: expect.objectContaining({
          kind: "tool",
          status: "success",
          title: "Lilia IAB snapshot",
          payload: expect.objectContaining({
            backend: "codex",
            subkind: "lilia_iab",
            url: "https://example.com/debug",
            title: "Debug Page",
            note: "button broken",
            screenshotPath: "C:/shot.png",
            status: "captured",
          }),
        }),
      },
    ]);
    expect(calls).toEqual([
      {
        method: "turn/steer",
        params: {
          threadId: "thread-1",
          turnId: "turn-1",
          additionalContext: expect.stringContaining("https://example.com/debug"),
        },
      },
    ]);
    expect(calls[0].params.additionalContext).toContain("Debug Page");
    expect(calls[0].params.additionalContext).toContain("button broken");
    expect(calls[0].params.additionalContext).toContain("C:/shot.png");
  });

  it("resume thread still registers Lilia dynamic tools without app-server plan-tool params", async () => {
    const calls: any[] = [];
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ method, params });
        return { thread: { id: "thread-1" }, model: "gpt-5.1" };
      },
    };

    await startCodexAppServerThread(server as any, {
      resumeSessionId: "thread-1",
      permission: "ask",
      planMode: true,
    }, () => "C:/repo");

    expect(calls[0]).toMatchObject({
      method: "thread/resume",
      params: {
        threadId: "thread-1",
        dynamicTools: expect.arrayContaining([
          expect.objectContaining({ name: "AskUserQuestion" }),
          expect.objectContaining({ name: "QueryQuotaUsage" }),
        ]),
      },
    });
    expect(calls[0].params.includePlanTool).toBeUndefined();
  });

  it("syncs Codex thread history only when resuming a thread", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/turns/list") {
          return {
            data: [{
              id: "turn-old",
              status: "completed",
              startedAt: 10,
              completedAt: 11,
              items: [{ type: "agentMessage", id: "msg-1", text: "旧回复" }],
            }],
            nextCursor: null,
            backwardsCursor: null,
          };
        }
        return {};
      },
    };

    await expect(syncCodexThreadHistory(
      server as any,
      "thread-1",
      { resumeSessionId: "thread-1" },
      protocol,
    )).resolves.toEqual({ ok: true, skipped: false, count: 1 });
    await expect(syncCodexThreadHistory(
      server as any,
      "thread-2",
      {},
      protocol,
    )).resolves.toEqual({ ok: true, skipped: true, count: 0 });

    expect(calls).toEqual([{
      method: "thread/turns/list",
      params: {
        threadId: "thread-1",
        limit: 50,
        sortDirection: "asc",
        itemsView: "full",
      },
    }]);
    expect(json()).toEqual([
      {
        type: "timeline",
        event: expect.objectContaining({
          kind: "message",
          sourceId: "codex-history:thread-1:turn-old:msg-1",
          turnIdOverride: "turn-old",
          createdAt: 10_000,
          updatedAt: 11_000,
          payload: expect.objectContaining({
            history: true,
            threadId: "thread-1",
            turnId: "turn-old",
            itemId: "msg-1",
          }),
        }),
      },
      {
        type: "timeline",
        event: expect.objectContaining({
          kind: "diagnostic",
          status: "info",
          title: "Codex history synced",
        }),
      },
    ]);
  });

  it("continues Codex run when history sync fails", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/resume") return { thread: { id: "thread-1" }, model: "gpt-5.1" };
        if (method === "thread/turns/list") throw new Error("history unavailable");
        if (method === "turn/start") return { turn: { id: "turn-new" } };
        return {};
      },
      {
        drainNotifications: () => [{
          method: "turn/completed",
          params: { threadId: "thread-1", turn: { status: "completed" } },
        }],
      },
    );

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "hi",
      resumeSessionId: "thread-1",
      permission: "ask",
      planMode: false,
    });

    const historyIndex = calls.findIndex((call) => call.method === "thread/turns/list");
    const turnIndex = calls.findIndex((call) => call.method === "turn/start");
    expect(historyIndex).toBeGreaterThan(-1);
    expect(historyIndex).toBeLessThan(turnIndex);
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "error" &&
      line.event.title === "Codex history sync failed"
    )).toBe(true);
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-1")).toBe(true);
  });

  it("applies Codex sticky settings before the first turn", async () => {
    const { protocol } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "turn/start") return { turn: { id: "turn-1" } };
        return {};
      },
      {
        drainNotifications: () => [{
          method: "turn/completed",
          params: { threadId: "thread-1", turn: { status: "completed" } },
        }],
      },
    );

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "hi",
      permission: "ask",
      planMode: false,
      runtimeOptions: {
        provider: {
          codex: {
            model: "gpt-5.4-mini",
            reasoningEffort: "high",
            runtimeWorkspaceRoots: ["C:/repo", "D:/shared"],
            responsesApiClientMetadata: { surface: "lilia-test" },
            additionalContext: "extra context",
            persistExtendedHistory: true,
            initialTurnsPage: { limit: 20 },
            excludeTurns: ["turn-old", "turn-old", ""],
          },
        },
      },
    });

    const updateIndex = calls.findIndex((call) => call.method === "thread/settings/update");
    const turnIndex = calls.findIndex((call) => call.method === "turn/start");
    expect(updateIndex).toBeGreaterThan(-1);
    expect(updateIndex).toBeLessThan(turnIndex);
    expect(calls[updateIndex].params).toMatchObject({
      threadId: "thread-1",
      model: "gpt-5.4-mini",
      reasoningEffort: "high",
      effort: "high",
      runtimeWorkspaceRoots: ["C:/repo", "D:/shared"],
      permissions: ":read-only",
      persistExtendedHistory: true,
    });
    expect(calls[updateIndex].params.collaborationMode).toBeUndefined();
    const startCall = calls.find((call) => call.method === "turn/start");
    expect(startCall.params).toMatchObject({
      responsesapiClientMetadata: { surface: "lilia-test" },
      additionalContext: "extra context",
      runtimeWorkspaceRoots: ["C:/repo", "D:/shared"],
      permissions: ":read-only",
    });
    expect(startCall.params.initialTurnsPage).toBeUndefined();
    expect(startCall.params.excludeTurns).toBeUndefined();
  });

  it("passes Codex resume-only advanced fields to thread/resume", async () => {
    const { protocol } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/resume") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/turns/list") return { data: [] };
        if (method === "turn/start") return { turn: { id: "turn-1" } };
        return {};
      },
      {
        drainNotifications: () => [{
          method: "turn/completed",
          params: { threadId: "thread-1", turn: { status: "completed" } },
        }],
      },
    );

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "resume",
      permission: "ask",
      planMode: false,
      resumeSessionId: "thread-1",
      runtimeOptions: {
        provider: {
          codex: {
            persistExtendedHistory: false,
            initialTurnsPage: { cursor: "cursor-1", limit: 10 },
            excludeTurns: ["turn-a", "turn-a", " turn-b "],
          },
        },
      },
    });

    expect(calls.find((call) => call.method === "thread/resume").params).toMatchObject({
      threadId: "thread-1",
      persistExtendedHistory: false,
      initialTurnsPage: { cursor: "cursor-1", limit: 10 },
      excludeTurns: ["turn-a", "turn-b"],
    });
  });

  it("starts Codex review workflow through review/start", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "review/start") return { turn: { id: "review-turn-1" } };
        return {};
      },
      {
        drainNotifications: () => [{
          method: "turn/completed",
          params: { threadId: "thread-1", turn: { status: "completed" } },
        }],
      },
    );

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "重点看权限边界",
      permission: "ask",
      planMode: false,
      workflow: {
        type: LILIA_REVIEW_WORKFLOW_TYPE,
        target: { type: "baseBranch", branch: "main" },
        instructions: "重点看权限边界",
      },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(calls.find((call) => call.method === "review/start")).toMatchObject({
      params: {
        threadId: "thread-1",
        target: { type: "baseBranch", branch: "main" },
        delivery: "inline",
      },
    });
    expect(calls.find((call) => call.method === "review/start").params).not.toHaveProperty("prompt");
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.title === "Codex review started" &&
      line.event.payload.hasInstructions === true
    )).toBe(true);
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-1")).toBe(true);
  });

  it("starts Codex fix suggestion workflow through turn/start in suggest mode", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = createCodexTurnTestServer(calls);

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "重点看权限边界",
      permission: "ask",
      planMode: false,
      workflow: {
        type: LILIA_FIX_SUGGESTION_WORKFLOW_TYPE,
        target: { type: "baseBranch", branch: "main" },
        instructions: "重点看权限边界",
      },
    });

    const startupEvent = json().find((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.title === "Codex runtime starting"
    );
    expect(startupEvent).toMatchObject({
      event: {
        status: "info",
        sourceId: "codex:runtime:start",
        payload: {
          backend: "codex",
          subkind: "runtime_start",
        },
      },
    });
    expect(calls.some((call) => call.method === "review/start")).toBe(false);
    expect(calls.findIndex((call) => call.method === "thread/settings/update"))
      .toBeLessThan(calls.findIndex((call) => call.method === "turn/start"));
    const turnStart = calls.find((call) => call.method === "turn/start");
    expect(turnStart).toMatchObject({
      params: {
        threadId: "thread-1",
        cwd: "C:/repo",
        approvalPolicy: "on-request",
        permissions: ":read-only",
        collaborationMode: { mode: "default" },
      },
    });
    const prompt = turnStart.params.input[0].text;
    expect(prompt).toContain("重点看权限边界");
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "started" &&
      line.event.payload.subkind === "fix_suggestion" &&
      line.event.payload.target.branch === "main" &&
      line.event.payload.effectivePermission === "readonly"
    )).toBe(true);
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "success" &&
      line.event.payload.subkind === "fix_suggestion"
    )).toBe(true);
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-1")).toBe(true);
  });

  it("forces Codex fix suggestion suggest mode to readonly even when composer permission is full", async () => {
    const { protocol } = captureProtocol();
    const calls: any[] = [];
    const server = createCodexTurnTestServer(calls);

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "full",
      workflow: {
        type: LILIA_FIX_SUGGESTION_WORKFLOW_TYPE,
        target: { type: "uncommittedChanges" },
        mode: "suggest",
      },
    });

    const turnStart = calls.find((call) => call.method === "turn/start");
    expect(turnStart.params.approvalPolicy).toBe("on-request");
    expect(turnStart.params.permissions).toBe(":read-only");
    expect(turnStart.params.sandboxPolicy).toEqual({ type: "readOnly" });
  });

  it("overrides Codex fix suggestion suggest mode permission profiles to read only", async () => {
    const { protocol } = captureProtocol();
    const calls: any[] = [];
    const server = createCodexTurnTestServer(calls);

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      runtimeOptions: {
        provider: {
          codex: {
            runtimeWorkspaceRoots: ["C:/repo"],
          },
        },
      },
      workflow: {
        type: LILIA_FIX_SUGGESTION_WORKFLOW_TYPE,
        target: { type: "uncommittedChanges" },
        mode: "suggest",
      },
    });

    const turnStart = calls.find((call) => call.method === "turn/start");
    expect(turnStart.params.permissions).toBe(":read-only");
    expect(turnStart.params.runtimeWorkspaceRoots).toEqual(["C:/repo"]);
  });

  it("Codex batch apply workflow 强制先走 Plan，确认后再执行", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    let turnStarts = 0;
    const completedTurns = new Set<number>();
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "collaborationMode/list") {
          return {
            data: [{
              mode: "plan",
              reasoning_effort: "high",
            }],
          };
        }
        if (method === "turn/start") {
          turnStarts += 1;
          return { turn: { id: `turn-${turnStarts}` } };
        }
        return {};
      },
      {
        drainNotifications: () => {
          if (turnStarts > 0 && !completedTurns.has(turnStarts)) {
            completedTurns.add(turnStarts);
            if (turnStarts === 1) {
              return [{
                method: "item/agentMessage/delta",
                params: { delta: "计划：先改代码，再补测试。" },
              }, {
                method: "turn/completed",
                params: { threadId: "thread-1", turn: { status: "completed" } },
              }];
            }
            return [{
              method: "item/agentMessage/delta",
              params: { delta: "已应用建议。" },
            }, {
              method: "turn/completed",
              params: { threadId: "thread-1", turn: { status: "completed" } },
            }];
          }
          return [];
        },
      },
    );

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      planMode: false,
      workflow: {
        type: LILIA_BATCH_APPLY_WORKFLOW_TYPE,
        sourceTurnId: "turn-source",
        sourceKind: "fix_suggestion",
        sourceSummary: "建议修复权限边界",
      },
    }, {
      interactions: approvedPlanInteractions(),
    });

    const startCalls = calls.filter((call) => call.method === "turn/start");
    expect(startCalls).toHaveLength(2);
    expect(startCalls[0].params.collaborationMode).toMatchObject({
      mode: "plan",
      settings: {
        model: "gpt-5.5",
        reasoning_effort: "high",
      },
    });
    expect(startCalls[1].params.collaborationMode).toMatchObject({
      mode: "default",
      settings: { model: "gpt-5.5" },
    });
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "started" &&
      line.event.payload.subkind === "batch_apply" &&
      line.event.payload.forcedPlan === true
    )).toBe(true);
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "success" &&
      line.event.payload.subkind === "batch_apply"
    )).toBe(true);
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "message" &&
      line.event.status === "success" &&
      line.event.payload.content === "已应用建议。" &&
      line.event.payload.workflowSource
    )).toBe(false);
  });

  it("starts Codex fix suggestion workflow through turn/start in apply mode", async () => {
    const { protocol } = captureProtocol();
    const calls: any[] = [];
    const server = createCodexTurnTestServer(calls);

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "可以直接修",
      permission: "full",
      workflow: {
        type: LILIA_FIX_SUGGESTION_WORKFLOW_TYPE,
        target: { type: "commit", sha: "abc123" },
        instructions: "可以直接修",
        mode: "apply",
      },
    });

    expect(calls.some((call) => call.method === "review/start")).toBe(false);
    const turnStart = calls.find((call) => call.method === "turn/start");
    expect(turnStart.params.approvalPolicy).toBe("never");
    expect(turnStart.params.sandboxPolicy).toEqual({ type: "dangerFullAccess" });
    expect(turnStart.params.permissions).toBe(":danger-no-sandbox");
  });

  it("emits an error diagnostic when Codex fix suggestion turn fails", async () => {
    const { protocol, json } = captureProtocol();
    const server = createCodexTurnTestServer([], {
      status: "failed",
      error: { message: "fix failed" },
    });

    await expect(runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      workflow: {
        type: LILIA_FIX_SUGGESTION_WORKFLOW_TYPE,
        target: { type: "baseBranch", branch: "main" },
      },
    })).rejects.toThrow("Codex fix suggestion turn failed");

    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "error" &&
      line.event.title === "Codex fix suggestion failed" &&
      line.event.payload.method === "turn/start" &&
      line.event.payload.target.branch === "main" &&
      line.event.payload.error === "Codex fix suggestion turn failed"
    )).toBe(true);
  });

  it("starts Codex compact workflow through thread/compact/start", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    let drainCount = 0;
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        return {};
      },
      {
        drainNotifications: () => {
          drainCount += 1;
          calls.push({ method: "drain", count: drainCount });
          if (drainCount < 2) return [];
          return [{ method: "thread/compacted", params: { threadId: "thread-1", turnId: "compact-turn-1" } }];
        },
      },
    );

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      planMode: false,
      workflow: { type: LILIA_COMPACT_WORKFLOW_TYPE },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(calls.find((call) => call.method === "thread/compact/start")).toMatchObject({
      params: { threadId: "thread-1" },
    });
    expect(calls.findIndex((call) => call.method === "thread/settings/update"))
      .toBeLessThan(calls.findIndex((call) => call.method === "thread/compact/start"));
    expect(calls.findIndex((call) => call.method === "drain" && call.count === 2))
      .toBeGreaterThan(calls.findIndex((call) => call.method === "thread/compact/start"));
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "success" &&
      line.event.payload.method === "thread/compact/start"
    )).toBe(true);
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-1")).toBe(true);
  });

  it("emits an error timeline when Codex compact fails", async () => {
    const { protocol, json } = captureProtocol();
    const server = codexAppServerStub(async (method: string) => {
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/compact/start") throw new Error("compact unavailable");
        return {};
    });

    await expect(runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      workflow: { type: LILIA_COMPACT_WORKFLOW_TYPE },
    })).rejects.toThrow("compact unavailable");

    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "error" &&
      line.event.title === "Codex compact failed"
    )).toBe(true);
  });

  it("starts Codex background terminals clean workflow through thread/backgroundTerminals/clean", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        return {};
    });

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      planMode: false,
      workflow: { type: LILIA_BACKGROUND_TERMINALS_CLEAN_WORKFLOW_TYPE },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(calls.find((call) => call.method === "thread/backgroundTerminals/clean")).toMatchObject({
      params: { threadId: "thread-1" },
    });
    expect(calls.findIndex((call) => call.method === "thread/settings/update"))
      .toBeLessThan(calls.findIndex((call) => call.method === "thread/backgroundTerminals/clean"));
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "started" &&
      line.event.payload.method === "thread/backgroundTerminals/clean" &&
      line.event.payload.threadId === "thread-1"
    )).toBe(true);
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "success" &&
      line.event.payload.method === "thread/backgroundTerminals/clean" &&
      line.event.payload.threadId === "thread-1"
    )).toBe(true);
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-1")).toBe(true);
  });

  it("emits an error timeline when Codex background terminals clean fails", async () => {
    const { protocol, json } = captureProtocol();
    const server = codexAppServerStub(async (method: string) => {
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/backgroundTerminals/clean") throw new Error("clean unavailable");
        return {};
    });

    await expect(runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      workflow: { type: LILIA_BACKGROUND_TERMINALS_CLEAN_WORKFLOW_TYPE },
    })).rejects.toThrow("clean unavailable");

    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "error" &&
      line.event.title === "Codex background terminals clean failed"
    )).toBe(true);
  });

  it("updates Codex memory mode through thread/memoryMode/set", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        return {};
    });

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      workflow: { type: "lilia_memory_mode", mode: "enabled" },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(calls.find((call) => call.method === "thread/memoryMode/set")).toMatchObject({
      params: { threadId: "thread-1", mode: "enabled" },
    });
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "success" &&
      line.event.payload.method === "thread/memoryMode/set"
    )).toBe(true);
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-1")).toBe(true);
  });

  it("rejects invalid Codex memory mode workflow", async () => {
    const { protocol } = captureProtocol();
    const server = codexAppServerStub(async (method: string) => {
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        return {};
    });

    await expect(runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      workflow: { type: "lilia_memory_mode", mode: "maybe" },
    })).rejects.toThrow("Lilia memory mode workflow missing a valid mode");
  });

  it("resets Codex memory through memory/reset", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        return {};
    });

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      workflow: { type: "lilia_memory_reset" },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(calls.find((call) => call.method === "memory/reset")).toEqual({
      method: "memory/reset",
      params: null,
    });
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "success" &&
      line.event.payload.method === "memory/reset"
    )).toBe(true);
  });

  it("forks Codex thread and returns the forked session id", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/fork") return { thread: { id: "thread-fork" } };
        return {};
    });

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      runtimeOptions: {
        provider: {
          codex: {
            model: "gpt-5.5",
            runtimeWorkspaceRoots: ["C:/repo", "D:/shared"],
          },
        },
      },
      runtimeCommand: { type: SESSION_FORK_COMMAND_TYPE },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(calls.find((call) => call.method === "thread/fork")).toMatchObject({
      params: {
        threadId: "thread-1",
        cwd: "C:/repo",
        model: "gpt-5.5",
        runtimeWorkspaceRoots: ["C:/repo", "D:/shared"],
        permissions: ":read-only",
        excludeTurns: true,
      },
    });
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "success" &&
      line.event.payload.sourceThreadId === "thread-1" &&
      line.event.payload.threadId === "thread-fork"
    )).toBe(true);
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-fork")).toBe(true);
  });

  it("auto forks Codex thread before continuing the current prompt", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/resume") return { thread: { id: "thread-source" }, model: "gpt-5.5" };
        if (method === "thread/turns/list") {
          return { data: [], nextCursor: null, backwardsCursor: null };
        }
        if (method === "thread/fork") return { thread: { id: "thread-fork" } };
        if (method === "turn/start") return { turn: { id: "turn-1" } };
        return {};
      },
      {
        drainNotifications: () => [{
          method: "turn/completed",
          params: { threadId: "thread-fork", turn: { status: "completed" } },
        }],
      },
    );

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "继续实现",
      permission: "ask",
      resumeSessionId: "thread-source",
      autoSessionFork: true,
    });

    expect(calls.find((call) => call.method === "thread/fork")).toMatchObject({
      params: {
        threadId: "thread-source",
        cwd: "C:/repo",
        excludeTurns: true,
      },
    });
    const turnStart = calls.find((call) => call.method === "turn/start");
    expect(turnStart).toMatchObject({
      params: {
        threadId: "thread-fork",
        cwd: "C:/repo",
      },
    });
    expect(turnStart.params.input[0].text).toBe("继续实现");
    expect(calls.findIndex((call) => call.method === "thread/fork"))
      .toBeLessThan(calls.findIndex((call) => call.method === "turn/start"));
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.status === "success" &&
      line.event.payload.autoTurnDecision === true &&
      line.event.payload.sourceThreadId === "thread-source" &&
      line.event.payload.threadId === "thread-fork"
    )).toBe(true);
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-fork")).toBe(true);
  });

  it("forks Codex thread from a selected turn before continuing the prompt", async () => {
    const { protocol } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/resume") return { thread: { id: "thread-source" }, model: "gpt-5.5" };
        if (method === "thread/turns/list") return { data: [], nextCursor: null, backwardsCursor: null };
        if (method === "thread/fork") return { thread: { id: "thread-fork" } };
        if (method === "turn/start") return { turn: { id: "turn-anchored" } };
        return {};
      },
      {
        drainNotifications: () => [{
          method: "turn/completed",
          params: { threadId: "thread-fork", turn: { status: "completed" } },
        }],
      },
    );

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "从这里继续修复",
      permission: "ask",
      resumeSessionId: "thread-source",
      runtimeCommand: {
        type: "session_fork",
        sourceTurnId: "turn-source",
        mode: "fork",
        excludeTurns: true,
      },
    });

    expect(calls.find((call) => call.method === "thread/fork")).toMatchObject({
      params: {
        threadId: "thread-source",
        turnId: "turn-source",
        excludeTurns: true,
      },
    });
    const turnStart = calls.find((call) => call.method === "turn/start");
    expect(turnStart).toMatchObject({
      params: { threadId: "thread-fork" },
    });
    expect(turnStart.params.input[0].text).toBe("从这里继续修复");
  });

  it("reads Codex config diagnostics through config/read and configRequirements/read", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "config/read") {
          return {
            config: {
              model: "gpt-5.5",
              apps: { lilia: { enabled: true } },
              model_provider: "openai",
              approval_policy: "on-request",
              sandbox_mode: "workspace-write",
            },
            origins: { model: { source: "user" } },
            layers: [{ name: "user" }],
          };
        }
        if (method === "configRequirements/read") {
          return {
            requirements: {
              allowedApprovalsReviewers: ["user"],
              hooks: { allowManagedHooksOnly: true },
              network: { disabled: false },
            },
          };
        }
        return {};
    });

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      workflow: { type: LILIA_CONFIG_DIAGNOSTICS_WORKFLOW_TYPE },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(calls.find((call) => call.method === "config/read")).toMatchObject({
      params: { cwd: "C:/repo", includeLayers: true },
    });
    expect(calls.find((call) => call.method === "configRequirements/read")).toEqual({
      method: "configRequirements/read",
      params: null,
    });
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "diagnostic" &&
      line.event.title === "Codex config diagnostics" &&
      line.event.payload.apps?.lilia?.enabled === true &&
      line.event.payload.config?.modelProvider === "openai" &&
      line.event.payload.config?.approvalPolicy === "on-request" &&
      line.event.payload.config?.sandboxMode === "workspace-write" &&
      line.event.payload.requirements?.allowedApprovalsReviewers?.[0] === "user"
    )).toBe(true);
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-1")).toBe(true);
  });

  it("handles Codex session management list/info/messages/rename without starting a turn", async () => {
    for (const runtimeCommand of [
      { type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE, action: "list", searchTerm: "fix", limit: 10 },
      { type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE, action: "messages", sessionId: "thread-target", limit: 5 },
      { type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE, action: "archive", sessionId: "thread-target", archived: true },
    ]) {
      const { protocol, json } = captureProtocol();
      const calls: any[] = [];
      const server = {
        request: async (method: string, params: any) => {
          calls.push({ method, params });
          if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
          if (method === "thread/search") {
            return {
              data: [{
                id: "thread-target",
                title: "Fix flow",
                updatedAt: 10,
              }],
              nextCursor: "next-1",
            };
          }
          if (method === "thread/turns/list") {
            return {
              data: [{
                id: "turn-1",
                items: [{
                  type: "userMessage",
                  id: "msg-1",
                  text: "用户问题",
                }, {
                  type: "agentMessage",
                  id: "msg-2",
                  text: "助手回答",
                }],
              }],
              nextCursor: null,
            };
          }
          if (method === "thread/archive") return {};
          return {};
        },
        notify: () => {},
        respond: () => {},
        drainNotifications: () => [],
        close: () => {},
      };

      await runCodexAppServerTestTurn(protocol, server, {
        backend: "codex",
        prompt: "",
        permission: "ask",
        runtimeCommand,
      });

      expect(calls.some((call) => call.method === "turn/start")).toBe(false);
      expect(json()).toContainEqual(expect.objectContaining({
        type: "timeline",
        event: expect.objectContaining({
          kind: "diagnostic",
          status: "success",
          payload: expect.objectContaining({
            backend: "codex",
            subkind: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE,
            action: runtimeCommand.action,
            native: true,
          }),
        }),
      }));
      expect(json().some((line) =>
        line.type === "done" &&
        line.sessionId === (runtimeCommand.sessionId || null)
      )).toBe(true);
    }
  });

  it("reports unsupported diagnostic for Codex session tag and delete", async () => {
    for (const runtimeCommand of [
      { type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE, action: "tag", sessionId: "thread-target", tag: "release" },
      { type: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE, action: "delete", sessionId: "thread-target" },
    ]) {
      const { protocol, json } = captureProtocol();
      const calls: any[] = [];
      const server = codexAppServerStub(async (method: string, params: any) => {
          calls.push({ method, params });
          return {};
      });

      await runCodexAppServerTestTurn(protocol, server, {
        backend: "codex",
        prompt: "",
        permission: "ask",
        runtimeCommand,
      });

      expect(calls.some((call) => call.method === "turn/start")).toBe(false);
      expect(json()).toContainEqual(expect.objectContaining({
        type: "timeline",
        event: expect.objectContaining({
          kind: "diagnostic",
          status: "success",
          payload: expect.objectContaining({
            backend: "codex",
            subkind: SESSION_MANAGEMENT_RUNTIME_COMMAND_TYPE,
            action: runtimeCommand.action,
            native: false,
            result: expect.objectContaining({ unsupported: true }),
          }),
        }),
      }));
    }
  });

  it("updates Codex provider settings through thread settings", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/settings/update") return {};
        return {};
    });

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      runtimeCommand: {
        type: RUNTIME_SETTINGS_COMMAND_TYPE,
        action: "update",
      },
      runtimeOptions: {
        common: { model: "gpt-5.6", permission: "readonly" },
        provider: {
          claude: {
            allowedTools: ["Read"],
          },
          codex: {
            profile: "deep",
            reasoningEffort: "high",
            runtimeWorkspaceRoots: ["C:/repo", "D:/shared"],
            persistExtendedHistory: true,
            environments: [{ id: "env-1" }],
            experimentalRawEvents: true,
            responsesApiClientMetadata: { surface: "lilia" },
          },
        },
      },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    const updateCalls = calls.filter((call) => call.method === "thread/settings/update");
    expect(updateCalls.at(-1)).toMatchObject({
      params: {
        threadId: "thread-1",
        model: "gpt-5.6",
        reasoningEffort: "high",
        effort: "high",
        runtimeWorkspaceRoots: ["C:/repo", "D:/shared"],
        permissions: ":read-only",
        approvalPolicy: "on-request",
        persistExtendedHistory: true,
        environments: [{ id: "env-1" }],
        experimentalRawEvents: true,
      },
    });
    expect(updateCalls.at(-1)?.params.responsesapiClientMetadata).toEqual({ surface: "lilia" });
    expect(updateCalls.at(-1)?.params.allowedTools).toBeUndefined();
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "success",
        title: "Codex provider settings updated",
        payload: expect.objectContaining({
          backend: "codex",
          subkind: "provider_settings",
          action: "update",
          method: "thread/settings/update",
          ignoredProviderKeys: ["allowedTools"],
        }),
      }),
    }));
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-1")).toBe(true);
  });

  it("emits Codex provider settings update errors with runtime command context", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/settings/update") throw new Error("settings update failed");
        return {};
    });

    await expect(runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      runtimeCommand: {
        type: RUNTIME_SETTINGS_COMMAND_TYPE,
        action: "update",
      },
      runtimeOptions: {
        common: { model: "gpt-5.6" },
      },
    })).rejects.toThrow("settings update failed");

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "error",
        title: "Codex provider settings update failed",
        payload: expect.objectContaining({
          backend: "codex",
          subkind: "provider_settings",
          action: "update",
          method: "thread/settings/update",
          error: "settings update failed",
        }),
      }),
    }));
  });

  it("handles Codex remote environment diagnostics without starting a turn", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/settings/update") return {};
        return {};
    });

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      runtimeCommand: {
        type: REMOTE_ENVIRONMENT_COMMAND_TYPE,
        action: "diagnose",
      },
      runtimeOptions: {
        provider: {
          codex: {
            environments: [{ id: "env-1" }],
          },
        },
      },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "info",
        title: "Codex remote environment diagnostics",
        payload: expect.objectContaining({
          backend: "codex",
          subkind: REMOTE_ENVIRONMENT_COMMAND_TYPE,
          action: "diagnose",
          threadId: "thread-1",
          native: true,
          environments: [{ id: "env-1" }],
        }),
      }),
    }));
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-1")).toBe(true);
  });

  it("adds Codex remote environment through app-server", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/settings/update") return {};
        if (method === "environment/add") return { environment: { id: "env-1", name: "Windows VM" } };
        return {};
    });

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      runtimeCommand: {
        type: REMOTE_ENVIRONMENT_COMMAND_TYPE,
        action: "add",
        environmentId: "env-1",
        environment: { name: "Windows VM" },
      },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(calls.find((call) => call.method === "environment/add")).toEqual({
      method: "environment/add",
      params: { name: "Windows VM", id: "env-1" },
    });
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "success",
        title: "Codex remote environment added",
        payload: expect.objectContaining({
          backend: "codex",
          subkind: REMOTE_ENVIRONMENT_COMMAND_TYPE,
          action: "add",
          method: "environment/add",
          threadId: "thread-1",
          environmentId: "env-1",
          native: true,
        }),
      }),
    }));
  });

  it("selects Codex remote environment through thread settings", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/settings/update") return {};
        return {};
    });

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      runtimeCommand: {
        type: REMOTE_ENVIRONMENT_COMMAND_TYPE,
        action: "select",
        environmentId: "env-1",
      },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(calls.filter((call) => call.method === "thread/settings/update").at(-1)).toEqual({
      method: "thread/settings/update",
      params: {
        threadId: "thread-1",
        environments: [{ id: "env-1" }],
      },
    });
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "success",
        title: "Codex remote environment selected",
        payload: expect.objectContaining({
          backend: "codex",
          subkind: REMOTE_ENVIRONMENT_COMMAND_TYPE,
          action: "select",
          method: "thread/settings/update",
          threadId: "thread-1",
          environmentId: "env-1",
          environments: [{ id: "env-1" }],
          native: true,
        }),
      }),
    }));
  });

  it("handles Codex sandbox diagnostics without starting a turn", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "windowsSandbox/readiness") {
          return {
            available: true,
            detail: { backend: "windows-sandbox" },
          };
        }
        return {};
    });

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      runtimeCommand: {
        type: SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
        includeDetails: true,
      },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(calls.find((call) => call.method === "windowsSandbox/readiness")).toEqual({
      method: "windowsSandbox/readiness",
      params: null,
    });
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "info",
        title: "Codex sandbox diagnostics",
        payload: expect.objectContaining({
          backend: "codex",
          subkind: SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
          method: "windowsSandbox/readiness",
          threadId: "thread-1",
          includeDetails: true,
          readiness: expect.objectContaining({
            available: true,
          }),
        }),
      }),
    }));
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-1")).toBe(true);
  });

  it("keeps Codex process sessions alive for output and control commands", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const interactions = createInteractionBroker({
      protocol,
      emitToolConsentTimeline: () => {},
      emitAskUserTimeline: () => {},
    });
    let controlSent = false;
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/settings/update") return {};
        if (method === "process/spawn") return { processId: "proc-1" };
        if (method === "process/writeStdin") return {};
        if (method === "process/resizePty") return {};
        if (method === "process/kill") return {};
        return {};
      },
      notify: () => {},
      respond: () => {},
      drainNotifications: () => {
        if (!controlSent) {
          controlSent = true;
          interactions.handleControlLine(JSON.stringify({
            type: "process_session_command",
            runtimeCommand: {
              type: "process_session",
              action: "write_stdin",
              stdin: "q",
            },
          }));
          interactions.handleControlLine(JSON.stringify({
            type: "process_session_command",
            runtimeCommand: {
              type: "process_session",
              action: "resize_pty",
              rows: 32,
              cols: 120,
            },
          }));
          interactions.handleControlLine(JSON.stringify({
            type: "process_session_command",
            runtimeCommand: {
              type: "process_session",
              action: "kill",
            },
          }));
          return [{
            method: "process/outputDelta",
            params: {
              processId: "proc-1",
              stream: "stdout",
              delta: "ready\n",
            },
          }];
        }
        return [{
          method: "process/exited",
          params: {
            processId: "proc-1",
            exitCode: 0,
          },
        }];
      },
      close: () => {},
    };

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      runtimeCommand: {
        type: "process_session",
        action: "spawn",
        command: "npm test -- --watch=false",
        cwd: "C:/repo",
        env: { CI: "1" },
        tty: true,
        rows: 24,
        cols: 100,
        permissionProfile: ":workspace",
      },
    }, {
      interactions,
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(calls.find((call) => call.method === "process/spawn")).toEqual({
      method: "process/spawn",
      params: {
        command: "npm test -- --watch=false",
        cwd: "C:/repo",
        env: { CI: "1" },
        tty: true,
        rows: 24,
        cols: 100,
        permissionProfile: ":workspace",
      },
    });
    expect(calls).toContainEqual({
      method: "process/writeStdin",
      params: { processId: "proc-1", stdin: "q" },
    });
    expect(calls).toContainEqual({
      method: "process/resizePty",
      params: { processId: "proc-1", rows: 32, cols: 120 },
    });
    expect(calls).toContainEqual({
      method: "process/kill",
      params: { processId: "proc-1" },
    });
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "command",
        status: "started",
        payload: expect.objectContaining({
          backend: "codex",
          subkind: "process_session",
          action: "spawn",
          method: "process/spawn",
          processId: "proc-1",
        }),
      }),
    }));
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "command",
        status: "running",
        payload: expect.objectContaining({
          subkind: "process_session",
          action: "output",
          delta: "ready\n",
        }),
      }),
    }));
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "command",
        status: "success",
        payload: expect.objectContaining({
          subkind: "process_session",
          action: "exited",
          exitCode: 0,
        }),
      }),
    }));
    expect(json().some((line) => line.type === "done" && line.sessionId === "thread-1")).toBe(true);
  });

  it("emits Codex sandbox diagnostics errors explicitly", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "windowsSandbox/readiness") throw new Error("sandbox not available");
        return {};
    });

    await expect(runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      runtimeCommand: {
        type: SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
      },
    })).rejects.toThrow("sandbox not available");

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "error",
        title: "Codex sandbox diagnostics failed",
        payload: expect.objectContaining({
          backend: "codex",
          subkind: SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
          method: "windowsSandbox/readiness",
          includeDetails: false,
          error: "sandbox not available",
        }),
      }),
    }));
  });

  it("falls back Codex provider settings advanced fields to turn start", async () => {
    const { protocol } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/settings/update") throw new Error("settings update failed");
        if (method === "turn/start") return { turnId: "turn-1" };
        return {};
      },
      {
        drainNotifications: () => [
          { method: "turn/completed", params: { threadId: "thread-1", turn: { status: "completed" } } },
        ],
      },
    );

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "hello",
      permission: "ask",
      runtimeOptions: {
        provider: {
          codex: {
            environments: [{ id: "env-1" }],
            experimentalRawEvents: true,
            responsesApiClientMetadata: { surface: "lilia" },
          },
        },
      },
    });

    expect(calls.find((call) => call.method === "turn/start")).toMatchObject({
      params: {
        environments: [{ id: "env-1" }],
        experimentalRawEvents: true,
        responsesapiClientMetadata: { surface: "lilia" },
      },
    });
  });

  it("rejects empty Codex provider settings update before turn start", async () => {
    const { protocol } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        return {};
    });

    await expect(runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      runtimeCommand: { type: RUNTIME_SETTINGS_COMMAND_TYPE, action: "update" },
    })).rejects.toThrow("Lilia provider settings update requires at least one supported setting");

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
  });

  it("Codex experimental provider options stop before app-server when fallback is unsupported", async () => {
    const { protocol, json } = captureProtocol();
    let serverCreated = false;

    await expect(runCodex({
      backend: "codex",
      prompt: "hello",
      permission: "ask",
      runtimeOptions: {
        experimentalProviderOptions: [{
          provider: "codex",
          capability: "future-remote-environment",
          payload: { environmentId: "env-1" },
          fallback: "unsupported",
        }],
      },
    }, {
      protocol,
      interactions: { requestAskUser: cancelledAskUser },
      emitToolConsentTimeline: () => {},
      createCodexAppServer: () => {
        serverCreated = true;
        throw new Error("server should not start");
      },
      env: {},
      cwd: () => "C:/repo",
    } as any)).rejects.toThrow(
      "codex experimental provider capability is unsupported: future-remote-environment",
    );

    expect(serverCreated).toBe(false);
    expect(json()).toContainEqual(expect.objectContaining({
      type: "timeline",
      event: expect.objectContaining({
        kind: "diagnostic",
        status: "error",
        title: "Unsupported experimental provider option",
        payload: expect.objectContaining({
          backend: "codex",
          subkind: "experimental_provider_option",
          capability: "future-remote-environment",
          fallback: "unsupported",
          payloadKeys: ["environmentId"],
        }),
      }),
    }));
  });

  it("normalizes Codex review commit target for app-server schema", async () => {
    const { protocol } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "review/start") return { turn: { id: "review-turn-1" } };
        return {};
      },
      {
        drainNotifications: () => [{
          method: "turn/completed",
          params: { threadId: "thread-1", turn: { status: "completed" } },
        }],
      },
    );

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      workflow: {
        type: LILIA_REVIEW_WORKFLOW_TYPE,
        target: { type: "commit", sha: "abc123" },
      },
    });

    expect(calls.find((call) => call.method === "review/start")).toMatchObject({
      params: {
        target: { type: "commit", sha: "abc123", title: null },
      },
    });
    expect(calls.find((call) => call.method === "review/start").params).not.toHaveProperty("prompt");
  });

  it("starts Codex goal workflow through thread/goal/set", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const goal = {
      threadId: "thread-1",
      objective: "完成 Thread Goal 接入",
      status: "active",
      tokenBudget: null,
      tokensUsed: 12,
      timeUsedSeconds: 3,
      createdAt: 1,
      updatedAt: 2,
    };
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/goal/set") return { goal };
        return {};
    });

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "",
      permission: "ask",
      workflow: {
        type: LILIA_GOAL_WORKFLOW_TYPE,
        action: "set",
        objective: "完成 Thread Goal 接入",
      },
    });

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
    expect(calls.find((call) => call.method === "thread/goal/set")).toMatchObject({
      params: {
        threadId: "thread-1",
        objective: "完成 Thread Goal 接入",
        status: "active",
        tokenBudget: null,
      },
    });
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "goal" &&
      line.event.payload.goal.objective === "完成 Thread Goal 接入"
    )).toBe(true);
  });

  it("refreshes and clears Codex goal without starting a turn", async () => {
    for (const action of ["refresh", "clear"] as const) {
      const { protocol, json } = captureProtocol();
      const calls: any[] = [];
      const server = codexAppServerStub(async (method: string, params: any) => {
          calls.push({ method, params });
          if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
          if (method === "thread/goal/get") {
            return {
              goal: {
                threadId: "thread-1",
                objective: "现有目标",
                status: "active",
                tokenBudget: 100,
                tokensUsed: 20,
                timeUsedSeconds: 5,
                createdAt: 1,
                updatedAt: 2,
              },
            };
          }
          return {};
      });

      await runCodexAppServerTestTurn(protocol, server, {
        backend: "codex",
        prompt: "",
        permission: "ask",
        workflow: {
          type: LILIA_GOAL_WORKFLOW_TYPE,
          action,
        },
      });

      expect(calls.some((call) => call.method === "turn/start")).toBe(false);
      expect(calls.some((call) =>
        call.method === (action === "refresh" ? "thread/goal/get" : "thread/goal/clear")
      )).toBe(true);
      expect(json().some((line) =>
        line.type === "timeline" &&
        line.event.kind === "goal" &&
        line.event.payload.action === action
      )).toBe(true);
    }
  });

  it("maps Codex goal notifications to timeline events", () => {
    const { protocol, json } = captureProtocol();
    const ctx = createCodexRunContext({ permission: "ask" }, protocol, "thread-1");
    const updated = normalizeCodexAppServerEvent({
      method: "thread/goal/updated",
      params: {
        threadId: "thread-1",
        turnId: "turn-1",
        goal: {
          threadId: "thread-1",
          objective: "同步目标",
          status: "active",
          tokenBudget: null,
          tokensUsed: 8,
          timeUsedSeconds: 1,
          createdAt: 1,
          updatedAt: 2,
        },
      },
    });
    const cleared = normalizeCodexAppServerEvent({
      method: "thread/goal/cleared",
      params: { threadId: "thread-1" },
    });

    mapCodexEventToNdjson(updated, ctx);
    mapCodexEventToNdjson(cleared, ctx);

    const goalEvents = json()
      .filter((line) => line.type === "timeline")
      .map((line) => line.event)
      .filter((event) => event.kind === "goal");
    expect(goalEvents).toHaveLength(2);
    expect(goalEvents[0].payload.goal.objective).toBe("同步目标");
    expect(goalEvents[1].payload.cleared).toBe(true);
  });

  it("falls back to turn/start settings and emits a diagnostic when thread settings update fails", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/settings/update") throw new Error("unsupported");
        return {};
      },
    };

    const result = await updateCodexThreadSettings(server as any, "thread-1", {
      permission: "ask",
      runtimeOptions: {
        provider: {
          codex: {
            runtimeWorkspaceRoots: ["C:/repo"],
          },
        },
      },
    }, protocol);

    expect(result).toEqual({ ok: false, fallback: true });
    expect(calls[0]).toMatchObject({
      method: "thread/settings/update",
      params: {
        threadId: "thread-1",
        runtimeWorkspaceRoots: ["C:/repo"],
        permissions: ":read-only",
      },
    });
    expect(json()).toEqual([
      {
        type: "timeline",
        event: expect.objectContaining({
          kind: "diagnostic",
          status: "error",
          title: "Codex settings update failed",
        }),
      },
    ]);
  });

  it("runtime settings update refreshes Codex thread settings and later turn params", async () => {
    const { protocol } = captureProtocol();
    const calls: any[] = [];
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "turn/start") return { turn: { id: "turn-1" } };
        return {};
      },
    };
    const cmd: any = {
      permission: "ask",
      cwd: "C:/repo",
      runtimeOptions: { provider: { codex: {} } },
    };
    const ctx: any = createCodexRunContext(cmd, protocol, "thread-1");
    ctx.settingsUpdatePromises = [];

    applyCodexRuntimeSettings(
      server as any,
      "thread-1",
      cmd,
      ctx,
      { permission: "readonly", model: "gpt-5.7" },
      protocol,
    );
    await flushCodexRuntimeSettings(ctx);
    await startCodexAppServerTurn(server as any, "thread-1", "after update", cmd, () => "C:/repo");

    expect(cmd.permission).toBe("readonly");
    expect(cmd.model).toBe("gpt-5.7");
    expect(ctx.executionPermission).toBe("readonly");
    expect(calls[0]).toMatchObject({
      method: "thread/settings/update",
      params: {
        threadId: "thread-1",
        model: "gpt-5.7",
        sandboxPolicy: { type: "readOnly" },
      },
    });
    expect(calls[1]).toMatchObject({
      method: "turn/start",
      params: {
        model: "gpt-5.7",
        approvalPolicy: "on-request",
        sandboxPolicy: { type: "readOnly" },
      },
    });
  });

  it("maps Codex thread history turns into stable timeline events", () => {
    const events = codexHistoryTimelineEvents("thread-1", [{
      id: "turn-old",
      status: "completed",
      startedAt: 10,
      completedAt: 12,
      items: [{
        type: "userMessage",
        id: "user-1",
        clientId: "client-1",
        content: [{ type: "text", text: "继续修复历史恢复" }],
      }, {
        type: "commandExecution",
        id: "cmd-1",
        command: "yarn test",
        cwd: "C:/repo",
        status: "completed",
        aggregatedOutput: "ok",
        exitCode: 0,
        durationMs: 120,
      }, {
        type: "mcpToolCall",
        id: "mcp-1",
        server: "docs",
        tool: "search",
        status: "completed",
        arguments: { query: "thread/turns/list" },
        result: { hits: 1 },
        error: null,
      }, {
        type: "webSearch",
        id: "search-1",
        query: "codex history",
      }, {
        type: "agentMessage",
        id: "assistant-1",
        text: "历史已恢复",
      }],
    }]);

    expect(events).toHaveLength(5);
    expect(events[0]).toMatchObject({
      kind: "message",
      status: "success",
      title: "用户输入",
      summary: "继续修复历史恢复",
      sourceId: "codex-history:thread-1:turn-old:user-1",
      turnIdOverride: "turn-old",
      createdAt: 10_000,
      updatedAt: 12_000,
      payload: {
        history: true,
        threadId: "thread-1",
        turnId: "turn-old",
        itemId: "user-1",
        role: "user",
        content: "继续修复历史恢复",
      },
    });
    expect(events[1]).toMatchObject({
      kind: "command",
      status: "success",
      payload: {
        command: "yarn test",
        aggregatedOutput: "ok",
        exitCode: 0,
      },
    });
    expect(events[2]).toMatchObject({
      kind: "mcp",
      payload: {
        server: "docs",
        tool: "search",
        arguments: { query: "thread/turns/list" },
      },
    });
    expect(events[3]).toMatchObject({
      kind: "search",
      payload: { query: "codex history" },
    });
    expect(events[4]).toMatchObject({
      kind: "message",
      title: "Assistant",
      summary: "历史已恢复",
      payload: {
        role: "assistant",
        content: "历史已恢复",
      },
    });
    expect(events.map((event) => event.createdAt)).toEqual([
      10_000,
      10_001,
      10_002,
      10_003,
      10_004,
    ]);
  });

  it("Codex 计划确认通过统一 interaction_request/response 往返", async () => {
    const { protocol, json } = captureProtocol();
    const broker = createInteractionBroker({
      protocol,
      emitToolConsentTimeline: () => {},
      emitAskUserTimeline: () => {},
    });
    let turnStarts = 0;
    let planSent = false;
    let planCompletionSent = false;
    let executionCompletionSent = false;
    const server = {
      request: async (method: string) => {
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.1" };
        if (method === "collaborationMode/list") {
          return { data: [{ name: "plan", mode: "plan", reasoning_effort: "medium" }] };
        }
        if (method === "turn/start") {
          turnStarts += 1;
          return { turn: { id: `turn-${turnStarts}` } };
        }
        return {};
      },
      notify: () => {},
      respond: () => {},
      drainNotifications: () => {
        if (turnStarts === 1 && !planSent) {
          planSent = true;
          return [{
            method: "turn/plan/updated",
            params: {
              threadId: "thread-1",
              turnId: "turn-1",
              explanation: "计划草稿",
              plan: [{ step: "改代码", status: "pending" }],
            },
          }];
        }
        if (turnStarts === 1 && !planCompletionSent) {
          planCompletionSent = true;
          return [{
            method: "turn/completed",
            params: { threadId: "thread-1", turn: { status: "completed" } },
          }];
        }
        if (turnStarts === 2 && !executionCompletionSent) {
          executionCompletionSent = true;
          return [{
            method: "turn/completed",
            params: { threadId: "thread-1", turn: { status: "completed" } },
          }];
        }
        return [];
      },
      close: () => {},
    };

    const run = runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "请制定计划",
      permission: "ask",
      planMode: true,
    }, {
      interactions: broker,
    });

    await waitUntil(() =>
      json().some((line) => line.type === "interaction_request" && line.kind === "plan_approval")
    );
    const requests = json().filter((line) => line.type === "interaction_request");
    expect(requests).toEqual([
      {
        type: "interaction_request",
        id: "ask-1",
        kind: "plan_approval",
        backend: "codex",
        payload: expect.objectContaining({
          title: "确认 Codex 计划",
          source: "Codex Plan",
          intent: "plan_approval",
        }),
      },
    ]);

    broker.handleControlLine(JSON.stringify({
      type: RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
      id: "ask-1",
      kind: "plan_approval",
      result: approvedPlanResponse(),
    }));

    await run;
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "plan" &&
      line.event.status === "success" &&
      line.event.payload.approved === true
    )).toBe(true);
  });

  it("Codex interrupt control calls app-server turn/interrupt for the active turn", async () => {
    const { protocol, json } = captureProtocol();
    const broker = createTestInteractionBroker(protocol);
    const calls: any[] = [];
    let turnStarted = false;
    let interrupted = false;
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "initialize") return {};
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.5" };
        if (method === "thread/settings/update") return {};
        if (method === "turn/start") {
          turnStarted = true;
          return { turn: { id: "turn-1" } };
        }
        if (method === "turn/interrupt") {
          interrupted = true;
          return {};
        }
        return {};
      },
      {
        drainNotifications: () => {
          if (!turnStarted || !interrupted) return [];
          return [{
            method: "turn/completed",
            params: {
              threadId: "thread-1",
              turn: { id: "turn-1", status: "interrupted" },
            },
          }];
        },
      },
    );

    const run = runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "long turn",
      permission: "ask",
    }, {
      interactions: broker,
    });

    await waitUntil(() => turnStarted);
    broker.handleControlLine(JSON.stringify({ type: RUNNER_INTERRUPT_TURN_CONTROL_TYPE }));
    await run;

    expect(calls).toContainEqual({
      method: "turn/interrupt",
      params: { threadId: "thread-1", turnId: "turn-1" },
    });
    expect(calls.filter((call) => call.method === "turn/start")).toHaveLength(1);
    expect(json()).toContainEqual({
      type: "done",
      sessionId: "thread-1",
      subtype: "interrupted",
    });
    expect(json().some((line) => line.type === "error")).toBe(false);
  });

  it("Codex plan mode waits for the plan turn to complete before asking Lilia", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    let turnStarts = 0;
    let planSent = false;
    let planCompletionSent = false;
    let executionCompletionSent = false;
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ type: "server", method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.1" };
        if (method === "collaborationMode/list") {
          return { data: [{ name: "plan", mode: "plan", reasoning_effort: "high" }] };
        }
        if (method === "turn/start") {
          turnStarts += 1;
          return { turn: { id: `turn-${turnStarts}` } };
        }
        return {};
      },
      notify: (method: string, params: any) => {
        calls.push({ type: "notify", method, params });
      },
      respond: () => {},
      drainNotifications: () => {
        if (turnStarts === 1 && !planSent) {
          planSent = true;
          return [{
            method: "turn/plan/updated",
            params: {
              threadId: "thread-1",
              turnId: "turn-1",
              explanation: "计划草稿",
              plan: [{ step: "改代码", status: "pending" }],
            },
          }];
        }
        if (turnStarts === 1 && !planCompletionSent) {
          planCompletionSent = true;
          return [{
            method: "turn/completed",
            params: { threadId: "thread-1", turn: { status: "completed" } },
          }];
        }
        if (turnStarts === 2 && !executionCompletionSent) {
          executionCompletionSent = true;
          return [{
            method: "turn/completed",
            params: { threadId: "thread-1", turn: { status: "completed" } },
          }];
        }
        return [];
      },
      close: () => {
        calls.push({ type: "close" });
      },
    };
    let seenSpec: any = null;
    let seenOptions: any = null;
    const interactions = {
      requestAskUser: async (spec: any, options: any) => {
        calls.push({ type: "ask", spec, options });
        seenSpec = spec;
        seenOptions = options;
        return approvedPlanResponse();
      },
    };

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "请制定计划",
      permission: "ask",
      planMode: true,
      runtimeOptions: {
        provider: {
          codex: {
            reasoningEffort: "xhigh",
          },
        },
      },
    }, {
      interactions,
    });

    const startCalls = calls.filter((call) => call.type === "server" && call.method === "turn/start");
    const askIndex = calls.findIndex((call) => call.type === "ask");
    const incrementIndex = calls.findIndex((call) =>
      call.type === "server" && call.method === "thread/increment_elicitation"
    );
    const decrementIndex = calls.findIndex((call) =>
      call.type === "server" && call.method === "thread/decrement_elicitation"
    );
    const executionStartIndex = calls.indexOf(startCalls[1]);
    expect(calls.some((call) => call.type === "server" && call.method === "collaborationMode/list")).toBe(true);
    expect(calls.some((call) => call.type === "server" && call.method === "turn/interrupt")).toBe(false);
    expect(askIndex).toBeGreaterThan(-1);
    expect(incrementIndex).toBeGreaterThan(-1);
    expect(decrementIndex).toBeGreaterThan(incrementIndex);
    expect(incrementIndex).toBeLessThan(askIndex);
    expect(decrementIndex).toBeLessThan(executionStartIndex);
    expect(calls.filter((call) =>
      call.type === "server" &&
      (call.method === "thread/increment_elicitation" ||
        call.method === "thread/decrement_elicitation")
    )).toEqual([
      {
        type: "server",
        method: "thread/increment_elicitation",
        params: { threadId: "thread-1" },
      },
      {
        type: "server",
        method: "thread/decrement_elicitation",
        params: { threadId: "thread-1" },
      },
    ]);
    expect(askIndex).toBeLessThan(executionStartIndex);
    expect(startCalls).toHaveLength(2);
    expect(startCalls[0].params.collaborationMode).toEqual({
      mode: "plan",
      settings: {
        model: "gpt-5.1",
        reasoning_effort: "xhigh",
        developer_instructions: null,
      },
    });
    expect(startCalls[1].params.collaborationMode).toMatchObject({
      mode: "default",
      settings: { model: "gpt-5.1" },
    });
    expect(seenSpec).toMatchObject({
      title: "确认 Codex 计划",
      source: "Codex Plan",
      intent: "plan_approval",
    });
    expect(seenOptions).toMatchObject({
      backend: "codex",
      emitTimelineEvent: false,
    });
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "todo_list" &&
      line.event.payload.items?.[0]?.text === "改代码"
    )).toBe(true);
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "plan" &&
      line.event.status === "requires_action"
    )).toBe(true);
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "plan" &&
      line.event.status === "success" &&
      line.event.payload.approved === true
    )).toBe(true);
  });

  it("Codex plan revision starts another plan-mode turn without creating a normal message", async () => {
    const { protocol, json } = captureProtocol();
    const calls: any[] = [];
    let turnStarts = 0;
    const completedTurns = new Set<number>();
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ type: "server", method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.1" };
        if (method === "collaborationMode/list") {
          return { data: [{ name: "plan", mode: "plan", reasoning_effort: "medium" }] };
        }
        if (method === "turn/start") {
          turnStarts += 1;
          return { turn: { id: `turn-${turnStarts}` } };
        }
        return {};
      },
      notify: (method: string, params: any) => {
        calls.push({ type: "notify", method, params });
      },
      respond: () => {},
      drainNotifications: () => {
        if (turnStarts > 0 && !completedTurns.has(turnStarts)) {
          completedTurns.add(turnStarts);
          return [{
            method: "item/agentMessage/delta",
            params: { delta: "计划：先改代码，再补测试。" },
          }, {
            method: "turn/completed",
            params: { threadId: "thread-1", turn: { status: "completed" } },
          }];
        }
        return [];
      },
      close: () => {},
    };
    let askCount = 0;
    const interactions = {
      requestAskUser: async () => {
        askCount += 1;
        if (askCount === 1) {
          return approvedPlanResponse("revision_request", "先补充回滚方案");
        }
        return { cancelled: true, answers: {} };
      },
    };

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "请制定计划",
      permission: "ask",
      planMode: true,
    }, {
      interactions,
    });

    const startCalls = calls.filter((call) => call.type === "server" && call.method === "turn/start");
    expect(startCalls).toHaveLength(2);
    expect(startCalls[0].params.collaborationMode).toMatchObject({
      mode: "plan",
      settings: { model: "gpt-5.1", reasoning_effort: "medium" },
    });
    expect(startCalls[1].params.collaborationMode).toMatchObject({
      mode: "plan",
      settings: { model: "gpt-5.1", reasoning_effort: "medium" },
    });
    expect(calls.some((call) => call.type === "server" && call.method === "turn/interrupt")).toBe(false);
    expect(json().some((line) =>
      line.type === "timeline" &&
      line.event.kind === "plan" &&
      line.event.status === "cancelled" &&
      line.event.payload.revisionRequest === "先补充回滚方案"
    )).toBe(true);
    expect(json().some((line) => line.type === "user_message")).toBe(false);
  });

  it("reuses Codex subagent instructions for both the plan turn and the approved execution turn", async () => {
    const { protocol } = captureProtocol();
    const calls: any[] = [];
    let turnStarts = 0;
    let planSent = false;
    let planCompletionSent = false;
    let executionCompletionSent = false;
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ type: "server", method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.1" };
        if (method === "collaborationMode/list") {
          return { data: [{ name: "plan", mode: "plan", reasoning_effort: "medium" }] };
        }
        if (method === "turn/start") {
          turnStarts += 1;
          return { turn: { id: `turn-${turnStarts}` } };
        }
        return {};
      },
      notify: () => {},
      respond: () => {},
      drainNotifications: () => {
        if (turnStarts === 1 && !planSent) {
          planSent = true;
          return [{
            method: "turn/plan/updated",
            params: {
              threadId: "thread-1",
              turnId: "turn-1",
              explanation: "计划草稿",
              plan: [{ step: "改代码", status: "pending" }],
            },
          }];
        }
        if (turnStarts === 1 && !planCompletionSent) {
          planCompletionSent = true;
          return [{
            method: "turn/completed",
            params: { threadId: "thread-1", turn: { status: "completed" } },
          }];
        }
        if (turnStarts === 2 && !executionCompletionSent) {
          executionCompletionSent = true;
          return [{
            method: "turn/completed",
            params: { threadId: "thread-1", turn: { status: "completed" } },
          }];
        }
        return [];
      },
      close: () => {},
    };

    await runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "请制定计划",
      permission: "ask",
      planMode: true,
      runtimeOptions: {
        provider: {
          codex: {
            subagentInstructions: "Delegate review work to Reviewer and avoid duplicate delegation.",
          },
        },
      },
    }, {
      interactions: approvedPlanInteractions(),
    });

    const startCalls = calls.filter((call) => call.type === "server" && call.method === "turn/start");
    expect(startCalls).toHaveLength(2);
    expect(startCalls[0].params.collaborationMode).toMatchObject({
      mode: "plan",
      settings: {
        developer_instructions: "Delegate review work to Reviewer and avoid duplicate delegation.",
      },
    });
    expect(startCalls[1].params.collaborationMode).toMatchObject({
      mode: "default",
      settings: {
        developer_instructions: "Delegate review work to Reviewer and avoid duplicate delegation.",
      },
    });
  });

  it("Codex plan mode fails before turn/start when plan preset is missing", async () => {
    const { protocol } = captureProtocol();
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/start") return { thread: { id: "thread-1" }, model: "gpt-5.1" };
        if (method === "collaborationMode/list") return { data: [] };
        return {};
    });

    await expect(runCodexAppServerTestTurn(protocol, server, {
      backend: "codex",
      prompt: "请制定计划",
      permission: "ask",
      planMode: true,
    })).rejects.toThrow("plan collaboration preset is missing");

    expect(calls.some((call) => call.method === "turn/start")).toBe(false);
  });
});

describe("Codex history utility", () => {
  it("searches Codex threads with cursor and archive params", async () => {
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "initialize") return {};
        if (method === "thread/search") {
          return {
            data: [{
              id: "thread-1",
              title: "修复构建",
              model: "gpt-5.1",
              updatedAt: 10,
              preview: "最后一条回复",
            }],
            nextCursor: "cursor-2",
          };
        }
        return {};
    });

    await expect(searchCodexThreads({
      searchTerm: "build",
      cursor: "cursor-1",
      archived: true,
      limit: 30,
    }, { createServer: () => server as any })).resolves.toEqual({
      threads: [expect.objectContaining({
        id: "thread-1",
        title: "修复构建",
        model: "gpt-5.1",
        updatedAt: 10_000,
        preview: "最后一条回复",
      })],
      nextCursor: "cursor-2",
    });

    expect(calls.find((call) => call.method === "thread/search")).toEqual({
      method: "thread/search",
      params: {
        limit: 30,
        sortDirection: "desc",
        archived: true,
        searchTerm: "build",
        cursor: "cursor-1",
      },
    });
  });

  it("lists recent Codex threads when search term is empty", async () => {
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "initialize") return {};
        if (method === "thread/list") {
          return {
            data: [{
              id: "thread-2",
              name: "最近对话",
              updatedAt: 20,
              preview: "最近一条消息",
            }],
            nextCursor: "cursor-3",
          };
        }
        if (method === "thread/search") {
          throw new Error("thread/search should not be called without a searchTerm");
        }
        return {};
    });

    await expect(searchCodexThreads({
      searchTerm: null,
      cursor: "cursor-2",
      archived: false,
      limit: 20,
    }, { createServer: () => server as any })).resolves.toEqual({
      threads: [expect.objectContaining({
        id: "thread-2",
        title: "最近对话",
        updatedAt: 20_000,
        preview: "最近一条消息",
      })],
      nextCursor: "cursor-3",
    });

    expect(calls.find((call) => call.method === "thread/list")).toEqual({
      method: "thread/list",
      params: {
        limit: 20,
        sortDirection: "desc",
        archived: false,
        cursor: "cursor-2",
      },
    });
    expect(calls.some((call) => call.method === "thread/search")).toBe(false);
  });

  it("cleans Codex thread background terminals through history utility", async () => {
    const calls: any[] = [];
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        return {};
      },
      { close: () => calls.push({ method: "close", params: null }) },
    );

    await expect(cleanCodexThreadBackgroundTerminals(" thread-1 ", {
      createServer: () => server as any,
    })).resolves.toEqual({
      threadId: "thread-1",
      cleaned: true,
    });

    expect(calls).toEqual([
      {
        method: "initialize",
        params: {
          clientInfo: {
            name: "lilia",
            title: "LiliaCode",
            version: "0.1.0",
          },
          capabilities: { experimentalApi: true },
        },
      },
      {
        method: "thread/backgroundTerminals/clean",
        params: { threadId: "thread-1" },
      },
      { method: "close", params: null },
    ]);
  });

  it("archives Codex threads through history utility", async () => {
    const calls: any[] = [];
    const server = codexAppServerStub(
      async (requestMethod: string, params: any) => {
        calls.push({ method: requestMethod, params });
        return {};
      },
      { close: () => calls.push({ method: "close", params: null }) },
    );

    await expect(archiveCodexThread(" thread-1 ", {
      createServer: () => server as any,
    })).resolves.toEqual({
      threadId: "thread-1",
      archived: true,
    });

    expect(calls).toEqual([
      {
        method: "initialize",
        params: {
          clientInfo: {
            name: "lilia",
            title: "LiliaCode",
            version: "0.1.0",
          },
          capabilities: { experimentalApi: true },
        },
      },
      {
        method: "thread/archive",
        params: { threadId: "thread-1" },
      },
      { method: "close", params: null },
    ]);

    await expect(archiveCodexThread("  ", {
      createServer: () => {
        throw new Error("server should not start");
      },
    })).rejects.toThrow("Codex threadId is required");
  });

  it("renames Codex thread through history utility", async () => {
    const calls: any[] = [];
    const server = codexAppServerStub(
      async (method: string, params: any) => {
        calls.push({ method, params });
        return {};
      },
      { close: () => calls.push({ method: "close", params: null }) },
    );

    await expect(renameCodexThread(" thread-1 ", " 新标题 ", {
      createServer: () => server as any,
    })).resolves.toEqual({
      threadId: "thread-1",
      name: "新标题",
      renamed: true,
    });

    expect(calls).toEqual([
      {
        method: "initialize",
        params: {
          clientInfo: {
            name: "lilia",
            title: "LiliaCode",
            version: "0.1.0",
          },
          capabilities: { experimentalApi: true },
        },
      },
      {
        method: "thread/name/set",
        params: { threadId: "thread-1", name: "新标题" },
      },
      { method: "close", params: null },
    ]);
  });

  it("lists turns and backfills truncated turn items", async () => {
    const calls: any[] = [];
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/turns/list") {
          return {
            data: [{
              id: "turn-1",
              status: "completed",
              startedAt: 1,
              completedAt: 2,
              itemsTruncated: true,
              items: [],
            }],
          };
        }
        if (method === "thread/turns/items/list") {
          return {
            data: [{ type: "agentMessage", id: "msg-1", text: "历史回复" }],
          };
        }
        return {};
      },
    };

    const result = await readCodexThreadTurns(server as any, "thread-1", { limit: 10 });

    expect(calls).toEqual([
      {
        method: "thread/turns/list",
        params: {
          threadId: "thread-1",
          limit: 10,
          sortDirection: "asc",
          itemsView: "full",
        },
      },
      {
        method: "thread/turns/items/list",
        params: {
          threadId: "thread-1",
          turnId: "turn-1",
          limit: 200,
          sortDirection: "asc",
        },
      },
    ]);
    expect(result.turns[0].items).toEqual([
      { type: "agentMessage", id: "msg-1", text: "历史回复" },
    ]);
  });

  it("passes cursor when listing Codex thread turns", async () => {
    const calls: any[] = [];
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/turns/list") {
          return {
            data: [],
            nextCursor: "cursor-3",
          };
        }
        return {};
      },
    };

    const result = await readCodexThreadTurns(server as any, "thread-1", {
      limit: 10,
      cursor: "cursor-2",
    });

    expect(calls).toEqual([{
      method: "thread/turns/list",
      params: {
        threadId: "thread-1",
        limit: 10,
        sortDirection: "asc",
        itemsView: "full",
        cursor: "cursor-2",
      },
    }]);
    expect(result.nextCursor).toBe("cursor-3");
  });

  it("backfills truncated turn items concurrently while preserving turn order", async () => {
    const calls: any[] = [];
    const server = {
      request: async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "thread/turns/list") {
          return {
            data: [
              { id: "turn-1", itemsTruncated: true, items: [] },
              { id: "turn-2", itemsTruncated: true, items: [] },
              { id: "turn-3", itemsTruncated: true, items: [] },
            ],
          };
        }
        if (method === "thread/turns/items/list") {
          const delay = params.turnId === "turn-1" ? 30 : params.turnId === "turn-2" ? 10 : 0;
          await new Promise((resolve) => setTimeout(resolve, delay));
          return { data: [{ type: "agentMessage", id: `msg-${params.turnId}`, text: params.turnId }] };
        }
        return {};
      },
    };

    const result = await readCodexThreadTurns(server as any, "thread-1", {
      limit: 10,
      backfillConcurrency: 3,
    });

    expect(result.turns.map((turn: any) => turn.id)).toEqual(["turn-1", "turn-2", "turn-3"]);
    expect(result.turns.map((turn: any) => turn.items[0].text)).toEqual(["turn-1", "turn-2", "turn-3"]);
    expect(calls.filter((call) => call.method === "thread/turns/items/list")).toHaveLength(3);
  });

  it("previews Codex threads in lite mode without timeline event conversion", async () => {
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "initialize") return {};
        if (method === "thread/turns/list") {
          return {
            data: [{
              id: "turn-1",
              status: "completed",
              startedAt: 1,
              items: [
                { type: "commandExecution", id: "cmd-1", command: "yarn test" },
                { type: "userMessage", id: "user-1", text: "旧问题" },
                { type: "agentMessage", id: "assistant-1", text: "旧回复" },
              ],
            }],
          };
        }
        return {};
    });

    const result = await previewCodexThreadLite("thread-1", { createServer: () => server as any });

    expect(result.events).toBeUndefined();
    expect(result.eventCount).toBe(3);
    expect(result.messages).toEqual([
      { id: "user-1", role: "user", summary: "旧问题" },
      { id: "assistant-1", role: "assistant", summary: "旧回复" },
    ]);
    expect(calls.find((call) => call.method === "thread/turns/list")).toEqual({
      method: "thread/turns/list",
      params: {
        threadId: "thread-1",
        limit: 8,
        sortDirection: "desc",
        itemsView: "full",
      },
    });
  });

  it("previews Codex threads in full mode with all paginated timeline events", async () => {
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "initialize") return {};
        if (method === "thread/turns/list") {
          if (!params.cursor) {
            return {
              data: [{
                id: "turn-1",
                status: "completed",
                startedAt: 1,
                completedAt: 2,
                items: [
                  { type: "userMessage", id: "user-1", text: "旧问题" },
                ],
              }],
              nextCursor: "cursor-2",
            };
          }
          return {
            data: [{
              id: "turn-2",
              status: "completed",
              startedAt: 3,
              completedAt: 4,
              items: [
                { type: "agentMessage", id: "assistant-1", text: "旧回复" },
              ],
            }],
            nextCursor: null,
          };
        }
        return {};
    });

    const result = await previewCodexThread("thread-1", { createServer: () => server as any });

    expect(result.eventCount).toBe(2);
    expect(result.events).toEqual([
      expect.objectContaining({ kind: "message", summary: "旧问题" }),
      expect.objectContaining({ kind: "message", summary: "旧回复" }),
    ]);
    expect(calls.filter((call) => call.method === "thread/turns/list")).toEqual([
      {
        method: "thread/turns/list",
        params: {
          threadId: "thread-1",
          limit: 50,
          sortDirection: "asc",
          itemsView: "full",
        },
      },
      {
        method: "thread/turns/list",
        params: {
          threadId: "thread-1",
          limit: 50,
          sortDirection: "asc",
          itemsView: "full",
          cursor: "cursor-2",
        },
      },
    ]);
  });

  it("syncs history into Lilia timeline inputs", async () => {
    const calls: any[] = [];
    const server = codexAppServerStub(async (method: string, params: any) => {
        calls.push({ method, params });
        if (method === "initialize") return {};
        if (method === "thread/turns/list") {
          return {
            data: [{
              id: "turn-1",
              status: "completed",
              startedAt: 1,
              completedAt: 2,
              items: [
                { type: "userMessage", id: "user-1", text: "旧问题" },
                { type: "agentMessage", id: "msg-1", text: "旧回复" },
              ],
            }],
            nextCursor: "cursor-2",
          };
        }
        return {};
    });

    const result = await syncCodexThreadHistoryForTask({
      taskId: "task-1",
      threadId: "thread-1",
      cursor: "cursor-1",
    }, { createServer: () => server as any });

    expect(result.eventCount).toBe(2);
    expect(result.nextCursor).toBe("cursor-2");
    expect(calls.find((call) => call.method === "thread/turns/list")).toEqual({
      method: "thread/turns/list",
      params: {
        threadId: "thread-1",
        limit: 50,
        sortDirection: "asc",
        itemsView: "full",
        cursor: "cursor-1",
      },
    });
    expect(result.events).toEqual([
      expect.objectContaining({
        taskId: "task-1",
        turnId: "turn-1",
        backend: "codex",
        kind: "message",
        payload: expect.objectContaining({
          history: true,
          threadId: "thread-1",
          itemId: "user-1",
        }),
      }),
      expect.objectContaining({
        taskId: "task-1",
        turnId: "turn-1",
        kind: "message",
        summary: "旧回复",
      }),
    ]);
    expect(codexHistoryTimelineInputs("task-2", "thread-1", [])).toEqual([]);
  });
});

describe("Claude history utility", () => {
  async function withClaudeFixture(fn: (projectsDir: string) => Promise<void>) {
    const root = join(tmpdir(), `lilia-claude-history-${Date.now()}-${Math.random().toString(16).slice(2)}`);
    const projectDir = join(root, "d--PROJECT-workspace-Lilia");
    await mkdir(projectDir, { recursive: true });
    const sessionPath = join(projectDir, "session-1.jsonl");
    const rows = [
      {
        type: "ai-title",
        sessionId: "session-1",
        title: "补齐 Claude 历史导入",
      },
      {
        type: "user",
        uuid: "user-1",
        timestamp: "2026-06-08T10:00:00.000Z",
        sessionId: "session-1",
        cwd: "D:\\PROJECT\\workspace\\Lilia",
        message: {
          role: "user",
          content: [{ type: "text", text: "请导入 Claude 历史" }],
        },
      },
      {
        type: "assistant",
        uuid: "assistant-1",
        timestamp: "2026-06-08T10:00:01.000Z",
        sessionId: "session-1",
        message: {
          role: "assistant",
          model: "claude-sonnet-4-5",
          content: [{ type: "text", text: "已读取历史" }],
        },
      },
    ];
    await writeFile(sessionPath, rows.map((row) => JSON.stringify(row)).join("\n"), "utf8");
    try {
      await fn(root);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  }

  it("searches and syncs Claude JSONL sessions", async () => {
    await withClaudeFixture(async (projectsDir) => {
      await expect(searchClaudeSessions({
        searchTerm: "Claude 历史",
        limit: 1,
      }, { projectsDir })).resolves.toMatchObject({
        sessions: [{
          id: "session-1",
          title: "补齐 Claude 历史导入",
          model: "claude-sonnet-4-5",
          sourceKind: "claude",
          archived: false,
          cwd: "D:\\PROJECT\\workspace\\Lilia",
        }],
        nextCursor: null,
      });

      const sync = await syncClaudeSessionHistoryForTask({
        taskId: "task-1",
        sessionId: "session-1",
        limit: 2,
      }, { projectsDir });
      expect(sync.nextCursor).toBe("2");
      expect(sync.events).toEqual(expect.arrayContaining([
        expect.objectContaining({
          taskId: "task-1",
          turnId: "user-1",
          backend: "claude",
          payload: expect.objectContaining({
            history: true,
            sessionId: "session-1",
            itemId: "user-1",
          }),
        }),
      ]));
    });
  });
});


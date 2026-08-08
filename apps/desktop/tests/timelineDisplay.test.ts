import { fireEvent, render, waitFor } from "@testing-library/vue";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AgentTimelineEvent } from "@lilia/contracts";
import { deriveTimelineDisplay, isAgentTimelineToolWindowKind } from "@lilia/contracts";
import AgentTimeline from "../src/components/chat/AgentTimeline.vue";
import {
  readTimelineDisplay,
  readTimelinePayloadRecord,
  timelineEventLabel,
  timelineInlinePreview,
  timelineKindClass,
  timelineStatusClass,
} from "../src/components/chat/timelineDisplay";

function codeContent(
  details: ReturnType<typeof deriveTimelineDisplay>["details"],
  label: string,
): string {
  const detail = details?.find((item) => item.type === "code" && item.label === label);
  return detail?.type === "code" ? detail.content : "";
}

function fieldValue(
  details: ReturnType<typeof deriveTimelineDisplay>["details"],
  label: string,
): string {
  const fieldDetail = details?.find((item) => item.type === "fields");
  if (fieldDetail?.type !== "fields") return "";
  return fieldDetail.fields.find((field) => field.label === label)?.value ?? "";
}

function markdownContents(
  details: ReturnType<typeof deriveTimelineDisplay>["details"],
): string[] {
  return details
    ?.filter((item) => item.type === "markdown")
    .map((item) => item.type === "markdown" ? item.content : "") ?? [];
}

function listItems(
  details: ReturnType<typeof deriveTimelineDisplay>["details"],
): string[] {
  return details?.flatMap((item) =>
    item.type === "list" ? item.items.map((entry) => entry.text) : []
  ) ?? [];
}

function timelineEvent(
  patch: Partial<AgentTimelineEvent> & Pick<AgentTimelineEvent, "id" | "kind" | "payload">,
): AgentTimelineEvent {
  return {
    id: patch.id,
    taskId: "task-1",
    turnId: patch.turnId ?? null,
    backend: "claude",
    kind: patch.kind,
    status: patch.status ?? "success",
    title: patch.title ?? patch.kind,
    summary: patch.summary ?? "",
    payload: patch.payload,
    createdAt: patch.createdAt ?? 1,
    updatedAt: patch.updatedAt ?? patch.createdAt ?? 1,
    turnSeq: patch.turnSeq ?? 1,
    intraTurnOrder: patch.intraTurnOrder ?? 1,
  };
}

function assistantReplyEvent(
  id: string,
  content: string,
  patch: Partial<AgentTimelineEvent> = {},
): AgentTimelineEvent {
  return timelineEvent({
    id,
    kind: "message",
    status: "success",
    title: "Assistant",
    summary: content,
    payload: {
      role: "assistant",
      content,
    },
    turnId: id,
    ...patch,
  });
}

function nextFrame(): Promise<void> {
  return new Promise((resolveFrame) => requestAnimationFrame(() => resolveFrame()));
}

async function flushTimelineAsyncComponents() {
  if (typeof vi.dynamicImportSettled === "function") {
    await vi.dynamicImportSettled();
  }
  await Promise.resolve();
  await Promise.resolve();
}

async function renderAgentTimeline(
  props: InstanceType<typeof AgentTimeline>["$props"],
) {
  const view = render(AgentTimeline, { props });
  await flushTimelineAsyncComponents();
  return view;
}

afterEach(() => {
  vi.useRealTimers();
});

describe("timeline display derivation", () => {
  it("统一生成 timeline 状态和 kind 样式类", () => {
    expect(timelineStatusClass("requires_action")).toBe("is-status-requires-action");
    expect(timelineKindClass("agent-timeline__item--", "mcp/tool.result")).toBe(
      "agent-timeline__item--mcp-tool-result",
    );
  });

  it("识别所有会触发高优先级引导的工具窗口 kind", () => {
    for (const kind of [
      "tool",
      "command",
      "mcp",
      "search",
      "web_search",
      "web_fetch",
      "file_read",
      "file_change",
      "todo_list",
      "subagent",
    ]) {
      expect(isAgentTimelineToolWindowKind(kind)).toBe(true);
    }
    expect(isAgentTimelineToolWindowKind("message")).toBe(false);
    expect(isAgentTimelineToolWindowKind("plan")).toBe(false);
  });

  it("命令输出详情保留原始分行", () => {
    const display = deriveTimelineDisplay({
      kind: "command",
      status: "success",
      title: "yarn test",
      summary: "",
      payload: {
        command: "yarn test",
        output: "line one\nline two\nline three",
      },
    });

    expect(codeContent(display.details, "OUTPUT")).toBe("line one\nline two\nline three");
  });

  it("Lilia 编辑后的命令执行展示原始命令和修改后命令", () => {
    const display = deriveTimelineDisplay({
      kind: "command",
      status: "success",
      title: "npm test -- --runInBand",
      summary: "",
      payload: {
        subkind: "lilia_edit_exec",
        executionOwner: "lilia",
        originalCommand: "npm test",
        modifiedCommand: "npm test -- --runInBand",
        exitCode: 0,
        output: "ok",
      },
    });

    expect(display.action).toBe("执行已编辑命令");
    expect(fieldValue(display.details, "owner")).toBe("lilia");
    expect(codeContent(display.details, "ORIGINAL COMMAND")).toBe("npm test");
    expect(codeContent(display.details, "MODIFIED COMMAND")).toBe("npm test -- --runInBand");
    expect(codeContent(display.details, "OUTPUT")).toBe("ok");
  });

  it("AskUser 提问事件使用问题文本做缩略", () => {
    const event = {
      kind: "ask_user",
      status: "started" as const,
      title: "AskUserQuestion",
      summary: "方案 · 选哪个方案？ 等 2 个问题",
      payload: {
        toolName: "AskUserQuestion",
        questions: [
          {
            header: "方案",
            question: "选哪个方案？",
            options: [{ label: "方案 A" }, { label: "方案 B" }],
          },
          {
            header: "范围",
            question: "是否包含测试？",
            options: [{ label: "包含" }, { label: "不包含" }],
          },
        ],
      },
    };

    expect(timelineEventLabel(event)).toBe("正在提问");
    expect(timelineInlinePreview(event)).toBe("方案 · 选哪个方案？ 等 2 个问题");
  });

  it("待确认计划事件可派生显示", () => {
    const event = {
      kind: "plan",
      status: "requires_action" as const,
      title: "ExitPlanMode",
      summary: "",
      payload: {
        toolName: "ExitPlanMode",
        plan: "## 修改计划\n- 接线 runner\n- 补测试",
        allowedPrompts: [{ tool: "Bash", prompt: "yarn test" }],
        approved: null,
        executionPermission: "ask",
      },
    };
    const display = deriveTimelineDisplay(event);

    expect(event.kind).toBe("plan");
    expect(timelineEventLabel(event)).toBe("等待确认计划");
    expect(timelineInlinePreview(event)).toBe("## 修改计划 - 接线 runner - 补测试");
    expect(display.defaultExpanded).toBeUndefined();
    expect(markdownContents(display.details)).toContain("## 修改计划\n- 接线 runner\n- 补测试");
    expect(listItems(display.details)).toContain("Bash：yarn test");
  });

  it("读取文件事件不把执行细节作为展开详情", () => {
    const display = deriveTimelineDisplay({
      kind: "file_read",
      status: "success",
      title: "Read",
      summary: "src/App.vue",
      payload: {
        path: "src/App.vue",
        offset: 10,
        limit: 20,
        output: "const app = createApp(App);",
      },
    });

    expect(display.preview).toBe("src/App.vue");
    expect(display.details).toBeUndefined();
  });

  it("查找文件和搜索内容事件不把执行细节作为展开详情", () => {
    const globDisplay = deriveTimelineDisplay({
      kind: "search",
      status: "success",
      title: "Glob",
      summary: "*.vue",
      payload: {
        subkind: "glob",
        query: "*.vue",
        path: "apps/desktop/src",
        output: "apps/desktop/src/App.vue",
      },
    });
    const grepDisplay = deriveTimelineDisplay({
      kind: "search",
      status: "success",
      title: "Grep",
      summary: "timelineCanExpand",
      payload: {
        subkind: "grep",
        query: "timelineCanExpand",
        path: "apps/desktop/src",
        glob: "*.ts",
        output: "apps/desktop/src/components/chat/timelineDisplay.ts",
      },
    });

    expect(globDisplay.preview).toBe("*.vue");
    expect(globDisplay.details).toBeUndefined();
    expect(grepDisplay.preview).toBe("timelineCanExpand");
    expect(grepDisplay.details).toBeUndefined();
  });

  it("MCP 成功态只在有输入或输出时展开", () => {
    const emptyDisplay = deriveTimelineDisplay({
      kind: "mcp",
      status: "success",
      title: "docs / search",
      summary: "docs / search",
      payload: {
        server: "docs",
        tool: "search",
      },
    });
    const outputDisplay = deriveTimelineDisplay({
      kind: "mcp",
      status: "success",
      title: "docs / search",
      summary: "docs / search",
      payload: {
        server: "docs",
        tool: "search",
        input: { query: "timeline" },
        output: "found docs",
      },
    });

    expect(emptyDisplay.details).toBeUndefined();
    expect(fieldValue(outputDisplay.details, "服务")).toBe("");
    expect(fieldValue(outputDisplay.details, "工具")).toBe("");
    expect(codeContent(outputDisplay.details, "INPUT")).toContain('"query": "timeline"');
    expect(codeContent(outputDisplay.details, "OUTPUT")).toBe("found docs");
  });

  it("Codex MCP 配置发现用诊断展示，不显示为 MCP 调用", () => {
    const display = deriveTimelineDisplay({
      kind: "diagnostic",
      status: "info",
      title: "Codex MCP config",
      summary: "已注册 2 个 MCP server",
      payload: {
        backend: "codex",
        subkind: "config",
        source: "config.toml",
        configPath: "C:\\Users\\Administrator\\.codex\\config.toml",
        serverCount: 2,
        servers: ["node_repl", "browser"],
      },
    });
    const event = {
      kind: "diagnostic",
      status: "info" as const,
      title: "Codex MCP config",
      summary: "已注册 2 个 MCP server",
      payload: {
        backend: "codex",
        subkind: "config",
        source: "config.toml",
        servers: ["node_repl", "browser"],
      },
    };

    expect(display.label).toBe("配置诊断");
    expect(display.action).toBeUndefined();
    expect(timelineEventLabel(event)).toBe("配置诊断");
    expect(timelineInlinePreview(event)).toBe("已注册 2 个 MCP server");
    expect(display.group).toBeUndefined();
    expect(listItems(display.details)).toEqual([]);
  });

  it("同一事件快照会复用 payload 和 display 派生结果，但不同 context 仍分开缓存", () => {
    const event = timelineEvent({
      id: "tool-1",
      kind: "tool",
      status: "success",
      title: "Read",
      summary: "src/App.vue",
      payload: {
        toolName: "Read",
        path: "src/App.vue",
      },
    });

    const payloadA = readTimelinePayloadRecord(event);
    const payloadB = readTimelinePayloadRecord(event);
    expect(payloadA).toBe(payloadB);

    const displayA = readTimelineDisplay(event);
    const displayB = readTimelineDisplay(event);
    const displayWithProject = readTimelineDisplay(event, { projectCwd: "D:/PROJECT/workspace/Lilia" });
    const displayWithProjectAgain = readTimelineDisplay(
      event,
      { projectCwd: "D:/PROJECT/workspace/Lilia" },
    );

    expect(displayA).toBe(displayB);
    expect(displayWithProject).toBe(displayWithProjectAgain);
    expect(displayWithProject).not.toBe(displayA);
  });

  it("配置要求用诊断事件展示", () => {
    const display = deriveTimelineDisplay({
      kind: "diagnostic",
      status: "info",
      title: "codex config requirement",
      summary: "跳过外部 Codex MCP server：docs",
      payload: {
        backend: "codex",
        subkind: "config_requirement",
        warnings: ["跳过外部 Codex MCP server：docs"],
      },
    });

    expect(display.label).toBe("配置要求");
    expect(display.icon).toBe("stethoscope");
    expect(display.group).toBeUndefined();
    expect(codeContent(display.details, "DETAILS")).toContain("docs");
  });

  it("Hook runtime 事件用工具 hook 子类展示", () => {
    const display = deriveTimelineDisplay({
      kind: "tool",
      status: "success",
      title: "CommandEdit",
      summary: "edited",
      payload: {
        subkind: "hook",
        hookName: "CommandEdit",
        hookEvent: "PostToolUse",
        output: "edited",
      },
    });

    expect(display.action).toBe("运行 Hook");
    expect(display.object).toBe("CommandEdit");
    expect(fieldValue(display.details, "event")).toBe("PostToolUse");
    expect(codeContent(display.details, "OUTPUT")).toBe("edited");
  });

  it("兜底工具只展示输入输出，不重复工具名字段", () => {
    const display = deriveTimelineDisplay({
      kind: "tool",
      status: "success",
      title: "Index",
      summary: "索引完成",
      payload: {
        toolName: "Index",
        input: { scope: "workspace" },
        output: "indexed 42 files",
      },
    });

    expect(fieldValue(display.details, "工具")).toBe("");
    expect(codeContent(display.details, "INPUT")).toContain('"scope": "workspace"');
    expect(codeContent(display.details, "OUTPUT")).toBe("indexed 42 files");
  });

  it("项目内文件路径在展示态缩短为相对路径", () => {
    const display = deriveTimelineDisplay({
      kind: "file_read",
      status: "success",
      title: "Read",
      summary: "C:\\Files\\workspace\\Lilia\\apps\\desktop\\src\\App.vue",
      payload: {
        path: "C:\\Files\\workspace\\Lilia\\apps\\desktop\\src\\App.vue",
      },
      projectCwd: "c:\\files\\workspace\\lilia",
    });

    expect(display.object).toBe("apps/desktop/src/App.vue");
    expect(display.preview).toBe("apps/desktop/src/App.vue");
  });

  it("错误事件标题固定为发生错误", () => {
    const display = deriveTimelineDisplay({
      kind: "error",
      status: "error",
      title: "Runner error",
      summary: "provider failed",
      payload: {},
    });

    expect(display.label).toBe("发生错误");
    expect(display.preview).toBe("provider failed");
  });

  it("标题更新事件只显示新标题概要且不可展开", () => {
    const event = timelineEvent({
      id: "title-update-1",
      kind: "title_update",
      status: "success",
      title: "标题已更新",
      summary: "对话标题事件化",
      payload: {
        proposedTitle: "对话标题事件化",
        previousTitle: "对话标题能否更新",
        source: "auto",
      },
    });
    const display = deriveTimelineDisplay(event);

    expect(display.label).toBe("标题已更新");
    expect(timelineEventLabel(event)).toBe("标题已更新");
    expect(timelineInlinePreview(event)).toBe("对话标题事件化");
    expect(display.details).toBeUndefined();
  });
});

describe("timeline event expansion", () => {
  it("折叠的过程事件不参与 rail 避让计算", async () => {
    const view = await renderAgentTimeline({
        events: [
          timelineEvent({
            id: "tool-1",
            kind: "mcp",
            status: "success",
            title: "docs / search",
            summary: "docs / search",
            payload: {
              server: "docs",
              tool: "search",
            },
            turnId: "turn-1",
            turnSeq: 1,
            intraTurnOrder: 1,
          }),
          timelineEvent({
            id: "reply-1",
            kind: "message",
            status: "success",
            title: "Assistant",
            summary: "done",
            payload: {
              role: "assistant",
              content: "功能测试全部通过。\n\n还有什么需要继续测试的吗？",
            },
            turnId: "turn-1",
            turnSeq: 1,
            intraTurnOrder: 2,
          }),
          timelineEvent({
            id: "turn-end-1",
            kind: "turn",
            status: "success",
            title: "turn done",
            summary: "",
            payload: {},
            turnId: "turn-1",
            turnSeq: 1,
            intraTurnOrder: 3,
          }),
        ],
    });

    await nextFrame();
    await nextFrame();

    const railLine = view.container.querySelector<HTMLElement>(".agent-timeline__rail-line");
    expect(railLine?.style.maskImage).toBe("");
    expect(railLine?.style.webkitMaskImage).toBe("");
  });

  it("大时间线默认只渲染尾部窗口，并可按需显示更早事件", async () => {
    const view = await renderAgentTimeline({
      events: Array.from({ length: 170 }, (_, index) =>
        timelineEvent({
          id: `message-${index + 1}`,
          kind: "message",
          title: "User",
          summary: `消息 ${index + 1}`,
          payload: {
            role: "user",
            content: `消息 ${index + 1}`,
          },
          turnSeq: index + 1,
          intraTurnOrder: 1,
          createdAt: index + 1,
        })
      ),
    });

    expect(view.container.querySelector('[data-scroll-anchor-id="message-1"]')).toBeNull();
    expect(view.container.querySelector('[data-scroll-anchor-id="message-11"]')).toBeInTheDocument();
    expect(view.container.querySelector('[data-scroll-anchor-id="message-170"]')).toBeInTheDocument();

    await fireEvent.click(view.getByRole("button", { name: "显示更早 10 项" }));

    expect(view.container.querySelector('[data-scroll-anchor-id="message-1"]')).toBeInTheDocument();
    expect(view.queryByRole("button", { name: /显示更早/ })).toBeNull();
  });

  it("有详情事件仍可展开", async () => {
    const view = await renderAgentTimeline({
        events: [
          timelineEvent({
            id: "command-1",
            kind: "command",
            title: "yarn test",
            summary: "",
            payload: {
              command: "yarn test",
              output: "ok",
            },
          }),
        ],
    });

    expect(view.container.querySelector(".agent-timeline__chevron")).toBeInTheDocument();

    await fireEvent.click(view.getByRole("button", { name: "已运行 yarn test" }));
    await flushTimelineAsyncComponents();

    await waitFor(() => {
      expect(view.container.querySelector(".agent-timeline__content")).toBeInTheDocument();
    });
  });

  it("运行失败错误按 Agent 回复渲染，不显示错误概要卡", async () => {
    const view = await renderAgentTimeline({
        events: [
          timelineEvent({
            id: "error-1",
            kind: "error",
            status: "error",
            title: "Runner error",
            summary: "provider failed",
            payload: {},
          }),
        ],
    });

    await waitFor(() => {
      expect(view.container.querySelector(".timeline-card--final-reply")).toBeInTheDocument();
      expect(view.queryByText("Runner error")).not.toBeInTheDocument();
      expect(view.container.querySelector(".agent-timeline__preview")).not.toBeInTheDocument();
    });
  });

  it("Codex 修复建议最终回复可发起批量应用", async () => {
    const view = await renderAgentTimeline({
        canStartLiliaBatchApply: true,
        events: [
          timelineEvent({
            id: "reply-apply-1",
            backend: "codex",
            kind: "message",
            status: "success",
            title: "Assistant",
            summary: "建议修复权限边界",
            payload: {
              role: "assistant",
              backend: "codex",
              content: "建议修复权限边界",
              workflowSource: {
                sourceKind: "fix_suggestion",
                codexTurnId: "codex-turn-1",
              },
            },
            turnId: "turn-source",
          }),
        ],
    });

    await waitFor(() => {
      expect(view.getByRole("button", { name: "应用建议" })).toBeInTheDocument();
    });
    await fireEvent.click(view.getByRole("button", { name: "应用建议" }));

    expect(view.emitted("start-lilia-batch-apply")?.[0]).toEqual([{
      sourceTurnId: "turn-source",
      sourceKind: "fix_suggestion",
      sourceSummary: "建议修复权限边界",
    }]);
  });

  it("普通 Codex 最终回复不显示批量应用入口", async () => {
    const view = await renderAgentTimeline({
        canStartLiliaBatchApply: true,
        events: [
          timelineEvent({
            id: "reply-normal-1",
            backend: "codex",
            kind: "message",
            status: "success",
            title: "Assistant",
            summary: "普通回复",
            payload: {
              role: "assistant",
              backend: "codex",
              content: "普通回复",
            },
            turnId: "turn-normal",
          }),
        ],
    });

    await waitFor(() => {
      expect(view.queryByRole("button", { name: "应用建议" })).toBeNull();
    });
  });

  it("每条非流式 Agent 最终回复都可设置继续或分叉锚点", async () => {
    const view = await renderAgentTimeline({
        canStartSessionFork: true,
        events: [
          assistantReplyEvent("reply-fork-1", "第一条回复", { turnSeq: 1 }),
          assistantReplyEvent("reply-fork-2", "第二条回复", { turnSeq: 2 }),
        ],
    });

    await waitFor(() => {
      expect(view.getAllByRole("button", { name: "从这里继续" })).toHaveLength(2);
      expect(view.getAllByRole("button", { name: "从这里分叉" })).toHaveLength(2);
    });
    const continueButtons = view.getAllByRole("button", { name: "从这里继续" });
    const forkButtons = view.getAllByRole("button", { name: "从这里分叉" });
    expect(forkButtons).toHaveLength(2);

    await fireEvent.click(continueButtons[0]);
    await fireEvent.click(forkButtons[0]);

    expect(view.emitted("start-session-fork")).toEqual([
      [{ sourceTurnId: "reply-fork-1", mode: "continue" }],
      [{ sourceTurnId: "reply-fork-1", mode: "fork" }],
    ]);
  });

  it("流式最终回复不显示分叉当前会话入口", async () => {
    const view = await renderAgentTimeline({
        canStartSessionFork: true,
        events: [
          assistantReplyEvent("reply-fork-streaming", "正在回复", {
            status: "running",
          }),
        ],
    });

    await waitFor(() => {
      expect(view.queryByRole("button", { name: "从这里继续" })).toBeNull();
      expect(view.queryByRole("button", { name: "从这里分叉" })).toBeNull();
    });
  });

  it("禁用分叉时最终回复不显示分叉当前会话入口", async () => {
    const view = await renderAgentTimeline({
        canStartSessionFork: false,
        events: [
          assistantReplyEvent("reply-fork-disabled", "普通回复"),
        ],
    });

    await waitFor(() => {
      expect(view.queryByRole("button", { name: "从这里继续" })).toBeNull();
      expect(view.queryByRole("button", { name: "从这里分叉" })).toBeNull();
    });
  });

  it("中断错误仍按过程状态渲染", async () => {
    const view = await renderAgentTimeline({
        events: [
          timelineEvent({
            id: "interrupt-1",
            kind: "error",
            status: "error",
            title: "Interrupted",
            summary: "用户已中断",
            payload: { interrupted: true },
          }),
        ],
    });

    expect(view.container.querySelector(".timeline-card--final-reply")).not.toBeInTheDocument();
    expect(view.getByRole("button", { name: "发生错误" })).toBeInTheDocument();
  });
});


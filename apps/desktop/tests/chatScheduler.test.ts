import { render, fireEvent, waitFor } from "@testing-library/vue";
import { createMemoryHistory } from "vue-router";
import { describe, expect, it, afterEach, vi } from "vitest";
import type { ChatAttachment } from "@lilia/contracts";
import {
  AGENT_TIMELINE_LIST_COMMAND,
  CHAT_DESCRIBE_ATTACHMENTS_COMMAND,
  CHAT_GET_RUNTIME_SNAPSHOT_COMMAND,
  CHAT_INTERRUPT_TURN_COMMAND,
  CHAT_SEND_MESSAGE_COMMAND,
  LILIA_IAB_OPEN_COMMAND,
  TODO_CREATE_COMMAND,
  WORKTREE_GET_FOR_TASK_COMMAND,
} from "@lilia/contracts";
import TaskDetail from "../src/pages/TaskDetail.vue";
import { createLiliaRouter } from "../src/router";
import {
  completeMockAgentTurn,
  emitMockTimelineEvent,
  emitMockTurnCompleted,
  emitTauriEvent,
  emitWebviewDragDropEvent,
  failNextMockChatSend,
  mockInvoke,
  replaceMockTimelineEvents,
  seedMockChatMessages,
  setMockActiveBackend,
  setMockChatRunning,
  setMockComposerStateHandler,
  setMockRuntimeSnapshotDelay,
  setMockRuntimeSnapshot,
  setMockTaskWorktree,
  setMockTasks,
} from "./tauriMock";
import { createDraftTask, ensureProjectTasksLoaded } from "../src/services/tasksStore";
import { createTodo, updateTodo } from "../src/services/todos";
import {
  respondConsent,
  useToolConsentForTask,
} from "../src/composables/useToolConsentBridge";
import { useConnectionStatus } from "../src/composables/useConnectionStatus";
import { closeChatSidebar, openChatSidebar } from "../src/composables/useChatSidebar";
import { domRect, placeEditableCaret } from "./domTestHelpers";

const LEGACY_IGNORED_CHAT_ERROR_EVENT_NAME = "chat:error";

async function flushAfterPaint() {
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      window.setTimeout(resolve, 0);
    });
  });
  if (typeof vi.dynamicImportSettled === "function") {
    await vi.dynamicImportSettled();
  }
  await Promise.resolve();
}

async function renderTaskDetailForTask(taskId: string, timeout?: number) {
  const router = createLiliaRouter(createMemoryHistory());
  await router.push(`/projects/lilia/tasks/${taskId}`);
  await router.isReady();

  const view = render(TaskDetail, {
    props: {
      projectId: "lilia",
      taskId,
    },
    global: {
      plugins: [router],
    },
  });
  await flushAfterPaint();
  await waitFor(() => {
    expect(view.container.querySelector(".chat-controls")).not.toBeNull();
  }, timeout ? { timeout } : undefined);
  await flushAfterPaint();
  await waitFor(() => {
    expect(view.container.querySelector(".chat-composer [role='textbox']")).toBeInstanceOf(HTMLElement);
  }, timeout ? { timeout } : undefined);
  return view;
}

async function renderTaskDetail() {
  return renderTaskDetailForTask("t-002", 3000);
}

async function renderProjectDraftTaskDetail(taskId: string) {
  return renderTaskDetailForTask(taskId);
}

async function setComposerText(view: ReturnType<typeof render>, text: string) {
  const input = await waitFor(() => {
    const element = view.container.querySelector(".chat-composer [role='textbox']");
    expect(element).toBeInstanceOf(HTMLElement);
    return element as HTMLElement;
  });
  if (input instanceof HTMLTextAreaElement) {
    await fireEvent.update(input, text);
    return input;
  }
  const hasInlineReference = input.querySelector(".chat-file-reference") !== null;
  if (hasInlineReference) {
    input.append(document.createTextNode(` ${text}`));
    placeEditableCaret(input, input.textContent?.length ?? text.length);
  } else {
    input.textContent = text;
    placeEditableCaret(input, text.length);
  }
  await fireEvent.input(input);
  return input;
}

function getComposerTextbox(view: ReturnType<typeof render>) {
  const element = view.container.querySelector(".chat-composer [role='textbox']");
  expect(element).toBeInstanceOf(HTMLElement);
  return element as HTMLElement;
}

async function sendText(view: ReturnType<typeof render>, text: string) {
  await setComposerText(view, text);
  const sendButton = await waitFor(() => {
    const element = view.container.querySelector(".chat-composer .chat-composer__send");
    expect(element).toBeInstanceOf(HTMLButtonElement);
    return element as HTMLButtonElement;
  });
  await fireEvent.click(sendButton);
}

async function setActiveBackendForTest(backend: "claude" | "codex") {
  setMockActiveBackend(backend);
  await useConnectionStatus({ probe: false }).setActiveBackend(backend);
}

function setChatDropBounds(view: ReturnType<typeof render>) {
  const page = view.container.querySelector(".chat-page") as HTMLElement | null;
  if (!page) throw new Error("未找到聊天页面");
  page.getBoundingClientRect = () => domRect(0, 0, 800, 800);
}

async function expectInitialReasoningHidden(view: ReturnType<typeof render>) {
  await waitFor(() => {
    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === AGENT_TIMELINE_LIST_COMMAND)).toBe(true);
  });
  expect(view.queryByText("从持久化时间线恢复的公开摘要。")).toBeNull();
}

describe("chat scheduler", () => {
  let unlistenToolConsent: (() => void) | null = null;

  afterEach(async () => {
    const pendingConsent = useToolConsentForTask("t-002").value;
    if (pendingConsent) {
      await respondConsent("t-002", pendingConsent.requestId, "deny", "测试清理");
    }
    await useConnectionStatus({ probe: false }).setActiveBackend("claude");
    closeChatSidebar();
    unlistenToolConsent?.();
    unlistenToolConsent = null;
  });


  it("只处理落在当前聊天区域内的文件 drop，并随消息发送", async () => {
    const view = await renderTaskDetail();
    setChatDropBounds(view);

    emitWebviewDragDropEvent({
      type: "drop",
      paths: ["D:\\PROJECT\\workspace\\Lilia\\IGNORED.md"],
      position: { x: 900, y: 900 },
    });

    expect(
      mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_DESCRIBE_ATTACHMENTS_COMMAND),
    ).toBe(false);
    expect(view.queryByText("IGNORED.md")).not.toBeInTheDocument();

    emitWebviewDragDropEvent({
      type: "drop",
      paths: ["D:\\PROJECT\\workspace\\Lilia\\README.md"],
      position: { x: 120, y: 160 },
    });

    await waitFor(() => {
      expect(view.getByText("README.md")).toBeInTheDocument();
    });

    await sendText(view, "参考附件总结项目");

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const send = mockInvoke.mock.calls.find(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
    expect(send?.[1]).toMatchObject({
      attachments: [expect.objectContaining({
        path: "D:\\PROJECT\\workspace\\Lilia\\README.md",
      })],
    });
    expect(send?.[1].content).not.toContain("[Lilia 引导]");
    expect(send?.[1].content).toContain("参考附件总结项目");
    expect(send?.[1].content).toContain("[文件引用: README.md | D:\\PROJECT\\workspace\\Lilia\\README.md]");
    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === TODO_CREATE_COMMAND)).toBe(false);
  });

  it.skip("全局 provider 为 Codex 时发送会覆盖旧 composer backend", async () => {
    await setActiveBackendForTest("codex");
    const view = await renderTaskDetail();

    await sendText(view, "检查 Codex 通路");

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const send = mockInvoke.mock.calls.find(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
    expect(send?.[1]).toMatchObject({
      composer: expect.objectContaining({
        backend: "codex",
      }),
    });
  });

  it("默认发送不再由前端携带模型选择 metadata", async () => {
    const view = await renderTaskDetail();

    await sendText(view, "短消息交给后端自动决策");

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const send = mockInvoke.mock.calls.find(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
    expect(send?.[1].runtimeOptions).toBeNull();
    expect(send?.[1].composer).toMatchObject({
      modelSelectionMode: "auto",
    });
  });

  it("任务依赖递归未完成时不会发送并写入本地错误", async () => {
    setMockTasks([
      {
        id: "blocked-current",
        projectId: "lilia",
        sessionId: "blocked-current",
        title: "当前任务",
        titleSource: "auto",
        status: "waiting",
        createdAt: 1000,
        parentId: null,
        dependsOn: ["ready-dep"],
        sortOrder: 0,
        pinned: false,
      },
      {
        id: "ready-dep",
        projectId: "lilia",
        sessionId: "ready-dep",
        title: "已完成依赖",
        titleSource: "auto",
        status: "done",
        createdAt: 1100,
        parentId: null,
        dependsOn: ["blocked-dep"],
        sortOrder: 1,
        pinned: false,
      },
      {
        id: "blocked-dep",
        projectId: "lilia",
        sessionId: "blocked-dep",
        title: "设计前置任务",
        titleSource: "auto",
        status: "waiting",
        createdAt: 1200,
        parentId: null,
        dependsOn: [],
        sortOrder: 2,
        pinned: false,
      },
    ]);
    await ensureProjectTasksLoaded("lilia", true);
    const router = createLiliaRouter(createMemoryHistory());
    await router.push("/projects/lilia/tasks/blocked-current");
    await router.isReady();
    const view = render(TaskDetail, {
      props: {
        projectId: "lilia",
        taskId: "blocked-current",
      },
      global: {
        plugins: [router],
      },
    });
    await flushAfterPaint();
    await waitFor(() => {
      expect(view.container.querySelector(".chat-composer [role='textbox']")).toBeInstanceOf(HTMLElement);
    });

    await sendText(view, "尝试启动被依赖阻塞的任务");

    await waitFor(() => {
      expect(view.getByText(/任务依赖未完成/)).toBeInTheDocument();
    });
    expect(view.getByText(/设计前置任务/)).toBeInTheDocument();
    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND)).toBe(false);
  });

  it("任务详情不再暴露手动 process session 控制", async () => {
    const view = await renderTaskDetail();
    mockInvoke.mockClear();

    await waitFor(() => {
      expect(view.container.querySelector(".chat-composer [role='textbox']")).toBeInstanceOf(HTMLElement);
    });

    expect(view.queryByLabelText("进程命令")).not.toBeInTheDocument();
    expect(view.queryByLabelText("stdin")).not.toBeInTheDocument();
    expect(view.queryByRole("button", { name: "启动进程会话" })).not.toBeInTheDocument();
    expect(view.queryByRole("button", { name: "发送 stdin" })).not.toBeInTheDocument();
    expect(view.queryByRole("button", { name: "停止进程" })).not.toBeInTheDocument();
    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND)).toBe(false);
  });

  it("绑定 worktree 后发送使用 worktree 路径作为 project cwd", async () => {
    setMockTaskWorktree("t-002", {
      taskId: "t-002",
      projectId: "lilia",
      baseRepoPath: "D:\\PROJECT\\workspace\\Lilia",
      worktreePath: "D:\\PROJECT\\workspace\\Lilia-task-worktree",
      branchName: "lilia/t-002",
      baseBranch: "main",
      status: "active",
      createdAt: Date.now(),
      updatedAt: Date.now(),
    });
    const view = await renderTaskDetail();
    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === WORKTREE_GET_FOR_TASK_COMMAND))
        .toBe(true);
    });
    await flushAfterPaint();

    await sendText(view, "在绑定 worktree 中执行");

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const send = mockInvoke.mock.calls.find(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
    expect(send?.[1]).toMatchObject({
      projectCwd: "D:\\PROJECT\\workspace\\Lilia-task-worktree",
    });
  });

  it("手动选择 Claude thinking 强度会随消息发送", async () => {
    setMockComposerStateHandler((taskId) => ({
      taskId,
      backend: "claude",
      model: "claude-opus-4-7",
      modelSelectionMode: "manual",
      reasoningEffort: "max",
      planMode: false,
      goalMode: false,
      permission: "ask",
    }));
    const view = await renderTaskDetail();

    await sendText(view, "使用手动 Claude 配置");

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const send = mockInvoke.mock.calls.find(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
    expect(send?.[1]).toMatchObject({
      composer: expect.objectContaining({
        modelSelectionMode: "manual",
        model: "claude-opus-4-7",
        reasoningEffort: "max",
      }),
    });
    expect(send?.[1].runtimeOptions).toBeNull();
  });

  it("计划模式作为 composer 状态交给后端决策", async () => {
    setMockComposerStateHandler((taskId) => ({
      taskId,
      backend: "claude",
      model: "claude-sonnet-4-6",
      modelSelectionMode: "auto",
      reasoningEffort: null,
      planMode: true,
      goalMode: false,
      permission: "ask",
    }));
    const view = await renderTaskDetail();

    await sendText(view, "需要先计划");

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const send = mockInvoke.mock.calls.find(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
    expect(send?.[1].composer).toMatchObject({
      modelSelectionMode: "auto",
      planMode: true,
    });
    expect(send?.[1].runtimeOptions).toBeNull();
  });

  it("Claude 后端可以在右侧栏打开 Lilia IAB", async () => {
    await setActiveBackendForTest("claude");
    openChatSidebar("iab");
    const view = await renderTaskDetail();
    await waitFor(() => {
      expect(view.getByRole("tab", { name: "IAB" })).toBeInTheDocument();
      expect(view.getByRole("button", { name: "打开 IAB" })).toBeInTheDocument();
      expect(view.queryByRole("button", { name: "回送 IAB 截图" })).toBeNull();
    });

    await fireEvent.update(view.getByRole("textbox", { name: "IAB URL" }), "https://example.com/debug");
    await fireEvent.click(view.getByRole("button", { name: "打开 IAB" }));

    await waitFor(() => {
      const frame = view.container.querySelector(".iab-panel__frame");
      expect(frame).toBeInstanceOf(HTMLIFrameElement);
      expect((frame as HTMLIFrameElement).getAttribute("src")).toBe("https://example.com/debug");
    });
    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === LILIA_IAB_OPEN_COMMAND)).toBe(false);
  });

  it("斜杠项目命令通过 workflow 执行并写入时间线", async () => {
    const view = await renderTaskDetail();
    await setComposerText(view, "/release");
    await waitFor(() => {
      expect(view.getAllByRole("option").length).toBeGreaterThan(0);
    }, { timeout: 3000 });

    await fireEvent.click(await view.findByRole("option", { name: /\/release/ }, { timeout: 3000 }));

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const send = mockInvoke.mock.calls.find(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
    expect(send?.[1]).toMatchObject({
      content: "",
      attachments: [],
      workflow: {
        type: "slash_command",
        commandId: "project:release",
        source: "project",
        arguments: {},
      },
    });
    await waitFor(() => {
      expect(view.getByText(/请完成发布前检查并整理风险项/)).toBeInTheDocument();
    });
  });

  it("项目草稿首条斜杠命令用命令名提升标题", async () => {
    const draft = createDraftTask("lilia");
    const view = await renderProjectDraftTaskDetail(draft.id);
    await setComposerText(view, "/release");
    await waitFor(() => {
      expect(view.getAllByRole("option").length).toBeGreaterThan(0);
    }, { timeout: 3000 });

    await fireEvent.click(await view.findByRole("option", { name: /\/release/ }, { timeout: 3000 }));

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const promote = mockInvoke.mock.calls.find(([cmd, args]) =>
      cmd === "product_create_entity" &&
      (args?.entity as { kind?: string } | undefined)?.kind === "task"
    );
    expect(promote?.[1]).toMatchObject({
      entity: {
        kind: "task",
        value: {
          id: draft.id,
          projectId: "lilia",
          title: "/release",
        },
      },
    });
  });

  it.skip("Codex 项目草稿首条消息会先提升草稿再发送", async () => {
    await setActiveBackendForTest("codex");
    const draft = createDraftTask("lilia");
    const view = await renderProjectDraftTaskDetail(draft.id);

    await sendText(view, "用 Codex 开始草稿对话");

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const promote = mockInvoke.mock.calls.find(([cmd, args]) =>
      cmd === "product_create_entity" &&
      (args?.entity as { kind?: string } | undefined)?.kind === "task"
    );
    const send = mockInvoke.mock.calls.find(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
    expect(promote?.[1]).toMatchObject({
      entity: {
        kind: "task",
        value: {
          id: draft.id,
          projectId: "lilia",
          title: "用 Codex 开始草稿对话",
        },
      },
    });
    expect(send?.[1]).toMatchObject({
      taskId: draft.id,
      composer: expect.objectContaining({
        backend: "codex",
      }),
    });
    expect(mockInvoke.mock.calls.findIndex(([cmd, args]) =>
      cmd === "product_create_entity" &&
      (args?.entity as { kind?: string } | undefined)?.kind === "task"
    ))
      .toBeLessThan(mockInvoke.mock.calls.findIndex(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND));
  });

  it("已入库的草稿前缀对话再次发送不会被判为失效草稿", async () => {
    const draft = createDraftTask("lilia");
    const view = await renderProjectDraftTaskDetail(draft.id);

    await sendText(view, "创建草稿前缀对话");

    await waitFor(() => {
      expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toHaveLength(1);
    });
    expect(mockInvoke.mock.calls.filter(([cmd, args]) =>
      cmd === "product_create_entity" &&
      (args?.entity as { kind?: string } | undefined)?.kind === "task"
    ))
      .toHaveLength(1);

    completeMockAgentTurn(draft.id);
    await waitFor(() => {
      expect(view.queryByRole("button", { name: "打断 Agent" })).toBeNull();
    });

    mockInvoke.mockClear();
    await sendText(view, "继续发送第二条消息");

    await waitFor(() => {
      expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toHaveLength(1);
    });
    expect(mockInvoke.mock.calls.some(([cmd, args]) =>
      cmd === "product_create_entity" &&
      (args?.entity as { kind?: string } | undefined)?.kind === "task"
    )).toBe(false);
    expect(view.queryByText(/草稿已失效，请重新创建对话/)).toBeNull();
  });

  it("子对话草稿首条消息会携带父对话关系提升入库", async () => {
    const draft = createDraftTask("lilia", "t-001");
    const view = await renderProjectDraftTaskDetail(draft.id);

    await sendText(view, "基于父对话继续追问");

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const promote = mockInvoke.mock.calls.find(([cmd, args]) =>
      cmd === "product_create_entity" &&
      (args?.entity as { kind?: string } | undefined)?.kind === "task"
    );
    expect(promote?.[1]).toMatchObject({
      entity: {
        kind: "task",
        value: {
          id: draft.id,
          projectId: "lilia",
          parentId: "t-001",
          title: "基于父对话继续追问",
        },
      },
    });
  });

  it("composer 尚未加载完成时发送会等待后端状态再发给 agent", async () => {
    let resolveComposer!: (value: unknown) => void;
    setMockComposerStateHandler((taskId) =>
      new Promise((resolve) => {
        resolveComposer = resolve;
      }).then(() => ({
        taskId,
        backend: "claude",
        model: "claude-sonnet-4-6",
        planMode: false,
        goalMode: false,
        permission: "ask",
      }))
    );

    const view = await renderTaskDetail();
    await sendText(view, "加载期间发送");

    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND)).toBe(false);

    resolveComposer(null);

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const send = mockInvoke.mock.calls.find(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
    expect(send?.[1].content).toContain("加载期间发送");
  });

  it("composer 加载失败时不会发送或清空附件", async () => {
    setMockComposerStateHandler(() => Promise.reject(new Error("composer failed")));
    const view = await renderTaskDetail();
    setChatDropBounds(view);

    emitWebviewDragDropEvent({
      type: "drop",
      paths: ["D:\\PROJECT\\workspace\\Lilia\\README.md"],
      position: { x: 120, y: 160 },
    });
    await waitFor(() => {
      expect(view.getByText("README.md")).toBeInTheDocument();
    });

    await sendText(view, "失败时保留附件");

    await waitFor(() => {
      expect(view.getByText(/发送失败：Error: composer failed/)).toBeInTheDocument();
    });
    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND)).toBe(false);
    expect(view.getByText("README.md")).toBeInTheDocument();
  });

  it("会把 Agent 运行中追加的用户消息先放入 Lilia 引导，并在 turn 结束后插入", async () => {
    const view = await renderTaskDetail();

    await sendText(view, "先检查当前实现");
    await waitFor(() => {
      expect(view.getByText("先检查当前实现")).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(view.getByRole("button", { name: "打断 Agent" })).toBeInTheDocument();
    });

    await sendText(view, "补充：优先看调度器");
    await waitFor(() => {
      expect(view.getByText("补充：优先看调度器")).toBeInTheDocument();
      expect(view.getByText("补充：优先看调度器").closest(".todo-float"))
        .not.toBeNull();
    });

    const sends = mockInvoke.mock.calls.filter(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
    expect(sends).toHaveLength(1);

    completeMockAgentTurn("t-002");

    await waitFor(() => {
      const nextSends = mockInvoke.mock.calls.filter(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
      expect(nextSends).toHaveLength(2);
      expect(nextSends[1][1].content).toContain("[Lilia 引导]");
      expect(nextSends[1][1].content).toContain("补充：优先看调度器");
    });
    await waitFor(() => {
      const guideRows = Array.from(view.container.querySelectorAll(".todo-float__row--guide"));
      expect(guideRows.some((row) => row.textContent?.includes("补充：优先看调度器")))
        .toBe(false);
    });
  });

  it("打断后立即恢复输入态并阻止重复打断", async () => {
    const view = await renderTaskDetail();
    setChatDropBounds(view);

    emitWebviewDragDropEvent({
      type: "drop",
      paths: ["D:\\PROJECT\\workspace\\Lilia\\README.md"],
      position: { x: 120, y: 160 },
    });
    await waitFor(() => {
      expect(view.getByText("README.md")).toBeInTheDocument();
    });

    await sendText(view, "先别发出去");
    await waitFor(() => {
      expect(view.getByRole("button", { name: "打断 Agent" })).toBeInTheDocument();
    });

    mockInvoke.mockClear();
    const interrupt = view.getByRole("button", { name: "打断 Agent" });
    await Promise.all([
      fireEvent.click(interrupt),
      fireEvent.click(interrupt),
    ]);

    await waitFor(() => {
      expect(view.queryByRole("button", { name: "打断 Agent" })).toBeNull();
      expect(getComposerTextbox(view)).toBeInTheDocument();
      expect(view.getByText("先别发出去")).toBeInTheDocument();
    });
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === CHAT_INTERRUPT_TURN_COMMAND)).toHaveLength(1);
  });

  it("打断成功后的新输入会直接发送而不是加入调度队列", async () => {
    const view = await renderTaskDetail();

    await sendText(view, "先运行一轮");
    await waitFor(() => {
      expect(view.getByRole("button", { name: "打断 Agent" })).toBeInTheDocument();
    });

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND)).toBe(true);
    });
    mockInvoke.mockClear();

    await fireEvent.click(view.getByRole("button", { name: "打断 Agent" }));
    await waitFor(() => {
      expect(view.queryByRole("button", { name: "打断 Agent" })).toBeNull();
    });

    await sendText(view, "打断后的新请求");

    await waitFor(() => {
      const sends = mockInvoke.mock.calls.filter(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
      expect(sends).toHaveLength(1);
      expect(sends[0][1].content).toBe("打断后的新请求");
    });
  });

  it("重新进入任务页时会从 runtime snapshot 恢复运行态", async () => {
    setMockChatRunning("t-002", true);

    const view = await renderTaskDetail();

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_GET_RUNTIME_SNAPSHOT_COMMAND))
        .toBe(true);
      expect(view.getByRole("button", { name: "打断 Agent" })).toBeInTheDocument();
    });
  });

  it("任务页首屏不会等待 runtime snapshot，运行态稍后补齐", async () => {
    setMockChatRunning("t-002", true);
    setMockRuntimeSnapshotDelay(300);

    const view = await renderTaskDetail();

    expect(getComposerTextbox(view)).toBeInTheDocument();
    expect(view.queryByRole("button", { name: "打断 Agent" })).toBeNull();

    await waitFor(() => {
      expect(view.getByRole("button", { name: "打断 Agent" })).toBeInTheDocument();
    });
  });

  it("重新进入任务页时会把 abandoned runtime snapshot 显示为异常而不是运行态", async () => {
    setMockRuntimeSnapshot("t-002", {
      phase: "abandoned",
      backend: "codex",
      turnId: "turn-old",
    });

    const view = await renderTaskDetail();

    await waitFor(() => {
      expect(view.getByText(/上次 codex 运行未正常结束/)).toBeInTheDocument();
      expect(view.queryByRole("button", { name: "打断 Agent" })).toBeNull();
    });
  });


  it("工具类 timeline 窗口会触发高优先级引导，但不会触发普通优先级引导", async () => {
    const view = await renderTaskDetail();

    const high = await createTodo("t-002", "高优先级工具窗口引导");
    await updateTodo(high.id, { priority: "high" });
    await createTodo("t-002", "普通优先级保留");
    mockInvoke.mockClear();

    emitMockTimelineEvent("t-002", {
      id: "tl-command-window",
      kind: "command",
      status: "started",
      title: "Bash",
      summary: "yarn test",
      payload: {
        backend: "claude",
        command: "yarn test",
      },
      turnId: "turn-tool-window",
    });

    await waitFor(() => {
      const sends = mockInvoke.mock.calls.filter(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
      expect(sends).toHaveLength(1);
      expect(sends[0][1].content).toContain("高优先级工具窗口引导");
      expect(sends[0][1].content).not.toContain("普通优先级保留");
    });

    await waitFor(() => {
      expect(view.getByText("普通优先级保留")).toBeInTheDocument();
    });
  });


  it("会显示持久化和实时 Agent 工作过程", async () => {
    const view = await renderTaskDetail();

    await expectInitialReasoningHidden(view);

    emitMockTimelineEvent("t-002", {
      id: "tl-live-command",
      kind: "command",
      status: "running",
      title: "yarn verify",
      summary: "正在运行完整验证",
      payload: { command: "yarn verify" },
      order: 1,
    });

    await waitFor(() => {
      expect(view.getByRole("button", { name: /yarn verify/ })).toBeInTheDocument();
      expect(view.getByText("正在运行完整验证")).toBeInTheDocument();
    });
  });

  it("图片查看器遮罩挂在主内容页层级，覆盖 shell 主区留白", async () => {
    seedMockChatMessages("t-002", [
      {
        id: "u-image",
        taskId: "t-002",
        role: "user",
        content: "看图",
        attachments: [
          {
            id: "image-1",
            name: "shot.png",
            path: "D:\\PROJECT\\workspace\\Lilia\\shot.png",
            kind: "file",
            size: 42,
            exists: true,
            mime: "image/png",
            directory: null,
          },
        ],
        createdAt: 2000,
      },
    ]);

    const view = await renderTaskDetail();
    const imageButton = await waitFor(() =>
      view.getByRole("button", { name: "查看图片 shot.png" })
    );
    await fireEvent.click(imageButton);

    const viewer = await view.findByRole("dialog", { name: "图片查看器" });
    const page = view.container.querySelector(".chat-page");
    const main = view.container.querySelector(".chat-layout__main");

    expect(viewer.parentElement).toBe(page);
    expect(viewer.parentElement).not.toBe(main);
    expect(viewer).toHaveClass("chat-file-drop-overlay");
  });


  it("turn 完成后，最终回复之前的多个工具/计划事件折叠到 final 卡下，点开按钮才显出", async () => {
    seedMockChatMessages("t-002", [
      {
        id: "u-turn-start",
        taskId: "t-002",
        role: "user",
        content: "请实现时间线折叠",
        createdAt: 2000,
      },
    ]);
    emitMockTimelineEvent("t-002", {
      id: "tl-hidden-process-command",
      kind: "command",
      status: "error",
      title: "yarn test",
      summary: "命令折叠态预览",
      payload: {
        command: "yarn test",
        stdout: "折叠后的命令详情",
      },
      turnId: "turn-fold",
      createdAt: 2100,
      updatedAt: 3100,
      order: 2,
    });
    emitMockTimelineEvent("t-002", {
      id: "tl-hidden-process-plan",
      kind: "plan",
      status: "completed",
      title: "更新计划",
      summary: "计划折叠态预览",
      payload: {
        plan: "折叠后的计划详情",
      },
      turnId: "turn-fold",
      createdAt: 6200,
      updatedAt: 8100,
      order: 3,
    });
    emitMockTimelineEvent("t-002", {
      id: "tl-final-with-hidden-process",
      kind: "message",
      status: "success",
      title: "Assistant",
      payload: {
        backend: "claude",
        role: "assistant",
        content: "最终回复应该直接可见。",
      },
      turnId: "turn-fold",
      createdAt: 8200,
      updatedAt: 8200,
      order: 4,
    });
    emitMockTurnCompleted("t-002", "turn-fold", "success", 8300);

    const view = await renderTaskDetail();

    await waitFor(() => {
      expect(view.getByText("请实现时间线折叠")).toBeInTheDocument();
      expect(view.getByText("最终回复应该直接可见。")).toBeInTheDocument();
      expect(view.queryByRole("button", { name: /yarn test/ })).toBeNull();
      expect(view.queryByRole("button", { name: /更新计划/ })).toBeNull();
      const toggle = view.getByRole("button", { name: "命令执行、计划更新 · 6 秒" });
      expect(toggle).not.toHaveTextContent("有失败");
      expect(toggle).not.toHaveTextContent(/展开过程|查看过程|2 项/);
      expect(toggle).not.toHaveClass("agent-timeline__process-toggle--failed");
      expect(toggle).toHaveAttribute("aria-expanded", "false");
    });

    const finalItem = view.getByText("最终回复应该直接可见。")
      .closest(".agent-timeline__item");
    const processCollapse = finalItem?.querySelector(".agent-timeline__process-collapse");
    expect(processCollapse).not.toBeNull();
    expect(processCollapse).toHaveAttribute("aria-hidden", "true");
    expect(processCollapse).toHaveAttribute("inert");
    expect(processCollapse).not.toHaveClass("is-open");
    expect(processCollapse?.querySelectorAll(".agent-timeline__item.is-process-child"))
      .toHaveLength(2);

    await fireEvent.click(view.getByRole("button", { name: "命令执行、计划更新 · 6 秒" }));

    await waitFor(() => {
      expect(processCollapse).toHaveAttribute("aria-hidden", "false");
      expect(processCollapse).not.toHaveAttribute("inert");
      expect(processCollapse).toHaveClass("is-open");
      expect(view.getByRole("button", { name: /yarn test/ })).toBeInTheDocument();
      expect(view.getByRole("button", { name: /更新计划/ })).toBeInTheDocument();
    });

    // 展开态下按 order 排：工具/计划（2、3）在 final（4）上方。
    const timelineText = finalItem?.textContent ?? "";
    expect(timelineText.indexOf("yarn test"))
      .toBeLessThan(timelineText.indexOf("最终回复应该直接可见。"));
    expect(timelineText.indexOf("更新计划"))
      .toBeLessThan(timelineText.indexOf("最终回复应该直接可见。"));

    await fireEvent.click(view.getByRole("button", { name: /yarn test/ }));
    await fireEvent.click(view.getByRole("button", { name: /更新计划/ }));
    await waitFor(() => {
      expect(view.getByText("折叠后的命令详情")).toBeInTheDocument();
      expect(view.getByText("折叠后的计划详情")).toBeInTheDocument();
    });
  });


  it("agent 错误走 timeline 错误事件，不再创建 system 普通气泡", async () => {
    const view = await renderTaskDetail();

    await expectInitialReasoningHidden(view);

    emitMockTimelineEvent("t-002", {
      id: "tl-agent-error",
      kind: "error",
      status: "error",
      title: "错误",
      summary: "agent 报错：连接失败",
      payload: {
        message: "agent 报错：连接失败",
      },
      order: 1,
    });
    emitTauriEvent(LEGACY_IGNORED_CHAT_ERROR_EVENT_NAME, {
      taskId: "t-002",
      message: "旧错误通道不应生成气泡",
    });

    await waitFor(() => {
      expect(view.getByText("发生错误")).toBeInTheDocument();
      expect(view.getByText("agent 报错：连接失败")).toBeInTheDocument();
    });
    expect(view.getByText("agent 报错：连接失败").closest(".chat-bubble")).toBeNull();
    expect(view.queryByText("旧错误通道不应生成气泡")).toBeNull();
  });

  it("可从同 turn 用户消息重试错误事件并复用附件", async () => {
    const retryAttachment: ChatAttachment = {
      id: "att-retry",
      name: "README.md",
      path: "D:\\PROJECT\\workspace\\Lilia\\README.md",
      kind: "file",
      size: 42,
      exists: true,
      mime: "text/markdown",
    };
    replaceMockTimelineEvents("t-002", [
      {
        id: "u-retry",
        kind: "message",
        turnId: "turn-retry",
        summary: "请读取 README",
        payload: {
          role: "user",
          content: "请读取 README",
          attachments: [retryAttachment],
          queued: false,
        },
        intraTurnOrder: 0,
      },
      {
        id: "err-retry",
        kind: "error",
        turnId: "turn-retry",
        status: "error",
        title: "错误",
        summary: "agent 报错：连接失败",
        payload: { message: "agent 报错：连接失败" },
        intraTurnOrder: 1,
      },
    ]);

    const view = await renderTaskDetail();

    await waitFor(() => {
      expect(view.getByText("agent 报错：连接失败")).toBeInTheDocument();
    });

    mockInvoke.mockClear();
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      await fireEvent.click(view.getByRole("button", { name: "重试" }));
    } finally {
      consoleError.mockRestore();
    }

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const send = mockInvoke.mock.calls.find(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
    expect(send?.[1]).toMatchObject({
      content: "请读取 README",
      attachments: [expect.objectContaining({
        path: retryAttachment.path,
      })],
    });
  });

  it("没有可定位上下文的错误事件不显示重试", async () => {
    replaceMockTimelineEvents("t-002", [
      {
        id: "err-no-context",
        kind: "error",
        turnId: null,
        status: "error",
        title: "错误",
        summary: "agent 报错：无法定位上下文",
        payload: { message: "agent 报错：无法定位上下文" },
      },
    ]);

    const view = await renderTaskDetail();

    await waitFor(() => {
      expect(view.getByText("agent 报错：无法定位上下文")).toBeInTheDocument();
    });
    expect(view.queryByRole("button", { name: "重试" })).toBeNull();
  });

  it("本地发送失败错误携带上下文并可重试", async () => {
    replaceMockTimelineEvents("t-002", [
      {
        id: "err-local-retry-source",
        kind: "error",
        turnId: null,
        status: "error",
        title: "错误",
        summary: "可重试错误",
        payload: {
          message: "可重试错误",
          retryContext: {
            content: "本地发送失败后重试",
            attachments: [],
          },
        },
      },
    ]);
    failNextMockChatSend("mock send failed");
    const view = await renderTaskDetail();

    await waitFor(() => {
      expect(view.getByText("可重试错误")).toBeInTheDocument();
    });

    await fireEvent.click(view.getByRole("button", { name: "重试" }));

    await waitFor(() => {
      expect(view.getByText(/发送失败：Error: mock send failed/)).toBeInTheDocument();
    });
    expect(view.container.querySelectorAll(".timeline-card--final-reply")).toHaveLength(2);

    mockInvoke.mockClear();
    const retryButtons = view.getAllByRole("button", { name: "重试" });
    const retryButton = retryButtons[retryButtons.length - 1];
    if (!retryButton) {
      throw new Error("未找到本地错误重试按钮");
    }
    await fireEvent.click(retryButton);

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND))
        .toBe(true);
    });
    const send = mockInvoke.mock.calls.find(([cmd]) => cmd === CHAT_SEND_MESSAGE_COMMAND);
    expect(send?.[1]).toMatchObject({ content: "本地发送失败后重试" });
  });
});

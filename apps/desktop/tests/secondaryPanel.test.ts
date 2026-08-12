import { render, fireEvent, waitFor, within } from "@testing-library/vue";
import { createMemoryHistory } from "vue-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";
import {
  CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME,
  CHAT_DONE_EVENT_NAME,
  CHAT_GET_RUNTIME_SNAPSHOT_COMMAND,
  CHAT_INTERRUPT_TURN_COMMAND,
  CHAT_TURN_STARTED_EVENT_NAME,
  POPUP_OPEN_TASK_COMMAND,
  TASK_LIST_COMMAND,
  TASK_LIST_SIDEBAR_CONVERSATIONS_COMMAND,
  TASK_REPARENT_COMMAND,
  type Task,
} from "@lilia/contracts";
import SecondaryPanel from "../src/layouts/SecondaryPanel.vue";
import ContextMenuHost from "@lilia/ui/components/ContextMenuHost";
import { vContextMenu } from "@lilia/ui/directives/contextMenu";
import { useConnectionStatus } from "../src/composables/useConnectionStatus";
import { useSidebarDisplayMode } from "../src/composables/useSidebarDisplayMode";
import { createLiliaRouter } from "../src/router";
import { resetSidebarConversationsCache } from "../src/data/sidebarConversations";
import {
  getTask,
  listTasks,
  ORPHAN_LIST,
  ORPHANS_LOADED,
  PROJECT_TASKS_LOADED,
  TASKS,
} from "../src/data/tasks";
import {
  emitMockTimelineEvent,
  emitTauriEvent,
  emitWebviewDragDropEvent,
  mockInvoke,
  replaceMockTimelineEvents,
  resetTauriMockData,
  setMockActiveBackend,
  setMockCodexAppServerStatus,
  setMockChatRunning,
  setMockProjectPinned,
  setMockTasks,
  setMockRuntimeSnapshot,
  setMockWindowScaleFactor,
} from "./tauriMock";
import { domRect } from "./domTestHelpers";
import {
  installConversationActivityBridge,
  resetConversationActivity,
} from "../src/composables/useConversationActivity";
import { installAgentInteractionBridge } from "../src/composables/useAgentInteractionBridge";

function seedTreeExpansionState(state: unknown) {
  localStorage.setItem("lilia.projectTree.expansion", JSON.stringify(state));
}

async function renderSecondaryPanel(initialRoute = "/") {
  const router = createLiliaRouter(createMemoryHistory());
  await router.push(initialRoute);
  await router.isReady();

  const Wrapper = defineComponent({
    components: { SecondaryPanel, ContextMenuHost },
    template: `
      <SecondaryPanel />
      <ContextMenuHost />
    `,
  });

  const view = render(Wrapper, {
    global: {
      plugins: [router],
      directives: {
        contextMenu: vContextMenu,
      },
      stubs: {
        transition: false,
      },
    },
  });
  if (typeof vi.dynamicImportSettled === "function") {
    await vi.dynamicImportSettled();
  }
  await new Promise((resolve) => globalThis.setTimeout(resolve, 10));
  if (typeof vi.dynamicImportSettled === "function") {
    await vi.dynamicImportSettled();
  }
  await Promise.resolve();
  await Promise.resolve();
  return { ...view, router };
}

function getProjectRow(view: ReturnType<typeof render>, projectName: string): HTMLElement {
  const row = view.getByText(projectName).closest(".sb-tree__row--project");
  if (!(row instanceof HTMLElement)) {
    throw new Error(`未找到项目行：${projectName}`);
  }
  return row;
}

function getConversationRow(view: ReturnType<typeof render>, title: string): HTMLElement {
  const row = view.getAllByText(title)
    .map((element) => element.closest(".sb-tree__row"))
    .find((candidate) =>
      candidate instanceof HTMLElement &&
      !candidate.classList.contains("sb-tree__row--running-process")
    );
  if (!(row instanceof HTMLElement)) {
    throw new Error(`未找到对话行：${title}`);
  }
  return row;
}

async function findConversationRow(view: ReturnType<typeof render>, title: string): Promise<HTMLElement> {
  let row: HTMLElement | null = null;
  await waitFor(() => {
    row = getConversationRow(view, title);
    expect(row).toBeInTheDocument();
  });
  return row as HTMLElement;
}

function getRunningProcessRow(view: ReturnType<typeof render>, title: string): HTMLElement {
  const section = view.getByText("进行中").closest(".sb-section--running-processes");
  if (!(section instanceof HTMLElement)) {
    throw new Error("未找到进行中进程区域");
  }
  const row = within(section).getByText(title).closest(".sb-tree__row--running-process");
  if (!(row instanceof HTMLElement)) {
    throw new Error(`未找到运行中进程行：${title}`);
  }
  return row;
}

async function findRunningProcessRow(view: ReturnType<typeof render>, title: string): Promise<HTMLElement> {
  let row: HTMLElement | null = null;
  await waitFor(() => {
    row = getRunningProcessRow(view, title);
    expect(row).toBeInTheDocument();
  });
  return row as HTMLElement;
}

function box(top: number, bottom: number): DOMRect {
  return domRect(0, top, 220, bottom - top);
}

function mockElementFromPoint(element: Element | null) {
  Object.defineProperty(document, "elementFromPoint", {
    configurable: true,
    value: vi.fn(() => element),
  });
}

function mockProjectTreeDropHit(element: Element | null) {
  const tree = document.querySelector('[data-agent-id="sidebar.projects.tree"]');
  if (tree instanceof HTMLElement) {
    tree.getBoundingClientRect = () => domRect(0, 0, 260, 420);
  }
  mockElementFromPoint(element);
}

function emitFolderDropEvent(
  type: "enter" | "over" | "drop" | "leave",
  paths: string[],
  position: { x: number; y: number } | null,
) {
  emitWebviewDragDropEvent({ type, paths, position });
}

function projectConversation(id: string, title: string, index: number): Task {
  return {
    id,
    projectId: "lilia",
    sessionId: `session-${id}`,
    title,
    status: "done",
    createdAt: 1000 + index,
    pinned: false,
    parentId: null,
    dependsOn: [],
  };
}

function pendingAskTimelineEvent(taskId: string, requestId: string) {
  return {
    id: `tl-pending-${requestId}`,
    taskId,
    turnId: "turn-ask",
    backend: "codex" as const,
    kind: "ask_user",
    status: "requires_action",
    title: "需要确认",
    summary: "继续？",
    payload: { requestId },
    createdAt: Date.now(),
    updatedAt: Date.now(),
    turnSeq: 1,
    intraTurnOrder: 0,
  };
}

function pendingToolTimelineEvent(taskId: string, requestId: string) {
  return {
    id: `tl-pending-${requestId}`,
    taskId,
    turnId: "turn-tool",
    backend: "codex" as const,
    kind: "command",
    status: "requires_action",
    title: "需要授权",
    summary: "运行命令？",
    payload: {
      interaction: "tool_consent",
      requestId,
    },
    createdAt: Date.now(),
    updatedAt: Date.now(),
    turnSeq: 1,
    intraTurnOrder: 0,
  };
}

function seedSecondaryPanelOverflowConversations() {
  TASKS.value = {
    ...TASKS.value,
    lilia: Array.from({ length: 6 }, (_, index) =>
      projectConversation(`t-overflow-${index + 1}`, `溢出对话 ${index + 1}`, index)
    ),
  };
}

function seedManyMockTasks(count: number) {
  setMockTasks(Array.from({ length: count }, (_, index) => ({
    id: `t-many-${index + 1}`,
    projectId: "lilia",
    sessionId: `session-many-${index + 1}`,
    title: `批量对话 ${index + 1}`,
    titleSource: "manual",
    status: "done",
    createdAt: 10_000 + index,
    parentId: null,
    dependsOn: [],
    sortOrder: index,
    pinned: false,
  })));
}

async function dragFromTo(source: HTMLElement, target: HTMLElement, targetY: number, targetX = 20) {
  await fireEvent.pointerDown(source, {
    button: 0,
    pointerId: 1,
    clientX: 20,
    clientY: 10,
  });
  await fireEvent.pointerMove(target, {
    pointerId: 1,
    clientX: targetX,
    clientY: targetY,
  });
  await fireEvent.pointerUp(target, {
    pointerId: 1,
    clientX: targetX,
    clientY: targetY,
  });
}

async function startDragOver(source: HTMLElement, target: HTMLElement, targetY: number, targetX = 20) {
  await fireEvent.pointerDown(source, {
    button: 0,
    pointerId: 1,
    clientX: 20,
    clientY: 10,
  });
  await fireEvent.pointerMove(target, {
    pointerId: 1,
    clientX: targetX,
    clientY: targetY,
  });
}

describe("SecondaryPanel project tree expansion", () => {
  beforeEach(() => {
    localStorage.clear();
    useSidebarDisplayMode().setSidebarDisplayMode("grouped");
    resetConversationActivity();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("卸载时取消侧栏 mount idle 打点", async () => {
    const cancelIdleCallback = vi.fn();
    vi.stubGlobal("requestIdleCallback", vi.fn(() => 64));
    vi.stubGlobal("cancelIdleCallback", cancelIdleCallback);
    const view = await renderSecondaryPanel();

    view.unmount();

    expect(cancelIdleCallback).toHaveBeenCalledWith(64);
  });

  it.skip("左下角连接徽章显示全局 active provider", async () => {
    setMockActiveBackend("native-agentkit");
    const view = await renderSecondaryPanel();

    await waitFor(() => {
      expect(view.getByRole("link", { name: /Codex/ })).toBeInTheDocument();
    }, { timeout: 2_000 });
  });

  it.skip("Codex app-server 环境不满足时左下角 provider 卡片标红", async () => {
    setMockActiveBackend("native-agentkit");
    setMockCodexAppServerStatus({
      supportsRequiredProtocol: false,
      failureKind: "experimentalApiUnsupported",
      issues: ["Codex app-server 环境不满足。"],
    });
    await useConnectionStatus().refresh();
    const view = await renderSecondaryPanel();

    await waitFor(() => {
      const card = view.getByRole("link", { name: /Codex app-server 环境不满足/ });
      expect(card).toHaveTextContent("异常");
      expect(card).toHaveAttribute("title", expect.stringContaining("Codex app-server 环境不满足"));
      expect(card).toHaveAttribute("title", expect.not.stringContaining("OpenAI Responses API"));
    });
  });

  it("会恢复上次关闭时的项目展开状态和收集箱状态", async () => {
    seedTreeExpansionState({
      projects: {
        lilia: false,
        tools: true,
      },
      orphansExpanded: false,
    });

    const view = await renderSecondaryPanel();

    const liliaRow = getProjectRow(view, "Lilia");
    const toolsRow = getProjectRow(view, "工具箱");
    const orphansToggle = view.getByRole("button", {
      name: "展开收集箱",
    });

    expect(liliaRow).toHaveAttribute("aria-expanded", "false");
    expect(toolsRow).toHaveAttribute("aria-expanded", "true");
    expect(orphansToggle).toBeInTheDocument();
  });

  it("首屏通过 Product Core 一次加载任务并填充所有已展开分组", async () => {
    seedTreeExpansionState({
      projects: {
        lilia: true,
        tools: true,
      },
      orphansExpanded: true,
    });
    TASKS.value = {};
    PROJECT_TASKS_LOADED.value = {};
    ORPHAN_LIST.value = [];
    ORPHANS_LOADED.value = false;
    mockInvoke.mockClear();

    const view = await renderSecondaryPanel("/projects/lilia");

    await findConversationRow(view, "接入 Claude Code 会话发现");
    await findConversationRow(view, "整理窗口快捷键");
    await findConversationRow(view, "随手问问 Claude：tsconfig paths");
    expect(mockInvoke.mock.calls.some(([cmd, args]) =>
      cmd === "product_list_entities" && args?.kind === "task"
    )).toBe(true);
  });

  it("统一模式卸载时取消延期 activity hydrate 的 paint 调度", async () => {
    try {
      seedManyMockTasks(30);
      useSidebarDisplayMode().setSidebarDisplayMode("unified");
      const cancelAnimationFrame = vi.fn();
      vi.stubGlobal("requestAnimationFrame", vi.fn(() => 71));
      vi.stubGlobal("cancelAnimationFrame", cancelAnimationFrame);

      const view = await renderSecondaryPanel();
      view.unmount();

      expect(cancelAnimationFrame).toHaveBeenCalledWith(71);
    } finally {
      resetTauriMockData();
      resetSidebarConversationsCache();
    }
  });


  it("用户切换后会写回本地存储", async () => {
    seedTreeExpansionState({
      projects: {
        lilia: true,
        tools: true,
      },
      orphansExpanded: true,
    });

    const view = await renderSecondaryPanel();
    await fireEvent.click(getProjectRow(view, "Lilia"));
    await fireEvent.click(view.getByRole("button", { name: "折叠收集箱" }));

    await waitFor(() => {
      const raw = localStorage.getItem("lilia.projectTree.expansion");
      expect(raw).toBeTruthy();
      const parsed = JSON.parse(raw ?? "{}") as {
        projects?: Record<string, boolean>;
        orphansExpanded?: boolean;
      };
      expect(parsed.projects?.lilia).toBe(false);
      expect(parsed.projects?.tools).toBe(true);
      expect(parsed.orphansExpanded).toBe(false);
    });
  });
});

describe("SecondaryPanel project chat navigation", () => {
  let unlistenConversationActivity: (() => void) | null = null;
  let unlistenAgentInteraction: (() => void) | null = null;

  beforeEach(async () => {
    localStorage.clear();
    useSidebarDisplayMode().setSidebarDisplayMode("grouped");
    resetConversationActivity();
    unlistenAgentInteraction = await installAgentInteractionBridge();
    unlistenConversationActivity = await installConversationActivityBridge();
  });

  afterEach(() => {
    unlistenAgentInteraction?.();
    unlistenConversationActivity?.();
    unlistenAgentInteraction = null;
    unlistenConversationActivity = null;
  });


  it("创建空分类后会自动进入该项目的新对话", async () => {
    const view = await renderSecondaryPanel();
    const pushSpy = vi.spyOn(view.router, "push").mockResolvedValue(undefined);

    await fireEvent.click(view.getByRole("button", { name: "添加项目" }));
    await fireEvent.click(await view.findByRole("menuitem", { name: "创建空分类" }));
    await fireEvent.update(
      await view.findByPlaceholderText("例如：实验、归档…"),
      "临时分类",
    );
    const createButton = view.getByRole("button", { name: "创建" });
    await waitFor(() => {
      expect(createButton).toBeEnabled();
    });
    await fireEvent.click(createButton);
    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd, args]) =>
        cmd === "product_create_entity" &&
        (args?.entity as { kind?: string; value?: { name?: string; workspacePath?: string | null } })
          ?.kind === "project" &&
        (args?.entity as { value?: { name?: string } }).value?.name === "临时分类" &&
        (args?.entity as { value?: { workspacePath?: string | null } }).value?.workspacePath === null
      )).toBe(true);
    });
    await waitFor(() => {
      expect(pushSpy).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(view.getByText("临时分类")).toBeInTheDocument();
      expect(pushSpy.mock.lastCall?.[0]).toMatch(
        /^\/projects\/project-[^/]+\/tasks\/t-draft-/,
      );
    });
  });

  it("添加项目菜单 Teleport 到 body，并保留 outside click / Escape 关闭", async () => {
    const view = await renderSecondaryPanel();

    await fireEvent.click(view.getByRole("button", { name: "添加项目" }));

    const menu = await view.findByRole("menu");
    expect(document.body.contains(menu)).toBe(true);
    expect(view.container.contains(menu)).toBe(false);

    await fireEvent.pointerDown(document.body);

    await waitFor(() => {
      expect(view.queryByRole("menu")).toBeNull();
    });

    await fireEvent.click(view.getByRole("button", { name: "添加项目" }));
    expect(await view.findByRole("menu")).toBeInTheDocument();

    await fireEvent.keyDown(document, { key: "Escape" });

    await waitFor(() => {
      expect(view.queryByRole("menu")).toBeNull();
    });
  });


  it("顶部搜索会话后点击结果进入对应对话并关闭搜索", async () => {
    const view = await renderSecondaryPanel();
    const pushSpy = vi.spyOn(view.router, "push").mockResolvedValue(undefined);
    mockInvoke.mockClear();

    await fireEvent.click(view.getByRole("button", { name: "搜索会话" }));
    await fireEvent.update(
      view.getByPlaceholderText("搜索会话…"),
      "tsconfig",
    );

    const listbox = await view.findByRole("listbox");
    await fireEvent.click(
      within(listbox).getByRole("option", {
        name: /打通 tsconfig paths 搜索/,
      }),
    );

    await waitFor(() => {
      expect(pushSpy).toHaveBeenCalledWith("/projects/lilia/tasks/t-002");
    });
    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === TASK_LIST_SIDEBAR_CONVERSATIONS_COMMAND)).toBe(true);
    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === TASK_LIST_COMMAND)).toBe(false);
    expect(view.queryByPlaceholderText("搜索会话…")).not.toBeInTheDocument();
    expect(view.getByRole("button", { name: "搜索会话" })).toBeInTheDocument();
  });


  it("归档当前打开的单条项目对话后会进入该项目的新对话", async () => {
    const view = await renderSecondaryPanel("/projects/lilia/tasks/t-001");
    const row = await findConversationRow(view, "接入 Claude Code 会话发现");

    await fireEvent.click(within(row).getByRole("button", { name: "归档" }));
    await fireEvent.click(
      within(row).getByRole("button", { name: "确认归档，再点一次" }),
    );

    await waitFor(() => {
      expect(view.router.currentRoute.value.path).toMatch(
        /^\/projects\/lilia\/tasks\/t-draft-/,
      );
    });
  });

  it("中键点击收集箱对话会在弹出窗口中打开", async () => {
    const view = await renderSecondaryPanel();
    const row = getConversationRow(view, "随手问问 Claude：tsconfig paths");

    await fireEvent(
      row,
      new MouseEvent("auxclick", { bubbles: true, button: 1 }),
    );

    expect(mockInvoke).toHaveBeenCalledWith(POPUP_OPEN_TASK_COMMAND, {
      projectId: null,
      taskId: "o-001",
    }, undefined);
  });

  it("统一列表模式隐藏项目分组和收集箱，并在项目会话右侧显示项目名", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    mockInvoke.mockClear();
    const view = await renderSecondaryPanel();

    expect(view.getByText("会话")).toBeInTheDocument();
    expect(view.queryByText("收集箱")).not.toBeInTheDocument();
    expect(view.queryByRole("button", { name: "添加项目" })).not.toBeInTheDocument();
    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === TASK_LIST_SIDEBAR_CONVERSATIONS_COMMAND)).toBe(true);
    });
    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === TASK_LIST_COMMAND)).toBe(false);

    const projectRow = await view.findByText("打通 tsconfig paths 搜索");
    const projectConversation = projectRow.closest(".sb-tree__row--unified");
    expect(projectConversation).toBeInstanceOf(HTMLElement);
    expect(within(projectConversation as HTMLElement).getByText("Lilia")).toHaveClass(
      "sb-tree__project-label",
    );

    const orphan = getConversationRow(view, "随手问问 Claude：tsconfig paths");
    expect(within(orphan).queryByText("Lilia")).not.toBeInTheDocument();
  });

  it("统一列表项目会话仍能把项目归属传给弹出窗口", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    const view = await renderSecondaryPanel();
    const row = getConversationRow(view, "整理窗口快捷键");

    await fireEvent(
      row,
      new MouseEvent("auxclick", { bubbles: true, button: 1 }),
    );

    expect(mockInvoke).toHaveBeenCalledWith(POPUP_OPEN_TASK_COMMAND, {
      projectId: "tools",
      taskId: "t-003",
    }, undefined);
  });

  it("非选中统一列表对话会在行尾显示运行中、等待交互和完成状态", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    const view = await renderSecondaryPanel();
    const row = getConversationRow(view, "整理窗口快捷键");

    emitTauriEvent(CHAT_TURN_STARTED_EVENT_NAME, { taskId: "t-003", queuedCount: 0 });
    await waitFor(() => {
      expect(within(row).getByLabelText("对话中")).toBeInTheDocument();
    });

    emitTauriEvent(CHAT_AGENT_INTERACTION_REQUEST_EVENT_NAME, {
      taskId: "t-003",
      turnId: "turn-1",
      backend: "codex",
      requestId: "ask-1",
      kind: "ask_user",
      payload: {
        title: "需要确认",
        intent: "ask_user",
        questions: [{ id: "q1", question: "继续？", mode: "confirm" }],
      },
    });
    await waitFor(() => {
      expect(within(row).getByLabelText("等待交互")).toBeInTheDocument();
    });

    emitTauriEvent(CHAT_DONE_EVENT_NAME, { taskId: "t-003", sessionId: null, subtype: null });
    await waitFor(() => {
      expect(within(row).getByLabelText("对话完成")).toBeInTheDocument();
    });
  });

  it("统一列表冷启动时会从 runtime snapshot 恢复运行中状态", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    setMockChatRunning("t-003", true);
    const view = await renderSecondaryPanel();
    const row = getConversationRow(view, "整理窗口快捷键");

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_GET_RUNTIME_SNAPSHOT_COMMAND))
        .toBe(true);
      expect(within(row).getByLabelText("对话中")).toBeInTheDocument();
    });
  });

  it("统一列表运行进程可以从进行中区域强行停止", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    setMockChatRunning("t-003", true);
    const view = await renderSecondaryPanel();

    const row = await findRunningProcessRow(view, "整理窗口快捷键");
    await fireEvent.click(within(row).getByRole("button", { name: "强行停止进程" }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(CHAT_INTERRUPT_TURN_COMMAND, {
        taskId: "t-003",
      }, undefined);
    });
    await waitFor(() => {
      expect(view.queryByRole("button", { name: "强行停止进程" })).not.toBeInTheDocument();
    });
  });

  it("统一列表冷启动时会把 reset_pending_finish runtime snapshot 显示为运行中", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    setMockRuntimeSnapshot("t-003", {
      phase: "reset_pending_finish",
      backend: "codex",
      turnId: "turn-reset",
      pendingResetCleanup: true,
    });

    const view = await renderSecondaryPanel();
    const row = getConversationRow(view, "整理窗口快捷键");

    await waitFor(() => {
      expect(within(row).getByLabelText("对话中")).toBeInTheDocument();
    });
  });

  it("统一列表冷启动时会把 abandoned runtime snapshot 显示为错误状态", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    setMockRuntimeSnapshot("t-003", {
      phase: "abandoned",
      backend: "codex",
      turnId: "turn-old",
    });

    const view = await renderSecondaryPanel();
    const row = getConversationRow(view, "整理窗口快捷键");

    await waitFor(() => {
      expect(within(row).getByLabelText("发生错误")).toBeInTheDocument();
    });
  });

  it("统一列表冷启动时 abandoned runtime snapshot 会覆盖持久化等待交互", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    setMockRuntimeSnapshot("t-003", {
      phase: "abandoned",
      backend: "codex",
      turnId: "turn-old",
    });
    replaceMockTimelineEvents("t-003", [pendingAskTimelineEvent("t-003", "ask-overridden")]);

    const view = await renderSecondaryPanel();
    const row = getConversationRow(view, "整理窗口快捷键");

    await waitFor(() => {
      expect(within(row).getByLabelText("发生错误")).toBeInTheDocument();
      expect(within(row).queryByLabelText("等待交互")).toBeNull();
    });
  });

  it("统一列表冷启动时会从持久化 timeline 恢复等待交互状态", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    replaceMockTimelineEvents("t-003", [pendingAskTimelineEvent("t-003", "ask-1")]);

    const askView = await renderSecondaryPanel();
    const askRow = getConversationRow(askView, "整理窗口快捷键");
    await waitFor(() => {
      expect(within(askRow).getByLabelText("等待交互")).toBeInTheDocument();
    });
  });

  it("统一列表冷启动时会从持久化 timeline 恢复错误状态", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    resetConversationActivity();
    replaceMockTimelineEvents("t-003", [{
      id: "tl-error-hydrated",
      taskId: "t-003",
      turnId: "turn-error",
      backend: "codex",
      kind: "error",
      status: "error",
      title: "错误",
      summary: "Agent 发生错误",
      payload: {},
      createdAt: Date.now(),
      updatedAt: Date.now(),
      turnSeq: 1,
      intraTurnOrder: 0,
    }]);

    const errorView = await renderSecondaryPanel();
    const errorRow = getConversationRow(errorView, "整理窗口快捷键");
    await waitFor(() => {
      expect(within(errorRow).getByLabelText("发生错误")).toBeInTheDocument();
    });
  });

  it("项目列表冷启动时会从 runtime snapshot 恢复项目会话运行中状态", async () => {
    seedTreeExpansionState({
      projects: {
        lilia: false,
        tools: true,
      },
      orphansExpanded: true,
    });
    setMockRuntimeSnapshot("t-003", {
      phase: "running",
      backend: "codex",
      turnId: "turn-project",
    });

    const view = await renderSecondaryPanel();
    const row = await findConversationRow(view, "整理窗口快捷键");

    await waitFor(() => {
      expect(within(row).getByLabelText("对话中")).toBeInTheDocument();
    });
  });

  it("项目列表冷启动时会从持久化 timeline 恢复收集箱等待交互状态", async () => {
    replaceMockTimelineEvents("o-001", [pendingToolTimelineEvent("o-001", "tool-orphan")]);

    const view = await renderSecondaryPanel();
    const row = getConversationRow(view, "随手问问 Claude：tsconfig paths");

    await waitFor(() => {
      expect(within(row).getByLabelText("等待交互")).toBeInTheDocument();
    });
  });

  it("结束事件会清理等待交互并显示完成状态", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    const view = await renderSecondaryPanel();
    const row = getConversationRow(view, "整理窗口快捷键");

    emitMockTimelineEvent("t-003", {
      ...pendingToolTimelineEvent("t-003", "tool-live"),
      id: "tl-live-pending-tool",
    });
    await waitFor(() => {
      expect(within(row).getByLabelText("等待交互")).toBeInTheDocument();
    });

    emitTauriEvent(CHAT_DONE_EVENT_NAME, { taskId: "t-003", sessionId: null, subtype: null });
    await waitFor(() => {
      expect(within(row).getByLabelText("对话完成")).toBeInTheDocument();
      expect(within(row).queryByLabelText("等待交互")).toBeNull();
    });
  });

  it("发生错误时显示错误图标，并且结束事件不会覆盖错误状态", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    const view = await renderSecondaryPanel();
    const row = getConversationRow(view, "整理窗口快捷键");

    emitTauriEvent(CHAT_TURN_STARTED_EVENT_NAME, { taskId: "t-003", queuedCount: 0 });
    emitMockTimelineEvent("t-003", {
      id: "tl-error-activity",
      kind: "error",
      status: "error",
      title: "错误",
      summary: "Agent 发生错误",
      turnId: "turn-1",
    });

    await waitFor(() => {
      expect(within(row).getByLabelText("发生错误")).toBeInTheDocument();
    });

    emitTauriEvent(CHAT_DONE_EVENT_NAME, { taskId: "t-003", sessionId: null, subtype: null });
    await waitFor(() => {
      expect(within(row).getByLabelText("发生错误")).toBeInTheDocument();
      expect(within(row).queryByLabelText("对话完成")).toBeNull();
    });
  });

  it("完成图标会保留到用户进入该对话，选中对话隐藏状态", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    const view = await renderSecondaryPanel();
    const row = getConversationRow(view, "整理窗口快捷键");

    emitTauriEvent(CHAT_DONE_EVENT_NAME, { taskId: "t-003", sessionId: null, subtype: null });
    await waitFor(() => {
      expect(within(row).getByLabelText("对话完成")).toBeInTheDocument();
    });

    await fireEvent.click(row);

    await waitFor(() => {
      expect(view.router.currentRoute.value.path).toBe("/projects/tools/tasks/t-003");
      expect(within(row).queryByLabelText("对话完成")).toBeNull();
    });

    emitTauriEvent(CHAT_TURN_STARTED_EVENT_NAME, { taskId: "t-003", queuedCount: 0 });
    await waitFor(() => {
      expect(within(row).queryByLabelText("对话中")).toBeNull();
    });
  });

  it("切换侧边栏对话不会调用打断接口", async () => {
    const view = await renderSecondaryPanel("/projects/lilia/tasks/t-001");
    const row = await findConversationRow(view, "打通 tsconfig paths 搜索");
    mockInvoke.mockClear();

    await fireEvent.click(row);

    await waitFor(() => {
      expect(view.router.currentRoute.value.path).toBe("/projects/lilia/tasks/t-002");
    });
    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CHAT_INTERRUPT_TURN_COMMAND))
      .toBe(false);
  });
});

describe("SecondaryPanel project tree drag", () => {
  beforeEach(() => {
    localStorage.clear();
    useSidebarDisplayMode().setSidebarDisplayMode("grouped");
    resetConversationActivity();
    seedTreeExpansionState({
      projects: {
        lilia: true,
        tools: true,
      },
      orphansExpanded: true,
    });
  });


  it("项目拖动时显示落点指示，松手后更新同置顶分组顺序", async () => {
    const view = await renderSecondaryPanel();
    const lilia = getProjectRow(view, "Lilia");
    const tools = getProjectRow(view, "工具箱");
    lilia.getBoundingClientRect = () => box(0, 28);
    tools.getBoundingClientRect = () => box(40, 68);

    await fireEvent.pointerDown(lilia, {
      button: 0,
      pointerId: 1,
      clientX: 20,
      clientY: 10,
    });
    await fireEvent.pointerMove(tools, {
      pointerId: 1,
      clientX: 20,
      clientY: 60,
    });

    expect(lilia).toHaveClass("is-tree-drag-source");
    expect(tools).toHaveClass("is-tree-drop-target");
    expect(tools).toHaveClass("is-tree-drop-after");
    expect(tools).not.toHaveClass("is-tree-drop-invalid");

    await fireEvent.pointerUp(tools, {
      pointerId: 1,
      clientX: 20,
      clientY: 60,
    });

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd, args]) =>
        cmd === "product_update_entity" &&
        args?.action === "reordered" &&
        (args?.entity as { kind?: string; value?: { id?: string; sortOrder?: number } })?.kind ===
          "project" &&
        (args?.entity as { value?: { id?: string } }).value?.id === "tools" &&
        (args?.entity as { value?: { sortOrder?: number } }).value?.sortOrder === 0
      )).toBe(true);
    });
  });

  it("任务拖入另一任务时会设置父任务", async () => {
    const view = await renderSecondaryPanel();
    const source = await findConversationRow(view, "整理窗口快捷键");
    const target = await findConversationRow(view, "接入 Claude Code 会话发现");
    target.getBoundingClientRect = () => box(40, 68);
    mockInvoke.mockClear();

    await startDragOver(source, target, 54, 80);

    await waitFor(() => {
      expect(target).toHaveClass("is-tree-drop-inside");
      expect(target).not.toHaveClass("is-tree-drop-invalid");
    });

    await fireEvent.pointerUp(target, {
      pointerId: 1,
      clientX: 80,
      clientY: 54,
    });

    await waitFor(() => {
      const args = mockInvoke.mock.calls.find(([cmd]) => cmd === TASK_REPARENT_COMMAND)?.[1];
      expect(args).toEqual({
        taskId: "t-003",
        newProjectId: "lilia",
        newParentId: "t-001",
      });
      expect(getTask("lilia", "t-003")?.parentId).toBe("t-001");
    });
  });

  it("任务拖到任务前方时会继承目标父级并重排", async () => {
    const view = await renderSecondaryPanel();
    const source = await findConversationRow(view, "打通 tsconfig paths 搜索");
    const target = await findConversationRow(view, "接入 Claude Code 会话发现");
    target.getBoundingClientRect = () => box(40, 68);
    mockInvoke.mockClear();

    await dragFromTo(source, target, 42);

    await waitFor(() => {
      expect(getTask("lilia", "t-002")?.parentId).toBeNull();
      const ids = listTasks("lilia").map((task) => task.id);
      expect(ids.indexOf("t-002")).toBeLessThan(ids.indexOf("t-001"));
    });
  });

  it("任务拖到自身时显示无效落点并不会落库", async () => {
    const view = await renderSecondaryPanel();
    const row = await findConversationRow(view, "接入 Claude Code 会话发现");
    row.getBoundingClientRect = () => box(40, 68);
    mockInvoke.mockClear();

    await startDragOver(row, row, 54, 80);

    await waitFor(() => {
      expect(row).toHaveClass("is-tree-drop-invalid");
    });

    await fireEvent.pointerUp(row, {
      pointerId: 1,
      clientX: 80,
      clientY: 54,
    });

    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === TASK_REPARENT_COMMAND)).toBe(false);
  });

  it("文件夹拖到未置顶项目行时显示项目落点，松手后创建并插入", async () => {
    const view = await renderSecondaryPanel();
    const tools = getProjectRow(view, "工具箱");
    tools.getBoundingClientRect = () => box(40, 68);
    mockProjectTreeDropHit(tools);
    mockInvoke.mockClear();

    emitFolderDropEvent("over", ["D:\\PROJECT\\workspace\\NewApp"], { x: 20, y: 42 });

    await waitFor(() => {
      expect(tools).toHaveClass("is-tree-drop-target");
      expect(tools).toHaveClass("is-tree-drop-before");
      expect(tools).not.toHaveClass("is-tree-drop-invalid");
    });

    emitFolderDropEvent("drop", ["D:\\PROJECT\\workspace\\NewApp"], { x: 20, y: 42 });

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd, args]) =>
        cmd === "product_create_entity" &&
        (args?.entity as { kind?: string; value?: { workspacePath?: string | null } })?.kind ===
          "project" &&
        (args?.entity as { value?: { workspacePath?: string | null } }).value?.workspacePath ===
          "D:\\PROJECT\\workspace\\NewApp"
      )).toBe(true);
      expect(mockInvoke.mock.calls.some(([cmd, args]) =>
        cmd === "product_update_entity" && args?.action === "reordered"
      )).toBe(true);
    });
  });

  it("一次拖入多个文件夹时按拖入顺序连续插入", async () => {
    const view = await renderSecondaryPanel();
    const tools = getProjectRow(view, "工具箱");
    tools.getBoundingClientRect = () => box(40, 68);
    mockProjectTreeDropHit(tools);
    mockInvoke.mockClear();

    emitFolderDropEvent("drop", [
      "D:\\PROJECT\\workspace\\Alpha",
      "D:\\PROJECT\\workspace\\Beta",
    ], { x: 20, y: 42 });

    await waitFor(() => {
      const createdPaths = mockInvoke.mock.calls
        .filter(([cmd, args]) =>
          cmd === "product_create_entity" &&
          (args?.entity as { kind?: string })?.kind === "project"
        )
        .map(([, args]) =>
          (args?.entity as { value?: { workspacePath?: string | null } }).value?.workspacePath
        );
      expect(createdPaths).toEqual([
        "D:\\PROJECT\\workspace\\Alpha",
        "D:\\PROJECT\\workspace\\Beta",
      ]);
      expect(mockInvoke.mock.calls.some(([cmd, args]) =>
        cmd === "product_update_entity" && args?.action === "reordered"
      )).toBe(true);
    });
  });

  it("文件夹拖到置顶项目行时显示无效落点且不创建项目", async () => {
    setMockProjectPinned("tools", true);
    const view = await renderSecondaryPanel();
    const tools = getProjectRow(view, "工具箱");
    tools.getBoundingClientRect = () => box(40, 68);
    mockProjectTreeDropHit(tools);
    mockInvoke.mockClear();

    emitFolderDropEvent("over", ["D:\\PROJECT\\workspace\\PinnedDrop"], { x: 20, y: 42 });

    await waitFor(() => {
      expect(tools).toHaveClass("is-tree-drop-target");
      expect(tools).toHaveClass("is-tree-drop-invalid");
    });

    emitFolderDropEvent("drop", ["D:\\PROJECT\\workspace\\PinnedDrop"], { x: 20, y: 42 });
    await Promise.resolve();
    await Promise.resolve();

    expect(mockInvoke.mock.calls.some(([cmd, args]) =>
      cmd === "product_create_entity" &&
      (args?.entity as { kind?: string })?.kind === "project"
    )).toBe(false);
    expect(mockInvoke.mock.calls.some(([cmd, args]) =>
      cmd === "product_update_entity" && args?.action === "reordered"
    )).toBe(false);
  });

  it("文件夹拖放按窗口缩放归一化坐标后解析项目落点", async () => {
    setMockWindowScaleFactor(2);
    const view = await renderSecondaryPanel();
    const tools = getProjectRow(view, "工具箱");
    tools.getBoundingClientRect = () => box(40, 68);
    mockProjectTreeDropHit(tools);
    mockInvoke.mockClear();

    emitFolderDropEvent("drop", ["D:\\PROJECT\\workspace\\Scaled"], { x: 40, y: 84 });

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd, args]) =>
        cmd === "product_update_entity" && args?.action === "reordered"
      )).toBe(true);
    });
  });
});

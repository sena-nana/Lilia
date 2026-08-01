import { fireEvent, render, waitFor } from "@testing-library/vue";
import { nextTick } from "vue";
import { createMemoryHistory } from "vue-router";
import { defineComponent } from "vue";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  POPUP_FOCUS_MAIN_COMMAND,
  POPUP_OPEN_NEW_CHAT_COMMAND,
  POPUP_OPEN_TASK_COMMAND,
} from "@lilia/contracts";
import { PRODUCT_GET_ENTITY_COMMAND } from "@lilia/contracts/productCoreContract.mjs";
import {
  createLiliaRouter,
  shouldUsePopupHashHistory,
} from "../src/router";
import {
  mockCurrentWindow,
  mockInvoke,
  setMockAgentTimelineDelay,
  setMockTasks,
  setMockTaskArchived,
} from "./tauriMock";
import { ORPHAN_LIST, TASKS } from "../src/data/tasks";

afterEach(() => {
  vi.unstubAllGlobals();
});

async function renderPopup(initialRoute = "/popup/projects/lilia/tasks/t-001") {
  const router = createLiliaRouter(createMemoryHistory());
  await router.push(initialRoute);
  await router.isReady();

  const Root = defineComponent({
    template: "<RouterView />",
  });

  return {
    ...render(Root, {
      global: {
        plugins: [router],
      },
    }),
    router,
  };
}

async function expectReturnToMainRoute(initialRoute: string, mainRoute: string) {
  const view = await renderPopup(initialRoute);

  await fireEvent.click(view.getByRole("button", { name: "回到主窗口" }));

  await waitFor(() => {
    expect(mockInvoke).toHaveBeenCalledWith(POPUP_FOCUS_MAIN_COMMAND, {
      route: mainRoute,
    }, undefined);
    expect(mockCurrentWindow.close).toHaveBeenCalledTimes(1);
  });
}

function seedPersistedDraftPrefixTask() {
  TASKS.value = {
    ...TASKS.value,
    lilia: [
      {
        id: "t-draft-persisted",
        projectId: "lilia",
        sessionId: "persisted-session",
        title: "已发送的草稿前缀对话",
        status: "done",
        createdAt: 5000,
        pinned: false,
        parentId: null,
        dependsOn: [],
      },
      ...(TASKS.value.lilia ?? []),
    ],
  };
}

function encodeDraftQueryText(text: string): string {
  const bytes = new TextEncoder().encode(text);
  const binary = Array.from(bytes, (byte) => String.fromCharCode(byte)).join("");
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function composerInput(view: ReturnType<typeof render>): HTMLElement | null {
  return view.container.querySelector(".chat-composer [role='textbox']");
}

function expectComposerFocused(view: ReturnType<typeof render>) {
  const input = composerInput(view);
  expect(input).toBeInstanceOf(HTMLElement);
  expect(document.activeElement).toBe(input);
  return input as HTMLElement;
}

describe("Popup shell", () => {
  it("弹窗 index hash 入口使用 hash history", () => {
    expect(shouldUsePopupHashHistory("#/popup/chats/new")).toBe(true);
    expect(shouldUsePopupHashHistory("#/projects/lilia")).toBe(false);
    expect(shouldUsePopupHashHistory("")).toBe(false);
  });

  it("弹出窗口路由只渲染对话主体，不渲染左右侧栏", async () => {
    const view = await renderPopup();

    expect(view.getByRole("button", { name: "关闭弹出窗口" })).toBeInTheDocument();
    expect(view.getByRole("button", { name: "新对话" })).toBeInTheDocument();
    expect(view.getByRole("button", { name: "回到主窗口" })).toBeInTheDocument();
    expect(view.queryByRole("button", { name: "折叠左侧栏" })).not.toBeInTheDocument();
  });

  it("对话状态悬浮窗路由只渲染状态列表", async () => {
    const view = await renderPopup("/popup/status");

    expect(view.getByRole("button", { name: "新对话" })).toBeInTheDocument();
    expect(view.getByRole("button", { name: "关闭对话悬浮窗" })).toBeInTheDocument();
    expect(view.queryByRole("button", { name: "回到主窗口" })).not.toBeInTheDocument();
    expect(view.queryByText("对话状态")).not.toBeInTheDocument();
    expect(view.queryByText("要在 Lilia 中构建什么？")).not.toBeInTheDocument();

    await waitFor(() => {
      expect(view.getByRole("button", { name: /接入 Claude Code 会话发现/ }))
        .toBeInTheDocument();
      expect(view.getAllByText("Lilia").length).toBeGreaterThan(0);
      expect(view.getByRole("button", { name: /随手问问 Claude/ })).toBeInTheDocument();
      expect(view.getByText("收集箱")).toBeInTheDocument();
    });
  });

  it("对话状态悬浮窗点击对话会打开对话弹出窗口", async () => {
    const view = await renderPopup("/popup/status");

    await fireEvent.click(await view.findByRole("button", {
      name: /接入 Claude Code 会话发现/,
    }));

    expect(mockInvoke).toHaveBeenCalledWith(POPUP_OPEN_TASK_COMMAND, {
      projectId: "lilia",
      taskId: "t-001",
    }, undefined);
  });

  it("对话状态悬浮窗空列表显示空状态", async () => {
    setMockTasks([]);
    TASKS.value = {
      lilia: [],
      tools: [],
    };
    ORPHAN_LIST.value = [];

    const view = await renderPopup("/popup/status");

    await waitFor(() => {
      expect(view.getByText("还没有会话")).toBeInTheDocument();
    });
  });

  it("对话状态悬浮窗顶部控件支持置顶、透明度、新对话和关闭", async () => {
    localStorage.removeItem("lilia.conversationStatus.alwaysOnTop");
    localStorage.removeItem("lilia.conversationStatus.opacity");
    localStorage.removeItem("lilia.conversationStatus.geometry");
    const view = await renderPopup("/popup/status");

    await waitFor(() => {
      expect(mockCurrentWindow.setAlwaysOnTop).toHaveBeenCalledWith(false);
    });

    await fireEvent.click(view.getByRole("button", { name: "置顶" }));
    expect(mockCurrentWindow.setAlwaysOnTop).toHaveBeenLastCalledWith(true);
    expect(localStorage.getItem("lilia.conversationStatus.alwaysOnTop")).toBe("1");

    await fireEvent.click(view.getByRole("button", { name: /窗口透明度/ }));
    const opacitySlider = view.getByRole("slider", { name: "窗口透明度" });
    expect(opacitySlider).toHaveAttribute("min", "0.4");
    await fireEvent.update(opacitySlider, "0.92");
    expect(localStorage.getItem("lilia.conversationStatus.opacity")).toBe("0.92");
    await fireEvent.update(opacitySlider, "0.4");
    expect(localStorage.getItem("lilia.conversationStatus.opacity")).toBe("0.40");

    await fireEvent.click(view.getByRole("button", { name: "新对话" }));
    expect(mockInvoke).toHaveBeenCalledWith(POPUP_OPEN_NEW_CHAT_COMMAND, {
      projectId: null,
      initialDraftContent: null,
    }, undefined);

    await fireEvent.click(view.getByRole("button", { name: "关闭对话悬浮窗" }));
    expect(mockCurrentWindow.close).toHaveBeenCalledTimes(1);
  });

  it("对话状态悬浮窗会记忆窗口大小和位置", async () => {
    localStorage.setItem("lilia.conversationStatus.geometry", JSON.stringify({
      x: 120,
      y: 140,
      width: 420,
      height: 480,
    }));

    await renderPopup("/popup/status");

    await waitFor(() => {
      expect(mockCurrentWindow.setSize).toHaveBeenCalledWith(expect.objectContaining({
        width: 420,
        height: 480,
      }));
      expect(mockCurrentWindow.setPosition).toHaveBeenCalledWith(expect.objectContaining({
        x: 120,
        y: 140,
      }));
      expect(mockCurrentWindow.onResized).toHaveBeenCalled();
      expect(mockCurrentWindow.onMoved).toHaveBeenCalled();
    });

    mockCurrentWindow.outerPosition.mockResolvedValueOnce({ x: 188, y: 212 });
    mockCurrentWindow.innerSize.mockResolvedValueOnce({ width: 366, height: 444 });
    const onResized = mockCurrentWindow.onResized.mock.calls[0]?.[0] as (() => void) | undefined;
    onResized?.();

    await waitFor(() => {
      expect(localStorage.getItem("lilia.conversationStatus.geometry")).toBe(JSON.stringify({
        x: 188,
        y: 212,
        width: 366,
        height: 444,
      }));
    });
  });

  it("弹窗打开已有对话时可用单条查询补齐当前上下文", async () => {
    TASKS.value = {};

    const view = await renderPopup("/popup/projects/lilia/tasks/t-001");

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        PRODUCT_GET_ENTITY_COMMAND,
        { kind: "task", id: "t-001" },
        undefined,
      );
      expect(view.getByText("要在 Lilia 中构建什么？")).toBeInTheDocument();
    }, { timeout: 10_000 });
  });

  it("弹窗已有对话等待首批内容时不显示新对话空状态", async () => {
    vi.useFakeTimers();
    try {
      setMockAgentTimelineDelay(700);

      const view = await renderPopup("/popup/projects/lilia/tasks/t-002");

      await waitFor(() => {
        expect(view.container.querySelector(".chat-page--pending")).toBeInTheDocument();
        expect(view.queryByText("要在 Lilia 中构建什么？")).not.toBeInTheDocument();
      });

      await vi.advanceTimersByTimeAsync(700);
      await nextTick();
      await nextTick();
      await waitFor(() => {
        expect(view.container.querySelector(".chat-page--pending")).not.toBeInTheDocument();
        expect(view.container.querySelector(".chat-composer")).toBeInTheDocument();
        expect(view.queryByText("要在 Lilia 中构建什么？")).not.toBeInTheDocument();
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it.each([
    ["/popup/projects/lilia/tasks/t-001", "/projects/lilia/tasks/t-001"],
    ["/popup/chats/o-001", "/chats/o-001"],
    ["/popup/projects/lilia/tasks/t-draft-temp", "/projects/lilia"],
    ["/popup/chats/o-draft-temp", "/"],
  ])("回到主窗口会把 %s 映射到 %s", async (popupRoute, mainRoute) => {
    await expectReturnToMainRoute(popupRoute, mainRoute);
  });

  it("弹窗被重新导航到对话后，回到主窗口会聚焦该对话", async () => {
    const view = await renderPopup("/popup/chats/o-001");

    await view.router.replace("/popup/projects/lilia/tasks/t-001");
    await fireEvent.click(view.getByRole("button", { name: "回到主窗口" }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(POPUP_FOCUS_MAIN_COMMAND, {
        route: "/projects/lilia/tasks/t-001",
      }, undefined);
      expect(mockCurrentWindow.close).toHaveBeenCalledTimes(1);
    });
  });

  it("弹窗可打开已持久化的草稿前缀对话，不会误跳到新对话", async () => {
    const route = "/popup/projects/lilia/tasks/t-draft-persisted";
    seedPersistedDraftPrefixTask();

    const view = await renderPopup(route);

    await waitFor(() => {
      expect(view.router.currentRoute.value.fullPath).toBe(route);
      expect(view.queryByText("正在创建新对话…")).not.toBeInTheDocument();
    });
  });

  it("已持久化的草稿前缀对话回到主窗口时会聚焦对话而不是项目", async () => {
    seedPersistedDraftPrefixTask();

    await expectReturnToMainRoute(
      "/popup/projects/lilia/tasks/t-draft-persisted",
      "/projects/lilia/tasks/t-draft-persisted",
    );
  });

  it("刷新到已丢失的弹窗草稿时会重新创建窗口内草稿", async () => {
    const lostDraftRoute = "/popup/projects/lilia/tasks/t-draft-missing-refresh";
    const view = await renderPopup(lostDraftRoute);

    await waitFor(() => {
      expect(view.router.currentRoute.value.fullPath).not.toBe(lostDraftRoute);
      expect(view.router.currentRoute.value.fullPath).toMatch(
        /^\/popup\/projects\/lilia\/tasks\/t-draft-/,
      );
    });
  });

  it("弹窗收集箱新对话 boot 到真实对话后自动聚焦输入框", async () => {
    const view = await renderPopup("/popup/chats/new");

    await waitFor(() => {
      expect(view.router.currentRoute.value.fullPath).toMatch(/^\/popup\/chats\/o-draft-/);
      expectComposerFocused(view);
    });
  });

  it("弹窗项目新对话 boot 到真实对话后自动聚焦输入框", async () => {
    const view = await renderPopup("/popup/projects/lilia/new");

    await waitFor(() => {
      expect(view.router.currentRoute.value.fullPath).toMatch(
        /^\/popup\/projects\/lilia\/tasks\/t-draft-/,
      );
      expectComposerFocused(view);
    });
  });

  it("弹窗初始草稿文本插入后焦点停留在输入框", async () => {
    const text = "继续追一下弹窗草稿";
    const view = await renderPopup(`/popup/chats/new?draft=${encodeDraftQueryText(text)}`);

    await waitFor(() => {
      const input = expectComposerFocused(view);
      expect(input).toHaveTextContent(text);
    });
  });

  it("归档对话不会被单条加载为可打开对话", async () => {
    setMockTaskArchived("t-001", true);
    TASKS.value = {
      ...TASKS.value,
      lilia: (TASKS.value.lilia ?? []).filter((task) => task.id !== "t-001"),
    };

    const view = await renderPopup("/popup/projects/lilia/tasks/t-001");

    await waitFor(() => {
      expect(view.getByText(/未找到任务/)).toBeInTheDocument();
    });
  });
});

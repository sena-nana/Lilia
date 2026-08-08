import { fireEvent, render, waitFor } from "@testing-library/vue";
import { createMemoryHistory } from "vue-router";
import { defineComponent } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  AGENT_TIMELINE_LIST_COMMAND,
  CHAT_GET_RUNTIME_SNAPSHOT_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND,
} from "@lilia/contracts";
import TaskDetail from "../src/pages/TaskDetail.vue";
import { useSidebarDisplayMode } from "../src/composables/useSidebarDisplayMode";
import { createLiliaRouter } from "../src/router";
import { createDraftOrphan, createDraftTask } from "../src/services/tasksStore";
import {
  failNextMockConversationSuggestions,
  mockInvoke,
  setMockConversationSuggestionSources,
  setMockConversationSuggestions,
  setMockConversationSuggestionsDelay,
} from "./tauriMock";

function getComposerTextbox(view: ReturnType<typeof render>): HTMLElement {
  const element = view.container.querySelector(".chat-composer [role='textbox']");
  expect(element).toBeInstanceOf(HTMLElement);
  return element as HTMLElement;
}

async function renderProjectDraftTaskDetail(taskId: string) {
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
  await vi.dynamicImportSettled();
  await waitFor(() => {
    expect(getComposerTextbox(view)).toBeInTheDocument();
  }, { timeout: 3000 });
  return view;
}

async function renderOrphanDraftTaskDetail(taskId: string) {
  const router = createLiliaRouter(createMemoryHistory());
  await router.push(`/chats/${taskId}`);
  await router.isReady();

  const view = render(TaskDetail, {
    props: {
      taskId,
    },
    global: {
      plugins: [router],
    },
  });
  await vi.dynamicImportSettled();
  await waitFor(() => {
    expect(getComposerTextbox(view)).toBeInTheDocument();
  }, { timeout: 3000 });
  return view;
}

async function renderRouterView(initialRoute: string) {
  const router = createLiliaRouter(createMemoryHistory());
  await router.push(initialRoute);
  await router.isReady();

  const Wrapper = defineComponent({
    template: "<RouterView />",
  });

  const view = render(Wrapper, {
    global: {
      plugins: [router],
    },
  });
  await vi.dynamicImportSettled();
  await waitFor(() => {
    expect(getComposerTextbox(view)).toBeInTheDocument();
  }, { timeout: 3000 });
  return { ...view, router };
}

function expectComposerFocused(view: ReturnType<typeof render>) {
  const input = getComposerTextbox(view);
  expect(document.activeElement).toBe(input);
}

describe("TaskDetail conversation suggestions", () => {
  beforeEach(() => {
    setMockConversationSuggestions(null);
    setMockConversationSuggestionSources(null);
    setMockConversationSuggestionsDelay(0);
  });

  it("项目空白草稿会在空态标题下方加载并展示新对话建议", async () => {
    const draft = createDraftTask("lilia");
    const view = await renderProjectDraftTaskDetail(draft.id);

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND))
        .toBe(true);
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CONVERSATION_SUGGESTIONS_GET_COMMAND))
        .toBe(true);
      expect(view.getByRole("button", { name: "补齐建议缓存测试" })).toBeInTheDocument();
    });
    expect(mockInvoke.mock.calls.some(([cmd, args]) =>
      cmd === AGENT_TIMELINE_LIST_COMMAND && args?.taskId === draft.id
    )).toBe(false);
    expect(mockInvoke.mock.calls.some(([cmd, args]) =>
      cmd === CHAT_GET_RUNTIME_SNAPSHOT_COMMAND && args?.taskId === draft.id
    )).toBe(false);
    const suggestions = view.getByLabelText("新对话建议");
    expect(suggestions.closest(".chat-empty")).not.toBeNull();
    expect(suggestions.closest(".chat-composer")).toBeNull();
    expect(view.queryByRole("button", { name: "来点灵感？" })).toBeNull();
  }, 10_000);

  it("GitHub 建议会展示可扫描来源且点击仍填入 prompt", async () => {
    setMockConversationSuggestions([
      {
        id: "sg-github-pr",
        projectId: "lilia",
        taskIds: [],
        source: "github",
        githubActivities: [
          {
            id: "gh-pr-12",
            repoFullName: "sena-nana/LiliaCode",
            kind: "pull_request",
            title: "PR #12: 补齐 GitHub 建议来源",
            url: "https://github.com/sena-nana/LiliaCode/pull/12",
          },
        ],
        localGitContexts: [],
        codexThreads: [],
        summary: "跟进 GitHub 建议来源",
        reason: "近期 PR 活动触发了新对话建议。",
        prompt: "请继续跟进 PR #12 的建议来源展示。",
        generatedAt: Date.now(),
      },
    ]);
    const draft = createDraftTask("lilia");
    const view = await renderProjectDraftTaskDetail(draft.id);

    await waitFor(() => {
      expect(view.getByText("跟进 GitHub 建议来源")).toBeInTheDocument();
      expect(view.getByText("sena-nana/LiliaCode · PR #12")).toBeInTheDocument();
    });

    await fireEvent.click(view.getByText("跟进 GitHub 建议来源"));

    expect(getComposerTextbox(view)).toHaveTextContent("请继续跟进 PR #12 的建议来源展示。");
  });

  it("会话线程建议会展示 thread 来源且点击填入 prompt", async () => {
    setMockConversationSuggestions([
      {
        id: "sg-session-thread",
        projectId: "lilia",
        taskIds: [],
        source: "session-thread",
        githubActivities: [],
        localGitContexts: [],
        codexThreads: [
          {
            id: "thread-1",
            title: "补齐会话建议来源",
            updatedAt: Date.now(),
            preview: "继续扩展 conversation suggestions",
          },
        ],
        summary: "继续会话线程",
        reason: "最近会话线程还有可继续处理的上下文。",
        prompt: "请基于会话线程继续补齐 conversation suggestions。",
        generatedAt: Date.now(),
      },
    ]);
    const draft = createDraftTask("lilia");
    const view = await renderProjectDraftTaskDetail(draft.id);

    await waitFor(() => {
      expect(view.getByText("继续会话线程")).toBeInTheDocument();
      expect(view.getByText("会话线程 · 补齐会话建议来源")).toBeInTheDocument();
    });

    await fireEvent.click(view.getByText("继续会话线程"));

    expect(getComposerTextbox(view)).toHaveTextContent("请基于会话线程继续补齐 conversation suggestions。");
  });

  it("点击来点灵感会强制刷新新对话建议并显示来源加载文案", async () => {
    setMockConversationSuggestions([]);
    const draft = createDraftTask("lilia");
    const view = await renderProjectDraftTaskDetail(draft.id);

    await waitFor(() => {
      expect(view.getByRole("button", { name: "来点灵感？" })).toBeInTheDocument();
    });
    mockInvoke.mockClear();
    setMockConversationSuggestionSources({
      sources: ["github"],
      localGit: null,
    });
    setMockConversationSuggestionsDelay(80);

    await fireEvent.click(view.getByRole("button", { name: "来点灵感？" }));

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some(([cmd, args]) =>
        cmd === CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND &&
        args?.projectId === "lilia" &&
        args?.forceRefresh === true
      )).toBe(true);
      expect(mockInvoke.mock.calls.some(([cmd, args]) =>
        cmd === CONVERSATION_SUGGESTIONS_GET_COMMAND &&
        args?.projectId === "lilia" &&
        args?.forceRefresh === true
      )).toBe(true);
    });
  });

  it("没有可用建议时显示来点灵感且不影响输入", async () => {
    setMockConversationSuggestions([]);
    const draft = createDraftTask("lilia");
    const view = await renderProjectDraftTaskDetail(draft.id);

    await waitFor(() => {
      expect(view.getByRole("button", { name: "来点灵感？" })).toBeInTheDocument();
    });

    const input = getComposerTextbox(view);
    await fireEvent.input(input, { target: { textContent: "继续写草稿" } });

    expect(input).toHaveTextContent("继续写草稿");
    expect(view.queryByRole("button", { name: "来点灵感？" })).toBeNull();
    expect(view.container.querySelector(".chat-suggestions.is-hidden")).toBeInTheDocument();
  });

  it("建议加载失败时显示轻量状态且不影响输入", async () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    setMockConversationSuggestionSources({ sources: ["task"], localGit: null });
    setMockConversationSuggestionsDelay(0);
    failNextMockConversationSuggestions("suggestions unavailable");
    try {
      const draft = createDraftTask("lilia");
      const view = await renderProjectDraftTaskDetail(draft.id);

      const input = getComposerTextbox(view);
      await fireEvent.input(input, { target: { textContent: "先自己输入" } });

      expect(input).toHaveTextContent("先自己输入");
      await waitFor(() => {
        expect(view.container.querySelector(".chat-suggestions.is-hidden")).not.toBeNull();
      });
    } finally {
      consoleError.mockRestore();
    }
  });

  it("主窗口项目草稿进入对话后自动聚焦输入框", async () => {
    const draft = createDraftTask("lilia");
    const view = await renderProjectDraftTaskDetail(draft.id);

    await waitFor(() => {
      expectComposerFocused(view);
    });
  });

  it("收集箱空白草稿不加载也不展示建议", async () => {
    const draft = createDraftOrphan();
    const view = await renderOrphanDraftTaskDetail(draft.id);

    await waitFor(() => {
      expect(view.getByText("今天想做什么？")).toBeInTheDocument();
    });

    expect(mockInvoke.mock.calls.some(([cmd]) => cmd === CONVERSATION_SUGGESTIONS_GET_COMMAND))
      .toBe(false);
    expect(view.queryByLabelText("新对话建议")).toBeNull();
    expect(view.queryByRole("button", { name: "补齐建议缓存测试" })).toBeNull();
    expect(view.queryByRole("button", { name: "来点灵感？" })).toBeNull();
  });

  it("主窗口收集箱草稿进入对话后自动聚焦输入框", async () => {
    const draft = createDraftOrphan();
    const view = await renderOrphanDraftTaskDetail(draft.id);

    await waitFor(() => {
      expectComposerFocused(view);
    });
  });

  it("统一侧栏模式下收集箱草稿可在输入框下方切换到项目草稿并保留输入", async () => {
    useSidebarDisplayMode().setSidebarDisplayMode("unified");
    const draft = createDraftOrphan();
    const view = await renderRouterView(`/chats/${draft.id}`);

    const input = await waitFor(() => getComposerTextbox(view));
    await fireEvent.input(input, { target: { textContent: "把这个想法放进项目" } });
    const projectPicker = await view.findByLabelText("选择项目");
    await fireEvent.change(projectPicker, {
      target: { value: "lilia" },
    });

    await waitFor(() => {
      expect(view.router.currentRoute.value.path).toMatch(
        /^\/projects\/lilia\/tasks\/t-draft-/,
      );
    });
    await waitFor(() => {
      expect(getComposerTextbox(view)).toHaveTextContent("把这个想法放进项目");
    });
  });
});


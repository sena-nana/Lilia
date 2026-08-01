import { render, fireEvent, waitFor, within } from "@testing-library/vue";
import { createMemoryHistory, createRouter } from "vue-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";
import {
  POPUP_OPEN_CHILD_QUESTION_COMMAND,
  POPUP_OPEN_NEW_CHAT_COMMAND,
  POPUP_OPEN_TASK_COMMAND,
  type Task,
} from "@lilia/contracts";
import ProjectTreeItem from "../src/components/sidebar/ProjectTreeItem.vue";
import ContextMenuHost from "@lilia/ui/components/ContextMenuHost";
import { installContextMenu } from "@lilia/ui/composables/useContextMenu";
import { vContextMenu } from "@lilia/ui/directives/contextMenu";
import { TASKS } from "../src/data/tasks";
import { getProject } from "../src/data/projects";
import { mockInvoke } from "./tauriMock";

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

function seedProjectConversations(count: number) {
  TASKS.value = {
    ...TASKS.value,
    lilia: Array.from({ length: count }, (_, index) =>
      projectConversation(`t-overflow-${index + 1}`, `对话 ${index + 1}`, index)
    ),
  };
}

async function renderProjectTreeItem(initialRoute = "/projects/lilia") {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: { template: "<div />" } },
      { path: "/projects", component: { template: "<div />" } },
      { path: "/projects/:projectId", component: { template: "<div />" } },
      {
        path: "/projects/:projectId/tasks/:taskId",
        component: { template: "<div />" },
      },
    ],
  });
  await router.push(initialRoute);
  await router.isReady();

  const Wrapper = defineComponent({
    components: { ProjectTreeItem, ContextMenuHost },
    props: {
      isExpanded: {
        type: Boolean,
        default: true,
      },
    },
    template: `
      <ProjectTreeItem
        :project="project"
        :is-expanded="isExpanded"
        :activity-for-task="activityForTask"
        @toggle="$emit('toggle', $event)"
        @error="emitError"
        @archived="emitArchived"
      />
      <ContextMenuHost />
    `,
    setup() {
      return {
        project: {
          id: "lilia",
          name: "Lilia",
          cwd: "D:\\PROJECT\\workspace\\Lilia",
          sessionCount: 2,
          pinned: false,
        },
        activityForTask: () => null,
      };
    },
    methods: {
      emitError(message: string) {
        this.$emit("error", message);
      },
      emitArchived() {
        this.$emit("archived");
      },
    },
  });

  const view = render(Wrapper, {
    props: {
      isExpanded: true,
    },
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
  return { ...view, router };
}

async function waitForProjectConversations(view: Awaited<ReturnType<typeof renderProjectTreeItem>>) {
  await waitFor(() => {
    expect(view.queryByText("准备对话列表…")).not.toBeInTheDocument();
    expect(view.getByText("接入 Claude Code 会话发现")).toBeInTheDocument();
  });
  if (typeof vi.dynamicImportSettled === "function") {
    await vi.dynamicImportSettled();
  }
  await new Promise((resolve) => globalThis.setTimeout(resolve, 10));
}

function getConversationRow(view: ReturnType<typeof render>, title: string): HTMLElement {
  const row = view.getByText(title).closest(".sb-tree__row");
  if (!(row instanceof HTMLElement)) {
    throw new Error(`未找到对话行：${title}`);
  }
  return row;
}

describe("ProjectTreeItem", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("项目行主体点击会展开折叠，项目名称不再直接导航", async () => {
    const view = await renderProjectTreeItem("/projects/tools");
    const projectRow = view.getByText("Lilia").closest(".sb-tree__row--project");

    expect(view.queryByRole("link", { name: "Lilia" })).not.toBeInTheDocument();
    expect(view.queryByLabelText("折叠项目")).not.toBeInTheDocument();
    expect(projectRow).toHaveAttribute("aria-expanded", "true");

    await fireEvent.click(projectRow!);

    expect(view.emitted("toggle")).toEqual([["lilia"]]);
  });


  it("项目右键菜单第一项进入对应项目", async () => {
    const view = await renderProjectTreeItem("/projects/tools");

    await fireEvent.click(view.getByLabelText("更多"));

    const menuItems = await view.findAllByRole("menuitem");
    expect(menuItems[0]).toHaveTextContent("进入项目");

    await fireEvent.click(menuItems[0]);

    await waitFor(() => {
      expect(view.router.currentRoute.value.path).toBe("/projects/lilia");
    });
  });

  it("项目菜单可在弹出窗口中创建对话", async () => {
    const view = await renderProjectTreeItem("/projects/tools");

    await fireEvent.click(view.getByLabelText("更多"));
    await fireEvent.click(await view.findByRole("menuitem", {
      name: "在弹出窗口中创建对话",
    }));

    expect(mockInvoke).toHaveBeenCalledWith(POPUP_OPEN_NEW_CHAT_COMMAND, {
      projectId: "lilia",
      initialDraftContent: null,
    }, undefined);
  });

  it("项目菜单可进入重命名并提交新名称", async () => {
    const view = await renderProjectTreeItem("/projects/tools");

    await fireEvent.click(view.getByLabelText("更多"));
    await fireEvent.click(await view.findByRole("menuitem", { name: "重命名项目" }));
    const input = await view.findByRole("textbox");
    await fireEvent.update(input, "  Lilia Next  ");
    await fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => {
      expect(getProject("lilia")?.name).toBe("Lilia Next");
      expect(view.queryByRole("textbox")).toBeNull();
    });
  });

  it("中键点击项目行会在弹出窗口中创建对话", async () => {
    const view = await renderProjectTreeItem("/projects/tools");
    const projectRow = view.getByText("Lilia").closest(".sb-tree__row--project");

    await fireEvent(
      projectRow!,
      new MouseEvent("auxclick", { bubbles: true, button: 1 }),
    );

    expect(mockInvoke).toHaveBeenCalledWith(POPUP_OPEN_NEW_CHAT_COMMAND, {
      projectId: "lilia",
      initialDraftContent: null,
    }, undefined);
  });

  it("中键点击项目对话会在弹出窗口中打开", async () => {
    const view = await renderProjectTreeItem();
    await waitForProjectConversations(view);
    const row = view.getByText("接入 Claude Code 会话发现").closest(".sb-tree__row");

    await fireEvent(
      row!,
      new MouseEvent("auxclick", { bubbles: true, button: 1 }),
    );

    expect(mockInvoke).toHaveBeenCalledWith(POPUP_OPEN_TASK_COMMAND, {
      projectId: "lilia",
      taskId: "t-001",
    }, undefined);
  });

  it("项目对话右键菜单拆分为弹窗继续和询问", async () => {
    installContextMenu();
    const view = await renderProjectTreeItem();
    await waitForProjectConversations(view);
    const row = view.getByText("接入 Claude Code 会话发现").closest(".sb-tree__row");

    await fireEvent.contextMenu(row!, { clientX: 10, clientY: 10 });

    expect(await view.findByRole("menuitem", { name: "在弹出窗口继续" })).toBeInTheDocument();
    expect(await view.findByRole("menuitem", { name: "在弹出窗口询问" })).toBeInTheDocument();
    expect(await view.findByRole("menuitem", { name: "置顶" })).toBeInTheDocument();
    expect(await view.findByRole("menuitem", { name: "归档" })).toBeInTheDocument();

    await fireEvent.click(view.getByRole("menuitem", { name: "在弹出窗口继续" }));
    expect(mockInvoke).toHaveBeenCalledWith(POPUP_OPEN_TASK_COMMAND, {
      projectId: "lilia",
      taskId: "t-001",
    }, undefined);

    await fireEvent.contextMenu(row!, { clientX: 10, clientY: 10 });
    await fireEvent.click(await view.findByRole("menuitem", { name: "在弹出窗口询问" }));
    expect(mockInvoke).toHaveBeenCalledWith(POPUP_OPEN_CHILD_QUESTION_COMMAND, {
      projectId: "lilia",
      parentTaskId: "t-001",
    }, undefined);
  });

  it("项目对话超过四条时用省略行折叠剩余对话，并可点击展开", async () => {
    seedProjectConversations(6);

    const view = await renderProjectTreeItem();
    await waitFor(() => {
      expect(view.queryByText("准备对话列表…")).not.toBeInTheDocument();
      expect(view.getByText("对话 1")).toBeInTheDocument();
    });

    expect(view.getByText("对话 4")).toBeInTheDocument();
    expect(view.queryByText("对话 5")).not.toBeInTheDocument();
    expect(view.queryByText("对话 6")).not.toBeInTheDocument();

    const more = view.getByRole("button", { name: "显示剩余对话" });
    expect(more).toHaveTextContent("...");
    expect(more).toHaveClass("sb-tree__row--child");

    await fireEvent.click(more);

    expect(view.getByText("对话 5")).toBeInTheDocument();
    expect(view.getByText("对话 6")).toBeInTheDocument();
    expect(view.queryByRole("button", { name: "显示剩余对话" })).not.toBeInTheDocument();
  });

  it("项目对话会展示任务树层级和依赖提示", async () => {
    const view = await renderProjectTreeItem();
    await waitForProjectConversations(view);

    const parentRow = getConversationRow(view, "接入 Claude Code 会话发现");
    const childRow = getConversationRow(view, "打通 tsconfig paths 搜索");

    expect(within(parentRow).getByText("1 子任务")).toHaveClass("sb-tree__meta");
    expect(within(childRow).getByText(/子任务.*1 依赖/)).toHaveClass("sb-tree__meta");
    expect(childRow).toHaveStyle("--task-tree-depth: 1");
  });

  it("卸载时取消项目对话列表延迟 reveal 的 paint 调度", async () => {
    const cancelAnimationFrame = vi.fn();
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 29));
    vi.stubGlobal("cancelAnimationFrame", cancelAnimationFrame);

    const view = await renderProjectTreeItem();
    view.unmount();

    expect(cancelAnimationFrame).toHaveBeenCalledWith(29);
  });
});

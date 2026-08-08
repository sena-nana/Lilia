import { fireEvent, render, waitFor, within } from "@testing-library/vue";
import { useVueFlow } from "@vue-flow/core";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  AUTOMATION_DELETE_WORKFLOW_COMMAND,
  AUTOMATION_GET_RUN_COMMAND,
  AUTOMATION_LIST_RUNS_COMMAND,
  AUTOMATION_PUBLISH_COMMAND,
  AUTOMATION_RESUME_RUN_COMMAND,
  AUTOMATION_RUN_ONCE_COMMAND,
  AUTOMATION_SAVE_DRAFT_COMMAND,
  AUTOMATION_SET_ENABLED_COMMAND,
} from "@lilia/contracts";
import Automations from "../src/pages/Automations.vue";
import { clearMockAutomations, finishMockAutomationRun, mockInvoke } from "./tauriMock";

const vueFlowMock = vi.hoisted(() => ({
  useVueFlow: vi.fn(() => ({
    fitView: vi.fn(),
    zoomIn: vi.fn(),
    zoomOut: vi.fn(),
    dimensions: { value: { width: 740, height: 470 } },
    viewport: { value: { x: 0, y: 0, zoom: 1 } },
  })),
}));

vi.mock("@vue-flow/core", async () => {
  const { defineComponent, h } = await import("vue");
  const VueFlow = defineComponent({
    name: "VueFlow",
    props: {
      nodes: { type: Array, default: () => [] },
      edges: { type: Array, default: () => [] },
    },
    emits: ["node-click"],
    setup(props, { slots, emit }) {
      return () =>
        h(
          "div",
          { "data-testid": "automation-flow" },
          (props.nodes as Array<{ id: string; data: unknown }>).map((node) =>
            h(
              "button",
              {
                key: node.id,
                type: "button",
                onClick: () => emit("node-click", { node }),
              },
              slots["node-automation"]?.({ data: node.data }),
            ),
          ),
        );
    },
  });
  return {
    Handle: defineComponent({ name: "Handle", setup: () => () => h("span") }),
    Position: { Left: "left", Right: "right" },
    VueFlow,
    useVueFlow: vueFlowMock.useVueFlow,
  };
});

function lastInvokeInput(command: string) {
  return mockInvoke.mock.calls
    .filter(([calledCommand]) => calledCommand === command)
    .at(-1)?.[1];
}

function invokeCount(command: string) {
  return mockInvoke.mock.calls.filter(([calledCommand]) => calledCommand === command).length;
}

async function renderAutomations() {
  document.getElementById("automation-sidebar-host")?.closest("aside")?.remove();
  const sidebar = document.createElement("aside");
  sidebar.setAttribute("aria-label", "自动化列表");
  const actions = document.createElement("div");
  actions.id = "automation-sidebar-actions";
  sidebar.appendChild(actions);
  const host = document.createElement("div");
  host.id = "automation-sidebar-host";
  sidebar.appendChild(host);
  document.body.appendChild(sidebar);
  const view = render(Automations);
  await view.findByRole("complementary", { name: "自动化列表" }, { timeout: 5000 });
  await view.findByRole("group", { name: "自动化操作" }, { timeout: 5000 });
  if (typeof vi.dynamicImportSettled === "function") {
    await vi.dynamicImportSettled();
  }
  return view;
}

type AutomationsView = Awaited<ReturnType<typeof renderAutomations>>;

function workflowNameInput(view: AutomationsView) {
  return within(view.getByRole("group", { name: "自动化操作" })).getByRole("textbox", {
    name: "自动化名称",
  });
}

async function waitForInspectorReady(view: AutomationsView) {
  const inspector = view.getByRole("complementary", { name: "自动化检查器" });
  await waitFor(() => {
    expect(inspector).toHaveAttribute("aria-busy", "false");
    expect(within(inspector).getByText("作用域")).toBeInTheDocument();
  });
  if (typeof vi.dynamicImportSettled === "function") {
    await vi.dynamicImportSettled();
  }
  return inspector;
}

async function editWorkflowName(view: AutomationsView, value: string) {
  const input = workflowNameInput(view);
  await fireEvent.pointerDown(input);
  await fireEvent.update(input, value);
  return input;
}

async function waitForWorkflowActionEnabled(view: AutomationsView, name: string) {
  await waitFor(() => {
    expect(view.getByRole("button", { name })).toBeEnabled();
  });
}

describe("Automations page", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("卸载时取消自动化工作区入口的 paint 挂载调度", () => {
    const cancelAnimationFrame = vi.fn();
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 91));
    vi.stubGlobal("cancelAnimationFrame", cancelAnimationFrame);

    const view = render(Automations);
    view.unmount();

    expect(cancelAnimationFrame).toHaveBeenCalledWith(91);
  });

  it("展示全局自动化列表、节点画布和检查器", async () => {
    const zoomIn = vi.fn();
    const zoomOut = vi.fn();
    const fitView = vi.fn();
    vi.mocked(useVueFlow).mockReturnValue({
      fitView,
      zoomIn,
      zoomOut,
      dimensions: { value: { width: 740, height: 470 } },
      viewport: { value: { x: 18, y: -12, zoom: 1.5 } },
    } as never);
    const view = await renderAutomations();

    await view.findByRole("complementary", { name: "自动化检查器" }, { timeout: 5000 });
    const list = view.getByRole("complementary", { name: "自动化列表" });
    const inspector = await waitForInspectorReady(view);

    await waitFor(() => {
      expect(within(list).getByRole("button", { name: /任务完成后复盘/ })).toBeInTheDocument();
    });
    expect(within(list).queryByRole("heading", { name: "自动化" })).not.toBeInTheDocument();
    expect(within(list).getByRole("button", { name: "新建自动化" })).toBeInTheDocument();

    expect(workflowNameInput(view)).toHaveValue("任务完成后复盘");
    expect(workflowNameInput(view)).toHaveAttribute("readonly");
    expect(within(view.getByTestId("automation-flow")).getByText("任务变化")).toBeInTheDocument();
    expect(within(view.getByTestId("automation-flow")).getByText("复盘 Agent")).toBeInTheDocument();
    expect(view.getByRole("group", { name: "画布控制" })).toBeInTheDocument();
    expect(view.getByRole("group", { name: "画布控制" }).closest(".automations-page__canvas")).toHaveStyle({
      "--automation-canvas-grid-x": "18px",
      "--automation-canvas-grid-y": "-12px",
      "--automation-canvas-grid-size": "36px",
    });
    expect(view.getByRole("img", { name: "节点小地图" })).toBeInTheDocument();
    const minimap = view.getByRole("img", { name: "节点小地图" });
    expect(minimap.querySelector(".automations-page__minimap-viewport")).not.toBeNull();
    expect(minimap.querySelector(".automations-page__minimap-node.is-selected")).toBeNull();
    expect(view.getByRole("button", { name: "添加事件触发" })).toBeDisabled();
    expect(view.getByRole("button", { name: "添加人工确认" })).toBeEnabled();
    expect(view.queryByRole("textbox", { name: "手动 Payload" })).not.toBeInTheDocument();
    await fireEvent.click(view.getByRole("button", { name: "放大画布" }));
    await fireEvent.click(view.getByRole("button", { name: "缩小画布" }));
    await fireEvent.click(view.getByRole("button", { name: "适应视图" }));
    expect(zoomIn).toHaveBeenCalled();
    expect(zoomOut).toHaveBeenCalled();
    expect(fitView).toHaveBeenCalledWith({ padding: 0.2 });
    expect(within(inspector).getByRole("button", { name: "Lilia" })).toBeInTheDocument();
    expect(within(inspector).getByRole("button", { name: "task_created" })).toBeInTheDocument();
    expect(within(inspector).getByRole("button", { name: "task_status_changed" })).toBeInTheDocument();
    expect(within(inspector).getByText("运行历史")).toBeInTheDocument();

    await fireEvent.click(view.getByRole("button", { name: "新建自动化" }));
    await waitFor(() => {
      expect(lastInvokeInput(AUTOMATION_SAVE_DRAFT_COMMAND)?.input).toMatchObject({
        name: "未命名自动化",
      });
      expect(workflowNameInput(view)).toHaveValue("未命名自动化");
      expect(within(list).getByRole("button", { name: /未命名自动化/ })).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(view.getByRole("button", { name: "添加事件触发" })).toBeEnabled();
    });

    const newWorkflowRow = within(list).getByRole("button", { name: /未命名自动化/ });
    await fireEvent.click(within(newWorkflowRow).getByRole("button", { name: "删除" }));
    expect(within(newWorkflowRow).getByRole("button", { name: "确认删除" })).toBeInTheDocument();
    await fireEvent.click(within(newWorkflowRow).getByRole("button", { name: "确认删除" }));
    await waitFor(() => {
      expect(lastInvokeInput(AUTOMATION_DELETE_WORKFLOW_COMMAND)).toEqual({ id: "auto-2" });
      expect(within(list).queryByRole("button", { name: /未命名自动化/ })).not.toBeInTheDocument();
      expect(workflowNameInput(view)).toHaveValue("任务完成后复盘");
    });
  });

  it("运行历史与检查器内容在首屏后异步加载", async () => {
    const view = await renderAutomations();
    const inspector = view.getByRole("complementary", { name: "自动化检查器" });

    expect(inspector).toHaveAttribute("aria-busy", "true");
    expect(invokeCount(AUTOMATION_LIST_RUNS_COMMAND)).toBe(0);
    expect(invokeCount(AUTOMATION_GET_RUN_COMMAND)).toBe(0);

    await waitForInspectorReady(view);

    await waitFor(() => {
      expect(invokeCount(AUTOMATION_LIST_RUNS_COMMAND)).toBeGreaterThan(0);
    });
  });

  it("工作流切换期间会短暂禁用保存操作，避免旧画布数据被直接提交", async () => {
    const view = await renderAutomations();

    await waitFor(() => {
      expect(workflowNameInput(view)).toHaveValue("任务完成后复盘");
    });
    await waitForWorkflowActionEnabled(view, "保存草稿");

    await fireEvent.click(view.getByRole("button", { name: "新建自动化" }));
    await waitFor(() => {
      expect(workflowNameInput(view)).toHaveValue("未命名自动化");
      expect(view.getByRole("button", { name: "保存草稿" })).toBeDisabled();
    });

    await waitForWorkflowActionEnabled(view, "保存草稿");
  });

  it("没有自动化或删除最后一项后自动创建空白自动化", async () => {
    clearMockAutomations();
    const view = await renderAutomations();
    const list = view.getByRole("complementary", { name: "自动化列表" });

    await waitFor(() => {
      expect(within(list).queryByText("没有自动化")).not.toBeInTheDocument();
      expect(within(list).getByRole("button", { name: /未命名自动化/ })).toBeInTheDocument();
      expect(workflowNameInput(view)).toHaveValue("未命名自动化");
      expect(lastInvokeInput(AUTOMATION_SAVE_DRAFT_COMMAND)?.input).toMatchObject({
        name: "未命名自动化",
        nodes: [],
        edges: [],
      });
    });

    const row = within(list).getByRole("button", { name: /未命名自动化/ });
    await fireEvent.click(within(row).getByRole("button", { name: "删除" }));
    await fireEvent.click(within(row).getByRole("button", { name: "确认删除" }));

    await waitFor(() => {
      expect(lastInvokeInput(AUTOMATION_DELETE_WORKFLOW_COMMAND)).toEqual({ id: "auto-1" });
      expect(lastInvokeInput(AUTOMATION_SAVE_DRAFT_COMMAND)?.input).toMatchObject({
        name: "未命名自动化",
        nodes: [],
        edges: [],
      });
      expect(within(list).getByRole("button", { name: /未命名自动化/ })).toBeInTheDocument();
    });
  });

  it("暴露 Agent、工具配置并支持手动运行", async () => {
    const view = await renderAutomations();

    await waitFor(() => {
      expect(workflowNameInput(view)).toHaveValue("任务完成后复盘");
    });

    const flow = view.getByTestId("automation-flow");
    await fireEvent.click(within(flow).getByText("复盘 Agent"));
    const inspector = await waitForInspectorReady(view);

    await fireEvent.update(within(inspector).getByLabelText("模型"), "gpt-5.5");
    await fireEvent.update(within(inspector).getByLabelText("工作目录"), "C:\\Files\\workspace\\Lilia");

    await fireEvent.click(view.getByRole("button", { name: "添加工具" }));
    await fireEvent.change(within(inspector).getByLabelText("动作"), { target: { value: "send_guide" } });
    await fireEvent.update(within(inspector).getByLabelText("引导内容"), "请确认 ${trigger.taskId}");
    await fireEvent.change(within(inspector).getByLabelText("优先级"), { target: { value: "high" } });

    await waitForWorkflowActionEnabled(view, "保存草稿");
    await fireEvent.click(view.getByRole("button", { name: "保存草稿" }));
    await waitFor(() => {
      expect(lastInvokeInput(AUTOMATION_SAVE_DRAFT_COMMAND)?.input.nodes).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            id: "agent-1",
            config: expect.objectContaining({
              model: "gpt-5.5",
              projectCwd: "C:\\Files\\workspace\\Lilia",
            }),
          }),
          expect.objectContaining({
            kind: "tool",
            config: expect.objectContaining({
              action: "send_guide",
              text: "请确认 ${trigger.taskId}",
              priority: "high",
            }),
          }),
        ]),
      );
    });

    await waitForWorkflowActionEnabled(view, "发布");
    await fireEvent.click(view.getByRole("button", { name: "发布" }));
    await waitForWorkflowActionEnabled(view, "手动运行");
    await fireEvent.click(view.getByRole("button", { name: "手动运行" }));

    await waitFor(() => {
      expect(lastInvokeInput(AUTOMATION_RUN_ONCE_COMMAND)).toEqual({
        id: "auto-1",
        input: {},
      });
    });
  });

  it("保存草稿、发布、启停并手动运行后显示运行历史和节点状态", async () => {
    const view = await renderAutomations();

    await waitFor(() => {
      expect(workflowNameInput(view)).toHaveValue("任务完成后复盘");
    });

    await editWorkflowName(view, "自动复盘");
    const inspector = await waitForInspectorReady(view);
    await fireEvent.click(within(inspector).getByRole("button", { name: "Lilia" }));
    await fireEvent.click(within(inspector).getByRole("button", { name: "运行" }));
    await fireEvent.click(within(inspector).getByRole("button", { name: "native-agentkit" }));
    await waitForWorkflowActionEnabled(view, "保存草稿");
    await fireEvent.click(view.getByRole("button", { name: "保存草稿" }));

    await waitFor(() => {
      expect(lastInvokeInput(AUTOMATION_SAVE_DRAFT_COMMAND)?.input).toMatchObject({
        name: "自动复盘",
        scope: {
          projectIds: ["lilia"],
          taskStatuses: ["running"],
          backends: ["native-agentkit"],
        },
      });
    });

    await waitFor(() => {
      expect(view.getByRole("button", { name: /自动复盘/ })).toBeInTheDocument();
    });

    await waitForWorkflowActionEnabled(view, "发布");
    await fireEvent.click(view.getByRole("button", { name: "发布" }));
    await waitFor(() => {
      expect(lastInvokeInput(AUTOMATION_PUBLISH_COMMAND)).toEqual({ id: "auto-1" });
    });

    await waitForWorkflowActionEnabled(view, "启用");
    await fireEvent.click(view.getByRole("button", { name: "启用" }));
    await waitFor(() => {
      expect(lastInvokeInput(AUTOMATION_SET_ENABLED_COMMAND)).toEqual({ id: "auto-1", enabled: true });
    });

    await waitForWorkflowActionEnabled(view, "手动运行");
    await fireEvent.click(view.getByRole("button", { name: "手动运行" }));
    await waitFor(() => {
      expect(lastInvokeInput(AUTOMATION_RUN_ONCE_COMMAND)).toEqual({ id: "auto-1", input: {} });
    });

    await waitFor(() => {
      expect(within(inspector).getAllByText("running").length).toBeGreaterThan(0);
      expect(within(inspector).getAllByText("succeeded").length).toBeGreaterThan(0);
      expect(within(inspector).getByText("agent-1")).toBeInTheDocument();
    });

    const agentStateButton = within(inspector).getByText("agent-1").closest("button");
    expect(agentStateButton).not.toBeNull();
    await fireEvent.click(agentStateButton!);

    await waitFor(() => {
      expect(within(inspector).getByText(/"waitingAgent": true/)).toBeInTheDocument();
    });

    finishMockAutomationRun("run-1");
    await waitFor(() => {
      expect(within(inspector).getByText(/"completedByEvent": true/)).toBeInTheDocument();
      expect(within(inspector).getAllByText("succeeded").length).toBeGreaterThan(1);
    });
  });

  it("人工节点等待用户确认后可以继续运行", async () => {
    const view = await renderAutomations();

    await waitFor(() => {
      expect(workflowNameInput(view)).toHaveValue("任务完成后复盘");
    });

    await fireEvent.click(view.getByRole("button", { name: "添加人工确认" }));
    await waitForWorkflowActionEnabled(view, "保存草稿");
    await fireEvent.click(view.getByRole("button", { name: "保存草稿" }));
    await waitFor(() => {
      expect(lastInvokeInput(AUTOMATION_SAVE_DRAFT_COMMAND)?.input.nodes).toEqual(
        expect.arrayContaining([expect.objectContaining({ kind: "human" })]),
      );
    });

    await waitForWorkflowActionEnabled(view, "发布");
    await fireEvent.click(view.getByRole("button", { name: "发布" }));
    await waitForWorkflowActionEnabled(view, "手动运行");
    await fireEvent.click(view.getByRole("button", { name: "手动运行" }));

    const inspector = await waitForInspectorReady(view);
    await waitFor(() => {
      expect(within(inspector).getAllByText("waiting_user").length).toBeGreaterThan(0);
    });

    let humanStateLabel: HTMLElement | null = null;
    await waitFor(() => {
      humanStateLabel = within(inspector).getByText(/^human-/);
      expect(humanStateLabel).toBeInTheDocument();
    });
    const humanStateButton = humanStateLabel.closest("button");
    expect(humanStateButton).not.toBeNull();
    await fireEvent.click(humanStateButton!);
    expect(within(inspector).getByRole("region", { name: "自动化等待确认" })).toBeInTheDocument();
    expect(within(inspector).getByText("人工节点")).toBeInTheDocument();
    await fireEvent.click(within(inspector).getByRole("button", { name: "继续" }));

    await waitFor(() => {
      expect(lastInvokeInput(AUTOMATION_RESUME_RUN_COMMAND)).toMatchObject({
        runId: "run-1",
        input: {
          nodeId: expect.stringMatching(/^human-/),
          payload: { confirmed: true },
        },
      });
      expect(within(inspector).getAllByText("succeeded").length).toBeGreaterThan(0);
    });
  });
});


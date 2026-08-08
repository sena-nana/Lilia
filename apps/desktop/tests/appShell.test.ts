import { fireEvent, render, waitFor } from "@testing-library/vue";
import { createMemoryHistory, createRouter } from "vue-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import AppShell from "../src/layouts/AppShell.vue";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    isMaximized: vi.fn(async () => false),
    onResized: vi.fn(async () => vi.fn()),
    minimize: vi.fn(async () => undefined),
    toggleMaximize: vi.fn(async () => undefined),
    close: vi.fn(async () => undefined),
  }),
}));

const COLLAPSED_STORAGE_KEY = "lilia.sidebarCollapsed";
const WIDTH_STORAGE_KEY = "lilia.sidebarWidth";

async function renderAppShell(initialRoute = "/") {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/:pathMatch(.*)*", component: { template: "<div />" } }],
  });
  await router.push(initialRoute);
  await router.isReady();

  const view = render(AppShell, {
    global: {
      plugins: [router],
    },
  });
  if (typeof vi.dynamicImportSettled === "function") {
    await vi.dynamicImportSettled();
  }
  await Promise.resolve();
  await Promise.resolve();

  return {
    ...view,
    router,
  };
}

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("AppShell left sidebar collapse", () => {
  it("卸载时取消路由 paint 打点调度", async () => {
    const cancelAnimationFrame = vi.fn();
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 31));
    vi.stubGlobal("cancelAnimationFrame", cancelAnimationFrame);

    const view = await renderAppShell();
    view.unmount();

    expect(cancelAnimationFrame).toHaveBeenCalledWith(31);
  });

  it("左上角按钮切换左侧栏折叠状态并写回本地存储", async () => {
    const view = await renderAppShell();
    const resizer = view.getByRole("separator");
    const collapse = view.getByRole("button", { name: "折叠左侧栏" });

    expect(resizer).not.toHaveAttribute("aria-disabled");
    expect(collapse).toHaveAttribute("aria-pressed", "false");

    await fireEvent.click(collapse);

    expect(resizer).toHaveAttribute("aria-disabled", "true");
    expect(localStorage.getItem(COLLAPSED_STORAGE_KEY)).toBe("1");

    const expand = view.getByRole("button", { name: "展开左侧栏" });
    expect(expand).toHaveAttribute("aria-pressed", "true");

    await fireEvent.click(expand);

    expect(resizer).not.toHaveAttribute("aria-disabled");
    expect(localStorage.getItem(COLLAPSED_STORAGE_KEY)).toBe("0");
  });

  it("设置页替换左侧栏、禁用折叠并保留折叠偏好", async () => {
    localStorage.setItem(COLLAPSED_STORAGE_KEY, "1");
    const view = await renderAppShell("/settings");
    const leftToggle = view.getByRole("button", { name: "折叠左侧栏" });

    expect(leftToggle).toBeDisabled();
    expect(view.getByRole("navigation", { name: "设置分类" })).toBeInTheDocument();
    expect(view.queryByRole("button", { name: "新对话" })).not.toBeInTheDocument();
    expect(view.getByRole("button", { name: /外观/ })).toHaveAttribute("aria-current", "page");
    expect(localStorage.getItem(COLLAPSED_STORAGE_KEY)).toBe("1");

    await fireEvent.click(view.getByRole("button", { name: /连接/ }));

    await waitFor(() => {
      expect(view.router.currentRoute.value.fullPath).toBe("/settings?tab=providers");
    });
    expect(view.getByRole("button", { name: /连接/ })).toHaveAttribute("aria-current", "page");

    await view.router.push("/");
    expect(view.getByRole("button", { name: "展开左侧栏" })).toBeEnabled();
    expect(localStorage.getItem(COLLAPSED_STORAGE_KEY)).toBe("1");
  });

  it("设置页返回进入设置前的主窗口路由", async () => {
    const view = await renderAppShell("/projects/lilia");

    await view.router.push("/settings?tab=agent");
    await fireEvent.click(await view.findByRole("button", { name: "返回" }));
    await waitFor(() => {
      expect(view.router.currentRoute.value.fullPath).toBe("/projects/lilia");
    });
  });

  it("自动化页替换左侧栏、禁用折叠并返回进入前路由", async () => {
    localStorage.setItem(COLLAPSED_STORAGE_KEY, "1");
    const view = await renderAppShell("/projects/lilia");

    await view.router.push("/automations");
    await waitFor(() => {
      expect(view.router.currentRoute.value.fullPath).toBe("/automations");
    });

    const leftToggle = view.getByRole("button", { name: "折叠左侧栏" });
    expect(leftToggle).toBeDisabled();
    expect(view.getByRole("complementary", { name: "自动化列表" })).toBeInTheDocument();
    expect(view.container.querySelector("#automation-sidebar-host")).toBeInTheDocument();
    expect(view.queryByRole("button", { name: "新对话" })).not.toBeInTheDocument();
    expect(localStorage.getItem(COLLAPSED_STORAGE_KEY)).toBe("1");

    await fireEvent.click(view.getByRole("button", { name: "返回" }));
    await waitFor(() => {
      expect(view.router.currentRoute.value.fullPath).toBe("/projects/lilia");
    });
  });

  it("插件资源入口收敛到设置分类，且不再提供对话导入", async () => {
    const view = await renderAppShell("/settings");

    expect(view.queryByRole("link", { name: "从 Claude / Codex 导入对话" })).not.toBeInTheDocument();
    expect(view.queryByRole("link", { name: "插件 / 技能" })).not.toBeInTheDocument();
    expect(view.queryByRole("button", { name: /导入对话/ })).not.toBeInTheDocument();

    await fireEvent.click(view.getByRole("button", { name: "技能" }));
    await waitFor(() => {
      expect(view.router.currentRoute.value.fullPath).toBe("/settings?tab=plugin-skills");
    });
  });

  it("左侧栏宽度可拖拽调整、写回存储并双击恢复默认", async () => {
    localStorage.setItem(WIDTH_STORAGE_KEY, "260");
    const view = await renderAppShell();
    const resizer = view.getByRole("separator");

    expect(resizer).toHaveAttribute("aria-valuemin", "180");
    expect(resizer).toHaveAttribute("aria-valuemax", "480");
    expect(resizer).toHaveAttribute("aria-valuenow", "260");

    await fireEvent.pointerDown(resizer, {
      button: 0,
      clientX: 200,
      pointerId: 1,
    });
    await fireEvent.pointerMove(window, {
      clientX: 300,
      pointerId: 1,
    });

    await waitFor(() => {
      expect(resizer).toHaveAttribute("aria-valuenow", "360");
    });

    await fireEvent.pointerUp(window, {
      clientX: 300,
      pointerId: 1,
    });

    expect(localStorage.getItem(WIDTH_STORAGE_KEY)).toBe("360");

    await fireEvent.dblClick(resizer);

    expect(resizer).toHaveAttribute("aria-valuenow", "220");
    expect(localStorage.getItem(WIDTH_STORAGE_KEY)).toBe("220");
  });
});


import { fireEvent, render, waitFor, within } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import {
  PLUGINS_CREATE_HOOK_SOURCE_COMMAND,
  PLUGINS_CREATE_MCP_SERVER_COMMAND,
  PLUGINS_DELETE_PACKAGE_COMMAND,
  PLUGINS_DELETE_SKILL_COMMAND,
  PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND,
  PLUGINS_SET_PACKAGE_ENABLED_COMMAND,
  PLUGINS_SET_SKILL_ENABLED_COMMAND,
  PLUGINS_UPDATE_HOOK_SOURCE_COMMAND,
} from "@lilia/contracts";
import Plugins from "../src/pages/Plugins.vue";
import { mockInvoke } from "./tauriMock";

function lastNativeMcpCreateInput() {
  const call = mockInvoke.mock.calls
    .filter(([command, args]) => command === PLUGINS_CREATE_MCP_SERVER_COMMAND && args?.backend === "native-agentkit")
    .at(-1);
  return call?.[1]?.input;
}

describe("Plugins page", () => {
  it("混合显示全局和项目 Skill，并把新建入口放在顶部", async () => {
    const view = render(Plugins, { props: { section: "skills" } });
    const list = view.getByRole("region", { name: "检索结果" });
    const detail = view.getByRole("region", { name: "插件和技能详情" });

    await waitFor(() => {
      expect(within(list).getByRole("button", { name: /mock-skill/ })).toBeInTheDocument();
    });
    expect(within(list).getByRole("button", { name: /project-skill.*Lilia/ })).toBeInTheDocument();
    expect(within(list).getByRole("button", { name: /tools-skill.*工具箱/ })).toBeInTheDocument();
    expect(view.queryByRole("radiogroup", { name: "Skill scope" })).not.toBeInTheDocument();
    expect(view.getByRole("button", { name: "新建 Skill" })).toBeInTheDocument();

    await fireEvent.click(within(list).getByRole("button", { name: /project-skill/ }));
    expect(within(detail).getByText(/项目 · 已启用 · Lilia/)).toBeInTheDocument();
    expect(within(detail).getByText("Lilia")).toBeInTheDocument();
  });

  it("按项目 Skill 条目的真实路径执行启停和删除", async () => {
    const view = render(Plugins, { props: { section: "skills" } });
    const list = view.getByRole("region", { name: "检索结果" });

    await waitFor(() => {
      expect(within(list).getByRole("button", { name: /tools-skill.*工具箱/ })).toBeInTheDocument();
    });
    await fireEvent.click(within(list).getByRole("button", { name: /tools-skill.*工具箱/ }));
    mockInvoke.mockClear();

    await fireEvent.click(view.getByRole("button", { name: "停用" }));
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        PLUGINS_SET_SKILL_ENABLED_COMMAND,
        expect.objectContaining({
          scope: "project",
          projectCwd: "D:\\PROJECT\\workspace\\tools",
          name: "tools-skill",
          enabled: false,
        }),
        undefined,
      );
    });

    await fireEvent.click(view.getByRole("button", { name: "删除" }));
    const deleteButtons = view.getAllByRole("button", { name: "删除" });
    await fireEvent.click(deleteButtons[deleteButtons.length - 1]);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        PLUGINS_DELETE_SKILL_COMMAND,
        expect.objectContaining({
          scope: "project",
          projectCwd: "D:\\PROJECT\\workspace\\tools",
          name: "tools-skill",
        }),
        undefined,
      );
    });
  });

  it("插件页显示 Plugin", async () => {
    const view = render(Plugins, { props: { section: "packages" } });
    const list = view.getByRole("region", { name: "检索结果" });

    await waitFor(() => {
      expect(within(list).getByRole("button", { name: /demo-plugin/ })).toBeInTheDocument();
    });
    expect(within(list).queryByRole("button", { name: /mock-skill/ })).not.toBeInTheDocument();
    expect(view.queryByRole("tab", { name: /Claude|Codex/ })).not.toBeInTheDocument();
    expect(view.queryByRole("button", { name: "新建 Skill" })).not.toBeInTheDocument();
    expect(view.getByRole("button", { name: "安装 Plugin" })).toBeInTheDocument();

    await fireEvent.click(within(list).getByRole("button", { name: /demo-plugin/ }));
    mockInvoke.mockClear();
    await fireEvent.click(view.getByRole("button", { name: "停用" }));
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        PLUGINS_SET_PACKAGE_ENABLED_COMMAND,
        expect.objectContaining({ name: "demo-plugin", enabled: false }),
        undefined,
      );
    });

    await fireEvent.click(view.getByRole("button", { name: "删除" }));
    const deleteButtons = view.getAllByRole("button", { name: "删除" });
    await fireEvent.click(deleteButtons[deleteButtons.length - 1]);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        PLUGINS_DELETE_PACKAGE_COMMAND,
        expect.objectContaining({ name: "demo-plugin" }),
        undefined,
      );
    });
  });

  it("展示并管理 MCP server", async () => {
    const view = render(Plugins, { props: { section: "mcp" } });
    const list = view.getByRole("region", { name: "检索结果" });
    const detail = view.getByRole("region", { name: "插件和技能详情" });

    await waitFor(() => {
      expect(within(list).getByRole("button", { name: /weather/ })).toBeInTheDocument();
    });
    expect(within(detail).getByText("node weather-mcp.js")).toBeInTheDocument();
    expect(within(detail).getByText("WEATHER_TOKEN")).toBeInTheDocument();

    await fireEvent.update(view.getByRole("searchbox", { name: "搜索插件和技能" }), "weather");
    expect(within(list).getByRole("button", { name: /weather/ })).toBeInTheDocument();
    await fireEvent.update(view.getByRole("searchbox", { name: "搜索插件和技能" }), "missing");
    expect(within(list).getByText("没有匹配项")).toBeInTheDocument();
    await fireEvent.update(view.getByRole("searchbox", { name: "搜索插件和技能" }), "");

    await fireEvent.click(view.getByRole("button", { name: "停用" }));
    await waitFor(() => {
      expect(within(detail).getByText(/已停用/)).toBeInTheDocument();
    });
    expect(
      mockInvoke.mock.calls.some(
        ([command, args]) =>
          command === PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND && args?.backend === "native-agentkit",
      ),
    ).toBe(true);

    await fireEvent.click(view.getByRole("button", { name: /新增 MCP/ }));
    await waitFor(() => {
      expect(view.getByPlaceholderText("weather-mcp")).toBeInTheDocument();
    });
    await fireEvent.update(view.getByPlaceholderText("weather-mcp"), "linear");
    await fireEvent.update(view.getByPlaceholderText("node"), "uvx");
    await fireEvent.update(view.getByPlaceholderText("每行一个参数"), "linear-mcp");
    await fireEvent.click(view.getByRole("button", { name: "保存" }));

    await waitFor(() => {
      expect(within(list).getByRole("button", { name: /linear/ })).toBeInTheDocument();
    });
    expect(
      mockInvoke.mock.calls.some(
        ([command, args]) =>
          command === PLUGINS_CREATE_MCP_SERVER_COMMAND && args?.backend === "native-agentkit",
      ),
    ).toBe(true);
    await fireEvent.click(within(list).getByRole("button", { name: /linear/ }));
    expect(within(detail).getByText("uvx linear-mcp")).toBeInTheDocument();
  });

  it.skip("管理 Codex stdio MCP 并保持 HTTP server 只读", async () => {
    const view = render(Plugins, { props: { section: "mcp" } });

    await fireEvent.click(view.getByRole("tab", { name: /Codex/ }));
    const list = view.getByRole("region", { name: "检索结果" });
    const detail = view.getByRole("region", { name: "插件和技能详情" });

    await waitFor(() => {
      expect(within(list).getByRole("button", { name: /mock-mcp/ })).toBeInTheDocument();
    });
    expect(within(list).getByRole("button", { name: /remote-mcp/ })).toBeInTheDocument();
    expect(within(detail).getByText("MOCK_TOKEN")).toBeInTheDocument();

    await fireEvent.click(within(list).getByRole("button", { name: /remote-mcp/ }));
    expect(within(detail).getByText("HTTP")).toBeInTheDocument();
    expect(within(detail).getByText("只读")).toBeInTheDocument();
    expect(view.queryByRole("button", { name: "停用" })).not.toBeInTheDocument();
    expect(view.queryByRole("button", { name: "编辑" })).not.toBeInTheDocument();
    expect(view.queryByRole("button", { name: "删除" })).not.toBeInTheDocument();

    await fireEvent.click(within(list).getByRole("button", { name: /mock-mcp/ }));

    await fireEvent.click(view.getByRole("button", { name: "停用" }));
    await waitFor(() => {
      expect(within(detail).getByText(/已停用/)).toBeInTheDocument();
    });
    expect(
      mockInvoke.mock.calls.some(
        ([command, args]) =>
          command === PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND && args?.backend === "native-agentkit",
      ),
    ).toBe(true);

    await fireEvent.click(view.getByRole("button", { name: /新增 MCP/ }));
    await waitFor(() => {
      expect(view.getByPlaceholderText("weather-mcp")).toBeInTheDocument();
    });
    await fireEvent.update(view.getByPlaceholderText("weather-mcp"), "codex-linear");
    await fireEvent.update(view.getByPlaceholderText("node"), "uvx");
    await fireEvent.update(view.getByPlaceholderText("每行一个参数"), "linear-mcp");
    await fireEvent.update(view.getByPlaceholderText("KEY"), "LINEAR_API_KEY");
    await fireEvent.update(view.getByPlaceholderText("value"), "secret");
    await fireEvent.click(view.getByRole("button", { name: "保存" }));

    await waitFor(() => {
      expect(within(list).getByRole("button", { name: /codex-linear/ })).toBeInTheDocument();
    });
    expect(lastNativeMcpCreateInput()).toMatchObject({
      name: "codex-linear",
      command: "uvx",
      args: ["linear-mcp"],
      env: { LINEAR_API_KEY: "secret" },
    });
    await fireEvent.click(within(list).getByRole("button", { name: /codex-linear/ }));
    expect(within(detail).getByText("uvx linear-mcp")).toBeInTheDocument();
  });

  it.skip("展示 Claude Hooks 并在编辑结构化字段后保留高级 JSON", async () => {
    const view = render(Plugins, { props: { section: "hooks" } });
    const list = view.getByRole("region", { name: "检索结果" });
    const detail = view.getByRole("region", { name: "插件和技能详情" });

    await waitFor(() => {
      expect(within(list).getByRole("button", { name: /Claude Project Hooks.*Lilia/ })).toBeInTheDocument();
    });
    await fireEvent.click(within(list).getByRole("button", { name: /Claude Project Hooks.*Lilia/ }));
    await waitFor(() => {
      expect(within(detail).getAllByText(/MODE/).length).toBeGreaterThan(0);
    });

    await fireEvent.click(view.getByRole("button", { name: "编辑" }));
    await waitFor(() => {
      expect(view.getByDisplayValue("node project-hook.js")).toBeInTheDocument();
    });
    const commandInput = view.getByDisplayValue("node project-hook.js");
    await fireEvent.update(commandInput, "node changed-hook.js");
    await fireEvent.click(view.getByRole("button", { name: "保存" }));

    await waitFor(() => {
      expect(within(detail).getByText("node changed-hook.js")).toBeInTheDocument();
    });
    expect(within(detail).getAllByText(/MODE/).length).toBeGreaterThan(0);
    expect(
      mockInvoke.mock.calls.some(
        ([command, args]) =>
          command === PLUGINS_UPDATE_HOOK_SOURCE_COMMAND &&
          Array.isArray(args?.input?.handlers) &&
          args.input.handlers[0]?.advancedJson?.includes?.("MODE"),
      ),
    ).toBe(true);
  });

  it.skip("支持创建缺失的 Hooks 来源，并对 Codex 只读来源展示限制", async () => {
    const view = render(Plugins, { props: { section: "hooks" } });

    const list = view.getByRole("region", { name: "检索结果" });
    const detail = view.getByRole("region", { name: "插件和技能详情" });

    await waitFor(() => {
      expect(within(list).getByRole("button", { name: /Claude Local Hooks.*Lilia/ })).toBeInTheDocument();
    });
    await fireEvent.click(within(list).getByRole("button", { name: /Claude Local Hooks.*Lilia/ }));
    expect(within(detail).getByText("未创建")).toBeInTheDocument();
    await fireEvent.click(view.getByRole("button", { name: "创建来源" }));

    await waitFor(() => {
      expect(within(detail).getByText("空来源")).toBeInTheDocument();
    });
    expect(
      mockInvoke.mock.calls.some(
        ([command, args]) =>
          command === PLUGINS_CREATE_HOOK_SOURCE_COMMAND &&
          args?.backend === "native-agentkit" &&
          args?.scope === "local" &&
          args?.projectCwd === "D:\\PROJECT\\workspace\\Lilia",
      ),
    ).toBe(true);

    await fireEvent.click(within(list).getByRole("button", { name: /Claude Local Hooks.*工具箱/ }));
    mockInvoke.mockClear();
    await fireEvent.click(view.getByRole("button", { name: "创建来源" }));
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        PLUGINS_CREATE_HOOK_SOURCE_COMMAND,
        expect.objectContaining({
          backend: "native-agentkit",
          scope: "local",
          projectCwd: "D:\\PROJECT\\workspace\\tools",
        }),
        undefined,
      );
    });

    await fireEvent.click(view.getByRole("tab", { name: /Codex/ }));
    await waitFor(() => {
      expect(within(list).getByRole("button", { name: /Codex User Config Hooks/ })).toBeInTheDocument();
    });
    await fireEvent.click(within(list).getByRole("button", { name: /Codex User Config Hooks/ }));
    await waitFor(() => {
      expect(within(detail).getByText(/Lilia 不会自动重写 config.toml/)).toBeInTheDocument();
    });
    expect(view.queryByRole("button", { name: "编辑" })).not.toBeInTheDocument();
    expect(view.queryByRole("button", { name: "删除" })).not.toBeInTheDocument();
  });
});


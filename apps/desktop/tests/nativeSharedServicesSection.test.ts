import { render, waitFor } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import NativeSharedServicesSection from "../src/pages/settings/NativeSharedServicesSection.vue";
import {
  NATIVE_SHARED_CODING_SERVICES_STATUS_COMMAND,
  NATIVE_SHARED_LSP_STATUS_COMMAND,
  NATIVE_SHARED_MCP_LIST_SERVERS_COMMAND,
} from "../src/services/nativeAgent";
import { mockInvoke } from "./tauriMock";

describe("NativeSharedServicesSection", () => {
  it("加载共享 Services 状态并标明 AgentKit 数据源", async () => {
    const view = render(NativeSharedServicesSection);

    await waitFor(() => {
      expect(view.getByRole("heading", { level: 2, name: "共享 Services" })).toBeInTheDocument();
      expect(
        mockInvoke.mock.calls.some(([cmd]) => cmd === NATIVE_SHARED_CODING_SERVICES_STATUS_COMMAND),
      ).toBe(true);
      expect(
        mockInvoke.mock.calls.some(([cmd]) => cmd === NATIVE_SHARED_MCP_LIST_SERVERS_COMMAND),
      ).toBe(true);
      expect(
        mockInvoke.mock.calls.some(([cmd]) => cmd === NATIVE_SHARED_LSP_STATUS_COMMAND),
      ).toBe(true);
      expect(
        view.container.querySelector('[data-agent-id="settings.shared-services.source"]')?.textContent,
      ).toContain("agentkit.native_coding_bundle");
      expect(view.getByText("单实例共享")).toBeInTheDocument();
      expect(view.getByText("Git")).toBeInTheDocument();
      expect(view.getByText("MCP")).toBeInTheDocument();
      expect(view.getByText("Memory")).toBeInTheDocument();
      expect(view.getByText("LSP")).toBeInTheDocument();
    });
  });
});

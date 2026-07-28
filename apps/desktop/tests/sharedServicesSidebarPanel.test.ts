import { fireEvent, render, waitFor } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import SharedServicesSidebarPanel from "../src/components/chat/SharedServicesSidebarPanel.vue";
import {
  NATIVE_SHARED_CODING_SERVICES_STATUS_COMMAND,
  NATIVE_SHARED_GIT_STATUS_COMMAND,
  NATIVE_SHARED_MEMORY_WRITE_COMMAND,
} from "../src/services/nativeAgent";
import { mockInvoke } from "./tauriMock";

describe("SharedServicesSidebarPanel", () => {
  it("侧栏可读取共享 Git Status 并写入 Memory", async () => {
    const view = render(SharedServicesSidebarPanel, {
      props: {
        taskId: "task-1",
        projectId: "project-1",
        projectCwd: "/tmp/lilia-demo-repo",
      },
    });

    await waitFor(() => {
      expect(
        mockInvoke.mock.calls.some(([cmd]) => cmd === NATIVE_SHARED_CODING_SERVICES_STATUS_COMMAND),
      ).toBe(true);
      expect(
        view.container.querySelector('[data-agent-id="chat.shared-services.source"]')?.textContent,
      ).toContain("agentkit.native_coding_bundle");
    });

    const pathInput = view.container.querySelector(
      '[data-agent-id="chat.shared-services.git.path"]',
    ) as HTMLInputElement;
    expect(pathInput.value).toBe("/tmp/lilia-demo-repo");

    await fireEvent.click(
      view.container.querySelector(
        '[data-agent-id="chat.shared-services.git.status"]',
      ) as HTMLButtonElement,
    );

    await waitFor(() => {
      expect(
        mockInvoke.mock.calls.some(
          ([cmd, args]) =>
            cmd === NATIVE_SHARED_GIT_STATUS_COMMAND &&
            (args as { path?: string }).path === "/tmp/lilia-demo-repo",
        ),
      ).toBe(true);
      expect(
        view.container.querySelector('[data-agent-id="chat.shared-services.result"]')?.textContent,
      ).toContain('"kind": "status"');
    });

    await fireEvent.click(
      view.container.querySelector(
        '[data-agent-id="chat.shared-services.tab.memory"]',
      ) as HTMLButtonElement,
    );
    const textArea = view.container.querySelector(
      '[data-agent-id="chat.shared-services.memory.text"]',
    ) as HTMLTextAreaElement;
    await fireEvent.update(textArea, "sidebar memory probe");
    await fireEvent.click(
      view.container.querySelector(
        '[data-agent-id="chat.shared-services.memory.write-btn"]',
      ) as HTMLButtonElement,
    );

    await waitFor(() => {
      expect(
        mockInvoke.mock.calls.some(
          ([cmd, args]) =>
            cmd === NATIVE_SHARED_MEMORY_WRITE_COMMAND &&
            (args as { text?: string }).text === "sidebar memory probe",
        ),
      ).toBe(true);
      expect(
        view.container.querySelector('[data-agent-id="chat.shared-services.result"]')?.textContent,
      ).toContain("mock-memory-1");
    });
  });
});

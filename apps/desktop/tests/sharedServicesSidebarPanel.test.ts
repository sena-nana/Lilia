import { fireEvent, render, waitFor } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import SharedServicesSidebarPanel from "../src/components/chat/SharedServicesSidebarPanel.vue";
import {
  NATIVE_SHARED_CODE_INDEX_SEARCH_COMMAND,
  NATIVE_SHARED_CODING_SERVICES_STATUS_COMMAND,
  NATIVE_SHARED_GIT_STATUS_COMMAND,
  NATIVE_SHARED_MEMORY_WRITE_COMMAND,
  NATIVE_SHARED_WORKSPACE_LIST_COMMAND,
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
      expect(view.getByText("已连接")).toBeInTheDocument();
    });

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
      expect(view.getByText("main · 工作区干净")).toBeInTheDocument();
    });

    await fireEvent.click(
      view.container.querySelector(
        '[data-agent-id="chat.shared-services.tab.files"]',
      ) as HTMLButtonElement,
    );
    await fireEvent.click(
      view.container.querySelector(
        '[data-agent-id="chat.shared-services.files.list"]',
      ) as HTMLButtonElement,
    );
    await waitFor(() => {
      expect(
        mockInvoke.mock.calls.some(
          ([cmd, args]) =>
            cmd === NATIVE_SHARED_WORKSPACE_LIST_COMMAND &&
            (args as { root?: string }).root === "/tmp/lilia-demo-repo",
        ),
      ).toBe(true);
      expect(view.getByText("Cargo.toml")).toBeInTheDocument();
    });

    await fireEvent.click(
      view.container.querySelector(
        '[data-agent-id="chat.shared-services.tab.index"]',
      ) as HTMLButtonElement,
    );
    await fireEvent.update(
      view.container.querySelector(
        '[data-agent-id="chat.shared-services.index.query"]',
      ) as HTMLInputElement,
      "NativeRuntimeBootstrap",
    );
    await fireEvent.click(
      view.container.querySelector(
        '[data-agent-id="chat.shared-services.index.search"]',
      ) as HTMLButtonElement,
    );
    await waitFor(() => {
      const call = mockInvoke.mock.calls.find(
        ([cmd]) => cmd === NATIVE_SHARED_CODE_INDEX_SEARCH_COMMAND,
      );
      expect(call?.[1]).toMatchObject({
        workspaceId: "project-1",
        root: "/tmp/lilia-demo-repo",
        query: "NativeRuntimeBootstrap",
      });
      expect(call?.[1]).not.toHaveProperty("content");
      expect(view.getByText("src/main.rs")).toBeInTheDocument();
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
    });
  });
});

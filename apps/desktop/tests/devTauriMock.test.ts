import { describe, expect, it } from "vitest";
import {
  PROJECT_DASHBOARD_LIST_COMMAND,
  PROJECT_LIST_COMMAND,
  QUOTA_USAGE_GET_STATS_COMMAND,
  TASK_LIST_COMMAND,
  type QuotaUsageStats,
} from "@lilia/contracts";
import { getCurrentWindow, invoke, listen } from "../src/tauri/devMock";

const DEV_MOCK_TEST_EVENT_NAME = "mock";

describe("dev Tauri mock", () => {
  it("provides the minimum browser-preview shell data", async () => {
    await expect(invoke(PROJECT_LIST_COMMAND)).resolves.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: "lilia" })]),
    );
    await expect(invoke(PROJECT_DASHBOARD_LIST_COMMAND)).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "lilia",
          statusCounts: expect.objectContaining({ running: 1 }),
          totalTokens: expect.any(Number),
        }),
      ]),
    );
    await expect(invoke(TASK_LIST_COMMAND, { projectId: null })).resolves.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: "o-001" })]),
    );
    await expect(listen(DEV_MOCK_TEST_EVENT_NAME, () => undefined)).resolves.toEqual(
      expect.any(Function),
    );
    expect(getCurrentWindow().label).toBe("main");
  });

  it("provides usable local quota usage stats for native-agentkit", async () => {
    const stats = await invoke<QuotaUsageStats>(QUOTA_USAGE_GET_STATS_COMMAND, {
      input: { days: 30, backend: "native-agentkit" },
    });

    expect(stats).toMatchObject({ days: 30, backend: "native-agentkit" });
    expect(stats.daily).toHaveLength(30);
    expect(stats.totals.totalTokens).toBeGreaterThan(0);
    expect(stats.backends).toEqual([
      expect.objectContaining({ backend: "native-agentkit", totalTokens: expect.any(Number) }),
    ]);
    expect(stats.projects).toEqual([
      expect.objectContaining({ projectId: "lilia", totalTokens: stats.totals.totalTokens }),
    ]);
  });
});

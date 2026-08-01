import { waitFor } from "@testing-library/vue";
import { describe, expect, it, vi } from "vitest";
import { TASKS_CHANGED_EVENT_NAME } from "@lilia/contracts";
import { PRODUCT_EVENT_NAME } from "@lilia/contracts/productCoreContract.mjs";
import {
  failNextMockListen,
  mockListen,
  mockListenerCount,
} from "./tauriMock";

describe("global data listener installation", () => {
  it("keeps tasks changed listener installation idempotent and retryable", async () => {
    vi.resetModules();
    const baseline = mockListenerCount(PRODUCT_EVENT_NAME);
    const listenAttemptCount = () =>
      mockListen.mock.calls.filter(([event]) => event === PRODUCT_EVENT_NAME).length;
    const initialListenAttempts = listenAttemptCount();
    failNextMockListen(PRODUCT_EVENT_NAME, "tasks listener failed");
    const module = await import("../src/data/tasks");

    await waitFor(() => {
      expect(listenAttemptCount()).toBeGreaterThan(initialListenAttempts);
    });
    expect(mockListenerCount(PRODUCT_EVENT_NAME)).toBe(baseline);

    module.installTasksChangedListener();
    await waitFor(() => {
      expect(mockListenerCount(PRODUCT_EVENT_NAME)).toBe(baseline + 1);
    });

    module.installTasksChangedListener();
    expect(mockListenerCount(PRODUCT_EVENT_NAME)).toBe(baseline + 1);
  });

  it("retries sidebar conversation listener registration after an initial failure", async () => {
    vi.resetModules();
    const baseline = mockListenerCount(TASKS_CHANGED_EVENT_NAME);
    const listenAttemptCount = () =>
      mockListen.mock.calls.filter(([event]) => event === TASKS_CHANGED_EVENT_NAME).length;
    const initialListenAttempts = listenAttemptCount();
    failNextMockListen(TASKS_CHANGED_EVENT_NAME, "sidebar listener failed");
    const module = await import("../src/data/sidebarConversations");

    await waitFor(() => {
      expect(listenAttemptCount()).toBeGreaterThan(initialListenAttempts);
    });
    expect(mockListenerCount(TASKS_CHANGED_EVENT_NAME)).toBe(baseline);

    await module.ensureSidebarConversationsLoaded(true);

    await waitFor(() => {
      expect(mockListenerCount(TASKS_CHANGED_EVENT_NAME)).toBe(baseline + 1);
    });
  });

  it("retries project dashboard listener registration after an initial failure", async () => {
    vi.resetModules();
    const baseline = mockListenerCount(TASKS_CHANGED_EVENT_NAME);
    const listenAttemptCount = () =>
      mockListen.mock.calls.filter(([event]) => event === TASKS_CHANGED_EVENT_NAME).length;
    const initialListenAttempts = listenAttemptCount();
    failNextMockListen(TASKS_CHANGED_EVENT_NAME, "dashboard listener failed");
    const module = await import("../src/data/projectDashboard");

    await waitFor(() => {
      expect(listenAttemptCount()).toBeGreaterThan(initialListenAttempts);
    });
    expect(mockListenerCount(TASKS_CHANGED_EVENT_NAME)).toBe(baseline);

    await module.ensureProjectDashboardLoaded(true);

    await waitFor(() => {
      expect(mockListenerCount(TASKS_CHANGED_EVENT_NAME)).toBe(baseline + 1);
    });
  });
});

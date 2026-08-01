import { waitFor } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import { PRODUCT_EVENT_NAME } from "@lilia/contracts/productCoreContract.mjs";
import {
  installTasksChangedListener,
  ensureProjectTasksLoaded,
  listProjectConversations,
} from "../src/data/tasks";
import {
  createProductEntity,
  newProductCommandMeta,
} from "../src/services/productCore";
import { mockListenerCount } from "./tauriMock";

describe("tasks:changed sync", () => {
  it("收到 task 变更事件后刷新对应项目对话缓存", async () => {
    installTasksChangedListener({ force: true });
    await waitFor(() => {
      expect(mockListenerCount(PRODUCT_EVENT_NAME)).toBeGreaterThan(0);
    });
    await ensureProjectTasksLoaded("lilia", true);
    expect(
      listProjectConversations("lilia").some((task) => task.title === "弹窗首条消息"),
    ).toBe(false);

    const now = Date.now();
    await createProductEntity(
      newProductCommandMeta("test-task-created"),
      {
        kind: "task",
        value: {
          id: "task-product-event",
          projectId: "lilia",
          title: "弹窗首条消息",
          description: null,
          status: "running",
          priority: "normal",
          assignmentId: null,
          completionCriteria: [],
          milestoneId: null,
          workflowId: null,
          agentProfileId: null,
          blockedReason: null,
          dependsOn: [],
          parentId: null,
          pinned: false,
          sortOrder: 99,
          archived: false,
          tags: [],
          createdAt: now,
          updatedAt: now,
          revision: 1,
          legacySource: null,
        },
      },
      "created",
    );

    await waitFor(() => {
      expect(
        listProjectConversations("lilia").some((task) => task.title === "弹窗首条消息"),
      ).toBe(true);
    });
  });
});

import { describe, expect, it } from "vitest";
import {
  PRODUCT_UPDATE_ENTITY_COMMAND,
  TASK_ARCHIVE_COMMAND,
  TASK_ARCHIVE_PROJECT_COMMAND,
  TASK_REORDER_COMMAND,
  TASK_REPARENT_COMMAND,
} from "@lilia/contracts";
import {
  archiveTask,
  archiveProjectConversations,
  ensureProjectTasksLoaded,
  listProjectConversations,
  reparentTask,
  reorderTasks,
} from "../src/data/tasks";
import { listProductEntities } from "../src/services/productCore";
import { mockInvoke } from "./tauriMock";

describe("task mutation application boundary", () => {
  it("reorders one complete task pin group through the shared desktop command", async () => {
    await ensureProjectTasksLoaded("lilia", true);
    mockInvoke.mockClear();

    await reorderTasks("lilia", ["t-002", "t-001"]);

    expect(listProjectConversations("lilia").map((task) => task.id)).toEqual([
      "t-002",
      "t-001",
    ]);
    const reorderArgs = mockInvoke.mock.calls.find(([command]) =>
      command === TASK_REORDER_COMMAND
    )?.[1];
    expect(reorderArgs).toEqual({
      projectId: "lilia",
      orderedIds: ["t-002", "t-001"],
    });
    expect(mockInvoke.mock.calls.some(([command]) => command === PRODUCT_UPDATE_ENTITY_COMMAND))
      .toBe(false);
  });

  it("moves the task and its conversation through the shared desktop command", async () => {
    await Promise.all([
      ensureProjectTasksLoaded("lilia", true),
      ensureProjectTasksLoaded("tools", true),
    ]);
    mockInvoke.mockClear();

    await reparentTask("t-002", "lilia", "tools", null);

    expect(listProjectConversations("lilia").some((task) => task.id === "t-002")).toBe(false);
    expect(listProjectConversations("tools").some((task) => task.id === "t-002")).toBe(true);
    const conversations = await listProductEntities("conversation");
    const movedConversation = conversations.find((entity) =>
      entity.kind === "conversation" && entity.value.taskId === "t-002"
    );
    expect(movedConversation?.kind === "conversation" && movedConversation.value.projectId)
      .toBe("tools");
    const reparentArgs = mockInvoke.mock.calls.find(([command]) =>
      command === TASK_REPARENT_COMMAND
    )?.[1];
    expect(reparentArgs).toEqual({
      taskId: "t-002",
      newProjectId: "tools",
      newParentId: null,
    });
    expect(mockInvoke.mock.calls.some(([command]) => command === PRODUCT_UPDATE_ENTITY_COMMAND))
      .toBe(false);
  });

  it("archives one project fact set through a single shared desktop command", async () => {
    await ensureProjectTasksLoaded("lilia", true);
    mockInvoke.mockClear();

    const archived = await archiveProjectConversations("lilia");

    expect(archived).toBe(2);
    expect(listProjectConversations("lilia")).toEqual([]);
    expect(mockInvoke.mock.calls.filter(([command]) =>
      command === TASK_ARCHIVE_PROJECT_COMMAND
    )).toEqual([[TASK_ARCHIVE_PROJECT_COMMAND, { projectId: "lilia" }, undefined]]);
    expect(mockInvoke.mock.calls.some(([command]) => command === PRODUCT_UPDATE_ENTITY_COMMAND))
      .toBe(false);
  });

  it("archives one task and its conversation through a single shared desktop command", async () => {
    await ensureProjectTasksLoaded("lilia", true);
    mockInvoke.mockClear();

    expect(await archiveTask("t-001")).toBe(true);

    expect(listProjectConversations("lilia").some((task) => task.id === "t-001")).toBe(false);
    const conversation = (await listProductEntities("conversation")).find((entity) =>
      entity.kind === "conversation" && entity.value.taskId === "t-001"
    );
    expect(conversation?.kind === "conversation" && conversation.value.archived).toBe(true);
    expect(conversation?.kind === "conversation" && conversation.value.status).toBe("closed");
    expect(mockInvoke.mock.calls.filter(([command]) => command === TASK_ARCHIVE_COMMAND))
      .toEqual([[TASK_ARCHIVE_COMMAND, { id: "t-001" }, undefined]]);
    expect(mockInvoke.mock.calls.some(([command]) => command === PRODUCT_UPDATE_ENTITY_COMMAND))
      .toBe(false);
  });
});

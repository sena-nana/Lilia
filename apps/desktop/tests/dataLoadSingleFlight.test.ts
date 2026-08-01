import { describe, expect, it } from "vitest";
import { PRODUCT_GET_ENTITY_COMMAND } from "@lilia/contracts/productCoreContract.mjs";
import {
  ensureProjectLoaded,
  PROJECTS,
} from "../src/data/projects";
import {
  ensureTaskLoaded,
  ORPHAN_LIST,
  TASKS,
} from "../src/data/tasks";
import { mockInvoke } from "./tauriMock";

describe("data loading single-flight", () => {
  it("deduplicates concurrent task row loads for the same task id", async () => {
    TASKS.value = {};
    ORPHAN_LIST.value = [];
    mockInvoke.mockClear();

    await Promise.all([
      ensureTaskLoaded("t-001", "lilia"),
      ensureTaskLoaded("t-001", "lilia"),
    ]);

    const taskGets = mockInvoke.mock.calls.filter(([cmd, args]) =>
      cmd === PRODUCT_GET_ENTITY_COMMAND &&
      (args as { kind?: string; id?: string }).kind === "task" &&
      (args as { id?: string }).id === "t-001"
    );
    expect(taskGets).toHaveLength(1);
  });

  it("deduplicates concurrent project loads for the same project id", async () => {
    PROJECTS.value = [];
    mockInvoke.mockClear();

    await Promise.all([
      ensureProjectLoaded("lilia"),
      ensureProjectLoaded("lilia"),
    ]);

    const projectGets = mockInvoke.mock.calls.filter(([cmd, args]) =>
      cmd === PRODUCT_GET_ENTITY_COMMAND &&
      (args as { kind?: string; id?: string }).kind === "project" &&
      (args as { id?: string }).id === "lilia"
    );
    expect(projectGets).toHaveLength(1);
  });
});

import { beforeEach, describe, expect, it } from "vitest";
import { TASK_UPDATE_COMMAND } from "@lilia/contracts";
import { ensureSessionSearchCorpusLoaded, searchSessions } from "../src/services/sessionSearch";
import { mockInvoke, resetTauriMockData } from "./tauriMock";

describe("sessionSearch", () => {
  beforeEach(async () => {
    resetTauriMockData();
    await ensureSessionSearchCorpusLoaded(true);
  });

  it("合并子串与相似度结果并按分值从高到低排序", async () => {
    const res = await searchSessions("tsconfig");
    expect(res.length).toBeGreaterThan(0);
    for (const r of res) {
      expect(r.score).toBeGreaterThan(0);
    }

    const routes = res.map((r) => r.route);
    expect(new Set(routes).size).toBe(routes.length);

    const scores = res.map((r) => r.score);
    const sorted = [...scores].sort((a, b) => b - a);
    expect(scores).toEqual(sorted);
  });

  it("project-task 走 /projects 路由，orphan 走 /chats 路由", async () => {
    const res = await searchSessions("Claude");
    const task = res.find((r) => r.kind === "project-task");
    const orphan = res.find((r) => r.kind === "orphan");
    expect(task?.route.startsWith("/projects/")).toBe(true);
    expect(orphan?.route.startsWith("/chats/")).toBe(true);
  });

  it("强制刷新后会更新搜索结果", async () => {
    await mockInvoke(TASK_UPDATE_COMMAND, {
      id: "t-002",
      title: "新的统一侧栏标题",
    });
    await ensureSessionSearchCorpusLoaded(true);

    const res = await searchSessions("统一侧栏标题");
    expect(res[0]).toMatchObject({
      taskId: "t-002",
      route: "/projects/lilia/tasks/t-002",
    });
  });
});

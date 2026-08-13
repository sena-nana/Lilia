/**
 * 会话搜索：薄适配共享 `DesktopApplication::search_sessions`。
 * 语料始终来自 Product Core 任务标题，不读前端 UI 缓存。
 */

import { SEARCH_SESSIONS_COMMAND } from "@lilia/contracts";
import { invoke } from "../tauri/runtime";

export type SearchKind = "project-task" | "orphan";

export interface SearchResult {
  kind: SearchKind;
  /** 项目任务才有；orphan 时为 undefined。 */
  projectId?: string;
  /** 项目展示名；orphan 时为 undefined（UI 显示「收集箱」标签）。 */
  projectName?: string;
  taskId: string;
  title: string;
  /** 直接可以 router.push 的路径。 */
  route: string;
  /** 越大越相关。不同模式的量纲不同，UI 只用来排序。 */
  score: number;
  /** title 上需要高亮的 [start, end) 区间。纯向量命中时为空。 */
  highlights: Array<[number, number]>;
}

interface SharedSearchResult {
  kind: "project-task" | "orphan" | "project_task";
  projectId?: string | null;
  projectName?: string | null;
  taskId: string;
  title: string;
  route: string;
  score: number;
  highlights: Array<[number, number]>;
}

function mapSharedResult(result: SharedSearchResult): SearchResult {
  const kind: SearchKind =
    result.kind === "project-task" || result.kind === "project_task"
      ? "project-task"
      : "orphan";
  return {
    kind,
    projectId: result.projectId ?? undefined,
    projectName: result.projectName ?? undefined,
    taskId: result.taskId,
    title: result.title,
    route: result.route,
    score: result.score,
    highlights: result.highlights ?? [],
  };
}

export async function searchSessions(
  query: string,
  limit = 100,
): Promise<SearchResult[]> {
  const trimmed = query.trim();
  if (!trimmed) return [];
  const results = await invoke<SharedSearchResult[]>(SEARCH_SESSIONS_COMMAND, {
    query: trimmed,
    limit,
  });
  return results.map(mapSharedResult);
}

/** 共享搜索按 Product Core 即时建库，无需前端语料预热。 */
export async function ensureSessionSearchCorpusLoaded(_force = false): Promise<void> {
  return;
}

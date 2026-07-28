import { createCodexAppServer, initializeCodexAppServer } from "./appServer.mjs";
import { codexHistoryTimelineEvents } from "./timeline.mjs";
import {
  DEFAULT_TURN_LIMIT,
  normalizeCodexThread,
  previewCodexThreadLiteWithServer,
  readCodexThreadTurns,
  renameCodexThreadWithServer,
  searchCodexThreadsWithServer,
} from "./threadData.mjs";
import { stringOrNull } from "../utils.mjs";

export { readCodexThreadTurns };

async function readAllCodexThreadTurns(server, threadId, { limit = DEFAULT_TURN_LIMIT } = {}) {
  const turns = [];
  let cursor = null;
  do {
    const previousCursor = cursor;
    const result = await readCodexThreadTurns(server, threadId, { limit, cursor });
    turns.push(...result.turns);
    cursor = stringOrNull(result.nextCursor)?.trim() || null;
    if (cursor && cursor === previousCursor) {
      throw new Error("Codex history preview 返回了重复 cursor，已停止读取。");
    }
  } while (cursor);
  return turns;
}

export function codexHistoryTimelineInputs(taskId, threadId, turns) {
  return codexHistoryTimelineEvents(threadId, turns).map((event) => ({
    id: event.sourceId ? `${taskId}:${event.turnIdOverride || event.turnId || "history"}:${event.sourceId}` : null,
    taskId,
    turnId: event.turnIdOverride || null,
    backend: "codex",
    kind: event.kind,
    status: event.status,
    title: event.title,
    summary: event.summary || null,
    payload: event.payload || {},
    createdAt: event.createdAt ?? null,
    updatedAt: event.updatedAt ?? null,
  }));
}

export async function searchCodexThreads(input = {}, { createServer = createCodexAppServer } = {}) {
  const server = createServer();
  try {
    await initializeCodexAppServer(server);
    return searchCodexThreadsWithServer(server, input);
  } finally {
    server.close();
  }
}

export async function cleanCodexThreadBackgroundTerminals(
  threadId,
  { createServer = createCodexAppServer } = {},
) {
  const trimmed = stringOrNull(threadId)?.trim();
  if (!trimmed) throw new Error("Codex threadId is required");
  const server = createServer();
  try {
    await initializeCodexAppServer(server);
    await server.request("thread/backgroundTerminals/clean", { threadId: trimmed });
    return { threadId: trimmed, cleaned: true };
  } finally {
    server.close();
  }
}

export async function archiveCodexThread(
  threadId,
  { createServer = createCodexAppServer } = {},
) {
  const trimmed = stringOrNull(threadId)?.trim();
  if (!trimmed) throw new Error("Codex threadId is required");
  const server = createServer();
  try {
    await initializeCodexAppServer(server);
    await server.request("thread/archive", { threadId: trimmed });
    return { threadId: trimmed, archived: true };
  } finally {
    server.close();
  }
}

export async function renameCodexThread(
  threadId,
  name,
  { createServer = createCodexAppServer } = {},
) {
  const server = createServer();
  try {
    await initializeCodexAppServer(server);
    return renameCodexThreadWithServer(server, threadId, name);
  } finally {
    server.close();
  }
}

export async function previewCodexThread(threadId, { createServer = createCodexAppServer } = {}) {
  const server = createServer();
  try {
    await initializeCodexAppServer(server);
    const turns = await readAllCodexThreadTurns(server, threadId);
    const events = codexHistoryTimelineInputs("preview", threadId, turns);
    const thread = normalizeCodexThread({ id: threadId }, turns);
    return {
      thread,
      events,
      eventCount: events.length,
    };
  } finally {
    server.close();
  }
}

export async function previewCodexThreadLite(threadId, { createServer = createCodexAppServer } = {}) {
  const server = createServer();
  try {
    await initializeCodexAppServer(server);
    return previewCodexThreadLiteWithServer(server, threadId);
  } finally {
    server.close();
  }
}

export async function syncCodexThreadHistoryForTask(
  { taskId, threadId, limit = DEFAULT_TURN_LIMIT, cursor = null },
  { createServer = createCodexAppServer } = {},
) {
  const server = createServer();
  try {
    await initializeCodexAppServer(server);
    const { turns, nextCursor } = await readCodexThreadTurns(server, threadId, { limit, cursor });
    const events = codexHistoryTimelineInputs(taskId, threadId, turns);
    const thread = normalizeCodexThread({ id: threadId }, turns);
    return {
      thread,
      events,
      eventCount: events.length,
      nextCursor,
    };
  } finally {
    server.close();
  }
}

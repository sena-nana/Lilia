import {
  CODEX_HISTORY_DEFAULT_TURN_LIMIT,
  CODEX_HISTORY_PREVIEW_TURN_LIMIT,
  HISTORY_IMPORT_DEFAULT_SEARCH_LIMIT,
  HISTORY_IMPORT_INLINE_PREVIEW_TEXT_LIMIT,
  HISTORY_IMPORT_MAX_SEARCH_LIMIT,
  HISTORY_IMPORT_MESSAGE_SUMMARY_TEXT_LIMIT,
  HISTORY_IMPORT_PREVIEW_MESSAGE_LIMIT,
  HISTORY_IMPORT_TITLE_TEXT_LIMIT,
} from "@lilia/contracts/historyImportContract.mjs";
import { isRecord, shortText, stringOrNull } from "../utils.mjs";

export const DEFAULT_TURN_LIMIT = CODEX_HISTORY_DEFAULT_TURN_LIMIT;
export const PREVIEW_TURN_LIMIT = CODEX_HISTORY_PREVIEW_TURN_LIMIT;
export const PREVIEW_MESSAGE_LIMIT = HISTORY_IMPORT_PREVIEW_MESSAGE_LIMIT;
const ITEM_BACKFILL_CONCURRENCY = 4;

function numberOrNull(value) {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function millisFromSeconds(value) {
  const seconds = numberOrNull(value);
  return seconds === null ? null : Math.trunc(seconds * 1000);
}

function readArray(value) {
  return Array.isArray(value) ? value.filter(isRecord) : [];
}

function firstString(...values) {
  for (const value of values) {
    const text = stringOrNull(value)?.trim();
    if (text) return text;
  }
  return null;
}

function pickMessageText(item) {
  if (!isRecord(item)) return "";
  if (typeof item.text === "string") return item.text;
  if (typeof item.content === "string") return item.content;
  if (Array.isArray(item.content)) {
    return item.content
      .map((part) => {
        if (typeof part === "string") return part;
        if (!isRecord(part)) return "";
        return stringOrNull(part.text) || stringOrNull(part.content) || "";
      })
      .filter(Boolean)
      .join("\n");
  }
  return "";
}

function previewRoleForItem(item) {
  if (item?.type === "userMessage") return "user";
  if (item?.type === "agentMessage") return "assistant";
  return null;
}

function previewFromTurns(turns) {
  for (let index = turns.length - 1; index >= 0; index -= 1) {
    const items = readArray(turns[index]?.items);
    for (let itemIndex = items.length - 1; itemIndex >= 0; itemIndex -= 1) {
      const item = items[itemIndex];
      if (!previewRoleForItem(item)) continue;
      const text = pickMessageText(item);
      if (text.trim()) return shortText(text, HISTORY_IMPORT_INLINE_PREVIEW_TEXT_LIMIT);
    }
  }
  return null;
}

export function normalizeCodexThread(row, turns = []) {
  const thread = isRecord(row?.thread) ? row.thread : {};
  const id = firstString(row?.id, row?.threadId, thread.id);
  if (!id) return null;
  const title = firstString(
    row.title,
    row.name,
    row.summary,
    thread.title,
    thread.name,
    previewFromTurns(turns),
  ) || "Codex thread";
  return {
    id,
    title: shortText(title, HISTORY_IMPORT_TITLE_TEXT_LIMIT) || "Codex thread",
    status: firstString(row.status, thread.status),
    model: firstString(row.model, row.modelProvider, thread.model, thread.modelProvider),
    sourceKind: firstString(row.sourceKind, row.source_kind, row.source?.kind),
    projectCwd: firstString(
      row.cwd,
      row.projectCwd,
      row.project_cwd,
      row.workspaceRoot,
      row.workspace_root,
      thread.cwd,
      thread.projectCwd,
      thread.project_cwd,
      thread.workspaceRoot,
      thread.workspace_root,
    ),
    createdAt: millisFromSeconds(row.createdAt ?? row.created_at ?? thread.createdAt),
    updatedAt: millisFromSeconds(
      row.updatedAt ??
        row.updated_at ??
        row.lastUpdatedAt ??
        thread.updatedAt ??
        thread.lastUpdatedAt,
    ),
    archived: row.archived === true,
    preview: firstString(row.snippet, row.preview, row.description, thread.preview, previewFromTurns(turns)),
  };
}

export function normalizeCodexSearchResult(result) {
  const data = readArray(result?.data || result?.threads || result?.items);
  const threads = data
    .map((row) => normalizeCodexThread(row))
    .filter(Boolean);
  return {
    threads,
    nextCursor: stringOrNull(result?.nextCursor) || stringOrNull(result?.next_cursor),
  };
}

export async function searchCodexThreadsWithServer(server, input = {}) {
  const params = {
    limit: Math.max(1, Math.min(
      HISTORY_IMPORT_MAX_SEARCH_LIMIT,
      Number(input.limit) || HISTORY_IMPORT_DEFAULT_SEARCH_LIMIT,
    )),
    sortDirection: "desc",
    archived: input.archived === true,
  };
  const searchTerm = stringOrNull(input.searchTerm)?.trim();
  const cursor = stringOrNull(input.cursor)?.trim();
  if (cursor) params.cursor = cursor;
  if (searchTerm) {
    params.searchTerm = searchTerm;
    return normalizeCodexSearchResult(await server.request("thread/search", params));
  }
  return normalizeCodexSearchResult(await server.request("thread/list", params));
}

export async function renameCodexThreadWithServer(server, threadId, name) {
  const trimmedThreadId = stringOrNull(threadId)?.trim();
  const trimmedName = stringOrNull(name)?.trim();
  if (!trimmedThreadId) throw new Error("Codex threadId is required");
  if (!trimmedName) throw new Error("Codex thread name is required");
  await server.request("thread/name/set", {
    threadId: trimmedThreadId,
    name: trimmedName,
  });
  return { threadId: trimmedThreadId, name: trimmedName, renamed: true };
}

export async function archiveCodexThreadWithServer(server, threadId, archived = true) {
  const trimmedThreadId = stringOrNull(threadId)?.trim();
  if (!trimmedThreadId) throw new Error("Codex threadId is required");
  await server.request("thread/archive", {
    threadId: trimmedThreadId,
    archived: archived !== false,
  });
  return { threadId: trimmedThreadId, archived: archived !== false };
}

function needsItemBackfill(turn) {
  if (!isRecord(turn)) return false;
  if (!Array.isArray(turn.items)) return true;
  return turn.itemsTruncated === true ||
    turn.items_truncated === true ||
    turn.hasMoreItems === true ||
    turn.has_more_items === true;
}

async function readTurnItems(server, threadId, turn) {
  const turnId = stringOrNull(turn?.id);
  if (!threadId || !turnId) return turn;
  try {
    const result = await server.request("thread/turns/items/list", {
      threadId,
      turnId,
      limit: 200,
      sortDirection: "asc",
    });
    const items = readArray(result?.data || result?.items);
    return { ...turn, items };
  } catch {
    return turn;
  }
}

async function mapWithConcurrency(items, limit, mapper) {
  const out = new Array(items.length);
  let index = 0;
  const workerCount = Math.max(1, Math.min(limit, items.length));
  await Promise.all(Array.from({ length: workerCount }, async () => {
    while (index < items.length) {
      const current = index;
      index += 1;
      out[current] = await mapper(items[current], current);
    }
  }));
  return out;
}

export async function readCodexThreadTurns(
  server,
  threadId,
  {
    limit = DEFAULT_TURN_LIMIT,
    sortDirection = "asc",
    backfillConcurrency = ITEM_BACKFILL_CONCURRENCY,
    cursor = null,
  } = {},
) {
  const params = {
    threadId,
    limit,
    sortDirection,
    itemsView: "full",
  };
  const trimmedCursor = stringOrNull(cursor)?.trim();
  if (trimmedCursor) params.cursor = trimmedCursor;
  const result = await server.request("thread/turns/list", params);
  const turns = readArray(result?.data || result?.turns);
  const out = await mapWithConcurrency(
    turns,
    backfillConcurrency,
    (turn) => needsItemBackfill(turn) ? readTurnItems(server, threadId, turn) : turn,
  );
  return {
    turns: out,
    nextCursor: stringOrNull(result?.nextCursor) || stringOrNull(result?.next_cursor),
  };
}

export async function backfillCodexTurnsUntilMessages(server, threadId, turns, messageLimit) {
  const out = turns.slice();
  const found = [];
  for (let index = 0; index < out.length && found.length < messageLimit; index += 1) {
    if (needsItemBackfill(out[index])) {
      out[index] = await readTurnItems(server, threadId, out[index]);
    }
    const items = readArray(out[index]?.items);
    for (let itemIndex = items.length - 1; itemIndex >= 0 && found.length < messageLimit; itemIndex -= 1) {
      if (previewRoleForItem(items[itemIndex])) found.push(items[itemIndex]);
    }
  }
  return out;
}

export function codexPreviewMessagesFromTurns(turns, messageLimit = PREVIEW_MESSAGE_LIMIT) {
  const messages = [];
  for (const turn of turns) {
    for (const item of readArray(turn?.items)) {
      const role = previewRoleForItem(item);
      if (!role) continue;
      messages.push({
        id: firstString(item.id, item.clientId) || `${role}:${messages.length}`,
        role,
        summary: shortText(pickMessageText(item), HISTORY_IMPORT_MESSAGE_SUMMARY_TEXT_LIMIT) || null,
      });
    }
  }
  return messages.slice(-messageLimit);
}

export function codexPreviewEventCountFromTurns(turns) {
  return turns.reduce((count, turn) => count + readArray(turn?.items).filter((item) => {
    switch (item.type) {
      case "agentMessage":
      case "userMessage":
      case "reasoning":
      case "commandExecution":
      case "fileChange":
      case "mcpToolCall":
      case "webSearch":
      case "plan":
        return true;
      default:
        return false;
    }
  }).length, 0);
}

export async function previewCodexThreadLiteWithServer(server, threadId) {
  const result = await server.request("thread/turns/list", {
    threadId,
    limit: PREVIEW_TURN_LIMIT,
    sortDirection: "desc",
    itemsView: "full",
  });
  const recentTurns = await backfillCodexTurnsUntilMessages(
    server,
    threadId,
    Array.isArray(result?.data || result?.turns) ? (result.data || result.turns) : [],
    PREVIEW_MESSAGE_LIMIT,
  );
  const turns = recentTurns.slice().reverse();
  return {
    thread: normalizeCodexThread({ id: threadId }, turns),
    eventCount: codexPreviewEventCountFromTurns(turns),
    messages: codexPreviewMessagesFromTurns(turns),
    hasFullPreview: true,
  };
}

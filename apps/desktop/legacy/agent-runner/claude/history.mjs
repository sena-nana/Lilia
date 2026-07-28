import { readdir, readFile } from "node:fs/promises";
import { basename, join } from "node:path";
import { normalizeClaudeTool } from "@lilia/contracts/claudeTools.mjs";
import {
  HISTORY_IMPORT_DEFAULT_SEARCH_LIMIT,
  HISTORY_IMPORT_DEFAULT_SYNC_LIMIT,
  HISTORY_IMPORT_ERROR_SUMMARY_TEXT_LIMIT,
  HISTORY_IMPORT_MAX_SEARCH_LIMIT,
  HISTORY_IMPORT_MAX_SYNC_LIMIT,
  HISTORY_IMPORT_MESSAGE_SUMMARY_TEXT_LIMIT,
  HISTORY_IMPORT_PREVIEW_MESSAGE_LIMIT,
  HISTORY_IMPORT_PREVIEW_TEXT_LIMIT,
  HISTORY_IMPORT_TITLE_TEXT_LIMIT,
} from "@lilia/contracts/historyImportContract.mjs";
import {
  isRecord,
  normalizeTimelineStatus,
  oneLineSummary,
  shortText,
  stringOrNull,
} from "../utils.mjs";

function defaultClaudeProjectsDir() {
  const home = process.env.USERPROFILE || process.env.HOME || "";
  return home ? join(home, ".claude", "projects") : "";
}

function limitValue(value, fallback = HISTORY_IMPORT_DEFAULT_SEARCH_LIMIT) {
  const number = Number(value);
  if (!Number.isFinite(number)) return fallback;
  return Math.max(1, Math.min(HISTORY_IMPORT_MAX_SEARCH_LIMIT, Math.trunc(number)));
}

function parseCursor(value) {
  const number = Number(stringOrNull(value)?.trim() || 0);
  return Number.isFinite(number) && number > 0 ? Math.trunc(number) : 0;
}

function timestampMillis(value) {
  const text = stringOrNull(value);
  if (!text) return null;
  const millis = Date.parse(text);
  return Number.isFinite(millis) ? millis : null;
}

function readArray(value) {
  return Array.isArray(value) ? value.filter(isRecord) : [];
}

function contentText(content) {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((part) => {
        if (typeof part === "string") return part;
        if (!isRecord(part)) return "";
        if (typeof part.text === "string") return part.text;
        if (typeof part.content === "string") return part.content;
        if (Array.isArray(part.content)) return contentText(part.content);
        return "";
      })
      .filter(Boolean)
      .join("\n");
  }
  return "";
}

function messageText(entry) {
  const content = entry?.message?.content;
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((part) => {
        if (typeof part === "string") return part;
        if (!isRecord(part) || part.type !== "text") return "";
        return typeof part.text === "string" ? part.text : "";
      })
      .filter(Boolean)
      .join("\n");
  }
  return "";
}

function sessionIdFromPath(filePath) {
  return basename(filePath).replace(/\.jsonl$/i, "");
}

function slugToPath(slug) {
  const text = stringOrNull(slug);
  if (!text) return null;
  const match = text.match(/^([a-zA-Z])--(.+)$/);
  if (!match) return text;
  return `${match[1].toUpperCase()}:\\${match[2].replace(/-/g, "\\")}`;
}

async function listSessionFiles(projectsDir = defaultClaudeProjectsDir()) {
  if (!projectsDir) return [];
  let projects;
  try {
    projects = await readdir(projectsDir, { withFileTypes: true });
  } catch {
    return [];
  }
  const files = [];
  for (const project of projects) {
    if (!project.isDirectory()) continue;
    const projectDir = join(projectsDir, project.name);
    let entries;
    try {
      entries = await readdir(projectDir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      if (!entry.isFile() || !entry.name.endsWith(".jsonl")) continue;
      files.push({
        path: join(projectDir, entry.name),
        project: project.name,
        id: sessionIdFromPath(entry.name),
      });
    }
  }
  return files;
}

async function readJsonl(filePath) {
  let text;
  try {
    text = await readFile(filePath, "utf8");
  } catch {
    return [];
  }
  const out = [];
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    try {
      const entry = JSON.parse(line.replace(/^\uFEFF/, ""));
      if (isRecord(entry)) out.push(entry);
    } catch {
    }
  }
  return out;
}

function titleFromEntry(entry) {
  if (!isRecord(entry)) return null;
  if (entry.type === "ai-title") {
    return stringOrNull(entry.title) ||
      stringOrNull(entry.text) ||
      stringOrNull(entry.summary) ||
      stringOrNull(entry.content);
  }
  return null;
}

function normalizeSession(file, entries) {
  const sessionId = entries
    .map((entry) => stringOrNull(entry.sessionId))
    .find(Boolean) || file.id;
  const messageEntries = entries.filter((entry) =>
    entry.type === "user" || entry.type === "assistant"
  );
  const titled = entries.map(titleFromEntry).find((title) => title?.trim());
  const preview = messageEntries
    .slice()
    .reverse()
    .map(messageText)
    .find((text) => text.trim());
  const firstTime = entries.map((entry) => timestampMillis(entry.timestamp)).find((time) => time);
  const lastTime = entries
    .slice()
    .reverse()
    .map((entry) => timestampMillis(entry.timestamp))
    .find((time) => time);
  const cwd = entries
    .map((entry) => stringOrNull(entry.cwd))
    .find(Boolean) || slugToPath(file.project);
  const model = entries
    .map((entry) => stringOrNull(entry.message?.model) || stringOrNull(entry.model))
    .find(Boolean) || null;
  const title = titled || preview || "Claude session";
  return {
    id: sessionId,
    title: shortText(title, HISTORY_IMPORT_TITLE_TEXT_LIMIT) || "Claude session",
    status: null,
    model,
    sourceKind: "claude",
    createdAt: firstTime,
    updatedAt: lastTime,
    archived: false,
    preview: preview ? shortText(preview, HISTORY_IMPORT_PREVIEW_TEXT_LIMIT) : null,
    cwd,
    project: file.project,
    path: file.path,
  };
}

async function readSessions(projectsDir) {
  const files = await listSessionFiles(projectsDir);
  const sessions = [];
  for (const file of files) {
    const entries = await readJsonl(file.path);
    if (entries.length === 0) continue;
    sessions.push(normalizeSession(file, entries));
  }
  sessions.sort((a, b) => (b.updatedAt || 0) - (a.updatedAt || 0));
  return sessions;
}

function publicSession(session) {
  const { path: _path, ...rest } = session;
  return rest;
}

function matchesSearch(session, searchTerm) {
  const term = stringOrNull(searchTerm)?.trim().toLowerCase();
  if (!term) return true;
  return [
    session.id,
    session.title,
    session.preview,
    session.cwd,
    session.project,
    session.model,
  ].some((value) => stringOrNull(value)?.toLowerCase().includes(term));
}

async function findSession(sessionId, projectsDir) {
  const id = stringOrNull(sessionId)?.trim();
  if (!id) return null;
  const sessions = await readSessions(projectsDir);
  return sessions.find((session) => session.id === id) || null;
}

function eventBase(entry, sessionId, index, partIndex = 0) {
  const createdAt = (timestampMillis(entry.timestamp) || Date.now()) + partIndex;
  const entryId = stringOrNull(entry.uuid) || `${sessionId}:${index}`;
  return {
    turnId: entryId,
    createdAt,
    updatedAt: createdAt,
  };
}

function historyPayload(sessionId, turnId, itemId, payload = {}) {
  return {
    ...payload,
    backend: "claude",
    history: true,
    threadId: sessionId,
    sessionId,
    turnId,
    itemId,
  };
}

function messageEvent(sessionId, entry, index) {
  const role = entry?.message?.role === "assistant" ? "assistant" : "user";
  const text = messageText(entry);
  if (!text.trim()) return null;
  const base = eventBase(entry, sessionId, index);
  const itemId = stringOrNull(entry.uuid) || `${role}-${index}`;
  return {
    kind: "message",
    status: "success",
    title: role === "assistant" ? "Assistant" : "User",
    summary: shortText(text, HISTORY_IMPORT_MESSAGE_SUMMARY_TEXT_LIMIT) || "",
    payload: historyPayload(sessionId, base.turnId, itemId, { role, content: text }),
    sourceId: `claude-history:${sessionId}:${base.turnId}:${itemId}`,
    turnIdOverride: base.turnId,
    createdAt: base.createdAt,
    updatedAt: base.updatedAt,
  };
}

function thinkingEvent(sessionId, entry, part, index, partIndex) {
  const text = stringOrNull(part.thinking) || "";
  if (!text.trim()) return null;
  const base = eventBase(entry, sessionId, index, partIndex);
  const itemId = `${stringOrNull(entry.uuid) || index}:thinking:${partIndex}`;
  return {
    kind: "reasoning",
    status: "success",
    title: "思考中",
    summary: shortText(text, HISTORY_IMPORT_MESSAGE_SUMMARY_TEXT_LIMIT) || "",
    payload: historyPayload(sessionId, base.turnId, itemId, { text }),
    sourceId: `claude-history:${sessionId}:${base.turnId}:${itemId}`,
    turnIdOverride: base.turnId,
    createdAt: base.createdAt,
    updatedAt: base.updatedAt,
  };
}

function toolUseEvent(sessionId, entry, part, index, partIndex) {
  const name = stringOrNull(part.name) || "tool";
  const input = isRecord(part.input) ? part.input : {};
  const normalized = normalizeClaudeTool(name, input);
  const base = eventBase(entry, sessionId, index, partIndex);
  const itemId = stringOrNull(part.id) || `${stringOrNull(entry.uuid) || index}:tool:${partIndex}`;
  const payload = {
    toolName: name,
    ...normalized.payload,
  };
  if (normalized.subkind) payload.subkind = normalized.subkind;
  return {
    kind: normalized.kind,
    status: "success",
    title: name,
    summary: normalized.summary || "",
    payload: historyPayload(sessionId, base.turnId, itemId, payload),
    sourceId: `claude-history:${sessionId}:${base.turnId}:${itemId}`,
    turnIdOverride: base.turnId,
    createdAt: base.createdAt,
    updatedAt: base.updatedAt,
  };
}

function toolResultEvent(sessionId, entry, part, index, partIndex) {
  const text = contentText(part.content);
  const base = eventBase(entry, sessionId, index, partIndex);
  const itemId = `${stringOrNull(part.tool_use_id) || stringOrNull(entry.uuid) || index}:result`;
  const isError = part.is_error === true;
  return {
    kind: isError ? "error" : "tool",
    status: isError ? "error" : "success",
    title: "Tool result",
    summary: isError ? shortText(text, HISTORY_IMPORT_ERROR_SUMMARY_TEXT_LIMIT) || "" : "",
    payload: historyPayload(sessionId, base.turnId, itemId, {
      toolUseId: stringOrNull(part.tool_use_id),
      output: text,
      isError,
    }),
    sourceId: `claude-history:${sessionId}:${base.turnId}:${itemId}`,
    turnIdOverride: base.turnId,
    createdAt: base.createdAt,
    updatedAt: base.updatedAt,
  };
}

function systemEvent(sessionId, entry, index) {
  const type = stringOrNull(entry.type);
  if (!type || type === "user" || type === "assistant" || type === "ai-title") return null;
  const subtype = stringOrNull(entry.subtype);
  const knownSystemType = type === "system" ||
    type === "error" ||
    type === "auth_status" ||
    type === "tool_progress" ||
    type === "tool_use_summary";
  if (!knownSystemType && !entry.error) return null;
  const base = eventBase(entry, sessionId, index);
  const itemId = stringOrNull(entry.uuid) || `${type}-${index}`;
  const isError = !!entry.error || type === "error";
  const title = type === "system" && subtype === "init" ? "Claude session" : type;
  const summary = oneLineSummary(
    entry.error || entry.summary || entry.text || entry.status || subtype || entry.operation,
  );
  return {
    kind: isError ? "error" : "turn",
    status: isError ? "error" : normalizeTimelineStatus(entry.status || "info"),
    title,
    summary,
    payload: historyPayload(sessionId, base.turnId, itemId, {
      type,
      subtype,
      operation: stringOrNull(entry.operation),
      status: stringOrNull(entry.status),
      cwd: stringOrNull(entry.cwd),
      model: stringOrNull(entry.model),
      error: stringOrNull(entry.error),
    }),
    sourceId: `claude-history:${sessionId}:${base.turnId}:${itemId}`,
    turnIdOverride: base.turnId,
    createdAt: base.createdAt,
    updatedAt: base.updatedAt,
  };
}

export function claudeHistoryTimelineEvents(sessionId, entries) {
  if (!sessionId || !Array.isArray(entries)) return [];
  const out = [];
  entries.forEach((entry, index) => {
    if (!isRecord(entry)) return;
    const message = messageEvent(sessionId, entry, index);
    if (message) out.push(message);
    readArray(entry.message?.content).forEach((part, partIndex) => {
      if (part.type === "thinking") {
        const event = thinkingEvent(sessionId, entry, part, index, partIndex);
        if (event) out.push(event);
      } else if (part.type === "tool_use") {
        const event = toolUseEvent(sessionId, entry, part, index, partIndex);
        if (event) out.push(event);
      } else if (part.type === "tool_result") {
        const event = toolResultEvent(sessionId, entry, part, index, partIndex);
        if (event) out.push(event);
      }
    });
    const system = systemEvent(sessionId, entry, index);
    if (system) out.push(system);
  });
  return out;
}

export function claudeHistoryTimelineInputs(taskId, sessionId, entries) {
  return claudeHistoryTimelineEvents(sessionId, entries).map((event) => ({
    id: event.sourceId
      ? `${taskId}:${event.turnIdOverride || event.turnId || "history"}:claude-history:${sessionId}:${event.payload?.itemId || event.sourceId}`
      : null,
    taskId,
    turnId: event.turnIdOverride || null,
    backend: "claude",
    kind: event.kind,
    status: event.status,
    title: event.title,
    summary: event.summary || null,
    payload: event.payload || {},
    createdAt: event.createdAt ?? null,
    updatedAt: event.updatedAt ?? null,
  }));
}

function previewMessages(entries, limit = HISTORY_IMPORT_PREVIEW_MESSAGE_LIMIT) {
  const messages = [];
  for (const [index, entry] of entries.entries()) {
    const role = entry?.message?.role;
    if (role !== "user" && role !== "assistant") continue;
    const text = messageText(entry);
    if (!text.trim()) continue;
    messages.push({
      id: stringOrNull(entry.uuid) || `${role}:${index}`,
      role,
      summary: shortText(text, HISTORY_IMPORT_MESSAGE_SUMMARY_TEXT_LIMIT) || null,
    });
  }
  return messages.slice(-limit);
}

export async function searchClaudeSessions(input = {}, { projectsDir = defaultClaudeProjectsDir() } = {}) {
  const limit = limitValue(input.limit);
  const cursor = parseCursor(input.cursor);
  const sessions = (await readSessions(projectsDir))
    .filter((session) => matchesSearch(session, input.searchTerm));
  const page = sessions.slice(cursor, cursor + limit);
  const nextOffset = cursor + page.length;
  return {
    sessions: page.map(publicSession),
    nextCursor: nextOffset < sessions.length ? String(nextOffset) : null,
  };
}

export async function previewClaudeSessionLite(sessionId, { projectsDir = defaultClaudeProjectsDir() } = {}) {
  const session = await findSession(sessionId, projectsDir);
  if (!session) throw new Error(`未找到 Claude session：${sessionId}`);
  const entries = await readJsonl(session.path);
  return {
    session: publicSession(session),
    eventCount: claudeHistoryTimelineEvents(session.id, entries).length,
    messages: previewMessages(entries),
    hasFullPreview: true,
  };
}

export async function previewClaudeSession(sessionId, { projectsDir = defaultClaudeProjectsDir() } = {}) {
  const session = await findSession(sessionId, projectsDir);
  if (!session) throw new Error(`未找到 Claude session：${sessionId}`);
  const entries = await readJsonl(session.path);
  const events = claudeHistoryTimelineInputs("preview", session.id, entries);
  return {
    session: publicSession(session),
    events,
    eventCount: events.length,
  };
}

export async function syncClaudeSessionHistoryForTask(
  { taskId, sessionId, limit = HISTORY_IMPORT_DEFAULT_SYNC_LIMIT, cursor = null },
  { projectsDir = defaultClaudeProjectsDir() } = {},
) {
  const session = await findSession(sessionId, projectsDir);
  if (!session) throw new Error(`未找到 Claude session：${sessionId}`);
  const entries = await readJsonl(session.path);
  const offset = parseCursor(cursor);
  const pageLimit = Math.max(1, Math.min(
    HISTORY_IMPORT_MAX_SYNC_LIMIT,
    Number(limit) || HISTORY_IMPORT_DEFAULT_SYNC_LIMIT,
  ));
  const page = entries.slice(offset, offset + pageLimit);
  const events = claudeHistoryTimelineInputs(taskId, session.id, page);
  const nextOffset = offset + page.length;
  return {
    session: publicSession(session),
    events,
    eventCount: events.length,
    nextCursor: nextOffset < entries.length ? String(nextOffset) : null,
  };
}

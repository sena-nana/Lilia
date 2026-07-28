import * as claudeSdk from "@anthropic-ai/claude-agent-sdk";
import {
  HISTORY_IMPORT_MESSAGE_SUMMARY_TEXT_LIMIT,
  HISTORY_IMPORT_TITLE_TEXT_LIMIT,
} from "@lilia/contracts/historyImportContract.mjs";
import { RUNNER_DONE_EVENT_TYPE } from "@lilia/contracts/runnerProtocolContract.mjs";
import { normalizeSessionManagementRuntimeCommand } from "@lilia/contracts/sessionManagementContract.mjs";
import {
  codexPreviewMessagesFromTurns,
  previewCodexThreadLiteWithServer,
  readCodexThreadTurns,
  renameCodexThreadWithServer,
  archiveCodexThreadWithServer,
  searchCodexThreadsWithServer,
} from "./codex/threadData.mjs";
import { readRunnerRuntimeCommand } from "./runnerCommand.mjs";
import { isRecord, shortText, stringOrNull } from "./utils.mjs";

function cursorOffset(value) {
  const number = Number(stringOrNull(value)?.trim() || 0);
  return Number.isFinite(number) && number > 0 ? Math.trunc(number) : 0;
}

function readSessionManagementRuntimeCommand(cmd) {
  return normalizeSessionManagementRuntimeCommand(
    readRunnerRuntimeCommand(cmd),
  );
}

function emitSessionManagementTimeline(protocol, backend, command, status, config) {
  const failed = status === "error";
  protocol.emitTimeline({
    kind: "diagnostic",
    status,
    title: failed
      ? `${backend} session management failed`
      : `${backend} session management ${command.action}`,
    summary: failed
      ? config.error
      : config.summary || `${command.action} completed`,
    payload: {
      backend,
      source: "lilia",
      subkind: "session_management",
      action: command.action,
      native: config.native === true,
      sessionId: command.sessionId || null,
      threadId: backend === "codex" ? command.sessionId || config.threadId || null : null,
      result: config.result ?? null,
      ...(failed ? { error: config.error } : {}),
    },
    sourceId: `${backend}:session-management:${command.action}:${command.sessionId || "list"}:${Date.now()}`,
  });
}

function emitSessionManagementDone(protocol, command, fallbackSessionId = null) {
  protocol.emit({
    type: RUNNER_DONE_EVENT_TYPE,
    sessionId: command.sessionId || fallbackSessionId || null,
    subtype: "success",
  });
}

function completeUnsupportedSessionManagement(protocol, backend, command, fallbackSessionId, result, summary) {
  emitSessionManagementTimeline(protocol, backend, command, "success", {
    native: false,
    result,
    threadId: fallbackSessionId,
    summary,
  });
  emitSessionManagementDone(protocol, command, fallbackSessionId);
  return true;
}

function normalizeClaudeSessionInfo(info) {
  if (!isRecord(info)) return null;
  const sessionId = stringOrNull(info.sessionId) || stringOrNull(info.session_id);
  if (!sessionId) return null;
  return {
    id: sessionId,
    title: shortText(
      stringOrNull(info.summary) ||
        stringOrNull(info.title) ||
        stringOrNull(info.customTitle) ||
        "Claude session",
      HISTORY_IMPORT_TITLE_TEXT_LIMIT,
    ),
    sourceKind: "claude",
    createdAt: typeof info.createdAt === "number" ? info.createdAt : null,
    updatedAt: typeof info.lastModified === "number" ? info.lastModified : null,
    cwd: stringOrNull(info.cwd) || null,
    gitBranch: stringOrNull(info.gitBranch) || null,
    tag: stringOrNull(info.tag) || null,
  };
}

function contentText(content) {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .map((part) => {
      if (typeof part === "string") return part;
      if (!isRecord(part)) return "";
      if (typeof part.text === "string") return part.text;
      if (typeof part.content === "string") return part.content;
      return "";
    })
    .filter(Boolean)
    .join("\n");
}

function normalizeClaudeSessionMessage(message, index = 0) {
  if (!isRecord(message)) return null;
  const type = stringOrNull(message.type);
  const role = type === "assistant" ? "assistant" : type === "system" ? "system" : "user";
  const payload = isRecord(message.message) ? message.message : {};
  const text = contentText(payload.content ?? message.content ?? message.text);
  return {
    id: stringOrNull(message.uuid) || `${role}:${index}`,
    role,
    summary: shortText(text, HISTORY_IMPORT_MESSAGE_SUMMARY_TEXT_LIMIT) || null,
  };
}

export async function runClaudeSessionManagementRuntimeCommand(cmd, context, workingDir) {
  const command = readSessionManagementRuntimeCommand(cmd);
  if (!command) return false;
  const options = { dir: workingDir };
  const safeContext = isRecord(context) ? context : {};
  const hasContextKey = (key) => Object.prototype.hasOwnProperty.call(safeContext, key);
  const sdk = {
    listSessions: hasContextKey("listClaudeSessions") ? safeContext.listClaudeSessions : claudeSdk.listSessions,
    getSessionInfo: hasContextKey("getClaudeSessionInfo") ? safeContext.getClaudeSessionInfo : claudeSdk.getSessionInfo,
    getSessionMessages: hasContextKey("getClaudeSessionMessages")
      ? safeContext.getClaudeSessionMessages
      : claudeSdk.getSessionMessages,
    renameSession: hasContextKey("renameClaudeSession") ? safeContext.renameClaudeSession : claudeSdk.renameSession,
    tagSession: hasContextKey("tagClaudeSession") ? safeContext.tagClaudeSession : claudeSdk.tagSession,
    deleteSession: hasContextKey("deleteClaudeSession") ? safeContext.deleteClaudeSession : claudeSdk.deleteSession,
  };
  try {
    let result;
    if (command.action === "list") {
      if (typeof sdk.listSessions !== "function") {
        throw new Error("Claude SDK listSessions is not available");
      }
      const sessions = await sdk.listSessions({
        ...options,
        limit: command.limit,
        offset: cursorOffset(command.cursor),
      });
      const normalized = (Array.isArray(sessions) ? sessions : [])
        .map(normalizeClaudeSessionInfo)
        .filter(Boolean);
      const nextOffset = cursorOffset(command.cursor) + normalized.length;
      result = {
        sessions: normalized.filter((session) => {
          const term = command.searchTerm.toLowerCase();
          if (!term) return true;
          return [session.id, session.title, session.cwd, session.gitBranch, session.tag]
            .some((value) => stringOrNull(value)?.toLowerCase().includes(term));
        }),
        nextCursor: normalized.length >= command.limit ? String(nextOffset) : null,
      };
    } else if (command.action === "info") {
      if (typeof sdk.getSessionInfo !== "function") {
        throw new Error("Claude SDK getSessionInfo is not available");
      }
      result = { session: normalizeClaudeSessionInfo(await sdk.getSessionInfo(command.sessionId, options)) };
    } else if (command.action === "messages") {
      if (typeof sdk.getSessionMessages !== "function") {
        throw new Error("Claude SDK getSessionMessages is not available");
      }
      const offset = cursorOffset(command.cursor);
      const messages = await sdk.getSessionMessages(command.sessionId, {
        ...options,
        limit: command.limit,
        offset,
        includeSystemMessages: command.includeSystemMessages,
      });
      const normalized = (Array.isArray(messages) ? messages : [])
        .map(normalizeClaudeSessionMessage)
        .filter(Boolean);
      result = {
        messages: normalized,
        nextCursor: normalized.length >= command.limit ? String(offset + normalized.length) : null,
      };
    } else if (command.action === "rename") {
      if (typeof sdk.renameSession !== "function") {
        throw new Error("Claude SDK renameSession is not available");
      }
      await sdk.renameSession(command.sessionId, command.title, options);
      result = { sessionId: command.sessionId, title: command.title, renamed: true };
    } else if (command.action === "tag") {
      if (typeof sdk.tagSession !== "function") {
        throw new Error("Claude SDK tagSession is not available");
      }
      await sdk.tagSession(command.sessionId, command.tag, options);
      result = { sessionId: command.sessionId, tag: command.tag, tagged: true };
    } else if (command.action === "delete") {
      if (typeof sdk.deleteSession !== "function") {
        throw new Error("Claude SDK deleteSession is not available");
      }
      await sdk.deleteSession(command.sessionId, options);
      result = { sessionId: command.sessionId, deleted: true };
    } else {
      return completeUnsupportedSessionManagement(
        safeContext.protocol,
        "claude",
        command,
        stringOrNull(cmd.resumeSessionId),
        { sessionId: command.sessionId, unsupported: true, reason: "Claude SDK has no archiveSession API." },
        "Claude session archive is unsupported by the SDK",
      );
    }
    emitSessionManagementTimeline(safeContext.protocol, "claude", command, "success", {
      native: true,
      result,
      summary: `Claude session management ${command.action} completed`,
    });
    emitSessionManagementDone(safeContext.protocol, command, stringOrNull(cmd.resumeSessionId));
  } catch (err) {
    emitSessionManagementTimeline(safeContext.protocol, "claude", command, "error", {
      native: false,
      error: err?.message || String(err),
    });
    throw err;
  }
  return true;
}

export async function runCodexSessionManagementRuntimeCommand(
  server,
  threadId,
  cmd,
  ctx,
) {
  const command = readSessionManagementRuntimeCommand(cmd);
  if (!command) return false;
  try {
    let result;
    if (command.action === "list") {
      result = await searchCodexThreadsWithServer(server, {
        limit: command.limit,
        archived: false,
        cursor: command.cursor,
        searchTerm: command.searchTerm,
      });
    } else if (command.action === "info") {
      result = await previewCodexThreadLiteWithServer(server, command.sessionId);
    } else if (command.action === "messages") {
      const { turns, nextCursor } = await readCodexThreadTurns(server, command.sessionId, {
        limit: command.limit,
        cursor: command.cursor,
      });
      result = {
        threadId: command.sessionId,
        messages: codexPreviewMessagesFromTurns(turns, command.limit),
        nextCursor,
      };
    } else if (command.action === "rename") {
      result = await renameCodexThreadWithServer(server, command.sessionId, command.title);
    } else if (command.action === "archive") {
      result = await archiveCodexThreadWithServer(server, command.sessionId, command.archived);
    } else {
      const reason = command.action === "tag"
        ? "Codex app-server thread tags are not available."
        : "Codex app-server thread delete is not available.";
      return completeUnsupportedSessionManagement(
        ctx.protocol,
        "codex",
        command,
        threadId,
        { threadId: command.sessionId, unsupported: true, reason },
        `Codex session management ${command.action} is unsupported`,
      );
    }
    emitSessionManagementTimeline(ctx.protocol, "codex", command, "success", {
      native: true,
      result,
      threadId,
      summary: `Codex session management ${command.action} completed`,
    });
    emitSessionManagementDone(ctx.protocol, command, threadId);
  } catch (err) {
    emitSessionManagementTimeline(ctx.protocol, "codex", command, "error", {
      native: false,
      error: err?.message || String(err),
      threadId,
    });
    throw err;
  }
  return true;
}

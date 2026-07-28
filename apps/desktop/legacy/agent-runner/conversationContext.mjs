import { z } from "zod/v4";
import {
  CONVERSATION_CONTEXT_TOOL_NAME,
  CONVERSATION_CONTEXT_TOOL_NAMES as LILIA_CONVERSATION_CONTEXT_TOOL_NAMES,
  DEFAULT_CONVERSATION_CONTEXT_INCLUDE_MESSAGES,
  MAX_CONVERSATION_CONTEXT_MESSAGES,
  MAX_CONVERSATION_CONTEXT_TEXT,
  QUERY_CONVERSATION_CONTEXT_INPUT_SCHEMA,
  isLiliaConversationContextTool,
} from "@lilia/contracts/conversationContextContract.mjs";
import { isRecord, shortText, stringOrNull } from "./utils.mjs";

export { LILIA_CONVERSATION_CONTEXT_TOOL_NAMES, isLiliaConversationContextTool };

export const queryConversationContextInputSchema = {
  taskId: z.string().optional(),
  includeMessages: z.boolean().optional().default(DEFAULT_CONVERSATION_CONTEXT_INCLUDE_MESSAGES),
};

export const queryConversationContextJsonSchema = QUERY_CONVERSATION_CONTEXT_INPUT_SCHEMA;

export const codexQueryConversationContextDynamicTool = {
  name: CONVERSATION_CONTEXT_TOOL_NAME,
  description:
    "Query Lilia conversation context available to this child conversation, including the parent conversation and readable message summaries.",
  inputSchema: queryConversationContextJsonSchema,
};

export function conversationContextEnabled(conversationContext) {
  return isRecord(conversationContext) &&
    stringOrNull(conversationContext.currentTaskId) &&
    stringOrNull(conversationContext.parentTaskId);
}

export function buildConversationContextToolDescription() {
  return [
    "Query Lilia conversation context available to this child conversation.",
    "Use it when the user asks about prior conversation details or asks you to continue from the parent conversation.",
    "If taskId is omitted, the parent conversation is returned.",
  ].join(" ");
}

function normalizeMessage(message) {
  if (!isRecord(message)) return null;
  const role = stringOrNull(message.role);
  const content = shortText(message.content, MAX_CONVERSATION_CONTEXT_TEXT);
  if (!role || !content) return null;
  return {
    role,
    content,
    createdAt: typeof message.createdAt === "number" ? message.createdAt : null,
  };
}

function normalizeTask(task, includeMessages) {
  if (!isRecord(task)) return null;
  const taskId = stringOrNull(task.taskId);
  if (!taskId) return null;
  const messages = includeMessages && Array.isArray(task.messages)
    ? task.messages.map(normalizeMessage).filter(Boolean).slice(0, MAX_CONVERSATION_CONTEXT_MESSAGES)
    : [];
  return {
    taskId,
    projectId: stringOrNull(task.projectId),
    title: stringOrNull(task.title) || "未命名对话",
    status: stringOrNull(task.status),
    parentId: stringOrNull(task.parentId),
    createdAt: typeof task.createdAt === "number" ? task.createdAt : null,
    messages,
    truncated: task.truncated === true || (Array.isArray(task.messages) && task.messages.length > messages.length),
  };
}

export function createConversationContextHandler(conversationContext) {
  return async function handleQueryConversationContext(input = {}) {
    if (!isRecord(conversationContext)) {
      return {
        ok: false,
        error: "Conversation context is not available for this turn.",
      };
    }
    const includeMessages = input.includeMessages !== false;
    const parentTaskId = stringOrNull(conversationContext.parentTaskId);
    const requestedTaskId = stringOrNull(input.taskId) || parentTaskId;
    const tasks = Array.isArray(conversationContext.tasks) ? conversationContext.tasks : [];
    const task = tasks.find((item) =>
      isRecord(item) && stringOrNull(item.taskId) === requestedTaskId
    );
    if (!task) {
      return {
        ok: false,
        currentTaskId: stringOrNull(conversationContext.currentTaskId),
        parentTaskId,
        requestedTaskId,
        availableTaskIds: tasks
          .map((item) => isRecord(item) ? stringOrNull(item.taskId) : null)
          .filter(Boolean),
        error: "Requested conversation is not in the available context set.",
      };
    }
    return {
      ok: true,
      currentTaskId: stringOrNull(conversationContext.currentTaskId),
      parentTaskId,
      requestedTaskId,
      task: normalizeTask(task, includeMessages),
    };
  };
}

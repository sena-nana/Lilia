/**
 * Todo 服务层：把 Tauri `todo_*` 命令 + TODO_CHANGED_EVENT_NAME 事件包成 typed 函数。
 *
 * Rust 端字段已 `rename_all = "camelCase"`，前端不需要再做 key 映射。
 */

import { invoke } from "../tauri/runtime";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ChatAttachment,
  ChatConversationReference,
  TaskTodo,
  TodoChangedEvent,
  TaskTodoGuideStatus,
  TaskTodoPriority,
} from "@lilia/contracts";
import {
  DEFAULT_TASK_TODO_PRIORITY,
  TODO_APPLY_AGENT_EVENT_COMMAND,
  TODO_CHANGED_EVENT_NAME,
  TODO_CREATE_COMMAND,
  TODO_DELETE_COMMAND,
  TODO_LIST_COMMAND,
  TODO_SUBMIT_GUIDE_COMMAND,
  TODO_UPDATE_COMMAND,
} from "@lilia/contracts";

export type { TaskTodo };
export type { TaskTodoGuideStatus, TaskTodoPriority };

export interface AgentTodoInput {
  content?: string;
  text?: string;
  title?: string;
  description?: string;
  status?: string;
  completed?: boolean;
  done?: boolean;
  priority?: string;
}

export function listTodos(taskId: string): Promise<TaskTodo[]> {
  return invoke<TaskTodo[]>(TODO_LIST_COMMAND, { taskId });
}

export function createTodo(
  taskId: string,
  text: string,
  priority: TaskTodoPriority = DEFAULT_TASK_TODO_PRIORITY,
  attachments: ChatAttachment[] = [],
  conversationReferences: ChatConversationReference[] = [],
): Promise<TaskTodo> {
  return invoke<TaskTodo>(TODO_CREATE_COMMAND, {
    taskId,
    text,
    priority,
    attachments,
    conversationReferences,
  });
}

export function submitGuide(
  taskId: string,
  expectedComposerRevision: number,
  text: string,
  priority: TaskTodoPriority = DEFAULT_TASK_TODO_PRIORITY,
  attachments: ChatAttachment[] = [],
  conversationReferences: ChatConversationReference[] = [],
): Promise<TaskTodo> {
  return invoke<TaskTodo>(TODO_SUBMIT_GUIDE_COMMAND, {
    taskId,
    expectedComposerRevision,
    text,
    priority,
    attachments,
    conversationReferences,
  });
}

export interface TodoPatch {
  text?: string;
  done?: boolean;
  order?: number;
  priority?: TaskTodoPriority;
  guideStatus?: TaskTodoGuideStatus;
}

/**
 * 部分字段更新。未传的字段保持原样；text/done/order 任一传入都会刷新 updatedAt。
 */
export function updateTodo(id: string, patch: TodoPatch): Promise<void> {
  return invoke<void>(TODO_UPDATE_COMMAND, {
    id,
    text: patch.text ?? null,
    done: patch.done ?? null,
    order: patch.order ?? null,
    priority: patch.priority ?? null,
    guideStatus: patch.guideStatus ?? null,
  });
}

export function deleteTodo(id: string): Promise<void> {
  return invoke<void>(TODO_DELETE_COMMAND, { id });
}

export function applyAgentTodoEvent(
  taskId: string,
  todos: AgentTodoInput[],
): Promise<TaskTodo[]> {
  return invoke<TaskTodo[]>(TODO_APPLY_AGENT_EVENT_COMMAND, { taskId, todos });
}

/**
 * 后端在 SDK 的 TodoWrite 工具事件落库后会 emit TODO_CHANGED_EVENT_NAME，
 * 这里订阅以让 TodoFloat 自动 refresh。
 */
export function onTodoChanged(
  handler: (e: TodoChangedEvent) => void,
): Promise<UnlistenFn> {
  return listen<TodoChangedEvent>(TODO_CHANGED_EVENT_NAME, (event) => handler(event.payload));
}


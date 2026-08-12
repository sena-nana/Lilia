import type { ChatAttachment, ChatConversationReference } from "./chat";
import {
  DEFAULT_TASK_TODO_PRIORITY,
  PENDING_TASK_TODO_GUIDE_STATUS,
  QUEUED_TASK_TODO_GUIDE_STATUS,
  SENT_TASK_TODO_GUIDE_STATUS,
  TASK_TODO_GUIDE_STATUSES,
  TASK_TODO_PRIORITIES,
  TASK_TODO_PRIORITY_LABELS,
  TASK_TODO_SOURCES,
  TODO_APPLY_AGENT_EVENT_COMMAND,
  TODO_CHANGED_EVENT_NAME,
  TODO_CREATE_COMMAND,
  TODO_DELETE_COMMAND,
  TODO_LIST_COMMAND,
  TODO_SUBMIT_GUIDE_COMMAND,
  TODO_UPDATE_COMMAND,
} from "./todoContract.mjs";

export type TaskTodoSource = typeof TASK_TODO_SOURCES[number];
export type TaskTodoPriority = typeof TASK_TODO_PRIORITIES[number];
export type TaskTodoGuideStatus = typeof TASK_TODO_GUIDE_STATUSES[number];

export {
  DEFAULT_TASK_TODO_PRIORITY,
  PENDING_TASK_TODO_GUIDE_STATUS,
  QUEUED_TASK_TODO_GUIDE_STATUS,
  SENT_TASK_TODO_GUIDE_STATUS,
  TASK_TODO_GUIDE_STATUSES,
  TASK_TODO_PRIORITIES,
  TASK_TODO_PRIORITY_LABELS,
  TASK_TODO_SOURCES,
  TODO_APPLY_AGENT_EVENT_COMMAND,
  TODO_CHANGED_EVENT_NAME,
  TODO_CREATE_COMMAND,
  TODO_DELETE_COMMAND,
  TODO_LIST_COMMAND,
  TODO_SUBMIT_GUIDE_COMMAND,
  TODO_UPDATE_COMMAND,
};

const TASK_TODO_SOURCE_SET = new Set<string>(TASK_TODO_SOURCES);
const TASK_TODO_PRIORITY_SET = new Set<string>(TASK_TODO_PRIORITIES);
const TASK_TODO_GUIDE_STATUS_SET = new Set<string>(TASK_TODO_GUIDE_STATUSES);

export function isTaskTodoSource(value: unknown): value is TaskTodoSource {
  return typeof value === "string" && TASK_TODO_SOURCE_SET.has(value);
}

export function isTaskTodoPriority(value: unknown): value is TaskTodoPriority {
  return typeof value === "string" && TASK_TODO_PRIORITY_SET.has(value);
}

export function normalizeTaskTodoPriority(
  value: unknown,
  fallback: TaskTodoPriority = DEFAULT_TASK_TODO_PRIORITY,
): TaskTodoPriority {
  return isTaskTodoPriority(value) ? value : fallback;
}

export function isTaskTodoGuideStatus(value: unknown): value is TaskTodoGuideStatus {
  return typeof value === "string" && TASK_TODO_GUIDE_STATUS_SET.has(value);
}

export function normalizeTaskTodoGuideStatus(value: unknown): TaskTodoGuideStatus | null {
  return isTaskTodoGuideStatus(value) ? value : null;
}

export function taskTodoPriorityLabel(priority: TaskTodoPriority): string {
  return TASK_TODO_PRIORITY_LABELS[priority] ?? priority;
}

export interface TaskTodo {
  id: string;
  taskId: string;
  text: string;
  done: boolean;
  order: number;
  source: TaskTodoSource;
  priority: TaskTodoPriority;
  guideStatus: TaskTodoGuideStatus | null;
  attachments?: ChatAttachment[];
  conversationReferences?: ChatConversationReference[];
  createdAt: number;
  updatedAt: number;
}

export interface TodoChangedEvent {
  taskId: string;
}

export function createTodoChangedEvent(taskId: string): TodoChangedEvent {
  return { taskId };
}

import todoContract from "./todo-contract.json" with { type: "json" };

const manifest = Object.freeze(todoContract);

export const TODO_CONTRACT = manifest;
export const TASK_TODO_SOURCES = manifest.taskTodoSources;
export const TASK_TODO_PRIORITIES = manifest.taskTodoPriorities;
export const DEFAULT_TASK_TODO_PRIORITY = manifest.defaultTaskTodoPriority;
export const TASK_TODO_PRIORITY_LABELS = manifest.taskTodoPriorityLabels;
export const TASK_TODO_GUIDE_STATUSES = manifest.taskTodoGuideStatuses;
export const PENDING_TASK_TODO_GUIDE_STATUS = manifest.pendingTaskTodoGuideStatus;
export const QUEUED_TASK_TODO_GUIDE_STATUS = manifest.queuedTaskTodoGuideStatus;
export const SENT_TASK_TODO_GUIDE_STATUS = manifest.sentTaskTodoGuideStatus;
export const TODO_LIST_COMMAND = manifest.commands.list;
export const TODO_CREATE_COMMAND = manifest.commands.create;
export const TODO_SUBMIT_GUIDE_COMMAND = manifest.commands.submitGuide;
export const TODO_UPDATE_COMMAND = manifest.commands.update;
export const TODO_DELETE_COMMAND = manifest.commands.delete;
export const TODO_APPLY_AGENT_EVENT_COMMAND = manifest.commands.applyAgentEvent;
export const TODO_CHANGED_EVENT_NAME = manifest.todoChangedEventName;

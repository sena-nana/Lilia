type AutomationTriggerKind = "manual" | "task_changed" | "timeline_event" | "todo_changed" | "interaction_request";
type AutomationScopeEventKind =
  | "task_created"
  | "task_status_changed"
  | "task_updated"
  | "timeline_event"
  | "todo_changed"
  | "interaction_request";
type AutomationScopeTaskStatus = "waiting" | "running" | "blocked" | "done";
type AutomationLogicKind = "condition" | "switch" | "stop";
type AutomationRunStatus = "pending" | "running" | "succeeded" | "failed" | "skipped" | "cancelled" | "waiting_user";
type AutomationStatusTone = "muted" | "accent" | "ok" | "err";
type AutomationToolAction =
  | "record_timeline"
  | "create_task"
  | "update_task_status"
  | "add_todo"
  | "send_guide";
type AutomationToolConfigField =
  | "taskId"
  | "projectId"
  | "title"
  | "text"
  | "summary"
  | "status"
  | "backend"
  | "priority";
type AutomationToolPriority = "low" | "normal" | "high";

export const AUTOMATION_CONTRACT: Record<string, unknown>;
export const AUTOMATION_TRIGGER_KINDS: readonly AutomationTriggerKind[];
export const DEFAULT_AUTOMATION_TRIGGER_KIND: AutomationTriggerKind;
export const AUTOMATION_SCOPE_EVENT_KINDS: readonly AutomationScopeEventKind[];
export const AUTOMATION_SCOPE_TASK_STATUSES: readonly AutomationScopeTaskStatus[];
export const AUTOMATION_TRIGGER_KIND_LABELS: Readonly<Record<AutomationTriggerKind, string>>;
export const AUTOMATION_LOGIC_KINDS: readonly AutomationLogicKind[];
export const DEFAULT_AUTOMATION_LOGIC_KIND: AutomationLogicKind;
export const DEFAULT_AUTOMATION_LOGIC_PATH: string;
export const AUTOMATION_LOGIC_KIND_LABELS: Readonly<Record<AutomationLogicKind, string>>;
export const DEFAULT_AUTOMATION_AGENT_PROMPT: string;
export const DEFAULT_AUTOMATION_HUMAN_PROMPT: string;
export const AUTOMATION_RUN_STATUSES: readonly AutomationRunStatus[];
export const AUTOMATION_CHANGED_EVENT_NAME: "automation:changed";
export const AUTOMATION_RUN_STARTED_EVENT_NAME: "automation:run-started";
export const AUTOMATION_RUN_UPDATED_EVENT_NAME: "automation:run-updated";
export const AUTOMATION_RUN_FINISHED_EVENT_NAME: "automation:run-finished";
export const AUTOMATION_LIST_WORKFLOWS_COMMAND: "automation_list_workflows";
export const AUTOMATION_SAVE_DRAFT_COMMAND: "automation_save_draft";
export const AUTOMATION_PUBLISH_COMMAND: "automation_publish";
export const AUTOMATION_DELETE_WORKFLOW_COMMAND: "automation_delete_workflow";
export const AUTOMATION_SET_ENABLED_COMMAND: "automation_set_enabled";
export const AUTOMATION_RUN_ONCE_COMMAND: "automation_run_once";
export const AUTOMATION_RESUME_RUN_COMMAND: "automation_resume_run";
export const AUTOMATION_LIST_RUNS_COMMAND: "automation_list_runs";
export const AUTOMATION_GET_RUN_COMMAND: "automation_get_run";
export const AUTOMATION_RUN_EVENT_NAMES: readonly [
  "automation:run-started",
  "automation:run-updated",
  "automation:run-finished",
];
export const DEFAULT_AUTOMATION_RUN_STATUS: AutomationRunStatus;
export const AUTOMATION_WAITING_USER_STATUS: AutomationRunStatus;
export const AUTOMATION_RUN_STATUS_TONES: Readonly<Record<AutomationRunStatus, AutomationStatusTone>>;
export const AUTOMATION_TOOL_ACTIONS: readonly AutomationToolAction[];
export const DEFAULT_AUTOMATION_TOOL_ACTION: AutomationToolAction;
export const AUTOMATION_TOOL_ACTION_LABELS: Readonly<Record<AutomationToolAction, string>>;
export const AUTOMATION_TOOL_ACTION_FIELDS: Readonly<
  Record<AutomationToolAction, readonly AutomationToolConfigField[]>
>;
export const AUTOMATION_TOOL_PRIORITIES: readonly AutomationToolPriority[];
export const DEFAULT_AUTOMATION_TOOL_PRIORITY: AutomationToolPriority;

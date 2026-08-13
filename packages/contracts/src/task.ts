import {
  MEMORY_DELETE_COMMAND,
  MEMORY_GET_INJECTION_STATE_COMMAND,
  MEMORY_GET_SETTINGS_COMMAND,
  MEMORY_LIST_COMMAND,
  MEMORY_RESET_TASK_COOLDOWN_COMMAND,
  MEMORY_SET_ENABLED_COMMAND,
  MEMORY_SET_SETTINGS_COMMAND,
  MEMORY_SET_TASK_ENABLED_COMMAND,
  MEMORY_UPSERT_COMMAND,
} from "./memoryCommandsContract.mjs";
import {
  MILESTONE_CREATE_COMMAND,
  MILESTONE_DELETE_COMMAND,
  MILESTONE_LIST_COMMAND,
  MILESTONE_REORDER_COMMAND,
  MILESTONE_SET_TASKS_COMMAND,
  MILESTONE_UPDATE_COMMAND,
} from "./milestoneCommandsContract.mjs";
import {
  TASK_ARCHIVE_COMMAND,
  TASK_ARCHIVE_PROJECT_COMMAND,
  TASK_CREATE_COMMAND,
  TASK_DELETE_COMMAND,
  TASK_GET_COMMAND,
  TASK_GOAL_CLEAR_COMMAND,
  TASK_GOAL_GET_COMMAND,
  TASK_GOAL_REFRESH_COMMAND,
  TASK_GOAL_SET_COMMAND,
  TASK_HANDOFF_GET_COMMAND,
  TASK_LIST_COMMAND,
  TASK_LIST_SIDEBAR_CONVERSATIONS_COMMAND,
  SEARCH_SESSIONS_COMMAND,
  TASK_PROMOTE_COMMAND,
  TASK_REORDER_COMMAND,
  TASK_REPARENT_COMMAND,
  TASK_TOGGLE_PIN_COMMAND,
  TASK_UPDATE_COMMAND,
  TASK_UPDATE_DEPENDENCIES_COMMAND,
} from "./taskCommandsContract.mjs";
import { TASKS_CHANGED_EVENT_NAME } from "./taskEventsContract.mjs";
import {
  DEFAULT_MEMORY_SCOPE,
  DEFAULT_MEMORY_SETTINGS,
  DEFAULT_MILESTONE_STATUS,
  isMemoryScope as isMemoryScopeImpl,
  isMilestoneStatus as isMilestoneStatusImpl,
  isTaskStatus as isTaskStatusImpl,
  memoryScopeEmptyLabel as memoryScopeEmptyLabelImpl,
  memoryScopeLabel as memoryScopeLabelImpl,
  MEMORY_SCOPE_EMPTY_LABELS,
  MEMORY_SCOPE_TITLES,
  MEMORY_SCOPES,
  MILESTONE_STATUSES,
  MILESTONE_STATUS_LABELS,
  milestoneStatusLabel as milestoneStatusLabelImpl,
  normalizeMemoryScope as normalizeMemoryScopeImpl,
  normalizeMilestoneStatus as normalizeMilestoneStatusImpl,
  TASK_STATUSES,
  TASK_STATUS_LABELS,
  TASK_STATUS_STATE_LABELS,
  taskStatusLabel as taskStatusLabelImpl,
  type MemoryScope as ContractMemoryScope,
  type MilestoneStatus as ContractMilestoneStatus,
  type TaskStatus as ContractTaskStatus,
} from "./taskStatusContract.mjs";

export type TaskStatus = ContractTaskStatus;
export type MilestoneStatus = ContractMilestoneStatus;
export type MemoryScope = ContractMemoryScope;

export {
  DEFAULT_MEMORY_SCOPE,
  DEFAULT_MEMORY_SETTINGS,
  DEFAULT_MILESTONE_STATUS,
  MEMORY_SCOPE_EMPTY_LABELS,
  MEMORY_SCOPE_TITLES,
  MEMORY_SCOPES,
  MILESTONE_STATUSES,
  MILESTONE_STATUS_LABELS,
  TASK_STATUSES,
  TASK_STATUS_LABELS,
  TASK_STATUS_STATE_LABELS,
};

export {
  MEMORY_DELETE_COMMAND,
  MEMORY_GET_INJECTION_STATE_COMMAND,
  MEMORY_GET_SETTINGS_COMMAND,
  MEMORY_LIST_COMMAND,
  MEMORY_RESET_TASK_COOLDOWN_COMMAND,
  MEMORY_SET_ENABLED_COMMAND,
  MEMORY_SET_SETTINGS_COMMAND,
  MEMORY_SET_TASK_ENABLED_COMMAND,
  MEMORY_UPSERT_COMMAND,
  MILESTONE_CREATE_COMMAND,
  MILESTONE_DELETE_COMMAND,
  MILESTONE_LIST_COMMAND,
  MILESTONE_REORDER_COMMAND,
  MILESTONE_SET_TASKS_COMMAND,
  MILESTONE_UPDATE_COMMAND,
  TASKS_CHANGED_EVENT_NAME,
  TASK_ARCHIVE_COMMAND,
  TASK_ARCHIVE_PROJECT_COMMAND,
  TASK_CREATE_COMMAND,
  TASK_DELETE_COMMAND,
  TASK_GET_COMMAND,
  TASK_GOAL_CLEAR_COMMAND,
  TASK_GOAL_GET_COMMAND,
  TASK_GOAL_REFRESH_COMMAND,
  TASK_GOAL_SET_COMMAND,
  TASK_HANDOFF_GET_COMMAND,
  TASK_LIST_COMMAND,
  TASK_LIST_SIDEBAR_CONVERSATIONS_COMMAND,
  SEARCH_SESSIONS_COMMAND,
  TASK_PROMOTE_COMMAND,
  TASK_REORDER_COMMAND,
  TASK_REPARENT_COMMAND,
  TASK_TOGGLE_PIN_COMMAND,
  TASK_UPDATE_COMMAND,
  TASK_UPDATE_DEPENDENCIES_COMMAND,
};

export const isTaskStatus = isTaskStatusImpl as (
  value: unknown,
) => value is TaskStatus;

export const taskStatusLabel = taskStatusLabelImpl as (
  status: TaskStatus,
  variant?: "compact" | "state",
) => string;

export interface Task {
  id: string;
  projectId: string;
  sessionId: string;
  title: string;
  status: TaskStatus;
  createdAt: number;
  pinned: boolean;
  parentId: string | null;
  dependsOn: string[];
}

export interface TasksChangedEvent {
  projectId: string | null;
}

export function createTasksChangedEvent(projectId: string | null | undefined): TasksChangedEvent {
  return { projectId: projectId ?? null };
}

export interface SidebarConversationSummary {
  taskId: string;
  projectId: string | null;
  projectName: string | null;
  title: string;
  status: TaskStatus;
  dependsOn: string[];
  createdAt: number;
  pinned: boolean;
  route: string;
}

export interface TaskGraph {
  tasks: Task[];
  childrenByParent: Record<string, string[]>;
}

export interface MemoryScopeDisplaySpec {
  scope: MemoryScope;
  title: string;
  empty: string;
}

export const MEMORY_SCOPE_DISPLAY_SPECS: readonly MemoryScopeDisplaySpec[] = MEMORY_SCOPES.map((
  scope,
) => ({
  scope,
  title: MEMORY_SCOPE_TITLES[scope],
  empty: MEMORY_SCOPE_EMPTY_LABELS[scope],
}));

export const isMemoryScope = isMemoryScopeImpl as (
  value: unknown,
) => value is MemoryScope;
export const memoryScopeLabel = memoryScopeLabelImpl as (
  scope: MemoryScope,
) => string;
export const memoryScopeEmptyLabel = memoryScopeEmptyLabelImpl as (
  scope: MemoryScope,
) => string;
export const normalizeMemoryScope = normalizeMemoryScopeImpl as (
  value: unknown,
  fallback?: MemoryScope,
) => MemoryScope;

export interface Memory {
  id: string;
  scope: MemoryScope;
  projectId: string | null;
  title: string;
  body: string;
  tags: string[];
  enabled: boolean;
  sourceTaskId: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface MemoryUpsertInput {
  id?: string | null;
  scope: MemoryScope;
  projectId?: string | null;
  title: string;
  body: string;
  tags?: string[];
  enabled?: boolean;
  sourceTaskId?: string | null;
}

export interface MemoryUpsertDraft {
  id?: string | null;
  scope: unknown;
  projectId?: string | null;
  title: string;
  body: string;
  tags?: readonly unknown[] | string | null;
  enabled?: boolean;
  sourceTaskId?: string | null;
}

export interface MemorySettings {
  enabled: boolean;
  baselineInjectionEnabled: boolean;
  cooldownTurns: number;
}

export function normalizeMemoryCooldownTurns(
  value: unknown,
  fallback = DEFAULT_MEMORY_SETTINGS.cooldownTurns,
): number {
  const numberValue = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(numberValue)) return fallback;
  const normalized = Math.trunc(numberValue);
  return normalized > 0 ? normalized : fallback;
}

export function normalizeMemorySettings(
  input: Partial<MemorySettings> | null | undefined,
  base: MemorySettings = DEFAULT_MEMORY_SETTINGS,
): MemorySettings {
  return {
    enabled: typeof input?.enabled === "boolean" ? input.enabled : base.enabled,
    baselineInjectionEnabled: typeof input?.baselineInjectionEnabled === "boolean"
      ? input.baselineInjectionEnabled
      : base.baselineInjectionEnabled,
    cooldownTurns: normalizeMemoryCooldownTurns(input?.cooldownTurns, base.cooldownTurns),
  };
}

export function normalizeMemoryTags(value: readonly unknown[] | string | null | undefined): string[] {
  const rawTags = typeof value === "string" ? value.split(",") : value ?? [];
  const tags = rawTags
    .filter((tag): tag is string => typeof tag === "string")
    .map((tag) => tag.trim())
    .filter(Boolean);
  return [...new Set(tags)];
}

function normalizeOptionalText(value: string | null | undefined): string | null {
  const normalized = value?.trim() ?? "";
  return normalized ? normalized : null;
}

export function createMemoryUpsertInput(draft: MemoryUpsertDraft): MemoryUpsertInput {
  const scope = normalizeMemoryScope(draft.scope);
  return {
    id: normalizeOptionalText(draft.id),
    scope,
    projectId: scope === "project" ? normalizeOptionalText(draft.projectId) : null,
    title: draft.title.trim(),
    body: draft.body.trim(),
    tags: normalizeMemoryTags(draft.tags),
    enabled: draft.enabled !== false,
    sourceTaskId: normalizeOptionalText(draft.sourceTaskId),
  };
}

export interface MemoryInjectionState {
  taskId: string;
  enabled: boolean;
  lastInjectedTurnSeq: number | null;
  updatedAt: number;
}

export const isMilestoneStatus = isMilestoneStatusImpl as (
  value: unknown,
) => value is MilestoneStatus;
export const milestoneStatusLabel = milestoneStatusLabelImpl as (
  status: MilestoneStatus,
) => string;
export const normalizeMilestoneStatus = normalizeMilestoneStatusImpl as (
  value: unknown,
  fallback?: MilestoneStatus,
) => MilestoneStatus;

export interface Milestone {
  id: string;
  projectId: string;
  title: string;
  description: string;
  status: MilestoneStatus;
  dueDate: number | null;
  order: number;
  createdAt: number;
}

export interface MilestoneUpdatePatch {
  title?: string;
  description?: string;
  status?: MilestoneStatus;
  dueDate?: number | null;
}

export interface TaskMilestoneLink {
  taskId: string;
  milestoneId: string;
}

export interface ProjectRoadmap {
  milestones: Milestone[];
  links: TaskMilestoneLink[];
}

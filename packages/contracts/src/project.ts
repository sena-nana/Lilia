import type { CodexComposerSettings, NativeComposerSettings } from "./provider";
import {
  GITHUB_CLONE_REPO_COMMAND,
  GITHUB_GET_BINDING_STATUS_COMMAND,
  GITHUB_LIST_REPOS_COMMAND,
  GITHUB_POLL_DEVICE_FLOW_COMMAND,
  GITHUB_START_DEVICE_FLOW_COMMAND,
  GITHUB_UNBIND_COMMAND,
  GIT_CLONE_REPO_COMMAND,
} from "./githubCommandsContract.mjs";
import {
  PROJECT_CREATE_COMMAND,
  PROJECT_DASHBOARD_LIST_COMMAND,
  PROJECT_ENSURE_FOLDERS_COMMAND,
  PROJECT_GET_COMMAND,
  PROJECT_GET_SETTINGS_COMMAND,
  PROJECT_LIST_COMMAND,
  PROJECT_REMOVE_COMMAND,
  PROJECT_RENAME_COMMAND,
  PROJECT_REORDER_COMMAND,
  PROJECT_SET_SETTINGS_COMMAND,
  PROJECT_TOGGLE_PIN_COMMAND,
  WORKTREE_ATTACH_TASK_COMMAND,
  WORKTREE_CLEANUP_ARCHIVE_COMMAND,
  WORKTREE_CLEAR_TASK_COMMAND,
  WORKTREE_CREATE_FOR_TASK_COMMAND,
  WORKTREE_GET_FOR_TASK_COMMAND,
  WORKTREE_LIST_COMMAND,
  WORKTREE_MERGE_DELETE_ARCHIVE_COMMAND,
} from "./projectCommandsContract.mjs";
import {
  SYSTEM_OPEN_IN_VSCODE_COMMAND,
  SYSTEM_OPEN_PATH_COMMAND,
  SYSTEM_OPEN_URL_COMMAND,
} from "./systemCommandsContract.mjs";
import { TASK_STATUSES, isTaskStatus, type TaskStatus } from "./task";
import {
  PROJECT_DASHBOARD_ACTIVE_STATUSES,
  PROJECT_DASHBOARD_BLOCKED_STATUSES,
  PROJECT_DASHBOARD_STATUS_ORDER,
  PROJECT_ROADMAP_STATUS_ORDER,
} from "./taskStatusContract.mjs";

export {
  GITHUB_CLONE_REPO_COMMAND,
  GITHUB_GET_BINDING_STATUS_COMMAND,
  GITHUB_LIST_REPOS_COMMAND,
  GITHUB_POLL_DEVICE_FLOW_COMMAND,
  GITHUB_START_DEVICE_FLOW_COMMAND,
  GITHUB_UNBIND_COMMAND,
  GIT_CLONE_REPO_COMMAND,
  PROJECT_CREATE_COMMAND,
  PROJECT_DASHBOARD_ACTIVE_STATUSES,
  PROJECT_DASHBOARD_BLOCKED_STATUSES,
  PROJECT_DASHBOARD_LIST_COMMAND,
  PROJECT_ENSURE_FOLDERS_COMMAND,
  PROJECT_GET_COMMAND,
  PROJECT_GET_SETTINGS_COMMAND,
  PROJECT_LIST_COMMAND,
  PROJECT_REMOVE_COMMAND,
  PROJECT_RENAME_COMMAND,
  PROJECT_REORDER_COMMAND,
  PROJECT_DASHBOARD_STATUS_ORDER,
  PROJECT_ROADMAP_STATUS_ORDER,
  PROJECT_SET_SETTINGS_COMMAND,
  PROJECT_TOGGLE_PIN_COMMAND,
  WORKTREE_ATTACH_TASK_COMMAND,
  WORKTREE_CLEANUP_ARCHIVE_COMMAND,
  WORKTREE_CLEAR_TASK_COMMAND,
  WORKTREE_CREATE_FOR_TASK_COMMAND,
  WORKTREE_GET_FOR_TASK_COMMAND,
  WORKTREE_LIST_COMMAND,
  WORKTREE_MERGE_DELETE_ARCHIVE_COMMAND,
  SYSTEM_OPEN_IN_VSCODE_COMMAND,
  SYSTEM_OPEN_PATH_COMMAND,
  SYSTEM_OPEN_URL_COMMAND,
};

export interface Project {
  id: string;
  name: string;
  cwd: string | null;
  sessionCount: number;
  pinned: boolean;
}

export type ProjectTaskStatusCounts = Record<TaskStatus, number>;

export function createEmptyProjectTaskStatusCounts(): ProjectTaskStatusCounts {
  return Object.fromEntries(
    TASK_STATUSES.map((status) => [status, 0]),
  ) as ProjectTaskStatusCounts;
}

export function incrementProjectTaskStatusCount(
  counts: ProjectTaskStatusCounts,
  status: unknown,
): boolean {
  if (!isTaskStatus(status)) return false;
  counts[status] += 1;
  return true;
}

export function countProjectTaskStatuses(
  tasks: readonly { status: unknown }[],
): ProjectTaskStatusCounts {
  const counts = createEmptyProjectTaskStatusCounts();
  for (const task of tasks) incrementProjectTaskStatusCount(counts, task.status);
  return counts;
}

export function sumProjectTaskStatuses(
  counts: ProjectTaskStatusCounts,
  statuses: readonly TaskStatus[],
): number {
  return statuses.reduce((total, status) => total + counts[status], 0);
}

export function deriveProjectDashboardCounts(
  counts: ProjectTaskStatusCounts,
): Pick<ProjectDashboardSummary, "activeCount" | "blockedCount"> {
  return {
    activeCount: sumProjectTaskStatuses(counts, PROJECT_DASHBOARD_ACTIVE_STATUSES),
    blockedCount: sumProjectTaskStatuses(counts, PROJECT_DASHBOARD_BLOCKED_STATUSES),
  };
}

export interface ProjectDashboardSummary {
  id: string;
  name: string;
  cwd: string | null;
  pinned: boolean;
  taskCount: number;
  sessionCount: number;
  statusCounts: ProjectTaskStatusCounts;
  blockedCount: number;
  activeCount: number;
  recentActivityAt: number | null;
  totalTokens: number;
  knownCostUsd: number | null;
  costRecordCount: number;
  usageRecordCount: number;
}

export type SessionKind = "interactive" | "headless" | "unknown";

export interface Session {
  sessionId: string;
  projectId: string;
  cwd: string;
  startedAt: number;
  kind: SessionKind;
  alive: boolean;
}

export type GitHubClientIdSource = "none" | "bundled" | "custom";

export interface GitHubBindingMetadata {
  login: string;
  avatarUrl: string | null;
  boundAt: number;
  scopes: string[];
  clientIdSource: Exclude<GitHubClientIdSource, "none">;
}

export interface GitHubBindingStatus {
  state: "unbound" | "bound";
  clientIdConfigured: boolean;
  clientIdSource: GitHubClientIdSource;
  binding: GitHubBindingMetadata | null;
}

export interface GitHubDeviceFlowStart {
  deviceCode: string;
  userCode: string;
  verificationUri: string;
  expiresAt: number;
  intervalSeconds: number;
}

export interface GitHubDeviceFlowPollResult {
  status: "pending" | "authorized" | "expired";
  intervalSeconds: number;
  bindingStatus: GitHubBindingStatus | null;
  error: string | null;
}

export interface GitHubRepoSummary {
  id: number;
  name: string;
  fullName: string;
  ownerLogin: string;
  private: boolean;
  description: string | null;
  defaultBranch: string | null;
  updatedAt: string;
  cloneUrl: string;
  htmlUrl: string;
}

export interface GitHubRepoPage {
  items: GitHubRepoSummary[];
  nextPage: number | null;
}

export type WorktreeSelectionMode = "current" | "create" | "existing";

export type TaskWorktreeStatus = "active" | "merged" | "removed";

export interface WorktreeSettings {
  defaultMode: WorktreeSelectionMode;
  parentDir: string | null;
  autoInstructions: string;
  cleanupOnArchive: boolean;
}

export interface TaskWorktree {
  taskId: string;
  projectId: string | null;
  baseRepoPath: string;
  worktreePath: string;
  branchName: string;
  baseBranch: string;
  status: TaskWorktreeStatus;
  createdAt: number;
  updatedAt: number;
}

export interface WorktreeListItem {
  path: string;
  head: string | null;
  branch: string | null;
  bare: boolean;
  detached: boolean;
  prunable: boolean;
  locked: boolean;
  isMain: boolean;
  isTaskBound?: boolean;
}

export interface WorktreeCreateInput {
  taskId: string;
  projectId?: string | null;
  baseRepoPath: string;
  parentDir?: string | null;
}

export interface WorktreeAttachInput {
  taskId: string;
  projectId?: string | null;
  baseRepoPath: string;
  worktreePath: string;
}

export interface WorktreeMergeResult {
  merged: boolean;
  removed: boolean;
  archived: boolean;
  message: string;
}

export interface ProjectSettings {
  cloneParentDir: string | null;
  /** Native AgentKit project defaults (Mutsuki path). */
  nativeDefaults?: NativeComposerSettings | null;
  /** @deprecated Use nativeDefaults. */
  codexDefaults?: CodexComposerSettings | null;
  githubBinding?: GitHubBindingMetadata | null;
  worktree?: WorktreeSettings | null;
}

export interface PopupWindowSettings {
  shortcut: string | null;
}

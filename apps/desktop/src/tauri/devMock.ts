import {
  CHAT_BACKENDS,
  APP_RESTART_COMMAND,
  AGENT_TIMELINE_LIST_COMMAND,
  AGENT_DEBUG_LOGS_COMMAND,
  AGENT_DEBUG_RECORD_ACTION_COMMAND,
  AGENT_DEBUG_RESET_STATE_COMMAND,
  AGENT_DEBUG_RUNTIME_SNAPSHOT_COMMAND,
  AGENT_DEBUG_STATUS_COMMAND,
  CHAT_ACK_RESTORED_ROLLBACK_COMMAND,
  CHAT_CHECK_ENV_COMMAND,
  CHAT_DESCRIBE_ATTACHMENTS_COMMAND,
  CHAT_GET_COMPOSER_STATE_COMMAND,
  CHAT_GET_RUNTIME_SNAPSHOT_COMMAND,
  CHAT_INTERRUPT_TURN_COMMAND,
  CHAT_LIST_MODELS_COMMAND,
  CHAT_READ_CLIPBOARD_FILE_PATHS_COMMAND,
  CHAT_RESPOND_AGENT_INTERACTION_COMMAND,
  CHAT_RESPOND_TITLE_UPDATE_COMMAND,
  CHAT_SEARCH_CONTEXT_ATTACHMENTS_COMMAND,
  CHAT_SEARCH_SLASH_COMMANDS_COMMAND,
  CHAT_SEND_MESSAGE_COMMAND,
  CHAT_SET_COMPOSER_STATE_COMMAND,
  CLI_PROJECT_OPEN_CONSUME_PENDING_COMMAND,
  DEFAULT_MODEL_BY_BACKEND,
  MODEL_OPTIONS_BY_BACKEND,
  DEFAULT_MEMORY_SETTINGS,
  ASSISTANT_AI_FETCH_MODELS_COMMAND,
  ASSISTANT_AI_GET_CONFIG_COMMAND,
  ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND,
  ASSISTANT_AI_SET_CONFIG_COMMAND,
  ASSISTANT_AI_TEST_CONNECTION_COMMAND,
  AUTOMATION_DELETE_WORKFLOW_COMMAND,
  AUTOMATION_LIST_RUNS_COMMAND,
  AUTOMATION_LIST_WORKFLOWS_COMMAND,
  AGENT_INTERACTION_DELETE_SUBAGENT_COMMAND,
  AGENT_INTERACTION_GET_SETTINGS_COMMAND,
  AGENT_INTERACTION_LIST_SUBAGENTS_COMMAND,
  AGENT_INTERACTION_SET_SETTINGS_COMMAND,
  AGENT_INTERACTION_UPSERT_SUBAGENT_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_SETTINGS_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND,
  CONVERSATION_SUGGESTIONS_SET_SETTINGS_COMMAND,
  MODEL_FEATURE_GET_SETTINGS_COMMAND,
  MODEL_FEATURE_LIST_MODEL_OPTIONS_COMMAND,
  MODEL_FEATURE_SET_SETTINGS_COMMAND,
  GIT_CLONE_REPO_COMMAND,
  GITHUB_CLONE_REPO_COMMAND,
  GITHUB_GET_BINDING_STATUS_COMMAND,
  GITHUB_LIST_REPOS_COMMAND,
  GITHUB_POLL_DEVICE_FLOW_COMMAND,
  GITHUB_START_DEVICE_FLOW_COMMAND,
  GITHUB_UNBIND_COMMAND,
  MILESTONE_CREATE_COMMAND,
  MILESTONE_DELETE_COMMAND,
  MILESTONE_LIST_COMMAND,
  MILESTONE_REORDER_COMMAND,
  MILESTONE_SET_TASKS_COMMAND,
  MILESTONE_UPDATE_COMMAND,
  MEMORY_DELETE_COMMAND,
  MEMORY_GET_INJECTION_STATE_COMMAND,
  MEMORY_GET_SETTINGS_COMMAND,
  MEMORY_LIST_COMMAND,
  MEMORY_RESET_TASK_COOLDOWN_COMMAND,
  MEMORY_SET_ENABLED_COMMAND,
  MEMORY_SET_SETTINGS_COMMAND,
  MEMORY_SET_TASK_ENABLED_COMMAND,
  MEMORY_UPSERT_COMMAND,
  PROJECT_ARCHITECTURE_APPLY_COMMAND,
  PROJECT_ARCHITECTURE_GET_COMMAND,
  PROJECT_ARCHITECTURE_LIST_CHANGES_COMMAND,
  PROJECT_ARCHITECTURE_REJECT_COMMAND,
  PROJECT_ARCHITECTURE_ROLLBACK_COMMAND,
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
  PLUGINS_CREATE_HOOK_SOURCE_COMMAND,
  PLUGINS_DELETE_HOOK_SOURCE_COMMAND,
  PLUGINS_DELETE_MCP_SERVER_COMMAND,
  PLUGINS_DELETE_SKILL_COMMAND,
  PLUGINS_HOOKS_OVERVIEW_COMMAND,
  PLUGINS_OPEN_HOOK_CONFIG_COMMAND,
  PLUGINS_OPEN_MCP_CONFIG_COMMAND,
  PLUGINS_OVERVIEW_COMMAND,
  PLUGINS_READ_HOOK_SOURCE_COMMAND,
  PLUGINS_SET_HOOK_SOURCE_ENABLED_COMMAND,
  PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND,
  PLUGINS_SET_PACKAGE_ENABLED_COMMAND,
  PLUGINS_SET_SKILL_ENABLED_COMMAND,
  PLUGINS_UPDATE_HOOK_SOURCE_COMMAND,
  PROVIDER_GET_ACTIVE_BACKEND_COMMAND,
  PROVIDER_GET_CONFIG_COMMAND,
  PROVIDER_SET_ACTIVE_BACKEND_COMMAND,
  PROVIDER_SET_CONFIG_COMMAND,
  POPUP_FOCUS_MAIN_COMMAND,
  POPUP_GET_WINDOW_SETTINGS_COMMAND,
  POPUP_OPEN_CHILD_QUESTION_COMMAND,
  POPUP_OPEN_NEW_CHAT_COMMAND,
  POPUP_OPEN_TASK_COMMAND,
  POPUP_REMEMBER_LAST_PROJECT_COMMAND,
  POPUP_SET_WINDOW_SETTINGS_COMMAND,
  QUOTA_USAGE_GET_STATS_COMMAND,
  REMOTE_CONTROL_CANCEL_PAIRING_COMMAND,
  REMOTE_CONTROL_PAIR_DEVICE_COMMAND,
  REMOTE_CONTROL_REVOKE_DEVICE_COMMAND,
  REMOTE_CONTROL_SET_KEEP_AWAKE_ENABLED_COMMAND,
  REMOTE_CONTROL_SET_HOST_ENABLED_COMMAND,
  REMOTE_CONTROL_SET_PC_NAME_COMMAND,
  REMOTE_CONTROL_START_PAIRING_COMMAND,
  REMOTE_CONTROL_STATUS_COMMAND,
  ROUTER_GET_MODE_COMMAND,
  ROUTER_SET_MODE_COMMAND,
  LILIA_IAB_OPEN_COMMAND,
  LILIA_IAB_SUBMIT_COMMAND,
  SYSTEM_OPEN_IN_VSCODE_COMMAND,
  SYSTEM_OPEN_PATH_COMMAND,
  SYSTEM_OPEN_URL_COMMAND,
  TASK_ARCHIVE_COMMAND,
  TASK_ARCHIVE_PROJECT_COMMAND,
  TASK_GET_COMMAND,
  TASK_HANDOFF_GET_COMMAND,
  TASK_LIST_COMMAND,
  TASK_LIST_SIDEBAR_CONVERSATIONS_COMMAND,
  TASK_PROMOTE_COMMAND,
  TASK_REORDER_COMMAND,
  TASK_REPARENT_COMMAND,
  TASK_TOGGLE_PIN_COMMAND,
  TODO_LIST_COMMAND,
  WORKTREE_ATTACH_TASK_COMMAND,
  WORKTREE_CLEANUP_ARCHIVE_COMMAND,
  WORKTREE_CLEAR_TASK_COMMAND,
  WORKTREE_CREATE_FOR_TASK_COMMAND,
  WORKTREE_GET_FOR_TASK_COMMAND,
  WORKTREE_LIST_COMMAND,
  WORKTREE_MERGE_DELETE_ARCHIVE_COMMAND,
  countProjectTaskStatuses,
  createChatBackendRecord,
  deriveProjectDashboardCounts,
  DEFAULT_QUOTA_USAGE_STATS_DAYS,
  defaultRouterModeForBackend,
  isQuotaUsageStatsBackendFilter,
  isQuotaUsageStatsDays,
  createMemoryUpsertInput,
  normalizeAgentInteractionSettings,
  normalizeMemorySettings,
  normalizePermissionMode,
  type AgentInteractionSettings,
  type BackendEnvStatus,
  type ChatBackendKind,
  type Memory,
  type MemorySettings,
  type ProductCommandMeta,
  type ProductCommandResult,
  type ProductEntity,
  type ProductEntityKind,
  type ProductEvent,
  type QuotaUsageStatsBackendFilter,
  type QuotaUsageStatsDays,
  type RouterMode,
  type Task,
} from "@lilia/contracts";
import {
  PRODUCT_CREATE_ENTITY_COMMAND,
  PRODUCT_EVENT_NAME,
  PRODUCT_GET_ENTITY_COMMAND,
  PRODUCT_LIST_ENTITIES_COMMAND,
  PRODUCT_LIST_EVENTS_COMMAND,
  PRODUCT_UPDATE_ENTITY_COMMAND,
} from "@lilia/contracts/productCoreContract.mjs";
import { TAURI_PLUGIN_DIALOG_OPEN_COMMAND } from "./pluginCommands";

type Args = Record<string, unknown>;
type UnlistenFn = () => void;

const SERIALIZE_TO_IPC_FN = "__TAURI_TO_IPC_KEY__";
let mockChannelId = 0;
const now = 1_720_000_000_000;
const dayMs = 86_400_000;

interface DevProject {
  id: string;
  name: string;
  cwd: string | null;
  sessionCount: number;
  sortOrder: number;
  pinned: boolean;
}

interface DevTask {
  id: string;
  projectId: string | null;
  sessionId: string;
  title: string;
  titleSource: "auto" | "manual";
  status: Task["status"];
  createdAt: number;
  parentId: string | null;
  dependsOn: string[];
  sortOrder: number;
  pinned: boolean;
  archived: boolean;
}

const projects: DevProject[] = [
  {
    id: "lilia",
    name: "Lilia",
    cwd: "C:\\Files\\workspace\\Lilia",
    sessionCount: 2,
    sortOrder: 0,
    pinned: true,
  },
  {
    id: "demo",
    name: "Demo Workspace",
    cwd: "C:\\Files\\workspace\\Demo",
    sessionCount: 1,
    sortOrder: 1,
    pinned: false,
  },
];

const tasks: DevTask[] = [
  {
    id: "t-001",
    projectId: "lilia",
    sessionId: "mock-session-001",
    title: "浏览开发期 mock 页面",
    titleSource: "manual",
    status: "running",
    createdAt: now - 86_400_000,
    parentId: null,
    dependsOn: [],
    sortOrder: 0,
    pinned: true,
    archived: false,
  },
  {
    id: "o-001",
    projectId: null,
    sessionId: "mock-inbox-001",
    title: "收集箱 mock 对话",
    titleSource: "auto",
    status: "waiting",
    createdAt: now - 21_600_000,
    parentId: null,
    dependsOn: [],
    sortOrder: 0,
    pinned: false,
    archived: false,
  },
];

const productProjectRevisions = new Map(projects.map((project) => [project.id, 1]));
const productTaskRevisions = new Map(tasks.map((task) => [task.id, 1]));
const productConversations = new Map<
  string,
  Extract<ProductEntity, { kind: "conversation" }>["value"]
>(tasks.map((task) => [
  task.id,
  {
    id: task.id,
    projectId: task.projectId,
    taskId: task.id,
    title: task.title,
    status: "active" as const,
    archived: task.archived,
    labels: [],
    bindingIds: [],
    forkedFrom: null,
    migratedFrom: null,
    legacySource: null,
    timelineCursor: 0,
    createdAt: task.createdAt,
    updatedAt: task.createdAt,
    revision: 1,
  },
]));
const productExtraEntities = new Map<string, ProductEntity>();
const productCommandResults = new Map<string, ProductCommandResult<ProductEntity>>();
const productEvents: ProductEvent[] = [];
const devEventListeners = new Map<string, Set<(event: { payload: unknown }) => void>>();

let taskWorktrees: Record<string, any> = {};
let agentDebugLogs: Record<string, unknown>[] = [];

const defaultWorktreeSettings = {
  defaultMode: "current",
  parentDir: null,
  autoInstructions: [
    "This task is running inside a dedicated git worktree managed by Lilia.",
    "Keep changes scoped to this task and create commits in the worktree before requesting merge/archive.",
  ].join("\n"),
  cleanupOnArchive: true,
};

function projectDashboardRows() {
  return projects.map((project) => {
    const projectTasks = tasks.filter((task) => task.projectId === project.id);
    const statusCounts = countProjectTaskStatuses(projectTasks);
    const dashboardCounts = deriveProjectDashboardCounts(statusCounts);
    const usage = project.id === "lilia"
      ? { totalTokens: 12_400, knownCostUsd: 0.084, costRecordCount: 1, usageRecordCount: 2 }
      : { totalTokens: 0, knownCostUsd: null, costRecordCount: 0, usageRecordCount: 0 };
    return {
      id: project.id,
      name: project.name,
      cwd: project.cwd,
      pinned: project.pinned,
      taskCount: projectTasks.length,
      sessionCount: new Set(projectTasks.map((task) => task.sessionId)).size,
      statusCounts,
      ...dashboardCounts,
      recentActivityAt: projectTasks.reduce<number | null>(
        (latest, task) => latest === null ? task.createdAt : Math.max(latest, task.createdAt),
        null,
      ),
      ...usage,
    };
  });
}

const emptyLists = new Set<string>([
  AGENT_TIMELINE_LIST_COMMAND,
  AUTOMATION_LIST_RUNS_COMMAND,
  AUTOMATION_LIST_WORKFLOWS_COMMAND,
  CHAT_DESCRIBE_ATTACHMENTS_COMMAND,
  CHAT_READ_CLIPBOARD_FILE_PATHS_COMMAND,
  CHAT_SEARCH_CONTEXT_ATTACHMENTS_COMMAND,
  CONVERSATION_SUGGESTIONS_GET_COMMAND,
  PROJECT_ARCHITECTURE_LIST_CHANGES_COMMAND,
  TODO_LIST_COMMAND,
]);

const noops = new Set<string>([
  ASSISTANT_AI_SET_CONFIG_COMMAND,
  AUTOMATION_DELETE_WORKFLOW_COMMAND,
  CHAT_ACK_RESTORED_ROLLBACK_COMMAND,
  CHAT_RESPOND_AGENT_INTERACTION_COMMAND,
  CHAT_RESPOND_TITLE_UPDATE_COMMAND,
  CHAT_SET_COMPOSER_STATE_COMMAND,
  CONVERSATION_SUGGESTIONS_SET_SETTINGS_COMMAND,
  GITHUB_UNBIND_COMMAND,
  LILIA_IAB_OPEN_COMMAND,
  MILESTONE_DELETE_COMMAND,
  MILESTONE_REORDER_COMMAND,
  MILESTONE_SET_TASKS_COMMAND,
  MILESTONE_UPDATE_COMMAND,
  PLUGINS_DELETE_MCP_SERVER_COMMAND,
  PLUGINS_DELETE_SKILL_COMMAND,
  PLUGINS_OPEN_MCP_CONFIG_COMMAND,
  PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND,
  PLUGINS_SET_PACKAGE_ENABLED_COMMAND,
  PLUGINS_SET_SKILL_ENABLED_COMMAND,
  POPUP_FOCUS_MAIN_COMMAND,
  POPUP_OPEN_CHILD_QUESTION_COMMAND,
  POPUP_OPEN_NEW_CHAT_COMMAND,
  POPUP_OPEN_TASK_COMMAND,
  POPUP_REMEMBER_LAST_PROJECT_COMMAND,
  POPUP_SET_WINDOW_SETTINGS_COMMAND,
  PROJECT_REORDER_COMMAND,
  PROJECT_SET_SETTINGS_COMMAND,
  PROVIDER_SET_ACTIVE_BACKEND_COMMAND,
  PROVIDER_SET_CONFIG_COMMAND,
  ROUTER_SET_MODE_COMMAND,
  SYSTEM_OPEN_IN_VSCODE_COMMAND,
  SYSTEM_OPEN_PATH_COMMAND,
  SYSTEM_OPEN_URL_COMMAND,
  TASK_REORDER_COMMAND,
  TASK_REPARENT_COMMAND,
]);

let agentInteractionSubagents = [{
  id: "reviewer",
  name: "Reviewer",
  description: "检查风险与回归",
  instruction: "Review code changes, identify risk, and summarize findings.",
  enabled: true,
}];

let agentInteractionSettings = normalizeAgentInteractionSettings(null);
let remoteControlEnabled = false;
let remoteControlKeepAwakeEnabled = true;
let remoteControlTicket: Record<string, unknown> | null = null;
let remoteControlDevices: Record<string, unknown>[] = [];
const remoteControlBridgeUrl = "http://127.0.0.1:41478";

function remoteControlStatus() {
  return {
    hostEnabled: remoteControlEnabled,
    state: remoteControlEnabled ? (remoteControlTicket ? "pairing" : "listening") : "disabled",
    pcName: "Lilia Dev PC",
    keepAwakeEnabled: remoteControlKeepAwakeEnabled,
    endpoint: remoteControlEnabled
      ? { endpointId: "mock-pc-endpoint", relayUrl: null, directAddresses: [] }
      : null,
    activeTicket: remoteControlTicket,
    trustedDevices: remoteControlDevices,
    capabilities: {
      protocolVersion: 1,
      minProtocolVersion: 1,
      alpn: "lilia.remote-control.v1",
      supportsPairing: true,
      supportsTaskInbox: true,
      supportsTimelineSubscription: true,
      supportsChatSend: true,
      supportsInteractionResponse: true,
      supportsInterrupt: true,
    },
  };
}

let memories: Memory[] = [
  {
    id: "memory-user-1",
    scope: "user",
    projectId: null,
    title: "PR 文案",
    body: "PR 描述里不要出现 emoji。",
    tags: ["preference"],
    enabled: true,
    sourceTaskId: null,
    createdAt: now - 3_600_000,
    updatedAt: now - 3_600_000,
  },
  {
    id: "memory-project-1",
    scope: "project",
    projectId: "lilia",
    title: "迁移检查",
    body: "涉及数据库迁移时先验证 dry-run 或最小 schema 测试。",
    tags: ["database"],
    enabled: true,
    sourceTaskId: "t-001",
    createdAt: now - 1_800_000,
    updatedAt: now - 1_800_000,
  },
];

let memorySettings: MemorySettings = { ...DEFAULT_MEMORY_SETTINGS };

const providerBackends = CHAT_BACKENDS as readonly ChatBackendKind[];

function clone<T>(value: T): T {
  return structuredClone(value);
}

function productEntityId(entity: ProductEntity): string {
  return entity.kind === "binding" ? entity.value.bindingId : entity.value.id;
}

function productProjectEntity(
  project: (typeof projects)[number],
): Extract<ProductEntity, { kind: "project" }> {
  return {
    kind: "project",
    value: {
      id: project.id,
      name: project.name,
      workspacePath: project.cwd,
      pinned: project.pinned,
      sortOrder: project.sortOrder,
      archive: "active",
      gitWorkspace: null,
      settings: {
        defaultAgentProfileId: null,
        values: {},
      },
      assetIds: [],
      revision: productProjectRevisions.get(project.id) ?? 1,
    },
  };
}

function productTaskEntity(
  task: (typeof tasks)[number],
): Extract<ProductEntity, { kind: "task" }> {
  return {
    kind: "task",
    value: {
      id: task.id,
      projectId: task.projectId,
      title: task.title,
      description: null,
      status: task.status as Extract<ProductEntity, { kind: "task" }>["value"]["status"],
      priority: "normal",
      assignmentId: null,
      completionCriteria: [],
      milestoneId: null,
      workflowId: null,
      agentProfileId: null,
      blockedReason: null,
      dependsOn: [...task.dependsOn],
      parentId: task.parentId,
      pinned: task.pinned,
      sortOrder: task.sortOrder,
      archived: task.archived,
      tags: [],
      createdAt: task.createdAt,
      updatedAt: task.createdAt,
      revision: productTaskRevisions.get(task.id) ?? 1,
      legacySource: null,
    },
  };
}

function listDevProductEntities(kind: ProductEntityKind): ProductEntity[] {
  if (kind === "project") return projects.map(productProjectEntity);
  if (kind === "task") return tasks.map(productTaskEntity);
  if (kind === "conversation") {
    return [...productConversations.values()].map((value) => ({
      kind: "conversation",
      value: clone(value),
    }));
  }
  return [...productExtraEntities.values()]
    .filter((entity) => entity.kind === kind)
    .map(clone);
}

function getDevProductEntity(kind: ProductEntityKind, id: string): ProductEntity | null {
  return listDevProductEntities(kind)
    .find((entity) => productEntityId(entity) === id) ?? null;
}

function emitDevEvent<T>(event: string, payload: T): void {
  for (const listener of devEventListeners.get(event) ?? []) {
    listener({ payload: clone(payload) });
  }
}

function finishDevProductCommand(
  meta: ProductCommandMeta,
  entity: ProductEntity,
  action: string,
): ProductCommandResult<ProductEntity> {
  const existing = productCommandResults.get(meta.idempotencyKey);
  if (existing) {
    if (existing.commandId !== meta.commandId) {
      throw new Error("idempotency key was already used by another command");
    }
    return { ...clone(existing), duplicate: true };
  }
  const event: ProductEvent = {
    sequence: productEvents.length + 1,
    commandId: meta.commandId,
    entity: entity.kind,
    entityId: productEntityId(entity),
    action,
    revision: entity.value.revision,
  };
  productEvents.push(event);
  const result: ProductCommandResult<ProductEntity> = {
    commandId: meta.commandId,
    eventSequence: event.sequence,
    value: clone(entity),
    duplicate: false,
  };
  productCommandResults.set(meta.idempotencyKey, clone(result));
  emitDevEvent(PRODUCT_EVENT_NAME, event);
  return result;
}

function createDevProductEntity(entity: ProductEntity): void {
  if (getDevProductEntity(entity.kind, productEntityId(entity))) {
    throw new Error(`${entity.kind} already exists: ${productEntityId(entity)}`);
  }
  if (entity.kind === "project") {
    projects.push({
      id: entity.value.id,
      name: entity.value.name,
      cwd: entity.value.workspacePath,
      sessionCount: 0,
      sortOrder: entity.value.sortOrder,
      pinned: entity.value.pinned,
    });
    productProjectRevisions.set(entity.value.id, entity.value.revision);
    return;
  }
  if (entity.kind === "task") {
    tasks.push({
      id: entity.value.id,
      projectId: entity.value.projectId,
      sessionId: entity.value.id,
      title: entity.value.title,
      titleSource: "auto",
      status: entity.value.status,
      createdAt: entity.value.createdAt,
      parentId: entity.value.parentId,
      dependsOn: [...entity.value.dependsOn],
      sortOrder: entity.value.sortOrder,
      pinned: entity.value.pinned,
      archived: entity.value.archived,
    });
    productTaskRevisions.set(entity.value.id, entity.value.revision);
    return;
  }
  if (entity.kind === "conversation") {
    productConversations.set(entity.value.id, clone(entity.value));
    return;
  }
  productExtraEntities.set(`${entity.kind}:${productEntityId(entity)}`, clone(entity));
}

function updateDevProductEntity(
  meta: ProductCommandMeta,
  input: ProductEntity,
): ProductEntity {
  const id = productEntityId(input);
  const current = getDevProductEntity(input.kind, id);
  if (!current) throw new Error(`${input.kind} not found: ${id}`);
  if (meta.expectedRevision !== current.value.revision) {
    throw new Error(`stale ${input.kind} revision`);
  }
  const updated = clone(input);
  updated.value.revision = current.value.revision + 1;
  if (updated.kind === "project") {
    const index = projects.findIndex((project) => project.id === updated.value.id);
    if (updated.value.archive === "archived") {
      if (index >= 0) projects.splice(index, 1);
    } else if (index >= 0) {
      projects[index] = {
        ...projects[index],
        name: updated.value.name,
        cwd: updated.value.workspacePath,
        pinned: updated.value.pinned,
        sortOrder: updated.value.sortOrder,
      };
    }
    productProjectRevisions.set(updated.value.id, updated.value.revision);
  } else if (updated.kind === "task") {
    const index = tasks.findIndex((task) => task.id === updated.value.id);
    if (index >= 0) {
      tasks[index] = {
        ...tasks[index],
        projectId: updated.value.projectId,
        title: updated.value.title,
        status: updated.value.status,
        parentId: updated.value.parentId,
        dependsOn: [...updated.value.dependsOn],
        sortOrder: updated.value.sortOrder,
        pinned: updated.value.pinned,
        archived: updated.value.archived,
      };
    }
    productTaskRevisions.set(updated.value.id, updated.value.revision);
  } else if (updated.kind === "conversation") {
    productConversations.set(updated.value.id, clone(updated.value));
  } else {
    productExtraEntities.set(`${updated.kind}:${productEntityId(updated)}`, clone(updated));
  }
  return updated;
}

function defaultDevRouterModes(): Record<ChatBackendKind, RouterMode> {
  return createChatBackendRecord(defaultRouterModeForBackend);
}

function defaultDevBackendEnvStatus(backend: ChatBackendKind): BackendEnvStatus {
  return {
    backend,
    hasApiKey: false,
    connectionMode: "unconfigured",
    effectiveUrl: null,
  };
}

function defaultDevBackendEnvStatuses(): Record<ChatBackendKind, BackendEnvStatus> {
  return createChatBackendRecord(defaultDevBackendEnvStatus);
}

function text(args: Args, key: string): string {
  return typeof args[key] === "string" ? args[key] : "";
}

function bool(args: Args, key: string, fallback = false): boolean {
  return typeof args[key] === "boolean" ? args[key] : fallback;
}

function projectNameFromPath(path: string): string {
  const parts = path.trim().replace(/[\\/]+$/, "").split(/[\\/]/).filter(Boolean);
  return parts.at(-1) || "未命名项目";
}

function projectCwdKey(path: string): string {
  return path.trim().replace(/[\\/]+$/, "").replace(/\//g, "\\").toLowerCase();
}

function ensureDevFolderProjects(paths: string[]) {
  return paths
    .filter((path, index) => path && paths.indexOf(path) === index && !/\.[^\\/]+$/.test(path))
    .map((path, index) => {
      const key = projectCwdKey(path);
      const existing = projects.find((project) => project.cwd && projectCwdKey(project.cwd) === key);
      if (existing) return existing;
      return {
        id: `project-${Date.now()}-${index}`,
        name: projectNameFromPath(path),
        cwd: path,
        sessionCount: 0,
        sortOrder: projects.length + index,
        pinned: false,
      };
    });
}

function record(value: unknown): Args {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Args : {};
}

function dayStart(timestamp: number) {
  return Math.floor(timestamp / dayMs) * dayMs;
}

function dateOnly(timestamp: number) {
  return new Date(timestamp).toISOString().slice(0, 10);
}

function quotaUsageStatsDays(value: unknown): QuotaUsageStatsDays {
  return isQuotaUsageStatsDays(value) ? value : DEFAULT_QUOTA_USAGE_STATS_DAYS;
}

function quotaUsageStatsBackend(value: unknown): QuotaUsageStatsBackendFilter {
  return isQuotaUsageStatsBackendFilter(value) ? value : "all";
}

function sumQuotaTokens(rows: Array<{
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  totalTokens: number;
}>) {
  return rows.reduce(
    (acc, row) => ({
      inputTokens: acc.inputTokens + row.inputTokens,
      outputTokens: acc.outputTokens + row.outputTokens,
      cacheReadTokens: acc.cacheReadTokens + row.cacheReadTokens,
      cacheCreationTokens: acc.cacheCreationTokens + row.cacheCreationTokens,
      totalTokens: acc.totalTokens + row.totalTokens,
    }),
    { inputTokens: 0, outputTokens: 0, cacheReadTokens: 0, cacheCreationTokens: 0, totalTokens: 0 },
  );
}

function quotaCostSummary(
  rows: Array<{ knownCostUsd: number | null; costRecordCount: number; recordCount: number }>,
) {
  const costRecordCount = rows.reduce((sum, row) => sum + row.costRecordCount, 0);
  const knownCostUsd = rows.reduce(
    (sum, row) => sum + (row.knownCostUsd ?? 0),
    0,
  );
  return {
    knownCostUsd: costRecordCount > 0 ? Number(knownCostUsd.toFixed(4)) : null,
    costRecordCount,
    totalRecordCount: rows.reduce((sum, row) => sum + row.recordCount, 0),
  };
}

function createDevQuotaUsageStats(args: Args = {}) {
  const input = record(args.input);
  const days = quotaUsageStatsDays(input.days);
  const backend = quotaUsageStatsBackend(input.backend);
  const rangeEnd = dayStart(Date.now()) + dayMs;
  const rangeStart = rangeEnd - days * dayMs;
  const backendNames: ChatBackendKind[] = backend === "all"
    ? [...CHAT_BACKENDS]
    : [CHAT_BACKENDS.includes(backend as ChatBackendKind) ? backend as ChatBackendKind : "native-agentkit"];
  const daily = Array.from({ length: days }, (_, index) => {
    const active = index >= Math.max(0, days - 6);
    const base = active ? Math.round((index + 3) * 420) : 0;
    const inputTokens = base * 3;
    const outputTokens = base;
    const cacheReadTokens = active ? Math.round(base * 0.42) : 0;
    const cacheCreationTokens = active ? Math.round(base * 0.16) : 0;
    return {
      dayStart: rangeStart + index * dayMs,
      inputTokens,
      outputTokens,
      cacheReadTokens,
      cacheCreationTokens,
      totalTokens: inputTokens + outputTokens + cacheReadTokens + cacheCreationTokens,
      knownCostUsd: active ? Number((0.012 * (index + 1)).toFixed(4)) : null,
      costRecordCount: active ? 1 : 0,
      recordCount: active ? 1 : 0,
    };
  });
  const totals = sumQuotaTokens(daily);
  const cost = quotaCostSummary(daily);
  const backends = backendNames.map((name) => ({
    backend: name,
    inputTokens: totals.inputTokens,
    outputTokens: totals.outputTokens,
    cacheReadTokens: totals.cacheReadTokens,
    cacheCreationTokens: totals.cacheCreationTokens,
    totalTokens: totals.totalTokens,
    knownCostUsd: Number((0.12).toFixed(2)),
    costRecordCount: 3,
    recordCount: Math.max(1, cost.totalRecordCount),
  }));
  const summary = {
    ...totals,
    knownCostUsd: cost.knownCostUsd,
    costRecordCount: cost.costRecordCount,
    recordCount: cost.totalRecordCount,
  };
  return {
    days,
    backend,
    rangeStart,
    rangeEnd,
    totals,
    cost,
    daily,
    backends,
    recent: backends.map((row, index) => ({
      eventId: `dev-quota-${row.backend}-${index}`,
      taskId: `dev-quota-task-${index + 1}`,
      turnId: `dev-quota-turn-${index + 1}`,
      backend: row.backend,
      sessionId: "dev-native-session",
      inputTokens: row.inputTokens,
      outputTokens: row.outputTokens,
      cacheReadTokens: row.cacheReadTokens,
      cacheCreationTokens: row.cacheCreationTokens,
      totalTokens: row.totalTokens,
      knownCostUsd: row.knownCostUsd,
      createdAt: rangeEnd - (index + 1) * 3_600_000,
    })),
    projects: [{
      projectId: "lilia",
      projectName: "Lilia",
      projectCwd: "C:\\Files\\workspace\\Lilia",
      ...summary,
    }],
    conversations: [{
      taskId: "t-001",
      taskTitle: "浏览开发期 mock 页面",
      taskStatus: "running",
      projectId: "lilia",
      projectName: "Lilia",
      ...summary,
    }],
    tools: [
      {
        key: "command::",
        label: "命令",
        kind: "command",
        subkind: null,
        toolName: null,
        callCount: Math.max(1, cost.totalRecordCount - 1),
        sharePercent: 62,
      },
      {
        key: "search:grep:",
        label: "内容搜索",
        kind: "search",
        subkind: "grep",
        toolName: null,
        callCount: Math.max(1, Math.floor(cost.totalRecordCount / 2)),
        sharePercent: 38,
      },
    ],
  };
}

function architecture(projectId: string) {
  return {
    projectId,
    version: 0,
    summary: "开发期 mock 架构图为空。",
    nodes: [],
    edges: [],
    updatedAt: now,
  };
}

export async function invoke<T>(cmd: string, args: Args = {}): Promise<T> {
  if (emptyLists.has(cmd)) return [] as T;
  if (noops.has(cmd)) return undefined as T;
  if (cmd.startsWith("plugin:updater|")) return null as T;

  if (
    cmd === "native_credential_providers" ||
    cmd === "native_product_timeline"
  ) {
    return [] as T;
  }
  if (cmd === "native_credential_diagnostics" || cmd === "native_agent_host_status") {
    return {
      wired: true,
      defaultBackend: "native-agentkit",
      activeBackend: "native-agentkit",
      runtimeBackend: "native-agentkit",
      runtimeReady: true,
      officialAgentServer: false,
      nodeRunnerDefault: false,
      profileId: null,
      profileHasCredentialRefs: false,
      credentialAndRuntimeIndependent: true,
      liveModelAdapterDrivesTurn: false,
      timelineIsAgentkitProjection: true,
      credential: {
        brokerReady: true,
        providerCount: 0,
        credentialCount: 0,
        activeCount: 0,
        unavailableCount: 0,
        hasUsableModelCredential: false,
        credentials: [],
      },
      diagnostics: {
        credential: {
          brokerReady: true,
          providerCount: 0,
          credentialCount: 0,
          activeCount: 0,
          unavailableCount: 0,
          hasUsableModelCredential: false,
          credentials: [],
        },
        runtimeBackend: "native-agentkit",
        runtimeReady: true,
        officialAgentServer: false,
        nodeRunnerDefault: false,
        profileId: null,
        profileHasCredentialRefs: false,
        credentialAndRuntimeIndependent: true,
        liveModelAdapterDrivesTurn: false,
      },
    } as T;
  }
  if (
    cmd === "native_credential_login" ||
    cmd === "native_credential_import" ||
    cmd === "native_credential_revoke" ||
    cmd === "native_respond_approval"
  ) {
    return {
      credentialId: "dev-mock",
      revision: 1,
      status: "active",
      approvalApplied: true,
    } as T;
  }
  if (cmd === "native_shared_coding_services_status") {
    return {
      gitServiceId: "mutsuki.agent.service.git",
      codeIndexServiceId: "mutsuki.agent.service.code-index",
      lspServiceId: "mutsuki.agent.service.lsp",
      computerUseServiceId: "mutsuki.agent.service.computer-use",
      mcpServiceId: "mutsuki.agent.service.mcp",
      memoryRunnerId: "mutsuki.agent.memory_router.runner",
      sharedIdentityOk: true,
      gitSameInstance: true,
      codeIndexSameInstance: true,
      lspSameInstance: true,
      mcpSameInstance: true,
      memorySharedRouter: true,
      mcpActiveServers: 0,
      lspActiveWorkspaces: 0,
      dataSource: "agentkit.native_coding_bundle",
      officialAgentServer: false,
    } as T;
  }
  if (cmd === "native_shared_mcp_list_servers") {
    return [] as T;
  }
  if (cmd === "native_shared_lsp_status") {
    return {
      serviceId: "mutsuki.agent.service.lsp",
      activeWorkspaces: 0,
      dataSource: "agentkit.native_coding_bundle",
      sameInstance: true,
    } as T;
  }
  if (cmd === "native_shared_git_status") {
    return {
      kind: "status",
      path: typeof args.path === "string" ? args.path : "",
      dataSource: "agentkit.native_coding_bundle",
      clean: true,
    } as T;
  }
  if (cmd === "native_shared_code_index_search") {
    return {
      hits: [
        {
          path: typeof args.relativePath === "string" ? args.relativePath : "src/shared_probe.rs",
          score: 1,
        },
      ],
      dataSource: "agentkit.native_coding_bundle",
    } as T;
  }
  if (cmd === "native_shared_memory_query") {
    return {
      records: [],
      dataSource: "agentkit.native_coding_bundle",
    } as T;
  }
  if (cmd === "native_shared_memory_write") {
    return {
      memory_id: "dev-memory-1",
      text: typeof args.text === "string" ? args.text : "",
      dataSource: "agentkit.native_coding_bundle",
    } as T;
  }

  switch (cmd) {
    case APP_RESTART_COMMAND:
      return undefined as T;
    case CLI_PROJECT_OPEN_CONSUME_PENDING_COMMAND:
      return null as T;
    case AGENT_DEBUG_STATUS_COMMAND:
      return {
        enabled: true,
        reason: null,
        startedAt: now,
        logCount: agentDebugLogs.length,
      } as T;
    case AGENT_DEBUG_LOGS_COMMAND:
      return clone(agentDebugLogs.slice(-(typeof args.limit === "number" ? args.limit : 200))) as T;
    case AGENT_DEBUG_RECORD_ACTION_COMMAND: {
      const entry = args.entry && typeof args.entry === "object" && !Array.isArray(args.entry)
        ? args.entry as Record<string, unknown>
        : {};
      agentDebugLogs = [...agentDebugLogs, { ...entry }];
      return undefined as T;
    }
    case AGENT_DEBUG_RESET_STATE_COMMAND:
      agentDebugLogs = [];
      return {
        enabled: true,
        reason: null,
        startedAt: Date.now(),
        logCount: 0,
      } as T;
    case AGENT_DEBUG_RUNTIME_SNAPSHOT_COMMAND:
      return {
        enabled: true,
        capturedAt: Date.now(),
        mainWindowPresent: true,
        liliaHome: "C:\\Users\\dev\\.lilia",
        runningTaskCount: 0,
        queuedTaskCount: 0,
      } as T;
    case TAURI_PLUGIN_DIALOG_OPEN_COMMAND:
      return null as T;
    case PRODUCT_LIST_ENTITIES_COMMAND:
      return clone(
        listDevProductEntities(text(args, "kind") as ProductEntityKind),
      ) as T;
    case PRODUCT_GET_ENTITY_COMMAND: {
      const kind = text(args, "kind") as ProductEntityKind;
      const id = text(args, "id");
      const entity = getDevProductEntity(kind, id);
      return clone(entity) as T;
    }
    case PRODUCT_LIST_EVENTS_COMMAND: {
      const request = args.request && typeof args.request === "object"
        ? args.request as { after?: number | null; limit?: number }
        : {};
      const items = productEvents
        .filter((event) => event.sequence > (request.after ?? 0))
        .slice(0, request.limit ?? 100)
        .map(clone);
      return {
        items,
        next: items.at(-1)?.sequence ?? null,
      } as T;
    }
    case PRODUCT_CREATE_ENTITY_COMMAND: {
      const meta = clone(args.meta as ProductCommandMeta);
      const input = clone(args.entity as ProductEntity);
      const existing = productCommandResults.get(meta.idempotencyKey);
      if (existing) {
        if (existing.commandId !== meta.commandId) {
          throw new Error("idempotency key was already used by another command");
        }
        return { ...clone(existing), duplicate: true } as T;
      }
      createDevProductEntity(input);
      return finishDevProductCommand(meta, input, text(args, "action") || "created") as T;
    }
    case PRODUCT_UPDATE_ENTITY_COMMAND: {
      const meta = clone(args.meta as ProductCommandMeta);
      const input = clone(args.entity as ProductEntity);
      const existing = productCommandResults.get(meta.idempotencyKey);
      if (existing) {
        if (existing.commandId !== meta.commandId) {
          throw new Error("idempotency key was already used by another command");
        }
        return { ...clone(existing), duplicate: true } as T;
      }
      const updated = updateDevProductEntity(meta, input);
      return finishDevProductCommand(meta, updated, text(args, "action") || "updated") as T;
    }
    case PROJECT_LIST_COMMAND:
      return clone(projects) as T;
    case PROJECT_DASHBOARD_LIST_COMMAND:
      return clone(projectDashboardRows()) as T;
    case PROJECT_GET_COMMAND:
      return clone(projects.find((project) => project.id === text(args, "id")) ?? null) as T;
    case PROJECT_CREATE_COMMAND:
      return clone({ ...projects[0], id: `project-${Date.now()}`, name: text(args, "name") || "未命名项目", pinned: false }) as T;
    case PROJECT_ENSURE_FOLDERS_COMMAND:
      return clone(ensureDevFolderProjects(
        Array.isArray(args.paths) ? args.paths.map(String) : [],
      )) as T;
    case PROJECT_RENAME_COMMAND:
    case PROJECT_REMOVE_COMMAND:
      return true as T;
    case PROJECT_TOGGLE_PIN_COMMAND:
      return false as T;
    case PROJECT_GET_SETTINGS_COMMAND:
      return {
        cloneParentDir: "C:\\Files\\workspace",
        codexDefaults: null,
        githubBinding: null,
        worktree: defaultWorktreeSettings,
      } as T;
    case REMOTE_CONTROL_STATUS_COMMAND:
      return clone(remoteControlStatus()) as T;
    case REMOTE_CONTROL_SET_HOST_ENABLED_COMMAND:
      remoteControlEnabled = bool(args, "enabled");
      return clone(remoteControlStatus()) as T;
    case REMOTE_CONTROL_SET_PC_NAME_COMMAND:
      return clone(remoteControlStatus()) as T;
    case REMOTE_CONTROL_SET_KEEP_AWAKE_ENABLED_COMMAND:
      remoteControlKeepAwakeEnabled = bool(args, "enabled", true);
      return clone(remoteControlStatus()) as T;
    case REMOTE_CONTROL_START_PAIRING_COMMAND:
      remoteControlEnabled = true;
      remoteControlTicket = {
        id: "mock-ticket",
        pcName: "Lilia Dev PC",
        pcEndpoint: { endpointId: "mock-pc-endpoint", relayUrl: null, directAddresses: [] },
        protocolVersion: 1,
        challenge: "mock-challenge",
        expiresAt: now + 600_000,
        bridgeUrl: remoteControlBridgeUrl,
        pairingUri: `lilia-remote://pair?v=1&ticket=mock-ticket&challenge=mock-challenge&endpoint=mock-pc-endpoint&name=Lilia%20Dev%20PC&bridge=${encodeURIComponent(remoteControlBridgeUrl)}`,
      };
      return clone(remoteControlTicket) as T;
    case REMOTE_CONTROL_CANCEL_PAIRING_COMMAND:
      remoteControlTicket = null;
      return undefined as T;
    case REMOTE_CONTROL_REVOKE_DEVICE_COMMAND:
      remoteControlDevices = remoteControlDevices.map((device) =>
        device.id === text(args, "deviceId")
          ? { ...device, trusted: false, revokedAt: now }
          : device
      );
      return clone(remoteControlStatus()) as T;
    case REMOTE_CONTROL_PAIR_DEVICE_COMMAND: {
      const input = (args.input ?? {}) as Args;
      const endpoint = (input.androidEndpoint ?? {}) as Args;
      const device = {
        id: `mock-device-${Date.now()}`,
        kind: "android",
        displayName: text(input, "deviceName") || "Android device",
        endpointId: text(endpoint, "endpointId") || "mock-android-endpoint",
        protocolVersion: 1,
        trusted: true,
        firstPairedAt: now,
        lastSeenAt: now,
        revokedAt: null,
      };
      remoteControlDevices = [device, ...remoteControlDevices];
      remoteControlTicket = null;
      return clone(device) as T;
    }
    case MEMORY_LIST_COMMAND: {
      const projectId = typeof args.projectId === "string" ? args.projectId : null;
      return clone(memories.filter((item) => item.scope === "user" || item.projectId === projectId)) as T;
    }
    case MEMORY_UPSERT_COMMAND: {
      const input = (args.input ?? {}) as Args;
      const normalized = createMemoryUpsertInput({
        id: typeof input.id === "string" ? input.id : null,
        scope: input.scope,
        projectId: text(input, "projectId") || "lilia",
        title: text(input, "title") || "新记忆",
        body: text(input, "body"),
        tags: Array.isArray(input.tags) ? input.tags : [],
        enabled: typeof input.enabled === "boolean" ? input.enabled : true,
        sourceTaskId: typeof input.sourceTaskId === "string" ? input.sourceTaskId : null,
      });
      const id = normalized.id || `memory-${Date.now()}`;
      const existing = memories.find((item) => item.id === id);
      const saved: Memory = {
        id,
        scope: normalized.scope,
        projectId: normalized.projectId ?? null,
        title: normalized.title,
        body: normalized.body,
        tags: normalized.tags ?? [],
        enabled: normalized.enabled !== false,
        sourceTaskId: normalized.sourceTaskId ?? null,
        createdAt: existing?.createdAt ?? Date.now(),
        updatedAt: Date.now(),
      };
      memories = existing
        ? memories.map((item) => item.id === id ? saved : item)
        : [saved, ...memories];
      return clone(saved) as T;
    }
    case MEMORY_SET_ENABLED_COMMAND: {
      const id = text(args, "id");
      memories = memories.map((item) =>
        item.id === id ? { ...item, enabled: bool(args, "enabled"), updatedAt: Date.now() } : item
      );
      return clone(memories.find((item) => item.id === id) ?? memories[0]) as T;
    }
    case MEMORY_DELETE_COMMAND: {
      const id = text(args, "id");
      const before = memories.length;
      memories = memories.filter((item) => item.id !== id);
      return (memories.length !== before) as T;
    }
    case MEMORY_GET_SETTINGS_COMMAND:
      return clone(memorySettings) as T;
    case MEMORY_SET_SETTINGS_COMMAND: {
      const input = (args.settings ?? {}) as Args;
      memorySettings = normalizeMemorySettings(input, memorySettings);
      return undefined as T;
    }
    case MEMORY_GET_INJECTION_STATE_COMMAND:
    case MEMORY_SET_TASK_ENABLED_COMMAND:
    case MEMORY_RESET_TASK_COOLDOWN_COMMAND:
      return {
        taskId: text(args, "taskId"),
        enabled: args.enabled !== false,
        lastInjectedTurnSeq: null,
        updatedAt: Date.now(),
      } as T;
    case TASK_LIST_COMMAND: {
      const projectId = args.projectId ?? null;
      return clone(tasks.filter((task) => task.projectId === projectId)) as T;
    }
    case TASK_LIST_SIDEBAR_CONVERSATIONS_COMMAND:
      return clone(
        tasks
          .filter((task) => task.archived !== true)
          .sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.createdAt - a.createdAt)
          .map((task) => ({
            taskId: task.id,
            projectId: task.projectId ?? null,
            projectName: task.projectId
              ? projects.find((project) => project.id === task.projectId)?.name ?? null
              : null,
            title: task.title,
            createdAt: task.createdAt,
            pinned: task.pinned,
            route: task.projectId ? `/projects/${task.projectId}/tasks/${task.id}` : `/chats/${task.id}`,
          })),
      ) as T;
    case TASK_GET_COMMAND:
      return clone(tasks.find((task) => task.id === text(args, "id")) ?? null) as T;
    case TASK_HANDOFF_GET_COMMAND:
      return null as T;
    case TASK_PROMOTE_COMMAND:
      return clone({
        ...tasks[0],
        id: text(args, "id") || `task-${Date.now()}`,
        projectId: args.projectId ?? null,
        title: text(args, "title") || "新对话",
        createdAt: Date.now(),
      }) as T;
    case TASK_TOGGLE_PIN_COMMAND:
    case TASK_ARCHIVE_COMMAND:
      return true as T;
    case TASK_ARCHIVE_PROJECT_COMMAND:
      return 0 as T;
    case WORKTREE_LIST_COMMAND:
      return [
        {
          path: text(args, "baseRepoPath") || "C:\\Files\\workspace\\Lilia",
          head: null,
          branch: "main",
          bare: false,
          detached: false,
          prunable: false,
          locked: false,
          isMain: true,
          isTaskBound: false,
        },
        {
          path: "C:\\Files\\workspace\\Lilia-task-worktree",
          head: null,
          branch: "lilia/mock-task",
          bare: false,
          detached: false,
          prunable: false,
          locked: false,
          isMain: false,
          isTaskBound: false,
        },
      ] as T;
    case WORKTREE_GET_FOR_TASK_COMMAND:
      return clone(taskWorktrees[text(args, "taskId")] ?? null) as T;
    case WORKTREE_CLEAR_TASK_COMMAND:
      delete taskWorktrees[text(args, "taskId")];
      return undefined as T;
    case WORKTREE_CREATE_FOR_TASK_COMMAND: {
      const input = (args.input ?? {}) as Args;
      const taskId = text(input, "taskId");
      const saved = {
        taskId,
        projectId: text(input, "projectId") || null,
        baseRepoPath: text(input, "baseRepoPath"),
        worktreePath: `${text(input, "baseRepoPath") || "C:\\Files\\workspace\\Lilia"}-task-worktree`,
        branchName: `lilia/${taskId || "mock-task"}`,
        baseBranch: "main",
        status: "active",
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };
      taskWorktrees[taskId] = saved;
      return clone(saved) as T;
    }
    case WORKTREE_ATTACH_TASK_COMMAND: {
      const input = (args.input ?? {}) as Args;
      const taskId = text(input, "taskId");
      const saved = {
        taskId,
        projectId: text(input, "projectId") || null,
        baseRepoPath: text(input, "baseRepoPath"),
        worktreePath: text(input, "worktreePath"),
        branchName: "lilia/mock-task",
        baseBranch: "main",
        status: "active",
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };
      taskWorktrees[taskId] = saved;
      return clone(saved) as T;
    }
    case WORKTREE_CLEANUP_ARCHIVE_COMMAND: {
      const taskId = text(args, "taskId");
      delete taskWorktrees[taskId];
      return { merged: false, removed: true, archived: true, message: "mock cleaned" } as T;
    }
    case WORKTREE_MERGE_DELETE_ARCHIVE_COMMAND: {
      const taskId = text(args, "taskId");
      delete taskWorktrees[taskId];
      return { merged: true, removed: true, archived: true, message: "mock merged" } as T;
    }
    case MILESTONE_LIST_COMMAND:
      return { milestones: [], links: [] } as T;
    case MILESTONE_CREATE_COMMAND:
      return {
        id: `milestone-${Date.now()}`,
        projectId: text(args, "projectId"),
        title: text(args, "title") || "新里程碑",
        description: "",
        status: "upcoming",
        dueDate: null,
        order: 0,
        createdAt: Date.now(),
      } as T;
    case CHAT_CHECK_ENV_COMMAND:
      return {
        nodeAvailable: true,
        routerModes: defaultDevRouterModes(),
        backends: defaultDevBackendEnvStatuses(),
      } as T;
    case PROVIDER_GET_ACTIVE_BACKEND_COMMAND:
      return "native-agentkit" as T;
    case PROVIDER_GET_CONFIG_COMMAND:
      return {
        backend: text(args, "backend") || "native-agentkit",
        baseUrl: null,
        apiKey: null,
        hasApiKey: false,
      } as T;
    case ROUTER_GET_MODE_COMMAND:
      return defaultRouterModeForBackend(
        providerBackends.includes(text(args, "backend") as ChatBackendKind)
          ? text(args, "backend") as ChatBackendKind
          : "native-agentkit",
      ) as T;
    case ASSISTANT_AI_GET_CONFIG_COMMAND:
      return {
        baseUrl: null,
        apiKey: null,
        model: null,
        modelPool: [],
        hasApiKey: false,
      } as T;
    case ASSISTANT_AI_FETCH_MODELS_COMMAND:
      return {
        ok: true,
        error: null,
        models: [
          { id: "mock-assistant", label: "mock-assistant", source: "remote", backend: "native-agentkit" },
          { id: "mock-assistant-pro", label: "mock-assistant-pro", source: "remote", backend: "native-agentkit" },
        ],
      } as T;
    case MODEL_FEATURE_LIST_MODEL_OPTIONS_COMMAND:
      return [
        { id: "mock-assistant", label: "mock-assistant", source: "remote", backend: "native-agentkit" },
        { id: "mock-assistant-pro", label: "mock-assistant-pro", source: "remote", backend: "native-agentkit" },
      ] as T;
    case MODEL_FEATURE_GET_SETTINGS_COMMAND:
      return {
        chat: { light: null, normal: null, deep: null },
        presets: [
          { id: "fast", label: "Fast", kind: "builtin", model: null, reasoningEffort: null, enabled: true },
          { id: "default", label: "Default", kind: "builtin", model: null, reasoningEffort: null, enabled: true },
          { id: "plan", label: "Plan", kind: "builtin", model: null, reasoningEffort: null, enabled: true },
          { id: "review", label: "Review", kind: "builtin", model: null, reasoningEffort: null, enabled: true },
        ],
        title: null,
        suggestion: null,
        promptRouter: null,
        promptOptimize: null,
        autoTurnDecision: null,
      } as T;
    case MODEL_FEATURE_SET_SETTINGS_COMMAND:
      return undefined as T;
    case ASSISTANT_AI_TEST_CONNECTION_COMMAND:
      return { ok: true, error: null, models: ["mock-assistant"], modelMatched: true } as T;
    case ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND:
      return [
        "请基于当前上下文处理以下任务：",
        text((args.input ?? {}) as Args, "prompt"),
        "",
        "要求：先做简单定位，明确本次修改范围，保留现有数据契约，不自动扩大任务。",
      ].join("\n") as T;
    case CONVERSATION_SUGGESTIONS_GET_SETTINGS_COMMAND:
      return { enabled: false, maxItems: 5 } as T;
    case CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND:
      return { sources: [], localGit: null } as T;
    case CHAT_LIST_MODELS_COMMAND: {
      const backend = providerBackends.includes(text(args, "backend") as ChatBackendKind)
        ? text(args, "backend") as ChatBackendKind
        : "native-agentkit";
      return MODEL_OPTIONS_BY_BACKEND[backend].map((option) => ({ ...option, backend })) as T;
    }
    case CHAT_GET_COMPOSER_STATE_COMMAND:
      return {
        taskId: text(args, "taskId"),
        backend: "native-agentkit",
        model: DEFAULT_MODEL_BY_BACKEND["native-agentkit"],
        planMode: false,
        goalMode: false,
        permission: normalizePermissionMode(null),
      } as T;
    case CHAT_GET_RUNTIME_SNAPSHOT_COMMAND:
      return { taskId: text(args, "taskId"), phase: "idle", backend: null, turnId: null, queuedCount: 0, pendingRollback: false, pendingResetCleanup: false, rollback: null } as T;
    case AGENT_INTERACTION_GET_SETTINGS_COMMAND:
      return clone(agentInteractionSettings) as T;
    case AGENT_INTERACTION_SET_SETTINGS_COMMAND:
      agentInteractionSettings = normalizeAgentInteractionSettings(
        (args.settings ?? null) as Partial<AgentInteractionSettings> | null,
        agentInteractionSettings,
      );
      return undefined as T;
    case AGENT_INTERACTION_LIST_SUBAGENTS_COMMAND:
      return agentInteractionSubagents.map((item) => ({ ...item })) as T;
    case AGENT_INTERACTION_UPSERT_SUBAGENT_COMMAND: {
      const input = (args.input ?? {}) as Args;
      const saved = {
        id: text(input, "id") || `agent-${agentInteractionSubagents.length + 1}`,
        name: text(input, "name"),
        description: text(input, "description"),
        instruction: text(input, "instruction"),
        enabled: input.enabled !== false,
      };
      const index = agentInteractionSubagents.findIndex((item) => item.id === saved.id);
      if (index === -1) agentInteractionSubagents = [...agentInteractionSubagents, saved];
      else agentInteractionSubagents = agentInteractionSubagents.map((item, itemIndex) => itemIndex === index ? saved : item);
      return saved as T;
    }
    case AGENT_INTERACTION_DELETE_SUBAGENT_COMMAND:
      agentInteractionSubagents = agentInteractionSubagents.filter((item) => item.id !== text(args, "id"));
      return undefined as T;
    case CHAT_SEARCH_SLASH_COMMANDS_COMMAND:
      return [{ command: { id: "native:help", name: "help", title: "显示可用斜杠命令", description: "开发期 mock 命令。", source: "native", parameters: [] }, matchedBy: "name" }] as T;
    case CHAT_SEND_MESSAGE_COMMAND:
      return { userEvent: null } as T;
    case CHAT_INTERRUPT_TURN_COMMAND:
      return { interrupted: false, reason: "dev-mock-idle" } as T;
    case PROJECT_ARCHITECTURE_GET_COMMAND:
      return architecture(text(args, "projectId")) as T;
    case PROJECT_ARCHITECTURE_APPLY_COMMAND:
    case PROJECT_ARCHITECTURE_REJECT_COMMAND: {
      const input = (args.input ?? {}) as Args;
      return { graph: architecture(text(input, "projectId")), event: null } as T;
    }
    case PROJECT_ARCHITECTURE_ROLLBACK_COMMAND:
      return { graph: architecture(text(args, "projectId")), event: null } as T;
    case PLUGINS_OVERVIEW_COMMAND:
      return {
        skills: [],
        packages: [],
        mcpServers: [],
        configPaths: { "native-agentkit": null },
        warnings: [],
      } as T;
    case PLUGINS_HOOKS_OVERVIEW_COMMAND:
      return {
        sources: [],
        warnings: [],
      } as T;
    case PLUGINS_READ_HOOK_SOURCE_COMMAND:
      return {
        source: args.source,
        handlers: [],
        rawDocument: "{\n  \"hooks\": {}\n}\n",
        rawFormat: "json",
        warnings: [],
        limitations: [],
      } as T;
    case PLUGINS_UPDATE_HOOK_SOURCE_COMMAND:
      return {
        source: args.source,
        handlers: (args.input as Args)?.handlers ?? [],
        rawDocument: "{\n  \"hooks\": {}\n}\n",
        rawFormat: "json",
        warnings: [],
        limitations: [],
      } as T;
    case PLUGINS_CREATE_HOOK_SOURCE_COMMAND:
      return {
        id: `${text(args, "backend") || "native-agentkit"}-${text(args, "scope") || "user"}`,
        backend: text(args, "backend") || "native-agentkit",
        scope: text(args, "scope") || "user",
        format: "hooks_json",
        name: "Mock Hooks",
        path: "C:\\Users\\dev\\.lilia\\hooks.json",
        exists: true,
        editable: true,
        managed: false,
        enabled: false,
        handlerCount: 0,
        warnings: [],
        limitations: [],
        trustState: "unknown",
        description: null,
      } as T;
    case PLUGINS_DELETE_HOOK_SOURCE_COMMAND:
    case PLUGINS_SET_HOOK_SOURCE_ENABLED_COMMAND:
    case PLUGINS_OPEN_HOOK_CONFIG_COMMAND:
      return undefined as T;
    case POPUP_GET_WINDOW_SETTINGS_COMMAND:
      return { shortcut: null } as T;
    case LILIA_IAB_SUBMIT_COMMAND:
      return { submitted: false, reason: "dev-mock" } as T;
    case GITHUB_GET_BINDING_STATUS_COMMAND:
      return { state: "unbound", clientIdConfigured: false, clientIdSource: "none", binding: null } as T;
    case GITHUB_LIST_REPOS_COMMAND:
      return { items: [], nextPage: null } as T;
    case GITHUB_START_DEVICE_FLOW_COMMAND:
      return { deviceCode: "mock-device", userCode: "MOCK-DEV", verificationUri: "https://github.com/login/device", expiresAt: Date.now() + 600_000, intervalSeconds: 5 } as T;
    case GITHUB_POLL_DEVICE_FLOW_COMMAND:
      return { status: "pending", intervalSeconds: 5, bindingStatus: null, error: null } as T;
    case GIT_CLONE_REPO_COMMAND:
    case GITHUB_CLONE_REPO_COMMAND:
      return "C:\\Files\\workspace\\mock-clone" as T;
    case QUOTA_USAGE_GET_STATS_COMMAND:
      return createDevQuotaUsageStats(args) as T;
    default:
      console.warn(`[lilia:dev-mock] Unhandled Tauri command: ${cmd}`, args);
      return undefined as T;
  }
}

export { SERIALIZE_TO_IPC_FN };

export class Channel {
  id = `mock-channel-${++mockChannelId}`;
  onmessage?: (message: unknown) => void;

  [SERIALIZE_TO_IPC_FN](): string {
    return `__CHANNEL__:${this.id}`;
  }

  toJSON() {
    return this[SERIALIZE_TO_IPC_FN]();
  }
}

export class Resource {
  constructor(public rid: number | string) {}

  async close(): Promise<void> {
    return undefined;
  }
}

export function isTauri(): boolean {
  return false;
}

export function convertFileSrc(path: string): string {
  return `asset://${path.replace(/\\/g, "/")}`;
}

export async function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<UnlistenFn> {
  const listeners = devEventListeners.get(event) ?? new Set();
  const listener = handler as (event: { payload: unknown }) => void;
  listeners.add(listener);
  devEventListeners.set(event, listeners);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) devEventListeners.delete(event);
  };
}

export function getCurrentWindow() {
  return {
    label: "main",
    listen,
    close: async () => undefined,
    minimize: async () => undefined,
    toggleMaximize: async () => undefined,
    isMaximized: async () => false,
    setAlwaysOnTop: async () => undefined,
    setDecorations: async () => undefined,
    setIgnoreCursorEvents: async () => undefined,
    setOpacity: async () => undefined,
    setPosition: async () => undefined,
    setSize: async () => undefined,
    innerPosition: async () => ({ x: 0, y: 0 }),
    innerSize: async () => ({ width: 960, height: 720 }),
    scaleFactor: async () => 1,
    startDragging: async () => undefined,
  };
}

export function getCurrentWebview() {
  return { onDragDropEvent: listen };
}

export async function homeDir(): Promise<string> {
  return "C:\\Users\\dev";
}

export class PhysicalPosition {
  constructor(public x: number, public y: number) {}
}

export class PhysicalSize {
  constructor(public width: number, public height: number) {}
}

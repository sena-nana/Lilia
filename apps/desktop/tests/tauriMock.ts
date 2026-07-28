import { vi } from "vitest";
import {
  AUTOMATION_CHANGED_EVENT_NAME,
  AUTOMATION_DELETE_WORKFLOW_COMMAND,
  AUTOMATION_GET_RUN_COMMAND,
  AUTOMATION_LIST_RUNS_COMMAND,
  AUTOMATION_LIST_WORKFLOWS_COMMAND,
  AUTOMATION_PUBLISH_COMMAND,
  AUTOMATION_RESUME_RUN_COMMAND,
  AUTOMATION_RUN_ONCE_COMMAND,
  AUTOMATION_RUN_FINISHED_EVENT_NAME,
  AUTOMATION_RUN_STARTED_EVENT_NAME,
  AUTOMATION_RUN_UPDATED_EVENT_NAME,
  AUTOMATION_SAVE_DRAFT_COMMAND,
  AUTOMATION_SET_ENABLED_COMMAND,
  APP_RESTART_COMMAND,
  AGENT_TIMELINE_BATCH_EVENT_NAME,
  AGENT_TIMELINE_CLEAR_TASK_COMMAND,
  AGENT_TIMELINE_LIST_COMMAND,
  AGENT_TIMELINE_EVENT_NAME,
  CHAT_ACK_RESTORED_ROLLBACK_COMMAND,
  CHAT_CHECK_ENV_COMMAND,
  CHAT_DESCRIBE_ATTACHMENTS_COMMAND,
  CHAT_DONE_EVENT_NAME,
  CHAT_GET_COMPOSER_STATE_COMMAND,
  CHAT_GET_RUNTIME_SNAPSHOT_COMMAND,
  CHAT_INTERRUPT_TURN_COMMAND,
  CHAT_LIST_MODELS_COMMAND,
  CHAT_READ_CLIPBOARD_FILE_PATHS_COMMAND,
  CHAT_RESPOND_AGENT_INTERACTION_COMMAND,
  CHAT_RESPOND_TITLE_UPDATE_COMMAND,
  CHAT_SAVE_CLIPBOARD_IMAGE_COMMAND,
  CHAT_SAVE_CLIPBOARD_TEXT_COMMAND,
  CHAT_SEARCH_CONTEXT_ATTACHMENTS_COMMAND,
  CHAT_SEARCH_SLASH_COMMANDS_COMMAND,
  CHAT_SEND_MESSAGE_COMMAND,
  CHAT_SEND_PROCESS_SESSION_COMMAND,
  CHAT_SET_COMPOSER_STATE_COMMAND,
  CHAT_TURN_STARTED_EVENT_NAME,
  CLI_PROJECT_OPEN_CONSUME_PENDING_COMMAND,
  AGENT_INTERACTION_DELETE_SUBAGENT_COMMAND,
  AGENT_INTERACTION_GET_SETTINGS_COMMAND,
  AGENT_INTERACTION_LIST_SUBAGENTS_COMMAND,
  AGENT_INTERACTION_SET_SETTINGS_COMMAND,
  AGENT_INTERACTION_UPSERT_SUBAGENT_COMMAND,
  ASSISTANT_AI_FETCH_MODELS_COMMAND,
  ASSISTANT_AI_GET_CONFIG_COMMAND,
  ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND,
  ASSISTANT_AI_SET_CONFIG_COMMAND,
  ASSISTANT_AI_TEST_CONNECTION_COMMAND,
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
  HISTORY_IMPORT_CLEAN_BACKGROUND_TERMINALS_COMMAND,
  HISTORY_IMPORT_RUNTIME_STATES_COMMAND,
  HISTORY_IMPORT_SEARCH_COMMAND,
  LILIA_IAB_OPEN_COMMAND,
  LILIA_IAB_SUBMIT_COMMAND,
  MILESTONE_CREATE_COMMAND,
  MILESTONE_DELETE_COMMAND,
  MILESTONE_LIST_COMMAND,
  MILESTONE_REORDER_COMMAND,
  MILESTONE_SET_TASKS_COMMAND,
  MILESTONE_UPDATE_COMMAND,
  PROJECT_ARCHITECTURE_APPLY_COMMAND,
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
  PLUGINS_CREATE_MCP_SERVER_COMMAND,
  PLUGINS_DELETE_SKILL_COMMAND,
  PLUGINS_DELETE_HOOK_SOURCE_COMMAND,
  PLUGINS_DELETE_MCP_SERVER_COMMAND,
  PLUGINS_HOOKS_OVERVIEW_COMMAND,
  PLUGINS_OPEN_HOOK_CONFIG_COMMAND,
  PLUGINS_OPEN_MCP_CONFIG_COMMAND,
  PLUGINS_OVERVIEW_COMMAND,
  PLUGINS_READ_HOOK_SOURCE_COMMAND,
  PLUGINS_SET_SKILL_ENABLED_COMMAND,
  PLUGINS_SET_HOOK_SOURCE_ENABLED_COMMAND,
  PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND,
  PLUGINS_SET_PACKAGE_ENABLED_COMMAND,
  PLUGINS_UPDATE_HOOK_SOURCE_COMMAND,
  PLUGINS_UPDATE_MCP_SERVER_COMMAND,
  PROVIDER_CODEX_APP_SERVER_CHECK_UPDATE_COMMAND,
  PROVIDER_CODEX_APP_SERVER_INSTALL_UPDATE_COMMAND,
  PROVIDER_CODEX_ACCOUNT_START_LOGIN_COMMAND,
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
  QUOTA_USAGE_CONSUME_CODEX_RATE_LIMIT_RESET_CREDIT_COMMAND,
  QUOTA_USAGE_GET_CODEX_ACCOUNT_STATUS_COMMAND,
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
  SYSTEM_OPEN_IN_VSCODE_COMMAND,
  SYSTEM_OPEN_PATH_COMMAND,
  SYSTEM_OPEN_URL_COMMAND,
  TASK_ARCHIVE_COMMAND,
  TASK_ARCHIVE_PROJECT_COMMAND,
  TASK_CREATE_COMMAND,
  TASK_GET_COMMAND,
  TASK_HANDOFF_GET_COMMAND,
  TASK_LIST_COMMAND,
  TASK_LIST_SIDEBAR_CONVERSATIONS_COMMAND,
  TASK_PROMOTE_COMMAND,
  TASK_REORDER_COMMAND,
  TASK_REPARENT_COMMAND,
  TASK_TOGGLE_PIN_COMMAND,
  TASK_UPDATE_COMMAND,
  TASK_UPDATE_DEPENDENCIES_COMMAND,
  TODO_APPLY_AGENT_EVENT_COMMAND,
  MAIN_NAVIGATE_EVENT_NAME,
  TODO_CREATE_COMMAND,
  TODO_DELETE_COMMAND,
  TODO_LIST_COMMAND,
  TODO_UPDATE_COMMAND,
  WORKTREE_ATTACH_TASK_COMMAND,
  WORKTREE_CLEANUP_ARCHIVE_COMMAND,
  WORKTREE_CLEAR_TASK_COMMAND,
  WORKTREE_CREATE_FOR_TASK_COMMAND,
  WORKTREE_GET_FOR_TASK_COMMAND,
  WORKTREE_LIST_COMMAND,
  WORKTREE_MERGE_DELETE_ARCHIVE_COMMAND,
  countProjectTaskStatuses,
  createAutomationChangedEvent,
  createAutomationRunEvent,
  createChatDoneEvent,
  createChatTurnStartedEvent,
  createAppNavigateEvent,
  createTodoChangedEvent,
  createTasksChangedEvent,
  deriveProjectDashboardCounts,
  DEFAULT_AUTOMATION_HUMAN_PROMPT,
  DEFAULT_MODEL_BY_BACKEND,
  DEFAULT_ROUTER_MODE_BY_BACKEND,
  DIRECT_DEFAULT_URLS,
  isMilestoneStatus,
  MODEL_OPTIONS_BY_BACKEND,
  normalizeRouterModeForBackend as normalizeContractRouterModeForBackend,
  normalizeAutomationScope,
  normalizeAgentInteractionSettings,
  routerModeUsesCodexAccount,
  TODO_CHANGED_EVENT_NAME,
  TASKS_CHANGED_EVENT_NAME,
  type AutomationScopeFilter,
  type AgentInteractionSettings,
  type BackendEnvStatus,
  type ConnectionMode,
  type MilestoneStatus,
  type ProjectTaskStatusCounts,
  type RouterMode,
} from "@lilia/contracts";
import {
  TAURI_PLUGIN_DIALOG_OPEN_COMMAND,
  TAURI_PLUGIN_EVENT_LISTEN_COMMAND,
} from "../src/tauri/pluginCommands";
import {
  NATIVE_AGENT_HOST_STATUS_COMMAND,
  NATIVE_CREDENTIAL_DIAGNOSTICS_COMMAND,
  NATIVE_CREDENTIAL_IMPORT_COMMAND,
  NATIVE_CREDENTIAL_LOGIN_COMMAND,
  NATIVE_CREDENTIAL_PROVIDERS_COMMAND,
  NATIVE_CREDENTIAL_REVOKE_COMMAND,
  NATIVE_PRODUCT_TIMELINE_COMMAND,
  NATIVE_QUOTA_SURFACE_COMMAND,
  NATIVE_RESPOND_APPROVAL_COMMAND,
  NATIVE_SHARED_CODING_SERVICES_STATUS_COMMAND,
  NATIVE_SHARED_CODE_INDEX_SEARCH_COMMAND,
  NATIVE_SHARED_GIT_STATUS_COMMAND,
  NATIVE_SHARED_LSP_STATUS_COMMAND,
  NATIVE_SHARED_MCP_LIST_SERVERS_COMMAND,
  NATIVE_SHARED_MEMORY_QUERY_COMMAND,
  NATIVE_SHARED_MEMORY_WRITE_COMMAND,
  type NativeCredentialDescriptorView,
  type NativeIndependentDiagnostics,
  type NativeQuotaSurface,
  type NativeSharedCodingServicesStatus,
} from "../src/services/nativeAgent";

interface ProjectRow {
  id: string;
  name: string;
  cwd: string | null;
  sessionCount: number;
  sortOrder: number;
  pinned: boolean;
}

interface CliProjectOpenPayload {
  projectId: string;
  cwd: string;
}

interface ProjectDashboardSummaryRow {
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

interface TaskRow {
  id: string;
  projectId: string | null;
  sessionId: string;
  title: string;
  titleSource: "auto" | "manual";
  status: string;
  createdAt: number;
  parentId: string | null;
  dependsOn: string[];
  sortOrder: number;
  pinned: boolean;
  archived?: boolean;
}

interface MilestoneRow {
  id: string;
  projectId: string;
  title: string;
  description: string;
  status: MilestoneStatus;
  dueDate: number | null;
  order: number;
  createdAt: number;
}

interface TaskMilestoneLinkRow {
  taskId: string;
  milestoneId: string;
}

interface ProjectRoadmapRow {
  milestones: MilestoneRow[];
  links: TaskMilestoneLinkRow[];
}

interface CodexThreadRow {
  id: string;
  title: string;
  status: string | null;
  model: string | null;
  sourceKind: string | null;
  createdAt: number | null;
  updatedAt: number | null;
  archived: boolean;
  preview: string | null;
}

/**
 * 与 @lilia/contracts 同形，display 已从持久层下线：前端用
 * `deriveTimelineDisplay()` 现算，mock 这里也只塞「事实」字段。
 */
interface AgentTimelineEvent {
  id: string;
  taskId: string;
  turnId: string | null;
  backend: "claude" | "codex";
  kind: string;
  status: string;
  title: string;
  summary: string | null;
  payload: unknown;
  createdAt: number;
  updatedAt: number;
  turnSeq: number;
  intraTurnOrder: number;
}

interface MockSlashCommand {
  id: string;
  name: string;
  title: string;
  description: string;
  source: "native" | "project";
  parameters: unknown[];
}

interface TodoRow {
  id: string;
  taskId: string;
  text: string;
  done: boolean;
  order: number;
  source: "lilia" | "agent";
  priority: "high" | "normal" | "low";
  guideStatus: "pending" | "queued" | "sent" | null;
  attachments: unknown[];
  createdAt: number;
  updatedAt: number;
}

interface AutomationNodeRow {
  id: string;
  kind: "trigger" | "agent" | "logic" | "tool" | "human";
  title: string;
  position: { x: number; y: number };
  config: Record<string, unknown>;
}

interface AutomationEdgeRow {
  id: string;
  source: string;
  target: string;
  sourceHandle?: string | null;
  targetHandle?: string | null;
}

type AutomationScopeRow = AutomationScopeFilter;

interface AutomationWorkflowRow {
  id: string;
  name: string;
  enabled: boolean;
  scope: AutomationScopeRow;
  draft: {
    nodes: AutomationNodeRow[];
    edges: AutomationEdgeRow[];
    scope: AutomationScopeRow;
  };
  publishedVersionId: string | null;
  createdAt: number;
  updatedAt: number;
}

interface AutomationRunRow {
  id: string;
  workflowId: string;
  workflowVersionId: string;
  status: "pending" | "running" | "succeeded" | "failed" | "skipped" | "waiting_user";
  trigger: {
    id: string;
    kind: string;
    projectId?: string | null;
    taskId?: string | null;
    backend?: "claude" | "codex" | null;
    eventKind?: string | null;
    automationRunId?: string | null;
    payload: Record<string, unknown>;
    createdAt: number;
  };
  scope: AutomationScopeRow;
  startedAt: number;
  finishedAt: number | null;
  error: string | null;
}

interface AutomationRunNodeStateRow {
  id: string;
  runId: string;
  nodeId: string;
  status: AutomationRunRow["status"];
  input: Record<string, unknown>;
  output: Record<string, unknown> | null;
  error: string | null;
  startedAt: number | null;
  finishedAt: number | null;
}

interface AutomationRunSummaryRow {
  id: string;
  workflowId: string;
  workflowVersionId: string;
  status: AutomationRunRow["status"];
  triggerKind: string;
  projectId?: string | null;
  taskId?: string | null;
  backend?: "claude" | "codex" | null;
  eventKind?: string | null;
  startedAt: number;
  finishedAt: number | null;
  error: string | null;
}

interface AgentTodoInput {
  content?: unknown;
  text?: unknown;
  title?: unknown;
  description?: unknown;
  status?: unknown;
  completed?: unknown;
  done?: unknown;
  priority?: unknown;
}

const baseProjects: ProjectRow[] = [
  {
    id: "lilia",
    name: "Lilia",
    cwd: "D:\\PROJECT\\workspace\\Lilia",
    sessionCount: 2,
    sortOrder: 0,
    pinned: false,
  },
  {
    id: "tools",
    name: "工具箱",
    cwd: "D:\\PROJECT\\workspace\\tools",
    sessionCount: 1,
    sortOrder: 1,
    pinned: false,
  },
];

const baseTasks: TaskRow[] = [
  {
    id: "t-001",
    projectId: "lilia",
    sessionId: "0192-lilia-0001",
    title: "接入 Claude Code 会话发现",
    titleSource: "auto",
    status: "done",
    createdAt: 1000,
    parentId: null,
    dependsOn: [],
    sortOrder: 0,
    pinned: false,
  },
  {
    id: "t-002",
    projectId: "lilia",
    sessionId: "0192-lilia-0002",
    title: "打通 tsconfig paths 搜索",
    titleSource: "auto",
    status: "running",
    createdAt: 2000,
    parentId: "t-001",
    dependsOn: ["t-001"],
    sortOrder: 1,
    pinned: false,
  },
  {
    id: "t-003",
    projectId: "tools",
    sessionId: "0192-tools-0001",
    title: "整理窗口快捷键",
    titleSource: "auto",
    status: "waiting",
    createdAt: 3000,
    parentId: null,
    dependsOn: [],
    sortOrder: 0,
    pinned: false,
  },
  {
    id: "o-001",
    projectId: null,
    sessionId: "0192-orphan-0001",
    title: "随手问问 Claude：tsconfig paths",
    titleSource: "auto",
    status: "running",
    createdAt: 4000,
    parentId: null,
    dependsOn: [],
    sortOrder: 0,
    pinned: false,
  },
];

const baseMilestones: MilestoneRow[] = [
  {
    id: "m-001",
    projectId: "lilia",
    title: "首发可用路线图",
    description: "",
    status: "in-progress",
    dueDate: null,
    order: 0,
    createdAt: 5000,
  },
];

const baseTaskMilestoneLinks: TaskMilestoneLinkRow[] = [
  { taskId: "t-001", milestoneId: "m-001" },
  { taskId: "t-002", milestoneId: "m-001" },
];

const baseCodexThreads: CodexThreadRow[] = [
  {
    id: "thread-1",
    title: "打通 tsconfig paths 搜索",
    status: null,
    model: "gpt-5.5",
    sourceKind: "lilia",
    createdAt: 10_000,
    updatedAt: 20_000,
    archived: false,
    preview: "最近在检查路径别名和上下文搜索。",
  },
  {
    id: "thread-2",
    title: "整理 Codex 会话管理",
    status: "idle",
    model: "gpt-5.4",
    sourceKind: "app-server",
    createdAt: 30_000,
    updatedAt: 40_000,
    archived: false,
    preview: "讨论设置页中的会话维护入口。",
  },
  {
    id: "thread-archived",
    title: "已归档的旧会话",
    status: "completed",
    model: "gpt-5.4-mini",
    sourceKind: "app-server",
    createdAt: 5_000,
    updatedAt: 6_000,
    archived: true,
    preview: "旧的 Codex thread。",
  },
];

let projects: ProjectRow[] = [];
let tasks: TaskRow[] = [];
let milestones: MilestoneRow[] = [];
let taskMilestoneLinks: TaskMilestoneLinkRow[] = [];
let codexThreads: CodexThreadRow[] = [];
let codexTaskSessions: Record<string, string> = {};
let cleanedCodexThreads: string[] = [];
let timelineEvents: Record<string, AgentTimelineEvent[]> = {};
let todosByTaskId: Record<string, TodoRow[]> = {};
let todoSeq = 0;
let automations: AutomationWorkflowRow[] = [];
let automationRuns: AutomationRunRow[] = [];
let automationRunNodes: Record<string, AutomationRunNodeStateRow[]> = {};
let automationVersionSeq = 0;
let automationRunSeq = 0;
let pendingCliProjectOpen: CliProjectOpenPayload | null = null;

function defaultAutomationScope(): AutomationScopeRow {
  return normalizeAutomationScope(null);
}

function cloneAutomationWorkflow(row: AutomationWorkflowRow): AutomationWorkflowRow {
  return {
    ...row,
    scope: { ...row.scope, projectIds: [...row.scope.projectIds], taskStatuses: [...row.scope.taskStatuses], backends: [...row.scope.backends], eventKinds: [...row.scope.eventKinds] },
    draft: {
      nodes: row.draft.nodes.map((node) => ({
        ...node,
        position: { ...node.position },
        config: { ...node.config },
      })),
      edges: row.draft.edges.map((edge) => ({ ...edge })),
      scope: {
        ...row.draft.scope,
        projectIds: [...row.draft.scope.projectIds],
        taskStatuses: [...row.draft.scope.taskStatuses],
        backends: [...row.draft.scope.backends],
        eventKinds: [...row.draft.scope.eventKinds],
      },
    },
  };
}

function cloneAutomationRun(row: AutomationRunRow): AutomationRunRow {
  return {
    ...row,
    trigger: {
      ...row.trigger,
      payload: { ...row.trigger.payload },
    },
    scope: {
      ...row.scope,
      projectIds: [...row.scope.projectIds],
      taskStatuses: [...row.scope.taskStatuses],
      backends: [...row.scope.backends],
      eventKinds: [...row.scope.eventKinds],
    },
  };
}

function cloneAutomationRunSummary(row: AutomationRunRow): AutomationRunSummaryRow {
  return {
    id: row.id,
    workflowId: row.workflowId,
    workflowVersionId: row.workflowVersionId,
    status: row.status,
    triggerKind: row.trigger.kind,
    projectId: row.trigger.projectId ?? null,
    taskId: row.trigger.taskId ?? null,
    backend: row.trigger.backend ?? null,
    eventKind: row.trigger.eventKind ?? null,
    startedAt: row.startedAt,
    finishedAt: row.finishedAt,
    error: row.error,
  };
}

function cloneAutomationRunNode(row: AutomationRunNodeStateRow): AutomationRunNodeStateRow {
  return {
    ...row,
    input: { ...row.input },
    output: row.output ? { ...row.output } : null,
  };
}
let chatRunning: Record<string, boolean> = {};
let chatQueued: Record<string, Array<Record<string, unknown>>> = {};
let chatInterruptRollbacks: Record<string, Record<string, unknown> | null> = {};
let runtimeSnapshotOverrides: Record<string, Record<string, unknown>> = {};
let processSessionCommands: Array<{ taskId: string; command: Record<string, unknown> }> = [];
let nextChatSendError: string | null = null;
let nextAgentInteractionResponseError: string | null = null;
let clipboardFilePaths: string[] = [];
let clipboardImageSeq = 0;
let clipboardTextSeq = 0;
let liliaIabSeq = 0;
let nextLiliaIabDelivery: "runner" | "message" = "message";
let activeBackend: "claude" | "codex" = "claude";
type MockProviderBackend = "claude" | "codex";
type MockRouterMode = RouterMode;
type MockProviderConfig = {
  backend: MockProviderBackend;
  baseUrl: string | null;
  hasApiKey: boolean;
};
let providerConfigs: Record<MockProviderBackend, MockProviderConfig> = {
  claude: { backend: "claude", baseUrl: null, hasApiKey: false },
  codex: { backend: "codex", baseUrl: null, hasApiKey: false },
};
let routerModes: Record<MockProviderBackend, MockRouterMode> = {
  claude: DEFAULT_ROUTER_MODE_BY_BACKEND.claude,
  codex: DEFAULT_ROUTER_MODE_BY_BACKEND.codex,
};
let nodeAvailable = true;
const remoteControlBridgeUrl = "http://127.0.0.1:41478";
const remoteControlRecentSeenMs = 2 * 60 * 1000;
let remoteControlEnabled = false;
let mockNativeCredentials: NativeCredentialDescriptorView[] = [];
let mockNativeCredentialSeq = 0;
let remoteControlKeepAwakeEnabled = true;
let remoteControlPcName = "Lilia Test PC";
let remoteControlTicket: Record<string, unknown> | null = null;
let remoteControlDevices: Record<string, unknown>[] = [];

function mockNativeProviders() {
  return [
    {
      providerId: "mutsuki.credential.openai",
      displayName: "OpenAI",
      protocolFamilies: ["openai-compatible"],
      supportedKinds: ["api_key", "generated_api_key"],
      supportsBrowserLogin: false,
      enterpriseIdentity: false,
    },
    {
      providerId: "mutsuki.credential.anthropic",
      displayName: "Anthropic",
      protocolFamilies: ["anthropic-messages"],
      supportedKinds: ["api_key", "generated_api_key"],
      supportsBrowserLogin: false,
      enterpriseIdentity: false,
    },
  ];
}

function mockNativeDiagnostics(): NativeIndependentDiagnostics {
  const activeCount = mockNativeCredentials.filter((item) => item.status === "active").length;
  return {
    credential: {
      brokerReady: true,
      providerCount: mockNativeProviders().length,
      credentialCount: mockNativeCredentials.length,
      activeCount,
      unavailableCount: mockNativeCredentials.length - activeCount,
      hasUsableModelCredential: activeCount > 0,
      credentials: mockNativeCredentials.map((item) => ({ ...item })),
    },
    runtimeBackend: "native-agentkit",
    runtimeReady: true,
    officialAgentServer: false,
    nodeRunnerDefault: false,
    profileId: activeCount > 0 ? "native-coding" : null,
    profileHasCredentialRefs: activeCount > 0,
    credentialAndRuntimeIndependent: true,
    liveModelAdapterDrivesTurn: activeCount > 0,
  };
}

function mockNativeQuotaSurface(): NativeQuotaSurface {
  const diagnostics = mockNativeDiagnostics();
  return {
    source: "credential-broker",
    localUsageAvailable: true,
    localUsageNote:
      "本地 Token/成本统计来自产品 usage 聚合（ModelUsage/ModelCost），不是 Provider 远程额度 API。",
    remoteQuotaApi: "unavailable",
    remoteQuotaNote:
      "AgentKit 尚未提供 Provider rate-limit/quota status service；远程剩余额度不可用，禁止伪造数字。",
    subscriptionNotEquatedToApiQuota: true,
    credential: diagnostics.credential,
    providers: [
      {
        providerId: "mutsuki.credential.openai",
        displayName: "OpenAI / OpenAI-compatible",
        adapterId: "openai-compatible",
        quotaApi: "unavailable",
        credentialHealth: diagnostics.credential.credentials.some(
          (item) => item.providerId === "mutsuki.credential.openai" && item.status === "active",
        )
          ? "active"
          : "none",
        hasUsableCredential: diagnostics.credential.credentials.some(
          (item) =>
            item.providerId === "mutsuki.credential.openai" &&
            item.status === "active" &&
            item.modelInference,
        ),
        knownLimits: [
          {
            kind: "context_window",
            label: "上下文窗口（tokens）",
            value: 128_000,
            note: "来自 openai-compatible Adapter ModelCapability，非远程剩余额度。",
          },
        ],
        note: "远程额度 API 不可用；下列为 Adapter 能力上限，不是剩余额度。",
      },
      {
        providerId: "mutsuki.credential.anthropic",
        displayName: "Anthropic",
        adapterId: "anthropic-messages",
        quotaApi: "unavailable",
        credentialHealth: diagnostics.credential.credentials.some(
          (item) => item.providerId === "mutsuki.credential.anthropic" && item.status === "active",
        )
          ? "active"
          : "none",
        hasUsableCredential: diagnostics.credential.credentials.some(
          (item) =>
            item.providerId === "mutsuki.credential.anthropic" &&
            item.status === "active" &&
            item.modelInference,
        ),
        knownLimits: [
          {
            kind: "context_window",
            label: "上下文窗口（tokens）",
            value: 200_000,
            note: "来自 anthropic-messages Adapter ModelCapability，非远程剩余额度。",
          },
        ],
        note: "远程额度 API 不可用；下列为 Adapter 能力上限，不是剩余额度。",
      },
    ],
  };
}

function mockNativeCredentialFromInput(args: Record<string, unknown>): NativeCredentialDescriptorView {
  const input = (args.input && typeof args.input === "object"
    ? args.input
    : {}) as Record<string, unknown>;
  mockNativeCredentialSeq += 1;
  const view: NativeCredentialDescriptorView = {
    credentialId: `cred-${mockNativeCredentialSeq}`,
    revision: 1,
    providerId: String(input.providerId ?? "mutsuki.credential.openai"),
    kind: (input.kind as NativeCredentialDescriptorView["kind"]) || "api_key",
    status: "active",
    accountLabel: typeof input.accountLabel === "string" ? input.accountLabel : null,
    source: typeof input.source === "string" ? input.source : null,
    modelInference: true,
  };
  mockNativeCredentials = [view, ...mockNativeCredentials];
  return view;
}

function mockRemoteControlState() {
  if (!remoteControlEnabled) return "disabled";
  if (remoteControlTicket) return "pairing";
  const cutoff = Date.now() - remoteControlRecentSeenMs;
  return remoteControlDevices.some((device) =>
    device.trusted === true &&
    device.revokedAt == null &&
    typeof device.lastSeenAt === "number" &&
    device.lastSeenAt >= cutoff
  )
    ? "connected"
    : "listening";
}

function mockRemoteControlStatus() {
  return {
    hostEnabled: remoteControlEnabled,
    state: mockRemoteControlState(),
    pcName: remoteControlPcName,
    keepAwakeEnabled: remoteControlKeepAwakeEnabled,
    endpoint: remoteControlEnabled
      ? { endpointId: "mock-pc-endpoint", relayUrl: null, directAddresses: [] }
      : null,
    activeTicket: remoteControlTicket ? { ...remoteControlTicket } : null,
    trustedDevices: remoteControlDevices.map((device) => ({ ...device })),
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

function createMockRemoteControlTicket() {
  return {
    id: "mock-ticket",
    pcName: remoteControlPcName,
    pcEndpoint: { endpointId: "mock-pc-endpoint", relayUrl: null, directAddresses: [] },
    protocolVersion: 1,
    challenge: "mock-challenge",
    expiresAt: Date.now() + 600_000,
    bridgeUrl: remoteControlBridgeUrl,
    pairingUri: `lilia-remote://pair?v=1&ticket=mock-ticket&challenge=mock-challenge&endpoint=mock-pc-endpoint&name=${encodeURIComponent(remoteControlPcName)}&bridge=${encodeURIComponent(remoteControlBridgeUrl)}`,
  };
}

function createMockLiliaIabSnapshot(args: Record<string, unknown>) {
  const taskId = String(args.taskId);
  liliaIabSeq += 1;
  const path = `C:\\Users\\mock\\.lilia\\cache\\iab-snapshots\\iab-${liliaIabSeq}.png`;
  return {
    taskId,
    url: "https://example.com/debug",
    title: "Debug Page",
    note: typeof args.note === "string" && args.note.trim() ? args.note.trim() : null,
    capturedAt: Date.now(),
    screenshotPath: path,
    screenshotAttachment: {
      id: `att-iab-${liliaIabSeq}`,
      name: "IAB 截图.png",
      path,
      kind: "file",
      size: 128,
      exists: true,
      mime: "image/png",
      directory: null,
    },
    status: "captured",
    warning: null,
  };
}
let codexAppServerStatus = {
  version: "codex-cli 0.136.0",
  installPath: null as string | null,
  managed: false,
  available: true,
  supportsRequiredProtocol: true,
  failureKind: null as
    | "missingCli"
    | "appServerUnavailable"
    | "experimentalApiUnsupported"
    | "providerIncompatible"
    | null,
  issues: [] as string[],
  latestVersion: null as string | null,
  updateAvailable: false,
  releaseNotes: [] as string[],
  updateError: null as string | null,
  updateState: "idle" as "idle" | "available" | "downloading" | "ready" | "switching" | "failed",
  preparedVersion: null as string | null,
  updateProgressPercent: null as number | null,
};
let composerStateHandler: ((taskId: string) => unknown | Promise<unknown>) | null = null;
type MockHookTrustState = "unknown" | "required" | "managed" | "n_a";
interface MockHookSourceSummary {
  id: string;
  backend: "claude" | "codex";
  scope: "managed" | "user" | "project" | "local" | "plugin" | "system";
  format: string;
  name: string;
  path: string;
  exists: boolean;
  editable: boolean;
  managed: boolean;
  enabled: boolean;
  handlerCount: number;
  warnings: string[];
  limitations: string[];
  trustState: MockHookTrustState;
  description?: string | null;
}
interface MockHookHandlerView {
  id: string;
  event: string;
  matcher: string | null;
  type: string;
  command: string | null;
  commandWindows: string | null;
  timeoutSeconds: number | null;
  statusMessage: string | null;
  supported: boolean;
  executable: boolean;
  groupAdvancedJson: string | null;
  advancedJson: string | null;
  warnings: string[];
}
interface MockHookDocumentView {
  source: MockHookSourceSummary;
  handlers: MockHookHandlerView[];
  rawDocument: string | null;
  rawFormat: "json" | "toml" | "text";
  warnings: string[];
  limitations: string[];
}
const CODEX_MIXED_SOURCE_WARNING = "同一层同时存在 hooks.json 与 inline [hooks]；Codex 会同时加载两者。";
const CODEX_PROMPT_WARNING = "type=prompt 当前上游解析但不执行。";
const CODEX_INLINE_LIMITATIONS = [
  "当前来源为只读 inline [hooks]，Lilia 不会自动重写 config.toml。",
  "如需编辑，请改用 hooks.json 或直接手工维护 TOML。",
];
const CODEX_REQUIREMENTS_LIMITATIONS = [
  "当前来源来自 requirements.toml，仅用于展示约束与托管 hooks。",
  "allow_managed_hooks_only = true：仅允许托管 hooks。",
  "requirements.toml 约束 features.hooks = false",
];
const baseClaudePlugins = [{
  backend: "claude",
  scope: "user",
  name: "demo-plugin",
  description: "测试用 Claude plugin",
  version: "1.0.0",
  enabled: true,
  path: "C:\\Users\\mock\\.claude\\plugins\\demo-plugin",
}];
const baseClaudeMcpServers = [{
  backend: "claude",
  name: "weather",
  command: "node",
  args: ["weather-mcp.js"],
  envKeys: ["WEATHER_TOKEN"],
  enabled: true,
  editable: true,
  transport: "stdio",
}];
const baseCodexMcpServers = [
  {
    backend: "codex",
    name: "mock-mcp",
    command: "node",
    args: ["mock-mcp.js"],
    envKeys: ["MOCK_TOKEN"],
    enabled: true,
    transport: "stdio",
    editable: true,
  },
  {
    backend: "codex",
    name: "remote-mcp",
    command: "",
    args: [],
    envKeys: [],
    enabled: true,
    transport: "http",
    editable: false,
  },
];
let claudePlugins = baseClaudePlugins.map((plugin) => ({ ...plugin }));
let claudeMcpServers = baseClaudeMcpServers.map((server) => ({
  ...server,
  args: [...server.args],
  envKeys: [...server.envKeys],
}));
let codexMcpServers = baseCodexMcpServers.map((server) => ({
  ...server,
  args: [...server.args],
  envKeys: [...server.envKeys],
}));

function makeHookSource(
  source: Partial<MockHookSourceSummary> & Pick<MockHookSourceSummary, "id" | "backend" | "scope" | "format" | "name" | "path">,
): MockHookSourceSummary {
  return {
    exists: true,
    editable: false,
    managed: false,
    enabled: false,
    handlerCount: 0,
    warnings: [],
    limitations: [],
    trustState: "unknown",
    description: null,
    ...source,
  };
}

function makeHookHandler(
  handler: Partial<MockHookHandlerView> & Pick<MockHookHandlerView, "id" | "event" | "type">,
): MockHookHandlerView {
  return {
    matcher: null,
    command: null,
    commandWindows: null,
    timeoutSeconds: null,
    statusMessage: null,
    supported: true,
    executable: true,
    groupAdvancedJson: null,
    advancedJson: null,
    warnings: [],
    ...handler,
  };
}

function buildHookRawDocument(handlers: MockHookHandlerView[]) {
  const grouped = handlers.reduce<Record<string, Array<Record<string, unknown>>>>((acc, handler) => {
    const event = handler.event.trim();
    if (!event) return acc;
    acc[event] ??= [];
    const row: Record<string, unknown> = {
      type: handler.type,
    };
    if (handler.matcher) row.matcher = handler.matcher;
    if (handler.command) row.command = handler.command;
    if (handler.commandWindows) row.commandWindows = handler.commandWindows;
    if (handler.timeoutSeconds != null) row.timeout = handler.timeoutSeconds;
    if (handler.statusMessage) row.statusMessage = handler.statusMessage;
    if (handler.groupAdvancedJson) Object.assign(row, JSON.parse(handler.groupAdvancedJson));
    if (handler.advancedJson) Object.assign(row, JSON.parse(handler.advancedJson));
    acc[event].push({ hooks: [row], ...(handler.matcher ? { matcher: handler.matcher } : {}) });
    return acc;
  }, {});
  return `${JSON.stringify({ hooks: grouped }, null, 2)}\n`;
}

function cloneHookDocument(document: MockHookDocumentView): MockHookDocumentView {
  return structuredClone(document);
}

function syncHookDocument(document: MockHookDocumentView) {
  document.source.handlerCount = document.handlers.length;
  document.source.enabled = document.source.exists && document.handlers.length > 0;
}

function makeHookDocument({
  source,
  handlers = [],
  rawDocument,
  rawFormat = "json",
  warnings = [],
  limitations = [],
}: {
  source: MockHookSourceSummary;
  handlers?: MockHookHandlerView[];
  rawDocument?: string | null;
  rawFormat?: MockHookDocumentView["rawFormat"];
  warnings?: string[];
  limitations?: string[];
}): MockHookDocumentView {
  const document: MockHookDocumentView = {
    source,
    handlers,
    rawDocument: rawDocument ?? (rawFormat === "json" ? buildHookRawDocument(handlers) : null),
    rawFormat,
    warnings,
    limitations,
  };
  syncHookDocument(document);
  return document;
}

function initialHookDocuments(): Record<string, MockHookDocumentView> {
  const claudeUser = makeHookDocument({
    source: makeHookSource({
      id: "claude-user",
      backend: "claude",
      scope: "user",
      format: "claude_settings_json",
      name: "Claude User Hooks",
      path: "C:\\Users\\mock\\.claude\\settings.json",
      editable: true,
      exists: true,
      enabled: true,
      handlerCount: 1,
      description: "~/.claude/settings.json 中的 hooks",
    }),
    handlers: [
      makeHookHandler({
        id: "claude-user-1",
        event: "PostToolUse",
        matcher: "Bash",
        type: "command",
        command: "node global-hook.js",
        statusMessage: "Running global Claude hook",
        timeoutSeconds: 30,
      }),
    ],
  });

  const claudeProject = makeHookDocument({
    source: makeHookSource({
      id: "claude-project",
      backend: "claude",
      scope: "project",
      format: "claude_settings_json",
      name: "Claude Project Hooks",
      path: "D:\\PROJECT\\workspace\\Lilia\\.claude\\settings.json",
      editable: true,
      exists: true,
      enabled: true,
      handlerCount: 1,
      description: ".claude/settings.json 中的 hooks",
    }),
    handlers: [
      makeHookHandler({
        id: "claude-project-1",
        event: "PostToolUse",
        matcher: "Edit",
        type: "command",
        command: "node project-hook.js",
        advancedJson: "{\n  \"env\": {\n    \"MODE\": \"project\"\n  }\n}",
      }),
    ],
  });

  const claudeLocal = makeHookDocument({
    source: makeHookSource({
      id: "claude-local",
      backend: "claude",
      scope: "local",
      format: "claude_settings_json",
      name: "Claude Local Hooks",
      path: "D:\\PROJECT\\workspace\\Lilia\\.claude\\settings.local.json",
      editable: true,
      exists: false,
      enabled: false,
      handlerCount: 0,
      description: ".claude/settings.local.json 中的 hooks",
    }),
    rawDocument: null,
  });

  const claudeToolsProject = makeHookDocument({
    source: makeHookSource({
      id: "claude-project-tools",
      backend: "claude",
      scope: "project",
      format: "claude_settings_json",
      name: "Claude Project Hooks",
      path: "D:\\\\PROJECT\\\\workspace\\\\tools\\\\.claude\\\\settings.json",
      editable: true,
      exists: true,
      enabled: true,
      handlerCount: 1,
      description: ".claude/settings.json 中的 hooks",
    }),
    handlers: [
      makeHookHandler({
        id: "claude-project-tools-1",
        event: "PostToolUse",
        matcher: "Edit",
        type: "command",
        command: "node tools-project-hook.js",
      }),
    ],
  });

  const claudeToolsLocal = makeHookDocument({
    source: makeHookSource({
      id: "claude-local-tools",
      backend: "claude",
      scope: "local",
      format: "claude_settings_json",
      name: "Claude Local Hooks",
      path: "D:\\\\PROJECT\\\\workspace\\\\tools\\\\.claude\\\\settings.local.json",
      editable: true,
      exists: false,
      enabled: false,
      handlerCount: 0,
      description: ".claude/settings.local.json 中的 hooks",
    }),
    rawDocument: null,
  });

  const claudeManaged = makeHookDocument({
    source: makeHookSource({
      id: "claude-managed",
      backend: "claude",
      scope: "managed",
      format: "managed_settings",
      name: "Claude Managed Hooks",
      path: "C:\\Program Files\\ClaudeCode\\managed-settings.json",
      editable: false,
      managed: true,
      exists: true,
      enabled: true,
      handlerCount: 1,
      trustState: "managed",
    }),
    handlers: [
      makeHookHandler({
        id: "claude-managed-1",
        event: "PreToolUse",
        matcher: "Bash",
        type: "command",
        command: "node managed-hook.js",
      }),
    ],
    rawDocument: "{\n  \"hooks\": {}\n}\n",
    limitations: ["托管 settings 来源不可在 Lilia 中修改。"],
  });

  const codexUser = makeHookDocument({
    source: makeHookSource({
      id: "codex-user-hooks",
      backend: "codex",
      scope: "user",
      format: "codex_hooks_json",
      name: "Codex User Hooks",
      path: "C:\\Users\\mock\\.codex\\hooks.json",
      editable: true,
      exists: true,
      enabled: true,
      handlerCount: 1,
      trustState: "required",
      warnings: [CODEX_MIXED_SOURCE_WARNING],
    }),
    handlers: [
      makeHookHandler({
        id: "codex-user-1",
        event: "PreToolUse",
        matcher: "Bash",
        type: "command",
        command: "node codex-hook.js",
        commandWindows: "powershell -File codex-hook.ps1",
        groupAdvancedJson: "{\n  \"shared\": true\n}",
      }),
    ],
    warnings: [CODEX_MIXED_SOURCE_WARNING],
  });

  const codexUserConfig = makeHookDocument({
    source: makeHookSource({
      id: "codex-user-config",
      backend: "codex",
      scope: "user",
      format: "codex_config_toml",
      name: "Codex User Config Hooks",
      path: "C:\\Users\\mock\\.codex\\config.toml",
      editable: false,
      exists: true,
      enabled: true,
      handlerCount: 1,
      trustState: "required",
      warnings: [CODEX_MIXED_SOURCE_WARNING],
    }),
    handlers: [
      makeHookHandler({
        id: "codex-user-config-1",
        event: "PreToolUse",
        matcher: "Read",
        type: "prompt",
        supported: false,
        executable: false,
        warnings: [CODEX_PROMPT_WARNING],
      }),
    ],
    rawDocument: "[hooks]\n",
    rawFormat: "toml",
    warnings: [CODEX_PROMPT_WARNING],
    limitations: CODEX_INLINE_LIMITATIONS,
  });

  const codexProject = makeHookDocument({
    source: makeHookSource({
      id: "codex-project-hooks",
      backend: "codex",
      scope: "project",
      format: "codex_hooks_json",
      name: "Codex Project Hooks",
      path: "D:\\PROJECT\\workspace\\Lilia\\.codex\\hooks.json",
      editable: true,
      exists: false,
      enabled: false,
      handlerCount: 0,
      trustState: "required",
    }),
    rawDocument: null,
  });

  const codexRequirements = makeHookDocument({
    source: makeHookSource({
      id: "codex-project-requirements",
      backend: "codex",
      scope: "managed",
      format: "requirements_toml",
      name: "Codex Project Requirements",
      path: "D:\\PROJECT\\workspace\\Lilia\\.codex\\requirements.toml",
      editable: false,
      managed: true,
      exists: true,
      enabled: false,
      handlerCount: 0,
      trustState: "managed",
    }),
    rawDocument: "allow_managed_hooks_only = true\n[features]\nhooks = false\n",
    rawFormat: "toml",
    limitations: CODEX_REQUIREMENTS_LIMITATIONS,
  });

  return {
    [claudeUser.source.id]: claudeUser,
    [claudeProject.source.id]: claudeProject,
    [claudeLocal.source.id]: claudeLocal,
    [claudeToolsProject.source.id]: claudeToolsProject,
    [claudeToolsLocal.source.id]: claudeToolsLocal,
    [claudeManaged.source.id]: claudeManaged,
    [codexUser.source.id]: codexUser,
    [codexUserConfig.source.id]: codexUserConfig,
    [codexProject.source.id]: codexProject,
    [codexRequirements.source.id]: codexRequirements,
  };
}

let hookDocuments = initialHookDocuments();

function mockProjectCwdFromPluginPath(path: string) {
  const normalized = path.replace(/\\/g, "/").replace(/\/+/g, "/").replace(/\/+$/, "");
  const lower = normalized.toLocaleLowerCase();
  for (const marker of [
    "/.claude/skills/",
    "/.claude/settings.json",
    "/.claude/settings.local.json",
    "/.codex/hooks.json",
    "/.codex/config.toml",
    "/.codex/requirements.toml",
  ]) {
    const index = lower.indexOf(marker);
    if (index > 0) return normalized.slice(0, index);
  }
  return null;
}

function knownMockProjectCwdFromPluginPath(path: string) {
  const ownerCwd = mockProjectCwdFromPluginPath(path);
  if (!ownerCwd) return null;
  return baseProjects.some((project) => project.cwd && projectCwdKey(project.cwd) === projectCwdKey(ownerCwd))
    ? ownerCwd
    : null;
}

function matchesMockProjectCwd(path: string, projectCwd: string | null) {
  const ownerCwd = knownMockProjectCwdFromPluginPath(path);
  if (!projectCwd) return ownerCwd === null;
  return ownerCwd === null || projectCwdKey(ownerCwd) === projectCwdKey(projectCwd);
}

function hookSourcesOverview(projectCwd: string | null) {
  return Object.values(hookDocuments).flatMap((document) => {
    syncHookDocument(document);
    if (!matchesMockProjectCwd(document.source.path, projectCwd)) return [];
    return [structuredClone(document.source)];
  });
}

function editableHookSourceFor(backend: string, scope: string, projectCwd: string | null) {
  return Object.values(hookDocuments).find((document) =>
    document.source.backend === backend &&
    document.source.scope === scope &&
    document.source.editable &&
    matchesMockProjectCwd(document.source.path, projectCwd),
  );
}

function mcpServersForBackend(backend: string) {
  return backend === "claude" ? claudeMcpServers : codexMcpServers;
}

function updateMcpServersForBackend(
  backend: string,
  updater: (servers: typeof claudeMcpServers) => typeof claudeMcpServers,
) {
  if (backend === "claude") claudeMcpServers = updater(claudeMcpServers);
  else codexMcpServers = updater(codexMcpServers);
}

function defaultAgentInteractionSettings(): AgentInteractionSettings {
  return normalizeAgentInteractionSettings(null);
}

let agentInteractionSettings = defaultAgentInteractionSettings();
let customSubagents = [
  {
    id: "reviewer",
    name: "Reviewer",
    description: "检查风险与回归",
    instruction: "Review code changes, identify risk, and summarize findings.",
    enabled: true,
  },
];
let assistantAIConfig = {
  baseUrl: null as string | null,
  apiKey: null as string | null,
  model: null as string | null,
  modelPool: [
    { id: "gpt-5.5", label: "GPT 5.5", source: "remote" as const, backend: "codex" as const },
  ] as Array<{ id: string; label: string; source: "remote" | "legacy"; backend: "codex" | "claude" }>,
  codexAccountSparkEnabled: false,
  hasApiKey: true,
};
let modelFeatureSettings = {
  chat: { light: null as string | null, normal: null as string | null, deep: null as string | null },
  title: null as string | null,
  suggestion: null as string | null,
  promptRouter: null as string | null,
  promptOptimize: null as string | null,
  autoTurnDecision: null as string | null,
};
let conversationSuggestionSettings = {
  enabled: true,
  source: "assistant-ai" as "assistant-ai" | "provider",
};
let conversationSuggestionsOverride: unknown[] | null = null;
let conversationSuggestionSourcesOverride: unknown | null = null;
let nextConversationSuggestionsError: string | null = null;
let conversationSuggestionsDelayMs = 0;
let runtimeSnapshotDelayMs = 0;
let projectSettings = {
  cloneParentDir: null as string | null,
  codexDefaults: null as unknown,
  githubBinding: null as Record<string, unknown> | null,
  worktree: {
    defaultMode: "current",
    parentDir: null as string | null,
    autoInstructions: [
      "This task is running inside a dedicated git worktree managed by Lilia.",
      "Keep changes scoped to this task and create commits in the worktree before requesting merge/archive.",
    ].join("\n"),
    cleanupOnArchive: true,
  },
};
let taskWorktrees: Record<string, Record<string, unknown>> = {};
let mockPickedFolderPath: string | null = "C:\\Users\\mock";
let githubBindingStatus = {
  state: "unbound" as "unbound" | "bound",
  clientIdConfigured: true,
  clientIdSource: "bundled" as "none" | "bundled" | "custom",
  binding: null as Record<string, unknown> | null,
};
let githubDeviceFlowStart = {
  deviceCode: "device-code-1",
  userCode: "ABCD-EFGH",
  verificationUri: "https://github.com/login/device",
  expiresAt: Date.now() + 15 * 60 * 1000,
  intervalSeconds: 1,
};
let githubDeviceFlowPollQueue: Array<Record<string, unknown>> = [];
let githubRepoPages: Record<number, { items: unknown[]; nextPage: number | null }> = {
  1: { items: [], nextPage: null },
};
let githubReposError: string | null = null;
let popupWindowSettings: { shortcut: string | null } = { shortcut: null };
let nextPopupSettingsError: string | null = null;
let popupLastProjectId: string | null = null;
let eventHandlers: Record<string, Array<(event: { payload: unknown }) => void>> = {};
let listenFailureByEvent: Record<string, string> = {};
let webviewDragDropHandlers: Array<(event: { payload: unknown }) => void> = [];
let projectPinUpdater: ((projectId: string, pinned: boolean) => void) | null = null;
let windowScaleFactor = 1;
let agentTimelineDelayMs = 0;
let quotaUsageStatsOverride: Record<string, unknown> | null = null;
let codexAccountQuotaStatusOverride: Record<string, unknown> | null = null;

function cloneProject(row: ProjectRow): ProjectRow {
  return { ...row };
}

function projectNameFromPath(path: string): string {
  const parts = path.trim().replace(/[\\/]+$/, "").split(/[\\/]/).filter(Boolean);
  return parts.at(-1) || "未命名项目";
}

function projectCwdKey(path: string): string {
  return path.trim().replace(/[\\/]+$/, "").replace(/[\\/]+/g, "\\").toLowerCase();
}

function isMockDirectoryPath(path: string): boolean {
  return path.trim().length > 0 && !/\.[^\\/]+$/.test(path);
}

function ensureMockFolderProjects(paths: string[]): ProjectRow[] {
  const seen = new Set<string>();
  const ensured: ProjectRow[] = [];
  for (const path of paths) {
    if (!isMockDirectoryPath(path)) continue;
    const key = projectCwdKey(path);
    if (seen.has(key)) continue;
    seen.add(key);
    const existing = projects.find((project) => project.cwd && projectCwdKey(project.cwd) === key);
    if (existing) {
      ensured.push(existing);
      continue;
    }
    const project: ProjectRow = {
      id: `p-${projects.length + 1}`,
      name: projectNameFromPath(path),
      cwd: path,
      sessionCount: 0,
      sortOrder: projects.length,
      pinned: false,
    };
    projects.push(project);
    ensured.push(project);
  }
  return ensured.map(cloneProject);
}

function cloneTask(row: TaskRow): TaskRow {
  return { ...row, dependsOn: [...row.dependsOn] };
}

function createProjectDashboardSummaries(): ProjectDashboardSummaryRow[] {
  return projects
    .map((project) => {
      const projectTasks = tasks.filter((task) => task.projectId === project.id && task.archived !== true);
      const statusCounts = countProjectTaskStatuses(projectTasks);
      const latestTaskAt = projectTasks.reduce<number | null>(
        (latest, task) => latest === null ? task.createdAt : Math.max(latest, task.createdAt),
        null,
      );
      const latestTimelineAt = projectTasks.reduce<number | null>((latest, task) => {
        const taskLatest = (timelineEvents[task.id] ?? []).reduce<number | null>(
          (eventLatest, event) => eventLatest === null
            ? event.updatedAt
            : Math.max(eventLatest, event.updatedAt),
          null,
        );
        if (taskLatest === null) return latest;
        return latest === null ? taskLatest : Math.max(latest, taskLatest);
      }, null);
      const recentActivityAt = [latestTaskAt, latestTimelineAt]
        .filter((value): value is number => value !== null)
        .reduce<number | null>((latest, value) => latest === null ? value : Math.max(latest, value), null);
      const usage = project.id === "lilia"
        ? { totalTokens: 4200, knownCostUsd: 0.15, costRecordCount: 1, usageRecordCount: 2 }
        : { totalTokens: 0, knownCostUsd: null, costRecordCount: 0, usageRecordCount: 0 };
      const dashboardCounts = deriveProjectDashboardCounts(statusCounts);
      return {
        id: project.id,
        name: project.name,
        cwd: project.cwd,
        pinned: project.pinned,
        taskCount: projectTasks.length,
        sessionCount: new Set(projectTasks.map((task) => task.sessionId)).size,
        statusCounts,
        ...dashboardCounts,
        recentActivityAt,
        ...usage,
      };
    })
    .sort((a, b) => Number(b.pinned) - Number(a.pinned));
}

function cloneMilestone(row: MilestoneRow): MilestoneRow {
  return { ...row };
}

function cloneTaskMilestoneLink(row: TaskMilestoneLinkRow): TaskMilestoneLinkRow {
  return { ...row };
}

function dayStart(timestamp: number) {
  const dayMs = 86_400_000;
  return Math.floor(timestamp / dayMs) * dayMs;
}

function createMockQuotaUsageStats(input: Record<string, unknown> = {}) {
  const days = input.days === 30 ? 30 : 7;
  const backend = input.backend === "claude" || input.backend === "codex" ? input.backend : "all";
  const rangeEnd = dayStart(Date.now()) + 86_400_000;
  const rangeStart = rangeEnd - days * 86_400_000;
  const daily = Array.from({ length: days }, (_, index) => {
    const day = rangeStart + index * 86_400_000;
    const active = index >= days - 3;
    const inputTokens = active ? (index + 1) * 100 : 0;
    const outputTokens = active ? (index + 1) * 40 : 0;
    const cacheReadTokens = active ? index * 12 : 0;
    const cacheCreationTokens = active ? index * 5 : 0;
    return {
      dayStart: day,
      inputTokens,
      outputTokens,
      cacheReadTokens,
      cacheCreationTokens,
      totalTokens: inputTokens + outputTokens + cacheReadTokens + cacheCreationTokens,
      knownCostUsd: active && backend !== "codex" ? 0.01 * (index + 1) : null,
      costRecordCount: active && backend !== "codex" ? 1 : 0,
      recordCount: active ? 1 : 0,
    };
  });
  const totals = daily.reduce(
    (acc, row) => ({
      inputTokens: acc.inputTokens + row.inputTokens,
      outputTokens: acc.outputTokens + row.outputTokens,
      cacheReadTokens: acc.cacheReadTokens + row.cacheReadTokens,
      cacheCreationTokens: acc.cacheCreationTokens + row.cacheCreationTokens,
      totalTokens: acc.totalTokens + row.totalTokens,
    }),
    { inputTokens: 0, outputTokens: 0, cacheReadTokens: 0, cacheCreationTokens: 0, totalTokens: 0 },
  );
  const backendRows = backend === "all"
    ? [
      { backend: "claude", ratio: 0.6, cost: 0.12, costRecords: 2, records: 2 },
      { backend: "codex", ratio: 0.4, cost: null, costRecords: 0, records: 1 },
    ]
    : [{ backend, ratio: 1, cost: backend === "claude" ? 0.12 : null, costRecords: backend === "claude" ? 2 : 0, records: 2 }];
  const backends = backendRows.map((row) => ({
    backend: row.backend,
    inputTokens: Math.round(totals.inputTokens * row.ratio),
    outputTokens: Math.round(totals.outputTokens * row.ratio),
    cacheReadTokens: Math.round(totals.cacheReadTokens * row.ratio),
    cacheCreationTokens: Math.round(totals.cacheCreationTokens * row.ratio),
    totalTokens: Math.round(totals.totalTokens * row.ratio),
    knownCostUsd: row.cost,
    costRecordCount: row.costRecords,
    recordCount: row.records,
  }));
  const totalRecordCount = daily.reduce((sum, row) => sum + row.recordCount, 0);
  const costRecordCount = backends.reduce((sum, row) => sum + row.costRecordCount, 0);
  const knownCostUsd = backends.reduce(
    (sum, row) => sum + (typeof row.knownCostUsd === "number" ? row.knownCostUsd : 0),
    0,
  );
  return {
    days,
    backend,
    rangeStart,
    rangeEnd,
    totals,
    cost: {
      knownCostUsd: costRecordCount > 0 ? knownCostUsd : null,
      costRecordCount,
      totalRecordCount,
    },
    daily,
    backends,
    recent: backends.map((row, index) => ({
      eventId: `usage-${row.backend}-${index}`,
      taskId: `task-${index + 1}`,
      turnId: `turn-${index + 1}`,
      backend: row.backend,
      sessionId: row.backend === "codex" ? "thread-1" : "claude-session",
      inputTokens: row.inputTokens,
      outputTokens: row.outputTokens,
      cacheReadTokens: row.cacheReadTokens,
      cacheCreationTokens: row.cacheCreationTokens,
      totalTokens: row.totalTokens,
      knownCostUsd: row.knownCostUsd,
      createdAt: rangeEnd - (index + 1) * 3_600_000,
    })),
    projects: [
      {
        projectId: "project-lilia",
        projectName: "Lilia",
        projectCwd: "C:\\Files\\workspace\\Lilia",
        ...totals,
        knownCostUsd: costRecordCount > 0 ? knownCostUsd : null,
        costRecordCount,
        recordCount: totalRecordCount,
      },
    ],
    conversations: [
      {
        taskId: "task-1",
        taskTitle: "额度统计",
        taskStatus: "running",
        projectId: "project-lilia",
        projectName: "Lilia",
        ...totals,
        knownCostUsd: costRecordCount > 0 ? knownCostUsd : null,
        costRecordCount,
        recordCount: totalRecordCount,
      },
    ],
    tools: [
      {
        key: "command::",
        label: "命令",
        kind: "command",
        subkind: null,
        toolName: null,
        callCount: 6,
        sharePercent: 60,
      },
      {
        key: "search:grep:",
        label: "内容搜索",
        kind: "search",
        subkind: "grep",
        toolName: null,
        callCount: 4,
        sharePercent: 40,
      },
    ],
  };
}

function createMockCodexAccountQuotaStatus() {
  if (!routerModeUsesCodexAccount(routerModes.codex)) {
    const codexStatus = mockBackendEnvStatus("codex");
    return {
      available: false,
      connectionMode: codexStatus.connectionMode,
      limitId: null,
      limitName: null,
      planType: null,
      rateLimitReachedType: null,
      fiveHour: null,
      weekly: null,
      sparkFiveHour: null,
      sparkWeekly: null,
      credits: null,
      sparkCredits: null,
      rateLimitResetCredits: null,
      accountUsage: null,
      usageError: null,
      fetchedAt: Date.now(),
      error: null,
    };
  }
  const nowSeconds = Math.floor(Date.now() / 1000);
  return {
    available: true,
    connectionMode: "codex-account",
    limitId: "codex",
    limitName: "Codex",
    planType: "pro",
    rateLimitReachedType: null,
    fiveHour: {
      usedPercent: 25,
      windowDurationMins: 300,
      resetsAt: nowSeconds + 3 * 60 * 60,
    },
    weekly: {
      usedPercent: 40,
      windowDurationMins: 10080,
      resetsAt: nowSeconds + 4 * 86_400,
    },
    sparkFiveHour: {
      usedPercent: 15,
      windowDurationMins: 300,
      resetsAt: nowSeconds + 2 * 60 * 60,
    },
    sparkWeekly: {
      usedPercent: 70,
      windowDurationMins: 10080,
      resetsAt: nowSeconds + 2 * 86_400,
    },
    credits: {
      hasCredits: true,
      unlimited: false,
      balance: "3",
    },
    sparkCredits: {
      hasCredits: true,
      unlimited: true,
      balance: null,
    },
    rateLimitResetCredits: {
      availableCount: 2,
    },
    accountUsage: {
      summary: {
        lifetimeTokens: 123456,
        peakDailyTokens: 4567,
        longestRunningTurnSec: 540,
        currentStreakDays: 8,
        longestStreakDays: 14,
      },
      dailyUsageBuckets: [
        { startDate: "2026-06-17", tokens: 1200 },
        { startDate: "2026-06-18", tokens: 3400 },
      ],
    },
    usageError: null,
    fetchedAt: Date.now(),
    error: null,
  };
}

function getProjectRoadmap(projectId: string): ProjectRoadmapRow {
  const projectMilestones = milestones
    .filter((milestone) => milestone.projectId === projectId)
    .sort((a, b) => a.order - b.order || a.createdAt - b.createdAt)
    .map(cloneMilestone);
  const projectLinks = taskMilestoneLinks
    .filter((link) => {
      const milestone = milestones.find((item) => item.id === link.milestoneId);
      const task = tasks.find((item) => item.id === link.taskId);
      return milestone?.projectId === projectId && task?.projectId === projectId && !task.archived;
    })
    .sort((a, b) => {
      const milestoneA = milestones.find((item) => item.id === a.milestoneId);
      const milestoneB = milestones.find((item) => item.id === b.milestoneId);
      const taskA = tasks.find((item) => item.id === a.taskId);
      const taskB = tasks.find((item) => item.id === b.taskId);
      return (milestoneA?.order ?? 0) - (milestoneB?.order ?? 0) ||
        (taskA?.sortOrder ?? 0) - (taskB?.sortOrder ?? 0) ||
        (taskA?.createdAt ?? 0) - (taskB?.createdAt ?? 0);
    })
    .map(cloneTaskMilestoneLink);
  return { milestones: projectMilestones, links: projectLinks };
}

function cloneTodo(row: TodoRow): TodoRow {
  return { ...row, attachments: [...row.attachments] };
}

function nextTodoId(): string {
  todoSeq += 1;
  return `todo-${todoSeq}`;
}

function readAgentTodoText(todo: unknown): string {
  if (typeof todo === "string") return todo.trim();
  if (!todo || typeof todo !== "object" || Array.isArray(todo)) return "";
  const row = todo as AgentTodoInput;
  const value = row.content ?? row.text ?? row.title ?? row.description;
  return typeof value === "string" ? value.trim() : "";
}

function readAgentTodoDone(todo: unknown): boolean {
  if (!todo || typeof todo !== "object" || Array.isArray(todo)) return false;
  const row = todo as AgentTodoInput;
  return row.completed === true ||
    row.done === true ||
    String(row.status ?? "").toLowerCase() === "completed";
}

function normalizeTodoPriority(value: unknown): "high" | "normal" | "low" {
  const text = typeof value === "string" ? value.trim().toLowerCase() : "";
  if (text === "high" || text === "low") return text;
  return "normal";
}

function listMockTodos(taskId: string): TodoRow[] {
  return [...(todosByTaskId[taskId] ?? [])]
    .sort((a, b) => a.order - b.order || a.createdAt - b.createdAt)
    .map(cloneTodo);
}

function setMockGuideStatus(
  guideId: unknown,
  guideStatus: "pending" | "queued" | "sent",
) {
  if (typeof guideId !== "string" || !guideId) return;
  const now = Date.now();
  for (const taskId of Object.keys(todosByTaskId)) {
    let changed = false;
    todosByTaskId[taskId] = todosByTaskId[taskId].map((todo) => {
      if (todo.id !== guideId || todo.source !== "lilia" || todo.guideStatus === guideStatus) {
        return todo;
      }
      changed = true;
      return { ...todo, guideStatus, updatedAt: now };
    });
    if (changed) {
      emitTauriEvent(TODO_CHANGED_EVENT_NAME, createTodoChangedEvent(taskId));
      return;
    }
  }
}

function resetMockQueuedGuides(turns: Array<Record<string, unknown>> = []) {
  for (const turn of turns) {
    setMockGuideStatus(turn.guideId, "pending");
  }
}

function applyMockAgentTodos(taskId: string, todos: unknown[]): TodoRow[] {
  const now = Date.now();
  const existing = todosByTaskId[taskId] ?? [];
  const userRows = existing.filter((todo) => todo.source === "lilia");
  const agentRows = existing.filter((todo) => todo.source === "agent");
  const userMax = userRows.reduce((max, todo) => Math.max(max, todo.order), -1);
  const nextAgentRows: TodoRow[] = [];

  todos.forEach((todo, index) => {
    const text = readAgentTodoText(todo);
    if (!text) return;
    const matched = agentRows.find((row) => row.text === text);
    const nextRow: TodoRow = {
      id: matched?.id ?? nextTodoId(),
      taskId,
      text,
      done: readAgentTodoDone(todo),
      order: userMax + 1 + index,
      source: "agent",
      priority: normalizeTodoPriority((todo as AgentTodoInput)?.priority),
      guideStatus: null,
      attachments: [],
      createdAt: matched?.createdAt ?? now,
      updatedAt: now,
    };
    nextAgentRows.push(nextRow);
  });

  todosByTaskId[taskId] = [...userRows, ...nextAgentRows];
  return listMockTodos(taskId);
}

function refreshSessionCounts() {
  projects = projects.map((project) => ({
    ...project,
    sessionCount: tasks.filter((task) =>
      !task.archived && task.projectId === project.id && task.status !== "cancelled"
    ).length,
  }));
}

function defaultModelForBackend(backend: "claude" | "codex") {
  return DEFAULT_MODEL_BY_BACKEND[backend];
}

function normalizeBackend(value: unknown): "claude" | "codex" {
  return value === "codex" ? "codex" : "claude";
}

function directDefaultUrl(backend: MockProviderBackend) {
  return DIRECT_DEFAULT_URLS[backend];
}

function mockApiConnectionMode(hasKey: boolean, hasUrl: boolean): ConnectionMode {
  if (hasUrl) return "custom";
  return hasKey ? "api" : "unconfigured";
}

function normalizeMockRouterMode(
  backend: MockProviderBackend,
  mode: "api" | "codex-account" | "direct" | "cc-switch",
): MockRouterMode {
  const normalized = mode === "direct" || mode === "cc-switch" ? "api" : mode;
  return normalizeContractRouterModeForBackend(backend, normalized);
}

function cloneProviderConfig(backend: MockProviderBackend) {
  return { ...providerConfigs[backend], apiKey: null };
}

function mockBackendEnvStatus(backend: MockProviderBackend): BackendEnvStatus {
  const mode = routerModes[backend];
  const config = providerConfigs[backend];
  if (routerModeUsesCodexAccount(mode) && backend === "codex") {
    return {
      backend,
      hasApiKey: false,
      connectionMode: "codex-account",
      effectiveUrl: null,
    };
  }
  const hasUrl = typeof config.baseUrl === "string" && config.baseUrl.trim().length > 0;
  const hasKey = config.hasApiKey;
  return {
    backend,
    hasApiKey: hasKey,
    connectionMode: mockApiConnectionMode(hasKey, hasUrl),
    effectiveUrl: hasUrl ? config.baseUrl : hasKey ? directDefaultUrl(backend) : null,
  };
}

function modelBelongsToBackend(model: string, backend: "claude" | "codex") {
  return MODEL_OPTIONS_BY_BACKEND[backend].some((option) => option.id === model);
}

function normalizeComposer(input: unknown, taskId: string) {
  const row = input && typeof input === "object" && !Array.isArray(input)
    ? input as Record<string, unknown>
    : {};
  const backend = activeBackend;
  const model = typeof row.model === "string" && modelBelongsToBackend(row.model, backend)
    ? row.model
    : defaultModelForBackend(backend);
  return {
    ...row,
    taskId,
    backend,
    model,
    modelSelectionMode: row.modelSelectionMode === "manual" ? "manual" : "auto",
    reasoningEffort: typeof row.reasoningEffort === "string" ? row.reasoningEffort : null,
  };
}

const mockSlashCommands: MockSlashCommand[] = [
  {
    id: "native:help",
    name: "help",
    title: "显示可用斜杠命令",
    description: "列出当前可执行的内置命令和项目命令。",
    source: "native",
    parameters: [],
  },
  {
    id: "native:status",
    name: "status",
    title: "显示当前会话状态",
    description: "写入当前后端和工作目录状态。",
    source: "native",
    parameters: [],
  },
  {
    id: "project:release",
    name: "release",
    title: "生成发布检查",
    description: "按项目命令模板生成发布前检查清单。",
    source: "project",
    parameters: [],
  },
];

function mockSlashCommandOutput(command: MockSlashCommand, projectCwd: string, backend: string) {
  if (command.id === "native:help") {
    return `内置命令：/help、/status\n项目命令：1 个\n项目命令文件目录：${projectCwd}\\.lilia\\commands`;
  }
  if (command.id === "native:status") {
    return `当前后端：${backend}\n工作目录：${projectCwd}`;
  }
  return "请完成发布前检查并整理风险项。\n\n来源：D:\\PROJECT\\workspace\\Lilia\\.lilia\\commands\\release.md";
}

export function resetTauriMockData() {
  projects = baseProjects.map(cloneProject);
  tasks = baseTasks.map(cloneTask);
  milestones = baseMilestones.map(cloneMilestone);
  taskMilestoneLinks = baseTaskMilestoneLinks.map(cloneTaskMilestoneLink);
  codexThreads = baseCodexThreads.map((thread) => ({ ...thread }));
  codexTaskSessions = { "t-002": "thread-1" };
  cleanedCodexThreads = [];
  todosByTaskId = {};
  todoSeq = 0;
  automationVersionSeq = 0;
  automationRunSeq = 0;
  const scope = defaultAutomationScope();
  automations = [
    {
      id: "auto-1",
      name: "任务完成后复盘",
      enabled: false,
      scope,
      draft: {
        nodes: [
          {
            id: "trigger-1",
            kind: "trigger",
            title: "任务变化",
            position: { x: 80, y: 120 },
            config: { triggerKind: "manual" },
          },
          {
            id: "agent-1",
            kind: "agent",
            title: "复盘 Agent",
            position: { x: 360, y: 120 },
            config: {
              taskId: "t-002",
              backend: "claude",
              prompt: "请复盘当前任务。",
              permission: "ask",
            },
          },
        ],
        edges: [{ id: "trigger-1-agent-1", source: "trigger-1", target: "agent-1" }],
        scope,
      },
      publishedVersionId: "auto-1-v1",
      createdAt: 1000,
      updatedAt: 1000,
    },
  ];
  automationRuns = [];
  automationRunNodes = {};
  pendingCliProjectOpen = null;
  timelineEvents = {
    "t-002": [
      {
        id: "tl-existing",
        taskId: "t-002",
        turnId: "turn-existing",
        backend: "claude",
        kind: "reasoning",
        status: "info",
        title: "历史思考摘要",
        summary: "从持久化时间线恢复的公开摘要。",
        payload: { source: "mock" },
        createdAt: 1500,
        updatedAt: 1500,
        turnSeq: 0,
        intraTurnOrder: 0,
      },
    ],
  };
  chatRunning = {};
  chatQueued = {};
  chatInterruptRollbacks = {};
  runtimeSnapshotOverrides = {};
  processSessionCommands = [];
  nextChatSendError = null;
  nextAgentInteractionResponseError = null;
  clipboardFilePaths = [];
  clipboardImageSeq = 0;
  clipboardTextSeq = 0;
  liliaIabSeq = 0;
  nextLiliaIabDelivery = "message";
  activeBackend = "claude";
  providerConfigs = {
    claude: { backend: "claude", baseUrl: null, hasApiKey: false },
    codex: { backend: "codex", baseUrl: null, hasApiKey: false },
  };
  routerModes = {
    claude: DEFAULT_ROUTER_MODE_BY_BACKEND.claude,
    codex: DEFAULT_ROUTER_MODE_BY_BACKEND.codex,
  };
  nodeAvailable = true;
  remoteControlEnabled = false;
  mockNativeCredentials = [];
  mockNativeCredentialSeq = 0;
  remoteControlKeepAwakeEnabled = true;
  remoteControlPcName = "Lilia Test PC";
  remoteControlTicket = null;
  remoteControlDevices = [];
  codexAppServerStatus = {
    version: "codex-cli 0.136.0",
    installPath: null,
    managed: false,
    available: true,
    supportsRequiredProtocol: true,
    failureKind: null,
    issues: [],
    latestVersion: null,
    updateAvailable: false,
    releaseNotes: [],
    updateError: null,
    updateState: "idle",
    preparedVersion: null,
    updateProgressPercent: null,
  };
  composerStateHandler = null;
  claudePlugins = baseClaudePlugins.map((plugin) => ({ ...plugin }));
  claudeMcpServers = baseClaudeMcpServers.map((server) => ({
    ...server,
    args: [...server.args],
    envKeys: [...server.envKeys],
  }));
  codexMcpServers = baseCodexMcpServers.map((server) => ({
    ...server,
    args: [...server.args],
    envKeys: [...server.envKeys],
  }));
  hookDocuments = initialHookDocuments();
  agentInteractionSettings = defaultAgentInteractionSettings();
  customSubagents = [
    {
      id: "reviewer",
      name: "Reviewer",
      description: "检查风险与回归",
      instruction: "Review code changes, identify risk, and summarize findings.",
      enabled: true,
    },
  ];
  assistantAIConfig = {
    baseUrl: null,
    apiKey: null,
    model: null,
    modelPool: [
      { id: "gpt-5.5", label: "GPT 5.5", source: "remote", backend: "codex" },
    ],
    codexAccountSparkEnabled: false,
    hasApiKey: true,
  };
  modelFeatureSettings = {
    chat: { light: null, normal: null, deep: null },
    title: null,
    suggestion: null,
    promptRouter: null,
    promptOptimize: null,
    autoTurnDecision: null,
  };
  conversationSuggestionSettings = { enabled: true, source: "assistant-ai" };
  conversationSuggestionsOverride = null;
  conversationSuggestionSourcesOverride = null;
  nextConversationSuggestionsError = null;
  conversationSuggestionsDelayMs = 0;
  runtimeSnapshotDelayMs = 0;
  projectSettings = {
    cloneParentDir: null,
    codexDefaults: null,
    githubBinding: null,
    worktree: {
      defaultMode: "current",
      parentDir: null,
      autoInstructions: [
        "This task is running inside a dedicated git worktree managed by Lilia.",
        "Keep changes scoped to this task and create commits in the worktree before requesting merge/archive.",
      ].join("\n"),
      cleanupOnArchive: true,
    },
  };
  taskWorktrees = {};
  mockPickedFolderPath = "C:\\Users\\mock";
  githubBindingStatus = {
    state: "unbound",
    clientIdConfigured: true,
    clientIdSource: "bundled",
    binding: null,
  };
  githubDeviceFlowStart = {
    deviceCode: "device-code-1",
    userCode: "ABCD-EFGH",
    verificationUri: "https://github.com/login/device",
    expiresAt: Date.now() + 15 * 60 * 1000,
    intervalSeconds: 1,
  };
  githubDeviceFlowPollQueue = [];
  githubRepoPages = { 1: { items: [], nextPage: null } };
  githubReposError = null;
  popupWindowSettings = { shortcut: null };
  nextPopupSettingsError = null;
  popupLastProjectId = null;
  eventHandlers = {};
  listenFailureByEvent = {};
  webviewDragDropHandlers = [];
  windowScaleFactor = 1;
  agentTimelineDelayMs = 0;
  quotaUsageStatsOverride = null;
  codexAccountQuotaStatusOverride = null;
  mockCurrentWindow.label = "main";
  mockCurrentWindow.isMaximized.mockClear();
  mockCurrentWindow.onResized.mockClear();
  mockCurrentWindow.onMoved.mockClear();
  mockCurrentWindow.outerPosition.mockClear();
  mockCurrentWindow.innerSize.mockClear();
  mockCurrentWindow.setPosition.mockClear();
  mockCurrentWindow.setSize.mockClear();
  mockCurrentWindow.minimize.mockClear();
  mockCurrentWindow.toggleMaximize.mockClear();
  mockCurrentWindow.setAlwaysOnTop.mockClear();
  mockCurrentWindow.close.mockClear();
  mockCurrentWindow.scaleFactor.mockClear();
  refreshSessionCounts();
  mockInvoke.mockClear();
  mockListen.mockClear();
  mockGetCurrentWebview.mockClear();
  mockGetCurrentWindow.mockClear();
}

export function emitTauriEvent(event: string, payload: unknown) {
  for (const handler of eventHandlers[event] ?? []) {
    handler({ payload });
  }
}

export function setMockPendingCliProjectOpen(payload: CliProjectOpenPayload | null) {
  pendingCliProjectOpen = payload ? { ...payload } : null;
}

export function finishMockAutomationRun(runId: string) {
  const run = automationRuns.find((item) => item.id === runId);
  if (!run) return;
  const now = Date.now();
  run.status = "succeeded";
  run.finishedAt = now;
  run.error = null;
  automationRunNodes[runId] = (automationRunNodes[runId] ?? []).map((node) => ({
    ...node,
    status: "succeeded",
    output: { ...(node.output ?? {}), completedByEvent: true },
    finishedAt: node.finishedAt ?? now,
  }));
  emitTauriEvent(AUTOMATION_RUN_FINISHED_EVENT_NAME, createAutomationRunEvent(cloneAutomationRun(run)));
}

export function clearMockAutomations() {
  automations = [];
  automationRuns = [];
  automationRunNodes = {};
}

export function seedMockAutomationRun() {
  const workflow = automations[0];
  if (!workflow) return;
  const now = Date.now();
  const run: AutomationRunRow = {
    id: `run-seeded-${automationRuns.length + 1}`,
    workflowId: workflow.id,
    workflowVersionId: workflow.publishedVersionId ?? `${workflow.id}-v1`,
    status: "succeeded",
    trigger: {
      id: `signal-seeded-${automationRuns.length + 1}`,
      kind: "manual",
      projectId: "lilia",
      taskId: "t-002",
      backend: "claude",
      eventKind: "manual",
      automationRunId: null,
      payload: { source: "seed", largePayload: "x".repeat(1_024) },
      createdAt: now,
    },
    scope: cloneAutomationWorkflow(workflow).scope,
    startedAt: now,
    finishedAt: now + 5,
    error: null,
  };
  automationRuns = [run, ...automationRuns];
  automationRunNodes[run.id] = workflow.draft.nodes.map((node) => ({
    id: `${run.id}:${node.id}`,
    runId: run.id,
    nodeId: node.id,
    status: "succeeded",
    input: { trigger: run.trigger },
    output: { ok: true },
    error: null,
    startedAt: now,
    finishedAt: now + 5,
  }));
}

export function mockListenerCount(event: string): number {
  return eventHandlers[event]?.length ?? 0;
}

export function failNextMockListen(event: string, message: string) {
  listenFailureByEvent[event] = message;
}

export function setMockChatRunning(taskId: string, running: boolean) {
  chatRunning[taskId] = running;
}

export function setMockRuntimeSnapshot(
  taskId: string,
  snapshot: Record<string, unknown>,
) {
  runtimeSnapshotOverrides[taskId] = snapshot;
}

export function mockProcessSessionCommands() {
  return processSessionCommands.map((item) => ({
    taskId: item.taskId,
    command: { ...item.command },
  }));
}

export function clearMockProcessSessionCommands() {
  processSessionCommands = [];
}

export function failNextMockChatSend(message: string) {
  nextChatSendError = message;
}

export function failNextMockAgentInteractionResponse(message: string) {
  nextAgentInteractionResponseError = message;
}

export function setNextMockLiliaIabDelivery(delivery: "runner" | "message") {
  nextLiliaIabDelivery = delivery;
}

export function emitWebviewDragDropEvent(payload: unknown) {
  for (const handler of webviewDragDropHandlers) {
    handler({ payload });
  }
}

export function setMockComposerStateHandler(
  handler: ((taskId: string) => unknown | Promise<unknown>) | null,
) {
  composerStateHandler = handler;
}

export function setMockProjectPinned(projectId: string, pinned: boolean) {
  projects = projects.map((project) =>
    project.id === projectId ? { ...project, pinned } : project
  );
  projectPinUpdater?.(projectId, pinned);
}

export function setMockTaskArchived(taskId: string, archived: boolean) {
  tasks = tasks.map((task) =>
    task.id === taskId ? { ...task, archived } : task
  );
  refreshSessionCounts();
}

export function setMockAgentTimelineDelay(delayMs: number) {
  agentTimelineDelayMs = delayMs;
}

export function setMockConversationSuggestions(items: unknown[] | null) {
  conversationSuggestionsOverride = items;
}

export function setMockConversationSuggestionSources(sources: unknown | null) {
  conversationSuggestionSourcesOverride = sources;
}

export function setMockConversationSuggestionsDelay(delayMs: number) {
  conversationSuggestionsDelayMs = delayMs;
}

export function setMockRuntimeSnapshotDelay(delayMs: number) {
  runtimeSnapshotDelayMs = delayMs;
}

export function failNextMockConversationSuggestions(message: string) {
  nextConversationSuggestionsError = message;
}

export function bindMockProjectPinUpdater(
  updater: ((projectId: string, pinned: boolean) => void) | null,
) {
  projectPinUpdater = updater;
}

export function mockProjectsForStore() {
  refreshSessionCounts();
  return projects.map((project) => ({
    id: project.id,
    name: project.name,
    cwd: project.cwd,
    sessionCount: project.sessionCount,
    pinned: project.pinned,
  }));
}

export function mockTasksByProjectForStore() {
  const byProject: Record<string, unknown[]> = {};
  for (const project of projects) {
    byProject[project.id] = tasks
      .filter((task) => !task.archived && task.projectId === project.id)
      .map(cloneTask)
      .sort((a, b) =>
        Number(b.pinned) - Number(a.pinned) || a.sortOrder - b.sortOrder
      )
      .map((task) => ({
        id: task.id,
        projectId: task.projectId ?? "",
        sessionId: task.sessionId,
        title: task.title,
        status: task.status,
        createdAt: task.createdAt,
        pinned: task.pinned,
        parentId: task.parentId,
        dependsOn: [...task.dependsOn],
      }));
  }
  return byProject;
}

export function mockOrphansForStore() {
  return tasks
    .filter((task) => !task.archived && task.projectId === null)
    .map(cloneTask)
    .sort((a, b) =>
      Number(b.pinned) - Number(a.pinned) || a.sortOrder - b.sortOrder
    )
    .map((task) => ({
      id: task.id,
      sessionId: task.sessionId,
      title: task.title,
      createdAt: task.createdAt,
      pinned: task.pinned,
      parentId: task.parentId,
    }));
}

export function setMockTasks(nextTasks: TaskRow[]) {
  tasks = nextTasks.map(cloneTask);
  refreshSessionCounts();
}

export function setMockTaskWorktree(taskId: string, worktree: Record<string, unknown> | null) {
  if (!worktree) {
    delete taskWorktrees[taskId];
    return;
  }
  taskWorktrees[taskId] = { ...worktree };
}

export function setMockRoadmap(
  nextMilestones: MilestoneRow[],
  nextLinks: TaskMilestoneLinkRow[] = [],
) {
  milestones = nextMilestones.map(cloneMilestone);
  taskMilestoneLinks = nextLinks.map(cloneTaskMilestoneLink);
}

export const mockGetCurrentWebview = vi.fn(() => ({
  onDragDropEvent: vi.fn(async (handler: (event: { payload: unknown }) => void) => {
    webviewDragDropHandlers = [...webviewDragDropHandlers, handler];
    return async () => {
      webviewDragDropHandlers = webviewDragDropHandlers.filter((h) => h !== handler);
    };
  }),
}));

export const mockCurrentWindow = {
  label: "main",
  isMaximized: vi.fn(async () => false),
  onResized: vi.fn(async () => vi.fn()),
  onMoved: vi.fn(async () => vi.fn()),
  outerPosition: vi.fn(async () => ({ x: 80, y: 90 })),
  innerSize: vi.fn(async () => ({ width: 360, height: 520 })),
  setPosition: vi.fn(async () => undefined),
  setSize: vi.fn(async () => undefined),
  minimize: vi.fn(async () => undefined),
  toggleMaximize: vi.fn(async () => undefined),
  setAlwaysOnTop: vi.fn(async () => undefined),
  close: vi.fn(async () => undefined),
  scaleFactor: vi.fn(async () => windowScaleFactor),
};

export const mockGetCurrentWindow = vi.fn(() => mockCurrentWindow);

export function setMockCurrentWindowLabel(label: string) {
  mockCurrentWindow.label = label;
}

export function setMockWindowScaleFactor(scaleFactor: number) {
  windowScaleFactor = scaleFactor;
}

export function completeMockAgentTurn(taskId: string) {
  const rollback = chatInterruptRollbacks[taskId] ?? null;
  delete chatInterruptRollbacks[taskId];
  emitTauriEvent(CHAT_DONE_EVENT_NAME, createChatDoneEvent({
    taskId,
    sessionId: `mock-${taskId}`,
    subtype: null,
    rollback,
  }));
  const [next, ...rest] = chatQueued[taskId] ?? [];
  if (next) {
    chatQueued[taskId] = rest;
    chatRunning[taskId] = true;
    setMockGuideStatus(next.guideId, "sent");
    const queuedMessage = (timelineEvents[taskId] ?? []).find((event) => {
      const payload = event.payload as Record<string, unknown> | null;
      return event.kind === "message" && payload?.queued === true;
    });
    if (queuedMessage) {
      emitMockTimelineEvent(taskId, {
        ...queuedMessage,
        status: "success",
        payload: {
          ...(queuedMessage.payload as Record<string, unknown>),
          queued: false,
        },
        updatedAt: Date.now(),
      });
    }
    queueMicrotask(() => {
      emitTauriEvent(CHAT_TURN_STARTED_EVENT_NAME, createChatTurnStartedEvent(taskId, rest.length));
    });
  } else {
    chatRunning[taskId] = false;
  }
}

function currentChatTurnId(taskId: string): string {
  const event = [...(timelineEvents[taskId] ?? [])].reverse().find((candidate) => {
    const payload = candidate.payload as Record<string, unknown> | null;
    return candidate.kind === "message" &&
      typeof candidate.turnId === "string" &&
      payload?.role === "user" &&
      payload?.queued !== true;
  });
  return event?.turnId ?? `turn-interrupted-${Date.now()}`;
}

export function emitMockTimelineEvent(
  taskId: string,
  patch: Partial<AgentTimelineEvent> = {},
) {
  const event = createMockTimelineEvent(taskId, patch, {
    id: `tl-${Date.now()}`,
    updatedAt: Date.now(),
    intraTurnOrder: timelineEvents[taskId]?.length ?? 0,
  });
  timelineEvents[taskId] = [
    ...(timelineEvents[taskId] ?? []).filter((item) => item.id !== event.id),
    event,
  ];
  emitTauriEvent(AGENT_TIMELINE_EVENT_NAME, event);
  return event;
}

function createMockTimelineEvent(
  taskId: string,
  patch: Partial<AgentTimelineEvent>,
  fallback: Partial<AgentTimelineEvent> = {},
): AgentTimelineEvent {
  const createdAt = patch.createdAt ?? fallback.createdAt ?? Date.now();
  return {
    id: patch.id ?? fallback.id ?? `tl-${Date.now()}`,
    taskId,
    turnId: patch.turnId ?? "turn-live",
    backend: patch.backend ?? "claude",
    kind: patch.kind ?? "command",
    status: patch.status ?? fallback.status ?? "running",
    title: patch.title ?? fallback.title ?? "实时命令",
    summary: patch.summary ?? fallback.summary ?? "正在运行命令",
    payload: patch.payload ?? fallback.payload ?? { command: "yarn test" },
    createdAt,
    updatedAt: patch.updatedAt ?? fallback.updatedAt ?? createdAt,
    turnSeq: patch.turnSeq ?? fallback.turnSeq ?? 0,
    intraTurnOrder: patch.intraTurnOrder ?? fallback.intraTurnOrder ?? 0,
  };
}

export function emitMockTimelineBatchEvent(
  taskId: string,
  patches: Partial<AgentTimelineEvent>[],
) {
  const events = patches.map((patch, index) =>
    createMockTimelineEvent(patch.taskId ?? taskId, patch, {
      id: `tl-batch-${Date.now()}-${index}`,
      updatedAt: Date.now(),
      intraTurnOrder: (timelineEvents[taskId]?.length ?? 0) + index,
    })
  );
  timelineEvents[taskId] = [
    ...(timelineEvents[taskId] ?? []).filter((item) =>
      !events.some((event) => event.id === item.id)
    ),
    ...events,
  ];
  emitTauriEvent(AGENT_TIMELINE_BATCH_EVENT_NAME, { taskId, events });
  return events;
}

export function replaceMockTimelineEvents(
  taskId: string,
  events: Partial<AgentTimelineEvent>[],
) {
  timelineEvents[taskId] = events.map((patch, index) =>
    createMockTimelineEvent(taskId, patch, {
      id: `tl-${taskId}-${index}`,
      status: "success",
      title: "历史事件",
      summary: "",
      payload: {},
      createdAt: 10_000 + index,
      turnSeq: index,
    })
  );
}

/**
 * Runner 在 `case "result"` 里 emit 的 `kind:"turn"` 终结事件——前端 UI 把这条
 * 当作"该 turn 已结束"的权威信号，进而把过程折叠到最终回复下。
 */
export function emitMockTurnCompleted(
  taskId: string,
  turnId: string,
  status: AgentTimelineEvent["status"] = "success",
  createdAt?: number,
) {
  const at = createdAt ?? Date.now();
  return emitMockTimelineEvent(taskId, {
    id: `tl-turn-complete-${turnId}`,
    kind: "turn",
    status,
    title: "Claude turn completed",
    summary: "",
    payload: { backend: "claude" },
    turnId,
    createdAt: at,
    updatedAt: at,
  });
}

export function seedMockChatMessages(taskId: string, messages: unknown[]) {
  const messageEvents = messages
    .map((message, index): AgentTimelineEvent | null => {
      if (!message || typeof message !== "object" || Array.isArray(message)) return null;
      const row = message as Record<string, unknown>;
      if (row.role !== "user" && row.role !== "system") return null;
      const id = String(row.id ?? `message-${index}`);
      const content = String(row.content ?? "");
      const attachments = Array.isArray(row.attachments) ? row.attachments : [];
      const createdAt = typeof row.createdAt === "number" ? row.createdAt : Date.now();
      const title = row.role === "system" ? "系统消息" : "用户输入";
      return {
        id,
        taskId,
        turnId: null,
        backend: "claude",
        kind: "message",
        status: "success",
        title,
        summary: content,
        payload: {
          role: row.role,
          content,
          attachments,
          queued: false,
        },
        createdAt,
        updatedAt: createdAt,
        turnSeq: index,
        intraTurnOrder: 0,
      };
    })
    .filter((event): event is AgentTimelineEvent => event !== null);
  timelineEvents[taskId] = [
    ...messageEvents,
    ...(timelineEvents[taskId] ?? []).filter((event) => event.kind !== "message"),
  ];
}

export function setMockClipboardFilePaths(paths: string[]) {
  clipboardFilePaths = [...paths];
}

export function setMockActiveBackend(backend: "claude" | "codex") {
  activeBackend = backend;
}

export function setMockRouterMode(
  backend: "claude" | "codex",
  mode: "api" | "codex-account" | "direct" | "cc-switch",
) {
  routerModes[backend] = normalizeMockRouterMode(backend, mode);
}

export function setMockProviderConfig(
  backend: "claude" | "codex",
  config: Partial<MockProviderConfig>,
) {
  providerConfigs[backend] = {
    ...providerConfigs[backend],
    ...config,
    backend,
  };
}

export function setMockQuotaUsageStats(stats: Record<string, unknown> | null) {
  quotaUsageStatsOverride = stats ? { ...stats } : null;
}

export function setMockCodexAccountQuotaStatus(stats: Record<string, unknown> | null) {
  codexAccountQuotaStatusOverride = stats ? { ...stats } : null;
}

export function setMockNodeAvailable(available: boolean) {
  nodeAvailable = available;
}

export function setMockCodexAppServerStatus(status: Partial<typeof codexAppServerStatus>) {
  codexAppServerStatus = {
    ...codexAppServerStatus,
    ...status,
    issues: status.issues ? [...status.issues] : codexAppServerStatus.issues,
    releaseNotes: status.releaseNotes ? [...status.releaseNotes] : codexAppServerStatus.releaseNotes,
  };
}

export function failNextPopupSettingsSave(message: string) {
  nextPopupSettingsError = message;
}

export function setMockPickedFolderPath(path: string | null) {
  mockPickedFolderPath = path;
}

export function setMockGitHubBindingStatus(
  status: Partial<typeof githubBindingStatus>,
) {
  githubBindingStatus = {
    ...githubBindingStatus,
    ...status,
    binding: "binding" in status ? (status.binding ?? null) : githubBindingStatus.binding,
  };
  projectSettings = {
    ...projectSettings,
    githubBinding: githubBindingStatus.binding,
  };
}

export function setMockGitHubDeviceFlowStart(
  next: Partial<typeof githubDeviceFlowStart>,
) {
  githubDeviceFlowStart = {
    ...githubDeviceFlowStart,
    ...next,
  };
}

export function setMockGitHubPollSequence(
  queue: Array<Record<string, unknown>>,
) {
  githubDeviceFlowPollQueue = queue.map((item) => ({ ...item }));
}

export function setMockGitHubRepos(
  pages: Record<number, { items: unknown[]; nextPage: number | null }>,
) {
  githubRepoPages = Object.fromEntries(
    Object.entries(pages).map(([page, value]) => [
      Number(page),
      {
        items: Array.isArray(value.items)
          ? value.items.map((item) => ({ ...(item as Record<string, unknown>) }))
          : [],
        nextPage: value.nextPage ?? null,
      },
    ]),
  );
}

export function setMockGitHubReposError(message: string | null) {
  githubReposError = message;
}

export const mockListen = vi.fn(async (
  event: string,
  handler: (event: { payload: unknown }) => void,
) => {
  const failure = listenFailureByEvent[event];
  if (failure) {
    delete listenFailureByEvent[event];
    throw new Error(failure);
  }
  eventHandlers[event] = [...(eventHandlers[event] ?? []), handler];
  return async () => {
    eventHandlers[event] = (eventHandlers[event] ?? []).filter((h) => h !== handler);
  };
});

export const mockInvoke = vi.fn(async (cmd: string, args: Record<string, unknown> = {}) => {
  switch (cmd) {
    case APP_RESTART_COMMAND:
      return undefined;

    case TAURI_PLUGIN_DIALOG_OPEN_COMMAND:
      return mockPickedFolderPath;

    case PROJECT_LIST_COMMAND:
      refreshSessionCounts();
      return projects
        .map(cloneProject)
        .sort((a, b) => a.sortOrder - b.sortOrder);

    case PROJECT_DASHBOARD_LIST_COMMAND:
      return createProjectDashboardSummaries();

    case PROJECT_GET_COMMAND: {
      const id = String(args.id);
      refreshSessionCounts();
      return projects.find((project) => project.id === id) ?? null;
    }

    case PROJECT_RENAME_COMMAND: {
      const id = String(args.id ?? "");
      const nextName = String(args.nextName ?? "").trim();
      if (!id || !nextName) return false;
      const index = projects.findIndex((project) => project.id === id);
      if (index < 0) return false;
      projects[index] = { ...projects[index], name: nextName };
      return true;
    }

    case CLI_PROJECT_OPEN_CONSUME_PENDING_COMMAND: {
      const payload = pendingCliProjectOpen;
      pendingCliProjectOpen = null;
      return payload ? { ...payload } : null;
    }

    case PROJECT_GET_SETTINGS_COMMAND:
      return { ...projectSettings };

    case PROJECT_SET_SETTINGS_COMMAND: {
      const settings = args.settings && typeof args.settings === "object" && !Array.isArray(args.settings)
        ? args.settings as Record<string, unknown>
        : {};
      projectSettings = {
        cloneParentDir: typeof settings.cloneParentDir === "string"
          ? settings.cloneParentDir
          : null,
        codexDefaults: "codexDefaults" in settings
          ? settings.codexDefaults
          : null,
        githubBinding: "githubBinding" in settings
          ? settings.githubBinding as Record<string, unknown> | null
          : null,
        worktree: settings.worktree && typeof settings.worktree === "object" && !Array.isArray(settings.worktree)
          ? settings.worktree as typeof projectSettings.worktree
          : projectSettings.worktree,
      };
      return undefined;
    }

    case GIT_CLONE_REPO_COMMAND: {
      const url = String(args.url ?? "").trim().replace(/\.git$/i, "").replace(/\/+$/, "");
      const parentDir = String(args.parentDir ?? "").replace(/[\\/]+$/, "");
      const base = url.split(/[/:]/).pop() || "repo";
      return `${parentDir}\\${base}`;
    }

    case GITHUB_GET_BINDING_STATUS_COMMAND:
      return {
        ...githubBindingStatus,
        binding: githubBindingStatus.binding ? { ...githubBindingStatus.binding } : null,
      };

    case GITHUB_START_DEVICE_FLOW_COMMAND:
      return { ...githubDeviceFlowStart };

    case GITHUB_POLL_DEVICE_FLOW_COMMAND: {
      const next = githubDeviceFlowPollQueue.shift() ?? {
        status: "pending",
        intervalSeconds: githubDeviceFlowStart.intervalSeconds,
        bindingStatus: null,
        error: null,
      };
      const result = {
        status: String(next.status ?? "pending"),
        intervalSeconds: Number(next.intervalSeconds ?? githubDeviceFlowStart.intervalSeconds),
        bindingStatus: next.bindingStatus ?? null,
        error: next.error ?? null,
      };
      if (result.status === "authorized" && result.bindingStatus && typeof result.bindingStatus === "object") {
        const bindingStatus = result.bindingStatus as Record<string, unknown>;
        githubBindingStatus = {
          state: bindingStatus.state === "bound" ? "bound" : "unbound",
          clientIdConfigured: bindingStatus.clientIdConfigured !== false,
          clientIdSource: bindingStatus.clientIdSource === "custom"
            ? "custom"
            : bindingStatus.clientIdSource === "none"
              ? "none"
              : "bundled",
          binding: bindingStatus.binding && typeof bindingStatus.binding === "object"
            ? bindingStatus.binding as Record<string, unknown>
            : null,
        };
        projectSettings = {
          ...projectSettings,
          githubBinding: githubBindingStatus.binding,
        };
      }
      return result;
    }

    case GITHUB_UNBIND_COMMAND:
      githubBindingStatus = {
        ...githubBindingStatus,
        state: "unbound",
        binding: null,
      };
      projectSettings = { ...projectSettings, githubBinding: null };
      return undefined;

    case GITHUB_LIST_REPOS_COMMAND: {
      if (githubReposError) {
        throw new Error(githubReposError);
      }
      const page = typeof args.page === "number" ? args.page : 1;
      const result = githubRepoPages[page] ?? { items: [], nextPage: null };
      return {
        items: result.items.map((item) => ({ ...(item as Record<string, unknown>) })),
        nextPage: result.nextPage,
      };
    }

    case GITHUB_CLONE_REPO_COMMAND: {
      const repo = String(args.repo ?? "").trim().replace(/\/+$/, "");
      const parentDir = String(args.parentDir ?? "").replace(/[\\/]+$/, "");
      const cleaned = repo.replace(/\.git$/i, "");
      const base = cleaned.split(/[/:]/).pop() || "repo";
      return `${parentDir}\\${base}`;
    }

    case SYSTEM_OPEN_PATH_COMMAND:
    case SYSTEM_OPEN_URL_COMMAND:
    case SYSTEM_OPEN_IN_VSCODE_COMMAND:
      return undefined;

    case REMOTE_CONTROL_STATUS_COMMAND:
      return mockRemoteControlStatus();

    case REMOTE_CONTROL_SET_HOST_ENABLED_COMMAND:
      remoteControlEnabled = args.enabled === true;
      if (!remoteControlEnabled) {
        remoteControlTicket = null;
      }
      return mockRemoteControlStatus();

    case REMOTE_CONTROL_SET_PC_NAME_COMMAND: {
      const name = String(args.name ?? "").trim();
      remoteControlPcName = name || "Lilia Test PC";
      if (remoteControlTicket) {
        remoteControlTicket = createMockRemoteControlTicket();
      }
      return mockRemoteControlStatus();
    }

    case REMOTE_CONTROL_SET_KEEP_AWAKE_ENABLED_COMMAND:
      remoteControlKeepAwakeEnabled = args.enabled === true;
      return mockRemoteControlStatus();

    case REMOTE_CONTROL_START_PAIRING_COMMAND:
      remoteControlEnabled = true;
      remoteControlTicket = createMockRemoteControlTicket();
      return { ...remoteControlTicket };

    case REMOTE_CONTROL_CANCEL_PAIRING_COMMAND:
      remoteControlTicket = null;
      return undefined;

    case REMOTE_CONTROL_REVOKE_DEVICE_COMMAND: {
      const deviceId = String(args.deviceId ?? "");
      remoteControlDevices = remoteControlDevices.map((device) =>
        device.id === deviceId
          ? { ...device, trusted: false, revokedAt: Date.now() }
          : device
      );
      return mockRemoteControlStatus();
    }

    case REMOTE_CONTROL_PAIR_DEVICE_COMMAND: {
      const input = args.input && typeof args.input === "object" && !Array.isArray(args.input)
        ? args.input as Record<string, unknown>
        : {};
      const endpoint = input.androidEndpoint && typeof input.androidEndpoint === "object" && !Array.isArray(input.androidEndpoint)
        ? input.androidEndpoint as Record<string, unknown>
        : {};
      const now = Date.now();
      const device = {
        id: `mock-android-${now}`,
        kind: "android",
        displayName: String(input.deviceName ?? "Android device"),
        endpointId: String(endpoint.endpointId ?? "mock-android-endpoint"),
        protocolVersion: Number(input.protocolVersion ?? 1),
        trusted: true,
        firstPairedAt: now,
        lastSeenAt: now,
        revokedAt: null,
      };
      remoteControlDevices = [device, ...remoteControlDevices];
      remoteControlTicket = null;
      return { ...device };
    }

    case PROJECT_CREATE_COMMAND: {
      const name = String(args.name || "未命名项目");
      const cwd = typeof args.cwd === "string" ? args.cwd : null;
      const project: ProjectRow = {
        id: `p-${projects.length + 1}`,
        name,
        cwd,
        sessionCount: 0,
        sortOrder: projects.length,
        pinned: false,
      };
      projects.push(project);
      return cloneProject(project);
    }

    case PROJECT_ENSURE_FOLDERS_COMMAND: {
      const paths = Array.isArray(args.paths) ? args.paths.map(String) : [];
      return ensureMockFolderProjects(paths);
    }

    case PROJECT_REMOVE_COMMAND: {
      const id = String(args.id);
      const before = projects.length;
      projects = projects.filter((project) => project.id !== id);
      tasks = tasks.map((task) =>
        task.projectId === id ? { ...task, projectId: null } : task
      );
      const removedMilestoneIds = new Set(
        milestones
          .filter((milestone) => milestone.projectId === id)
          .map((milestone) => milestone.id),
      );
      milestones = milestones.filter((milestone) => milestone.projectId !== id);
      taskMilestoneLinks = taskMilestoneLinks.filter((link) =>
        !removedMilestoneIds.has(link.milestoneId)
      );
      refreshSessionCounts();
      return projects.length !== before;
    }

    case PROJECT_REORDER_COMMAND: {
      const orderedIds = Array.isArray(args.orderedIds) ? args.orderedIds.map(String) : [];
      projects = projects.map((project) => {
        const index = orderedIds.indexOf(project.id);
        return index >= 0 ? { ...project, sortOrder: index } : project;
      });
      return undefined;
    }

    case MILESTONE_LIST_COMMAND: {
      return getProjectRoadmap(String(args.projectId));
    }

    case MILESTONE_CREATE_COMMAND: {
      const projectId = String(args.projectId);
      const title = String(args.title ?? "").trim();
      if (!title) throw new Error("milestone_create: 标题不能为空");
      if (!projects.some((project) => project.id === projectId)) {
        throw new Error("milestone_create: 项目不存在");
      }
      const row: MilestoneRow = {
        id: `m-${milestones.length + 1}`,
        projectId,
        title,
        description: "",
        status: "upcoming",
        dueDate: null,
        order: milestones.filter((milestone) => milestone.projectId === projectId).length,
        createdAt: Date.now(),
      };
      milestones = [...milestones, row];
      return cloneMilestone(row);
    }

    case MILESTONE_UPDATE_COMMAND: {
      const id = String(args.id);
      const title = typeof args.title === "string" ? args.title.trim() : null;
      const description = typeof args.description === "string" ? args.description.trim() : null;
      const status = typeof args.status === "string" ? args.status : null;
      const dueDate = typeof args.dueDate === "number"
        ? args.dueDate
        : args.clearDueDate === true
          ? null
          : undefined;
      if (title !== null && !title) throw new Error("milestone_update: 标题不能为空");
      if (status !== null && !isMilestoneStatus(status)) {
        throw new Error(`milestone_update: 无效状态：${status}`);
      }
      let changed = false;
      milestones = milestones.map((milestone) => {
        if (milestone.id !== id) return milestone;
        changed = true;
        const nextStatus = status ?? milestone.status;
        return {
          ...milestone,
          title: title ?? milestone.title,
          description: description ?? milestone.description,
          status: nextStatus,
          dueDate: dueDate === undefined ? milestone.dueDate : dueDate,
        };
      });
      if (!changed) throw new Error("milestone_update: milestone 不存在");
      return undefined;
    }

    case MILESTONE_DELETE_COMMAND: {
      const id = String(args.id);
      const before = milestones.length;
      milestones = milestones.filter((milestone) => milestone.id !== id);
      taskMilestoneLinks = taskMilestoneLinks.filter((link) => link.milestoneId !== id);
      return milestones.length !== before;
    }

    case MILESTONE_REORDER_COMMAND: {
      const projectId = String(args.projectId);
      const orderedIds = Array.isArray(args.orderedIds) ? args.orderedIds.map(String) : [];
      milestones = milestones.map((milestone) => {
        if (milestone.projectId !== projectId) return milestone;
        const index = orderedIds.indexOf(milestone.id);
        return index >= 0 ? { ...milestone, order: index } : milestone;
      });
      return undefined;
    }

    case MILESTONE_SET_TASKS_COMMAND: {
      const milestoneId = String(args.milestoneId);
      const taskIds = Array.isArray(args.taskIds) ? Array.from(new Set(args.taskIds.map(String))) : [];
      const milestone = milestones.find((item) => item.id === milestoneId);
      if (!milestone) throw new Error("milestone_set_tasks: milestone 不存在");
      for (const taskId of taskIds) {
        const task = tasks.find((item) => item.id === taskId);
        if (!task || task.archived || task.projectId !== milestone.projectId) {
          throw new Error(`milestone_set_tasks: 任务不属于当前项目：${taskId}`);
        }
      }
      taskMilestoneLinks = [
        ...taskMilestoneLinks.filter((link) => link.milestoneId !== milestoneId),
        ...taskIds.map((taskId) => ({ taskId, milestoneId })),
      ];
      return taskIds.map((taskId) => ({ taskId, milestoneId }));
    }

    case POPUP_GET_WINDOW_SETTINGS_COMMAND:
      return { ...popupWindowSettings };

    case POPUP_SET_WINDOW_SETTINGS_COMMAND: {
      if (nextPopupSettingsError) {
        const message = nextPopupSettingsError;
        nextPopupSettingsError = null;
        throw new Error(message);
      }
      const input = args.settings && typeof args.settings === "object" && !Array.isArray(args.settings)
        ? args.settings as Record<string, unknown>
        : {};
      const shortcut = typeof input.shortcut === "string" && input.shortcut.trim()
        ? input.shortcut.trim()
        : null;
      popupWindowSettings = { shortcut };
      return undefined;
    }

    case POPUP_REMEMBER_LAST_PROJECT_COMMAND: {
      const projectId = String(args.projectId ?? "");
      if (projects.some((project) => project.id === projectId)) {
        popupLastProjectId = projectId;
      }
      return undefined;
    }

    case POPUP_OPEN_NEW_CHAT_COMMAND: {
      const projectId = typeof args.projectId === "string" ? args.projectId : null;
      if (projectId) popupLastProjectId = projectId;
      return undefined;
    }

    case POPUP_OPEN_TASK_COMMAND: {
      const projectId = typeof args.projectId === "string" ? args.projectId : null;
      if (projectId) popupLastProjectId = projectId;
      return undefined;
    }

    case POPUP_OPEN_CHILD_QUESTION_COMMAND: {
      const projectId = typeof args.projectId === "string" ? args.projectId : null;
      if (projectId) popupLastProjectId = projectId;
      return undefined;
    }

    case POPUP_FOCUS_MAIN_COMMAND:
      emitTauriEvent(MAIN_NAVIGATE_EVENT_NAME, createAppNavigateEvent(String(args.route ?? "/")));
      return undefined;

    case TASK_LIST_COMMAND: {
      const projectId = args.projectId as string | null | undefined;
      return tasks
        .filter((task) => !task.archived && task.projectId === (projectId ?? null))
        .map(cloneTask)
        .sort((a, b) =>
          Number(b.pinned) - Number(a.pinned) || a.sortOrder - b.sortOrder
        );
    }

    case TASK_LIST_SIDEBAR_CONVERSATIONS_COMMAND:
      return tasks
        .filter((task) => !task.archived)
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
        }));

    case TASK_GET_COMMAND: {
      const id = String(args.id);
      const row = tasks.find((task) => !task.archived && task.id === id);
      return row ? cloneTask(row) : null;
    }

    case TASK_HANDOFF_GET_COMMAND:
      return null;

    case TASK_UPDATE_COMMAND: {
      const id = String(args.id);
      const title = typeof args.title === "string" ? args.title : null;
      const status = typeof args.status === "string" ? args.status : null;
      let changedProjectId: string | null = null;
      let changed = false;
      tasks = tasks.map((task) => {
        if (task.id !== id || task.archived) return task;
        changedProjectId = task.projectId;
        changed = true;
        return {
          ...task,
          title: title ?? task.title,
          titleSource: title === null ? task.titleSource : "manual",
          status: status ?? task.status,
        };
      });
      if (changed) {
        emitTauriEvent(TASKS_CHANGED_EVENT_NAME, createTasksChangedEvent(changedProjectId));
      }
      return undefined;
    }

    case TASK_TOGGLE_PIN_COMMAND: {
      const id = String(args.id);
      let pinned = false;
      tasks = tasks.map((task) => {
        if (task.id !== id) return task;
        pinned = !task.pinned;
        return { ...task, pinned };
      });
      return pinned;
    }

    case TASK_CREATE_COMMAND: {
      const projectId = typeof args.projectId === "string" ? args.projectId : null;
      const title = String(args.title ?? "新对话");
      const status = String(args.status ?? "draft");
      const dependsOn = Array.isArray(args.dependsOn) ? args.dependsOn.map(String) : [];
      const id = `mock-task-${tasks.length + 1}`;
      const row: TaskRow = {
        id,
        projectId,
        sessionId: id,
        title,
        titleSource: "auto",
        status,
        createdAt: Date.now(),
        parentId: typeof args.parentId === "string" ? args.parentId : null,
        dependsOn,
        sortOrder: tasks.filter((task) => task.projectId === projectId).length,
        pinned: false,
      };
      tasks = [row, ...tasks];
      refreshSessionCounts();
      emitTauriEvent(TASKS_CHANGED_EVENT_NAME, createTasksChangedEvent(projectId));
      return cloneTask(row);
    }

    case TASK_PROMOTE_COMMAND: {
      const id = String(args.id);
      const projectId = typeof args.projectId === "string" ? args.projectId : null;
      const title = String(args.title ?? "新对话");
      const dependsOn = Array.isArray(args.dependsOn) ? args.dependsOn.map(String) : [];
      const sortOrder = tasks.filter((task) => task.projectId === projectId).length;
      const row: TaskRow = {
        id,
        projectId,
        sessionId: id,
        title,
        titleSource: "auto",
        status: "running",
        createdAt: Date.now(),
        parentId: typeof args.parentId === "string" ? args.parentId : null,
        dependsOn,
        sortOrder,
        pinned: false,
      };
      tasks = [row, ...tasks.filter((task) => task.id !== id)];
      refreshSessionCounts();
      emitTauriEvent(TASKS_CHANGED_EVENT_NAME, createTasksChangedEvent(projectId));
      return cloneTask(row);
    }

    case TASK_REORDER_COMMAND: {
      const projectId = args.projectId as string | null | undefined;
      const targetProjectId = projectId ?? null;
      const orderedIds = Array.isArray(args.orderedIds) ? args.orderedIds.map(String) : [];
      tasks = tasks.map((task) => {
        if (task.projectId !== targetProjectId) return task;
        const index = orderedIds.indexOf(task.id);
        return index >= 0 ? { ...task, sortOrder: index } : task;
      });
      return undefined;
    }

    case TASK_REPARENT_COMMAND: {
      const taskId = String(args.taskId);
      const newProjectId = args.newProjectId === null || args.newProjectId === undefined
        ? null
        : String(args.newProjectId);
      const newParentId = args.newParentId === null || args.newParentId === undefined
        ? null
        : String(args.newParentId);
      tasks = tasks.map((task) =>
        task.id === taskId
          ? {
              ...task,
              projectId: newProjectId,
              parentId: newParentId,
              sortOrder: Number.MAX_SAFE_INTEGER,
            }
          : task.parentId === taskId && task.projectId !== newProjectId
            ? { ...task, projectId: newProjectId }
          : task
      );
      return undefined;
    }

    case TASK_UPDATE_DEPENDENCIES_COMMAND: {
      const id = String(args.id);
      const dependsOn = Array.isArray(args.dependsOn)
        ? [...new Set(args.dependsOn.map(String).filter((dep) => dep && dep !== id))]
        : [];
      let row: TaskRow | null = null;
      tasks = tasks.map((task) => {
        if (task.id !== id) return task;
        row = { ...task, dependsOn };
        return row;
      });
      if (!row) throw new Error("task_update_dependencies: 任务不存在");
      emitTauriEvent(TASKS_CHANGED_EVENT_NAME, createTasksChangedEvent(row.projectId));
      return cloneTask(row);
    }

    case TASK_ARCHIVE_COMMAND: {
      const id = String(args.id);
      const before = tasks.length;
      tasks = tasks.filter((task) => task.id !== id);
      refreshSessionCounts();
      return tasks.length !== before;
    }

    case TASK_ARCHIVE_PROJECT_COMMAND: {
      const projectId = String(args.projectId);
      const count = tasks.filter((task) => task.projectId === projectId).length;
      tasks = tasks.filter((task) => task.projectId !== projectId);
      refreshSessionCounts();
      return count;
    }

    case WORKTREE_LIST_COMMAND: {
      const baseRepoPath = String(args.baseRepoPath ?? "D:\\PROJECT\\workspace\\Lilia");
      return [
        {
          path: baseRepoPath,
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
          path: `${baseRepoPath}-task-worktree`,
          head: null,
          branch: "lilia/mock-task",
          bare: false,
          detached: false,
          prunable: false,
          locked: false,
          isMain: false,
          isTaskBound: false,
        },
      ];
    }

    case WORKTREE_GET_FOR_TASK_COMMAND:
      return taskWorktrees[String(args.taskId)] ?? null;

    case WORKTREE_CLEAR_TASK_COMMAND:
      delete taskWorktrees[String(args.taskId)];
      return undefined;

    case WORKTREE_CREATE_FOR_TASK_COMMAND: {
      const input = args.input && typeof args.input === "object" && !Array.isArray(args.input)
        ? args.input as Record<string, unknown>
        : {};
      const taskId = String(input.taskId ?? "");
      const baseRepoPath = String(input.baseRepoPath ?? "D:\\PROJECT\\workspace\\Lilia");
      const saved = {
        taskId,
        projectId: input.projectId === null || input.projectId === undefined ? null : String(input.projectId),
        baseRepoPath,
        worktreePath: `${baseRepoPath}-task-worktree`,
        branchName: `lilia/${taskId || "mock-task"}`,
        baseBranch: "main",
        status: "active",
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };
      taskWorktrees[taskId] = saved;
      return saved;
    }

    case WORKTREE_ATTACH_TASK_COMMAND: {
      const input = args.input && typeof args.input === "object" && !Array.isArray(args.input)
        ? args.input as Record<string, unknown>
        : {};
      const taskId = String(input.taskId ?? "");
      const saved = {
        taskId,
        projectId: input.projectId === null || input.projectId === undefined ? null : String(input.projectId),
        baseRepoPath: String(input.baseRepoPath ?? "D:\\PROJECT\\workspace\\Lilia"),
        worktreePath: String(input.worktreePath ?? "D:\\PROJECT\\workspace\\Lilia-task-worktree"),
        branchName: "lilia/mock-task",
        baseBranch: "main",
        status: "active",
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };
      taskWorktrees[taskId] = saved;
      return saved;
    }

    case WORKTREE_CLEANUP_ARCHIVE_COMMAND: {
      const taskId = String(args.taskId);
      delete taskWorktrees[taskId];
      tasks = tasks.filter((task) => task.id !== taskId);
      refreshSessionCounts();
      return { merged: false, removed: true, archived: true, message: "mock cleaned" };
    }

    case WORKTREE_MERGE_DELETE_ARCHIVE_COMMAND: {
      const taskId = String(args.taskId);
      delete taskWorktrees[taskId];
      tasks = tasks.filter((task) => task.id !== taskId);
      refreshSessionCounts();
      return { merged: true, removed: true, archived: true, message: "mock merged" };
    }

    case AUTOMATION_LIST_WORKFLOWS_COMMAND:
      return automations.map(cloneAutomationWorkflow);

    case AUTOMATION_SAVE_DRAFT_COMMAND: {
      const input = args.input && typeof args.input === "object" && !Array.isArray(args.input)
        ? args.input as Record<string, unknown>
        : {};
      const id = typeof input.id === "string" && input.id ? input.id : `auto-${automations.length + 1}`;
      const now = Date.now();
      const scopeInput = input.scope && typeof input.scope === "object" && !Array.isArray(input.scope)
        ? input.scope as Partial<AutomationScopeRow>
        : {};
      const nextScope = normalizeAutomationScope(scopeInput);
      const nodes = Array.isArray(input.nodes)
        ? input.nodes.map((node, index) => {
          const row = node && typeof node === "object" && !Array.isArray(node)
            ? node as Record<string, unknown>
            : {};
          return {
            id: typeof row.id === "string" ? row.id : `node-${index}`,
            kind: row.kind === "trigger" || row.kind === "agent" || row.kind === "logic" || row.kind === "tool" || row.kind === "human"
              ? row.kind
              : "tool",
            title: typeof row.title === "string" ? row.title : "节点",
            position: row.position && typeof row.position === "object" && !Array.isArray(row.position)
              ? {
                x: Number((row.position as Record<string, unknown>).x ?? 0),
                y: Number((row.position as Record<string, unknown>).y ?? 0),
              }
              : { x: 0, y: 0 },
            config: row.config && typeof row.config === "object" && !Array.isArray(row.config)
              ? { ...row.config as Record<string, unknown> }
              : {},
          } satisfies AutomationNodeRow;
        })
        : [];
      const edges = Array.isArray(input.edges)
        ? input.edges.map((edge, index) => {
          const row = edge && typeof edge === "object" && !Array.isArray(edge)
            ? edge as Record<string, unknown>
            : {};
          return {
            id: typeof row.id === "string" ? row.id : `edge-${index}`,
            source: String(row.source ?? ""),
            target: String(row.target ?? ""),
            sourceHandle: typeof row.sourceHandle === "string" ? row.sourceHandle : null,
            targetHandle: typeof row.targetHandle === "string" ? row.targetHandle : null,
          } satisfies AutomationEdgeRow;
        })
        : [];
      const existing = automations.find((item) => item.id === id);
      const workflow: AutomationWorkflowRow = {
        id,
        name: String(input.name ?? "新自动化"),
        enabled: existing?.enabled ?? false,
        scope: nextScope,
        draft: { nodes, edges, scope: nextScope },
        publishedVersionId: existing?.publishedVersionId ?? null,
        createdAt: existing?.createdAt ?? now,
        updatedAt: now,
      };
      automations = [workflow, ...automations.filter((item) => item.id !== id)];
      emitTauriEvent(AUTOMATION_CHANGED_EVENT_NAME, createAutomationChangedEvent(id));
      return cloneAutomationWorkflow(workflow);
    }

    case AUTOMATION_PUBLISH_COMMAND: {
      const id = String(args.id);
      const workflow = automations.find((item) => item.id === id);
      if (!workflow) throw new Error("自动化不存在");
      automationVersionSeq += 1;
      const versionId = `${id}-v${automationVersionSeq + 1}`;
      workflow.publishedVersionId = versionId;
      workflow.updatedAt = Date.now();
      emitTauriEvent(AUTOMATION_CHANGED_EVENT_NAME, createAutomationChangedEvent(id));
      return {
        id: versionId,
        workflowId: id,
        version: automationVersionSeq + 1,
        snapshot: workflow.draft,
        createdAt: Date.now(),
      };
    }

    case AUTOMATION_DELETE_WORKFLOW_COMMAND: {
      const id = String(args.id);
      const before = automations.length;
      automations = automations.filter((item) => item.id !== id);
      if (automations.length === before) throw new Error("自动化不存在");
      const deletedRunIds = automationRuns
        .filter((run) => run.workflowId === id)
        .map((run) => run.id);
      automationRuns = automationRuns.filter((run) => run.workflowId !== id);
      for (const runId of deletedRunIds) delete automationRunNodes[runId];
      emitTauriEvent(AUTOMATION_CHANGED_EVENT_NAME, createAutomationChangedEvent(id));
      return undefined;
    }

    case AUTOMATION_SET_ENABLED_COMMAND: {
      const id = String(args.id);
      const enabled = args.enabled === true;
      automations = automations.map((workflow) =>
        workflow.id === id ? { ...workflow, enabled, updatedAt: Date.now() } : workflow
      );
      emitTauriEvent(AUTOMATION_CHANGED_EVENT_NAME, createAutomationChangedEvent(id));
      return undefined;
    }

    case AUTOMATION_RUN_ONCE_COMMAND: {
      const id = String(args.id);
      const workflow = automations.find((item) => item.id === id);
      if (!workflow?.publishedVersionId) throw new Error("运行前需要先发布");
      automationRunSeq += 1;
      const now = Date.now();
      const waitingNode = workflow.draft.nodes.find((node) => node.kind === "human");
      const waitingAgent = waitingNode ? null : workflow.draft.nodes.find((node) => node.kind === "agent");
      const run: AutomationRunRow = {
        id: `run-${automationRunSeq}`,
        workflowId: id,
        workflowVersionId: workflow.publishedVersionId,
        status: waitingNode ? "waiting_user" : waitingAgent ? "running" : "succeeded",
        trigger: {
          id: `signal-${automationRunSeq}`,
          kind: "manual",
          projectId: workflow.scope.projectIds[0] ?? null,
          taskId: "t-002",
          backend: workflow.scope.backends[0] ?? "claude",
          eventKind: "manual",
          automationRunId: null,
          payload: {},
          createdAt: now,
        },
        scope: workflow.scope,
        startedAt: now,
        finishedAt: waitingNode || waitingAgent ? null : now,
        error: null,
      };
      automationRuns = [run, ...automationRuns];
      automationRunNodes[run.id] = workflow.draft.nodes.map((node) => ({
        id: `${run.id}:${node.id}`,
        runId: run.id,
        nodeId: node.id,
        status: node.id === waitingNode?.id
          ? "waiting_user"
          : node.id === waitingAgent?.id
            ? "running"
            : "succeeded",
        input: { trigger: run.trigger },
        output: node.id === waitingNode?.id
          ? { waitingUser: true, prompt: String(node.config.prompt ?? DEFAULT_AUTOMATION_HUMAN_PROMPT) }
          : node.id === waitingAgent?.id
            ? { waitingAgent: true, taskId: "t-002", turnId: `turn-${run.id}` }
          : { ok: true },
        error: null,
        startedAt: now,
        finishedAt: node.id === waitingNode?.id || node.id === waitingAgent?.id ? null : now,
      }));
      emitTauriEvent(AUTOMATION_RUN_STARTED_EVENT_NAME, createAutomationRunEvent(cloneAutomationRun(run)));
      emitTauriEvent(
        waitingNode || waitingAgent ? AUTOMATION_RUN_UPDATED_EVENT_NAME : AUTOMATION_RUN_FINISHED_EVENT_NAME,
        createAutomationRunEvent(cloneAutomationRun(run)),
      );
      return cloneAutomationRun(run);
    }

    case AUTOMATION_RESUME_RUN_COMMAND: {
      const runId = String(args.runId);
      const run = automationRuns.find((item) => item.id === runId);
      if (!run) throw new Error("运行不存在");
      const now = Date.now();
      run.status = "succeeded";
      run.finishedAt = now;
      automationRunNodes[runId] = (automationRunNodes[runId] ?? []).map((node) =>
        node.status === "waiting_user"
          ? {
            ...node,
            status: "succeeded",
            output: { waitingUser: false, confirmed: true },
            finishedAt: now,
          }
          : node,
      );
      emitTauriEvent(AUTOMATION_RUN_FINISHED_EVENT_NAME, createAutomationRunEvent(cloneAutomationRun(run)));
      return cloneAutomationRun(run);
    }

    case AUTOMATION_LIST_RUNS_COMMAND: {
      const workflowId = typeof args.workflowId === "string" ? args.workflowId : null;
      return automationRuns
        .filter((run) => !workflowId || run.workflowId === workflowId)
        .map(cloneAutomationRunSummary);
    }

    case AUTOMATION_GET_RUN_COMMAND: {
      const runId = String(args.runId);
      const run = automationRuns.find((item) => item.id === runId);
      if (!run) return null;
      return {
        run: cloneAutomationRun(run),
        nodes: (automationRunNodes[runId] ?? []).map(cloneAutomationRunNode),
      };
    }

    case CHAT_CHECK_ENV_COMMAND:
      return {
        nodeAvailable,
        codexCliAvailable: codexAppServerStatus.failureKind !== "missingCli",
        codexAppServer: {
          ...codexAppServerStatus,
          issues: [...codexAppServerStatus.issues],
        },
        routerModes: {
          ...routerModes,
        },
        backends: {
          claude: mockBackendEnvStatus("claude"),
          codex: mockBackendEnvStatus("codex"),
        },
      };

    case PROVIDER_CODEX_APP_SERVER_CHECK_UPDATE_COMMAND:
      return {
        ...codexAppServerStatus,
        issues: [...codexAppServerStatus.issues],
        releaseNotes: [...codexAppServerStatus.releaseNotes],
      };

    case PROVIDER_CODEX_APP_SERVER_INSTALL_UPDATE_COMMAND:
      if (
        codexAppServerStatus.updateState !== "ready" &&
        !(codexAppServerStatus.updateState === "failed" && codexAppServerStatus.preparedVersion)
      ) {
        throw new Error("Codex app-server 更新尚未准备好。");
      }
      codexAppServerStatus = {
        ...codexAppServerStatus,
        managed: true,
        updateAvailable: false,
        updateError: null,
        updateState: "idle",
        preparedVersion: null,
        updateProgressPercent: null,
        installPath: codexAppServerStatus.installPath ?? "C:/Users/me/.lilia/runtime/codex/bin/codex.exe",
        issues: [...codexAppServerStatus.issues],
        releaseNotes: [...codexAppServerStatus.releaseNotes],
      };
      return {
        ...codexAppServerStatus,
        issues: [...codexAppServerStatus.issues],
        releaseNotes: [...codexAppServerStatus.releaseNotes],
      };

    case PROVIDER_CODEX_ACCOUNT_START_LOGIN_COMMAND:
      return undefined;

    case PROVIDER_GET_ACTIVE_BACKEND_COMMAND:
      return activeBackend;

    case PROVIDER_SET_ACTIVE_BACKEND_COMMAND:
      activeBackend = normalizeBackend(args.backend);
      return undefined;

    case PROVIDER_GET_CONFIG_COMMAND: {
      const backend = normalizeBackend(args.backend);
      return cloneProviderConfig(backend);
    }

    case PROVIDER_SET_CONFIG_COMMAND: {
      const input = args.config && typeof args.config === "object" && !Array.isArray(args.config)
        ? args.config as Record<string, unknown>
        : {};
      const backend = normalizeBackend(input.backend);
      providerConfigs[backend] = {
        backend,
        baseUrl: typeof input.baseUrl === "string" && input.baseUrl.trim()
          ? input.baseUrl.trim()
          : null,
        apiKey: null,
        hasApiKey: input.clearApiKey === true
          ? false
          : typeof input.apiKey === "string" && input.apiKey.trim()
            ? true
            : providerConfigs[backend].hasApiKey,
      };
      return undefined;
    }

    case ASSISTANT_AI_GET_CONFIG_COMMAND:
      return { ...assistantAIConfig, apiKey: null };

    case ASSISTANT_AI_SET_CONFIG_COMMAND: {
      const config = args.config && typeof args.config === "object" && !Array.isArray(args.config)
        ? args.config as Record<string, unknown>
        : {};
      assistantAIConfig = {
        baseUrl: typeof config.baseUrl === "string" && config.baseUrl.trim() ? config.baseUrl.trim() : null,
        apiKey: null,
        model: typeof config.model === "string" && config.model.trim() ? config.model.trim() : null,
        modelPool: Array.isArray(config.modelPool)
          ? config.modelPool
            .filter((item): item is Record<string, unknown> => item && typeof item === "object" && !Array.isArray(item))
            .map((item) => ({
              id: String(item.id ?? "").trim(),
              label: String(item.label ?? item.id ?? "").trim(),
              source: item.source === "legacy" ? "legacy" as const : "remote" as const,
              backend: item.backend === "claude" ? "claude" as const : "codex" as const,
            }))
            .filter((item) => item.id)
          : assistantAIConfig.modelPool,
        codexAccountSparkEnabled: config.codexAccountSparkEnabled === true,
        hasApiKey: config.clearApiKey === true
          ? false
          : typeof config.apiKey === "string" && config.apiKey.trim()
            ? true
            : assistantAIConfig.hasApiKey,
      };
      return undefined;
    }

    case ASSISTANT_AI_FETCH_MODELS_COMMAND:
      return {
        ok: true,
        error: null,
        models: [
          { id: "remote-mini", label: "remote-mini", source: "remote", backend: "codex" },
          { id: "remote-pro", label: "remote-pro", source: "remote", backend: "codex" },
        ],
      };

    case MODEL_FEATURE_LIST_MODEL_OPTIONS_COMMAND:
      return assistantAIConfig.modelPool.map((item) => ({ ...item }));

    case MODEL_FEATURE_GET_SETTINGS_COMMAND:
      return {
        ...modelFeatureSettings,
        chat: { ...modelFeatureSettings.chat },
      };

    case MODEL_FEATURE_SET_SETTINGS_COMMAND: {
      const settings = args.settings && typeof args.settings === "object" && !Array.isArray(args.settings)
        ? args.settings as typeof modelFeatureSettings
        : modelFeatureSettings;
      modelFeatureSettings = {
        chat: {
          light: settings.chat?.light ?? null,
          normal: settings.chat?.normal ?? null,
          deep: settings.chat?.deep ?? null,
        },
        title: settings.title ?? null,
        suggestion: settings.suggestion ?? null,
        promptRouter: settings.promptRouter ?? null,
        promptOptimize: settings.promptOptimize ?? null,
        autoTurnDecision: settings.autoTurnDecision ?? null,
      };
      return undefined;
    }

    case ASSISTANT_AI_TEST_CONNECTION_COMMAND:
      return { ok: false, error: "baseUrl / apiKey / model 必须全部填写", models: null, modelMatched: null };

    case ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND: {
      const input = args.input && typeof args.input === "object" && !Array.isArray(args.input)
        ? args.input as Record<string, unknown>
        : {};
      const prompt = typeof input.prompt === "string" ? input.prompt : "";
      const wantsReview = /\breview\b|审查|评审/.test(prompt);
      return {
        optimizedPrompt: `优化后：${prompt}`,
        route: {
          scenario: wantsReview ? "review" : "general_task_optimize",
          workflow: wantsReview
            ? {
              type: "lilia_review",
              target: { type: "uncommittedChanges" },
              delivery: "inline",
            }
            : null,
          confidence: wantsReview ? 0.86 : 0.5,
          reason: wantsReview ? "用户明确请求 review" : "普通提示词优化",
          signals: wantsReview ? ["review"] : [],
        },
      };
    }

    case CONVERSATION_SUGGESTIONS_GET_SETTINGS_COMMAND:
      return { ...conversationSuggestionSettings };

    case CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND:
      if (!conversationSuggestionSettings.enabled) return { sources: [], localGit: null };
      if (conversationSuggestionSourcesOverride) return conversationSuggestionSourcesOverride;
      return { sources: ["task"], localGit: null };

    case CONVERSATION_SUGGESTIONS_SET_SETTINGS_COMMAND: {
      const settings = args.settings && typeof args.settings === "object" && !Array.isArray(args.settings)
        ? args.settings as Record<string, unknown>
        : {};
      conversationSuggestionSettings = {
        enabled: settings.enabled !== false,
        source: settings.source === "provider" ? "provider" : "assistant-ai",
      };
      return undefined;
    }

    case CONVERSATION_SUGGESTIONS_GET_COMMAND:
      if (!conversationSuggestionSettings.enabled) return [];
      if (conversationSuggestionsDelayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, conversationSuggestionsDelayMs));
      }
      if (nextConversationSuggestionsError) {
        const message = nextConversationSuggestionsError;
        nextConversationSuggestionsError = null;
        throw new Error(message);
      }
      if (conversationSuggestionsOverride) return conversationSuggestionsOverride;
      return [
        {
          id: "sg-1",
          projectId: typeof args.projectId === "string" ? args.projectId : null,
          taskIds: ["t-002"],
          source: "task",
          githubActivities: [],
          localGitContexts: [],
          codexThreads: [],
          summary: "补齐建议缓存测试",
          reason: "最近对话已经接入了设置与空草稿入口，但缓存失效路径还需要验证。",
          prompt: "请检查新对话建议的缓存命中、强制刷新和最近 timeline 更新失效逻辑，并补齐最小测试。",
          generatedAt: Date.now(),
        },
        {
          id: "sg-2",
          projectId: typeof args.projectId === "string" ? args.projectId : null,
          taskIds: ["t-002"],
          source: "github",
          githubActivities: [
            {
              id: "gh-1",
              repoFullName: "sena-nana/LiliaCode",
              kind: "pull_request",
              title: "PR #1: 优化空状态体验",
              url: "https://github.com/sena-nana/LiliaCode/pull/1",
            },
          ],
          localGitContexts: [],
          codexThreads: [],
          summary: "优化空状态体验",
          reason: "空草稿页现在可以显示建议，下一步可以确认加载失败和无建议时的轻量反馈。",
          prompt: "请优化空白新对话页的建议加载失败和无建议状态，保持不阻塞输入并符合现有样式。",
          generatedAt: Date.now(),
        },
      ];

    case HISTORY_IMPORT_SEARCH_COMMAND: {
      const input = args.input && typeof args.input === "object" && !Array.isArray(args.input)
        ? args.input as Record<string, unknown>
        : {};
      const provider = input.provider === "claude" ? "claude" : "codex";
      if (provider === "claude") {
        return {
          items: [],
          nextCursor: null,
        };
      }
      const term = String(input.searchTerm ?? "").trim().toLowerCase();
      const includeArchived = input.archived === true;
      const limit = typeof input.limit === "number" ? Math.max(1, input.limit) : 20;
      const cursor = typeof input.cursor === "string" && input.cursor.startsWith("offset:")
        ? Number(input.cursor.slice("offset:".length))
        : 0;
      const filtered = codexThreads
        .filter((thread) => includeArchived || !thread.archived)
        .filter((thread) => {
          if (!term) return true;
          return [
            thread.title,
            thread.id,
            thread.preview,
            thread.model,
            thread.status,
          ].some((value) => String(value ?? "").toLowerCase().includes(term));
        })
        .sort((a, b) => (b.updatedAt ?? 0) - (a.updatedAt ?? 0));
      const page = filtered.slice(cursor, cursor + limit);
      const nextOffset = cursor + limit;
      return {
        items: page.map((thread) => ({ ...thread, provider: "codex" })),
        nextCursor: nextOffset < filtered.length ? `offset:${nextOffset}` : null,
      };
    }

    case HISTORY_IMPORT_RUNTIME_STATES_COMMAND: {
      return Object.entries(codexTaskSessions).flatMap(([taskId, threadId]) => {
        const task = tasks.find((row) => row.id === taskId && !row.archived);
        if (!task) return [];
        const queuedCount = chatQueued[taskId]?.length ?? 0;
        const running = chatRunning[taskId] === true;
        return [{
          itemId: threadId,
          taskId,
          taskTitle: task.title,
          projectId: task.projectId,
          running,
          queued: queuedCount > 0,
          pending: running || queuedCount > 0,
          queuedCount,
        }];
      });
    }

    case HISTORY_IMPORT_CLEAN_BACKGROUND_TERMINALS_COMMAND: {
      const itemId = String(args.itemId ?? "").trim();
      if (!itemId) throw new Error("Codex threadId 不能为空");
      cleanedCodexThreads.push(itemId);
      return undefined;
    }

    case ROUTER_GET_MODE_COMMAND: {
      const backend = normalizeBackend(args.backend);
      return routerModes[backend];
    }

    case ROUTER_SET_MODE_COMMAND: {
      const backend = normalizeBackend(args.backend);
      const mode = args.mode === "codex-account" ||
        args.mode === "direct" ||
        args.mode === "cc-switch"
        ? args.mode
        : "api";
      routerModes[backend] = normalizeMockRouterMode(backend, mode);
      return undefined;
    }

    case AGENT_TIMELINE_LIST_COMMAND: {
      if (agentTimelineDelayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, agentTimelineDelayMs));
      }
      const taskId = String(args.taskId);
      return timelineEvents[taskId] ?? [];
    }

    case AGENT_TIMELINE_CLEAR_TASK_COMMAND: {
      const taskId = String(args.taskId);
      const count = timelineEvents[taskId]?.length ?? 0;
      timelineEvents[taskId] = [];
      return count;
    }

    case QUOTA_USAGE_GET_STATS_COMMAND:
      return quotaUsageStatsOverride ?? createMockQuotaUsageStats(
        args.input && typeof args.input === "object" && !Array.isArray(args.input)
          ? args.input as Record<string, unknown>
          : {},
      );

    case QUOTA_USAGE_GET_CODEX_ACCOUNT_STATUS_COMMAND:
      return codexAccountQuotaStatusOverride ?? createMockCodexAccountQuotaStatus();

    case QUOTA_USAGE_CONSUME_CODEX_RATE_LIMIT_RESET_CREDIT_COMMAND: {
      const status = codexAccountQuotaStatusOverride ?? createMockCodexAccountQuotaStatus();
      return {
        outcome: "reset",
        status: {
          ...status,
          rateLimitResetCredits: { availableCount: 1 },
        },
      };
    }

    case CHAT_GET_COMPOSER_STATE_COMMAND: {
      const taskId = String(args.taskId);
      if (composerStateHandler) return composerStateHandler(taskId);
      return {
        taskId,
        backend: "claude",
        model: "claude-sonnet-4-6",
        planMode: false,
        goalMode: false,
        permission: "ask",
      };
    }

    case CHAT_GET_RUNTIME_SNAPSHOT_COMMAND: {
      const taskId = String(args.taskId);
      if (runtimeSnapshotDelayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, runtimeSnapshotDelayMs));
      }
      const queuedCount = chatQueued[taskId]?.length ?? 0;
      const running = chatRunning[taskId] === true;
      return {
        taskId,
        phase: running && queuedCount > 0
          ? "running_and_queued"
          : running
            ? "running"
            : queuedCount > 0
              ? "queued"
              : "idle",
        backend: running ? activeBackend : null,
        turnId: running ? currentChatTurnId(taskId) : null,
        queuedCount,
        pendingRollback: false,
        pendingResetCleanup: false,
        contextUsage: null,
        rollback: null,
        ...(runtimeSnapshotOverrides[taskId] ?? {}),
      };
    }

    case PLUGINS_OVERVIEW_COMMAND: {
      const projectCwd = typeof args.projectCwd === "string" ? args.projectCwd : null;
      const projectSkillName = projectCwd && projectCwdKey(projectCwd) === projectCwdKey("D:\\PROJECT\\workspace\\tools")
        ? "tools-skill"
        : "project-skill";
      const projectSkillPath = projectCwd
        ? `${projectCwd}\\.claude\\skills\\${projectSkillName}\\SKILL.md`
        : null;
      return {
        skills: [
          {
            backend: "claude",
            scope: "user",
            name: "mock-skill",
            description: "测试用 Skill",
            enabled: true,
            path: "C:\\Users\\mock\\.claude\\skills\\mock-skill\\SKILL.md",
          },
          ...(projectSkillPath
            ? [{
                backend: "claude",
                scope: "project",
                name: projectSkillName,
                description: "项目 Skill",
                enabled: true,
                path: projectSkillPath,
              }]
            : []),
        ],
        packages: claudePlugins.map((plugin) => ({ ...plugin })),
        mcpServers: [
          ...claudeMcpServers.map((server) => ({
            ...server,
            args: [...server.args],
            envKeys: [...server.envKeys],
          })),
          ...codexMcpServers.map((server) => ({
            ...server,
            args: [...server.args],
            envKeys: [...server.envKeys],
          })),
        ],
        configPaths: {
          claude: "C:\\Users\\mock\\.lilia\\config\\claude-mcp-servers.json",
          codex: "C:\\Users\\mock\\.codex\\config.toml",
        },
        warnings: [],
      };
    }

    case PLUGINS_HOOKS_OVERVIEW_COMMAND:
      return {
        sources: hookSourcesOverview(typeof args.projectCwd === "string" ? args.projectCwd : null),
        warnings: [],
      };

    case PLUGINS_READ_HOOK_SOURCE_COMMAND: {
      const source = args.source as { id?: string };
      const document = source?.id ? hookDocuments[String(source.id)] : undefined;
      if (!document) throw new Error("unknown hook source");
      syncHookDocument(document);
      return cloneHookDocument(document);
    }

    case PLUGINS_UPDATE_HOOK_SOURCE_COMMAND: {
      const source = args.source as { id?: string };
      const input = args.input as {
        handlers?: Array<{
          id?: string;
          event?: string;
          matcher?: string | null;
          type?: string;
          command?: string | null;
          commandWindows?: string | null;
          timeoutSeconds?: number | null;
          statusMessage?: string | null;
          groupAdvancedJson?: string | null;
          advancedJson?: string | null;
        }>;
      };
      const document = source?.id ? hookDocuments[String(source.id)] : undefined;
      if (!document) throw new Error("unknown hook source");
      if (!document.source.editable) throw new Error("当前 hooks source 为只读，不能保存");
      document.source.exists = true;
      document.handlers = Array.isArray(input.handlers)
        ? input.handlers.map((handler, index) => {
          const type = String(handler.type ?? "");
          const supported = document.source.backend !== "codex" || type === "command";
          const executable = supported;
          return makeHookHandler({
            id: String(handler.id ?? `${document.source.id}:${index}`),
            event: String(handler.event ?? ""),
            matcher: typeof handler.matcher === "string" ? handler.matcher : null,
            type,
            command: typeof handler.command === "string" ? handler.command : null,
            commandWindows: typeof handler.commandWindows === "string" ? handler.commandWindows : null,
            timeoutSeconds: typeof handler.timeoutSeconds === "number" ? handler.timeoutSeconds : null,
            statusMessage: typeof handler.statusMessage === "string" ? handler.statusMessage : null,
            groupAdvancedJson: typeof handler.groupAdvancedJson === "string" ? handler.groupAdvancedJson : null,
            advancedJson: typeof handler.advancedJson === "string" ? handler.advancedJson : null,
            supported,
            executable,
            warnings: supported ? [] : [`type=${type} 当前上游解析但不执行。`],
          });
        })
        : [];
      syncHookDocument(document);
      document.rawDocument = buildHookRawDocument(document.handlers);
      document.warnings = document.handlers.flatMap((handler) => handler.warnings);
      document.source.warnings = [...document.warnings];
      return cloneHookDocument(document);
    }

    case PLUGINS_CREATE_HOOK_SOURCE_COMMAND: {
      const backend = String(args.backend);
      const scope = String(args.scope);
      const projectCwd = typeof args.projectCwd === "string" ? args.projectCwd : null;
      const document = editableHookSourceFor(backend, scope, projectCwd);
      if (!document) throw new Error("当前 scope 不支持创建 hooks source");
      document.source.exists = true;
      document.handlers = [];
      document.rawDocument = "{\n  \"hooks\": {}\n}\n";
      syncHookDocument(document);
      return structuredClone(document.source);
    }

    case PLUGINS_SET_SKILL_ENABLED_COMMAND:
    case PLUGINS_DELETE_SKILL_COMMAND:
      return undefined;

    case PLUGINS_DELETE_HOOK_SOURCE_COMMAND: {
      const source = args.source as { id?: string };
      const document = source?.id ? hookDocuments[String(source.id)] : undefined;
      if (!document) throw new Error("unknown hook source");
      if (!document.source.editable) throw new Error("当前 hooks source 为只读，不能删除");
      document.source.exists = false;
      document.handlers = [];
      document.rawDocument = null;
      document.warnings = [];
      document.source.warnings = [];
      syncHookDocument(document);
      return undefined;
    }

    case PLUGINS_SET_HOOK_SOURCE_ENABLED_COMMAND: {
      const source = args.source as { id?: string };
      const enabled = args.enabled === true;
      const document = source?.id ? hookDocuments[String(source.id)] : undefined;
      if (!document) throw new Error("unknown hook source");
      document.source.exists = enabled;
      syncHookDocument(document);
      return structuredClone(document.source);
    }

    case PLUGINS_SET_PACKAGE_ENABLED_COMMAND: {
      if (args.backend !== "claude") throw new Error("unsupported package backend");
      const name = String(args.name);
      const enabled = args.enabled === true;
      claudePlugins = claudePlugins.map((plugin) =>
        plugin.name === name ? { ...plugin, enabled } : plugin
      );
      return undefined;
    }

    case PLUGINS_CREATE_MCP_SERVER_COMMAND: {
      const backend = String(args.backend);
      const input = args.input as {
        name?: string;
        command?: string;
        args?: string[];
        env?: Record<string, string>;
      };
      const server = {
        backend,
        name: String(input.name ?? ""),
        command: String(input.command ?? ""),
        args: Array.isArray(input.args) ? input.args.map(String) : [],
        envKeys: Object.keys(input.env ?? {}),
        enabled: true,
        editable: true,
        transport: "stdio",
      };
      updateMcpServersForBackend(backend, (servers) => [...servers, server]);
      return { ...server, args: [...server.args], envKeys: [...server.envKeys] };
    }

    case PLUGINS_UPDATE_MCP_SERVER_COMMAND: {
      const backend = String(args.backend);
      const name = String(args.name);
      const input = args.input as {
        name?: string;
        command?: string;
        args?: string[];
        env?: Record<string, string>;
        removeEnvKeys?: string[];
      };
      const servers = mcpServersForBackend(backend);
      let updated = servers.find((server) => server.name === name);
      if (!updated) return undefined;
      if (backend === "codex" && !updated.editable) return undefined;
      const removed = new Set(
        Array.isArray(input.removeEnvKeys) ? input.removeEnvKeys.map(String) : [],
      );
      const envKeys = input.env
        ? [
            ...updated.envKeys.filter((key) => !removed.has(key) && !(key in input.env!)),
            ...Object.keys(input.env),
          ]
        : updated.envKeys.filter((key) => !removed.has(key));
      updated = {
        ...updated,
        name: String(input.name ?? updated.name),
        command: String(input.command ?? updated.command),
        args: Array.isArray(input.args) ? input.args.map(String) : updated.args,
        envKeys,
      };
      updateMcpServersForBackend(backend, (servers) =>
        servers.map((server) => (server.name === name ? updated : server)),
      );
      return { ...updated, args: [...updated.args], envKeys: [...updated.envKeys] };
    }

    case PLUGINS_DELETE_MCP_SERVER_COMMAND: {
      const backend = String(args.backend);
      const name = String(args.name);
      updateMcpServersForBackend(backend, (servers) =>
        servers.filter((server) => server.name !== name || (backend === "codex" && !server.editable)),
      );
      return undefined;
    }

    case PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND: {
      const backend = String(args.backend);
      const name = String(args.name);
      const enabled = args.enabled === true;
      updateMcpServersForBackend(backend, (servers) =>
        servers.map((server) =>
          server.name === name && (backend === "claude" || server.editable)
            ? { ...server, enabled }
            : server,
        ),
      );
      return undefined;
    }

    case PLUGINS_OPEN_MCP_CONFIG_COMMAND:
    case PLUGINS_OPEN_HOOK_CONFIG_COMMAND:
      return undefined;

    case CHAT_SET_COMPOSER_STATE_COMMAND:
      return undefined;

    case AGENT_INTERACTION_GET_SETTINGS_COMMAND:
      return { ...agentInteractionSettings };

    case AGENT_INTERACTION_LIST_SUBAGENTS_COMMAND:
      return customSubagents.map((item) => ({ ...item }));

    case AGENT_INTERACTION_SET_SETTINGS_COMMAND: {
      agentInteractionSettings = normalizeAgentInteractionSettings(
        args.settings as Partial<AgentInteractionSettings> | null | undefined,
      );
      return undefined;
    }

    case AGENT_INTERACTION_UPSERT_SUBAGENT_COMMAND: {
      const input = args.input as {
        id?: unknown;
        name?: unknown;
        description?: unknown;
        instruction?: unknown;
        enabled?: unknown;
      } | undefined;
      const id = typeof input?.id === "string" && input.id.trim()
        ? input.id.trim()
        : `agent-${customSubagents.length + 1}`;
      const saved = {
        id,
        name: typeof input?.name === "string" ? input.name.trim() : "",
        description: typeof input?.description === "string" ? input.description.trim() : "",
        instruction: typeof input?.instruction === "string" ? input.instruction.trim() : "",
        enabled: input?.enabled !== false,
      };
      const index = customSubagents.findIndex((item) => item.id === id);
      if (index === -1) customSubagents = [...customSubagents, saved];
      else customSubagents = customSubagents.map((item, itemIndex) => itemIndex === index ? saved : item);
      return saved;
    }

    case AGENT_INTERACTION_DELETE_SUBAGENT_COMMAND:
      customSubagents = customSubagents.filter((item) => item.id !== String(args.id || ""));
      return undefined;

    case CHAT_LIST_MODELS_COMMAND: {
      const backend = normalizeBackend(args.backend);
      const options = MODEL_OPTIONS_BY_BACKEND[backend].map((option) => ({ ...option, backend }));
      if (backend !== "codex") return options;
      const seen = new Set(options.map((option) => option.id));
      return [
        ...options,
        ...assistantAIConfig.modelPool
          .filter((item) => item.backend === "codex" && !seen.has(item.id))
          .map((item) => ({ id: item.id, label: item.label, backend })),
      ];
    }

    case CHAT_RESPOND_AGENT_INTERACTION_COMMAND:
      if (nextAgentInteractionResponseError) {
        const message = nextAgentInteractionResponseError;
        nextAgentInteractionResponseError = null;
        throw new Error(message);
      }
      return undefined;
    case PROJECT_ARCHITECTURE_APPLY_COMMAND: {
      const input = args.input && typeof args.input === "object" && !Array.isArray(args.input)
        ? args.input as Record<string, unknown>
        : {};
      const graph = {
        projectId: String(input.projectId ?? "lilia"),
        version: 1,
        summary: "",
        nodes: [],
        edges: [],
        updatedAt: Date.now(),
      };
      return {
        graph,
        event: {
          id: typeof input.requestId === "string" ? input.requestId : "architecture-1",
          projectId: graph.projectId,
          taskId: String(input.taskId ?? ""),
          turnId: typeof input.turnId === "string" ? input.turnId : null,
          backend: input.backend === "codex" ? "codex" : "claude",
          permission: typeof input.permission === "string" ? input.permission : "ask",
          status: "applied",
          reason: typeof input.reason === "string" ? input.reason : "",
          changes: Array.isArray(input.changes) ? input.changes : [],
          beforeVersion: 0,
          afterVersion: 1,
          createdAt: Date.now(),
          resolvedAt: Date.now(),
        },
      };
    }
    case CHAT_ACK_RESTORED_ROLLBACK_COMMAND:
      return undefined;

    case CHAT_RESPOND_TITLE_UPDATE_COMMAND: {
      const taskId = String(args.taskId);
      const requestId = String(args.requestId);
      const decision = args.decision === "accept" ? "accept" : "decline";
      const eventId = `title-update:${taskId}:${requestId}`;
      const event = (timelineEvents[taskId] ?? []).find((item) =>
        item.id === eventId && item.kind === "title_update"
      );
      if (!event || event.status !== "requires_action") return undefined;
      const payload = event.payload && typeof event.payload === "object" && !Array.isArray(event.payload)
        ? event.payload as Record<string, unknown>
        : {};
      const proposedTitle = typeof payload.proposedTitle === "string"
        ? payload.proposedTitle
        : event.summary ?? "";
      if (decision === "accept" && proposedTitle.trim()) {
        let changedProjectId: string | null = null;
        tasks = tasks.map((task) => {
          if (task.id !== taskId || task.archived) return task;
          changedProjectId = task.projectId;
          return { ...task, title: proposedTitle, titleSource: "manual" };
        });
        emitTauriEvent(TASKS_CHANGED_EVENT_NAME, createTasksChangedEvent(changedProjectId));
      }
      emitMockTimelineEvent(taskId, {
        ...event,
        status: decision === "accept" ? "success" : "skipped",
        payload: {
          ...payload,
          accepted: decision === "accept",
          decision,
        },
        updatedAt: Date.now(),
      });
      return undefined;
    }

    case CHAT_DESCRIBE_ATTACHMENTS_COMMAND: {
      const paths = Array.isArray(args.paths) ? args.paths.map(String) : [];
      return paths.map((path, index) => {
        const name = path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
        const lower = path.toLowerCase();
        const exists = !lower.includes("missing") && !lower.includes("not-found");
        const isImage = /\.(png|jpe?g|gif|webp|bmp|svg)$/i.test(path);
        const isFile = path.includes(".") || isImage;
        const isBigDir = lower.includes("big-dir") || lower.includes("大型目录");
        return {
          id: `att-${index + 1}`,
          name,
          path,
          kind: exists ? (isFile ? "file" : "directory") : "unknown",
          size: exists && isFile ? 42 : null,
          exists,
          mime: exists && isImage ? "image/png" : null,
          directory: exists && !isFile
            ? {
              fileCount: isBigDir ? 250 : 12,
              directoryCount: isBigDir ? 18 : 2,
              totalSize: isBigDir ? 24 * 1024 * 1024 : 4096,
              truncated: isBigDir,
              unreadableCount: 0,
            }
            : null,
        };
      });
    }

    case CHAT_READ_CLIPBOARD_FILE_PATHS_COMMAND:
      return [...clipboardFilePaths];

    case CHAT_SAVE_CLIPBOARD_IMAGE_COMMAND: {
      clipboardImageSeq += 1;
      const input = args.input && typeof args.input === "object" && !Array.isArray(args.input)
        ? args.input as Record<string, unknown>
        : {};
      const mime = typeof input.mime === "string" && input.mime.startsWith("image/")
        ? input.mime
        : "image/png";
      const ext = mime === "image/jpeg"
        ? "jpg"
        : mime === "image/webp"
          ? "webp"
          : "png";
      const path = `C:\\Users\\mock\\.lilia\\cache\\clipboard-images\\clipboard-${clipboardImageSeq}.${ext}`;
      return {
        id: `clip-${clipboardImageSeq}`,
        name: `图片 ${clipboardImageSeq}.${ext}`,
        path,
        kind: "file",
        size: 42,
        exists: true,
        mime,
        directory: null,
      };
    }

    case CHAT_SAVE_CLIPBOARD_TEXT_COMMAND: {
      clipboardTextSeq += 1;
      const input = args.input && typeof args.input === "object" && !Array.isArray(args.input)
        ? args.input as Record<string, unknown>
        : {};
      const text = typeof input.text === "string" ? input.text : "";
      const path = `C:\\Users\\mock\\.lilia\\cache\\clipboard-texts\\clipboard-${clipboardTextSeq}.txt`;
      return {
        id: `clip-text-${clipboardTextSeq}`,
        name: `粘贴文本 ${clipboardTextSeq}.txt`,
        path,
        kind: "file",
        size: new TextEncoder().encode(text).length,
        exists: true,
        mime: null,
        directory: null,
      };
    }

    case CHAT_SEARCH_CONTEXT_ATTACHMENTS_COMMAND: {
      const projectCwd = String(args.projectCwd ?? "D:\\PROJECT\\workspace\\Lilia");
      const query = String(args.query ?? "").toLowerCase().replace(/\\/g, "/");
      const limit = typeof args.limit === "number" ? args.limit : 12;
      const pathMode = query.includes("/");
      const allowHidden = query.includes(".");
      const ignoredRoots = ["dist"];
      const fixtures = [
        { relativePath: "README.md", kind: "file" },
        { relativePath: "apps", kind: "directory" },
        { relativePath: "apps/desktop", kind: "directory" },
        { relativePath: "apps/desktop/src/components/chat/ChatComposer.vue", kind: "file" },
        { relativePath: "apps/desktop/src-tauri/src/lib.rs", kind: "file" },
        { relativePath: "docs/design/memory.md", kind: "file" },
        { relativePath: "screenshots/context-preview.png", kind: "file" },
        { relativePath: "big-dir", kind: "directory" },
        { relativePath: "big-dir/inside.md", kind: "file" },
        { relativePath: ".env", kind: "file" },
        { relativePath: ".github", kind: "directory" },
        { relativePath: ".github/workflows", kind: "directory" },
        { relativePath: "dist", kind: "directory" },
        { relativePath: "dist/app.js", kind: "file" },
      ];
      const directParent = (() => {
        if (!pathMode) return null;
        const normalized = query.startsWith("./") ? query.slice(2) : query;
        if (!normalized) return "";
        if (normalized.endsWith("/")) return normalized.replace(/\/+$/, "");
        return normalized.slice(0, normalized.lastIndexOf("/"));
      })();
      return fixtures
        .filter(({ relativePath }) => {
          const normalized = relativePath.toLowerCase();
          const parts = normalized.split("/");
          const name = parts.at(-1) ?? normalized;
          if (!allowHidden && parts.some((part) => part.startsWith("."))) return false;
          if (!pathMode && ignoredRoots.some((root) => normalized === root || normalized.startsWith(`${root}/`))) {
            return false;
          }
          if (directParent !== null) {
            const parent = parts.slice(0, -1).join("/");
            if (parent !== directParent) return false;
          }
          if (!query) return true;
          return normalized.includes(query) || name.includes(query);
        })
        .slice(0, limit)
        .map(({ relativePath, kind }, index) => {
          const path = `${projectCwd}\\${relativePath.replace(/\//g, "\\")}`;
          const name = relativePath.split("/").at(-1) ?? relativePath;
          const isImage = /\.(png|jpe?g|gif|webp|bmp|svg)$/i.test(relativePath);
          const isFile = kind === "file";
          const isBigDir = relativePath === "big-dir";
          return {
            attachment: {
              id: `ctx-${index + 1}`,
              name,
              path,
              kind,
              size: isFile ? 42 : null,
              exists: true,
              mime: isImage ? "image/png" : null,
              directory: !isFile
                ? {
                  fileCount: isBigDir ? 250 : 12,
                  directoryCount: isBigDir ? 18 : 2,
                  totalSize: isBigDir ? 24 * 1024 * 1024 : 4096,
                  truncated: isBigDir,
                  unreadableCount: 0,
                }
                : null,
            },
            relativePath,
            matchedBy: (name.toLowerCase().includes(query) ? "name" : "path"),
          };
        });
    }

    case CHAT_SEARCH_SLASH_COMMANDS_COMMAND: {
      const query = String(args.query ?? "").trim().toLowerCase();
      const limit = typeof args.limit === "number" ? args.limit : 12;
      return mockSlashCommands
        .filter((command) =>
          !query ||
          command.name.includes(query) ||
          command.title.toLowerCase().includes(query) ||
          command.description.toLowerCase().includes(query)
        )
        .slice(0, limit)
        .map((command) => ({
          command: { ...command, parameters: [...command.parameters] },
          matchedBy: !query || command.name.includes(query) ? "name" : "title",
        }));
    }

    case TODO_LIST_COMMAND: {
      const taskId = String(args.taskId);
      return listMockTodos(taskId);
    }

    case TODO_CREATE_COMMAND: {
      const taskId = String(args.taskId);
      const text = String(args.text ?? "").trim();
      const now = Date.now();
      const order = (todosByTaskId[taskId] ?? []).reduce(
        (max, todo) => Math.max(max, todo.order),
        -1,
      ) + 1;
      const todo: TodoRow = {
        id: nextTodoId(),
        taskId,
        text,
        done: false,
        order,
        source: "lilia",
        priority: normalizeTodoPriority(args.priority),
        guideStatus: "pending",
        attachments: Array.isArray(args.attachments) ? args.attachments : [],
        createdAt: now,
        updatedAt: now,
      };
      todosByTaskId[taskId] = [...(todosByTaskId[taskId] ?? []), todo];
      emitTauriEvent(TODO_CHANGED_EVENT_NAME, createTodoChangedEvent(taskId));
      return cloneTodo(todo);
    }

    case TODO_UPDATE_COMMAND: {
      const id = String(args.id);
      const now = Date.now();
      for (const taskId of Object.keys(todosByTaskId)) {
        let changed = false;
        todosByTaskId[taskId] = todosByTaskId[taskId].map((todo) => {
          if (todo.id !== id) return todo;
          if (todo.source !== "lilia") return todo;
          changed = true;
          return {
            ...todo,
            text: typeof args.text === "string" ? args.text : todo.text,
            done: typeof args.done === "boolean" ? args.done : todo.done,
            order: typeof args.order === "number" ? args.order : todo.order,
            priority: args.priority === "high" || args.priority === "normal" || args.priority === "low"
              ? args.priority
              : todo.priority,
            guideStatus: args.guideStatus === "pending" ||
              args.guideStatus === "queued" ||
              args.guideStatus === "sent"
              ? args.guideStatus
              : todo.guideStatus,
            updatedAt: now,
          };
        });
        if (changed) {
          emitTauriEvent(TODO_CHANGED_EVENT_NAME, createTodoChangedEvent(taskId));
          break;
        }
      }
      return undefined;
    }

    case TODO_DELETE_COMMAND: {
      const id = String(args.id);
      for (const taskId of Object.keys(todosByTaskId)) {
        const before = todosByTaskId[taskId].length;
        todosByTaskId[taskId] = todosByTaskId[taskId].filter((todo) =>
          todo.id !== id || todo.source !== "lilia"
        );
        if (todosByTaskId[taskId].length !== before) {
          emitTauriEvent(TODO_CHANGED_EVENT_NAME, createTodoChangedEvent(taskId));
          break;
        }
      }
      return undefined;
    }

    case TODO_APPLY_AGENT_EVENT_COMMAND: {
      const taskId = String(args.taskId);
      const todos = Array.isArray(args.todos) ? args.todos : [];
      const updated = applyMockAgentTodos(taskId, todos);
      emitTauriEvent(TODO_CHANGED_EVENT_NAME, createTodoChangedEvent(taskId));
      return updated;
    }

    case CHAT_SEND_MESSAGE_COMMAND: {
      if (nextChatSendError) {
        const message = nextChatSendError;
        nextChatSendError = null;
        throw new Error(message);
      }
      const taskId = String(args.taskId);
      const content = String(args.content);
      const composer = normalizeComposer(args.composer, taskId);
      args.composer = composer;
      const attachments = Array.isArray(args.attachments) ? args.attachments : [];
      const conversationReferences = Array.isArray(args.conversationReferences)
        ? args.conversationReferences
        : [];
      const workflow = args.workflow && typeof args.workflow === "object" && !Array.isArray(args.workflow)
        ? args.workflow as Record<string, unknown>
        : null;
      const queued = chatRunning[taskId] === true;
      const message = {
        id: `u-${(timelineEvents[taskId]?.filter((event) => event.kind === "message").length ?? 0) + 1}`,
        taskId,
        role: "user",
        content,
        attachments,
        conversationReferences,
        createdAt: Date.now(),
      };
      const turnId = queued
        ? `turn-queued-${message.id}`
        : `turn-${message.id}`;
      if (workflow?.type === "slash_command" && typeof workflow.commandId === "string") {
        const command = mockSlashCommands.find((item) => item.id === workflow.commandId);
        if (!command) throw new Error(`未知斜杠命令：${workflow.commandId}`);
        const projectCwd = String(args.projectCwd ?? "D:\\PROJECT\\workspace\\Lilia");
        const output = mockSlashCommandOutput(command, projectCwd, composer.backend);
        emitMockTimelineEvent(taskId, {
          id: `${taskId}:${turnId}:slash-command`,
          turnId,
          kind: "command",
          backend: composer.backend,
          status: "success",
          title: `/${command.name}`,
          summary: output,
          payload: {
            command: `/${command.name}`,
            source: command.source,
            title: command.title,
            output,
            exitCode: 0,
            subkind: "slash_command",
          },
          createdAt: message.createdAt,
          updatedAt: message.createdAt,
          turnSeq: 0,
          intraTurnOrder: 0,
        });
        queueMicrotask(() => {
          emitTauriEvent(CHAT_DONE_EVENT_NAME, createChatDoneEvent({
            taskId,
            sessionId: null,
            subtype: "slash_command",
            rollback: null,
          }));
        });
        return {
          message,
          dispatch: "started",
          queuedCount: 0,
          turnId,
        };
      }
      emitMockTimelineEvent(taskId, {
        id: message.id,
        turnId,
        kind: "message",
        backend: composer.backend,
        status: queued ? "pending" : "success",
        title: "用户输入",
        summary: content,
        payload: {
          role: "user",
          content,
          attachments,
          conversationReferences,
          queued,
        },
        createdAt: message.createdAt,
        updatedAt: message.createdAt,
        turnSeq: 0,
        intraTurnOrder: 0,
      });
      const runtimeOptions = args.runtimeOptions && typeof args.runtimeOptions === "object" && !Array.isArray(args.runtimeOptions)
        ? args.runtimeOptions as Record<string, unknown>
        : {};
      const commonOptions = runtimeOptions.common && typeof runtimeOptions.common === "object" && !Array.isArray(runtimeOptions.common)
        ? runtimeOptions.common as Record<string, unknown>
        : {};
      const modelSelection = commonOptions.modelSelection && typeof commonOptions.modelSelection === "object" && !Array.isArray(commonOptions.modelSelection)
        ? commonOptions.modelSelection as Record<string, unknown>
        : null;
      if (modelSelection) {
        emitMockTimelineEvent(taskId, {
          id: `${taskId}:${turnId}:model-selection:${turnId}`,
          turnId,
          kind: "diagnostic",
          backend: composer.backend,
          status: "info",
          title: "模型选择",
          summary: typeof modelSelection.summary === "string" ? modelSelection.summary : "已自动选择本轮模型",
          payload: {
            backend: composer.backend,
            sourceId: `model-selection:${turnId}`,
            subkind: "model_selection",
            selection: modelSelection,
          },
          createdAt: message.createdAt,
          updatedAt: message.createdAt,
          turnSeq: 0,
          intraTurnOrder: 1,
        });
      }
      if (queued) {
        setMockGuideStatus(args.guideId, "queued");
        chatQueued[taskId] = [...(chatQueued[taskId] ?? []), args];
        return {
          message,
          dispatch: "queued",
          queuedCount: chatQueued[taskId].length,
          turnId,
        };
      }
      setMockGuideStatus(args.guideId, "sent");
      chatRunning[taskId] = true;
      queueMicrotask(() => {
        emitTauriEvent(CHAT_TURN_STARTED_EVENT_NAME, createChatTurnStartedEvent(taskId, 0));
      });
      return {
        message,
        dispatch: "started",
        queuedCount: 0,
        turnId,
      };
    }

    case CHAT_SEND_PROCESS_SESSION_COMMAND: {
      const taskId = String(args.taskId);
      const command = args.command && typeof args.command === "object" && !Array.isArray(args.command)
        ? args.command as Record<string, unknown>
        : {};
      processSessionCommands.push({ taskId, command: { ...command } });
      if (command.action === "kill" && chatRunning[taskId] === true) {
        chatRunning[taskId] = false;
        queueMicrotask(() => {
          emitTauriEvent(CHAT_DONE_EVENT_NAME, createChatDoneEvent({
            taskId,
            sessionId: `mock-${taskId}`,
            subtype: null,
            rollback: null,
          }));
        });
      }
      return undefined;
    }

    case LILIA_IAB_OPEN_COMMAND:
      return undefined;

    case LILIA_IAB_SUBMIT_COMMAND: {
      const snapshot = createMockLiliaIabSnapshot(args);
      const delivery = nextLiliaIabDelivery;
      nextLiliaIabDelivery = "message";
      return {
        snapshot,
        delivery,
        stdinForwarded: delivery === "runner",
      };
    }

    case CHAT_INTERRUPT_TURN_COMMAND: {
      const taskId = String(args.taskId);
      const turnId = currentChatTurnId(taskId);
      const message = "用户打断了当前 Agent 运行";
      resetMockQueuedGuides(chatQueued[taskId]);
      chatQueued[taskId] = [];
      if (chatRunning[taskId] === true) {
        chatRunning[taskId] = false;
        const turnEvents = (timelineEvents[taskId] ?? []).filter((event) =>
          event.turnId === turnId
        );
        const [onlyEvent] = turnEvents;
        const payload = onlyEvent?.payload as Record<string, unknown> | null;
        if (
          turnEvents.length === 1 &&
          onlyEvent?.kind === "message" &&
          payload?.role === "user"
        ) {
          chatInterruptRollbacks[taskId] = {
            rolledBack: true,
            restoredContent: typeof payload.content === "string" ? payload.content : "",
            restoredAttachments: Array.isArray(payload.attachments) ? payload.attachments : [],
            restoredConversationReferences: Array.isArray(payload.conversationReferences)
              ? payload.conversationReferences
              : [],
            removedEventIds: [onlyEvent.id],
          };
          return {
            rolledBack: false,
            restoredContent: "",
            restoredAttachments: [],
            restoredConversationReferences: [],
            removedEventIds: [],
          };
        }
        emitMockTimelineEvent(taskId, {
          id: `tl-interrupted-${turnId}`,
          backend: activeBackend,
          kind: "error",
          status: "error",
          title: "Agent 已打断",
          summary: message,
          payload: {
            backend: activeBackend,
            interrupted: true,
            message,
          },
          turnId,
        });
        chatInterruptRollbacks[taskId] = null;
      }
      return {
        rolledBack: false,
        restoredContent: "",
        restoredAttachments: [],
        restoredConversationReferences: [],
        removedEventIds: [],
      };
    }

    case TAURI_PLUGIN_EVENT_LISTEN_COMMAND: {
      const event = String(args.event);
      const handlerId = Number(args.handler);
      return handlerId || 1;
    }

    case NATIVE_AGENT_HOST_STATUS_COMMAND:
      return {
        wired: true,
        defaultBackend: "native-agentkit",
        activeBackend: "native-agentkit",
        timelineIsAgentkitProjection: true,
        liveModelAdapterDrivesTurn: mockNativeCredentials.some((item) => item.status === "active"),
        diagnostics: mockNativeDiagnostics(),
      };

    case NATIVE_CREDENTIAL_PROVIDERS_COMMAND:
      return mockNativeProviders();

    case NATIVE_CREDENTIAL_DIAGNOSTICS_COMMAND:
      return mockNativeDiagnostics();

    case NATIVE_QUOTA_SURFACE_COMMAND:
      return mockNativeQuotaSurface();

    case NATIVE_CREDENTIAL_LOGIN_COMMAND:
    case NATIVE_CREDENTIAL_IMPORT_COMMAND:
      return mockNativeCredentialFromInput(args);

    case NATIVE_CREDENTIAL_REVOKE_COMMAND: {
      const credentialId = String(args.credentialId ?? "");
      const revision = Number(args.revision ?? 0);
      const existing = mockNativeCredentials.find((item) => item.credentialId === credentialId);
      if (!existing) throw new Error(`credential not found: ${credentialId}`);
      const revoked = {
        ...existing,
        revision: revision || existing.revision,
        status: "revoked" as const,
      };
      mockNativeCredentials = mockNativeCredentials.map((item) =>
        item.credentialId === credentialId ? revoked : item,
      );
      return revoked;
    }

    case NATIVE_RESPOND_APPROVAL_COMMAND:
      return {
        sessionId: (args.decision as { sessionId?: string } | undefined)?.sessionId ?? null,
        approvalApplied: true,
        approved: (args.decision as { approved?: boolean } | undefined)?.approved === true,
        events: [],
      };

    case NATIVE_PRODUCT_TIMELINE_COMMAND:
      return [];

    case NATIVE_SHARED_CODING_SERVICES_STATUS_COMMAND:
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
      } satisfies NativeSharedCodingServicesStatus;

    case NATIVE_SHARED_MCP_LIST_SERVERS_COMMAND:
      return [];

    case NATIVE_SHARED_LSP_STATUS_COMMAND:
      return {
        serviceId: "mutsuki.agent.service.lsp",
        activeWorkspaces: 0,
        dataSource: "agentkit.native_coding_bundle",
        sameInstance: true,
      };

    case NATIVE_SHARED_GIT_STATUS_COMMAND:
      return {
        kind: "status",
        path: String(args.path ?? ""),
        dataSource: "agentkit.native_coding_bundle",
        clean: true,
      };

    case NATIVE_SHARED_CODE_INDEX_SEARCH_COMMAND:
      return {
        hits: [
          {
            path: String(args.relativePath ?? "src/shared_probe.rs"),
            score: 1,
          },
        ],
        dataSource: "agentkit.native_coding_bundle",
      };

    case NATIVE_SHARED_MEMORY_QUERY_COMMAND:
      return {
        records: [],
        dataSource: "agentkit.native_coding_bundle",
        query: String(args.query ?? ""),
      };

    case NATIVE_SHARED_MEMORY_WRITE_COMMAND:
      return {
        memory_id: "mock-memory-1",
        text: String(args.text ?? ""),
        dataSource: "agentkit.native_coding_bundle",
      };

    default:
      throw new Error(`未配置的 Tauri mock 命令：${cmd}`);
  }
});

resetTauriMockData();

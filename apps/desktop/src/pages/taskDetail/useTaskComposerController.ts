import { computed, ref, watch, type Ref } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  createChatBackendRecord,
  isAgentTimelineToolWindowKind,
  latestLiliaGoalFromTimeline,
  serializeChatAttachmentReference,
  stripSerializedConversationReferences,
  TITLE_UPDATE_ACTION_KIND,
  taskStatusLabel,
} from "@lilia/contracts";
import type {
  AgentTimelineEvent,
  AskUserResult,
  ChatBranchAnchor,
  ChatAttachment,
  ChatBackendKind,
  ChatContextUsage,
  ChatComposerState,
  ChatConversationReference,
  ChatModelOption,
  ChatRuntimePhase,
  ChatSlashCommandWorkflow,
  ChatWorkflow,
  LiliaBatchApplyInput,
  LiliaThreadGoal,
  LiliaReviewTarget,
  PermissionMode,
  ProviderRuntimeOptions,
  Task,
  TaskWorktree,
  WorktreeListItem,
} from "@lilia/contracts";
import {
  findPlanApprovalAsk,
  isPlanApprovalAsk,
  useAskUserForTask,
  usePendingAsksForTask,
} from "../../composables/useAskUser";
import {
  usePendingAgentActionsForTask,
} from "../../composables/usePendingAgentActions";
import {
  pendingAgentActionBuckets,
  type PendingAgentActionResolution,
} from "../../composables/pendingAgentActions";
import {
  usePendingAgentInteractionsForTask,
} from "../../composables/useAgentPendingInteractions";
import {
  usePendingProjectArchitectureChangesForTask,
} from "../../composables/useProjectArchitectureInteractions";
import {
  usePendingToolConsentsForTask,
  useToolConsentForTask,
} from "../../composables/useToolConsentBridge";
import {
  loadAgentInteractionSettings,
  useAgentInteractionSettings,
} from "../../composables/useAgentInteractionSettings";
import { useConnectionStatus } from "../../composables/useConnectionStatus";
import {
  getComposerState,
  listModels,
  getRuntimeSnapshot,
  ackRestoredRollback,
  interruptTurn,
  onAgentTimelineEvents,
  onContextUsage,
  onDone,
  onTurnStarted,
  sendMessage,
  setComposerState,
} from "../../services/chat";
import { getProjectSettings } from "../../services/projects";
import { getTask, listTasks } from "../../services/tasksStore";
import {
  attachWorktreeToTask,
  clearTaskWorktree,
  createWorktreeForTask,
  getTaskWorktree,
  listWorktrees,
} from "../../services/worktrees";
import type {
  SendMessageInput,
  ToolConsentDecision,
  ToolConsentUpdatedInput,
} from "../../services/chat";
import type { TaskTodo } from "../../services/todos";
import type { TaskDetailRouteProps, useTaskConversationContext } from "./useTaskConversationContext";
import type { useTaskTimeline } from "./useTaskTimeline";
import type { LiliaWorkflowSendAgentMessageInput } from "./useLiliaWorkflowActions";
import { taskTimelineLoadMergePlan } from "./taskTimelineLoadMerge";
import {
  clearPendingInteractionsForTask,
  hydratePendingInteractions,
  pendingInteractionRequestIdsForSources,
  pendingInteractionRequestIdsToKeepAfterLoad,
  syncPendingInteractionsForTimelineEvents,
} from "./usePendingInteractionActions";
import { createLazyLoadState } from "@lilia/ui/utils/lazyLoadState";
import { installUnlistenFns } from "@lilia/ui/utils/eventListeners";
import { measurePerfAsync, scheduleAfterPaint } from "@lilia/ui/diagnostics";
import { singleFlight } from "@lilia/ui/utils/singleFlight";

type SendAgentMessageInput = LiliaWorkflowSendAgentMessageInput;

function emptyModelOptionsByBackend(): Record<ChatBackendKind, ChatModelOption[]> {
  return createChatBackendRecord(() => []);
}
type GuideDispatchWindow = "tool" | "user" | "idle";
type WorktreeOption = { value: string; label: string; hint?: string };

const WORKTREE_CURRENT_VALUE = "__current__";
const WORKTREE_CREATE_VALUE = "__create__";

function dependencyBlockReason(
  task: Task,
  tasksById: Map<string, Task>,
  visiting: Set<string>,
  visited: Set<string>,
): string | null {
  if (visited.has(task.id)) return null;
  if (visiting.has(task.id)) return "任务依赖存在循环，暂不能启动会话。";
  visiting.add(task.id);
  for (const dependencyId of task.dependsOn) {
    const dependency = tasksById.get(dependencyId);
    if (!dependency) continue;
    if (visiting.has(dependency.id)) return "任务依赖存在循环，暂不能启动会话。";
    if (dependency.status !== "done") {
      return `任务依赖未完成，暂不能启动会话：${dependency.title}（${taskStatusLabel(dependency.status)}）`;
    }
    const nestedReason = dependencyBlockReason(dependency, tasksById, visiting, visited);
    if (nestedReason) return nestedReason;
  }
  visiting.delete(task.id);
  visited.add(task.id);
  return null;
}

function taskRunBlockReason(projectId: string | undefined, taskId: string): string | null {
  if (!projectId) return null;
  const tasks = listTasks(projectId);
  const tasksById = new Map(tasks.map((task) => [task.id, task]));
  const task = tasksById.get(taskId) ?? getTask(projectId, taskId);
  if (!task) return null;
  if (task.status === "blocked") {
    return `任务已标记为阻塞，暂不能启动会话：${task.title}`;
  }
  return dependencyBlockReason(task, tasksById, new Set(), new Set());
}

interface GuideDispatchController {
  createGuideFromComposer: (
    content: string,
    outgoingAttachments?: ChatAttachment[],
  ) => Promise<void>;
  dispatchGuide: (todo: TaskTodo) => Promise<void>;
  scheduleGuideInsertion: (windowKind: GuideDispatchWindow) => Promise<void>;
}

interface PendingInteractionResolvers {
  onResolveAskUser: (result: AskUserResult) => Promise<void>;
  onResolveToolConsent: (
    decision: ToolConsentDecision,
    message?: string,
    updatedInput?: ToolConsentUpdatedInput,
  ) => Promise<void>;
  onResolvePendingAgentAction: (resolution: PendingAgentActionResolution) => Promise<void>;
}

interface LiliaWorkflowActionHandlers {
  onStartLiliaReview: (
    content: string,
    outgoingAttachments: ChatAttachment[],
    outgoingConversationReferences: ChatConversationReference[],
    target: LiliaReviewTarget,
  ) => Promise<void>;
  onStartLiliaFixSuggestion: (
    content: string,
    outgoingAttachments: ChatAttachment[],
    outgoingConversationReferences: ChatConversationReference[],
    target: LiliaReviewTarget,
  ) => Promise<void>;
  onStartLiliaCompact: () => Promise<void>;
  onStartSessionFork: () => Promise<void>;
  onStartLiliaBatchApply: (input: LiliaBatchApplyInput) => Promise<void>;
  onSetLiliaGoal: (objective: string) => Promise<void>;
  onRefreshLiliaGoal: () => Promise<void>;
  onClearLiliaGoal: () => Promise<void>;
}

interface TaskContextSnapshot {
  taskId: string;
  projectId?: string;
  contextSnapshotVersion: number;
}

interface TaskLoadCycle extends TaskContextSnapshot {
  seq: number;
  pendingBeforeLoad: Set<string>;
  timelineEventIdsBeforeLoad: Set<string>;
  liveTimelineEventsDuringLoad: Map<string, AgentTimelineEvent>;
  runtimeSeqBeforeLoad: number;
}

const composerStateLoads = new Map<string, Promise<ChatComposerState>>();
const taskWorktreeLoads = new Map<string, Promise<TaskWorktree | null>>();
const worktreeListLoads = new Map<string, Promise<WorktreeListItem[]>>();
const runtimeSnapshotLoads = new Map<string, Promise<Awaited<ReturnType<typeof getRuntimeSnapshot>>>>();

function loadComposerState(taskId: string): Promise<ChatComposerState> {
  return singleFlight(composerStateLoads, taskId, () => getComposerState(taskId));
}

function loadTaskWorktree(taskId: string): Promise<TaskWorktree | null> {
  return singleFlight(taskWorktreeLoads, taskId, () => getTaskWorktree(taskId));
}

function loadWorktreeList(base: string): Promise<WorktreeListItem[]> {
  return singleFlight(worktreeListLoads, base, () => listWorktrees(base));
}

function loadRuntimeSnapshot(taskId: string): Promise<Awaited<ReturnType<typeof getRuntimeSnapshot>>> {
  return singleFlight(runtimeSnapshotLoads, taskId, () => getRuntimeSnapshot(taskId));
}

export function useTaskComposerController(options: {
  props: TaskDetailRouteProps;
  context: ReturnType<typeof useTaskConversationContext>;
  timeline: ReturnType<typeof useTaskTimeline>;
  attachments: Ref<ChatAttachment[]>;
}) {
  const { props, context, timeline, attachments } = options;
  const composer = ref<ChatComposerState | null>(null);
  const modelOptionsByBackend = ref<Record<ChatBackendKind, ChatModelOption[]>>(
    emptyModelOptionsByBackend(),
  );
  const contextUsage = ref<ChatContextUsage | null>(null);
  const isTurnRunning = ref(false);
  const interruptInFlight = ref(false);
  const userSendScrollKey = ref(0);
  const restoreDraftKey = ref(0);
  const restoreDraftContent = ref("");
  const restoreDraftConversationReferences = ref<ChatConversationReference[]>([]);
  const insertDraftTextKey = ref(0);
  const insertDraftTextContent = ref("");
  const pendingBranchAnchor = ref<ChatBranchAnchor | null>(null);
  const taskWorktree = ref<TaskWorktree | null>(null);
  const worktreeOptions = ref<WorktreeOption[]>([
    { value: WORKTREE_CURRENT_VALUE, label: "当前环境", hint: "使用项目目录" },
    { value: WORKTREE_CREATE_VALUE, label: "新建工作树", hint: "为当前对话创建独立 worktree" },
  ]);
  const worktreeBusy = ref(false);
  const worktreeError = ref<string | null>(null);
  const pendingAskUser = useAskUserForTask(() => props.taskId);
  const pendingAskUsers = usePendingAsksForTask(() => props.taskId);
  const pendingToolConsent = useToolConsentForTask(() => props.taskId);
  const pendingToolConsents = usePendingToolConsentsForTask(() => props.taskId);
  const pendingAgentInteractions = usePendingAgentInteractionsForTask(() => props.taskId);
  const pendingArchitectureChanges = usePendingProjectArchitectureChangesForTask(() => props.taskId);
  const runtimePendingAgentActions = usePendingAgentActionsForTask(
    pendingAskUsers,
    pendingToolConsents,
    pendingAgentInteractions,
    timeline.timelineEvents,
    pendingArchitectureChanges,
  );
  const pendingTitleUpdateRequestIds = computed(() => {
    const requestIds = new Set<string>();
    for (const action of runtimePendingAgentActions.value) {
      if (action.kind === TITLE_UPDATE_ACTION_KIND) requestIds.add(action.requestId);
    }
    return requestIds;
  });
  const agentInteractionSettings = useAgentInteractionSettings();
  const nonInterruptMode = agentInteractionSettings.nonInterruptMode;
  const permissionMode = agentInteractionSettings.permissionMode;
  const { activeBackend } = useConnectionStatus({
    probe: !context.isPopup.value && !context.conversationRouteState.value.isLiveDraft,
  });
  let loadSeq = 0;
  let runtimeEventSeq = 0;
  let composerStateWriteSeq = 0;
  let activeLoadCycle: TaskLoadCycle | null = null;
  let composerLoad: Promise<ChatComposerState | null> | null = null;
  let composerBackendLoaded = false;
  let cancelRuntimeSnapshotHydrationPaint: (() => void) | null = null;
  let cancelComposerStateHydrationPaint: (() => void) | null = null;
  const modelOptionsLoads: Partial<Record<ChatBackendKind, Promise<ChatModelOption[]>>> = {};
  const guideDispatchLoad = createLazyLoadState<GuideDispatchController>(() =>
    measurePerfAsync(
      "task-detail.guide-dispatch.load",
      async () => {
        const { useGuideDispatch } = await import("../../composables/useGuideDispatch");
        return useGuideDispatch({
          taskId: () => props.taskId,
          ensureReady: context.ensureTaskReadyForMessage,
          sendAgentMessage: (content, outgoingAttachments, guideId) =>
            sendAgentMessage({ turn: { content, outgoingAttachments, guideId } }),
          ensureDispatchReady: async () => {
            await ensureComposerLoaded();
          },
          hasBlockingPendingAgentAction: () =>
            blockingPendingAgentActions.value.length > 0,
          isTurnRunning: () => isTurnRunning.value,
          clearAttachments: () => {
            attachments.value = [];
          },
          reportError: (message) => {
            timeline.upsertTimelineEvent(timeline.createLocalErrorTimelineEvent(message));
          },
        });
      },
      { detail: props.taskId },
    )
  );
  const liliaWorkflowActionsLoad = createLazyLoadState<LiliaWorkflowActionHandlers>(() =>
    measurePerfAsync(
      "task-detail.workflows.load",
      async () => {
        const { useLiliaWorkflowActions } = await import("./useLiliaWorkflowActions");
        return useLiliaWorkflowActions({
          hasContext: context.hasContext,
          isTurnRunning,
          blockingPendingAgentActions,
          attachments,
          sendAgentMessage,
        });
      },
      { detail: props.taskId },
    )
  );
  const pendingInteractionResolversLoad = createLazyLoadState<PendingInteractionResolvers>(() =>
    measurePerfAsync(
      "task-detail.pending-interactions.load",
      async () => {
        const { usePendingInteractionResolvers } = await import("./usePendingInteractionResolvers");
        return usePendingInteractionResolvers({
          taskId: () => props.taskId,
          pendingAskUser,
          pendingAskUsers,
          pendingTitleUpdateRequestIds,
          pendingToolConsent,
          pendingToolConsents,
          pendingAgentInteractions,
          pendingArchitectureChanges,
        });
      },
      { detail: props.taskId },
    )
  );

  const composerForView = computed<ChatComposerState>(() =>
    withActiveBackend(composer.value ?? {
      taskId: props.taskId,
      backend: activeBackend.value,
      model: "",
      modelSelectionMode: "auto",
      reasoningEffort: null,
      planMode: false,
      goalMode: false,
      permission: permissionMode.value,
    }),
  );
  const modelOptionsForView = computed(() => modelOptionsByBackend.value[activeBackend.value]);
  const classifiedPendingAgentActions = computed(() =>
    pendingAgentActionBuckets(runtimePendingAgentActions.value, {
      nonInterruptMode: nonInterruptMode.value,
    })
  );
  const pendingAgentActions = computed(() => classifiedPendingAgentActions.value.visible);
  const blockingPendingAgentActions = computed(() => classifiedPendingAgentActions.value.blocking);
  const pendingPlanApproval = computed(() => {
    const ask = nonInterruptMode.value
      ? findPlanApprovalAsk(pendingAskUsers.value)
      : pendingAskUser.value;
    if (!ask) return null;
    if (!isPlanApprovalAsk(ask)) return null;
    const question = ask.spec.questions[0];
    return question ? { questionId: question.id, turnId: ask.turnId } : null;
  });
  const currentLiliaGoal = computed<LiliaThreadGoal | null>(() =>
    latestLiliaGoalFromTimeline(timeline.timelineEvents.value),
  );
  const taskRunBlockingReason = computed(() =>
    taskRunBlockReason(props.projectId, props.taskId),
  );
  const effectiveProjectCwd = computed(() =>
    taskWorktree.value?.worktreePath ?? context.project.value?.cwd ?? null,
  );
  const worktreeSelectionValue = computed(() =>
    taskWorktree.value?.worktreePath ?? WORKTREE_CURRENT_VALUE,
  );

  function withActiveBackend(
    state: ChatComposerState,
    permissionOverride?: PermissionMode,
  ): ChatComposerState {
    return {
      ...state,
      taskId: props.taskId,
      backend: activeBackend.value,
      modelSelectionMode: state.modelSelectionMode === "manual" ? "manual" : "auto",
      reasoningEffort: state.reasoningEffort ?? null,
      permission: permissionOverride ?? permissionMode.value,
    };
  }

  function mergeAdditionalContext(
    runtimeOptions: ProviderRuntimeOptions | null,
    _backend: ChatBackendKind,
    contextText: string,
  ): ProviderRuntimeOptions {
    const current = runtimeOptions ?? {};
    const provider = current.provider ?? {};
    const native = provider["native-agentkit"] ?? {};
    const previous = native.additionalContext?.trim();
    return {
      ...current,
      provider: {
        ...provider,
        "native-agentkit": {
          ...native,
          additionalContext: previous ? `${previous}\n\n${contextText}` : contextText,
        },
      },
    };
  }

  async function runtimeOptionsWithWorktreeContext(
    currentComposer: ChatComposerState,
    runtimeOptions: ProviderRuntimeOptions | null,
  ): Promise<ProviderRuntimeOptions | null> {
    if (!taskWorktree.value) return runtimeOptions;
    try {
      const settings = await getProjectSettings();
      const text = settings.worktree?.autoInstructions?.trim();
      if (!text) return runtimeOptions;
      return mergeAdditionalContext(runtimeOptions, currentComposer.backend, text);
    } catch (err) {
      console.error("[worktree] load settings failed", err);
      return runtimeOptions;
    }
  }

  function worktreeOptionFromItem(item: WorktreeListItem): WorktreeOption | null {
    if (item.isMain || item.bare || item.prunable) return null;
    const branch = item.branch ? ` · ${item.branch}` : "";
    return {
      value: item.path,
      label: item.branch || item.path.split(/[\\/]/).pop() || item.path,
      hint: `${item.path}${branch}`,
    };
  }

  function captureTaskSnapshot(): TaskContextSnapshot {
    return {
      taskId: props.taskId,
      projectId: props.projectId,
      contextSnapshotVersion: context.captureContextSnapshot(),
    };
  }

  function isCurrentTaskSnapshot(snapshot: TaskContextSnapshot): boolean {
    return context.isCurrentContextSnapshot(
      snapshot.contextSnapshotVersion,
      snapshot.projectId,
      snapshot.taskId,
    );
  }

  function assertCurrentTaskSnapshot(snapshot: TaskContextSnapshot) {
    context.assertContextSnapshotCurrent(
      snapshot.contextSnapshotVersion,
      snapshot.projectId,
      snapshot.taskId,
    );
  }

  async function refreshTaskWorktree(
    snapshot = captureTaskSnapshot(),
  ): Promise<TaskWorktree | null> {
    try {
      const loaded = await loadTaskWorktree(snapshot.taskId);
      if (!isCurrentTaskSnapshot(snapshot)) return null;
      taskWorktree.value = loaded;
      if (!loaded && !worktreeBusy.value && context.project.value?.cwd) {
        const settings = await getProjectSettings();
        if (!isCurrentTaskSnapshot(snapshot)) return taskWorktree.value;
        if (settings.worktree?.defaultMode === "create") {
          await onSelectWorktree(WORKTREE_CREATE_VALUE);
        }
      }
      return taskWorktree.value;
    } catch (err) {
      if (!isCurrentTaskSnapshot(snapshot)) return null;
      taskWorktree.value = null;
      worktreeError.value = `读取工作树失败：${String(err)}`;
      return null;
    }
  }

  async function refreshWorktreeOptions(
    snapshot = captureTaskSnapshot(),
  ) {
    const base = context.project.value?.cwd;
    const current: WorktreeOption[] = [
      { value: WORKTREE_CURRENT_VALUE, label: "当前环境", hint: base || "使用当前项目目录" },
      { value: WORKTREE_CREATE_VALUE, label: "新建工作树", hint: "为当前对话创建独立 worktree" },
    ];
    if (!base) {
      if (!isCurrentTaskSnapshot(snapshot)) return;
      worktreeOptions.value = current;
      return;
    }
    try {
      const items = await loadWorktreeList(base);
      if (!isCurrentTaskSnapshot(snapshot)) return;
      const existing = items
        .map(worktreeOptionFromItem)
        .filter((item): item is WorktreeOption => Boolean(item));
      worktreeOptions.value = [...current, ...existing];
      worktreeError.value = null;
    } catch (err) {
      if (!isCurrentTaskSnapshot(snapshot)) return;
      worktreeOptions.value = current;
      worktreeError.value = `读取工作树列表失败：${String(err)}`;
    }
  }

  async function onSelectWorktree(value: string) {
    const snapshot = captureTaskSnapshot();
    const base = context.project.value?.cwd;
    if (!base || worktreeBusy.value) return;
    worktreeBusy.value = true;
    worktreeError.value = null;
    try {
      if (!isCurrentTaskSnapshot(snapshot)) return;
      if (value === WORKTREE_CURRENT_VALUE) {
        await clearTaskWorktree(snapshot.taskId);
        if (!isCurrentTaskSnapshot(snapshot)) return;
        taskWorktree.value = null;
        return;
      }
      const settings = await getProjectSettings();
      if (!isCurrentTaskSnapshot(snapshot)) return;
      taskWorktree.value = value === WORKTREE_CREATE_VALUE
        ? await createWorktreeForTask({
          taskId: snapshot.taskId,
          projectId: snapshot.projectId ?? null,
          baseRepoPath: base,
          parentDir: settings.worktree?.parentDir ?? null,
        })
        : await attachWorktreeToTask({
          taskId: snapshot.taskId,
          projectId: snapshot.projectId ?? null,
          baseRepoPath: base,
          worktreePath: value,
        });
      if (!isCurrentTaskSnapshot(snapshot)) return;
      const nextComposer = withActiveBackend(composerForView.value);
      composer.value = nextComposer;
      await persistComposerState(nextComposer, snapshot);
      await refreshWorktreeOptions(snapshot);
    } catch (err) {
      if (!isCurrentTaskSnapshot(snapshot)) return;
      worktreeError.value = `切换工作树失败：${String(err)}`;
      timeline.upsertTimelineEvent(timeline.createLocalErrorTimelineEvent(worktreeError.value));
    } finally {
      if (isCurrentTaskSnapshot(snapshot)) {
        worktreeBusy.value = false;
      }
    }
  }

  async function ensureModelOptions(backend: ChatBackendKind): Promise<ChatModelOption[]> {
    if (modelOptionsByBackend.value[backend].length > 0) {
      return modelOptionsByBackend.value[backend];
    }
    if (!modelOptionsLoads[backend]) {
      modelOptionsLoads[backend] = listModels(backend)
        .then((options) => {
          modelOptionsByBackend.value = {
            ...modelOptionsByBackend.value,
            [backend]: options,
          };
          return options;
        })
        .catch((err) => {
          delete modelOptionsLoads[backend];
          console.error("[chat] listModels failed", err);
          return [];
        });
    }
    return modelOptionsLoads[backend]!;
  }

  async function getGuideDispatch(): Promise<GuideDispatchController> {
    return guideDispatchLoad.load();
  }

  async function createGuideFromComposer(
    content: string,
    outgoingAttachments: ChatAttachment[] = [],
  ) {
    const guideDispatch = await getGuideDispatch();
    await guideDispatch.createGuideFromComposer(content, outgoingAttachments);
  }

  function reportTaskRunBlock(): string | null {
    const reason = taskRunBlockingReason.value;
    if (!reason) return null;
    timeline.upsertTimelineEvent(timeline.createLocalErrorTimelineEvent(reason));
    return reason;
  }

  async function dispatchGuide(todo: TaskTodo) {
    const guideDispatch = await getGuideDispatch();
    await guideDispatch.dispatchGuide(todo);
  }

  async function scheduleGuideInsertion(windowKind: GuideDispatchWindow) {
    const guideDispatch = await getGuideDispatch();
    await guideDispatch.scheduleGuideInsertion(windowKind);
  }

  async function sendAgentMessage(input: SendAgentMessageInput) {
    if (!context.hasContext.value) return;
    const snapshot = captureTaskSnapshot();
    const outgoingAttachments = input.turn.outgoingAttachments ?? [];
    const outgoingConversationReferences = input.turn.outgoingConversationReferences ?? [];
    const workflow = input.workflow ?? null;
    let runtimeCommand = input.runtimeCommand ?? null;
    let runtimeOptions = input.runtimeOptions ?? null;
    const titleContent = input.turn.titleContent;
    const content = input.turn.content;
    if (
      !content.trim() &&
      outgoingAttachments.length === 0 &&
      outgoingConversationReferences.length === 0 &&
      !workflow &&
      !runtimeCommand
    ) return;

    const blockReason = reportTaskRunBlock();
    if (blockReason) {
      throw new Error(blockReason);
    }

    let optimisticId: string | null = null;
    try {
      assertCurrentTaskSnapshot(snapshot);
      await ensureComposerLoaded();
      assertCurrentTaskSnapshot(snapshot);
      const currentComposer = composerForView.value;
      await context.ensureTaskReadyForMessage(titleContent ?? content, outgoingAttachments);
      assertCurrentTaskSnapshot(snapshot);
      const cwd = taskWorktree.value?.worktreePath ??
        context.project.value?.cwd ??
        (await context.ensureOrphanCwd());
      assertCurrentTaskSnapshot(snapshot);
      if (
        runtimeCommand?.type === "process_session" &&
        runtimeCommand.action === "spawn" &&
        !runtimeCommand.cwd
      ) {
        runtimeCommand = { ...runtimeCommand, cwd };
      }
      runtimeOptions = await runtimeOptionsWithWorktreeContext(currentComposer, runtimeOptions);
      assertCurrentTaskSnapshot(snapshot);

      const optimistic = timeline.createOptimisticMessageEvent({
        content,
        attachments: outgoingAttachments,
        conversationReferences: outgoingConversationReferences,
      });
      optimisticId = optimistic.id;
      timeline.upsertTimelineEvent(optimistic);
      userSendScrollKey.value += 1;
      const sendInput: SendMessageInput = {
        taskId: snapshot.taskId,
        turn: {
          content,
          composer: currentComposer,
          projectCwd: cwd,
          attachments: outgoingAttachments,
          conversationReferences: outgoingConversationReferences,
          guideId: input.turn.guideId ?? null,
        },
        workflow,
        runtimeCommand,
        runtimeOptions,
      };
      assertCurrentTaskSnapshot(snapshot);
      await sendMessage(sendInput);
      if (!isCurrentTaskSnapshot(snapshot)) return;
      timeline.removeTimelineEvent(optimistic.id);
    } catch (err) {
      if (!isCurrentTaskSnapshot(snapshot)) return;
      if (optimisticId) timeline.removeTimelineEvent(optimisticId);
      isTurnRunning.value = false;
      timeline.upsertTimelineEvent(timeline.createLocalErrorTimelineEvent(`发送失败：${String(err)}`, {
        content,
        attachments: outgoingAttachments,
        conversationReferences: outgoingConversationReferences,
      }));
      throw err;
    }
  }

  watch(
    activeBackend,
    (backend) => {
      void ensureModelOptions(backend);
    },
    { immediate: true },
  );

  watch(
    () => [props.taskId, context.project.value?.cwd ?? ""] as const,
    () => {
      const snapshot = captureTaskSnapshot();
      void refreshTaskWorktree(snapshot);
      void refreshWorktreeOptions(snapshot);
    },
    { immediate: true },
  );

  async function onSend(
    content: string,
    outgoingAttachments: ChatAttachment[] = [],
    outgoingConversationReferences: ChatConversationReference[] = [],
    workflow: ChatWorkflow | null = null,
  ) {
    if (!context.hasContext.value) return;
    if (reportTaskRunBlock()) return;
    if (isTurnRunning.value || blockingPendingAgentActions.value.length > 0) {
      await createGuideFromComposer(content, outgoingAttachments);
      return;
    }

    try {
      const branchAnchor = pendingBranchAnchor.value;
      await sendAgentMessage({
        turn: { content, outgoingAttachments, outgoingConversationReferences },
        workflow,
        runtimeCommand: branchAnchor
          ? {
            type: "session_fork",
            excludeTurns: true,
            sourceTurnId: branchAnchor.sourceTurnId,
            mode: branchAnchor.mode,
          }
          : null,
      });
      if (branchAnchor && pendingBranchAnchor.value === branchAnchor) {
        pendingBranchAnchor.value = null;
      }
      attachments.value = [];
    } catch {
      // sendAgentMessage 已经把失败写入 timeline；这里吞掉异常避免 Vue 事件处理链重复报错。
    }
  }

  async function onExecuteSlashCommand(workflow: ChatSlashCommandWorkflow) {
    if (!context.hasContext.value) return;
    if (reportTaskRunBlock()) return;
    if (isTurnRunning.value || blockingPendingAgentActions.value.length > 0) {
      timeline.upsertTimelineEvent(timeline.createLocalErrorTimelineEvent("当前 Agent 正在运行，暂不能执行斜杠命令。"));
      return;
    }
    try {
      const commandName = workflow.commandId.split(":").at(1) ?? workflow.commandId;
      await sendAgentMessage({
        turn: {
          content: "",
          outgoingAttachments: [],
          titleContent: `/${commandName}`,
        },
        workflow,
      });
    } catch {
      // sendAgentMessage 已写入本地错误 timeline。
    }
  }

  async function getLiliaWorkflowActions() {
    return liliaWorkflowActionsLoad.load();
  }

  async function onStartLiliaReview(
    content: string,
    outgoingAttachments: ChatAttachment[],
    outgoingConversationReferences: ChatConversationReference[],
    target: LiliaReviewTarget,
  ) {
    const actions = await getLiliaWorkflowActions();
    await actions.onStartLiliaReview(
      content,
      outgoingAttachments,
      outgoingConversationReferences,
      target,
    );
  }

  async function onStartLiliaFixSuggestion(
    content: string,
    outgoingAttachments: ChatAttachment[],
    outgoingConversationReferences: ChatConversationReference[],
    target: LiliaReviewTarget,
  ) {
    const actions = await getLiliaWorkflowActions();
    await actions.onStartLiliaFixSuggestion(
      content,
      outgoingAttachments,
      outgoingConversationReferences,
      target,
    );
  }

  async function onStartLiliaCompact() {
    const actions = await getLiliaWorkflowActions();
    await actions.onStartLiliaCompact();
  }

  async function onStartSessionFork(anchor?: ChatBranchAnchor) {
    if (anchor?.sourceTurnId) {
      pendingBranchAnchor.value = {
        sourceTurnId: anchor.sourceTurnId,
        mode: anchor.mode,
      };
      return;
    }
    const actions = await getLiliaWorkflowActions();
    await actions.onStartSessionFork();
  }

  function onClearBranchAnchor() {
    pendingBranchAnchor.value = null;
  }

  async function onStartLiliaBatchApply(input: LiliaBatchApplyInput) {
    const actions = await getLiliaWorkflowActions();
    await actions.onStartLiliaBatchApply(input);
  }

  async function onSetLiliaGoal(objective: string) {
    const actions = await getLiliaWorkflowActions();
    await actions.onSetLiliaGoal(objective);
  }

  async function onRefreshLiliaGoal() {
    const actions = await getLiliaWorkflowActions();
    await actions.onRefreshLiliaGoal();
  }

  async function onClearLiliaGoal() {
    const actions = await getLiliaWorkflowActions();
    await actions.onClearLiliaGoal();
  }

  function onInsertGuide(todo: TaskTodo) {
    void dispatchGuide(todo);
  }

  function onInsertDraftText(text: string) {
    if (!text) return;
    insertDraftTextContent.value = text;
    insertDraftTextKey.value += 1;
  }

  async function onInterrupt() {
    if (!context.hasContext.value || !isTurnRunning.value || interruptInFlight.value) return;
    interruptInFlight.value = true;
    try {
      await interruptTurn(props.taskId);
      isTurnRunning.value = false;
      clearPendingInteractionsForTask(props.taskId);
    } catch (err) {
      interruptInFlight.value = false;
      timeline.upsertTimelineEvent(timeline.createLocalErrorTimelineEvent(`打断失败：${String(err)}`));
    }
  }

  async function getPendingInteractionResolvers() {
    return pendingInteractionResolversLoad.load();
  }

  async function onResolveAskUser(result: AskUserResult) {
    const resolvers = await getPendingInteractionResolvers();
    await resolvers.onResolveAskUser(result);
  }

  async function onResolveToolConsent(
    decision: ToolConsentDecision,
    message?: string,
    updatedInput?: ToolConsentUpdatedInput,
  ) {
    const resolvers = await getPendingInteractionResolvers();
    await resolvers.onResolveToolConsent(decision, message, updatedInput);
  }

  async function onResolvePendingAgentAction(
    resolution: PendingAgentActionResolution,
  ) {
    const resolvers = await getPendingInteractionResolvers();
    await resolvers.onResolvePendingAgentAction(resolution);
  }

  async function persistComposerState(
    state: ChatComposerState,
    snapshot = captureTaskSnapshot(),
  ) {
    const writeSeq = ++composerStateWriteSeq;
    await Promise.resolve();
    if (
      writeSeq !== composerStateWriteSeq ||
      state.taskId !== snapshot.taskId ||
      !isCurrentTaskSnapshot(snapshot)
    ) return;
    await setComposerState(state);
  }

  async function onComposerUpdate(next: ChatComposerState) {
    const snapshot = captureTaskSnapshot();
    const requestedPermission = next.permission;
    const normalized = withActiveBackend(next, requestedPermission);
    composer.value = normalized;
    if (requestedPermission !== permissionMode.value) {
      void agentInteractionSettings.update({ permissionMode: requestedPermission });
    }
    try {
      await persistComposerState(normalized, snapshot);
    } catch (err) {
      console.error("[chat] setComposerState failed", err);
    }
  }

  async function onRetryTimelineEvent(event: Parameters<typeof timeline.retryContextForEvent>[0]) {
    const retryContext = timeline.retryContextForEvent(event);
    if (!retryContext) return;
    try {
      await sendAgentMessage({
        turn: {
          content: retryContext.content,
          outgoingAttachments: retryContext.attachments,
          outgoingConversationReferences: retryContext.conversationReferences ?? [],
        },
      });
    } catch (err) {
      console.error("[chat] retry failed", err);
    }
  }

  async function loadComposerForCurrentTask(
    cycle: TaskLoadCycle,
  ): Promise<ChatComposerState | null> {
    const comp = await loadComposerState(cycle.taskId);
    if (!isCurrentLoadCycle(cycle)) return null;
    await refreshTaskWorktree(cycle);
    if (!isCurrentLoadCycle(cycle)) return null;
    composer.value = withActiveBackend(comp);
    composerBackendLoaded = true;
    return composer.value;
  }

  async function loadComposerForSend(
    snapshot: TaskContextSnapshot,
  ): Promise<ChatComposerState | null> {
    const comp = await loadComposerState(snapshot.taskId);
    if (!isCurrentTaskSnapshot(snapshot)) return null;
    await refreshTaskWorktree(snapshot);
    if (!isCurrentTaskSnapshot(snapshot)) return null;
    composer.value = withActiveBackend(comp);
    composerBackendLoaded = true;
    return composer.value;
  }

  async function ensureComposerLoaded(): Promise<ChatComposerState> {
    if (composer.value && composerBackendLoaded) return composer.value;
    const snapshot = captureTaskSnapshot();
    if (!composerLoad) {
      const directLoad = loadComposerForSend(snapshot).catch((err) => {
        console.error("[chat] getComposerState failed", err);
        return null;
      });
      composerLoad = directLoad;
    }
    const pending = composerLoad;
    let loaded = await pending;
    if (composerLoad === pending) composerLoad = null;
    if (!loaded && isCurrentTaskSnapshot(snapshot)) {
      loaded = await loadComposerForSend(snapshot);
    }
    if (!loaded) throw new Error("Composer 尚未就绪");
    return loaded;
  }

  function runtimePhaseKeepsTurnRunning(phase: ChatRuntimePhase): boolean {
    return phase === "running" ||
      phase === "running_and_queued" ||
      phase === "reset_pending_finish";
  }

  function runtimeAbandonedMessage(snapshot: Awaited<ReturnType<typeof getRuntimeSnapshot>>): string {
    const backend = snapshot.backend ?? "agent";
    const turn = snapshot.turnId ? `，turn=${snapshot.turnId}` : "";
    return `上次 ${backend} 运行未正常结束，旧执行态已放弃${turn}。你可以重新发送或重置会话。`;
  }

  function restoreDraftFromRollback(rollback: {
    restoredContent: string;
    restoredAttachments: ChatAttachment[];
    restoredConversationReferences?: ChatConversationReference[];
  }) {
    restoreDraftContent.value = stripRestoredReferences(
      rollback.restoredContent,
      rollback.restoredAttachments,
      rollback.restoredConversationReferences ?? [],
    );
    restoreDraftConversationReferences.value = rollback.restoredConversationReferences ?? [];
    restoreDraftKey.value += 1;
    attachments.value = rollback.restoredAttachments;
  }

  function currentPendingRequestIds(): Set<string> {
    return pendingInteractionRequestIdsForSources({
      asks: pendingAskUsers.value,
      toolConsents: pendingToolConsents.value,
      agentInteractions: pendingAgentInteractions.value,
      architectureChanges: pendingArchitectureChanges.value,
    });
  }

  function applyRuntimeSnapshotForCurrentLoad(
    cycle: TaskLoadCycle,
    runtimeSnapshot: Awaited<ReturnType<typeof getRuntimeSnapshot>> | null,
  ) {
    if (!isCurrentLoadCycle(cycle)) return;
    if (cycle.runtimeSeqBeforeLoad !== runtimeEventSeq) return;
    contextUsage.value = runtimeSnapshot?.contextUsage ?? null;
    isTurnRunning.value = runtimeSnapshot
      ? runtimePhaseKeepsTurnRunning(runtimeSnapshot.phase)
      : false;
    if (runtimeSnapshot?.phase === "abandoned") {
      clearPendingInteractionsForTask(cycle.taskId);
      timeline.upsertTimelineEvent(timeline.createLocalErrorTimelineEvent(
        runtimeAbandonedMessage(runtimeSnapshot),
      ));
    }
    if (runtimeSnapshot?.rollback?.rolledBack) {
      restoreDraftFromRollback(runtimeSnapshot.rollback);
      void ackRestoredRollback(cycle.taskId);
    }
  }

  function scheduleRuntimeSnapshotHydration(cycle: TaskLoadCycle) {
    cancelRuntimeSnapshotHydrationPaint?.();
    const cancelPaint = scheduleAfterPaint(() => {
      if (cancelRuntimeSnapshotHydrationPaint === cancelPaint) {
        cancelRuntimeSnapshotHydrationPaint = null;
      }
      if (!isCurrentLoadCycle(cycle)) return;
      void measurePerfAsync(
        "task-detail.runtime-snapshot",
        async () => {
          const runtimeSnapshot = await loadRuntimeSnapshot(cycle.taskId).catch((err) => {
            console.error("[chat] getRuntimeSnapshot failed", err);
            return null;
          });
          applyRuntimeSnapshotForCurrentLoad(cycle, runtimeSnapshot);
        },
        { detail: cycle.taskId },
      );
    });
    cancelRuntimeSnapshotHydrationPaint = cancelPaint;
  }

  function beginLoadCycle(): TaskLoadCycle {
    cancelScheduledHydration();
    const cycle: TaskLoadCycle = {
      seq: ++loadSeq,
      taskId: props.taskId,
      projectId: props.projectId,
      contextSnapshotVersion: context.captureContextSnapshot(),
      pendingBeforeLoad: currentPendingRequestIds(),
      timelineEventIdsBeforeLoad: new Set(
        timeline.persistedTimelineEvents.value.map((event) => event.id),
      ),
      liveTimelineEventsDuringLoad: new Map(),
      runtimeSeqBeforeLoad: runtimeEventSeq,
    };
    activeLoadCycle = cycle;
    if (context.isPopup.value && !context.conversationRouteState.value.isLiveDraft) {
      context.popupContentReady.value = false;
    }
    composer.value = null;
    composerLoad = null;
    composerBackendLoaded = false;
    return cycle;
  }

  function isCurrentLoadCycle(cycle: TaskLoadCycle): boolean {
    return cycle.seq === loadSeq &&
      cycle.taskId === props.taskId &&
      cycle.projectId === props.projectId &&
      isCurrentTaskSnapshot(cycle);
  }

  function finalizeLoadCycle(cycle: TaskLoadCycle) {
    if (activeLoadCycle?.seq === cycle.seq) {
      activeLoadCycle = null;
    }
    if (
      isCurrentLoadCycle(cycle) &&
      context.isPopup.value
    ) {
      context.popupContentReady.value = true;
    }
  }

  function rememberLiveTimelineEventsDuringLoad(events: readonly AgentTimelineEvent[]) {
    const cycle = activeLoadCycle;
    if (
      !cycle ||
      !isCurrentLoadCycle(cycle)
    ) {
      return;
    }
    for (const event of events) {
      if (event.taskId === cycle.taskId) {
        cycle.liveTimelineEventsDuringLoad.set(event.id, event);
      }
    }
  }

  function scheduleComposerStateHydration(cycle: TaskLoadCycle) {
    cancelComposerStateHydrationPaint?.();
    const cancelPaint = scheduleAfterPaint(() => {
      if (cancelComposerStateHydrationPaint === cancelPaint) {
        cancelComposerStateHydrationPaint = null;
      }
      if (
        !isCurrentLoadCycle(cycle) ||
        composer.value
      ) {
        return;
      }
      const pending = composerLoad ?? measurePerfAsync(
        "task-detail.composer-state.load",
        () => loadComposerForCurrentTask(cycle),
        { detail: cycle.taskId },
      ).catch((err) => {
        console.error("[chat] getComposerState failed", err);
        return null;
      });
      composerLoad = pending;
      void pending.finally(() => {
        if (composerLoad === pending) composerLoad = null;
      });
    });
    cancelComposerStateHydrationPaint = cancelPaint;
  }

  async function loadAll() {
    const cycle = beginLoadCycle();
    try {
      const events = await measurePerfAsync(
        "task-detail.timeline.load",
        () => timeline.loadTimelineEvents(cycle.taskId),
        { detail: cycle.taskId },
      );
      if (
        !isCurrentLoadCycle(cycle)
      ) {
        return;
      }
      const mergePlan = taskTimelineLoadMergePlan({
        loadedEvents: events,
        currentEvents: timeline.persistedTimelineEvents.value,
        eventIdsBeforeLoad: cycle.timelineEventIdsBeforeLoad,
        liveEventsDuringLoad: cycle.liveTimelineEventsDuringLoad,
      });
      timeline.applyLoadedTimelineEvents(events, mergePlan.preserveEventIds);
      const liveEventsToReplay = mergePlan.liveEventsToReplay;
      if (liveEventsToReplay.length > 0) {
        timeline.upsertTimelineEvents(liveEventsToReplay);
      }
      const hydratedRequestIds = hydratePendingInteractions(events, props.taskId);
      const keepRequestIds = pendingInteractionRequestIdsToKeepAfterLoad({
        hydratedRequestIds,
        currentRequestIds: currentPendingRequestIds(),
        pendingBeforeLoadRequestIds: cycle.pendingBeforeLoad,
      });
      clearPendingInteractionsForTask(cycle.taskId, { keepRequestIds });
      scheduleRuntimeSnapshotHydration(cycle);
    } finally {
      finalizeLoadCycle(cycle);
    }
    scheduleComposerStateHydration(cycle);
  }

  async function installRuntimeListeners(): Promise<UnlistenFn[]> {
    return await installUnlistenFns([
      () => onAgentTimelineEvents((events, source) => {
        const taskEvents = events.filter((event) => event.taskId === props.taskId);
        if (taskEvents.length === 0) return;
        rememberLiveTimelineEventsDuringLoad(taskEvents);
        if (source === "batch") {
          timeline.queueTimelineEvents(taskEvents);
        } else {
          for (const event of taskEvents) timeline.upsertTimelineEvent(event);
        }
        syncPendingInteractionsForTimelineEvents(taskEvents, props.taskId);
        if (taskEvents.some((event) => isAgentTimelineToolWindowKind(event.kind))) {
          void scheduleGuideInsertion("tool");
        }
      }),
      () => onTurnStarted((e) => {
        if (e.taskId !== props.taskId) return;
        runtimeEventSeq += 1;
        interruptInFlight.value = false;
        isTurnRunning.value = true;
        timeline.markQueuedUserMessageSuccessful();
      }),
      () => onDone((e) => {
        if (e.taskId !== props.taskId) return;
        runtimeEventSeq += 1;
        interruptInFlight.value = false;
        isTurnRunning.value = false;
        clearPendingInteractionsForTask(e.taskId);
        if (e.rollback?.rolledBack) {
          for (const eventId of e.rollback.removedEventIds) {
            timeline.removeTimelineEvent(eventId);
          }
          restoreDraftFromRollback(e.rollback);
        }
        void scheduleGuideInsertion("idle");
      }),
    ]);
  }

  async function installContextUsageListener(): Promise<UnlistenFn> {
    return await onContextUsage((e) => {
      if (e.taskId !== props.taskId) return;
      contextUsage.value = e;
    });
  }

  function resetForRouteChange() {
    cancelScheduledHydration();
    loadSeq += 1;
    composerStateWriteSeq += 1;
    activeLoadCycle = null;
    composerLoad = null;
    isTurnRunning.value = false;
    interruptInFlight.value = false;
    composer.value = null;
    taskWorktree.value = null;
    contextUsage.value = null;
    restoreDraftConversationReferences.value = [];
  }

  function cancelScheduledHydration() {
    cancelRuntimeSnapshotHydrationPaint?.();
    cancelRuntimeSnapshotHydrationPaint = null;
    cancelComposerStateHydrationPaint?.();
    cancelComposerStateHydrationPaint = null;
  }

  function canAcceptInteractiveDrop(): boolean {
    return nonInterruptMode.value || (!pendingAskUser.value && !pendingToolConsent.value);
  }

  return {
    composer,
    composerForView,
    modelOptionsForView,
    contextUsage,
    isTurnRunning,
    userSendScrollKey,
    restoreDraftKey,
    restoreDraftContent,
    restoreDraftConversationReferences,
    insertDraftTextKey,
    insertDraftTextContent,
    pendingBranchAnchor,
    pendingAskUser,
    pendingAskUsers,
    pendingToolConsent,
    pendingToolConsents,
    pendingAgentActions,
    blockingPendingAgentActions,
    taskRunBlockingReason,
    pendingPlanApproval,
    currentLiliaGoal,
    taskWorktree,
    worktreeOptions,
    worktreeSelectionValue,
    worktreeBusy,
    worktreeError,
    effectiveProjectCwd,
    agentInteractionSettings,
    nonInterruptMode,
    activeBackend,
    sendAgentMessage,
    onSend,
    onExecuteSlashCommand,
    onStartLiliaReview,
    onStartLiliaFixSuggestion,
    onStartLiliaCompact,
    onStartSessionFork,
    onClearBranchAnchor,
    onStartLiliaBatchApply,
    onSetLiliaGoal,
    onRefreshLiliaGoal,
    onClearLiliaGoal,
    onInsertGuide,
    onInsertDraftText,
    onInterrupt,
    onResolveAskUser,
    onResolveToolConsent,
    onResolvePendingAgentAction,
    onComposerUpdate,
    onSelectWorktree,
    onRetryTimelineEvent,
    loadAll,
    loadAgentInteractionSettings,
    installRuntimeListeners,
    installContextUsageListener,
    resetForRouteChange,
    cancelScheduledHydration,
    canAcceptInteractiveDrop,
    scheduleUserGuideInsertion: () => scheduleGuideInsertion("user"),
  };
}

function stripRestoredReferences(
  content: string,
  attachments: ChatAttachment[],
  conversationReferences: ChatConversationReference[],
): string {
  let next = content;
  for (const attachment of attachments) {
    next = next.split(serializeChatAttachmentReference(attachment)).join("");
  }
  next = stripSerializedConversationReferences(next, conversationReferences);
  return next.replace(/[ \t]{2,}/g, " ").trim();
}

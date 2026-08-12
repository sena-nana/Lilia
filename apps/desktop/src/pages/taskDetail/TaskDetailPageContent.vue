<script setup lang="ts">
import "../../styles/chat.css";
/**
 * Task 详情 = 聊天面板。此页只编排路由上下文、timeline、composer 与附件控制器；
 * 具体聊天布局由 TaskDetailChatSurface 负责。
 */

import { computed, defineAsyncComponent, nextTick, onMounted, onUnmounted, ref, shallowRef, watch } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useRoute, useRouter } from "vue-router";
import { isChatBackendKind, type ChatAttachment, type Project, type SuggestionItem, type SuggestionStatus } from "@lilia/contracts";
import { useChatSidebar } from "../../composables/useChatSidebar";
import { useSidebarDisplayMode } from "../../composables/useSidebarDisplayMode";
import { withComponentEpoch } from "@lilia/ui/composables/useComponentEpoch";
import { useTaskAttachments } from "./useTaskAttachments";
import { useTaskComposerController } from "./useTaskComposerController";
import { isAgentDebugFrontendEnabled } from "../../agentDebug/harness";
import {
  useTaskConversationContext,
  type TaskDetailRouteProps,
} from "./useTaskConversationContext";
import { useTaskTimeline } from "./useTaskTimeline";
import { cancelIdleRun, measurePerfAsync, measurePerfSync, runWhenIdle, scheduleAfterPaint } from "@lilia/ui/diagnostics";
import { createLazyLoadState } from "@lilia/ui/utils/lazyLoadState";

const props = withDefaults(defineProps<{
  projectId?: string;
  taskId: string;
  variant?: "main" | "popup";
}>(), {
  variant: "main",
});

const TaskDetailChatSurface = defineAsyncComponent({
  suspensible: false,
  loader: () => taskDetailChatSurfaceLoad.load(),
});

const routeProps = props as TaskDetailRouteProps;
const route = useRoute();
const router = useRouter();
const chatSurfaceRef = ref<InstanceType<typeof TaskDetailChatSurface> | null>(null);
const pendingSurfaceFocus = ref(false);
const chatPageRef = computed<HTMLElement | null>(() =>
  chatSurfaceRef.value?.chatPageRef ?? null,
);
const focusConversationKey = computed(() =>
  shouldRenderChat.value
    ? `${props.variant ?? "main"}:${props.projectId ?? ""}:${props.taskId}`
    : "",
);
const sharedAttachments = ref<ChatAttachment[]>([]);
const suggestions = ref<SuggestionItem[]>([]);
const { sidebarDisplayMode } = useSidebarDisplayMode();
const suggestionsStatus = ref<SuggestionStatus>("idle");
const suggestionsLoadingText = ref("正在寻找灵感");
const suggestionsReady = ref(false);
const sidebarPanelsReady = ref(false);
const sidebarPanelsActivated = ref(false);
const agentInteractionSettingsReady = ref(false);
const draftProjectPickerProjects = shallowRef<Project[]>([]);
let suggestionsSeq = 0;
let draftProjectPickerSeq = 0;
let deferredHydrationSeq = 0;
let backgroundStateLoadSeq = 0;
const taskDetailChatSurfaceLoad = createLazyLoadState(() =>
  measurePerfAsync(
    "task-detail.surface.load",
    async () => (await import("./TaskDetailChatSurface.vue")).default,
  )
);
const taskDetailDeferredUiLoad = createLazyLoadState(() =>
  measurePerfAsync(
    "task-detail.deferred-ui.load",
    async () => await import("./taskDetailDeferredUi"),
    { detail: taskDetailPerfDetail() },
  )
);
const taskDetailSidebarPanelsLoad = createLazyLoadState(() =>
  measurePerfAsync(
    "task-detail.sidebar-panels.module.load",
    async () => await import("./taskDetailSidebarPanels"),
    { detail: taskDetailPerfDetail() },
  )
);
let deferredSettingsHydrationHandle: number | null = null;
let contextUsageListenerInstallHandle: number | null = null;
let dragDropListenerInstallHandle: number | null = null;
let cancelDeferredHydrationPaint: (() => void) | null = null;
let cancelBackgroundStateLoadPaint: (() => void) | null = null;
let cancelDragDropListenerInstallPaint: (() => void) | null = null;
let cancelRuntimeListenerInstallPaint: (() => void) | null = null;
let cancelContextUsageListenerInstallPaint: (() => void) | null = null;
const taskDetailLifecycle = withComponentEpoch();

function taskDetailPerfDetail() {
  return `${props.projectId ?? "orphan"}:${props.taskId}`;
}

function isCurrentTaskDetailRoute(projectId: string | undefined, taskId: string) {
  return taskDetailLifecycle.assertAlive() && props.projectId === projectId && props.taskId === taskId;
}

const conversation = measurePerfSync(
  "task-detail.setup.context",
  () => useTaskConversationContext(routeProps),
  { detail: taskDetailPerfDetail() },
);
const chatSidebar = useChatSidebar();
const timeline: ReturnType<typeof useTaskTimeline> = measurePerfSync(
  "task-detail.setup.timeline",
  () => useTaskTimeline({
    taskId: () => props.taskId,
    backend: () => composerController.composerForView.value.backend,
  }),
  { detail: taskDetailPerfDetail() },
);
const composerController: ReturnType<typeof useTaskComposerController> = measurePerfSync(
  "task-detail.setup.composer-controller",
  () => useTaskComposerController({
    props: routeProps,
    context: conversation,
    timeline,
    attachments: sharedAttachments,
  }),
  { detail: taskDetailPerfDetail() },
);
const attachmentController = measurePerfSync(
  "task-detail.setup.attachments",
  () => useTaskAttachments({
    chatPageRef,
    attachments: sharedAttachments,
    hasContext: () => conversation.hasContext.value,
    canAcceptInteractiveDrop: composerController.canAcceptInteractiveDrop,
  }),
  { detail: taskDetailPerfDetail() },
);

const {
  isPopup,
  project,
  hasContext,
  conversationRouteState,
  isContextLoading,
  isPopupPending,
  shouldRenderChat,
  shouldShowContextLoading,
  emptyHeadline,
  contextSearchCwd,
} = conversation;
const {
  timelineEvents,
  canRetryEvent,
} = timeline;
const {
  attachments,
  viewingImage,
  droppedAttachmentAppendKey,
  fileDropActive,
} = attachmentController;
const {
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
  pendingAskUser,
  pendingToolConsent,
  pendingAgentActions,
  blockingPendingAgentActions,
  pendingPlanApproval,
  nonInterruptMode,
} = composerController;
const hydratedTaskHandoffs = new Set<string>();

async function hydrateTaskHandoff(taskId: string) {
  if (!taskId || hydratedTaskHandoffs.has(taskId)) return;
  const { getImportedTaskHandoff } = await import("../../services/taskHandoff");
  const imported = await getImportedTaskHandoff(taskId);
  if (!imported || imported.taskId !== taskId || !imported.prompt.trim()) return;
  hydratedTaskHandoffs.add(taskId);
  composerController.onInsertDraftText(imported.prompt);
}

function queryString(value: unknown): string | null {
  if (Array.isArray(value)) return typeof value[0] === "string" ? value[0] : null;
  return typeof value === "string" ? value : null;
}

function decodeDraftTextParam(value: string | null): string {
  if (!value) return "";
  try {
    const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
    const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
    const binary = atob(padded);
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    return new TextDecoder().decode(bytes);
  } catch (err) {
    console.error("[popup] decode initial draft failed", err);
    return "";
  }
}

const isIdleEmptyDraft = computed(() =>
  shouldRenderChat.value &&
  conversationRouteState.value.isLiveDraft &&
  timelineEvents.value.length === 0 &&
  !isTurnRunning.value &&
  blockingPendingAgentActions.value.length === 0 &&
  !pendingAskUser.value &&
  !pendingToolConsent.value,
);
const shouldLoadSuggestions = computed(() =>
  suggestionsReady.value &&
  !!props.projectId &&
  isIdleEmptyDraft.value,
);
const shouldShowDraftProjectPicker = computed(() =>
  sidebarDisplayMode.value === "unified" &&
  !props.projectId &&
  isIdleEmptyDraft.value,
);

async function loadTaskDetailDeferredUiModule() {
  return taskDetailDeferredUiLoad.load();
}

async function loadTaskDetailSidebarPanelsModule() {
  return taskDetailSidebarPanelsLoad.load();
}

async function loadSuggestions(forceRefresh = false) {
  const seq = ++suggestionsSeq;
  const routeProjectId = props.projectId;
  const routeTaskId = props.taskId;
  const module = await loadTaskDetailDeferredUiModule();
  const isCurrent = () =>
    seq === suggestionsSeq &&
    isCurrentTaskDetailRoute(routeProjectId, routeTaskId);
  if (!isCurrent()) return;
  await module.loadTaskDetailSuggestions({
    detail: taskDetailPerfDetail(),
    projectId: routeProjectId ?? null,
    forceRefresh,
    shouldLoad: shouldLoadSuggestions.value,
    seq,
    isCurrentSeq: (value) => value === seq && isCurrent(),
    setSuggestions: (items) => {
      if (!isCurrent()) return;
      suggestions.value = items;
    },
    setStatus: (status) => {
      if (!isCurrent()) return;
      suggestionsStatus.value = status;
    },
    setLoadingText: (text) => {
      if (!isCurrent()) return;
      suggestionsLoadingText.value = text;
    },
  });
}

async function moveCurrentDraftToProject(projectId: string) {
  const sourceProjectId = props.projectId;
  const sourceTaskId = props.taskId;
  const module = await loadTaskDetailDeferredUiModule();
  if (!isCurrentTaskDetailRoute(sourceProjectId, sourceTaskId)) return;
  await module.moveTaskDetailDraftToProject({
    detail: taskDetailPerfDetail(),
    projectId,
    attachments: sharedAttachments.value,
    router,
    isSourceCurrent: () => isCurrentTaskDetailRoute(sourceProjectId, sourceTaskId),
    isTargetCurrent: (taskId) => isCurrentTaskDetailRoute(projectId, taskId),
    getDraftSnapshot: () => chatSurfaceRef.value?.getComposerDraftSnapshot() ?? { content: "" },
    restoreDraft: (content, nextAttachments) => {
      if (!taskDetailLifecycle.assertAlive()) return;
      sharedAttachments.value = nextAttachments;
      if (content.trim()) {
        composerController.onInsertDraftText(content);
      }
    },
  });
}

function onDraftProjectPickerError(message: string) {
  timeline.upsertTimelineEvent(timeline.createLocalErrorTimelineEvent(message));
}

function scheduleDeferredTaskDetailHydration() {
  cancelDeferredTaskDetailHydrationSchedule();
  const seq = ++deferredHydrationSeq;
  const cancelPaint = scheduleAfterPaint(() => {
    if (cancelDeferredHydrationPaint === cancelPaint) cancelDeferredHydrationPaint = null;
    if (!taskDetailLifecycle.assertAlive() || seq !== deferredHydrationSeq) return;
    suggestionsReady.value = true;
    sidebarPanelsReady.value = true;
    deferredSettingsHydrationHandle = runWhenIdle(() => {
      deferredSettingsHydrationHandle = null;
      if (!taskDetailLifecycle.assertAlive() || seq !== deferredHydrationSeq) return;
      void conversation.hydrateMainTaskRecord();
      void measurePerfAsync(
        "task-detail.agent-settings.load",
        () => composerController.loadAgentInteractionSettings(),
        { detail: taskDetailPerfDetail() },
      ).then(() => {
        if (!taskDetailLifecycle.assertAlive() || seq !== deferredHydrationSeq) return;
        agentInteractionSettingsReady.value = true;
      }).catch((err) => {
        console.error("[task-detail] deferred agent settings hydration failed", err);
      });
    });
  });
  cancelDeferredHydrationPaint = cancelPaint;
}

function scheduleBackgroundTaskDetailStateLoad() {
  const seq = ++backgroundStateLoadSeq;
  const routeProjectId = props.projectId;
  const routeTaskId = props.taskId;
  cancelBackgroundStateLoadPaint?.();
  const cancelPaint = scheduleAfterPaint(() => {
    if (cancelBackgroundStateLoadPaint === cancelPaint) {
      cancelBackgroundStateLoadPaint = null;
    }
    if (
      !taskDetailLifecycle.assertAlive() ||
      seq !== backgroundStateLoadSeq ||
      routeProjectId !== props.projectId ||
      routeTaskId !== props.taskId
    ) {
      return;
    }
    void measurePerfAsync(
      "task-detail.background-state",
      async () => {
        await conversation.hydrateMainContext();
        if (
          !taskDetailLifecycle.assertAlive() ||
          seq !== backgroundStateLoadSeq ||
          routeProjectId !== props.projectId ||
          routeTaskId !== props.taskId
        ) {
          return;
        }
        if (!conversation.conversationRouteState.value.isLiveDraft) {
          await composerController.loadAll();
        }
      },
      { detail: taskDetailPerfDetail() },
    ).catch((err) => {
      console.error("[task-detail] background state hydration failed", err);
    });
  });
  cancelBackgroundStateLoadPaint = cancelPaint;
}

function cancelBackgroundTaskDetailStateLoadSchedule() {
  backgroundStateLoadSeq += 1;
  cancelBackgroundStateLoadPaint?.();
  cancelBackgroundStateLoadPaint = null;
}

function cancelDeferredTaskDetailHydrationSchedule() {
  cancelDeferredHydrationPaint?.();
  cancelDeferredHydrationPaint = null;
  if (deferredSettingsHydrationHandle !== null) {
    cancelIdleRun(deferredSettingsHydrationHandle);
    deferredSettingsHydrationHandle = null;
  }
}

const unlisteners: UnlistenFn[] = [];
let unregisterDebugPanel: (() => void) | null = null;
let unregisterArchitecturePanel: (() => void) | null = null;
let unregisterIabPanel: (() => void) | null = null;
let unregisterSharedServicesPanel: (() => void) | null = null;
let runtimeListenerInstallSeq = 0;
let contextUsageListenerInstallSeq = 0;
let dragDropListenerInstallSeq = 0;
let debugPanelRegistrationSeq = 0;
let architecturePanelRegistrationSeq = 0;
let iabPanelRegistrationSeq = 0;
let sharedServicesPanelRegistrationSeq = 0;
let dragDropListenerInstalled = false;
let contextUsageListenerInstalled = false;

function supportsSidebarIab(backend: string) {
  return isChatBackendKind(backend);
}

function shouldRegisterDebugPanel() {
  if (isAgentDebugFrontendEnabled()) return hasContext.value;
  return sidebarPanelsActivated.value &&
    agentInteractionSettingsReady.value &&
    hasContext.value &&
    composerController.agentInteractionSettings.debug.value;
}

function shouldRegisterArchitecturePanel() {
  return sidebarPanelsReady.value &&
    sidebarPanelsActivated.value &&
    hasContext.value &&
    !!props.projectId &&
    !isPopup.value;
}

function shouldRegisterIabPanel() {
  return sidebarPanelsReady.value &&
    sidebarPanelsActivated.value &&
    hasContext.value &&
    !isPopup.value &&
    supportsSidebarIab(composerForView.value.backend);
}

function shouldRegisterSharedServicesPanel() {
  return sidebarPanelsReady.value &&
    sidebarPanelsActivated.value &&
    hasContext.value &&
    !isPopup.value;
}

function syncDebugPanelRegistration() {
  const seq = ++debugPanelRegistrationSeq;
  if (!shouldRegisterDebugPanel()) {
    unregisterDebugPanel?.();
    unregisterDebugPanel = null;
    return;
  }
  if (unregisterDebugPanel) return;
  void loadTaskDetailSidebarPanelsModule()
    .then((module) => {
      if (!taskDetailLifecycle.assertAlive() || seq !== debugPanelRegistrationSeq || unregisterDebugPanel) return;
      if (!shouldRegisterDebugPanel()) return;
      unregisterDebugPanel = module.registerTaskDetailDebugSidebarPanel();
    })
    .catch((err) => {
      console.error("[task-detail] load debug sidebar panel module failed", err);
    });
}

function syncArchitecturePanelRegistration() {
  const seq = ++architecturePanelRegistrationSeq;
  if (!shouldRegisterArchitecturePanel()) {
    unregisterArchitecturePanel?.();
    unregisterArchitecturePanel = null;
    return;
  }
  if (unregisterArchitecturePanel) return;
  void loadTaskDetailSidebarPanelsModule()
    .then((module) => {
      if (!taskDetailLifecycle.assertAlive() || seq !== architecturePanelRegistrationSeq || unregisterArchitecturePanel) {
        return;
      }
      if (!shouldRegisterArchitecturePanel()) return;
      unregisterArchitecturePanel = module.registerTaskDetailArchitectureSidebarPanel();
    })
    .catch((err) => {
      console.error("[task-detail] load architecture sidebar panel module failed", err);
    });
}

function syncIabPanelRegistration() {
  const seq = ++iabPanelRegistrationSeq;
  if (!shouldRegisterIabPanel()) {
    unregisterIabPanel?.();
    unregisterIabPanel = null;
    return;
  }
  if (unregisterIabPanel) return;
  void loadTaskDetailSidebarPanelsModule()
    .then((module) => {
      if (!taskDetailLifecycle.assertAlive() || seq !== iabPanelRegistrationSeq || unregisterIabPanel) return;
      if (!shouldRegisterIabPanel()) return;
      unregisterIabPanel = module.registerTaskDetailIabSidebarPanel();
    })
    .catch((err) => {
      console.error("[task-detail] load IAB sidebar panel module failed", err);
    });
}

function syncSharedServicesPanelRegistration() {
  const seq = ++sharedServicesPanelRegistrationSeq;
  if (!shouldRegisterSharedServicesPanel()) {
    unregisterSharedServicesPanel?.();
    unregisterSharedServicesPanel = null;
    return;
  }
  if (unregisterSharedServicesPanel) return;
  void loadTaskDetailSidebarPanelsModule()
    .then((module) => {
      if (
        !taskDetailLifecycle.assertAlive() ||
        seq !== sharedServicesPanelRegistrationSeq ||
        unregisterSharedServicesPanel
      ) {
        return;
      }
      if (!shouldRegisterSharedServicesPanel()) return;
      unregisterSharedServicesPanel = module.registerTaskDetailSharedServicesSidebarPanel();
    })
    .catch((err) => {
      console.error("[task-detail] load shared services sidebar panel module failed", err);
    });
}

function scheduleDeferredDragDropListenerInstall() {
  if (dragDropListenerInstalled) return;
  cancelDragDropListenerInstallSchedule();
  const seq = ++dragDropListenerInstallSeq;
  const cancelPaint = scheduleAfterPaint(() => {
    if (cancelDragDropListenerInstallPaint === cancelPaint) {
      cancelDragDropListenerInstallPaint = null;
    }
    if (!taskDetailLifecycle.assertAlive() || seq !== dragDropListenerInstallSeq || dragDropListenerInstalled) return;
    dragDropListenerInstallHandle = runWhenIdle(() => {
      dragDropListenerInstallHandle = null;
      if (!taskDetailLifecycle.assertAlive() || seq !== dragDropListenerInstallSeq || dragDropListenerInstalled) {
        return;
      }
      void measurePerfAsync(
        "task-detail.dragdrop.install",
        async () => {
          const unlisten = await attachmentController.installDragDropListener();
          if (!taskDetailLifecycle.assertAlive() || seq !== dragDropListenerInstallSeq) {
            await unlisten();
            return;
          }
          dragDropListenerInstalled = true;
          unlisteners.push(unlisten);
        },
        { detail: `${props.projectId ?? "orphan"}:${props.taskId}` },
      );
    });
  });
  cancelDragDropListenerInstallPaint = cancelPaint;
}

function installRuntimeListenersInBackground() {
  const seq = ++runtimeListenerInstallSeq;
  cancelRuntimeListenerInstallPaint?.();
  const cancelPaint = scheduleAfterPaint(() => {
    if (cancelRuntimeListenerInstallPaint === cancelPaint) {
      cancelRuntimeListenerInstallPaint = null;
    }
    if (!taskDetailLifecycle.assertAlive() || seq !== runtimeListenerInstallSeq) return;
    void measurePerfAsync(
      "task-detail.runtime-listeners.install",
      () => composerController.installRuntimeListeners(),
      { detail: `${props.projectId ?? "orphan"}:${props.taskId}` },
    )
      .then(async (listeners) => {
        if (!taskDetailLifecycle.assertAlive() || seq !== runtimeListenerInstallSeq) {
          for (const unlisten of listeners) {
            try {
              await unlisten();
            } catch {
              // ignore
            }
          }
          return;
        }
        unlisteners.push(...listeners);
      })
      .catch((err) => {
        console.error("[task-detail] install runtime listeners failed", err);
      });
  });
  cancelRuntimeListenerInstallPaint = cancelPaint;
}

function installContextUsageListenerInBackground() {
  if (contextUsageListenerInstalled) return;
  cancelContextUsageListenerInstallSchedule();
  const seq = ++contextUsageListenerInstallSeq;
  const cancelPaint = scheduleAfterPaint(() => {
    if (cancelContextUsageListenerInstallPaint === cancelPaint) {
      cancelContextUsageListenerInstallPaint = null;
    }
    if (!taskDetailLifecycle.assertAlive() || seq !== contextUsageListenerInstallSeq) return;
    contextUsageListenerInstallHandle = runWhenIdle(async () => {
      contextUsageListenerInstallHandle = null;
      if (!taskDetailLifecycle.assertAlive() || seq !== contextUsageListenerInstallSeq) return;
      try {
        const unlisten = await measurePerfAsync(
          "task-detail.context-usage-listener.install",
          () => composerController.installContextUsageListener(),
          { detail: `${props.projectId ?? "orphan"}:${props.taskId}` },
        );
        if (!taskDetailLifecycle.assertAlive() || seq !== contextUsageListenerInstallSeq) {
          await unlisten();
          return;
        }
        contextUsageListenerInstalled = true;
        unlisteners.push(unlisten);
      } catch (err) {
        console.error("[task-detail] install context usage listener failed", err);
      }
    });
  });
  cancelContextUsageListenerInstallPaint = cancelPaint;
}

function cancelDragDropListenerInstallSchedule() {
  cancelDragDropListenerInstallPaint?.();
  cancelDragDropListenerInstallPaint = null;
  if (dragDropListenerInstallHandle !== null) {
    cancelIdleRun(dragDropListenerInstallHandle);
    dragDropListenerInstallHandle = null;
  }
}

function cancelContextUsageListenerInstallSchedule() {
  cancelContextUsageListenerInstallPaint?.();
  cancelContextUsageListenerInstallPaint = null;
  if (contextUsageListenerInstallHandle !== null) {
    cancelIdleRun(contextUsageListenerInstallHandle);
    contextUsageListenerInstallHandle = null;
  }
}

onMounted(() => {
  installRuntimeListenersInBackground();
  installContextUsageListenerInBackground();
  scheduleDeferredDragDropListenerInstall();
  if (!conversation.isPopup.value) {
    scheduleBackgroundTaskDetailStateLoad();
  }
  scheduleDeferredTaskDetailHydration();
});

onUnmounted(async () => {
  runtimeListenerInstallSeq += 1;
  contextUsageListenerInstallSeq += 1;
  dragDropListenerInstallSeq += 1;
  debugPanelRegistrationSeq += 1;
  architecturePanelRegistrationSeq += 1;
  iabPanelRegistrationSeq += 1;
  draftProjectPickerSeq += 1;
  deferredHydrationSeq += 1;
  cancelBackgroundTaskDetailStateLoadSchedule();
  cancelRuntimeListenerInstallPaint?.();
  cancelRuntimeListenerInstallPaint = null;
  cancelDragDropListenerInstallSchedule();
  cancelContextUsageListenerInstallSchedule();
  cancelDeferredTaskDetailHydrationSchedule();
  composerController.cancelScheduledHydration();
  timeline.disposeTimeline();
  unregisterDebugPanel?.();
  unregisterDebugPanel = null;
  unregisterArchitecturePanel?.();
  unregisterArchitecturePanel = null;
  unregisterIabPanel?.();
  unregisterIabPanel = null;
  unregisterSharedServicesPanel?.();
  unregisterSharedServicesPanel = null;
  for (const unlisten of unlisteners) {
    try {
      await unlisten();
    } catch {
      // ignore
    }
  }
  unlisteners.length = 0;
  dragDropListenerInstalled = false;
  contextUsageListenerInstalled = false;
});

watch(
  () => chatSidebar.state.open,
  (open) => {
    if (!open || sidebarPanelsActivated.value) return;
    sidebarPanelsActivated.value = true;
  },
  { immediate: true },
);

watch(
  () => [props.variant, props.projectId, props.taskId, conversation.hasContext.value] as const,
  ([variant, _projectId, _taskId, ready]) => {
    if (variant !== "popup" || !ready) return;
    void measurePerfAsync(
      "task-detail.popup-state",
      () => composerController.loadAll(),
      { detail: taskDetailPerfDetail() },
    ).catch((err) => {
      console.error("[task-detail] popup state hydration failed", err);
    });
  },
  { immediate: true },
);

watch(
  () => [props.projectId, props.taskId] as const,
  () => {
    deferredHydrationSeq += 1;
    dragDropListenerInstallSeq += 1;
    contextUsageListenerInstallSeq += 1;
    cancelDragDropListenerInstallSchedule();
    cancelContextUsageListenerInstallSchedule();
    cancelDeferredTaskDetailHydrationSchedule();
    cancelBackgroundTaskDetailStateLoadSchedule();
    suggestionsReady.value = false;
    sidebarPanelsReady.value = false;
    sidebarPanelsActivated.value = chatSidebar.state.open;
    agentInteractionSettingsReady.value = false;
    suggestionsSeq += 1;
    draftProjectPickerSeq += 1;
    suggestions.value = [];
    suggestionsStatus.value = "idle";
    suggestionsLoadingText.value = "正在寻找灵感";
    draftProjectPickerProjects.value = [];
    conversation.prepareForRouteChange();
    composerController.resetForRouteChange();
    timeline.resetTimeline();
    attachmentController.resetAttachments();
    timeline.resubscribeDebugTimeline();
    if (!dragDropListenerInstalled) {
      scheduleDeferredDragDropListenerInstall();
    }
    installContextUsageListenerInBackground();
    if (!conversation.isPopup.value) {
      scheduleBackgroundTaskDetailStateLoad();
      scheduleDeferredTaskDetailHydration();
    }
  },
);

watch(
  () => [
    shouldLoadSuggestions.value,
    props.projectId ?? "",
    props.taskId,
    timelineEvents.value.length,
  ] as const,
  ([ready]) => {
    if (!ready) {
      suggestionsSeq += 1;
      suggestions.value = [];
      suggestionsStatus.value = "idle";
      suggestionsLoadingText.value = "正在寻找灵感";
      return;
    }
    void loadSuggestions(false);
  },
  { immediate: true },
);

watch(
  () => [
    sidebarPanelsActivated.value,
    agentInteractionSettingsReady.value,
    hasContext.value,
    composerController.agentInteractionSettings.debug.value,
  ] as const,
  syncDebugPanelRegistration,
  { immediate: true },
);

watch(
  () => [
    sidebarPanelsReady.value,
    sidebarPanelsActivated.value,
    hasContext.value,
    props.projectId ?? "",
    isPopup.value,
  ] as const,
  syncArchitecturePanelRegistration,
  { immediate: true },
);

watch(
  shouldShowDraftProjectPicker,
  (visible) => {
    if (!visible) {
      draftProjectPickerSeq += 1;
      return;
    }
    if (draftProjectPickerProjects.value.length > 0) return;
    const seq = ++draftProjectPickerSeq;
    const routeProjectId = props.projectId;
    const routeTaskId = props.taskId;
    void loadTaskDetailDeferredUiModule()
      .then((module) => module.listTaskDetailDraftProjects(taskDetailPerfDetail()))
      .then((projects) => {
        if (
          seq !== draftProjectPickerSeq ||
          !isCurrentTaskDetailRoute(routeProjectId, routeTaskId) ||
          !shouldShowDraftProjectPicker.value
        ) {
          return;
        }
        draftProjectPickerProjects.value = projects;
      })
      .catch((err) => {
        console.error("[task-detail] load draft project picker deps failed", err);
      });
  },
  { immediate: true },
);

watch(
  () => [
    sidebarPanelsReady.value,
    sidebarPanelsActivated.value,
    hasContext.value,
    isPopup.value,
    composerForView.value.backend,
  ] as const,
  syncIabPanelRegistration,
  { immediate: true },
);

watch(
  () => [
    sidebarPanelsReady.value,
    sidebarPanelsActivated.value,
    hasContext.value,
    isPopup.value,
  ] as const,
  syncSharedServicesPanelRegistration,
  { immediate: true },
);

watch(
  () => [
    composerController.pendingAskUsers.value.length,
    composerController.pendingToolConsents.value.length,
    composerController.pendingPlanApproval.value?.turnId ?? "",
  ] as const,
  ([askCount, consentCount, planTurn], [prevAskCount, prevConsentCount, prevPlanTurn]) => {
    if (
      askCount > prevAskCount ||
      consentCount > prevConsentCount ||
      (planTurn && planTurn !== prevPlanTurn)
    ) {
      void composerController.scheduleUserGuideInsertion();
    }
  },
);

watch(
  () => [props.taskId, route.query.draft] as const,
  ([taskId, draft]) => {
    if (props.variant !== "popup" || !taskId) return;
    const text = decodeDraftTextParam(queryString(draft));
    if (text) composerController.onInsertDraftText(text);
  },
  { immediate: true },
);

watch(
  () => [props.taskId, shouldRenderChat.value, timelineEvents.value.length] as const,
  ([taskId, ready, eventCount]) => {
    if (!ready || eventCount > 0 || !taskId) return;
    void hydrateTaskHandoff(taskId).catch((error) => {
      console.error("[task-handoff] load imported handoff failed", error);
    });
  },
  { immediate: true },
);

watch(
  chatSurfaceRef,
  (surface) => {
    if (!surface || !pendingSurfaceFocus.value) return;
    pendingSurfaceFocus.value = false;
    surface.focusComposer();
  },
);

watch(
  focusConversationKey,
  async (key) => {
    if (!key) return;
    pendingSurfaceFocus.value = true;
    await nextTick();
    if (!taskDetailLifecycle.assertAlive() || key !== focusConversationKey.value) return;
    if (!chatSurfaceRef.value) return;
    pendingSurfaceFocus.value = false;
    chatSurfaceRef.value.focusComposer();
  },
  { immediate: true },
);
</script>

<template>
  <TaskDetailChatSurface
    ref="chatSurfaceRef"
    :task-id="taskId"
    :project-id="projectId"
    :is-popup="isPopup"
    :should-render-chat="shouldRenderChat"
    :is-popup-pending="isPopupPending"
    :should-show-context-loading="shouldShowContextLoading"
    :is-context-loading="isContextLoading"
    :file-drop-active="fileDropActive"
    :timeline-events="timelineEvents"
    :empty-headline="emptyHeadline"
    :is-turn-running="isTurnRunning"
    :project-cwd="project?.cwd ?? null"
    :context-search-cwd="composerController.effectiveProjectCwd.value ?? contextSearchCwd"
    :active-plan-approval-turn-id="pendingPlanApproval?.turnId ?? null"
    :user-send-scroll-key="userSendScrollKey"
    :restore-draft-key="restoreDraftKey"
    :restore-draft-content="restoreDraftContent"
    :restore-draft-conversation-references="restoreDraftConversationReferences"
    :insert-draft-text-key="insertDraftTextKey"
    :insert-draft-text-content="insertDraftTextContent"
    :pending-branch-anchor="composerController.pendingBranchAnchor.value"
    :pending-agent-actions="pendingAgentActions"
    :has-blocking-pending-action="blockingPendingAgentActions.length > 0"
    :task-run-blocking-reason="composerController.taskRunBlockingReason.value"
    :current-lilia-goal="composerController.currentLiliaGoal.value"
    :show-expired-pending-actions="nonInterruptMode"
    :can-retry-event="canRetryEvent"
    :composer-state="composerForView"
    :worktree-value="composerController.worktreeSelectionValue.value"
    :worktree-options="composerController.worktreeOptions.value"
    :worktree-busy="composerController.worktreeBusy.value"
    :worktree-error="composerController.worktreeError.value"
    :model-options="modelOptionsForView"
    :context-usage="contextUsage"
    :attachments="attachments"
    :append-attachments-to-end-key="droppedAttachmentAppendKey"
    :pending-ask="nonInterruptMode ? null : pendingAskUser"
    :tool-consent="nonInterruptMode ? null : pendingToolConsent"
    :viewing-image="viewingImage"
    :suggestions="suggestions"
    :suggestions-status="suggestionsStatus"
    :suggestions-loading-text="suggestionsLoadingText"
    :suggestions-visible="shouldLoadSuggestions"
    :show-draft-project-picker="shouldShowDraftProjectPicker"
    :draft-project-picker-projects="draftProjectPickerProjects"
    @resolve-pending-agent-action="composerController.onResolvePendingAgentAction"
    @retry-event="composerController.onRetryTimelineEvent"
    @open-image="viewingImage = $event"
    @close-image="viewingImage = null"
    @insert-guide="composerController.onInsertGuide"
    @set-lilia-goal="composerController.onSetLiliaGoal"
    @refresh-lilia-goal="composerController.onRefreshLiliaGoal"
    @clear-lilia-goal="composerController.onClearLiliaGoal"
    @insert-draft-text="composerController.onInsertDraftText"
    @clear-branch-anchor="composerController.onClearBranchAnchor"
    @send="composerController.onSend"
    @start-lilia-review="composerController.onStartLiliaReview"
    @start-lilia-fix-suggestion="composerController.onStartLiliaFixSuggestion"
    @start-lilia-compact="composerController.onStartLiliaCompact"
    @start-session-fork="composerController.onStartSessionFork"
    @execute-slash-command="composerController.onExecuteSlashCommand"
    @start-lilia-batch-apply="composerController.onStartLiliaBatchApply"
    @interrupt="composerController.onInterrupt"
    @update-composer="composerController.onComposerUpdate"
    @composer-draft-change="composerController.onComposerDraftChange"
    @select-worktree="composerController.onSelectWorktree"
    @remove-attachment="attachmentController.removeAttachment"
    @pick-attachments="attachmentController.onPickAttachments"
    @add-context-attachment="attachmentController.addContextAttachment"
    @resolve-ask-user="composerController.onResolveAskUser"
    @resolve-tool-consent="composerController.onResolveToolConsent"
    @refresh-suggestions="loadSuggestions(true)"
    @select-draft-project="moveCurrentDraftToProject"
    @created-draft-project="(project) => moveCurrentDraftToProject(project.id)"
    @draft-project-picker-error="onDraftProjectPickerError"
  />
</template>

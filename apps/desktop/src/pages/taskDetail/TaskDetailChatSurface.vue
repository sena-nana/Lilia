<script setup lang="ts">
import { defineAsyncComponent, onBeforeUnmount, onMounted, ref, watch } from "vue";
import Plus from "@lucide/vue/dist/esm/icons/plus.mjs";
import type {
  AgentTimelineEvent,
  AskUserResult,
  ChatBranchAnchor,
  ChatAttachment,
  ChatComposerState,
  ChatConversationReference,
  ChatContextUsage,
  ChatModelOption,
  ChatSlashCommandWorkflow,
  ChatWorkflow,
  LiliaThreadGoal,
  LiliaReviewTarget,
  Project,
  SuggestionItem,
  SuggestionStatus,
} from "@lilia/contracts";
import type { LiliaBatchApplyInput } from "@lilia/contracts";
import { TITLE_UPDATE_ACTION_KIND } from "@lilia/contracts";
import type { ChatImageViewerSource } from "../../components/chat/imageViewer";
import { cancelIdleRun, measurePerfAsync, measurePerfSync, runWhenIdle, scheduleAfterPaint } from "@lilia/ui/diagnostics";
import { createLazyLoadState } from "@lilia/ui/utils/lazyLoadState";
import { loadChatComposer, loadChatTranscript } from "./taskDetailLazyLoaders";
import type { PendingAsk } from "../../composables/useAskUser";
import { useChatSidebar } from "../../composables/useChatSidebar";
import { withComponentEpoch } from "@lilia/ui/composables/useComponentEpoch";
import type {
  PendingAgentAction,
  PendingAgentActionResolution,
} from "../../composables/pendingAgentActions";
import type {
  ToolConsentDecision,
  ToolConsentRequest,
  ToolConsentUpdatedInput,
} from "../../services/chat";
import type { TaskTodo } from "../../services/todos";

const chatSuggestionsLoad = createLazyLoadState(() =>
  measurePerfAsync(
    "task-detail.chat-suggestions.load",
    async () => (await import("../../components/chat/ChatSuggestions.vue")).default,
  )
);
const composerProjectPickerLoad = createLazyLoadState(() =>
  measurePerfAsync(
    "task-detail.project-picker.load",
    async () => (await import("../../components/chat/ComposerProjectPicker.vue")).default,
  )
);
const imageViewerLoad = createLazyLoadState(() =>
  measurePerfAsync(
    "task-detail.image-viewer.load",
    async () => (await import("../../components/chat/ImageViewer.vue")).default,
  )
);
const todoFloatLoad = createLazyLoadState(() =>
  measurePerfAsync(
    "task-detail.todo-float.load",
    async () => (await import("../../components/todo/TodoFloat.vue")).default,
  )
);
const chatSidebarHostLoad = createLazyLoadState(() =>
  measurePerfAsync(
    "task-detail.sidebar-host.load",
    async () => (await import("../../components/chat/ChatSidebarHost.vue")).default,
  )
);

const ChatTranscript = defineAsyncComponent({
  suspensible: false,
  loader: () => loadChatTranscript(),
});
const ChatComposer = defineAsyncComponent({
  suspensible: false,
  loader: () => loadChatComposer(),
});
const ChatSuggestions = defineAsyncComponent({
  suspensible: false,
  loader: () => chatSuggestionsLoad.load(),
});
const ComposerProjectPicker = defineAsyncComponent({
  suspensible: false,
  loader: () => composerProjectPickerLoad.load(),
});
const ImageViewer = defineAsyncComponent({
  suspensible: false,
  loader: () => imageViewerLoad.load(),
});
const TodoFloat = defineAsyncComponent({
  suspensible: false,
  loader: () => todoFloatLoad.load(),
});
const ChatSidebarHost = defineAsyncComponent({
  suspensible: false,
  loader: () => chatSidebarHostLoad.load(),
});

const props = defineProps<{
  taskId: string;
  projectId?: string;
  isPopup: boolean;
  shouldRenderChat: boolean;
  isPopupPending: boolean;
  shouldShowContextLoading: boolean;
  isContextLoading: boolean;
  fileDropActive: boolean;
  timelineEvents: AgentTimelineEvent[];
  emptyHeadline: string;
  isTurnRunning: boolean;
  projectCwd: string | null;
  contextSearchCwd: string | null;
  activePlanApprovalTurnId: string | null;
  userSendScrollKey: number;
  restoreDraftKey: number;
  restoreDraftContent: string;
  restoreDraftConversationReferences?: ChatConversationReference[];
  insertDraftTextKey: number;
  insertDraftTextContent: string;
  pendingBranchAnchor: ChatBranchAnchor | null;
  pendingAgentActions: PendingAgentAction[];
  hasBlockingPendingAction: boolean;
  taskRunBlockingReason: string | null;
  currentLiliaGoal: LiliaThreadGoal | null;
  showExpiredPendingActions: boolean;
  canRetryEvent: (event: AgentTimelineEvent) => boolean;
  composerState: ChatComposerState;
  worktreeValue: string;
  worktreeOptions: Array<{ value: string; label: string; hint?: string }>;
  worktreeBusy: boolean;
  worktreeError: string | null;
  modelOptions: ChatModelOption[];
  contextUsage: ChatContextUsage | null;
  attachments: ChatAttachment[];
  appendAttachmentsToEndKey: number;
  pendingAsk: PendingAsk | null;
  toolConsent: ToolConsentRequest | null;
  viewingImage: ChatImageViewerSource | null;
  suggestions: SuggestionItem[];
  suggestionsStatus: SuggestionStatus;
  suggestionsLoadingText: string;
  suggestionsVisible: boolean;
  showDraftProjectPicker?: boolean;
  draftProjectPickerProjects?: Project[];
}>();

const emit = defineEmits<{
  "resolve-pending-agent-action": [resolution: PendingAgentActionResolution];
  "retry-event": [event: AgentTimelineEvent];
  "open-image": [image: ChatImageViewerSource];
  "close-image": [];
  "insert-guide": [todo: TaskTodo];
  "set-lilia-goal": [objective: string];
  "refresh-lilia-goal": [];
  "clear-lilia-goal": [];
  "insert-draft-text": [text: string];
  "clear-branch-anchor": [];
  send: [
    content: string,
    attachments: ChatAttachment[],
    conversationReferences: ChatConversationReference[],
    workflow?: ChatWorkflow | null,
  ];
  "start-lilia-review": [
    content: string,
    attachments: ChatAttachment[],
    conversationReferences: ChatConversationReference[],
    target: LiliaReviewTarget,
  ];
  "start-lilia-fix-suggestion": [
    content: string,
    attachments: ChatAttachment[],
    conversationReferences: ChatConversationReference[],
    target: LiliaReviewTarget,
  ];
  "start-lilia-compact": [];
  "start-session-fork": [anchor: ChatBranchAnchor];
  "execute-slash-command": [workflow: ChatSlashCommandWorkflow];
  "start-lilia-batch-apply": [input: LiliaBatchApplyInput];
  interrupt: [];
  "update-composer": [next: ChatComposerState];
  "select-worktree": [value: string];
  "remove-attachment": [attachmentId: string];
  "pick-attachments": [];
  "add-context-attachment": [attachment: ChatAttachment];
  "resolve-ask-user": [result: AskUserResult];
  "resolve-tool-consent": [
    decision: ToolConsentDecision,
    message?: string,
    updatedInput?: ToolConsentUpdatedInput,
  ];
  "refresh-suggestions": [];
  "select-draft-project": [projectId: string];
  "created-draft-project": [project: Project];
  "draft-project-picker-error": [message: string];
  "composer-draft-change": [content: string];
}>();

interface ChatComposerHandle {
  fillSuggestionPrompt: (prompt: string) => void;
  focusInput: () => void;
  getDraftSnapshot: () => { content: string };
  triggerConversationReference: () => void;
}

const chatPageRef = ref<HTMLElement | null>(null);
const composerRef = ref<ChatComposerHandle | null>(null);
const composerDraftEmpty = ref(true);
const pendingComposerFocus = ref(false);
const pendingConversationReference = ref(false);
const pendingSuggestionPrompt = ref<string | null>(null);
const shouldRenderTodoFloat = ref(false);
const composerActivated = ref(false);
const chatSidebar = useChatSidebar();
const sidebarHostActivated = ref(!props.isPopup && chatSidebar.state.open);
let todoFloatIdleHandle: number | null = null;
let cancelTodoFloatPaint: (() => void) | null = null;
let cancelComposerActivationPaint: (() => void) | null = null;
const todoFloatRenderEpoch = withComponentEpoch();
const composerActivationEpoch = withComponentEpoch();

function cancelTodoFloatRenderSchedule() {
  todoFloatRenderEpoch.invalidate();
  cancelTodoFloatPaint?.();
  cancelTodoFloatPaint = null;
  if (todoFloatIdleHandle !== null) {
    cancelIdleRun(todoFloatIdleHandle);
    todoFloatIdleHandle = null;
  }
}

function cancelComposerActivationSchedule() {
  composerActivationEpoch.invalidate();
  cancelComposerActivationPaint?.();
  cancelComposerActivationPaint = null;
}

function canRenderTodoFloat(epoch: number) {
  return todoFloatRenderEpoch.assertAlive(epoch) &&
    !shouldRenderTodoFloat.value &&
    !!props.taskId &&
    props.shouldRenderChat &&
    composerActivated.value;
}

function scheduleTodoFloatRender() {
  if (
    shouldRenderTodoFloat.value ||
    !props.taskId ||
    !props.shouldRenderChat ||
    !composerActivated.value
  ) return;
  cancelTodoFloatRenderSchedule();
  const epoch = todoFloatRenderEpoch.nextEpoch();
  cancelTodoFloatPaint = scheduleAfterPaint(() => {
    cancelTodoFloatPaint = null;
    if (!canRenderTodoFloat(epoch)) return;
    todoFloatIdleHandle = runWhenIdle(() => {
      todoFloatIdleHandle = null;
      if (!canRenderTodoFloat(epoch)) return;
      cancelTodoFloatPaint = scheduleAfterPaint(() => {
        cancelTodoFloatPaint = null;
        if (!canRenderTodoFloat(epoch)) return;
        todoFloatIdleHandle = runWhenIdle(() => {
          todoFloatIdleHandle = null;
          if (!canRenderTodoFloat(epoch)) return;
          shouldRenderTodoFloat.value = true;
        });
      });
    });
  });
}

function shouldPrioritizeComposerActivation() {
  return props.timelineEvents.length === 0 &&
    !props.isTurnRunning &&
    props.pendingAgentActions.length === 0;
}

function scheduleComposerActivation() {
  if (composerActivated.value || !props.shouldRenderChat) return;
  cancelComposerActivationSchedule();
  const epoch = composerActivationEpoch.nextEpoch();
  const activate = () => {
    if (
      !composerActivationEpoch.assertAlive(epoch) ||
      composerActivated.value ||
      !props.shouldRenderChat
    ) return;
    measurePerfSync("task-detail.composer.activate", () => {
      composerActivated.value = true;
    }, { detail: props.taskId });
  };
  const cancelPaint = scheduleAfterPaint(() => {
    if (cancelComposerActivationPaint === cancelPaint) {
      cancelComposerActivationPaint = null;
    }
    if (
      !composerActivationEpoch.assertAlive(epoch) ||
      composerActivated.value ||
      !props.shouldRenderChat
    ) return;
    if (shouldPrioritizeComposerActivation()) {
      activate();
      return;
    }
    const cancelSecondPaint = scheduleAfterPaint(() => {
      if (cancelComposerActivationPaint === cancelSecondPaint) {
        cancelComposerActivationPaint = null;
      }
      activate();
    });
    cancelComposerActivationPaint = cancelSecondPaint;
  });
  cancelComposerActivationPaint = cancelPaint;
}

onMounted(() => {
  scheduleComposerActivation();
});

onBeforeUnmount(() => {
  cancelComposerActivationSchedule();
  cancelTodoFloatRenderSchedule();
});

watch(
  () => [props.isPopup, chatSidebar.state.open] as const,
  ([isPopup, open]) => {
    if (isPopup || !open || sidebarHostActivated.value) return;
    sidebarHostActivated.value = true;
  },
  { immediate: true },
);

watch(
  () => props.shouldRenderChat,
  (shouldRenderChat) => {
    if (!shouldRenderChat) {
      cancelComposerActivationSchedule();
      cancelTodoFloatRenderSchedule();
      return;
    }
    scheduleComposerActivation();
  },
  { immediate: true },
);

watch(
  () => composerActivated.value,
  (active) => {
    if (!active) return;
    scheduleTodoFloatRender();
  },
  { immediate: true },
);

watch(composerRef, (composer) => {
  if (!composer) return;
  const suggestionPrompt = pendingSuggestionPrompt.value;
  if (suggestionPrompt) {
    pendingSuggestionPrompt.value = null;
    composer.fillSuggestionPrompt(suggestionPrompt);
  }
  if (pendingConversationReference.value) {
    pendingConversationReference.value = false;
    composer.triggerConversationReference();
  }
  if (pendingComposerFocus.value) {
    pendingComposerFocus.value = false;
    composer.focusInput();
  }
});

function focusComposer() {
  if (composerRef.value) {
    composerRef.value.focusInput();
    return;
  }
  pendingComposerFocus.value = true;
}

function triggerConversationReference() {
  if (composerRef.value) {
    composerRef.value.triggerConversationReference();
    return;
  }
  pendingConversationReference.value = true;
}

function getComposerDraftSnapshot() {
  return composerRef.value?.getDraftSnapshot() ?? { content: "" };
}

defineExpose({ chatPageRef, focusComposer, getComposerDraftSnapshot, triggerConversationReference });

function emitSend(
  content: string,
  outgoingAttachments: ChatAttachment[],
  conversationReferences: ChatConversationReference[],
  workflow?: ChatWorkflow | null,
) {
  emit("send", content, outgoingAttachments, conversationReferences, workflow);
}

function selectSuggestion(suggestion: SuggestionItem) {
  if (composerRef.value) {
    composerRef.value.fillSuggestionPrompt(suggestion.prompt);
    return;
  }
  pendingSuggestionPrompt.value = suggestion.prompt;
}
</script>

<template>
  <section
    v-if="shouldRenderChat"
    ref="chatPageRef"
    class="chat-page"
    :class="{ 'chat-page--popup': isPopup }"
  >
    <div class="chat">
      <div class="chat-layout">
        <div class="chat-layout__main">
          <div
            v-if="fileDropActive"
            class="chat-file-drop-overlay"
            aria-live="polite"
            role="status"
          >
            <Plus class="chat-file-drop-overlay__icon" aria-hidden="true" />
            <span class="chat-file-drop-overlay__text">拖入文件以追加到输入框</span>
          </div>
          <ChatTranscript
            :timeline-events="timelineEvents"
            :empty-headline="emptyHeadline"
            :project-id="projectId"
            :is-thinking="isTurnRunning"
            :project-cwd="projectCwd"
            :active-plan-approval-turn-id="activePlanApprovalTurnId"
            :force-scroll-bottom-key="userSendScrollKey"
            :pending-agent-actions="pendingAgentActions"
            :show-expired-pending-actions="showExpiredPendingActions"
            :can-retry-event="canRetryEvent"
            :can-start-lilia-batch-apply="!isTurnRunning && !hasBlockingPendingAction && !taskRunBlockingReason"
            :can-start-session-fork="!isTurnRunning && !hasBlockingPendingAction && !taskRunBlockingReason"
            @resolve-pending-agent-action="emit('resolve-pending-agent-action', $event)"
            @retry-event="emit('retry-event', $event)"
            @open-image="emit('open-image', $event)"
            @insert-draft-text="emit('insert-draft-text', $event)"
            @start-lilia-batch-apply="emit('start-lilia-batch-apply', $event)"
            @start-session-fork="emit('start-session-fork', $event)"
          >
            <template #empty-actions>
              <ChatSuggestions
                :suggestions="suggestions"
                :suggestions-status="suggestionsStatus"
                :suggestions-loading-text="suggestionsLoadingText"
                :suggestions-visible="suggestionsVisible && composerDraftEmpty"
                @select="selectSuggestion"
                @refresh-suggestions="emit('refresh-suggestions')"
              />
            </template>
          </ChatTranscript>
          <div class="chat-controls-wrap">
            <div class="chat-controls">
              <TodoFloat
                v-if="taskId && shouldRenderTodoFloat"
                :task-id="taskId"
                :show-goal="true"
                :goal="currentLiliaGoal"
                :goal-disabled="isTurnRunning || pendingAgentActions.some((action) => action.kind !== TITLE_UPDATE_ACTION_KIND)"
                @insert-guide="emit('insert-guide', $event)"
                @set-lilia-goal="emit('set-lilia-goal', $event)"
                @refresh-lilia-goal="emit('refresh-lilia-goal')"
                @clear-lilia-goal="emit('clear-lilia-goal')"
              />
              <ChatComposer
                v-if="composerActivated"
                ref="composerRef"
                :state="composerState"
                :worktree-value="worktreeValue"
                :worktree-options="worktreeOptions"
                :worktree-busy="worktreeBusy"
                :worktree-error="worktreeError"
                :model-options="modelOptions"
                :attachments="attachments"
                :append-attachments-to-end-key="appendAttachmentsToEndKey"
                :project-cwd="contextSearchCwd"
                :sending="isTurnRunning"
                :compact-disabled="hasBlockingPendingAction || !!taskRunBlockingReason"
                :context-usage="contextUsage"
                :pending-ask="pendingAsk"
                :tool-consent="toolConsent"
                :restore-draft-key="restoreDraftKey"
                :restore-draft-content="restoreDraftContent"
                :restore-draft-conversation-references="restoreDraftConversationReferences"
                :insert-draft-text-key="insertDraftTextKey"
                :insert-draft-text-content="insertDraftTextContent"
                :pending-branch-anchor="pendingBranchAnchor"
                @send="emitSend"
                @start-lilia-review="(content, outgoingAttachments, conversationReferences, target) => emit('start-lilia-review', content, outgoingAttachments, conversationReferences, target)"
                @start-lilia-fix-suggestion="(content, outgoingAttachments, conversationReferences, target) => emit('start-lilia-fix-suggestion', content, outgoingAttachments, conversationReferences, target)"
                @start-lilia-compact="emit('start-lilia-compact')"
                @execute-slash-command="emit('execute-slash-command', $event)"
                @interrupt="emit('interrupt')"
                @update:state="emit('update-composer', $event)"
                @select-worktree="emit('select-worktree', $event)"
                @remove-attachment="emit('remove-attachment', $event)"
                @pick-attachments="emit('pick-attachments')"
                @add-context-attachment="emit('add-context-attachment', $event)"
                @resolve-ask-user="emit('resolve-ask-user', $event)"
                @resolve-tool-consent="(decision, message, updatedInput) => emit('resolve-tool-consent', decision, message, updatedInput)"
                @open-image="emit('open-image', $event)"
                @draft-empty-change="composerDraftEmpty = $event"
                @draft-change="emit('composer-draft-change', $event)"
                @clear-branch-anchor="emit('clear-branch-anchor')"
              />
              <div
                v-else
                class="chat-composer chat-composer--loading"
                aria-hidden="true"
              >
                <div class="chat-composer__placeholder">
                  <div class="chat-composer__placeholder-line chat-composer__placeholder-line--primary" />
                  <div class="chat-composer__placeholder-line" />
                  <div class="chat-composer__placeholder-line chat-composer__placeholder-line--short" />
                </div>
              </div>
              <ComposerProjectPicker
                v-if="showDraftProjectPicker"
                :projects="draftProjectPickerProjects ?? []"
                @select-project="emit('select-draft-project', $event)"
                @created-project="emit('created-draft-project', $event)"
                @error="emit('draft-project-picker-error', $event)"
              />
            </div>
          </div>
        </div>
        <ChatSidebarHost
          v-if="!isPopup && sidebarHostActivated"
          :task-id="taskId"
          :project-id="projectId"
          :project-cwd="projectCwd"
        />
        <aside
          v-else-if="!isPopup"
          class="chat-sidebar"
          aria-label="对话侧栏"
          aria-hidden="true"
          inert
        >
          <div class="chat-sidebar__inner">
            <section class="chat-sidebar__body">
              <div class="chat-sidebar__empty">暂无内容</div>
            </section>
          </div>
        </aside>
      </div>
    </div>
    <ImageViewer
      v-if="viewingImage"
      :image="viewingImage"
      @close="emit('close-image')"
    />
  </section>

  <section
    v-else-if="isPopupPending"
    class="chat-page chat-page--popup chat-page--pending"
    aria-busy="true"
  >
    <div class="chat">
      <div class="chat-layout">
        <div class="chat-layout__main">
          <div
            v-if="shouldShowContextLoading"
            class="popup-context-loading"
            role="status"
          >
            正在加载对话…
          </div>
        </div>
      </div>
    </div>
  </section>

  <section v-else-if="isContextLoading">
    <div class="empty-state">正在加载对话…</div>
  </section>

  <section v-else>
    <div class="empty-state">未找到任务 <code>{{ taskId }}</code></div>
  </section>
</template>

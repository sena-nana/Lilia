<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, ref, watch, type Component } from "vue";
import ArrowUp from "@lucide/vue/dist/esm/icons/arrow-up.mjs";
import CornerDownRight from "@lucide/vue/dist/esm/icons/corner-down-right.mjs";
import GitFork from "@lucide/vue/dist/esm/icons/git-fork.mjs";
import ListChecks from "@lucide/vue/dist/esm/icons/list-checks.mjs";
import Paperclip from "@lucide/vue/dist/esm/icons/paperclip.mjs";
import Plus from "@lucide/vue/dist/esm/icons/plus.mjs";
import MessageSquareQuote from "@lucide/vue/dist/esm/icons/message-square-quote.mjs";
import Goal from "@lucide/vue/dist/esm/icons/goal.mjs";
import GitBranch from "@lucide/vue/dist/esm/icons/git-branch.mjs";
import ShieldCheck from "@lucide/vue/dist/esm/icons/shield-check.mjs";
import Square from "@lucide/vue/dist/esm/icons/square.mjs";
import WandSparkles from "@lucide/vue/dist/esm/icons/wand-sparkles.mjs";
import X from "@lucide/vue/dist/esm/icons/x.mjs";
import { clampPercent } from "@lilia/contracts";
import type {
  ChatAttachment,
  ChatBranchAnchor,
  ChatComposerState,
  ChatContextUsage,
  ChatModelOption,
  ModelSelectionExplanation,
  PermissionMode,
} from "@lilia/contracts";
import { attachmentImageSrc } from "./imageViewer";
import Dropdown from "@lilia/ui/components/Dropdown";
import { addDomEventListener, runUnlistenFns } from "@lilia/ui/utils/eventListeners";
import { createLazyLoadState } from "@lilia/ui/utils/lazyLoadState";
import { measurePerfAsync } from "@lilia/ui/diagnostics";
import { SB_MENU_POP_TRANSITION_MS } from "@lilia/ui/composables/menuMotion";
import { useAnchoredMenuMotion } from "@lilia/ui/composables/useAnchoredMenuMotion";

const composerModelPickerLoad = createLazyLoadState<Component>(() =>
  measurePerfAsync(
    "chat-composer.model-picker.load",
    async () => (await import("./ComposerModelPicker.vue")).default,
  )
);

const ComposerModelPicker = defineAsyncComponent({
  suspensible: false,
  loader: () => composerModelPickerLoad.load(),
});

const props = defineProps<{
  state: ChatComposerState;
  worktreeValue: string;
  worktreeOptions: Array<{ value: string; label: string; hint?: string }>;
  worktreeBusy: boolean;
  worktreeError?: string | null;
  modelOptions: ChatModelOption[];
  autoModelPreview: ModelSelectionExplanation;
  permissionOptions: Array<{ value: PermissionMode; label: string; hint: string }>;
  previewAttachments: ChatAttachment[];
  canInterrupt: boolean;
  canSubmitEntry: boolean;
  actionsBlocked: boolean;
  compactDisabled: boolean;
  canOptimizePrompt: boolean;
  promptOptimizing: boolean;
  promptOptimizeError?: string | null;
  contextUsage?: ChatContextUsage | null;
  pendingBranchAnchor?: ChatBranchAnchor | null;
  sendTitle: string;
  sendAriaLabel: string;
}>();

const emit = defineEmits<{
  pickAttachments: [];
  referenceConversation: [];
  setPermission: [permission: PermissionMode];
  selectWorktree: [value: string];
  updateComposer: [patch: Partial<ChatComposerState>];
  togglePlanMode: [];
  toggleGoalMode: [];
  startLiliaCompact: [];
  optimizePrompt: [];
  clearBranchAnchor: [];
  submitEntry: [];
  openImage: [attachment: ChatAttachment];
}>();

const numberFormatter = new Intl.NumberFormat("zh-CN");
const actionMenuOpen = ref(false);
let actionMenuDocumentUnlisteners: Array<() => void> = [];
const actionMenuPlacement = ref<"top" | "bottom">("top");
const {
  triggerEl: actionTriggerEl,
  menuEl: actionMenuEl,
  overlayStyle: actionMenuStyle,
  resolvedPlacement: resolvedActionMenuPlacement,
  containsTarget: actionMenuContainsTarget,
  clearAnchor: clearActionMenuAnchor,
  captureAnchor: captureActionMenuAnchor,
  updateOrigin: updateActionMenuOrigin,
} = useAnchoredMenuMotion(actionMenuOpen, actionMenuPlacement);
const actionMenuPlacementClass = computed(() =>
  resolvedActionMenuPlacement.value.startsWith("bottom") ? "dd__menu--bottom" : "dd__menu--top",
);

type ModeChip = {
  key: "plan" | "goal" | "branch";
  label: string;
  title: string;
  icon: Component;
  toggle: () => void;
};

function supportsBuiltinAgentActions(backend: ChatComposerState["backend"]) {
  return backend === "native-agentkit" || backend === "codex" || backend === "claude";
}

function contextUsageTone(usage: ChatContextUsage | null | undefined): string {
  const percent = usage?.usedPercent;
  if (typeof percent !== "number" || !Number.isFinite(percent)) return "chat-context-ring--empty";
  if (percent >= 100) return "chat-context-ring--error";
  if (percent >= 85) return "chat-context-ring--warn";
  return "chat-context-ring--normal";
}

function formatTokens(value: number | null | undefined): string {
  if (typeof value !== "number" || !Number.isFinite(value)) return "--";
  return numberFormatter.format(Math.max(0, Math.trunc(value)));
}

function formatContextUsageTime(value: number | null | undefined): string {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) return "未知时间";
  return new Date(value).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

const contextUsageProgressStyle = computed<Record<string, string>>(() => ({
  "--quota-progress": String(clampPercent(props.contextUsage?.usedPercent ?? 0)),
}));

function formatContextUsagePercent(usage: ChatContextUsage): string {
  return typeof usage.usedPercent === "number" && Number.isFinite(usage.usedPercent)
    ? `${clampPercent(usage.usedPercent).toFixed(1)}%`
    : "未知";
}

const contextUsageTitle = computed(() => {
  const usage = props.contextUsage;
  if (!usage) return "暂无上下文占用数据，点击压缩上下文";
  const parts = [
    `压缩上下文`,
    `已用 ${formatTokens(usage.usedTokens)} tokens`,
    usage.limitTokens ? `上限 ${formatTokens(usage.limitTokens)} tokens` : "上限未知",
    typeof usage.usedPercent === "number" && Number.isFinite(usage.usedPercent)
      ? `占用 ${clampPercent(usage.usedPercent).toFixed(1)}%`
      : "占用比例未知",
    `来源 ${usage.source || "runtime"}`,
    `更新 ${formatContextUsageTime(usage.updatedAt)}`,
  ];
  if (usage.unavailableReason) parts.push(usage.unavailableReason);
  return parts.join(" · ");
});

const contextUsageRows = computed(() => {
  const usage = props.contextUsage;
  if (!usage) {
    return [
      { label: "状态", value: "暂无上下文占用数据" },
      { label: "操作", value: "点击压缩上下文" },
    ];
  }
  const rows = [
    { label: "已用", value: `${formatTokens(usage.usedTokens)} tokens` },
    {
      label: "上限",
      value: usage.limitTokens ? `${formatTokens(usage.limitTokens)} tokens` : "未知",
    },
    { label: "占用", value: formatContextUsagePercent(usage) },
    { label: "来源", value: usage.source || "runtime" },
    { label: "更新", value: formatContextUsageTime(usage.updatedAt) },
  ];
  if (usage.unavailableReason) rows.push({ label: "说明", value: usage.unavailableReason });
  return rows;
});

const activeModeChips = computed<ModeChip[]>(() => {
  const chips: ModeChip[] = [];
  const branchAnchor = props.pendingBranchAnchor;
  if (branchAnchor) {
    const isFork = branchAnchor.mode === "fork";
    chips.push({
      key: "branch",
      label: isFork ? "分叉锚点" : "继续锚点",
      title: `${isFork ? "清除分叉锚点" : "清除继续锚点"}：${branchAnchor.sourceTurnId}`,
      icon: isFork ? GitFork : CornerDownRight,
      toggle: () => emit("clearBranchAnchor"),
    });
  }
  if (props.state.planMode) {
    chips.push({
      key: "plan",
      label: "计划",
      title: "关闭计划模式",
      icon: ListChecks,
      toggle: () => emit("togglePlanMode"),
    });
  }
  if (props.state.goalMode) {
    chips.push({
      key: "goal",
      label: "目标",
      title: "关闭目标模式",
      icon: Goal,
      toggle: () => emit("toggleGoalMode"),
    });
  }
  return chips;
});

const worktreeTitle = computed(() =>
  props.worktreeError || props.worktreeOptions.find((option) => option.value === props.worktreeValue)?.hint || "工作树",
);
const permissionTitle = computed(() => {
  const option = props.permissionOptions.find((item) => item.value === props.state.permission);
  return option?.hint || option?.label || "权限模式";
});

function toggleActionMenu(event: MouseEvent) {
  captureActionMenuAnchor(event);
  actionMenuOpen.value = !actionMenuOpen.value;
}

function closeActionMenu() {
  actionMenuOpen.value = false;
}

function pickAction(action: "attachments" | "reference" | "plan" | "goal") {
  closeActionMenu();
  if (action === "attachments") emit("pickAttachments");
  else if (action === "reference") emit("referenceConversation");
  else if (action === "plan") emit("togglePlanMode");
  else emit("toggleGoalMode");
}

function onDocPointer(e: PointerEvent) {
  if (!actionMenuContainsTarget(e.target)) closeActionMenu();
}

function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape" || !actionMenuOpen.value) return;
  closeActionMenu();
  e.stopPropagation();
}

function clearActionMenuDocumentListeners() {
  runUnlistenFns(actionMenuDocumentUnlisteners.splice(0).reverse());
}

function installActionMenuDocumentListeners() {
  clearActionMenuDocumentListeners();
  actionMenuDocumentUnlisteners = [
    addDomEventListener(document, "pointerdown", onDocPointer, true),
    addDomEventListener(document, "keydown", onKey),
  ];
}

watch(actionMenuOpen, (open) => {
  clearActionMenuDocumentListeners();
  if (open) {
    void updateActionMenuOrigin();
    installActionMenuDocumentListeners();
  } else {
    clearActionMenuAnchor();
  }
});

onBeforeUnmount(() => {
  actionTriggerEl.value = null;
  actionMenuEl.value = null;
  clearActionMenuDocumentListeners();
});

</script>

<template>
  <div class="chat-composer__toolbar" data-agent-id="chat.composer.toolbar">
    <div
      v-if="previewAttachments.length"
      class="chat-composer__attachments"
      aria-label="图片预览"
    >
      <button
        v-for="attachment in previewAttachments"
        :key="attachment.id"
        type="button"
        class="chat-attachment-chip chat-attachment-chip--image-preview"
        :data-agent-id="`chat.composer.attachment.preview.${attachment.id}`"
        :title="attachment.path"
        :aria-label="`查看图片 ${attachment.name}`"
        @click="emit('openImage', attachment)"
      >
        <img
          class="chat-attachment-chip__thumb"
          :src="attachmentImageSrc(attachment) ?? undefined"
          alt=""
        />
      </button>
    </div>

    <div class="chat-composer__row">
      <div class="chat-composer__group">
        <div class="chat-composer__action-menu">
          <button
            ref="actionTriggerEl"
            type="button"
            class="chat-composer__action-trigger"
            data-agent-id="chat.composer.actions.open"
            :class="{ 'is-open': actionMenuOpen }"
            title="更多输入操作"
            aria-label="更多输入操作"
            aria-haspopup="menu"
            :aria-expanded="actionMenuOpen"
            @click="toggleActionMenu"
          >
            <Plus :size="16" aria-hidden="true" />
          </button>
          <Teleport to="body">
            <Transition name="sb-menu-pop" :duration="SB_MENU_POP_TRANSITION_MS">
              <div
                v-if="actionMenuOpen"
                ref="actionMenuEl"
                class="dd__menu chat-composer__action-menu-popover"
                :class="actionMenuPlacementClass"
                role="menu"
                :style="actionMenuStyle"
              >
                <button
                  type="button"
                  class="dd__item chat-composer__action-menu-item"
                  data-agent-id="chat.composer.actions.attachments"
                  role="menuitem"
                  @click="pickAction('attachments')"
                >
                  <span class="chat-composer__action-menu-icon">
                    <Paperclip :size="14" aria-hidden="true" />
                  </span>
                  <span class="dd__item-label">添加附件</span>
                </button>
                <button
                  type="button"
                  class="dd__item chat-composer__action-menu-item"
                  data-agent-id="chat.composer.actions.reference-conversation"
                  role="menuitem"
                  @click="pickAction('reference')"
                >
                  <span class="chat-composer__action-menu-icon">
                    <MessageSquareQuote :size="14" aria-hidden="true" />
                  </span>
                  <span class="dd__item-label">引用其他对话</span>
                </button>
                <button
                  type="button"
                  class="dd__item chat-composer__action-menu-item chat-composer__action-menu-toggle"
                  data-agent-id="chat.composer.actions.plan-mode"
                  :class="{ 'is-active': state.planMode }"
                  role="menuitemcheckbox"
                  :aria-checked="state.planMode"
                  @click="pickAction('plan')"
                >
                  <span class="chat-composer__action-menu-icon">
                    <ListChecks :size="14" aria-hidden="true" />
                  </span>
                  <span class="dd__item-label">计划模式</span>
                  <span class="chat-composer__action-switch" aria-hidden="true">
                    <span class="chat-composer__action-switch-thumb" />
                  </span>
                </button>
                <button
                  type="button"
                  class="dd__item chat-composer__action-menu-item chat-composer__action-menu-toggle"
                  data-agent-id="chat.composer.actions.goal-mode"
                  :class="{ 'is-active': state.goalMode }"
                  role="menuitemcheckbox"
                  :aria-checked="state.goalMode"
                  @click="pickAction('goal')"
                >
                  <span class="chat-composer__action-menu-icon">
                    <Goal :size="14" aria-hidden="true" />
                  </span>
                  <span class="dd__item-label">目标模式</span>
                  <span class="chat-composer__action-switch" aria-hidden="true">
                    <span class="chat-composer__action-switch-thumb" />
                  </span>
                </button>
              </div>
            </Transition>
          </Teleport>
        </div>
        <Dropdown
          class="chat-composer__permission-dropdown"
          agent-id="chat.composer.permission"
          :class="{ 'is-full-access': state.permission === 'full' }"
          :model-value="state.permission"
          :options="permissionOptions"
          :icon="ShieldCheck"
          :title="permissionTitle"
          @update:model-value="emit('setPermission', $event)"
        />
        <Dropdown
          class="chat-composer__worktree-dropdown"
          agent-id="chat.composer.worktree"
          :class="{ 'is-error': worktreeError }"
          :model-value="worktreeValue"
          :options="worktreeOptions"
          :icon="GitBranch"
          :disabled="worktreeBusy"
          :placeholder="worktreeBusy ? '工作树...' : '当前环境'"
          :title="worktreeTitle"
          @update:model-value="emit('selectWorktree', $event)"
        />
        <ComposerModelPicker
          data-agent-id="chat.composer.model"
          :state="state"
          :model-options="modelOptions"
          :auto-model-preview="autoModelPreview"
          @update="emit('updateComposer', $event)"
        />
        <button
          v-for="chip in activeModeChips"
          :key="chip.key"
          type="button"
          class="chat-composer__mode-chip"
          :data-agent-id="`chat.composer.mode.${chip.key}`"
          :title="chip.title"
          :aria-label="chip.title"
          @click="chip.toggle"
        >
          <span class="chat-composer__mode-chip-icon-slot" aria-hidden="true">
            <component
              :is="chip.icon"
              :size="12"
              class="chat-composer__mode-chip-icon"
            />
            <X :size="11" class="chat-composer__mode-chip-delete" />
          </span>
          <span class="chat-composer__mode-chip-label">{{ chip.label }}</span>
        </button>
      </div>

      <div class="chat-composer__entry-actions">
        <span
          v-if="promptOptimizeError"
          class="chat-composer__optimize-status"
          role="status"
        >
          {{ promptOptimizeError }}
        </span>
        <div
          v-if="supportsBuiltinAgentActions(state.backend)"
          class="chat-composer__context-wrap"
        >
          <button
            type="button"
            class="chat-composer__context-action"
            data-agent-id="chat.composer.context.compact"
            :class="[contextUsageTone(contextUsage), { 'is-disabled': compactDisabled }]"
            :style="contextUsageProgressStyle"
            :disabled="compactDisabled || actionsBlocked"
            :aria-label="contextUsageTitle"
            @click="emit('startLiliaCompact')"
          >
            <svg viewBox="0 0 16 16" focusable="false" aria-hidden="true">
              <circle class="chat-context-ring__track" cx="8" cy="8" r="6" pathLength="100" />
              <circle class="chat-context-ring__value" cx="8" cy="8" r="6" pathLength="100" />
            </svg>
          </button>
          <div
            class="chat-composer__context-card"
            role="tooltip"
          >
            <dl class="chat-composer__context-card-list">
              <template v-for="row in contextUsageRows" :key="row.label">
                <dt>{{ row.label }}</dt>
                <dd>{{ row.value }}</dd>
              </template>
            </dl>
          </div>
        </div>
        <button
          type="button"
          class="chat-composer__optimize"
          data-agent-id="chat.composer.prompt.optimize"
          :disabled="!canOptimizePrompt"
          :title="promptOptimizing ? '正在优化提示词' : '优化提示词'"
          :aria-label="promptOptimizing ? '正在优化提示词' : '优化提示词'"
          @click="emit('optimizePrompt')"
        >
          <WandSparkles :size="15" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="chat-composer__send"
          data-agent-id="chat.composer.send"
          :class="{ 'chat-composer__send--interrupt': canInterrupt }"
          :disabled="actionsBlocked || !canSubmitEntry"
          :title="sendTitle"
          :aria-label="sendAriaLabel"
          @click="emit('submitEntry')"
        >
          <component :is="canInterrupt ? Square : ArrowUp" :size="16" aria-hidden="true" />
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted, ref, type Component, watch } from "vue";
import AlertTriangle from "@lucide/vue/dist/esm/icons/triangle-alert.mjs";
import Sparkles from "@lucide/vue/dist/esm/icons/sparkles.mjs";
import type {
  AgentInteractionSettings,
  MainAgentPromptMode,
  PermissionMode,
} from "@lilia/contracts";
import {
  AUTO_TURN_DECISION_PERMISSION_OPTIONS,
  LOCKED_PERMISSION_MODES,
  PERMISSION_MODE_DISPLAY,
  PERMISSION_MODE_DISPLAY_ORDER,
  PROMPT_MAIN_AGENT,
} from "@lilia/contracts";
import {
  useAgentInteractionSettings,
} from "../../composables/useAgentInteractionSettings";
import SettingsCollapsibleCard from "@lilia/ui/components/SettingsCollapsibleCard";
import { beginPerfStage, cancelIdleRun, measurePerfAsync, runWhenIdle, scheduleAfterPaint } from "@lilia/ui/diagnostics";
import { createLazyLoadState } from "@lilia/ui/utils/lazyLoadState";

const subagentCatalogSectionLoad = createLazyLoadState<Component>(() =>
  measurePerfAsync(
    "settings.agent.subagents.load",
    async () => (await import("./SubagentCatalogSection.vue")).default as Component,
  )
);

const SubagentCatalogSection = defineAsyncComponent({
  suspensible: false,
  loader: () => subagentCatalogSectionLoad.load(),
});

const agentInteractionStore = useAgentInteractionSettings();
const agentInteraction = agentInteractionStore.settings;
const savingAgentInteraction = ref(false);
const agentInteractionError = ref<string | null>(null);
const mainAgentCustomPromptDraft = ref("");
const mainAgentStrategyExpanded = ref(false);
const mainAgentPromptExpanded = ref(false);
const autoTurnCardExpanded = ref(false);
const subagentCardExpanded = ref(false);
const subagentCatalogReady = ref(false);
const mainAgentStrategyDetailsId = "agent-main-strategy-details";
const mainAgentPromptDetailsId = "agent-main-prompt-details";
const autoTurnDetailsId = "agent-auto-turn-details";
const subagentDetailsId = "agent-subagent-mode-details";
let subagentCatalogIdleHandle: number | null = null;
let cancelSubagentCatalogPaint: (() => void) | null = null;
let subagentCatalogMountSeq = 0;
let disposed = false;

const lockedPermissionModes = new Set<PermissionMode>(LOCKED_PERMISSION_MODES);
const askReadonlyPermissionModes: readonly [PermissionMode, PermissionMode] = ["ask", "readonly"];
type PermissionAvailabilityOption = {
  id: string;
  mode: PermissionMode | null;
  currentModes: readonly PermissionMode[];
  label: string;
  description: string;
  locked: boolean;
};
const permissionAvailabilityOptions: PermissionAvailabilityOption[] = [
  {
    id: "ask-readonly",
    mode: null,
    currentModes: askReadonlyPermissionModes,
    label: "默认权限",
    description: "允许读取与分析，涉及变更时等待确认。",
    locked: askReadonlyPermissionModes.every((mode) => lockedPermissionModes.has(mode)),
  },
  ...PERMISSION_MODE_DISPLAY_ORDER
    .filter((value) => value !== "ask" && value !== "readonly")
    .map((value) => ({
      id: value,
      mode: value,
      currentModes: [value],
      label: PERMISSION_MODE_DISPLAY[value].label,
      description: PERMISSION_MODE_DISPLAY[value].description,
      locked: lockedPermissionModes.has(value),
    })),
];



const autoTurnDecisionPermissionOptions = AUTO_TURN_DECISION_PERMISSION_OPTIONS;
const mainAgentWorkflowsPrompt = PROMPT_MAIN_AGENT.workflowOrder
  .map((key) => PROMPT_MAIN_AGENT.workflowTypes[key])
  .filter((workflow): workflow is NonNullable<typeof workflow> => Boolean(workflow))
  .map((workflow) => `## ${workflow.title.trim()}\n${workflow.summary.trim()}\n\n${workflow.prompt.trim()}`)
  .filter((part) => part.trim())
  .join("\n\n");
const mainAgentPromptModeOptions: Array<{
  value: MainAgentPromptMode;
  label: string;
  description: string;
}> = [
  {
    value: "conservative",
    label: "保守",
    description: "小步低风险，优先沿用现有结构。",
  },
  {
    value: "aggressive",
    label: "激进",
    description: "允许根因级检查和必要功能重构。",
  },
  {
    value: "custom",
    label: "自定义",
    description: "使用自己填写的策略片段，保留内置工具与工作流提示。",
  },
];

function mainAgentStrategyPrompt(mode: MainAgentPromptMode, customPrompt: string): string {
  if (mode === "custom") {
    const trimmed = customPrompt.trim();
    if (trimmed) return trimmed;
    return PROMPT_MAIN_AGENT.modes.conservative.trim();
  }
  return PROMPT_MAIN_AGENT.modes[mode].trim();
}

function buildMainAgentPromptPreview(mode: MainAgentPromptMode, customPrompt: string): string {
  return [
    PROMPT_MAIN_AGENT.basePrompt.trim(),
    mainAgentStrategyPrompt(mode, customPrompt),
    PROMPT_MAIN_AGENT.toolsPrompt.trim(),
    mainAgentWorkflowsPrompt,
  ]
    .filter((part) => part.trim())
    .join("\n\n");
}

function activeMainAgentPromptDescription(): string {
  return mainAgentPromptModeOptions.find((option) => option.value === agentInteraction.value.mainAgentPromptMode)
    ?.description ?? mainAgentPromptModeOptions[0].description;
}

function activeMainAgentPromptLabel(): string {
  return mainAgentPromptModeOptions.find((option) => option.value === agentInteraction.value.mainAgentPromptMode)
    ?.label ?? mainAgentPromptModeOptions[0].label;
}

function defaultCustomMainAgentPrompt(): string {
  const persisted = agentInteraction.value.mainAgentCustomPrompt.trim();
  if (persisted) return persisted;
  const currentMode = agentInteraction.value.mainAgentPromptMode;
  if (currentMode === "custom") return PROMPT_MAIN_AGENT.modes.conservative.trim();
  return mainAgentStrategyPrompt(currentMode, "");
}

const mainAgentPromptPreview = computed(() =>
  buildMainAgentPromptPreview(
    agentInteraction.value.mainAgentPromptMode,
    agentInteraction.value.mainAgentPromptMode === "custom"
      ? mainAgentCustomPromptDraft.value
      : agentInteraction.value.mainAgentCustomPrompt,
  )
);

async function loadAgentInteraction() {
  try {
    await agentInteractionStore.load();
  } catch (err) {
    if (disposed) return;
    agentInteractionError.value = `读取 Agent 交互设置失败：${String(err)}`;
  }
}

async function setPermissionModeAvailable(permissionMode: PermissionMode, enabled: boolean) {
  if (lockedPermissionModes.has(permissionMode)) return;
  await setAgentInteraction({
    permissionModeAvailability: {
      ...agentInteraction.value.permissionModeAvailability,
      [permissionMode]: enabled,
    },
  });
}

function setPermissionAvailabilityOptionAvailable(option: PermissionAvailabilityOption, enabled: boolean) {
  if (option.mode === null) return;
  return setPermissionModeAvailable(option.mode, enabled);
}

function isPermissionAvailabilityOptionCurrent(option: PermissionAvailabilityOption) {
  return option.currentModes.includes(agentInteraction.value.permissionMode);
}

function isPermissionAvailabilityOptionEnabled(option: PermissionAvailabilityOption) {
  return option.mode !== null && agentInteraction.value.permissionModeAvailability[option.mode];
}

async function setMainAgentPromptMode(mainAgentPromptMode: MainAgentPromptMode) {
  if (mainAgentPromptMode === "custom") {
    const mainAgentCustomPrompt = defaultCustomMainAgentPrompt();
    mainAgentCustomPromptDraft.value = mainAgentCustomPrompt;
    await setAgentInteraction({ mainAgentPromptMode, mainAgentCustomPrompt });
    return;
  }
  await setAgentInteraction({ mainAgentPromptMode });
}

async function saveMainAgentCustomPrompt() {
  await setAgentInteraction({
    mainAgentPromptMode: "custom",
    mainAgentCustomPrompt: mainAgentCustomPromptDraft.value,
  });
}

function nextSubagentMode(
  patch: Partial<AgentInteractionSettings["subagentMode"]>,
): AgentInteractionSettings["subagentMode"] {
  return {
    ...agentInteraction.value.subagentMode,
    ...patch,
  };
}

async function setSubagentModeEnabled(enabled: boolean) {
  await setAgentInteraction({
    subagentMode: nextSubagentMode({ enabled }),
  });
}

async function setSubagentModeField(
  key: "forwardSubagentText" | "agentProgressSummaries",
  value: boolean,
) {
  await setAgentInteraction({
    subagentMode: nextSubagentMode({ [key]: value }),
  });
}

function nextAutoTurnDecision(
  patch: Partial<AgentInteractionSettings["autoTurnDecision"]>,
): AgentInteractionSettings["autoTurnDecision"] {
  return {
    ...agentInteraction.value.autoTurnDecision,
    ...patch,
  };
}

async function setAutoTurnDecisionField(
  key: keyof AgentInteractionSettings["autoTurnDecision"],
  value: boolean,
) {
  await setAgentInteraction({
    autoTurnDecision: nextAutoTurnDecision({ [key]: value }),
  });
}

async function setAgentInteraction(patch: Partial<AgentInteractionSettings>) {
  if (disposed) return;
  savingAgentInteraction.value = true;
  agentInteractionError.value = null;
  try {
    await agentInteractionStore.update(patch);
  } catch (err) {
    if (!disposed) agentInteractionError.value = `保存 Agent 交互设置失败：${String(err)}`;
  } finally {
    if (!disposed) savingAgentInteraction.value = false;
  }
}

function scheduleSubagentCatalogMount() {
  if (subagentCatalogReady.value) return;
  cancelSubagentCatalogPaint?.();
  cancelSubagentCatalogPaint = null;
  const seq = ++subagentCatalogMountSeq;
  const stage = beginPerfStage("settings.agent.subagents.mount");
  cancelSubagentCatalogPaint = scheduleAfterPaint(() => {
    cancelSubagentCatalogPaint = null;
    if (seq !== subagentCatalogMountSeq || subagentCatalogReady.value) {
      stage.end("cancelled");
      return;
    }
    subagentCatalogIdleHandle = runWhenIdle(() => {
      subagentCatalogIdleHandle = null;
      if (seq !== subagentCatalogMountSeq || subagentCatalogReady.value) {
        stage.end("cancelled");
        return;
      }
      subagentCatalogReady.value = true;
      stage.end("idle");
    });
  });
}

watch(
  () => agentInteraction.value.subagentMode.enabled,
  (enabled) => {
    subagentCardExpanded.value = enabled;
  },
  { immediate: true },
);

watch(
  () => agentInteraction.value.mainAgentCustomPrompt,
  (prompt) => {
    mainAgentCustomPromptDraft.value = prompt;
  },
  { immediate: true },
);

onMounted(() => {
  disposed = false;
  void loadAgentInteraction();
  scheduleSubagentCatalogMount();
});

onBeforeUnmount(() => {
  disposed = true;
  subagentCatalogMountSeq += 1;
  cancelSubagentCatalogPaint?.();
  cancelSubagentCatalogPaint = null;
  if (subagentCatalogIdleHandle === null) return;
  cancelIdleRun(subagentCatalogIdleHandle);
  subagentCatalogIdleHandle = null;
});
</script>

<template>
  <div class="agent-settings-stack" data-agent-id="settings.agent">
    <section class="card agent-settings-card" aria-label="权限行为">
      <h2>
        <span class="card-h2__title">
          <Sparkles :size="14" aria-hidden="true" />
          权限行为
        </span>
      </h2>
      <div class="permission-toggle-list">
        <div
          v-for="option in permissionAvailabilityOptions"
          :key="option.id"
          class="permission-toggle-item"
          :data-agent-id="`settings.agent.permission-availability.${option.id}`"
        >
          <div class="permission-toggle-item__content">
            <div class="settings-row__label">
              {{ option.label }}
              <span
                v-if="isPermissionAvailabilityOptionCurrent(option)"
                class="permission-toggle-item__badge"
              >
                当前默认
              </span>
            </div>
            <p class="permission-toggle-item__hint">{{ option.description }}</p>
          </div>
          <button
            v-if="option.locked"
            type="button"
            class="ui-button ui-button--ghost permission-toggle-item__locked"
            :data-agent-id="`settings.agent.permission-availability.${option.id}.locked`"
            disabled
          >
            固定启用
          </button>
          <div v-else class="ui-segmented" role="radiogroup" :aria-label="`${option.label}权限可用性`">
            <button
              type="button"
              role="radio"
              :aria-checked="!isPermissionAvailabilityOptionEnabled(option)"
              :data-agent-id="`settings.agent.permission-availability.${option.id}.off`"
              :class="{ 'is-active': !isPermissionAvailabilityOptionEnabled(option) }"
              :disabled="savingAgentInteraction"
              @click="setPermissionAvailabilityOptionAvailable(option, false)"
            >
              关闭
            </button>
            <button
              type="button"
              role="radio"
              :aria-checked="isPermissionAvailabilityOptionEnabled(option)"
              :data-agent-id="`settings.agent.permission-availability.${option.id}.on`"
              :class="{ 'is-active': isPermissionAvailabilityOptionEnabled(option) }"
              :disabled="savingAgentInteraction"
              @click="setPermissionAvailabilityOptionAvailable(option, true)"
            >
              启用
            </button>
          </div>
        </div>
      </div>
    </section>

    <SettingsCollapsibleCard
      v-model:expanded="mainAgentStrategyExpanded"
      aria-label="策略模式"
      :controls-id="mainAgentStrategyDetailsId"
      toggle-agent-id="settings.agent.main-prompt-mode.toggle"
      expand-label="展开策略模式"
      collapse-label="收起策略模式"
    >
      <template #summary>
        <div>
          <div class="settings-row__label">策略模式</div>
          <p class="agent-collapsible-hint">
            {{ activeMainAgentPromptLabel() }}：{{ activeMainAgentPromptDescription() }}
          </p>
        </div>
      </template>
      <div class="settings-row settings-row--nested">
        <div class="settings-row__label">主 Agent 策略</div>
        <div class="settings-row__control settings-row__control--loose">
          <div class="ui-segmented" role="radiogroup" aria-label="主 Agent 策略">
            <button
              v-for="option in mainAgentPromptModeOptions"
              :key="option.value"
              type="button"
              role="radio"
              :aria-checked="agentInteraction.mainAgentPromptMode === option.value"
              :data-agent-id="`settings.agent.main-prompt-mode.${option.value}`"
              :class="{ 'is-active': agentInteraction.mainAgentPromptMode === option.value }"
              :disabled="savingAgentInteraction"
              @click="setMainAgentPromptMode(option.value)"
            >
              {{ option.label }}
            </button>
          </div>
          <span class="settings-row__status muted">
            {{ activeMainAgentPromptDescription() }}
          </span>
        </div>
      </div>
    </SettingsCollapsibleCard>

    <SettingsCollapsibleCard
      v-model:expanded="mainAgentPromptExpanded"
      aria-label="主 Agent 提示词"
      :controls-id="mainAgentPromptDetailsId"
      toggle-agent-id="settings.agent.main-prompt.toggle"
      expand-label="展开主 Agent 提示词"
      collapse-label="收起主 Agent 提示词"
    >
      <template #summary>
        <div>
          <div class="settings-row__label">提示词显示</div>
          <p class="agent-collapsible-hint">
            {{ agentInteraction.mainAgentPromptMode === "custom" ? "自定义策略" : "内置策略" }}
          </p>
        </div>
      </template>
      <section class="main-agent-prompt-panel" aria-label="主 Agent 工作流提示">
        <div class="main-agent-prompt-panel__head">
          <div class="settings-row__label">主 Agent 工作流提示预览</div>
          <span class="main-agent-prompt-panel__meta">
            {{ agentInteraction.mainAgentPromptMode === "custom" ? "自定义策略" : "内置策略" }}
          </span>
        </div>
        <pre class="main-agent-prompt-panel__preview">{{ mainAgentPromptPreview }}</pre>
        <div
          v-if="agentInteraction.mainAgentPromptMode === 'custom'"
          class="main-agent-prompt-panel__editor"
        >
          <label class="settings-row__label" for="main-agent-custom-prompt">
            自定义主 Agent 提示词
          </label>
          <textarea
            id="main-agent-custom-prompt"
            v-model="mainAgentCustomPromptDraft"
            class="ui-textarea main-agent-prompt-panel__textarea"
            data-agent-id="settings.agent.main-prompt.custom"
            rows="8"
            :disabled="savingAgentInteraction"
          />
          <button
            type="button"
            class="ui-button ui-button--ghost main-agent-prompt-panel__save"
            data-agent-id="settings.agent.main-prompt.save"
            :disabled="savingAgentInteraction"
            @click="saveMainAgentCustomPrompt"
          >
            应用自定义提示词
          </button>
        </div>
      </section>
    </SettingsCollapsibleCard>

    <SettingsCollapsibleCard
      v-model:expanded="autoTurnCardExpanded"
      aria-label="自动轮次策略"
      :controls-id="autoTurnDetailsId"
      toggle-agent-id="settings.agent.auto-turn.toggle"
      expand-label="展开自动轮次策略"
      collapse-label="收起自动轮次策略"
      with-switch
    >
      <template #summary>
        <div>
          <div class="settings-row__label">自动轮次策略</div>
          <p class="agent-collapsible-hint">Auto 模式下调整轮次启动前的执行策略。</p>
        </div>
      </template>
      <template #switch>
        <div class="ui-segmented" role="radiogroup" aria-label="自动轮次策略">
          <button
            type="button"
            role="radio"
            :aria-checked="!agentInteraction.autoTurnDecision.enabled"
            data-agent-id="settings.agent.auto-turn.off"
            :class="{ 'is-active': !agentInteraction.autoTurnDecision.enabled }"
            :disabled="savingAgentInteraction"
            @click="setAutoTurnDecisionField('enabled', false)"
          >
            关闭
          </button>
          <button
            type="button"
            role="radio"
            :aria-checked="agentInteraction.autoTurnDecision.enabled"
            data-agent-id="settings.agent.auto-turn.on"
            :class="{ 'is-active': agentInteraction.autoTurnDecision.enabled }"
            :disabled="savingAgentInteraction"
            @click="setAutoTurnDecisionField('enabled', true)"
          >
            开启
          </button>
        </div>
      </template>

      <div class="auto-turn-card__grid">
        <div
          v-for="option in autoTurnDecisionPermissionOptions"
          :key="option.key"
          class="settings-row settings-row--nested"
        >
          <div class="settings-row__label">{{ option.label }}</div>
          <div class="ui-segmented" role="radiogroup" :aria-label="option.ariaLabel">
            <button
              type="button"
              role="radio"
              :aria-checked="!agentInteraction.autoTurnDecision[option.key]"
              :data-agent-id="`settings.agent.auto-turn.${option.key}.off`"
              :class="{ 'is-active': !agentInteraction.autoTurnDecision[option.key] }"
              :disabled="savingAgentInteraction || !agentInteraction.autoTurnDecision.enabled"
              @click="setAutoTurnDecisionField(option.key, false)"
            >
              禁止
            </button>
            <button
              type="button"
              role="radio"
              :aria-checked="agentInteraction.autoTurnDecision[option.key]"
              :data-agent-id="`settings.agent.auto-turn.${option.key}.on`"
              :class="{ 'is-active': agentInteraction.autoTurnDecision[option.key] }"
              :disabled="savingAgentInteraction || !agentInteraction.autoTurnDecision.enabled"
              @click="setAutoTurnDecisionField(option.key, true)"
            >
              允许
            </button>
          </div>
        </div>
      </div>
    </SettingsCollapsibleCard>

    <SettingsCollapsibleCard
      v-model:expanded="subagentCardExpanded"
      aria-label="Subagent 模式"
      :controls-id="subagentDetailsId"
      toggle-agent-id="settings.agent-interaction.subagent-details.toggle"
      expand-label="展开 Subagent 详细配置"
      collapse-label="收起 Subagent 详细配置"
      :disabled="!agentInteraction.subagentMode.enabled"
      with-switch
    >
      <template #summary>
        <div>
          <div class="settings-row__label">Subagent 模式</div>
          <p class="agent-collapsible-hint">启用后可展开配置 native-agentkit 子代理行为。</p>
        </div>
      </template>

      <template #switch>
        <div class="ui-segmented" role="radiogroup" aria-label="Subagent 模式">
          <button
            type="button"
            role="radio"
            :aria-checked="!agentInteraction.subagentMode.enabled"
            data-agent-id="settings.agent.subagent.off"
            :class="{ 'is-active': !agentInteraction.subagentMode.enabled }"
            :disabled="savingAgentInteraction"
            @click="setSubagentModeEnabled(false)"
          >
            关闭
          </button>
          <button
            type="button"
            role="radio"
            :aria-checked="agentInteraction.subagentMode.enabled"
            data-agent-id="settings.agent.subagent.on"
            :class="{ 'is-active': agentInteraction.subagentMode.enabled }"
            :disabled="savingAgentInteraction"
            @click="setSubagentModeEnabled(true)"
          >
            开启
          </button>
        </div>
      </template>

      <div class="settings-row settings-row--nested">
        <div class="settings-row__label">转发子代理文本</div>
        <div class="ui-segmented" role="radiogroup" aria-label="转发子代理文本">
          <button
            type="button"
            role="radio"
            :aria-checked="!agentInteraction.subagentMode.forwardSubagentText"
            data-agent-id="settings.agent.subagent.forward-text.off"
            :class="{ 'is-active': !agentInteraction.subagentMode.forwardSubagentText }"
            :disabled="savingAgentInteraction || !agentInteraction.subagentMode.enabled"
            @click="setSubagentModeField('forwardSubagentText', false)"
          >
            关闭
          </button>
          <button
            type="button"
            role="radio"
            :aria-checked="agentInteraction.subagentMode.forwardSubagentText"
            data-agent-id="settings.agent.subagent.forward-text.on"
            :class="{ 'is-active': agentInteraction.subagentMode.forwardSubagentText }"
            :disabled="savingAgentInteraction || !agentInteraction.subagentMode.enabled"
            @click="setSubagentModeField('forwardSubagentText', true)"
          >
            开启
          </button>
        </div>
      </div>

      <div class="settings-row settings-row--nested">
        <div class="settings-row__label">进度摘要</div>
        <div class="ui-segmented" role="radiogroup" aria-label="进度摘要">
          <button
            type="button"
            role="radio"
            :aria-checked="!agentInteraction.subagentMode.agentProgressSummaries"
            data-agent-id="settings.agent.subagent.progress-summary.off"
            :class="{ 'is-active': !agentInteraction.subagentMode.agentProgressSummaries }"
            :disabled="savingAgentInteraction || !agentInteraction.subagentMode.enabled"
            @click="setSubagentModeField('agentProgressSummaries', false)"
          >
            关闭
          </button>
          <button
            type="button"
            role="radio"
            :aria-checked="agentInteraction.subagentMode.agentProgressSummaries"
            data-agent-id="settings.agent.subagent.progress-summary.on"
            :class="{ 'is-active': agentInteraction.subagentMode.agentProgressSummaries }"
            :disabled="savingAgentInteraction || !agentInteraction.subagentMode.enabled"
            @click="setSubagentModeField('agentProgressSummaries', true)"
          >
            开启
          </button>
        </div>
      </div>
    </SettingsCollapsibleCard>

    <Suspense v-if="subagentCatalogReady">
      <SubagentCatalogSection />
    </Suspense>
    <section v-else class="card subagent-section subagent-section--placeholder" aria-busy="true">
      <div class="subagent-section__header">
        <div class="subagent-section__title">
          <span>自定义 Agent</span>
        </div>
      </div>
      <div class="subagent-empty">首屏加载后补齐自定义 Agent 目录…</div>
    </section>

    <div v-if="agentInteractionError" class="conn-banner conn-banner--err">
      <AlertTriangle :size="16" aria-hidden="true" />
      <div>
        <div class="conn-banner__title">Agent 交互</div>
        <div class="conn-banner__hint">{{ agentInteractionError }}</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.agent-settings-stack {
  display: grid;
  gap: 12px;
}

.agent-settings-card {
  display: grid;
  gap: 12px;
}

.agent-collapsible-hint {
  margin: 4px 0 0;
  color: var(--text-secondary, rgba(255, 255, 255, 0.6));
  font-size: 13px;
}

.auto-turn-card__grid {
  display: grid;
  gap: 4px;
}

.permission-toggle-list {
  display: grid;
  gap: 8px;
}

.permission-toggle-item {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 12px;
  padding: 10px 0;
  border-top: 1px solid var(--ui-border, rgba(255, 255, 255, 0.08));
}

.permission-toggle-item:first-child {
  border-top: 0;
}

.permission-toggle-item__content {
  min-width: 0;
}

.permission-toggle-item__hint {
  margin: 4px 0 0;
  color: var(--text-secondary, rgba(255, 255, 255, 0.6));
  font-size: 13px;
}

.permission-toggle-item__badge {
  margin-left: 8px;
  color: var(--text-secondary, rgba(255, 255, 255, 0.6));
  font-size: 12px;
  font-weight: 500;
}

.permission-toggle-item__locked {
  min-width: 74px;
  justify-content: center;
}

.main-agent-prompt-panel {
  display: grid;
  gap: 10px;
}

.main-agent-prompt-panel__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.main-agent-prompt-panel__meta {
  color: var(--text-secondary, rgba(255, 255, 255, 0.6));
  font-size: 12px;
}

.main-agent-prompt-panel__preview {
  max-height: 240px;
  margin: 0;
  padding: 10px 12px;
  overflow: auto;
  border: 1px solid var(--ui-border, rgba(255, 255, 255, 0.08));
  border-radius: 8px;
  background: var(--bg-elev-2, rgba(255, 255, 255, 0.02));
  color: var(--text-secondary, rgba(255, 255, 255, 0.68));
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace);
  font-size: 12px;
  line-height: 1.55;
  white-space: pre-wrap;
}

.main-agent-prompt-panel__editor {
  display: grid;
  gap: 8px;
}

.main-agent-prompt-panel__textarea {
  min-height: 156px;
  resize: vertical;
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace);
  font-size: 12px;
  line-height: 1.55;
}

.main-agent-prompt-panel__save {
  justify-self: start;
}

.settings-row--nested {
  padding-left: 12px;
}

.subagent-section--placeholder {
  opacity: 0.78;
}
</style>

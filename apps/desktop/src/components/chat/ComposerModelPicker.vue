<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import Brain from "@lucide/vue/dist/esm/icons/brain.mjs";
import Check from "@lucide/vue/dist/esm/icons/check.mjs";
import Sparkles from "@lucide/vue/dist/esm/icons/sparkles.mjs";
import {
  reasoningEffortsForBackend,
  type ChatComposerState,
  type ChatModelOption,
  type ModelSelectionExplanation,
  type ReasoningEffort,
} from "@lilia/contracts";
import { addDomEventListener, runUnlistenFns } from "@lilia/ui/utils/eventListeners";

const props = defineProps<{
  state: ChatComposerState;
  modelOptions: ChatModelOption[];
  autoModelPreview: ModelSelectionExplanation;
}>();

const emit = defineEmits<{
  update: [patch: Partial<ChatComposerState>];
}>();

const open = ref(false);
const root = ref<HTMLElement | null>(null);
let documentUnlisteners: Array<() => void> = [];

const manualMode = computed(() => props.state.modelSelectionMode === "manual");
const labelForModel = (model: string) => props.modelOptions.find((option) => option.id === model)?.label ?? model;
const modelLabel = computed(() => labelForModel(props.state.model || props.autoModelPreview.model));
const autoModelLabel = computed(() => labelForModel(props.autoModelPreview.model));
const effectiveEffort = computed<ReasoningEffort | null>(() =>
  manualMode.value
    ? props.state.reasoningEffort ?? props.autoModelPreview.reasoningEffort ?? null
    : props.autoModelPreview.reasoningEffort ?? null,
);
const effortLabel = computed(() => effectiveEffort.value ?? "auto");
const autoPresetLabel = computed(
  () => props.autoModelPreview.presetLabel || props.autoModelPreview.presetId || null,
);
const triggerLabel = computed(() =>
  manualMode.value
    ? `手动 · ${modelLabel.value} · ${effortLabel.value}`
    : autoPresetLabel.value
      ? `自动 · ${autoPresetLabel.value} · ${autoModelLabel.value} · ${effortLabel.value}`
      : `自动 · ${autoModelLabel.value} · ${effortLabel.value}`,
);
const effortOptions = computed(() =>
  reasoningEffortsForBackend(props.state.backend),
);

function close() {
  open.value = false;
}

function toggle() {
  open.value = !open.value;
}

function setAuto() {
  emit("update", { modelSelectionMode: "auto", reasoningEffort: null });
  close();
}

function setModel(model: string) {
  emit("update", {
    modelSelectionMode: "manual",
    model,
    reasoningEffort: props.state.reasoningEffort ?? props.autoModelPreview.reasoningEffort ?? "medium",
  });
}

function setEffort(reasoningEffort: ReasoningEffort) {
  emit("update", {
    modelSelectionMode: "manual",
    reasoningEffort,
  });
}

function onDocPointer(event: PointerEvent) {
  if (!root.value?.contains(event.target as Node | null)) close();
}

function onKey(event: KeyboardEvent) {
  if (event.key !== "Escape" || !open.value) return;
  close();
  event.stopPropagation();
}

function clearDocumentListeners() {
  runUnlistenFns(documentUnlisteners.splice(0).reverse());
}

function installDocumentListeners() {
  clearDocumentListeners();
  documentUnlisteners = [
    addDomEventListener(document, "pointerdown", onDocPointer, true),
    addDomEventListener(document, "keydown", onKey),
  ];
}

watch(open, (visible) => {
  clearDocumentListeners();
  if (visible) {
    installDocumentListeners();
  }
});

onBeforeUnmount(() => {
  clearDocumentListeners();
});
</script>

<template>
  <div ref="root" class="composer-model-picker" data-agent-id="chat.model-picker">
    <button
      type="button"
      class="composer-model-picker__trigger"
      :class="{ 'is-manual': manualMode }"
      :title="triggerLabel"
      aria-haspopup="menu"
      :aria-expanded="open"
      data-agent-id="chat.model-picker.trigger"
      @click="toggle"
    >
      <Brain :size="14" aria-hidden="true" />
      <span>{{ triggerLabel }}</span>
    </button>
    <div
      v-if="open"
      class="composer-model-picker__menu"
      role="menu"
      data-agent-id="chat.model-picker.menu"
    >
      <button
        type="button"
        class="composer-model-picker__item composer-model-picker__item--mode"
        :class="{ 'is-active': !manualMode }"
        role="menuitemradio"
        :aria-checked="!manualMode"
        data-agent-id="chat.model-picker.auto"
        @click="setAuto"
      >
        <Sparkles :size="14" aria-hidden="true" />
        <span class="composer-model-picker__main">自动</span>
        <span class="composer-model-picker__meta">
          <template v-if="autoPresetLabel">{{ autoPresetLabel }} · </template>
          {{ autoModelLabel }} · {{ autoModelPreview.reasoningEffort ?? "auto" }}
        </span>
        <Check v-if="!manualMode" :size="14" aria-hidden="true" />
      </button>
      <div class="composer-model-picker__section" role="none">模型</div>
      <button
        v-for="option in modelOptions"
        :key="option.id"
        type="button"
        class="composer-model-picker__item"
        :class="{ 'is-active': manualMode && state.model === option.id }"
        role="menuitemradio"
        :aria-checked="manualMode && state.model === option.id"
        :data-agent-id="`chat.model-picker.model.${option.id}`"
        @click="setModel(option.id)"
      >
        <span class="composer-model-picker__main">{{ option.label }}</span>
        <span class="composer-model-picker__meta">{{ option.id }}</span>
        <Check v-if="manualMode && state.model === option.id" :size="14" aria-hidden="true" />
      </button>
      <div class="composer-model-picker__section" role="none">Thinking</div>
      <div class="composer-model-picker__efforts" role="none">
        <button
          v-for="effort in effortOptions"
          :key="effort"
          type="button"
          class="composer-model-picker__effort"
          :class="{ 'is-active': manualMode && effectiveEffort === effort }"
          role="menuitemradio"
          :aria-checked="manualMode && effectiveEffort === effort"
          :data-agent-id="`chat.model-picker.effort.${effort}`"
          @click="setEffort(effort)"
        >
          {{ effort }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.composer-model-picker {
  position: relative;
}

.composer-model-picker__trigger {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: min(42vw, 270px);
  height: 28px;
  padding: 0 9px;
  border: 1px solid var(--border-soft);
  border-radius: 8px;
  background: var(--bg-subtle);
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1;
  white-space: nowrap;
}

.composer-model-picker__trigger > span {
  overflow: hidden;
  text-overflow: ellipsis;
}

.composer-model-picker__trigger:hover,
.composer-model-picker__trigger:focus-visible,
.composer-model-picker__trigger.is-manual {
  color: var(--text);
  border-color: var(--border);
  background: var(--bg-elev);
}

.composer-model-picker__menu {
  position: absolute;
  left: 0;
  bottom: calc(100% + 8px);
  z-index: 40;
  width: min(340px, 86vw);
  padding: 6px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-elev);
  box-shadow: 0 8px 24px -8px rgba(0, 0, 0, 0.5);
}

.composer-model-picker__section {
  padding: 8px 8px 4px;
  color: var(--text-faint);
  font-size: 11px;
  line-height: 1;
}

.composer-model-picker__item {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 4px 10px;
  width: 100%;
  min-height: 34px;
  padding: 6px 8px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text);
  text-align: left;
}

.composer-model-picker__item--mode {
  grid-template-columns: auto minmax(0, 1fr) auto;
}

.composer-model-picker__item:hover,
.composer-model-picker__item:focus-visible,
.composer-model-picker__item.is-active {
  background: var(--bg-hover);
}

.composer-model-picker__main,
.composer-model-picker__meta {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.composer-model-picker__main {
  font-size: 12px;
  font-weight: 600;
}

.composer-model-picker__meta {
  grid-column: 1 / -1;
  color: var(--text-faint);
  font-size: 11px;
}

.composer-model-picker__item--mode .composer-model-picker__meta {
  grid-column: 2 / 3;
}

.composer-model-picker__efforts {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 4px;
  padding: 0 2px 2px;
}

.composer-model-picker__effort {
  min-width: 0;
  height: 28px;
  border: 1px solid var(--border-soft);
  border-radius: 6px;
  background: transparent;
  color: var(--text-muted);
  font-size: 11px;
}

.composer-model-picker__effort:hover,
.composer-model-picker__effort:focus-visible,
.composer-model-picker__effort.is-active {
  color: var(--text);
  border-color: var(--border);
  background: var(--bg-hover);
}
</style>

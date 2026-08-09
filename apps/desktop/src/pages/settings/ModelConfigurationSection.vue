<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import Brain from "@lucide/vue/dist/esm/icons/brain.mjs";
import Plus from "@lucide/vue/dist/esm/icons/plus.mjs";
import Trash2 from "@lucide/vue/dist/esm/icons/trash-2.mjs";
import type {
  AssistantAIModelPoolItem,
  ModelFeatureSettings,
  ModelPresetGroup,
  ReasoningEffort,
  SuggestionSettings,
} from "@lilia/contracts";
import {
  BUILTIN_MODEL_PRESET_IDS,
  DEFAULT_SUGGESTION_SOURCE,
  REASONING_EFFORTS,
  builtinPresetLabel,
  normalizeModelFeatureSettings,
} from "@lilia/contracts";
import {
  getConversationSuggestionSettings,
  getModelFeatureSettings,
  listModelFeatureOptions,
  setConversationSuggestionSettings,
  setModelFeatureSettings,
} from "../../services/chat";

type FeatureKey = Exclude<keyof ModelFeatureSettings, "chat" | "presets">;

const suggestionSettings = ref<SuggestionSettings>({
  enabled: true,
  source: DEFAULT_SUGGESTION_SOURCE,
});
const modelFeatureSettings = ref<ModelFeatureSettings>(
  normalizeModelFeatureSettings(null),
);
const modelOptions = ref<AssistantAIModelPoolItem[]>([]);
const savingSuggestions = ref(false);
const savingModelFeatures = ref(false);
const newPresetLabel = ref("");
let disposed = false;

const featureRows: Array<{ key: FeatureKey; label: string }> = [
  { key: "title", label: "标题生成" },
  { key: "suggestion", label: "新对话建议" },
  { key: "promptRouter", label: "Prompt Router" },
  { key: "promptOptimize", label: "Prompt Optimize" },
  { key: "autoTurnDecision", label: "自动回合决策" },
];

const effortOptions: Array<{ value: "" | ReasoningEffort; label: string }> = [
  { value: "", label: "默认强度" },
  ...REASONING_EFFORTS.map((effort) => ({ value: effort, label: effort })),
];

const hasModelOptions = computed(() => modelOptions.value.length > 0);

const builtinPresets = computed(() =>
  modelFeatureSettings.value.presets.filter((preset) => preset.kind === "builtin"),
);
const customPresets = computed(() =>
  modelFeatureSettings.value.presets.filter((preset) => preset.kind === "custom"),
);

const presetDescription: Record<string, string> = {
  fast: "压缩、诊断与轻量维护",
  default: "普通编码回合",
  plan: "规划与长程拆解",
  review: "审查与修复分析",
};

async function loadAll() {
  try {
    const [suggestions, featureSettings, models] = await Promise.all([
      getConversationSuggestionSettings(),
      getModelFeatureSettings(),
      listModelFeatureOptions(),
    ]);
    if (disposed) return;
    suggestionSettings.value = suggestions;
    modelFeatureSettings.value = normalizeModelFeatureSettings(featureSettings);
    modelOptions.value = models;
  } catch (err) {
    console.error("[settings] load model configuration failed", err);
  }
}

async function setSuggestionEnabled(enabled: boolean) {
  if (disposed) return;
  const next: SuggestionSettings = { ...suggestionSettings.value, enabled };
  suggestionSettings.value = next;
  savingSuggestions.value = true;
  try {
    await setConversationSuggestionSettings(next);
  } catch (err) {
    console.error("[settings] save suggestion settings failed", err);
  } finally {
    if (!disposed) savingSuggestions.value = false;
  }
}

async function updateModelFeatureSettings(next: ModelFeatureSettings) {
  if (disposed) return;
  const previous = modelFeatureSettings.value;
  const normalized = normalizeModelFeatureSettings(next);
  modelFeatureSettings.value = normalized;
  savingModelFeatures.value = true;
  try {
    await setModelFeatureSettings(normalized);
  } catch (err) {
    if (!disposed) {
      modelFeatureSettings.value = previous;
      console.error("[settings] save model feature settings failed", err);
    }
  } finally {
    if (!disposed) savingModelFeatures.value = false;
  }
}

function updatePreset(presetId: string, patch: Partial<ModelPresetGroup>) {
  const presets = modelFeatureSettings.value.presets.map((preset) =>
    preset.id === presetId ? { ...preset, ...patch } : preset,
  );
  return updateModelFeatureSettings({
    ...modelFeatureSettings.value,
    presets,
  });
}

async function setPresetModel(presetId: string, value: string) {
  await updatePreset(presetId, { model: value || null });
}

async function setPresetEffort(presetId: string, value: string) {
  const effort = (REASONING_EFFORTS as readonly string[]).includes(value)
    ? (value as ReasoningEffort)
    : null;
  await updatePreset(presetId, { reasoningEffort: effort });
}

async function setFeature(key: FeatureKey, value: string) {
  await updateModelFeatureSettings({
    ...modelFeatureSettings.value,
    [key]: value || null,
  });
}

function makeCustomId(label: string): string {
  const slug = label
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9\u4e00-\u9fff]+/gi, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 32);
  const base = slug || "preset";
  let id = `custom-${base}`;
  let n = 2;
  const existing = new Set(modelFeatureSettings.value.presets.map((p) => p.id));
  while (existing.has(id)) {
    id = `custom-${base}-${n}`;
    n += 1;
  }
  return id;
}

async function addCustomPreset() {
  const label = newPresetLabel.value.trim() || "自定义预设";
  const id = makeCustomId(label);
  newPresetLabel.value = "";
  await updateModelFeatureSettings({
    ...modelFeatureSettings.value,
    presets: [
      ...modelFeatureSettings.value.presets,
      {
        id,
        label,
        kind: "custom",
        model: null,
        reasoningEffort: null,
        enabled: true,
      },
    ],
  });
}

async function removeCustomPreset(presetId: string) {
  if (BUILTIN_MODEL_PRESET_IDS.includes(presetId as (typeof BUILTIN_MODEL_PRESET_IDS)[number])) {
    return;
  }
  await updateModelFeatureSettings({
    ...modelFeatureSettings.value,
    presets: modelFeatureSettings.value.presets.filter((preset) => preset.id !== presetId),
  });
}

async function renameCustomPreset(presetId: string, label: string) {
  const next = label.trim();
  if (!next) return;
  await updatePreset(presetId, { label: next });
}

onMounted(() => {
  disposed = false;
  void loadAll();
});

onBeforeUnmount(() => {
  disposed = true;
});
</script>

<template>
  <div class="card">
    <h2>
      <span class="card-h2__title">
        <Brain :size="14" aria-hidden="true" />
        模型预设组
      </span>
    </h2>
    <p class="settings-hint">
      按角色绑定模型；自动分流会根据任务意图选择 Default / Plan / Fast / Review。自定义组可增删，供手动或后续扩展使用。
    </p>

    <div
      v-for="preset in builtinPresets"
      :key="preset.id"
      class="settings-row preset-row"
    >
      <div class="settings-row__label">
        <div class="preset-row__title">{{ builtinPresetLabel(preset.id) }}</div>
        <div class="preset-row__desc">{{ presetDescription[preset.id] ?? "" }}</div>
      </div>
      <div class="settings-row__control preset-row__controls">
        <select
          class="ui-input"
          :aria-label="`${builtinPresetLabel(preset.id)} 模型`"
          :data-agent-id="`settings.model-config.preset.${preset.id}.model`"
          :value="preset.model ?? ''"
          :disabled="!hasModelOptions || savingModelFeatures"
          @change="(e) => setPresetModel(preset.id, (e.target as HTMLSelectElement).value)"
        >
          <option value="">目录默认</option>
          <option v-for="option in modelOptions" :key="option.id" :value="option.id">
            {{ option.label }} ({{ option.id }})
          </option>
        </select>
        <select
          class="ui-input preset-row__effort"
          :aria-label="`${builtinPresetLabel(preset.id)} 思考强度`"
          :data-agent-id="`settings.model-config.preset.${preset.id}.effort`"
          :value="preset.reasoningEffort ?? ''"
          :disabled="savingModelFeatures"
          @change="(e) => setPresetEffort(preset.id, (e.target as HTMLSelectElement).value)"
        >
          <option v-for="opt in effortOptions" :key="opt.value || 'default'" :value="opt.value">
            {{ opt.label }}
          </option>
        </select>
      </div>
    </div>

    <div class="preset-custom-header">
      <div class="preset-custom-header__title">自定义预设</div>
      <div class="preset-custom-header__add">
        <input
          v-model="newPresetLabel"
          class="ui-input"
          type="text"
          placeholder="名称"
          aria-label="新预设名称"
          data-agent-id="settings.model-config.preset.custom.name"
          :disabled="savingModelFeatures"
          @keydown.enter.prevent="addCustomPreset"
        >
        <button
          type="button"
          class="ui-button"
          data-agent-id="settings.model-config.preset.custom.add"
          :disabled="savingModelFeatures"
          @click="addCustomPreset"
        >
          <Plus :size="14" aria-hidden="true" />
          添加
        </button>
      </div>
    </div>

    <div v-if="customPresets.length === 0" class="settings-empty">
      暂无自定义预设。添加后可单独绑定模型。
    </div>

    <div
      v-for="preset in customPresets"
      :key="preset.id"
      class="settings-row preset-row"
    >
      <div class="settings-row__label">
        <input
          class="ui-input preset-row__label-input"
          type="text"
          :aria-label="`${preset.label} 名称`"
          :data-agent-id="`settings.model-config.preset.${preset.id}.label`"
          :value="preset.label"
          :disabled="savingModelFeatures"
          @change="(e) => renameCustomPreset(preset.id, (e.target as HTMLInputElement).value)"
        >
      </div>
      <div class="settings-row__control preset-row__controls">
        <select
          class="ui-input"
          :aria-label="`${preset.label} 模型`"
          :data-agent-id="`settings.model-config.preset.${preset.id}.model`"
          :value="preset.model ?? ''"
          :disabled="!hasModelOptions || savingModelFeatures"
          @change="(e) => setPresetModel(preset.id, (e.target as HTMLSelectElement).value)"
        >
          <option value="">未绑定</option>
          <option v-for="option in modelOptions" :key="option.id" :value="option.id">
            {{ option.label }} ({{ option.id }})
          </option>
        </select>
        <select
          class="ui-input preset-row__effort"
          :aria-label="`${preset.label} 思考强度`"
          :data-agent-id="`settings.model-config.preset.${preset.id}.effort`"
          :value="preset.reasoningEffort ?? ''"
          :disabled="savingModelFeatures"
          @change="(e) => setPresetEffort(preset.id, (e.target as HTMLSelectElement).value)"
        >
          <option v-for="opt in effortOptions" :key="opt.value || 'default'" :value="opt.value">
            {{ opt.label }}
          </option>
        </select>
        <button
          type="button"
          class="ui-button ui-button--ghost"
          :aria-label="`删除 ${preset.label}`"
          :data-agent-id="`settings.model-config.preset.${preset.id}.remove`"
          :disabled="savingModelFeatures"
          @click="removeCustomPreset(preset.id)"
        >
          <Trash2 :size="14" aria-hidden="true" />
        </button>
      </div>
    </div>
  </div>

  <div class="card">
    <h2>
      <span class="card-h2__title">
        <Brain :size="14" aria-hidden="true" />
        辅助模型
      </span>
    </h2>

    <div v-for="row in featureRows" :key="row.key" class="settings-row">
      <div class="settings-row__label">{{ row.label }}</div>
      <div class="settings-row__control model-feature-row__control">
        <div
          v-if="row.key === 'suggestion'"
          class="ui-segmented"
          role="radiogroup"
          aria-label="新对话建议启用状态"
        >
          <button
            type="button"
            role="radio"
            :aria-checked="suggestionSettings.enabled"
            data-agent-id="settings.suggestions.enabled.on"
            :class="{ 'is-active': suggestionSettings.enabled }"
            :disabled="savingSuggestions"
            @click="setSuggestionEnabled(true)"
          >
            开启
          </button>
          <button
            type="button"
            role="radio"
            :aria-checked="!suggestionSettings.enabled"
            data-agent-id="settings.suggestions.enabled.off"
            :class="{ 'is-active': !suggestionSettings.enabled }"
            :disabled="savingSuggestions"
            @click="setSuggestionEnabled(false)"
          >
            关闭
          </button>
        </div>
        <select
          class="ui-input"
          :class="{ 'model-feature-row__select': row.key === 'suggestion' }"
          :aria-label="row.key === 'suggestion' ? '新对话建议模型' : row.label"
          :data-agent-id="`settings.model-config.feature.${row.key}`"
          :value="modelFeatureSettings[row.key] ?? ''"
          :disabled="!hasModelOptions || savingModelFeatures"
          @change="(e) => setFeature(row.key, (e.target as HTMLSelectElement).value)"
        >
          <option value="">默认</option>
          <option v-for="option in modelOptions" :key="option.id" :value="option.id">
            {{ option.label }} ({{ option.id }})
          </option>
        </select>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-hint {
  margin: 0 0 12px;
  color: var(--text-muted, #8b93a7);
  font-size: 12px;
  line-height: 1.45;
}

.preset-row__title {
  font-weight: 600;
}

.preset-row__desc {
  margin-top: 2px;
  color: var(--text-muted, #8b93a7);
  font-size: 11px;
}

.preset-row__controls {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  justify-content: flex-end;
  flex: 1 1 auto;
  min-width: 0;
}

.preset-row__effort {
  width: min(140px, 28vw);
}

.preset-row__label-input {
  width: min(160px, 34vw);
}

.preset-custom-header {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  justify-content: space-between;
  margin: 16px 0 8px;
}

.preset-custom-header__title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-muted, #8b93a7);
}

.preset-custom-header__add {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.preset-custom-header__add .ui-input {
  width: min(180px, 40vw);
}

.settings-empty {
  margin: 0 0 8px;
  color: var(--text-muted, #8b93a7);
  font-size: 12px;
}

.model-feature-row__control {
  flex: 1 1 auto;
  min-width: 0;
}

.settings-row .model-feature-row__select {
  width: min(320px, 34vw);
}

@media (max-width: 720px) {
  .model-feature-row__control,
  .preset-row__controls {
    justify-content: flex-start;
  }

  .settings-row .model-feature-row__select,
  .preset-row__effort,
  .preset-row__label-input {
    width: min(360px, 100%);
  }
}
</style>

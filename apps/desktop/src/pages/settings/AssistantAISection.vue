<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import AlertTriangle from "@lucide/vue/dist/esm/icons/triangle-alert.mjs";
import KeyRound from "@lucide/vue/dist/esm/icons/key-round.mjs";
import Pencil from "@lucide/vue/dist/esm/icons/pencil.mjs";
import Plug from "@lucide/vue/dist/esm/icons/plug.mjs";
import Plus from "@lucide/vue/dist/esm/icons/plus.mjs";
import Save from "@lucide/vue/dist/esm/icons/save.mjs";
import Sparkles from "@lucide/vue/dist/esm/icons/sparkles.mjs";
import X from "@lucide/vue/dist/esm/icons/x.mjs";
import type {
  AssistantAIConfig,
  AssistantAIModelPoolItem,
  AssistantAIModelsResult,
  AssistantAITestResult,
} from "@lilia/contracts";
import {
  fetchAssistantAIModels,
  getAssistantAIConfig,
  setAssistantAIConfig,
  testAssistantAIConnection,
} from "../../services/chat";

const assistantAIForm = ref<AssistantAIConfig>({
  baseUrl: null,
  apiKey: null,
  model: null,
  modelPool: [],
  hasApiKey: false,
});
const savingAssistantAI = ref(false);
const fetchingModels = ref(false);
const testingAssistantAI = ref(false);
const assistantAIResult = ref<AssistantAITestResult | null>(null);
const fetchModelsResult = ref<AssistantAIModelsResult | null>(null);
const addingModel = ref(false);
const editingProviderConfig = ref(false);
const newModelId = ref("");
const newModelLabel = ref("");
let disposed = false;

const assistantAIBannerHint = computed(() => {
  const r = assistantAIResult.value;
  if (!r) return "";
  if (!r.ok) return r.error ?? "未知错误";
  if (r.modelMatched === false) {
    return `已连接，但当前模型不在 /models 列表里（共 ${r.models?.length ?? 0} 个可用）。`;
  }
  if (r.modelMatched === true) {
    return "已连接，当前模型在端点 /models 列表里。";
  }
  return "已连接（端点未返回 /models 列表）。";
});

function modelPool(): AssistantAIModelPoolItem[] {
  return assistantAIForm.value.modelPool ?? [];
}

const assistantAIProviderState = computed(() =>
  assistantAIForm.value.hasApiKey ? "密钥已保存" : "未保存密钥"
);
const modelPoolCount = computed(() => modelPool().length);

function normalizedAssistantAI(): AssistantAIConfig {
  return {
    baseUrl: assistantAIForm.value.baseUrl?.trim() || null,
    apiKey: assistantAIForm.value.apiKey?.trim() || null,
    model: assistantAIForm.value.model?.trim() || null,
    modelPool: modelPool(),
    hasApiKey: assistantAIForm.value.hasApiKey,
  };
}

function mergeFetchedModels(models: AssistantAIModelPoolItem[]) {
  const byId = new Map(modelPool().map((item) => [item.id, item]));
  for (const item of models) {
    const id = item.id.trim();
    if (!id) continue;
    const existing = byId.get(id);
    byId.set(id, {
      id,
      label: existing?.label?.trim() || item.label.trim() || id,
      source: "remote",
      backend: "native-agentkit",
    });
  }
  assistantAIForm.value.modelPool = [...byId.values()];
}

async function loadAssistantAI() {
  try {
    const config = await getAssistantAIConfig();
    if (disposed) return;
    assistantAIForm.value = {
      ...config,
      apiKey: null,
      modelPool: config.modelPool ?? [],
    };
  }
  catch (err) { console.error("[settings] load assistant ai config failed", err); }
}

async function saveAssistantAI() {
  if (disposed) return;
  savingAssistantAI.value = true;
  try {
    await setAssistantAIConfig(normalizedAssistantAI());
    if (disposed) return;
    await loadAssistantAI();
    editingProviderConfig.value = false;
  } catch (err) { console.error("[settings] saveAssistantAI failed", err); }
  finally { if (!disposed) savingAssistantAI.value = false; }
}

async function fetchModels() {
  if (disposed) return;
  fetchingModels.value = true;
  fetchModelsResult.value = null;
  try {
    const result = await fetchAssistantAIModels(normalizedAssistantAI());
    if (disposed) return;
    fetchModelsResult.value = result;
    if (result.ok) mergeFetchedModels(result.models);
  } catch (err) {
    if (!disposed) fetchModelsResult.value = { ok: false, error: String(err), models: [] };
  } finally { if (!disposed) fetchingModels.value = false; }
}

function cancelAddModel() {
  addingModel.value = false;
  newModelId.value = "";
  newModelLabel.value = "";
}

function startProviderConfigEdit() {
  editingProviderConfig.value = true;
}

async function cancelProviderConfigEdit() {
  if (disposed) return;
  editingProviderConfig.value = false;
  cancelAddModel();
  fetchModelsResult.value = null;
  assistantAIResult.value = null;
  await loadAssistantAI();
}

function addModelToPool() {
  const id = newModelId.value.trim();
  if (!id) return;
  const label = newModelLabel.value.trim() || id;
  const item: AssistantAIModelPoolItem = {
    id,
    label,
    source: "remote",
    backend: "native-agentkit",
  };
  const next = [...modelPool()];
  const index = next.findIndex((candidate) => candidate.id === id);
  if (index >= 0) next[index] = item;
  else next.push(item);
  assistantAIForm.value.modelPool = next;
  cancelAddModel();
}

async function testAssistantAI() {
  if (disposed) return;
  testingAssistantAI.value = true;
  assistantAIResult.value = null;
  try {
    const result = await testAssistantAIConnection(normalizedAssistantAI());
    if (!disposed) assistantAIResult.value = result;
  } catch (err) {
    if (!disposed) assistantAIResult.value = {
      ok: false, error: String(err), models: null, modelMatched: null,
    };
  } finally { if (!disposed) testingAssistantAI.value = false; }
}

function updateModelLabel(id: string, label: string) {
  assistantAIForm.value.modelPool = modelPool().map((item) =>
    item.id === id ? { ...item, label } : item
  );
}

onMounted(() => {
  disposed = false;
  void loadAssistantAI();
});

onBeforeUnmount(() => {
  disposed = true;
});
</script>

<template>
  <div class="card assistant-ai-card">
    <div class="assistant-ai-card__header">
      <h2>
        <span class="card-h2__title">
          <Sparkles :size="14" aria-hidden="true" />
          Provider 配置
        </span>
      </h2>
      <button
        v-if="!editingProviderConfig"
        type="button"
        class="ui-button ui-button--primary"
        data-agent-id="settings.assistant-ai.add-provider"
        @click="startProviderConfigEdit"
      >
        <Plus :size="12" aria-hidden="true" />
        添加
      </button>
      <button
        v-else
        type="button"
        class="ui-button ui-button--ghost"
        data-agent-id="settings.assistant-ai.cancel-provider-edit"
        :disabled="savingAssistantAI || testingAssistantAI || fetchingModels"
        @click="cancelProviderConfigEdit"
      >
        <X :size="12" aria-hidden="true" />
        取消
      </button>
    </div>

    <template v-if="!editingProviderConfig">
      <div class="assistant-ai-provider-list" data-agent-id="settings.assistant-ai.provider-list">
        <button
          type="button"
          class="assistant-ai-provider-row"
          data-agent-id="settings.assistant-ai.provider-row"
          @click="startProviderConfigEdit"
        >
          <span class="assistant-ai-provider-row__avatar" aria-hidden="true">AI</span>
          <span class="assistant-ai-provider-row__body">
            <span class="assistant-ai-provider-row__title">Assistant AI Provider</span>
            <span class="assistant-ai-provider-row__meta">
              {{ modelPoolCount }} 个模型
            </span>
          </span>
          <span class="assistant-ai-provider-row__status">
            <KeyRound :size="12" aria-hidden="true" />
            {{ assistantAIProviderState }}
          </span>
          <span class="assistant-ai-provider-row__action">
            <Pencil :size="12" aria-hidden="true" />
            编辑
          </span>
        </button>
      </div>
    </template>

    <template v-else>
      <div class="settings-row">
        <div class="settings-row__label">基础 URL</div>
        <input
          type="text"
          class="ui-input"
          placeholder="https://api.example.com/v1"
          data-agent-id="settings.assistant-ai.base-url"
          :value="assistantAIForm.baseUrl ?? ''"
          @input="(e) => (assistantAIForm.baseUrl = (e.target as HTMLInputElement).value)"
        />
      </div>
      <div class="settings-row">
        <div class="settings-row__label">API 密钥</div>
        <input
          type="password"
          class="ui-input"
          :placeholder="assistantAIForm.hasApiKey ? '已保存，留空保留现有值' : 'sk-...'"
          data-agent-id="settings.assistant-ai.api-key"
          :value="assistantAIForm.apiKey ?? ''"
          @input="(e) => (assistantAIForm.apiKey = (e.target as HTMLInputElement).value)"
        />
      </div>

      <div class="settings-row assistant-ai-model-toolbar">
        <div class="settings-row__label">模型池</div>
        <div class="settings-row__control settings-row__control--loose">
          <button
            type="button"
            class="ui-button ui-button--ghost"
            data-agent-id="settings.assistant-ai.add-model"
            @click="addingModel = true"
          >
            <Plus :size="12" aria-hidden="true" />
            添加
          </button>
          <button
            type="button"
            class="ui-button ui-button--ghost"
            data-agent-id="settings.assistant-ai.fetch-models"
            :disabled="fetchingModels || savingAssistantAI"
            @click="fetchModels"
          >
            <Plug :size="12" aria-hidden="true" />
            {{ fetchingModels ? "获取中…" : "获取模型" }}
          </button>
        </div>
      </div>

      <div class="settings-model-pool" data-agent-id="settings.assistant-ai.model-pool">
        <div
          v-if="addingModel"
          class="settings-row settings-row--stacked assistant-ai-add-row"
          data-agent-id="settings.assistant-ai.add-model-row"
        >
          <div class="settings-row__label">添加模型</div>
          <div class="assistant-ai-add-row__fields">
            <input
              type="text"
              class="ui-input"
              aria-label="模型 ID"
              placeholder="model-id"
              data-agent-id="settings.assistant-ai.new-model-id"
              :value="newModelId"
              @input="(e) => (newModelId = (e.target as HTMLInputElement).value)"
              @keyup.enter="addModelToPool"
            />
            <input
              type="text"
              class="ui-input"
              aria-label="显示名"
              placeholder="显示名"
              data-agent-id="settings.assistant-ai.new-model-label"
              :value="newModelLabel"
              @input="(e) => (newModelLabel = (e.target as HTMLInputElement).value)"
              @keyup.enter="addModelToPool"
            />
            <button
              type="button"
              class="ui-button ui-button--primary"
              data-agent-id="settings.assistant-ai.confirm-add-model"
              :disabled="!newModelId.trim()"
              @click="addModelToPool"
            >
              <Plus :size="12" aria-hidden="true" />
              添加模型
            </button>
            <button
              type="button"
              class="ui-button ui-button--ghost"
              data-agent-id="settings.assistant-ai.cancel-add-model"
              @click="cancelAddModel"
            >
              取消
            </button>
          </div>
        </div>

        <div
          v-for="item in modelPool()"
          :key="item.id"
          class="settings-row"
        >
          <div class="settings-row__label">{{ item.id }}</div>
          <input
            type="text"
            class="ui-input"
            placeholder="显示名"
            :aria-label="`${item.id} 显示名`"
            :data-agent-id="`settings.assistant-ai.model-label.${item.id}`"
            :value="item.label"
            @input="(e) => updateModelLabel(item.id, (e.target as HTMLInputElement).value)"
          />
        </div>
        <p v-if="!modelPool().length && !addingModel" class="muted assistant-ai-empty">
          暂无模型
        </p>
      </div>

      <div class="settings-row">
        <div class="settings-row__label">配置操作</div>
        <div class="settings-row__control">
          <button
            type="button"
            class="ui-button ui-button--ghost"
            data-agent-id="settings.assistant-ai.save"
            :disabled="savingAssistantAI || testingAssistantAI || fetchingModels"
            @click="saveAssistantAI"
          >
            <Save :size="12" aria-hidden="true" />
            {{ savingAssistantAI ? "保存中…" : "保存" }}
          </button>
          <button
            type="button"
            class="ui-button ui-button--ghost"
            data-agent-id="settings.assistant-ai.test"
            :disabled="testingAssistantAI || savingAssistantAI || fetchingModels"
            @click="testAssistantAI"
          >
            <Plug :size="12" aria-hidden="true" />
            {{ testingAssistantAI ? "测试中…" : "测试连接" }}
          </button>
        </div>
      </div>

      <div
        v-if="fetchModelsResult"
        class="conn-banner"
        :class="fetchModelsResult.ok ? 'conn-banner--ok' : 'conn-banner--err'"
      >
        <component :is="fetchModelsResult.ok ? Plug : AlertTriangle" :size="16" aria-hidden="true" />
        <div>
          <div class="conn-banner__title">{{ fetchModelsResult.ok ? "已获取" : "获取失败" }}</div>
          <div class="conn-banner__hint">
            {{ fetchModelsResult.ok ? `已添加 ${fetchModelsResult.models.length} 个模型。` : fetchModelsResult.error }}
          </div>
        </div>
      </div>

      <div
        v-if="assistantAIResult"
        class="conn-banner"
        :class="assistantAIResult.ok ? 'conn-banner--ok' : 'conn-banner--err'"
      >
        <component :is="assistantAIResult.ok ? Plug : AlertTriangle" :size="16" aria-hidden="true" />
        <div>
          <div class="conn-banner__title">{{ assistantAIResult.ok ? "可达" : "不可达" }}</div>
          <div class="conn-banner__hint">{{ assistantAIBannerHint }}</div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.assistant-ai-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 10px;
}

.assistant-ai-card__header h2 {
  margin: 0;
}

.assistant-ai-provider-list {
  display: grid;
  gap: 8px;
}

.assistant-ai-provider-row {
  display: grid;
  grid-template-columns: 28px minmax(0, 1fr) auto auto;
  width: 100%;
  min-height: 44px;
  padding: 8px 10px;
  border: 1px solid var(--border-soft);
  border-radius: 8px;
  background: var(--surface-soft);
  color: inherit;
  text-align: left;
  align-items: center;
  gap: 12px;
  cursor: pointer;
}

.assistant-ai-provider-row:hover {
  border-color: var(--border-strong);
}

.assistant-ai-provider-row__avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: 6px;
  border: 1px solid var(--border-soft);
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 700;
}

.assistant-ai-provider-row__body {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
}

.assistant-ai-provider-row__title {
  font-weight: 650;
}

.assistant-ai-provider-row__meta {
  color: var(--text-muted);
  font-size: 12px;
  white-space: nowrap;
}

.assistant-ai-provider-row__status,
.assistant-ai-provider-row__action {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.assistant-ai-provider-row__status {
  color: var(--text-muted);
  white-space: nowrap;
}

.assistant-ai-provider-row__action {
  color: var(--accent);
  font-size: 12px;
  white-space: nowrap;
}

.assistant-ai-model-toolbar .settings-row__control {
  flex-wrap: wrap;
}

.assistant-ai-add-row__fields {
  display: grid;
  grid-template-columns: minmax(150px, 1fr) minmax(150px, 1fr) auto auto;
  gap: 8px;
  align-items: center;
}

.assistant-ai-add-row__fields .ui-input {
  width: 100%;
}

.assistant-ai-empty {
  margin: 10px 0 12px;
  font-size: 13px;
}

@media (max-width: 760px) {
  .assistant-ai-card__header {
    align-items: stretch;
    flex-direction: column;
  }

  .assistant-ai-card__header .ui-button {
    align-self: flex-start;
  }

  .assistant-ai-add-row__fields {
    grid-template-columns: 1fr;
  }

  .assistant-ai-provider-row {
    grid-template-columns: 28px minmax(0, 1fr);
  }

  .assistant-ai-provider-row__status,
  .assistant-ai-provider-row__action {
    grid-column: 2;
  }
}
</style>


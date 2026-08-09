<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { RouterLink } from "vue-router";
import AlertTriangle from "@lucide/vue/dist/esm/icons/triangle-alert.mjs";
import KeyRound from "@lucide/vue/dist/esm/icons/key-round.mjs";
import Loader2 from "@lucide/vue/dist/esm/icons/loader-circle.mjs";
import Network from "@lucide/vue/dist/esm/icons/network.mjs";
import Pencil from "@lucide/vue/dist/esm/icons/pencil.mjs";
import RotateCw from "@lucide/vue/dist/esm/icons/rotate-cw.mjs";
import Save from "@lucide/vue/dist/esm/icons/save.mjs";
import Trash2 from "@lucide/vue/dist/esm/icons/trash-2.mjs";
import X from "@lucide/vue/dist/esm/icons/x.mjs";
import {
  API_KEY_ENV_BY_BACKEND,
  CHAT_BACKENDS,
  DIRECT_DEFAULT_URLS,
  apiDescriptionForBackend,
  chatBackendLabel,
  connectionDiagnostic,
  createChatBackendRecord,
  defaultRouterModeForBackend,
  normalizeRouterModeForBackend,
  routerModeUsesApiConfig,
  runtimeDiagnostic,
  type ChatBackendKind,
  type ProviderConfig,
  type RouterMode,
} from "@lilia/contracts";
import { useConnectionStatus } from "../../composables/useConnectionStatus";
import {
  getProviderConfig,
  getRouterMode,
  setProviderConfig,
  setRouterMode,
} from "../../services/chat";

const {
  report,
  activeBackend,
  setActiveBackend,
  statusFor,
  probing,
  refresh,
} = useConnectionStatus();

const backendOptions: { value: ChatBackendKind; label: string }[] = CHAT_BACKENDS.map((backend) => ({
  value: backend,
  label: chatBackendLabel(backend),
}));

function emptyProviderConfig(backend: ChatBackendKind): ProviderConfig {
  return { backend, baseUrl: null, apiKey: null, hasApiKey: false };
}

function providerConfigMapFromBackends(): Record<ChatBackendKind, ProviderConfig> {
  return createChatBackendRecord(emptyProviderConfig);
}

function routerModeMapFromBackends(): Record<ChatBackendKind, RouterMode> {
  return createChatBackendRecord(defaultRouterModeForBackend);
}

const switchingBackend = ref<ChatBackendKind | null>(null);
const savingProvider = ref(false);
const editingProvider = ref(false);
const providerForms = ref<Record<ChatBackendKind, ProviderConfig>>(providerConfigMapFromBackends());
const routerModes = ref<Record<ChatBackendKind, RouterMode>>(routerModeMapFromBackends());

const selectedBackend = computed(() => activeBackend.value);
const selectedStatus = computed(() => statusFor(selectedBackend.value));
const selectedRouterMode = computed(() => routerModes.value[selectedBackend.value]);
const selectedProviderForm = computed(() => providerForms.value[selectedBackend.value]);
const selectedRuntime = computed(() => runtimeDiagnostic(selectedBackend.value, report.value));
const selectedConnection = computed(() =>
  connectionDiagnostic(
    selectedBackend.value,
    selectedStatus.value,
    selectedRouterMode.value,
  ),
);
const selectedDiagnostic = computed(() => {
  if (probing.value) {
    return { tone: "probing" as const, title: "检查中", hint: "正在读取本机运行时和连接配置。" };
  }
  const runtime = selectedRuntime.value;
  if (runtime) return runtime;
  return selectedConnection.value;
});
const apiDefaultUrl = computed(() => DIRECT_DEFAULT_URLS[selectedBackend.value]);
const apiKeyEnv = computed(() => API_KEY_ENV_BY_BACKEND[selectedBackend.value]);
const apiDescription = computed(() => apiDescriptionForBackend(selectedBackend.value));
const showApiConfig = computed(() => routerModeUsesApiConfig(selectedRouterMode.value));
const providerConfigState = computed(() =>
  selectedProviderForm.value.hasApiKey ? "密钥已保存" : "未保存密钥"
);
let disposed = false;

async function loadProvider(backend: ChatBackendKind) {
  try {
    const config = await getProviderConfig(backend);
    if (disposed) return;
    providerForms.value = {
      ...providerForms.value,
      [backend]: { ...config, apiKey: null },
    };
  } catch (err) {
    console.error("[settings] load provider config failed", err);
  }
}

async function loadRouter(backend: ChatBackendKind) {
  try {
    const mode = await getRouterMode(backend);
    if (disposed) return;
    routerModes.value = {
      ...routerModes.value,
      [backend]: normalizeRouterModeForBackend(backend, mode),
    };
  } catch (err) {
    console.error("[settings] load router mode failed", err);
  }
}

async function loadAllConfig() {
  await Promise.all(CHAT_BACKENDS.flatMap((backend) => [
    loadProvider(backend),
    loadRouter(backend),
  ]));
}

function normalizedProviderConfig(clearApiKey = false): ProviderConfig {
  const form = selectedProviderForm.value;
  return {
    backend: selectedBackend.value,
    baseUrl: form.baseUrl?.trim() || null,
    apiKey: form.apiKey?.trim() || null,
    hasApiKey: form.hasApiKey,
    clearApiKey,
  };
}

async function saveProvider() {
  if (disposed) return;
  const backend = selectedBackend.value;
  savingProvider.value = true;
  try {
    await setProviderConfig(normalizedProviderConfig(false));
    if (disposed) return;
    await loadProvider(backend);
    if (disposed) return;
    await refresh();
    editingProvider.value = false;
  } catch (err) {
    console.error("[settings] save provider config failed", err);
  } finally {
    if (!disposed) savingProvider.value = false;
  }
}

async function clearProviderKey() {
  if (disposed) return;
  const backend = selectedBackend.value;
  savingProvider.value = true;
  try {
    await setProviderConfig({ ...normalizedProviderConfig(true), apiKey: null, clearApiKey: true });
    if (disposed) return;
    providerForms.value[backend].apiKey = null;
    await loadProvider(backend);
    if (disposed) return;
    await refresh();
  } catch (err) {
    console.error("[settings] clear provider key failed", err);
  } finally {
    if (!disposed) savingProvider.value = false;
  }
}

function startProviderEdit() {
  editingProvider.value = true;
}

async function cancelProviderEdit() {
  if (disposed) return;
  const backend = selectedBackend.value;
  editingProvider.value = false;
  await loadProvider(backend);
}

async function ensureApiRouterMode() {
  if (disposed) return;
  await Promise.all(CHAT_BACKENDS.map(async (backend) => {
    const defaultMode = defaultRouterModeForBackend(backend);
    if (routerModes.value[backend] === defaultMode) return;
    routerModes.value = { ...routerModes.value, [backend]: defaultMode };
    try {
      await setRouterMode(backend, defaultMode);
    } catch (err) {
      console.error("[settings] set API router mode failed", err);
    }
  }));
}

async function probe() {
  if (disposed) return;
  await refresh();
}

async function selectBackend(backend: ChatBackendKind) {
  if (disposed || switchingBackend.value) return;
  switchingBackend.value = backend;
  editingProvider.value = false;
  try {
    await setActiveBackend(backend);
    if (disposed) return;
    await Promise.all([loadProvider(backend), loadRouter(backend), refresh()]);
  } catch (err) {
    console.error("[settings] setActiveBackend failed", err);
  } finally {
    if (!disposed) switchingBackend.value = null;
  }
}

onMounted(async () => {
  disposed = false;
  await Promise.all([loadAllConfig(), refresh()]);
  if (disposed) return;
  await ensureApiRouterMode();
});

onBeforeUnmount(() => {
  disposed = true;
});
</script>

<template>
  <div class="card">
    <h2>
      <span class="card-h2__title">
        <Network :size="14" aria-hidden="true" />
        连接
      </span>
    </h2>

    <div class="settings-row">
      <div class="settings-row__label">使用</div>
      <div class="ui-segmented" role="radiogroup" aria-label="对话后端">
        <button
          v-for="opt in backendOptions"
          :key="opt.value"
          type="button"
          role="radio"
          :aria-checked="selectedBackend === opt.value"
          :data-agent-id="`settings.provider.backend.${opt.value}`"
          :class="{ 'is-active': selectedBackend === opt.value }"
          :disabled="switchingBackend !== null"
          @click="selectBackend(opt.value)"
        >
          {{ opt.label }}
        </button>
      </div>
    </div>

    <div class="settings-row settings-row--stacked">
      <div class="settings-row__label">接入说明</div>
      <div class="settings-row__status muted">
        此处配置 <strong>LLM Provider 连接</strong>（端点 / API Key），与执行后端
        <code>native-agentkit</code>（Mutsuki）正交。模型目录与角色预设在「模型」设置中管理。
        OpenAI / Anthropic 等凭据也可到
        <RouterLink
          class="inline-link"
          to="/settings?tab=credentials"
          data-agent-id="settings.provider.open-credentials"
        >
          凭据
        </RouterLink>
        配置；官方 Claude Code / Codex 产品路径已移除。
      </div>
    </div>

    <div class="settings-row">
      <div class="settings-row__label">接入方式</div>
      <div class="settings-row__control settings-row__control--loose">
        <span class="settings-row__status-text muted">API</span>
      </div>
    </div>

    <template v-if="showApiConfig">
      <div class="settings-row">
        <div class="settings-row__label">API 配置</div>
        <div class="settings-row__control settings-row__control--loose">
          <span class="provider-config-status muted">
            <KeyRound :size="12" aria-hidden="true" />
            {{ providerConfigState }}
          </span>
          <button
            type="button"
            class="ui-button ui-button--ghost"
            data-agent-id="settings.provider.edit"
            :disabled="savingProvider"
            @click="startProviderEdit"
          >
            <Pencil :size="12" aria-hidden="true" />
            {{ selectedProviderForm.hasApiKey || selectedProviderForm.baseUrl ? "编辑" : "新建配置" }}
          </button>
        </div>
      </div>

      <template v-if="editingProvider">
        <div class="settings-row settings-row--stacked">
          <div class="settings-row__label">API 来源</div>
          <div class="settings-row__status muted">
            {{ apiDescription }} 默认 URL：{{ apiDefaultUrl }}
          </div>
        </div>

        <div class="settings-row">
          <div class="settings-row__label">Base URL</div>
          <input
            type="text"
            class="ui-input"
            :placeholder="apiDefaultUrl"
            data-agent-id="settings.provider.base-url"
            :value="selectedProviderForm.baseUrl ?? ''"
            @input="(e) => (selectedProviderForm.baseUrl = (e.target as HTMLInputElement).value)"
          />
        </div>

        <div class="settings-row">
          <div class="settings-row__label">API key</div>
          <div class="settings-row__control">
            <input
              type="password"
              class="ui-input"
              :placeholder="selectedProviderForm.hasApiKey ? '已保存，留空保留现有值' : apiKeyEnv"
              data-agent-id="settings.provider.api-key"
              :value="selectedProviderForm.apiKey ?? ''"
              @input="(e) => (selectedProviderForm.apiKey = (e.target as HTMLInputElement).value)"
            />
            <button
              type="button"
              class="ui-button ui-button--ghost"
              data-agent-id="settings.provider.clear-key"
              :disabled="savingProvider || !selectedProviderForm.hasApiKey"
              title="清除已保存的 API key"
              @click="clearProviderKey"
            >
              <Trash2 :size="12" aria-hidden="true" />
              清除
            </button>
          </div>
        </div>

        <div class="settings-row">
          <div class="settings-row__label">配置操作</div>
          <div class="settings-row__control">
            <span class="provider-config-status muted">
              <KeyRound :size="12" aria-hidden="true" />
              {{ providerConfigState }}
            </span>
            <button
              type="button"
              class="ui-button ui-button--ghost"
              data-agent-id="settings.provider.save"
              :disabled="savingProvider"
              @click="saveProvider"
            >
              <Save :size="12" aria-hidden="true" />
              {{ savingProvider ? "保存中..." : "保存" }}
            </button>
            <button
              type="button"
              class="ui-button ui-button--ghost"
              data-agent-id="settings.provider.cancel-edit"
              :disabled="savingProvider"
              @click="cancelProviderEdit"
            >
              <X :size="12" aria-hidden="true" />
              取消
            </button>
          </div>
        </div>
      </template>
    </template>

    <div
      v-if="selectedDiagnostic"
      class="conn-banner"
      :class="`conn-banner--${selectedDiagnostic.tone}`"
    >
      <Loader2
        v-if="selectedDiagnostic.tone === 'probing'"
        :size="14"
        class="is-spinning"
        aria-hidden="true"
      />
      <AlertTriangle
        v-else
        :size="16"
        aria-hidden="true"
      />
      <div>
        <div class="conn-banner__title">{{ selectedDiagnostic.title }}</div>
        <div class="conn-banner__hint">
          {{ selectedDiagnostic.hint }}
          <button type="button" class="inline-link" data-agent-id="settings.provider.retry-probe" :disabled="probing" @click="probe">
            <RotateCw :size="11" aria-hidden="true" />
            重新检测
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.provider-config-status {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
</style>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import KeyRound from "@lucide/vue/dist/esm/icons/key-round.mjs";
import Loader2 from "@lucide/vue/dist/esm/icons/loader-circle.mjs";
import LogIn from "@lucide/vue/dist/esm/icons/log-in.mjs";
import RefreshCw from "@lucide/vue/dist/esm/icons/refresh-cw.mjs";
import Trash2 from "@lucide/vue/dist/esm/icons/trash-2.mjs";
import Upload from "@lucide/vue/dist/esm/icons/upload.mjs";
import {
  listNativeCredentialProviders,
  nativeCredentialDiagnostics,
  nativeCredentialImport,
  nativeCredentialLogin,
  nativeCredentialRevoke,
  type NativeCredentialDescriptorView,
  type NativeCredentialKind,
  type NativeCredentialProvider,
  type NativeIndependentDiagnostics,
} from "../../services/nativeAgent";

const PROVIDER_LABELS: Record<string, string> = {
  "mutsuki.credential.openai": "OpenAI",
  "mutsuki.credential.anthropic": "Anthropic",
};

const KIND_LABELS: Record<NativeCredentialKind, string> = {
  api_key: "API Key",
  oauth_grant: "OAuth",
  generated_api_key: "官方生成 Key",
  cloud_identity: "云身份",
};

const STATUS_LABELS: Record<string, string> = {
  active: "可用",
  expired: "已过期",
  revoked: "已撤销",
  insufficient_scope: "权限不足",
  account_disabled: "账号停用",
  unsupported_for_custom_runtime: "不支持自定义运行时",
  pending_refresh: "刷新中",
};

const providers = ref<NativeCredentialProvider[]>([]);
const diagnostics = ref<NativeIndependentDiagnostics | null>(null);
const loading = ref(false);
const submitting = ref(false);
const errorText = ref("");
const mode = ref<"login" | "import">("login");
const providerId = ref("");
const kind = ref<NativeCredentialKind>("api_key");
const secretMaterial = ref("");
const accountLabel = ref("");
let disposed = false;

const credentials = computed(() => diagnostics.value?.credential.credentials ?? []);
const selectedProvider = computed(
  () => providers.value.find((item) => item.providerId === providerId.value) ?? null,
);
const kindOptions = computed(() => {
  const supported = selectedProvider.value?.supportedKinds ?? ["api_key"];
  return supported.map((value) => ({
    value,
    label: KIND_LABELS[value] ?? value,
  }));
});
const canSubmit = computed(
  () => Boolean(providerId.value) && secretMaterial.value.trim().length > 0 && !submitting.value,
);
const credentialHealth = computed(() => diagnostics.value?.credential ?? null);
const runtimeReady = computed(() => diagnostics.value?.runtimeReady ?? false);
const liveAdapter = computed(() => diagnostics.value?.liveModelAdapterDrivesTurn ?? false);
const profileBound = computed(() => diagnostics.value?.profileHasCredentialRefs ?? false);
const profileId = computed(() => diagnostics.value?.profileId ?? null);

function providerLabel(id: string): string {
  return PROVIDER_LABELS[id] ?? id;
}

function statusLabel(status: string): string {
  return STATUS_LABELS[status] ?? status;
}

function syncProviderDefaults() {
  if (!providers.value.length) {
    providerId.value = "";
    return;
  }
  if (!providers.value.some((item) => item.providerId === providerId.value)) {
    providerId.value = providers.value[0]!.providerId;
  }
  const supported = selectedProvider.value?.supportedKinds ?? ["api_key"];
  if (!supported.includes(kind.value)) {
    kind.value = supported[0] ?? "api_key";
  }
}

async function refresh() {
  if (disposed) return;
  loading.value = true;
  errorText.value = "";
  try {
    const [nextProviders, nextDiagnostics] = await Promise.all([
      listNativeCredentialProviders(),
      nativeCredentialDiagnostics(),
    ]);
    if (disposed) return;
    providers.value = Array.isArray(nextProviders) ? nextProviders : [];
    diagnostics.value = nextDiagnostics;
    syncProviderDefaults();
  } catch (err) {
    if (!disposed) errorText.value = err instanceof Error ? err.message : String(err);
  } finally {
    if (!disposed) loading.value = false;
  }
}

function clearSecret() {
  secretMaterial.value = "";
}

async function submitCredential() {
  if (!canSubmit.value) return;
  submitting.value = true;
  errorText.value = "";
  const input = {
    providerId: providerId.value,
    kind: kind.value,
    secretMaterial: secretMaterial.value.trim(),
    accountLabel: accountLabel.value.trim() || null,
    source: mode.value === "import" ? "official-login-generated" : "settings-login",
  };
  try {
    if (mode.value === "import") {
      await nativeCredentialImport(input);
    } else {
      await nativeCredentialLogin(input);
    }
    clearSecret();
    await refresh();
  } catch (err) {
    errorText.value = err instanceof Error ? err.message : String(err);
  } finally {
    submitting.value = false;
  }
}

async function revokeCredential(item: NativeCredentialDescriptorView) {
  submitting.value = true;
  errorText.value = "";
  try {
    await nativeCredentialRevoke(item.credentialId, item.revision, "settings-revoke");
    await refresh();
  } catch (err) {
    errorText.value = err instanceof Error ? err.message : String(err);
  } finally {
    submitting.value = false;
  }
}

onMounted(() => {
  void refresh();
});

onBeforeUnmount(() => {
  disposed = true;
  clearSecret();
});
</script>

<template>
  <div class="native-credential-settings" data-agent-id="settings.credentials">
    <section class="card">
      <div class="settings-section-header">
        <div>
          <h2>凭据</h2>
          <p class="muted">官方登录与 API Key 仅进入 Credential Broker，不写入 Provider 明文配置。</p>
        </div>
        <button
          type="button"
          class="ui-button ui-button--ghost"
          data-agent-id="settings.credentials.refresh"
          :disabled="loading"
          aria-label="刷新凭据状态"
          @click="refresh"
        >
          <Loader2 v-if="loading" :size="12" class="spin" aria-hidden="true" />
          <RefreshCw v-else :size="12" aria-hidden="true" />
          刷新
        </button>
      </div>

      <div v-if="errorText" class="settings-banner settings-banner--err" role="alert">
        {{ errorText }}
      </div>

      <div class="settings-row settings-row--stacked">
        <div class="settings-row__label">诊断</div>
        <ul class="kv native-credential-diagnostics" data-agent-id="settings.credentials.diagnostics">
          <li>
            <span>Broker</span>
            <span>{{ credentialHealth?.brokerReady ? "就绪" : "未就绪" }}</span>
          </li>
          <li>
            <span>可用凭据</span>
            <span>{{ credentialHealth?.activeCount ?? 0 }} / {{ credentialHealth?.credentialCount ?? 0 }}</span>
          </li>
          <li>
            <span>Runtime</span>
            <span>{{ runtimeReady ? "就绪" : "未就绪" }}</span>
          </li>
          <li>
            <span>Profile 绑定</span>
            <span>{{ profileBound ? "已绑定" : "未绑定" }}</span>
          </li>
          <li>
            <span>Profile</span>
            <span data-agent-id="settings.credentials.profile-id">{{ profileId ?? "—" }}</span>
          </li>
          <li>
            <span>Live Adapter</span>
            <span>{{ liveAdapter ? "驱动 Turn" : "参考路径" }}</span>
          </li>
          <li>
            <span>独立诊断</span>
            <span>{{ diagnostics?.credentialAndRuntimeIndependent ? "是" : "否" }}</span>
          </li>
        </ul>
      </div>
    </section>

    <section class="card">
      <h2>{{ mode === "import" ? "导入官方生成 Key" : "登录 / 保存 API Key" }}</h2>
      <div class="settings-row">
        <div class="settings-row__label">方式</div>
        <div class="settings-row__control native-credential-mode">
          <button
            type="button"
            class="ui-button ui-button--ghost"
            data-agent-id="settings.credentials.mode.login"
            :class="{ 'is-active': mode === 'login' }"
            @click="mode = 'login'"
          >
            <LogIn :size="12" aria-hidden="true" />
            登录
          </button>
          <button
            type="button"
            class="ui-button ui-button--ghost"
            data-agent-id="settings.credentials.mode.import"
            :class="{ 'is-active': mode === 'import' }"
            @click="mode = 'import'"
          >
            <Upload :size="12" aria-hidden="true" />
            导入
          </button>
        </div>
      </div>

      <div class="settings-row">
        <div class="settings-row__label">Provider</div>
        <div class="settings-row__control">
          <select
            v-model="providerId"
            class="ui-input"
            data-agent-id="settings.credentials.provider"
            aria-label="凭据 Provider"
            @change="syncProviderDefaults"
          >
            <option v-for="item in providers" :key="item.providerId" :value="item.providerId">
              {{ item.displayName || providerLabel(item.providerId) }}
            </option>
          </select>
        </div>
      </div>

      <div class="settings-row">
        <div class="settings-row__label">类型</div>
        <div class="settings-row__control">
          <select
            v-model="kind"
            class="ui-input"
            data-agent-id="settings.credentials.kind"
            aria-label="凭据类型"
          >
            <option v-for="option in kindOptions" :key="option.value" :value="option.value">
              {{ option.label }}
            </option>
          </select>
        </div>
      </div>

      <div class="settings-row">
        <div class="settings-row__label">账号备注</div>
        <div class="settings-row__control">
          <input
            v-model="accountLabel"
            type="text"
            class="ui-input"
            data-agent-id="settings.credentials.account-label"
            placeholder="可选"
          />
        </div>
      </div>

      <div class="settings-row settings-row--stacked">
        <div class="settings-row__label">
          <KeyRound :size="12" aria-hidden="true" />
          API Key
        </div>
        <div class="settings-row__control">
          <input
            v-model="secretMaterial"
            type="password"
            class="ui-input"
            autocomplete="off"
            data-agent-id="settings.credentials.secret"
            :placeholder="mode === 'import' ? '粘贴官方流程生成的 Key' : '粘贴 Console API Key'"
            @keydown.enter.prevent="submitCredential"
          />
          <button
            type="button"
            class="ui-button"
            data-agent-id="settings.credentials.submit"
            :disabled="!canSubmit"
            @click="submitCredential"
          >
            <Loader2 v-if="submitting" :size="12" class="spin" aria-hidden="true" />
            <template v-else>{{ mode === "import" ? "导入" : "保存" }}</template>
          </button>
        </div>
      </div>
    </section>

    <section class="card">
      <h2>已保存凭据</h2>
      <p v-if="!credentials.length" class="muted" data-agent-id="settings.credentials.empty">
        尚未保存可用凭据。
      </p>
      <ul v-else class="native-credential-list" data-agent-id="settings.credentials.list">
        <li v-for="item in credentials" :key="`${item.credentialId}:${item.revision}`" class="native-credential-item">
          <div class="native-credential-item__body">
            <div class="native-credential-item__title">
              {{ providerLabel(item.providerId) }}
              <span class="muted">· {{ KIND_LABELS[item.kind] ?? item.kind }}</span>
            </div>
            <div class="native-credential-item__meta muted">
              {{ statusLabel(item.status) }}
              <template v-if="item.accountLabel"> · {{ item.accountLabel }}</template>
              · rev {{ item.revision }}
            </div>
          </div>
          <button
            type="button"
            class="ui-button ui-button--ghost"
            :data-agent-id="`settings.credentials.revoke.${item.credentialId}`"
            :disabled="submitting || item.status === 'revoked'"
            aria-label="撤销凭据"
            @click="revokeCredential(item)"
          >
            <Trash2 :size="12" aria-hidden="true" />
            撤销
          </button>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped>
.native-credential-settings {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.settings-section-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 8px;
}

.settings-section-header h2,
.card > h2 {
  margin: 0 0 4px;
  font-size: 15px;
  font-weight: 600;
}

.native-credential-diagnostics {
  margin: 0;
}

.native-credential-mode {
  display: flex;
  gap: 6px;
}

.native-credential-mode .is-active {
  border-color: var(--accent);
  color: var(--accent-text);
  background: var(--accent-soft);
}

.native-credential-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.native-credential-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-subtle);
}

.native-credential-item__title {
  font-size: 13px;
  font-weight: 550;
}

.native-credential-item__meta {
  margin-top: 2px;
  font-size: 12px;
}

.spin {
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

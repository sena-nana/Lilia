<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import Loader2 from "@lucide/vue/dist/esm/icons/loader-circle.mjs";
import RefreshCw from "@lucide/vue/dist/esm/icons/refresh-cw.mjs";
import type { ChatSidebarContext } from "../../composables/useChatSidebar";
import {
  getNativeSharedCodingServicesStatus,
  getNativeSharedGitStatus,
  getNativeSharedLspStatus,
  listNativeSharedMcpServers,
  queryNativeSharedMemory,
  searchNativeSharedCodeIndex,
  writeNativeSharedMemory,
  type NativeSharedCodingServicesStatus,
} from "../../services/nativeAgent";

const props = defineProps<ChatSidebarContext>();

type ServiceTab = "git" | "index" | "lsp" | "mcp" | "memory";

const tabs: { id: ServiceTab; label: string }[] = [
  { id: "git", label: "Git" },
  { id: "index", label: "Index" },
  { id: "lsp", label: "LSP" },
  { id: "mcp", label: "MCP" },
  { id: "memory", label: "Memory" },
];

const activeTab = ref<ServiceTab>("git");
const loading = ref(false);
const acting = ref(false);
const errorText = ref("");
const status = ref<NativeSharedCodingServicesStatus | null>(null);
const resultText = ref("");
const gitPath = ref("");
const indexQuery = ref("shared_marker");
const memoryQuery = ref("");
const memoryText = ref("");
const memoryNamespace = ref("lilia.product");
const memoryScopeId = ref("");
const mcpServers = ref<unknown[]>([]);
const lspWorkspaces = ref(0);
let disposed = false;

const dataSource = computed(() => status.value?.dataSource ?? "…");
const sharedOk = computed(
  () =>
    Boolean(status.value?.sharedIdentityOk) &&
    status.value?.officialAgentServer === false &&
    Boolean(status.value?.gitSameInstance) &&
    Boolean(status.value?.codeIndexSameInstance) &&
    Boolean(status.value?.lspSameInstance) &&
    Boolean(status.value?.mcpSameInstance) &&
    Boolean(status.value?.memorySharedRouter),
);

function pretty(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function syncDefaultsFromContext() {
  if (!gitPath.value.trim() && props.projectCwd) {
    gitPath.value = props.projectCwd;
  }
  if (!memoryScopeId.value.trim()) {
    memoryScopeId.value = props.projectId?.trim() || props.taskId || "default";
  }
}

async function refreshInventory() {
  if (disposed) return;
  loading.value = true;
  errorText.value = "";
  try {
    const [next, servers, lsp] = await Promise.all([
      getNativeSharedCodingServicesStatus(),
      listNativeSharedMcpServers(),
      getNativeSharedLspStatus(),
    ]);
    if (disposed) return;
    status.value = next;
    mcpServers.value = Array.isArray(servers) ? servers : [];
    lspWorkspaces.value = lsp.activeWorkspaces ?? next.lspActiveWorkspaces;
    if (activeTab.value === "mcp") {
      resultText.value = pretty(mcpServers.value);
    } else if (activeTab.value === "lsp") {
      resultText.value = pretty(lsp);
    }
  } catch (err) {
    if (!disposed) errorText.value = err instanceof Error ? err.message : String(err);
  } finally {
    if (!disposed) loading.value = false;
  }
}

async function runGitStatus() {
  if (disposed || acting.value) return;
  const path = gitPath.value.trim();
  if (!path) {
    errorText.value = "请填写仓库路径";
    return;
  }
  acting.value = true;
  errorText.value = "";
  try {
    const value = await getNativeSharedGitStatus(path);
    if (!disposed) resultText.value = pretty(value);
  } catch (err) {
    if (!disposed) errorText.value = err instanceof Error ? err.message : String(err);
  } finally {
    if (!disposed) acting.value = false;
  }
}

async function runIndexSearch() {
  if (disposed || acting.value) return;
  const root = gitPath.value.trim() || props.projectCwd?.trim() || "/tmp/lilia-shared-index";
  const query = indexQuery.value.trim();
  if (!query) {
    errorText.value = "请填写检索词";
    return;
  }
  acting.value = true;
  errorText.value = "";
  try {
    const value = await searchNativeSharedCodeIndex({
      workspaceId: props.projectId?.trim() || "ws-shared",
      root,
      relativePath: "src/shared_probe.rs",
      content: `pub fn ${query.replace(/\W+/g, "_") || "shared_marker"}() {}\n`,
      query,
    });
    if (!disposed) resultText.value = pretty(value);
  } catch (err) {
    if (!disposed) errorText.value = err instanceof Error ? err.message : String(err);
  } finally {
    if (!disposed) acting.value = false;
  }
}

async function runMemoryQuery() {
  if (disposed || acting.value) return;
  const query = memoryQuery.value.trim();
  if (!query) {
    errorText.value = "请填写 Memory 查询";
    return;
  }
  acting.value = true;
  errorText.value = "";
  try {
    const value = await queryNativeSharedMemory({
      query,
      namespace: memoryNamespace.value.trim() || null,
      scopeId: memoryScopeId.value.trim() || null,
      limit: 16,
    });
    if (!disposed) resultText.value = pretty(value);
  } catch (err) {
    if (!disposed) errorText.value = err instanceof Error ? err.message : String(err);
  } finally {
    if (!disposed) acting.value = false;
  }
}

async function runMemoryWrite() {
  if (disposed || acting.value) return;
  const text = memoryText.value.trim();
  if (!text) {
    errorText.value = "请填写要写入的 Memory 文本";
    return;
  }
  acting.value = true;
  errorText.value = "";
  try {
    const value = await writeNativeSharedMemory({
      text,
      namespace: memoryNamespace.value.trim() || null,
      scopeId: memoryScopeId.value.trim() || null,
    });
    if (!disposed) {
      resultText.value = pretty(value);
      memoryQuery.value = text.slice(0, 48);
    }
  } catch (err) {
    if (!disposed) errorText.value = err instanceof Error ? err.message : String(err);
  } finally {
    if (!disposed) acting.value = false;
  }
}

async function onTabAction() {
  if (activeTab.value === "git") return runGitStatus();
  if (activeTab.value === "index") return runIndexSearch();
  if (activeTab.value === "memory") return runMemoryQuery();
  return refreshInventory();
}

watch(
  () => [props.projectCwd, props.projectId, props.taskId] as const,
  () => {
    syncDefaultsFromContext();
  },
  { immediate: true },
);

watch(activeTab, (tab) => {
  errorText.value = "";
  if (tab === "mcp") resultText.value = pretty(mcpServers.value);
  else if (tab === "lsp") {
    resultText.value = pretty({
      serviceId: status.value?.lspServiceId,
      activeWorkspaces: lspWorkspaces.value,
      dataSource: status.value?.dataSource,
    });
  } else if (!resultText.value) {
    resultText.value = "";
  }
});

onMounted(() => {
  syncDefaultsFromContext();
  void refreshInventory();
});

onBeforeUnmount(() => {
  disposed = true;
});
</script>

<template>
  <div class="shared-panel" data-agent-id="chat.shared-services">
    <header class="shared-panel__head">
      <div>
        <p class="shared-panel__title">共享 Services</p>
        <p class="shared-panel__meta" data-agent-id="chat.shared-services.source">
          {{ dataSource }}
          <span :class="sharedOk ? 'ok' : 'warn'">{{ sharedOk ? "单实例" : "未就绪" }}</span>
        </p>
      </div>
      <button
        type="button"
        class="shared-panel__icon-btn"
        data-agent-id="chat.shared-services.refresh"
        :disabled="loading || acting"
        title="刷新清单"
        @click="refreshInventory"
      >
        <Loader2 v-if="loading" :size="14" class="spin" aria-hidden="true" />
        <RefreshCw v-else :size="14" aria-hidden="true" />
      </button>
    </header>

    <div class="shared-panel__tabs" role="tablist" aria-label="共享 Services">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        type="button"
        role="tab"
        class="shared-panel__tab"
        :class="{ 'is-active': activeTab === tab.id }"
        :aria-selected="activeTab === tab.id"
        :data-agent-id="`chat.shared-services.tab.${tab.id}`"
        @click="activeTab = tab.id"
      >
        {{ tab.label }}
      </button>
    </div>

    <p v-if="errorText" class="shared-panel__error" role="alert">{{ errorText }}</p>

    <section v-if="activeTab === 'git'" class="shared-panel__form">
      <label>
        仓库路径
        <input
          v-model="gitPath"
          type="text"
          spellcheck="false"
          data-agent-id="chat.shared-services.git.path"
          placeholder="/path/to/repo"
        />
      </label>
      <button
        type="button"
        class="shared-panel__action"
        data-agent-id="chat.shared-services.git.status"
        :disabled="acting"
        @click="runGitStatus"
      >
        读取 Git Status
      </button>
    </section>

    <section v-else-if="activeTab === 'index'" class="shared-panel__form">
      <label>
        工作区根（默认项目目录）
        <input
          v-model="gitPath"
          type="text"
          spellcheck="false"
          data-agent-id="chat.shared-services.index.root"
        />
      </label>
      <label>
        检索词
        <input
          v-model="indexQuery"
          type="text"
          spellcheck="false"
          data-agent-id="chat.shared-services.index.query"
        />
      </label>
      <button
        type="button"
        class="shared-panel__action"
        data-agent-id="chat.shared-services.index.search"
        :disabled="acting"
        @click="runIndexSearch"
      >
        搜索 Code Index
      </button>
    </section>

    <section v-else-if="activeTab === 'lsp'" class="shared-panel__form">
      <p class="shared-panel__hint">
        活跃 workspace：{{ lspWorkspaces }}（不启动第二个 language server）
      </p>
      <button
        type="button"
        class="shared-panel__action"
        data-agent-id="chat.shared-services.lsp.refresh"
        :disabled="acting || loading"
        @click="onTabAction"
      >
        刷新 LSP 状态
      </button>
    </section>

    <section v-else-if="activeTab === 'mcp'" class="shared-panel__form">
      <p class="shared-panel__hint">已注册 server：{{ mcpServers.length }}</p>
      <button
        type="button"
        class="shared-panel__action"
        data-agent-id="chat.shared-services.mcp.refresh"
        :disabled="acting || loading"
        @click="onTabAction"
      >
        刷新 MCP 列表
      </button>
    </section>

    <section v-else class="shared-panel__form">
      <label>
        Namespace
        <input
          v-model="memoryNamespace"
          type="text"
          spellcheck="false"
          data-agent-id="chat.shared-services.memory.namespace"
        />
      </label>
      <label>
        Scope
        <input
          v-model="memoryScopeId"
          type="text"
          spellcheck="false"
          data-agent-id="chat.shared-services.memory.scope"
        />
      </label>
      <label>
        查询
        <input
          v-model="memoryQuery"
          type="text"
          data-agent-id="chat.shared-services.memory.query"
        />
      </label>
      <label>
        写入文本
        <textarea
          v-model="memoryText"
          rows="3"
          data-agent-id="chat.shared-services.memory.text"
        />
      </label>
      <div class="shared-panel__actions">
        <button
          type="button"
          class="shared-panel__action"
          data-agent-id="chat.shared-services.memory.query-btn"
          :disabled="acting"
          @click="runMemoryQuery"
        >
          查询
        </button>
        <button
          type="button"
          class="shared-panel__action shared-panel__action--secondary"
          data-agent-id="chat.shared-services.memory.write-btn"
          :disabled="acting"
          @click="runMemoryWrite"
        >
          写入
        </button>
      </div>
    </section>

    <pre
      v-if="resultText"
      class="shared-panel__result"
      data-agent-id="chat.shared-services.result"
    >{{ resultText }}</pre>
  </div>
</template>

<style scoped>
.shared-panel {
  display: grid;
  gap: 0.75rem;
  padding: 0.75rem;
  height: 100%;
  align-content: start;
  overflow: auto;
}
.shared-panel__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.5rem;
}
.shared-panel__title {
  margin: 0;
  font-weight: 600;
  font-size: 0.875rem;
}
.shared-panel__meta {
  margin: 0.25rem 0 0;
  font-size: 0.75rem;
  color: var(--text-muted, #6b7280);
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}
.shared-panel__icon-btn,
.shared-panel__tab,
.shared-panel__action {
  border: 1px solid var(--border-subtle, #d1d5db);
  background: var(--surface-2, transparent);
  border-radius: 0.375rem;
  color: inherit;
  font: inherit;
  cursor: pointer;
}
.shared-panel__icon-btn {
  display: inline-flex;
  padding: 0.35rem;
}
.shared-panel__tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem;
}
.shared-panel__tab {
  padding: 0.25rem 0.55rem;
  font-size: 0.75rem;
}
.shared-panel__tab.is-active {
  border-color: var(--accent, #2563eb);
  color: var(--accent, #2563eb);
}
.shared-panel__form {
  display: grid;
  gap: 0.55rem;
}
.shared-panel__form label {
  display: grid;
  gap: 0.25rem;
  font-size: 0.75rem;
  color: var(--text-muted, #6b7280);
}
.shared-panel__form input,
.shared-panel__form textarea {
  border: 1px solid var(--border-subtle, #d1d5db);
  border-radius: 0.375rem;
  background: var(--surface-1, #fff);
  color: inherit;
  font: inherit;
  padding: 0.35rem 0.5rem;
}
.shared-panel__hint {
  margin: 0;
  font-size: 0.8125rem;
  color: var(--text-muted, #6b7280);
}
.shared-panel__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}
.shared-panel__action {
  padding: 0.35rem 0.7rem;
  font-size: 0.8125rem;
}
.shared-panel__action--secondary {
  opacity: 0.92;
}
.shared-panel__error {
  margin: 0;
  color: #b91c1c;
  font-size: 0.8125rem;
}
.shared-panel__result {
  margin: 0;
  padding: 0.55rem;
  border-radius: 0.375rem;
  background: var(--surface-2, #f3f4f6);
  font-size: 0.7rem;
  line-height: 1.4;
  overflow: auto;
  max-height: 18rem;
  white-space: pre-wrap;
  word-break: break-word;
}
.ok {
  color: #047857;
}
.warn {
  color: #b45309;
}
.spin {
  animation: spin 0.9s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

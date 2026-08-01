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
  listNativeSharedWorkspace,
  openNativeSharedLspWorkspace,
  queryNativeSharedMemory,
  searchNativeSharedCodeIndex,
  writeNativeSharedMemory,
  type NativeSharedCodingServicesStatus,
} from "../../services/nativeAgent";

const props = defineProps<ChatSidebarContext>();

type ServiceTab = "git" | "files" | "index" | "lsp" | "mcp" | "memory";
type Row = Record<string, unknown>;

const tabs: { id: ServiceTab; label: string }[] = [
  { id: "git", label: "Git" },
  { id: "files", label: "文件" },
  { id: "index", label: "搜索" },
  { id: "lsp", label: "代码分析" },
  { id: "mcp", label: "扩展工具" },
  { id: "memory", label: "记忆" },
];

const activeTab = ref<ServiceTab>("git");
const loading = ref(false);
const acting = ref(false);
const errorText = ref("");
const status = ref<NativeSharedCodingServicesStatus | null>(null);
const workspaceRoot = ref("");
const workspacePath = ref("");
const indexQuery = ref("");
const memoryQuery = ref("");
const memoryText = ref("");
const gitStatus = ref<Row | null>(null);
const workspaceEntries = ref<Row[]>([]);
const indexHits = ref<Row[]>([]);
const lspWorkspaces = ref<Row[]>([]);
const mcpServers = ref<Row[]>([]);
const memoryRecords = ref<Row[]>([]);
let disposed = false;

const workspaceId = computed(() => props.projectId?.trim() || props.taskId);
const toolsReady = computed(
  () =>
    Boolean(status.value?.sharedIdentityOk) &&
    Boolean(status.value?.gitSameInstance) &&
    Boolean(status.value?.codeIndexSameInstance) &&
    Boolean(status.value?.lspSameInstance) &&
    Boolean(status.value?.computerUseSameInstance),
);
const gitBranch = computed(() => {
  const head = asRow(gitStatus.value?.head);
  return stringValue(head?.branch) || shortCommit(stringValue(head?.commit));
});
const gitChanges = computed(() => rows(gitStatus.value?.changes));

function asRow(value: unknown): Row | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Row
    : null;
}

function rows(value: unknown): Row[] {
  return Array.isArray(value)
    ? value.map(asRow).filter((row): row is Row => row !== null)
    : [];
}

function stringValue(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function numberValue(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function shortCommit(value: string): string {
  return value ? value.slice(0, 8) : "未知分支";
}

function syncWorkspaceRoot() {
  if (props.projectCwd?.trim()) {
    workspaceRoot.value = props.projectCwd.trim();
  }
}

function requireWorkspace(): string | null {
  const root = workspaceRoot.value.trim();
  if (!root) {
    errorText.value = "当前任务没有关联工作区";
    return null;
  }
  return root;
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
    mcpServers.value = rows(servers);
    lspWorkspaces.value = rows(lsp.workspaces);
  } catch (error) {
    if (!disposed) errorText.value = error instanceof Error ? error.message : String(error);
  } finally {
    if (!disposed) loading.value = false;
  }
}

async function runAction(action: () => Promise<void>) {
  if (disposed || acting.value) return;
  acting.value = true;
  errorText.value = "";
  try {
    await action();
  } catch (error) {
    if (!disposed) errorText.value = error instanceof Error ? error.message : String(error);
  } finally {
    if (!disposed) acting.value = false;
  }
}

function runGitStatus() {
  return runAction(async () => {
    const root = requireWorkspace();
    if (!root) return;
    gitStatus.value = asRow(await getNativeSharedGitStatus(root));
  });
}

function runWorkspaceList() {
  return runAction(async () => {
    const root = requireWorkspace();
    if (!root) return;
    const result = asRow(await listNativeSharedWorkspace({
      workspaceId: workspaceId.value,
      root,
      path: workspacePath.value.trim(),
    }));
    workspaceEntries.value = rows(result?.entries);
  });
}

function runIndexSearch() {
  return runAction(async () => {
    const root = requireWorkspace();
    if (!root) return;
    const query = indexQuery.value.trim();
    if (!query) {
      errorText.value = "请输入要搜索的代码";
      return;
    }
    const result = asRow(await searchNativeSharedCodeIndex({
      workspaceId: workspaceId.value,
      root,
      query,
    }));
    indexHits.value = rows(result?.hits);
  });
}

function openLspWorkspace() {
  return runAction(async () => {
    const root = requireWorkspace();
    if (!root) return;
    await openNativeSharedLspWorkspace({
      workspaceId: workspaceId.value,
      root,
    });
    const result = await getNativeSharedLspStatus();
    lspWorkspaces.value = rows(result.workspaces);
  });
}

function runMemoryQuery() {
  return runAction(async () => {
    const query = memoryQuery.value.trim();
    if (!query) {
      errorText.value = "请输入要查找的内容";
      return;
    }
    const result = asRow(await queryNativeSharedMemory({
      query,
      namespace: "lilia.project",
      scopeId: workspaceId.value,
      limit: 16,
    }));
    memoryRecords.value = rows(result?.records);
  });
}

function runMemoryWrite() {
  return runAction(async () => {
    const text = memoryText.value.trim();
    if (!text) {
      errorText.value = "请输入要记住的内容";
      return;
    }
    await writeNativeSharedMemory({
      text,
      namespace: "lilia.project",
      scopeId: workspaceId.value,
    });
    memoryText.value = "";
    memoryQuery.value = text;
    const result = asRow(await queryNativeSharedMemory({
      query: text,
      namespace: "lilia.project",
      scopeId: workspaceId.value,
      limit: 16,
    }));
    memoryRecords.value = rows(result?.records);
  });
}

watch(
  () => props.projectCwd,
  () => syncWorkspaceRoot(),
  { immediate: true },
);

onMounted(() => {
  syncWorkspaceRoot();
  void refreshInventory();
  if (workspaceRoot.value) void runGitStatus();
});

onBeforeUnmount(() => {
  disposed = true;
});
</script>

<template>
  <div class="shared-panel" data-agent-id="chat.shared-services">
    <header class="shared-panel__head">
      <div>
        <p class="shared-panel__title">工作区工具</p>
        <p class="shared-panel__meta">
          {{ toolsReady ? "已连接" : "正在连接" }}
        </p>
      </div>
      <button
        type="button"
        class="shared-panel__icon-btn"
        data-agent-id="chat.shared-services.refresh"
        :disabled="loading || acting"
        title="刷新"
        @click="refreshInventory"
      >
        <Loader2 v-if="loading" :size="14" class="spin" aria-hidden="true" />
        <RefreshCw v-else :size="14" aria-hidden="true" />
      </button>
    </header>

    <div class="shared-panel__tabs" role="tablist" aria-label="工作区工具">
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
      <p v-if="gitStatus" class="shared-panel__summary">
        {{ gitBranch }} · {{ gitChanges.length ? `${gitChanges.length} 个改动` : "工作区干净" }}
      </p>
      <ul v-if="gitChanges.length" class="shared-panel__list">
        <li v-for="change in gitChanges" :key="stringValue(change.path)">
          <span>{{ stringValue(change.path) }}</span>
          <small>{{ stringValue(change.status) }}{{ change.staged ? " · 已暂存" : "" }}</small>
        </li>
      </ul>
      <button
        type="button"
        class="shared-panel__action"
        data-agent-id="chat.shared-services.git.status"
        :disabled="acting || !workspaceRoot"
        @click="runGitStatus"
      >
        刷新状态
      </button>
    </section>

    <section v-else-if="activeTab === 'files'" class="shared-panel__form">
      <label>
        目录
        <input
          v-model="workspacePath"
          type="text"
          spellcheck="false"
          data-agent-id="chat.shared-services.files.path"
          placeholder="留空查看工作区根目录"
        />
      </label>
      <button
        type="button"
        class="shared-panel__action"
        data-agent-id="chat.shared-services.files.list"
        :disabled="acting || !workspaceRoot"
        @click="runWorkspaceList"
      >
        查看文件
      </button>
      <ul class="shared-panel__list">
        <li v-for="entry in workspaceEntries" :key="stringValue(entry.path)">
          <span>{{ stringValue(entry.path) }}</span>
          <small>{{ stringValue(entry.kind) === "dir" ? "文件夹" : `${numberValue(entry.size)} B` }}</small>
        </li>
      </ul>
    </section>

    <section v-else-if="activeTab === 'index'" class="shared-panel__form">
      <label>
        搜索代码
        <input
          v-model="indexQuery"
          type="search"
          data-agent-id="chat.shared-services.index.query"
          placeholder="函数、类型或文本"
          @keyup.enter="runIndexSearch"
        />
      </label>
      <button
        type="button"
        class="shared-panel__action"
        data-agent-id="chat.shared-services.index.search"
        :disabled="acting || !workspaceRoot"
        @click="runIndexSearch"
      >
        搜索
      </button>
      <p v-if="indexQuery && !acting && !indexHits.length" class="shared-panel__empty">
        没有匹配结果
      </p>
      <ul class="shared-panel__list">
        <li v-for="(hit, index) in indexHits" :key="`${stringValue(hit.path)}:${index}`">
          <strong>{{ stringValue(hit.path) }}</strong>
          <span>{{ stringValue(hit.summary) }}</span>
        </li>
      </ul>
    </section>

    <section v-else-if="activeTab === 'lsp'" class="shared-panel__form">
      <p class="shared-panel__summary">
        {{ lspWorkspaces.length ? `${lspWorkspaces.length} 个工作区正在分析` : "尚未启动代码分析" }}
      </p>
      <ul class="shared-panel__list">
        <li v-for="workspace in lspWorkspaces" :key="stringValue(workspace.server_id)">
          <span>{{ stringValue(workspace.server_id) }}</span>
          <small>{{ stringValue(workspace.state) }}</small>
        </li>
      </ul>
      <button
        type="button"
        class="shared-panel__action"
        data-agent-id="chat.shared-services.lsp.open"
        :disabled="acting || !workspaceRoot"
        @click="openLspWorkspace"
      >
        启动代码分析
      </button>
    </section>

    <section v-else-if="activeTab === 'mcp'" class="shared-panel__form">
      <p v-if="!mcpServers.length" class="shared-panel__empty">暂无已连接的扩展工具</p>
      <ul class="shared-panel__list">
        <li v-for="server in mcpServers" :key="stringValue(server.server_id)">
          <span>{{ stringValue(server.server_id) }}</span>
          <small>
            {{ stringValue(server.state) }} · {{ numberValue(server.tool_count) }} 个工具
          </small>
        </li>
      </ul>
      <button
        type="button"
        class="shared-panel__action"
        :disabled="loading || acting"
        @click="refreshInventory"
      >
        刷新
      </button>
    </section>

    <section v-else class="shared-panel__form">
      <label>
        查找记忆
        <input
          v-model="memoryQuery"
          type="search"
          data-agent-id="chat.shared-services.memory.query"
          @keyup.enter="runMemoryQuery"
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
          查找
        </button>
      </div>
      <ul class="shared-panel__list">
        <li v-for="record in memoryRecords" :key="stringValue(record.memory_id)">
          <span>{{ stringValue(record.text) }}</span>
        </li>
      </ul>
      <label>
        添加记忆
        <textarea
          v-model="memoryText"
          rows="3"
          data-agent-id="chat.shared-services.memory.text"
        />
      </label>
      <button
        type="button"
        class="shared-panel__action shared-panel__action--secondary"
        data-agent-id="chat.shared-services.memory.write-btn"
        :disabled="acting"
        @click="runMemoryWrite"
      >
        保存
      </button>
    </section>
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
.shared-panel__title,
.shared-panel__summary,
.shared-panel__empty {
  margin: 0;
}
.shared-panel__title {
  font-weight: 600;
  font-size: 0.875rem;
}
.shared-panel__meta,
.shared-panel__empty {
  margin: 0.25rem 0 0;
  font-size: 0.75rem;
  color: var(--text-muted, #6b7280);
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
.shared-panel__tabs,
.shared-panel__actions {
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
.shared-panel__list {
  display: grid;
  gap: 0.4rem;
  margin: 0;
  padding: 0;
  list-style: none;
}
.shared-panel__list li {
  display: grid;
  gap: 0.15rem;
  padding: 0.45rem 0.5rem;
  border-radius: 0.375rem;
  background: var(--surface-2, #f3f4f6);
  font-size: 0.75rem;
  overflow-wrap: anywhere;
}
.shared-panel__list small {
  color: var(--text-muted, #6b7280);
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

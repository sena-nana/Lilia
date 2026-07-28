<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import Layers from "@lucide/vue/dist/esm/icons/layers.mjs";
import Loader2 from "@lucide/vue/dist/esm/icons/loader-circle.mjs";
import RefreshCw from "@lucide/vue/dist/esm/icons/refresh-cw.mjs";
import {
  getNativeSharedCodingServicesStatus,
  getNativeSharedLspStatus,
  listNativeSharedMcpServers,
  type NativeSharedCodingServicesStatus,
} from "../../services/nativeAgent";

const loading = ref(false);
const errorText = ref("");
const status = ref<NativeSharedCodingServicesStatus | null>(null);
const mcpCount = ref(0);
const lspWorkspaces = ref(0);
let disposed = false;

const rows = computed(() => {
  const s = status.value;
  if (!s) return [];
  return [
    {
      name: "Git",
      id: s.gitServiceId,
      source: s.dataSource,
      ok: s.gitSameInstance,
    },
    {
      name: "Code Index",
      id: s.codeIndexServiceId,
      source: s.dataSource,
      ok: s.codeIndexSameInstance,
    },
    {
      name: "LSP",
      id: s.lspServiceId,
      source: s.dataSource,
      ok: s.lspSameInstance,
      detail: `${lspWorkspaces.value} workspace`,
    },
    {
      name: "MCP",
      id: s.mcpServiceId,
      source: s.dataSource,
      ok: s.mcpSameInstance,
      detail: `${mcpCount.value} server`,
    },
    {
      name: "Memory",
      id: s.memoryRunnerId,
      source: s.dataSource,
      ok: s.memorySharedRouter,
    },
  ];
});

const allShared = computed(
  () =>
    Boolean(status.value?.sharedIdentityOk) &&
    rows.value.every((row) => row.ok) &&
    status.value?.officialAgentServer === false,
);

async function refresh() {
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
    mcpCount.value = Array.isArray(servers) ? servers.length : next.mcpActiveServers;
    lspWorkspaces.value = lsp.activeWorkspaces ?? next.lspActiveWorkspaces;
  } catch (err) {
    if (!disposed) errorText.value = err instanceof Error ? err.message : String(err);
  } finally {
    if (!disposed) loading.value = false;
  }
}

onMounted(() => {
  void refresh();
});

onBeforeUnmount(() => {
  disposed = true;
});
</script>

<template>
  <section class="shared-services" data-agent-id="settings.shared-services">
    <header class="shared-services__header">
      <div>
        <h2>共享 Services</h2>
        <p class="shared-services__lead">
          Git / Index / LSP / MCP / Memory 读取 AgentKit Native Bundle 同一实例（不新建 session）。
          旧 Claude/Codex 配置文件仅作迁移来源，不再作为产品主数据源。
        </p>
      </div>
      <button
        type="button"
        class="shared-services__refresh"
        data-agent-id="settings.shared-services.refresh"
        :disabled="loading"
        @click="refresh"
      >
        <Loader2 v-if="loading" :size="14" class="spin" aria-hidden="true" />
        <RefreshCw v-else :size="14" aria-hidden="true" />
        刷新
      </button>
    </header>

    <p v-if="errorText" class="shared-services__error" role="alert">{{ errorText }}</p>

    <div class="shared-services__badge" data-agent-id="settings.shared-services.source">
      <Layers :size="14" aria-hidden="true" />
      <span>数据源：{{ status?.dataSource ?? "…" }}</span>
      <span :class="allShared ? 'ok' : 'warn'">
        {{ allShared ? "单实例共享" : "未就绪 / 需检查" }}
      </span>
    </div>

    <ul class="shared-services__list">
      <li v-for="row in rows" :key="row.name">
        <span class="name">{{ row.name }}</span>
        <span class="id">{{ row.id }}</span>
        <span class="source">{{ row.source }}</span>
        <span :class="row.ok ? 'ok' : 'warn'">{{ row.ok ? "共享 Arc" : "未共享" }}</span>
        <span v-if="row.detail" class="detail">{{ row.detail }}</span>
      </li>
    </ul>

    <p class="shared-services__note">
      对话侧栏「共享 Services」面板可直接读取 Git / Index / LSP / MCP，并查询或写入 Memory（同一 AgentKit Bundle Arc）。
      本页只展示绑定状态与数据源；完整工作台页面仍逐步迁出 Claude/Codex 私有配置。
    </p>
  </section>
</template>

<style scoped>
.shared-services {
  display: grid;
  gap: 1rem;
  max-width: 52rem;
}
.shared-services__header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
}
.shared-services__lead,
.shared-services__note {
  margin: 0.35rem 0 0;
  color: var(--lilia-muted, #6b7280);
  font-size: 0.875rem;
  line-height: 1.45;
}
.shared-services__refresh {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  border: 1px solid var(--lilia-border, #d1d5db);
  background: transparent;
  border-radius: 0.4rem;
  padding: 0.35rem 0.65rem;
  cursor: pointer;
}
.shared-services__error {
  color: #b91c1c;
  margin: 0;
}
.shared-services__badge {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.875rem;
}
.shared-services__list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 0.5rem;
}
.shared-services__list li {
  display: grid;
  grid-template-columns: 6.5rem 1fr 1fr auto auto;
  gap: 0.5rem;
  align-items: center;
  font-size: 0.8125rem;
  padding: 0.5rem 0;
  border-bottom: 1px solid var(--lilia-border, #e5e7eb);
}
.name {
  font-weight: 600;
}
.id,
.source,
.detail {
  color: var(--lilia-muted, #6b7280);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
@media (max-width: 720px) {
  .shared-services__list li {
    grid-template-columns: 1fr;
  }
}
</style>

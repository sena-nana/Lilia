<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { RouterLink, type RouteLocationRaw } from "vue-router";
import AlertTriangle from "@lucide/vue/dist/esm/icons/triangle-alert.mjs";
import Sparkles from "@lucide/vue/dist/esm/icons/sparkles.mjs";
import type { ChatBackendKind } from "@lilia/contracts";
import {
  CHAT_BACKEND_LABELS,
  UNCONFIGURED_CONNECTION_MODES,
} from "@lilia/contracts/chatBackendsContract.mjs";
import { useConnectionStatus } from "../composables/useConnectionStatus";
import { cancelIdleRun, runWhenIdle, scheduleAfterPaint } from "@lilia/ui/diagnostics";

const STARTUP_CONNECTION_REFRESH_DELAY_MS = 1_200;
const UNCONFIGURED_CONNECTION_MODE_SET = new Set<string>(UNCONFIGURED_CONNECTION_MODES);

function chatBackendLabel(backend: ChatBackendKind): string {
  return CHAT_BACKEND_LABELS[backend];
}

function connectionModeIsUnconfigured(mode: unknown): mode is "unconfigured" {
  return typeof mode === "string" && UNCONFIGURED_CONNECTION_MODE_SET.has(mode);
}

const props = withDefaults(defineProps<{
  to?: RouteLocationRaw | null;
}>(), {
  to: null,
});

const {
  report,
  activeBackend,
  statusFor,
  refresh,
} = useConnectionStatus({ probe: false, loadBackend: false });

const activeStatus = computed(() => statusFor(activeBackend.value));
let startupSeq = 0;
let idleHandle: ReturnType<typeof runWhenIdle> | null = null;
let cancelPaint: (() => void) | null = null;
let disposed = false;

const backendLabel = computed(() => chatBackendLabel(activeBackend.value));

const badgeTag = computed(() => props.to ? RouterLink : "button");
const badgeAttrs = computed(() => props.to ? { to: props.to } : { type: "button" });

const hasConnectionIssue = computed(
  () => connectionModeIsUnconfigured(activeStatus.value?.connectionMode) ||
    activeStatus.value === null,
);

const connectionTone = computed(() => {
  if (report.value === null) return "probing";
  if (hasConnectionIssue.value) return "warn";
  return "ok";
});

const connectionTooltip = computed(() => {
  const s = activeStatus.value;
  if (!s) return "正在检测 agent 连接…";
  if (connectionModeIsUnconfigured(s.connectionMode)) {
    return `${backendLabel.value} 未配置。请到设置 → 凭据 配置 OpenAI / Anthropic。`;
  }
  return `${backendLabel.value} · ${s.effectiveUrl ?? "—"}`;
});

function cancelStartupRefreshSchedule() {
  if (idleHandle) {
    cancelIdleRun(idleHandle);
    idleHandle = null;
  }
  cancelPaint?.();
  cancelPaint = null;
}

function scheduleStartupRefresh() {
  cancelStartupRefreshSchedule();
  const seq = ++startupSeq;
  idleHandle = runWhenIdle(() => {
    idleHandle = null;
    if (disposed || seq !== startupSeq) return;
    cancelPaint = scheduleAfterPaint(() => {
      cancelPaint = null;
      if (disposed || seq !== startupSeq) return;
      void refresh(true);
    }, STARTUP_CONNECTION_REFRESH_DELAY_MS);
  });
}

onMounted(() => {
  disposed = false;
  scheduleStartupRefresh();
});

onBeforeUnmount(() => {
  disposed = true;
  cancelStartupRefreshSchedule();
});
</script>

<template>
  <component
    :is="badgeTag"
    v-bind="badgeAttrs"
    class="sb-conn"
    :class="`sb-conn--${connectionTone}`"
    data-agent-id="provider-connection.badge"
    :title="connectionTooltip"
    :aria-label="connectionTooltip"
  >
    <template v-if="connectionTone === 'probing'">
      <span class="sb-conn__label sb-conn__label--probing">检测中...</span>
    </template>
    <template v-else-if="connectionTone !== 'ok'">
      <AlertTriangle :size="12" aria-hidden="true" />
      <span class="sb-conn__label">未连接</span>
    </template>
    <template v-else>
      <Sparkles :size="12" aria-hidden="true" />
      <span class="sb-conn__label">{{ backendLabel }}</span>
    </template>
  </component>
</template>

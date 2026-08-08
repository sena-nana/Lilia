<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { RouterLink, type RouteLocationRaw } from "vue-router";
import AlertTriangle from "@lucide/vue/dist/esm/icons/triangle-alert.mjs";
import Download from "@lucide/vue/dist/esm/icons/download.mjs";
import Loader2 from "@lucide/vue/dist/esm/icons/loader-circle.mjs";
import LogIn from "@lucide/vue/dist/esm/icons/log-in.mjs";
import RefreshCw from "@lucide/vue/dist/esm/icons/refresh-cw.mjs";
import RotateCw from "@lucide/vue/dist/esm/icons/rotate-cw.mjs";
import Sparkles from "@lucide/vue/dist/esm/icons/sparkles.mjs";
import type {
  ChatBackendKind,
  CodexAccountQuotaStatus,
  CodexAccountQuotaWindow,
  RouterMode,
} from "@lilia/contracts";
import {
  CHAT_BACKEND_LABELS,
  CONNECTION_MODES_USING_CODEX_ACCOUNT,
  ROUTER_MODES_USING_CODEX_ACCOUNT,
  UNCONFIGURED_CONNECTION_MODES,
} from "@lilia/contracts/chatBackendsContract.mjs";
import { useAnchoredOverlay } from "@lilia/ui/composables/useAnchoredOverlay";
import { useConnectionStatus } from "../composables/useConnectionStatus";
import { getCodexAccountQuotaStatus, startCodexAccountLogin } from "../services/chat";
import { cancelIdleRun, runWhenIdle, scheduleAfterPaint } from "@lilia/ui/diagnostics";
import {
  codexAccountNeedsLogin,
  codexQuotaUnavailableStatus,
  formatCompactNumber,
  formatUnixSeconds,
  quotaWindowLine as quotaRemainingLine,
  quotaWindowRemainingPercent,
  quotaPercentTone,
} from "../utils/quotaDisplay";

const QUOTA_STALE_MS = 60_000;
const STARTUP_CONNECTION_REFRESH_DELAY_MS = 1_200;
const STARTUP_REMOTE_REFRESH_DELAY_MS = 2_500;
const CONNECTION_MODES_USING_CODEX_ACCOUNT_SET = new Set<string>(CONNECTION_MODES_USING_CODEX_ACCOUNT);
const ROUTER_MODES_USING_CODEX_ACCOUNT_SET = new Set<string>(ROUTER_MODES_USING_CODEX_ACCOUNT);
const UNCONFIGURED_CONNECTION_MODE_SET = new Set<string>(UNCONFIGURED_CONNECTION_MODES);

function chatBackendLabel(backend: ChatBackendKind): string {
  return CHAT_BACKEND_LABELS[backend];
}

function connectionModeIsUnconfigured(mode: unknown): mode is "unconfigured" {
  return typeof mode === "string" && UNCONFIGURED_CONNECTION_MODE_SET.has(mode);
}

function connectionModeUsesCodexAccount(mode: unknown): mode is "codex-account" {
  return typeof mode === "string" && CONNECTION_MODES_USING_CODEX_ACCOUNT_SET.has(mode);
}

function routerModeUsesCodexAccount(mode: RouterMode): boolean {
  return ROUTER_MODES_USING_CODEX_ACCOUNT_SET.has(mode);
}

const props = withDefaults(defineProps<{
  to?: RouteLocationRaw | null;
  popoverId?: string;
  preferredPlacement?: "top-start" | "top-end" | "bottom-start" | "bottom-end";
}>(), {
  to: null,
  popoverId: "provider-conn-quota-popover",
  preferredPlacement: "top-start",
});

const {
  report,
  activeBackend,
  statusFor,
  routerFor,
  codexAppServer,
  refresh,
  checkCodexAppServerUpdate,
  installCodexAppServerUpdate,
  codexAppServerUpdateChecking,
  codexAppServerUpdating,
  codexAppServerUpdateError,
} = useConnectionStatus({ probe: false, loadBackend: false });

const activeStatus = computed(() => statusFor(activeBackend.value));
const badgeAnchorEl = ref<HTMLElement | null>(null);
const officialQuota = ref<CodexAccountQuotaStatus | null>(null);
const quotaLoading = ref(false);
const codexLoginStarting = ref(false);
const quotaTooltipOpen = ref(false);
const updateAnchorEl = ref<HTMLElement | null>(null);
const updateTooltipOpen = ref(false);
const closeTimerId = ref<number | null>(null);
const updateCloseTimerId = ref<number | null>(null);
let quotaRequestSeq = 0;
let quotaInflight: Promise<void> | null = null;
let startupSeq = 0;
let cancelStartupConnectionRefresh: (() => void) | null = null;
let cancelStartupRemoteRefresh: (() => void) | null = null;
let disposed = false;
let quotaOpenIntent = false;
const openState = computed(() => quotaTooltipOpen.value);
const updateOpenState = computed(() => updateTooltipOpen.value);
const preferredPlacement = computed(() => props.preferredPlacement);
const {
  overlayEl: quotaPopoverEl,
  overlayStyle: quotaPopoverStyle,
  resolvedPlacement: resolvedQuotaPlacement,
  updatePosition: updateQuotaPopoverPosition,
} = useAnchoredOverlay({
  open: openState,
  anchorEl: badgeAnchorEl,
  preferredPlacement,
  offset: 8,
});
const {
  overlayEl: updatePopoverEl,
  overlayStyle: updatePopoverStyle,
  resolvedPlacement: resolvedUpdatePlacement,
  updatePosition: updatePopoverPosition,
} = useAnchoredOverlay({
  open: updateOpenState,
  anchorEl: updateAnchorEl,
  preferredPlacement,
  offset: 8,
});
void quotaPopoverEl;
void updatePopoverEl;

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
    return `${backendLabel.value} API 未配置。点击进入设置。`;
  }
  if (connectionModeUsesCodexAccount(s.connectionMode)) {
    return "Codex · 官方账号";
  }
  return `${backendLabel.value} · ${s.effectiveUrl ?? "—"}`;
});

const isCodexOfficialAccount = computed(() => {
  const codexRouterMode = routerFor("codex");
  return activeBackend.value === "codex" &&
    connectionTone.value === "ok" &&
    codexRouterMode !== null &&
    routerModeUsesCodexAccount(codexRouterMode) &&
    connectionModeUsesCodexAccount(activeStatus.value?.connectionMode);
});

const quotaRows = computed(() => [
  { key: "fiveHour", window: officialQuota.value?.fiveHour, suffix: "" },
  { key: "weekly", window: officialQuota.value?.weekly, suffix: "" },
] as const);
const sparkQuotaRows = computed(() => [
  { key: "sparkFiveHour", window: officialQuota.value?.sparkFiveHour, suffix: "Spark" },
  { key: "sparkWeekly", window: officialQuota.value?.sparkWeekly, suffix: "Spark" },
] as const);
const quotaDetailRows = computed(() => [
  ...quotaRows.value,
  ...sparkQuotaRows.value.filter((row) => row.window),
]);
const resetCreditAvailableCount = computed(() =>
  officialQuota.value?.rateLimitResetCredits?.availableCount ?? 0,
);
const accountUsageSummary = computed(() => officialQuota.value?.accountUsage?.summary ?? null);
const accountUsageLine = computed(() => {
  const summary = accountUsageSummary.value;
  if (!summary) return null;
  const parts = [];
  if (summary.lifetimeTokens !== null) parts.push(`累计 ${formatCompactNumber(summary.lifetimeTokens)} tokens`);
  if (summary.currentStreakDays !== null) parts.push(`连续 ${summary.currentStreakDays} 天`);
  return parts.join(" · ") || null;
});
const codexLoginNeedsAction = computed(() =>
  codexAccountNeedsLogin(
    officialQuota.value,
    codexAppServer.value?.supportsRequiredProtocol ?? false,
  ),
);

const shouldShowQuotaRings = computed(() =>
  isCodexOfficialAccount.value && Boolean(officialQuota.value?.fiveHour || officialQuota.value?.weekly),
);
const codexUpdateState = computed(() => codexAppServer.value?.updateState ?? "idle");
const codexUpdateDownloading = computed(() => codexUpdateState.value === "downloading");
const codexUpdateReady = computed(() => codexUpdateState.value === "ready");
const codexUpdateTarget = computed(() =>
  codexAppServer.value?.preparedVersion ??
  codexAppServer.value?.latestVersion ??
  "最新版本",
);
const codexUpdateRetryMode = computed<"prepare" | "switch" | null>(() => {
  if (codexUpdateState.value !== "failed") return null;
  return codexAppServer.value?.preparedVersion ? "switch" : "prepare";
});
const shouldShowCodexUpdate = computed(() =>
  activeBackend.value === "codex" &&
  connectionModeUsesCodexAccount(activeStatus.value?.connectionMode) &&
  (
    Boolean(codexAppServer.value?.updateAvailable) ||
    codexUpdateDownloading.value ||
    codexUpdateReady.value ||
    codexUpdateRetryMode.value !== null ||
    codexAppServerUpdating.value
  ),
);
const codexUpdateProgressPercent = computed(() => codexAppServer.value?.updateProgressPercent ?? null);
const codexUpdateProgressStyle = computed(() => ({
  "--quota-progress": String(codexUpdateProgressPercent.value ?? 0),
}));
const codexUpdateActionEnabled = computed(() =>
  codexUpdateReady.value ||
  codexUpdateRetryMode.value !== null
);
const codexUpdateTitle = computed(() => {
  const current = codexAppServer.value?.version ?? "未安装";
  if (codexAppServerUpdating.value || codexUpdateState.value === "switching") {
    return `切换 Codex app-server：${current} -> ${codexUpdateTarget.value}`;
  }
  if (codexUpdateDownloading.value || codexAppServerUpdateChecking.value) {
    if (codexUpdateDownloading.value && codexUpdateProgressPercent.value !== null) {
      return `下载 Codex app-server（${codexUpdateProgressPercent.value}%）：${current} -> ${codexUpdateTarget.value}`;
    }
    return `下载 Codex app-server：${current} -> ${codexUpdateTarget.value}`;
  }
  if (codexUpdateReady.value) {
    return `切换 Codex app-server：${current} -> ${codexUpdateTarget.value}`;
  }
  if (codexUpdateRetryMode.value === "switch") {
    return `重试切换 Codex app-server：${current} -> ${codexUpdateTarget.value}`;
  }
  if (codexUpdateRetryMode.value === "prepare") {
    return `重新准备 Codex app-server：${current} -> ${codexUpdateTarget.value}`;
  }
  return `准备 Codex app-server：${current} -> ${codexUpdateTarget.value}`;
});
const codexReleaseNotes = computed(() => codexAppServer.value?.releaseNotes ?? []);
const codexUpdateErrorText = computed(() =>
  codexAppServer.value?.updateError ?? codexAppServerUpdateError.value ?? null
);

function quotaRingStyle(window: CodexAccountQuotaWindow | null | undefined) {
  const remainingPercent = quotaWindowRemainingPercent(window);
  return {
    "--quota-progress": String(remainingPercent),
  };
}

function quotaRingTone(window: CodexAccountQuotaWindow | null | undefined) {
  return window ? `sb-quota-ring--${quotaPercentTone(window.usedPercent)}` : "sb-quota-ring--empty";
}

function cancelCloseTimer() {
  if (closeTimerId.value !== null) {
    window.clearTimeout(closeTimerId.value);
    closeTimerId.value = null;
  }
}

function cancelUpdateCloseTimer() {
  if (updateCloseTimerId.value !== null) {
    window.clearTimeout(updateCloseTimerId.value);
    updateCloseTimerId.value = null;
  }
}

function scheduleCloseQuotaDetails() {
  cancelCloseTimer();
  closeTimerId.value = window.setTimeout(() => {
    quotaTooltipOpen.value = false;
    closeTimerId.value = null;
  }, 80);
}

function scheduleDelayedIdle(delayMs: number, run: () => void): () => void {
  let active = true;
  let idleHandle: number | null = null;
  let timerId: number | null = window.setTimeout(() => {
    timerId = null;
    if (!active) return;
    idleHandle = runWhenIdle(() => {
      idleHandle = null;
      if (active) run();
    });
  }, delayMs);
  return () => {
    active = false;
    if (timerId !== null) {
      window.clearTimeout(timerId);
      timerId = null;
    }
    if (idleHandle !== null) {
      cancelIdleRun(idleHandle);
      idleHandle = null;
    }
  };
}

function scheduleAfterPaintDelayedIdle(delayMs: number, run: () => void): () => void {
  let cancelDelay: (() => void) | null = null;
  let cancelPaint: (() => void) | null = scheduleAfterPaint(() => {
    cancelPaint = null;
    cancelDelay = scheduleDelayedIdle(delayMs, run);
  });
  return () => {
    cancelPaint?.();
    cancelPaint = null;
    cancelDelay?.();
    cancelDelay = null;
  };
}

function cancelStartupRefreshSchedule() {
  startupSeq += 1;
  cancelStartupConnectionRefresh?.();
  cancelStartupConnectionRefresh = null;
  cancelStartupRemoteRefresh?.();
  cancelStartupRemoteRefresh = null;
}

function shouldRunStartupRemoteRefresh(): boolean {
  return activeBackend.value === "codex" &&
    connectionModeUsesCodexAccount(activeStatus.value?.connectionMode);
}

function scheduleStartupRemoteRefresh(seq: number) {
  cancelStartupRemoteRefresh?.();
  cancelStartupRemoteRefresh = scheduleDelayedIdle(STARTUP_REMOTE_REFRESH_DELAY_MS, () => {
    if (disposed || seq !== startupSeq || !shouldRunStartupRemoteRefresh()) return;
    void Promise.all([
      checkCodexAppServerUpdate(),
      loadOfficialQuota(),
    ]).catch((err) => {
      console.error("[provider-connection] startup remote refresh failed", err);
    });
  });
}

function scheduleStartupRefresh() {
  cancelStartupRefreshSchedule();
  const seq = startupSeq;
  cancelStartupConnectionRefresh = scheduleAfterPaintDelayedIdle(
    STARTUP_CONNECTION_REFRESH_DELAY_MS,
    () => {
      if (disposed || seq !== startupSeq) return;
      void refresh(false).then(() => {
        if (disposed || seq !== startupSeq) return;
        scheduleStartupRemoteRefresh(seq);
      }).catch((err) => {
        console.error("[provider-connection] startup connection refresh failed", err);
      });
    },
  );
}

async function loadOfficialQuota() {
  if (!isCodexOfficialAccount.value) {
    clearOfficialQuota();
    return;
  }
  if (officialQuota.value?.fetchedAt && Date.now() - officialQuota.value.fetchedAt < QUOTA_STALE_MS) {
    return;
  }
  if (quotaInflight) return quotaInflight;

  const seq = ++quotaRequestSeq;
  quotaLoading.value = true;
  quotaInflight = (async () => {
    try {
      const result = await getCodexAccountQuotaStatus();
      if (!disposed && seq === quotaRequestSeq) officialQuota.value = result;
    } catch (err) {
      if (!disposed && seq === quotaRequestSeq) officialQuota.value = codexQuotaUnavailableStatus(err);
    } finally {
      if (!disposed && seq === quotaRequestSeq) quotaLoading.value = false;
      quotaInflight = null;
    }
  })();
  return quotaInflight;
}

async function openQuotaDetails() {
  if (!isCodexOfficialAccount.value) return;
  quotaOpenIntent = true;
  cancelCloseTimer();
  await loadOfficialQuota();
  if (disposed || !quotaOpenIntent || !isCodexOfficialAccount.value) return;
  quotaTooltipOpen.value = true;
  void nextTick(() => {
    if (!disposed && quotaTooltipOpen.value) void updateQuotaPopoverPosition();
  });
}

function closeQuotaDetails() {
  quotaOpenIntent = false;
  scheduleCloseQuotaDetails();
}

function scheduleCloseUpdateDetails() {
  cancelUpdateCloseTimer();
  updateCloseTimerId.value = window.setTimeout(() => {
    updateTooltipOpen.value = false;
    updateCloseTimerId.value = null;
  }, 80);
}

function openUpdateDetails() {
  if (!shouldShowCodexUpdate.value && !codexAppServerUpdating.value) return;
  cancelUpdateCloseTimer();
  updateTooltipOpen.value = true;
  void nextTick(() => {
    if (!disposed && updateTooltipOpen.value) void updatePopoverPosition();
  });
}

function closeUpdateDetails() {
  scheduleCloseUpdateDetails();
}

async function installUpdate() {
  if (disposed) return;
  if (codexUpdateRetryMode.value === "prepare") {
    await checkCodexAppServerUpdate();
    return;
  }
  await installCodexAppServerUpdate();
}

async function startCodexLogin() {
  if (disposed || codexLoginStarting.value) return;
  codexLoginStarting.value = true;
  try {
    await startCodexAccountLogin();
    if (!disposed && officialQuota.value) {
      officialQuota.value = { ...officialQuota.value, fetchedAt: 0 };
    }
  } catch (err) {
    if (!disposed) officialQuota.value = codexQuotaUnavailableStatus(err);
  } finally {
    if (!disposed) codexLoginStarting.value = false;
  }
}

function clearOfficialQuota() {
  cancelCloseTimer();
  quotaOpenIntent = false;
  quotaRequestSeq += 1;
  officialQuota.value = null;
  quotaLoading.value = false;
  quotaTooltipOpen.value = false;
}

watch(
  isCodexOfficialAccount,
  (enabled) => {
    if (enabled) return;
    cancelStartupRemoteRefresh?.();
    cancelStartupRemoteRefresh = null;
    clearOfficialQuota();
  },
  { immediate: true },
);

watch(
  () => [
    quotaTooltipOpen.value,
    officialQuota.value?.available ?? false,
    officialQuota.value?.error ?? "",
    quotaDetailRows.value.length,
    quotaLoading.value,
    codexLoginStarting.value,
  ] as const,
  ([open]) => {
    if (!open) return;
    void updateQuotaPopoverPosition();
  },
);

watch(
  () => [
    updateTooltipOpen.value,
    codexAppServer.value?.latestVersion ?? "",
    codexAppServer.value?.releaseNotes.join("\n") ?? "",
    codexAppServerUpdating.value,
    codexUpdateErrorText.value ?? "",
  ] as const,
  ([open]) => {
    if (!open) return;
    void updatePopoverPosition();
  },
);

onMounted(() => {
  disposed = false;
  scheduleStartupRefresh();
});

onBeforeUnmount(() => {
  disposed = true;
  quotaOpenIntent = false;
  quotaRequestSeq += 1;
  quotaInflight = null;
  quotaLoading.value = false;
  codexLoginStarting.value = false;
  cancelStartupRefreshSchedule();
  cancelCloseTimer();
  cancelUpdateCloseTimer();
});
</script>

<template>
  <span
    ref="badgeAnchorEl"
    class="sb-conn-anchor"
  >
    <component
      :is="badgeTag"
      v-bind="badgeAttrs"
      class="sb-conn"
      :class="[
        `sb-conn--${connectionTone}`,
        { 'sb-conn--quota': shouldShowQuotaRings },
      ]"
      data-agent-id="provider-connection.badge"
      :title="connectionTooltip"
      :aria-label="connectionTooltip"
      :aria-describedby="quotaTooltipOpen ? popoverId : undefined"
      @mouseenter="openQuotaDetails"
      @mouseleave="closeQuotaDetails"
      @focus="openQuotaDetails"
      @blur="closeQuotaDetails"
    >
      <template v-if="connectionTone === 'probing'">
        <span class="sb-conn__label sb-conn__label--probing">检测中...</span>
      </template>
      <template v-else-if="connectionTone !== 'ok'">
        <AlertTriangle :size="12" aria-hidden="true" />
        <span class="sb-conn__label">{{ connectionTone === "error" ? "异常" : "未连接" }}</span>
      </template>
      <template v-else-if="activeStatus">
        <Sparkles :size="12" aria-hidden="true" />
        <span class="sb-conn__label">{{ backendLabel }}</span>
        <span v-if="shouldShowQuotaRings" class="sb-quota-rings" aria-hidden="true">
          <span
            v-for="row in quotaRows"
            :key="row.key"
            class="sb-quota-ring"
            :class="quotaRingTone(row.window)"
            :style="quotaRingStyle(row.window)"
          >
            <svg viewBox="0 0 16 16" focusable="false" aria-hidden="true">
              <circle class="sb-quota-ring__track" cx="8" cy="8" r="6" pathLength="100" />
              <circle class="sb-quota-ring__value" cx="8" cy="8" r="6" pathLength="100" />
            </svg>
          </span>
        </span>
      </template>
    </component>
    <button
      v-if="shouldShowCodexUpdate || codexAppServerUpdating"
      ref="updateAnchorEl"
      type="button"
      class="sb-conn-update"
      data-agent-id="provider-connection.codex-update"
      :disabled="
        codexAppServerUpdating ||
        codexAppServerUpdateChecking ||
        codexUpdateDownloading ||
        !codexUpdateActionEnabled
      "
      :title="codexUpdateTitle"
      :aria-label="codexUpdateTitle"
      :aria-describedby="updateTooltipOpen ? `${popoverId}-update` : undefined"
      @mouseenter="openUpdateDetails"
      @mouseleave="closeUpdateDetails"
      @focus="openUpdateDetails"
      @blur="closeUpdateDetails"
      @click.stop.prevent="installUpdate"
    >
      <Loader2
        v-if="codexAppServerUpdating || codexAppServerUpdateChecking"
        :size="12"
        class="is-spinning"
        aria-hidden="true"
      />
      <span
        v-else-if="codexUpdateDownloading"
        class="sb-quota-ring"
        :class="{ 'sb-quota-ring--empty': codexUpdateProgressPercent === null }"
        :style="codexUpdateProgressStyle"
        aria-hidden="true"
      >
        <svg viewBox="0 0 16 16" focusable="false" aria-hidden="true">
          <circle class="sb-quota-ring__track" cx="8" cy="8" r="6" pathLength="100" />
          <circle class="sb-quota-ring__value" cx="8" cy="8" r="6" pathLength="100" />
        </svg>
      </span>
      <RefreshCw v-else-if="codexUpdateReady" :size="12" aria-hidden="true" />
      <RefreshCw v-else-if="codexUpdateRetryMode === 'switch'" :size="12" aria-hidden="true" />
      <RotateCw v-else-if="codexUpdateRetryMode === 'prepare'" :size="12" aria-hidden="true" />
      <Download v-else :size="12" aria-hidden="true" />
    </button>
  </span>
  <Teleport to="body">
    <span
      v-if="quotaTooltipOpen"
      :id="popoverId"
      ref="quotaPopoverEl"
      role="tooltip"
      class="sb-conn-popover"
      :class="`sb-conn-popover--${resolvedQuotaPlacement}`"
      :data-placement="resolvedQuotaPlacement"
      :style="quotaPopoverStyle"
      @mouseenter="openQuotaDetails"
      @mouseleave="closeQuotaDetails"
    >
      <span v-if="officialQuota?.available" class="sb-conn-popover__quota-list">
        <span v-for="row in quotaDetailRows" :key="row.key" class="sb-conn-popover__quota-row">
          <span
            class="sb-conn-popover__quota-meter"
            :class="quotaRingTone(row.window)"
            :style="quotaRingStyle(row.window)"
            aria-hidden="true"
          >
            <span />
          </span>
          <span class="sb-conn-popover__quota-foot">
            <span>{{ quotaRemainingLine(row.window, row.suffix) }}</span>
            <span>刷新 {{ formatUnixSeconds(row.window?.resetsAt) }}</span>
          </span>
        </span>
      </span>
      <span v-if="officialQuota?.rateLimitReachedType" class="sb-conn-popover__warn">
        已触发 {{ officialQuota.rateLimitReachedType }}
      </span>
      <span v-if="resetCreditAvailableCount > 0" class="sb-conn-popover__warn">
        重置次数可用 {{ resetCreditAvailableCount }} 次
      </span>
      <span v-if="accountUsageLine" class="sb-conn-popover__warn">
        {{ accountUsageLine }}
      </span>
      <span v-if="officialQuota?.usageError" class="sb-conn-popover__warn">
        {{ officialQuota.usageError }}
      </span>
      <span v-if="officialQuota?.error" class="sb-conn-popover__error">
        {{ officialQuota.error }}
      </span>
      <button
        v-if="codexLoginNeedsAction"
        type="button"
        class="ui-button ui-button--ghost"
        data-agent-id="provider-connection.codex-login"
        :disabled="codexLoginStarting"
        @click.stop.prevent="startCodexLogin"
      >
        <Loader2
          v-if="codexLoginStarting"
          :size="12"
          class="is-spinning"
          aria-hidden="true"
        />
        <LogIn v-else :size="12" aria-hidden="true" />
        {{ codexLoginStarting ? "启动中..." : "登录" }}
      </button>
    </span>
  </Teleport>
  <Teleport to="body">
    <span
      v-if="updateTooltipOpen"
      :id="`${popoverId}-update`"
      ref="updatePopoverEl"
      role="tooltip"
      class="sb-conn-popover sb-conn-popover--update"
      :class="`sb-conn-popover--${resolvedUpdatePlacement}`"
      :data-placement="resolvedUpdatePlacement"
      :style="updatePopoverStyle"
      @mouseenter="openUpdateDetails"
      @mouseleave="closeUpdateDetails"
    >
      <span class="sb-conn-popover__update-title">{{ codexUpdateTitle }}</span>
      <span v-if="codexReleaseNotes.length" class="sb-conn-popover__update-list">
        <span v-for="note in codexReleaseNotes" :key="note">{{ note }}</span>
      </span>
      <span v-else class="sb-conn-popover__warn">暂未获取更新内容。</span>
      <span v-if="codexUpdateErrorText" class="sb-conn-popover__error">
        {{ codexUpdateErrorText }}
      </span>
    </span>
  </Teleport>
</template>

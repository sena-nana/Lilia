<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, type CSSProperties } from "vue";
import type { ChartData, ChartOptions, ChartType, TooltipItem } from "chart.js";
import AlertTriangle from "@lucide/vue/dist/esm/icons/triangle-alert.mjs";
import Coins from "@lucide/vue/dist/esm/icons/coins.mjs";
import Database from "@lucide/vue/dist/esm/icons/database.mjs";
import RotateCcw from "@lucide/vue/dist/esm/icons/rotate-ccw.mjs";
import RefreshCw from "@lucide/vue/dist/esm/icons/refresh-cw.mjs";
import {
  type CodexAccountUsageDailyBucket,
  type CodexAccountQuotaWindow,
  type CodexAccountQuotaStatus,
  connectionModeUsesCodexAccount,
  codexRateLimitResetCreditConsumeOutcomeLabel,
  DEFAULT_QUOTA_USAGE_STATS_DAYS,
  QUOTA_USAGE_STATS_BACKEND_FILTERS,
  QUOTA_USAGE_STATS_DAYS,
  type QuotaUsageStats,
  type QuotaUsageStatsBackendFilter,
  type QuotaUsageStatsDays,
  quotaUsageStatsBackendFilterLabel,
  quotaUsageStatsDaysLabel,
} from "@lilia/contracts";
import QuotaChartCanvas from "./QuotaChartCanvas.vue";
import {
  consumeCodexRateLimitResetCredit,
  getCodexAccountQuotaStatus,
  getQuotaUsageStats,
} from "../../services/chat";
import {
  getNativeQuotaSurface,
  type NativeQuotaSurface,
} from "../../services/nativeAgent";
import {
  codexQuotaUnavailableStatus,
  clampPercent,
  formatCompactNumber,
  formatDateTime,
  formatPercent,
  formatUnixSeconds,
} from "../../utils/quotaDisplay";

type BackendOption = { value: QuotaUsageStatsBackendFilter; label: string };
type DaysOption = { value: QuotaUsageStatsDays; label: string };
type QuotaBreakdownItem = {
  key: string;
  label: string;
  value: number;
  meta: string;
  color: string;
};
type AccountUsageHeatmapCell = CodexAccountUsageDailyBucket & {
  level: number;
  weekStart: string;
};
type OfficialQuotaWindowRow = {
  key: string;
  label: string;
  resetsAt: number | null;
  remainingLabel: string;
  barStyle: CSSProperties;
};
type NumericChartData = ChartData<ChartType, number[], string>;

const backendOptions: BackendOption[] = [
  ...QUOTA_USAGE_STATS_BACKEND_FILTERS.map((value) => ({
    value,
    label: quotaUsageStatsBackendFilterLabel(value),
  })),
];
const daysOptions: DaysOption[] = [
  ...QUOTA_USAGE_STATS_DAYS.map((value) => ({ value, label: quotaUsageStatsDaysLabel(value) })),
];
const maxStatsDays = Math.max(...QUOTA_USAGE_STATS_DAYS);
const chartPalette = {
  input: "var(--accent)",
  output: "var(--ok)",
  cacheRead: "var(--warn)",
  cacheCreate: "var(--text-muted)",
  total: "var(--text)",
  grid: "var(--border-soft)",
  muted: "var(--text-muted)",
};
const breakdownPalette = [
  "var(--accent)",
  "var(--ok)",
  "var(--warn)",
  "var(--text)",
  "var(--text-muted)",
];
const accountUsageHeatmapLayout = {
  cellSize: 12,
  gap: 3,
  minWeeks: 53,
} as const;

const selectedBackend = ref<QuotaUsageStatsBackendFilter>("all");
const selectedDays = ref<QuotaUsageStatsDays>(DEFAULT_QUOTA_USAGE_STATS_DAYS);
const stats = ref<QuotaUsageStats | null>(null);
const officialQuota = ref<CodexAccountQuotaStatus | null>(null);
const nativeQuota = ref<NativeQuotaSurface | null>(null);
const loading = ref(false);
const quotaLoading = ref(false);
const nativeQuotaLoading = ref(false);
const resetCreditLoading = ref(false);
const resetCreditMessage = ref("");
const resetCreditError = ref("");
const error = ref("");
let requestSeq = 0;
let quotaRequestSeq = 0;
let nativeQuotaRequestSeq = 0;
let disposed = false;

const totalRecords = computed(() => stats.value?.cost.totalRecordCount ?? 0);
const hasUsage = computed(() => totalRecords.value > 0);
const totalTokens = computed(() => stats.value?.totals.totalTokens ?? 0);
const refreshing = computed(
  () => loading.value || quotaLoading.value || nativeQuotaLoading.value,
);
const remoteQuotaUnavailable = computed(
  () => nativeQuota.value?.remoteQuotaApi === "unavailable",
);
const CREDENTIAL_HEALTH_LABELS: Record<string, string> = {
  none: "无凭据",
  active: "可用",
  expired: "已过期",
  revoked: "已撤销",
  unsupported_for_custom_runtime: "不支持自定义运行时",
  unavailable: "不可用",
};
const showOfficialQuota = computed(() =>
  connectionModeUsesCodexAccount(officialQuota.value?.connectionMode),
);
const officialQuotaGroups = computed(() => [
  {
    key: "codex",
    label: "通用额度",
    rows: [
      officialQuotaWindowRow("fiveHour", "5 小时使用限额", officialQuota.value?.fiveHour),
      officialQuotaWindowRow("weekly", "每周使用限制", officialQuota.value?.weekly),
    ].filter(isOfficialQuotaWindowRow),
  },
  {
    key: "spark",
    label: "Spark额度",
    rows: [
      officialQuotaWindowRow("sparkFiveHour", "5 小时使用限额", officialQuota.value?.sparkFiveHour),
      officialQuotaWindowRow("sparkWeekly", "每周使用限制", officialQuota.value?.sparkWeekly),
    ].filter(isOfficialQuotaWindowRow),
  },
].filter((group) => group.rows.length));
const hasOfficialQuotaWindow = computed(() => officialQuotaGroups.value.length > 0);
const resetCreditAvailableCount = computed(() =>
  officialQuota.value?.rateLimitResetCredits?.availableCount ?? 0,
);
const canConsumeResetCredit = computed(() =>
  showOfficialQuota.value &&
  Boolean(officialQuota.value?.available) &&
  resetCreditAvailableCount.value > 0 &&
  !quotaLoading.value &&
  !resetCreditLoading.value,
);
const accountUsageSummaryRows = computed(() => {
  const summary = officialQuota.value?.accountUsage?.summary;
  if (!summary) return [];
  return [
    {
      key: "lifetime",
      label: "累计 Token 数",
      value: accountUsageTokenValue(summary.lifetimeTokens),
    },
    {
      key: "peak",
      label: "峰值 Token 数",
      value: accountUsageTokenValue(summary.peakDailyTokens),
    },
    {
      key: "longestTurn",
      label: "最长任务时长",
      value: accountUsageDurationValue(summary.longestRunningTurnSec),
    },
    {
      key: "currentStreak",
      label: "当前连续天数",
      value: accountUsageDaysValue(summary.currentStreakDays),
    },
    {
      key: "longestStreak",
      label: "最长连续天数",
      value: accountUsageDaysValue(summary.longestStreakDays),
    },
  ];
});
const accountUsageBuckets = computed(() =>
  [...(officialQuota.value?.accountUsage?.dailyUsageBuckets ?? [])]
    .filter((bucket) => isDateOnly(bucket.startDate))
    .sort((a, b) => a.startDate.localeCompare(b.startDate)),
);
const accountUsageWeeks = computed(() => buildAccountUsageWeeks(accountUsageBuckets.value));
const accountUsageMonthLabels = computed(() =>
  buildAccountUsageMonthLabels(accountUsageWeeks.value),
);
const accountUsageHeatmapStyle = computed<CSSProperties>(() => {
  const weekCount = Math.max(1, accountUsageWeeks.value.length);
  const { cellSize, gap } = accountUsageHeatmapLayout;
  const weekGridWidth =
    weekCount * cellSize + (weekCount - 1) * gap;
  return {
    "--quota-heatmap-week-count": weekCount,
    "--quota-heatmap-cell-size": `${cellSize}px`,
    "--quota-heatmap-gap": `${gap}px`,
    "--quota-heatmap-max-width": `${weekGridWidth}px`,
  };
});
const sortedBackends = computed(() =>
  [...(stats.value?.backends ?? [])].sort((a, b) => b.totalTokens - a.totalTokens),
);
const projectBreakdown = computed(() =>
  topBreakdown(
    stats.value?.projects ?? [],
    (row) => row.projectId ?? "__unassigned__",
    (row) => row.projectName,
    (row) => row.totalTokens,
    (row) => `${formatCompactNumber(row.totalTokens)} tokens`,
  ),
);
const conversationBreakdown = computed(() =>
  topBreakdown(
    stats.value?.conversations ?? [],
    (row) => row.taskId,
    (row) => row.taskTitle,
    (row) => row.totalTokens,
    (row) => `${formatCompactNumber(row.totalTokens)} tokens`,
  ),
);
const toolBreakdown = computed(() =>
  topBreakdown(
    stats.value?.tools ?? [],
    (row) => row.key,
    (row) => row.label,
    (row) => row.callCount,
    (row) => `${formatNumber(row.callCount)} 次`,
  ),
);
const usageTrendChartLabel = computed(
  () => `Token 用量趋势（${quotaUsageStatsBackendFilterLabel(selectedBackend.value)} · 近 ${selectedDays.value} 天）`,
);
const usageTrendChartData = computed<NumericChartData>(() => {
  const daily = stats.value?.daily ?? [];
  return {
    labels: daily.map((bucket) => formatDay(bucket.dayStart)),
    datasets: [
      tokenBarDataset("输入", daily.map((bucket) => bucket.inputTokens), chartPalette.input),
      tokenBarDataset("输出", daily.map((bucket) => bucket.outputTokens), chartPalette.output),
      tokenBarDataset("缓存命中", daily.map((bucket) => bucket.cacheReadTokens), chartPalette.cacheRead),
      tokenBarDataset("缓存写入", daily.map((bucket) => bucket.cacheCreationTokens), chartPalette.cacheCreate),
      {
        type: "line",
        label: "总量",
        data: daily.map((bucket) => bucket.totalTokens),
        yAxisID: "total",
        borderColor: chartPalette.total,
        backgroundColor: chartPalette.total,
        borderWidth: 2,
        pointRadius: 2,
        pointHoverRadius: 3,
        tension: 0.25,
        fill: false,
        order: 0,
      },
    ],
  };
});
const usageTrendChartOptions = computed<ChartOptions>(() => ({
  interaction: {
    intersect: false,
    mode: "index",
  },
  plugins: {
    legend: {
      position: "bottom",
      labels: {
        boxWidth: 10,
        boxHeight: 10,
        color: chartPalette.muted,
        usePointStyle: true,
      },
    },
    tooltip: {
      callbacks: {
        label(context: TooltipItem<ChartType>) {
          const value = typeof context.parsed.y === "number" ? context.parsed.y : context.parsed;
          return `${context.dataset.label ?? ""}: ${formatCompactNumber(Number(value))} tokens`;
        },
        footer(items: TooltipItem<ChartType>[]) {
          const index = items[0]?.dataIndex;
          const bucket = typeof index === "number" ? stats.value?.daily[index] : null;
          if (!bucket) return "";
          return [
            `成本 ${formatRecordCost(bucket.knownCostUsd)}`,
            `记录 ${formatNumber(bucket.recordCount)}`,
          ];
        },
      },
    },
  },
  scales: {
    x: {
      stacked: true,
      grid: {
        display: false,
      },
      ticks: {
        autoSkip: selectedDays.value === maxStatsDays,
        maxRotation: 0,
        color: chartPalette.muted,
      },
    },
    y: {
      stacked: true,
      beginAtZero: true,
      grid: {
        color: chartPalette.grid,
      },
      ticks: {
        color: chartPalette.muted,
        callback(value) {
          return formatCompactNumber(Number(value));
        },
      },
    },
    total: {
      display: false,
      beginAtZero: true,
      grid: {
        display: false,
      },
    },
  },
}));
const backendChartData = computed<NumericChartData>(() => ({
  labels: sortedBackends.value.map((row) => quotaUsageStatsBackendFilterLabel(row.backend)),
  datasets: [
    {
      label: "Token",
      data: sortedBackends.value.map((row) => row.totalTokens),
      backgroundColor: sortedBackends.value.map((_, index) =>
        breakdownPalette[index % breakdownPalette.length]
      ),
      borderWidth: 0,
      borderRadius: 4,
    },
  ],
}));
const backendChartOptions = computed<ChartOptions>(() => ({
  indexAxis: "y",
  plugins: {
    legend: {
      display: false,
    },
    tooltip: {
      callbacks: {
        label(context: TooltipItem<ChartType>) {
          const value = Number(context.raw ?? 0);
          return `${formatCompactNumber(value)} tokens · ${backendShareText(value)}`;
        },
      },
    },
  },
  scales: {
    x: {
      beginAtZero: true,
      grid: {
        color: chartPalette.grid,
      },
      ticks: {
        color: chartPalette.muted,
        callback(value) {
          return formatCompactNumber(Number(value));
        },
      },
    },
    y: {
      grid: {
        display: false,
      },
      ticks: {
        color: chartPalette.muted,
      },
    },
  },
}));
const projectBreakdownChartData = computed(() => breakdownChartData(projectBreakdown.value));
const conversationBreakdownChartData = computed(() => breakdownChartData(conversationBreakdown.value));
const toolBreakdownChartData = computed(() => breakdownChartData(toolBreakdown.value));
const breakdownChartOptions = computed<ChartOptions>(() => ({
  cutout: "62%",
  plugins: {
    legend: {
      display: false,
    },
    tooltip: {
      callbacks: {
        label(context: TooltipItem<ChartType>) {
          const value = Number(context.raw ?? 0);
          return `${context.label}: ${formatCompactNumber(value)} · ${breakdownShareText(context)}`;
        },
      },
    },
  },
}));
const costText = computed(() => {
  const coverage = stats.value?.cost;
  if (!coverage || coverage.totalRecordCount === 0 || coverage.knownCostUsd === null) return "--";
  const value = formatCost(coverage.knownCostUsd);
  return coverage.costRecordCount < coverage.totalRecordCount ? `${value} 已知` : value;
});
const costCoverageText = computed(() => {
  const coverage = stats.value?.cost;
  if (!coverage || coverage.totalRecordCount === 0) return "无新增记录";
  if (coverage.costRecordCount === 0) return "未上报成本";
  if (coverage.costRecordCount === coverage.totalRecordCount) return "成本记录完整";
  return `${coverage.costRecordCount}/${coverage.totalRecordCount} 条记录含成本`;
});

function formatNumber(value: number) {
  return new Intl.NumberFormat("zh-CN").format(value);
}

function formatCost(value: number) {
  return `$${value.toFixed(value >= 1 ? 2 : 4)}`;
}

function formatRecordCost(value: number | null) {
  return value === null ? "--" : formatCost(value);
}

function topBreakdown<T>(
  rows: T[],
  keyOf: (row: T) => string,
  labelOf: (row: T) => string,
  valueOf: (row: T) => number,
  metaOf: (row: T) => string,
): QuotaBreakdownItem[] {
  const normalized = rows
    .map((row) => ({
      key: keyOf(row),
      label: labelOf(row),
      value: Math.max(0, valueOf(row)),
      meta: metaOf(row),
    }))
    .filter((row) => row.value > 0)
    .sort((a, b) => b.value - a.value)
    .slice(0, 5);
  return normalized.map((row, index) => ({
    ...row,
    color: breakdownPalette[index % breakdownPalette.length],
  }));
}

function formatDay(value: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(value));
}

function tokenBarDataset(label: string, data: number[], color: string) {
  return {
    type: "bar" as const,
    label,
    data,
    backgroundColor: color,
    borderColor: color,
    borderWidth: 0,
    borderRadius: 3,
    stack: "tokens",
  };
}

function backendShareText(tokens: number) {
  if (totalTokens.value <= 0) return "0%";
  return `${Math.round((tokens / totalTokens.value) * 100)}%`;
}

function breakdownChartData(items: QuotaBreakdownItem[]): NumericChartData {
  return {
    labels: items.map((item) => item.label),
    datasets: [
      {
        data: items.map((item) => item.value),
        backgroundColor: items.map((item) => item.color),
        borderColor: "var(--bg)",
        borderWidth: 2,
        hoverOffset: 3,
      },
    ],
  };
}

function breakdownShareText(context: TooltipItem<ChartType>) {
  const values = context.dataset.data.map((value) => Number(value) || 0);
  const total = values.reduce((sum, value) => sum + value, 0);
  if (total <= 0) return "0%";
  return `${Math.round((Number(context.raw ?? 0) / total) * 100)}%`;
}

function resetCreditText(count: number) {
  return count > 0 ? `可用 ${formatNumber(count)} 次` : "暂无可用重置次数";
}

function officialQuotaWindowRow(
  key: string,
  label: string,
  window: CodexAccountQuotaWindow | null | undefined,
): OfficialQuotaWindowRow | null {
  if (!window) return null;
  const remainingPercent = clampPercent(100 - window.usedPercent);
  return {
    key,
    label,
    resetsAt: window.resetsAt,
    remainingLabel: `剩余 ${formatPercent(remainingPercent)}`,
    barStyle: {
      "--quota-official-window-value": `${remainingPercent}%`,
    },
  };
}

function isOfficialQuotaWindowRow(row: OfficialQuotaWindowRow | null): row is OfficialQuotaWindowRow {
  return Boolean(row);
}

function accountUsageTokenValue(value: number | null) {
  if (value === null) return "--";
  return formatCompactNumber(value);
}

function accountUsageDaysValue(value: number | null) {
  if (value === null) return "--";
  return `${formatNumber(value)} 天`;
}

function accountUsageDurationValue(value: number | null) {
  if (value === null) return "--";
  const totalMinutes = Math.max(0, Math.round(value / 60));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours > 0 && minutes > 0) return `${formatNumber(hours)} 小时 ${minutes} 分`;
  if (hours > 0) return `${formatNumber(hours)} 小时`;
  return `${formatNumber(minutes)} 分钟`;
}

function buildAccountUsageWeeks(days: readonly CodexAccountUsageDailyBucket[]) {
  if (!days.length) return [];
  const maxTokens = Math.max(1, ...days.map((day) => day.tokens));
  const firstWeekStart = weekStartDate(days[0].startDate);
  const lastWeekStart = weekStartDate(days[days.length - 1].startDate);
  const start = new Date(lastWeekStart);
  start.setUTCDate(start.getUTCDate() - (accountUsageHeatmapLayout.minWeeks - 1) * 7);
  if (firstWeekStart < start) start.setTime(firstWeekStart.getTime());
  const end = new Date(lastWeekStart);
  end.setUTCDate(end.getUTCDate() + 6);
  const byDate = new Map(days.map((day) => [day.startDate, day]));
  const weeks: AccountUsageHeatmapCell[][] = [];
  for (const cursor = new Date(start); cursor <= end; cursor.setUTCDate(cursor.getUTCDate() + 7)) {
    const week: AccountUsageHeatmapCell[] = [];
    for (let offset = 0; offset < 7; offset += 1) {
      const date = new Date(cursor);
      date.setUTCDate(cursor.getUTCDate() + offset);
      const key = date.toISOString().slice(0, 10);
      const day = byDate.get(key) ?? { startDate: key, tokens: 0 };
      week.push({
        ...day,
        level: accountUsageHeatLevel(day.tokens, maxTokens),
        weekStart: cursor.toISOString().slice(0, 10),
      });
    }
    weeks.push(week);
  }
  return weeks;
}

function buildAccountUsageMonthLabels(weeks: readonly AccountUsageHeatmapCell[][]) {
  const monthStarts: { index: number; key: string; month: string }[] = [];
  let lastMonth = "";
  weeks.forEach((week, index) => {
    const weekStart = week[0]?.weekStart ?? String(index);
    const month = week
      .map((day) => day.startDate.slice(0, 7))
      .find((value) => value && value !== lastMonth) ?? "";
    if (month) lastMonth = month;
    if (month && index > 0 && index < weeks.length - 1) {
      monthStarts.push({ index, key: weekStart, month });
    }
  });
  return monthStarts.map((start, index) => {
    const endIndex = Math.min(monthStarts[index + 1]?.index ?? weeks.length, weeks.length - 1);
    return {
      key: start.key,
      label: formatAccountUsageMonth(start.month),
      style: { gridColumn: `${start.index + 1} / span ${Math.max(1, endIndex - start.index)}` },
    };
  });
}

function accountUsageHeatLevel(tokens: number, maxTokens: number) {
  if (tokens <= 0) return 0;
  return Math.min(4, Math.max(1, Math.ceil((tokens / maxTokens) * 4)));
}

function accountUsageHeatTitle(day: CodexAccountUsageDailyBucket) {
  const tokens = Math.max(0, Math.trunc(day.tokens));
  const tokenText = tokens >= 100_000_000 ? `${(tokens / 100_000_000).toFixed(3)} 亿` : formatNumber(tokens);
  return `${day.startDate}: ${tokenText} tokens`;
}

function formatAccountUsageMonth(month: string) {
  const [, rawMonth] = month.split("-");
  return `${Number(rawMonth)}月`;
}

function weekStartDate(date: string) {
  const result = new Date(`${date}T00:00:00Z`);
  result.setUTCDate(result.getUTCDate() - result.getUTCDay());
  return result;
}

function isDateOnly(value: string) {
  return /^\d{4}-\d{2}-\d{2}$/.test(value);
}

function createIdempotencyKey() {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

async function loadStats() {
  const seq = ++requestSeq;
  loading.value = true;
  error.value = "";
  try {
    const result = await getQuotaUsageStats({
      days: selectedDays.value,
      backend: selectedBackend.value,
    });
    if (!disposed && seq === requestSeq) stats.value = result;
  } catch (err) {
    if (!disposed && seq === requestSeq) error.value = String(err);
  } finally {
    if (!disposed && seq === requestSeq) loading.value = false;
  }
}

async function loadOfficialQuota() {
  const seq = ++quotaRequestSeq;
  quotaLoading.value = true;
  try {
    const result = await getCodexAccountQuotaStatus();
    if (!disposed && seq === quotaRequestSeq) officialQuota.value = result;
  } catch (err) {
    if (!disposed && seq === quotaRequestSeq) {
      officialQuota.value = codexQuotaUnavailableStatus(err);
    }
  } finally {
    if (!disposed && seq === quotaRequestSeq) quotaLoading.value = false;
  }
}

async function loadNativeQuotaSurface() {
  const seq = ++nativeQuotaRequestSeq;
  nativeQuotaLoading.value = true;
  try {
    const result = await getNativeQuotaSurface();
    if (!disposed && seq === nativeQuotaRequestSeq) nativeQuota.value = result;
  } catch {
    if (!disposed && seq === nativeQuotaRequestSeq) nativeQuota.value = null;
  } finally {
    if (!disposed && seq === nativeQuotaRequestSeq) nativeQuotaLoading.value = false;
  }
}

async function consumeResetCredit() {
  if (disposed || !canConsumeResetCredit.value) return;
  resetCreditLoading.value = true;
  resetCreditMessage.value = "";
  resetCreditError.value = "";
  try {
    const result = await consumeCodexRateLimitResetCredit({
      idempotencyKey: createIdempotencyKey(),
    });
    if (disposed) return;
    officialQuota.value = result.status;
    resetCreditMessage.value = codexRateLimitResetCreditConsumeOutcomeLabel(result.outcome);
  } catch (err) {
    if (!disposed) resetCreditError.value = String(err);
  } finally {
    if (!disposed) resetCreditLoading.value = false;
  }
}

async function refreshAll() {
  if (disposed) return;
  resetCreditMessage.value = "";
  resetCreditError.value = "";
  await Promise.all([loadStats(), loadOfficialQuota(), loadNativeQuotaSurface()]);
}

async function selectBackend(backend: QuotaUsageStatsBackendFilter) {
  if (selectedBackend.value === backend) return;
  selectedBackend.value = backend;
  await loadStats();
}

async function selectDays(days: QuotaUsageStatsDays) {
  if (selectedDays.value === days) return;
  selectedDays.value = days;
  await loadStats();
}

onMounted(() => {
  disposed = false;
  void refreshAll();
});

onBeforeUnmount(() => {
  disposed = true;
  requestSeq += 1;
  quotaRequestSeq += 1;
});
</script>

<template>
  <div class="quota-toolbar" data-agent-id="settings.quota">
    <div class="ui-segmented" role="radiogroup" aria-label="额度统计后端">
      <button
        v-for="opt in backendOptions"
        :key="opt.value"
        type="button"
        role="radio"
        :data-agent-id="`settings.quota.backend.${opt.value}`"
        :aria-checked="selectedBackend === opt.value"
        :class="{ 'is-active': selectedBackend === opt.value }"
        :disabled="loading"
        @click="selectBackend(opt.value)"
      >
        {{ opt.label }}
      </button>
    </div>
    <div class="quota-toolbar__actions">
      <button
        type="button"
        class="ui-button ui-button--ghost quota-panel__refresh"
        data-agent-id="settings.quota.refresh"
        :disabled="refreshing"
        @click="refreshAll"
      >
        <RefreshCw :size="12" :class="{ 'is-spinning': refreshing }" aria-hidden="true" />
        刷新
      </button>
      <div class="ui-segmented" role="radiogroup" aria-label="额度统计时间范围">
        <button
          v-for="opt in daysOptions"
          :key="opt.value"
          type="button"
          role="radio"
          :data-agent-id="`settings.quota.days.${opt.value}`"
          :aria-checked="selectedDays === opt.value"
          :class="{ 'is-active': selectedDays === opt.value }"
          :disabled="loading"
          @click="selectDays(opt.value)"
        >
          {{ opt.label }}
        </button>
      </div>
    </div>
  </div>

  <div v-if="error" class="card quota-panel">
    <div class="conn-banner conn-banner--err">
      <AlertTriangle :size="16" aria-hidden="true" />
      <div>
        <div class="conn-banner__title">额度统计</div>
        <div class="conn-banner__hint">{{ error }}</div>
      </div>
    </div>
  </div>

  <div
    v-if="nativeQuota"
    class="card quota-panel quota-native"
    data-agent-id="settings.quota.native"
  >
    <div class="quota-official__head">
      <div class="quota-official__title">
        <h2>
          <span class="card-h2__title">Credential Broker / Native</span>
        </h2>
        <div class="quota-official__meta">
          <span>{{ nativeQuota.source }}</span>
          <span v-if="nativeQuotaLoading">读取中</span>
          <span
            class="ui-badge"
            :class="remoteQuotaUnavailable ? 'ui-badge--muted' : 'ui-badge--ok'"
            data-agent-id="settings.quota.native.remote-api"
          >
            远程额度 API：{{ remoteQuotaUnavailable ? "不可用" : "可用" }}
          </span>
        </div>
      </div>
    </div>
    <p class="muted" data-agent-id="settings.quota.native.note">
      {{ nativeQuota.remoteQuotaNote }}
    </p>
    <p class="muted">{{ nativeQuota.localUsageNote }}</p>
    <ul class="kv quota-native__diagnostics" data-agent-id="settings.quota.native.diagnostics">
      <li>
        <span>Broker</span>
        <span>{{ nativeQuota.credential.brokerReady ? "就绪" : "未就绪" }}</span>
      </li>
      <li>
        <span>可用凭据</span>
        <span>
          {{ nativeQuota.credential.activeCount }} / {{ nativeQuota.credential.credentialCount }}
        </span>
      </li>
      <li>
        <span>订阅 ≠ API 额度</span>
        <span>{{ nativeQuota.subscriptionNotEquatedToApiQuota ? "已隔离" : "未隔离" }}</span>
      </li>
    </ul>
    <div class="quota-native__providers">
      <div
        v-for="row in nativeQuota.providers"
        :key="row.providerId"
        class="quota-native__provider"
        :data-agent-id="`settings.quota.native.provider.${row.providerId}`"
      >
        <div class="quota-native__provider-head">
          <strong>{{ row.displayName }}</strong>
          <span class="ui-badge ui-badge--muted">
            {{ CREDENTIAL_HEALTH_LABELS[row.credentialHealth] ?? row.credentialHealth }}
          </span>
          <span class="ui-badge ui-badge--muted">额度 API 不可用</span>
        </div>
        <p class="muted">{{ row.note }}</p>
        <ul class="kv">
          <li v-for="limit in row.knownLimits" :key="limit.kind">
            <span>{{ limit.label }}</span>
            <span>
              <template v-if="limit.value != null">{{ formatNumber(limit.value) }}</template>
              <template v-else>{{ limit.note }}</template>
            </span>
          </li>
        </ul>
      </div>
    </div>
  </div>

  <div
    v-if="showOfficialQuota && (accountUsageSummaryRows.length || accountUsageBuckets.length || officialQuota?.usageError)"
    class="quota-official-usage"
  >
    <div v-if="accountUsageSummaryRows.length" class="quota-official-usage__metrics">
      <div
        v-for="row in accountUsageSummaryRows"
        :key="row.key"
        class="quota-official-usage__metric"
      >
        <strong>{{ row.value }}</strong>
        <span>{{ row.label }}</span>
      </div>
    </div>
    <div v-if="officialQuota?.usageError" class="quota-official-usage__head">
      <span>{{ officialQuota.usageError }}</span>
    </div>
    <div
      v-if="accountUsageWeeks.length"
      class="quota-official-usage__heatmap"
      aria-label="官方账号每日 Token 活动"
    >
      <div class="quota-official-usage__heatmap-window">
        <div class="quota-official-usage__heatmap-grid" :style="accountUsageHeatmapStyle">
          <div class="quota-official-usage__weeks">
            <div
              v-for="week in accountUsageWeeks"
              :key="week[0]?.weekStart"
              class="quota-official-usage__week"
            >
              <span
                v-for="day in week"
                :key="day.startDate"
                class="quota-official-usage__day"
                :class="`quota-official-usage__day--${day.level}`"
                :title="accountUsageHeatTitle(day)"
                :aria-label="accountUsageHeatTitle(day)"
              />
            </div>
          </div>
          <div class="quota-official-usage__months" aria-hidden="true">
            <span
              v-for="month in accountUsageMonthLabels"
              :key="month.key"
              class="quota-official-usage__month"
              :style="month.style"
            >
              {{ month.label }}
            </span>
          </div>
        </div>
      </div>
    </div>
    <div v-else-if="!officialQuota?.usageError" class="quota-official-usage__empty">
      暂无 Token 活动
    </div>
  </div>

  <div v-if="showOfficialQuota" class="card quota-panel quota-official">
    <div class="quota-official__head">
      <div class="quota-official__title">
        <h2>
          <span class="card-h2__title">Codex 官方额度</span>
        </h2>
        <div class="quota-official__meta">
          <span>
            {{ officialQuota?.planType ? `计划 ${officialQuota.planType}` : "官方账号" }}
            <template v-if="officialQuota?.limitName"> · {{ officialQuota.limitName }}</template>
          </span>
          <span>
            {{ quotaLoading ? "读取中" : `查询 ${formatDateTime(officialQuota?.fetchedAt ?? Date.now())}` }}
          </span>
          <span>重置次数 {{ resetCreditText(resetCreditAvailableCount) }}</span>
        </div>
      </div>
      <div class="quota-official__actions">
        <button
          type="button"
          class="ui-button ui-button--ghost quota-official__reset"
          data-agent-id="settings.quota.reset-credit.consume"
          :disabled="!canConsumeResetCredit"
          @click="consumeResetCredit"
        >
          <RotateCcw :size="12" :class="{ 'is-spinning': resetCreditLoading }" aria-hidden="true" />
          {{ resetCreditLoading ? "使用中" : "使用重置次数" }}
        </button>
      </div>
    </div>
    <div
      v-for="group in officialQuotaGroups"
      :key="group.key"
      class="quota-official__section"
    >
      <strong v-if="group.label">{{ group.label }}</strong>
      <div class="quota-official__grid">
        <div
          v-for="row in group.rows"
          :key="row.key"
          class="quota-official-window"
          role="group"
          :aria-label="`${group.label}${row.label}`"
        >
          <div class="quota-official-window__main">
            <strong>{{ row.label }}</strong>
            <small>重置时间：{{ formatUnixSeconds(row.resetsAt) }}</small>
          </div>
          <div class="quota-official-window__usage">
            <span
              class="quota-official-window__bar"
              :style="row.barStyle"
              aria-hidden="true"
            />
            <strong>{{ row.remainingLabel }}</strong>
          </div>
        </div>
      </div>
    </div>
    <div v-if="!hasOfficialQuotaWindow" class="quota-official__empty">
      暂无官方额度数据
    </div>
    <div v-if="resetCreditMessage" class="quota-official__message">
      {{ resetCreditMessage }}
    </div>
    <div v-if="resetCreditError" class="quota-official__error">
      {{ resetCreditError }}
    </div>
    <div v-if="officialQuota?.error" class="quota-official__error">
      {{ officialQuota.error }}
    </div>
  </div>

  <div class="card quota-panel">
    <h2>
      <span class="card-h2__title">统计摘要</span>
      <span class="ui-badge ui-badge--muted">{{ quotaUsageStatsBackendFilterLabel(selectedBackend) }} · 近 {{ selectedDays }} 天</span>
    </h2>

    <div class="quota-metrics" aria-label="额度统计摘要">
      <div class="quota-metric">
        <span>总 Token</span>
        <strong>{{ formatNumber(stats?.totals.totalTokens ?? 0) }}</strong>
      </div>
      <div class="quota-metric">
        <span>输入</span>
        <strong>{{ formatNumber(stats?.totals.inputTokens ?? 0) }}</strong>
      </div>
      <div class="quota-metric">
        <span>输出</span>
        <strong>{{ formatNumber(stats?.totals.outputTokens ?? 0) }}</strong>
      </div>
      <div class="quota-metric">
        <span>缓存命中</span>
        <strong>{{ formatNumber(stats?.totals.cacheReadTokens ?? 0) }}</strong>
      </div>
      <div class="quota-metric">
        <span>缓存写入</span>
        <strong>{{ formatNumber(stats?.totals.cacheCreationTokens ?? 0) }}</strong>
      </div>
      <div class="quota-metric quota-metric--cost">
        <span>已知成本</span>
        <strong>{{ costText }}</strong>
        <small>{{ costCoverageText }}</small>
      </div>
    </div>
  </div>

  <div class="card quota-panel">
    <h2>
      <span class="card-h2__title">
        <Database :size="14" aria-hidden="true" />
        趋势
      </span>
      <span class="ui-badge ui-badge--muted">{{ quotaUsageStatsBackendFilterLabel(selectedBackend) }} · 近 {{ selectedDays }} 天</span>
    </h2>

    <div v-if="!hasUsage && !loading" class="quota-empty">
      暂无新增额度数据
    </div>

    <div v-else class="quota-chart-wrap">
      <QuotaChartCanvas
        class="quota-chart"
        type="bar"
        :label="usageTrendChartLabel"
        :data="usageTrendChartData"
        :options="usageTrendChartOptions"
      />
    </div>
  </div>

  <div class="card quota-panel">
    <h2>
      <span class="card-h2__title">
        <Coins :size="14" aria-hidden="true" />
        后端分布
      </span>
    </h2>

    <div v-if="sortedBackends.length" class="quota-backends">
      <QuotaChartCanvas
        class="quota-backends__chart"
        type="bar"
        label="后端 Token 分布"
        :data="backendChartData"
        :options="backendChartOptions"
      />
      <div class="quota-backends__list">
        <div
          v-for="row in sortedBackends"
          :key="row.backend"
          class="quota-backend-row"
        >
          <span>{{ quotaUsageStatsBackendFilterLabel(row.backend) }}</span>
          <strong>{{ formatCompactNumber(row.totalTokens) }} tokens · {{ backendShareText(row.totalTokens) }}</strong>
        </div>
      </div>
    </div>

    <div v-else class="quota-empty quota-empty--compact">
      暂无后端分布
    </div>
  </div>

  <section class="card quota-panel quota-breakdown" aria-label="项目消耗">
    <h2>
      <span class="card-h2__title">项目消耗</span>
      <span class="ui-badge ui-badge--muted">按 Token</span>
    </h2>
    <div class="quota-breakdown__body">
      <QuotaChartCanvas
        v-if="projectBreakdown.length"
        class="quota-breakdown__chart"
        type="doughnut"
        label="项目消耗图表"
        :data="projectBreakdownChartData"
        :options="breakdownChartOptions"
      />
      <div v-else class="quota-breakdown__chart quota-breakdown__chart--empty" aria-hidden="true" />
      <div class="quota-breakdown__list">
        <div
          v-for="item in projectBreakdown"
          :key="item.key"
          class="quota-breakdown__row"
        >
          <i :style="{ background: item.color }" />
          <span>{{ item.label }}</span>
          <strong>{{ item.meta }}</strong>
        </div>
        <div v-if="projectBreakdown.length === 0" class="quota-breakdown__empty">
          暂无项目消耗
        </div>
      </div>
    </div>
  </section>

  <section class="card quota-panel quota-breakdown" aria-label="对话消耗">
    <h2>
      <span class="card-h2__title">对话消耗</span>
      <span class="ui-badge ui-badge--muted">按 Token</span>
    </h2>
    <div class="quota-breakdown__body">
      <QuotaChartCanvas
        v-if="conversationBreakdown.length"
        class="quota-breakdown__chart"
        type="doughnut"
        label="对话消耗图表"
        :data="conversationBreakdownChartData"
        :options="breakdownChartOptions"
      />
      <div v-else class="quota-breakdown__chart quota-breakdown__chart--empty" aria-hidden="true" />
      <div class="quota-breakdown__list">
        <div
          v-for="item in conversationBreakdown"
          :key="item.key"
          class="quota-breakdown__row"
        >
          <i :style="{ background: item.color }" />
          <span>{{ item.label }}</span>
          <strong>{{ item.meta }}</strong>
        </div>
        <div v-if="conversationBreakdown.length === 0" class="quota-breakdown__empty">
          暂无对话消耗
        </div>
      </div>
    </div>
  </section>

  <section class="card quota-panel quota-breakdown" aria-label="工具活跃度">
    <h2>
      <span class="card-h2__title">工具活跃度</span>
      <span class="ui-badge ui-badge--muted">按调用次数统计</span>
    </h2>
    <div class="quota-breakdown__body">
      <QuotaChartCanvas
        v-if="toolBreakdown.length"
        class="quota-breakdown__chart"
        type="doughnut"
        label="工具活跃度图表"
        :data="toolBreakdownChartData"
        :options="breakdownChartOptions"
      />
      <div v-else class="quota-breakdown__chart quota-breakdown__chart--empty" aria-hidden="true" />
      <div class="quota-breakdown__list">
        <div
          v-for="item in toolBreakdown"
          :key="item.key"
          class="quota-breakdown__row"
        >
          <i :style="{ background: item.color }" />
          <span>{{ item.label }}</span>
          <strong>{{ item.meta }}</strong>
        </div>
        <div v-if="toolBreakdown.length === 0" class="quota-breakdown__empty">
          暂无工具调用
        </div>
      </div>
    </div>
  </section>

  <div v-if="stats?.recent.length" class="card quota-panel">
    <h2>
      <span class="card-h2__title">最近记录</span>
    </h2>
    <div class="quota-recent">
      <div
        v-for="record in stats.recent"
        :key="record.eventId"
        class="quota-recent__row"
      >
        <div class="quota-recent__main">
          <strong>{{ quotaUsageStatsBackendFilterLabel(record.backend) }}</strong>
          <span>{{ formatDateTime(record.createdAt) }}</span>
        </div>
        <span>{{ formatCompactNumber(record.totalTokens) }} tokens</span>
        <span>{{ formatRecordCost(record.knownCostUsd) }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.quota-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-start;
  gap: 8px;
  margin-bottom: 12px;
}

.quota-toolbar__actions {
  min-width: 0;
  display: inline-flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  margin-left: auto;
}

.quota-panel {
  display: grid;
  gap: 12px;
}

.quota-panel h2 {
  min-width: 0;
  margin-bottom: 0;
}

.quota-panel h2 .ui-badge {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quota-panel__refresh {
  height: 28px;
  padding-inline: 8px;
  font-size: 12px;
}

.quota-metrics {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  gap: 8px;
}

.quota-metric {
  min-width: 0;
  padding: 9px 10px;
  border: 1px solid var(--border-soft);
  border-radius: 7px;
  background: var(--bg);
}

.quota-metric span,
.quota-metric small {
  display: block;
  overflow: hidden;
  color: var(--text-muted);
  font-size: 11px;
  line-height: 1.35;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quota-metric strong {
  display: block;
  margin-top: 3px;
  overflow: hidden;
  color: var(--text);
  font-size: 17px;
  font-variant-numeric: tabular-nums;
  line-height: 1.2;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quota-metric--cost strong {
  color: var(--accent);
}

.quota-official {
  display: grid;
  gap: 9px;
}

.quota-native {
  display: grid;
  gap: 10px;
}

.quota-native__diagnostics {
  margin: 0;
}

.quota-native__providers {
  display: grid;
  gap: 10px;
}

.quota-native__provider {
  display: grid;
  gap: 6px;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-subtle);
}

.quota-native__provider-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.quota-official__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 14px;
}

.quota-official__title {
  min-width: 0;
  display: grid;
  gap: 2px;
}

.quota-official__meta,
.quota-official-window__main small,
.quota-official__empty,
.quota-official__message,
.quota-official__error {
  overflow: hidden;
  color: var(--text-muted);
  font-size: 11px;
  line-height: 1.35;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quota-official__meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 12px;
}

.quota-official__meta span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quota-official__actions {
  min-width: 0;
  display: grid;
  justify-items: end;
  justify-content: flex-end;
  gap: 5px;
}

.quota-official__reset {
  height: 28px;
  padding-inline: 8px;
  font-size: 12px;
}

.quota-official__grid {
  display: grid;
  overflow: hidden;
  border: 1px solid var(--border-soft);
  border-radius: 7px;
  background: var(--bg-subtle);
}

.quota-official__section {
  display: grid;
  gap: 7px;
}

.quota-official__section > strong {
  color: var(--text);
  font-size: 12px;
}

.quota-official-window {
  min-width: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(154px, 176px);
  gap: 14px;
  align-items: center;
  padding: 12px 12px;
}

.quota-official-window + .quota-official-window {
  border-top: 1px solid var(--border-soft);
}

.quota-official-window__main,
.quota-official-window__usage {
  min-width: 0;
  display: grid;
}

.quota-official-window__main {
  gap: 4px;
}

.quota-official-window__main strong {
  overflow: hidden;
  color: var(--text);
  font-size: 12px;
  line-height: 1.25;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quota-official-window__usage {
  grid-template-columns: minmax(86px, 1fr) auto;
  gap: 10px;
  align-items: center;
}

.quota-official-window__usage strong {
  color: var(--text-muted);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  line-height: 1.25;
  white-space: nowrap;
}

.quota-official-window__bar {
  position: relative;
  width: 100%;
  height: 6px;
  overflow: hidden;
  border-radius: var(--radius-pill);
  background: color-mix(in srgb, var(--text-muted) 22%, transparent);
}

.quota-official-window__bar::before {
  position: absolute;
  inset-block: 0;
  left: 0;
  width: var(--quota-official-window-value, 0%);
  border-radius: inherit;
  background: var(--text);
  content: "";
}

.quota-official__empty {
  padding: 8px 2px;
}

.quota-official__message {
  color: var(--ok);
}

.quota-official__error {
  color: var(--warn);
}

.quota-official-usage {
  box-sizing: border-box;
  min-width: 0;
  width: min(100%, 792px);
  margin-block: 4px 2px;
  margin-inline: auto;
  padding: 8px 12px 4px;
  display: grid;
  gap: 10px;
}

.quota-official-usage__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  min-width: 0;
}

.quota-official-usage__head span,
.quota-official-usage__empty {
  overflow: hidden;
  color: var(--warn);
  font-size: 11px;
  line-height: 1.35;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quota-official-usage__empty {
  color: var(--text-muted);
}

.quota-official-usage__metrics {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  min-width: 0;
}

.quota-official-usage__metric {
  min-width: 0;
  display: grid;
  gap: 3px;
  justify-items: center;
  padding: 11px 10px 10px;
}

.quota-official-usage__metric span,
.quota-official-usage__metric strong {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quota-official-usage__metric span {
  color: var(--text-muted);
  font-size: 11px;
  line-height: 1.35;
}

.quota-official-usage__metric strong {
  color: var(--text);
  font-size: 13px;
  font-variant-numeric: tabular-nums;
  line-height: 1.2;
}

.quota-official-usage__heatmap {
  min-width: 0;
  display: grid;
  gap: 6px;
}

.quota-official-usage__heatmap-window {
  width: 100%;
  min-width: 0;
  display: flex;
  justify-content: flex-end;
  overflow: hidden;
}

.quota-official-usage__heatmap-grid {
  flex: none;
  width: var(--quota-heatmap-max-width, 100%);
}

.quota-official-usage__months,
.quota-official-usage__weeks {
  display: grid;
  grid-template-columns: repeat(var(--quota-heatmap-week-count, 1), var(--quota-heatmap-cell-size));
  gap: var(--quota-heatmap-gap);
}

.quota-official-usage__months {
  margin-top: 6px;
}

.quota-official-usage__month {
  min-width: 0;
  height: 16px;
  overflow: hidden;
  color: var(--text-muted);
  font-size: 11px;
  line-height: 16px;
  text-align: center;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quota-official-usage__week {
  display: grid;
  grid-template-rows: repeat(7, var(--quota-heatmap-cell-size));
  gap: var(--quota-heatmap-gap);
  justify-items: center;
}

.quota-official-usage__day {
  box-sizing: border-box;
  width: var(--quota-heatmap-cell-size);
  height: var(--quota-heatmap-cell-size);
  border: 1px solid color-mix(in srgb, var(--bg) 22%, transparent);
  border-radius: 3px;
  background: var(--bg-subtle);
}

.quota-official-usage__day--1 {
  background: color-mix(in srgb, var(--accent) 28%, var(--bg-subtle));
}

.quota-official-usage__day--2 {
  background: color-mix(in srgb, var(--accent) 46%, var(--bg-subtle));
}

.quota-official-usage__day--3 {
  background: color-mix(in srgb, var(--accent) 66%, var(--bg-subtle));
}

.quota-official-usage__day--4 {
  background: color-mix(in srgb, var(--accent) 86%, var(--text));
}

.quota-chart-wrap {
  min-width: 0;
  display: grid;
  gap: 10px;
}

.quota-chart {
  height: 248px;
}

.quota-backends {
  display: grid;
  gap: 8px;
}

.quota-backends__chart {
  height: 116px;
}

.quota-backends__list {
  display: grid;
  gap: 6px;
}

.quota-breakdown {
  min-width: 0;
  display: grid;
  gap: 9px;
}

.quota-breakdown__empty {
  overflow: hidden;
  color: var(--text-muted);
  font-size: 11px;
  line-height: 1.35;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quota-breakdown__body {
  display: grid;
  grid-template-columns: 66px minmax(0, 1fr);
  gap: 10px;
  align-items: center;
}

.quota-breakdown__chart {
  width: 66px;
  height: 66px;
}

.quota-breakdown__chart--empty {
  border: 1px solid var(--border-soft);
  border-radius: 50%;
  background: var(--bg-subtle);
}

.quota-breakdown__list {
  min-width: 0;
  display: grid;
  gap: 5px;
}

.quota-breakdown__row {
  min-width: 0;
  display: grid;
  grid-template-columns: 8px minmax(0, 1fr) auto;
  gap: 6px;
  align-items: center;
  color: var(--text-muted);
  font-size: 11px;
}

.quota-breakdown__row i {
  width: 8px;
  height: 8px;
  border-radius: var(--radius-pill);
}

.quota-breakdown__row span,
.quota-breakdown__row strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quota-breakdown__row strong {
  color: var(--text);
  font-variant-numeric: tabular-nums;
}

.quota-backend-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  min-width: 0;
  padding: 0 2px;
  color: var(--text-muted);
  font-size: 12px;
}

.quota-backend-row span,
.quota-backend-row strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quota-backend-row strong {
  color: var(--text);
  font-variant-numeric: tabular-nums;
}

.quota-recent {
  display: grid;
  gap: 1px;
}

.quota-recent__row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  gap: 12px;
  align-items: center;
  min-height: 32px;
  padding: 6px 8px;
  border-radius: var(--radius-sm);
  color: var(--text-muted);
  font-size: 12px;
}

.quota-recent__row:hover {
  background: var(--bg-hover);
}

.quota-recent__main {
  min-width: 0;
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.quota-recent__main strong {
  color: var(--text);
}

.quota-recent__main span,
.quota-recent__row > span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.quota-empty {
  padding: 28px 12px;
  color: var(--text-muted);
  text-align: center;
  font-size: 13px;
}

.quota-empty--compact {
  padding: 12px;
}

@media (max-width: 860px) {
  .quota-toolbar {
    align-items: flex-start;
  }

  .quota-metrics {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .quota-metric--cost {
    grid-column: 1 / -1;
  }

  .quota-official__head {
    flex-wrap: wrap;
    align-items: flex-start;
  }

  .quota-official__actions {
    width: 100%;
    justify-items: start;
    justify-content: stretch;
  }

  .quota-official-window {
    grid-template-columns: 1fr;
    gap: 8px;
  }

  .quota-official-window__usage {
    grid-template-columns: minmax(82px, 1fr) auto;
  }

  .quota-official-usage__metrics {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .quota-recent__row {
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .quota-recent__row > span:last-child {
    display: none;
  }

  .quota-chart {
    height: 220px;
  }

  .quota-breakdown__body {
    grid-template-columns: 74px minmax(0, 1fr);
  }

  .quota-breakdown__chart {
    width: 74px;
    height: 74px;
  }
}
</style>


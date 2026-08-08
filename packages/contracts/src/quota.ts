import { chatBackendLabel, type ChatBackendKind } from "./chat";
import {
  DEFAULT_QUOTA_USAGE_QUERY_SCOPE,
  DEFAULT_QUOTA_USAGE_STATS_DAYS,
  isLiliaQuotaTool as isLiliaQuotaToolImpl,
  isQuotaUsageQueryScope as isQuotaUsageQueryScopeImpl,
  isQuotaUsageStatsBackendExtraFilter as isQuotaUsageStatsBackendExtraFilterImpl,
  isQuotaUsageStatsBackendFilter as isQuotaUsageStatsBackendFilterImpl,
  isQuotaUsageStatsDays as isQuotaUsageStatsDaysImpl,
  normalizeQuotaUsageQueryScope as normalizeQuotaUsageQueryScopeImpl,
  QUOTA_CONTRACT,
  QUOTA_USAGE_CLAUDE_TOOL_NAME,
  QUOTA_USAGE_GET_STATS_COMMAND,
  QUOTA_USAGE_MCP_TOOL_NAME,
  QUOTA_USAGE_QUERY_SCOPES,
  QUOTA_USAGE_STATS_BACKEND_EXTRA_FILTERS,
  QUOTA_USAGE_STATS_BACKEND_FILTER_LABELS,
  QUOTA_USAGE_STATS_BACKEND_FILTERS,
  QUOTA_USAGE_STATS_DAYS,
  QUOTA_USAGE_TOOL_NAME,
  QUOTA_USAGE_TOOL_NAMES,
  QUERY_QUOTA_USAGE_INPUT_SCHEMA,
  type QuotaUsageQueryScope as ContractQuotaUsageQueryScope,
  type QuotaUsageStatsBackendFilter as ContractQuotaUsageStatsBackendFilter,
  type QuotaUsageStatsDays as ContractQuotaUsageStatsDays,
  type QuotaUsageToolName as ContractQuotaUsageToolName,
} from "./quotaContract.mjs";

export {
  DEFAULT_QUOTA_USAGE_QUERY_SCOPE,
  DEFAULT_QUOTA_USAGE_STATS_DAYS,
  QUOTA_CONTRACT,
  QUOTA_USAGE_CLAUDE_TOOL_NAME,
  QUOTA_USAGE_GET_STATS_COMMAND,
  QUOTA_USAGE_MCP_TOOL_NAME,
  QUOTA_USAGE_QUERY_SCOPES,
  QUOTA_USAGE_STATS_BACKEND_EXTRA_FILTERS,
  QUOTA_USAGE_STATS_BACKEND_FILTER_LABELS,
  QUOTA_USAGE_STATS_BACKEND_FILTERS,
  QUOTA_USAGE_STATS_DAYS,
  QUOTA_USAGE_TOOL_NAME,
  QUOTA_USAGE_TOOL_NAMES,
  QUERY_QUOTA_USAGE_INPUT_SCHEMA,
};

export type QuotaUsageStatsDays = ContractQuotaUsageStatsDays;
export type QuotaUsageStatsBackendExtraFilter =
  (typeof QUOTA_USAGE_STATS_BACKEND_EXTRA_FILTERS)[number];
export type QuotaUsageStatsBackendFilter = ContractQuotaUsageStatsBackendFilter;
export type QuotaUsageToolName = ContractQuotaUsageToolName;

export const isQuotaUsageStatsDays = isQuotaUsageStatsDaysImpl as (
  value: unknown,
) => value is QuotaUsageStatsDays;
export const isQuotaUsageStatsBackendExtraFilter =
  isQuotaUsageStatsBackendExtraFilterImpl as (
    value: unknown,
  ) => value is QuotaUsageStatsBackendExtraFilter;
export const isQuotaUsageStatsBackendFilter =
  isQuotaUsageStatsBackendFilterImpl as (
    value: unknown,
  ) => value is QuotaUsageStatsBackendFilter;

export function quotaUsageStatsBackendFilterLabel(
  backend: QuotaUsageStatsBackendFilter,
): string {
  return isQuotaUsageStatsBackendExtraFilter(backend)
    ? QUOTA_USAGE_STATS_BACKEND_FILTER_LABELS[backend]
    : chatBackendLabel(backend);
}

export function quotaUsageStatsDaysLabel(days: QuotaUsageStatsDays): string {
  return `${days} 天`;
}

export interface QuotaUsageStatsInput {
  days?: QuotaUsageStatsDays;
  backend?: QuotaUsageStatsBackendFilter;
}

export type QuotaUsageQueryScope = ContractQuotaUsageQueryScope;

export const isQuotaUsageQueryScope = isQuotaUsageQueryScopeImpl as (
  value: unknown,
) => value is QuotaUsageQueryScope;
export const normalizeQuotaUsageQueryScope = normalizeQuotaUsageQueryScopeImpl as (
  value: unknown,
) => QuotaUsageQueryScope;
export const isLiliaQuotaTool = isLiliaQuotaToolImpl as (
  toolName: unknown,
) => toolName is QuotaUsageToolName;

export interface QuotaUsageQueryInput extends QuotaUsageStatsInput {
  scope?: QuotaUsageQueryScope;
}

export interface QuotaUsageTokenTotals {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  totalTokens: number;
}

export interface QuotaUsageCostCoverage {
  knownCostUsd: number | null;
  costRecordCount: number;
  totalRecordCount: number;
}

export interface QuotaUsageDailyBucket extends QuotaUsageTokenTotals {
  dayStart: number;
  knownCostUsd: number | null;
  costRecordCount: number;
  recordCount: number;
}

export interface QuotaUsageBackendSummary extends QuotaUsageTokenTotals {
  backend: ChatBackendKind;
  knownCostUsd: number | null;
  costRecordCount: number;
  recordCount: number;
}

export interface QuotaUsageRecentRecord extends QuotaUsageTokenTotals {
  eventId: string;
  taskId: string;
  turnId: string | null;
  backend: ChatBackendKind;
  sessionId: string | null;
  knownCostUsd: number | null;
  createdAt: number;
}

export interface QuotaUsageProjectSummary extends QuotaUsageTokenTotals {
  projectId: string | null;
  projectName: string;
  projectCwd: string | null;
  knownCostUsd: number | null;
  costRecordCount: number;
  recordCount: number;
}

export interface QuotaUsageConversationSummary extends QuotaUsageTokenTotals {
  taskId: string;
  taskTitle: string;
  taskStatus: string;
  projectId: string | null;
  projectName: string | null;
  knownCostUsd: number | null;
  costRecordCount: number;
  recordCount: number;
}

export interface QuotaUsageToolSummary {
  key: string;
  label: string;
  kind: string;
  subkind: string | null;
  toolName: string | null;
  callCount: number;
  sharePercent: number;
}

export interface QuotaUsageStats {
  days: QuotaUsageStatsDays;
  backend: QuotaUsageStatsBackendFilter;
  rangeStart: number;
  rangeEnd: number;
  totals: QuotaUsageTokenTotals;
  cost: QuotaUsageCostCoverage;
  daily: QuotaUsageDailyBucket[];
  backends: QuotaUsageBackendSummary[];
  recent: QuotaUsageRecentRecord[];
  projects: QuotaUsageProjectSummary[];
  conversations: QuotaUsageConversationSummary[];
  tools: QuotaUsageToolSummary[];
}

export function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(100, value));
}

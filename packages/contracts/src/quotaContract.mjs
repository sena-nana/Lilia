import { CHAT_BACKENDS } from "./chatBackendsContract.mjs";
import quotaContract from "./quota-contract.json" with { type: "json" };

const manifest = deepFreeze(quotaContract);

export const QUOTA_CONTRACT = manifest;
export const QUOTA_USAGE_STATS_DAYS = manifest.usageStatsDays;
export const DEFAULT_QUOTA_USAGE_STATS_DAYS = manifest.defaultUsageStatsDays;
export const QUOTA_USAGE_STATS_BACKEND_EXTRA_FILTERS = manifest.usageStatsBackendFilters;
export const QUOTA_USAGE_STATS_BACKEND_FILTER_LABELS =
  manifest.usageStatsBackendFilterLabels;
export const QUOTA_USAGE_STATS_BACKEND_FILTERS = [
  ...manifest.usageStatsBackendFilters,
  ...CHAT_BACKENDS,
];
export const QUOTA_USAGE_QUERY_SCOPES = manifest.usageQueryScopes;
export const DEFAULT_QUOTA_USAGE_QUERY_SCOPE = manifest.defaultUsageQueryScope;
export const QUOTA_USAGE_GET_STATS_COMMAND = manifest.commands.getStats;
export const QUOTA_USAGE_TOOL_NAMES = manifest.quotaUsageToolNames;
export const QUOTA_USAGE_TOOL_NAME = QUOTA_USAGE_TOOL_NAMES[0];
export const QUOTA_USAGE_CLAUDE_TOOL_NAME = QUOTA_USAGE_TOOL_NAMES[1];
export const QUOTA_USAGE_MCP_TOOL_NAME = QUOTA_USAGE_TOOL_NAMES[2];
export const QUERY_QUOTA_USAGE_INPUT_SCHEMA = manifest.queryQuotaUsageInputSchema;

const daysSet = new Set(QUOTA_USAGE_STATS_DAYS);
const backendFilterSet = new Set(QUOTA_USAGE_STATS_BACKEND_FILTERS);
const scopeSet = new Set(QUOTA_USAGE_QUERY_SCOPES);
const quotaUsageToolNameSet = new Set(QUOTA_USAGE_TOOL_NAMES);

export function isQuotaUsageStatsDays(value) {
  return typeof value === "number" && daysSet.has(value);
}

export function normalizeQuotaUsageStatsDays(value) {
  return isQuotaUsageStatsDays(value)
    ? value
    : DEFAULT_QUOTA_USAGE_STATS_DAYS;
}

export function isQuotaUsageStatsBackendExtraFilter(value) {
  return typeof value === "string" && QUOTA_USAGE_STATS_BACKEND_EXTRA_FILTERS.includes(value);
}

export function isQuotaUsageStatsBackendFilter(value) {
  return typeof value === "string" && backendFilterSet.has(value);
}

export function normalizeQuotaUsageStatsBackendFilter(value) {
  return isQuotaUsageStatsBackendFilter(value)
    ? value
    : QUOTA_USAGE_STATS_BACKEND_EXTRA_FILTERS[0];
}

export function isQuotaUsageQueryScope(value) {
  return typeof value === "string" && scopeSet.has(value);
}

export function normalizeQuotaUsageQueryScope(value) {
  return isQuotaUsageQueryScope(value)
    ? value
    : DEFAULT_QUOTA_USAGE_QUERY_SCOPE;
}

export function isLiliaQuotaTool(toolName) {
  return quotaUsageToolNameSet.has(String(toolName || ""));
}

function deepFreeze(value) {
  if (!value || typeof value !== "object") return value;
  for (const child of Object.values(value)) {
    deepFreeze(child);
  }
  return Object.freeze(value);
}

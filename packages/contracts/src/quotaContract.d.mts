export type QuotaUsageStatsDays = 7 | 30;
export type QuotaUsageStatsBackendFilter = "all" | "native-agentkit";
export type QuotaUsageQueryScope = "summary" | "projects" | "conversations" | "tools" | "all";
export type QuotaUsageToolName =
  | "QueryQuotaUsage"
  | "query_quota_usage"
  | "mcp__lilia__query_quota_usage";

export const QUOTA_CONTRACT: Record<string, unknown>;
export const QUOTA_USAGE_STATS_DAYS: readonly QuotaUsageStatsDays[];
export const DEFAULT_QUOTA_USAGE_STATS_DAYS: QuotaUsageStatsDays;
export const QUOTA_USAGE_STATS_BACKEND_EXTRA_FILTERS: readonly ["all"];
export const QUOTA_USAGE_STATS_BACKEND_FILTER_LABELS: Readonly<Record<"all", string>>;
export const QUOTA_USAGE_STATS_BACKEND_FILTERS: readonly QuotaUsageStatsBackendFilter[];
export const QUOTA_USAGE_QUERY_SCOPES: readonly QuotaUsageQueryScope[];
export const DEFAULT_QUOTA_USAGE_QUERY_SCOPE: QuotaUsageQueryScope;
export const QUOTA_USAGE_GET_STATS_COMMAND: "quota_usage_get_stats";
export const QUOTA_USAGE_TOOL_NAMES: readonly QuotaUsageToolName[];
export const QUOTA_USAGE_TOOL_NAME: "QueryQuotaUsage";
export const QUOTA_USAGE_CLAUDE_TOOL_NAME: "query_quota_usage";
export const QUOTA_USAGE_MCP_TOOL_NAME: "mcp__lilia__query_quota_usage";
export const QUERY_QUOTA_USAGE_INPUT_SCHEMA: Record<string, unknown>;

export function isQuotaUsageStatsDays(value: unknown): value is QuotaUsageStatsDays;
export function normalizeQuotaUsageStatsDays(value: unknown): QuotaUsageStatsDays;
export function isQuotaUsageStatsBackendExtraFilter(value: unknown): value is "all";
export function isQuotaUsageStatsBackendFilter(
  value: unknown,
): value is QuotaUsageStatsBackendFilter;
export function normalizeQuotaUsageStatsBackendFilter(value: unknown): QuotaUsageStatsBackendFilter;
export function isQuotaUsageQueryScope(value: unknown): value is QuotaUsageQueryScope;
export function normalizeQuotaUsageQueryScope(value: unknown): QuotaUsageQueryScope;
export function isLiliaQuotaTool(toolName: unknown): toolName is QuotaUsageToolName;

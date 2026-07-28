import { z } from "zod/v4";
import {
  DEFAULT_QUOTA_USAGE_QUERY_SCOPE,
  DEFAULT_QUOTA_USAGE_STATS_DAYS,
  QUERY_QUOTA_USAGE_INPUT_SCHEMA,
  QUOTA_USAGE_QUERY_SCOPES,
  QUOTA_USAGE_TOOL_NAME,
  QUOTA_USAGE_STATS_BACKEND_FILTERS,
  QUOTA_USAGE_STATS_DAYS,
  QUOTA_USAGE_TOOL_NAMES as LILIA_QUOTA_TOOL_NAMES,
  isLiliaQuotaTool,
  normalizeQuotaUsageQueryScope,
  normalizeQuotaUsageStatsBackendFilter,
  normalizeQuotaUsageStatsDays,
} from "@lilia/contracts/quotaContract.mjs";
import { isRecord, stringOrNull } from "./utils.mjs";

function literalUnion(values) {
  return z.union(values.map((value) => z.literal(value)));
}

export { LILIA_QUOTA_TOOL_NAMES, isLiliaQuotaTool };

export const queryQuotaUsageInputSchema = {
  days: literalUnion(QUOTA_USAGE_STATS_DAYS).optional().default(DEFAULT_QUOTA_USAGE_STATS_DAYS),
  backend: z.enum(QUOTA_USAGE_STATS_BACKEND_FILTERS).optional().default(
    QUOTA_USAGE_STATS_BACKEND_FILTERS[0],
  ),
  scope: z.enum(QUOTA_USAGE_QUERY_SCOPES).optional().default(DEFAULT_QUOTA_USAGE_QUERY_SCOPE),
};

export const queryQuotaUsageJsonSchema = QUERY_QUOTA_USAGE_INPUT_SCHEMA;

export const codexQueryQuotaUsageDynamicTool = {
  name: QUOTA_USAGE_TOOL_NAME,
  description: "Query Lilia quota usage summaries through the Lilia internal quota plugin.",
  inputSchema: queryQuotaUsageJsonSchema,
};

function normalizeQuotaUsageInput(input) {
  const row = isRecord(input) ? input : {};
  return {
    days: normalizeQuotaUsageStatsDays(row.days),
    backend: normalizeQuotaUsageStatsBackendFilter(stringOrNull(row.backend)),
    scope: normalizeQuotaUsageQueryScope(stringOrNull(row.scope)),
  };
}

export function createQuotaUsageHandler(requestQuotaUsage) {
  return async function handleQueryQuotaUsage(input) {
    if (typeof requestQuotaUsage !== "function") {
      return {
        ok: false,
        error: "Lilia internal quota plugin is not available.",
      };
    }
    const payload = normalizeQuotaUsageInput(input);
    const response = await requestQuotaUsage(payload);
    if (response?.ok !== true) {
      return {
        ok: false,
        error: response?.error || "Lilia internal quota plugin failed.",
      };
    }
    return response.result ?? {
      ok: false,
      error: "Lilia internal quota plugin returned no result.",
    };
  };
}

import { createCodexAppServer, initializeCodexAppServer } from "./appServer.mjs";

const FIVE_HOUR_MINUTES = 300;
const WEEKLY_MINUTES = 10080;
const FIVE_HOUR_TOLERANCE = 90;
const WEEKLY_TOLERANCE = 1440;
const RATE_LIMIT_READ_RETRIES = 2;
const RATE_LIMIT_READ_RETRY_DELAY_MS = 500;
const RESET_CREDIT_CONSUME_OUTCOMES = new Set([
  "reset",
  "alreadyRedeemed",
  "nothingToReset",
  "noCredit",
]);

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function stringOrNull(value) {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return null;
}

function numberOrNull(value) {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function normalizeCredits(value) {
  if (!isRecord(value)) return null;
  return {
    hasCredits: Boolean(value.hasCredits),
    unlimited: Boolean(value.unlimited),
    balance: stringOrNull(value.balance),
  };
}

function normalizeResetCredits(value) {
  if (!isRecord(value)) return null;
  const availableCount = numberOrNull(value.availableCount);
  if (availableCount === null) return null;
  return { availableCount: Math.max(0, Math.trunc(availableCount)) };
}

function normalizeUsageSummary(value) {
  const input = isRecord(value) ? value : {};
  return {
    lifetimeTokens: numberOrNull(input.lifetimeTokens),
    peakDailyTokens: numberOrNull(input.peakDailyTokens),
    longestRunningTurnSec: numberOrNull(input.longestRunningTurnSec),
    currentStreakDays: numberOrNull(input.currentStreakDays),
    longestStreakDays: numberOrNull(input.longestStreakDays),
  };
}

function normalizeUsageDailyBucket(value) {
  if (!isRecord(value)) return null;
  const startDate = stringOrNull(value.startDate);
  const tokens = numberOrNull(value.tokens);
  if (!startDate || tokens === null) return null;
  return { startDate, tokens };
}

function normalizeAccountUsage(value) {
  if (!isRecord(value)) return null;
  const dailyUsageBuckets = Array.isArray(value.dailyUsageBuckets)
    ? value.dailyUsageBuckets.map(normalizeUsageDailyBucket).filter(Boolean)
    : null;
  return {
    summary: normalizeUsageSummary(value.summary),
    dailyUsageBuckets,
  };
}

function normalizeWindow(value) {
  if (!isRecord(value)) return null;
  const usedPercent = numberOrNull(value.usedPercent);
  if (usedPercent === null) return null;
  return {
    usedPercent,
    windowDurationMins: numberOrNull(value.windowDurationMins),
    resetsAt: numberOrNull(value.resetsAt),
  };
}

function durationNear(window, target, tolerance) {
  return typeof window?.windowDurationMins === "number" &&
    Math.abs(window.windowDurationMins - target) <= tolerance;
}

function normalizeLimitSnapshot(value) {
  if (!isRecord(value)) return null;
  const primary = normalizeWindow(value.primary);
  const secondary = normalizeWindow(value.secondary);
  const windows = [primary, secondary].filter(Boolean);
  const fiveHour = windows.find((window) =>
    durationNear(window, FIVE_HOUR_MINUTES, FIVE_HOUR_TOLERANCE)
  ) || primary;
  const weekly = windows.find((window) =>
    window !== fiveHour && durationNear(window, WEEKLY_MINUTES, WEEKLY_TOLERANCE)
  ) || (secondary !== fiveHour ? secondary : null);
  return {
    available: Boolean(fiveHour || weekly),
    connectionMode: "codex-account",
    limitId: stringOrNull(value.limitId),
    limitName: stringOrNull(value.limitName),
    planType: stringOrNull(value.planType),
    rateLimitReachedType: stringOrNull(value.rateLimitReachedType),
    fiveHour,
    weekly,
    credits: normalizeCredits(value.credits),
  };
}

function readLimitSnapshot(result, limitId) {
  if (!isRecord(result)) return null;
  const byLimitId = isRecord(result.rateLimitsByLimitId) ? result.rateLimitsByLimitId : null;
  if (byLimitId && isRecord(byLimitId[limitId])) return byLimitId[limitId];
  if (limitId === "codex" && isRecord(result.rateLimits)) return result.rateLimits;
  return null;
}

function isSparkLimit(key, value) {
  if (!isRecord(value)) return false;
  const text = [
    key,
    stringOrNull(value.limitId),
    stringOrNull(value.limitName),
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  return text.includes("spark");
}

function readSparkLimitSnapshot(result) {
  if (!isRecord(result)) return null;
  const byLimitId = isRecord(result.rateLimitsByLimitId) ? result.rateLimitsByLimitId : null;
  if (!byLimitId) return null;
  if (isRecord(byLimitId.spark)) return byLimitId.spark;
  return Object.entries(byLimitId)
    .find(([key, value]) => isSparkLimit(key, value))?.[1] ?? null;
}

function normalizeRateLimitSnapshot(result) {
  const codex = normalizeLimitSnapshot(readLimitSnapshot(result, "codex"));
  const spark = normalizeLimitSnapshot(readSparkLimitSnapshot(result));
  if (!codex && !spark) return null;
  return {
    available: Boolean(codex?.available || spark?.available),
    connectionMode: "codex-account",
    limitId: codex?.limitId ?? null,
    limitName: codex?.limitName ?? null,
    planType: codex?.planType ?? spark?.planType ?? null,
    rateLimitReachedType: codex?.rateLimitReachedType ?? spark?.rateLimitReachedType ?? null,
    fiveHour: codex?.fiveHour ?? null,
    weekly: codex?.weekly ?? null,
    sparkFiveHour: spark?.fiveHour ?? null,
    sparkWeekly: spark?.weekly ?? null,
    credits: codex?.credits ?? null,
    sparkCredits: spark?.credits ?? null,
    rateLimitResetCredits: normalizeResetCredits(result.rateLimitResetCredits),
    accountUsage: null,
    usageError: null,
    fetchedAt: Date.now(),
    error: null,
  };
}

function sleep(ms) {
  return ms > 0 ? new Promise((resolve) => setTimeout(resolve, ms)) : Promise.resolve();
}

async function requestRateLimits(server, { retries = RATE_LIMIT_READ_RETRIES, retryDelayMs = RATE_LIMIT_READ_RETRY_DELAY_MS } = {}) {
  let lastError = null;
  for (let attempt = 0; attempt <= retries; attempt += 1) {
    try {
      return await server.request("account/rateLimits/read");
    } catch (err) {
      lastError = err;
      if (attempt >= retries) break;
      await sleep(retryDelayMs);
    }
  }
  throw lastError;
}

function unavailableQuotaStatus(error) {
  return {
    available: false,
    connectionMode: "codex-account",
    limitId: null,
    limitName: null,
    planType: null,
    rateLimitReachedType: null,
    fiveHour: null,
    weekly: null,
    sparkFiveHour: null,
    sparkWeekly: null,
    credits: null,
    sparkCredits: null,
    rateLimitResetCredits: null,
    accountUsage: null,
    usageError: null,
    fetchedAt: Date.now(),
    error,
  };
}

async function readQuotaStatusFromServer(server, { retries, retryDelayMs } = {}) {
  const result = await requestRateLimits(server, { retries, retryDelayMs });
  const snapshot = normalizeRateLimitSnapshot(result);
  if (!snapshot) {
    return unavailableQuotaStatus("Codex 官方额度接口未返回可识别的额度数据。");
  }
  try {
    const usage = normalizeAccountUsage(await server.request("account/usage/read"));
    if (usage) {
      snapshot.accountUsage = usage;
      return snapshot;
    }
    snapshot.usageError = "Codex 官方用量接口未返回可识别的用量数据。";
    return snapshot;
  } catch (err) {
    snapshot.usageError = err?.message || String(err);
    return snapshot;
  }
}

export async function readCodexAccountQuotaStatus(
  {
    createServer = createCodexAppServer,
    retries = RATE_LIMIT_READ_RETRIES,
    retryDelayMs = RATE_LIMIT_READ_RETRY_DELAY_MS,
  } = {},
) {
  const server = createServer();
  try {
    await initializeCodexAppServer(server);
    return await readQuotaStatusFromServer(server, { retries, retryDelayMs });
  } finally {
    server.close();
  }
}

export async function consumeCodexRateLimitResetCredit(
  idempotencyKey,
  {
    createServer = createCodexAppServer,
    retries = RATE_LIMIT_READ_RETRIES,
    retryDelayMs = RATE_LIMIT_READ_RETRY_DELAY_MS,
  } = {},
) {
  const key = stringOrNull(idempotencyKey)?.trim();
  if (!key) throw new Error("idempotencyKey is required");
  const server = createServer();
  try {
    await initializeCodexAppServer(server);
    const result = await server.request("account/rateLimitResetCredit/consume", {
      idempotencyKey: key,
    });
    const outcome = stringOrNull(result?.outcome);
    if (!RESET_CREDIT_CONSUME_OUTCOMES.has(outcome)) {
      throw new Error("Codex 官方额度重置接口返回了未知结果。");
    }
    const status = await readQuotaStatusFromServer(server, { retries, retryDelayMs });
    return { outcome, status };
  } finally {
    server.close();
  }
}

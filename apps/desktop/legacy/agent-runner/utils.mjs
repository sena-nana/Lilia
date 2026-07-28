import {
  TIMELINE_DISPLAY_INLINE_TEXT_LIMIT,
  TIMELINE_DISPLAY_ONE_LINE_TEXT_LIMIT,
  TIMELINE_PAYLOAD_MAX_DEPTH,
  TIMELINE_PAYLOAD_RESERVED_KEYS,
  normalizeTimelineStatus as normalizeContractTimelineStatus,
} from "@lilia/contracts/timelineContract.mjs";

export const TIMELINE_RESERVED_KEYS = new Set(TIMELINE_PAYLOAD_RESERVED_KEYS);

export function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export function stringOrNull(value) {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return null;
}

export function shortText(value, max = TIMELINE_DISPLAY_INLINE_TEXT_LIMIT) {
  const text = stringOrNull(value);
  if (!text) return null;
  return text.length > max ? `${text.slice(0, max)}...` : text;
}

export function oneLineSummary(value, max = TIMELINE_DISPLAY_ONE_LINE_TEXT_LIMIT) {
  const text = stringOrNull(value);
  if (!text) return "";
  return shortText(text.replace(/\s+/g, " ").trim(), max) || "";
}

export function fullTextOrNull(value) {
  return typeof value === "string" && value.length > 0 ? value : null;
}

export function normalizeTimelineStatus(status) {
  return normalizeContractTimelineStatus(status);
}

export function toJsonSafe(value, seen = new WeakSet()) {
  if (
    value === undefined ||
    typeof value === "function" ||
    typeof value === "symbol"
  ) {
    return undefined;
  }
  if (typeof value === "bigint") return value.toString();
  if (value === null || typeof value !== "object") return value;
  if (value instanceof Error) {
    return {
      name: value.name,
      message: value.message,
      stack: value.stack,
    };
  }
  if (seen.has(value)) return "[Circular]";

  seen.add(value);
  if (Array.isArray(value)) {
    const safeArray = value
      .map((item) => toJsonSafe(item, seen))
      .filter((item) => item !== undefined);
    seen.delete(value);
    return safeArray;
  }

  const safeObject = {};
  for (const [key, item] of Object.entries(value)) {
    const safeItem = toJsonSafe(item, seen);
    if (safeItem !== undefined) safeObject[key] = safeItem;
  }
  seen.delete(value);
  return safeObject;
}

export function sanitizeTimelinePayload(value, seen = new WeakSet(), depth = 0) {
  if (
    value === undefined ||
    typeof value === "function" ||
    typeof value === "symbol"
  ) {
    return undefined;
  }
  if (typeof value === "bigint") return value.toString();
  if (value === null || typeof value !== "object") return value;
  if (value instanceof Error) {
    return {
      name: value.name,
      message: value.message,
      stack: value.stack,
    };
  }
  if (depth > TIMELINE_PAYLOAD_MAX_DEPTH) return "[Truncated]";
  if (seen.has(value)) return "[Circular]";

  seen.add(value);
  if (Array.isArray(value)) {
    const safeArray = value
      .map((item) => sanitizeTimelinePayload(item, seen, depth + 1))
      .filter((item) => item !== undefined);
    seen.delete(value);
    return safeArray;
  }

  const safeObject = {};
  for (const [key, item] of Object.entries(value)) {
    if (TIMELINE_RESERVED_KEYS.has(key)) continue;
    const safeItem = sanitizeTimelinePayload(item, seen, depth + 1);
    if (safeItem !== undefined) safeObject[key] = safeItem;
  }
  seen.delete(value);
  return safeObject;
}

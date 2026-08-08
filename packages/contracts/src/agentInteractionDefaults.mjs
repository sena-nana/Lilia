import agentInteractionDefaults from "./agent-interaction-defaults.json" with { type: "json" };
import {
  DEFAULT_PERMISSION_MODE,
  PERMISSION_MODE_DISPLAY_ORDER,
  PERMISSION_MODES,
  isRuntimePermissionMode,
  normalizeRuntimePermissionMode,
} from "./permissionModes.mjs";

export const AUTO_TURN_DECISION_PERMISSION_OPTIONS = Object.freeze([
  { key: "allowModelTier", label: "模型层级", ariaLabel: "辅助模型操作模型层级" },
  { key: "allowReasoningEffort", label: "思考强度", ariaLabel: "辅助模型操作思考强度" },
  { key: "allowPlanMode", label: "计划模式", ariaLabel: "辅助模型操作计划模式" },
  { key: "allowGoalMode", label: "Goal 模式", ariaLabel: "辅助模型操作 Goal 模式" },
  { key: "allowSessionFork", label: "会话分叉", ariaLabel: "辅助模型操作会话分叉" },
]);

export const AUTO_TURN_DECISION_PERMISSION_KEYS = Object.freeze(
  AUTO_TURN_DECISION_PERMISSION_OPTIONS.map((option) => option.key),
);
export const LOCKED_PERMISSION_MODES = Object.freeze(["ask", "readonly"]);
const LOCKED_PERMISSION_MODE_SET = new Set(LOCKED_PERMISSION_MODES);
export const DEFAULT_PERMISSION_MODE_AVAILABILITY = Object.freeze(
  Object.fromEntries(PERMISSION_MODES.map((mode) => [mode, true])),
);

const MAIN_AGENT_PROMPT_MODES = Object.freeze(["conservative", "aggressive", "custom"]);
const MAIN_AGENT_PROMPT_MODE_SET = new Set(MAIN_AGENT_PROMPT_MODES);

const defaults = readAgentInteractionDefaultsManifest(agentInteractionDefaults);

export const DEFAULT_AGENT_SUBAGENT_MODE_SETTINGS = freezeSubagentModeSettings(
  defaults.subagentMode,
);
export const DEFAULT_AUTO_TURN_DECISION_SETTINGS = Object.freeze({
  ...defaults.autoTurnDecision,
});
export const DEFAULT_AGENT_INTERACTION_SETTINGS = freezeAgentInteractionSettings(defaults);

export function normalizeMainAgentPromptMode(value, fallback = "conservative") {
  return typeof value === "string" && MAIN_AGENT_PROMPT_MODE_SET.has(value)
    ? value
    : fallback;
}

export function normalizeMainAgentCustomPrompt(value, fallback = "") {
  return typeof value === "string" ? value.trim() : fallback;
}

export function normalizeAgentSubagentModeSettings(
  input,
  base = DEFAULT_AGENT_SUBAGENT_MODE_SETTINGS,
) {
  return {
    enabled: typeof input?.enabled === "boolean" ? input.enabled : base.enabled,
    forwardSubagentText: typeof input?.forwardSubagentText === "boolean"
      ? input.forwardSubagentText
      : base.forwardSubagentText,
    agentProgressSummaries: typeof input?.agentProgressSummaries === "boolean"
      ? input.agentProgressSummaries
      : base.agentProgressSummaries,
  };
}

export function normalizeAutoTurnDecisionSettings(
  input,
  base = DEFAULT_AUTO_TURN_DECISION_SETTINGS,
) {
  const normalized = {
    enabled: typeof input?.enabled === "boolean" ? input.enabled : base.enabled,
  };
  for (const key of AUTO_TURN_DECISION_PERMISSION_KEYS) {
    normalized[key] = typeof input?.[key] === "boolean" ? input[key] : base[key];
  }
  return normalized;
}

export function normalizePermissionModeAvailability(
  input,
  base = DEFAULT_PERMISSION_MODE_AVAILABILITY,
) {
  const normalized = {};
  for (const mode of PERMISSION_MODES) {
    normalized[mode] = LOCKED_PERMISSION_MODE_SET.has(mode)
      ? true
      : typeof input?.[mode] === "boolean"
        ? input[mode]
        : base[mode] !== false;
  }
  return normalized;
}

export function enabledPermissionModes(availability) {
  const normalized = normalizePermissionModeAvailability(availability);
  return PERMISSION_MODE_DISPLAY_ORDER.filter((mode) => normalized[mode]);
}

export function normalizeAvailablePermissionMode(
  value,
  availability,
  fallback = DEFAULT_PERMISSION_MODE,
) {
  const normalizedAvailability = normalizePermissionModeAvailability(availability);
  const normalized = normalizeRuntimePermissionMode(value, fallback);
  if (normalizedAvailability[normalized]) return normalized;
  const normalizedFallback = normalizeRuntimePermissionMode(fallback, DEFAULT_PERMISSION_MODE);
  if (normalizedAvailability[normalizedFallback]) return normalizedFallback;
  return DEFAULT_PERMISSION_MODE;
}

export function normalizeAgentInteractionSettings(
  input,
  base = DEFAULT_AGENT_INTERACTION_SETTINGS,
) {
  const permissionModeAvailability = normalizePermissionModeAvailability(
    input?.permissionModeAvailability,
    base.permissionModeAvailability,
  );
  return {
    nonInterruptMode: typeof input?.nonInterruptMode === "boolean"
      ? input.nonInterruptMode
      : base.nonInterruptMode,
    debug: typeof input?.debug === "boolean" ? input.debug : base.debug,
    permissionMode: normalizeAvailablePermissionMode(
      input?.permissionMode,
      permissionModeAvailability,
      base.permissionMode,
    ),
    permissionModeAvailability,
    mainAgentPromptMode: normalizeMainAgentPromptMode(
      input?.mainAgentPromptMode,
      base.mainAgentPromptMode,
    ),
    mainAgentCustomPrompt: normalizeMainAgentCustomPrompt(
      input?.mainAgentCustomPrompt,
      base.mainAgentCustomPrompt,
    ),
    subagentMode: normalizeAgentSubagentModeSettings(input?.subagentMode, base.subagentMode),
    autoTurnDecision: normalizeAutoTurnDecisionSettings(
      input?.autoTurnDecision,
      base.autoTurnDecision,
    ),
  };
}

function readAgentInteractionDefaultsManifest(value) {
  const row = requireManifestRecord(value, "agent-interaction-defaults.json");
  if (!isRuntimePermissionMode(row.permissionMode)) {
    throw new Error("agent-interaction-defaults.json.permissionMode must be a permission mode");
  }
  const permissionModeAvailability = requireManifestRecord(
    row.permissionModeAvailability,
    "agent-interaction-defaults.json.permissionModeAvailability",
  );
  const subagentMode = requireManifestRecord(
    row.subagentMode,
    "agent-interaction-defaults.json.subagentMode",
  );
  const autoTurnDecision = requireManifestRecord(
    row.autoTurnDecision,
    "agent-interaction-defaults.json.autoTurnDecision",
  );
  return {
    nonInterruptMode: booleanManifestField(
      row,
      "nonInterruptMode",
      "agent-interaction-defaults.json",
    ),
    debug: booleanManifestField(row, "debug", "agent-interaction-defaults.json"),
    permissionMode: row.permissionMode,
    permissionModeAvailability: readPermissionModeAvailabilityDefaults(
      permissionModeAvailability,
    ),
    mainAgentPromptMode: normalizeMainAgentPromptMode(row.mainAgentPromptMode),
    mainAgentCustomPrompt: normalizeMainAgentCustomPrompt(row.mainAgentCustomPrompt),
    subagentMode: readAgentSubagentModeDefaults(subagentMode),
    autoTurnDecision: readAutoTurnDecisionDefaults(autoTurnDecision),
  };
}

function readPermissionModeAvailabilityDefaults(row) {
  const defaults = {};
  for (const mode of PERMISSION_MODES) {
    defaults[mode] = booleanManifestField(
      row,
      mode,
      "agent-interaction-defaults.json.permissionModeAvailability",
    );
  }
  return normalizePermissionModeAvailability(defaults);
}

function readAgentSubagentModeDefaults(row) {
  return {
    enabled: booleanManifestField(
      row,
      "enabled",
      "agent-interaction-defaults.json.subagentMode",
    ),
    forwardSubagentText: booleanManifestField(
      row,
      "forwardSubagentText",
      "agent-interaction-defaults.json.subagentMode",
    ),
    agentProgressSummaries: booleanManifestField(
      row,
      "agentProgressSummaries",
      "agent-interaction-defaults.json.subagentMode",
    ),
  };
}

function readAutoTurnDecisionDefaults(row) {
  const path = "agent-interaction-defaults.json.autoTurnDecision";
  const defaults = {
    enabled: booleanManifestField(row, "enabled", path),
  };
  for (const key of AUTO_TURN_DECISION_PERMISSION_KEYS) {
    defaults[key] = booleanManifestField(row, key, path);
  }
  return defaults;
}

function requireManifestRecord(value, path) {
  const row = recordValue(value);
  if (!row) throw new Error(`${path} must be an object`);
  return row;
}

function booleanManifestField(row, key, path) {
  const value = row[key];
  if (typeof value !== "boolean") throw new Error(`${path}.${key} must be a boolean`);
  return value;
}

function freezeSubagentModeSettings(value) {
  return Object.freeze({
    enabled: value.enabled,
    forwardSubagentText: value.forwardSubagentText,
    agentProgressSummaries: value.agentProgressSummaries,
  });
}

function freezeAgentInteractionSettings(value) {
  return Object.freeze({
    nonInterruptMode: value.nonInterruptMode,
    debug: value.debug,
    permissionMode: value.permissionMode,
    permissionModeAvailability: Object.freeze({ ...value.permissionModeAvailability }),
    mainAgentPromptMode: value.mainAgentPromptMode,
    mainAgentCustomPrompt: value.mainAgentCustomPrompt,
    subagentMode: freezeSubagentModeSettings(value.subagentMode),
    autoTurnDecision: Object.freeze({ ...value.autoTurnDecision }),
  });
}

function recordValue(value) {
  return value && typeof value === "object" && !Array.isArray(value) ? value : null;
}

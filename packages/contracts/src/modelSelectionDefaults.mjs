import modelSelectionDefaults from "./model-selection-defaults.json" with { type: "json" };
import {
  CHAT_BACKENDS,
  DEFAULT_CHAT_BACKEND,
  MODEL_OPTIONS_BY_BACKEND,
  REASONING_EFFORTS,
} from "./chatBackendsContract.mjs";

export const MODEL_SELECTION_TIERS = Object.freeze(["light", "normal", "deep"]);
export const MODEL_SELECTION_CONTEXT_SCALES = Object.freeze(["medium", "large"]);

const manifest = readModelSelectionDefaultsManifest(modelSelectionDefaults);

export const AUTO_MODEL_BY_BACKEND_AND_TIER = manifest.autoModels;
export const AUTO_REASONING_EFFORT_BY_TIER = manifest.autoReasoningEfforts;
export const AUTO_WORKFLOW_TYPES_BY_TIER = manifest.autoTierRules.workflowTiers;
export const AUTO_RUNTIME_COMMAND_TYPES_BY_TIER =
  manifest.autoTierRules.runtimeCommandTiers;
export const AUTO_RUNTIME_COMMAND_SIGNAL_LABELS =
  manifest.autoTierRules.runtimeCommandSignals;
export const AUTO_CONTEXT_THRESHOLDS = manifest.autoTierRules.contextThresholds;

export function autoModelForBackendTier(backend, tier) {
  const row =
    AUTO_MODEL_BY_BACKEND_AND_TIER[backend] ??
    AUTO_MODEL_BY_BACKEND_AND_TIER[DEFAULT_CHAT_BACKEND] ??
    AUTO_MODEL_BY_BACKEND_AND_TIER[CHAT_BACKENDS[0]];
  return row?.[tier] ?? null;
}

export function autoReasoningEffortForTier(tier) {
  return AUTO_REASONING_EFFORT_BY_TIER[tier];
}

export function autoTierForWorkflowType(value) {
  return autoTierForValue(AUTO_WORKFLOW_TYPES_BY_TIER, value);
}

export function autoTierForRuntimeCommandType(value) {
  return autoTierForValue(AUTO_RUNTIME_COMMAND_TYPES_BY_TIER, value);
}

export function autoRuntimeCommandSignalLabel(value) {
  const trimmed = value?.trim();
  return trimmed ? AUTO_RUNTIME_COMMAND_SIGNAL_LABELS[trimmed] ?? null : null;
}

export function autoContextThresholdsForScale(scale) {
  return AUTO_CONTEXT_THRESHOLDS[scale];
}

function autoTierForValue(tiers, value) {
  const trimmed = value?.trim();
  if (!trimmed) return null;
  for (const tier of MODEL_SELECTION_TIERS) {
    if (tiers[tier].includes(trimmed)) return tier;
  }
  return null;
}

function readModelSelectionDefaultsManifest(value) {
  const manifest = recordValue(value);
  const autoModelsRow = recordValue(manifest?.autoModels);
  const effortsRow = recordValue(manifest?.autoReasoningEfforts);
  if (!autoModelsRow || !effortsRow) {
    throw new Error("model-selection-defaults.json must define autoModels and autoReasoningEfforts");
  }

  const autoModels = Object.freeze(Object.fromEntries(
    CHAT_BACKENDS.map((backend) => {
      const tierRow = recordValue(autoModelsRow[backend]);
      if (!tierRow) {
        throw new Error(`model-selection-defaults.json missing autoModels.${backend}`);
      }
      const tiers = Object.freeze(Object.fromEntries(
        MODEL_SELECTION_TIERS.map((tier) => {
          const model = tierRow[tier];
          if (typeof model !== "string" || !model.trim()) {
            throw new Error(`model-selection-defaults.json missing autoModels.${backend}.${tier}`);
          }
          if (!MODEL_OPTIONS_BY_BACKEND[backend].some((option) => option.id === model)) {
            throw new Error(`model-selection-defaults.json has unknown autoModels.${backend}.${tier}`);
          }
          return [tier, model];
        }),
      ));
      return [backend, tiers];
    }),
  ));

  const autoReasoningEfforts = Object.freeze(Object.fromEntries(
    MODEL_SELECTION_TIERS.map((tier) => {
      const effort = effortsRow[tier];
      if (!isReasoningEffort(effort)) {
        throw new Error(`model-selection-defaults.json has invalid autoReasoningEfforts.${tier}`);
      }
      return [tier, effort];
    }),
  ));

  const autoTierRules = recordValue(manifest?.autoTierRules);
  if (!autoTierRules) {
    throw new Error("model-selection-defaults.json must define autoTierRules");
  }

  return Object.freeze({
    autoModels,
    autoReasoningEfforts,
    autoTierRules: Object.freeze({
      workflowTiers: readTierStringLists(
        autoTierRules.workflowTiers,
        "autoTierRules.workflowTiers",
      ),
      runtimeCommandTiers: readTierStringLists(
        autoTierRules.runtimeCommandTiers,
        "autoTierRules.runtimeCommandTiers",
      ),
      runtimeCommandSignals: readStringRecord(
        autoTierRules.runtimeCommandSignals,
        "autoTierRules.runtimeCommandSignals",
      ),
      contextThresholds: readContextThresholds(autoTierRules.contextThresholds),
    }),
  });
}

function readStringListManifestField(row, field) {
  const value = row?.[field];
  if (
    !Array.isArray(value) ||
    !value.every((item) => typeof item === "string" && item.trim())
  ) {
    throw new Error(`model-selection-defaults.json must define ${field} as a string array`);
  }
  return Object.freeze(value.map((item) => item.trim()));
}

function readTierStringLists(value, field) {
  const row = recordValue(value);
  if (!row) {
    throw new Error(`model-selection-defaults.json must define ${field}`);
  }
  return Object.freeze(Object.fromEntries(
    MODEL_SELECTION_TIERS.map((tier) => [tier, readStringListManifestField(row, tier)]),
  ));
}

function readStringRecord(value, field) {
  const row = recordValue(value);
  if (!row) {
    throw new Error(`model-selection-defaults.json must define ${field}`);
  }
  return Object.freeze(Object.fromEntries(
    Object.entries(row).map(([key, raw]) => {
      if (typeof raw !== "string" || !raw.trim()) {
        throw new Error(`model-selection-defaults.json has invalid ${field}.${key}`);
      }
      return [key, raw.trim()];
    }),
  ));
}

function readContextThresholds(value) {
  const row = recordValue(value);
  if (!row) {
    throw new Error("model-selection-defaults.json must define autoTierRules.contextThresholds");
  }
  return Object.freeze(Object.fromEntries(
    MODEL_SELECTION_CONTEXT_SCALES.map((scale) => {
      const scaleRow = recordValue(row[scale]);
      if (!scaleRow) {
        throw new Error(
          `model-selection-defaults.json missing autoTierRules.contextThresholds.${scale}`,
        );
      }
      return [scale, Object.freeze({
        contextUsagePercent: readPositiveNumberField(
          scaleRow,
          "contextUsagePercent",
          `autoTierRules.contextThresholds.${scale}`,
        ),
        promptLength: readPositiveNumberField(
          scaleRow,
          "promptLength",
          `autoTierRules.contextThresholds.${scale}`,
        ),
        attachmentCount: readPositiveNumberField(
          scaleRow,
          "attachmentCount",
          `autoTierRules.contextThresholds.${scale}`,
        ),
        conversationReferenceCount: readPositiveNumberField(
          scaleRow,
          "conversationReferenceCount",
          `autoTierRules.contextThresholds.${scale}`,
        ),
        directoryFileCount: readOptionalPositiveNumberField(
          scaleRow,
          "directoryFileCount",
          `autoTierRules.contextThresholds.${scale}`,
        ),
        directoryTotalSize: readOptionalPositiveNumberField(
          scaleRow,
          "directoryTotalSize",
          `autoTierRules.contextThresholds.${scale}`,
        ),
      })];
    }),
  ));
}

function readPositiveNumberField(row, field, label) {
  const value = row[field];
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
    throw new Error(`model-selection-defaults.json has invalid ${label}.${field}`);
  }
  return value;
}

function readOptionalPositiveNumberField(row, field, label) {
  const value = row[field];
  if (value === undefined) return undefined;
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
    throw new Error(`model-selection-defaults.json has invalid ${label}.${field}`);
  }
  return value;
}

function isReasoningEffort(value) {
  return typeof value === "string" && REASONING_EFFORTS.includes(value);
}

function recordValue(value) {
  return value && typeof value === "object" && !Array.isArray(value) ? value : null;
}

import modelSelectionDefaults from "./model-selection-defaults.json" with { type: "json" };
import {
  CHAT_BACKENDS,
  DEFAULT_CHAT_BACKEND,
  MODEL_OPTIONS_BY_BACKEND,
  REASONING_EFFORTS,
} from "./chatBackendsContract.mjs";

export const MODEL_SELECTION_TIERS = Object.freeze(["light", "normal", "deep"]);
export const MODEL_PROVIDER_FAMILIES = Object.freeze(["openai", "anthropic"]);
export const MODEL_SELECTION_CONTEXT_SCALES = Object.freeze(["medium", "large"]);
export const MODEL_SELECTION_CONTEXT_SCALE_ALL = Object.freeze(["small", "medium", "large"]);
export const BUILTIN_MODEL_PRESET_IDS = Object.freeze(["fast", "default", "plan", "review"]);

const manifest = readModelSelectionDefaultsManifest(modelSelectionDefaults);

export const AUTO_MODEL_BY_BACKEND_AND_TIER = manifest.autoModels;
export const AUTO_MODEL_BY_FAMILY_AND_TIER = manifest.autoModelFamilies;
export const AUTO_REASONING_EFFORT_BY_TIER = manifest.autoReasoningEfforts;
export const AUTO_PRESET_REASONING_EFFORT = manifest.autoPresetReasoningEfforts;
export const PRESET_TIER_MAP = manifest.presetTierMap;
export const BUILTIN_PRESET_LABELS = manifest.builtinPresetLabels;
export const AUTO_WORKFLOW_TYPES_BY_TIER = manifest.autoTierRules.workflowTiers;
export const AUTO_RUNTIME_COMMAND_TYPES_BY_TIER =
  manifest.autoTierRules.runtimeCommandTiers;
export const AUTO_RUNTIME_COMMAND_SIGNAL_LABELS =
  manifest.autoPresetRules.runtimeCommandSignals;
export const AUTO_CONTEXT_THRESHOLDS = manifest.autoPresetRules.contextThresholds;
export const AUTO_WORKFLOW_TYPES_BY_PRESET = manifest.autoPresetRules.workflowPresets;
export const AUTO_RUNTIME_COMMAND_TYPES_BY_PRESET =
  manifest.autoPresetRules.runtimeCommandPresets;
export const AUTO_CONTEXT_SCALE_PRESETS = manifest.autoPresetRules.contextScalePresets;
export const PLAN_MODE_PRESET = manifest.autoPresetRules.planModePreset;

export function autoModelForBackendTier(backend, tier) {
  const row =
    AUTO_MODEL_BY_BACKEND_AND_TIER[backend] ??
    AUTO_MODEL_BY_BACKEND_AND_TIER[DEFAULT_CHAT_BACKEND] ??
    AUTO_MODEL_BY_BACKEND_AND_TIER[CHAT_BACKENDS[0]];
  return row?.[tier] ?? null;
}

export function autoModelForProviderFamilyTier(family, tier) {
  return AUTO_MODEL_BY_FAMILY_AND_TIER[family]?.[tier] ?? null;
}

export function autoReasoningEffortForTier(tier) {
  return AUTO_REASONING_EFFORT_BY_TIER[tier];
}

export function autoReasoningEffortForPreset(presetId) {
  if (isBuiltinModelPresetId(presetId)) {
    return AUTO_PRESET_REASONING_EFFORT[presetId];
  }
  return AUTO_PRESET_REASONING_EFFORT.default;
}

export function tierForPreset(presetId) {
  if (isBuiltinModelPresetId(presetId)) {
    return PRESET_TIER_MAP[presetId];
  }
  return PRESET_TIER_MAP.default;
}

export function autoModelForBackendPreset(backend, presetId) {
  return autoModelForBackendTier(backend, tierForPreset(presetId));
}

export function autoTierForWorkflowType(value) {
  return autoTierForValue(AUTO_WORKFLOW_TYPES_BY_TIER, value);
}

export function autoTierForRuntimeCommandType(value) {
  return autoTierForValue(AUTO_RUNTIME_COMMAND_TYPES_BY_TIER, value);
}

export function autoPresetForWorkflowType(value) {
  return autoPresetForValue(AUTO_WORKFLOW_TYPES_BY_PRESET, value);
}

export function autoPresetForRuntimeCommandType(value) {
  return autoPresetForValue(AUTO_RUNTIME_COMMAND_TYPES_BY_PRESET, value);
}

export function autoPresetForContextScale(scale) {
  if (scale === "small" || scale === "medium" || scale === "large") {
    return AUTO_CONTEXT_SCALE_PRESETS[scale] ?? "default";
  }
  return "default";
}

export function autoRuntimeCommandSignalLabel(value) {
  const trimmed = value?.trim();
  return trimmed ? AUTO_RUNTIME_COMMAND_SIGNAL_LABELS[trimmed] ?? null : null;
}

export function autoContextThresholdsForScale(scale) {
  return AUTO_CONTEXT_THRESHOLDS[scale];
}

export function isBuiltinModelPresetId(value) {
  return typeof value === "string" && BUILTIN_MODEL_PRESET_IDS.includes(value);
}

export function builtinPresetLabel(presetId) {
  if (!isBuiltinModelPresetId(presetId)) return presetId;
  return BUILTIN_PRESET_LABELS[presetId] ?? presetId;
}

export function createDefaultBuiltinPresets(backend = DEFAULT_CHAT_BACKEND) {
  return BUILTIN_MODEL_PRESET_IDS.map((id) => ({
    id,
    label: builtinPresetLabel(id),
    kind: "builtin",
    model: null,
    reasoningEffort: null,
    enabled: true,
  }));
}

/**
 * Normalize model feature settings: ensure builtin presets exist, migrate chat tiers,
 * keep custom presets, and mirror builtin models back into chat tiers for legacy readers.
 */
export function normalizeModelFeatureSettings(input, backend = DEFAULT_CHAT_BACKEND) {
  const source = input && typeof input === "object" ? input : {};
  const chatSource = source.chat && typeof source.chat === "object" ? source.chat : {};
  const chat = {
    light: normalizeOptionalString(chatSource.light),
    normal: normalizeOptionalString(chatSource.normal),
    deep: normalizeOptionalString(chatSource.deep),
  };

  const rawPresets = Array.isArray(source.presets) ? source.presets : null;
  let presets;
  if (!rawPresets || rawPresets.length === 0) {
    presets = migrateChatTiersToPresets(chat, backend);
  } else {
    presets = mergePresetsWithBuiltins(rawPresets, chat, backend);
  }

  // Mirror builtin preset models into chat tiers for legacy consumers.
  const byId = Object.fromEntries(presets.map((p) => [p.id, p]));
  const nextChat = {
    light: byId.fast?.model ?? chat.light,
    normal: byId.default?.model ?? chat.normal,
    deep: byId.plan?.model ?? byId.review?.model ?? chat.deep,
  };

  return {
    chat: nextChat,
    presets,
    title: normalizeOptionalString(source.title),
    suggestion: normalizeOptionalString(source.suggestion),
    promptRouter: normalizeOptionalString(source.promptRouter),
    promptOptimize: normalizeOptionalString(source.promptOptimize),
    autoTurnDecision: normalizeOptionalString(source.autoTurnDecision),
  };
}

export function resolvePresetModel(backend, preset, modelOptions) {
  const presetId = preset?.id ?? "default";
  const desired =
    (typeof preset?.model === "string" && preset.model.trim()) ||
    autoModelForBackendPreset(backend, isBuiltinModelPresetId(presetId) ? presetId : "default");
  if (!Array.isArray(modelOptions) || modelOptions.length === 0) {
    return { model: desired, usedFallback: false };
  }
  if (modelOptions.some((option) => option.id === desired)) {
    return { model: desired, usedFallback: false };
  }
  const fallback =
    modelOptions.find((option) => option.backend === backend)?.id ??
    modelOptions[0]?.id ??
    desired;
  return { model: fallback, usedFallback: fallback !== desired, desired };
}

export function resolvePresetEffort(backend, preset, normalizeEffortFn) {
  const presetId = preset?.id ?? "default";
  const configured =
    typeof preset?.reasoningEffort === "string" ? preset.reasoningEffort : null;
  const raw =
    configured ||
    autoReasoningEffortForPreset(
      isBuiltinModelPresetId(presetId) ? presetId : "default",
    );
  if (typeof normalizeEffortFn === "function") {
    return normalizeEffortFn(backend, raw);
  }
  return raw;
}

function migrateChatTiersToPresets(chat, backend) {
  return BUILTIN_MODEL_PRESET_IDS.map((id) => {
    const tier = PRESET_TIER_MAP[id];
    const fromChat = chat[tier] ?? null;
    return {
      id,
      label: builtinPresetLabel(id),
      kind: "builtin",
      model: fromChat,
      reasoningEffort: null,
      enabled: true,
    };
  });
}

function mergePresetsWithBuiltins(rawPresets, chat, backend) {
  const normalizedCustom = [];
  const seenCustomIds = new Set();
  const builtinOverrides = new Map();

  for (const raw of rawPresets) {
    if (!raw || typeof raw !== "object") continue;
    const id = typeof raw.id === "string" ? raw.id.trim() : "";
    if (!id) continue;
    if (isBuiltinModelPresetId(id)) {
      builtinOverrides.set(id, {
        model: normalizeOptionalString(raw.model),
        reasoningEffort: normalizeOptionalEffort(raw.reasoningEffort),
        enabled: raw.enabled !== false,
        label: builtinPresetLabel(id),
      });
      continue;
    }
    if (seenCustomIds.has(id)) continue;
    seenCustomIds.add(id);
    const label =
      (typeof raw.label === "string" && raw.label.trim()) || id;
    normalizedCustom.push({
      id,
      label,
      kind: "custom",
      model: normalizeOptionalString(raw.model),
      reasoningEffort: normalizeOptionalEffort(raw.reasoningEffort),
      enabled: raw.enabled !== false,
    });
  }

  const builtins = BUILTIN_MODEL_PRESET_IDS.map((id) => {
    const override = builtinOverrides.get(id);
    const tier = PRESET_TIER_MAP[id];
    return {
      id,
      label: builtinPresetLabel(id),
      kind: "builtin",
      model: override?.model ?? chat[tier] ?? null,
      reasoningEffort: override?.reasoningEffort ?? null,
      enabled: override ? override.enabled : true,
    };
  });

  return [...builtins, ...normalizedCustom];
}

function autoPresetForValue(presets, value) {
  const trimmed = value?.trim();
  if (!trimmed) return null;
  for (const presetId of BUILTIN_MODEL_PRESET_IDS) {
    const list = presets[presetId];
    if (Array.isArray(list) && list.includes(trimmed)) return presetId;
  }
  return null;
}

function autoTierForValue(tiers, value) {
  const trimmed = value?.trim();
  if (!trimmed) return null;
  for (const tier of MODEL_SELECTION_TIERS) {
    if (tiers[tier].includes(trimmed)) return tier;
  }
  return null;
}

function normalizeOptionalString(value) {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

function normalizeOptionalEffort(value) {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return REASONING_EFFORTS.includes(trimmed) ? trimmed : null;
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
  const familyModelsRow = recordValue(manifest?.autoModelFamilies);
  if (!familyModelsRow) {
    throw new Error("model-selection-defaults.json must define autoModelFamilies");
  }
  const autoModelFamilies = Object.freeze(Object.fromEntries(
    MODEL_PROVIDER_FAMILIES.map((family) => {
      const tierRow = recordValue(familyModelsRow[family]);
      if (!tierRow) {
        throw new Error(`model-selection-defaults.json missing autoModelFamilies.${family}`);
      }
      return [family, Object.freeze(Object.fromEntries(
        MODEL_SELECTION_TIERS.map((tier) => {
          const model = tierRow[tier];
          if (typeof model !== "string" || !model.trim()) {
            throw new Error(
              `model-selection-defaults.json missing autoModelFamilies.${family}.${tier}`,
            );
          }
          return [tier, model];
        }),
      ))];
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

  const presetEffortRow = recordValue(manifest?.autoPresetReasoningEfforts);
  if (!presetEffortRow) {
    throw new Error("model-selection-defaults.json must define autoPresetReasoningEfforts");
  }
  const autoPresetReasoningEfforts = Object.freeze(Object.fromEntries(
    BUILTIN_MODEL_PRESET_IDS.map((id) => {
      const effort = presetEffortRow[id];
      if (!isReasoningEffort(effort)) {
        throw new Error(`model-selection-defaults.json has invalid autoPresetReasoningEfforts.${id}`);
      }
      return [id, effort];
    }),
  ));

  const presetTierRow = recordValue(manifest?.presetTierMap);
  if (!presetTierRow) {
    throw new Error("model-selection-defaults.json must define presetTierMap");
  }
  const presetTierMap = Object.freeze(Object.fromEntries(
    BUILTIN_MODEL_PRESET_IDS.map((id) => {
      const tier = presetTierRow[id];
      if (!MODEL_SELECTION_TIERS.includes(tier)) {
        throw new Error(`model-selection-defaults.json has invalid presetTierMap.${id}`);
      }
      return [id, tier];
    }),
  ));

  const labelsRow = recordValue(manifest?.builtinPresetLabels);
  if (!labelsRow) {
    throw new Error("model-selection-defaults.json must define builtinPresetLabels");
  }
  const builtinPresetLabels = Object.freeze(Object.fromEntries(
    BUILTIN_MODEL_PRESET_IDS.map((id) => {
      const label = labelsRow[id];
      if (typeof label !== "string" || !label.trim()) {
        throw new Error(`model-selection-defaults.json has invalid builtinPresetLabels.${id}`);
      }
      return [id, label.trim()];
    }),
  ));

  const autoTierRules = recordValue(manifest?.autoTierRules);
  if (!autoTierRules) {
    throw new Error("model-selection-defaults.json must define autoTierRules");
  }

  const autoPresetRules = recordValue(manifest?.autoPresetRules);
  if (!autoPresetRules) {
    throw new Error("model-selection-defaults.json must define autoPresetRules");
  }

  const planModePreset = autoPresetRules.planModePreset;
  if (!isBuiltinModelPresetId(planModePreset)) {
    throw new Error("model-selection-defaults.json has invalid autoPresetRules.planModePreset");
  }

  const contextScalePresetsRow = recordValue(autoPresetRules.contextScalePresets);
  if (!contextScalePresetsRow) {
    throw new Error("model-selection-defaults.json must define autoPresetRules.contextScalePresets");
  }
  const contextScalePresets = Object.freeze(Object.fromEntries(
    MODEL_SELECTION_CONTEXT_SCALE_ALL.map((scale) => {
      const presetId = contextScalePresetsRow[scale];
      if (!isBuiltinModelPresetId(presetId)) {
        throw new Error(
          `model-selection-defaults.json has invalid autoPresetRules.contextScalePresets.${scale}`,
        );
      }
      return [scale, presetId];
    }),
  ));

  return Object.freeze({
    autoModels,
    autoModelFamilies,
    autoReasoningEfforts,
    autoPresetReasoningEfforts,
    presetTierMap,
    builtinPresetLabels,
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
    autoPresetRules: Object.freeze({
      planModePreset,
      workflowPresets: readPresetStringLists(
        autoPresetRules.workflowPresets,
        "autoPresetRules.workflowPresets",
      ),
      runtimeCommandPresets: readPresetStringLists(
        autoPresetRules.runtimeCommandPresets,
        "autoPresetRules.runtimeCommandPresets",
      ),
      runtimeCommandSignals: readStringRecord(
        autoPresetRules.runtimeCommandSignals,
        "autoPresetRules.runtimeCommandSignals",
      ),
      contextScalePresets,
      contextThresholds: readContextThresholds(autoPresetRules.contextThresholds),
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

function readOptionalStringListManifestField(row, field) {
  const value = row?.[field];
  if (value === undefined) return Object.freeze([]);
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

function readPresetStringLists(value, field) {
  const row = recordValue(value);
  if (!row) {
    throw new Error(`model-selection-defaults.json must define ${field}`);
  }
  return Object.freeze(Object.fromEntries(
    BUILTIN_MODEL_PRESET_IDS.map((id) => [
      id,
      readOptionalStringListManifestField(row, id),
    ]),
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
    throw new Error("model-selection-defaults.json must define contextThresholds");
  }
  return Object.freeze(Object.fromEntries(
    MODEL_SELECTION_CONTEXT_SCALES.map((scale) => {
      const scaleRow = recordValue(row[scale]);
      if (!scaleRow) {
        throw new Error(
          `model-selection-defaults.json missing contextThresholds.${scale}`,
        );
      }
      return [scale, Object.freeze({
        contextUsagePercent: readPositiveNumberField(
          scaleRow,
          "contextUsagePercent",
          `contextThresholds.${scale}`,
        ),
        promptLength: readPositiveNumberField(
          scaleRow,
          "promptLength",
          `contextThresholds.${scale}`,
        ),
        attachmentCount: readPositiveNumberField(
          scaleRow,
          "attachmentCount",
          `contextThresholds.${scale}`,
        ),
        conversationReferenceCount: readPositiveNumberField(
          scaleRow,
          "conversationReferenceCount",
          `contextThresholds.${scale}`,
        ),
        directoryFileCount: readOptionalPositiveNumberField(
          scaleRow,
          "directoryFileCount",
          `contextThresholds.${scale}`,
        ),
        directoryTotalSize: readOptionalPositiveNumberField(
          scaleRow,
          "directoryTotalSize",
          `contextThresholds.${scale}`,
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

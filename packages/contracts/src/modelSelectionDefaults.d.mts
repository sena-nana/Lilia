import type {
  ChatBackendKind,
  ReasoningEffort,
} from "./chatBackendsContract.mjs";

export type ModelTier = "light" | "normal" | "deep";
export type ModelSelectionContextScale = "small" | "medium" | "large";
export type BuiltinModelPresetId = "fast" | "default" | "plan" | "review";
export type ModelPresetKind = "builtin" | "custom";

export interface ModelSelectionContextThresholds {
  contextUsagePercent: number;
  promptLength: number;
  attachmentCount: number;
  conversationReferenceCount: number;
  directoryFileCount?: number;
  directoryTotalSize?: number;
}

export interface ModelPresetGroup {
  id: string;
  label: string;
  kind: ModelPresetKind;
  model: string | null;
  reasoningEffort?: ReasoningEffort | null;
  enabled: boolean;
}

export interface ModelFeatureSettingsShape {
  chat: Record<ModelTier, string | null>;
  presets: ModelPresetGroup[];
  title: string | null;
  suggestion: string | null;
  promptRouter: string | null;
  promptOptimize: string | null;
  autoTurnDecision: string | null;
}

export const MODEL_SELECTION_TIERS: readonly ["light", "normal", "deep"];
export const MODEL_SELECTION_CONTEXT_SCALES: readonly ["medium", "large"];
export const MODEL_SELECTION_CONTEXT_SCALE_ALL: readonly ["small", "medium", "large"];
export const BUILTIN_MODEL_PRESET_IDS: readonly ["fast", "default", "plan", "review"];

export const AUTO_MODEL_BY_BACKEND_AND_TIER: Readonly<
  Record<ChatBackendKind, Readonly<Record<ModelTier, string>>>
>;
export const AUTO_REASONING_EFFORT_BY_TIER: Readonly<
  Record<ModelTier, ReasoningEffort>
>;
export const AUTO_PRESET_REASONING_EFFORT: Readonly<
  Record<BuiltinModelPresetId, ReasoningEffort>
>;
export const PRESET_TIER_MAP: Readonly<Record<BuiltinModelPresetId, ModelTier>>;
export const BUILTIN_PRESET_LABELS: Readonly<Record<BuiltinModelPresetId, string>>;
export const AUTO_WORKFLOW_TYPES_BY_TIER: Readonly<
  Record<ModelTier, readonly string[]>
>;
export const AUTO_RUNTIME_COMMAND_TYPES_BY_TIER: Readonly<
  Record<ModelTier, readonly string[]>
>;
export const AUTO_RUNTIME_COMMAND_SIGNAL_LABELS: Readonly<Record<string, string>>;
export const AUTO_CONTEXT_THRESHOLDS: Readonly<
  Record<Exclude<ModelSelectionContextScale, "small">, ModelSelectionContextThresholds>
>;
export const AUTO_WORKFLOW_TYPES_BY_PRESET: Readonly<
  Record<BuiltinModelPresetId, readonly string[]>
>;
export const AUTO_RUNTIME_COMMAND_TYPES_BY_PRESET: Readonly<
  Record<BuiltinModelPresetId, readonly string[]>
>;
export const AUTO_CONTEXT_SCALE_PRESETS: Readonly<
  Record<ModelSelectionContextScale, BuiltinModelPresetId>
>;
export const PLAN_MODE_PRESET: BuiltinModelPresetId;

export function autoModelForBackendTier(
  backend: ChatBackendKind,
  tier: ModelTier,
): string;

export function autoReasoningEffortForTier(tier: ModelTier): ReasoningEffort;

export function autoReasoningEffortForPreset(
  presetId: string,
): ReasoningEffort;

export function tierForPreset(presetId: string): ModelTier;

export function autoModelForBackendPreset(
  backend: ChatBackendKind,
  presetId: string,
): string;

export function autoTierForWorkflowType(
  value: string | null | undefined,
): ModelTier | null;

export function autoTierForRuntimeCommandType(
  value: string | null | undefined,
): ModelTier | null;

export function autoPresetForWorkflowType(
  value: string | null | undefined,
): BuiltinModelPresetId | null;

export function autoPresetForRuntimeCommandType(
  value: string | null | undefined,
): BuiltinModelPresetId | null;

export function autoPresetForContextScale(
  scale: ModelSelectionContextScale,
): BuiltinModelPresetId;

export function autoRuntimeCommandSignalLabel(
  value: string | null | undefined,
): string | null;

export function autoContextThresholdsForScale(
  scale: Exclude<ModelSelectionContextScale, "small">,
): ModelSelectionContextThresholds;

export function isBuiltinModelPresetId(
  value: unknown,
): value is BuiltinModelPresetId;

export function builtinPresetLabel(presetId: string): string;

export function createDefaultBuiltinPresets(
  backend?: ChatBackendKind,
): ModelPresetGroup[];

export function normalizeModelFeatureSettings(
  input: unknown,
  backend?: ChatBackendKind,
): ModelFeatureSettingsShape;

export function resolvePresetModel(
  backend: ChatBackendKind,
  preset: ModelPresetGroup | null | undefined,
  modelOptions: ReadonlyArray<{ id: string; backend?: string }>,
): { model: string; usedFallback: boolean; desired?: string };

export function resolvePresetEffort(
  backend: ChatBackendKind,
  preset: ModelPresetGroup | null | undefined,
  normalizeEffortFn?: (backend: ChatBackendKind, raw: string) => ReasoningEffort | null,
): ReasoningEffort | string | null;

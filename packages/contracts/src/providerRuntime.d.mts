export type CodexJsonObject = Record<string, unknown>;

/** @deprecated Official Codex product removed. */
export interface CodexProfileSettings {
  profile: string;
  model: string | null;
  reasoningEffort: string | null;
  runtimeWorkspaceRoots: string[];
  responsesApiClientMetadata: CodexJsonObject | null;
  additionalContext: string | null;
  persistExtendedHistory: boolean | null;
  initialTurnsPage: CodexJsonObject | null;
  excludeTurns: string[];
}

/** @deprecated Official Codex product removed; empty. */
export const CODEX_REASONING_EFFORTS: readonly string[];
export const REASONING_EFFORTS: readonly string[];
export const BACKEND_REASONING_EFFORTS: Record<string, readonly string[]>;
/** @deprecated Official Codex product removed; empty. */
export const CODEX_SETTINGS_PROFILES: readonly string[];
/** @deprecated Official Codex product removed. */
export const DEFAULT_CODEX_PROFILE_SETTINGS: CodexProfileSettings;

export function isReasoningEffort(value: unknown): boolean;
export function normalizeReasoningEffort(value: unknown): string | null;
export function reasoningEffortsForBackend(backend: string): readonly string[];
export function normalizeReasoningEffortForBackend(
  backend: string,
  value: unknown,
): string | null;
/** @deprecated Official Codex product removed. */
export function isCodexReasoningEffort(value: unknown): boolean;
/** @deprecated Official Codex product removed. */
export function normalizeCodexReasoningEffort(value: unknown): string | null;
/** @deprecated Official Codex product removed. */
export function isCodexSettingsProfile(value: unknown): boolean;
/** @deprecated Official Codex product removed. */
export function normalizeCodexSettingsProfile(value: unknown): string;
export function normalizeUniqueTrimmedStrings(value: unknown): string[];
export function normalizeCodexJsonObject(value: unknown): CodexJsonObject | null;
/** @deprecated Official Codex product removed. */
export function normalizeCodexProfileSettings(
  input: Partial<CodexProfileSettings> | null | undefined,
  base?: CodexProfileSettings,
): CodexProfileSettings;

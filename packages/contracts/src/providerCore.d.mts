export type CodexJsonObject = Record<string, unknown>;

/** @deprecated Official Codex product removed. */
export interface CodexProfileSettingsCore {
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

export interface ProviderHelpers {
  REASONING_EFFORTS: readonly string[];
  BACKEND_REASONING_EFFORTS: Record<string, readonly string[]>;
  /** @deprecated Official Codex product removed. */
  CODEX_REASONING_EFFORTS: readonly string[];
  /** @deprecated Official Codex product removed. */
  CODEX_SETTINGS_PROFILES: readonly string[];
  /** @deprecated Official Codex product removed. */
  DEFAULT_CODEX_PROFILE_SETTINGS: CodexProfileSettingsCore;
  isReasoningEffort(value: unknown): boolean;
  normalizeReasoningEffort(value: unknown): string | null;
  reasoningEffortsForBackend(backend: string): readonly string[];
  normalizeReasoningEffortForBackend(backend: string, value: unknown): string | null;
  /** @deprecated Official Codex product removed. */
  isCodexReasoningEffort(value: unknown): boolean;
  /** @deprecated Official Codex product removed. */
  normalizeCodexReasoningEffort(value: unknown): string | null;
  /** @deprecated Official Codex product removed. */
  isCodexSettingsProfile(value: unknown): boolean;
  /** @deprecated Official Codex product removed. */
  normalizeCodexSettingsProfile(value: unknown): string;
  normalizeUniqueTrimmedStrings(value: unknown): string[];
  normalizeCodexJsonObject(value: unknown): CodexJsonObject | null;
  /** @deprecated Official Codex product removed. */
  normalizeCodexProfileSettings(
    input: Partial<CodexProfileSettingsCore> | null | undefined,
    base?: CodexProfileSettingsCore,
  ): CodexProfileSettingsCore;
}

export function createProviderHelpers(
  chatBackendsJson: unknown,
): ProviderHelpers;

export function normalizeUniqueTrimmedStrings(value: unknown): string[];
export function normalizeCodexJsonObject(value: unknown): CodexJsonObject | null;

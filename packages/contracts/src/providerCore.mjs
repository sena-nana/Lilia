export function createProviderHelpers(chatBackendsJson) {
  const chatBackends = deepFreeze(chatBackendsJson);
  const reasoningEfforts = chatBackends.reasoningEfforts;
  const backendReasoningEfforts = chatBackends.backendReasoningEfforts;
  const reasoningEffortSet = new Set(reasoningEfforts);

  function isReasoningEffort(value) {
    return typeof value === "string" && reasoningEffortSet.has(value);
  }

  function normalizeReasoningEffort(value) {
    return isReasoningEffort(value) ? value : null;
  }

  function reasoningEffortsForBackend(backend) {
    return backendReasoningEfforts[backend] || [];
  }

  function normalizeReasoningEffortForBackend(backend, value) {
    const effort = normalizeReasoningEffort(value);
    if (!effort) return null;
    const supportedEfforts = reasoningEffortsForBackend(backend);
    if (supportedEfforts.includes(effort)) return effort;
    const effortIndex = reasoningEfforts.indexOf(effort);
    for (let index = effortIndex - 1; index >= 0; index -= 1) {
      const candidate = reasoningEfforts[index];
      if (supportedEfforts.includes(candidate)) return candidate;
    }
    return supportedEfforts[0] || null;
  }

  return {
    REASONING_EFFORTS: reasoningEfforts,
    BACKEND_REASONING_EFFORTS: backendReasoningEfforts,
    /** @deprecated Official Codex product removed; empty list. */
    CODEX_REASONING_EFFORTS: Object.freeze([]),
    /** @deprecated Official Codex product removed; empty list. */
    CODEX_SETTINGS_PROFILES: Object.freeze([]),
    /** @deprecated Official Codex product removed. */
    DEFAULT_CODEX_PROFILE_SETTINGS: Object.freeze({
      profile: "default",
      model: null,
      reasoningEffort: null,
      runtimeWorkspaceRoots: [],
      responsesApiClientMetadata: null,
      additionalContext: null,
      persistExtendedHistory: null,
      initialTurnsPage: null,
      excludeTurns: [],
    }),
    isReasoningEffort,
    normalizeReasoningEffort,
    reasoningEffortsForBackend,
    normalizeReasoningEffortForBackend,
    /** @deprecated Official Codex product removed. */
    isCodexReasoningEffort: () => false,
    /** @deprecated Official Codex product removed. */
    normalizeCodexReasoningEffort: () => null,
    /** @deprecated Official Codex product removed. */
    isCodexSettingsProfile: () => false,
    /** @deprecated Official Codex product removed. */
    normalizeCodexSettingsProfile: () => "default",
    normalizeUniqueTrimmedStrings,
    normalizeCodexJsonObject,
    /** @deprecated Official Codex product removed. */
    normalizeCodexProfileSettings: (input, base) => ({
      profile: "default",
      model: normalizeNullableText(input?.model ?? base?.model ?? null),
      reasoningEffort: null,
      runtimeWorkspaceRoots: normalizeUniqueTrimmedStrings(
        input?.runtimeWorkspaceRoots ?? base?.runtimeWorkspaceRoots ?? [],
      ),
      responsesApiClientMetadata: normalizeCodexJsonObject(
        input?.responsesApiClientMetadata ?? base?.responsesApiClientMetadata ?? null,
      ),
      additionalContext: normalizeNullableText(
        input?.additionalContext ?? base?.additionalContext ?? null,
      ),
      persistExtendedHistory: normalizeNullableBoolean(
        input?.persistExtendedHistory ?? base?.persistExtendedHistory ?? null,
      ),
      initialTurnsPage: normalizeCodexJsonObject(
        input?.initialTurnsPage ?? base?.initialTurnsPage ?? null,
      ),
      excludeTurns: normalizeUniqueTrimmedStrings(
        input?.excludeTurns ?? base?.excludeTurns ?? [],
      ),
    }),
  };
}

export function normalizeUniqueTrimmedStrings(value) {
  if (!Array.isArray(value)) return [];
  const items = value
    .filter((item) => typeof item === "string")
    .map((item) => item.trim())
    .filter(Boolean);
  return [...new Set(items)];
}

export function normalizeCodexJsonObject(value) {
  return value && typeof value === "object" && !Array.isArray(value)
    ? { ...value }
    : null;
}

function normalizeNullableText(value) {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function normalizeNullableBoolean(value) {
  return typeof value === "boolean" ? value : null;
}

function deepFreeze(value) {
  if (!value || typeof value !== "object") return value;
  for (const child of Object.values(value)) {
    deepFreeze(child);
  }
  return Object.freeze(value);
}

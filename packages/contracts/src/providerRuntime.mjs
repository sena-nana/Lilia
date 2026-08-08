import { CHAT_BACKENDS_CONTRACT } from "./chatBackendsContract.mjs";
import { createProviderHelpers } from "./providerCore.mjs";

const helpers = createProviderHelpers(CHAT_BACKENDS_CONTRACT);

export const CODEX_REASONING_EFFORTS = helpers.CODEX_REASONING_EFFORTS;
export const REASONING_EFFORTS = helpers.REASONING_EFFORTS;
export const BACKEND_REASONING_EFFORTS = helpers.BACKEND_REASONING_EFFORTS;
export const CODEX_SETTINGS_PROFILES = helpers.CODEX_SETTINGS_PROFILES;
export const DEFAULT_CODEX_PROFILE_SETTINGS = helpers.DEFAULT_CODEX_PROFILE_SETTINGS;
export const isReasoningEffort = helpers.isReasoningEffort;
export const normalizeReasoningEffort = helpers.normalizeReasoningEffort;
export const reasoningEffortsForBackend = helpers.reasoningEffortsForBackend;
export const normalizeReasoningEffortForBackend = helpers.normalizeReasoningEffortForBackend;
export const isCodexReasoningEffort = helpers.isCodexReasoningEffort;
export const normalizeCodexReasoningEffort = helpers.normalizeCodexReasoningEffort;
export const isCodexSettingsProfile = helpers.isCodexSettingsProfile;
export const normalizeCodexSettingsProfile = helpers.normalizeCodexSettingsProfile;
export const normalizeUniqueTrimmedStrings = helpers.normalizeUniqueTrimmedStrings;
export const normalizeCodexJsonObject = helpers.normalizeCodexJsonObject;
export const normalizeCodexProfileSettings = helpers.normalizeCodexProfileSettings;

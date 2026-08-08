import type { ChatBackendKind, ModelTier } from "./chat";
import {
  API_DESCRIPTION_BY_BACKEND,
  API_KEY_ENV_BY_BACKEND,
  CODEX_REASONING_EFFORTS,
  CONNECTION_MODES,
  CONNECTION_MODES_USING_API_KEY,
  CONNECTION_MODES_USING_CODEX_ACCOUNT,
  CONNECTION_MODES_USING_CUSTOM_URL,
  CONNECTION_MODES_USING_DEFAULT_API,
  DEFAULT_ROUTER_MODE_BY_BACKEND,
  DIRECT_DEFAULT_URLS,
  PROVIDER_STORE_KEY_BY_BACKEND,
  ROUTER_MODE_LABELS,
  ROUTER_MODES_BY_BACKEND,
  ROUTER_MODES_USING_API_CONFIG,
  ROUTER_MODES_USING_CODEX_ACCOUNT,
  ROUTER_STORE_KEY_BY_BACKEND,
  UNCONFIGURED_CONNECTION_MODES,
} from "./chatBackendsContract.mjs";
import {
  ASSISTANT_AI_FETCH_MODELS_COMMAND,
  ASSISTANT_AI_GET_CONFIG_COMMAND,
  ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND,
  ASSISTANT_AI_SET_CONFIG_COMMAND,
  ASSISTANT_AI_TEST_CONNECTION_COMMAND,
  CHAT_CHECK_ENV_COMMAND,
  MODEL_FEATURE_GET_SETTINGS_COMMAND,
  MODEL_FEATURE_LIST_MODEL_OPTIONS_COMMAND,
  MODEL_FEATURE_SET_SETTINGS_COMMAND,
  PROVIDER_GET_ACTIVE_BACKEND_COMMAND,
  PROVIDER_GET_CONFIG_COMMAND,
  PROVIDER_SET_ACTIVE_BACKEND_COMMAND,
  PROVIDER_SET_CONFIG_COMMAND,
  ROUTER_GET_MODE_COMMAND,
  ROUTER_SET_MODE_COMMAND,
} from "./providerCommandsContract.mjs";
import {
  CODEX_SETTINGS_PROFILES as CODEX_SETTINGS_PROFILES_IMPL,
  DEFAULT_CODEX_PROFILE_SETTINGS as DEFAULT_CODEX_PROFILE_SETTINGS_IMPL,
  isCodexReasoningEffort as isCodexReasoningEffortImpl,
  isCodexSettingsProfile as isCodexSettingsProfileImpl,
  normalizeCodexJsonObject as normalizeCodexJsonObjectImpl,
  normalizeCodexProfileSettings as normalizeCodexProfileSettingsImpl,
  normalizeCodexReasoningEffort as normalizeCodexReasoningEffortImpl,
  normalizeCodexSettingsProfile as normalizeCodexSettingsProfileImpl,
  normalizeUniqueTrimmedStrings as normalizeUniqueTrimmedStringsImpl,
} from "./providerRuntime.mjs";

type ProviderRouterModeTuple = readonly ["api"];
type ConnectionModeTuple = readonly ["api", "custom", "unconfigured"];

export type ProviderConnectionMode = ProviderRouterModeTuple[number];
export type RouterMode = ProviderConnectionMode;
export type ConnectionMode = ConnectionModeTuple[number];

/** @deprecated Official Codex product removed. */
export type CodexReasoningEffort = never;
/** @deprecated Official Codex product removed. */
export type CodexSettingsProfile = never;

/** @deprecated Official Codex product removed. */
export type CodexJsonObject = Record<string, unknown>;

export {
  ASSISTANT_AI_FETCH_MODELS_COMMAND,
  ASSISTANT_AI_GET_CONFIG_COMMAND,
  ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND,
  ASSISTANT_AI_SET_CONFIG_COMMAND,
  ASSISTANT_AI_TEST_CONNECTION_COMMAND,
  API_DESCRIPTION_BY_BACKEND,
  API_KEY_ENV_BY_BACKEND,
  CHAT_CHECK_ENV_COMMAND,
  CODEX_REASONING_EFFORTS,
  CONNECTION_MODES,
  CONNECTION_MODES_USING_API_KEY,
  CONNECTION_MODES_USING_CODEX_ACCOUNT,
  CONNECTION_MODES_USING_CUSTOM_URL,
  CONNECTION_MODES_USING_DEFAULT_API,
  DEFAULT_ROUTER_MODE_BY_BACKEND,
  DIRECT_DEFAULT_URLS,
  MODEL_FEATURE_GET_SETTINGS_COMMAND,
  MODEL_FEATURE_LIST_MODEL_OPTIONS_COMMAND,
  MODEL_FEATURE_SET_SETTINGS_COMMAND,
  PROVIDER_STORE_KEY_BY_BACKEND,
  PROVIDER_GET_ACTIVE_BACKEND_COMMAND,
  PROVIDER_GET_CONFIG_COMMAND,
  PROVIDER_SET_ACTIVE_BACKEND_COMMAND,
  PROVIDER_SET_CONFIG_COMMAND,
  ROUTER_GET_MODE_COMMAND,
  ROUTER_MODE_LABELS,
  ROUTER_MODES_BY_BACKEND,
  ROUTER_MODES_USING_API_CONFIG,
  ROUTER_MODES_USING_CODEX_ACCOUNT,
  ROUTER_STORE_KEY_BY_BACKEND,
  ROUTER_SET_MODE_COMMAND,
  UNCONFIGURED_CONNECTION_MODES,
};

/** @deprecated Official Codex product removed; empty. */
export const CODEX_SETTINGS_PROFILES =
  CODEX_SETTINGS_PROFILES_IMPL as readonly never[];

const ROUTER_MODES_USING_API_CONFIG_SET = new Set<string>(ROUTER_MODES_USING_API_CONFIG);
const ROUTER_MODES_USING_CODEX_ACCOUNT_SET = new Set<string>(ROUTER_MODES_USING_CODEX_ACCOUNT);
const CONNECTION_MODE_SET = new Set<string>(CONNECTION_MODES);
const CONNECTION_MODES_USING_API_KEY_SET = new Set<string>(CONNECTION_MODES_USING_API_KEY);
const CONNECTION_MODES_USING_DEFAULT_API_SET = new Set<string>(CONNECTION_MODES_USING_DEFAULT_API);
const CONNECTION_MODES_USING_CUSTOM_URL_SET = new Set<string>(CONNECTION_MODES_USING_CUSTOM_URL);
const CONNECTION_MODES_USING_CODEX_ACCOUNT_SET = new Set<string>(CONNECTION_MODES_USING_CODEX_ACCOUNT);
const UNCONFIGURED_CONNECTION_MODE_SET = new Set<string>(UNCONFIGURED_CONNECTION_MODES);

export function directDefaultUrlForBackend(backend: ChatBackendKind): string {
  return DIRECT_DEFAULT_URLS[backend];
}

export function apiKeyEnvForBackend(backend: ChatBackendKind): string {
  return API_KEY_ENV_BY_BACKEND[backend];
}

export function routerModesForBackend(backend: ChatBackendKind): readonly RouterMode[] {
  return ROUTER_MODES_BY_BACKEND[backend];
}

export function defaultRouterModeForBackend(backend: ChatBackendKind): RouterMode {
  return DEFAULT_ROUTER_MODE_BY_BACKEND[backend];
}

export function isRouterModeForBackend(
  backend: ChatBackendKind,
  mode: unknown,
): mode is RouterMode {
  return typeof mode === "string" && routerModesForBackend(backend).includes(mode as RouterMode);
}

export function normalizeRouterModeForBackend(
  backend: ChatBackendKind,
  mode: unknown,
): RouterMode {
  return isRouterModeForBackend(backend, mode)
    ? mode
    : defaultRouterModeForBackend(backend);
}

export function routerModeLabel(mode: RouterMode): string {
  return ROUTER_MODE_LABELS[mode];
}

export function routerModeUsesApiConfig(mode: RouterMode): boolean {
  return ROUTER_MODES_USING_API_CONFIG_SET.has(mode);
}

/** @deprecated Official Codex account modes removed; always false. */
export function routerModeUsesCodexAccount(mode: RouterMode): boolean {
  return ROUTER_MODES_USING_CODEX_ACCOUNT_SET.has(mode);
}

export function apiDescriptionForBackend(backend: ChatBackendKind): string {
  return API_DESCRIPTION_BY_BACKEND[backend];
}

export function isConnectionMode(value: unknown): value is ConnectionMode {
  return typeof value === "string" && CONNECTION_MODE_SET.has(value);
}

export function connectionModeUsesApiKey(mode: unknown): mode is "api" | "custom" {
  return typeof mode === "string" && CONNECTION_MODES_USING_API_KEY_SET.has(mode);
}

export function connectionModeUsesDefaultApi(mode: unknown): mode is "api" {
  return typeof mode === "string" && CONNECTION_MODES_USING_DEFAULT_API_SET.has(mode);
}

export function connectionModeUsesCustomUrl(mode: unknown): mode is "custom" {
  return typeof mode === "string" && CONNECTION_MODES_USING_CUSTOM_URL_SET.has(mode);
}

/** @deprecated Official Codex account modes removed; always false. */
export function connectionModeUsesCodexAccount(mode: unknown): boolean {
  return typeof mode === "string" && CONNECTION_MODES_USING_CODEX_ACCOUNT_SET.has(mode);
}

export function connectionModeIsUnconfigured(mode: unknown): mode is "unconfigured" {
  return typeof mode === "string" && UNCONFIGURED_CONNECTION_MODE_SET.has(mode);
}

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

/** @deprecated Official Codex product removed. */
export const DEFAULT_CODEX_PROFILE_SETTINGS =
  DEFAULT_CODEX_PROFILE_SETTINGS_IMPL as CodexProfileSettings;

/** @deprecated Official Codex product removed. */
export const isCodexReasoningEffort = isCodexReasoningEffortImpl as (
  value: unknown,
) => value is never;

/** @deprecated Official Codex product removed. */
export const normalizeCodexReasoningEffort = normalizeCodexReasoningEffortImpl as (
  value: unknown,
) => null;

/** @deprecated Official Codex product removed. */
export const isCodexSettingsProfile = isCodexSettingsProfileImpl as (
  value: unknown,
) => value is never;

/** @deprecated Official Codex product removed. */
export const normalizeCodexSettingsProfile = normalizeCodexSettingsProfileImpl as (
  value: unknown,
) => string;

export const normalizeUniqueTrimmedStrings = normalizeUniqueTrimmedStringsImpl as (
  value: unknown,
) => string[];

/** @deprecated Official Codex product removed. */
export const normalizeCodexJsonObject = normalizeCodexJsonObjectImpl as (
  value: unknown,
) => CodexJsonObject | null;

/** @deprecated Official Codex product removed. */
export const normalizeCodexProfileSettings = normalizeCodexProfileSettingsImpl as (
  input: Partial<CodexProfileSettings> | null | undefined,
  base?: CodexProfileSettings,
) => CodexProfileSettings;

/** Native AgentKit composer defaults for a project. */
export interface NativeComposerSettings {
  profile?: string | null;
  model?: string | null;
  reasoningEffort?: string | null;
  runtimeWorkspaceRoots?: string[] | null;
  responsesApiClientMetadata?: CodexJsonObject | null;
  additionalContext?: string | null;
  persistExtendedHistory?: boolean | null;
  initialTurnsPage?: CodexJsonObject | null;
  excludeTurns?: string[] | null;
}

/** @deprecated Use NativeComposerSettings. Official Codex product removed. */
export type CodexComposerSettings = NativeComposerSettings;

export interface ProviderConfig {
  backend: ChatBackendKind;
  baseUrl: string | null;
  apiKey: string | null;
  hasApiKey: boolean;
  clearApiKey?: boolean;
}

export interface AssistantAIConfig {
  baseUrl: string | null;
  apiKey: string | null;
  model: string | null;
  modelPool?: AssistantAIModelPoolItem[];
  hasApiKey: boolean;
  clearApiKey?: boolean;
}

export interface AssistantAIModelPoolItem {
  id: string;
  label: string;
  source: "remote" | "legacy";
  backend: ChatBackendKind;
}

export interface AssistantAITestResult {
  ok: boolean;
  error: string | null;
  models: string[] | null;
  modelMatched: boolean | null;
}

export interface AssistantAIModelsResult {
  ok: boolean;
  error: string | null;
  models: AssistantAIModelPoolItem[];
}

export interface ModelFeatureSettings {
  chat: Record<ModelTier, string | null>;
  title: string | null;
  suggestion: string | null;
  promptRouter: string | null;
  promptOptimize: string | null;
  autoTurnDecision: string | null;
}

export interface BackendEnvStatus {
  backend: ChatBackendKind;
  hasApiKey: boolean;
  connectionMode: ConnectionMode;
  effectiveUrl: string | null;
}

export interface EnvStatusReport {
  nodeAvailable: boolean;
  routerModes: Record<ChatBackendKind, RouterMode>;
  backends: Record<ChatBackendKind, BackendEnvStatus>;
}

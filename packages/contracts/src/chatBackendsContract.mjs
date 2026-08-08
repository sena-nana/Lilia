import chatBackendsJson from "./chat-backends.json" with { type: "json" };

export const CHAT_BACKENDS_CONTRACT = deepFreeze(chatBackendsJson);

export const CHAT_BACKENDS = CHAT_BACKENDS_CONTRACT.chatBackends;
export const DEFAULT_CHAT_BACKEND = CHAT_BACKENDS_CONTRACT.defaultBackend;
export const CHAT_BACKEND_LABELS = CHAT_BACKENDS_CONTRACT.backendLabels;
export const DEFAULT_MODEL_BY_BACKEND = CHAT_BACKENDS_CONTRACT.defaultModels;
export const MODEL_OPTIONS_BY_BACKEND = CHAT_BACKENDS_CONTRACT.backendModels;
export const ALLOWED_MODEL_PREFIXES_BY_BACKEND =
  CHAT_BACKENDS_CONTRACT.allowedModelPrefixes;
export const REASONING_EFFORTS = CHAT_BACKENDS_CONTRACT.reasoningEfforts;
export const BACKEND_REASONING_EFFORTS =
  CHAT_BACKENDS_CONTRACT.backendReasoningEfforts;
export const DIRECT_DEFAULT_URLS = CHAT_BACKENDS_CONTRACT.directUrls;
export const API_KEY_ENV_BY_BACKEND = CHAT_BACKENDS_CONTRACT.apiKeyEnv;
export const PROVIDER_STORE_KEY_BY_BACKEND =
  CHAT_BACKENDS_CONTRACT.providerStoreKeys;
export const ROUTER_STORE_KEY_BY_BACKEND =
  CHAT_BACKENDS_CONTRACT.routerStoreKeys;
export const ROUTER_MODES_BY_BACKEND =
  CHAT_BACKENDS_CONTRACT.backendRouterModes;
export const DEFAULT_ROUTER_MODE_BY_BACKEND =
  CHAT_BACKENDS_CONTRACT.defaultRouterModes;
export const ROUTER_MODE_LABELS = CHAT_BACKENDS_CONTRACT.routerModeLabels;
export const ROUTER_MODES_USING_API_CONFIG =
  CHAT_BACKENDS_CONTRACT.routerModesUsingApiConfig;
export const API_DESCRIPTION_BY_BACKEND =
  CHAT_BACKENDS_CONTRACT.apiDescriptions;
export const CONNECTION_MODES = CHAT_BACKENDS_CONTRACT.connectionModes;
export const CONNECTION_MODES_USING_API_KEY =
  CHAT_BACKENDS_CONTRACT.connectionModesUsingApiKey;
export const CONNECTION_MODES_USING_DEFAULT_API =
  CHAT_BACKENDS_CONTRACT.connectionModesUsingDefaultApi;
export const CONNECTION_MODES_USING_CUSTOM_URL =
  CHAT_BACKENDS_CONTRACT.connectionModesUsingCustomUrl;
export const UNCONFIGURED_CONNECTION_MODES =
  CHAT_BACKENDS_CONTRACT.unconfiguredConnectionModes;

/** Empty stubs kept for type/export stability until callers drop brand account modes. */
export const ROUTER_MODES_USING_CODEX_ACCOUNT = Object.freeze([]);
export const CONNECTION_MODES_USING_CODEX_ACCOUNT = Object.freeze([]);
export const CODEX_REASONING_EFFORTS = Object.freeze([]);

function deepFreeze(value) {
  if (!value || typeof value !== "object") return value;
  for (const child of Object.values(value)) {
    deepFreeze(child);
  }
  return Object.freeze(value);
}

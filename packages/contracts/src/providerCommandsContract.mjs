import providerCommandsContract from "./provider-commands-contract.json" with { type: "json" };

const manifest = Object.freeze(providerCommandsContract);

export const PROVIDER_COMMANDS_CONTRACT = manifest;
export const CHAT_CHECK_ENV_COMMAND = manifest.chatCheckEnvCommand;
export const PROVIDER_GET_CONFIG_COMMAND = manifest.providerGetConfigCommand;
export const PROVIDER_SET_CONFIG_COMMAND = manifest.providerSetConfigCommand;
export const PROVIDER_GET_ACTIVE_BACKEND_COMMAND =
  manifest.providerGetActiveBackendCommand;
export const PROVIDER_SET_ACTIVE_BACKEND_COMMAND =
  manifest.providerSetActiveBackendCommand;
export const ROUTER_GET_MODE_COMMAND = manifest.routerGetModeCommand;
export const ROUTER_SET_MODE_COMMAND = manifest.routerSetModeCommand;
export const ASSISTANT_AI_GET_CONFIG_COMMAND = manifest.assistantAiGetConfigCommand;
export const ASSISTANT_AI_SET_CONFIG_COMMAND = manifest.assistantAiSetConfigCommand;
export const ASSISTANT_AI_FETCH_MODELS_COMMAND =
  manifest.assistantAiFetchModelsCommand;
export const MODEL_FEATURE_LIST_MODEL_OPTIONS_COMMAND =
  manifest.modelFeatureListModelOptionsCommand;
export const MODEL_FEATURE_GET_SETTINGS_COMMAND =
  manifest.modelFeatureGetSettingsCommand;
export const MODEL_FEATURE_SET_SETTINGS_COMMAND =
  manifest.modelFeatureSetSettingsCommand;
export const ASSISTANT_AI_TEST_CONNECTION_COMMAND =
  manifest.assistantAiTestConnectionCommand;
export const ASSISTANT_AI_OPTIMIZE_PROMPT_COMMAND =
  manifest.assistantAiOptimizePromptCommand;

import pluginsContract from "./plugins-contract.json" with { type: "json" };

const manifest = Object.freeze(pluginsContract);

export const PLUGINS_CONTRACT = manifest;
export const PLUGIN_SCOPES = manifest.pluginScopes;
export const PLUGIN_BACKENDS = manifest.pluginBackends;
export const PLUGIN_MCP_TRANSPORTS = manifest.pluginMcpTransports;
export const PLUGIN_ENABLED_LABELS = manifest.pluginEnabledLabels;
export const PLUGIN_TOGGLE_ACTION_LABELS = manifest.pluginToggleActionLabels;
export const PLUGIN_BACKEND_LABELS = manifest.pluginBackendLabels;
export const PLUGIN_MCP_TRANSPORT_LABELS = manifest.pluginMcpTransportLabels;
export const PLUGINS_OVERVIEW_COMMAND = manifest.commands.overview;
export const PLUGINS_HOOKS_OVERVIEW_COMMAND = manifest.commands.hooksOverview;
export const PLUGINS_CREATE_SKILL_COMMAND = manifest.commands.createSkill;
export const PLUGINS_DELETE_SKILL_COMMAND = manifest.commands.deleteSkill;
export const PLUGINS_SET_SKILL_ENABLED_COMMAND = manifest.commands.setSkillEnabled;
export const PLUGINS_INSTALL_PACKAGE_COMMAND = manifest.commands.installPackage;
export const PLUGINS_DELETE_PACKAGE_COMMAND = manifest.commands.deletePackage;
export const PLUGINS_SET_PACKAGE_ENABLED_COMMAND =
  manifest.commands.setPackageEnabled;
export const PLUGINS_CREATE_MCP_SERVER_COMMAND = manifest.commands.createMcpServer;
export const PLUGINS_UPDATE_MCP_SERVER_COMMAND = manifest.commands.updateMcpServer;
export const PLUGINS_DELETE_MCP_SERVER_COMMAND = manifest.commands.deleteMcpServer;
export const PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND =
  manifest.commands.setMcpServerEnabled;
export const PLUGINS_OPEN_MCP_CONFIG_COMMAND = manifest.commands.openMcpConfig;
export const PLUGINS_READ_HOOK_SOURCE_COMMAND = manifest.commands.readHookSource;
export const PLUGINS_UPDATE_HOOK_SOURCE_COMMAND = manifest.commands.updateHookSource;
export const PLUGINS_CREATE_HOOK_SOURCE_COMMAND = manifest.commands.createHookSource;
export const PLUGINS_DELETE_HOOK_SOURCE_COMMAND = manifest.commands.deleteHookSource;
export const PLUGINS_SET_HOOK_SOURCE_ENABLED_COMMAND =
  manifest.commands.setHookSourceEnabled;
export const PLUGINS_OPEN_HOOK_CONFIG_COMMAND = manifest.commands.openHookConfig;

type PluginScope = "user" | "project";
type PluginBackendKind = "native-agentkit";
type PluginKnownMcpTransport = "stdio" | "http" | "oauth" | "unknown";

export const PLUGINS_CONTRACT: Record<string, unknown>;
export const PLUGIN_SCOPES: readonly PluginScope[];
export const PLUGIN_BACKENDS: readonly PluginBackendKind[];
export const PLUGIN_MCP_TRANSPORTS: readonly PluginKnownMcpTransport[];
export const PLUGIN_ENABLED_LABELS: Readonly<{
  enabled: string;
  disabled: string;
}>;
export const PLUGIN_TOGGLE_ACTION_LABELS: Readonly<{
  enable: string;
  disable: string;
}>;
export const PLUGIN_BACKEND_LABELS: Readonly<Record<PluginBackendKind, string>>;
export const PLUGIN_MCP_TRANSPORT_LABELS: Readonly<Record<PluginKnownMcpTransport, string>>;
export const PLUGINS_OVERVIEW_COMMAND: "plugins_overview";
export const PLUGINS_HOOKS_OVERVIEW_COMMAND: "plugins_hooks_overview";
export const PLUGINS_CREATE_SKILL_COMMAND: "plugins_create_skill";
export const PLUGINS_DELETE_SKILL_COMMAND: "plugins_delete_skill";
export const PLUGINS_SET_SKILL_ENABLED_COMMAND: "plugins_set_skill_enabled";
export const PLUGINS_SET_PACKAGE_ENABLED_COMMAND: "plugins_set_package_enabled";
export const PLUGINS_CREATE_MCP_SERVER_COMMAND: "plugins_create_mcp_server";
export const PLUGINS_UPDATE_MCP_SERVER_COMMAND: "plugins_update_mcp_server";
export const PLUGINS_DELETE_MCP_SERVER_COMMAND: "plugins_delete_mcp_server";
export const PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND: "plugins_set_mcp_server_enabled";
export const PLUGINS_OPEN_MCP_CONFIG_COMMAND: "plugins_open_mcp_config";
export const PLUGINS_READ_HOOK_SOURCE_COMMAND: "plugins_read_hook_source";
export const PLUGINS_UPDATE_HOOK_SOURCE_COMMAND: "plugins_update_hook_source";
export const PLUGINS_CREATE_HOOK_SOURCE_COMMAND: "plugins_create_hook_source";
export const PLUGINS_DELETE_HOOK_SOURCE_COMMAND: "plugins_delete_hook_source";
export const PLUGINS_SET_HOOK_SOURCE_ENABLED_COMMAND: "plugins_set_hook_source_enabled";
export const PLUGINS_OPEN_HOOK_CONFIG_COMMAND: "plugins_open_hook_config";

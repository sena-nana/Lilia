import {
  PLUGINS_CREATE_HOOK_SOURCE_COMMAND,
  PLUGINS_CREATE_MCP_SERVER_COMMAND,
  PLUGINS_CREATE_SKILL_COMMAND,
  PLUGINS_DELETE_HOOK_SOURCE_COMMAND,
  PLUGINS_DELETE_MCP_SERVER_COMMAND,
  PLUGINS_DELETE_PACKAGE_COMMAND,
  PLUGINS_DELETE_SKILL_COMMAND,
  PLUGINS_HOOKS_OVERVIEW_COMMAND,
  PLUGINS_INSTALL_PACKAGE_COMMAND,
  PLUGINS_OPEN_HOOK_CONFIG_COMMAND,
  PLUGINS_OPEN_MCP_CONFIG_COMMAND,
  PLUGINS_OVERVIEW_COMMAND,
  PLUGINS_READ_HOOK_SOURCE_COMMAND,
  PLUGINS_SET_HOOK_SOURCE_ENABLED_COMMAND,
  PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND,
  PLUGINS_SET_PACKAGE_ENABLED_COMMAND,
  PLUGINS_SET_SKILL_ENABLED_COMMAND,
  PLUGINS_UPDATE_HOOK_SOURCE_COMMAND,
  PLUGINS_UPDATE_MCP_SERVER_COMMAND,
  PLUGIN_BACKENDS,
  PLUGIN_BACKEND_LABELS,
  PLUGIN_ENABLED_LABELS,
  PLUGIN_MCP_TRANSPORTS,
  PLUGIN_MCP_TRANSPORT_LABELS,
  PLUGIN_SCOPES,
  PLUGIN_TOGGLE_ACTION_LABELS,
} from "./pluginsContract.mjs";

export type PluginScope = typeof PLUGIN_SCOPES[number];

export type PluginBackendKind = typeof PLUGIN_BACKENDS[number];

export type PluginMcpTransport = "stdio" | "http" | "oauth" | "unknown" | (string & {});

export {
  PLUGINS_CREATE_HOOK_SOURCE_COMMAND,
  PLUGINS_CREATE_MCP_SERVER_COMMAND,
  PLUGINS_CREATE_SKILL_COMMAND,
  PLUGINS_DELETE_HOOK_SOURCE_COMMAND,
  PLUGINS_DELETE_MCP_SERVER_COMMAND,
  PLUGINS_DELETE_PACKAGE_COMMAND,
  PLUGINS_DELETE_SKILL_COMMAND,
  PLUGINS_HOOKS_OVERVIEW_COMMAND,
  PLUGINS_INSTALL_PACKAGE_COMMAND,
  PLUGINS_OPEN_HOOK_CONFIG_COMMAND,
  PLUGINS_OPEN_MCP_CONFIG_COMMAND,
  PLUGINS_OVERVIEW_COMMAND,
  PLUGINS_READ_HOOK_SOURCE_COMMAND,
  PLUGINS_SET_HOOK_SOURCE_ENABLED_COMMAND,
  PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND,
  PLUGINS_SET_PACKAGE_ENABLED_COMMAND,
  PLUGINS_SET_SKILL_ENABLED_COMMAND,
  PLUGINS_UPDATE_HOOK_SOURCE_COMMAND,
  PLUGINS_UPDATE_MCP_SERVER_COMMAND,
  PLUGIN_BACKENDS,
  PLUGIN_BACKEND_LABELS,
  PLUGIN_ENABLED_LABELS,
  PLUGIN_MCP_TRANSPORTS,
  PLUGIN_MCP_TRANSPORT_LABELS,
  PLUGIN_SCOPES,
  PLUGIN_TOGGLE_ACTION_LABELS,
};

const PLUGIN_SCOPE_SET = new Set<string>(PLUGIN_SCOPES);
const PLUGIN_BACKEND_SET = new Set<string>(PLUGIN_BACKENDS);
const PLUGIN_MCP_TRANSPORT_SET = new Set<string>(PLUGIN_MCP_TRANSPORTS);

export function isPluginScope(value: unknown): value is PluginScope {
  return typeof value === "string" && PLUGIN_SCOPE_SET.has(value);
}

export function isPluginBackendKind(value: unknown): value is PluginBackendKind {
  return typeof value === "string" && PLUGIN_BACKEND_SET.has(value);
}

export function pluginEnabledLabel(enabled: boolean): string {
  return enabled ? PLUGIN_ENABLED_LABELS.enabled : PLUGIN_ENABLED_LABELS.disabled;
}

export function pluginToggleActionLabel(enabled: boolean): string {
  return enabled ? PLUGIN_TOGGLE_ACTION_LABELS.disable : PLUGIN_TOGGLE_ACTION_LABELS.enable;
}

export function pluginBackendLabel(backend: PluginBackendKind): string {
  return PLUGIN_BACKEND_LABELS[backend];
}

export function pluginMcpBackendLabel(backend: PluginBackendKind): string {
  return `${pluginBackendLabel(backend)} MCP`;
}

export function pluginMcpTransportLabel(transport: PluginMcpTransport | null | undefined): string {
  return typeof transport === "string" && PLUGIN_MCP_TRANSPORT_SET.has(transport)
    ? PLUGIN_MCP_TRANSPORT_LABELS[transport as keyof typeof PLUGIN_MCP_TRANSPORT_LABELS]
    : PLUGIN_MCP_TRANSPORT_LABELS.unknown;
}

export interface PluginSkill {
  backend: PluginBackendKind;
  scope: PluginScope;
  name: string;
  description: string;
  enabled: boolean;
  editable: boolean;
  path: string;
}

export interface PluginPackage {
  id: string;
  backend: PluginBackendKind;
  scope: PluginScope;
  name: string;
  description: string;
  version: string;
  enabled: boolean;
  editable: boolean;
  runtimeAvailable: boolean;
  path: string;
  packageSha256: string;
  skillCount: number;
  hookCount: number;
  mcpServerCount: number;
  warnings: string[];
}

export interface PluginMcpServer {
  backend: PluginBackendKind;
  name: string;
  command: string;
  args: string[];
  env?: Record<string, string>;
  envKeys: string[];
  enabled: boolean;
  editable: boolean;
  transport?: PluginMcpTransport;
}

export interface PluginMcpServerInput {
  name: string;
  command: string;
  args: string[];
  env?: Record<string, string>;
  removeEnvKeys?: string[];
}

export interface NativeRuntimePlugin {
  type: "local";
  path: string;
}

export interface NativeRuntimeMcpServer {
  type: "stdio";
  command: string;
  args?: string[];
  env?: Record<string, string>;
}

/** AgentKit-native runtime extensions (Mutsuki plugins / MCP / hooks). */
export interface NativeRuntimeExtensions {
  skills: string[];
  plugins: NativeRuntimePlugin[];
  mcpServers: Record<string, NativeRuntimeMcpServer> | PluginMcpServer[];
  hooks?: Record<string, unknown>;
  configPath?: string | null;
  warnings: string[];
}

/** @deprecated Use NativeRuntimePlugin. */
export type ClaudeRuntimePlugin = NativeRuntimePlugin;
/** @deprecated Use NativeRuntimeMcpServer. */
export type ClaudeRuntimeMcpServer = NativeRuntimeMcpServer;
/** @deprecated Use NativeRuntimeExtensions. */
export type ClaudeRuntimeExtensions = NativeRuntimeExtensions;
/** @deprecated Use NativeRuntimeExtensions. */
export type CodexRuntimeExtensions = NativeRuntimeExtensions;

export interface AgentRuntimeExtensions {
  "native-agentkit"?: NativeRuntimeExtensions;
  /** @deprecated Official product path removed; prefer native-agentkit. */
  claude?: NativeRuntimeExtensions;
  /** @deprecated Official product path removed; prefer native-agentkit. */
  codex?: NativeRuntimeExtensions;
}

export interface PluginsOverview {
  skills: PluginSkill[];
  packages: PluginPackage[];
  mcpServers: PluginMcpServer[];
  configPaths: Partial<Record<PluginBackendKind, string | null>>;
  pluginsRegistryRevision: number;
  pluginsRegistryPath: string | null;
  warnings: string[];
}

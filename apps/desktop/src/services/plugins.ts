/**
 * 插件 / 技能服务：包装 plugins_* 系列 Tauri command。
 * Rust 侧字段命名走 camelCase（serde rename_all），前端不需 key 映射。
 */

import { invoke } from "../tauri/runtime";
import type {
  HookDocumentUpdateInput,
  HookDocumentView,
  HookHandlerUpdateInput,
  HookHandlerView,
  HooksOverview,
  HookSourceSummary,
  PluginBackendKind,
  PluginMcpServer,
  PluginMcpServerInput,
  PluginPackage,
  PluginScope,
  PluginSkill,
  PluginsOverview,
} from "@lilia/contracts";
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
  PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND,
  PLUGINS_SET_HOOK_SOURCE_ENABLED_COMMAND,
  PLUGINS_SET_PACKAGE_ENABLED_COMMAND,
  PLUGINS_SET_SKILL_ENABLED_COMMAND,
  PLUGINS_UPDATE_HOOK_SOURCE_COMMAND,
  PLUGINS_UPDATE_MCP_SERVER_COMMAND,
} from "@lilia/contracts";

export type {
  HookDocumentUpdateInput,
  HookDocumentView,
  HookHandlerUpdateInput,
  HookHandlerView,
  HooksOverview,
  HookSourceSummary,
  PluginBackendKind,
  PluginMcpServer,
  PluginMcpServerInput,
  PluginPackage,
  PluginScope,
  PluginSkill,
  PluginsOverview,
};

export function pluginsOverview(projectCwd?: string | null): Promise<PluginsOverview> {
  return invoke<PluginsOverview>(PLUGINS_OVERVIEW_COMMAND, {
    projectCwd: projectCwd ?? null,
  });
}

export function hooksOverview(projectCwd?: string | null): Promise<HooksOverview> {
  return invoke<HooksOverview>(PLUGINS_HOOKS_OVERVIEW_COMMAND, {
    projectCwd: projectCwd ?? null,
  });
}

export function createSkill(
  scope: PluginScope,
  projectCwd: string | null,
  name: string,
  description: string,
): Promise<PluginSkill> {
  return invoke<PluginSkill>(PLUGINS_CREATE_SKILL_COMMAND, {
    scope,
    projectCwd,
    name,
    description,
  });
}

export function deleteSkill(
  scope: PluginScope,
  projectCwd: string | null,
  name: string,
): Promise<void> {
  return invoke<void>(PLUGINS_DELETE_SKILL_COMMAND, {
    scope,
    projectCwd,
    name,
  });
}

export function setSkillEnabled(
  scope: PluginScope,
  projectCwd: string | null,
  name: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>(PLUGINS_SET_SKILL_ENABLED_COMMAND, {
    scope,
    projectCwd,
    name,
    enabled,
  });
}

export function setPackageEnabled(
  backend: PluginBackendKind,
  scope: PluginScope,
  name: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>(PLUGINS_SET_PACKAGE_ENABLED_COMMAND, {
    backend,
    scope,
    name,
    enabled,
  });
}

export function createMcpServer(
  backend: PluginBackendKind,
  input: PluginMcpServerInput,
): Promise<PluginMcpServer> {
  return invoke<PluginMcpServer>(PLUGINS_CREATE_MCP_SERVER_COMMAND, { backend, input });
}

export function updateMcpServer(
  backend: PluginBackendKind,
  name: string,
  input: PluginMcpServerInput,
): Promise<PluginMcpServer> {
  return invoke<PluginMcpServer>(PLUGINS_UPDATE_MCP_SERVER_COMMAND, { backend, name, input });
}

export function deleteMcpServer(backend: PluginBackendKind, name: string): Promise<void> {
  return invoke<void>(PLUGINS_DELETE_MCP_SERVER_COMMAND, { backend, name });
}

export function setMcpServerEnabled(
  backend: PluginBackendKind,
  name: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>(PLUGINS_SET_MCP_SERVER_ENABLED_COMMAND, { backend, name, enabled });
}

export function openMcpConfig(backend: PluginBackendKind): Promise<void> {
  return invoke<void>(PLUGINS_OPEN_MCP_CONFIG_COMMAND, { backend });
}

export function readHookSource(source: HookSourceSummary): Promise<HookDocumentView> {
  return invoke<HookDocumentView>(PLUGINS_READ_HOOK_SOURCE_COMMAND, { source });
}

export function updateHookSource(
  source: HookSourceSummary,
  input: HookDocumentUpdateInput,
): Promise<HookDocumentView> {
  return invoke<HookDocumentView>(PLUGINS_UPDATE_HOOK_SOURCE_COMMAND, { source, input });
}

export function installPackage(
  backend: PluginBackendKind,
): Promise<PluginPackage | null> {
  return invoke<PluginPackage | null>(PLUGINS_INSTALL_PACKAGE_COMMAND, { backend });
}

export function deletePackage(
  backend: PluginBackendKind,
  name: string,
): Promise<void> {
  return invoke<void>(PLUGINS_DELETE_PACKAGE_COMMAND, { backend, name });
}

export function setHookSourceEnabled(
  source: HookSourceSummary,
  enabled: boolean,
): Promise<HookSourceSummary> {
  return invoke<HookSourceSummary>(PLUGINS_SET_HOOK_SOURCE_ENABLED_COMMAND, { source, enabled });
}

export function createHookSource(
  backend: PluginBackendKind,
  scope: string,
  projectCwd?: string | null,
): Promise<HookSourceSummary> {
  return invoke<HookSourceSummary>(PLUGINS_CREATE_HOOK_SOURCE_COMMAND, {
    backend,
    scope,
    projectCwd: projectCwd ?? null,
  });
}

export function deleteHookSource(source: HookSourceSummary): Promise<void> {
  return invoke<void>(PLUGINS_DELETE_HOOK_SOURCE_COMMAND, { source });
}

export function openHookConfig(source: HookSourceSummary): Promise<void> {
  return invoke<void>(PLUGINS_OPEN_HOOK_CONFIG_COMMAND, { source });
}


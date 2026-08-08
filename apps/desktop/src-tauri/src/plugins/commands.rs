use tauri::AppHandle;

use super::hooks::hooks_overview;
use super::runtime::overview;
use super::types::{
    HookDocumentUpdateInput, HookDocumentView, HookSourceSummary, HooksOverview, PluginMcpServer,
    PluginMcpServerInput, PluginSkill, PluginsOverview,
};

const REMOVED: &str = "官方 Claude / Codex 插件与 MCP 管理已移除；请使用 LiliaCore / Native AgentKit。";

#[tauri::command]
pub fn plugins_overview(app: AppHandle, project_cwd: Option<String>) -> PluginsOverview {
    overview(&app, project_cwd.as_deref())
}

#[tauri::command]
pub fn plugins_hooks_overview(app: AppHandle, project_cwd: Option<String>) -> HooksOverview {
    hooks_overview(&app, project_cwd.as_deref())
}

#[tauri::command]
pub fn plugins_create_skill(
    _app: AppHandle,
    _scope: String,
    _project_cwd: Option<String>,
    _name: String,
    _description: String,
) -> Result<PluginSkill, String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_delete_skill(
    _app: AppHandle,
    _scope: String,
    _project_cwd: Option<String>,
    _name: String,
) -> Result<(), String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_set_skill_enabled(
    _app: AppHandle,
    _scope: String,
    _project_cwd: Option<String>,
    _name: String,
    _enabled: bool,
) -> Result<(), String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_set_package_enabled(
    _app: AppHandle,
    _backend: String,
    _scope: String,
    _name: String,
    _enabled: bool,
) -> Result<(), String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_create_mcp_server(
    _app: AppHandle,
    _backend: String,
    _input: PluginMcpServerInput,
) -> Result<PluginMcpServer, String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_update_mcp_server(
    _app: AppHandle,
    _backend: String,
    _name: String,
    _input: PluginMcpServerInput,
) -> Result<PluginMcpServer, String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_delete_mcp_server(
    _app: AppHandle,
    _backend: String,
    _name: String,
) -> Result<(), String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_set_mcp_server_enabled(
    _app: AppHandle,
    _backend: String,
    _name: String,
    _enabled: bool,
) -> Result<(), String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_open_mcp_config(_app: AppHandle, _backend: String) -> Result<(), String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_read_hook_source(
    _app: AppHandle,
    _source: HookSourceSummary,
) -> Result<HookDocumentView, String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_update_hook_source(
    _app: AppHandle,
    _source: HookSourceSummary,
    _input: HookDocumentUpdateInput,
) -> Result<HookDocumentView, String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_create_hook_source(
    _app: AppHandle,
    _backend: String,
    _scope: String,
    _project_cwd: Option<String>,
) -> Result<HookSourceSummary, String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_delete_hook_source(
    _app: AppHandle,
    _source: HookSourceSummary,
) -> Result<(), String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_set_hook_source_enabled(
    _app: AppHandle,
    _source: HookSourceSummary,
    _enabled: bool,
) -> Result<HookSourceSummary, String> {
    Err(REMOVED.to_string())
}

#[tauri::command]
pub fn plugins_open_hook_config(
    _app: AppHandle,
    _source: HookSourceSummary,
) -> Result<(), String> {
    Err(REMOVED.to_string())
}

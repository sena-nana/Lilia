use std::collections::BTreeSet;
use std::path::PathBuf;

use lilia_desktop_application::{
    DesktopApplication, DesktopExtensionsSnapshot, DesktopFileDialogRequest,
    DesktopHookDocumentUpdate, DesktopHookHandlerUpdate, DesktopHostAction, DesktopHostResult,
    DesktopMcpCredentialKind, DesktopMcpServerUpsert, DesktopMcpServerView, DesktopMcpTransport,
    DesktopPluginInstall, DesktopSecret, DesktopSkillCreate, DesktopSkillScope,
};
use tauri::{AppHandle, State};

use super::hooks::{desktop_hook_scope, hook_document, hook_source, hooks_overview};
use super::runtime::{
    overview, plugin_mcp_server, plugin_package, plugin_skill, NATIVE_AGENTKIT_BACKEND,
};
use super::types::{
    HookDocumentUpdateInput, HookDocumentView, HookSourceSummary, HooksOverview, PluginMcpServer,
    PluginMcpServerInput, PluginPackage, PluginSkill, PluginsOverview,
};

#[tauri::command]
pub fn plugins_overview(
    _app: AppHandle,
    _project_cwd: Option<String>,
    application: State<'_, DesktopApplication>,
) -> PluginsOverview {
    overview(&application)
}

#[tauri::command]
pub fn plugins_hooks_overview(
    _app: AppHandle,
    project_cwd: Option<String>,
    application: State<'_, DesktopApplication>,
) -> Result<HooksOverview, String> {
    hooks_overview(&application, project_cwd.as_deref())
}

#[tauri::command]
pub fn plugins_create_skill(
    _app: AppHandle,
    scope: String,
    project_cwd: Option<String>,
    name: String,
    description: String,
    application: State<'_, DesktopApplication>,
) -> Result<PluginSkill, String> {
    let snapshot = application.extensions_snapshot().map_err(string_error)?;
    let requested_skill_id = name.trim().to_owned();
    let result = application
        .create_skill_package(DesktopSkillCreate {
            expected_registry_revision: snapshot.skills_registry_revision,
            scope: desktop_skill_scope(&scope)?,
            project_cwd,
            skill_id: name.clone(),
            description,
        })
        .map_err(string_error)?;
    result
        .skills
        .into_iter()
        .find(|skill| skill.skill_id == requested_skill_id)
        .map(plugin_skill)
        .ok_or_else(|| format!("Skill `{name}` was not persisted"))
}

#[tauri::command]
pub fn plugins_delete_skill(
    _app: AppHandle,
    scope: String,
    _project_cwd: Option<String>,
    name: String,
    application: State<'_, DesktopApplication>,
) -> Result<(), String> {
    let snapshot = application.extensions_snapshot().map_err(string_error)?;
    ensure_skill_scope(&snapshot, &scope, &name)?;
    application
        .delete_skill_package(&name, snapshot.skills_registry_revision)
        .map_err(string_error)?;
    Ok(())
}

#[tauri::command]
pub fn plugins_set_skill_enabled(
    _app: AppHandle,
    scope: String,
    _project_cwd: Option<String>,
    name: String,
    enabled: bool,
    application: State<'_, DesktopApplication>,
) -> Result<(), String> {
    let snapshot = application.extensions_snapshot().map_err(string_error)?;
    ensure_skill_scope(&snapshot, &scope, &name)?;
    application
        .set_skill_package_enabled(&name, enabled, snapshot.skills_registry_revision)
        .map_err(string_error)?;
    Ok(())
}

#[tauri::command]
pub fn plugins_install_package(
    _app: AppHandle,
    backend: String,
    application: State<'_, DesktopApplication>,
) -> Result<Option<PluginPackage>, String> {
    ensure_native_agentkit_backend(&backend)?;
    let snapshot = application.extensions_snapshot().map_err(string_error)?;
    let selected = application
        .execute_host(DesktopHostAction::FileDialog(DesktopFileDialogRequest {
            dialog_id: "plugins.install-package".to_owned(),
            title: Some("选择 Lilia Plugin 目录".to_owned()),
            initial_directory: None,
            filters: Vec::new(),
            select_directories: true,
            multiple: false,
        }))
        .map_err(string_error)?;
    let DesktopHostResult::FileDialogSelection(paths) = selected else {
        return Err("Plugin directory picker returned an invalid result".to_owned());
    };
    let Some(source_path) = paths.into_iter().next() else {
        return Ok(None);
    };
    application
        .install_plugin_package(DesktopPluginInstall {
            expected_registry_revision: snapshot.plugins_registry_revision,
            source_path: source_path.to_string_lossy().into_owned(),
        })
        .map(plugin_package)
        .map(Some)
        .map_err(string_error)
}

#[tauri::command]
pub fn plugins_delete_package(
    _app: AppHandle,
    backend: String,
    name: String,
    application: State<'_, DesktopApplication>,
) -> Result<(), String> {
    ensure_native_agentkit_backend(&backend)?;
    let snapshot = application.extensions_snapshot().map_err(string_error)?;
    application
        .delete_plugin_package(&name, snapshot.plugins_registry_revision)
        .map_err(string_error)
}

#[tauri::command]
pub fn plugins_set_package_enabled(
    _app: AppHandle,
    backend: String,
    scope: String,
    name: String,
    enabled: bool,
    application: State<'_, DesktopApplication>,
) -> Result<(), String> {
    ensure_native_agentkit_backend(&backend)?;
    if scope != "user" {
        return Err(format!("unsupported Plugin scope `{scope}`"));
    }
    let snapshot = application.extensions_snapshot().map_err(string_error)?;
    application
        .set_plugin_package_enabled(&name, enabled, snapshot.plugins_registry_revision)
        .map_err(string_error)?;
    Ok(())
}

#[tauri::command]
pub fn plugins_create_mcp_server(
    _app: AppHandle,
    backend: String,
    input: PluginMcpServerInput,
    application: State<'_, DesktopApplication>,
) -> Result<PluginMcpServer, String> {
    ensure_native_agentkit_backend(&backend)?;
    let snapshot = application.extensions_snapshot().map_err(string_error)?;
    if find_mcp_server(&snapshot, &input.name).is_some() {
        return Err(format!("MCP server `{}` already exists", input.name));
    }
    save_stdio_mcp_server(&application, None, input, &snapshot)
}

#[tauri::command]
pub fn plugins_update_mcp_server(
    _app: AppHandle,
    backend: String,
    name: String,
    input: PluginMcpServerInput,
    application: State<'_, DesktopApplication>,
) -> Result<PluginMcpServer, String> {
    ensure_native_agentkit_backend(&backend)?;
    let snapshot = application.extensions_snapshot().map_err(string_error)?;
    let current = find_mcp_server(&snapshot, &name)
        .cloned()
        .ok_or_else(|| format!("MCP server `{name}` is not registered"))?;
    if input.name.trim() != name {
        return Err("MCP server ID cannot be changed after creation".to_owned());
    }
    if current.transport != "stdio" || !current.editable {
        return Err(format!("MCP server `{name}` is not editable as stdio"));
    }
    save_stdio_mcp_server(&application, Some(current), input, &snapshot)
}

#[tauri::command]
pub fn plugins_delete_mcp_server(
    _app: AppHandle,
    backend: String,
    name: String,
    application: State<'_, DesktopApplication>,
) -> Result<(), String> {
    ensure_native_agentkit_backend(&backend)?;
    let snapshot = application.extensions_snapshot().map_err(string_error)?;
    application
        .delete_mcp_server(&name, snapshot.mcp_registry_revision)
        .map_err(string_error)?;
    Ok(())
}

#[tauri::command]
pub fn plugins_set_mcp_server_enabled(
    _app: AppHandle,
    backend: String,
    name: String,
    enabled: bool,
    application: State<'_, DesktopApplication>,
) -> Result<(), String> {
    ensure_native_agentkit_backend(&backend)?;
    let snapshot = application.extensions_snapshot().map_err(string_error)?;
    application
        .set_mcp_server_enabled(&name, enabled, snapshot.mcp_registry_revision)
        .map_err(string_error)?;
    Ok(())
}

#[tauri::command]
pub fn plugins_open_mcp_config(
    _app: AppHandle,
    backend: String,
    application: State<'_, DesktopApplication>,
) -> Result<(), String> {
    ensure_native_agentkit_backend(&backend)?;
    let path = application
        .extensions_snapshot()
        .map_err(string_error)?
        .mcp_registry_path;
    application
        .execute_host(DesktopHostAction::OpenPath(PathBuf::from(path)))
        .map_err(string_error)?;
    Ok(())
}

fn ensure_native_agentkit_backend(backend: &str) -> Result<(), String> {
    if backend == NATIVE_AGENTKIT_BACKEND {
        Ok(())
    } else {
        Err(format!("unsupported plugin backend `{backend}`"))
    }
}

fn desktop_skill_scope(scope: &str) -> Result<DesktopSkillScope, String> {
    match scope {
        "user" => Ok(DesktopSkillScope::User),
        "project" => Ok(DesktopSkillScope::Project),
        value => Err(format!("unsupported Skill scope `{value}`")),
    }
}

fn ensure_skill_scope(
    snapshot: &DesktopExtensionsSnapshot,
    scope: &str,
    skill_id: &str,
) -> Result<(), String> {
    desktop_skill_scope(scope)?;
    let skill = snapshot
        .skills
        .iter()
        .find(|skill| skill.skill_id == skill_id)
        .ok_or_else(|| format!("Skill `{skill_id}` is not registered"))?;
    if skill.scope != scope {
        return Err(format!(
            "Skill `{skill_id}` is registered in `{}` scope, not `{scope}`",
            skill.scope
        ));
    }
    Ok(())
}

fn find_mcp_server<'a>(
    snapshot: &'a DesktopExtensionsSnapshot,
    server_id: &str,
) -> Option<&'a DesktopMcpServerView> {
    snapshot
        .mcp_servers
        .iter()
        .find(|server| server.server_id == server_id)
}

fn save_stdio_mcp_server(
    application: &DesktopApplication,
    current: Option<DesktopMcpServerView>,
    input: PluginMcpServerInput,
    snapshot: &DesktopExtensionsSnapshot,
) -> Result<PluginMcpServer, String> {
    let mut env_secret_names = current
        .as_ref()
        .into_iter()
        .flat_map(|server| &server.credentials)
        .filter(|credential| credential.kind == DesktopMcpCredentialKind::Environment)
        .map(|credential| credential.name.clone())
        .collect::<BTreeSet<_>>();
    for name in &input.remove_env_keys {
        env_secret_names.remove(name);
    }
    let env = input.env.unwrap_or_default();
    env_secret_names.extend(env.keys().cloned());
    let server_id = input.name.trim().to_owned();
    let enabled = current.as_ref().is_some_and(|server| server.enabled);
    application
        .upsert_mcp_server(DesktopMcpServerUpsert {
            expected_registry_revision: snapshot.mcp_registry_revision,
            server_id: server_id.clone(),
            transport: DesktopMcpTransport::Stdio,
            command: Some(input.command),
            args: input.args,
            url: None,
            env_secret_names: env_secret_names.into_iter().collect(),
            header_secret_names: Vec::new(),
            enabled,
        })
        .map_err(string_error)?;
    for (name, value) in env {
        application
            .set_mcp_server_credential(
                &server_id,
                DesktopMcpCredentialKind::Environment,
                &name,
                DesktopSecret::new(value.into_bytes()),
            )
            .map_err(string_error)?;
    }
    application
        .extensions_snapshot()
        .map_err(string_error)?
        .mcp_servers
        .into_iter()
        .find(|server| server.server_id == server_id)
        .map(plugin_mcp_server)
        .ok_or_else(|| format!("MCP server `{server_id}` was not persisted"))
}

fn string_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod mcp_adapter_tests {
    use super::*;

    #[test]
    fn backend_guard_accepts_only_native_agentkit() {
        assert!(ensure_native_agentkit_backend(NATIVE_AGENTKIT_BACKEND).is_ok());
        assert!(ensure_native_agentkit_backend("codex").is_err());
        assert!(ensure_native_agentkit_backend("claude").is_err());
    }
}

#[tauri::command]
pub fn plugins_read_hook_source(
    _app: AppHandle,
    source: HookSourceSummary,
    application: State<'_, DesktopApplication>,
) -> Result<HookDocumentView, String> {
    ensure_native_agentkit_backend(&source.backend)?;
    application
        .read_hook_source(
            desktop_hook_scope(&source.scope)?,
            source.project_cwd.as_deref(),
        )
        .map(hook_document)
        .map_err(string_error)
}

#[tauri::command]
pub fn plugins_update_hook_source(
    _app: AppHandle,
    source: HookSourceSummary,
    input: HookDocumentUpdateInput,
    application: State<'_, DesktopApplication>,
) -> Result<HookDocumentView, String> {
    ensure_native_agentkit_backend(&source.backend)?;
    if input
        .handlers
        .iter()
        .any(|handler| handler.group_advanced_json.is_some() || handler.advanced_json.is_some())
    {
        return Err("Native AgentKit Hooks 不支持高级 JSON 字段".to_owned());
    }
    application
        .update_hook_source(
            desktop_hook_scope(&source.scope)?,
            source.project_cwd.as_deref(),
            DesktopHookDocumentUpdate {
                expected_revision: input.expected_revision,
                handlers: input
                    .handlers
                    .into_iter()
                    .map(|handler| DesktopHookHandlerUpdate {
                        id: handler.id,
                        event: handler.event,
                        matcher: handler.matcher,
                        handler_type: handler.handler_type,
                        command: handler.command,
                        command_windows: handler.command_windows,
                        timeout_seconds: handler.timeout_seconds,
                        status_message: handler.status_message,
                    })
                    .collect(),
            },
        )
        .map(hook_document)
        .map_err(string_error)
}

#[tauri::command]
pub fn plugins_create_hook_source(
    _app: AppHandle,
    backend: String,
    scope: String,
    project_cwd: Option<String>,
    application: State<'_, DesktopApplication>,
) -> Result<HookSourceSummary, String> {
    ensure_native_agentkit_backend(&backend)?;
    application
        .create_hook_source(desktop_hook_scope(&scope)?, project_cwd.as_deref())
        .map(hook_source)
        .map_err(string_error)
}

#[tauri::command]
pub fn plugins_delete_hook_source(
    _app: AppHandle,
    source: HookSourceSummary,
    application: State<'_, DesktopApplication>,
) -> Result<(), String> {
    ensure_native_agentkit_backend(&source.backend)?;
    application
        .delete_hook_source(
            desktop_hook_scope(&source.scope)?,
            source.project_cwd.as_deref(),
            source.revision,
        )
        .map_err(string_error)
}

#[tauri::command]
pub fn plugins_set_hook_source_enabled(
    _app: AppHandle,
    source: HookSourceSummary,
    enabled: bool,
    application: State<'_, DesktopApplication>,
) -> Result<HookSourceSummary, String> {
    ensure_native_agentkit_backend(&source.backend)?;
    application
        .set_hook_source_enabled(
            desktop_hook_scope(&source.scope)?,
            source.project_cwd.as_deref(),
            source.revision,
            enabled,
        )
        .map(hook_source)
        .map_err(string_error)
}

#[tauri::command]
pub fn plugins_open_hook_config(
    _app: AppHandle,
    source: HookSourceSummary,
    application: State<'_, DesktopApplication>,
) -> Result<(), String> {
    ensure_native_agentkit_backend(&source.backend)?;
    let current = application
        .hook_source(
            desktop_hook_scope(&source.scope)?,
            source.project_cwd.as_deref(),
        )
        .map_err(string_error)?;
    application
        .execute_host(DesktopHostAction::OpenPath(PathBuf::from(current.path)))
        .map_err(string_error)?;
    Ok(())
}

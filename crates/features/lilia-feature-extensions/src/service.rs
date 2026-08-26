//! Skill / MCP registry snapshot and mutations.
//!
//! The host supplies registry I/O that talks to AgentKit and the OS keyring.
//! This module owns revision checks, skill directory publish, credential
//! lifecycle and the snapshot fold.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use lilia_storage::{
    AgentkitMcpRegistryEntry, AgentkitSkillPackageRef, AgentkitSkillsRegistry, LiliaDataPaths,
};
use mutsuki_agent_contracts::{
    McpCatalog, McpServerStatus, SkillDiscoverResult, SkillLoadResult, SkillSourceKind,
};
use serde_json::Value;

use crate::error::{invalid_input, ExtensionsError};
use crate::mcp::{
    bump_registry_revision, ensure_mcp_credential_is_configured, ensure_registry_revision,
    mcp_activation_error, mcp_credential_key, mcp_prompts, mcp_resource_contents, mcp_resources,
    mcp_state_key, mcp_tools, normalized_mcp_credential_name, normalized_server_id,
    removed_mcp_credentials, required_mcp_value, validate_mcp_secret, NormalizedMcpServer,
};
use crate::skill::{
    bump_skills_registry_revision, ensure_skills_registry_revision, is_managed_skill,
    managed_skill_root, normalized_skill_description, normalized_skill_id, skill_io_error,
    verified_managed_skill_path, write_skill_document,
};
use crate::types::{
    ExtensionsSnapshot, McpActivationReport, McpActivationResult, McpCredentialKind,
    McpCredentialView, McpPromptFragmentView, McpPromptGetView, McpResourceReadView,
    McpServerUpsert, McpServerView, PluginPackageView, RuntimeServiceView, SkillCreate,
    SkillPackageView, SkillScope,
};

/// Plugin package facts the snapshot needs, without the host's plugin type.
#[derive(Clone, Debug)]
pub struct LoadedPluginFacts {
    pub root: PathBuf,
    pub skill_paths: Vec<PathBuf>,
    pub mcp_servers: Vec<AgentkitMcpRegistryEntry>,
}

/// Shared coding-service identity the extensions surface renders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodingRuntimeFacts {
    pub data_source: String,
    pub shared_identity_ok: bool,
    pub mcp_same_instance: bool,
    pub git_service_id: String,
    pub git_same_instance: bool,
    pub code_index_service_id: String,
    pub code_index_same_instance: bool,
    pub lsp_service_id: String,
    pub lsp_same_instance: bool,
    pub mcp_service_id: String,
    pub computer_use_service_id: String,
    pub computer_use_same_instance: bool,
    pub memory_runner_id: String,
    pub memory_shared_router: bool,
}

/// Prompt body the host already fetched from AgentKit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpPromptRead {
    pub description: Option<String>,
    pub fragments: Vec<McpPromptFragmentView>,
}

/// Host I/O the extensions domain cannot own: paths, plugins, AgentKit, keyring.
pub trait ExtensionsHost: Send + Sync {
    fn data_paths(&self) -> LiliaDataPaths;
    fn plugin_packages(&self) -> Result<(u64, String, Vec<PluginPackageView>), ExtensionsError>;
    fn loaded_plugins(&self) -> Vec<LoadedPluginFacts>;
    fn coding_runtime(&self) -> Result<CodingRuntimeFacts, ExtensionsError>;
    fn active_mcp_servers(&self) -> Result<Vec<McpServerStatus>, ExtensionsError>;
    fn mcp_catalog(&self) -> Result<McpCatalog, ExtensionsError>;
    fn skill_catalog(&self) -> Result<SkillDiscoverResult, ExtensionsError>;
    fn load_skill(&self, skill_id: &str) -> Result<SkillLoadResult, ExtensionsError>;
    fn reload_registered_skills(&self) -> Result<(), ExtensionsError>;
    fn disconnect_mcp(&self, server_id: &str) -> Result<(), String>;
    fn activate_registered_mcp(
        &self,
        entry: AgentkitMcpRegistryEntry,
        env: Vec<(String, String)>,
        headers: Vec<(String, String)>,
    ) -> McpActivationResult;
    fn read_mcp_resource_raw(
        &self,
        server_id: &str,
        uri: &str,
    ) -> Result<(String, String, Value), ExtensionsError>;
    fn get_mcp_prompt_raw(
        &self,
        namespaced_name: &str,
        arguments: Value,
    ) -> Result<McpPromptRead, ExtensionsError>;
    fn read_secret(&self, key: &str) -> Result<Option<Vec<u8>>, ExtensionsError>;
    fn write_secret(&self, key: &str, secret: Vec<u8>) -> Result<(), ExtensionsError>;
    fn delete_secret(&self, key: &str) -> Result<(), ExtensionsError>;
}

#[derive(Default)]
struct ResolvedMcpCredentials {
    env: Vec<(String, String)>,
    headers: Vec<(String, String)>,
}

pub fn extensions_snapshot(
    host: &dyn ExtensionsHost,
) -> Result<ExtensionsSnapshot, ExtensionsError> {
    let paths = host.data_paths();
    let skill_registry = lilia_storage::load_skills_registry(&paths)?;
    let mcp_registry = lilia_storage::load_mcp_registry(&paths)?;
    let (plugins_registry_revision, plugins_registry_path, plugins) = host.plugin_packages()?;
    let loaded_plugins = host.loaded_plugins();
    let plugin_mcp_entries = loaded_plugins
        .iter()
        .flat_map(|package| package.mcp_servers.iter().cloned())
        .collect::<Vec<_>>();
    let coding_status = host.coding_runtime()?;
    let active_servers = host.active_mcp_servers()?;
    let catalog = host.mcp_catalog()?;
    let skill_catalog = host.skill_catalog()?;
    let runtime_skill_ids = skill_catalog
        .catalog
        .iter()
        .map(|skill| skill.skill_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut skills = skill_registry
        .as_ref()
        .map(|registry| {
            registry
                .packages
                .iter()
                .map(|package| SkillPackageView {
                    runtime_available: runtime_skill_ids.contains(package.skill_id.as_str()),
                    editable: package.registered_from == "lilia.desktop.skill-manager",
                    skill_id: package.skill_id.clone(),
                    path: package.path.clone(),
                    registered_from: package.registered_from.clone(),
                    scope: package.scope.clone(),
                    description: package.description.clone(),
                    enabled: package.enabled,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for entry in skill_catalog
        .catalog
        .iter()
        .filter(|entry| entry.source_kind == SkillSourceKind::Plugin)
    {
        let loaded = host.load_skill(&entry.skill_id)?;
        let source_path = PathBuf::from(&loaded.descriptor.provenance.source_path);
        let owner = loaded_plugins
            .iter()
            .find(|package| source_path.starts_with(&package.root))
            .and_then(|package| package.root.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        skills.push(SkillPackageView {
            skill_id: entry.skill_id.clone(),
            path: source_path.to_string_lossy().into_owned(),
            registered_from: format!("lilia.plugin:{owner}"),
            scope: "plugin".to_owned(),
            description: entry.summary.clone(),
            enabled: true,
            editable: false,
            runtime_available: entry.available,
        });
    }
    skills.sort_by(|left, right| {
        left.skill_id
            .cmp(&right.skill_id)
            .then_with(|| left.registered_from.cmp(&right.registered_from))
    });
    let active_by_id = active_servers
        .iter()
        .map(|status| (status.server_id.as_str(), status))
        .collect::<BTreeMap<_, _>>();
    let registered_ids = mcp_registry
        .iter()
        .flat_map(|registry| &registry.servers)
        .chain(plugin_mcp_entries.iter())
        .map(|entry| entry.server_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut mcp_servers = mcp_registry
        .iter()
        .flat_map(|registry| &registry.servers)
        .map(|entry| -> Result<McpServerView, ExtensionsError> {
            mcp_server_view(host, &catalog, &active_by_id, entry, true)
        })
        .collect::<Result<Vec<_>, _>>()?;
    mcp_servers.extend(
        plugin_mcp_entries
            .iter()
            .map(|entry| -> Result<McpServerView, ExtensionsError> {
                mcp_server_view(host, &catalog, &active_by_id, entry, false)
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    mcp_servers.extend(
        active_servers
            .iter()
            .filter(|status| !registered_ids.contains(status.server_id.as_str()))
            .map(|status| McpServerView {
                server_id: status.server_id.clone(),
                source: "runtime".to_owned(),
                transport: "runtime".to_owned(),
                location: None,
                registered: false,
                editable: false,
                enabled: true,
                command: None,
                args: Vec::new(),
                url: None,
                registered_from: None,
                runtime_state: Some(mcp_state_key(&status.state).to_owned()),
                tool_count: status.tool_count,
                resource_count: status.resource_count,
                prompt_count: status.prompt_count,
                restart_count: status.restart_count,
                last_error: status.last_error.clone(),
                tools: mcp_tools(&catalog, &status.server_id),
                resources: mcp_resources(&catalog, &status.server_id),
                prompts: mcp_prompts(&catalog, &status.server_id),
                credentials: Vec::new(),
            }),
    );
    mcp_servers.sort_by(|left, right| left.server_id.cmp(&right.server_id));

    Ok(ExtensionsSnapshot {
        data_source: coding_status.data_source.clone(),
        shared_identity_ok: coding_status.shared_identity_ok && coding_status.mcp_same_instance,
        skills_registry_path: lilia_storage::skills_registry_path(&paths)
            .display()
            .to_string(),
        skills_registry_revision: skill_registry
            .as_ref()
            .map(|registry| registry.revision)
            .unwrap_or_default(),
        mcp_registry_path: lilia_storage::mcp_registry_path(&paths)
            .display()
            .to_string(),
        mcp_registry_revision: mcp_registry
            .as_ref()
            .map(|registry| registry.revision)
            .unwrap_or_default(),
        plugins_registry_path,
        plugins_registry_revision,
        skill_roots: skill_registry
            .as_ref()
            .map(|registry| registry.user_skill_roots.clone())
            .unwrap_or_default(),
        skills,
        plugins,
        mcp_servers,
        runtime_services: coding_runtime_services(&coding_status),
        legacy_plugin_manager_available: true,
        legacy_hooks_manager_available: false,
    })
}

pub fn create_skill_package(
    host: &dyn ExtensionsHost,
    input: SkillCreate,
) -> Result<ExtensionsSnapshot, ExtensionsError> {
    let skill_id = normalized_skill_id(&input.skill_id)?;
    let description = normalized_skill_description(&input.description)?;
    if input.scope == SkillScope::Project {
        return Err(invalid_input(
            "scope",
            "project Skills require task-scoped runtime catalogs and are not globally mutable",
        ));
    }
    let root = managed_skill_root(
        host.data_paths().home(),
        input.scope,
        input.project_cwd.as_deref(),
    )?;
    let package_path = root.join(&skill_id);
    let paths = host.data_paths();
    let mut registry = lilia_storage::load_skills_registry(&paths)?.unwrap_or_default();
    ensure_skills_registry_revision(registry.revision, input.expected_registry_revision)?;
    if registry
        .packages
        .iter()
        .any(|package| package.skill_id == skill_id)
    {
        return Err(invalid_input(
            "skill_id",
            format!("Skill `{skill_id}` is already registered"),
        ));
    }
    if package_path.exists() {
        return Err(invalid_input(
            "skill_id",
            format!(
                "Skill directory `{}` already exists",
                package_path.display()
            ),
        ));
    }
    fs::create_dir_all(&root)
        .map_err(|error| skill_io_error("create managed Skill root", error))?;
    let staging = root.join(format!(".{skill_id}.creating-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&staging)
        .map_err(|error| skill_io_error("create Skill staging directory", error))?;
    if let Err(error) = write_skill_document(&staging, &skill_id, &description) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if let Err(error) = fs::rename(&staging, &package_path) {
        let _ = fs::remove_dir_all(&staging);
        return Err(skill_io_error("publish Skill directory", error));
    }

    let previous = registry.clone();
    if !registry
        .user_skill_roots
        .iter()
        .any(|candidate| Path::new(candidate) == root)
    {
        registry
            .user_skill_roots
            .push(root.to_string_lossy().into_owned());
        registry.user_skill_roots.sort();
    }
    registry.packages.push(AgentkitSkillPackageRef {
        skill_id: skill_id.clone(),
        path: package_path.to_string_lossy().into_owned(),
        registered_from: "lilia.desktop.skill-manager".to_owned(),
        scope: input.scope.as_registry().to_owned(),
        description,
        enabled: true,
    });
    registry
        .packages
        .sort_by(|left, right| left.skill_id.cmp(&right.skill_id));
    bump_skills_registry_revision(&mut registry)?;
    if let Err(error) = persist_and_reload_skills(host, &paths, &registry) {
        let _ = lilia_storage::save_skills_registry(&paths, &previous);
        let _ = host.reload_registered_skills();
        let _ = fs::remove_dir_all(&package_path);
        return Err(error);
    }
    extensions_snapshot(host)
}

pub fn set_skill_package_enabled(
    host: &dyn ExtensionsHost,
    skill_id: &str,
    enabled: bool,
    expected_registry_revision: u64,
) -> Result<ExtensionsSnapshot, ExtensionsError> {
    let skill_id = normalized_skill_id(skill_id)?;
    let paths = host.data_paths();
    let mut registry = lilia_storage::load_skills_registry(&paths)?.unwrap_or_default();
    ensure_skills_registry_revision(registry.revision, expected_registry_revision)?;
    let previous = registry.clone();
    let package = registry
        .packages
        .iter_mut()
        .find(|package| package.skill_id == skill_id)
        .ok_or_else(|| {
            invalid_input("skill_id", format!("Skill `{skill_id}` is not registered"))
        })?;
    if !is_managed_skill(package) {
        return Err(invalid_input("skill_id", "imported Skills are read-only"));
    }
    if package.enabled == enabled {
        return extensions_snapshot(host);
    }
    package.enabled = enabled;
    bump_skills_registry_revision(&mut registry)?;
    if let Err(error) = persist_and_reload_skills(host, &paths, &registry) {
        let _ = lilia_storage::save_skills_registry(&paths, &previous);
        let _ = host.reload_registered_skills();
        return Err(error);
    }
    extensions_snapshot(host)
}

pub fn delete_skill_package(
    host: &dyn ExtensionsHost,
    skill_id: &str,
    expected_registry_revision: u64,
) -> Result<ExtensionsSnapshot, ExtensionsError> {
    let skill_id = normalized_skill_id(skill_id)?;
    let paths = host.data_paths();
    let mut registry = lilia_storage::load_skills_registry(&paths)?.unwrap_or_default();
    ensure_skills_registry_revision(registry.revision, expected_registry_revision)?;
    let index = registry
        .packages
        .iter()
        .position(|package| package.skill_id == skill_id)
        .ok_or_else(|| {
            invalid_input("skill_id", format!("Skill `{skill_id}` is not registered"))
        })?;
    let package = registry.packages[index].clone();
    if !is_managed_skill(&package) {
        return Err(invalid_input("skill_id", "imported Skills are read-only"));
    }
    let root = managed_skill_root(paths.home(), SkillScope::User, None)?;
    let package_path = verified_managed_skill_path(&root, &package.path, &skill_id)?;
    let staging = root.join(format!(".{skill_id}.deleting-{}", uuid::Uuid::new_v4()));
    fs::rename(&package_path, &staging)
        .map_err(|error| skill_io_error("stage Skill deletion", error))?;
    let previous = registry.clone();
    registry.packages.remove(index);
    bump_skills_registry_revision(&mut registry)?;
    if let Err(error) = persist_and_reload_skills(host, &paths, &registry) {
        let _ = lilia_storage::save_skills_registry(&paths, &previous);
        let _ = host.reload_registered_skills();
        let _ = fs::rename(&staging, &package_path);
        return Err(error);
    }
    fs::remove_dir_all(&staging)
        .map_err(|error| skill_io_error("remove deleted Skill directory", error))?;
    extensions_snapshot(host)
}

pub fn upsert_mcp_server(
    host: &dyn ExtensionsHost,
    input: McpServerUpsert,
) -> Result<McpActivationReport, ExtensionsError> {
    let normalized = NormalizedMcpServer::new(input)?;
    let paths = host.data_paths();
    let mut registry = lilia_storage::load_mcp_registry(&paths)?.unwrap_or_default();
    ensure_registry_revision(registry.revision, normalized.expected_registry_revision)?;
    let entry = normalized.registry_entry();
    let removed_credentials = registry
        .servers
        .iter()
        .find(|current| current.server_id == entry.server_id)
        .map(|current| removed_mcp_credentials(current, &entry));
    let removed_credential_backup = removed_credentials
        .as_ref()
        .map(|removed| read_mcp_credentials(host, removed))
        .transpose()?
        .unwrap_or_default();
    if let Some(removed) = &removed_credentials {
        delete_mcp_credentials(host, removed, &removed_credential_backup)?;
    }
    if let Some(current) = registry
        .servers
        .iter_mut()
        .find(|current| current.server_id == entry.server_id)
    {
        *current = entry;
    } else {
        registry.servers.push(entry);
        registry
            .servers
            .sort_by(|left, right| left.server_id.cmp(&right.server_id));
    }
    if let Err(error) = bump_registry_revision(&mut registry) {
        if let Some(removed) = &removed_credentials {
            restore_mcp_credentials(host, removed, &removed_credential_backup)?;
        }
        return Err(error);
    }
    if let Err(error) = lilia_storage::save_mcp_registry(&paths, &registry) {
        if let Some(removed) = &removed_credentials {
            restore_mcp_credentials(host, removed, &removed_credential_backup)?;
        }
        return Err(error.into());
    }
    let result = reconcile_mcp_runtime(host, &normalized.server_id, normalized.enabled);
    Ok(McpActivationReport {
        results: vec![result],
        snapshot: extensions_snapshot(host)?,
    })
}

pub fn set_mcp_server_enabled(
    host: &dyn ExtensionsHost,
    server_id: &str,
    enabled: bool,
    expected_registry_revision: u64,
) -> Result<McpActivationReport, ExtensionsError> {
    let server_id = normalized_server_id(server_id)?;
    let paths = host.data_paths();
    let mut registry = lilia_storage::load_mcp_registry(&paths)?.unwrap_or_default();
    ensure_registry_revision(registry.revision, expected_registry_revision)?;
    let entry = registry
        .servers
        .iter_mut()
        .find(|entry| entry.server_id == server_id)
        .ok_or_else(|| invalid_input("server_id", "MCP server is not registered"))?;
    if entry.enabled != enabled {
        entry.enabled = enabled;
        bump_registry_revision(&mut registry)?;
        lilia_storage::save_mcp_registry(&paths, &registry)?;
    }
    let result = reconcile_mcp_runtime(host, &server_id, enabled);
    Ok(McpActivationReport {
        results: vec![result],
        snapshot: extensions_snapshot(host)?,
    })
}

pub fn delete_mcp_server(
    host: &dyn ExtensionsHost,
    server_id: &str,
    expected_registry_revision: u64,
) -> Result<McpActivationReport, ExtensionsError> {
    let server_id = normalized_server_id(server_id)?;
    let paths = host.data_paths();
    let mut registry = lilia_storage::load_mcp_registry(&paths)?.unwrap_or_default();
    ensure_registry_revision(registry.revision, expected_registry_revision)?;
    let entry = registry
        .servers
        .iter()
        .find(|entry| entry.server_id == server_id)
        .cloned()
        .ok_or_else(|| invalid_input("server_id", "MCP server is not registered"))?;
    let credential_backup = read_mcp_credentials(host, &entry)?;
    delete_mcp_credentials(host, &entry, &credential_backup)?;
    let before = registry.servers.len();
    registry
        .servers
        .retain(|entry| entry.server_id != server_id);
    if registry.servers.len() == before {
        return Err(invalid_input("server_id", "MCP server is not registered"));
    }
    if let Err(error) = bump_registry_revision(&mut registry) {
        restore_mcp_credentials(host, &entry, &credential_backup)?;
        return Err(error);
    }
    if let Err(error) = lilia_storage::save_mcp_registry(&paths, &registry) {
        restore_mcp_credentials(host, &entry, &credential_backup)?;
        return Err(error.into());
    }
    let result = disconnect_mcp_runtime(host, &server_id);
    Ok(McpActivationReport {
        results: vec![result],
        snapshot: extensions_snapshot(host)?,
    })
}

pub fn set_mcp_server_credential(
    host: &dyn ExtensionsHost,
    server_id: &str,
    kind: McpCredentialKind,
    name: &str,
    secret: Vec<u8>,
) -> Result<McpActivationReport, ExtensionsError> {
    let server_id = required_mcp_value("server_id", server_id)?.to_owned();
    let name = normalized_mcp_credential_name(kind, name)?;
    validate_mcp_secret(kind, &secret)?;
    let entry = registered_mcp_entry(host, &server_id)?
        .ok_or_else(|| invalid_input("server_id", "MCP server is not registered"))?;
    ensure_mcp_credential_is_configured(&entry, kind, &name)?;
    write_mcp_credential(host, &server_id, kind, &name, secret)?;
    let result = reconcile_mcp_runtime(host, &server_id, entry.enabled);
    Ok(McpActivationReport {
        results: vec![result],
        snapshot: extensions_snapshot(host)?,
    })
}

pub fn delete_mcp_server_credential(
    host: &dyn ExtensionsHost,
    server_id: &str,
    kind: McpCredentialKind,
    name: &str,
) -> Result<McpActivationReport, ExtensionsError> {
    let server_id = required_mcp_value("server_id", server_id)?.to_owned();
    let name = normalized_mcp_credential_name(kind, name)?;
    let entry = registered_mcp_entry(host, &server_id)?
        .ok_or_else(|| invalid_input("server_id", "MCP server is not registered"))?;
    ensure_mcp_credential_is_configured(&entry, kind, &name)?;
    delete_mcp_credential(host, &server_id, kind, &name)?;
    let result = reconcile_mcp_runtime(host, &server_id, entry.enabled);
    Ok(McpActivationReport {
        results: vec![result],
        snapshot: extensions_snapshot(host)?,
    })
}

pub fn activate_registered_mcp_servers(
    host: &dyn ExtensionsHost,
) -> Result<McpActivationReport, ExtensionsError> {
    let registry = lilia_storage::load_mcp_registry(&host.data_paths())?.unwrap_or_default();
    let mut entries = registry
        .servers
        .into_iter()
        .filter(|entry| entry.enabled)
        .collect::<Vec<_>>();
    entries.extend(
        host.loaded_plugins()
            .into_iter()
            .flat_map(|package| package.mcp_servers)
            .filter(|entry| entry.enabled),
    );
    let results = entries
        .into_iter()
        .map(|entry| activate_mcp_entry(host, entry))
        .collect();
    Ok(McpActivationReport {
        results,
        snapshot: extensions_snapshot(host)?,
    })
}

pub fn read_mcp_resource(
    host: &dyn ExtensionsHost,
    server_id: &str,
    uri: &str,
) -> Result<McpResourceReadView, ExtensionsError> {
    let server_id = required_mcp_value("server_id", server_id)?;
    let uri = required_mcp_value("uri", uri)?;
    let (result_uri, summary, content) = host.read_mcp_resource_raw(server_id, uri)?;
    Ok(McpResourceReadView {
        server_id: server_id.to_owned(),
        uri: result_uri,
        summary,
        contents: mcp_resource_contents(&content)?,
    })
}

pub fn get_mcp_prompt(
    host: &dyn ExtensionsHost,
    namespaced_name: &str,
    arguments: Value,
) -> Result<McpPromptGetView, ExtensionsError> {
    let namespaced_name = required_mcp_value("namespaced_name", namespaced_name)?;
    if !arguments.is_object() {
        return Err(invalid_input(
            "arguments",
            "MCP prompt arguments must be a JSON object",
        ));
    }
    let prompt = host.get_mcp_prompt_raw(namespaced_name, arguments)?;
    Ok(McpPromptGetView {
        namespaced_name: namespaced_name.to_owned(),
        description: prompt.description,
        fragments: prompt.fragments,
    })
}

pub fn delete_mcp_credentials_for_entries(
    host: &dyn ExtensionsHost,
    entries: &[AgentkitMcpRegistryEntry],
) -> Result<(), ExtensionsError> {
    let backups = entries
        .iter()
        .map(|entry| read_mcp_credentials(host, entry))
        .collect::<Result<Vec<_>, _>>()?;
    for (index, (entry, backup)) in entries.iter().zip(&backups).enumerate() {
        if let Err(error) = delete_mcp_credentials(host, entry, backup) {
            for (previous, previous_backup) in entries[..index].iter().zip(&backups[..index]) {
                restore_mcp_credentials(host, previous, previous_backup)?;
            }
            return Err(error);
        }
    }
    Ok(())
}

fn persist_and_reload_skills(
    host: &dyn ExtensionsHost,
    paths: &LiliaDataPaths,
    registry: &AgentkitSkillsRegistry,
) -> Result<(), ExtensionsError> {
    lilia_storage::save_skills_registry(paths, registry)?;
    host.reload_registered_skills()
}

fn mcp_server_view(
    host: &dyn ExtensionsHost,
    catalog: &McpCatalog,
    active_by_id: &BTreeMap<&str, &McpServerStatus>,
    entry: &AgentkitMcpRegistryEntry,
    editable: bool,
) -> Result<McpServerView, ExtensionsError> {
    let status = active_by_id.get(entry.server_id.as_str()).copied();
    Ok(McpServerView {
        server_id: entry.server_id.clone(),
        source: entry.source.clone(),
        transport: entry.transport.clone(),
        location: entry.command.clone().or_else(|| entry.url.clone()),
        registered: true,
        editable,
        enabled: entry.enabled,
        command: entry.command.clone(),
        args: entry.args.clone(),
        url: entry.url.clone(),
        registered_from: Some(entry.registered_from.clone()),
        runtime_state: status.map(|status| mcp_state_key(&status.state).to_owned()),
        tool_count: status.map(|status| status.tool_count).unwrap_or_default(),
        resource_count: status
            .map(|status| status.resource_count)
            .unwrap_or_default(),
        prompt_count: status.map(|status| status.prompt_count).unwrap_or_default(),
        restart_count: status
            .map(|status| status.restart_count)
            .unwrap_or_default(),
        last_error: status.and_then(|status| status.last_error.clone()),
        tools: mcp_tools(catalog, &entry.server_id),
        resources: mcp_resources(catalog, &entry.server_id),
        prompts: mcp_prompts(catalog, &entry.server_id),
        credentials: mcp_credential_views(host, entry)?,
    })
}

fn reconcile_mcp_runtime(
    host: &dyn ExtensionsHost,
    server_id: &str,
    enabled: bool,
) -> McpActivationResult {
    if !enabled {
        return disconnect_mcp_runtime(host, server_id);
    }
    registered_mcp_entry(host, server_id)
        .ok()
        .flatten()
        .map_or_else(
            || mcp_activation_error(server_id, "MCP server is not registered"),
            |entry| activate_mcp_entry(host, entry),
        )
}

fn registered_mcp_entry(
    host: &dyn ExtensionsHost,
    server_id: &str,
) -> Result<Option<AgentkitMcpRegistryEntry>, ExtensionsError> {
    let entry = lilia_storage::load_mcp_registry(&host.data_paths())?
        .into_iter()
        .flat_map(|registry| registry.servers)
        .chain(
            host.loaded_plugins()
                .into_iter()
                .flat_map(|package| package.mcp_servers),
        )
        .find(|entry| entry.server_id == server_id);
    Ok(entry)
}

pub fn activate_mcp_entry(
    host: &dyn ExtensionsHost,
    entry: AgentkitMcpRegistryEntry,
) -> McpActivationResult {
    let server_id = entry.server_id.clone();
    if let Err(error) = host.disconnect_mcp(&server_id) {
        return mcp_activation_error(&server_id, error);
    }
    let credentials = match resolve_mcp_credentials(host, &entry) {
        Ok(credentials) => credentials,
        Err(error) => return mcp_activation_error(&server_id, error.to_string()),
    };
    host.activate_registered_mcp(entry, credentials.env, credentials.headers)
}

fn disconnect_mcp_runtime(host: &dyn ExtensionsHost, server_id: &str) -> McpActivationResult {
    McpActivationResult {
        server_id: server_id.to_owned(),
        runtime_state: None,
        tool_count: 0,
        resource_count: 0,
        prompt_count: 0,
        error: host.disconnect_mcp(server_id).err(),
    }
}

fn mcp_credential_views(
    host: &dyn ExtensionsHost,
    entry: &AgentkitMcpRegistryEntry,
) -> Result<Vec<McpCredentialView>, ExtensionsError> {
    let mut views = Vec::new();
    for (kind, names) in [
        (McpCredentialKind::Environment, &entry.env_secret_names),
        (McpCredentialKind::Header, &entry.header_secret_names),
    ] {
        for name in names {
            views.push(McpCredentialView {
                kind,
                name: name.clone(),
                present: read_mcp_credential(host, &entry.server_id, kind, name)?.is_some(),
            });
        }
    }
    Ok(views)
}

fn resolve_mcp_credentials(
    host: &dyn ExtensionsHost,
    entry: &AgentkitMcpRegistryEntry,
) -> Result<ResolvedMcpCredentials, ExtensionsError> {
    let mut resolved = ResolvedMcpCredentials::default();
    for (kind, names) in [
        (McpCredentialKind::Environment, &entry.env_secret_names),
        (McpCredentialKind::Header, &entry.header_secret_names),
    ] {
        for name in names {
            let secret =
                read_mcp_credential(host, &entry.server_id, kind, name)?.ok_or_else(|| {
                    invalid_input(
                        "credential",
                        format!("MCP credential `{name}` is not configured in OS Keyring"),
                    )
                })?;
            let value = String::from_utf8(secret).map_err(|_| {
                invalid_input("credential", "MCP credential must contain UTF-8 text")
            })?;
            match kind {
                McpCredentialKind::Environment => resolved.env.push((name.clone(), value)),
                McpCredentialKind::Header => resolved.headers.push((name.clone(), value)),
            }
        }
    }
    Ok(resolved)
}

fn read_mcp_credentials(
    host: &dyn ExtensionsHost,
    entry: &AgentkitMcpRegistryEntry,
) -> Result<Vec<(McpCredentialKind, String, Option<Vec<u8>>)>, ExtensionsError> {
    let mut values = Vec::new();
    for (kind, names) in [
        (McpCredentialKind::Environment, &entry.env_secret_names),
        (McpCredentialKind::Header, &entry.header_secret_names),
    ] {
        for name in names {
            values.push((
                kind,
                name.clone(),
                read_mcp_credential(host, &entry.server_id, kind, name)?,
            ));
        }
    }
    Ok(values)
}

fn delete_mcp_credentials(
    host: &dyn ExtensionsHost,
    entry: &AgentkitMcpRegistryEntry,
    backup: &[(McpCredentialKind, String, Option<Vec<u8>>)],
) -> Result<(), ExtensionsError> {
    for (index, (kind, name, _)) in backup.iter().enumerate() {
        if let Err(error) = delete_mcp_credential(host, &entry.server_id, *kind, name) {
            restore_mcp_credentials(host, entry, &backup[..index])?;
            return Err(error);
        }
    }
    Ok(())
}

fn restore_mcp_credentials(
    host: &dyn ExtensionsHost,
    entry: &AgentkitMcpRegistryEntry,
    backup: &[(McpCredentialKind, String, Option<Vec<u8>>)],
) -> Result<(), ExtensionsError> {
    for (kind, name, secret) in backup {
        if let Some(secret) = secret {
            write_mcp_credential(host, &entry.server_id, *kind, name, secret.clone())?;
        }
    }
    Ok(())
}

fn read_mcp_credential(
    host: &dyn ExtensionsHost,
    server_id: &str,
    kind: McpCredentialKind,
    name: &str,
) -> Result<Option<Vec<u8>>, ExtensionsError> {
    host.read_secret(&mcp_credential_key(server_id, kind, name))
}

fn write_mcp_credential(
    host: &dyn ExtensionsHost,
    server_id: &str,
    kind: McpCredentialKind,
    name: &str,
    secret: Vec<u8>,
) -> Result<(), ExtensionsError> {
    host.write_secret(&mcp_credential_key(server_id, kind, name), secret)
}

fn delete_mcp_credential(
    host: &dyn ExtensionsHost,
    server_id: &str,
    kind: McpCredentialKind,
    name: &str,
) -> Result<(), ExtensionsError> {
    host.delete_secret(&mcp_credential_key(server_id, kind, name))
}

pub fn coding_runtime_services(status: &CodingRuntimeFacts) -> Vec<RuntimeServiceView> {
    [
        (
            status.git_service_id.as_str(),
            "Git",
            status.git_same_instance,
        ),
        (
            status.code_index_service_id.as_str(),
            "Code Index",
            status.code_index_same_instance,
        ),
        (
            status.lsp_service_id.as_str(),
            "LSP",
            status.lsp_same_instance,
        ),
        (
            status.mcp_service_id.as_str(),
            "MCP",
            status.mcp_same_instance,
        ),
        (
            status.computer_use_service_id.as_str(),
            "Computer Use",
            status.computer_use_same_instance,
        ),
        (
            status.memory_runner_id.as_str(),
            "Memory Router",
            status.memory_shared_router,
        ),
    ]
    .into_iter()
    .map(
        |(service_id, label, shared_with_agent)| RuntimeServiceView {
            service_id: service_id.to_owned(),
            label: label.to_owned(),
            shared_with_agent,
        },
    )
    .collect()
}

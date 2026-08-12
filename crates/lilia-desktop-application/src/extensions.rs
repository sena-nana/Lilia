use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use lilia_agent_integration::{RegisteredMcpActivation, SharedCodingServicesStatus};
use lilia_storage::{
    AgentkitMcpRegistry, AgentkitMcpRegistryEntry, AgentkitSkillPackageRef, AgentkitSkillsRegistry,
};
use mutsuki_agent_contracts::{
    McpCatalog, McpServerState, McpServerStatus, SkillDiscoverResult, SkillLoadResult,
    SkillSourceKind,
};
use serde::{Deserialize, Serialize};

use crate::{
    DesktopApplication, DesktopApplicationError, DesktopCredentialAction, DesktopHostAction,
    DesktopHostResult, DesktopPluginPackageView, DesktopSecret,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSkillPackageView {
    pub skill_id: String,
    pub path: String,
    pub registered_from: String,
    pub scope: String,
    pub description: String,
    pub enabled: bool,
    pub editable: bool,
    pub runtime_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSkillScope {
    User,
    Project,
}

impl DesktopSkillScope {
    const fn as_registry(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSkillCreate {
    pub expected_registry_revision: u64,
    pub scope: DesktopSkillScope,
    pub project_cwd: Option<String>,
    pub skill_id: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpServerView {
    pub server_id: String,
    pub source: String,
    pub transport: String,
    pub location: Option<String>,
    pub registered: bool,
    pub editable: bool,
    pub enabled: bool,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub registered_from: Option<String>,
    pub runtime_state: Option<String>,
    pub tool_count: usize,
    pub resource_count: usize,
    pub prompt_count: usize,
    pub restart_count: u64,
    pub last_error: Option<String>,
    pub tools: Vec<DesktopMcpToolView>,
    pub resources: Vec<DesktopMcpResourceView>,
    pub prompts: Vec<DesktopMcpPromptView>,
    pub credentials: Vec<DesktopMcpCredentialView>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopMcpCredentialKind {
    Environment,
    Header,
}

impl DesktopMcpCredentialKind {
    pub const fn key_segment(self) -> &'static str {
        match self {
            Self::Environment => "env",
            Self::Header => "header",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpCredentialView {
    pub kind: DesktopMcpCredentialKind,
    pub name: String,
    pub present: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpToolView {
    pub name: String,
    pub namespaced_name: String,
    pub description: String,
    pub read_only: Option<bool>,
    pub destructive: Option<bool>,
    pub idempotent: Option<bool>,
    pub open_world: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpResourceView {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpPromptArgumentView {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpPromptView {
    pub name: String,
    pub namespaced_name: String,
    pub description: Option<String>,
    pub arguments: Vec<DesktopMcpPromptArgumentView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpResourceContentView {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: Option<String>,
    pub encoded_blob_length: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpResourceReadView {
    pub server_id: String,
    pub uri: String,
    pub summary: String,
    pub contents: Vec<DesktopMcpResourceContentView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpPromptFragmentView {
    pub fragment_id: String,
    pub content: String,
    pub priority: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpPromptGetView {
    pub namespaced_name: String,
    pub description: Option<String>,
    pub fragments: Vec<DesktopMcpPromptFragmentView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopRuntimeServiceView {
    pub service_id: String,
    pub label: String,
    pub shared_with_agent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopExtensionsSnapshot {
    pub data_source: String,
    pub shared_identity_ok: bool,
    pub skills_registry_path: String,
    pub skills_registry_revision: u64,
    pub mcp_registry_path: String,
    pub mcp_registry_revision: u64,
    pub plugins_registry_path: String,
    pub plugins_registry_revision: u64,
    pub skill_roots: Vec<String>,
    pub skills: Vec<DesktopSkillPackageView>,
    pub plugins: Vec<DesktopPluginPackageView>,
    pub mcp_servers: Vec<DesktopMcpServerView>,
    pub runtime_services: Vec<DesktopRuntimeServiceView>,
    pub legacy_plugin_manager_available: bool,
    pub legacy_hooks_manager_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopMcpTransport {
    Stdio,
    StreamableHttp,
    Sse,
}

impl DesktopMcpTransport {
    pub const fn as_registry(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::StreamableHttp => "streamable_http",
            Self::Sse => "sse",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpServerUpsert {
    pub expected_registry_revision: u64,
    pub server_id: String,
    pub transport: DesktopMcpTransport,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub env_secret_names: Vec<String>,
    #[serde(default)]
    pub header_secret_names: Vec<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpActivationResult {
    pub server_id: String,
    pub runtime_state: Option<String>,
    pub tool_count: usize,
    pub resource_count: usize,
    pub prompt_count: usize,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpActivationReport {
    pub results: Vec<DesktopMcpActivationResult>,
    pub snapshot: DesktopExtensionsSnapshot,
}

impl DesktopApplication {
    pub fn extensions_snapshot(
        &self,
    ) -> Result<DesktopExtensionsSnapshot, DesktopApplicationError> {
        let paths = self.config().data_paths();
        let skill_registry = lilia_storage::load_skills_registry(&paths)?;
        let mcp_registry = lilia_storage::load_mcp_registry(&paths)?;
        let (plugins_registry_revision, plugins_registry_path, plugins) = self.plugin_packages()?;
        let loaded_plugins = self.loaded_plugin_packages();
        let plugin_mcp_entries = loaded_plugins
            .iter()
            .flat_map(|package| package.mcp_servers.iter().cloned())
            .collect::<Vec<_>>();
        let runtime = self.authority().shared_runtime();
        let coding_status = runtime
            .inner()
            .shared_coding_services_status()
            .map_err(extension_runtime_error)?;
        let active_servers = serde_json::from_value::<Vec<McpServerStatus>>(
            runtime
                .inner()
                .shared_mcp_list_servers()
                .map_err(extension_runtime_error)?,
        )
        .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        let catalog = serde_json::from_value::<McpCatalog>(
            runtime
                .inner()
                .shared_mcp_catalog(None)
                .map_err(extension_runtime_error)?,
        )
        .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        let skill_catalog = serde_json::from_value::<SkillDiscoverResult>(
            runtime
                .inner()
                .shared_skill_catalog()
                .map_err(extension_runtime_error)?,
        )
        .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
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
                    .map(|package| DesktopSkillPackageView {
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
            let loaded = serde_json::from_value::<SkillLoadResult>(
                runtime
                    .inner()
                    .shared_skill_load(&entry.skill_id)
                    .map_err(extension_runtime_error)?,
            )
            .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
            let source_path = PathBuf::from(&loaded.descriptor.provenance.source_path);
            let owner = loaded_plugins
                .iter()
                .find(|package| source_path.starts_with(&package.root))
                .and_then(|package| package.root.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");
            skills.push(DesktopSkillPackageView {
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
            .map(
                |entry| -> Result<DesktopMcpServerView, DesktopApplicationError> {
                    let status = active_by_id.get(entry.server_id.as_str()).copied();
                    Ok(DesktopMcpServerView {
                        server_id: entry.server_id.clone(),
                        source: entry.source.clone(),
                        transport: entry.transport.clone(),
                        location: entry.command.clone().or_else(|| entry.url.clone()),
                        registered: true,
                        editable: true,
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
                        tools: mcp_tools(&catalog, &entry.server_id),
                        resources: mcp_resources(&catalog, &entry.server_id),
                        prompts: mcp_prompts(&catalog, &entry.server_id),
                        credentials: self.mcp_credential_views(entry)?,
                    })
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        mcp_servers.extend(
            plugin_mcp_entries
                .iter()
                .map(
                    |entry| -> Result<DesktopMcpServerView, DesktopApplicationError> {
                        let status = active_by_id.get(entry.server_id.as_str()).copied();
                        Ok(DesktopMcpServerView {
                            server_id: entry.server_id.clone(),
                            source: entry.source.clone(),
                            transport: entry.transport.clone(),
                            location: entry.command.clone().or_else(|| entry.url.clone()),
                            registered: true,
                            editable: false,
                            enabled: entry.enabled,
                            command: entry.command.clone(),
                            args: entry.args.clone(),
                            url: entry.url.clone(),
                            registered_from: Some(entry.registered_from.clone()),
                            runtime_state: status
                                .map(|status| mcp_state_key(&status.state).to_owned()),
                            tool_count: status.map(|status| status.tool_count).unwrap_or_default(),
                            resource_count: status
                                .map(|status| status.resource_count)
                                .unwrap_or_default(),
                            prompt_count: status
                                .map(|status| status.prompt_count)
                                .unwrap_or_default(),
                            restart_count: status
                                .map(|status| status.restart_count)
                                .unwrap_or_default(),
                            last_error: status.and_then(|status| status.last_error.clone()),
                            tools: mcp_tools(&catalog, &entry.server_id),
                            resources: mcp_resources(&catalog, &entry.server_id),
                            prompts: mcp_prompts(&catalog, &entry.server_id),
                            credentials: self.mcp_credential_views(entry)?,
                        })
                    },
                )
                .collect::<Result<Vec<_>, _>>()?,
        );
        mcp_servers.extend(
            active_servers
                .iter()
                .filter(|status| !registered_ids.contains(status.server_id.as_str()))
                .map(|status| DesktopMcpServerView {
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

        Ok(DesktopExtensionsSnapshot {
            data_source: coding_status.data_source.to_owned(),
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
            runtime_services: runtime_services(&coding_status),
            legacy_plugin_manager_available: true,
            legacy_hooks_manager_available: false,
        })
    }

    pub fn create_skill_package(
        &self,
        input: DesktopSkillCreate,
    ) -> Result<DesktopExtensionsSnapshot, DesktopApplicationError> {
        let skill_id = normalized_skill_id(&input.skill_id)?;
        let description = normalized_skill_description(&input.description)?;
        if input.scope == DesktopSkillScope::Project {
            return Err(invalid_input(
                "scope",
                "project Skills require task-scoped runtime catalogs and are not globally mutable",
            ));
        }
        let root = managed_skill_root(
            self.config().data_paths().home(),
            input.scope,
            input.project_cwd.as_deref(),
        )?;
        let package_path = root.join(&skill_id);
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let paths = self.config().data_paths();
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
        if let Err(error) = self.persist_and_reload_skills(&registry) {
            let _ = lilia_storage::save_skills_registry(&paths, &previous);
            let _ = self.reload_registered_skills();
            let _ = fs::remove_dir_all(&package_path);
            return Err(error);
        }
        self.extensions_snapshot()
    }

    pub fn set_skill_package_enabled(
        &self,
        skill_id: &str,
        enabled: bool,
        expected_registry_revision: u64,
    ) -> Result<DesktopExtensionsSnapshot, DesktopApplicationError> {
        let skill_id = normalized_skill_id(skill_id)?;
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let paths = self.config().data_paths();
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
            return self.extensions_snapshot();
        }
        package.enabled = enabled;
        bump_skills_registry_revision(&mut registry)?;
        if let Err(error) = self.persist_and_reload_skills(&registry) {
            let _ = lilia_storage::save_skills_registry(&paths, &previous);
            let _ = self.reload_registered_skills();
            return Err(error);
        }
        self.extensions_snapshot()
    }

    pub fn delete_skill_package(
        &self,
        skill_id: &str,
        expected_registry_revision: u64,
    ) -> Result<DesktopExtensionsSnapshot, DesktopApplicationError> {
        let skill_id = normalized_skill_id(skill_id)?;
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let paths = self.config().data_paths();
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
        let root = managed_skill_root(paths.home(), DesktopSkillScope::User, None)?;
        let package_path = verified_managed_skill_path(&root, &package.path, &skill_id)?;
        let staging = root.join(format!(".{skill_id}.deleting-{}", uuid::Uuid::new_v4()));
        fs::rename(&package_path, &staging)
            .map_err(|error| skill_io_error("stage Skill deletion", error))?;
        let previous = registry.clone();
        registry.packages.remove(index);
        bump_skills_registry_revision(&mut registry)?;
        if let Err(error) = self.persist_and_reload_skills(&registry) {
            let _ = lilia_storage::save_skills_registry(&paths, &previous);
            let _ = self.reload_registered_skills();
            let _ = fs::rename(&staging, &package_path);
            return Err(error);
        }
        fs::remove_dir_all(&staging)
            .map_err(|error| skill_io_error("remove deleted Skill directory", error))?;
        self.extensions_snapshot()
    }

    fn persist_and_reload_skills(
        &self,
        registry: &AgentkitSkillsRegistry,
    ) -> Result<(), DesktopApplicationError> {
        lilia_storage::save_skills_registry(&self.config().data_paths(), registry)?;
        self.reload_registered_skills()
    }

    pub(crate) fn reload_registered_skills(&self) -> Result<(), DesktopApplicationError> {
        let plugin_skill_paths = self
            .loaded_plugin_packages()
            .into_iter()
            .flat_map(|package| package.skill_paths)
            .collect();
        self.authority()
            .shared_runtime()
            .inner()
            .apply_registered_skill_packages_with_extra_roots(
                &self.config().data_paths(),
                plugin_skill_paths,
            )
            .map(|_| ())
            .map_err(extension_runtime_error)
    }

    pub(crate) fn reload_extension_contributions(&self) -> Result<(), DesktopApplicationError> {
        self.reload_registered_skills()?;
        let runtime = self.authority().shared_runtime();
        let active_servers = serde_json::from_value::<Vec<McpServerStatus>>(
            runtime
                .inner()
                .shared_mcp_list_servers()
                .map_err(extension_runtime_error)?,
        )
        .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        for server in active_servers
            .iter()
            .filter(|server| server.server_id.starts_with("plugin."))
        {
            runtime
                .inner()
                .disconnect_shared_mcp_server(&server.server_id)
                .map_err(extension_runtime_error)?;
        }
        for server in self
            .loaded_plugin_packages()
            .into_iter()
            .flat_map(|package| package.mcp_servers)
            .filter(|server| server.enabled)
        {
            let _ = self.activate_mcp_entry(server);
        }
        Ok(())
    }

    pub fn upsert_mcp_server(
        &self,
        input: DesktopMcpServerUpsert,
    ) -> Result<DesktopMcpActivationReport, DesktopApplicationError> {
        let normalized = NormalizedMcpServer::new(input)?;
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let paths = self.config().data_paths();
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
            .map(|removed| self.read_mcp_credentials(removed))
            .transpose()?
            .unwrap_or_default();
        if let Some(removed) = &removed_credentials {
            self.delete_mcp_credentials(removed, &removed_credential_backup)?;
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
                self.restore_mcp_credentials(removed, &removed_credential_backup)?;
            }
            return Err(error);
        }
        if let Err(error) = lilia_storage::save_mcp_registry(&paths, &registry) {
            if let Some(removed) = &removed_credentials {
                self.restore_mcp_credentials(removed, &removed_credential_backup)?;
            }
            return Err(error.into());
        }
        let result = self.reconcile_mcp_runtime(&normalized.server_id, normalized.enabled);
        Ok(DesktopMcpActivationReport {
            results: vec![result],
            snapshot: self.extensions_snapshot()?,
        })
    }

    pub fn set_mcp_server_enabled(
        &self,
        server_id: &str,
        enabled: bool,
        expected_registry_revision: u64,
    ) -> Result<DesktopMcpActivationReport, DesktopApplicationError> {
        let server_id = normalized_server_id(server_id)?;
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let paths = self.config().data_paths();
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
        let result = self.reconcile_mcp_runtime(&server_id, enabled);
        Ok(DesktopMcpActivationReport {
            results: vec![result],
            snapshot: self.extensions_snapshot()?,
        })
    }

    pub fn delete_mcp_server(
        &self,
        server_id: &str,
        expected_registry_revision: u64,
    ) -> Result<DesktopMcpActivationReport, DesktopApplicationError> {
        let server_id = normalized_server_id(server_id)?;
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let paths = self.config().data_paths();
        let mut registry = lilia_storage::load_mcp_registry(&paths)?.unwrap_or_default();
        ensure_registry_revision(registry.revision, expected_registry_revision)?;
        let entry = registry
            .servers
            .iter()
            .find(|entry| entry.server_id == server_id)
            .cloned()
            .ok_or_else(|| invalid_input("server_id", "MCP server is not registered"))?;
        let credential_backup = self.read_mcp_credentials(&entry)?;
        self.delete_mcp_credentials(&entry, &credential_backup)?;
        let before = registry.servers.len();
        registry
            .servers
            .retain(|entry| entry.server_id != server_id);
        if registry.servers.len() == before {
            return Err(invalid_input("server_id", "MCP server is not registered"));
        }
        if let Err(error) = bump_registry_revision(&mut registry) {
            self.restore_mcp_credentials(&entry, &credential_backup)?;
            return Err(error);
        }
        if let Err(error) = lilia_storage::save_mcp_registry(&paths, &registry) {
            self.restore_mcp_credentials(&entry, &credential_backup)?;
            return Err(error.into());
        }
        let result = self.disconnect_mcp_runtime(&server_id);
        Ok(DesktopMcpActivationReport {
            results: vec![result],
            snapshot: self.extensions_snapshot()?,
        })
    }

    pub fn set_mcp_server_credential(
        &self,
        server_id: &str,
        kind: DesktopMcpCredentialKind,
        name: &str,
        secret: DesktopSecret,
    ) -> Result<DesktopMcpActivationReport, DesktopApplicationError> {
        let server_id = required_mcp_value("server_id", server_id)?.to_owned();
        let name = normalized_mcp_credential_name(kind, name)?;
        validate_mcp_secret(kind, &secret)?;
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let entry = self
            .registered_mcp_entry(&server_id)?
            .ok_or_else(|| invalid_input("server_id", "MCP server is not registered"))?;
        ensure_mcp_credential_is_configured(&entry, kind, &name)?;
        self.write_mcp_credential(&server_id, kind, &name, secret)?;
        let result = self.reconcile_mcp_runtime(&server_id, entry.enabled);
        Ok(DesktopMcpActivationReport {
            results: vec![result],
            snapshot: self.extensions_snapshot()?,
        })
    }

    pub fn delete_mcp_server_credential(
        &self,
        server_id: &str,
        kind: DesktopMcpCredentialKind,
        name: &str,
    ) -> Result<DesktopMcpActivationReport, DesktopApplicationError> {
        let server_id = required_mcp_value("server_id", server_id)?.to_owned();
        let name = normalized_mcp_credential_name(kind, name)?;
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let entry = self
            .registered_mcp_entry(&server_id)?
            .ok_or_else(|| invalid_input("server_id", "MCP server is not registered"))?;
        ensure_mcp_credential_is_configured(&entry, kind, &name)?;
        self.delete_mcp_credential(&server_id, kind, &name)?;
        let result = self.reconcile_mcp_runtime(&server_id, entry.enabled);
        Ok(DesktopMcpActivationReport {
            results: vec![result],
            snapshot: self.extensions_snapshot()?,
        })
    }

    pub fn activate_registered_mcp_servers(
        &self,
    ) -> Result<DesktopMcpActivationReport, DesktopApplicationError> {
        let registry =
            lilia_storage::load_mcp_registry(&self.config().data_paths())?.unwrap_or_default();
        let mut entries = registry
            .servers
            .into_iter()
            .filter(|entry| entry.enabled)
            .collect::<Vec<_>>();
        entries.extend(
            self.loaded_plugin_packages()
                .into_iter()
                .flat_map(|package| package.mcp_servers)
                .filter(|entry| entry.enabled),
        );
        let results = entries
            .into_iter()
            .map(|entry| self.activate_mcp_entry(entry))
            .collect();
        Ok(DesktopMcpActivationReport {
            results,
            snapshot: self.extensions_snapshot()?,
        })
    }

    pub fn read_mcp_resource(
        &self,
        server_id: &str,
        uri: &str,
    ) -> Result<DesktopMcpResourceReadView, DesktopApplicationError> {
        let server_id = required_mcp_value("server_id", server_id)?;
        let uri = required_mcp_value("uri", uri)?;
        let resource = self
            .authority()
            .shared_runtime()
            .inner()
            .shared_mcp_read_resource(server_id, uri)
            .map_err(extension_runtime_error)?;
        Ok(DesktopMcpResourceReadView {
            server_id: server_id.to_owned(),
            uri: resource.result.uri,
            summary: resource.result.summary,
            contents: mcp_resource_contents(&resource.content)?,
        })
    }

    pub fn get_mcp_prompt(
        &self,
        namespaced_name: &str,
        arguments: serde_json::Value,
    ) -> Result<DesktopMcpPromptGetView, DesktopApplicationError> {
        let namespaced_name = required_mcp_value("namespaced_name", namespaced_name)?;
        if !arguments.is_object() {
            return Err(invalid_input(
                "arguments",
                "MCP prompt arguments must be a JSON object",
            ));
        }
        let prompt = self
            .authority()
            .shared_runtime()
            .inner()
            .shared_mcp_get_prompt(namespaced_name, arguments)
            .map_err(extension_runtime_error)?;
        Ok(DesktopMcpPromptGetView {
            namespaced_name: namespaced_name.to_owned(),
            description: prompt.prompt.description,
            fragments: prompt
                .fragments
                .into_iter()
                .map(|fragment| DesktopMcpPromptFragmentView {
                    fragment_id: fragment.fragment_id,
                    content: fragment.content,
                    priority: fragment.priority,
                })
                .collect(),
        })
    }

    fn reconcile_mcp_runtime(&self, server_id: &str, enabled: bool) -> DesktopMcpActivationResult {
        if !enabled {
            let disconnect_error = self
                .authority()
                .shared_runtime()
                .inner()
                .disconnect_shared_mcp_server(server_id)
                .err()
                .map(|error| error.to_string());
            return DesktopMcpActivationResult {
                server_id: server_id.to_owned(),
                runtime_state: None,
                tool_count: 0,
                resource_count: 0,
                prompt_count: 0,
                error: disconnect_error,
            };
        }
        let entry = self.registered_mcp_entry(server_id).ok().flatten();
        entry.map_or_else(
            || mcp_activation_error(server_id, "MCP server is not registered"),
            |entry| self.activate_mcp_entry(entry),
        )
    }

    fn registered_mcp_entry(
        &self,
        server_id: &str,
    ) -> Result<Option<AgentkitMcpRegistryEntry>, DesktopApplicationError> {
        let entry = lilia_storage::load_mcp_registry(&self.config().data_paths())?
            .into_iter()
            .flat_map(|registry| registry.servers)
            .chain(
                self.loaded_plugin_packages()
                    .into_iter()
                    .flat_map(|package| package.mcp_servers),
            )
            .find(|entry| entry.server_id == server_id);
        Ok(entry)
    }

    fn activate_mcp_entry(&self, entry: AgentkitMcpRegistryEntry) -> DesktopMcpActivationResult {
        let server_id = entry.server_id.clone();
        if let Err(error) = self
            .authority()
            .shared_runtime()
            .inner()
            .disconnect_shared_mcp_server(&server_id)
        {
            return mcp_activation_error(&server_id, error.to_string());
        }
        let credentials = match self.resolve_mcp_credentials(&entry) {
            Ok(credentials) => credentials,
            Err(error) => return mcp_activation_error(&server_id, error.to_string()),
        };
        activation_result(
            self.authority()
                .shared_runtime()
                .inner()
                .activate_mcp_registry_entry(entry, credentials.env, credentials.headers),
        )
    }

    fn mcp_credential_views(
        &self,
        entry: &AgentkitMcpRegistryEntry,
    ) -> Result<Vec<DesktopMcpCredentialView>, DesktopApplicationError> {
        let mut views = Vec::new();
        for (kind, names) in [
            (
                DesktopMcpCredentialKind::Environment,
                &entry.env_secret_names,
            ),
            (DesktopMcpCredentialKind::Header, &entry.header_secret_names),
        ] {
            for name in names {
                views.push(DesktopMcpCredentialView {
                    kind,
                    name: name.clone(),
                    present: self
                        .read_mcp_credential(&entry.server_id, kind, name)?
                        .is_some(),
                });
            }
        }
        Ok(views)
    }

    fn resolve_mcp_credentials(
        &self,
        entry: &AgentkitMcpRegistryEntry,
    ) -> Result<ResolvedMcpCredentials, DesktopApplicationError> {
        let mut resolved = ResolvedMcpCredentials::default();
        for (kind, names) in [
            (
                DesktopMcpCredentialKind::Environment,
                &entry.env_secret_names,
            ),
            (DesktopMcpCredentialKind::Header, &entry.header_secret_names),
        ] {
            for name in names {
                let secret = self
                    .read_mcp_credential(&entry.server_id, kind, name)?
                    .ok_or_else(|| {
                        invalid_input(
                            "credential",
                            format!("MCP credential `{name}` is not configured in OS Keyring"),
                        )
                    })?;
                let value = String::from_utf8(secret.into_inner()).map_err(|_| {
                    invalid_input("credential", "MCP credential must contain UTF-8 text")
                })?;
                match kind {
                    DesktopMcpCredentialKind::Environment => {
                        resolved.env.push((name.clone(), value))
                    }
                    DesktopMcpCredentialKind::Header => {
                        resolved.headers.push((name.clone(), value))
                    }
                }
            }
        }
        Ok(resolved)
    }

    fn read_mcp_credentials(
        &self,
        entry: &AgentkitMcpRegistryEntry,
    ) -> Result<
        Vec<(DesktopMcpCredentialKind, String, Option<DesktopSecret>)>,
        DesktopApplicationError,
    > {
        let mut values = Vec::new();
        for (kind, names) in [
            (
                DesktopMcpCredentialKind::Environment,
                &entry.env_secret_names,
            ),
            (DesktopMcpCredentialKind::Header, &entry.header_secret_names),
        ] {
            for name in names {
                values.push((
                    kind,
                    name.clone(),
                    self.read_mcp_credential(&entry.server_id, kind, name)?,
                ));
            }
        }
        Ok(values)
    }

    fn delete_mcp_credentials(
        &self,
        entry: &AgentkitMcpRegistryEntry,
        backup: &[(DesktopMcpCredentialKind, String, Option<DesktopSecret>)],
    ) -> Result<(), DesktopApplicationError> {
        for (index, (kind, name, _)) in backup.iter().enumerate() {
            if let Err(error) = self.delete_mcp_credential(&entry.server_id, *kind, name) {
                self.restore_mcp_credentials(entry, &backup[..index])?;
                return Err(error);
            }
        }
        Ok(())
    }

    pub(crate) fn delete_mcp_credentials_for_entries(
        &self,
        entries: &[AgentkitMcpRegistryEntry],
    ) -> Result<(), DesktopApplicationError> {
        let backups = entries
            .iter()
            .map(|entry| self.read_mcp_credentials(entry))
            .collect::<Result<Vec<_>, _>>()?;
        for (index, (entry, backup)) in entries.iter().zip(&backups).enumerate() {
            if let Err(error) = self.delete_mcp_credentials(entry, backup) {
                for (previous, previous_backup) in entries[..index].iter().zip(&backups[..index]) {
                    self.restore_mcp_credentials(previous, previous_backup)?;
                }
                return Err(error);
            }
        }
        Ok(())
    }

    fn restore_mcp_credentials(
        &self,
        entry: &AgentkitMcpRegistryEntry,
        backup: &[(DesktopMcpCredentialKind, String, Option<DesktopSecret>)],
    ) -> Result<(), DesktopApplicationError> {
        for (kind, name, secret) in backup {
            if let Some(secret) = secret {
                self.write_mcp_credential(&entry.server_id, *kind, name, secret.clone())?;
            }
        }
        Ok(())
    }

    fn read_mcp_credential(
        &self,
        server_id: &str,
        kind: DesktopMcpCredentialKind,
        name: &str,
    ) -> Result<Option<DesktopSecret>, DesktopApplicationError> {
        match self.execute_host(DesktopHostAction::Credential(
            DesktopCredentialAction::Read {
                key: mcp_credential_key(server_id, kind, name),
            },
        ))? {
            DesktopHostResult::Credential(secret) => Ok(secret),
            _ => Err(DesktopApplicationError::StateUnavailable(
                "MCP credential host result",
            )),
        }
    }

    fn write_mcp_credential(
        &self,
        server_id: &str,
        kind: DesktopMcpCredentialKind,
        name: &str,
        secret: DesktopSecret,
    ) -> Result<(), DesktopApplicationError> {
        match self.execute_host(DesktopHostAction::Credential(
            DesktopCredentialAction::Write {
                key: mcp_credential_key(server_id, kind, name),
                secret,
            },
        ))? {
            DesktopHostResult::Completed => Ok(()),
            _ => Err(DesktopApplicationError::StateUnavailable(
                "MCP credential host result",
            )),
        }
    }

    fn delete_mcp_credential(
        &self,
        server_id: &str,
        kind: DesktopMcpCredentialKind,
        name: &str,
    ) -> Result<(), DesktopApplicationError> {
        match self.execute_host(DesktopHostAction::Credential(
            DesktopCredentialAction::Delete {
                key: mcp_credential_key(server_id, kind, name),
            },
        ))? {
            DesktopHostResult::Completed => Ok(()),
            _ => Err(DesktopApplicationError::StateUnavailable(
                "MCP credential host result",
            )),
        }
    }

    fn disconnect_mcp_runtime(&self, server_id: &str) -> DesktopMcpActivationResult {
        DesktopMcpActivationResult {
            server_id: server_id.to_owned(),
            runtime_state: None,
            tool_count: 0,
            resource_count: 0,
            prompt_count: 0,
            error: self
                .authority()
                .shared_runtime()
                .inner()
                .disconnect_shared_mcp_server(server_id)
                .err()
                .map(|error| error.to_string()),
        }
    }
}

fn mcp_tools(catalog: &McpCatalog, server_id: &str) -> Vec<DesktopMcpToolView> {
    catalog
        .tools
        .iter()
        .filter(|tool| tool.server_id == server_id)
        .map(|tool| DesktopMcpToolView {
            name: tool.name.clone(),
            namespaced_name: tool.namespaced_name.clone(),
            description: tool.description.clone(),
            read_only: tool.annotations.read_only_hint,
            destructive: tool.annotations.destructive_hint,
            idempotent: tool.annotations.idempotent_hint,
            open_world: tool.annotations.open_world_hint,
        })
        .collect()
}

fn mcp_resources(catalog: &McpCatalog, server_id: &str) -> Vec<DesktopMcpResourceView> {
    catalog
        .resources
        .iter()
        .filter(|resource| resource.server_id == server_id)
        .map(|resource| DesktopMcpResourceView {
            uri: resource.uri.clone(),
            name: resource.name.clone(),
            description: resource.description.clone(),
            mime_type: resource.mime_type.clone(),
        })
        .collect()
}

fn mcp_prompts(catalog: &McpCatalog, server_id: &str) -> Vec<DesktopMcpPromptView> {
    catalog
        .prompts
        .iter()
        .filter(|prompt| prompt.server_id == server_id)
        .map(|prompt| DesktopMcpPromptView {
            name: prompt.name.clone(),
            namespaced_name: prompt.namespaced_name.clone(),
            description: prompt.description.clone(),
            arguments: prompt
                .arguments
                .iter()
                .map(|argument| DesktopMcpPromptArgumentView {
                    name: argument.name.clone(),
                    description: argument.description.clone(),
                    required: argument.required,
                })
                .collect(),
        })
        .collect()
}

fn mcp_resource_contents(
    content: &serde_json::Value,
) -> Result<Vec<DesktopMcpResourceContentView>, DesktopApplicationError> {
    let contents = content
        .get("contents")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid_input("resource", "MCP resource response has no contents"))?;
    contents
        .iter()
        .map(|content| {
            let uri = content
                .get("uri")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| invalid_input("resource", "MCP resource content has no URI"))?;
            Ok(DesktopMcpResourceContentView {
                uri: uri.to_owned(),
                mime_type: content
                    .get("mimeType")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                text: content
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                encoded_blob_length: content
                    .get("blob")
                    .and_then(serde_json::Value::as_str)
                    .map(str::len),
            })
        })
        .collect()
}

fn required_mcp_value<'a>(
    field: &'static str,
    value: &'a str,
) -> Result<&'a str, DesktopApplicationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid_input(field, format!("MCP {field} is required")));
    }
    Ok(value)
}

#[derive(Default)]
struct ResolvedMcpCredentials {
    env: Vec<(String, String)>,
    headers: Vec<(String, String)>,
}

fn ensure_mcp_credential_is_configured(
    entry: &AgentkitMcpRegistryEntry,
    kind: DesktopMcpCredentialKind,
    name: &str,
) -> Result<(), DesktopApplicationError> {
    let configured = match kind {
        DesktopMcpCredentialKind::Environment => entry
            .env_secret_names
            .iter()
            .any(|configured| configured == name),
        DesktopMcpCredentialKind::Header => entry
            .header_secret_names
            .iter()
            .any(|configured| configured.eq_ignore_ascii_case(name)),
    };
    if configured {
        Ok(())
    } else {
        Err(invalid_input(
            "credential",
            "MCP credential name is not registered for this server",
        ))
    }
}

fn removed_mcp_credentials(
    previous: &AgentkitMcpRegistryEntry,
    next: &AgentkitMcpRegistryEntry,
) -> AgentkitMcpRegistryEntry {
    let mut removed = previous.clone();
    removed.env_secret_names.retain(|name| {
        !next
            .env_secret_names
            .iter()
            .any(|next_name| next_name == name)
    });
    removed.header_secret_names.retain(|name| {
        !next
            .header_secret_names
            .iter()
            .any(|next_name| next_name.eq_ignore_ascii_case(name))
    });
    removed
}

fn mcp_credential_key(server_id: &str, kind: DesktopMcpCredentialKind, name: &str) -> String {
    let name = if kind == DesktopMcpCredentialKind::Header {
        name.to_ascii_lowercase()
    } else {
        name.to_owned()
    };
    format!("mcp.server.{server_id}.{}.{}", kind.key_segment(), name)
}

fn validate_mcp_secret(
    kind: DesktopMcpCredentialKind,
    secret: &DesktopSecret,
) -> Result<(), DesktopApplicationError> {
    let bytes = secret.expose();
    if bytes.is_empty() || bytes.len() > 65_536 {
        return Err(invalid_input(
            "credential",
            "MCP credential must contain 1-65536 UTF-8 bytes",
        ));
    }
    let value = std::str::from_utf8(bytes)
        .map_err(|_| invalid_input("credential", "MCP credential must contain UTF-8 text"))?;
    let unsafe_control = match kind {
        DesktopMcpCredentialKind::Environment => value.contains('\0'),
        DesktopMcpCredentialKind::Header => value.chars().any(char::is_control),
    };
    if unsafe_control {
        return Err(invalid_input(
            "credential",
            "MCP credential contains characters unsafe for its transport",
        ));
    }
    Ok(())
}

fn mcp_activation_error(server_id: &str, error: impl Into<String>) -> DesktopMcpActivationResult {
    DesktopMcpActivationResult {
        server_id: server_id.to_owned(),
        runtime_state: None,
        tool_count: 0,
        resource_count: 0,
        prompt_count: 0,
        error: Some(error.into()),
    }
}

struct NormalizedMcpServer {
    expected_registry_revision: u64,
    server_id: String,
    transport: DesktopMcpTransport,
    command: Option<String>,
    args: Vec<String>,
    url: Option<String>,
    env_secret_names: Vec<String>,
    header_secret_names: Vec<String>,
    enabled: bool,
}

impl NormalizedMcpServer {
    fn new(input: DesktopMcpServerUpsert) -> Result<Self, DesktopApplicationError> {
        let server_id = normalized_server_id(&input.server_id)?;
        if input.args.len() > 64 {
            return Err(invalid_input(
                "args",
                "MCP args may contain at most 64 items",
            ));
        }
        let args = input
            .args
            .into_iter()
            .map(|argument| {
                normalized_text("args", argument, 4096)?
                    .ok_or_else(|| invalid_input("args", "MCP args must not be empty"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let command = normalized_text("command", input.command.unwrap_or_default(), 4096)?;
        let url = normalized_text("url", input.url.unwrap_or_default(), 8192)?;
        let env_secret_names = normalized_mcp_credential_names(
            DesktopMcpCredentialKind::Environment,
            input.env_secret_names,
        )?;
        let header_secret_names = normalized_mcp_credential_names(
            DesktopMcpCredentialKind::Header,
            input.header_secret_names,
        )?;
        match input.transport {
            DesktopMcpTransport::Stdio => {
                if command.is_none() {
                    return Err(invalid_input("command", "stdio MCP requires a command"));
                }
                if url.is_some() {
                    return Err(invalid_input("url", "stdio MCP must not define a URL"));
                }
                if !header_secret_names.is_empty() {
                    return Err(invalid_input(
                        "header_secret_names",
                        "stdio MCP must not define HTTP header credentials",
                    ));
                }
            }
            DesktopMcpTransport::StreamableHttp | DesktopMcpTransport::Sse => {
                if command.is_some() || !args.is_empty() {
                    return Err(invalid_input(
                        "command",
                        "HTTP MCP must not define a command or args",
                    ));
                }
                let value = url
                    .as_deref()
                    .ok_or_else(|| invalid_input("url", "HTTP MCP requires a URL"))?;
                validate_mcp_url(value)?;
                if !env_secret_names.is_empty() {
                    return Err(invalid_input(
                        "env_secret_names",
                        "HTTP MCP must not define environment credentials",
                    ));
                }
            }
        }
        Ok(Self {
            expected_registry_revision: input.expected_registry_revision,
            server_id,
            transport: input.transport,
            command,
            args,
            url,
            env_secret_names,
            header_secret_names,
            enabled: input.enabled,
        })
    }

    fn registry_entry(&self) -> AgentkitMcpRegistryEntry {
        AgentkitMcpRegistryEntry {
            server_id: self.server_id.clone(),
            source: "lilia.desktop".to_owned(),
            transport: self.transport.as_registry().to_owned(),
            command: self.command.clone(),
            args: self.args.clone(),
            env_allowlist: Vec::new(),
            env_secret_names: self.env_secret_names.clone(),
            url: self.url.clone(),
            header_secret_names: self.header_secret_names.clone(),
            registered_from: "lilia-native-settings".to_owned(),
            enabled: self.enabled,
        }
    }
}

fn normalized_mcp_credential_names(
    kind: DesktopMcpCredentialKind,
    names: Vec<String>,
) -> Result<Vec<String>, DesktopApplicationError> {
    if names.len() > 32 {
        return Err(invalid_input(
            "credential_names",
            "MCP credentials may contain at most 32 names",
        ));
    }
    let mut normalized = names
        .into_iter()
        .map(|name| normalized_mcp_credential_name(kind, &name))
        .collect::<Result<Vec<_>, _>>()?;
    normalized.sort_by(|left, right| {
        if kind == DesktopMcpCredentialKind::Header {
            left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
        } else {
            left.cmp(right)
        }
    });
    let duplicate = normalized.windows(2).any(|pair| {
        if kind == DesktopMcpCredentialKind::Header {
            pair[0].eq_ignore_ascii_case(&pair[1])
        } else {
            pair[0] == pair[1]
        }
    });
    if duplicate {
        return Err(invalid_input(
            "credential_names",
            "MCP credential names must be unique",
        ));
    }
    Ok(normalized)
}

fn normalized_mcp_credential_name(
    kind: DesktopMcpCredentialKind,
    name: &str,
) -> Result<String, DesktopApplicationError> {
    let name = name.trim();
    let valid = match kind {
        DesktopMcpCredentialKind::Environment => {
            let mut bytes = name.bytes();
            bytes
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        }
        DesktopMcpCredentialKind::Header => {
            !name.is_empty()
                && name.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(
                            byte,
                            b'!' | b'#'
                                | b'$'
                                | b'%'
                                | b'&'
                                | b'\''
                                | b'*'
                                | b'+'
                                | b'-'
                                | b'.'
                                | b'^'
                                | b'_'
                                | b'`'
                                | b'|'
                                | b'~'
                        )
                })
                && !matches!(
                    name.to_ascii_lowercase().as_str(),
                    "connection" | "content-length" | "content-type" | "host" | "transfer-encoding"
                )
        }
    };
    if name.len() > 128 || !valid {
        return Err(invalid_input(
            "credential_names",
            match kind {
                DesktopMcpCredentialKind::Environment => {
                    "environment credential names must be valid ASCII environment variables"
                }
                DesktopMcpCredentialKind::Header => {
                    "header credential names must be safe HTTP field names"
                }
            },
        ));
    }
    Ok(name.to_owned())
}

fn ensure_registry_revision(actual: u64, expected: u64) -> Result<(), DesktopApplicationError> {
    if actual == expected {
        return Ok(());
    }
    Err(invalid_input(
        "expected_registry_revision",
        format!("MCP registry changed: expected revision {expected}, actual {actual}"),
    ))
}

fn ensure_skills_registry_revision(
    actual: u64,
    expected: u64,
) -> Result<(), DesktopApplicationError> {
    if actual == expected {
        return Ok(());
    }
    Err(invalid_input(
        "expected_registry_revision",
        format!("Skills registry changed: expected revision {expected}, actual {actual}"),
    ))
}

fn bump_skills_registry_revision(
    registry: &mut AgentkitSkillsRegistry,
) -> Result<(), DesktopApplicationError> {
    registry.revision =
        registry
            .revision
            .checked_add(1)
            .ok_or(DesktopApplicationError::StateRevisionOverflow(
                "Skills registry",
            ))?;
    Ok(())
}

fn normalized_skill_id(value: &str) -> Result<String, DesktopApplicationError> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value != "."
        && value != "..";
    if !valid {
        return Err(invalid_input(
            "skill_id",
            "Skill ID must use 1-64 ASCII letters, digits, dots, dashes, or underscores",
        ));
    }
    Ok(value.to_owned())
}

fn normalized_skill_description(value: &str) -> Result<String, DesktopApplicationError> {
    let value = value.trim();
    if value.len() > 2_048 || value.chars().any(|character| character == '\0') {
        return Err(invalid_input(
            "description",
            "Skill description must be at most 2048 characters and contain no NUL",
        ));
    }
    Ok(value.to_owned())
}

fn managed_skill_root(
    home: &Path,
    scope: DesktopSkillScope,
    project_cwd: Option<&str>,
) -> Result<PathBuf, DesktopApplicationError> {
    match scope {
        DesktopSkillScope::User => Ok(home.join("skills")),
        DesktopSkillScope::Project => {
            let project_cwd = project_cwd
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    invalid_input("project_cwd", "project Skills require a workspace")
                })?;
            let path = PathBuf::from(project_cwd);
            if !path.is_absolute() || !path.is_dir() {
                return Err(invalid_input(
                    "project_cwd",
                    "project Skill workspace must be an existing absolute directory",
                ));
            }
            Ok(path.join(".lilia").join("skills"))
        }
    }
}

fn write_skill_document(
    package_path: &Path,
    skill_id: &str,
    description: &str,
) -> Result<(), DesktopApplicationError> {
    use std::io::Write;

    let quoted_id = serde_json::to_string(skill_id)
        .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
    let quoted_description = serde_json::to_string(description)
        .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
    let instructions = if description.is_empty() {
        format!("Apply the `{skill_id}` skill to the requested task.")
    } else {
        description.to_owned()
    };
    let document = format!(
        "---\nid: {quoted_id}\nversion: \"0.1.0\"\ntitle: {quoted_id}\nsummary: {quoted_description}\n---\n\n{instructions}\n"
    );
    let path = package_path.join("SKILL.md");
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|error| skill_io_error("create SKILL.md", error))?;
    file.write_all(document.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| skill_io_error("write SKILL.md", error))
}

fn is_managed_skill(package: &AgentkitSkillPackageRef) -> bool {
    package.registered_from == "lilia.desktop.skill-manager" && package.scope == "user"
}

fn verified_managed_skill_path(
    root: &Path,
    package_path: &str,
    skill_id: &str,
) -> Result<PathBuf, DesktopApplicationError> {
    let expected = root.join(skill_id);
    let expected = expected
        .canonicalize()
        .map_err(|error| skill_io_error("resolve managed Skill directory", error))?;
    let registered = PathBuf::from(package_path)
        .canonicalize()
        .map_err(|error| skill_io_error("resolve registered Skill directory", error))?;
    let root = root
        .canonicalize()
        .map_err(|error| skill_io_error("resolve managed Skill root", error))?;
    if registered != expected || registered.parent() != Some(root.as_path()) {
        return Err(invalid_input(
            "skill_id",
            "registered Skill path is outside the managed Skill root",
        ));
    }
    Ok(registered)
}

fn skill_io_error(action: &str, error: impl std::fmt::Display) -> DesktopApplicationError {
    DesktopApplicationError::Agent(format!("{action}: {error}"))
}

fn bump_registry_revision(
    registry: &mut AgentkitMcpRegistry,
) -> Result<(), DesktopApplicationError> {
    registry.revision =
        registry
            .revision
            .checked_add(1)
            .ok_or(DesktopApplicationError::StateRevisionOverflow(
                "extension registry",
            ))?;
    Ok(())
}

fn normalized_server_id(value: &str) -> Result<String, DesktopApplicationError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || value.starts_with("plugin.")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(invalid_input(
            "server_id",
            "MCP server id must use 1-128 ASCII letters, digits, dot, dash, or underscore and cannot use the reserved plugin prefix",
        ));
    }
    Ok(value.to_owned())
}

fn normalized_text(
    field: &'static str,
    value: String,
    max_bytes: usize,
) -> Result<Option<String>, DesktopApplicationError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(invalid_input(
            field,
            format!("value must not contain control characters or exceed {max_bytes} bytes"),
        ));
    }
    Ok(Some(value.to_owned()))
}

fn validate_mcp_url(value: &str) -> Result<(), DesktopApplicationError> {
    let url = reqwest::Url::parse(value).map_err(|_| invalid_input("url", "MCP URL is invalid"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid_input(
            "url",
            "MCP URL must use http or https and include a host",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid_input(
            "url",
            "MCP URL must not contain inline credentials",
        ));
    }
    if url.fragment().is_some() {
        return Err(invalid_input("url", "MCP URL must not contain a fragment"));
    }
    Ok(())
}

fn invalid_input(field: &'static str, message: impl Into<String>) -> DesktopApplicationError {
    DesktopApplicationError::InvalidInput {
        field,
        message: message.into(),
    }
}

fn activation_result(result: RegisteredMcpActivation) -> DesktopMcpActivationResult {
    DesktopMcpActivationResult {
        server_id: result.server_id,
        runtime_state: result.state.as_ref().map(mcp_state_key).map(str::to_owned),
        tool_count: result.tool_count,
        resource_count: result.resource_count,
        prompt_count: result.prompt_count,
        error: result.error,
    }
}

fn runtime_services(status: &SharedCodingServicesStatus) -> Vec<DesktopRuntimeServiceView> {
    [
        (status.git_service_id, "Git", status.git_same_instance),
        (
            status.code_index_service_id,
            "Code Index",
            status.code_index_same_instance,
        ),
        (status.lsp_service_id, "LSP", status.lsp_same_instance),
        (status.mcp_service_id, "MCP", status.mcp_same_instance),
        (
            status.computer_use_service_id,
            "Computer Use",
            status.computer_use_same_instance,
        ),
        (
            status.memory_runner_id,
            "Memory Router",
            status.memory_shared_router,
        ),
    ]
    .into_iter()
    .map(
        |(service_id, label, shared_with_agent)| DesktopRuntimeServiceView {
            service_id: service_id.to_owned(),
            label: label.to_owned(),
            shared_with_agent,
        },
    )
    .collect()
}

fn mcp_state_key(state: &McpServerState) -> &'static str {
    match state {
        McpServerState::Connecting => "connecting",
        McpServerState::Ready => "ready",
        McpServerState::Failed => "failed",
        McpServerState::Draining => "draining",
    }
}

fn extension_runtime_error(error: impl std::fmt::Display) -> DesktopApplicationError {
    DesktopApplicationError::Agent(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };
    use lilia_service::ServiceAuthority;
    use tempfile::TempDir;

    #[test]
    fn mcp_resource_content_mapping_preserves_text_and_bounds_binary_to_metadata() {
        let contents = mcp_resource_contents(&serde_json::json!({
            "contents": [
                {
                    "uri": "mcp://fixture/note",
                    "mimeType": "text/plain",
                    "text": "visible resource"
                },
                {
                    "uri": "mcp://fixture/image",
                    "mimeType": "image/png",
                    "blob": "YWJjZA=="
                }
            ]
        }))
        .unwrap();
        assert_eq!(contents[0].text.as_deref(), Some("visible resource"));
        assert_eq!(contents[0].encoded_blob_length, None);
        assert_eq!(contents[1].text, None);
        assert_eq!(contents[1].encoded_blob_length, Some(8));
    }

    #[derive(Default)]
    struct NoopHost {
        secrets: Mutex<BTreeMap<String, Vec<u8>>>,
    }

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            match action {
                DesktopHostAction::Credential(DesktopCredentialAction::Read { key }) => {
                    Ok(DesktopHostResult::Credential(
                        self.secrets
                            .lock()
                            .unwrap()
                            .get(&key)
                            .cloned()
                            .map(DesktopSecret::new),
                    ))
                }
                DesktopHostAction::Credential(DesktopCredentialAction::Write { key, secret }) => {
                    self.secrets
                        .lock()
                        .unwrap()
                        .insert(key, secret.into_inner());
                    Ok(DesktopHostResult::Completed)
                }
                DesktopHostAction::Credential(DesktopCredentialAction::Delete { key }) => {
                    self.secrets.lock().unwrap().remove(&key);
                    Ok(DesktopHostResult::Completed)
                }
                _ => Ok(DesktopHostResult::Completed),
            }
        }
    }

    fn application() -> (DesktopApplication, TempDir) {
        let home = tempfile::tempdir().unwrap();
        let authority = ServiceAuthority::bootstrap_with_home(home.path()).unwrap();
        let application = DesktopApplication::from_authority(
            DesktopApplicationConfig::new(home.path(), "liliacode.extensions-test").unwrap(),
            authority,
            Arc::new(NoopHost::default()),
        )
        .unwrap();
        (application, home)
    }

    #[test]
    fn runtime_services_preserve_shared_service_identity() {
        let status = SharedCodingServicesStatus {
            git_service_id: "git",
            code_index_service_id: "index",
            lsp_service_id: "lsp",
            computer_use_service_id: "computer",
            mcp_service_id: "mcp",
            memory_runner_id: "memory",
            shared_identity_ok: true,
            git_same_instance: true,
            code_index_same_instance: true,
            lsp_same_instance: true,
            mcp_same_instance: true,
            computer_use_same_instance: true,
            memory_shared_router: true,
            mcp_active_servers: 0,
            lsp_active_workspaces: 0,
            data_source: "test",
            official_agent_server: false,
        };
        let services = runtime_services(&status);
        assert_eq!(services.len(), 6);
        assert!(services.iter().all(|service| service.shared_with_agent));
        assert_eq!(services[3].label, "MCP");
    }

    #[test]
    fn managed_skill_lifecycle_is_revision_safe_and_controls_runtime_discovery() {
        let (application, home) = application();
        let created = application
            .create_skill_package(DesktopSkillCreate {
                expected_registry_revision: 0,
                scope: DesktopSkillScope::User,
                project_cwd: None,
                skill_id: "review-changes".to_owned(),
                description: "Review the current change set.".to_owned(),
            })
            .unwrap();
        assert_eq!(created.skills_registry_revision, 1);
        assert_eq!(created.skills.len(), 1);
        assert!(created.skills[0].enabled);
        assert!(created.skills[0].editable);
        assert!(created.skills[0].runtime_available);
        let skill_dir = home.path().join("skills").join("review-changes");
        assert!(skill_dir.join("SKILL.md").is_file());

        let disabled = application
            .set_skill_package_enabled("review-changes", false, 1)
            .unwrap();
        assert_eq!(disabled.skills_registry_revision, 2);
        assert!(!disabled.skills[0].enabled);
        assert!(!disabled.skills[0].runtime_available);
        assert!(application
            .set_skill_package_enabled("review-changes", true, 1)
            .is_err());

        let enabled = application
            .set_skill_package_enabled("review-changes", true, 2)
            .unwrap();
        assert_eq!(enabled.skills_registry_revision, 3);
        assert!(enabled.skills[0].runtime_available);

        let deleted = application
            .delete_skill_package("review-changes", 3)
            .unwrap();
        assert_eq!(deleted.skills_registry_revision, 4);
        assert!(deleted.skills.is_empty());
        assert!(!skill_dir.exists());
        let runtime_catalog = serde_json::from_value::<SkillDiscoverResult>(
            application
                .authority()
                .shared_runtime()
                .inner()
                .shared_skill_catalog()
                .unwrap(),
        )
        .unwrap();
        assert!(runtime_catalog.catalog.is_empty());
    }

    #[test]
    fn mcp_registry_mutations_are_revision_safe_and_reconcile_runtime() {
        let (application, _home) = application();
        let created = application
            .upsert_mcp_server(DesktopMcpServerUpsert {
                expected_registry_revision: 0,
                server_id: "fixture.server".to_owned(),
                transport: DesktopMcpTransport::Stdio,
                command: Some("lilia-command-that-does-not-exist".to_owned()),
                args: vec!["--stdio".to_owned()],
                url: None,
                env_secret_names: Vec::new(),
                header_secret_names: Vec::new(),
                enabled: false,
            })
            .unwrap();
        assert_eq!(created.snapshot.mcp_registry_revision, 1);
        assert_eq!(created.snapshot.mcp_servers.len(), 1);
        assert!(!created.snapshot.mcp_servers[0].enabled);
        assert!(created.results[0].error.is_none());
        assert!(application
            .activate_registered_mcp_servers()
            .unwrap()
            .results
            .is_empty());

        let stale = application
            .set_mcp_server_enabled("fixture.server", true, 0)
            .unwrap_err();
        assert!(matches!(
            stale,
            DesktopApplicationError::InvalidInput {
                field: "expected_registry_revision",
                ..
            }
        ));
        assert_eq!(
            application
                .extensions_snapshot()
                .unwrap()
                .mcp_registry_revision,
            1
        );

        let enabled = application
            .set_mcp_server_enabled("fixture.server", true, 1)
            .unwrap();
        assert_eq!(enabled.snapshot.mcp_registry_revision, 2);
        assert!(enabled.snapshot.mcp_servers[0].enabled);
        assert!(enabled.results[0].error.is_some());

        let deleted = application.delete_mcp_server("fixture.server", 2).unwrap();
        assert_eq!(deleted.snapshot.mcp_registry_revision, 3);
        assert!(deleted.snapshot.mcp_servers.is_empty());
        assert!(
            lilia_storage::load_mcp_registry(&application.config().data_paths())
                .unwrap()
                .unwrap()
                .servers
                .is_empty()
        );
    }

    #[test]
    fn mcp_registry_rejects_ambiguous_or_credential_bearing_inputs_before_write() {
        let (application, _home) = application();
        let invalid_id = application
            .upsert_mcp_server(DesktopMcpServerUpsert {
                expected_registry_revision: 0,
                server_id: "invalid server".to_owned(),
                transport: DesktopMcpTransport::Stdio,
                command: Some("fixture".to_owned()),
                args: Vec::new(),
                url: None,
                env_secret_names: Vec::new(),
                header_secret_names: Vec::new(),
                enabled: false,
            })
            .unwrap_err();
        assert!(matches!(
            invalid_id,
            DesktopApplicationError::InvalidInput {
                field: "server_id",
                ..
            }
        ));
        let credential_url = application
            .upsert_mcp_server(DesktopMcpServerUpsert {
                expected_registry_revision: 0,
                server_id: "remote".to_owned(),
                transport: DesktopMcpTransport::StreamableHttp,
                command: None,
                args: Vec::new(),
                url: Some("https://token@example.com/mcp".to_owned()),
                env_secret_names: Vec::new(),
                header_secret_names: Vec::new(),
                enabled: false,
            })
            .unwrap_err();
        assert!(matches!(
            credential_url,
            DesktopApplicationError::InvalidInput { field: "url", .. }
        ));
        assert!(!lilia_storage::mcp_registry_path(&application.config().data_paths()).exists());
    }

    #[test]
    fn mcp_credentials_are_keyring_only_and_follow_registry_lifecycle() {
        let (application, _home) = application();
        let created = application
            .upsert_mcp_server(DesktopMcpServerUpsert {
                expected_registry_revision: 0,
                server_id: "secured".to_owned(),
                transport: DesktopMcpTransport::Stdio,
                command: Some("missing-secured-server".to_owned()),
                args: Vec::new(),
                url: None,
                env_secret_names: vec!["API_TOKEN".to_owned()],
                header_secret_names: Vec::new(),
                enabled: false,
            })
            .unwrap();
        assert_eq!(created.snapshot.mcp_servers[0].credentials.len(), 1);
        assert!(!created.snapshot.mcp_servers[0].credentials[0].present);

        let canary = "mcp-keyring-only-canary";
        let saved = application
            .set_mcp_server_credential(
                "secured",
                DesktopMcpCredentialKind::Environment,
                "API_TOKEN",
                DesktopSecret::new(canary.as_bytes().to_vec()),
            )
            .unwrap();
        assert!(saved.snapshot.mcp_servers[0].credentials[0].present);
        let registry_path = lilia_storage::mcp_registry_path(&application.config().data_paths());
        let registry_text = std::fs::read_to_string(&registry_path).unwrap();
        assert!(registry_text.contains("API_TOKEN"));
        assert!(!registry_text.contains(canary));

        let enabled = application
            .set_mcp_server_enabled("secured", true, 1)
            .unwrap();
        assert!(enabled.results[0].error.is_some());
        assert!(!enabled.results[0]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains(canary));

        let cleared = application
            .delete_mcp_server_credential(
                "secured",
                DesktopMcpCredentialKind::Environment,
                "API_TOKEN",
            )
            .unwrap();
        assert!(!cleared.snapshot.mcp_servers[0].credentials[0].present);
        assert!(cleared.results[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("OS Keyring")));

        application
            .set_mcp_server_credential(
                "secured",
                DesktopMcpCredentialKind::Environment,
                "API_TOKEN",
                DesktopSecret::new(canary.as_bytes().to_vec()),
            )
            .unwrap();
        let deleted = application.delete_mcp_server("secured", 2).unwrap();
        assert!(deleted.snapshot.mcp_servers.is_empty());
        assert!(!application
            .read_mcp_credential(
                "secured",
                DesktopMcpCredentialKind::Environment,
                "API_TOKEN"
            )
            .unwrap()
            .is_some());
    }
}

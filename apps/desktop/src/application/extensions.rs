use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use lilia_agent::{RegisteredMcpActivation, SharedCodingServicesStatus};
use lilia_feature_extensions::{
    bump_registry_revision, bump_skills_registry_revision, ensure_mcp_credential_is_configured,
    ensure_registry_revision, ensure_skills_registry_revision, is_managed_skill,
    managed_skill_root, mcp_activation_error, mcp_credential_key, mcp_prompts,
    mcp_resource_contents, mcp_resources, mcp_state_key, mcp_tools, normalized_mcp_credential_name,
    normalized_server_id, normalized_skill_description, normalized_skill_id,
    removed_mcp_credentials, required_mcp_value, skill_io_error, validate_mcp_secret,
    verified_managed_skill_path, write_skill_document, NormalizedMcpServer,
};
use lilia_storage::{AgentkitMcpRegistryEntry, AgentkitSkillPackageRef, AgentkitSkillsRegistry};
use mutsuki_agent_contracts::{
    McpCatalog, McpServerStatus, SkillDiscoverResult, SkillLoadResult, SkillSourceKind,
};

use crate::application::{
    DesktopApplication, DesktopApplicationError, DesktopCredentialAction, DesktopHostAction,
    DesktopHostResult, DesktopSecret,
};

/// The extensions domain raises the same failures the desktop surface already
/// renders, so it keeps the existing error shape instead of nesting one more.
impl From<lilia_feature_extensions::ExtensionsError> for DesktopApplicationError {
    fn from(error: lilia_feature_extensions::ExtensionsError) -> Self {
        use lilia_feature_extensions::ExtensionsError;

        match error {
            ExtensionsError::InvalidInput { field, message } => {
                Self::InvalidInput { field, message }
            }
            ExtensionsError::Agent(message) => Self::Agent(message),
            ExtensionsError::StateUnavailable(state) => Self::StateUnavailable(state),
            ExtensionsError::StateRevisionOverflow(state) => Self::StateRevisionOverflow(state),
        }
    }
}

pub use lilia_feature_extensions::{
    ExtensionsSnapshot as DesktopExtensionsSnapshot,
    McpActivationReport as DesktopMcpActivationReport,
    McpActivationResult as DesktopMcpActivationResult,
    McpCredentialKind as DesktopMcpCredentialKind, McpCredentialView as DesktopMcpCredentialView,
    McpPromptArgumentView as DesktopMcpPromptArgumentView,
    McpPromptFragmentView as DesktopMcpPromptFragmentView,
    McpPromptGetView as DesktopMcpPromptGetView, McpPromptView as DesktopMcpPromptView,
    McpResourceContentView as DesktopMcpResourceContentView,
    McpResourceReadView as DesktopMcpResourceReadView, McpResourceView as DesktopMcpResourceView,
    McpServerUpsert as DesktopMcpServerUpsert, McpServerView as DesktopMcpServerView,
    McpToolView as DesktopMcpToolView, McpTransport as DesktopMcpTransport,
    RuntimeServiceView as DesktopRuntimeServiceView, SkillCreate as DesktopSkillCreate,
    SkillPackageView as DesktopSkillPackageView, SkillScope as DesktopSkillScope,
};

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
            return Err(error.into());
        }
        if let Err(error) = fs::rename(&staging, &package_path) {
            let _ = fs::remove_dir_all(&staging);
            return Err(skill_io_error("publish Skill directory", error).into());
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
            return Err(error.into());
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
            return Err(error.into());
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
        validate_mcp_secret(kind, secret.expose())?;
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

#[derive(Default)]
struct ResolvedMcpCredentials {
    env: Vec<(String, String)>,
    headers: Vec<(String, String)>,
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
fn invalid_input(field: &'static str, message: impl Into<String>) -> DesktopApplicationError {
    DesktopApplicationError::InvalidInput {
        field,
        message: message.into(),
    }
}

fn extension_runtime_error(error: impl std::fmt::Display) -> DesktopApplicationError {
    DesktopApplicationError::Agent(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use crate::application::{
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

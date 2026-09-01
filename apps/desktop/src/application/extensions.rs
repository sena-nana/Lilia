use lilia_agent::{RegisteredMcpActivation, SharedCodingServicesStatus};
use lilia_feature_extensions::{
    activate_mcp_entry, activate_registered_mcp_servers, create_skill_package,
    delete_mcp_credentials_for_entries, delete_mcp_server, delete_mcp_server_credential,
    delete_skill_package, extensions_snapshot, get_mcp_prompt, mcp_state_key, read_mcp_resource,
    set_mcp_server_credential, set_mcp_server_enabled, set_skill_package_enabled,
    upsert_mcp_server, CodingRuntimeFacts, ExtensionsHost, LoadedPluginFacts, McpPromptRead,
};
use lilia_storage::{AgentkitMcpRegistryEntry, LiliaDataPaths};
use mutsuki_agent_contracts::{McpCatalog, McpServerStatus, SkillDiscoverResult, SkillLoadResult};

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
            ExtensionsError::Product(error) => Self::Product(error),
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

impl ExtensionsHost for DesktopApplication {
    fn data_paths(&self) -> LiliaDataPaths {
        self.config().data_paths()
    }

    fn plugin_packages(
        &self,
    ) -> Result<
        (
            u64,
            String,
            Vec<lilia_feature_extensions::PluginPackageView>,
        ),
        lilia_feature_extensions::ExtensionsError,
    > {
        desktop_plugin_packages(self)
            .map_err(|error| lilia_feature_extensions::ExtensionsError::Agent(error.to_string()))
    }

    fn loaded_plugins(&self) -> Vec<LoadedPluginFacts> {
        self.loaded_plugin_packages()
            .into_iter()
            .map(|package| LoadedPluginFacts {
                root: package.root,
                skill_paths: package.skill_paths,
                mcp_servers: package.mcp_servers,
            })
            .collect()
    }

    fn coding_runtime(
        &self,
    ) -> Result<CodingRuntimeFacts, lilia_feature_extensions::ExtensionsError> {
        let status = self
            .authority()
            .shared_runtime()
            .inner()
            .shared_coding_services_status()
            .map_err(extension_runtime_error)?;
        Ok(coding_runtime_facts(&status))
    }

    fn active_mcp_servers(
        &self,
    ) -> Result<Vec<McpServerStatus>, lilia_feature_extensions::ExtensionsError> {
        serde_json::from_value(
            self.authority()
                .shared_runtime()
                .inner()
                .shared_mcp_list_servers()
                .map_err(extension_runtime_error)?,
        )
        .map_err(|error| lilia_feature_extensions::ExtensionsError::Agent(error.to_string()))
    }

    fn mcp_catalog(&self) -> Result<McpCatalog, lilia_feature_extensions::ExtensionsError> {
        serde_json::from_value(
            self.authority()
                .shared_runtime()
                .inner()
                .shared_mcp_catalog(None)
                .map_err(extension_runtime_error)?,
        )
        .map_err(|error| lilia_feature_extensions::ExtensionsError::Agent(error.to_string()))
    }

    fn skill_catalog(
        &self,
    ) -> Result<SkillDiscoverResult, lilia_feature_extensions::ExtensionsError> {
        serde_json::from_value(
            self.authority()
                .shared_runtime()
                .inner()
                .shared_skill_catalog()
                .map_err(extension_runtime_error)?,
        )
        .map_err(|error| lilia_feature_extensions::ExtensionsError::Agent(error.to_string()))
    }

    fn load_skill(
        &self,
        skill_id: &str,
    ) -> Result<SkillLoadResult, lilia_feature_extensions::ExtensionsError> {
        serde_json::from_value(
            self.authority()
                .shared_runtime()
                .inner()
                .shared_skill_load(skill_id)
                .map_err(extension_runtime_error)?,
        )
        .map_err(|error| lilia_feature_extensions::ExtensionsError::Agent(error.to_string()))
    }

    fn reload_registered_skills(&self) -> Result<(), lilia_feature_extensions::ExtensionsError> {
        DesktopApplication::reload_registered_skills(self)
            .map_err(|error| lilia_feature_extensions::ExtensionsError::Agent(error.to_string()))
    }

    fn disconnect_mcp(&self, server_id: &str) -> Result<(), String> {
        self.authority()
            .shared_runtime()
            .inner()
            .disconnect_shared_mcp_server(server_id)
            .map_err(|error| error.to_string())
    }

    fn activate_registered_mcp(
        &self,
        entry: AgentkitMcpRegistryEntry,
        env: Vec<(String, String)>,
        headers: Vec<(String, String)>,
    ) -> lilia_feature_extensions::McpActivationResult {
        activation_result(
            self.authority()
                .shared_runtime()
                .inner()
                .activate_mcp_registry_entry(entry, env, headers),
        )
    }

    fn read_mcp_resource_raw(
        &self,
        server_id: &str,
        uri: &str,
    ) -> Result<(String, String, serde_json::Value), lilia_feature_extensions::ExtensionsError>
    {
        let resource = self
            .authority()
            .shared_runtime()
            .inner()
            .shared_mcp_read_resource(server_id, uri)
            .map_err(extension_runtime_error)?;
        Ok((
            resource.result.uri,
            resource.result.summary,
            resource.content,
        ))
    }

    fn get_mcp_prompt_raw(
        &self,
        namespaced_name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpPromptRead, lilia_feature_extensions::ExtensionsError> {
        let prompt = self
            .authority()
            .shared_runtime()
            .inner()
            .shared_mcp_get_prompt(namespaced_name, arguments)
            .map_err(extension_runtime_error)?;
        Ok(McpPromptRead {
            description: prompt.prompt.description,
            fragments: prompt
                .fragments
                .into_iter()
                .map(|fragment| lilia_feature_extensions::McpPromptFragmentView {
                    fragment_id: fragment.fragment_id,
                    content: fragment.content,
                    priority: fragment.priority,
                })
                .collect(),
        })
    }

    fn read_secret(
        &self,
        key: &str,
    ) -> Result<Option<Vec<u8>>, lilia_feature_extensions::ExtensionsError> {
        match self
            .execute_host(DesktopHostAction::Credential(
                DesktopCredentialAction::Read {
                    key: key.to_owned(),
                },
            ))
            .map_err(|error| lilia_feature_extensions::ExtensionsError::Agent(error.to_string()))?
        {
            DesktopHostResult::Credential(secret) => Ok(secret.map(DesktopSecret::into_inner)),
            _ => Err(lilia_feature_extensions::ExtensionsError::StateUnavailable(
                "MCP credential host result",
            )),
        }
    }

    fn write_secret(
        &self,
        key: &str,
        secret: Vec<u8>,
    ) -> Result<(), lilia_feature_extensions::ExtensionsError> {
        match self
            .execute_host(DesktopHostAction::Credential(
                DesktopCredentialAction::Write {
                    key: key.to_owned(),
                    secret: DesktopSecret::new(secret),
                },
            ))
            .map_err(|error| lilia_feature_extensions::ExtensionsError::Agent(error.to_string()))?
        {
            DesktopHostResult::Completed => Ok(()),
            _ => Err(lilia_feature_extensions::ExtensionsError::StateUnavailable(
                "MCP credential host result",
            )),
        }
    }

    fn delete_secret(&self, key: &str) -> Result<(), lilia_feature_extensions::ExtensionsError> {
        match self
            .execute_host(DesktopHostAction::Credential(
                DesktopCredentialAction::Delete {
                    key: key.to_owned(),
                },
            ))
            .map_err(|error| lilia_feature_extensions::ExtensionsError::Agent(error.to_string()))?
        {
            DesktopHostResult::Completed => Ok(()),
            _ => Err(lilia_feature_extensions::ExtensionsError::StateUnavailable(
                "MCP credential host result",
            )),
        }
    }
}

impl DesktopApplication {
    fn with_extension_registry<T>(
        &self,
        run: impl FnOnce(&Self) -> Result<T, DesktopApplicationError>,
    ) -> Result<T, DesktopApplicationError> {
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        run(self)
    }

    pub fn extensions_snapshot(
        &self,
    ) -> Result<DesktopExtensionsSnapshot, DesktopApplicationError> {
        Ok(extensions_snapshot(self)?)
    }

    pub fn create_skill_package(
        &self,
        input: DesktopSkillCreate,
    ) -> Result<DesktopExtensionsSnapshot, DesktopApplicationError> {
        self.with_extension_registry(|host| Ok(create_skill_package(host, input)?))
    }

    pub fn set_skill_package_enabled(
        &self,
        skill_id: &str,
        enabled: bool,
        expected_registry_revision: u64,
    ) -> Result<DesktopExtensionsSnapshot, DesktopApplicationError> {
        self.with_extension_registry(|host| {
            Ok(set_skill_package_enabled(
                host,
                skill_id,
                enabled,
                expected_registry_revision,
            )?)
        })
    }

    pub fn delete_skill_package(
        &self,
        skill_id: &str,
        expected_registry_revision: u64,
    ) -> Result<DesktopExtensionsSnapshot, DesktopApplicationError> {
        self.with_extension_registry(|host| {
            Ok(delete_skill_package(
                host,
                skill_id,
                expected_registry_revision,
            )?)
        })
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
            .map_err(|error| DesktopApplicationError::Agent(error.to_string()))
    }

    pub(crate) fn reload_extension_contributions(&self) -> Result<(), DesktopApplicationError> {
        self.reload_registered_skills()?;
        let runtime = self.authority().shared_runtime();
        let active_servers = serde_json::from_value::<Vec<McpServerStatus>>(
            runtime
                .inner()
                .shared_mcp_list_servers()
                .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?,
        )
        .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        for server in active_servers
            .iter()
            .filter(|server| server.server_id.starts_with("plugin."))
        {
            runtime
                .inner()
                .disconnect_shared_mcp_server(&server.server_id)
                .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        }
        for server in self
            .loaded_plugin_packages()
            .into_iter()
            .flat_map(|package| package.mcp_servers)
            .filter(|server| server.enabled)
        {
            let _ = activate_mcp_entry(self, server);
        }
        Ok(())
    }

    pub fn upsert_mcp_server(
        &self,
        input: DesktopMcpServerUpsert,
    ) -> Result<DesktopMcpActivationReport, DesktopApplicationError> {
        self.with_extension_registry(|host| Ok(upsert_mcp_server(host, input)?))
    }

    pub fn set_mcp_server_enabled(
        &self,
        server_id: &str,
        enabled: bool,
        expected_registry_revision: u64,
    ) -> Result<DesktopMcpActivationReport, DesktopApplicationError> {
        self.with_extension_registry(|host| {
            Ok(set_mcp_server_enabled(
                host,
                server_id,
                enabled,
                expected_registry_revision,
            )?)
        })
    }

    pub fn delete_mcp_server(
        &self,
        server_id: &str,
        expected_registry_revision: u64,
    ) -> Result<DesktopMcpActivationReport, DesktopApplicationError> {
        self.with_extension_registry(|host| {
            Ok(delete_mcp_server(
                host,
                server_id,
                expected_registry_revision,
            )?)
        })
    }

    pub fn set_mcp_server_credential(
        &self,
        server_id: &str,
        kind: DesktopMcpCredentialKind,
        name: &str,
        secret: DesktopSecret,
    ) -> Result<DesktopMcpActivationReport, DesktopApplicationError> {
        self.with_extension_registry(|host| {
            Ok(set_mcp_server_credential(
                host,
                server_id,
                kind,
                name,
                secret.into_inner(),
            )?)
        })
    }

    pub fn delete_mcp_server_credential(
        &self,
        server_id: &str,
        kind: DesktopMcpCredentialKind,
        name: &str,
    ) -> Result<DesktopMcpActivationReport, DesktopApplicationError> {
        self.with_extension_registry(|host| {
            Ok(delete_mcp_server_credential(host, server_id, kind, name)?)
        })
    }

    pub fn activate_registered_mcp_servers(
        &self,
    ) -> Result<DesktopMcpActivationReport, DesktopApplicationError> {
        Ok(activate_registered_mcp_servers(self)?)
    }

    pub fn read_mcp_resource(
        &self,
        server_id: &str,
        uri: &str,
    ) -> Result<DesktopMcpResourceReadView, DesktopApplicationError> {
        Ok(read_mcp_resource(self, server_id, uri)?)
    }

    pub fn get_mcp_prompt(
        &self,
        namespaced_name: &str,
        arguments: serde_json::Value,
    ) -> Result<DesktopMcpPromptGetView, DesktopApplicationError> {
        Ok(get_mcp_prompt(self, namespaced_name, arguments)?)
    }

    pub(crate) fn delete_mcp_credentials_for_entries(
        &self,
        entries: &[AgentkitMcpRegistryEntry],
    ) -> Result<(), DesktopApplicationError> {
        self.with_extension_registry(|host| Ok(delete_mcp_credentials_for_entries(host, entries)?))
    }

    #[cfg(test)]
    fn read_mcp_credential(
        &self,
        server_id: &str,
        kind: DesktopMcpCredentialKind,
        name: &str,
    ) -> Result<Option<DesktopSecret>, DesktopApplicationError> {
        Ok(ExtensionsHost::read_secret(
            self,
            &lilia_feature_extensions::mcp_credential_key(server_id, kind, name),
        )?
        .map(DesktopSecret::new))
    }
}

fn desktop_plugin_packages(
    application: &DesktopApplication,
) -> Result<
    (
        u64,
        String,
        Vec<lilia_feature_extensions::PluginPackageView>,
    ),
    DesktopApplicationError,
> {
    application.plugin_packages()
}

fn coding_runtime_facts(status: &SharedCodingServicesStatus) -> CodingRuntimeFacts {
    CodingRuntimeFacts {
        data_source: status.data_source.to_owned(),
        shared_identity_ok: status.shared_identity_ok,
        mcp_same_instance: status.mcp_same_instance,
        git_service_id: status.git_service_id.to_owned(),
        git_same_instance: status.git_same_instance,
        code_index_service_id: status.code_index_service_id.to_owned(),
        code_index_same_instance: status.code_index_same_instance,
        lsp_service_id: status.lsp_service_id.to_owned(),
        lsp_same_instance: status.lsp_same_instance,
        mcp_service_id: status.mcp_service_id.to_owned(),
        computer_use_service_id: status.computer_use_service_id.to_owned(),
        computer_use_same_instance: status.computer_use_same_instance,
        memory_runner_id: status.memory_runner_id.to_owned(),
        memory_shared_router: status.memory_shared_router,
    }
}

fn activation_result(
    result: RegisteredMcpActivation,
) -> lilia_feature_extensions::McpActivationResult {
    lilia_feature_extensions::McpActivationResult {
        server_id: result.server_id,
        runtime_state: result.state.as_ref().map(mcp_state_key).map(str::to_owned),
        tool_count: result.tool_count,
        resource_count: result.resource_count,
        prompt_count: result.prompt_count,
        error: result.error,
    }
}

fn extension_runtime_error(
    error: impl std::fmt::Display,
) -> lilia_feature_extensions::ExtensionsError {
    lilia_feature_extensions::ExtensionsError::Agent(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };
    use lilia_feature_extensions::{coding_runtime_services, mcp_resource_contents};
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
        let status = CodingRuntimeFacts {
            git_service_id: "git".to_owned(),
            code_index_service_id: "index".to_owned(),
            lsp_service_id: "lsp".to_owned(),
            computer_use_service_id: "computer".to_owned(),
            mcp_service_id: "mcp".to_owned(),
            memory_runner_id: "memory".to_owned(),
            shared_identity_ok: true,
            git_same_instance: true,
            code_index_same_instance: true,
            lsp_same_instance: true,
            mcp_same_instance: true,
            computer_use_same_instance: true,
            memory_shared_router: true,
            data_source: "test".to_owned(),
        };
        let services = coding_runtime_services(&status);
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

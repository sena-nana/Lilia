//! Extensions domain feature.
//!
//! Owns everything the extensions surface installs and toggles: skill packages,
//! plugin packages, MCP servers and the runtime services those servers share
//! with the Agent. The registries themselves live in `lilia-storage`; this crate
//! owns the vocabulary the product speaks and the validation every mutation must
//! pass before it reaches a registry.

mod error;
mod jobs;
mod mcp;
mod skill;
mod types;

use std::sync::Arc;

use lilia_kernel::{Feature, FeatureContext, FeatureId, JobProtocol, KernelError};

pub use error::{invalid_input, ExtensionsError};
pub use jobs::{extensions_slot, ExtensionsPort, MutateRequest, MUTATE_PROTOCOL};
pub use mcp::{
    bump_registry_revision, ensure_mcp_credential_is_configured, ensure_registry_revision,
    mcp_activation_error, mcp_credential_key, mcp_prompts, mcp_resource_contents, mcp_resources,
    mcp_state_key, mcp_tools, normalized_mcp_credential_name, normalized_server_id,
    removed_mcp_credentials, required_mcp_value,
    validate_mcp_secret, NormalizedMcpServer,
};
pub use skill::{
    bump_skills_registry_revision, ensure_skills_registry_revision, is_managed_skill,
    managed_skill_root, normalized_skill_description, normalized_skill_id, skill_io_error,
    verified_managed_skill_path, write_skill_document,
};
pub use types::{
    ExtensionsSnapshot, McpActivationReport, McpActivationResult, McpCredentialKind,
    McpCredentialView, McpPromptArgumentView, McpPromptFragmentView, McpPromptGetView,
    McpPromptView, McpResourceContentView, McpResourceReadView, McpResourceView, McpServerUpsert,
    McpServerView, McpToolView, McpTransport, PluginPackageView, RuntimeServiceView,
    SkillPackageView, SkillScope, SkillCreate,
};

pub struct ExtensionsFeature {
    port: Arc<dyn ExtensionsPort>,
}

impl ExtensionsFeature {
    pub fn new(port: Arc<dyn ExtensionsPort>) -> Self {
        Self { port }
    }
}

impl Feature for ExtensionsFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.extensions").expect("the extensions feature id is not blank")
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        vec![jobs::mutate_protocol(Arc::clone(&self.port))]
    }

    fn mount(&self, _cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reserved_plugin_prefix_never_becomes_a_user_defined_server_id() {
        let error = normalized_server_id("plugin.docs")
            .expect_err("the plugin prefix belongs to installed packages");

        assert!(matches!(
            error,
            ExtensionsError::InvalidInput {
                field: "server_id",
                ..
            }
        ));
    }

    #[test]
    fn a_stdio_server_rejects_the_http_only_fields_it_cannot_honour() {
        let error = NormalizedMcpServer::new(McpServerUpsert {
            expected_registry_revision: 3,
            server_id: "docs".to_owned(),
            transport: McpTransport::Stdio,
            command: Some("docs-server".to_owned()),
            args: Vec::new(),
            url: Some("https://example.test".to_owned()),
            env_secret_names: Vec::new(),
            header_secret_names: Vec::new(),
            enabled: true,
        })
        .expect_err("a stdio server has no URL");

        assert!(matches!(
            error,
            ExtensionsError::InvalidInput { field: "url", .. }
        ));
    }

    #[test]
    fn an_http_server_keeps_credentials_out_of_its_url() {
        let error = NormalizedMcpServer::new(McpServerUpsert {
            expected_registry_revision: 1,
            server_id: "docs".to_owned(),
            transport: McpTransport::StreamableHttp,
            command: None,
            args: Vec::new(),
            url: Some("https://user:pass@example.test/mcp".to_owned()),
            env_secret_names: Vec::new(),
            header_secret_names: Vec::new(),
            enabled: true,
        })
        .expect_err("inline credentials leak into logs and registries");

        assert!(matches!(
            error,
            ExtensionsError::InvalidInput { field: "url", .. }
        ));
    }

    #[test]
    fn header_credential_names_collapse_case_before_they_become_registry_keys() {
        let server = NormalizedMcpServer::new(McpServerUpsert {
            expected_registry_revision: 1,
            server_id: "docs".to_owned(),
            transport: McpTransport::Sse,
            command: None,
            args: Vec::new(),
            url: Some("https://example.test/mcp".to_owned()),
            env_secret_names: Vec::new(),
            header_secret_names: vec!["Authorization".to_owned()],
            enabled: true,
        })
        .expect("an SSE server with one header credential is valid");

        assert_eq!(
            mcp_credential_key(
                &server.server_id,
                McpCredentialKind::Header,
                &server.header_secret_names[0]
            ),
            "mcp.server.docs.header.authorization"
        );
    }

    #[test]
    fn a_credential_with_a_control_character_never_reaches_a_header() {
        let error = validate_mcp_secret(McpCredentialKind::Header, b"token\nInjected: 1")
            .expect_err("a newline splits an HTTP header");

        assert!(matches!(
            error,
            ExtensionsError::InvalidInput {
                field: "credential",
                ..
            }
        ));
    }
}

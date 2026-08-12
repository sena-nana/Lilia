use std::collections::BTreeMap;

use lilia_desktop_application::{
    DesktopApplication, DesktopMcpCredentialKind, DesktopMcpServerView, DesktopPluginPackageView,
    DesktopSkillPackageView,
};

use super::types::{PluginMcpServer, PluginPackage, PluginSkill, PluginsOverview};

pub const NATIVE_AGENTKIT_BACKEND: &str = "native-agentkit";

pub fn overview(application: &DesktopApplication) -> PluginsOverview {
    match application.extensions_snapshot() {
        Ok(snapshot) => PluginsOverview {
            skills: snapshot.skills.into_iter().map(plugin_skill).collect(),
            packages: snapshot.plugins.into_iter().map(plugin_package).collect(),
            mcp_servers: snapshot
                .mcp_servers
                .into_iter()
                .map(plugin_mcp_server)
                .collect(),
            config_paths: BTreeMap::from([(
                NATIVE_AGENTKIT_BACKEND.to_owned(),
                Some(snapshot.mcp_registry_path.clone()),
            )]),
            plugins_registry_revision: snapshot.plugins_registry_revision,
            plugins_registry_path: Some(snapshot.plugins_registry_path),
            warnings: Vec::new(),
        },
        Err(error) => PluginsOverview {
            skills: Vec::new(),
            packages: Vec::new(),
            mcp_servers: Vec::new(),
            config_paths: BTreeMap::new(),
            plugins_registry_revision: 0,
            plugins_registry_path: None,
            warnings: vec![format!("Native AgentKit 扩展状态不可用：{error}")],
        },
    }
}

pub fn plugin_package(package: DesktopPluginPackageView) -> PluginPackage {
    PluginPackage {
        id: package.plugin_id,
        backend: NATIVE_AGENTKIT_BACKEND.to_owned(),
        scope: "user".to_owned(),
        name: package.name,
        description: package.description,
        version: package.version,
        enabled: package.enabled,
        editable: package.editable,
        runtime_available: package.runtime_available,
        path: package.path,
        package_sha256: package.package_sha256,
        skill_count: package.skill_count,
        hook_count: package.hook_count,
        mcp_server_count: package.mcp_server_count,
        warnings: package.warnings,
    }
}

pub fn plugin_skill(skill: DesktopSkillPackageView) -> PluginSkill {
    PluginSkill {
        backend: NATIVE_AGENTKIT_BACKEND.to_owned(),
        scope: skill.scope,
        name: skill.skill_id,
        description: skill.description,
        enabled: skill.enabled,
        editable: skill.editable,
        path: skill.path,
    }
}

pub fn plugin_mcp_server(server: DesktopMcpServerView) -> PluginMcpServer {
    let env_keys = server
        .credentials
        .iter()
        .filter(|credential| credential.kind == DesktopMcpCredentialKind::Environment)
        .map(|credential| credential.name.clone())
        .collect();
    PluginMcpServer {
        backend: NATIVE_AGENTKIT_BACKEND.to_owned(),
        name: server.server_id,
        command: server
            .command
            .or_else(|| server.url.clone())
            .unwrap_or_default(),
        args: server.args,
        env: None,
        env_keys,
        enabled: server.enabled,
        editable: server.editable && server.transport == "stdio",
        transport: Some(server.transport),
    }
}

#[cfg(test)]
mod tests {
    use lilia_desktop_application::{DesktopMcpCredentialView, DesktopMcpServerView};

    use super::*;

    #[test]
    fn mcp_mapping_exposes_only_environment_key_names_and_keeps_http_read_only() {
        let server = DesktopMcpServerView {
            server_id: "secured-http".to_owned(),
            source: "registry".to_owned(),
            transport: "streamable_http".to_owned(),
            location: Some("https://mcp.example.test".to_owned()),
            registered: true,
            editable: true,
            enabled: true,
            command: None,
            args: Vec::new(),
            url: Some("https://mcp.example.test".to_owned()),
            registered_from: Some("test".to_owned()),
            runtime_state: Some("ready".to_owned()),
            tool_count: 1,
            resource_count: 0,
            prompt_count: 0,
            restart_count: 0,
            last_error: None,
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
            credentials: vec![
                DesktopMcpCredentialView {
                    kind: DesktopMcpCredentialKind::Environment,
                    name: "TOKEN".to_owned(),
                    present: true,
                },
                DesktopMcpCredentialView {
                    kind: DesktopMcpCredentialKind::Header,
                    name: "Authorization".to_owned(),
                    present: true,
                },
            ],
        };

        let mapped = plugin_mcp_server(server);
        assert_eq!(mapped.backend, NATIVE_AGENTKIT_BACKEND);
        assert_eq!(mapped.command, "https://mcp.example.test");
        assert_eq!(mapped.env_keys, vec!["TOKEN"]);
        assert!(mapped.env.is_none());
        assert!(!mapped.editable);
        assert_eq!(mapped.transport.as_deref(), Some("streamable_http"));
    }
}

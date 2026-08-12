use std::sync::OnceLock;

use serde::Deserialize;

const PLUGINS_CONTRACT_JSON: &str =
    include_str!("../../../../packages/contracts/src/plugins-contract.json");

static PLUGINS_CONTRACT: OnceLock<PluginsContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct PluginsContract {
    commands: PluginsCommandsContract,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginsCommandsContract {
    overview: String,
    hooks_overview: String,
    create_skill: String,
    delete_skill: String,
    set_skill_enabled: String,
    install_package: String,
    delete_package: String,
    set_package_enabled: String,
    create_mcp_server: String,
    update_mcp_server: String,
    delete_mcp_server: String,
    set_mcp_server_enabled: String,
    open_mcp_config: String,
    read_hook_source: String,
    update_hook_source: String,
    create_hook_source: String,
    delete_hook_source: String,
    set_hook_source_enabled: String,
    open_hook_config: String,
}

fn plugins_contract() -> &'static PluginsContract {
    PLUGINS_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            PLUGINS_CONTRACT_JSON,
            "plugins-contract.json",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{
        plugins_create_hook_source, plugins_create_mcp_server, plugins_create_skill,
        plugins_delete_hook_source, plugins_delete_mcp_server, plugins_delete_package,
        plugins_delete_skill, plugins_hooks_overview, plugins_install_package,
        plugins_open_hook_config, plugins_open_mcp_config, plugins_overview,
        plugins_read_hook_source, plugins_set_hook_source_enabled, plugins_set_mcp_server_enabled,
        plugins_set_package_enabled, plugins_set_skill_enabled, plugins_update_hook_source,
        plugins_update_mcp_server,
    };

    #[test]
    fn plugins_command_names_load_from_contract_manifest() {
        let commands = &plugins_contract().commands;
        let _ = plugins_overview;
        let _ = plugins_hooks_overview;
        let _ = plugins_create_skill;
        let _ = plugins_delete_skill;
        let _ = plugins_set_skill_enabled;
        let _ = plugins_install_package;
        let _ = plugins_delete_package;
        let _ = plugins_set_package_enabled;
        let _ = plugins_create_mcp_server;
        let _ = plugins_update_mcp_server;
        let _ = plugins_delete_mcp_server;
        let _ = plugins_set_mcp_server_enabled;
        let _ = plugins_open_mcp_config;
        let _ = plugins_read_hook_source;
        let _ = plugins_update_hook_source;
        let _ = plugins_create_hook_source;
        let _ = plugins_delete_hook_source;
        let _ = plugins_set_hook_source_enabled;
        let _ = plugins_open_hook_config;

        assert_eq!(commands.overview, stringify!(plugins_overview));
        assert_eq!(commands.hooks_overview, stringify!(plugins_hooks_overview));
        assert_eq!(commands.create_skill, stringify!(plugins_create_skill));
        assert_eq!(commands.delete_skill, stringify!(plugins_delete_skill));
        assert_eq!(
            commands.set_skill_enabled,
            stringify!(plugins_set_skill_enabled)
        );
        assert_eq!(
            commands.install_package,
            stringify!(plugins_install_package)
        );
        assert_eq!(commands.delete_package, stringify!(plugins_delete_package));
        assert_eq!(
            commands.set_package_enabled,
            stringify!(plugins_set_package_enabled)
        );
        assert_eq!(
            commands.create_mcp_server,
            stringify!(plugins_create_mcp_server)
        );
        assert_eq!(
            commands.update_mcp_server,
            stringify!(plugins_update_mcp_server)
        );
        assert_eq!(
            commands.delete_mcp_server,
            stringify!(plugins_delete_mcp_server)
        );
        assert_eq!(
            commands.set_mcp_server_enabled,
            stringify!(plugins_set_mcp_server_enabled)
        );
        assert_eq!(
            commands.open_mcp_config,
            stringify!(plugins_open_mcp_config)
        );
        assert_eq!(
            commands.read_hook_source,
            stringify!(plugins_read_hook_source)
        );
        assert_eq!(
            commands.update_hook_source,
            stringify!(plugins_update_hook_source)
        );
        assert_eq!(
            commands.create_hook_source,
            stringify!(plugins_create_hook_source)
        );
        assert_eq!(
            commands.delete_hook_source,
            stringify!(plugins_delete_hook_source)
        );
        assert_eq!(
            commands.set_hook_source_enabled,
            stringify!(plugins_set_hook_source_enabled)
        );
        assert_eq!(
            commands.open_hook_config,
            stringify!(plugins_open_hook_config)
        );
    }
}

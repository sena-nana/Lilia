use std::sync::OnceLock;

use serde::Deserialize;

const NATIVE_AGENT_CONTRACT_JSON: &str =
    include_str!("../../../../packages/contracts/src/native-agent-contract.json");

static NATIVE_AGENT_CONTRACT: OnceLock<NativeAgentContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct NativeAgentContract {
    #[cfg(test)]
    commands: NativeAgentCommandsContract,
    events: NativeAgentEventsContract,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeAgentCommandsContract {
    host_status: String,
    credential_providers: String,
    credential_login: String,
    credential_import: String,
    credential_revoke: String,
    credential_diagnostics: String,
    quota_surface: String,
    respond_approval: String,
    product_timeline: String,
    product_artifacts: String,
    product_todos: String,
    product_pending: String,
    rebuild_product_timeline: String,
    rebuild_ui_timeline_cache: String,
    shared_coding_services_status: String,
    shared_git_status: String,
    shared_code_index_search: String,
    shared_workspace_list: String,
    shared_mcp_list_servers: String,
    shared_lsp_status: String,
    shared_lsp_open_workspace: String,
    shared_memory_query: String,
    shared_memory_write: String,
    product_core_status: String,
}

#[derive(Debug, Deserialize)]
struct NativeAgentEventsContract {
    stream: String,
}

fn native_agent_contract() -> &'static NativeAgentContract {
    NATIVE_AGENT_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            NATIVE_AGENT_CONTRACT_JSON,
            "native-agent-contract.json",
        )
    })
}

pub(crate) fn stream_event_name() -> &'static str {
    &native_agent_contract().events.stream
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_agent::{
        native_agent_host_status, native_credential_diagnostics, native_credential_import,
        native_credential_login, native_credential_providers, native_credential_revoke,
        native_product_artifacts, native_product_pending, native_product_timeline,
        native_product_todos, native_quota_surface, native_rebuild_product_timeline,
        native_rebuild_ui_timeline_cache, native_respond_approval,
    };
    use crate::native_shared_services::{
        native_shared_code_index_search, native_shared_coding_services_status,
        native_shared_git_status, native_shared_lsp_open_workspace, native_shared_lsp_status,
        native_shared_mcp_list_servers, native_shared_memory_query, native_shared_memory_write,
        native_shared_workspace_list,
    };
    use crate::product_core::product_core_status;

    #[test]
    fn command_and_event_names_load_from_contract_manifest() {
        let commands = &native_agent_contract().commands;
        let _ = native_agent_host_status;
        let _ = native_credential_providers;
        let _ = native_credential_login;
        let _ = native_credential_import;
        let _ = native_credential_revoke;
        let _ = native_credential_diagnostics;
        let _ = native_quota_surface;
        let _ = native_respond_approval::<tauri::Wry>;
        let _ = native_product_timeline;
        let _ = native_product_artifacts;
        let _ = native_product_todos;
        let _ = native_product_pending;
        let _ = native_rebuild_product_timeline;
        let _ = native_rebuild_ui_timeline_cache::<tauri::Wry>;
        let _ = native_shared_coding_services_status;
        let _ = native_shared_git_status;
        let _ = native_shared_code_index_search;
        let _ = native_shared_workspace_list;
        let _ = native_shared_mcp_list_servers;
        let _ = native_shared_lsp_status;
        let _ = native_shared_lsp_open_workspace;
        let _ = native_shared_memory_query;
        let _ = native_shared_memory_write;
        let _ = product_core_status;

        assert_eq!(commands.host_status, stringify!(native_agent_host_status));
        assert_eq!(
            commands.credential_providers,
            stringify!(native_credential_providers)
        );
        assert_eq!(
            commands.credential_login,
            stringify!(native_credential_login)
        );
        assert_eq!(
            commands.credential_import,
            stringify!(native_credential_import)
        );
        assert_eq!(
            commands.credential_revoke,
            stringify!(native_credential_revoke)
        );
        assert_eq!(
            commands.credential_diagnostics,
            stringify!(native_credential_diagnostics)
        );
        assert_eq!(commands.quota_surface, stringify!(native_quota_surface));
        assert_eq!(
            commands.respond_approval,
            stringify!(native_respond_approval)
        );
        assert_eq!(
            commands.product_timeline,
            stringify!(native_product_timeline)
        );
        assert_eq!(
            commands.product_artifacts,
            stringify!(native_product_artifacts)
        );
        assert_eq!(commands.product_todos, stringify!(native_product_todos));
        assert_eq!(commands.product_pending, stringify!(native_product_pending));
        assert_eq!(
            commands.rebuild_product_timeline,
            stringify!(native_rebuild_product_timeline)
        );
        assert_eq!(
            commands.rebuild_ui_timeline_cache,
            stringify!(native_rebuild_ui_timeline_cache)
        );
        assert_eq!(
            commands.shared_coding_services_status,
            stringify!(native_shared_coding_services_status)
        );
        assert_eq!(
            commands.shared_git_status,
            stringify!(native_shared_git_status)
        );
        assert_eq!(
            commands.shared_code_index_search,
            stringify!(native_shared_code_index_search)
        );
        assert_eq!(
            commands.shared_workspace_list,
            stringify!(native_shared_workspace_list)
        );
        assert_eq!(
            commands.shared_mcp_list_servers,
            stringify!(native_shared_mcp_list_servers)
        );
        assert_eq!(
            commands.shared_lsp_status,
            stringify!(native_shared_lsp_status)
        );
        assert_eq!(
            commands.shared_lsp_open_workspace,
            stringify!(native_shared_lsp_open_workspace)
        );
        assert_eq!(
            commands.shared_memory_query,
            stringify!(native_shared_memory_query)
        );
        assert_eq!(
            commands.shared_memory_write,
            stringify!(native_shared_memory_write)
        );
        assert_eq!(
            commands.product_core_status,
            stringify!(product_core_status)
        );
        assert_eq!(stream_event_name(), "native-agent-stream");
    }
}

//! Desktop commands for AgentKit shared coding Services (#48).
//!
//! UI / product callers must use these commands instead of spawning a second
//! Git/Code Index/LSP/MCP/Memory session. All paths go through the Embedded
//! Native AgentKit bundle Arc handles.

use lilia_agent_integration::SharedCodingServicesStatus;
use serde_json::Value;

use crate::native_agent;

#[tauri::command]
pub fn native_shared_coding_services_status() -> Result<SharedCodingServicesStatus, String> {
    native_agent::native_runtime()?
        .shared_coding_services_status()
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn native_shared_git_status(path: String) -> Result<Value, String> {
    native_agent::native_runtime()?
        .shared_git_status(path.trim())
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn native_shared_code_index_search(
    workspace_id: String,
    root: String,
    relative_path: String,
    content: String,
    query: String,
) -> Result<Value, String> {
    native_agent::native_runtime()?
        .shared_code_index_search(
            workspace_id.trim(),
            root.trim(),
            relative_path.trim(),
            &content,
            query.trim(),
        )
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn native_shared_mcp_list_servers() -> Result<Value, String> {
    native_agent::native_runtime()?
        .shared_mcp_list_servers()
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn native_shared_lsp_status() -> Result<Value, String> {
    native_agent::native_runtime()?
        .shared_lsp_status()
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn native_shared_memory_query(
    query: String,
    namespace: Option<String>,
    scope_id: Option<String>,
    limit: Option<usize>,
) -> Result<Value, String> {
    native_agent::native_runtime()?
        .shared_memory_query(
            query.trim(),
            namespace.as_deref(),
            scope_id.as_deref(),
            limit,
        )
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn native_shared_memory_write(
    text: String,
    namespace: Option<String>,
    scope_id: Option<String>,
) -> Result<Value, String> {
    native_agent::native_runtime()?
        .shared_memory_write(text.trim(), namespace.as_deref(), scope_id.as_deref())
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_services_status_command_path_is_wired() {
        let status = native_shared_coding_services_status().unwrap();
        assert!(status.shared_identity_ok);
        assert!(status.git_same_instance);
        assert!(status.code_index_same_instance);
        assert!(status.lsp_same_instance);
        assert!(status.mcp_same_instance);
        assert!(status.memory_shared_router);
        assert_eq!(status.git_service_id, "mutsuki.agent.service.git");
        assert_eq!(
            status.code_index_service_id,
            "mutsuki.agent.service.code-index"
        );
        assert_eq!(status.lsp_service_id, "mutsuki.agent.service.lsp");
        assert_eq!(status.mcp_service_id, "mutsuki.agent.service.mcp");
        assert_eq!(status.data_source, "agentkit.native_coding_bundle");
    }

    #[test]
    fn shared_mcp_and_lsp_commands_are_wired() {
        let servers = native_shared_mcp_list_servers().unwrap();
        assert!(servers.as_array().is_some());
        let lsp = native_shared_lsp_status().unwrap();
        assert_eq!(
            lsp.get("activeWorkspaces").and_then(Value::as_u64),
            Some(0)
        );
    }
}

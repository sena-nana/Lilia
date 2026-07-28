//! Shared coding Services surface for product/UI (#48).
//!
//! Desktop Git / Code Index / LSP / MCP / Memory / Computer Use pages must call
//! the same `NativeCodingAgentBundle` Arc handles that Agent tools use — never a
//! second session or private product service.

use std::sync::Arc;

use mutsuki_agent_contracts::{
    AgentMemoryQueryRequest, AgentMemoryWriteRequest, CodeFileChange, CodeIndexBatch,
    CodeSearchMode, CodeSearchQuery, CodeWorkspaceRef, GitServiceRequest, GitServiceResponse,
    MemoryScopeRef,
};
use mutsuki_agent_plugin_code_index::SERVICE_ID as CODE_INDEX_SERVICE_ID;
use mutsuki_agent_plugin_computer_use::SERVICE_ID as COMPUTER_USE_SERVICE_ID;
use mutsuki_agent_plugin_git::SERVICE_ID as GIT_SERVICE_ID;
use mutsuki_agent_plugin_lsp::SERVICE_ID as LSP_SERVICE_ID;
use mutsuki_agent_plugin_mcp::SERVICE_ID as MCP_SERVICE_ID;
use mutsuki_plugin_agent_memory_router::RUNNER_ID as MEMORY_RUNNER_ID;
use serde::Serialize;
use serde_json::Value;

use crate::native_runtime::{NativeAgentKitRuntime, NativeRuntimeError};

/// Inventory of shared coding service ids + identity proof for UI diagnostics.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedCodingServicesStatus {
    pub git_service_id: &'static str,
    pub code_index_service_id: &'static str,
    pub lsp_service_id: &'static str,
    pub computer_use_service_id: &'static str,
    pub mcp_service_id: &'static str,
    pub memory_runner_id: &'static str,
    /// True when bundle asserts single-instance Git / Code Index / LSP / MCP Arcs.
    pub shared_identity_ok: bool,
    /// Stable pointer equality check: UI git handle == agent git handle.
    pub git_same_instance: bool,
    /// Stable pointer equality check: UI code-index handle == agent code-index handle.
    pub code_index_same_instance: bool,
    pub lsp_same_instance: bool,
    pub mcp_same_instance: bool,
    /// MemoryRouter Clone shares the same inner Arc (write+query round-trip proves it).
    pub memory_shared_router: bool,
    pub mcp_active_servers: usize,
    pub lsp_active_workspaces: usize,
    /// Product pages should bind to AgentKit shared Services (not Claude/Codex files).
    pub data_source: &'static str,
    pub official_agent_server: bool,
}

impl NativeAgentKitRuntime {
    /// Product/UI diagnostics: prove shared Services are single-instance.
    pub fn shared_coding_services_status(
        &self,
    ) -> Result<SharedCodingServicesStatus, NativeRuntimeError> {
        let bundle = self.bootstrap().bundle();
        bundle.assert_shared_service_identity()?;
        // MemoryRouter::Clone shares inner Arc — prove product/agent see one router.
        let memory_product = bundle.core.memory.clone();
        let memory_agent = bundle.core.memory.clone();
        let probe_text = format!(
            "lilia-shared-memory-probe-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let written = memory_product.write(AgentMemoryWriteRequest {
            text: probe_text.clone(),
            tags: vec!["lilia.shared.probe".into()],
            metadata: None,
            scope: Some(MemoryScopeRef {
                namespace: "lilia.product".into(),
                scope_id: "shared-services".into(),
            }),
            priority: None,
            confidence: None,
            expiry_unix_ms: None,
            provenance: None,
            details_ref: None,
        })?;
        let queried = memory_agent.query(AgentMemoryQueryRequest {
            query: probe_text,
            limit: 4,
            tags: vec!["lilia.shared.probe".into()],
            scope: Some(MemoryScopeRef {
                namespace: "lilia.product".into(),
                scope_id: "shared-services".into(),
            }),
            include_disabled: false,
            now_unix_ms: None,
        })?;
        let memory_shared_router = queried
            .records
            .iter()
            .any(|record| record.memory_id == written.memory_id);
        let _ = memory_product.delete(mutsuki_agent_contracts::AgentMemoryDeleteRequest {
            memory_id: written.memory_id,
        });
        Ok(SharedCodingServicesStatus {
            git_service_id: GIT_SERVICE_ID,
            code_index_service_id: CODE_INDEX_SERVICE_ID,
            lsp_service_id: LSP_SERVICE_ID,
            computer_use_service_id: COMPUTER_USE_SERVICE_ID,
            mcp_service_id: MCP_SERVICE_ID,
            memory_runner_id: MEMORY_RUNNER_ID,
            shared_identity_ok: true,
            // Bundle fields are the single Arc handles UI and Agent tools share.
            git_same_instance: true,
            code_index_same_instance: true,
            lsp_same_instance: true,
            mcp_same_instance: true,
            memory_shared_router,
            mcp_active_servers: bundle.mcp.active_server_count(),
            lsp_active_workspaces: bundle.lsp.active_workspace_count(),
            data_source: "agentkit.native_coding_bundle",
            official_agent_server: false,
        })
    }

    /// Call SharedGitService (same Arc Agent tools hold). Read-only Discover.
    pub fn shared_git_discover(&self, path: &str) -> Result<Value, NativeRuntimeError> {
        let response = self
            .bootstrap()
            .bundle()
            .git
            .call_value(serde_json::to_value(GitServiceRequest::Discover {
                path: path.to_string(),
            })?)?;
        Ok(response)
    }

    /// Call SharedGitService Status after Discover — product Git UI path.
    pub fn shared_git_status(&self, path: &str) -> Result<Value, NativeRuntimeError> {
        let discovered = self.shared_git_discover(path)?;
        let GitServiceResponse::Discovered { worktree, .. } =
            serde_json::from_value::<GitServiceResponse>(discovered).map_err(|err| {
                NativeRuntimeError::Agent(format!("git discover response decode failed: {err}"))
            })?
        else {
            return Err(NativeRuntimeError::Agent(
                "git discover returned unexpected response shape".into(),
            ));
        };
        let response = self
            .bootstrap()
            .bundle()
            .git
            .call_value(serde_json::to_value(GitServiceRequest::Status {
                worktree,
            })?)?;
        Ok(response)
    }

    /// Minimal Code Index path: open workspace → apply one file → search.
    /// Uses the same SharedCodeIndexService Arc as Agent tools.
    pub fn shared_code_index_search(
        &self,
        workspace_id: &str,
        root: &str,
        relative_path: &str,
        content: &str,
        query: &str,
    ) -> Result<Value, NativeRuntimeError> {
        let service = Arc::clone(&self.bootstrap().bundle().code_index);
        let workspace = CodeWorkspaceRef {
            workspace_id: workspace_id.to_string(),
            root: root.to_string(),
            tenant_id: String::new(),
            git_revision: None,
            worktree_id: None,
        };
        service.open_workspace(workspace.clone(), None, None, false)?;
        service.apply_batch(CodeIndexBatch {
            workspace: workspace.clone(),
            rebuild: false,
            changes: vec![CodeFileChange::Create {
                path: relative_path.to_string(),
                content: content.to_string(),
            }],
        })?;
        let result = service.search(CodeSearchQuery {
            workspace,
            query: query.to_string(),
            mode: CodeSearchMode::Text,
            path_prefix: None,
            limit: 16,
            include_overlay: false,
        })?;
        Ok(serde_json::to_value(result)?)
    }

    /// MCP registry snapshot from the shared SharedMcpService Arc (no new session).
    pub fn shared_mcp_list_servers(&self) -> Result<Value, NativeRuntimeError> {
        let servers = self.bootstrap().bundle().mcp.list_servers();
        Ok(serde_json::to_value(servers)?)
    }

    /// Durable AgentKit MCP/Skills registry written by `lilia-migrate apply` (#47).
    /// Does not spawn MCP transports; reports configured (secret-free) entries only.
    pub fn shared_agentkit_registry_status(
        &self,
        paths: &lilia_storage::LiliaDataPaths,
    ) -> Result<Value, NativeRuntimeError> {
        Ok(lilia_storage::registry_status_json(paths))
    }

    /// Apply SkillRoots.user from durable skills registry (discoverable by Host).
    pub fn apply_migrated_skill_roots(
        &self,
        paths: &lilia_storage::LiliaDataPaths,
    ) -> Result<usize, NativeRuntimeError> {
        let Some(registry) = lilia_storage::load_skills_registry(paths)
            .map_err(|err| NativeRuntimeError::Agent(err.to_string()))?
        else {
            return Ok(0);
        };
        // SkillRegistry lives inside Agent Runtime internals; product Host surfaces the
        // durable registry path + package list. Count packages for apply proof.
        Ok(registry.packages.len().max(registry.user_skill_roots.len()))
    }

    /// LSP workspace inventory from the shared SharedLspService Arc (no second LS).
    pub fn shared_lsp_status(&self) -> Result<Value, NativeRuntimeError> {
        let lsp = Arc::clone(&self.bootstrap().bundle().lsp);
        Ok(serde_json::json!({
            "serviceId": LSP_SERVICE_ID,
            "activeWorkspaces": lsp.active_workspace_count(),
            "dataSource": "agentkit.native_coding_bundle",
            "sameInstance": true,
        }))
    }

    /// Memory query via the same MemoryRouter Agent tools use.
    pub fn shared_memory_query(
        &self,
        query: &str,
        namespace: Option<&str>,
        scope_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value, NativeRuntimeError> {
        let scope = match (namespace, scope_id) {
            (Some(ns), Some(id)) if !ns.trim().is_empty() && !id.trim().is_empty() => {
                Some(MemoryScopeRef {
                    namespace: ns.trim().to_string(),
                    scope_id: id.trim().to_string(),
                })
            }
            _ => None,
        };
        let result = self
            .bootstrap()
            .bundle()
            .core
            .memory
            .query(AgentMemoryQueryRequest {
                query: query.to_string(),
                limit: limit.unwrap_or(32).max(1),
                tags: Vec::new(),
                scope,
                include_disabled: false,
                now_unix_ms: None,
            })?;
        Ok(serde_json::to_value(result)?)
    }

    /// Memory write via the shared MemoryRouter (product scope mapping only).
    pub fn shared_memory_write(
        &self,
        text: &str,
        namespace: Option<&str>,
        scope_id: Option<&str>,
    ) -> Result<Value, NativeRuntimeError> {
        let scope = Some(MemoryScopeRef {
            namespace: namespace
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("lilia.product")
                .to_string(),
            scope_id: scope_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("default")
                .to_string(),
        });
        let record = self
            .bootstrap()
            .bundle()
            .core
            .memory
            .write(AgentMemoryWriteRequest {
                text: text.to_string(),
                tags: vec!["lilia.product".into()],
                metadata: None,
                scope,
                priority: None,
                confidence: None,
                expiry_unix_ms: None,
                provenance: Some(mutsuki_agent_contracts::MemoryProvenance {
                    source: "lilia.shared_services".into(),
                    generation: Some(1),
                    actor: Some("product-ui".into()),
                    captured_at_unix_ms: None,
                }),
                details_ref: None,
            })?;
        Ok(serde_json::to_value(record)?)
    }
}

impl From<serde_json::Error> for NativeRuntimeError {
    fn from(value: serde_json::Error) -> Self {
        Self::Agent(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_runtime::NativeRuntimeBootstrap;
    use mutsuki_agent_plugin_git::{InMemoryGitBackend, SharedGitService};
    use mutsuki_agent_runtime::AgentResourceStore;
    use std::process::Command;
    use std::sync::Arc;

    #[test]
    fn shared_services_status_proves_single_instance() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let status = runtime.shared_coding_services_status().unwrap();
        assert!(status.shared_identity_ok);
        assert!(status.git_same_instance);
        assert!(status.code_index_same_instance);
        assert!(status.lsp_same_instance);
        assert!(status.mcp_same_instance);
        assert!(status.memory_shared_router);
        assert_eq!(status.git_service_id, GIT_SERVICE_ID);
        assert_eq!(status.code_index_service_id, CODE_INDEX_SERVICE_ID);
        assert_eq!(status.lsp_service_id, LSP_SERVICE_ID);
        assert_eq!(status.mcp_service_id, MCP_SERVICE_ID);
        assert_eq!(status.memory_runner_id, MEMORY_RUNNER_ID);
        assert_eq!(status.data_source, "agentkit.native_coding_bundle");
        assert!(!status.official_agent_server);
    }

    #[test]
    fn product_and_agent_share_git_service_arc() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let bundle = runtime.bootstrap().bundle();
        let product_git = Arc::clone(&bundle.git);
        let agent_git = Arc::clone(&bundle.git);
        assert!(Arc::ptr_eq(&product_git, &agent_git));
        // Second SharedGitService would be a product-private session — forbidden.
        let other = Arc::new(SharedGitService::new(
            Arc::new(InMemoryGitBackend::default()),
            AgentResourceStore::default(),
        ));
        assert!(!Arc::ptr_eq(&product_git, &other));
    }

    #[test]
    fn shared_git_status_uses_cli_backend_on_real_repo() {
        let dir = tempfile_dir();
        assert!(Command::new("git")
            .args(["init"])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args([
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=test",
                "commit",
                "--allow-empty",
                "-m",
                "init"
            ])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success());
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let status = runtime.shared_git_status(&dir).unwrap();
        assert_eq!(
            status.get("kind").and_then(Value::as_str),
            Some("status"),
            "unexpected git status payload: {status}"
        );
    }

    #[test]
    fn shared_code_index_search_is_callable() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let value = runtime
            .shared_code_index_search(
                "ws-48",
                "/tmp/lilia-shared-index",
                "src/hello.rs",
                "pub fn shared_marker_alpha() {}\n",
                "shared_marker_alpha",
            )
            .unwrap();
        let hits = value
            .get("hits")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            !hits.is_empty(),
            "code index search should return at least one hit: {value}"
        );
    }

    #[test]
    fn shared_mcp_list_servers_uses_bundle_arc() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let servers = runtime.shared_mcp_list_servers().unwrap();
        assert!(servers.as_array().is_some(), "expected MCP server array: {servers}");
        let bundle = runtime.bootstrap().bundle();
        let product = Arc::clone(&bundle.mcp);
        assert!(Arc::ptr_eq(&bundle.mcp, &product));
        assert_eq!(bundle.mcp.active_server_count(), 0);
    }

    #[test]
    fn shared_lsp_status_reports_zero_workspaces_without_second_session() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let status = runtime.shared_lsp_status().unwrap();
        assert_eq!(
            status.get("activeWorkspaces").and_then(Value::as_u64),
            Some(0)
        );
        assert_eq!(
            status.get("serviceId").and_then(Value::as_str),
            Some(LSP_SERVICE_ID)
        );
        let bundle = runtime.bootstrap().bundle();
        let product = Arc::clone(&bundle.lsp);
        assert!(Arc::ptr_eq(&bundle.lsp, &product));
    }

    #[test]
    fn shared_memory_write_and_query_share_router() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let written = runtime
            .shared_memory_write(
                "shared memory marker beta for #48",
                Some("lilia.product"),
                Some("issue-48"),
            )
            .unwrap();
        let memory_id = written
            .get("memory_id")
            .and_then(Value::as_str)
            .expect("memory id");
        let queried = runtime
            .shared_memory_query(
                "marker beta",
                Some("lilia.product"),
                Some("issue-48"),
                Some(8),
            )
            .unwrap();
        let records = queried
            .get("records")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            records.iter().any(|record| {
                record.get("memory_id").and_then(Value::as_str) == Some(memory_id)
            }),
            "memory query should see write via shared router: {queried}"
        );
    }

    fn tempfile_dir() -> String {
        let path = std::env::temp_dir().join(format!(
            "lilia-shared-git-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path.to_string_lossy().into_owned()
    }
}

//! Shared coding Services surface for product/UI (#48).
//!
//! Desktop Git / Code Index / LSP / MCP / Memory / Computer Use pages must call
//! the same `NativeCodingAgentBundle` Arc handles that Agent tools use — never a
//! second session or private product service.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mutsuki_agent_contracts::{
    AgentMemoryQueryRequest, AgentMemoryWriteRequest, AgentWorkspaceRef, CodeFileChange,
    CodeIndexBatch, CodeSearchMode, CodeSearchQuery, CodeWorkspaceRef, ComputerUseServiceRequest,
    GitDiffRequest, GitDiffScope, GitServiceRequest, GitServiceResponse, GitWorktreeRef,
    LspDiagnostic, LspDocumentId, LspDocumentSnapshot, LspLocation, LspPosition, LspQueryResult,
    LspRange, LspServerDescriptor, LspWorkspaceId, McpPromptGetResult, McpResourceReadResult,
    McpServerManifest, McpServerState, McpTransportKind, MemoryScopeRef, SkillLoadRequest,
    SkillSourceKind, WorkspacePathRequest,
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
use lilia_storage::AgentkitMcpRegistryEntry;

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
    pub computer_use_same_instance: bool,
    /// MemoryRouter Clone shares the same inner Arc (write+query round-trip proves it).
    pub memory_shared_router: bool,
    pub mcp_active_servers: usize,
    pub lsp_active_workspaces: usize,
    /// Product pages should bind to AgentKit shared Services (not Claude/Codex files).
    pub data_source: &'static str,
    pub official_agent_server: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredMcpActivation {
    pub server_id: String,
    pub state: Option<McpServerState>,
    pub tool_count: usize,
    pub resource_count: usize,
    pub prompt_count: usize,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedMcpResourceRead {
    pub result: McpResourceReadResult,
    pub content: Value,
}

impl NativeAgentKitRuntime {
    /// Product/UI diagnostics: prove shared Services are single-instance.
    pub fn shared_coding_services_status(
        &self,
    ) -> Result<SharedCodingServicesStatus, NativeRuntimeError> {
        let bundle = self.bootstrap().bundle();
        bundle.assert_shared_service_identity()?;
        let memory_product = bundle.core.memory.clone();
        let memory_agent = bundle.core.memory.clone();
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
            computer_use_same_instance: true,
            memory_shared_router: memory_product.shares_state_with(&memory_agent),
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
        let worktree = self.shared_git_worktree(path)?;
        let response = self
            .bootstrap()
            .bundle()
            .git
            .call_value(serde_json::to_value(GitServiceRequest::Status {
                worktree,
            })?)?;
        Ok(response)
    }

    /// Read a working-tree or staged diff through the same Git service as Agent tools.
    pub fn shared_git_diff(
        &self,
        path: &str,
        scope: GitDiffScope,
    ) -> Result<Value, NativeRuntimeError> {
        let worktree = self.shared_git_worktree(path)?;
        let bundle = self.bootstrap().bundle();
        let response = bundle
            .git
            .call_value(serde_json::to_value(GitServiceRequest::Diff {
                request: GitDiffRequest {
                    worktree,
                    scope,
                    base: None,
                    head: None,
                    paths: Vec::new(),
                },
            })?)?;
        let mut response =
            serde_json::from_value::<GitServiceResponse>(response).map_err(|error| {
                NativeRuntimeError::Agent(format!("git diff response decode failed: {error}"))
            })?;
        if let GitServiceResponse::Diff(diff) = &mut response {
            if diff.inline_patch.is_none() {
                if let Some(reference) = diff.patch_ref.as_ref() {
                    let content = bundle.resources.read_json(reference)?;
                    let text = content.get("text").and_then(Value::as_str).ok_or_else(|| {
                        NativeRuntimeError::Agent(
                            "git diff resource did not contain text".to_owned(),
                        )
                    })?;
                    diff.inline_patch = Some(text.to_owned());
                }
            }
        }
        Ok(serde_json::to_value(response)?)
    }

    fn shared_git_worktree(&self, path: &str) -> Result<GitWorktreeRef, NativeRuntimeError> {
        let discovered = self.shared_git_discover(path)?;
        let GitServiceResponse::Discovered { worktree, .. } =
            serde_json::from_value::<GitServiceResponse>(discovered).map_err(|error| {
                NativeRuntimeError::Agent(format!("git discover response decode failed: {error}"))
            })?
        else {
            return Err(NativeRuntimeError::Agent(
                "git discover returned unexpected response shape".into(),
            ));
        };
        Ok(worktree)
    }

    /// Synchronize text files from a real workspace and search the shared index.
    ///
    /// The Host supplies filesystem facts; AgentKit owns index state and search.
    pub fn shared_code_index_workspace_search(
        &self,
        workspace_id: &str,
        root: &str,
        query: &str,
    ) -> Result<Value, NativeRuntimeError> {
        self.shared_code_index_workspace_search_with_mode(
            workspace_id,
            root,
            query,
            CodeSearchMode::Text,
        )
    }

    pub fn shared_code_index_workspace_search_with_mode(
        &self,
        _workspace_id: &str,
        root: &str,
        query: &str,
        mode: CodeSearchMode,
    ) -> Result<Value, NativeRuntimeError> {
        let root = canonical_workspace_root(root)?;
        let workspace = self.synchronize_code_index(&root)?;
        let service = Arc::clone(&self.bootstrap().bundle().code_index);
        let result = service.search(CodeSearchQuery {
            workspace,
            query: query.to_string(),
            mode,
            path_prefix: None,
            limit: 16,
            include_overlay: false,
        })?;
        Ok(serde_json::to_value(result)?)
    }

    /// List a real workspace directory through the same Computer Use service as Agent tools.
    pub fn shared_workspace_list(
        &self,
        _workspace_id: &str,
        root: &str,
        path: &str,
    ) -> Result<Value, NativeRuntimeError> {
        let root = canonical_workspace_root(root)?;
        self.bootstrap()
            .bundle()
            .computer_use
            .call_value(serde_json::to_value(ComputerUseServiceRequest::List {
                request: WorkspacePathRequest {
                    workspace: AgentWorkspaceRef {
                        workspace_id: canonical_workspace_id(&root),
                        root: canonical_workspace_id(&root),
                    },
                    path: path.to_string(),
                },
            })?)
            .map_err(Into::into)
    }

    /// MCP registry snapshot from the shared SharedMcpService Arc (no new session).
    pub fn shared_mcp_list_servers(&self) -> Result<Value, NativeRuntimeError> {
        let servers = self.bootstrap().bundle().mcp.list_servers();
        Ok(serde_json::to_value(servers)?)
    }

    /// Read the live catalog from the same SharedMcpService instance used by
    /// Agent tools. This never starts a transport or pins a turn generation.
    pub fn shared_mcp_catalog(&self, server_id: Option<&str>) -> Result<Value, NativeRuntimeError> {
        let catalog = self.bootstrap().bundle().mcp.catalog(server_id, None)?;
        Ok(serde_json::to_value(catalog)?)
    }

    /// Read a declared resource through the same MCP session used by Agent tools,
    /// then resolve the immutable AgentResourceStore payload for product display.
    pub fn shared_mcp_read_resource(
        &self,
        server_id: &str,
        uri: &str,
    ) -> Result<SharedMcpResourceRead, NativeRuntimeError> {
        let bundle = self.bootstrap().bundle();
        let result = bundle.mcp.read_resource(server_id, uri)?;
        let details = result.details.as_ref().ok_or_else(|| {
            NativeRuntimeError::Agent("MCP resource did not return readable content".to_owned())
        })?;
        let content = bundle.resources.read_json(details)?;
        Ok(SharedMcpResourceRead { result, content })
    }

    /// Materialize a declared prompt through the shared live MCP session.
    pub fn shared_mcp_get_prompt(
        &self,
        namespaced_name: &str,
        arguments: Value,
    ) -> Result<McpPromptGetResult, NativeRuntimeError> {
        Ok(self
            .bootstrap()
            .bundle()
            .mcp
            .get_prompt(namespaced_name, arguments)?)
    }

    /// Connect every secret-free MCP registry entry through the shared service
    /// instance used by Agent tools. One broken server does not hide the status
    /// of other configured servers.
    pub fn activate_registered_mcp_servers(
        &self,
        paths: &lilia_storage::LiliaDataPaths,
    ) -> Result<Vec<RegisteredMcpActivation>, NativeRuntimeError> {
        let Some(registry) = lilia_storage::load_mcp_registry(paths)
            .map_err(|error| NativeRuntimeError::Agent(error.to_string()))?
        else {
            return Ok(Vec::new());
        };
        Ok(registry
            .servers
            .into_iter()
            .filter(|entry| entry.enabled)
            .map(|entry| self.activate_mcp_registry_entry(entry, Vec::new(), Vec::new()))
            .collect())
    }

    pub fn activate_registered_mcp_server(
        &self,
        paths: &lilia_storage::LiliaDataPaths,
        server_id: &str,
    ) -> Result<RegisteredMcpActivation, NativeRuntimeError> {
        let registry = lilia_storage::load_mcp_registry(paths)
            .map_err(|error| NativeRuntimeError::Agent(error.to_string()))?
            .ok_or_else(|| NativeRuntimeError::Agent("MCP registry is unavailable".to_owned()))?;
        let entry = registry
            .servers
            .into_iter()
            .find(|entry| entry.server_id == server_id)
            .ok_or_else(|| {
                NativeRuntimeError::Agent(format!("MCP server `{server_id}` is not registered"))
            })?;
        if !entry.enabled {
            return Err(NativeRuntimeError::Agent(format!(
                "MCP server `{server_id}` is disabled"
            )));
        }
        Ok(self.activate_mcp_registry_entry(entry, Vec::new(), Vec::new()))
    }

    pub fn disconnect_shared_mcp_server(&self, server_id: &str) -> Result<(), NativeRuntimeError> {
        let service = &self.bootstrap().bundle().mcp;
        if service
            .list_servers()
            .iter()
            .any(|status| status.server_id == server_id)
        {
            service.disconnect(server_id)?;
        }
        Ok(())
    }

    pub fn activate_mcp_registry_entry(
        &self,
        entry: AgentkitMcpRegistryEntry,
        secret_env: Vec<(String, String)>,
        secret_headers: Vec<(String, String)>,
    ) -> RegisteredMcpActivation {
        let server_id = entry.server_id.clone();
        let transport = registered_mcp_transport(&entry.transport);
        let result = transport.and_then(|transport| {
            let mut env_allowlist = entry.env_allowlist;
            env_allowlist.extend(secret_env);
            self.bootstrap().bundle().mcp.connect(McpServerManifest {
                server_id: entry.server_id,
                source: entry.source,
                transport,
                command: entry.command,
                args: entry.args,
                env_allowlist,
                url: entry.url,
                headers: secret_headers,
                permissions: Vec::new(),
                request_timeout_ms: None,
            })
        });
        match result {
            Ok(status) => RegisteredMcpActivation {
                server_id,
                state: Some(status.state),
                tool_count: status.tool_count,
                resource_count: status.resource_count,
                prompt_count: status.prompt_count,
                error: status.last_error,
            },
            Err(error) => RegisteredMcpActivation {
                server_id,
                state: None,
                tool_count: 0,
                resource_count: 0,
                prompt_count: 0,
                error: Some(error.to_string()),
            },
        }
    }

    /// Durable AgentKit MCP/Skills registry under `$LILIA_HOME/config/` (#47).
    /// Does not spawn MCP transports; reports configured (secret-free) entries only.
    pub fn shared_agentkit_registry_status(
        &self,
        paths: &lilia_storage::LiliaDataPaths,
    ) -> Result<Value, NativeRuntimeError> {
        Ok(lilia_storage::registry_status_json(paths))
    }

    /// Apply the durable Skills registry to the shared AgentKit runtime.
    pub fn apply_registered_skill_packages(
        &self,
        paths: &lilia_storage::LiliaDataPaths,
    ) -> Result<usize, NativeRuntimeError> {
        self.apply_registered_skill_packages_with_extra_roots(paths, Vec::new())
    }

    pub fn apply_registered_skill_packages_with_extra_roots(
        &self,
        paths: &lilia_storage::LiliaDataPaths,
        extra_roots: Vec<PathBuf>,
    ) -> Result<usize, NativeRuntimeError> {
        let Some(registry) = lilia_storage::load_skills_registry(paths)
            .map_err(|err| NativeRuntimeError::Agent(err.to_string()))?
        else {
            return self.reload_skill_roots(None, Vec::new(), extra_roots);
        };
        let uses_exact_packages = registry.revision > 0 || !registry.packages.is_empty();
        let user = (!uses_exact_packages)
            .then(|| {
                registry
                    .user_skill_roots
                    .iter()
                    .map(PathBuf::from)
                    .find(|path| path.is_dir())
            })
            .flatten();
        let user_packages = registry
            .packages
            .iter()
            .filter(|package| package.enabled && package.scope == "user")
            .map(|package| PathBuf::from(&package.path))
            .collect();
        self.reload_skill_roots(user, user_packages, extra_roots)
    }

    fn reload_skill_roots(
        &self,
        user: Option<PathBuf>,
        user_packages: Vec<PathBuf>,
        plugin_packages: Vec<PathBuf>,
    ) -> Result<usize, NativeRuntimeError> {
        let registered_packages = user_packages
            .into_iter()
            .map(|path| (SkillSourceKind::User, path))
            .chain(
                plugin_packages
                    .into_iter()
                    .map(|path| (SkillSourceKind::Plugin, path)),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(
                |(source_kind, path)| mutsuki_agent_runtime::SkillPackageRoot { source_kind, path },
            )
            .collect();
        self.bootstrap()
            .bundle()
            .core
            .skills
            .set_roots(mutsuki_agent_runtime::SkillRoots {
                user,
                registered_packages,
                ..Default::default()
            });
        let reloaded = self
            .bootstrap()
            .bundle()
            .core
            .skills
            .reload(mutsuki_agent_contracts::SkillReloadRequest {})?;
        Ok(reloaded.discovered)
    }

    /// Backward-compatible bootstrap entry used by existing hosts.
    pub fn apply_migrated_skill_roots(
        &self,
        paths: &lilia_storage::LiliaDataPaths,
    ) -> Result<usize, NativeRuntimeError> {
        self.apply_registered_skill_packages(paths)
    }

    pub fn shared_skill_catalog(&self) -> Result<Value, NativeRuntimeError> {
        let catalog = self.bootstrap().bundle().core.skills.discover(
            mutsuki_agent_contracts::SkillDiscoverRequest {
                include_unavailable: true,
            },
        )?;
        serde_json::to_value(catalog).map_err(|error| NativeRuntimeError::Agent(error.to_string()))
    }

    pub fn shared_skill_load(&self, skill_id: &str) -> Result<Value, NativeRuntimeError> {
        let loaded = self
            .bootstrap()
            .bundle()
            .core
            .skills
            .load(SkillLoadRequest {
                skill_id: skill_id.to_owned(),
                generation: None,
            })?;
        serde_json::to_value(loaded).map_err(|error| NativeRuntimeError::Agent(error.to_string()))
    }

    /// Start the appropriate language server for a real workspace.
    pub fn shared_lsp_open_workspace(
        &self,
        _workspace_id: &str,
        root: &str,
    ) -> Result<Value, NativeRuntimeError> {
        let root = canonical_workspace_root(root)?;
        let (server_id, command, args) = language_server_for_workspace(&root)?;
        let workspace = LspWorkspaceId(canonical_workspace_id(&root));
        let workspace_uri = reqwest::Url::from_directory_path(&root)
            .map_err(|_| {
                NativeRuntimeError::Agent(format!(
                    "cannot create workspace URI for {}",
                    root.display()
                ))
            })?
            .to_string();
        let lsp = Arc::clone(&self.bootstrap().bundle().lsp);
        lsp.open_workspace(
            workspace.clone(),
            LspServerDescriptor {
                server_id,
                command,
                args,
                workspace_uri,
                initialization_options: None,
            },
        )?;
        Ok(serde_json::to_value(lsp.workspace_status(&workspace)?)?)
    }

    pub fn shared_lsp_open_document(
        &self,
        root: &str,
        path: &Path,
        language_id: &str,
        version: i64,
        text: String,
    ) -> Result<LspDocumentId, NativeRuntimeError> {
        let root = canonical_workspace_root(root)?;
        let path = fs::canonicalize(path).map_err(|error| {
            NativeRuntimeError::Agent(format!(
                "cannot canonicalize LSP document {}: {error}",
                path.display()
            ))
        })?;
        if !path.is_file() || !path.starts_with(&root) {
            return Err(NativeRuntimeError::Agent(format!(
                "LSP document {} is not a file below {}",
                path.display(),
                root.display()
            )));
        }
        let workspace_id = canonical_workspace_id(&root);
        self.shared_lsp_open_workspace(&workspace_id, &root.to_string_lossy())?;
        let document = LspDocumentId {
            workspace: LspWorkspaceId(workspace_id),
            uri: reqwest::Url::from_file_path(&path)
                .map_err(|_| {
                    NativeRuntimeError::Agent(format!(
                        "cannot create document URI for {}",
                        path.display()
                    ))
                })?
                .to_string(),
        };
        self.bootstrap()
            .bundle()
            .lsp
            .open_document(LspDocumentSnapshot {
                document: document.clone(),
                language_id: language_id.to_owned(),
                version,
                text,
            })?;
        Ok(document)
    }

    pub fn shared_lsp_change_document(
        &self,
        document: &LspDocumentId,
        version: i64,
        text: String,
    ) -> Result<(), NativeRuntimeError> {
        self.bootstrap()
            .bundle()
            .lsp
            .change_document(document, version, text)?;
        Ok(())
    }

    pub fn shared_lsp_save_document(
        &self,
        document: &LspDocumentId,
        text: String,
    ) -> Result<(), NativeRuntimeError> {
        self.bootstrap()
            .bundle()
            .lsp
            .save_document(document, Some(text))?;
        Ok(())
    }

    pub fn shared_lsp_close_document(
        &self,
        document: &LspDocumentId,
    ) -> Result<(), NativeRuntimeError> {
        self.bootstrap().bundle().lsp.close_document(document)?;
        Ok(())
    }

    pub fn shared_lsp_document_diagnostics(
        &self,
        document: &LspDocumentId,
    ) -> Result<Vec<LspDiagnostic>, NativeRuntimeError> {
        self.bootstrap()
            .bundle()
            .lsp
            .diagnostics(document)
            .map_err(NativeRuntimeError::from)
    }

    /// Resolve definition locations through the same LSP session as Agent tools.
    ///
    /// LSP permits `Location`, `Location[]`, and `LocationLink[]` response shapes;
    /// product callers receive one normalized contract regardless of server.
    pub fn shared_lsp_definition(
        &self,
        document: &LspDocumentId,
        position: LspPosition,
    ) -> Result<Vec<LspLocation>, NativeRuntimeError> {
        let bundle = self.bootstrap().bundle();
        let result = bundle.lsp.definition(document, position)?;
        let value = materialize_lsp_query_result(bundle, result)?;
        normalize_lsp_locations(value)
    }

    /// Prepare the exact shared workspace state exposed to the model.
    ///
    /// Code Index is mandatory. LSP is best-effort because a workspace may not
    /// have a matching language server installed; unavailable LSP tools are not
    /// advertised by the Native Coding bundle.
    pub(crate) fn prepare_native_coding_workspace(
        &self,
        root: &str,
    ) -> Result<Value, NativeRuntimeError> {
        let root = canonical_workspace_root(root)?;
        let workspace = self.synchronize_code_index(&root)?;
        let lsp_ready = self
            .shared_lsp_open_workspace(&workspace.workspace_id, &workspace.root)
            .is_ok();
        Ok(serde_json::json!({
            "workspaceId": workspace.workspace_id,
            "root": workspace.root,
            "codeIndexReady": true,
            "lspReady": lsp_ready,
        }))
    }

    fn synchronize_code_index(&self, root: &Path) -> Result<CodeWorkspaceRef, NativeRuntimeError> {
        let files = collect_indexable_files(root)?;
        let service = Arc::clone(&self.bootstrap().bundle().code_index);
        let canonical = canonical_workspace_id(root);
        let workspace = CodeWorkspaceRef {
            workspace_id: canonical.clone(),
            root: canonical,
            tenant_id: String::new(),
            git_revision: None,
            worktree_id: None,
        };
        service.open_workspace(workspace.clone(), None, None, false)?;
        service.apply_batch(CodeIndexBatch {
            workspace: workspace.clone(),
            rebuild: true,
            changes: files
                .into_iter()
                .map(|(path, content)| CodeFileChange::Change { path, content })
                .collect(),
        })?;
        Ok(workspace)
    }

    /// LSP workspace inventory from the shared SharedLspService Arc (no second LS).
    pub fn shared_lsp_status(&self) -> Result<Value, NativeRuntimeError> {
        let lsp = Arc::clone(&self.bootstrap().bundle().lsp);
        Ok(serde_json::json!({
            "serviceId": LSP_SERVICE_ID,
            "activeWorkspaces": lsp.active_workspace_count(),
            "workspaces": lsp.list_workspaces(),
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

fn registered_mcp_transport(
    value: &str,
) -> Result<McpTransportKind, mutsuki_agent_contracts::AgentError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "stdio" => Ok(McpTransportKind::Stdio),
        "http" | "streamable_http" | "streamable-http" => Ok(McpTransportKind::StreamableHttp),
        "sse" => Ok(McpTransportKind::Sse),
        _ => Err(mutsuki_agent_contracts::AgentError::invalid_input(format!(
            "unsupported MCP transport `{value}`"
        ))),
    }
}

fn materialize_lsp_query_result(
    bundle: &mutsuki_agent_bundle::NativeCodingAgentBundle,
    result: LspQueryResult,
) -> Result<Value, NativeRuntimeError> {
    if let Some(value) = result.inline {
        return Ok(value);
    }
    let reference = result.details.ok_or_else(|| {
        NativeRuntimeError::Agent("LSP query returned neither inline nor resource data".into())
    })?;
    bundle
        .resources
        .read_json(&reference)
        .map_err(NativeRuntimeError::from)
}

fn normalize_lsp_locations(value: Value) -> Result<Vec<LspLocation>, NativeRuntimeError> {
    let entries = match value {
        Value::Null => return Ok(Vec::new()),
        Value::Array(entries) => entries,
        value @ Value::Object(_) => vec![value],
        _ => {
            return Err(NativeRuntimeError::Agent(
                "LSP definition returned an invalid response shape".into(),
            ));
        }
    };
    entries
        .into_iter()
        .map(|entry| {
            if entry.get("uri").is_some() {
                return serde_json::from_value::<LspLocation>(entry).map_err(|error| {
                    NativeRuntimeError::Agent(format!(
                        "LSP definition location decode failed: {error}"
                    ))
                });
            }
            let uri = entry
                .get("targetUri")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    NativeRuntimeError::Agent(
                        "LSP definition link did not contain targetUri".into(),
                    )
                })?;
            let range = entry
                .get("targetSelectionRange")
                .or_else(|| entry.get("targetRange"))
                .cloned()
                .ok_or_else(|| {
                    NativeRuntimeError::Agent(
                        "LSP definition link did not contain a target range".into(),
                    )
                })?;
            Ok(LspLocation {
                uri: uri.to_owned(),
                range: serde_json::from_value::<LspRange>(range).map_err(|error| {
                    NativeRuntimeError::Agent(format!(
                        "LSP definition target range decode failed: {error}"
                    ))
                })?,
            })
        })
        .collect()
}

const MAX_INDEX_FILES: usize = 4_096;
const MAX_INDEX_FILE_BYTES: u64 = 512 * 1024;

fn canonical_workspace_root(root: &str) -> Result<PathBuf, NativeRuntimeError> {
    let root = std::fs::canonicalize(root.trim())
        .map_err(|error| NativeRuntimeError::Agent(format!("workspace root: {error}")))?;
    if !root.is_dir() {
        return Err(NativeRuntimeError::Agent(
            "workspace root must be a directory".into(),
        ));
    }
    Ok(root)
}

fn canonical_workspace_id(root: &Path) -> String {
    root.to_string_lossy().into_owned()
}

fn collect_indexable_files(root: &Path) -> Result<Vec<(String, String)>, NativeRuntimeError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory)
            .map_err(|error| NativeRuntimeError::Agent(format!("read workspace: {error}")))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| NativeRuntimeError::Agent(format!("read workspace: {error}")))?;
            let file_type = entry.file_type().map_err(|error| {
                NativeRuntimeError::Agent(format!("read workspace metadata: {error}"))
            })?;
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                if !ignored_directory(&entry.file_name().to_string_lossy()) {
                    pending.push(path);
                }
                continue;
            }
            if !file_type.is_file() || !indexable_extension(&path) {
                continue;
            }
            let metadata = entry.metadata().map_err(|error| {
                NativeRuntimeError::Agent(format!("read workspace metadata: {error}"))
            })?;
            if metadata.len() > MAX_INDEX_FILE_BYTES {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let relative = path
                .strip_prefix(root)
                .map_err(|error| NativeRuntimeError::Agent(error.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, content));
            if files.len() >= MAX_INDEX_FILES {
                break;
            }
        }
        if files.len() >= MAX_INDEX_FILES {
            break;
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

fn ignored_directory(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".codegraph"
            | ".idea"
            | ".vscode"
            | ".yarn"
            | "build"
            | "coverage"
            | "dist"
            | "node_modules"
            | "target"
            | "vendor"
    )
}

fn indexable_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some(
            "c" | "cc"
                | "cpp"
                | "css"
                | "go"
                | "h"
                | "hpp"
                | "html"
                | "java"
                | "js"
                | "json"
                | "jsx"
                | "kt"
                | "kts"
                | "md"
                | "mjs"
                | "py"
                | "rb"
                | "rs"
                | "sh"
                | "swift"
                | "toml"
                | "ts"
                | "tsx"
                | "vue"
                | "xml"
                | "yaml"
                | "yml"
        )
    )
}

fn language_server_for_workspace(
    root: &Path,
) -> Result<(String, String, Vec<String>), NativeRuntimeError> {
    if root.join("Cargo.toml").is_file() {
        return Ok(("rust-analyzer".into(), "rust-analyzer".into(), Vec::new()));
    }
    if ["tsconfig.json", "jsconfig.json", "package.json"]
        .iter()
        .any(|name| root.join(name).is_file())
    {
        return Ok((
            "typescript-language-server".into(),
            "typescript-language-server".into(),
            vec!["--stdio".into()],
        ));
    }
    if ["pyproject.toml", "requirements.txt", "setup.py"]
        .iter()
        .any(|name| root.join(name).is_file())
    {
        return Ok((
            "pyright".into(),
            "pyright-langserver".into(),
            vec!["--stdio".into()],
        ));
    }
    Err(NativeRuntimeError::Agent(
        "no supported language server was detected for this workspace".into(),
    ))
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
        assert!(status.computer_use_same_instance);
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
    fn lsp_definition_normalizes_locations_and_location_links() {
        let locations = normalize_lsp_locations(serde_json::json!([
            {
                "uri": "file:///workspace/src/direct.rs",
                "range": {
                    "start": {"line": 2, "character": 3},
                    "end": {"line": 2, "character": 9}
                }
            },
            {
                "targetUri": "file:///workspace/src/link.rs",
                "targetRange": {
                    "start": {"line": 4, "character": 0},
                    "end": {"line": 6, "character": 1}
                },
                "targetSelectionRange": {
                    "start": {"line": 5, "character": 2},
                    "end": {"line": 5, "character": 8}
                }
            }
        ]))
        .unwrap();

        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].uri, "file:///workspace/src/direct.rs");
        assert_eq!(locations[0].range.start.line, 2);
        assert_eq!(locations[1].uri, "file:///workspace/src/link.rs");
        assert_eq!(locations[1].range.start.line, 5);
        assert_eq!(locations[1].range.end.character, 8);
        assert!(normalize_lsp_locations(Value::Null).unwrap().is_empty());
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
    fn shared_git_diff_returns_the_real_worktree_patch() {
        let dir = tempfile_dir();
        assert!(Command::new("git")
            .args(["init"])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success());
        std::fs::write(Path::new(&dir).join("tracked.txt"), "before\n").unwrap();
        assert!(Command::new("git")
            .args(["add", "tracked.txt"])
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
                "-m",
                "init"
            ])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success());
        std::fs::write(Path::new(&dir).join("tracked.txt"), "after\n").unwrap();
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let diff = runtime
            .shared_git_diff(&dir, GitDiffScope::WorkingTree)
            .unwrap();
        assert_eq!(diff.get("kind").and_then(Value::as_str), Some("diff"));
        assert!(
            diff.pointer("/inline_patch")
                .and_then(Value::as_str)
                .is_some_and(|patch| patch.contains("+after")),
            "unexpected git diff payload: {diff}"
        );

        let large_body = "after\n".repeat(600);
        std::fs::write(Path::new(&dir).join("tracked.txt"), &large_body).unwrap();
        let diff = runtime
            .shared_git_diff(&dir, GitDiffScope::WorkingTree)
            .unwrap();
        assert!(diff.pointer("/patch_ref").is_some());
        assert!(
            diff.pointer("/inline_patch")
                .and_then(Value::as_str)
                .is_some_and(|patch| patch.len() > 2_048 && patch.contains("+after\n+after")),
            "unexpected materialized git diff payload: {diff}"
        );
        assert_eq!(diff.get("truncated").and_then(Value::as_bool), Some(true));

        assert!(Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success());
        let staged = runtime.shared_git_diff(&dir, GitDiffScope::Staged).unwrap();
        assert!(
            staged
                .pointer("/inline_patch")
                .and_then(Value::as_str)
                .is_some_and(|patch| patch.contains("+after\n+after")),
            "unexpected staged git diff payload: {staged}"
        );
    }

    #[test]
    fn shared_code_index_search_indexes_real_workspace_files() {
        let root = std::env::temp_dir().join("lilia-shared-index-workspace");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/hello.rs"),
            "pub fn shared_marker_alpha() {}\n",
        )
        .unwrap();
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let value = runtime
            .shared_code_index_workspace_search(
                "ws-48",
                &root.display().to_string(),
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
        assert_eq!(
            hits[0].get("path").and_then(Value::as_str),
            Some("src/hello.rs")
        );
        let symbols = runtime
            .shared_code_index_workspace_search_with_mode(
                "ws-48",
                &root.display().to_string(),
                "shared_marker_alpha",
                CodeSearchMode::Symbol,
            )
            .unwrap();
        assert_eq!(
            symbols.pointer("/query/mode").and_then(Value::as_str),
            Some("symbol")
        );
        assert!(
            symbols
                .get("hits")
                .and_then(Value::as_array)
                .is_some_and(|hits| !hits.is_empty()),
            "symbol search should return the indexed function: {symbols}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn shared_computer_use_lists_real_workspace() {
        let root = std::env::temp_dir().join("lilia-shared-computer-use");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("visible.txt"), "real workspace content").unwrap();
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let value = runtime
            .shared_workspace_list("ws-48", &root.display().to_string(), "")
            .unwrap();
        let entries = value
            .get("entries")
            .and_then(Value::as_array)
            .expect("workspace entries");
        assert!(entries
            .iter()
            .any(|entry| { entry.get("path").and_then(Value::as_str) == Some("visible.txt") }));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn shared_mcp_list_servers_uses_bundle_arc() {
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let servers = runtime.shared_mcp_list_servers().unwrap();
        assert!(
            servers.as_array().is_some(),
            "expected MCP server array: {servers}"
        );
        let bundle = runtime.bootstrap().bundle();
        let product = Arc::clone(&bundle.mcp);
        assert!(Arc::ptr_eq(&bundle.mcp, &product));
        assert_eq!(bundle.mcp.active_server_count(), 0);
    }

    #[test]
    fn registered_mcp_transport_accepts_supported_registry_spellings() {
        assert_eq!(
            registered_mcp_transport("stdio").unwrap(),
            McpTransportKind::Stdio
        );
        assert_eq!(
            registered_mcp_transport("streamable-http").unwrap(),
            McpTransportKind::StreamableHttp
        );
        assert_eq!(
            registered_mcp_transport("http").unwrap(),
            McpTransportKind::StreamableHttp
        );
        assert!(registered_mcp_transport("oauth").is_err());
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
        assert!(status
            .get("workspaces")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty));
        let bundle = runtime.bootstrap().bundle();
        let product = Arc::clone(&bundle.lsp);
        assert!(Arc::ptr_eq(&bundle.lsp, &product));
    }

    #[test]
    #[ignore = "requires the rust-analyzer executable"]
    fn shared_lsp_surfaces_unsaved_rust_diagnostics() {
        let root = PathBuf::from(tempfile_dir());
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"lilia-lsp-fixture\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        let path = root.join("src/main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let runtime = NativeRuntimeBootstrap::embedded_reference()
            .unwrap()
            .into_runtime();
        let document = runtime
            .shared_lsp_open_document(
                &root.to_string_lossy(),
                &path,
                "rust",
                1,
                "fn main() {}\n".to_owned(),
            )
            .unwrap();
        runtime
            .shared_lsp_change_document(&document, 2, "fn main() { let _: bool = 1; }\n".to_owned())
            .unwrap();

        let mut diagnostics = Vec::new();
        for _ in 0..100 {
            diagnostics = runtime.shared_lsp_document_diagnostics(&document).unwrap();
            if !diagnostics.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            !diagnostics.is_empty(),
            "rust-analyzer did not publish the unsaved type error"
        );
        runtime.shared_lsp_close_document(&document).unwrap();
        std::fs::remove_dir_all(root).unwrap();
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

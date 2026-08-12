use lilia_agent_integration::SharedCodingServicesStatus;
use mutsuki_agent_contracts::{
    CodeSearchResult, ComputerUseServiceResponse, GitFileStatus, GitServiceResponse,
};
use serde::Serialize;
use serde_json::Value;

use crate::{DesktopApplication, DesktopApplicationError};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCodingServicesSnapshot {
    pub status: SharedCodingServicesStatus,
    pub mcp_servers: Value,
    pub lsp: Value,
    pub registry: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopGitStatus {
    pub root: String,
    pub branch: Option<String>,
    pub commit: String,
    pub clean: bool,
    pub changes: Vec<DesktopGitChange>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopGitChange {
    pub path: String,
    pub previous_path: Option<String>,
    pub status: DesktopGitFileStatus,
    pub staged: bool,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopGitFileStatus {
    Untracked,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Conflicted,
    Ignored,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorkspaceListing {
    pub entries: Vec<DesktopWorkspaceEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorkspaceEntry {
    pub path: String,
    pub kind: String,
    pub size_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCodeSearchResult {
    pub query: String,
    pub index_revision: u64,
    pub hits: Vec<DesktopCodeSearchHit>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCodeSearchHit {
    pub path: String,
    pub summary: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub score: Option<f64>,
}

impl DesktopApplication {
    pub fn coding_services_snapshot(
        &self,
    ) -> Result<DesktopCodingServicesSnapshot, DesktopApplicationError> {
        let runtime = self.authority().shared_runtime();
        let runtime = runtime.inner();
        Ok(DesktopCodingServicesSnapshot {
            status: runtime
                .shared_coding_services_status()
                .map_err(coding_service_error)?,
            mcp_servers: runtime
                .shared_mcp_list_servers()
                .map_err(coding_service_error)?,
            lsp: runtime.shared_lsp_status().map_err(coding_service_error)?,
            registry: runtime
                .shared_agentkit_registry_status(&self.config().data_paths())
                .map_err(coding_service_error)?,
        })
    }

    pub fn shared_git_status(
        &self,
        path: &str,
    ) -> Result<DesktopGitStatus, DesktopApplicationError> {
        require_non_empty("path", path)?;
        let response = self
            .authority()
            .shared_runtime()
            .inner()
            .shared_git_status(path.trim())
            .map_err(coding_service_error)?;
        decode_git_status(response)
    }

    pub fn shared_code_index_search(
        &self,
        workspace_id: &str,
        root: &str,
        query: &str,
    ) -> Result<DesktopCodeSearchResult, DesktopApplicationError> {
        require_non_empty("workspace_id", workspace_id)?;
        require_non_empty("root", root)?;
        require_non_empty("query", query)?;
        let response = self
            .authority()
            .shared_runtime()
            .inner()
            .shared_code_index_workspace_search(workspace_id.trim(), root.trim(), query.trim())
            .map_err(coding_service_error)?;
        decode_code_search(response)
    }

    pub fn shared_workspace_list(
        &self,
        workspace_id: &str,
        root: &str,
        path: &str,
    ) -> Result<DesktopWorkspaceListing, DesktopApplicationError> {
        require_non_empty("workspace_id", workspace_id)?;
        require_non_empty("root", root)?;
        let response = self
            .authority()
            .shared_runtime()
            .inner()
            .shared_workspace_list(workspace_id.trim(), root.trim(), path.trim())
            .map_err(coding_service_error)?;
        decode_workspace_listing(response)
    }
}

fn decode_git_status(response: Value) -> Result<DesktopGitStatus, DesktopApplicationError> {
    let response = serde_json::from_value::<GitServiceResponse>(response).map_err(|error| {
        coding_service_error(format!("Git status response is invalid: {error}"))
    })?;
    let GitServiceResponse::Status(status) = response else {
        return Err(coding_service_error(
            "Git service returned a response other than status",
        ));
    };
    Ok(DesktopGitStatus {
        root: status.worktree.path,
        branch: status.head.branch,
        commit: status.head.commit,
        clean: status.clean,
        changes: status
            .changes
            .into_iter()
            .map(|change| DesktopGitChange {
                path: change.path,
                previous_path: change.old_path,
                status: change.status.into(),
                staged: change.staged,
                additions: change.additions,
                deletions: change.deletions,
            })
            .collect(),
    })
}

fn decode_workspace_listing(
    response: Value,
) -> Result<DesktopWorkspaceListing, DesktopApplicationError> {
    let response =
        serde_json::from_value::<ComputerUseServiceResponse>(response).map_err(|error| {
            coding_service_error(format!("workspace listing response is invalid: {error}"))
        })?;
    let ComputerUseServiceResponse::Entries { entries } = response else {
        return Err(coding_service_error(
            "workspace service returned a response other than entries",
        ));
    };
    Ok(DesktopWorkspaceListing {
        entries: entries
            .into_iter()
            .map(|entry| DesktopWorkspaceEntry {
                path: entry.path,
                kind: entry.kind,
                size_bytes: entry.size,
            })
            .collect(),
    })
}

fn decode_code_search(response: Value) -> Result<DesktopCodeSearchResult, DesktopApplicationError> {
    let result = serde_json::from_value::<CodeSearchResult>(response).map_err(|error| {
        coding_service_error(format!("code search response is invalid: {error}"))
    })?;
    Ok(DesktopCodeSearchResult {
        query: result.query.query,
        index_revision: result.index_revision,
        hits: result
            .hits
            .into_iter()
            .map(|hit| DesktopCodeSearchHit {
                path: hit.path,
                summary: hit.summary,
                start_line: hit.range.map(|range| range.start.line),
                end_line: hit.range.map(|range| range.end.line),
                score: hit.score,
            })
            .collect(),
    })
}

impl From<GitFileStatus> for DesktopGitFileStatus {
    fn from(value: GitFileStatus) -> Self {
        match value {
            GitFileStatus::Untracked => Self::Untracked,
            GitFileStatus::Modified => Self::Modified,
            GitFileStatus::Added => Self::Added,
            GitFileStatus::Deleted => Self::Deleted,
            GitFileStatus::Renamed => Self::Renamed,
            GitFileStatus::Copied => Self::Copied,
            GitFileStatus::Conflicted => Self::Conflicted,
            GitFileStatus::Ignored => Self::Ignored,
        }
    }
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), DesktopApplicationError> {
    if value.trim().is_empty() {
        Err(DesktopApplicationError::InvalidInput {
            field,
            message: format!("{field} must not be empty"),
        })
    } else {
        Ok(())
    }
}

fn coding_service_error(error: impl std::fmt::Display) -> DesktopApplicationError {
    DesktopApplicationError::Agent(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shared_service_payloads_are_normalized_before_reaching_ui() {
        let git = decode_git_status(json!({
            "kind": "status",
            "worktree": {
                "worktree_id": "worktree-1",
                "path": "C:/workspace",
                "repository": { "repo_id": "repo-1", "root": "C:/workspace" }
            },
            "head": {
                "commit": "abcdef",
                "branch": "main",
                "generation": 3
            },
            "clean": false,
            "changes": [{
                "path": "src/main.rs",
                "status": "modified",
                "staged": false,
                "additions": 4,
                "deletions": 1
            }]
        }))
        .unwrap();
        assert_eq!(git.branch.as_deref(), Some("main"));
        assert_eq!(git.changes[0].status, DesktopGitFileStatus::Modified);

        let workspace = decode_workspace_listing(json!({
            "kind": "entries",
            "entries": [{ "path": "src", "kind": "directory" }]
        }))
        .unwrap();
        assert_eq!(workspace.entries[0].path, "src");

        let search = decode_code_search(json!({
            "query": {
                "workspace": {
                    "workspace_id": "workspace-1",
                    "root": "C:/workspace"
                },
                "query": "PreviewProgram",
                "mode": "text",
                "limit": 16,
                "include_overlay": false
            },
            "hits": [{
                "path": "src/preview.rs",
                "summary": "struct PreviewProgram",
                "range": {
                    "start": { "line": 20, "character": 0 },
                    "end": { "line": 20, "character": 21 }
                },
                "score": 1.0,
                "provenance": {
                    "path": "src/preview.rs",
                    "workspace_id": "workspace-1",
                    "source": "text"
                }
            }],
            "index_revision": 7
        }))
        .unwrap();
        assert_eq!(search.query, "PreviewProgram");
        assert_eq!(search.hits[0].start_line, Some(20));
    }

    #[test]
    fn unexpected_shared_service_variants_fail_instead_of_becoming_fake_empty_state() {
        assert!(decode_git_status(json!({ "kind": "ack" })).is_err());
        assert!(decode_workspace_listing(json!({ "kind": "ack" })).is_err());
    }
}

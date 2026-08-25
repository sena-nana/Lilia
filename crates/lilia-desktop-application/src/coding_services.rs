use lilia_agent::SharedCodingServicesStatus;
use mutsuki_agent_contracts::{
    CodeSearchMode, CodeSearchResult, ComputerUseServiceResponse, GitDiffScope, GitFileChange,
    GitFileStatus, GitServiceResponse,
};
use serde::Serialize;
use serde_json::Value;

use crate::{DesktopApplication, DesktopApplicationError, ProjectQuery};

pub use lilia_feature_coding::{
    CodeSearchHit as DesktopCodeSearchHit, CodeSearchMode as DesktopCodeSearchMode,
    CodeSearchResult as DesktopCodeSearchResult, CodeSearchScope as DesktopCodeSearchScope,
    GitChange as DesktopGitChange, GitDiff as DesktopGitDiff,
    GitDiffScope as DesktopGitDiffScope, GitFileStatus as DesktopGitFileStatus,
    GitStatus as DesktopGitStatus,
    WorkspaceCodeSearchFailure as DesktopWorkspaceCodeSearchFailure,
    WorkspaceCodeSearchHit as DesktopWorkspaceCodeSearchHit,
    WorkspaceCodeSearchResult as DesktopWorkspaceCodeSearchResult,
    WorkspaceEntry as DesktopWorkspaceEntry, WorkspaceListing as DesktopWorkspaceListing,
};

const MAX_CODE_SEARCH_PROJECTS: usize = 32;
const MAX_CODE_SEARCH_HITS: usize = 128;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCodingServicesSnapshot {
    pub status: SharedCodingServicesStatus,
    pub mcp_servers: Value,
    pub lsp: Value,
    pub registry: Value,
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

    pub fn shared_git_diff(
        &self,
        path: &str,
        scope: DesktopGitDiffScope,
    ) -> Result<DesktopGitDiff, DesktopApplicationError> {
        require_non_empty("path", path)?;
        let response = self
            .authority()
            .shared_runtime()
            .inner()
            .shared_git_diff(path.trim(), runtime_git_diff_scope(scope))
            .map_err(coding_service_error)?;
        decode_git_diff(response, scope)
    }

    pub fn shared_code_index_search(
        &self,
        workspace_id: &str,
        root: &str,
        query: &str,
    ) -> Result<DesktopCodeSearchResult, DesktopApplicationError> {
        self.shared_code_index_search_with_mode(
            workspace_id,
            root,
            query,
            DesktopCodeSearchMode::Text,
        )
    }

    pub fn shared_code_index_search_with_mode(
        &self,
        workspace_id: &str,
        root: &str,
        query: &str,
        mode: DesktopCodeSearchMode,
    ) -> Result<DesktopCodeSearchResult, DesktopApplicationError> {
        require_non_empty("workspace_id", workspace_id)?;
        require_non_empty("root", root)?;
        require_non_empty("query", query)?;
        let response = self
            .authority()
            .shared_runtime()
            .inner()
            .shared_code_index_workspace_search_with_mode(
                workspace_id.trim(),
                root.trim(),
                query.trim(),
                runtime_code_search_mode(mode),
            )
            .map_err(coding_service_error)?;
        decode_code_search(response)
    }

    pub fn search_code_indexes(
        &self,
        scope: DesktopCodeSearchScope,
        query: &str,
        mode: DesktopCodeSearchMode,
    ) -> Result<DesktopWorkspaceCodeSearchResult, DesktopApplicationError> {
        require_non_empty("query", query)?;
        let query = query.trim();
        let mut projects = self.query_projects(ProjectQuery::default())?;
        projects.sort_by(|left, right| {
            right
                .pinned
                .cmp(&left.pinned)
                .then_with(|| left.sort_order.cmp(&right.sort_order))
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });
        projects = match &scope {
            DesktopCodeSearchScope::Project { project_id } => vec![projects
                .into_iter()
                .find(|project| &project.id == project_id)
                .ok_or_else(|| DesktopApplicationError::InvalidInput {
                    field: "projectId",
                    message: format!("active project `{}` was not found", project_id.as_str()),
                })?],
            DesktopCodeSearchScope::AllProjects => projects,
        };
        projects.retain(|project| {
            project
                .workspace_path
                .as_deref()
                .is_some_and(|root| !root.trim().is_empty())
        });
        if matches!(scope, DesktopCodeSearchScope::Project { .. }) && projects.is_empty() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "projectId",
                message: "project does not have a workspace path".to_owned(),
            });
        }

        let eligible_project_count = projects.len();
        let truncated_projects = eligible_project_count > MAX_CODE_SEARCH_PROJECTS;
        projects.truncate(MAX_CODE_SEARCH_PROJECTS);
        let projects_searched = projects.len();
        let single_project = matches!(scope, DesktopCodeSearchScope::Project { .. });
        let mut hits = Vec::new();
        let mut failures = Vec::new();
        for project in projects {
            let root = project
                .workspace_path
                .as_deref()
                .map(str::trim)
                .expect("workspace projects were filtered")
                .to_owned();
            match self.shared_code_index_search_with_mode(project.id.as_str(), &root, query, mode) {
                Ok(result) => {
                    hits.extend(
                        result
                            .hits
                            .into_iter()
                            .map(|hit| DesktopWorkspaceCodeSearchHit {
                                project_id: project.id.clone(),
                                project_name: project.name.clone(),
                                workspace_root: root.clone(),
                                index_revision: result.index_revision,
                                hit,
                            }),
                    )
                }
                Err(error) if single_project => return Err(error),
                Err(error) => failures.push(DesktopWorkspaceCodeSearchFailure {
                    project_id: project.id,
                    project_name: project.name,
                    message: error.to_string(),
                }),
            }
        }
        hits.sort_by(|left, right| {
            right
                .hit
                .score
                .unwrap_or(f64::NEG_INFINITY)
                .total_cmp(&left.hit.score.unwrap_or(f64::NEG_INFINITY))
                .then_with(|| left.project_name.cmp(&right.project_name))
                .then_with(|| left.hit.path.cmp(&right.hit.path))
                .then_with(|| left.hit.start_line.cmp(&right.hit.start_line))
                .then_with(|| left.hit.start_character.cmp(&right.hit.start_character))
        });
        let truncated_hits = hits.len() > MAX_CODE_SEARCH_HITS;
        hits.truncate(MAX_CODE_SEARCH_HITS);
        Ok(DesktopWorkspaceCodeSearchResult {
            query: query.to_owned(),
            mode,
            scope,
            eligible_project_count,
            projects_searched,
            truncated_projects,
            truncated_hits,
            hits,
            failures,
        })
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
        changes: status.changes.into_iter().map(desktop_git_change).collect(),
    })
}

fn decode_git_diff(
    response: Value,
    scope: DesktopGitDiffScope,
) -> Result<DesktopGitDiff, DesktopApplicationError> {
    let response = serde_json::from_value::<GitServiceResponse>(response)
        .map_err(|error| coding_service_error(format!("Git diff response is invalid: {error}")))?;
    let GitServiceResponse::Diff(diff) = response else {
        return Err(coding_service_error(
            "Git service returned a response other than diff",
        ));
    };
    Ok(DesktopGitDiff {
        root: diff.worktree.path,
        scope,
        summary: diff.summary,
        files: diff.files.into_iter().map(desktop_git_change).collect(),
        patch: diff.inline_patch,
        truncated: diff.truncated,
    })
}

fn desktop_git_change(change: GitFileChange) -> DesktopGitChange {
    DesktopGitChange {
        path: change.path,
        previous_path: change.old_path,
        status: desktop_git_file_status(change.status),
        staged: change.staged,
        additions: change.additions,
        deletions: change.deletions,
    }
}

fn runtime_git_diff_scope(value: DesktopGitDiffScope) -> GitDiffScope {
    match value {
        DesktopGitDiffScope::WorkingTree => GitDiffScope::WorkingTree,
        DesktopGitDiffScope::Staged => GitDiffScope::Staged,
    }
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
        mode: desktop_code_search_mode(result.query.mode),
        index_revision: result.index_revision,
        hits: result
            .hits
            .into_iter()
            .map(|hit| DesktopCodeSearchHit {
                path: hit.path,
                summary: hit.summary,
                start_line: hit.range.map(|range| range.start.line),
                start_character: hit.range.map(|range| range.start.character),
                end_line: hit.range.map(|range| range.end.line),
                end_character: hit.range.map(|range| range.end.character),
                score: hit.score,
            })
            .collect(),
    })
}

fn runtime_code_search_mode(value: DesktopCodeSearchMode) -> CodeSearchMode {
    match value {
        DesktopCodeSearchMode::Text => CodeSearchMode::Text,
        DesktopCodeSearchMode::Regex => CodeSearchMode::Regex,
        DesktopCodeSearchMode::Symbol => CodeSearchMode::Symbol,
        DesktopCodeSearchMode::Semantic => CodeSearchMode::Semantic,
    }
}

fn desktop_code_search_mode(value: CodeSearchMode) -> DesktopCodeSearchMode {
    match value {
        CodeSearchMode::Text => DesktopCodeSearchMode::Text,
        CodeSearchMode::Regex => DesktopCodeSearchMode::Regex,
        CodeSearchMode::Symbol => DesktopCodeSearchMode::Symbol,
        CodeSearchMode::Semantic => DesktopCodeSearchMode::Semantic,
    }
}

fn desktop_git_file_status(value: GitFileStatus) -> DesktopGitFileStatus {
    match value {
        GitFileStatus::Untracked => DesktopGitFileStatus::Untracked,
        GitFileStatus::Modified => DesktopGitFileStatus::Modified,
        GitFileStatus::Added => DesktopGitFileStatus::Added,
        GitFileStatus::Deleted => DesktopGitFileStatus::Deleted,
        GitFileStatus::Renamed => DesktopGitFileStatus::Renamed,
        GitFileStatus::Copied => DesktopGitFileStatus::Copied,
        GitFileStatus::Conflicted => DesktopGitFileStatus::Conflicted,
        GitFileStatus::Ignored => DesktopGitFileStatus::Ignored,
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
    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, DesktopProjectCreate,
    };
    use lilia_service::ServiceAuthority;
    use serde_json::json;
    use std::sync::Arc;

    struct NoopHost;

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

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
            "state": {
                "head_commit": "abcdef",
                "head_ref": "main",
                "index_hash": "index-1",
                "worktree_hash": "worktree-1"
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
        assert_eq!(search.mode, DesktopCodeSearchMode::Text);
        assert_eq!(search.hits[0].start_line, Some(20));
        assert_eq!(search.hits[0].start_character, Some(0));
        assert_eq!(search.hits[0].end_character, Some(21));

        let diff = decode_git_diff(
            json!({
                "kind": "diff",
                "worktree": {
                    "worktree_id": "worktree-1",
                    "path": "C:/workspace",
                    "repository": { "repo_id": "repo-1", "root": "C:/workspace" }
                },
                "base": { "commit": "abcdef", "generation": 3 },
                "head": { "commit": "abcdef", "generation": 3 },
                "summary": "1 file changed",
                "files": [{
                    "path": "src/main.rs",
                    "status": "modified",
                    "staged": false,
                    "additions": 1,
                    "deletions": 1
                }],
                "inline_patch": "-before\n+after\n",
                "truncated": false
            }),
            DesktopGitDiffScope::WorkingTree,
        )
        .unwrap();
        assert_eq!(diff.files[0].path, "src/main.rs");
        assert_eq!(diff.patch.as_deref(), Some("-before\n+after\n"));
    }

    #[test]
    fn unexpected_shared_service_variants_fail_instead_of_becoming_fake_empty_state() {
        assert!(decode_git_status(json!({ "kind": "ack" })).is_err());
        assert!(decode_git_diff(json!({ "kind": "ack" }), DesktopGitDiffScope::Staged).is_err());
        assert!(decode_workspace_listing(json!({ "kind": "ack" })).is_err());
    }

    #[test]
    fn all_project_code_search_keeps_project_identity_and_uses_real_indexes() {
        let data = tempfile::tempdir().unwrap();
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(first.path().join("src")).unwrap();
        std::fs::create_dir_all(second.path().join("lib")).unwrap();
        std::fs::write(
            first.path().join("src/alpha.rs"),
            "pub fn cross_project_marker() {}\n",
        )
        .unwrap();
        std::fs::write(
            second.path().join("lib/beta.rs"),
            "pub const BETA_MARKER: &str = \"cross_project_marker\";\n",
        )
        .unwrap();
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            "test:all-project-code-search",
            "all-project-code-search",
        )
        .unwrap();
        let app = DesktopApplication::from_authority(
            DesktopApplicationConfig::new(data.path(), "all-project-code-search").unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();
        let alpha = app
            .create_project(DesktopProjectCreate {
                workspace_path: Some(first.path().display().to_string()),
                ..DesktopProjectCreate::new("Alpha")
            })
            .unwrap();
        let beta = app
            .create_project(DesktopProjectCreate {
                workspace_path: Some(second.path().display().to_string()),
                ..DesktopProjectCreate::new("Beta")
            })
            .unwrap();

        let result = app
            .search_code_indexes(
                DesktopCodeSearchScope::AllProjects,
                "cross_project_marker",
                DesktopCodeSearchMode::Text,
            )
            .unwrap();

        assert_eq!(result.eligible_project_count, 2);
        assert_eq!(result.projects_searched, 2);
        assert!(result.failures.is_empty());
        assert!(!result.truncated_projects);
        assert!(!result.truncated_hits);
        assert!(result
            .hits
            .iter()
            .any(|hit| hit.project_id == alpha.id && hit.hit.path == "src/alpha.rs"));
        assert!(result
            .hits
            .iter()
            .any(|hit| hit.project_id == beta.id && hit.hit.path == "lib/beta.rs"));
        assert!(result.hits.iter().all(|hit| hit.index_revision > 0));
    }
}

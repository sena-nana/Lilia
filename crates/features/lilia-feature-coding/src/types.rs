//! Vocabulary the workspace surface renders.
//!
//! Every type here is plain data: the host decodes an agent runtime response
//! into it, and the shell projects it. Both directions cross a job boundary, so
//! all of them round-trip through JSON.

use lilia_contracts::ProjectId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub root: String,
    pub branch: Option<String>,
    pub commit: String,
    pub clean: bool,
    pub changes: Vec<GitChange>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitDiffScope {
    WorkingTree,
    Staged,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiff {
    pub root: String,
    pub scope: GitDiffScope,
    pub summary: String,
    pub files: Vec<GitChange>,
    pub patch: Option<String>,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitChange {
    pub path: String,
    pub previous_path: Option<String>,
    pub status: GitFileStatus,
    pub staged: bool,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitFileStatus {
    Untracked,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Conflicted,
    Ignored,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceListing {
    pub entries: Vec<WorkspaceEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntry {
    pub path: String,
    pub kind: String,
    pub size_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeSearchResult {
    pub query: String,
    pub mode: CodeSearchMode,
    pub index_revision: u64,
    pub hits: Vec<CodeSearchHit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeSearchMode {
    Text,
    Regex,
    Symbol,
    Semantic,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeSearchHit {
    pub path: String,
    pub summary: String,
    pub start_line: Option<u32>,
    pub start_character: Option<u32>,
    pub end_line: Option<u32>,
    pub end_character: Option<u32>,
    pub score: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CodeSearchScope {
    Project { project_id: ProjectId },
    AllProjects,
}

impl CodeSearchScope {
    pub fn project_id(&self) -> Option<&ProjectId> {
        match self {
            Self::Project { project_id } => Some(project_id),
            Self::AllProjects => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeSearchHit {
    pub project_id: ProjectId,
    pub project_name: String,
    pub workspace_root: String,
    pub index_revision: u64,
    pub hit: CodeSearchHit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeSearchFailure {
    pub project_id: ProjectId,
    pub project_name: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeSearchResult {
    pub query: String,
    pub mode: CodeSearchMode,
    pub scope: CodeSearchScope,
    pub eligible_project_count: usize,
    pub projects_searched: usize,
    pub truncated_projects: bool,
    pub truncated_hits: bool,
    pub hits: Vec<WorkspaceCodeSearchHit>,
    pub failures: Vec<WorkspaceCodeSearchFailure>,
}

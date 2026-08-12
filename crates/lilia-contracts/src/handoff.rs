use serde::{Deserialize, Serialize};

use crate::{ProductTask, Project};

pub const LILIA_CODE_TASK_HANDOFF_PROTOCOL: &str = "lilia-code-task-handoff";
pub const LILIA_CODE_TASK_HANDOFF_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LiliaCodeTaskHandoffKind {
    Issue,
    PullRequestReview,
    WorkflowFailure,
    SyncConflict,
    Repository,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskHandoffRepository {
    pub full_name: String,
    pub worktree_path: String,
    pub branch: String,
    pub remote_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskHandoffSource {
    pub application: String,
    pub route: String,
    pub object_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHandoffContext {
    pub number: u64,
    pub base_branch: String,
    pub head_branch: String,
    pub base_sha: Option<String>,
    pub head_sha: Option<String>,
    #[serde(default)]
    pub review_requirements: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowHandoffContext {
    pub run_id: u64,
    pub run_url: String,
    pub workflow_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiliaCodeTaskHandoff {
    pub protocol: String,
    pub version: u32,
    pub id: String,
    pub created_at: String,
    pub title: String,
    pub kind: LiliaCodeTaskHandoffKind,
    pub repository: TaskHandoffRepository,
    pub source: TaskHandoffSource,
    pub problem: String,
    #[serde(default)]
    pub related_files: Vec<String>,
    pub log_summary: Option<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    pub pull_request: Option<PullRequestHandoffContext>,
    pub workflow: Option<WorkflowHandoffContext>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductTaskHandoffImport {
    pub handoff: LiliaCodeTaskHandoff,
    pub payload_json: String,
    pub project: Project,
    pub task: ProductTask,
    pub accepted_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductTaskHandoffRecord {
    pub handoff: LiliaCodeTaskHandoff,
    pub payload_json: String,
    pub project: Project,
    pub task: ProductTask,
    pub accepted_at: i64,
    pub duplicate: bool,
}

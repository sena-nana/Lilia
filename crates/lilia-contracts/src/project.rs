use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ConversationId, ExpectedRevision, ProductRevision, ProjectAssetId, ProjectId, TaskId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectArchiveState {
    Active,
    Archived,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWorkspaceRef {
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub worktree_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    #[serde(default)]
    pub default_agent_profile_id: Option<String>,
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub workspace_path: Option<String>,
    pub pinned: bool,
    pub sort_order: i64,
    pub archive: ProjectArchiveState,
    #[serde(default)]
    pub git_workspace: Option<GitWorkspaceRef>,
    #[serde(default)]
    pub settings: ProjectSettings,
    #[serde(default)]
    pub asset_ids: Vec<ProjectAssetId>,
    pub revision: ProductRevision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductProjectRemovalOutcome {
    pub project: Project,
    pub moved_task_ids: Vec<TaskId>,
    pub moved_conversation_ids: Vec<ConversationId>,
    pub already_removed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductProjectReorderEntry {
    pub project_id: ProjectId,
    pub expected_revision: ExpectedRevision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductProjectReorderOutcome {
    pub projects: Vec<Project>,
}

impl Project {
    pub fn new(id: ProjectId, name: impl Into<String>) -> Result<Self, crate::ProductError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(crate::ProductError::InvalidInput {
                field: "name".into(),
                message: "project name must not be empty".into(),
            });
        }
        Ok(Self {
            id,
            name,
            workspace_path: None,
            pinned: false,
            sort_order: 0,
            archive: ProjectArchiveState::Active,
            git_workspace: None,
            settings: ProjectSettings::default(),
            asset_ids: Vec::new(),
            revision: ProductRevision::INITIAL,
        })
    }
}

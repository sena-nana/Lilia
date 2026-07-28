use serde::{Deserialize, Serialize};

use crate::{ProductRevision, ProjectId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectArchiveState {
    Active,
    Archived,
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
    pub revision: ProductRevision,
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
            revision: ProductRevision::INITIAL,
        })
    }
}

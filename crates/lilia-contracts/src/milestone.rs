use serde::{Deserialize, Serialize};

use crate::{MilestoneId, ProductRevision, ProjectId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductMilestoneStatus {
    Planned,
    Active,
    Completed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductMilestone {
    pub id: MilestoneId,
    pub project_id: ProjectId,
    pub title: String,
    pub description: Option<String>,
    pub status: ProductMilestoneStatus,
    pub sort_order: i64,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub revision: ProductRevision,
}

impl ProductMilestone {
    pub fn new(
        id: MilestoneId,
        project_id: ProjectId,
        title: impl Into<String>,
    ) -> Result<Self, crate::ProductError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(crate::ProductError::InvalidInput {
                field: "title".into(),
                message: "milestone title must not be empty".into(),
            });
        }
        Ok(Self {
            id,
            project_id,
            title,
            description: None,
            status: ProductMilestoneStatus::Planned,
            sort_order: 0,
            start_date: None,
            due_date: None,
            revision: ProductRevision::INITIAL,
        })
    }
}

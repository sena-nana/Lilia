use std::fmt;

use super::{Milestone, MilestoneUpdatePatch, ProjectRoadmap, TaskMilestoneLink};

pub trait RoadmapStore: Send {
    fn list(&self, project_id: &str) -> Result<ProjectRoadmap, RoadmapStoreError>;

    fn create(&mut self, project_id: &str, title: &str) -> Result<Milestone, RoadmapStoreError>;

    fn update(
        &mut self,
        milestone_id: &str,
        patch: MilestoneUpdatePatch,
    ) -> Result<Milestone, RoadmapStoreError>;

    fn delete(&mut self, milestone_id: &str) -> Result<bool, RoadmapStoreError>;

    fn reorder(
        &mut self,
        project_id: &str,
        ordered_ids: Vec<String>,
    ) -> Result<Vec<Milestone>, RoadmapStoreError>;

    fn set_tasks(
        &mut self,
        milestone_id: &str,
        task_ids: Vec<String>,
    ) -> Result<Vec<TaskMilestoneLink>, RoadmapStoreError>;
}

#[derive(Debug, thiserror::Error)]
pub enum RoadmapStoreError {
    #[error("milestone title must not be empty")]
    InvalidTitle,
    #[error("project does not exist: {project_id}")]
    ProjectNotFound { project_id: String },
    #[error("milestone does not exist: {milestone_id}")]
    MilestoneNotFound { milestone_id: String },
    #[error("task cannot be linked to milestone project {project_id}: {task_id}")]
    TaskNotEligible { task_id: String, project_id: String },
    #[error("milestone reorder contains duplicate id: {milestone_id}")]
    DuplicateReorderId { milestone_id: String },
    #[error(
        "milestone reorder must contain every milestone in project {project_id} exactly once; missing={missing:?}, unexpected={unexpected:?}"
    )]
    IncompleteReorder {
        project_id: String,
        missing: Vec<String>,
        unexpected: Vec<String>,
    },
    #[error("stored milestone {milestone_id} has invalid status {status}")]
    InvalidStoredStatus {
        milestone_id: String,
        status: String,
    },
    #[error("roadmap state is unavailable")]
    StateUnavailable,
    #[error("roadmap storage operation {operation} failed: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },
}

impl RoadmapStoreError {
    pub(crate) fn storage(operation: &'static str, error: impl fmt::Display) -> Self {
        Self::Storage {
            operation,
            message: error.to_string(),
        }
    }
}

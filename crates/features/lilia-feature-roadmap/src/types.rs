use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MilestoneStatus {
    #[default]
    Upcoming,
    InProgress,
    Done,
    Abandoned,
}

impl MilestoneStatus {
    pub const ALL: [Self; 4] = [
        Self::Upcoming,
        Self::InProgress,
        Self::Done,
        Self::Abandoned,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Upcoming => "upcoming",
            Self::InProgress => "in-progress",
            Self::Done => "done",
            Self::Abandoned => "abandoned",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Option<Self> {
        match value {
            "upcoming" => Some(Self::Upcoming),
            "in-progress" => Some(Self::InProgress),
            "done" => Some(Self::Done),
            "abandoned" => Some(Self::Abandoned),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Milestone {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub status: MilestoneStatus,
    pub due_date: Option<i64>,
    pub order: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMilestoneLink {
    pub task_id: String,
    pub milestone_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRoadmap {
    pub milestones: Vec<Milestone>,
    pub links: Vec<TaskMilestoneLink>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MilestoneDueDateUpdate {
    #[default]
    Unchanged,
    Set(i64),
    Clear,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MilestoneUpdatePatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<MilestoneStatus>,
    pub due_date: MilestoneDueDateUpdate,
}

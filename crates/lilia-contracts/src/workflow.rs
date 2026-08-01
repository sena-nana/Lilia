use serde::{Deserialize, Serialize};

use crate::{AgentSessionRef, AssignmentId, ProductRevision, TaskId, WorkflowId, WorkflowRunId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductWorkflowStatus {
    Draft,
    Published,
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductWorkflow {
    pub id: WorkflowId,
    pub name: String,
    pub version: u64,
    pub status: ProductWorkflowStatus,
    pub definition_ref: Option<String>,
    pub revision: ProductRevision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductWorkflowRunStatus {
    Queued,
    Running,
    Waiting,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductWorkflowRun {
    pub id: WorkflowRunId,
    pub workflow_id: WorkflowId,
    pub workflow_version: u64,
    pub task_id: Option<TaskId>,
    pub status: ProductWorkflowRunStatus,
    pub node_projection_ref: Option<String>,
    pub agent_session: Option<AgentSessionRef>,
    pub revision: ProductRevision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    Proposed,
    Accepted,
    Active,
    Completed,
    Released,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductAssignment {
    pub id: AssignmentId,
    pub task_id: TaskId,
    pub role: String,
    pub assignee: String,
    pub agent_profile_id: Option<String>,
    pub status: AssignmentStatus,
    pub revision: ProductRevision,
}

impl ProductWorkflow {
    pub fn new(id: WorkflowId, name: impl Into<String>) -> Result<Self, crate::ProductError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(crate::ProductError::InvalidInput {
                field: "name".into(),
                message: "workflow name must not be empty".into(),
            });
        }
        Ok(Self {
            id,
            name,
            version: 1,
            status: ProductWorkflowStatus::Draft,
            definition_ref: None,
            revision: ProductRevision::INITIAL,
        })
    }

    pub fn publish(&mut self) -> bool {
        if self.status == ProductWorkflowStatus::Published {
            return false;
        }
        self.status = ProductWorkflowStatus::Published;
        self.revision = self.revision.next();
        true
    }

    pub fn set_enabled(&mut self, enabled: bool) -> bool {
        let next = if enabled {
            ProductWorkflowStatus::Published
        } else {
            ProductWorkflowStatus::Disabled
        };
        if self.status == next {
            return false;
        }
        self.status = next;
        self.revision = self.revision.next();
        true
    }
}

impl ProductWorkflowRun {
    pub fn new(
        id: WorkflowRunId,
        workflow: &ProductWorkflow,
        task_id: Option<TaskId>,
    ) -> Result<Self, crate::ProductError> {
        if workflow.status != ProductWorkflowStatus::Published {
            return Err(crate::ProductError::InvalidState {
                message: "workflow must be published before starting a run".into(),
            });
        }
        Ok(Self {
            id,
            workflow_id: workflow.id.clone(),
            workflow_version: workflow.version,
            task_id,
            status: ProductWorkflowRunStatus::Queued,
            node_projection_ref: None,
            agent_session: None,
            revision: ProductRevision::INITIAL,
        })
    }
}

impl ProductAssignment {
    pub fn new(
        id: AssignmentId,
        task_id: TaskId,
        role: impl Into<String>,
        assignee: impl Into<String>,
    ) -> Result<Self, crate::ProductError> {
        let role = role.into();
        let assignee = assignee.into();
        if role.trim().is_empty() {
            return Err(crate::ProductError::InvalidInput {
                field: "role".into(),
                message: "assignment role must not be empty".into(),
            });
        }
        if assignee.trim().is_empty() {
            return Err(crate::ProductError::InvalidInput {
                field: "assignee".into(),
                message: "assignment assignee must not be empty".into(),
            });
        }
        Ok(Self {
            id,
            task_id,
            role,
            assignee,
            agent_profile_id: None,
            status: AssignmentStatus::Proposed,
            revision: ProductRevision::INITIAL,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_run_requires_published_workflow_and_does_not_require_agent_session() {
        let mut workflow =
            ProductWorkflow::new(WorkflowId::new("workflow-1").unwrap(), "Review").unwrap();
        assert!(matches!(
            ProductWorkflowRun::new(
                WorkflowRunId::new("run-1").unwrap(),
                &workflow,
                Some(TaskId::new("task-1").unwrap())
            ),
            Err(crate::ProductError::InvalidState { .. })
        ));
        assert!(workflow.publish());
        let run = ProductWorkflowRun::new(
            WorkflowRunId::new("run-1").unwrap(),
            &workflow,
            Some(TaskId::new("task-1").unwrap()),
        )
        .unwrap();
        assert_eq!(run.status, ProductWorkflowRunStatus::Queued);
        assert!(run.agent_session.is_none());
    }
}

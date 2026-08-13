use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{AgentSessionRef, AssignmentId, ProductRevision, TaskId, WorkflowId, WorkflowRunId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum LiliaAgentWorkflow {
    #[serde(rename = "lilia_review")]
    LiliaReview {
        target: LiliaReviewTarget,
        #[serde(default)]
        instructions: Option<String>,
        #[serde(default)]
        delivery: Option<String>,
    },
    #[serde(rename = "lilia_fix_suggestion")]
    LiliaFixSuggestion {
        target: LiliaReviewTarget,
        #[serde(default)]
        instructions: Option<String>,
        #[serde(default)]
        mode: Option<String>,
    },
    #[serde(rename = "lilia_batch_apply")]
    LiliaBatchApply {
        source_turn_id: String,
        source_kind: String,
        source_summary: String,
        #[serde(default)]
        instructions: Option<String>,
    },
    #[serde(rename = "lilia_task_workflow")]
    LiliaTaskWorkflow {
        kind: String,
        #[serde(default)]
        instructions: Option<String>,
    },
    #[serde(rename = "lilia_goal")]
    LiliaGoal {
        action: String,
        #[serde(default)]
        objective: Option<String>,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        token_budget: Option<u64>,
    },
    #[serde(rename = "lilia_compact")]
    LiliaCompact,
    #[serde(rename = "lilia_background_terminals_clean")]
    LiliaBackgroundTerminalsClean,
    #[serde(rename = "lilia_memory_mode")]
    LiliaMemoryMode { mode: String },
    #[serde(rename = "lilia_memory_reset")]
    LiliaMemoryReset,
    #[serde(rename = "lilia_config_diagnostics")]
    LiliaConfigDiagnostics {
        #[serde(default)]
        include_layers: Option<bool>,
    },
    #[serde(rename = "automation")]
    Automation { automation_run_id: String },
    #[serde(rename = "slash_command")]
    SlashCommand {
        command_id: String,
        source: String,
        #[serde(default)]
        arguments: BTreeMap<String, String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum LiliaReviewTarget {
    #[serde(rename = "uncommittedChanges")]
    UncommittedChanges,
    #[serde(rename = "baseBranch")]
    BaseBranch { branch: String },
    #[serde(rename = "commit")]
    Commit { sha: String },
}

impl LiliaAgentWorkflow {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::LiliaReview { .. } => "lilia_review",
            Self::LiliaFixSuggestion { .. } => "lilia_fix_suggestion",
            Self::LiliaBatchApply { .. } => "lilia_batch_apply",
            Self::LiliaTaskWorkflow { .. } => "lilia_task_workflow",
            Self::LiliaGoal { .. } => "lilia_goal",
            Self::LiliaCompact => "lilia_compact",
            Self::LiliaBackgroundTerminalsClean => "lilia_background_terminals_clean",
            Self::LiliaMemoryMode { .. } => "lilia_memory_mode",
            Self::LiliaMemoryReset => "lilia_memory_reset",
            Self::LiliaConfigDiagnostics { .. } => "lilia_config_diagnostics",
            Self::Automation { .. } => "automation",
            Self::SlashCommand { .. } => "slash_command",
        }
    }

    pub fn automation_run_id(&self) -> Option<&str> {
        match self {
            Self::Automation { automation_run_id } => Some(automation_run_id),
            _ => None,
        }
    }
}

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
    use serde_json::json;

    const AGENT_WORKFLOW_CONTRACT: &str = include_str!("../contracts/lilia-workflow-contract.json");

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

    #[test]
    fn agent_workflows_match_the_cross_end_manifest_and_round_trip() {
        let declared = serde_json::from_str::<serde_json::Value>(AGENT_WORKFLOW_CONTRACT).unwrap();
        let workflows = [
            LiliaAgentWorkflow::LiliaReview {
                target: LiliaReviewTarget::UncommittedChanges,
                instructions: None,
                delivery: None,
            },
            LiliaAgentWorkflow::LiliaFixSuggestion {
                target: LiliaReviewTarget::BaseBranch {
                    branch: "main".into(),
                },
                instructions: Some("inspect".into()),
                mode: Some("suggest".into()),
            },
            LiliaAgentWorkflow::LiliaBatchApply {
                source_turn_id: "turn-1".into(),
                source_kind: "review".into(),
                source_summary: "summary".into(),
                instructions: None,
            },
            LiliaAgentWorkflow::LiliaTaskWorkflow {
                kind: "frontend".into(),
                instructions: None,
            },
            LiliaAgentWorkflow::LiliaGoal {
                action: "set".into(),
                objective: Some("finish".into()),
                status: Some("active".into()),
                token_budget: Some(1_000),
            },
            LiliaAgentWorkflow::LiliaCompact,
            LiliaAgentWorkflow::LiliaBackgroundTerminalsClean,
            LiliaAgentWorkflow::LiliaMemoryMode {
                mode: "enabled".into(),
            },
            LiliaAgentWorkflow::LiliaMemoryReset,
            LiliaAgentWorkflow::LiliaConfigDiagnostics {
                include_layers: Some(true),
            },
            LiliaAgentWorkflow::Automation {
                automation_run_id: "run-1".into(),
            },
            LiliaAgentWorkflow::SlashCommand {
                command_id: "native:help".into(),
                source: "native".into(),
                arguments: BTreeMap::new(),
            },
        ];

        for workflow in workflows {
            let encoded = serde_json::to_value(&workflow).unwrap();
            assert_eq!(encoded["type"], json!(workflow.kind()));
            let entry = match workflow.kind() {
                "lilia_task_workflow" => &declared["taskWorkflow"],
                "lilia_review" => &declared["review"],
                "lilia_fix_suggestion" => &declared["fixSuggestion"],
                "lilia_batch_apply" => &declared["batchApply"],
                "lilia_goal" => &declared["goal"],
                "lilia_compact" => &declared["compact"],
                "lilia_background_terminals_clean" => &declared["backgroundTerminalsClean"],
                "lilia_memory_mode" => &declared["memoryMode"],
                "lilia_memory_reset" => &declared["memoryReset"],
                "lilia_config_diagnostics" => &declared["configDiagnostics"],
                "automation" => &declared["automation"],
                "slash_command" => &declared["slashCommand"],
                other => panic!("missing manifest route for {other}"),
            };
            assert_eq!(entry["type"], json!(workflow.kind()));
            assert_eq!(
                serde_json::from_value::<LiliaAgentWorkflow>(encoded).unwrap(),
                workflow
            );
        }
    }
}

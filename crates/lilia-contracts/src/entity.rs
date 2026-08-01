use serde::{Deserialize, Serialize};

use crate::{
    AgentSessionBinding, ProductArtifact, ProductAssignment, ProductConversation, ProductMilestone,
    ProductRevision, ProductTask, ProductWorkflow, ProductWorkflowRun, Project, ProjectAsset,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductEntityKind {
    Project,
    Task,
    Conversation,
    Milestone,
    Binding,
    Workflow,
    WorkflowRun,
    Assignment,
    Artifact,
    ProjectAsset,
}

impl ProductEntityKind {
    pub const ALL: [Self; 10] = [
        Self::Project,
        Self::Task,
        Self::Conversation,
        Self::Milestone,
        Self::Binding,
        Self::Workflow,
        Self::WorkflowRun,
        Self::Assignment,
        Self::Artifact,
        Self::ProjectAsset,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Task => "task",
            Self::Conversation => "conversation",
            Self::Milestone => "milestone",
            Self::Binding => "binding",
            Self::Workflow => "workflow",
            Self::WorkflowRun => "workflow_run",
            Self::Assignment => "assignment",
            Self::Artifact => "artifact",
            Self::ProjectAsset => "project_asset",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ProductEntity {
    Project(Project),
    Task(ProductTask),
    Conversation(ProductConversation),
    Milestone(ProductMilestone),
    Binding(AgentSessionBinding),
    Workflow(ProductWorkflow),
    WorkflowRun(ProductWorkflowRun),
    Assignment(ProductAssignment),
    Artifact(ProductArtifact),
    ProjectAsset(ProjectAsset),
}

impl ProductEntity {
    pub fn kind(&self) -> ProductEntityKind {
        match self {
            Self::Project(_) => ProductEntityKind::Project,
            Self::Task(_) => ProductEntityKind::Task,
            Self::Conversation(_) => ProductEntityKind::Conversation,
            Self::Milestone(_) => ProductEntityKind::Milestone,
            Self::Binding(_) => ProductEntityKind::Binding,
            Self::Workflow(_) => ProductEntityKind::Workflow,
            Self::WorkflowRun(_) => ProductEntityKind::WorkflowRun,
            Self::Assignment(_) => ProductEntityKind::Assignment,
            Self::Artifact(_) => ProductEntityKind::Artifact,
            Self::ProjectAsset(_) => ProductEntityKind::ProjectAsset,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Project(value) => value.id.as_str(),
            Self::Task(value) => value.id.as_str(),
            Self::Conversation(value) => value.id.as_str(),
            Self::Milestone(value) => value.id.as_str(),
            Self::Binding(value) => value.binding_id.as_str(),
            Self::Workflow(value) => value.id.as_str(),
            Self::WorkflowRun(value) => value.id.as_str(),
            Self::Assignment(value) => value.id.as_str(),
            Self::Artifact(value) => value.id.as_str(),
            Self::ProjectAsset(value) => value.id.as_str(),
        }
    }

    pub fn revision(&self) -> ProductRevision {
        match self {
            Self::Project(value) => value.revision,
            Self::Task(value) => value.revision,
            Self::Conversation(value) => value.revision,
            Self::Milestone(value) => value.revision,
            Self::Binding(value) => value.revision,
            Self::Workflow(value) => value.revision,
            Self::WorkflowRun(value) => value.revision,
            Self::Assignment(value) => value.revision,
            Self::Artifact(value) => value.revision,
            Self::ProjectAsset(value) => value.revision,
        }
    }

    pub fn set_revision(&mut self, revision: ProductRevision) {
        match self {
            Self::Project(value) => value.revision = revision,
            Self::Task(value) => value.revision = revision,
            Self::Conversation(value) => value.revision = revision,
            Self::Milestone(value) => value.revision = revision,
            Self::Binding(value) => value.revision = revision,
            Self::Workflow(value) => value.revision = revision,
            Self::WorkflowRun(value) => value.revision = revision,
            Self::Assignment(value) => value.revision = revision,
            Self::Artifact(value) => value.revision = revision,
            Self::ProjectAsset(value) => value.revision = revision,
        }
    }
}

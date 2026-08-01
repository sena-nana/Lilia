use lilia_contracts::{
    AgentSessionRef, ArtifactId, ArtifactMaterializationStatus, AssignmentId, BindingId,
    ConversationId, ExpectedRevision, MilestoneId, ProductArtifact, ProductAssignment,
    ProductConversation, ProductEntity, ProductEntityKind, ProductMilestone, ProductResult,
    ProductWorkflow, ProductWorkflowRun, ProjectAsset, ProjectAssetId, ProjectAssetKind, ProjectId,
    TaskId, WorkflowId, WorkflowRunId,
};

use super::ProductServices;

impl ProductServices {
    pub fn create_conversation(
        &self,
        id: ConversationId,
        project_id: Option<ProjectId>,
        task_id: Option<TaskId>,
        title: impl Into<String>,
    ) -> ProductResult<ProductConversation> {
        if let Some(project_id) = &project_id {
            self.get_project(project_id)?;
        }
        if let Some(task_id) = &task_id {
            self.get_task(task_id)?;
        }
        let conversation = ProductConversation::new(id, project_id, task_id, title)?;
        entity_conversation(self.create_entity(ProductEntity::Conversation(conversation))?)
    }

    pub fn fork_conversation(
        &self,
        source_id: &ConversationId,
        id: ConversationId,
        title: impl Into<String>,
    ) -> ProductResult<ProductConversation> {
        let source = self.get_conversation(source_id)?;
        let conversation = ProductConversation::fork(id, &source, title)?;
        entity_conversation(self.create_entity(ProductEntity::Conversation(conversation))?)
    }

    pub fn get_conversation(&self, id: &ConversationId) -> ProductResult<ProductConversation> {
        entity_conversation(self.get_entity(ProductEntityKind::Conversation, id.as_str())?)
    }

    pub fn list_conversations(&self) -> ProductResult<Vec<ProductConversation>> {
        self.list_entities(ProductEntityKind::Conversation)?
            .into_iter()
            .map(entity_conversation)
            .collect()
    }

    pub fn bind_conversation_session(
        &self,
        conversation_id: &ConversationId,
        binding_id: BindingId,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductConversation> {
        let mut conversation = self.get_conversation(conversation_id)?;
        conversation.bind_session(binding_id);
        entity_conversation(
            self.update_entity(ProductEntity::Conversation(conversation), expected)?,
        )
    }

    pub fn advance_conversation_timeline(
        &self,
        conversation_id: &ConversationId,
        cursor: u64,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductConversation> {
        let mut conversation = self.get_conversation(conversation_id)?;
        conversation.advance_timeline_cursor(cursor)?;
        entity_conversation(
            self.update_entity(ProductEntity::Conversation(conversation), expected)?,
        )
    }

    pub fn create_milestone(
        &self,
        id: MilestoneId,
        project_id: ProjectId,
        title: impl Into<String>,
    ) -> ProductResult<ProductMilestone> {
        self.get_project(&project_id)?;
        let milestone = ProductMilestone::new(id, project_id, title)?;
        entity_milestone(self.create_entity(ProductEntity::Milestone(milestone))?)
    }

    pub fn create_workflow(
        &self,
        id: WorkflowId,
        name: impl Into<String>,
    ) -> ProductResult<ProductWorkflow> {
        let workflow = ProductWorkflow::new(id, name)?;
        entity_workflow(self.create_entity(ProductEntity::Workflow(workflow))?)
    }

    pub fn publish_workflow(
        &self,
        workflow_id: &WorkflowId,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductWorkflow> {
        let mut workflow =
            entity_workflow(self.get_entity(ProductEntityKind::Workflow, workflow_id.as_str())?)?;
        workflow.publish();
        entity_workflow(self.update_entity(ProductEntity::Workflow(workflow), expected)?)
    }

    pub fn start_workflow_run(
        &self,
        id: WorkflowRunId,
        workflow_id: &WorkflowId,
        task_id: Option<TaskId>,
    ) -> ProductResult<ProductWorkflowRun> {
        let workflow =
            entity_workflow(self.get_entity(ProductEntityKind::Workflow, workflow_id.as_str())?)?;
        if let Some(task_id) = &task_id {
            self.get_task(task_id)?;
        }
        let run = ProductWorkflowRun::new(id, &workflow, task_id)?;
        entity_workflow_run(self.create_entity(ProductEntity::WorkflowRun(run))?)
    }

    pub fn create_assignment(
        &self,
        id: AssignmentId,
        task_id: TaskId,
        role: impl Into<String>,
        assignee: impl Into<String>,
    ) -> ProductResult<ProductAssignment> {
        self.get_task(&task_id)?;
        let assignment = ProductAssignment::new(id, task_id, role, assignee)?;
        entity_assignment(self.create_entity(ProductEntity::Assignment(assignment))?)
    }

    pub fn attach_artifact(
        &self,
        id: ArtifactId,
        task_id: TaskId,
        agent_session: AgentSessionRef,
        artifact_ref: impl Into<String>,
        media_type: impl Into<String>,
    ) -> ProductResult<ProductArtifact> {
        self.get_task(&task_id)?;
        let artifact = ProductArtifact::new(id, task_id, agent_session, artifact_ref, media_type)?;
        entity_artifact(self.create_entity(ProductEntity::Artifact(artifact))?)
    }

    pub fn materialize_artifact(
        &self,
        artifact_id: &ArtifactId,
        resource_ref: String,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductArtifact> {
        let mut artifact =
            entity_artifact(self.get_entity(ProductEntityKind::Artifact, artifact_id.as_str())?)?;
        artifact.set_materialization(
            ArtifactMaterializationStatus::Materialized,
            Some(resource_ref),
        )?;
        entity_artifact(self.update_entity(ProductEntity::Artifact(artifact), expected)?)
    }

    pub fn create_project_asset(
        &self,
        id: ProjectAssetId,
        project_id: ProjectId,
        kind: ProjectAssetKind,
        title: impl Into<String>,
        content_ref: impl Into<String>,
    ) -> ProductResult<ProjectAsset> {
        self.get_project(&project_id)?;
        let asset = ProjectAsset::new(id, project_id, kind, title, content_ref)?;
        entity_project_asset(self.create_entity(ProductEntity::ProjectAsset(asset))?)
    }
}

macro_rules! entity_decoder {
    ($name:ident, $variant:ident, $ty:ty) => {
        fn $name(entity: ProductEntity) -> ProductResult<$ty> {
            match entity {
                ProductEntity::$variant(value) => Ok(value),
                other => Err(lilia_contracts::ProductError::InvalidState {
                    message: format!(
                        "expected {}, received {}",
                        ProductEntityKind::$variant.as_str(),
                        other.kind().as_str()
                    ),
                }),
            }
        }
    };
}

entity_decoder!(entity_conversation, Conversation, ProductConversation);
entity_decoder!(entity_milestone, Milestone, ProductMilestone);
entity_decoder!(entity_workflow, Workflow, ProductWorkflow);
entity_decoder!(entity_workflow_run, WorkflowRun, ProductWorkflowRun);
entity_decoder!(entity_assignment, Assignment, ProductAssignment);
entity_decoder!(entity_artifact, Artifact, ProductArtifact);
entity_decoder!(entity_project_asset, ProjectAsset, ProjectAsset);

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use lilia_contracts::{ProductRevision, ProjectId, TaskId};

    use super::*;
    use crate::application::InMemoryProductStore;

    #[test]
    fn core_use_cases_run_without_agentkit_or_host_dependencies() {
        let products = ProductServices::new(Arc::new(Mutex::new(InMemoryProductStore::new())));
        let project = products
            .create_project(ProjectId::new("project-1").unwrap(), "Product")
            .unwrap();
        let task = products
            .create_task(
                TaskId::new("task-1").unwrap(),
                Some(project.id.clone()),
                "Implement",
            )
            .unwrap();
        let conversation = products
            .create_conversation(
                ConversationId::new("conversation-1").unwrap(),
                Some(project.id),
                Some(task.id.clone()),
                "Implementation",
            )
            .unwrap();
        let workflow = products
            .create_workflow(WorkflowId::new("workflow-1").unwrap(), "Review")
            .unwrap();
        let workflow = products
            .publish_workflow(
                &workflow.id,
                ExpectedRevision::new(workflow.revision.get()).unwrap(),
            )
            .unwrap();
        let run = products
            .start_workflow_run(
                WorkflowRunId::new("run-1").unwrap(),
                &workflow.id,
                Some(task.id),
            )
            .unwrap();

        assert_eq!(conversation.revision, ProductRevision::INITIAL);
        assert!(run.agent_session.is_none());
    }

    #[test]
    fn stale_revision_is_rejected_for_generic_product_entities() {
        let products = ProductServices::new(Arc::new(Mutex::new(InMemoryProductStore::new())));
        let workflow = products
            .create_workflow(WorkflowId::new("workflow-1").unwrap(), "Review")
            .unwrap();
        let expected = ExpectedRevision::new(workflow.revision.get()).unwrap();
        products.publish_workflow(&workflow.id, expected).unwrap();
        assert!(matches!(
            products.publish_workflow(&workflow.id, expected),
            Err(lilia_contracts::ProductError::Conflict {
                conflict: lilia_contracts::ConflictKind::StaleRevision,
                ..
            })
        ));
    }
}

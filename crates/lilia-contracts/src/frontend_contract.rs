use std::sync::OnceLock;

use serde::Deserialize;

/// Frontend and non-Rust clients consume this checked-in authority manifest.
pub const PRODUCT_CORE_FRONTEND_CONTRACT_JSON: &str =
    include_str!("../../../packages/contracts/src/product-core-contract.json");

#[derive(Deserialize)]
struct FrontendContract {
    events: FrontendEvents,
}

#[derive(Deserialize)]
struct FrontendEvents {
    product: String,
}

pub fn product_event_name() -> &'static str {
    static CONTRACT: OnceLock<FrontendContract> = OnceLock::new();
    &CONTRACT
        .get_or_init(|| {
            serde_json::from_str(PRODUCT_CORE_FRONTEND_CONTRACT_JSON)
                .expect("product-core-contract.json must be valid")
        })
        .events
        .product
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::Value;

    use crate::{
        AgentSessionBinding, AgentSessionRef, ArtifactId, AssignmentId, BindingId, ConversationId,
        MilestoneId, ProductArtifact, ProductAssignment, ProductConversation, ProductEntity,
        ProductEntityKind, ProductMilestone, ProductRevision, ProductTask, ProductWorkflow,
        ProductWorkflowRun, Project, ProjectAsset, ProjectAssetId, ProjectAssetKind, ProjectId,
        TaskId, WorkflowId, WorkflowRunId,
    };

    use super::PRODUCT_CORE_FRONTEND_CONTRACT_JSON;

    #[test]
    fn frontend_manifest_matches_serialized_rust_product_entities() {
        let manifest: Value = serde_json::from_str(PRODUCT_CORE_FRONTEND_CONTRACT_JSON).unwrap();
        let manifest_kinds = manifest["entityKinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            manifest_kinds,
            ProductEntityKind::ALL
                .iter()
                .map(|kind| kind.as_str())
                .collect::<Vec<_>>()
        );

        for entity in fixtures() {
            let serialized = serde_json::to_value(&entity).unwrap();
            let kind = serialized["kind"].as_str().unwrap();
            let rust_fields = serialized["value"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            let manifest_fields = manifest["fields"][kind]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<BTreeSet<_>>();
            assert_eq!(manifest_fields, rust_fields, "contract drift for {kind}");
        }
    }

    fn fixtures() -> Vec<ProductEntity> {
        let project_id = ProjectId::new("project-1").unwrap();
        let task_id = TaskId::new("task-1").unwrap();
        let conversation_id = ConversationId::new("conversation-1").unwrap();
        let mut workflow =
            ProductWorkflow::new(WorkflowId::new("workflow-1").unwrap(), "Workflow").unwrap();
        workflow.publish();
        vec![
            ProductEntity::Project(Project::new(project_id.clone(), "Project").unwrap()),
            ProductEntity::Task(
                ProductTask::new(task_id.clone(), Some(project_id.clone()), "Task").unwrap(),
            ),
            ProductEntity::Conversation(
                ProductConversation::new(
                    conversation_id.clone(),
                    Some(project_id.clone()),
                    Some(task_id.clone()),
                    "Conversation",
                )
                .unwrap(),
            ),
            ProductEntity::Milestone(
                ProductMilestone::new(
                    MilestoneId::new("milestone-1").unwrap(),
                    project_id.clone(),
                    "Milestone",
                )
                .unwrap(),
            ),
            ProductEntity::Binding(AgentSessionBinding {
                binding_id: BindingId::new("binding-1").unwrap(),
                task_id: task_id.clone(),
                conversation_id: Some(conversation_id),
                agent_session: AgentSessionRef::new("session-1").unwrap(),
                profile_id: None,
                revision: ProductRevision::INITIAL,
            }),
            ProductEntity::Workflow(workflow.clone()),
            ProductEntity::WorkflowRun(
                ProductWorkflowRun::new(
                    WorkflowRunId::new("run-1").unwrap(),
                    &workflow,
                    Some(task_id.clone()),
                )
                .unwrap(),
            ),
            ProductEntity::Assignment(
                ProductAssignment::new(
                    AssignmentId::new("assignment-1").unwrap(),
                    task_id.clone(),
                    "reviewer",
                    "user-1",
                )
                .unwrap(),
            ),
            ProductEntity::Artifact(
                ProductArtifact::new(
                    ArtifactId::new("artifact-1").unwrap(),
                    task_id,
                    AgentSessionRef::new("session-1").unwrap(),
                    "artifact-ref-1",
                    "text/plain",
                )
                .unwrap(),
            ),
            ProductEntity::ProjectAsset(
                ProjectAsset::new(
                    ProjectAssetId::new("asset-1").unwrap(),
                    project_id,
                    ProjectAssetKind::Architecture,
                    "Architecture",
                    "resource://architecture",
                )
                .unwrap(),
            ),
        ]
    }
}

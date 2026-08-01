use serde::{Deserialize, Serialize};

use crate::{AgentSessionRef, ArtifactId, ProductRevision, ProjectAssetId, ProjectId, TaskId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactMaterializationStatus {
    Referenced,
    Materialized,
    Missing,
    Archived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRetention {
    Session,
    Task,
    Project,
    Permanent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductArtifact {
    pub id: ArtifactId,
    pub task_id: TaskId,
    pub agent_session: AgentSessionRef,
    pub source_event_id: Option<String>,
    pub artifact_ref: String,
    pub resource_ref: Option<String>,
    pub media_type: String,
    pub materialization: ArtifactMaterializationStatus,
    pub retention: ArtifactRetention,
    pub provenance: Option<String>,
    pub revision: ProductRevision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectAssetKind {
    Architecture,
    DesignPrinciple,
    Specification,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectAssetProposalStatus {
    Draft,
    Proposed,
    Applied,
    Rejected,
    RolledBack,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAsset {
    pub id: ProjectAssetId,
    pub project_id: ProjectId,
    pub kind: ProjectAssetKind,
    pub title: String,
    pub content_ref: String,
    pub version: u64,
    pub proposal_status: ProjectAssetProposalStatus,
    pub rollback_of: Option<ProjectAssetId>,
    pub revision: ProductRevision,
}

impl ProductArtifact {
    pub fn new(
        id: ArtifactId,
        task_id: TaskId,
        agent_session: AgentSessionRef,
        artifact_ref: impl Into<String>,
        media_type: impl Into<String>,
    ) -> Result<Self, crate::ProductError> {
        let artifact_ref = artifact_ref.into();
        let media_type = media_type.into();
        if artifact_ref.trim().is_empty() {
            return Err(crate::ProductError::InvalidInput {
                field: "artifact_ref".into(),
                message: "artifact_ref must not be empty".into(),
            });
        }
        if media_type.trim().is_empty() {
            return Err(crate::ProductError::InvalidInput {
                field: "media_type".into(),
                message: "media_type must not be empty".into(),
            });
        }
        Ok(Self {
            id,
            task_id,
            agent_session,
            source_event_id: None,
            artifact_ref,
            resource_ref: None,
            media_type,
            materialization: ArtifactMaterializationStatus::Referenced,
            retention: ArtifactRetention::Session,
            provenance: None,
            revision: ProductRevision::INITIAL,
        })
    }

    pub fn set_materialization(
        &mut self,
        status: ArtifactMaterializationStatus,
        resource_ref: Option<String>,
    ) -> Result<bool, crate::ProductError> {
        if status == ArtifactMaterializationStatus::Materialized
            && resource_ref.as_deref().is_none_or(str::is_empty)
        {
            return Err(crate::ProductError::InvalidInput {
                field: "resource_ref".into(),
                message: "materialized artifact requires resource_ref".into(),
            });
        }
        if self.materialization == status && self.resource_ref == resource_ref {
            return Ok(false);
        }
        self.materialization = status;
        self.resource_ref = resource_ref;
        self.revision = self.revision.next();
        Ok(true)
    }
}

impl ProjectAsset {
    pub fn new(
        id: ProjectAssetId,
        project_id: ProjectId,
        kind: ProjectAssetKind,
        title: impl Into<String>,
        content_ref: impl Into<String>,
    ) -> Result<Self, crate::ProductError> {
        let title = title.into();
        let content_ref = content_ref.into();
        if title.trim().is_empty() || content_ref.trim().is_empty() {
            return Err(crate::ProductError::InvalidInput {
                field: if title.trim().is_empty() {
                    "title".into()
                } else {
                    "content_ref".into()
                },
                message: "project asset title and content_ref must not be empty".into(),
            });
        }
        Ok(Self {
            id,
            project_id,
            kind,
            title,
            content_ref,
            version: 1,
            proposal_status: ProjectAssetProposalStatus::Draft,
            rollback_of: None,
            revision: ProductRevision::INITIAL,
        })
    }

    pub fn transition_proposal(
        &mut self,
        next: ProjectAssetProposalStatus,
    ) -> Result<bool, crate::ProductError> {
        let allowed = matches!(
            (self.proposal_status, next),
            (
                ProjectAssetProposalStatus::Draft,
                ProjectAssetProposalStatus::Proposed
            ) | (
                ProjectAssetProposalStatus::Proposed,
                ProjectAssetProposalStatus::Applied
            ) | (
                ProjectAssetProposalStatus::Proposed,
                ProjectAssetProposalStatus::Rejected
            ) | (
                ProjectAssetProposalStatus::Applied,
                ProjectAssetProposalStatus::RolledBack
            )
        );
        if self.proposal_status == next {
            return Ok(false);
        }
        if !allowed {
            return Err(crate::ProductError::InvalidState {
                message: "invalid project asset proposal transition".into(),
            });
        }
        self.proposal_status = next;
        self.revision = self.revision.next();
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialized_artifact_requires_resource_reference() {
        let mut artifact = ProductArtifact::new(
            ArtifactId::new("artifact-1").unwrap(),
            TaskId::new("task-1").unwrap(),
            AgentSessionRef::new("session-1").unwrap(),
            "agent-artifact-1",
            "text/plain",
        )
        .unwrap();
        assert!(matches!(
            artifact.set_materialization(ArtifactMaterializationStatus::Materialized, None),
            Err(crate::ProductError::InvalidInput { .. })
        ));
        assert!(artifact
            .set_materialization(
                ArtifactMaterializationStatus::Materialized,
                Some("resource://artifact-1".into())
            )
            .unwrap());
    }

    #[test]
    fn asset_proposal_uses_explicit_lifecycle() {
        let mut asset = ProjectAsset::new(
            ProjectAssetId::new("asset-1").unwrap(),
            ProjectId::new("project-1").unwrap(),
            ProjectAssetKind::Architecture,
            "Architecture",
            "resource://architecture",
        )
        .unwrap();
        assert!(asset
            .transition_proposal(ProjectAssetProposalStatus::Proposed)
            .unwrap());
        assert!(asset
            .transition_proposal(ProjectAssetProposalStatus::Applied)
            .unwrap());
        assert!(matches!(
            asset.transition_proposal(ProjectAssetProposalStatus::Rejected),
            Err(crate::ProductError::InvalidState { .. })
        ));
    }
}

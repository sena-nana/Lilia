//! Minimal Artifact pin / expire retention policy (#56).
//!
//! Large blobs stay out of SQLite; this module only mutates projection
//! metadata (`status` / pin / expires_at). Deleting a product association must
//! not delete artifacts still referenced by other rows.

use lilia_contracts::{ArtifactProjection, ProductError, ProductResult, TaskId};

use crate::timeline::TimelineProjectionRepository;

/// Artifact retention statuses written into `artifact_projections.status`.
pub const ARTIFACT_STATUS_AVAILABLE: &str = "available";
pub const ARTIFACT_STATUS_PINNED: &str = "pinned";
pub const ARTIFACT_STATUS_EXPIRED: &str = "expired";
pub const ARTIFACT_STATUS_INACCESSIBLE: &str = "inaccessible";

/// Product version after which unpinned ephemeral artifacts may be expired.
/// Mirrors legacy runner cutoff honesty (#47) — not a hard delete gate.
pub const ARTIFACT_DEFAULT_COMPAT_UNTIL: &str = "1.0.0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactRetentionPolicy {
    /// Unpinned artifacts older than this age (ms) become `expired`.
    pub default_ttl_ms: u64,
    pub compat_until: &'static str,
}

impl Default for ArtifactRetentionPolicy {
    fn default() -> Self {
        Self {
            // 30 days — conservative default for local product projections.
            default_ttl_ms: 30 * 24 * 60 * 60 * 1000,
            compat_until: ARTIFACT_DEFAULT_COMPAT_UNTIL,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactPolicyDecision {
    pub artifact_id: String,
    pub previous_status: String,
    pub next_status: String,
    pub pinned: bool,
    pub reason: &'static str,
}

/// Evaluate whether an artifact should stay available, remain pinned, or expire.
pub fn evaluate_artifact(
    artifact: &ArtifactProjection,
    now_ms: u64,
    created_or_sequence_ms: Option<u64>,
    policy: &ArtifactRetentionPolicy,
) -> ArtifactPolicyDecision {
    let pinned = artifact.status == ARTIFACT_STATUS_PINNED;
    if pinned {
        return ArtifactPolicyDecision {
            artifact_id: artifact.artifact_id.clone(),
            previous_status: artifact.status.clone(),
            next_status: ARTIFACT_STATUS_PINNED.into(),
            pinned: true,
            reason: "pinned",
        };
    }
    if artifact.status == ARTIFACT_STATUS_INACCESSIBLE {
        return ArtifactPolicyDecision {
            artifact_id: artifact.artifact_id.clone(),
            previous_status: artifact.status.clone(),
            next_status: ARTIFACT_STATUS_INACCESSIBLE.into(),
            pinned: false,
            reason: "inaccessible",
        };
    }
    if artifact.status == ARTIFACT_STATUS_EXPIRED {
        return ArtifactPolicyDecision {
            artifact_id: artifact.artifact_id.clone(),
            previous_status: artifact.status.clone(),
            next_status: ARTIFACT_STATUS_EXPIRED.into(),
            pinned: false,
            reason: "already_expired",
        };
    }
    let age_anchor = created_or_sequence_ms.unwrap_or(artifact.sequence);
    let age = now_ms.saturating_sub(age_anchor);
    if age >= policy.default_ttl_ms {
        ArtifactPolicyDecision {
            artifact_id: artifact.artifact_id.clone(),
            previous_status: artifact.status.clone(),
            next_status: ARTIFACT_STATUS_EXPIRED.into(),
            pinned: false,
            reason: "ttl_exceeded",
        }
    } else {
        ArtifactPolicyDecision {
            artifact_id: artifact.artifact_id.clone(),
            previous_status: artifact.status.clone(),
            next_status: ARTIFACT_STATUS_AVAILABLE.into(),
            pinned: false,
            reason: "within_ttl",
        }
    }
}

/// Apply pin / expire decisions for one task against a projection repository.
pub fn apply_retention_for_task(
    store: &dyn TimelineProjectionRepository,
    task_id: &TaskId,
    now_ms: u64,
    policy: &ArtifactRetentionPolicy,
) -> ProductResult<Vec<ArtifactPolicyDecision>> {
    let artifacts = store.list_artifacts_for_task(task_id);
    let mut decisions = Vec::new();
    for mut artifact in artifacts {
        let decision = evaluate_artifact(&artifact, now_ms, None, policy);
        if decision.next_status != artifact.status {
            artifact.status = decision.next_status.clone();
            store.apply(lilia_contracts::TimelineProjectionCommand::UpsertArtifact { artifact })?;
        }
        decisions.push(decision);
    }
    Ok(decisions)
}

pub fn pin_artifact_row(artifact: &mut ArtifactProjection) -> ProductResult<()> {
    if artifact.content_ref.is_none() && artifact.content_hash.is_none() {
        return Err(ProductError::InvalidState {
            message: "cannot pin artifact without content_ref or content_hash".into(),
        });
    }
    artifact.status = ARTIFACT_STATUS_PINNED.into();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryTimelineProjectionStore;
    use lilia_contracts::{AgentSessionRef, TimelineProjectionCommand};
    use serde_json::json;

    fn sample(status: &str) -> ArtifactProjection {
        ArtifactProjection {
            id: "sess:art".into(),
            task_id: TaskId::new("task-art").unwrap(),
            agent_session: AgentSessionRef::new("sess").unwrap(),
            sequence: 1,
            turn_id: None,
            artifact_id: "art".into(),
            media_type: "text/plain".into(),
            summary: "note".into(),
            kind: Some("file".into()),
            size_bytes: Some(3),
            content_hash: Some("h".into()),
            content_ref: Some(json!({ "id": "r1" })),
            provenance: Some("test".into()),
            status: status.into(),
        }
    }

    #[test]
    fn pin_blocks_expire() {
        let policy = ArtifactRetentionPolicy {
            default_ttl_ms: 1,
            ..Default::default()
        };
        let mut art = sample(ARTIFACT_STATUS_PINNED);
        let decision = evaluate_artifact(&art, 1_000_000, Some(0), &policy);
        assert_eq!(decision.next_status, ARTIFACT_STATUS_PINNED);
        pin_artifact_row(&mut art).unwrap();
        assert_eq!(art.status, ARTIFACT_STATUS_PINNED);
    }

    #[test]
    fn ttl_expires_unpinned() {
        let store = InMemoryTimelineProjectionStore::new();
        let task = TaskId::new("task-art").unwrap();
        store
            .apply(TimelineProjectionCommand::UpsertArtifact {
                artifact: sample(ARTIFACT_STATUS_AVAILABLE),
            })
            .unwrap();
        let policy = ArtifactRetentionPolicy {
            default_ttl_ms: 10,
            ..Default::default()
        };
        let decisions = apply_retention_for_task(&store, &task, 100, &policy).unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].next_status, ARTIFACT_STATUS_EXPIRED);
        assert_eq!(
            store.list_artifacts_for_task(&task)[0].status,
            ARTIFACT_STATUS_EXPIRED
        );
    }
}

//! Project domain feature.
//!
//! Owns the repository clone protocol. The feature declares the protocol and
//! its handler; the kernel's [`Jobs`](lilia_kernel::Jobs) facade owns
//! scheduling, single-flight slots, cancellation and terminal state, so nothing
//! here tracks an operation sequence or spawns a worker thread.

mod clone;

use std::sync::Arc;

use lilia_kernel::{
    Feature, FeatureContext, FeatureId, JobContext, JobProtocol, JobSlot, KernelError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use clone::{
    clone, clone_with_command, clone_with_github_token, CloneCommandFactory, CloneError,
    CloneProgress, CloneRequest, CloneResult,
};

pub const CLONE_PROTOCOL: &str = "lilia.project/clone@1";

/// Payload of [`CLONE_PROTOCOL`].
///
/// Carries no credential: when `use_github_binding` is set the handler resolves
/// the token through [`CloneCredentials`] on the worker thread, so no secret
/// reaches the task payload or the journal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneJobRequest {
    #[serde(flatten)]
    pub clone: CloneRequest,
    #[serde(default)]
    pub use_github_binding: bool,
}

/// Resolves the product's GitHub binding for a clone that needs one.
pub trait CloneCredentials: Send + Sync + 'static {
    fn github_token(&self, repository: &str) -> Result<Option<Vec<u8>>, String>;
}

/// Credentials port for hosts without a GitHub binding, such as tests.
pub struct NoCloneCredentials;

impl CloneCredentials for NoCloneCredentials {
    fn github_token(&self, _repository: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}

/// Single-flight lane for repository clones. A second clone submission cancels
/// the first, which is what the shell used to emulate with
/// `project_clone_operation_sequence` plus `active_project_clone_operation`.
pub fn clone_slot() -> JobSlot {
    JobSlot::new("lilia.project.clone").expect("the clone slot name is not blank")
}

pub struct ProjectFeature {
    credentials: Arc<dyn CloneCredentials>,
}

impl ProjectFeature {
    pub fn new(credentials: Arc<dyn CloneCredentials>) -> Self {
        Self { credentials }
    }
}

impl Default for ProjectFeature {
    fn default() -> Self {
        Self::new(Arc::new(NoCloneCredentials))
    }
}

impl Feature for ProjectFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.project").expect("the project feature id is not blank")
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        let credentials = Arc::clone(&self.credentials);
        vec![JobProtocol::new(
            CLONE_PROTOCOL,
            Arc::new(move |payload, context: &JobContext| {
                run_clone_job(payload, context, credentials.as_ref())
            }),
        )]
    }

    fn mount(&self, _cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        Ok(())
    }
}

fn run_clone_job(
    payload: Value,
    context: &JobContext,
    credentials: &dyn CloneCredentials,
) -> Result<Value, String> {
    let request: CloneJobRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid clone request: {error}"))?;
    let token = if request.use_github_binding {
        credentials.github_token(&request.clone.repository)?
    } else {
        None
    };
    let result = match token {
        Some(token) => clone_with_github_token(request.clone, token, context),
        None => clone(request.clone, context),
    }
    .map_err(|error| error.to_string())?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    struct RecordingCredentials;

    impl CloneCredentials for RecordingCredentials {
        fn github_token(&self, _repository: &str) -> Result<Option<Vec<u8>>, String> {
            Err("the binding expired".to_owned())
        }
    }

    #[test]
    fn the_feature_declares_its_clone_protocol_before_any_mount() {
        let protocols = ProjectFeature::default().protocols();

        assert_eq!(protocols.len(), 1);
        assert_eq!(protocols[0].id, CLONE_PROTOCOL);
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_clone_job(
            serde_json::json!({ "repository": 7 }),
            &JobContext::new(),
            &NoCloneCredentials,
        )
        .expect_err("a malformed request cannot be cloned");

        assert!(error.contains("invalid clone request"), "{error}");
    }

    #[test]
    fn a_github_clone_fails_before_git_when_the_binding_cannot_be_resolved() {
        let request = CloneJobRequest {
            clone: CloneRequest {
                repository: "https://github.com/example/repository.git".to_owned(),
                parent_directory: PathBuf::from("missing"),
            },
            use_github_binding: true,
        };

        let error = run_clone_job(
            serde_json::to_value(request).unwrap(),
            &JobContext::new(),
            &RecordingCredentials,
        )
        .expect_err("an unresolvable binding stops the clone");

        assert_eq!(error, "the binding expired");
    }
}

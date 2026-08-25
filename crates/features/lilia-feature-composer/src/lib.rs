//! Composer domain feature.
//!
//! Owns the durable per-task draft: its reducer, its optimistic revision and
//! its SQLite home. Turn dispatch lives in the agent-session domain; this crate
//! only guarantees that a draft mutation is revision-safe and durable.

mod prompt;
mod state;
mod store;

use std::sync::Arc;

use lilia_contracts::TaskId;
use lilia_kernel::{
    Event, EventBus, Feature, FeatureContext, FeatureId, JobContext, JobProtocol, KernelError,
    ServiceKey, ServiceRef,
};
use lilia_storage::Db;
use serde_json::Value;

pub use prompt::{
    optimize_prompt_slot, PromptOptimizeInput, PromptOptimizePort, PromptOptimizeResult,
    PromptRoute, OPTIMIZE_PROMPT_PROTOCOL,
};
pub use state::{ensure_expected_revision, ComposerCommand, ComposerState};
pub use store::ComposerStore;

#[derive(Debug, thiserror::Error)]
pub enum ComposerError {
    #[error("composer revision overflowed")]
    RevisionOverflow,
    #[error("composer revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("composer serialization failed for {field}: {message}")]
    Serialization {
        field: &'static str,
        message: String,
    },
    #[error("composer storage failed during {operation}: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },
}

/// Published whenever a draft reaches a new revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerChanged {
    pub task_id: TaskId,
    pub revision: u64,
}

impl Event for ComposerChanged {
    const NAME: &'static str = "lilia.composer.changed";
}

/// Authority over composer drafts.
pub struct ComposerService {
    store: ComposerStore,
    events: EventBus,
}

impl ComposerService {
    pub fn new(store: ComposerStore, events: EventBus) -> Self {
        Self { store, events }
    }

    pub fn snapshot(&self, task_id: &TaskId) -> Result<ComposerState, ComposerError> {
        self.store.snapshot(task_id)
    }

    pub fn execute(
        &self,
        task_id: &TaskId,
        command: ComposerCommand,
    ) -> Result<(ComposerState, bool), ComposerError> {
        let (state, changed) = self.store.execute(task_id, command)?;
        if changed {
            self.publish(&state);
        }
        Ok((state, changed))
    }

    pub fn save(&self, state: &ComposerState) -> Result<(), ComposerError> {
        self.store.save(state)
    }

    pub fn remove(&self, task_id: &TaskId) -> Result<(), ComposerError> {
        self.store.remove(task_id)
    }

    /// Announces a revision the caller committed through another path, such as
    /// a turn submission that cleared the dispatched payload in its own
    /// transaction.
    pub fn publish(&self, state: &ComposerState) {
        self.events.publish(ComposerChanged {
            task_id: state.task_id.clone(),
            revision: state.revision,
        });
    }
}

/// Service slot for [`ComposerService`].
pub enum ComposerServiceKey {}

impl ServiceKey for ComposerServiceKey {
    type Value = Arc<ComposerService>;

    const NAME: &'static str = "lilia.composer";
}

pub struct ComposerFeature {
    db: Db,
    prompt_optimize: Arc<dyn PromptOptimizePort>,
}

impl ComposerFeature {
    pub fn new(db: Db, prompt_optimize: Arc<dyn PromptOptimizePort>) -> Self {
        Self {
            db,
            prompt_optimize,
        }
    }
}

impl Feature for ComposerFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.composer").expect("the composer feature id is not blank")
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<ComposerServiceKey>()]
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        let port = Arc::clone(&self.prompt_optimize);
        vec![JobProtocol::new(
            OPTIMIZE_PROMPT_PROTOCOL,
            Arc::new(move |payload, _context: &JobContext| {
                run_optimize_prompt_job(payload, port.as_ref())
            }),
        )]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        let store = ComposerStore::new(self.db.clone()).map_err(|error| KernelError::Mount {
            feature: self.id(),
            source: Box::new(error),
        })?;
        let service = Arc::new(ComposerService::new(store, cx.events().clone()));
        cx.provide::<ComposerServiceKey>(service)
    }
}

fn run_optimize_prompt_job(payload: Value, port: &dyn PromptOptimizePort) -> Result<Value, String> {
    let input: PromptOptimizeInput = serde_json::from_value(payload)
        .map_err(|error| format!("invalid prompt optimization request: {error}"))?;
    let result = port.optimize(input)?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

#[cfg(test)]
mod prompt_job_tests {
    use super::*;

    struct FailingPort;

    impl PromptOptimizePort for FailingPort {
        fn optimize(&self, _input: PromptOptimizeInput) -> Result<PromptOptimizeResult, String> {
            Err("the auxiliary model is not configured".to_owned())
        }
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_optimize_prompt_job(serde_json::json!({ "prompt": 7 }), &FailingPort)
            .expect_err("a malformed request cannot be optimized");

        assert!(
            error.contains("invalid prompt optimization request"),
            "{error}"
        );
    }

    #[test]
    fn a_failing_port_fails_the_job_with_the_hosts_message() {
        let error = run_optimize_prompt_job(serde_json::json!({ "prompt": "ship it" }), &FailingPort)
            .expect_err("a failing auxiliary model fails the job");

        assert_eq!(error, "the auxiliary model is not configured");
    }
}

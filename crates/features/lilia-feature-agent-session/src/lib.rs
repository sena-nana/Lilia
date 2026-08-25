//! Agent session domain feature.
//!
//! Owns the two things the agent runtime does not: what a caller asked a turn
//! to do ([`turn`]) and which turns are still waiting for a free session
//! ([`queue`]). Turn execution, approval and interaction state stay with the
//! agent runtime; this crate never models a turn's lifecycle.

mod queue;
mod settings;
mod title;
mod turn;

use std::sync::Arc;

use lilia_kernel::{
    Feature, FeatureContext, FeatureId, JobProtocol, KernelError, ServiceKey, ServiceRef,
};
use lilia_storage::Db;

pub use queue::{
    DesktopTurnQueueError, DesktopTurnQueueStore, PersistedDesktopTurn, PersistedDesktopTurnState,
    QuarantinedDesktopTurn,
};
#[cfg(debug_assertions)]
pub use queue::PersistedDesktopTurnDebugState;
pub use settings::DesktopAutoTurnDecisionSettings;
pub use title::{title_slot, TitlePort, TitleRequest, TITLE_PROTOCOL};
pub use turn::{
    DesktopAutomaticTurnSelection, DesktopAutomationTurnCorrelation, DesktopSessionBranchAnchor,
    DesktopSessionBranchMode, DesktopTurnDispatch, DesktopTurnDispatchKind, DesktopTurnRequest,
};

/// Service slot for the pending-turn queue.
pub enum TurnQueueKey {}

impl ServiceKey for TurnQueueKey {
    type Value = Arc<DesktopTurnQueueStore>;

    const NAME: &'static str = "lilia.agent.turn_queue";
}

pub struct AgentSessionFeature {
    db: Db,
    title: Arc<dyn TitlePort>,
}

impl AgentSessionFeature {
    pub fn new(db: Db, title: Arc<dyn TitlePort>) -> Self {
        Self { db, title }
    }
}

impl Feature for AgentSessionFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.agent-session")
            .expect("the agent session feature id is not blank")
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<TurnQueueKey>()]
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        vec![title::title_protocol(Arc::clone(&self.title))]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        let queue =
            DesktopTurnQueueStore::from_shared(self.db.clone()).map_err(|error| {
                KernelError::Mount {
                    feature: self.id(),
                    source: Box::new(error),
                }
            })?;
        cx.provide::<TurnQueueKey>(Arc::new(queue))
    }
}

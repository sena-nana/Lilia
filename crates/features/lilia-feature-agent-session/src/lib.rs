//! Agent session domain feature.
//!
//! Owns turn vocabulary ([`turn`]), the durable pending-turn queue ([`queue`]),
//! the in-memory coordinator that still holds `claim_token` ([`runtime`]),
//! task todos ([`todo`]), and the title job protocol ([`title`]). Turn
//! *execution* stays with AgentKit; this crate never holds `Jobs`.

mod execution;
mod queue;
mod runtime;
mod settings;
mod title;
mod todo;
mod turn;

use std::sync::Arc;

use lilia_kernel::{
    Feature, FeatureContext, FeatureId, JobProtocol, KernelError, ServiceKey, ServiceRef,
};
use lilia_storage::Db;

pub use execution::{
    turn_slot, ApprovalJobRequest, InteractionJobRequest, TurnJobRequest, TurnPort,
    APPROVAL_PROTOCOL, INTERACTION_PROTOCOL, TURN_PROTOCOL,
};
pub use runtime::{
    ActiveTurnSnapshot, ActiveWait, CancelSnapshot, DesktopAgentRuntime, DesktopApprovalResponse,
    DesktopInteractionResponse, DesktopInterruptResult, DesktopTaskRuntimeSnapshot, QueuedTurn,
    TurnCancellationMode, WaitingApprovalSnapshot,
};
#[cfg(debug_assertions)]
pub use runtime::{DesktopDurableTurnDebugSnapshot, DesktopQuarantinedTurnDebugSnapshot};
pub use todo::{
    guide_message, merge_todos_with_latest_projection, DesktopGuideDispatchResult,
    DesktopGuideDispatchWindow, DesktopTaskTodo, DesktopTodoCreate, DesktopTodoError,
    DesktopTodoGuideStatus, DesktopTodoPriority, DesktopTodoSource, DesktopTodoStore,
    DesktopTodoUpdate,
};
#[cfg(debug_assertions)]
pub use queue::PersistedDesktopTurnDebugState;
pub use queue::{
    DesktopTurnQueueError, DesktopTurnQueueStore, PersistedDesktopTurn, PersistedDesktopTurnState,
    QuarantinedDesktopTurn,
};
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
    turns: Arc<dyn TurnPort>,
}

impl AgentSessionFeature {
    pub fn new(db: Db, title: Arc<dyn TitlePort>, turns: Arc<dyn TurnPort>) -> Self {
        Self { db, title, turns }
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
        let mut protocols = vec![title::title_protocol(Arc::clone(&self.title))];
        protocols.extend(execution::turn_protocols(Arc::clone(&self.turns)));
        protocols
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        let queue = DesktopTurnQueueStore::from_shared(self.db.clone()).map_err(|error| {
            KernelError::Mount {
                feature: self.id(),
                source: Box::new(error),
            }
        })?;
        cx.provide::<TurnQueueKey>(Arc::new(queue))
    }
}

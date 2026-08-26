//! Agent session domain feature.
//!
//! Owns turn vocabulary ([`turn`]), the durable pending-turn queue ([`queue`]),
//! the in-memory coordinator that still holds `claim_token` ([`runtime`]),
//! task todos ([`todo`]), title HTTP/persist ([`title_apply`]), and the
//! prepared-turn sequence ([`turn_run`]). AgentKit and Jobs stay behind ports;
//! this crate never holds `Jobs`.

mod claim;
mod execution;
mod queue;
mod runtime;
mod settings;
mod title;
mod title_apply;
mod title_coordinator;
mod todo;
mod turn;
mod turn_page;
mod turn_resume;
mod turn_run;
mod turn_start;

use std::sync::Arc;

use lilia_contracts::TaskId;
use lilia_kernel::{
    Event, Feature, FeatureContext, FeatureId, JobProtocol, KernelError, ServiceKey, ServiceRef,
};
use lilia_storage::Db;

/// A task's durable todos changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TodosChanged {
    pub task_id: TaskId,
}

impl Event for TodosChanged {
    const NAME: &'static str = "lilia.todo.changed";

    fn subject(&self) -> Option<String> {
        Some(self.task_id.as_str().to_owned())
    }
}

/// A task's goal snapshot changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalChanged {
    pub task_id: TaskId,
}

impl Event for GoalChanged {
    const NAME: &'static str = "lilia.goal.changed";

    fn subject(&self) -> Option<String> {
        Some(self.task_id.as_str().to_owned())
    }
}

pub use claim::{
    accept_claimed_worker, claim_first_for_worker, claim_turn_for_worker, ClaimWorkerOutcome,
};
pub use execution::{
    turn_slot, ApprovalJobRequest, InteractionJobRequest, TurnJobRequest, TurnPort,
    APPROVAL_PROTOCOL, INTERACTION_PROTOCOL, TURN_PROTOCOL,
};
#[cfg(debug_assertions)]
pub use queue::PersistedDesktopTurnDebugState;
pub use queue::{
    DesktopTurnQueueError, DesktopTurnQueueStore, PersistedDesktopTurn, PersistedDesktopTurnState,
    QuarantinedDesktopTurn,
};
pub use runtime::{
    ActiveTurnSnapshot, ActiveWait, CancelSnapshot, DesktopAgentRuntime, DesktopApprovalResponse,
    DesktopInteractionResponse, DesktopInterruptResult, DesktopTaskRuntimeSnapshot, QueuedTurn,
    TurnCancellationMode, WaitingApprovalSnapshot,
};
#[cfg(debug_assertions)]
pub use runtime::{DesktopDurableTurnDebugSnapshot, DesktopQuarantinedTurnDebugSnapshot};
pub use settings::DesktopAutoTurnDecisionSettings;
pub use title::{title_slot, TitlePort, TitleRequest, TITLE_PROTOCOL};
pub use title_apply::{
    apply_title_proposal, build_title_prompt_for_job, persist_task_title, request_title,
    respond_title_update, respond_title_update_review, run_title_update_after_turn,
    schedule_title_update, task_title_state, StoredTitleSources, TitleError, TitleHost,
    TitleModelRequest,
};
pub use title_coordinator::{
    compact_line, normalize_title, title_event_id, title_system_instruction, truncate_chars,
    DesktopTaskTitleSource, DesktopTaskTitleState, DesktopTimelineUpperBound,
    DesktopTitleUpdateCoordinator, DesktopTitleUpdateDecision, DesktopTitleUpdateJob,
    DesktopTitleUpdateReview, DesktopTitleUpdateScheduler, TITLE_MAX_CHARS, TITLE_MIN_CHARS,
    TITLE_SOURCE_SCHEMA_VERSION, TITLE_SOURCE_SETTINGS_KEY, TITLE_UPDATE_ACTION_KIND,
};
pub use todo::{
    guide_message, merge_todos_with_latest_projection, DesktopGuideDispatchResult,
    DesktopGuideDispatchWindow, DesktopTaskTodo, DesktopTodoCreate, DesktopTodoError,
    DesktopTodoGuideStatus, DesktopTodoPriority, DesktopTodoSource, DesktopTodoStore,
    DesktopTodoUpdate,
};
pub use turn::{
    DesktopAutomaticTurnSelection, DesktopAutomationTurnCorrelation, DesktopSessionBranchAnchor,
    DesktopSessionBranchMode, DesktopTurnDispatch, DesktopTurnDispatchKind, DesktopTurnRequest,
};
pub use turn_page::{
    handle_observed_page, supported_pending_interaction_kind, TurnFinishKind, TurnPageHost,
};
pub use turn_resume::{
    run_approval_resume, run_interaction_resume, InteractionResumeSpec, TurnResumeHost,
};
pub use turn_run::{
    run_prepared_turn, AgentTurnError, AgentTurnHost, ObservedTurnOutcome, TurnSubmitSpec,
};
pub use turn_start::{accept_persisted_turn, prepare_turn_request, TurnStartHost};

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

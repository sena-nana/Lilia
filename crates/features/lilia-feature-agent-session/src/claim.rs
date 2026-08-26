//! Claim-token fence for durable queue ownership.
//!
//! The queue mints `claim_token`. After claim, AgentKit `SessionVersion` is
//! stored as `claim_epoch = sv:{n}`. The in-memory runtime must accept the
//! token before a worker may start.

use lilia_contracts::TaskId;

use crate::{
    DesktopAgentRuntime, DesktopTurnQueueError, DesktopTurnQueueStore, PersistedDesktopTurn,
};

/// What the host should do after the fence accepts or rejects a worker start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimWorkerOutcome {
    /// The caller owns the turn and must submit the worker.
    Submit { claim_token: String },
    /// Another worker already owns the turn.
    AlreadyOwned,
}

pub fn claim_turn_for_worker(
    queue: &mut DesktopTurnQueueStore,
    runtime: &DesktopAgentRuntime,
    task_id: &TaskId,
    turn_id: &str,
) -> Result<Option<ClaimWorkerOutcome>, DesktopTurnQueueError> {
    let claimed = queue.claim(turn_id)?;
    let Some(claimed) = claimed else {
        if runtime
            .active(task_id, turn_id)
            .is_some_and(|active| active.claim_token.is_some())
        {
            return Ok(Some(ClaimWorkerOutcome::AlreadyOwned));
        }
        return Ok(None);
    };
    take_worker_start(runtime, task_id, turn_id, claimed)
}

pub fn claim_first_for_worker(
    queue: &mut DesktopTurnQueueStore,
    runtime: &DesktopAgentRuntime,
    task_id: &TaskId,
) -> Result<Option<(String, ClaimWorkerOutcome)>, DesktopTurnQueueError> {
    let claimed = queue.claim_first(task_id)?;
    let Some(claimed) = claimed else {
        return Ok(None);
    };
    let turn_id = claimed.turn_id.clone();
    Ok(take_worker_start(runtime, task_id, &turn_id, claimed)?.map(|outcome| (turn_id, outcome)))
}

/// Binds a durable claim to the in-memory owner. Returns the turn id when
/// this caller must submit the worker.
pub fn accept_claimed_worker(
    runtime: &DesktopAgentRuntime,
    task_id: &TaskId,
    claimed: PersistedDesktopTurn,
) -> Result<Option<String>, DesktopTurnQueueError> {
    let turn_id = claimed.turn_id.clone();
    match take_worker_start(runtime, task_id, &turn_id, claimed)? {
        Some(ClaimWorkerOutcome::Submit { .. }) => Ok(Some(turn_id)),
        Some(ClaimWorkerOutcome::AlreadyOwned) | None => Ok(None),
    }
}

fn take_worker_start(
    runtime: &DesktopAgentRuntime,
    task_id: &TaskId,
    turn_id: &str,
    claimed: PersistedDesktopTurn,
) -> Result<Option<ClaimWorkerOutcome>, DesktopTurnQueueError> {
    let claim_token =
        claimed
            .claim_token
            .ok_or_else(|| DesktopTurnQueueError::InvalidStoredValue {
                field: "claim_token",
                message: format!("Native Agent turn `{turn_id}` claim has no ownership token"),
            })?;
    if !runtime.claim_worker_start(task_id, turn_id, claim_token.clone()) {
        return Ok(Some(ClaimWorkerOutcome::AlreadyOwned));
    }
    Ok(Some(ClaimWorkerOutcome::Submit { claim_token }))
}

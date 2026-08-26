//! In-memory turn queue coordinator for one desktop session.
//!
//! Owns `claim_token` and FIFO promotion. Turn execution stays with AgentKit;
//! this type never holds Jobs.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use lilia_contracts::TaskId;
use serde::{Deserialize, Serialize};

use crate::{DesktopTurnDispatch, DesktopTurnDispatchKind, DesktopTurnRequest};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopInterruptResult {
    pub turn_id: String,
    pub cancellation_requested: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTaskRuntimeSnapshot {
    pub phase: String,
    pub turn_id: Option<String>,
    pub session_id: Option<String>,
    pub queued_turns: usize,
    pub queued_turn_ids: Vec<String>,
}

#[cfg(debug_assertions)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopDurableTurnDebugSnapshot {
    pub turn_id: String,
    pub state: String,
    pub claim_attempts: u64,
    pub owned_by_current_epoch: bool,
}

#[cfg(debug_assertions)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopQuarantinedTurnDebugSnapshot {
    pub task_id: String,
    pub turn_id: String,
    pub original_state: String,
    pub reason_code: String,
    pub quarantined_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopApprovalResponse {
    pub turn_id: String,
    pub request_id: String,
    pub approved: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopInteractionResponse {
    pub turn_id: String,
    pub request_id: String,
    pub accepted: bool,
    pub continuation: Option<DesktopTurnDispatch>,
}

pub struct DesktopAgentRuntime {
    state: Mutex<AgentRuntimeState>,
}

#[derive(Default)]
struct AgentRuntimeState {
    tasks: BTreeMap<String, TaskRuntimeState>,
}

#[derive(Default)]
struct TaskRuntimeState {
    active: Option<ActiveTurn>,
    queue: VecDeque<QueuedTurn>,
}

#[derive(Clone)]
pub struct QueuedTurn {
    pub turn_id: String,
    pub request: DesktopTurnRequest,
}

pub struct ActiveTurn {
    turn_id: String,
    request: DesktopTurnRequest,
    claim_token: Option<String>,
    session_id: Option<String>,
    wait: Option<ActiveWait>,
    cancellation_mode: Option<TurnCancellationMode>,
    submitted: bool,
    acked: bool,
    resolution_inflight: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnCancellationMode {
    User,
    AutomationRun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveWait {
    Approval,
    Interaction,
}

impl Default for DesktopAgentRuntime {
    fn default() -> Self {
        Self {
            state: Mutex::new(AgentRuntimeState::default()),
        }
    }
}

impl DesktopAgentRuntime {
    pub fn enqueue_idempotent(
        &self,
        request: DesktopTurnRequest,
        turn_id: String,
    ) -> (DesktopTurnDispatch, bool, bool) {
        self.enqueue_with_turn_id(request, turn_id)
    }

    pub fn enqueue_with_turn_id(
        &self,
        request: DesktopTurnRequest,
        turn_id: String,
    ) -> (DesktopTurnDispatch, bool, bool) {
        let task_key = request.task_id.as_str().to_owned();
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let task = state.tasks.entry(task_key).or_default();
        if task.active.as_ref().map(|turn| turn.turn_id.as_str()) == Some(turn_id.as_str()) {
            return (
                DesktopTurnDispatch {
                    turn_id,
                    kind: DesktopTurnDispatchKind::Started,
                },
                false,
                false,
            );
        }
        if let Some(position) = task.queue.iter().position(|turn| turn.turn_id == turn_id) {
            return (
                DesktopTurnDispatch {
                    turn_id,
                    kind: DesktopTurnDispatchKind::Queued {
                        position: position + 1,
                    },
                },
                false,
                false,
            );
        }
        if task.active.is_some() || !task.queue.is_empty() {
            task.queue.push_back(QueuedTurn {
                turn_id: turn_id.clone(),
                request,
            });
            let position = task.queue.len();
            return (
                DesktopTurnDispatch {
                    turn_id,
                    kind: DesktopTurnDispatchKind::Queued { position },
                },
                false,
                true,
            );
        }
        task.active = Some(ActiveTurn {
            turn_id: turn_id.clone(),
            request,
            claim_token: None,
            session_id: None,
            wait: None,
            cancellation_mode: None,
            submitted: false,
            acked: false,
            resolution_inflight: false,
        });
        (
            DesktopTurnDispatch {
                turn_id,
                kind: DesktopTurnDispatchKind::Started,
            },
            true,
            true,
        )
    }

    pub fn active(&self, task_id: &TaskId, turn_id: &str) -> Option<ActiveTurnSnapshot> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let active = state.tasks.get(task_id.as_str())?.active.as_ref()?;
        (active.turn_id == turn_id).then(|| ActiveTurnSnapshot {
            request: active.request.clone(),
            cancellation_mode: active.cancellation_mode,
            claim_token: active.claim_token.clone(),
        })
    }

    pub fn replace_active_request(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        request: DesktopTurnRequest,
    ) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| active.turn_id == turn_id)
        else {
            return false;
        };
        active.request = request;
        true
    }

    pub fn request(&self, task_id: &TaskId, turn_id: &str) -> Option<DesktopTurnRequest> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let task = state.tasks.get(task_id.as_str())?;
        task.active
            .as_ref()
            .filter(|active| active.turn_id == turn_id)
            .map(|active| active.request.clone())
            .or_else(|| {
                task.queue
                    .iter()
                    .find(|queued| queued.turn_id == turn_id)
                    .map(|queued| queued.request.clone())
            })
    }

    pub fn promote_queued_if_idle(&self, task_id: &TaskId, turn_id: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(task) = state.tasks.get_mut(task_id.as_str()) else {
            return false;
        };
        if task.active.is_some()
            || task.queue.front().map(|queued| queued.turn_id.as_str()) != Some(turn_id)
        {
            return false;
        }
        let next = task.queue.pop_front().expect("front was checked");
        task.active = Some(ActiveTurn {
            turn_id: next.turn_id,
            request: next.request,
            claim_token: None,
            session_id: None,
            wait: None,
            cancellation_mode: None,
            submitted: false,
            acked: false,
            resolution_inflight: false,
        });
        true
    }

    pub fn attach_session(&self, task_id: &TaskId, turn_id: &str, session_id: String) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| active.turn_id == turn_id)
        else {
            return false;
        };
        active.session_id = Some(session_id);
        active.wait = None;
        active.cancellation_mode.is_some()
    }

    pub fn wait_for_approval(&self, task_id: &TaskId, turn_id: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| active.turn_id == turn_id)
        else {
            return false;
        };
        active.wait = Some(ActiveWait::Approval);
        true
    }

    pub fn begin_approval(&self, task_id: &TaskId, turn_id: &str) -> bool {
        self.begin_resolution(task_id, turn_id, ActiveWait::Approval)
    }

    pub fn wait_for_interaction(&self, task_id: &TaskId, turn_id: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| active.turn_id == turn_id)
        else {
            return false;
        };
        active.wait = Some(ActiveWait::Interaction);
        true
    }

    pub fn begin_interaction(&self, task_id: &TaskId, turn_id: &str) -> bool {
        self.begin_resolution(task_id, turn_id, ActiveWait::Interaction)
    }

    pub fn begin_resolution(&self, task_id: &TaskId, turn_id: &str, wait: ActiveWait) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| {
                active.turn_id == turn_id
                    && active.wait == Some(wait)
                    && !active.resolution_inflight
                    && !active.acked
            })
        else {
            return false;
        };
        active.resolution_inflight = true;
        true
    }

    pub fn waiting_approval(&self, task_id: &TaskId) -> Option<WaitingApprovalSnapshot> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let active = state.tasks.get(task_id.as_str())?.active.as_ref()?;
        (active.wait == Some(ActiveWait::Approval) && !active.resolution_inflight && !active.acked)
            .then(|| WaitingApprovalSnapshot {
                turn_id: active.turn_id.clone(),
                session_id: active.session_id.clone(),
            })
    }

    pub fn waiting_interaction(&self, task_id: &TaskId) -> Option<WaitingApprovalSnapshot> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let active = state.tasks.get(task_id.as_str())?.active.as_ref()?;
        (active.wait == Some(ActiveWait::Interaction)
            && !active.resolution_inflight
            && !active.acked)
            .then(|| WaitingApprovalSnapshot {
                turn_id: active.turn_id.clone(),
                session_id: active.session_id.clone(),
            })
    }

    pub fn restore_projected_wait(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        session_id: &str,
        wait: ActiveWait,
    ) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let task = state.tasks.entry(task_id.as_str().to_owned()).or_default();
        if task.active.is_some() {
            return false;
        }
        task.active = Some(ActiveTurn {
            turn_id: turn_id.to_owned(),
            request: DesktopTurnRequest::new(task_id.clone(), ""),
            claim_token: None,
            session_id: Some(session_id.to_owned()),
            wait: Some(wait),
            cancellation_mode: None,
            submitted: true,
            acked: false,
            resolution_inflight: false,
        });
        true
    }

    pub fn restore_waiting_approval(&self, task_id: &TaskId, turn_id: &str) {
        self.restore_wait(task_id, turn_id, ActiveWait::Approval);
    }

    pub fn restore_waiting_interaction(&self, task_id: &TaskId, turn_id: &str) {
        self.restore_wait(task_id, turn_id, ActiveWait::Interaction);
    }

    pub fn restore_wait(&self, task_id: &TaskId, turn_id: &str, wait: ActiveWait) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| active.turn_id == turn_id)
        {
            active.wait = Some(wait);
            active.resolution_inflight = false;
        }
    }

    pub fn request_cancel(&self, task_id: &TaskId) -> Option<CancelSnapshot> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let active = state.tasks.get_mut(task_id.as_str())?.active.as_mut()?;
        active.cancellation_mode = Some(TurnCancellationMode::User);
        Some(CancelSnapshot {
            turn_id: active.turn_id.clone(),
            session_id: active.session_id.clone(),
            wait: active.wait,
        })
    }

    pub fn request_automation_cancel(
        &self,
        task_id: &TaskId,
        turn_id: &str,
    ) -> Option<CancelSnapshot> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let active = state
            .tasks
            .get_mut(task_id.as_str())?
            .active
            .as_mut()
            .filter(|active| active.turn_id == turn_id)?;
        active.cancellation_mode = Some(TurnCancellationMode::AutomationRun);
        Some(CancelSnapshot {
            turn_id: active.turn_id.clone(),
            session_id: active.session_id.clone(),
            wait: active.wait,
        })
    }

    pub fn revert_automation_cancel(&self, task_id: &TaskId, turn_id: &str) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| active.turn_id == turn_id)
        {
            active.cancellation_mode = None;
        }
    }

    pub fn snapshot(&self, task_id: &TaskId) -> DesktopTaskRuntimeSnapshot {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(task) = state.tasks.get(task_id.as_str()) else {
            return DesktopTaskRuntimeSnapshot {
                phase: "idle".to_owned(),
                turn_id: None,
                session_id: None,
                queued_turns: 0,
                queued_turn_ids: Vec::new(),
            };
        };
        let (phase, turn_id, session_id) = match task.active.as_ref() {
            Some(active) => (
                active.projected_phase().to_owned(),
                Some(active.turn_id.clone()),
                active.session_id.clone(),
            ),
            None => ("idle".to_owned(), None, None),
        };
        DesktopTaskRuntimeSnapshot {
            phase,
            turn_id,
            session_id,
            queued_turns: task.queue.len(),
            queued_turn_ids: task.queue.iter().map(|turn| turn.turn_id.clone()).collect(),
        }
    }

    pub fn cancel_requested(&self, task_id: &TaskId, turn_id: &str) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .tasks
            .get(task_id.as_str())
            .and_then(|task| task.active.as_ref())
            .is_some_and(|active| active.turn_id == turn_id && active.cancellation_mode.is_some())
    }

    pub fn claim_worker_start(&self, task_id: &TaskId, turn_id: &str, claim_token: String) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
        else {
            return false;
        };
        if active.turn_id != turn_id || active.submitted {
            return false;
        }
        active.claim_token = Some(claim_token);
        active.submitted = true;
        true
    }

    pub fn hydrate_restored_active(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        request: DesktopTurnRequest,
        claim_token: String,
    ) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| active.turn_id == turn_id)
        else {
            return false;
        };
        active.request = request;
        active.claim_token = Some(claim_token);
        true
    }

    pub fn begin_finish(&self, task_id: &TaskId, turn_id: &str) -> Option<ActiveTurnSnapshot> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let active = state.tasks.get_mut(task_id.as_str())?.active.as_mut()?;
        if active.turn_id != turn_id || active.acked {
            return None;
        }
        active.acked = true;
        Some(ActiveTurnSnapshot {
            request: active.request.clone(),
            cancellation_mode: active.cancellation_mode,
            claim_token: active.claim_token.clone(),
        })
    }

    pub fn abort_prepared(&self, task_id: &TaskId, turn_id: &str) -> Option<QueuedTurn> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let task = state.tasks.get_mut(task_id.as_str())?;
        if task
            .active
            .as_ref()
            .is_some_and(|active| active.turn_id == turn_id && !active.submitted)
        {
            task.active = None;
            let next = task.queue.pop_front();
            if let Some(next) = &next {
                task.active = Some(prepared_active_turn(next.clone()));
            }
            return next;
        }
        task.queue.retain(|queued| queued.turn_id != turn_id);
        None
    }

    pub fn is_prepared_active(&self, task_id: &TaskId, turn_id: &str) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .tasks
            .get(task_id.as_str())
            .and_then(|task| task.active.as_ref())
            .is_some_and(|active| active.turn_id == turn_id && !active.submitted)
    }

    pub fn finish_and_activate_next(&self, task_id: &TaskId, turn_id: &str) -> Option<QueuedTurn> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let task = state.tasks.get_mut(task_id.as_str())?;
        if task.active.as_ref().map(|active| active.turn_id.as_str()) != Some(turn_id) {
            return None;
        }
        task.active = None;
        let next = task.queue.pop_front();
        if let Some(next) = &next {
            task.active = Some(prepared_active_turn(next.clone()));
        }
        next
    }

    pub fn finish_and_clear_queue(
        &self,
        task_id: &TaskId,
        turn_id: &str,
    ) -> Option<Vec<QueuedTurn>> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let task = state.tasks.get_mut(task_id.as_str())?;
        if task.active.as_ref().map(|active| active.turn_id.as_str()) != Some(turn_id) {
            return None;
        }
        task.active = None;
        Some(task.queue.drain(..).collect())
    }

    pub fn finish_without_next(&self, task_id: &TaskId, turn_id: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(task) = state.tasks.get_mut(task_id.as_str()) else {
            return false;
        };
        if task.active.as_ref().map(|active| active.turn_id.as_str()) != Some(turn_id) {
            return false;
        }
        task.active = None;
        true
    }

    pub fn queued_front(&self, task_id: &TaskId) -> Option<QueuedTurn> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .tasks
            .get(task_id.as_str())
            .and_then(|task| task.queue.front())
            .cloned()
    }

    pub fn queued(&self, task_id: &TaskId) -> Vec<QueuedTurn> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .tasks
            .get(task_id.as_str())
            .map(|task| task.queue.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl ActiveTurn {
    pub fn projected_phase(&self) -> &'static str {
        match self.wait {
            Some(ActiveWait::Approval) => "waiting_approval",
            Some(ActiveWait::Interaction) => "waiting_interaction",
            None if !self.submitted => "starting",
            None => "running",
        }
    }
}

fn prepared_active_turn(next: QueuedTurn) -> ActiveTurn {
    ActiveTurn {
        turn_id: next.turn_id,
        request: next.request,
        claim_token: None,
        session_id: None,
        wait: None,
        cancellation_mode: None,
        submitted: false,
        acked: false,
        resolution_inflight: false,
    }
}

pub struct ActiveTurnSnapshot {
    pub request: DesktopTurnRequest,
    pub cancellation_mode: Option<TurnCancellationMode>,
    pub claim_token: Option<String>,
}

pub struct CancelSnapshot {
    pub turn_id: String,
    pub session_id: Option<String>,
    pub wait: Option<ActiveWait>,
}

pub struct WaitingApprovalSnapshot {
    pub turn_id: String,
    pub session_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DesktopAutomationTurnCorrelation;
    use lilia_contracts::TaskId;

    #[test]
    fn idempotent_enqueue_reuses_an_active_turn_without_redispatch() {
        let runtime = DesktopAgentRuntime::default();
        let request = DesktopTurnRequest::new(
            TaskId::new("automation-agent-task").unwrap(),
            "Continue the native migration",
        );
        let turn_id = "automation-turn:run-1:agent-node".to_owned();

        let (first, should_start, inserted) =
            runtime.enqueue_idempotent(request.clone(), turn_id.clone());
        let (replay, replay_should_start, replay_inserted) =
            runtime.enqueue_idempotent(request, turn_id);

        assert!(should_start);
        assert!(inserted);
        assert_eq!(first, replay);
        assert!(!replay_should_start);
        assert!(!replay_inserted);
    }

    #[test]
    fn finish_claim_is_single_use_for_each_active_turn() {
        let runtime = DesktopAgentRuntime::default();
        let task_id = TaskId::new("single-finish-task").unwrap();
        let turn_id = "single-finish-turn".to_owned();
        let (_, should_start, inserted) = runtime.enqueue_idempotent(
            DesktopTurnRequest::new(task_id.clone(), "finish once"),
            turn_id.clone(),
        );
        assert!(should_start);
        assert!(inserted);
        assert!(runtime.claim_worker_start(&task_id, &turn_id, "single-finish-claim".to_owned()));

        let first = runtime.begin_finish(&task_id, &turn_id).unwrap();
        assert_eq!(first.claim_token.as_deref(), Some("single-finish-claim"));
        assert!(runtime.begin_finish(&task_id, &turn_id).is_none());
        assert_eq!(runtime.snapshot(&task_id).phase, "running");
        assert!(runtime.active(&task_id, &turn_id).is_some());
    }

    #[test]
    fn projected_wait_restores_a_worker_owned_interaction_runtime() {
        let runtime = DesktopAgentRuntime::default();
        let task_id = TaskId::new("restored-interaction-task").unwrap();

        assert!(runtime.restore_projected_wait(
            &task_id,
            "turn-restored",
            "session-restored",
            ActiveWait::Interaction,
        ));

        let snapshot = runtime.snapshot(&task_id);
        assert_eq!(snapshot.phase, "waiting_interaction");
        assert_eq!(snapshot.turn_id.as_deref(), Some("turn-restored"));
        assert_eq!(snapshot.session_id.as_deref(), Some("session-restored"));
        assert!(!runtime.claim_worker_start(
            &task_id,
            "turn-restored",
            "restored-claim".to_owned()
        ));
    }

    #[test]
    fn projected_wait_never_replaces_a_live_turn() {
        let runtime = DesktopAgentRuntime::default();
        let task_id = TaskId::new("live-turn-task").unwrap();
        let (dispatch, _, _) = runtime.enqueue_idempotent(
            DesktopTurnRequest::new(task_id.clone(), "continue"),
            "live-turn".to_owned(),
        );

        assert!(!runtime.restore_projected_wait(
            &task_id,
            "stale-turn",
            "stale-session",
            ActiveWait::Approval,
        ));

        let snapshot = runtime.snapshot(&task_id);
        assert_eq!(snapshot.phase, "starting");
        assert_eq!(snapshot.turn_id.as_deref(), Some(dispatch.turn_id.as_str()));
        assert_eq!(snapshot.session_id, None);
    }

    #[test]
    fn restored_wait_keeps_persisted_turns_in_original_fifo_order() {
        let runtime = DesktopAgentRuntime::default();
        let task_id = TaskId::new("restored-queue-task").unwrap();
        assert!(runtime.restore_projected_wait(
            &task_id,
            "turn-active",
            "session-active",
            ActiveWait::Approval,
        ));

        let (first, first_should_start, first_inserted) = runtime.enqueue_idempotent(
            DesktopTurnRequest::new(task_id.clone(), "first queued"),
            "turn-queued-1".to_owned(),
        );
        let (second, second_should_start, second_inserted) = runtime.enqueue_idempotent(
            DesktopTurnRequest::new(task_id.clone(), "second queued"),
            "turn-queued-2".to_owned(),
        );

        assert_eq!(first.kind, DesktopTurnDispatchKind::Queued { position: 1 });
        assert_eq!(second.kind, DesktopTurnDispatchKind::Queued { position: 2 });
        assert!(!first_should_start);
        assert!(!second_should_start);
        assert!(first_inserted);
        assert!(second_inserted);
        assert_eq!(
            runtime.snapshot(&task_id).queued_turn_ids,
            vec!["turn-queued-1".to_owned(), "turn-queued-2".to_owned()]
        );

        let promoted = runtime
            .finish_and_activate_next(&task_id, "turn-active")
            .unwrap();
        assert_eq!(promoted.turn_id, "turn-queued-1");
        let snapshot = runtime.snapshot(&task_id);
        assert_eq!(snapshot.turn_id.as_deref(), Some("turn-queued-1"));
        assert_eq!(snapshot.queued_turn_ids, vec!["turn-queued-2".to_owned()]);
    }

    #[test]
    fn automation_cancellation_targets_one_turn_and_preserves_fifo() {
        let runtime = DesktopAgentRuntime::default();
        let task_id = TaskId::new("automation-cancel-task").unwrap();
        let mut automation = DesktopTurnRequest::new(task_id.clone(), "automation");
        automation.automation = Some(DesktopAutomationTurnCorrelation {
            run_id: "run-1".to_owned(),
            node_id: "agent-1".to_owned(),
        });
        runtime.enqueue_idempotent(automation, "turn-automation".to_owned());
        runtime.enqueue_idempotent(
            DesktopTurnRequest::new(task_id.clone(), "user follow-up"),
            "turn-user".to_owned(),
        );

        assert!(runtime
            .request_automation_cancel(&task_id, "turn-other")
            .is_none());
        let cancel = runtime
            .request_automation_cancel(&task_id, "turn-automation")
            .unwrap();
        assert_eq!(cancel.turn_id, "turn-automation");
        let finishing = runtime.begin_finish(&task_id, "turn-automation").unwrap();
        assert_eq!(
            finishing.cancellation_mode,
            Some(TurnCancellationMode::AutomationRun)
        );
        let promoted = runtime
            .finish_and_activate_next(&task_id, "turn-automation")
            .unwrap();
        assert_eq!(promoted.turn_id, "turn-user");
        let snapshot = runtime.snapshot(&task_id);
        assert_eq!(snapshot.turn_id.as_deref(), Some("turn-user"));
        assert!(snapshot.queued_turn_ids.is_empty());
    }

    #[test]
    fn explicit_cancellation_discards_queued_turns_without_promoting_them() {
        let runtime = DesktopAgentRuntime::default();
        let task_id = TaskId::new("cancel-queued-task").unwrap();
        runtime.enqueue_idempotent(
            DesktopTurnRequest::new(task_id.clone(), "active"),
            "turn-active".to_owned(),
        );
        runtime.enqueue_idempotent(
            DesktopTurnRequest::new(task_id.clone(), "queued one"),
            "turn-queued-1".to_owned(),
        );
        runtime.enqueue_idempotent(
            DesktopTurnRequest::new(task_id.clone(), "queued two"),
            "turn-queued-2".to_owned(),
        );

        assert!(runtime.request_cancel(&task_id).is_some());
        let discarded = runtime
            .finish_and_clear_queue(&task_id, "turn-active")
            .unwrap();

        assert_eq!(
            discarded
                .into_iter()
                .map(|turn| turn.turn_id)
                .collect::<Vec<_>>(),
            vec!["turn-queued-1".to_owned(), "turn-queued-2".to_owned()]
        );
        let snapshot = runtime.snapshot(&task_id);
        assert_eq!(snapshot.phase, "idle");
        assert!(snapshot.turn_id.is_none());
        assert!(snapshot.queued_turn_ids.is_empty());
    }
}

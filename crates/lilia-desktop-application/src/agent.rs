use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use lilia_contracts::{
    AgentSessionBinding, AgentSessionRef, BindingId, ChatAttachment, ChatConversationReference,
    LiliaAgentWorkflow, PendingProjection, PendingProjectionStatus, ProductApprovalDecision,
    ProductEntity, ProductEntityKind, ProductRevision, ProductTask, TaskId,
};
#[cfg(debug_assertions)]
use mutsuki_agent_contracts::AgentToolCall;
use mutsuki_agent_contracts::{
    AgentEvent, AgentMessage, AgentSession, AgentWireError, AgentWireRequestEnvelope,
    AgentWireResponseEnvelope, InteractionResolution,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::hooks::DesktopHookEvent;
use crate::turn_queue::PersistedDesktopTurnState;
use crate::{
    ArchitectureBackend, ArchitecturePermission, AutomationCompleteAgentInput, DesktopApplication,
    DesktopApplicationError, DesktopApprovalState, DesktopEventKind, DesktopGuideDispatchWindow,
    DesktopInteractionState, DesktopMcpElicitation, DesktopMcpElicitationAction,
    DesktopTodoGuideStatus, DesktopTurnState, ProjectArchitectureApplyInput,
    ProjectArchitectureChange, ProjectArchitectureChangeEvent, ProjectArchitectureGraph,
    ProjectArchitectureRejectInput,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopExecutionPermission {
    Full,
    #[default]
    Ask,
    Readonly,
}

impl DesktopExecutionPermission {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Ask => "ask",
            Self::Readonly => "readonly",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "full" => Some(Self::Full),
            "ask" => Some(Self::Ask),
            "readonly" => Some(Self::Readonly),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAutomationTurnCorrelation {
    pub run_id: String,
    pub node_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSessionBranchMode {
    Continue,
    Fork,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSessionBranchAnchor {
    pub source_turn_id: String,
    pub mode: DesktopSessionBranchMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTurnRequest {
    pub task_id: TaskId,
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<ChatAttachment>,
    #[serde(default)]
    pub conversation_references: Vec<ChatConversationReference>,
    pub workspace_path: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub permission: DesktopExecutionPermission,
    pub plan_mode: bool,
    pub goal_mode: bool,
    #[serde(default)]
    pub allow_auto_turn_decision: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_turn_settings: Option<crate::DesktopAutoTurnDecisionSettings>,
    #[serde(default)]
    pub auto_turn_decision_applied: bool,
    #[serde(default)]
    pub session_fork: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_branch: Option<DesktopSessionBranchAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automatic_selection: Option<DesktopAutomaticTurnSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automation: Option<DesktopAutomationTurnCorrelation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<LiliaAgentWorkflow>,
}

impl DesktopTurnRequest {
    pub fn new(task_id: TaskId, content: impl Into<String>) -> Self {
        Self {
            task_id,
            content: content.into(),
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            workspace_path: None,
            model: None,
            reasoning_effort: None,
            permission: DesktopExecutionPermission::Ask,
            plan_mode: false,
            goal_mode: false,
            allow_auto_turn_decision: false,
            auto_turn_settings: None,
            auto_turn_decision_applied: false,
            session_fork: false,
            session_branch: None,
            automatic_selection: None,
            automation: None,
            guide_id: None,
            workflow: None,
        }
    }

    pub fn with_attachments(mut self, attachments: Vec<ChatAttachment>) -> Self {
        self.attachments = attachments;
        self
    }

    pub fn with_conversation_references(
        mut self,
        references: Vec<ChatConversationReference>,
    ) -> Self {
        self.conversation_references = references;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAutomaticTurnSelection {
    pub source: String,
    pub tier: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub plan_mode: bool,
    pub goal_mode: bool,
    pub session_fork: bool,
    pub summary: Option<String>,
    pub signals: Vec<String>,
    pub decision_provider_id: String,
    pub decision_model: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopTurnDispatchKind {
    Started,
    Queued { position: usize },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTurnDispatch {
    pub turn_id: String,
    pub kind: DesktopTurnDispatchKind,
}

struct ObservedTurnPage {
    session_id: String,
    waiting_approval: bool,
    waiting_interaction: bool,
    completed: bool,
    cancelled_by_user: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DesktopIdempotentTurnStart {
    Dispatch {
        dispatch: DesktopTurnDispatch,
        worker_start_required: bool,
        turn_inserted: bool,
    },
    Completed {
        turn_id: String,
    },
    TerminalConflict {
        turn_id: String,
        status: String,
    },
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopArchitectureInteractionDecision {
    Allow,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopArchitectureInteractionResponse {
    pub decision: DesktopArchitectureInteractionDecision,
    pub graph: Option<ProjectArchitectureGraph>,
    pub event: ProjectArchitectureChangeEvent,
    pub message: String,
    pub interaction: DesktopInteractionResponse,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopArchitectureInteractionPayload {
    project_id: String,
    task_id: String,
    turn_id: Option<String>,
    backend: ArchitectureBackend,
    permission: ArchitecturePermission,
    reason: String,
    changes: Vec<ProjectArchitectureChange>,
    expected_version: Option<i64>,
}

pub(crate) struct DesktopAgentRuntime {
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
struct QueuedTurn {
    turn_id: String,
    request: DesktopTurnRequest,
}

struct ActiveTurn {
    turn_id: String,
    request: DesktopTurnRequest,
    claim_token: Option<String>,
    session_id: Option<String>,
    phase: ActiveTurnPhase,
    cancellation_mode: Option<TurnCancellationMode>,
    worker_started: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TurnCancellationMode {
    User,
    AutomationRun,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActiveTurnPhase {
    Starting,
    Running,
    WaitingApproval,
    ResolvingApproval,
    WaitingInteraction,
    ResolvingInteraction,
    Finishing,
}

impl Default for DesktopAgentRuntime {
    fn default() -> Self {
        Self {
            state: Mutex::new(AgentRuntimeState::default()),
        }
    }
}

impl DesktopAgentRuntime {
    fn enqueue_idempotent(
        &self,
        request: DesktopTurnRequest,
        turn_id: String,
    ) -> (DesktopTurnDispatch, bool, bool) {
        self.enqueue_with_turn_id(request, turn_id)
    }

    fn enqueue_with_turn_id(
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
            phase: ActiveTurnPhase::Starting,
            cancellation_mode: None,
            worker_started: false,
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

    fn active(&self, task_id: &TaskId, turn_id: &str) -> Option<ActiveTurnSnapshot> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let active = state.tasks.get(task_id.as_str())?.active.as_ref()?;
        (active.turn_id == turn_id).then(|| ActiveTurnSnapshot {
            request: active.request.clone(),
            cancellation_mode: active.cancellation_mode,
            claim_token: active.claim_token.clone(),
        })
    }

    fn replace_active_request(
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

    fn request(&self, task_id: &TaskId, turn_id: &str) -> Option<DesktopTurnRequest> {
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

    fn promote_queued_if_idle(&self, task_id: &TaskId, turn_id: &str) -> bool {
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
            phase: ActiveTurnPhase::Starting,
            cancellation_mode: None,
            worker_started: false,
        });
        true
    }

    fn attach_session(&self, task_id: &TaskId, turn_id: &str, session_id: String) -> bool {
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
        active.phase = ActiveTurnPhase::Running;
        active.cancellation_mode.is_some()
    }

    fn wait_for_approval(&self, task_id: &TaskId, turn_id: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| active.turn_id == turn_id)
        else {
            return false;
        };
        active.phase = ActiveTurnPhase::WaitingApproval;
        true
    }

    fn begin_approval(&self, task_id: &TaskId, turn_id: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| {
                active.turn_id == turn_id && active.phase == ActiveTurnPhase::WaitingApproval
            })
        else {
            return false;
        };
        active.phase = ActiveTurnPhase::ResolvingApproval;
        true
    }

    fn wait_for_interaction(&self, task_id: &TaskId, turn_id: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| active.turn_id == turn_id)
        else {
            return false;
        };
        active.phase = ActiveTurnPhase::WaitingInteraction;
        true
    }

    fn begin_interaction(&self, task_id: &TaskId, turn_id: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| {
                active.turn_id == turn_id && active.phase == ActiveTurnPhase::WaitingInteraction
            })
        else {
            return false;
        };
        active.phase = ActiveTurnPhase::ResolvingInteraction;
        true
    }

    fn waiting_approval(&self, task_id: &TaskId) -> Option<WaitingApprovalSnapshot> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let active = state.tasks.get(task_id.as_str())?.active.as_ref()?;
        (active.phase == ActiveTurnPhase::WaitingApproval).then(|| WaitingApprovalSnapshot {
            turn_id: active.turn_id.clone(),
            session_id: active.session_id.clone(),
        })
    }

    fn waiting_interaction(&self, task_id: &TaskId) -> Option<WaitingApprovalSnapshot> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let active = state.tasks.get(task_id.as_str())?.active.as_ref()?;
        (active.phase == ActiveTurnPhase::WaitingInteraction).then(|| WaitingApprovalSnapshot {
            turn_id: active.turn_id.clone(),
            session_id: active.session_id.clone(),
        })
    }

    fn restore_projected_wait(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        session_id: &str,
        phase: ActiveTurnPhase,
    ) -> bool {
        debug_assert!(matches!(
            phase,
            ActiveTurnPhase::WaitingApproval | ActiveTurnPhase::WaitingInteraction
        ));
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
            phase,
            cancellation_mode: None,
            worker_started: true,
        });
        true
    }

    fn restore_waiting_approval(&self, task_id: &TaskId, turn_id: &str) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| active.turn_id == turn_id)
        {
            active.phase = ActiveTurnPhase::WaitingApproval;
        }
    }

    fn restore_waiting_interaction(&self, task_id: &TaskId, turn_id: &str) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
            .filter(|active| active.turn_id == turn_id)
        {
            active.phase = ActiveTurnPhase::WaitingInteraction;
        }
    }

    fn request_cancel(&self, task_id: &TaskId) -> Option<CancelSnapshot> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let active = state.tasks.get_mut(task_id.as_str())?.active.as_mut()?;
        active.cancellation_mode = Some(TurnCancellationMode::User);
        Some(CancelSnapshot {
            turn_id: active.turn_id.clone(),
            session_id: active.session_id.clone(),
            phase: active.phase,
        })
    }

    fn request_automation_cancel(&self, task_id: &TaskId, turn_id: &str) -> Option<CancelSnapshot> {
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
            phase: active.phase,
        })
    }

    fn revert_automation_cancel(&self, task_id: &TaskId, turn_id: &str) {
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

    fn snapshot(&self, task_id: &TaskId) -> DesktopTaskRuntimeSnapshot {
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
                active.phase.as_str().to_owned(),
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

    fn cancel_requested(&self, task_id: &TaskId, turn_id: &str) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .tasks
            .get(task_id.as_str())
            .and_then(|task| task.active.as_ref())
            .is_some_and(|active| active.turn_id == turn_id && active.cancellation_mode.is_some())
    }

    fn claim_worker_start(&self, task_id: &TaskId, turn_id: &str, claim_token: String) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state
            .tasks
            .get_mut(task_id.as_str())
            .and_then(|task| task.active.as_mut())
        else {
            return false;
        };
        if active.turn_id != turn_id || active.worker_started {
            return false;
        }
        active.claim_token = Some(claim_token);
        active.worker_started = true;
        true
    }

    fn hydrate_restored_active(
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

    fn begin_finish(&self, task_id: &TaskId, turn_id: &str) -> Option<ActiveTurnSnapshot> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let active = state.tasks.get_mut(task_id.as_str())?.active.as_mut()?;
        if active.turn_id != turn_id || active.phase == ActiveTurnPhase::Finishing {
            return None;
        }
        active.phase = ActiveTurnPhase::Finishing;
        Some(ActiveTurnSnapshot {
            request: active.request.clone(),
            cancellation_mode: active.cancellation_mode,
            claim_token: active.claim_token.clone(),
        })
    }

    fn abort_prepared(&self, task_id: &TaskId, turn_id: &str) -> Option<QueuedTurn> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let task = state.tasks.get_mut(task_id.as_str())?;
        if task
            .active
            .as_ref()
            .is_some_and(|active| active.turn_id == turn_id && !active.worker_started)
        {
            task.active = None;
            let next = task.queue.pop_front();
            if let Some(next) = &next {
                task.active = Some(ActiveTurn {
                    turn_id: next.turn_id.clone(),
                    request: next.request.clone(),
                    claim_token: None,
                    session_id: None,
                    phase: ActiveTurnPhase::Starting,
                    cancellation_mode: None,
                    worker_started: false,
                });
            }
            return next;
        }
        task.queue.retain(|queued| queued.turn_id != turn_id);
        None
    }

    fn is_prepared_active(&self, task_id: &TaskId, turn_id: &str) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .tasks
            .get(task_id.as_str())
            .and_then(|task| task.active.as_ref())
            .is_some_and(|active| active.turn_id == turn_id && !active.worker_started)
    }

    fn finish_and_activate_next(&self, task_id: &TaskId, turn_id: &str) -> Option<QueuedTurn> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let task = state.tasks.get_mut(task_id.as_str())?;
        if task.active.as_ref().map(|active| active.turn_id.as_str()) != Some(turn_id) {
            return None;
        }
        task.active = None;
        let next = task.queue.pop_front();
        if let Some(next) = &next {
            task.active = Some(ActiveTurn {
                turn_id: next.turn_id.clone(),
                request: next.request.clone(),
                claim_token: None,
                session_id: None,
                phase: ActiveTurnPhase::Starting,
                cancellation_mode: None,
                worker_started: false,
            });
        }
        next
    }

    fn finish_and_clear_queue(&self, task_id: &TaskId, turn_id: &str) -> Option<Vec<QueuedTurn>> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let task = state.tasks.get_mut(task_id.as_str())?;
        if task.active.as_ref().map(|active| active.turn_id.as_str()) != Some(turn_id) {
            return None;
        }
        task.active = None;
        Some(task.queue.drain(..).collect())
    }

    fn finish_without_next(&self, task_id: &TaskId, turn_id: &str) -> bool {
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

    fn queued_front(&self, task_id: &TaskId) -> Option<QueuedTurn> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .tasks
            .get(task_id.as_str())
            .and_then(|task| task.queue.front())
            .cloned()
    }

    fn queued(&self, task_id: &TaskId) -> Vec<QueuedTurn> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .tasks
            .get(task_id.as_str())
            .map(|task| task.queue.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl ActiveTurnPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::ResolvingApproval => "resolving_approval",
            Self::WaitingInteraction => "waiting_interaction",
            Self::ResolvingInteraction => "resolving_interaction",
            Self::Finishing => "finishing",
        }
    }
}

struct ActiveTurnSnapshot {
    request: DesktopTurnRequest,
    cancellation_mode: Option<TurnCancellationMode>,
    claim_token: Option<String>,
}

struct CancelSnapshot {
    turn_id: String,
    session_id: Option<String>,
    phase: ActiveTurnPhase,
}

struct WaitingApprovalSnapshot {
    turn_id: String,
    session_id: Option<String>,
}

fn supported_pending_interaction_kind(kind: &str) -> bool {
    matches!(
        kind,
        "ask_user" | "plan_approval" | "tool_consent" | "mcp_elicitation" | "architecture_change"
    )
}

fn normalized_pending_interaction_response(
    pending: &PendingProjection,
    accepted: bool,
    response: Value,
) -> Result<(bool, Value), DesktopApplicationError> {
    if pending.kind == "tool_consent" {
        let decision = response.get("decision").and_then(Value::as_str);
        let expected_accepted = match decision {
            Some("allow") => true,
            Some("deny") => false,
            _ => {
                return Err(DesktopApplicationError::InvalidPendingInteraction {
                    request_id: pending.request_id.clone(),
                    message: "tool consent response has an invalid decision".to_owned(),
                })
            }
        };
        if accepted != expected_accepted {
            return Err(DesktopApplicationError::InvalidPendingInteraction {
                request_id: pending.request_id.clone(),
                message: "tool consent decision does not match its accepted state".to_owned(),
            });
        }
        if response
            .get("updatedInput")
            .is_some_and(|input| !input.is_object())
        {
            return Err(DesktopApplicationError::InvalidPendingInteraction {
                request_id: pending.request_id.clone(),
                message: "tool consent updated input must be an object".to_owned(),
            });
        }
        return Ok((accepted, response));
    }
    if pending.kind != "mcp_elicitation" {
        return Ok((accepted, response));
    }
    let action = match response.get("action").and_then(Value::as_str) {
        Some("accept") => DesktopMcpElicitationAction::Accept,
        Some("decline") => DesktopMcpElicitationAction::Decline,
        Some("cancel") => DesktopMcpElicitationAction::Cancel,
        _ => {
            return Err(DesktopApplicationError::InvalidPendingInteraction {
                request_id: pending.request_id.clone(),
                message: "MCP elicitation response has an invalid action".to_owned(),
            })
        }
    };
    if accepted != action.accepted() {
        return Err(DesktopApplicationError::InvalidPendingInteraction {
            request_id: pending.request_id.clone(),
            message: "MCP elicitation action does not match its accepted state".to_owned(),
        });
    }
    if action != DesktopMcpElicitationAction::Accept {
        return Ok((false, json!({ "action": action.as_str() })));
    }
    let content = response.get("content").and_then(Value::as_object);
    DesktopMcpElicitation::from_payload(&pending.payload)
        .and_then(|elicitation| elicitation.response(action, content))
        .map_err(|error| DesktopApplicationError::InvalidPendingInteraction {
            request_id: pending.request_id.clone(),
            message: error.to_string(),
        })
}

impl DesktopApplication {
    pub fn task_runtime_snapshot(&self, task_id: &TaskId) -> DesktopTaskRuntimeSnapshot {
        self.inner.agent.snapshot(task_id)
    }

    #[cfg(debug_assertions)]
    pub fn task_turn_queue_debug_snapshot(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<DesktopDurableTurnDebugSnapshot>, DesktopApplicationError> {
        self.inner
            .pending_turns
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
            .list_debug(task_id)
            .map(|turns| {
                turns
                    .into_iter()
                    .map(|turn| DesktopDurableTurnDebugSnapshot {
                        turn_id: turn.turn_id,
                        state: turn.state,
                        claim_attempts: turn.claim_attempts,
                        owned_by_current_epoch: turn.claim_epoch.as_deref()
                            == Some(self.inner.turn_claim_epoch.as_str()),
                    })
                    .collect()
            })
            .map_err(Into::into)
    }

    #[cfg(debug_assertions)]
    pub fn turn_queue_quarantine_debug_snapshot(
        &self,
    ) -> Result<Vec<DesktopQuarantinedTurnDebugSnapshot>, DesktopApplicationError> {
        self.inner
            .pending_turns
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
            .list_quarantined()
            .map(|turns| {
                turns
                    .into_iter()
                    .map(|turn| DesktopQuarantinedTurnDebugSnapshot {
                        task_id: turn.task_id,
                        turn_id: turn.turn_id,
                        original_state: turn.original_state,
                        reason_code: turn.reason_code,
                        quarantined_at: turn.quarantined_at,
                    })
                    .collect()
            })
            .map_err(Into::into)
    }

    #[cfg(debug_assertions)]
    pub fn corrupt_queued_turn_for_debug(
        &self,
        turn_id: &str,
    ) -> Result<bool, DesktopApplicationError> {
        if turn_id.trim().is_empty() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "turn_id",
                message: "debug corruption target must not be empty".to_owned(),
            });
        }
        self.inner
            .pending_turns
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
            .corrupt_request_for_debug(turn_id)
            .map_err(Into::into)
    }

    #[cfg(debug_assertions)]
    pub fn seed_interrupted_tool_for_debug(
        &self,
        task_id: &TaskId,
        turn_id: &str,
    ) -> Result<DesktopTurnDispatch, DesktopApplicationError> {
        if turn_id.trim().is_empty() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "turn_id",
                message: "debug interrupted tool turn must not be empty".to_owned(),
            });
        }
        let mut request = DesktopTurnRequest::new(task_id.clone(), "验证中断操作恢复");
        request.permission = DesktopExecutionPermission::Full;
        request.auto_turn_decision_applied = true;
        let request = self.prepare_task_turn_request(request)?;
        let session = self.open_task_agent_wire_session(task_id)?;
        let mut message = AgentMessage::user(&request.content);
        let goal = self.task_goal(task_id)?;
        let task = self.get_task(task_id)?;
        let architecture = task
            .project_id
            .as_ref()
            .map(|project_id| self.project_architecture(project_id))
            .transpose()?;
        let worktree_instructions = self.worktree_auto_instructions_for_task(task_id)?;
        message.metadata = Some(turn_context(
            task_id,
            turn_id,
            &request,
            goal.as_ref(),
            task.project_id.as_ref(),
            architecture.as_ref(),
            worktree_instructions.as_deref(),
        ));
        self.authority()
            .shared_runtime()
            .inner()
            .seed_interrupted_tool_for_debug(
                task_id,
                &session.session_id,
                turn_id,
                message,
                AgentToolCall {
                    call_id: format!("{turn_id}:tool"),
                    name: "computer.fs.write".into(),
                    input: json!({
                        "path": "native-agent-debug-tool-recovery.txt",
                        "content": "this write must not run before recovery confirmation"
                    }),
                },
            )
            .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        let submission = self
            .inner
            .turn_submission
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("turn submission"))?;
        self.inner
            .pending_turns
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
            .enqueue(turn_id, &request)?;
        let (dispatch, _should_start) =
            self.accept_persisted_task_turn(request, turn_id.to_owned(), false)?;
        drop(submission);
        Ok(dispatch)
    }

    pub fn restore_task_runtime_from_projection(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopTaskRuntimeSnapshot, DesktopApplicationError> {
        let current = self.inner.agent.snapshot(task_id);
        if current.phase != "idle" {
            return Ok(current);
        }
        let pending = self
            .task_session_snapshot(task_id)?
            .pending
            .into_iter()
            .rev()
            .find(|pending| {
                pending.status == PendingProjectionStatus::Open
                    && matches!(
                        pending.kind.as_str(),
                        "permission_approval"
                            | "ask_user"
                            | "plan_approval"
                            | "mcp_elicitation"
                            | "architecture_change"
                    )
            });
        let Some(pending) = pending else {
            return Ok(self.inner.agent.snapshot(task_id));
        };
        let turn_id = pending.turn_id.as_deref().ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: pending.request_id.clone(),
                message: "interaction is missing its turn id".to_owned(),
            }
        })?;
        let phase = if pending.kind == "permission_approval" {
            ActiveTurnPhase::WaitingApproval
        } else {
            ActiveTurnPhase::WaitingInteraction
        };
        self.inner.agent.restore_projected_wait(
            task_id,
            turn_id,
            pending.agent_session.as_str(),
            phase,
        );
        if pending.kind == "architecture_change" {
            let payload: DesktopArchitectureInteractionPayload =
                serde_json::from_value(pending.payload).map_err(|error| {
                    DesktopApplicationError::InvalidPendingInteraction {
                        request_id: pending.request_id,
                        message: format!("invalid restored architecture payload: {error}"),
                    }
                })?;
            let mut request = DesktopTurnRequest::new(task_id.clone(), "");
            request.permission = match payload.permission {
                ArchitecturePermission::Full => DesktopExecutionPermission::Full,
                ArchitecturePermission::Ask => DesktopExecutionPermission::Ask,
                ArchitecturePermission::Readonly => DesktopExecutionPermission::Readonly,
            };
            self.inner
                .agent
                .replace_active_request(task_id, turn_id, request);
        }
        Ok(self.inner.agent.snapshot(task_id))
    }

    pub fn open_task_agent_wire_session(
        &self,
        task_id: &TaskId,
    ) -> Result<AgentSession, DesktopApplicationError> {
        let task = self.get_task(task_id)?;
        let runtime = self.authority().shared_runtime();
        let profile = runtime
            .inner()
            .refresh_product_profile(None)
            .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        let existing = self
            .authority()
            .list_session_bindings(task_id)?
            .into_iter()
            .next();
        let session = self
            .authority()
            .open_agent_task_session(
                task_id,
                existing
                    .as_ref()
                    .map(|binding| binding.agent_session.as_str()),
                &profile.profile_id,
                Some(task.title),
            )
            .map_err(agent_wire_error)?;
        self.persist_session_binding(task_id, &session.session_id, &profile.profile_id)?;
        Ok(session)
    }

    pub fn dispatch_agent_wire(
        &self,
        request: AgentWireRequestEnvelope,
    ) -> Result<AgentWireResponseEnvelope, AgentWireError> {
        self.authority().dispatch_agent_wire(request)
    }

    pub fn fork_task_agent_session_through_turn(
        &self,
        task_id: &TaskId,
        source_turn_id: &str,
    ) -> Result<String, DesktopApplicationError> {
        let source_turn_id = source_turn_id.trim();
        if source_turn_id.is_empty() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "source_turn_id",
                message: "source turn id must not be empty".to_owned(),
            });
        }
        if self.task_runtime_snapshot(task_id).phase != "idle" {
            return Err(DesktopApplicationError::InvalidInput {
                field: "task_runtime",
                message: "task must be idle before forking its Agent session".to_owned(),
            });
        }
        let source = self
            .authority()
            .list_session_bindings(task_id)?
            .into_iter()
            .next()
            .ok_or_else(|| DesktopApplicationError::InvalidInput {
                field: "agent_session",
                message: "task has no Agent session to fork".to_owned(),
            })?;
        let target_session_id =
            format!("native-{}-fork-{}", task_id.as_str(), uuid::Uuid::new_v4());
        let forked = self
            .authority()
            .fork_agent_task_session_through_turn(
                source.agent_session.as_str(),
                &target_session_id,
                source_turn_id,
            )
            .map_err(agent_wire_error)?;
        self.replace_session_binding(task_id, &forked.session_id, &forked.profile_id)?;
        Ok(forked.session_id)
    }

    pub fn interrupt_projected_task_turn(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopInterruptResult, DesktopApplicationError> {
        let runtime = self.authority().shared_runtime();
        let mut candidate = None::<(u64, String, String)>;
        for binding in self.authority().list_session_bindings(task_id)? {
            let session_id = binding.agent_session.as_str().to_owned();
            let session = runtime
                .inner()
                .session_snapshot(&session_id)
                .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
            for event in session.events {
                let AgentEvent::TurnState { turn_id, status } = event.event else {
                    continue;
                };
                if matches!(status.as_str(), "completed" | "cancelled" | "failed") {
                    continue;
                }
                if candidate
                    .as_ref()
                    .is_none_or(|(sequence, _, _)| event.sequence > *sequence)
                {
                    candidate = Some((event.sequence, session_id.clone(), turn_id));
                }
            }
        }
        let Some((_, session_id, turn_id)) = candidate else {
            return Err(DesktopApplicationError::NoActiveTurn(task_id.clone()));
        };
        runtime
            .inner()
            .cancel_session_turn(&session_id, &turn_id)
            .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        self.emit_event(DesktopEventKind::TimelineChanged {
            task_id: task_id.clone(),
            cursor: None,
        });
        Ok(DesktopInterruptResult {
            turn_id,
            cancellation_requested: true,
        })
    }

    pub fn respond_projected_task_approval(
        &self,
        task_id: &TaskId,
        request_id: &str,
        approved: bool,
    ) -> Result<DesktopApprovalResponse, DesktopApplicationError> {
        let pending = self
            .task_session_snapshot(task_id)?
            .pending
            .into_iter()
            .find(|pending| {
                pending.request_id == request_id
                    && pending.status == PendingProjectionStatus::Open
                    && pending.kind == "permission_approval"
            })
            .ok_or_else(|| DesktopApplicationError::PendingInteractionNotFound {
                task_id: task_id.clone(),
                request_id: request_id.to_owned(),
            })?;
        let turn_id = pending.turn_id.clone().ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "approval is missing its turn id".to_owned(),
            }
        })?;
        let version = pending.action_revision.ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "approval is missing its action revision".to_owned(),
            }
        })?;
        let events_application = self.clone();
        let events_task_id = task_id.clone();
        self.authority()
            .respond_agent_task_approval_observed(
                ProductApprovalDecision {
                    session_id: pending.agent_session.as_str().to_owned(),
                    turn_id: turn_id.clone(),
                    action_id: request_id.to_owned(),
                    version,
                    approved,
                },
                move |_| {
                    events_application.emit_event(DesktopEventKind::TimelineChanged {
                        task_id: events_task_id.clone(),
                        cursor: None,
                    });
                },
            )
            .map_err(agent_wire_error)?;
        Ok(DesktopApprovalResponse {
            turn_id,
            request_id: request_id.to_owned(),
            approved,
        })
    }

    pub fn respond_projected_task_interaction(
        &self,
        task_id: &TaskId,
        request_id: &str,
        accepted: bool,
        response: Value,
    ) -> Result<DesktopInteractionResponse, DesktopApplicationError> {
        let pending = self
            .task_session_snapshot(task_id)?
            .pending
            .into_iter()
            .find(|pending| {
                pending.request_id == request_id
                    && pending.status == PendingProjectionStatus::Open
                    && supported_pending_interaction_kind(&pending.kind)
            })
            .ok_or_else(|| DesktopApplicationError::PendingInteractionNotFound {
                task_id: task_id.clone(),
                request_id: request_id.to_owned(),
            })?;
        let turn_id = pending.turn_id.clone().ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "interaction is missing its turn id".to_owned(),
            }
        })?;
        let version = pending.action_revision.ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "interaction is missing its version".to_owned(),
            }
        })?;
        let (accepted, response) =
            normalized_pending_interaction_response(&pending, accepted, response)?;
        let events_application = self.clone();
        let events_task_id = task_id.clone();
        self.authority()
            .respond_agent_task_interaction_observed(
                InteractionResolution {
                    session_id: pending.agent_session.as_str().to_owned(),
                    turn_id: turn_id.clone(),
                    version,
                    interaction_id: request_id.to_owned(),
                    accepted,
                    response,
                },
                move |_| {
                    events_application.emit_event(DesktopEventKind::TimelineChanged {
                        task_id: events_task_id.clone(),
                        cursor: None,
                    });
                },
            )
            .map_err(agent_wire_error)?;
        Ok(DesktopInteractionResponse {
            turn_id,
            request_id: request_id.to_owned(),
            accepted,
            continuation: None,
        })
    }

    pub fn start_task_turn(
        &self,
        request: DesktopTurnRequest,
    ) -> Result<DesktopTurnDispatch, DesktopApplicationError> {
        let request = self.prepare_task_turn_request(request)?;
        let task_id = request.task_id.clone();
        let submission = self
            .inner
            .turn_submission
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("turn submission"))?;
        let turn_id = format!("native-turn-{}", Uuid::new_v4());
        self.inner
            .pending_turns
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
            .enqueue(&turn_id, &request)?;
        let (dispatch, should_start) = self.accept_persisted_task_turn(request, turn_id, false)?;
        drop(submission);
        if should_start {
            self.activate_turn_worker(task_id, dispatch.turn_id.clone())?;
        }
        Ok(dispatch)
    }

    pub(crate) fn prepare_task_turn_request(
        &self,
        mut request: DesktopTurnRequest,
    ) -> Result<DesktopTurnRequest, DesktopApplicationError> {
        request.content = turn_content_with_references(&request);
        if let Some(branch) = request.session_branch.as_mut() {
            branch.source_turn_id = branch.source_turn_id.trim().to_owned();
            if branch.source_turn_id.is_empty() {
                return Err(DesktopApplicationError::InvalidInput {
                    field: "session_branch.source_turn_id",
                    message: "source turn id must not be empty".to_owned(),
                });
            }
        }
        if request.content.is_empty()
            && request.attachments.is_empty()
            && request.conversation_references.is_empty()
            && request.workflow.is_none()
        {
            return Err(DesktopApplicationError::InvalidInput {
                field: "content",
                message: "message content and attachments must not both be empty".to_owned(),
            });
        }
        let task = self.get_task(&request.task_id)?;
        self.ensure_task_runnable(&request.task_id)?;
        if request.workspace_path.is_none() {
            request.workspace_path = self.workspace_path_for_task(&task)?;
        }
        if request.allow_auto_turn_decision && request.auto_turn_settings.is_none() {
            let settings = self.agent_interaction_settings()?.auto_turn_decision;
            request.auto_turn_settings = Some(settings);
        } else if !request.allow_auto_turn_decision {
            request.auto_turn_decision_applied = true;
        }
        Ok(request)
    }

    pub(crate) fn accept_persisted_task_turn(
        &self,
        request: DesktopTurnRequest,
        turn_id: String,
        guide_already_queued: bool,
    ) -> Result<(DesktopTurnDispatch, bool), DesktopApplicationError> {
        let task_id = request.task_id.clone();
        let (dispatch, should_start, inserted) = self
            .inner
            .agent
            .enqueue_idempotent(request.clone(), turn_id.clone());
        if !inserted {
            return Err(DesktopApplicationError::Agent(format!(
                "Native Agent turn id `{turn_id}` is already active"
            )));
        }
        if !guide_already_queued && matches!(dispatch.kind, DesktopTurnDispatchKind::Queued { .. })
        {
            if let Some(guide_id) = request.guide_id.as_deref() {
                if let Err(error) =
                    self.set_task_guide_status(guide_id, DesktopTodoGuideStatus::Queued)
                {
                    self.inner.agent.abort_prepared(&task_id, &dispatch.turn_id);
                    return Err(error);
                }
            }
        }
        if let DesktopTurnDispatchKind::Queued { position } = dispatch.kind {
            self.emit_event(DesktopEventKind::TurnStateChanged {
                task_id: task_id.clone(),
                turn_id: dispatch.turn_id.clone(),
                state: DesktopTurnState::Queued { position },
            });
        }
        Ok((dispatch, should_start))
    }

    pub fn restore_persisted_turn_queue(
        &self,
    ) -> Result<Vec<DesktopTurnDispatch>, DesktopApplicationError> {
        let (newly_quarantined, quarantine_history) = {
            let mut pending_turns = self
                .inner
                .pending_turns
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?;
            let newly_quarantined = pending_turns.quarantine_invalid_rows()?;
            let quarantine_history = pending_turns.list_quarantined()?;
            (newly_quarantined, quarantine_history)
        };
        for quarantined in quarantine_history {
            let Ok(task_id) = TaskId::new(quarantined.task_id) else {
                continue;
            };
            let runtime = self.inner.agent.snapshot(&task_id);
            if runtime.turn_id.as_deref() != Some(quarantined.turn_id.as_str()) {
                continue;
            }
            if let Some(session_id) = runtime.session_id.as_deref() {
                if let Err(error) = self
                    .authority()
                    .shared_runtime()
                    .inner()
                    .cancel_session_turn(session_id, quarantined.turn_id.as_str())
                {
                    eprintln!(
                        "failed to cancel quarantined Native Agent turn `{}`: {error}",
                        quarantined.turn_id
                    );
                } else {
                    self.emit_event(DesktopEventKind::TimelineChanged {
                        task_id: task_id.clone(),
                        cursor: None,
                    });
                }
            }
            if self
                .inner
                .agent
                .finish_without_next(&task_id, quarantined.turn_id.as_str())
            {
                self.emit_event(DesktopEventKind::TurnStateChanged {
                    task_id,
                    turn_id: quarantined.turn_id,
                    state: DesktopTurnState::Failed {
                        message: "一条损坏的待处理任务记录已被隔离，其余任务将继续恢复。"
                            .to_owned(),
                    },
                });
            }
        }
        for quarantined in newly_quarantined {
            let task_id = TaskId::new(quarantined.task_id).ok();
            if let Some(guide_id) = quarantined.guide_id.as_deref() {
                if let Err(error) =
                    self.set_task_guide_status(guide_id, DesktopTodoGuideStatus::Pending)
                {
                    eprintln!(
                        "failed to reset Guide for quarantined Native Agent turn `{}`: {error}",
                        quarantined.turn_id
                    );
                }
            }
            if let (Some(run_id), Some(node_id)) = (
                quarantined.automation_run_id.as_deref(),
                quarantined.automation_node_id.as_deref(),
            ) {
                if let Err(error) =
                    self.complete_automation_agent_turn(AutomationCompleteAgentInput {
                        run_id: run_id.to_owned(),
                        node_id: Some(node_id.to_owned()),
                        turn_id: quarantined.turn_id.clone(),
                        success: false,
                        payload: task_id.as_ref().map(|task_id| {
                            json!({
                                "taskId": task_id.as_str(),
                                "turnId": quarantined.turn_id.clone(),
                                "recovery": "quarantined",
                            })
                        }),
                        error: Some(
                            "Native Agent durable turn was quarantined during recovery".to_owned(),
                        ),
                    })
                {
                    eprintln!(
                        "failed to complete Automation node for quarantined Native Agent turn `{}`: {error}",
                        quarantined.turn_id
                    );
                }
            }
            eprintln!(
                "quarantined invalid Native Agent turn `{}` ({})",
                quarantined.turn_id, quarantined.reason_code
            );
            self.emit_event(DesktopEventKind::TurnRecoveryIssue {
                task_id,
                turn_id: quarantined.turn_id,
                reason: quarantined.reason_code,
            });
        }
        let submission = self
            .inner
            .turn_submission
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("turn submission"))?;
        let task_ids = self
            .inner
            .pending_turns
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
            .list_task_ids()?;
        let mut dispatches = Vec::new();
        let mut workers = Vec::new();
        for task_id in task_ids {
            if self.get_task(&task_id).is_err() {
                self.inner
                    .pending_turns
                    .lock()
                    .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
                    .clear_task(&task_id)?;
                continue;
            }
            let active_turn_id = self.inner.agent.snapshot(&task_id).turn_id;
            self.inner
                .pending_turns
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
                .prepare_recovery(
                    &task_id,
                    active_turn_id.as_deref(),
                    &self.inner.turn_claim_epoch,
                )?;
            let turns = self
                .inner
                .pending_turns
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
                .list(&task_id)?;
            for mut persisted in turns {
                if active_turn_id.as_deref() != Some(persisted.turn_id.as_str())
                    && self
                        .persisted_turn_terminal_status(&task_id, &persisted.turn_id)?
                        .is_some()
                {
                    self.inner
                        .pending_turns
                        .lock()
                        .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
                        .remove(&persisted.turn_id)?;
                    continue;
                }
                if persisted.request.allow_auto_turn_decision
                    && persisted.request.auto_turn_settings.is_none()
                {
                    let prepared = self.prepare_task_turn_request(persisted.request.clone())?;
                    self.inner
                        .pending_turns
                        .lock()
                        .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
                        .update_request(&persisted.turn_id, &prepared)?;
                    persisted.request = prepared;
                }
                let guide_id = persisted.request.guide_id.clone();
                if active_turn_id.as_deref() == Some(persisted.turn_id.as_str()) {
                    let claim_token = persisted.claim_token.clone().ok_or_else(|| {
                        DesktopApplicationError::Agent(format!(
                            "restored active turn `{}` has no durable claim token",
                            persisted.turn_id
                        ))
                    })?;
                    if !self.inner.agent.hydrate_restored_active(
                        &task_id,
                        &persisted.turn_id,
                        persisted.request,
                        claim_token,
                    ) {
                        return Err(DesktopApplicationError::Agent(format!(
                            "restored active turn `{}` could not bind its durable request",
                            persisted.turn_id
                        )));
                    }
                    if let Some(guide_id) = guide_id.as_deref() {
                        self.set_task_guide_status(guide_id, DesktopTodoGuideStatus::Sent)?;
                    }
                    continue;
                }
                if persisted.state != PersistedDesktopTurnState::Queued {
                    return Err(DesktopApplicationError::Agent(format!(
                        "restored turn `{}` remained claimed without an active projection",
                        persisted.turn_id
                    )));
                }
                let (dispatch, should_start, inserted) = self
                    .inner
                    .agent
                    .enqueue_idempotent(persisted.request, persisted.turn_id.clone());
                if !inserted {
                    continue;
                }
                if let Some(guide_id) = guide_id.as_deref() {
                    self.set_task_guide_status(guide_id, DesktopTodoGuideStatus::Queued)?;
                }
                if should_start {
                    workers.push((task_id.clone(), persisted.turn_id));
                }
                if let DesktopTurnDispatchKind::Queued { position } = dispatch.kind {
                    self.emit_event(DesktopEventKind::TurnStateChanged {
                        task_id: task_id.clone(),
                        turn_id: dispatch.turn_id.clone(),
                        state: DesktopTurnState::Queued { position },
                    });
                }
                dispatches.push(dispatch);
            }
        }
        drop(submission);
        for (task_id, turn_id) in workers {
            self.activate_turn_worker(task_id, turn_id)?;
        }
        Ok(dispatches)
    }

    pub(crate) fn start_task_turn_idempotent(
        &self,
        request: DesktopTurnRequest,
        idempotency_key: &str,
    ) -> Result<DesktopIdempotentTurnStart, DesktopApplicationError> {
        if idempotency_key.trim().is_empty() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "idempotency_key",
                message: "automation turn idempotency key must not be empty".to_owned(),
            });
        }
        let request = self.prepare_task_turn_request(request)?;
        let task_id = request.task_id.clone();
        let turn_id = format!("automation-turn:{idempotency_key}");
        let submission = self
            .inner
            .turn_submission
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("turn submission"))?;
        if let Some(status) = self.persisted_turn_terminal_status(&task_id, &turn_id)? {
            self.inner
                .pending_turns
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
                .remove(&turn_id)?;
            if status == "completed" {
                return Ok(DesktopIdempotentTurnStart::Completed { turn_id });
            }
            return Ok(DesktopIdempotentTurnStart::TerminalConflict { turn_id, status });
        }
        let (dispatch, should_start, inserted) = self
            .inner
            .agent
            .enqueue_idempotent(request, turn_id.clone());
        drop(submission);
        Ok(DesktopIdempotentTurnStart::Dispatch {
            dispatch,
            worker_start_required: should_start,
            turn_inserted: inserted,
        })
    }

    fn persisted_turn_terminal_status(
        &self,
        task_id: &TaskId,
        turn_id: &str,
    ) -> Result<Option<String>, DesktopApplicationError> {
        let runtime = self.authority().shared_runtime();
        let mut latest = None::<(u64, String)>;
        for binding in self.authority().list_session_bindings(task_id)? {
            let Ok(session) = runtime
                .inner()
                .session_snapshot(binding.agent_session.as_str())
            else {
                continue;
            };
            for event in session.events {
                let AgentEvent::TurnState {
                    turn_id: event_turn_id,
                    status,
                } = event.event
                else {
                    continue;
                };
                if event_turn_id == turn_id
                    && matches!(status.as_str(), "completed" | "cancelled" | "failed")
                    && latest
                        .as_ref()
                        .is_none_or(|(sequence, _)| event.sequence > *sequence)
                {
                    latest = Some((event.sequence, status));
                }
            }
        }
        Ok(latest.map(|(_, status)| status))
    }

    pub(crate) fn cancel_automation_agent_turn(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        run_id: &str,
        node_id: &str,
    ) -> Result<(), DesktopApplicationError> {
        let Some(request) = self.inner.agent.request(task_id, turn_id) else {
            return match self.persisted_turn_terminal_status(task_id, turn_id)? {
                Some(status) if status == "cancelled" => Ok(()),
                Some(status) => Err(DesktopApplicationError::Agent(format!(
                    "Automation Agent turn `{turn_id}` is already {status}"
                ))),
                None => Err(DesktopApplicationError::Agent(format!(
                    "Automation Agent turn `{turn_id}` does not exist"
                ))),
            };
        };
        let correlation_matches = request.automation.as_ref().is_some_and(|correlation| {
            correlation.run_id == run_id && correlation.node_id == node_id
        });
        if !correlation_matches {
            return Err(DesktopApplicationError::Agent(format!(
                "Automation Agent turn `{turn_id}` does not belong to run `{run_id}` node `{node_id}`"
            )));
        }

        if self.inner.agent.is_prepared_active(task_id, turn_id) {
            return self.discard_prepared_turn(
                task_id.clone(),
                turn_id.to_owned(),
                DesktopTurnState::Cancelled,
            );
        }

        let Some(cancel) = self.inner.agent.request_automation_cancel(task_id, turn_id) else {
            return self.discard_prepared_turn(
                task_id.clone(),
                turn_id.to_owned(),
                DesktopTurnState::Cancelled,
            );
        };
        if let Some(session_id) = &cancel.session_id {
            if let Err(error) = self
                .authority()
                .shared_runtime()
                .inner()
                .cancel_session_turn(session_id, &cancel.turn_id)
            {
                self.inner.agent.revert_automation_cancel(task_id, turn_id);
                return Err(DesktopApplicationError::Agent(error.to_string()));
            }
        }
        self.emit_event(DesktopEventKind::TimelineChanged {
            task_id: task_id.clone(),
            cursor: None,
        });
        if matches!(
            cancel.phase,
            ActiveTurnPhase::WaitingApproval | ActiveTurnPhase::WaitingInteraction
        ) {
            self.finish_turn(task_id.clone(), cancel.turn_id, DesktopTurnState::Cancelled);
        }
        Ok(())
    }

    pub fn interrupt_task_turn(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopInterruptResult, DesktopApplicationError> {
        let cancel = self
            .inner
            .agent
            .request_cancel(task_id)
            .ok_or_else(|| DesktopApplicationError::NoActiveTurn(task_id.clone()))?;
        if let Some(session_id) = &cancel.session_id {
            self.authority()
                .shared_runtime()
                .inner()
                .cancel_session_turn(session_id, &cancel.turn_id)
                .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        }
        self.emit_event(DesktopEventKind::TimelineChanged {
            task_id: task_id.clone(),
            cursor: None,
        });
        if matches!(
            cancel.phase,
            ActiveTurnPhase::WaitingApproval | ActiveTurnPhase::WaitingInteraction
        ) {
            self.finish_turn(
                task_id.clone(),
                cancel.turn_id.clone(),
                DesktopTurnState::Cancelled,
            );
        }
        Ok(DesktopInterruptResult {
            turn_id: cancel.turn_id,
            cancellation_requested: true,
        })
    }

    pub fn respond_task_approval(
        &self,
        task_id: &TaskId,
        request_id: &str,
        approved: bool,
    ) -> Result<DesktopApprovalResponse, DesktopApplicationError> {
        let active = self
            .inner
            .agent
            .waiting_approval(task_id)
            .ok_or_else(|| DesktopApplicationError::NoActiveTurn(task_id.clone()))?;
        let active_session = active.session_id.ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "the active turn is missing its Agent session".to_owned(),
            }
        })?;
        let pending = self
            .task_session_snapshot(task_id)?
            .pending
            .into_iter()
            .find(|pending| {
                pending.request_id == request_id
                    && pending.status == PendingProjectionStatus::Open
                    && pending.agent_session.as_str() == active_session
                    && pending.turn_id.as_deref() == Some(active.turn_id.as_str())
            })
            .ok_or_else(|| DesktopApplicationError::PendingInteractionNotFound {
                task_id: task_id.clone(),
                request_id: request_id.to_owned(),
            })?;
        if pending.kind != "permission_approval" {
            return Err(DesktopApplicationError::UnsupportedPendingInteraction {
                request_id: request_id.to_owned(),
                kind: pending.kind,
            });
        }
        let turn_id = pending.turn_id.clone().ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "approval is missing its turn id".to_owned(),
            }
        })?;
        let version = pending.action_revision.ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "approval is missing its action revision".to_owned(),
            }
        })?;
        if !self.inner.agent.begin_approval(task_id, &turn_id) {
            return Err(DesktopApplicationError::TurnNotWaitingApproval {
                task_id: task_id.clone(),
                turn_id,
            });
        }
        let decision = ProductApprovalDecision {
            session_id: pending.agent_session.as_str().to_owned(),
            turn_id: turn_id.clone(),
            action_id: pending.request_id.clone(),
            version,
            approved,
        };
        self.emit_event(DesktopEventKind::TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.clone(),
            state: DesktopTurnState::ResolvingApproval,
        });
        self.spawn_approval_worker(task_id.clone(), decision)?;
        Ok(DesktopApprovalResponse {
            turn_id,
            request_id: request_id.to_owned(),
            approved,
        })
    }

    pub fn respond_task_interaction(
        &self,
        task_id: &TaskId,
        request_id: &str,
        accepted: bool,
        response: Value,
    ) -> Result<DesktopInteractionResponse, DesktopApplicationError> {
        let pending = self
            .task_session_snapshot(task_id)?
            .pending
            .into_iter()
            .find(|pending| {
                pending.request_id == request_id && pending.status == PendingProjectionStatus::Open
            })
            .ok_or_else(|| DesktopApplicationError::PendingInteractionNotFound {
                task_id: task_id.clone(),
                request_id: request_id.to_owned(),
            })?;
        if !supported_pending_interaction_kind(&pending.kind) {
            return Err(DesktopApplicationError::UnsupportedPendingInteraction {
                request_id: request_id.to_owned(),
                kind: pending.kind,
            });
        }
        let turn_id = pending.turn_id.clone().ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "interaction is missing its turn id".to_owned(),
            }
        })?;
        let version = pending.action_revision.ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "interaction is missing its version".to_owned(),
            }
        })?;
        let (accepted, response) =
            normalized_pending_interaction_response(&pending, accepted, response)?;
        let active = self
            .inner
            .agent
            .waiting_interaction(task_id)
            .ok_or_else(|| DesktopApplicationError::TurnNotWaitingInteraction {
                task_id: task_id.clone(),
                turn_id: turn_id.clone(),
            })?;
        if active.turn_id != turn_id
            || active.session_id.as_deref() != Some(pending.agent_session.as_str())
        {
            return Err(DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "interaction does not belong to the active turn".to_owned(),
            });
        }
        if !self.inner.agent.begin_interaction(task_id, &turn_id) {
            return Err(DesktopApplicationError::TurnNotWaitingInteraction {
                task_id: task_id.clone(),
                turn_id,
            });
        }
        let resolution = InteractionResolution {
            session_id: pending.agent_session.as_str().to_owned(),
            turn_id: turn_id.clone(),
            version,
            interaction_id: request_id.to_owned(),
            accepted,
            response,
        };
        self.emit_event(DesktopEventKind::TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.clone(),
            state: DesktopTurnState::ResolvingInteraction,
        });
        self.spawn_interaction_worker(task_id.clone(), resolution)?;
        Ok(DesktopInteractionResponse {
            turn_id,
            request_id: request_id.to_owned(),
            accepted,
            continuation: None,
        })
    }

    pub fn respond_task_architecture_interaction(
        &self,
        task_id: &TaskId,
        request_id: &str,
        decision: DesktopArchitectureInteractionDecision,
    ) -> Result<DesktopArchitectureInteractionResponse, DesktopApplicationError> {
        let pending = self
            .task_session_snapshot(task_id)?
            .pending
            .into_iter()
            .find(|pending| {
                pending.request_id == request_id
                    && pending.kind == "architecture_change"
                    && pending.status == PendingProjectionStatus::Open
            })
            .ok_or_else(|| DesktopApplicationError::PendingInteractionNotFound {
                task_id: task_id.clone(),
                request_id: request_id.to_owned(),
            })?;
        let turn_id = pending.turn_id.clone().ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "architecture interaction is missing its turn id".to_owned(),
            }
        })?;
        let waiting = self
            .inner
            .agent
            .waiting_interaction(task_id)
            .ok_or_else(|| DesktopApplicationError::TurnNotWaitingInteraction {
                task_id: task_id.clone(),
                turn_id: turn_id.clone(),
            })?;
        if waiting.turn_id != turn_id
            || waiting.session_id.as_deref() != Some(pending.agent_session.as_str())
        {
            return Err(DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "architecture interaction does not belong to the active turn".to_owned(),
            });
        }
        let request = self
            .inner
            .agent
            .request(task_id, &turn_id)
            .ok_or_else(|| DesktopApplicationError::NoActiveTurn(task_id.clone()))?;
        let payload: DesktopArchitectureInteractionPayload =
            serde_json::from_value(pending.payload.clone()).map_err(|error| {
                DesktopApplicationError::InvalidPendingInteraction {
                    request_id: request_id.to_owned(),
                    message: format!("invalid architecture payload: {error}"),
                }
            })?;
        let task = self.get_task(task_id)?;
        let project_id =
            task.project_id
                .ok_or_else(|| DesktopApplicationError::InvalidPendingInteraction {
                    request_id: request_id.to_owned(),
                    message: "architecture changes require a project task".to_owned(),
                })?;
        let permission = architecture_permission(request.permission);
        if payload.project_id != project_id.as_str()
            || payload.task_id != task_id.as_str()
            || payload.turn_id.as_deref() != Some(turn_id.as_str())
            || payload.backend != ArchitectureBackend::NativeAgentkit
            || payload.permission != permission
        {
            return Err(DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "architecture payload does not match the authoritative task, turn, backend or permission"
                    .to_owned(),
            });
        }
        let expected_version = payload.expected_version.ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "architecture interaction is missing its expected graph version"
                    .to_owned(),
            }
        })?;
        if decision == DesktopArchitectureInteractionDecision::Allow
            && permission == ArchitecturePermission::Readonly
        {
            return Err(DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "readonly turns cannot apply architecture changes".to_owned(),
            });
        }

        let (graph, event, message) = match decision {
            DesktopArchitectureInteractionDecision::Allow => {
                let result = self.apply_project_architecture(ProjectArchitectureApplyInput {
                    project_id: project_id.as_str().to_owned(),
                    task_id: task_id.as_str().to_owned(),
                    turn_id: Some(turn_id.clone()),
                    backend: ArchitectureBackend::NativeAgentkit,
                    permission,
                    reason: payload.reason,
                    changes: payload.changes,
                    request_id: Some(request_id.to_owned()),
                    expected_version: Some(expected_version),
                })?;
                (Some(result.graph), result.event, "架构图已更新".to_owned())
            }
            DesktopArchitectureInteractionDecision::Deny => {
                let event = self.reject_project_architecture(ProjectArchitectureRejectInput {
                    project_id: project_id.as_str().to_owned(),
                    task_id: task_id.as_str().to_owned(),
                    turn_id: Some(turn_id.clone()),
                    backend: ArchitectureBackend::NativeAgentkit,
                    permission,
                    reason: payload.reason,
                    changes: payload.changes,
                    request_id: Some(request_id.to_owned()),
                    expected_version: Some(expected_version),
                })?;
                (None, event, "架构图变更已拒绝".to_owned())
            }
        };
        let response = json!({
            "interaction": "architecture_change",
            "decision": decision,
            "graph": graph,
            "event": event,
            "message": message,
        });
        let interaction = self.respond_task_interaction(task_id, request_id, true, response)?;
        Ok(DesktopArchitectureInteractionResponse {
            decision,
            graph,
            event,
            message,
            interaction,
        })
    }

    fn workspace_path_for_task(
        &self,
        task: &ProductTask,
    ) -> Result<Option<String>, DesktopApplicationError> {
        task.project_id
            .as_ref()
            .map(|project_id| {
                self.get_project(project_id)
                    .map(|project| project.workspace_path)
            })
            .transpose()
            .map(Option::flatten)
    }

    pub(crate) fn activate_turn_worker(
        &self,
        task_id: TaskId,
        turn_id: String,
    ) -> Result<(), DesktopApplicationError> {
        let submission = self
            .inner
            .turn_submission
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("turn submission"))?;
        let request = self
            .inner
            .agent
            .request(&task_id, &turn_id)
            .ok_or_else(|| {
                DesktopApplicationError::Agent(format!(
                    "Native Agent turn `{turn_id}` has no prepared request"
                ))
            })?;
        self.inner
            .pending_turns
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
            .enqueue_idempotent(&turn_id, &request)?;
        if self.inner.agent.active(&task_id, &turn_id).is_none()
            && !self.inner.agent.promote_queued_if_idle(&task_id, &turn_id)
        {
            if let Some(position) = self
                .inner
                .agent
                .snapshot(&task_id)
                .queued_turn_ids
                .iter()
                .position(|queued_turn_id| queued_turn_id == &turn_id)
            {
                self.emit_event(DesktopEventKind::TurnStateChanged {
                    task_id,
                    turn_id,
                    state: DesktopTurnState::Queued {
                        position: position + 1,
                    },
                });
                return Ok(());
            }
            return Err(DesktopApplicationError::Agent(format!(
                "Native Agent turn `{turn_id}` is not prepared for activation"
            )));
        }
        let claimed = self
            .inner
            .pending_turns
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
            .claim(&turn_id, &self.inner.turn_claim_epoch)?;
        let Some(claimed) = claimed else {
            if self
                .inner
                .agent
                .active(&task_id, &turn_id)
                .is_some_and(|active| active.claim_token.is_some())
            {
                return Ok(());
            }
            return Err(DesktopApplicationError::Agent(format!(
                "Native Agent turn `{turn_id}` is not the first durably queued turn"
            )));
        };
        let claim_token = claimed.claim_token.ok_or_else(|| {
            DesktopApplicationError::Agent(format!(
                "Native Agent turn `{turn_id}` claim has no ownership token"
            ))
        })?;
        if !self
            .inner
            .agent
            .claim_worker_start(&task_id, &turn_id, claim_token)
        {
            return Ok(());
        }
        drop(submission);
        self.spawn_claimed_turn_worker(task_id, turn_id)
    }

    fn spawn_claimed_turn_worker(
        &self,
        task_id: TaskId,
        turn_id: String,
    ) -> Result<(), DesktopApplicationError> {
        self.emit_event(DesktopEventKind::TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.clone(),
            state: DesktopTurnState::Starting,
        });
        let application = self.clone();
        let thread_task_id = task_id.clone();
        let thread_turn_id = turn_id.clone();
        std::thread::Builder::new()
            .name(format!("lilia-native-turn-{}", task_id.as_str()))
            .spawn(move || application.run_turn_worker(thread_task_id, thread_turn_id))
            .map(|_| ())
            .map_err(|error| {
                self.finish_turn(
                    task_id,
                    turn_id,
                    DesktopTurnState::Failed {
                        message: error.to_string(),
                    },
                );
                DesktopApplicationError::Agent(format!("start Native Agent turn: {error}"))
            })
    }

    pub(crate) fn abort_prepared_turn(&self, task_id: TaskId, turn_id: String) {
        if let Err(error) = self.discard_prepared_turn(
            task_id,
            turn_id,
            DesktopTurnState::Failed {
                message: "Automation could not persist the Agent dispatch".to_owned(),
            },
        ) {
            eprintln!("failed to discard prepared Native Agent turn: {error}");
        }
    }

    fn discard_prepared_turn(
        &self,
        task_id: TaskId,
        turn_id: String,
        terminal_state: DesktopTurnState,
    ) -> Result<(), DesktopApplicationError> {
        let submission = self
            .inner
            .turn_submission
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("turn submission"))?;
        let prepared_active = self.inner.agent.is_prepared_active(&task_id, &turn_id);
        let durable_next = {
            let mut pending_turns = self
                .inner
                .pending_turns
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?;
            let persisted = pending_turns.contains(&turn_id)?;
            let result = if prepared_active {
                if persisted {
                    pending_turns.discard_queued_and_claim_next(
                        &task_id,
                        &turn_id,
                        &self.inner.turn_claim_epoch,
                    )
                } else {
                    pending_turns.claim_first(&task_id, &self.inner.turn_claim_epoch)
                }
            } else if persisted {
                pending_turns
                    .discard_queued(&task_id, &turn_id)
                    .and_then(|discarded| {
                        if discarded {
                            Ok(None)
                        } else {
                            Err(
                                crate::turn_queue::DesktopTurnQueueError::InvalidTransition {
                                    turn_id: turn_id.clone(),
                                    state: "not_queued".to_owned(),
                                    operation: "discard prepared turn",
                                },
                            )
                        }
                    })
            } else {
                Ok(None)
            };
            result?
        };
        let next = self.inner.agent.abort_prepared(&task_id, &turn_id);
        let claimed_next = match (durable_next, next) {
            (Some(durable), Some(memory)) if durable.turn_id == memory.turn_id => {
                durable.claim_token.map(|token| (memory.turn_id, token))
            }
            (None, None) => None,
            (durable, memory) => {
                return Err(DesktopApplicationError::Agent(format!(
                    "Native Agent prepared-turn recovery diverged: durable={:?}, memory={:?}",
                    durable.as_ref().map(|turn| turn.turn_id.as_str()),
                    memory.as_ref().map(|turn| turn.turn_id.as_str())
                )));
            }
        };
        self.emit_event(DesktopEventKind::TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.clone(),
            state: terminal_state,
        });
        let ready_next = claimed_next.and_then(|(next_turn_id, claim_token)| {
            if self
                .inner
                .agent
                .claim_worker_start(&task_id, &next_turn_id, claim_token)
            {
                Some(next_turn_id)
            } else {
                eprintln!(
                    "claimed Native Agent turn `{next_turn_id}` could not bind to its memory owner"
                );
                None
            }
        });
        drop(submission);
        if let Some(next_turn_id) = ready_next {
            self.spawn_claimed_turn_worker(task_id, next_turn_id)?;
        }
        Ok(())
    }

    fn run_turn_worker(&self, task_id: TaskId, turn_id: String) {
        if let Err(error) = self.run_turn(&task_id, &turn_id) {
            let state = if self.inner.agent.cancel_requested(&task_id, &turn_id) {
                DesktopTurnState::Cancelled
            } else {
                DesktopTurnState::Failed {
                    message: error.to_string(),
                }
            };
            self.finish_turn(task_id, turn_id, state);
        }
    }

    fn run_turn(&self, task_id: &TaskId, turn_id: &str) -> Result<(), DesktopApplicationError> {
        let mut active = self
            .inner
            .agent
            .active(task_id, turn_id)
            .ok_or_else(|| DesktopApplicationError::NoActiveTurn(task_id.clone()))?;
        let prepared_request = self.apply_automatic_turn_selection(active.request.clone())?;
        if prepared_request != active.request {
            self.inner
                .pending_turns
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?
                .update_request(turn_id, &prepared_request)?;
            if !self
                .inner
                .agent
                .replace_active_request(task_id, turn_id, prepared_request.clone())
            {
                return Err(DesktopApplicationError::NoActiveTurn(task_id.clone()));
            }
            active.request = prepared_request;
        }
        if let Some(guide_id) = active.request.guide_id.as_deref() {
            self.set_task_guide_status(guide_id, DesktopTodoGuideStatus::Sent)?;
        }
        if matches!(
            active.request.workflow.as_ref(),
            Some(LiliaAgentWorkflow::LiliaCompact)
        ) {
            return self.run_context_compaction_turn(task_id, turn_id, &active.request);
        }
        let task = self.get_task(task_id)?;
        let runtime = self.authority().shared_runtime();
        let profile = runtime
            .inner()
            .refresh_product_profile(None)
            .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        let existing_binding = self
            .authority()
            .list_session_bindings(task_id)?
            .into_iter()
            .next();
        let session = if let Some(branch) = active.request.session_branch.as_ref() {
            let source =
                existing_binding
                    .as_ref()
                    .ok_or_else(|| DesktopApplicationError::InvalidInput {
                        field: "agent_session",
                        message: "task has no Agent session to branch".to_owned(),
                    })?;
            let target_session_id = format!(
                "native-{}-{}-{}",
                task_id.as_str(),
                match branch.mode {
                    DesktopSessionBranchMode::Continue => "continue",
                    DesktopSessionBranchMode::Fork => "fork",
                },
                uuid::Uuid::new_v4()
            );
            self.authority()
                .fork_agent_task_session_through_turn(
                    source.agent_session.as_str(),
                    &target_session_id,
                    &branch.source_turn_id,
                )
                .map_err(agent_wire_error)?
        } else if active.request.session_fork {
            if let Some(source) = existing_binding.as_ref() {
                let target_session_id =
                    format!("native-{}-fork-{}", task_id.as_str(), uuid::Uuid::new_v4());
                self.authority()
                    .fork_agent_task_session(source.agent_session.as_str(), &target_session_id)
                    .map_err(agent_wire_error)?
            } else {
                self.authority()
                    .open_agent_task_session(
                        task_id,
                        None,
                        &profile.profile_id,
                        Some(task.title.clone()),
                    )
                    .map_err(agent_wire_error)?
            }
        } else {
            self.authority()
                .open_agent_task_session(
                    task_id,
                    existing_binding
                        .as_ref()
                        .map(|binding| binding.agent_session.as_str()),
                    &profile.profile_id,
                    Some(task.title.clone()),
                )
                .map_err(agent_wire_error)?
        };
        if active.request.session_fork || active.request.session_branch.is_some() {
            self.replace_session_binding(task_id, &session.session_id, &profile.profile_id)?;
        } else {
            self.persist_session_binding(task_id, &session.session_id, &profile.profile_id)?;
        }
        let cancel_requested =
            self.inner
                .agent
                .attach_session(task_id, turn_id, session.session_id.clone());
        if cancel_requested || active.cancellation_mode.is_some() {
            runtime
                .inner()
                .cancel_session_turn(&session.session_id, turn_id)
                .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        }
        self.emit_event(DesktopEventKind::TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.to_owned(),
            state: DesktopTurnState::Running,
        });
        self.execute_turn_hooks(
            DesktopHookEvent::UserPromptSubmit,
            task_id,
            turn_id,
            active.request.workspace_path.as_deref(),
            &active.request.content,
        )?;
        let mut message = AgentMessage::user(&active.request.content);
        let goal = self.task_goal(task_id)?;
        let architecture = task
            .project_id
            .as_ref()
            .map(|project_id| self.project_architecture(project_id))
            .transpose()?;
        let worktree_instructions = self.worktree_auto_instructions_for_task(task_id)?;
        message.metadata = Some(turn_context(
            task_id,
            turn_id,
            &active.request,
            goal.as_ref(),
            task.project_id.as_ref(),
            architecture.as_ref(),
            worktree_instructions.as_deref(),
        ));
        let events_application = self.clone();
        let events_task_id = task_id.clone();
        let page = self
            .authority()
            .submit_agent_task_turn_observed(
                &session.session_id,
                turn_id,
                vec![message],
                &format!("native-desktop:{}:{turn_id}", task_id.as_str()),
                move |events| {
                    let has_tool_window = events
                        .iter()
                        .any(|event| is_agent_tool_window_event(&event.event));
                    events_application.emit_event(DesktopEventKind::TimelineChanged {
                        task_id: events_task_id.clone(),
                        cursor: events.last().map(|event| event.sequence),
                    });
                    if has_tool_window {
                        if let Err(error) = events_application.dispatch_next_task_guide(
                            &events_task_id,
                            DesktopGuideDispatchWindow::Tool,
                        ) {
                            eprintln!("failed to dispatch Native tool-window Guide: {error}");
                        }
                    }
                },
            )
            .map_err(agent_wire_error)?;
        self.handle_turn_page(
            task_id.clone(),
            turn_id.to_owned(),
            ObservedTurnPage {
                session_id: page.session_id,
                waiting_approval: page.waiting_approval,
                waiting_interaction: page.waiting_interaction,
                completed: page.completed,
                cancelled_by_user: page.cancelled,
            },
        )
    }

    fn run_context_compaction_turn(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        request: &DesktopTurnRequest,
    ) -> Result<(), DesktopApplicationError> {
        if self.inner.agent.cancel_requested(task_id, turn_id) {
            self.finish_turn(
                task_id.clone(),
                turn_id.to_owned(),
                DesktopTurnState::Cancelled,
            );
            return Ok(());
        }
        self.emit_event(DesktopEventKind::TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.to_owned(),
            state: DesktopTurnState::Running,
        });
        self.execute_turn_hooks(
            DesktopHookEvent::UserPromptSubmit,
            task_id,
            turn_id,
            request.workspace_path.as_deref(),
            &request.content,
        )?;
        let result =
            self.compact_task_agent_context_with_commit_guard(task_id, turn_id, None, || {
                !self.inner.agent.cancel_requested(task_id, turn_id)
            })?;
        self.inner
            .agent
            .attach_session(task_id, turn_id, result.session_id);
        self.finish_turn(
            task_id.clone(),
            turn_id.to_owned(),
            DesktopTurnState::Completed,
        );
        Ok(())
    }

    fn spawn_approval_worker(
        &self,
        task_id: TaskId,
        decision: ProductApprovalDecision,
    ) -> Result<(), DesktopApplicationError> {
        let application = self.clone();
        let thread_task_id = task_id.clone();
        let turn_id = decision.turn_id.clone();
        std::thread::Builder::new()
            .name(format!("lilia-native-approval-{}", task_id.as_str()))
            .spawn(move || application.run_approval_worker(thread_task_id, decision))
            .map(|_| ())
            .map_err(|error| {
                self.inner
                    .agent
                    .restore_waiting_approval(&task_id, &turn_id);
                DesktopApplicationError::Agent(format!("start Native approval response: {error}"))
            })
    }

    fn run_approval_worker(&self, task_id: TaskId, decision: ProductApprovalDecision) {
        let turn_id = decision.turn_id.clone();
        let request_id = decision.action_id.clone();
        let approved = decision.approved;
        let events_application = self.clone();
        let events_task_id = task_id.clone();
        match self
            .authority()
            .respond_agent_task_approval_observed(decision, move |events| {
                events_application.emit_event(DesktopEventKind::TimelineChanged {
                    task_id: events_task_id.clone(),
                    cursor: events.last().map(|event| event.sequence),
                });
            }) {
            Ok(page) => {
                self.emit_event(DesktopEventKind::ApprovalChanged {
                    task_id: task_id.clone(),
                    request_id,
                    state: if approved {
                        DesktopApprovalState::Approved
                    } else {
                        DesktopApprovalState::Denied
                    },
                });
                if let Err(error) = self.handle_turn_page(
                    task_id.clone(),
                    turn_id.clone(),
                    ObservedTurnPage {
                        session_id: page.session_id,
                        waiting_approval: page.waiting_approval,
                        waiting_interaction: page.waiting_interaction,
                        completed: page.completed,
                        cancelled_by_user: page.cancelled || !approved,
                    },
                ) {
                    self.finish_turn(
                        task_id,
                        turn_id,
                        DesktopTurnState::Failed {
                            message: error.to_string(),
                        },
                    );
                }
            }
            Err(error) => {
                self.inner
                    .agent
                    .restore_waiting_approval(&task_id, &turn_id);
                self.emit_event(DesktopEventKind::TurnStateChanged {
                    task_id,
                    turn_id,
                    state: DesktopTurnState::WaitingApproval {
                        request_id: Some(request_id),
                        error: Some(format!("{}: {}", error.code, error.message)),
                    },
                });
            }
        }
    }

    fn spawn_interaction_worker(
        &self,
        task_id: TaskId,
        resolution: InteractionResolution,
    ) -> Result<(), DesktopApplicationError> {
        let application = self.clone();
        let thread_task_id = task_id.clone();
        let turn_id = resolution.turn_id.clone();
        std::thread::Builder::new()
            .name(format!("lilia-native-interaction-{}", task_id.as_str()))
            .spawn(move || application.run_interaction_worker(thread_task_id, resolution))
            .map(|_| ())
            .map_err(|error| {
                self.inner
                    .agent
                    .restore_waiting_interaction(&task_id, &turn_id);
                DesktopApplicationError::Agent(format!(
                    "start Native interaction response: {error}"
                ))
            })
    }

    fn run_interaction_worker(&self, task_id: TaskId, resolution: InteractionResolution) {
        let turn_id = resolution.turn_id.clone();
        let request_id = resolution.interaction_id.clone();
        let accepted = resolution.accepted;
        let events_application = self.clone();
        let events_task_id = task_id.clone();
        match self
            .authority()
            .respond_agent_task_interaction_observed(resolution, move |events| {
                events_application.emit_event(DesktopEventKind::TimelineChanged {
                    task_id: events_task_id.clone(),
                    cursor: events.last().map(|event| event.sequence),
                });
            }) {
            Ok(page) => {
                self.emit_event(DesktopEventKind::InteractionChanged {
                    task_id: task_id.clone(),
                    request_id,
                    state: if accepted {
                        DesktopInteractionState::Accepted
                    } else {
                        DesktopInteractionState::Declined
                    },
                });
                if let Err(error) = self.handle_turn_page(
                    task_id.clone(),
                    turn_id.clone(),
                    ObservedTurnPage {
                        session_id: page.session_id,
                        waiting_approval: page.waiting_approval,
                        waiting_interaction: page.waiting_interaction,
                        completed: page.completed,
                        cancelled_by_user: page.cancelled || !accepted,
                    },
                ) {
                    self.finish_turn(
                        task_id,
                        turn_id,
                        DesktopTurnState::Failed {
                            message: error.to_string(),
                        },
                    );
                }
            }
            Err(error) => {
                self.inner
                    .agent
                    .restore_waiting_interaction(&task_id, &turn_id);
                self.emit_event(DesktopEventKind::TurnStateChanged {
                    task_id,
                    turn_id,
                    state: DesktopTurnState::WaitingInteraction {
                        request_id: Some(request_id),
                        kind: None,
                        error: Some(format!("{}: {}", error.code, error.message)),
                    },
                });
            }
        }
    }

    fn handle_turn_page(
        &self,
        task_id: TaskId,
        turn_id: String,
        page: ObservedTurnPage,
    ) -> Result<(), DesktopApplicationError> {
        self.emit_event(DesktopEventKind::TimelineChanged {
            task_id: task_id.clone(),
            cursor: None,
        });
        if page.waiting_approval {
            if !self.inner.agent.wait_for_approval(&task_id, &turn_id) {
                return Err(DesktopApplicationError::NoActiveTurn(task_id));
            }
            let request_id = self
                .task_session_snapshot(&task_id)?
                .pending
                .into_iter()
                .rev()
                .find(|pending| {
                    pending.status == PendingProjectionStatus::Open
                        && pending.kind == "permission_approval"
                        && pending.agent_session.as_str() == page.session_id
                        && pending.turn_id.as_deref() == Some(turn_id.as_str())
                })
                .map(|pending| pending.request_id);
            self.emit_event(DesktopEventKind::TurnStateChanged {
                task_id: task_id.clone(),
                turn_id,
                state: DesktopTurnState::WaitingApproval {
                    request_id,
                    error: None,
                },
            });
            if let Err(error) =
                self.dispatch_next_task_guide(&task_id, DesktopGuideDispatchWindow::User)
            {
                eprintln!("failed to dispatch Native approval-window Guide: {error}");
            }
        } else if page.waiting_interaction {
            if !self.inner.agent.wait_for_interaction(&task_id, &turn_id) {
                return Err(DesktopApplicationError::NoActiveTurn(task_id));
            }
            let pending = self
                .task_session_snapshot(&task_id)?
                .pending
                .into_iter()
                .rev()
                .find(|pending| {
                    pending.status == PendingProjectionStatus::Open
                        && supported_pending_interaction_kind(&pending.kind)
                        && pending.agent_session.as_str() == page.session_id
                        && pending.turn_id.as_deref() == Some(turn_id.as_str())
                });
            let auto_architecture_decision = pending.as_ref().and_then(|pending| {
                (pending.kind == "architecture_change")
                    .then(|| self.inner.agent.request(&task_id, &turn_id))
                    .flatten()
                    .and_then(|request| match request.permission {
                        DesktopExecutionPermission::Full => {
                            Some(DesktopArchitectureInteractionDecision::Allow)
                        }
                        DesktopExecutionPermission::Readonly => {
                            Some(DesktopArchitectureInteractionDecision::Deny)
                        }
                        DesktopExecutionPermission::Ask => None,
                    })
            });
            self.emit_event(DesktopEventKind::TurnStateChanged {
                task_id: task_id.clone(),
                turn_id: turn_id.clone(),
                state: DesktopTurnState::WaitingInteraction {
                    request_id: pending.as_ref().map(|pending| pending.request_id.clone()),
                    kind: pending.as_ref().map(|pending| pending.kind.clone()),
                    error: None,
                },
            });
            if let (Some(pending), Some(decision)) = (pending.as_ref(), auto_architecture_decision)
            {
                match self.respond_task_architecture_interaction(
                    &task_id,
                    &pending.request_id,
                    decision,
                ) {
                    Ok(_) => return Ok(()),
                    Err(error) => {
                        self.emit_event(DesktopEventKind::TurnStateChanged {
                            task_id: task_id.clone(),
                            turn_id: turn_id.clone(),
                            state: DesktopTurnState::WaitingInteraction {
                                request_id: Some(pending.request_id.clone()),
                                kind: Some(pending.kind.clone()),
                                error: Some(error.to_string()),
                            },
                        });
                    }
                }
            }
            if let Err(error) =
                self.dispatch_next_task_guide(&task_id, DesktopGuideDispatchWindow::User)
            {
                eprintln!("failed to dispatch Native interaction-window Guide: {error}");
            }
        } else if page.cancelled_by_user {
            self.finish_turn(task_id, turn_id, DesktopTurnState::Cancelled);
        } else if page.completed {
            let state = if self.inner.agent.cancel_requested(&task_id, &turn_id) {
                DesktopTurnState::Cancelled
            } else {
                DesktopTurnState::Completed
            };
            let completed = matches!(state, DesktopTurnState::Completed);
            self.finish_turn(task_id.clone(), turn_id.clone(), state);
            if completed {
                self.spawn_title_update_after_turn(task_id, Some(turn_id));
            }
        } else {
            self.finish_turn(
                task_id,
                turn_id,
                DesktopTurnState::Failed {
                    message: "Native Agent turn ended without completion".to_owned(),
                },
            );
        }
        Ok(())
    }

    fn finish_turn(&self, task_id: TaskId, turn_id: String, state: DesktopTurnState) {
        let Some(active) = self.inner.agent.begin_finish(&task_id, &turn_id) else {
            return;
        };
        let cancellation_mode = active.cancellation_mode;
        let claim_token = active.claim_token.clone();
        let hook_workspace_path = active.request.workspace_path.clone();
        let automation = active.request.automation;
        self.emit_event(DesktopEventKind::TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.clone(),
            state: state.clone(),
        });
        let hook_context = match &state {
            DesktopTurnState::Completed => "completed".to_owned(),
            DesktopTurnState::Cancelled => "cancelled".to_owned(),
            DesktopTurnState::Failed { message } => format!("failed:{message}"),
            other => format!("{other:?}"),
        };
        if let Err(error) = self.execute_turn_hooks(
            DesktopHookEvent::Stop,
            &task_id,
            &turn_id,
            hook_workspace_path.as_deref(),
            &hook_context,
        ) {
            eprintln!("Native Agent Stop Hook failed: {error}");
        }
        if cancellation_mode != Some(TurnCancellationMode::AutomationRun) {
            if let Some(correlation) = automation {
                let (success, error) = match &state {
                    DesktopTurnState::Completed => (true, None),
                    DesktopTurnState::Cancelled => {
                        (false, Some("Native Agent turn was cancelled".to_owned()))
                    }
                    DesktopTurnState::Failed { message } => (false, Some(message.clone())),
                    _ => (
                        false,
                        Some("Native Agent turn ended without a terminal state".to_owned()),
                    ),
                };
                if let Err(error) =
                    self.complete_automation_agent_turn(AutomationCompleteAgentInput {
                        run_id: correlation.run_id,
                        node_id: Some(correlation.node_id),
                        turn_id: turn_id.clone(),
                        success,
                        payload: Some(json!({
                            "taskId": task_id.as_str(),
                            "turnId": turn_id.clone(),
                        })),
                        error,
                    })
                {
                    eprintln!("failed to complete Automation Agent node: {error}");
                }
            }
        }
        let submission = self
            .inner
            .turn_submission
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if cancellation_mode == Some(TurnCancellationMode::User) {
            let queued = self.inner.agent.queued(&task_id);
            let guides_reset = queued.iter().try_for_each(|turn| {
                let Some(guide_id) = turn.request.guide_id.as_deref() else {
                    return Ok(());
                };
                self.set_task_guide_status(guide_id, DesktopTodoGuideStatus::Pending)
                    .map(|_| ())
            });
            let discarded = if let Err(error) = guides_reset {
                eprintln!("failed to reset cancelled Native Guide queue: {error}");
                self.inner.agent.finish_without_next(&task_id, &turn_id);
                Vec::new()
            } else {
                match self
                    .inner
                    .pending_turns
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .cancel_claim_and_clear_task(&task_id, &turn_id, claim_token.as_deref())
                {
                    Ok(_) => self
                        .inner
                        .agent
                        .finish_and_clear_queue(&task_id, &turn_id)
                        .unwrap_or_default(),
                    Err(error) => {
                        eprintln!(
                            "failed to clear persisted Native Agent turns after cancellation: {error}"
                        );
                        self.inner.agent.finish_without_next(&task_id, &turn_id);
                        Vec::new()
                    }
                }
            };
            drop(submission);
            for discarded_turn in discarded {
                self.emit_event(DesktopEventKind::TurnStateChanged {
                    task_id: task_id.clone(),
                    turn_id: discarded_turn.turn_id,
                    state: DesktopTurnState::Cancelled,
                });
            }
            return;
        }
        let expected_next = self.inner.agent.queued_front(&task_id);
        let durable_next = {
            let mut pending_turns = self
                .inner
                .pending_turns
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let result = if let Some(claim_token) = claim_token.as_deref() {
                pending_turns.ack_and_claim_next(
                    &task_id,
                    &turn_id,
                    claim_token,
                    &self.inner.turn_claim_epoch,
                )
            } else {
                pending_turns.claim_first(&task_id, &self.inner.turn_claim_epoch)
            };
            match result {
                Ok(next) => next,
                Err(error) => {
                    eprintln!("failed to acknowledge persisted Native Agent turn: {error}");
                    self.inner.agent.finish_without_next(&task_id, &turn_id);
                    return;
                }
            }
        };
        if expected_next.as_ref().map(|turn| turn.turn_id.as_str())
            != durable_next.as_ref().map(|turn| turn.turn_id.as_str())
        {
            eprintln!(
                "Native Agent FIFO diverged after acknowledgement: durable={:?}, memory={:?}",
                durable_next.as_ref().map(|turn| turn.turn_id.as_str()),
                expected_next.as_ref().map(|turn| turn.turn_id.as_str())
            );
            self.inner.agent.finish_without_next(&task_id, &turn_id);
            return;
        }
        let next = self
            .inner
            .agent
            .finish_and_activate_next(&task_id, &turn_id);
        let claimed_next = match (durable_next, next) {
            (Some(durable), Some(memory)) => durable
                .claim_token
                .map(|claim_token| (memory.turn_id, claim_token)),
            (None, None) => None,
            _ => None,
        };
        let had_next = claimed_next.is_some();
        let ready_next = claimed_next.and_then(|(next_turn_id, claim_token)| {
            if self
                .inner
                .agent
                .claim_worker_start(&task_id, &next_turn_id, claim_token)
            {
                Some(next_turn_id)
            } else {
                eprintln!(
                    "claimed Native Agent turn `{next_turn_id}` could not bind to its memory owner"
                );
                None
            }
        });
        drop(submission);
        if let Some(next_turn_id) = ready_next {
            if let Err(error) = self.spawn_claimed_turn_worker(task_id, next_turn_id) {
                eprintln!("failed to start queued Native Agent turn: {error}");
            }
        } else if !had_next {
            if let Err(error) =
                self.dispatch_next_task_guide(&task_id, DesktopGuideDispatchWindow::Idle)
            {
                eprintln!("failed to dispatch Native idle-window Guide: {error}");
            }
        }
    }

    fn persist_session_binding(
        &self,
        task_id: &TaskId,
        session_id: &str,
        profile_id: &str,
    ) -> Result<AgentSessionBinding, DesktopApplicationError> {
        if let Some(binding) = self
            .authority()
            .list_session_bindings(task_id)?
            .into_iter()
            .find(|binding| binding.agent_session.as_str() == session_id)
        {
            return Ok(binding);
        }
        let client = self.authority().client()?;
        let binding = self.session_binding(task_id, session_id, profile_id)?;
        Ok(client.products().record_binding(binding)?)
    }

    pub(crate) fn replace_session_binding(
        &self,
        task_id: &TaskId,
        session_id: &str,
        profile_id: &str,
    ) -> Result<AgentSessionBinding, DesktopApplicationError> {
        let client = self.authority().client()?;
        let binding = self.session_binding(task_id, session_id, profile_id)?;
        Ok(client.replace_binding(binding)?)
    }

    fn session_binding(
        &self,
        task_id: &TaskId,
        session_id: &str,
        profile_id: &str,
    ) -> Result<AgentSessionBinding, DesktopApplicationError> {
        let client = self.authority().client()?;
        let conversation_id = client
            .products()
            .list_entities(ProductEntityKind::Conversation)?
            .into_iter()
            .find_map(|entity| match entity {
                ProductEntity::Conversation(conversation)
                    if conversation.task_id.as_ref() == Some(task_id) =>
                {
                    Some(conversation.id)
                }
                _ => None,
            });
        let binding = AgentSessionBinding {
            binding_id: BindingId::new(format!("binding:{}:{session_id}", task_id.as_str()))?,
            task_id: task_id.clone(),
            conversation_id,
            agent_session: AgentSessionRef::new(session_id.to_owned())?,
            profile_id: Some(profile_id.to_owned()),
            revision: ProductRevision::INITIAL,
        };
        Ok(binding)
    }
}

fn architecture_permission(permission: DesktopExecutionPermission) -> ArchitecturePermission {
    match permission {
        DesktopExecutionPermission::Full => ArchitecturePermission::Full,
        DesktopExecutionPermission::Ask => ArchitecturePermission::Ask,
        DesktopExecutionPermission::Readonly => ArchitecturePermission::Readonly,
    }
}

pub(crate) fn turn_content_with_references(request: &DesktopTurnRequest) -> String {
    let mut content = request.content.trim().to_owned();
    for attachment in &request.attachments {
        let reference = attachment.reference_text();
        if content.contains(&reference) {
            continue;
        }
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&reference);
    }
    for conversation in &request.conversation_references {
        let reference = conversation.reference_text();
        if content.contains(&reference) {
            continue;
        }
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&reference);
    }
    content
}

fn is_agent_tool_window_event(event: &AgentEvent) -> bool {
    matches!(
        event,
        AgentEvent::ToolCall { .. }
            | AgentEvent::ToolResult { .. }
            | AgentEvent::ToolCallStarted { .. }
            | AgentEvent::ToolCallCompleted { .. }
            | AgentEvent::TodoUpdated { .. }
            | AgentEvent::CommandStarted { .. }
            | AgentEvent::CommandOutput { .. }
            | AgentEvent::CommandExited { .. }
            | AgentEvent::FileChangeProposed { .. }
            | AgentEvent::FileChangeApplied { .. }
            | AgentEvent::FileChangeRejected { .. }
            | AgentEvent::WorkspaceEditProposed { .. }
            | AgentEvent::SubAgentStatus { .. }
    )
}

fn turn_context(
    task_id: &TaskId,
    turn_id: &str,
    request: &DesktopTurnRequest,
    goal: Option<&crate::DesktopGoalSnapshot>,
    project_id: Option<&lilia_contracts::ProjectId>,
    architecture: Option<&ProjectArchitectureGraph>,
    worktree_instructions: Option<&str>,
) -> serde_json::Value {
    let folders = request
        .workspace_path
        .as_ref()
        .filter(|path| !path.trim().is_empty())
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    json!({
        "workspace": {
            "workspaceId": format!("lilia.task:{}", task_id.as_str()),
            "folders": folders,
            "metadata": {
                "productTaskId": task_id.as_str(),
                "productProjectId": project_id.map(lilia_contracts::ProjectId::as_str),
                "source": "lilia-native-desktop",
            },
        },
        "model": request.model,
        "reasoningEffort": request.reasoning_effort,
        "permission": request.permission.as_str(),
        "planMode": request.plan_mode,
        "goalMode": request.goal_mode,
        "sessionFork": request.session_fork || request.session_branch.is_some(),
        "sessionBranch": request.session_branch,
        "automaticSelection": request.automatic_selection,
        "goal": goal,
        "projectArchitecture": architecture,
        "attachments": request.attachments,
        "conversationReferences": request.conversation_references,
        "additionalContext": worktree_instructions,
        "workflow": request.workflow,
        "turnId": turn_id,
    })
}

fn agent_wire_error(error: mutsuki_agent_contracts::AgentWireError) -> DesktopApplicationError {
    DesktopApplicationError::Agent(format!("{}: {}", error.code, error.message))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopGoalSnapshot, DesktopGoalStatus, DesktopHost,
        DesktopHostAction, DesktopHostContext, DesktopHostError, DesktopHostResult,
    };
    use lilia_contracts::{ChatAttachmentKind, ProductEntity};
    use lilia_service::ServiceAuthority;

    static NEXT_AGENT_APPLICATION_ID: AtomicU64 = AtomicU64::new(1);

    struct NoopHost;

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    fn mcp_pending() -> PendingProjection {
        PendingProjection {
            id: "pending-mcp".to_owned(),
            task_id: TaskId::new("task-mcp").unwrap(),
            agent_session: AgentSessionRef::new("session-mcp").unwrap(),
            sequence: 1,
            turn_id: Some("turn-mcp".to_owned()),
            request_id: "request-mcp".to_owned(),
            kind: "mcp_elicitation".to_owned(),
            status: PendingProjectionStatus::Open,
            prompt: Some("选择项目".to_owned()),
            action_revision: Some(1),
            payload: json!({
                "threadId": "thread-mcp",
                "turnId": "turn-mcp",
                "serverName": "linear",
                "mode": "form",
                "message": "选择项目",
                "requestedSchema": {
                    "type": "object",
                    "required": ["project"],
                    "properties": {
                        "project": {"type": "string", "enum": ["A", "B"]}
                    }
                }
            }),
        }
    }

    #[test]
    fn session_fork_replaces_the_task_binding_without_leaving_the_parent_preferred() {
        let id = NEXT_AGENT_APPLICATION_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:desktop-agent-session-fork:{id}"),
            format!("desktop-agent-session-fork:{id}"),
        )
        .unwrap();
        let task_id = TaskId::new(format!("session-fork-task-{id}")).unwrap();
        authority
            .client()
            .unwrap()
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task_id.clone(), None, "Session fork").unwrap(),
            ))
            .unwrap();
        let application = DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                "C:/lilia/native-session-fork-test",
                format!("liliacode.native-session-fork-test.{id}"),
            )
            .unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();

        application
            .persist_session_binding(&task_id, "parent-session", "profile")
            .unwrap();
        application
            .replace_session_binding(&task_id, "forked-session", "profile")
            .unwrap();

        let bindings = application
            .authority()
            .list_session_bindings(&task_id)
            .unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].agent_session.as_str(), "forked-session");
    }

    #[test]
    fn typed_workflow_can_start_without_message_content_and_reaches_turn_context() {
        let id = NEXT_AGENT_APPLICATION_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:desktop-agent-workflow:{id}"),
            format!("desktop-agent-workflow:{id}"),
        )
        .unwrap();
        let task_id = TaskId::new(format!("workflow-task-{id}")).unwrap();
        authority
            .client()
            .unwrap()
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task_id.clone(), None, "Workflow turn").unwrap(),
            ))
            .unwrap();
        let application = DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                "C:/lilia/native-workflow-test",
                format!("liliacode.native-workflow-test.{id}"),
            )
            .unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();
        let mut request = DesktopTurnRequest::new(task_id.clone(), "");
        request.workflow = Some(LiliaAgentWorkflow::LiliaCompact);

        let prepared = application.prepare_task_turn_request(request).unwrap();
        let context = turn_context(&task_id, "turn-workflow", &prepared, None, None, None, None);

        assert_eq!(context["workflow"]["type"], "lilia_compact");
    }

    #[test]
    fn mcp_interaction_response_preserves_actions_and_validates_form_content() {
        assert!(supported_pending_interaction_kind("mcp_elicitation"));
        let pending = mcp_pending();
        let accepted = normalized_pending_interaction_response(
            &pending,
            true,
            json!({"action": "accept", "content": {"project": "B"}}),
        )
        .unwrap();
        assert!(accepted.0);
        assert_eq!(accepted.1["content"]["project"], "B");
        assert!(normalized_pending_interaction_response(
            &pending,
            true,
            json!({"action": "accept", "content": {}}),
        )
        .is_err());
        assert_eq!(
            normalized_pending_interaction_response(&pending, false, json!({"action": "decline"}),)
                .unwrap(),
            (false, json!({"action": "decline"}))
        );
        assert!(normalized_pending_interaction_response(
            &pending,
            true,
            json!({"action": "cancel"}),
        )
        .is_err());
    }

    #[test]
    fn tool_consent_response_is_supported_and_decision_fenced() {
        assert!(supported_pending_interaction_kind("tool_consent"));
        assert!(!supported_pending_interaction_kind("agent_interaction"));
        let mut pending = mcp_pending();
        pending.kind = "tool_consent".to_owned();
        pending.payload = json!({
            "toolName": "shell",
            "input": {"command": "cargo test"}
        });

        let response = json!({
            "taskId": "task-1",
            "requestId": pending.request_id.clone(),
            "decision": "allow",
            "message": null,
            "updatedInput": {"command": "cargo test --locked"}
        });
        assert_eq!(
            normalized_pending_interaction_response(&pending, true, response.clone()).unwrap(),
            (true, response)
        );
        assert!(normalized_pending_interaction_response(
            &pending,
            false,
            json!({"decision": "allow"}),
        )
        .is_err());
        assert!(normalized_pending_interaction_response(
            &pending,
            false,
            json!({"decision": "deny", "updatedInput": "invalid"}),
        )
        .is_err());
    }

    #[test]
    fn native_turn_context_preserves_structured_attachments() {
        let task_id = TaskId::new("task-attachment").unwrap();
        let request = DesktopTurnRequest::new(task_id.clone(), "inspect").with_attachments(vec![
            ChatAttachment {
                id: "att-1".to_owned(),
                name: "README.md".to_owned(),
                path: "C:/repo/README.md".to_owned(),
                kind: ChatAttachmentKind::File,
                size: Some(42),
                exists: true,
                mime: None,
                directory: None,
            },
        ]);

        let context = turn_context(&task_id, "turn-1", &request, None, None, None, None);

        assert_eq!(context["attachments"][0]["id"], "att-1");
        assert_eq!(context["attachments"][0]["kind"], "file");
        assert_eq!(context["attachments"][0]["path"], "C:/repo/README.md");
    }

    #[test]
    fn conversation_references_are_structured_and_serialized_once() {
        let task_id = TaskId::new("task-reference").unwrap();
        let reference = ChatConversationReference {
            task_id: "related-task".to_owned(),
            title: "相关设计".to_owned(),
            route: "/chats/related-task".to_owned(),
            project_id: None,
            project_name: None,
        };
        let request = DesktopTurnRequest::new(task_id.clone(), "inspect")
            .with_conversation_references(vec![reference.clone()]);

        assert_eq!(
            turn_content_with_references(&request),
            "inspect\n[对话引用: 相关设计 | related-task]"
        );
        let already_referenced = DesktopTurnRequest::new(
            task_id.clone(),
            "inspect\n[对话引用: 相关设计 | related-task]",
        )
        .with_conversation_references(vec![reference]);
        assert_eq!(
            turn_content_with_references(&already_referenced),
            "inspect\n[对话引用: 相关设计 | related-task]"
        );
        let context = turn_context(&task_id, "turn-reference", &request, None, None, None, None);
        assert_eq!(
            context["conversationReferences"][0]["taskId"],
            "related-task"
        );
    }

    #[test]
    fn native_turn_context_includes_the_task_goal_snapshot() {
        let task_id = TaskId::new("task-goal").unwrap();
        let request = DesktopTurnRequest::new(task_id.clone(), "continue");
        let goal = DesktopGoalSnapshot {
            thread_id: task_id.as_str().to_owned(),
            objective: "finish Native parity".to_owned(),
            status: DesktopGoalStatus::Active,
            token_budget: Some(4_096),
            tokens_used: 512,
            time_used_seconds: 30,
            created_at: 100,
            updated_at: 200,
        };

        let context = turn_context(
            &task_id,
            "turn-goal",
            &request,
            Some(&goal),
            None,
            None,
            None,
        );

        assert_eq!(context["goal"]["objective"], "finish Native parity");
        assert_eq!(context["goal"]["status"], "active");
        assert_eq!(context["goal"]["tokenBudget"], 4_096);
    }

    #[test]
    fn attachment_references_are_appended_once_and_support_attachment_only_turns() {
        let attachment = ChatAttachment {
            id: "att-1".to_owned(),
            name: "src".to_owned(),
            path: "C:/repo/src".to_owned(),
            kind: ChatAttachmentKind::Directory,
            size: None,
            exists: true,
            mime: None,
            directory: None,
        };
        let task_id = TaskId::new("task-attachment").unwrap();
        let only_attachment =
            DesktopTurnRequest::new(task_id.clone(), "").with_attachments(vec![attachment.clone()]);
        assert_eq!(
            turn_content_with_references(&only_attachment),
            "[目录引用: src | C:/repo/src]"
        );

        let referenced = DesktopTurnRequest::new(task_id, "Inspect\n[目录引用: src | C:/repo/src]")
            .with_attachments(vec![attachment]);
        assert_eq!(
            turn_content_with_references(&referenced),
            "Inspect\n[目录引用: src | C:/repo/src]"
        );
    }

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
        assert_eq!(runtime.snapshot(&task_id).phase, "finishing");
    }

    #[test]
    fn projected_wait_restores_a_worker_owned_interaction_runtime() {
        let runtime = DesktopAgentRuntime::default();
        let task_id = TaskId::new("restored-interaction-task").unwrap();

        assert!(runtime.restore_projected_wait(
            &task_id,
            "turn-restored",
            "session-restored",
            ActiveTurnPhase::WaitingInteraction,
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
            ActiveTurnPhase::WaitingApproval,
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
            ActiveTurnPhase::WaitingApproval,
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

use std::sync::Arc;

use lilia_contracts::{
    AgentSessionBinding, AgentSessionRef, BindingId, PendingProjection, PendingProjectionStatus,
    ProductApprovalDecision, ProductEntity, ProductEntityKind, ProductRevision, ProductTask,
    TaskId,
};
#[cfg(debug_assertions)]
use mutsuki_agent_contracts::AgentToolCall;
use mutsuki_agent_contracts::{
    AgentEvent, AgentMessage, AgentSession, AgentWireError, AgentWireRequestEnvelope,
    AgentWireResponseEnvelope, InteractionResolution,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::application::agent_architecture::DesktopArchitectureInteractionPayload;
use crate::application::application::DesktopApplicationInner;
use crate::application::architecture::ArchitecturePermission;
use crate::application::hooks::DesktopHookEvent;
use crate::application::{
    AutomationCompleteAgentInput, DesktopApplication, DesktopApplicationError,
    DesktopGuideDispatchWindow, DesktopMcpElicitation, DesktopMcpElicitationAction,
    DesktopTodoGuideStatus, DesktopTurnState,
};
use crate::application::{TimelineChanged, TurnRecoveryIssue, TurnStateChanged};
use lilia_feature_agent_session::PersistedDesktopTurnState;

pub use lilia_contracts::ExecutionPermission as DesktopExecutionPermission;

use lilia_feature_agent_session::{
    accept_persisted_turn, prepare_turn_request, run_approval_resume, run_interaction_resume,
    ActiveWait, InteractionResumeSpec, TurnCancellationMode,
};
pub use lilia_feature_agent_session::{
    DesktopAgentRuntime, DesktopApprovalResponse, DesktopAutomaticTurnSelection,
    DesktopAutomationTurnCorrelation, DesktopInteractionResponse, DesktopInterruptResult,
    DesktopSessionBranchAnchor, DesktopSessionBranchMode, DesktopTaskRuntimeSnapshot,
    DesktopTurnDispatch, DesktopTurnDispatchKind, DesktopTurnRequest, APPROVAL_PROTOCOL,
    INTERACTION_PROTOCOL, TURN_PROTOCOL,
};
#[cfg(debug_assertions)]
pub use lilia_feature_agent_session::{
    DesktopDurableTurnDebugSnapshot, DesktopQuarantinedTurnDebugSnapshot,
};

pub use crate::application::agent_architecture::{
    DesktopArchitectureInteractionDecision, DesktopArchitectureInteractionResponse,
};
pub(crate) use crate::application::agent_turn_host::turn_context;
pub(crate) use lilia_feature_agent_session::supported_pending_interaction_kind;

/// Submits claimed turns, approval decisions and interaction resolutions.
/// The desktop host installs a kernel-backed executor; nothing here holds Jobs.
pub trait DesktopTurnExecutor: Send + Sync + 'static {
    fn execute_turn(&self, task_id: TaskId, turn_id: String) -> Result<(), String>;
    fn execute_approval(
        &self,
        task_id: TaskId,
        decision: ProductApprovalDecision,
    ) -> Result<(), String>;
    fn execute_interaction(
        &self,
        task_id: TaskId,
        resolution: InteractionResolution,
    ) -> Result<(), String>;
}

/// Runs turn workers on the caller thread. Used only when the host has not
/// installed a queue executor (tests). Does not create a job runtime.
struct InlineTurnExecutor {
    inner: std::sync::Weak<DesktopApplicationInner>,
}

impl InlineTurnExecutor {
    fn application(&self) -> Result<DesktopApplication, String> {
        self.inner
            .upgrade()
            .map(|inner| DesktopApplication { inner })
            .ok_or_else(|| "desktop application has shut down".to_owned())
    }
}

impl DesktopTurnExecutor for InlineTurnExecutor {
    fn execute_turn(&self, task_id: TaskId, turn_id: String) -> Result<(), String> {
        self.application()?.execute_turn_job(task_id, turn_id);
        Ok(())
    }

    fn execute_approval(
        &self,
        task_id: TaskId,
        decision: ProductApprovalDecision,
    ) -> Result<(), String> {
        self.application()?.execute_approval_job(task_id, decision);
        Ok(())
    }

    fn execute_interaction(
        &self,
        task_id: TaskId,
        resolution: InteractionResolution,
    ) -> Result<(), String> {
        self.application()?
            .execute_interaction_job(task_id, resolution);
        Ok(())
    }
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
    /// Installed once by the host after the kernel exists. Hosts that skip this
    /// run workers inline so tests still submit turns without holding Jobs.
    pub fn install_turn_executor(
        &self,
        executor: Arc<dyn DesktopTurnExecutor>,
    ) -> Result<(), DesktopApplicationError> {
        self.inner
            .turn_executor
            .set(executor)
            .map_err(|_| DesktopApplicationError::InvalidInput {
                field: "turnExecutor",
                message: "turn executor is already installed".to_owned(),
            })
    }

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
        let wait = if pending.kind == "permission_approval" {
            ActiveWait::Approval
        } else {
            ActiveWait::Interaction
        };
        self.inner.agent.restore_projected_wait(
            task_id,
            turn_id,
            pending.agent_session.as_str(),
            wait,
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
        self.emit_event(TimelineChanged {
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
                    events_application.emit_event(TimelineChanged {
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
                    events_application.emit_event(TimelineChanged {
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
        request: DesktopTurnRequest,
    ) -> Result<DesktopTurnRequest, DesktopApplicationError> {
        prepare_turn_request(request, self).map_err(Into::into)
    }

    pub(crate) fn accept_persisted_task_turn(
        &self,
        request: DesktopTurnRequest,
        turn_id: String,
        guide_already_queued: bool,
    ) -> Result<(DesktopTurnDispatch, bool), DesktopApplicationError> {
        accept_persisted_turn(
            &self.inner.agent,
            self,
            request,
            turn_id,
            guide_already_queued,
        )
        .map_err(Into::into)
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
                    self.emit_event(TimelineChanged {
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
                self.emit_event(TurnStateChanged {
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
            self.emit_event(TurnRecoveryIssue {
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
                    self.emit_event(TurnStateChanged {
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
        self.emit_event(TimelineChanged {
            task_id: task_id.clone(),
            cursor: None,
        });
        if cancel.wait.is_some() {
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
        self.emit_event(TimelineChanged {
            task_id: task_id.clone(),
            cursor: None,
        });
        if cancel.wait.is_some() {
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
        self.emit_event(TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.clone(),
            state: DesktopTurnState::ResolvingApproval,
        });
        self.submit_approval_job(task_id.clone(), decision)?;
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
        self.emit_event(TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.clone(),
            state: DesktopTurnState::ResolvingInteraction,
        });
        self.submit_interaction_job(task_id.clone(), resolution)?;
        Ok(DesktopInteractionResponse {
            turn_id,
            request_id: request_id.to_owned(),
            accepted,
            continuation: None,
        })
    }

    pub(crate) fn workspace_path_for_task(
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
                self.emit_event(TurnStateChanged {
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
        let outcome = {
            let mut pending_turns = self
                .inner
                .pending_turns
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("pending turns"))?;
            lilia_feature_agent_session::claim_turn_for_worker(
                &mut pending_turns,
                &self.inner.agent,
                &task_id,
                &turn_id,
                &self.inner.turn_claim_epoch,
            )?
        };
        match outcome {
            None => {
                return Err(DesktopApplicationError::Agent(format!(
                    "Native Agent turn `{turn_id}` is not the first durably queued turn"
                )));
            }
            Some(lilia_feature_agent_session::ClaimWorkerOutcome::AlreadyOwned) => return Ok(()),
            Some(lilia_feature_agent_session::ClaimWorkerOutcome::Submit { .. }) => {}
        }
        drop(submission);
        self.submit_claimed_turn(task_id, turn_id)
    }

    fn submit_claimed_turn(
        &self,
        task_id: TaskId,
        turn_id: String,
    ) -> Result<(), DesktopApplicationError> {
        self.emit_event(TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.clone(),
            state: DesktopTurnState::Starting,
        });
        self.turn_executor()?
            .execute_turn(task_id.clone(), turn_id.clone())
            .map_err(|error| {
                self.finish_turn(
                    task_id,
                    turn_id,
                    DesktopTurnState::Failed {
                        message: error.clone(),
                    },
                );
                DesktopApplicationError::Agent(format!("start Native Agent turn: {error}"))
            })
    }

    pub fn execute_turn_job(&self, task_id: TaskId, turn_id: String) {
        self.run_turn_worker(task_id, turn_id);
    }

    pub fn execute_approval_job(&self, task_id: TaskId, decision: ProductApprovalDecision) {
        self.run_approval_worker(task_id, decision);
    }

    pub fn execute_interaction_job(&self, task_id: TaskId, resolution: InteractionResolution) {
        self.run_interaction_worker(task_id, resolution);
    }

    fn turn_executor(&self) -> Result<Arc<dyn DesktopTurnExecutor>, DesktopApplicationError> {
        if let Some(executor) = self.inner.turn_executor.get() {
            return Ok(Arc::clone(executor));
        }
        let executor: Arc<dyn DesktopTurnExecutor> = Arc::new(InlineTurnExecutor {
            inner: Arc::downgrade(&self.inner),
        });
        match self.inner.turn_executor.set(Arc::clone(&executor)) {
            Ok(()) => Ok(executor),
            Err(_) => self
                .inner
                .turn_executor
                .get()
                .map(Arc::clone)
                .ok_or_else(|| DesktopApplicationError::Agent("turn executor was dropped".into())),
        }
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
                                lilia_feature_agent_session::DesktopTurnQueueError::InvalidTransition {
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
            (Some(durable), Some(memory)) if durable.turn_id == memory.turn_id => Some(durable),
            (None, None) => None,
            (durable, memory) => {
                return Err(DesktopApplicationError::Agent(format!(
                    "Native Agent prepared-turn recovery diverged: durable={:?}, memory={:?}",
                    durable.as_ref().map(|turn| turn.turn_id.as_str()),
                    memory.as_ref().map(|turn| turn.turn_id.as_str())
                )));
            }
        };
        self.emit_event(TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.clone(),
            state: terminal_state,
        });
        let ready_next =
            claimed_next.and_then(
                |claimed| match lilia_feature_agent_session::accept_claimed_worker(
                    &self.inner.agent,
                    &task_id,
                    claimed,
                ) {
                    Ok(next) => next,
                    Err(error) => {
                        eprintln!(
                            "claimed Native Agent turn could not bind to its memory owner: {error}"
                        );
                        None
                    }
                },
            );
        drop(submission);
        if let Some(next_turn_id) = ready_next {
            self.submit_claimed_turn(task_id, next_turn_id)?;
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
        lilia_feature_agent_session::run_prepared_turn(&self.inner.agent, self, task_id, turn_id)
            .map_err(DesktopApplicationError::from)
    }

    pub(crate) fn run_context_compaction_turn(
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
        self.emit_event(TurnStateChanged {
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

    fn submit_approval_job(
        &self,
        task_id: TaskId,
        decision: ProductApprovalDecision,
    ) -> Result<(), DesktopApplicationError> {
        let turn_id = decision.turn_id.clone();
        self.turn_executor()?
            .execute_approval(task_id.clone(), decision)
            .map_err(|error| {
                self.inner
                    .agent
                    .restore_waiting_approval(&task_id, &turn_id);
                DesktopApplicationError::Agent(format!("start Native approval response: {error}"))
            })
    }

    fn run_approval_worker(&self, task_id: TaskId, decision: ProductApprovalDecision) {
        run_approval_resume(&self.inner.agent, self, task_id, decision);
    }

    fn submit_interaction_job(
        &self,
        task_id: TaskId,
        resolution: InteractionResolution,
    ) -> Result<(), DesktopApplicationError> {
        let turn_id = resolution.turn_id.clone();
        self.turn_executor()?
            .execute_interaction(task_id.clone(), resolution)
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
        run_interaction_resume(
            &self.inner.agent,
            self,
            task_id,
            InteractionResumeSpec {
                session_id: resolution.session_id,
                turn_id: resolution.turn_id,
                version: resolution.version,
                interaction_id: resolution.interaction_id,
                accepted: resolution.accepted,
                response: resolution.response,
            },
        );
    }

    pub(crate) fn finish_turn(&self, task_id: TaskId, turn_id: String, state: DesktopTurnState) {
        let Some(active) = self.inner.agent.begin_finish(&task_id, &turn_id) else {
            return;
        };
        let cancellation_mode = active.cancellation_mode;
        let claim_token = active.claim_token.clone();
        let hook_workspace_path = active.request.workspace_path.clone();
        let automation = active.request.automation;
        self.emit_event(TurnStateChanged {
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
                self.emit_event(TurnStateChanged {
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
            (Some(durable), Some(memory)) if durable.turn_id == memory.turn_id => Some(durable),
            (None, None) => None,
            _ => None,
        };
        let had_next = claimed_next.is_some();
        let ready_next =
            claimed_next.and_then(
                |claimed| match lilia_feature_agent_session::accept_claimed_worker(
                    &self.inner.agent,
                    &task_id,
                    claimed,
                ) {
                    Ok(next) => next,
                    Err(error) => {
                        eprintln!(
                            "claimed Native Agent turn could not bind to its memory owner: {error}"
                        );
                        None
                    }
                },
            );
        drop(submission);
        if let Some(next_turn_id) = ready_next {
            if let Err(error) = self.submit_claimed_turn(task_id, next_turn_id) {
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

    pub(crate) fn persist_session_binding(
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

pub(crate) fn turn_content_with_references(request: &DesktopTurnRequest) -> String {
    request.content_with_references()
}

pub(crate) fn agent_wire_error(error: AgentWireError) -> DesktopApplicationError {
    DesktopApplicationError::Agent(format!("{}: {}", error.code, error.message))
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;

//! Host port adapters for turn start, page handling and AgentKit I/O.

use lilia_contracts::{ExecutionPermission, PendingProjection, ProductApprovalDecision, TaskId};
use mutsuki_agent_contracts::{AgentEvent, AgentMessage, InteractionResolution};
use serde_json::json;

use crate::application::hooks::DesktopHookEvent;
use crate::application::{
    ApprovalChanged, DesktopApplication, DesktopApplicationError, DesktopApprovalState,
    DesktopGuideDispatchWindow, DesktopInteractionState, DesktopTodoGuideStatus, DesktopTurnState,
    InteractionChanged, ProjectArchitectureGraph, TimelineChanged, TurnStateChanged,
};
use lilia_feature_agent_session::{
    AgentTurnError, AgentTurnHost, DesktopAutoTurnDecisionSettings, DesktopTurnRequest,
    InteractionResumeSpec, ObservedTurnOutcome, TurnFinishKind, TurnPageHost, TurnResumeHost,
    TurnStartHost, TurnSubmitSpec,
};

impl From<AgentTurnError> for DesktopApplicationError {
    fn from(error: AgentTurnError) -> Self {
        match error {
            AgentTurnError::NoActiveTurn(task_id) => Self::NoActiveTurn(task_id),
            AgentTurnError::StateUnavailable(state) => Self::StateUnavailable(state),
            AgentTurnError::Agent(message) => Self::Agent(message),
            AgentTurnError::InvalidInput { field, message } => {
                Self::InvalidInput { field, message }
            }
            AgentTurnError::Queue(error) => Self::TurnQueue(error),
            AgentTurnError::Product(error) => Self::Product(error),
        }
    }
}

pub(crate) fn agent_turn_error(error: DesktopApplicationError) -> AgentTurnError {
    match error {
        DesktopApplicationError::NoActiveTurn(task_id) => AgentTurnError::NoActiveTurn(task_id),
        DesktopApplicationError::StateUnavailable(state) => AgentTurnError::StateUnavailable(state),
        DesktopApplicationError::Agent(message) => AgentTurnError::Agent(message),
        DesktopApplicationError::InvalidInput { field, message } => {
            AgentTurnError::InvalidInput { field, message }
        }
        DesktopApplicationError::TurnQueue(error) => AgentTurnError::Queue(error),
        DesktopApplicationError::Product(error) => AgentTurnError::Product(error),
        other => AgentTurnError::Agent(other.to_string()),
    }
}

impl TurnStartHost for DesktopApplication {
    fn ensure_runnable(&self, task_id: &TaskId) -> Result<(), AgentTurnError> {
        self.get_task(task_id).map_err(agent_turn_error)?;
        self.ensure_task_runnable(task_id).map_err(agent_turn_error)
    }

    fn workspace_path(&self, task_id: &TaskId) -> Result<Option<String>, AgentTurnError> {
        let task = self.get_task(task_id).map_err(agent_turn_error)?;
        self.workspace_path_for_task(&task)
            .map_err(agent_turn_error)
    }

    fn auto_turn_settings(&self) -> Result<DesktopAutoTurnDecisionSettings, AgentTurnError> {
        self.agent_interaction_settings()
            .map(|settings| settings.auto_turn_decision)
            .map_err(agent_turn_error)
    }

    fn mark_guide_queued(&self, guide_id: &str) -> Result<(), AgentTurnError> {
        self.set_task_guide_status(guide_id, DesktopTodoGuideStatus::Queued)
            .map(|_| ())
            .map_err(agent_turn_error)
    }

    fn abort_prepared(&self, task_id: &TaskId, turn_id: &str) {
        self.inner.agent.abort_prepared(task_id, turn_id);
    }

    fn emit_queued(&self, task_id: &TaskId, turn_id: &str, position: usize) {
        self.emit_event(TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.to_owned(),
            state: DesktopTurnState::Queued { position },
        });
    }
}

impl TurnPageHost for DesktopApplication {
    fn pending_projections(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<PendingProjection>, AgentTurnError> {
        Ok(self
            .task_session_snapshot(task_id)
            .map_err(agent_turn_error)?
            .pending)
    }

    fn emit_waiting_approval(&self, task_id: &TaskId, turn_id: &str, request_id: Option<String>) {
        self.emit_event(TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.to_owned(),
            state: DesktopTurnState::WaitingApproval {
                request_id,
                error: None,
            },
        });
    }

    fn emit_waiting_interaction(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        request_id: Option<String>,
        kind: Option<String>,
        error: Option<String>,
    ) {
        self.emit_event(TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.to_owned(),
            state: DesktopTurnState::WaitingInteraction {
                request_id,
                kind,
                error,
            },
        });
    }

    fn dispatch_user_guide(&self, task_id: &TaskId) {
        if let Err(error) = self.dispatch_next_task_guide(task_id, DesktopGuideDispatchWindow::User)
        {
            eprintln!("failed to dispatch Native user-window Guide: {error}");
        }
    }

    fn turn_permission(&self, task_id: &TaskId, turn_id: &str) -> Option<ExecutionPermission> {
        self.inner
            .agent
            .request(task_id, turn_id)
            .map(|request| request.permission)
    }

    fn respond_architecture(
        &self,
        task_id: &TaskId,
        request_id: &str,
        allow: bool,
    ) -> Result<(), AgentTurnError> {
        let decision = if allow {
            crate::application::DesktopArchitectureInteractionDecision::Allow
        } else {
            crate::application::DesktopArchitectureInteractionDecision::Deny
        };
        self.respond_task_architecture_interaction(task_id, request_id, decision)
            .map(|_| ())
            .map_err(agent_turn_error)
    }

    fn finish_turn(
        &self,
        task_id: TaskId,
        turn_id: String,
        kind: TurnFinishKind,
        message: Option<String>,
    ) {
        let state = match kind {
            TurnFinishKind::Cancelled => DesktopTurnState::Cancelled,
            TurnFinishKind::Completed => DesktopTurnState::Completed,
            TurnFinishKind::Failed => DesktopTurnState::Failed {
                message: message.unwrap_or_else(|| "Native Agent turn failed".to_owned()),
            },
        };
        DesktopApplication::finish_turn(self, task_id, turn_id, state);
    }

    fn request_title_update(&self, task_id: TaskId, turn_id: String) {
        self.request_title_update_after_turn(task_id, Some(turn_id));
    }
}

impl TurnResumeHost for DesktopApplication {
    fn respond_approval_observed(
        &self,
        task_id: &TaskId,
        decision: ProductApprovalDecision,
    ) -> Result<ObservedTurnOutcome, AgentTurnError> {
        let events_application = self.clone();
        let events_task_id = task_id.clone();
        let page = self
            .authority()
            .respond_agent_task_approval_observed(decision, move |events| {
                events_application.emit_event(TimelineChanged {
                    task_id: events_task_id.clone(),
                    cursor: events.last().map(|event| event.sequence),
                });
            })
            .map_err(|error| {
                agent_turn_error(crate::application::agent::agent_wire_error(error))
            })?;
        Ok(ObservedTurnOutcome {
            session_id: page.session_id,
            waiting_approval: page.waiting_approval,
            waiting_interaction: page.waiting_interaction,
            completed: page.completed,
            cancelled_by_user: page.cancelled,
        })
    }

    fn respond_interaction_observed(
        &self,
        task_id: &TaskId,
        spec: InteractionResumeSpec,
    ) -> Result<ObservedTurnOutcome, AgentTurnError> {
        let events_application = self.clone();
        let events_task_id = task_id.clone();
        let page = self
            .authority()
            .respond_agent_task_interaction_observed(
                InteractionResolution {
                    session_id: spec.session_id,
                    turn_id: spec.turn_id,
                    version: spec.version,
                    interaction_id: spec.interaction_id,
                    accepted: spec.accepted,
                    response: spec.response,
                },
                move |events| {
                    events_application.emit_event(TimelineChanged {
                        task_id: events_task_id.clone(),
                        cursor: events.last().map(|event| event.sequence),
                    });
                },
            )
            .map_err(|error| {
                agent_turn_error(crate::application::agent::agent_wire_error(error))
            })?;
        Ok(ObservedTurnOutcome {
            session_id: page.session_id,
            waiting_approval: page.waiting_approval,
            waiting_interaction: page.waiting_interaction,
            completed: page.completed,
            cancelled_by_user: page.cancelled,
        })
    }

    fn emit_approval_changed(&self, task_id: &TaskId, request_id: &str, approved: bool) {
        self.emit_event(ApprovalChanged {
            task_id: task_id.clone(),
            request_id: request_id.to_owned(),
            state: if approved {
                DesktopApprovalState::Approved
            } else {
                DesktopApprovalState::Denied
            },
        });
    }

    fn emit_interaction_changed(&self, task_id: &TaskId, request_id: &str, accepted: bool) {
        self.emit_event(InteractionChanged {
            task_id: task_id.clone(),
            request_id: request_id.to_owned(),
            state: if accepted {
                DesktopInteractionState::Accepted
            } else {
                DesktopInteractionState::Declined
            },
        });
    }

    fn restore_waiting_approval(&self, task_id: &TaskId, turn_id: &str) {
        self.inner.agent.restore_waiting_approval(task_id, turn_id);
    }

    fn restore_waiting_interaction(&self, task_id: &TaskId, turn_id: &str) {
        self.inner
            .agent
            .restore_waiting_interaction(task_id, turn_id);
    }

    fn emit_waiting_approval_error(
        &self,
        task_id: TaskId,
        turn_id: String,
        request_id: String,
        error: String,
    ) {
        self.emit_event(TurnStateChanged {
            task_id,
            turn_id,
            state: DesktopTurnState::WaitingApproval {
                request_id: Some(request_id),
                error: Some(error),
            },
        });
    }

    fn emit_waiting_interaction_error(
        &self,
        task_id: TaskId,
        turn_id: String,
        request_id: String,
        error: String,
    ) {
        self.emit_event(TurnStateChanged {
            task_id,
            turn_id,
            state: DesktopTurnState::WaitingInteraction {
                request_id: Some(request_id),
                kind: None,
                error: Some(error),
            },
        });
    }
}

impl AgentTurnHost for DesktopApplication {
    fn apply_automatic_selection(
        &self,
        request: DesktopTurnRequest,
    ) -> Result<DesktopTurnRequest, AgentTurnError> {
        self.apply_automatic_turn_selection(request)
            .map_err(agent_turn_error)
    }

    fn persist_request(
        &self,
        turn_id: &str,
        request: &DesktopTurnRequest,
    ) -> Result<(), AgentTurnError> {
        self.inner
            .pending_turns
            .lock()
            .map_err(|_| AgentTurnError::StateUnavailable("pending turns"))?
            .update_request(turn_id, request)
            .map_err(AgentTurnError::from)
    }

    fn mark_guide_sent(&self, guide_id: &str) -> Result<(), AgentTurnError> {
        self.set_task_guide_status(guide_id, DesktopTodoGuideStatus::Sent)
            .map(|_| ())
            .map_err(agent_turn_error)
    }

    fn run_compaction(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        request: &DesktopTurnRequest,
    ) -> Result<(), AgentTurnError> {
        self.run_context_compaction_turn(task_id, turn_id, request)
            .map_err(agent_turn_error)
    }

    fn load_task(
        &self,
        task_id: &TaskId,
    ) -> Result<(String, Option<lilia_contracts::ProjectId>), AgentTurnError> {
        let task = self.get_task(task_id).map_err(agent_turn_error)?;
        Ok((task.title, task.project_id))
    }

    fn refresh_profile(&self) -> Result<String, AgentTurnError> {
        self.authority()
            .shared_runtime()
            .inner()
            .refresh_product_profile(None)
            .map(|profile| profile.profile_id)
            .map_err(|error| AgentTurnError::Agent(error.to_string()))
    }

    fn existing_session(&self, task_id: &TaskId) -> Result<Option<String>, AgentTurnError> {
        Ok(self
            .authority()
            .list_session_bindings(task_id)
            .map_err(|error| agent_turn_error(error.into()))?
            .into_iter()
            .next()
            .map(|binding| binding.agent_session.as_str().to_owned()))
    }

    fn fork_through_turn(
        &self,
        source: &str,
        target: &str,
        source_turn_id: &str,
    ) -> Result<String, AgentTurnError> {
        self.authority()
            .fork_agent_task_session_through_turn(source, target, source_turn_id)
            .map(|session| session.session_id)
            .map_err(|error| agent_turn_error(crate::application::agent::agent_wire_error(error)))
    }

    fn fork_session(&self, source: &str, target: &str) -> Result<String, AgentTurnError> {
        self.authority()
            .fork_agent_task_session(source, target)
            .map(|session| session.session_id)
            .map_err(|error| agent_turn_error(crate::application::agent::agent_wire_error(error)))
    }

    fn open_session(
        &self,
        task_id: &TaskId,
        existing: Option<&str>,
        profile_id: &str,
        title: Option<&str>,
    ) -> Result<String, AgentTurnError> {
        self.authority()
            .open_agent_task_session(task_id, existing, profile_id, title.map(str::to_owned))
            .map(|session| session.session_id)
            .map_err(|error| agent_turn_error(crate::application::agent::agent_wire_error(error)))
    }

    fn persist_binding(
        &self,
        task_id: &TaskId,
        session_id: &str,
        profile_id: &str,
        replace: bool,
    ) -> Result<(), AgentTurnError> {
        if replace {
            self.replace_session_binding(task_id, session_id, profile_id)
        } else {
            self.persist_session_binding(task_id, session_id, profile_id)
        }
        .map(|_| ())
        .map_err(agent_turn_error)
    }

    fn cancel_session_turn(&self, session_id: &str, turn_id: &str) -> Result<(), AgentTurnError> {
        self.authority()
            .shared_runtime()
            .inner()
            .cancel_session_turn(session_id, turn_id)
            .map(|_| ())
            .map_err(|error| AgentTurnError::Agent(error.to_string()))
    }

    fn emit_running(&self, task_id: &TaskId, turn_id: &str) {
        self.emit_event(TurnStateChanged {
            task_id: task_id.clone(),
            turn_id: turn_id.to_owned(),
            state: DesktopTurnState::Running,
        });
    }

    fn execute_prompt_hooks(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        workspace: Option<&str>,
        content: &str,
    ) -> Result<(), AgentTurnError> {
        self.execute_turn_hooks(
            DesktopHookEvent::UserPromptSubmit,
            task_id,
            turn_id,
            workspace,
            content,
        )
        .map_err(|error| agent_turn_error(error.into()))
    }

    fn submit_observed(&self, spec: TurnSubmitSpec) -> Result<ObservedTurnOutcome, AgentTurnError> {
        let task = self.get_task(&spec.task_id).map_err(agent_turn_error)?;
        let mut message = AgentMessage::user(&spec.request.content);
        let goal = self.task_goal(&spec.task_id).map_err(agent_turn_error)?;
        let architecture = task
            .project_id
            .as_ref()
            .map(|project_id| self.project_architecture(project_id))
            .transpose()
            .map_err(agent_turn_error)?;
        let worktree_instructions = self
            .worktree_auto_instructions_for_task(&spec.task_id)
            .map_err(agent_turn_error)?;
        message.metadata = Some(turn_context(
            &spec.task_id,
            &spec.turn_id,
            &spec.request,
            goal.as_ref(),
            task.project_id.as_ref(),
            architecture.as_ref(),
            worktree_instructions.as_deref(),
        ));
        let events_application = self.clone();
        let events_task_id = spec.task_id.clone();
        let page = self
            .authority()
            .submit_agent_task_turn_observed(
                &spec.session_id,
                &spec.turn_id,
                vec![message],
                &format!("native-desktop:{}:{}", spec.task_id.as_str(), spec.turn_id),
                move |events| {
                    let has_tool_window = events
                        .iter()
                        .any(|event| is_agent_tool_window_event(&event.event));
                    events_application.emit_event(TimelineChanged {
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
            .map_err(|error| {
                agent_turn_error(crate::application::agent::agent_wire_error(error))
            })?;
        Ok(ObservedTurnOutcome {
            session_id: page.session_id,
            waiting_approval: page.waiting_approval,
            waiting_interaction: page.waiting_interaction,
            completed: page.completed,
            cancelled_by_user: page.cancelled,
        })
    }
}

pub(crate) fn turn_context(
    task_id: &TaskId,
    turn_id: &str,
    request: &DesktopTurnRequest,
    goal: Option<&crate::application::DesktopGoalSnapshot>,
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

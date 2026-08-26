//! Prepared-turn execution sequence.
//!
//! AgentKit, Jobs and product I/O stay behind [`AgentTurnHost`]. This module
//! owns the order: auto-select, guide, compaction, session bind/fork, hooks,
//! observed submit and page hand-off.

use lilia_contracts::{LiliaAgentWorkflow, ProjectId, TaskId};
use thiserror::Error;

use crate::runtime::DesktopAgentRuntime;
use crate::turn::{DesktopSessionBranchMode, DesktopTurnRequest};
use crate::turn_page::{handle_observed_page, TurnPageHost};
use crate::DesktopTurnQueueError;

#[derive(Debug, Error)]
pub enum AgentTurnError {
    #[error("task `{0}` has no active Native Agent turn")]
    NoActiveTurn(TaskId),
    #[error("desktop {0} state is unavailable")]
    StateUnavailable(&'static str),
    #[error("{0}")]
    Agent(String),
    #[error("invalid desktop input `{field}`: {message}")]
    InvalidInput {
        field: &'static str,
        message: String,
    },
    #[error(transparent)]
    Queue(#[from] DesktopTurnQueueError),
    #[error(transparent)]
    Product(#[from] lilia_contracts::ProductError),
}

#[derive(Clone, Debug)]
pub struct ObservedTurnOutcome {
    pub session_id: String,
    pub session_version: u64,
    pub waiting_approval: bool,
    pub waiting_interaction: bool,
    pub completed: bool,
    pub cancelled_by_user: bool,
}

#[derive(Clone, Debug)]
pub struct TurnSubmitSpec {
    pub task_id: TaskId,
    pub turn_id: String,
    pub session_id: String,
    pub request: DesktopTurnRequest,
}

/// Host I/O for one prepared turn. Does not hold Jobs.
pub trait AgentTurnHost: TurnPageHost {
    fn apply_automatic_selection(
        &self,
        request: DesktopTurnRequest,
    ) -> Result<DesktopTurnRequest, AgentTurnError>;
    fn persist_request(
        &self,
        turn_id: &str,
        request: &DesktopTurnRequest,
    ) -> Result<(), AgentTurnError>;
    fn mark_guide_sent(&self, guide_id: &str) -> Result<(), AgentTurnError>;
    fn run_compaction(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        request: &DesktopTurnRequest,
    ) -> Result<(), AgentTurnError>;
    fn load_task(&self, task_id: &TaskId) -> Result<(String, Option<ProjectId>), AgentTurnError>;
    fn refresh_profile(&self) -> Result<String, AgentTurnError>;
    fn existing_session(&self, task_id: &TaskId) -> Result<Option<String>, AgentTurnError>;
    fn fork_through_turn(
        &self,
        source: &str,
        target: &str,
        source_turn_id: &str,
    ) -> Result<String, AgentTurnError>;
    fn fork_session(&self, source: &str, target: &str) -> Result<String, AgentTurnError>;
    fn open_session(
        &self,
        task_id: &TaskId,
        existing: Option<&str>,
        profile_id: &str,
        title: Option<&str>,
    ) -> Result<String, AgentTurnError>;
    fn persist_binding(
        &self,
        task_id: &TaskId,
        session_id: &str,
        profile_id: &str,
        replace: bool,
    ) -> Result<(), AgentTurnError>;
    fn cancel_session_turn(&self, session_id: &str, turn_id: &str) -> Result<(), AgentTurnError>;
    fn emit_running(&self, task_id: &TaskId, turn_id: &str);
    fn execute_prompt_hooks(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        workspace: Option<&str>,
        content: &str,
    ) -> Result<(), AgentTurnError>;
    fn submit_observed(&self, spec: TurnSubmitSpec) -> Result<ObservedTurnOutcome, AgentTurnError>;
}

pub fn run_prepared_turn(
    runtime: &DesktopAgentRuntime,
    host: &dyn AgentTurnHost,
    task_id: &TaskId,
    turn_id: &str,
) -> Result<(), AgentTurnError> {
    let mut active = runtime
        .active(task_id, turn_id)
        .ok_or_else(|| AgentTurnError::NoActiveTurn(task_id.clone()))?;
    let prepared_request = host.apply_automatic_selection(active.request.clone())?;
    if prepared_request != active.request {
        host.persist_request(turn_id, &prepared_request)?;
        if !runtime.replace_active_request(task_id, turn_id, prepared_request.clone()) {
            return Err(AgentTurnError::NoActiveTurn(task_id.clone()));
        }
        active.request = prepared_request;
    }
    if let Some(guide_id) = active.request.guide_id.as_deref() {
        host.mark_guide_sent(guide_id)?;
    }
    if matches!(
        active.request.workflow.as_ref(),
        Some(LiliaAgentWorkflow::LiliaCompact)
    ) {
        return host.run_compaction(task_id, turn_id, &active.request);
    }
    let (title, _project_id) = host.load_task(task_id)?;
    let profile_id = host.refresh_profile()?;
    let existing_binding = host.existing_session(task_id)?;
    let session_id = if let Some(branch) = active.request.session_branch.as_ref() {
        let source = existing_binding
            .as_ref()
            .ok_or_else(|| AgentTurnError::InvalidInput {
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
        host.fork_through_turn(source, &target_session_id, &branch.source_turn_id)?
    } else if active.request.session_fork {
        if let Some(source) = existing_binding.as_ref() {
            let target_session_id =
                format!("native-{}-fork-{}", task_id.as_str(), uuid::Uuid::new_v4());
            host.fork_session(source, &target_session_id)?
        } else {
            host.open_session(task_id, None, &profile_id, Some(&title))?
        }
    } else {
        host.open_session(
            task_id,
            existing_binding.as_deref(),
            &profile_id,
            Some(&title),
        )?
    };
    if active.request.session_fork || active.request.session_branch.is_some() {
        host.persist_binding(task_id, &session_id, &profile_id, true)?;
    } else {
        host.persist_binding(task_id, &session_id, &profile_id, false)?;
    }
    let cancel_requested = runtime.attach_session(task_id, turn_id, session_id.clone());
    if cancel_requested || active.cancellation_mode.is_some() {
        host.cancel_session_turn(&session_id, turn_id)?;
    }
    host.emit_running(task_id, turn_id);
    host.execute_prompt_hooks(
        task_id,
        turn_id,
        active.request.workspace_path.as_deref(),
        &active.request.content,
    )?;
    let page = host.submit_observed(TurnSubmitSpec {
        task_id: task_id.clone(),
        turn_id: turn_id.to_owned(),
        session_id,
        request: active.request,
    })?;
    handle_observed_page(runtime, host, task_id, turn_id, page)
}

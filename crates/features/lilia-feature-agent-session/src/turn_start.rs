//! Prepare and accept a persisted turn before the host claim fence starts it.

use lilia_contracts::TaskId;

use crate::runtime::DesktopAgentRuntime;
use crate::settings::DesktopAutoTurnDecisionSettings;
use crate::turn::{DesktopTurnDispatch, DesktopTurnDispatchKind, DesktopTurnRequest};
use crate::turn_run::AgentTurnError;

pub trait TurnStartHost {
    fn ensure_runnable(&self, task_id: &TaskId) -> Result<(), AgentTurnError>;
    fn workspace_path(&self, task_id: &TaskId) -> Result<Option<String>, AgentTurnError>;
    fn auto_turn_settings(&self) -> Result<DesktopAutoTurnDecisionSettings, AgentTurnError>;
    fn mark_guide_queued(&self, guide_id: &str) -> Result<(), AgentTurnError>;
    fn abort_prepared(&self, task_id: &TaskId, turn_id: &str);
    fn emit_queued(&self, task_id: &TaskId, turn_id: &str, position: usize);
}

pub fn prepare_turn_request(
    mut request: DesktopTurnRequest,
    host: &dyn TurnStartHost,
) -> Result<DesktopTurnRequest, AgentTurnError> {
    request.content = request.content_with_references();
    if let Some(branch) = request.session_branch.as_mut() {
        branch.source_turn_id = branch.source_turn_id.trim().to_owned();
        if branch.source_turn_id.is_empty() {
            return Err(AgentTurnError::InvalidInput {
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
        return Err(AgentTurnError::InvalidInput {
            field: "content",
            message: "message content and attachments must not both be empty".to_owned(),
        });
    }
    host.ensure_runnable(&request.task_id)?;
    if request.workspace_path.is_none() {
        request.workspace_path = host.workspace_path(&request.task_id)?;
    }
    if request.allow_auto_turn_decision && request.auto_turn_settings.is_none() {
        request.auto_turn_settings = Some(host.auto_turn_settings()?);
    } else if !request.allow_auto_turn_decision {
        request.auto_turn_decision_applied = true;
    }
    Ok(request)
}

pub fn accept_persisted_turn(
    runtime: &DesktopAgentRuntime,
    host: &dyn TurnStartHost,
    request: DesktopTurnRequest,
    turn_id: String,
    guide_already_queued: bool,
) -> Result<(DesktopTurnDispatch, bool), AgentTurnError> {
    let task_id = request.task_id.clone();
    let (dispatch, should_start, inserted) =
        runtime.enqueue_idempotent(request.clone(), turn_id.clone());
    if !inserted {
        return Err(AgentTurnError::Agent(format!(
            "Native Agent turn id `{turn_id}` is already active"
        )));
    }
    if !guide_already_queued && matches!(dispatch.kind, DesktopTurnDispatchKind::Queued { .. }) {
        if let Some(guide_id) = request.guide_id.as_deref() {
            if let Err(error) = host.mark_guide_queued(guide_id) {
                host.abort_prepared(&task_id, &dispatch.turn_id);
                return Err(error);
            }
        }
    }
    if let DesktopTurnDispatchKind::Queued { position } = dispatch.kind {
        host.emit_queued(&task_id, &dispatch.turn_id, position);
    }
    Ok((dispatch, should_start))
}

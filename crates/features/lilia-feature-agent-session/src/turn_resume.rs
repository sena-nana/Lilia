//! Approval and interaction worker sequences after the user answers.

use lilia_contracts::{ProductApprovalDecision, TaskId};
use serde_json::Value;

use crate::runtime::DesktopAgentRuntime;
use crate::turn_page::{handle_observed_page, TurnPageHost};
use crate::turn_run::{AgentTurnError, ObservedTurnOutcome};

#[derive(Clone, Debug)]
pub struct InteractionResumeSpec {
    pub session_id: String,
    pub turn_id: String,
    pub version: u64,
    pub interaction_id: String,
    pub accepted: bool,
    pub response: Value,
}

pub trait TurnResumeHost: TurnPageHost {
    fn respond_approval_observed(
        &self,
        task_id: &TaskId,
        decision: ProductApprovalDecision,
    ) -> Result<ObservedTurnOutcome, AgentTurnError>;
    fn respond_interaction_observed(
        &self,
        task_id: &TaskId,
        spec: InteractionResumeSpec,
    ) -> Result<ObservedTurnOutcome, AgentTurnError>;
    fn emit_approval_changed(&self, task_id: &TaskId, request_id: &str, approved: bool);
    fn emit_interaction_changed(&self, task_id: &TaskId, request_id: &str, accepted: bool);
    fn emit_waiting_approval_error(
        &self,
        task_id: TaskId,
        turn_id: String,
        request_id: String,
        error: String,
    );
    fn emit_waiting_interaction_error(
        &self,
        task_id: TaskId,
        turn_id: String,
        request_id: String,
        error: String,
    );
}

pub fn run_approval_resume(
    runtime: &DesktopAgentRuntime,
    host: &dyn TurnResumeHost,
    task_id: TaskId,
    decision: ProductApprovalDecision,
) {
    let turn_id = decision.turn_id.clone();
    let request_id = decision.action_id.clone();
    let approved = decision.approved;
    match host.respond_approval_observed(&task_id, decision) {
        Ok(page) => {
            host.emit_approval_changed(&task_id, &request_id, approved);
            let page = ObservedTurnOutcome {
                cancelled_by_user: page.cancelled_by_user || !approved,
                ..page
            };
            if let Err(error) = handle_observed_page(runtime, host, &task_id, &turn_id, page) {
                host.finish_turn(
                    task_id,
                    turn_id,
                    crate::turn_page::TurnFinishKind::Failed,
                    Some(error.to_string()),
                );
            }
        }
        Err(error) => {
            host.emit_waiting_approval_error(task_id, turn_id, request_id, error.to_string());
        }
    }
}

pub fn run_interaction_resume(
    runtime: &DesktopAgentRuntime,
    host: &dyn TurnResumeHost,
    task_id: TaskId,
    spec: InteractionResumeSpec,
) {
    let turn_id = spec.turn_id.clone();
    let request_id = spec.interaction_id.clone();
    let accepted = spec.accepted;
    match host.respond_interaction_observed(&task_id, spec) {
        Ok(page) => {
            host.emit_interaction_changed(&task_id, &request_id, accepted);
            let page = ObservedTurnOutcome {
                cancelled_by_user: page.cancelled_by_user || !accepted,
                ..page
            };
            if let Err(error) = handle_observed_page(runtime, host, &task_id, &turn_id, page) {
                host.finish_turn(
                    task_id,
                    turn_id,
                    crate::turn_page::TurnFinishKind::Failed,
                    Some(error.to_string()),
                );
            }
        }
        Err(error) => {
            host.emit_waiting_interaction_error(task_id, turn_id, request_id, error.to_string());
        }
    }
}

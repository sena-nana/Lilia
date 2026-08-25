//! Kernel job lanes for submitting a claimed turn and resuming it after an
//! approval or interaction. The feature does not model turn lifecycle; the
//! host port talks to AgentKit and the durable queue.

use std::sync::Arc;

use lilia_contracts::ProductApprovalDecision;
use lilia_kernel::{JobContext, JobProtocol, JobSlot};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TURN_PROTOCOL: &str = "lilia.agent/turn@1";
pub const APPROVAL_PROTOCOL: &str = "lilia.agent/approval@1";
pub const INTERACTION_PROTOCOL: &str = "lilia.agent/interaction@1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnJobRequest {
    pub task_id: String,
    pub turn_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalJobRequest {
    pub task_id: String,
    pub decision: ProductApprovalDecision,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionJobRequest {
    pub task_id: String,
    pub resolution: Value,
}

pub trait TurnPort: Send + Sync + 'static {
    fn run_turn(&self, request: TurnJobRequest) -> Result<(), String>;
    fn run_approval(&self, request: ApprovalJobRequest) -> Result<(), String>;
    fn run_interaction(&self, request: InteractionJobRequest) -> Result<(), String>;
}

pub fn turn_slot(task_id: &str) -> JobSlot {
    JobSlot::new(format!("lilia.agent.turn.{task_id}")).expect("the turn slot name is not blank")
}

pub(crate) fn turn_protocols(port: Arc<dyn TurnPort>) -> Vec<JobProtocol> {
    let turn = Arc::clone(&port);
    let approval = Arc::clone(&port);
    vec![
        JobProtocol::new(
            TURN_PROTOCOL,
            Arc::new(move |payload, _context: &JobContext| run_turn_job(payload, turn.as_ref())),
        ),
        JobProtocol::new(
            APPROVAL_PROTOCOL,
            Arc::new(move |payload, _context: &JobContext| {
                run_approval_job(payload, approval.as_ref())
            }),
        ),
        JobProtocol::new(
            INTERACTION_PROTOCOL,
            Arc::new(move |payload, _context: &JobContext| {
                run_interaction_job(payload, port.as_ref())
            }),
        ),
    ]
}

fn run_turn_job(payload: Value, port: &dyn TurnPort) -> Result<Value, String> {
    let request: TurnJobRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid turn request: {error}"))?;
    port.run_turn(request)?;
    Ok(Value::Null)
}

fn run_approval_job(payload: Value, port: &dyn TurnPort) -> Result<Value, String> {
    let request: ApprovalJobRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid approval request: {error}"))?;
    port.run_approval(request)?;
    Ok(Value::Null)
}

fn run_interaction_job(payload: Value, port: &dyn TurnPort) -> Result<Value, String> {
    let request: InteractionJobRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid interaction request: {error}"))?;
    port.run_interaction(request)?;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        turns: Mutex<Vec<TurnJobRequest>>,
        approvals: Mutex<Vec<ApprovalJobRequest>>,
        interactions: Mutex<Vec<InteractionJobRequest>>,
        failure: Option<String>,
    }

    impl TurnPort for RecordingPort {
        fn run_turn(&self, request: TurnJobRequest) -> Result<(), String> {
            self.turns.lock().unwrap().push(request);
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(()),
            }
        }

        fn run_approval(&self, request: ApprovalJobRequest) -> Result<(), String> {
            self.approvals.lock().unwrap().push(request);
            Ok(())
        }

        fn run_interaction(&self, request: InteractionJobRequest) -> Result<(), String> {
            self.interactions.lock().unwrap().push(request);
            Ok(())
        }
    }

    fn turn() -> TurnJobRequest {
        TurnJobRequest {
            task_id: "task-1".to_owned(),
            turn_id: "turn-7".to_owned(),
        }
    }

    #[test]
    fn the_turn_job_forwards_the_claimed_task_and_turn() {
        let port = RecordingPort::default();

        run_turn_job(serde_json::to_value(turn()).unwrap(), &port).unwrap();

        assert_eq!(port.turns.lock().unwrap().as_slice(), [turn()]);
    }

    #[test]
    fn a_failed_port_fails_the_job_with_its_message() {
        let port = RecordingPort {
            failure: Some("session unavailable".to_owned()),
            ..RecordingPort::default()
        };

        let error = run_turn_job(serde_json::to_value(turn()).unwrap(), &port)
            .expect_err("an unavailable session fails the job");

        assert_eq!(error, "session unavailable");
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_turn_job(
            serde_json::json!({ "turnId": "turn-7" }),
            &RecordingPort::default(),
        )
        .expect_err("a request without a task cannot run");

        assert!(error.contains("invalid turn request"), "{error}");
    }

    #[test]
    fn each_task_runs_in_its_own_lane() {
        assert_ne!(turn_slot("task-1"), turn_slot("task-2"));
    }
}

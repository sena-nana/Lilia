//! Kernel job lane for naming a task after one of its turns finished.
//!
//! Auto-titling calls an auxiliary model, so it must not run on the turn worker
//! that just finished. It used to run on a private two-thread pool with a
//! bounded queue and a per-task generation counter deciding which answer was
//! still current. The kernel already owns all three: the task pool runs it, a
//! slot per task supersedes the previous turn's pending title, and the journal
//! records that a title was attempted at all.

use std::sync::Arc;

use lilia_kernel::{JobContext, JobProtocol, JobSlot};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TITLE_PROTOCOL: &str = "lilia.agent/title@1";

/// Payload of [`TITLE_PROTOCOL`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleRequest {
    pub task_id: String,
    /// The turn whose transcript the title is drawn from. Absent when a task is
    /// titled outside a turn.
    pub turn_id: Option<String>,
}

/// Produces and applies one task title. The host owns the auxiliary model, the
/// prompt and the staleness check against the timeline the title was read from.
pub trait TitlePort: Send + Sync + 'static {
    fn title(&self, request: TitleRequest) -> Result<(), String>;
}

/// One lane per task. Two tasks may be titled at once, but a task's newer turn
/// replaces the title still pending from its previous one.
pub fn title_slot(task_id: &str) -> JobSlot {
    JobSlot::new(format!("lilia.agent.title.{task_id}")).expect("the title slot name is not blank")
}

pub(crate) fn title_protocol(port: Arc<dyn TitlePort>) -> JobProtocol {
    JobProtocol::new(
        TITLE_PROTOCOL,
        Arc::new(move |payload, _context: &JobContext| run_title_job(payload, port.as_ref())),
    )
}

fn run_title_job(payload: Value, port: &dyn TitlePort) -> Result<Value, String> {
    let request: TitleRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid title request: {error}"))?;
    port.title(request)?;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        requests: Mutex<Vec<TitleRequest>>,
        failure: Option<String>,
    }

    impl TitlePort for RecordingPort {
        fn title(&self, request: TitleRequest) -> Result<(), String> {
            self.requests.lock().unwrap().push(request);
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(()),
            }
        }
    }

    fn request() -> TitleRequest {
        TitleRequest {
            task_id: "task-1".to_owned(),
            turn_id: Some("turn-7".to_owned()),
        }
    }

    #[test]
    fn the_job_forwards_the_task_and_the_turn_it_reads() {
        let port = RecordingPort::default();

        run_title_job(serde_json::to_value(request()).unwrap(), &port).unwrap();

        assert_eq!(port.requests.lock().unwrap().as_slice(), [request()]);
    }

    #[test]
    fn a_task_titled_outside_a_turn_carries_no_turn_id() {
        let port = RecordingPort::default();
        let untied = TitleRequest {
            task_id: "task-1".to_owned(),
            turn_id: None,
        };

        run_title_job(serde_json::to_value(&untied).unwrap(), &port).unwrap();

        assert_eq!(port.requests.lock().unwrap().as_slice(), [untied]);
    }

    #[test]
    fn an_unavailable_model_fails_the_job_with_its_message() {
        let port = RecordingPort {
            failure: Some("辅助模型未配置".to_owned()),
            ..RecordingPort::default()
        };

        let error = run_title_job(serde_json::to_value(request()).unwrap(), &port)
            .expect_err("an unavailable model fails the job");

        assert_eq!(error, "辅助模型未配置");
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_title_job(
            serde_json::json!({ "turnId": "turn-7" }),
            &RecordingPort::default(),
        )
        .expect_err("a request without a task cannot run");

        assert!(error.contains("invalid title request"), "{error}");
    }

    #[test]
    fn each_task_is_titled_in_its_own_lane() {
        assert_ne!(title_slot("task-1"), title_slot("task-2"));
    }
}

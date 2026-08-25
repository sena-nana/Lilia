//! Kernel job lane for generating a suggestion set.
//!
//! Generation calls an auxiliary model, so it is slow enough that the user can
//! switch tasks while it runs. The shell used to guard that with a
//! `conversation_suggestion_operation_sequence` shared by every window; a slot
//! per window says the same thing and lets the kernel discard the superseded
//! answer instead of the surface recognising and dropping it.

use std::sync::Arc;

use lilia_kernel::{JobContext, JobProtocol, JobSlot};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const GENERATE_PROTOCOL: &str = "lilia.suggestion/generate@1";

/// Payload of [`GENERATE_PROTOCOL`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    pub project_id: String,
    /// Ignores a cached set that is otherwise still fresh.
    pub force: bool,
}

/// Generates one suggestion set. The host owns the auxiliary model and the
/// project scope the suggestions are drawn from.
pub trait SuggestionPort: Send + Sync + 'static {
    fn generate(&self, request: GenerateRequest) -> Result<Value, String>;
}

/// One lane per window. The main window and a task popup ask independently, so
/// neither may cancel the other; asking twice in one window replaces.
pub fn generate_slot(window: u64) -> JobSlot {
    JobSlot::new(format!("lilia.suggestion.generate.{window}"))
        .expect("the suggestion slot name is not blank")
}

pub(crate) fn generate_protocol(port: Arc<dyn SuggestionPort>) -> JobProtocol {
    JobProtocol::new(
        GENERATE_PROTOCOL,
        Arc::new(move |payload, _context: &JobContext| {
            run_generate_job(payload, port.as_ref())
        }),
    )
}

fn run_generate_job(payload: Value, port: &dyn SuggestionPort) -> Result<Value, String> {
    let request: GenerateRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid suggestion request: {error}"))?;
    port.generate(request)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        requests: Mutex<Vec<GenerateRequest>>,
        failure: Option<String>,
    }

    impl SuggestionPort for RecordingPort {
        fn generate(&self, request: GenerateRequest) -> Result<Value, String> {
            self.requests.lock().unwrap().push(request);
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(serde_json::json!([])),
            }
        }
    }

    fn request() -> GenerateRequest {
        GenerateRequest {
            project_id: "project-1".to_owned(),
            force: true,
        }
    }

    #[test]
    fn the_job_forwards_the_scope_and_the_force_flag() {
        let port = RecordingPort::default();

        run_generate_job(serde_json::to_value(request()).unwrap(), &port).unwrap();

        assert_eq!(port.requests.lock().unwrap().as_slice(), [request()]);
    }

    #[test]
    fn a_model_failure_fails_the_job_with_its_message() {
        let port = RecordingPort {
            failure: Some("辅助模型未配置".to_owned()),
            ..RecordingPort::default()
        };

        let error = run_generate_job(serde_json::to_value(request()).unwrap(), &port)
            .expect_err("an unavailable model fails the job");

        assert_eq!(error, "辅助模型未配置");
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_generate_job(
            serde_json::json!({ "force": "yes" }),
            &RecordingPort::default(),
        )
        .expect_err("a malformed request cannot run");

        assert!(error.contains("invalid suggestion request"), "{error}");
    }

    #[test]
    fn each_window_generates_in_its_own_lane() {
        assert_ne!(generate_slot(0), generate_slot(1));
    }
}

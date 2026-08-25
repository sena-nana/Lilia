//! Legacy data import domain feature.
//!
//! Importing an old `~/.lilia` home is a two-step conversation: plan what can
//! be moved, show the user, then execute what they approved. Both steps copy
//! whole SQLite databases and touch the OS credential store, so neither may run
//! on the UI thread.
//!
//! Both lanes are ticket-based. An import runs against a
//! `DesktopApplicationConfig` and a live host handle — values that describe
//! open file locks and OS services rather than data — so they stay staged with
//! the host and the payload names only the ticket. That also keeps the source
//! and target home paths, which identify a specific user's disk, out of the
//! journal.

use std::sync::Arc;

use lilia_kernel::{
    Feature, FeatureContext, FeatureId, JobContext, JobProtocol, JobSlot, KernelError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PLAN_PROTOCOL: &str = "lilia.import/plan@1";
pub const EXECUTE_PROTOCOL: &str = "lilia.import/execute@1";

/// Payload of both protocols.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub ticket: u64,
}

impl ImportRequest {
    pub fn new(ticket: u64) -> Self {
        Self { ticket }
    }
}

/// Runs the import step the host staged under `ticket`.
pub trait ImportPort: Send + Sync + 'static {
    fn plan(&self, ticket: u64) -> Result<Value, String>;
    fn execute(&self, ticket: u64) -> Result<Value, String>;
}

/// Single-flight lane. Planning and executing share it deliberately: executing
/// a plan while a new one is being computed would import a set the user never
/// saw.
pub fn import_slot() -> JobSlot {
    JobSlot::new("lilia.import").expect("the import slot name is not blank")
}

pub struct ImportFeature {
    port: Arc<dyn ImportPort>,
}

impl ImportFeature {
    pub fn new(port: Arc<dyn ImportPort>) -> Self {
        Self { port }
    }
}

impl Feature for ImportFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.import").expect("the import feature id is not blank")
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        let plan = Arc::clone(&self.port);
        let execute = Arc::clone(&self.port);
        vec![
            JobProtocol::new(
                PLAN_PROTOCOL,
                Arc::new(move |payload, _context: &JobContext| {
                    run_import_job(payload, |ticket| plan.plan(ticket))
                }),
            ),
            JobProtocol::new(
                EXECUTE_PROTOCOL,
                Arc::new(move |payload, _context: &JobContext| {
                    run_import_job(payload, |ticket| execute.execute(ticket))
                }),
            ),
        ]
    }

    fn mount(&self, _cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        Ok(())
    }
}

fn run_import_job(
    payload: Value,
    step: impl FnOnce(u64) -> Result<Value, String>,
) -> Result<Value, String> {
    let request: ImportRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid import request: {error}"))?;
    step(request.ticket)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        planned: Mutex<Vec<u64>>,
        executed: Mutex<Vec<u64>>,
        failure: Option<String>,
    }

    impl ImportPort for RecordingPort {
        fn plan(&self, ticket: u64) -> Result<Value, String> {
            self.planned.lock().unwrap().push(ticket);
            self.answer(serde_json::json!({ "status": "ready" }))
        }

        fn execute(&self, ticket: u64) -> Result<Value, String> {
            self.executed.lock().unwrap().push(ticket);
            self.answer(serde_json::json!({ "imported": 3 }))
        }
    }

    impl RecordingPort {
        fn answer(&self, value: Value) -> Result<Value, String> {
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(value),
            }
        }
    }

    #[test]
    fn planning_claims_the_ticket_the_host_staged() {
        let port = RecordingPort::default();

        let output = run_import_job(serde_json::to_value(ImportRequest::new(5)).unwrap(), |ticket| {
            port.plan(ticket)
        })
        .unwrap();

        assert_eq!(output, serde_json::json!({ "status": "ready" }));
        assert_eq!(port.planned.lock().unwrap().as_slice(), [5]);
    }

    #[test]
    fn executing_claims_the_ticket_the_host_staged() {
        let port = RecordingPort::default();

        run_import_job(serde_json::to_value(ImportRequest::new(6)).unwrap(), |ticket| {
            port.execute(ticket)
        })
        .unwrap();

        assert_eq!(port.executed.lock().unwrap().as_slice(), [6]);
        assert!(port.planned.lock().unwrap().is_empty());
    }

    #[test]
    fn a_request_never_names_the_home_being_imported() {
        let payload = serde_json::to_value(ImportRequest::new(2)).unwrap();

        assert_eq!(payload, serde_json::json!({ "ticket": 2 }));
    }

    #[test]
    fn a_failing_step_fails_the_job_with_the_hosts_message() {
        let port = RecordingPort {
            failure: Some("旧版数据目录仍被占用".to_owned()),
            ..RecordingPort::default()
        };

        let error = run_import_job(
            serde_json::to_value(ImportRequest::new(1)).unwrap(),
            |ticket| port.plan(ticket),
        )
        .expect_err("a locked source home fails the job");

        assert_eq!(error, "旧版数据目录仍被占用");
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_import_job(serde_json::json!({ "ticket": "five" }), |_| {
            panic!("the step must not run on a malformed request")
        })
        .expect_err("a malformed request cannot run");

        assert!(error.contains("invalid import request"), "{error}");
    }
}

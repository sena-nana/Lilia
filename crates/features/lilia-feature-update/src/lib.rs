//! Update domain feature.
//!
//! Turns the release check and the installer launch into kernel job protocols.
//! Both run long enough to block the UI thread, and both must be single-flight:
//! the kernel slot enforces that, replacing the shell's
//! `update_operation_sequence` / `active_update_operation` pair.
//!
//! The update state itself is authored by the host through its own event
//! stream, so a job here reports only whether the operation succeeded.

use std::sync::Arc;

use lilia_kernel::{
    Feature, FeatureContext, FeatureId, JobContext, JobProtocol, JobSlot, KernelError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CHECK_PROTOCOL: &str = "lilia.update/check@1";
pub const INSTALL_PROTOCOL: &str = "lilia.update/install@1";

/// Payload of [`CHECK_PROTOCOL`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRequest {
    pub channel: String,
}

/// Payload of [`INSTALL_PROTOCOL`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRequest {
    pub version: String,
}

/// Runs the release check and the installer launch against the OS updater. The
/// port publishes the resulting update state itself; the job only reports the
/// failure message a caller should surface.
pub trait UpdatePort: Send + Sync + 'static {
    fn check(&self, channel: &str) -> Result<(), String>;
    fn install(&self, version: &str) -> Result<(), String>;
}

/// Single-flight lane shared by both protocols: a check and an install must
/// never overlap, because both drive the same update state.
pub fn update_slot() -> JobSlot {
    JobSlot::new("lilia.update").expect("the update slot name is not blank")
}

pub struct UpdateFeature {
    port: Arc<dyn UpdatePort>,
}

impl UpdateFeature {
    pub fn new(port: Arc<dyn UpdatePort>) -> Self {
        Self { port }
    }
}

impl Feature for UpdateFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.update").expect("the update feature id is not blank")
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        let check_port = Arc::clone(&self.port);
        let install_port = Arc::clone(&self.port);
        vec![
            JobProtocol::new(
                CHECK_PROTOCOL,
                Arc::new(move |payload, _context: &JobContext| {
                    run_check_job(payload, check_port.as_ref())
                }),
            ),
            JobProtocol::new(
                INSTALL_PROTOCOL,
                Arc::new(move |payload, _context: &JobContext| {
                    run_install_job(payload, install_port.as_ref())
                }),
            ),
        ]
    }

    fn mount(&self, _cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        Ok(())
    }
}

fn run_check_job(payload: Value, port: &dyn UpdatePort) -> Result<Value, String> {
    let request: CheckRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid update check request: {error}"))?;
    port.check(&request.channel)?;
    Ok(Value::Null)
}

fn run_install_job(payload: Value, port: &dyn UpdatePort) -> Result<Value, String> {
    let request: InstallRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid update install request: {error}"))?;
    port.install(&request.version)?;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        calls: Mutex<Vec<String>>,
        failure: Option<String>,
    }

    impl UpdatePort for RecordingPort {
        fn check(&self, channel: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("check:{channel}"));
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(()),
            }
        }

        fn install(&self, version: &str) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("install:{version}"));
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(()),
            }
        }
    }

    #[test]
    fn both_protocols_share_one_slot_so_a_check_and_an_install_cannot_overlap() {
        let feature = UpdateFeature::new(Arc::new(RecordingPort::default()));
        let protocols = feature.protocols();

        assert_eq!(
            protocols
                .iter()
                .map(|protocol| protocol.id.as_str())
                .collect::<Vec<_>>(),
            vec![CHECK_PROTOCOL, INSTALL_PROTOCOL]
        );
        assert_eq!(update_slot().as_str(), "lilia.update");
    }

    #[test]
    fn the_check_job_forwards_the_requested_channel() {
        let port = Arc::new(RecordingPort::default());

        run_check_job(
            serde_json::json!({ "channel": "preview" }),
            port.as_ref() as &dyn UpdatePort,
        )
        .unwrap();

        assert_eq!(port.calls.lock().unwrap().as_slice(), ["check:preview"]);
    }

    #[test]
    fn a_failing_port_fails_the_job_with_the_hosts_message() {
        let port = RecordingPort {
            failure: Some("the installer could not start".to_owned()),
            ..RecordingPort::default()
        };

        let error = run_install_job(serde_json::json!({ "version": "0.2.0" }), &port)
            .expect_err("a failing installer fails the job");

        assert_eq!(error, "the installer could not start");
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_check_job(serde_json::json!({ "channel": 7 }), &RecordingPort::default())
            .expect_err("a malformed request cannot be checked");

        assert!(error.contains("invalid update check request"), "{error}");
    }
}

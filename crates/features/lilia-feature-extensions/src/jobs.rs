//! Kernel job lane for extension registry mutations.
//!
//! Every extensions surface operation — skill, plugin and hook registry edits,
//! MCP activation, MCP content reads and MCP registry edits — runs in this one
//! lane, because they all mutate or read the same registries and the surface
//! disables itself while any of them is in flight. A single kernel slot states
//! that directly, replacing the shell's
//! `extensions_operation_sequence` / `active_extensions_operation` pair and the
//! seven threads it used to arbitrate.
//!
//! An MCP credential edit carries secret material, so the command itself never
//! enters the job payload: the host parks it under a ticket and the port claims
//! it on the worker thread. The payload keeps the operation label, which is what
//! a journal reader needs and all it may safely see.

use std::sync::Arc;

use lilia_kernel::{JobContext, JobProtocol, JobSlot};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MUTATE_PROTOCOL: &str = "lilia.extensions/mutate@1";

/// Payload of [`MUTATE_PROTOCOL`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutateRequest {
    /// Identifies the command the host parked for this job.
    pub ticket: u64,
    /// What the command does, for the journal: `skills.create`, `mcp.activate`.
    pub operation: String,
}

impl MutateRequest {
    pub fn new(ticket: u64, operation: impl Into<String>) -> Self {
        Self {
            ticket,
            operation: operation.into(),
        }
    }
}

/// Runs a parked extensions command against the registries. The host keeps the
/// command and its outcome, so the port only reports whether the ticket ran.
pub trait ExtensionsPort: Send + Sync + 'static {
    fn run(&self, ticket: u64) -> Result<(), String>;
}

/// The lane every extensions operation shares.
pub fn extensions_slot() -> JobSlot {
    JobSlot::new("lilia.extensions").expect("the extensions slot name is not blank")
}

pub(crate) fn mutate_protocol(port: Arc<dyn ExtensionsPort>) -> JobProtocol {
    JobProtocol::new(
        MUTATE_PROTOCOL,
        Arc::new(move |payload, _context: &JobContext| run_mutate_job(payload, port.as_ref())),
    )
}

fn run_mutate_job(payload: Value, port: &dyn ExtensionsPort) -> Result<Value, String> {
    let request: MutateRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid extensions request: {error}"))?;
    port.run(request.ticket)?;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        tickets: Mutex<Vec<u64>>,
        failure: Option<String>,
    }

    impl ExtensionsPort for RecordingPort {
        fn run(&self, ticket: u64) -> Result<(), String> {
            self.tickets.lock().unwrap().push(ticket);
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(()),
            }
        }
    }

    #[test]
    fn the_job_claims_the_ticket_the_host_parked() {
        let port = RecordingPort::default();

        run_mutate_job(
            serde_json::to_value(MutateRequest::new(7, "skills.create")).unwrap(),
            &port,
        )
        .unwrap();

        assert_eq!(port.tickets.lock().unwrap().as_slice(), [7]);
    }

    #[test]
    fn a_failing_command_fails_the_job_with_the_hosts_message() {
        let port = RecordingPort {
            failure: Some("the registry revision moved on".to_owned()),
            ..RecordingPort::default()
        };

        let error = run_mutate_job(
            serde_json::to_value(MutateRequest::new(1, "mcp.upsert")).unwrap(),
            &port,
        )
        .expect_err("a rejected registry edit fails the job");

        assert_eq!(error, "the registry revision moved on");
    }

    #[test]
    fn a_credential_edit_never_puts_the_secret_in_the_payload() {
        let payload = serde_json::to_value(MutateRequest::new(3, "mcp.credential.set")).unwrap();

        assert_eq!(
            payload,
            serde_json::json!({ "ticket": 3, "operation": "mcp.credential.set" })
        );
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_mutate_job(
            serde_json::json!({ "ticket": "seven" }),
            &RecordingPort::default(),
        )
        .expect_err("a malformed request cannot be run");

        assert!(error.contains("invalid extensions request"), "{error}");
    }
}

//! GitHub binding domain feature.
//!
//! Two lanes, because the surface treats them independently: binding the
//! account, and paging its repositories.
//!
//! Binding is a device flow — start it, show the user a code, then poll until
//! GitHub says authorized or the code expires. The shell used to run that as
//! two threads sharing an `AtomicBool`, with a `github_binding_operation_sequence`
//! deciding which reply was still wanted. It is one job here: the handler
//! reports the user-facing code as job progress and polls against
//! [`JobContext::is_cancelled`], so the kernel's cancellation replaces the flag
//! and the slot replaces the sequence.

use std::sync::Arc;

use lilia_kernel::{
    Feature, FeatureContext, FeatureId, JobContext, JobProtocol, JobSlot, KernelError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const BIND_PROTOCOL: &str = "lilia.github/bind@1";
pub const REPOSITORIES_PROTOCOL: &str = "lilia.github/repositories@1";

/// Payload of [`REPOSITORIES_PROTOCOL`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoriesRequest {
    pub page: u32,
}

impl RepositoriesRequest {
    pub fn new(page: u32) -> Self {
        Self { page }
    }
}

/// Runs the two GitHub operations against the host's HTTP client.
///
/// `bind` takes the [`JobContext`] because the device flow is a loop the host
/// owns: it decides the polling interval GitHub asked for, reports the user
/// code as progress, and must stop at a cancellation point without leaving a
/// half-authorized binding behind.
pub trait GitHubPort: Send + Sync + 'static {
    fn bind(&self, context: &JobContext) -> Result<Value, String>;
    fn repositories(&self, page: u32) -> Result<Value, String>;
}

/// The binding lane. Single flight: a second bind would race the first one's
/// device code.
pub fn bind_slot() -> JobSlot {
    JobSlot::new("lilia.github.bind").expect("the github bind slot name is not blank")
}

/// The repository paging lane, separate from binding so loading the next page
/// never supersedes an authorization the user is part-way through.
pub fn repositories_slot() -> JobSlot {
    JobSlot::new("lilia.github.repositories")
        .expect("the github repositories slot name is not blank")
}

pub struct GitHubFeature {
    port: Arc<dyn GitHubPort>,
}

impl GitHubFeature {
    pub fn new(port: Arc<dyn GitHubPort>) -> Self {
        Self { port }
    }
}

impl Feature for GitHubFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.github").expect("the github feature id is not blank")
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        let bind_port = Arc::clone(&self.port);
        let repositories_port = Arc::clone(&self.port);
        vec![
            JobProtocol::new(
                BIND_PROTOCOL,
                Arc::new(move |_payload, context: &JobContext| bind_port.bind(context)),
            ),
            JobProtocol::new(
                REPOSITORIES_PROTOCOL,
                Arc::new(move |payload, _context: &JobContext| {
                    run_repositories_job(payload, repositories_port.as_ref())
                }),
            ),
        ]
    }

    fn mount(&self, _cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        Ok(())
    }
}

fn run_repositories_job(payload: Value, port: &dyn GitHubPort) -> Result<Value, String> {
    let request: RepositoriesRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid github repositories request: {error}"))?;
    port.repositories(request.page)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        pages: Mutex<Vec<u32>>,
        failure: Option<String>,
    }

    impl GitHubPort for RecordingPort {
        fn bind(&self, context: &JobContext) -> Result<Value, String> {
            context.report(serde_json::json!({ "userCode": "ABCD-1234" }));
            if context.is_cancelled() {
                return Err("cancelled".to_owned());
            }
            Ok(serde_json::json!({ "status": "authorized" }))
        }

        fn repositories(&self, page: u32) -> Result<Value, String> {
            self.pages.lock().unwrap().push(page);
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(serde_json::json!({ "page": page })),
            }
        }
    }

    #[test]
    fn the_repositories_job_asks_for_the_page_the_surface_wants() {
        let port = RecordingPort::default();

        let output = run_repositories_job(
            serde_json::to_value(RepositoriesRequest::new(3)).unwrap(),
            &port,
        )
        .unwrap();

        assert_eq!(output, serde_json::json!({ "page": 3 }));
        assert_eq!(port.pages.lock().unwrap().as_slice(), [3]);
    }

    #[test]
    fn a_failing_page_read_fails_the_job_with_the_hosts_message() {
        let port = RecordingPort {
            failure: Some("GitHub 授权已过期".to_owned()),
            ..RecordingPort::default()
        };

        let error = run_repositories_job(
            serde_json::to_value(RepositoriesRequest::new(1)).unwrap(),
            &port,
        )
        .expect_err("a rejected page read fails the job");

        assert_eq!(error, "GitHub 授权已过期");
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_repositories_job(
            serde_json::json!({ "page": "first" }),
            &RecordingPort::default(),
        )
        .expect_err("a malformed request cannot run");

        assert!(
            error.contains("invalid github repositories request"),
            "{error}"
        );
    }

    #[test]
    fn binding_reports_the_user_code_as_progress_before_it_settles() {
        let context = JobContext::new();

        RecordingPort::default().bind(&context).unwrap();

        assert_eq!(
            context.progress(),
            serde_json::json!({ "userCode": "ABCD-1234" })
        );
    }

    #[test]
    fn a_cancelled_binding_stops_instead_of_reporting_success() {
        let context = JobContext::new();
        context.request_cancel();

        let error = RecordingPort::default()
            .bind(&context)
            .expect_err("a cancelled device flow must not report an authorized binding");

        assert_eq!(error, "cancelled");
    }

    #[test]
    fn the_two_lanes_stay_separate() {
        assert_ne!(bind_slot(), repositories_slot());
    }
}

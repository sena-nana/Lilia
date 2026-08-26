//! Remote control domain feature.
//!
//! Every remote control operation ends by reading back the host status, and the
//! surface shows exactly one of them at a time. That makes them one kernel job
//! lane, replacing the shell's `remote_operation_sequence` /
//! `active_remote_operation` pair and the thread they arbitrated.
//!
//! The status itself is the job output, so the shell never has to re-read it.

use std::sync::Arc;

use lilia_kernel::{
    Feature, FeatureContext, FeatureId, JobContext, JobProtocol, JobSlot, KernelError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod dispatch;
mod http;
mod service;
mod types;

pub use dispatch::{
    dispatch_remote_payload, dispatch_remote_request, remote_interaction_respond,
    remote_process_session_command, remote_session_fork_command, remote_timeline_snapshot,
    RemoteChatPermission, RemoteChatSpec, RemoteHost, RemoteProcessSessionCommand,
    RemoteSessionForkCommand,
};
pub use http::serve_http_bridge;
pub use service::{
    active_ticket, advertised_bridge_url, authorize_request, cancel_pairing, database_error,
    endpoint, endpoint_id, host_enabled, keep_awake_enabled, now_millis, pair_device, pc_name,
    refresh_trusted_peer_seen, remote_capabilities, remote_status, set_setting, start_pairing,
    url_encode, DesktopRemoteControlError, DesktopRemoteControlService, RemoteWakeHost,
    DEFAULT_HTTP_BRIDGE_PORT, HOST_ENABLED_KEY, KEEP_AWAKE_ENABLED_KEY, PC_NAME_KEY,
};
pub use types::{
    RemoteCapabilitySet, RemoteControlStatus, RemoteEndpointAddress, RemotePairDeviceInput,
    RemotePairingTicket, RemotePeerSummary, RemoteRequestEnvelope, REMOTE_ALPN,
    REMOTE_MIN_PROTOCOL_VERSION, REMOTE_PROTOCOL_VERSION,
};

pub const OPERATE_PROTOCOL: &str = "lilia.remote/operate@1";

/// Payload of [`OPERATE_PROTOCOL`]. Nothing here is secret: a pairing ticket is
/// minted by the host and never travels in a request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase")]
pub enum RemoteRequest {
    /// Re-reads the host status without changing it.
    Refresh,
    SetEnabled { enabled: bool },
    SetPcName { name: String },
    SetKeepAwake { enabled: bool },
    StartPairing,
    CancelPairing,
    RevokeDevice { device_id: String },
}

/// Applies one remote operation and returns the resulting host status as JSON.
/// The status shape belongs to the host, which owns the remote server.
pub trait RemotePort: Send + Sync + 'static {
    fn operate(&self, request: RemoteRequest) -> Result<Value, String>;
}

/// The lane every remote operation shares.
pub fn remote_slot() -> JobSlot {
    JobSlot::new("lilia.remote").expect("the remote slot name is not blank")
}

pub struct RemoteFeature {
    port: Arc<dyn RemotePort>,
}

impl RemoteFeature {
    pub fn new(port: Arc<dyn RemotePort>) -> Self {
        Self { port }
    }
}

impl Feature for RemoteFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.remote").expect("the remote feature id is not blank")
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        let port = Arc::clone(&self.port);
        vec![JobProtocol::new(
            OPERATE_PROTOCOL,
            Arc::new(move |payload, _context: &JobContext| {
                run_operate_job(payload, port.as_ref())
            }),
        )]
    }

    fn mount(&self, _cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        Ok(())
    }
}

fn run_operate_job(payload: Value, port: &dyn RemotePort) -> Result<Value, String> {
    let request: RemoteRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid remote request: {error}"))?;
    port.operate(request)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        requests: Mutex<Vec<RemoteRequest>>,
        failure: Option<String>,
    }

    impl RemotePort for RecordingPort {
        fn operate(&self, request: RemoteRequest) -> Result<Value, String> {
            self.requests.lock().unwrap().push(request);
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(serde_json::json!({ "hostEnabled": true })),
            }
        }
    }

    #[test]
    fn the_job_returns_the_status_the_host_read_back() {
        let port = RecordingPort::default();

        let output = run_operate_job(
            serde_json::to_value(RemoteRequest::SetEnabled { enabled: true }).unwrap(),
            &port,
        )
        .unwrap();

        assert_eq!(output, serde_json::json!({ "hostEnabled": true }));
        assert_eq!(
            port.requests.lock().unwrap().as_slice(),
            [RemoteRequest::SetEnabled { enabled: true }]
        );
    }

    #[test]
    fn every_operation_survives_the_payload_round_trip() {
        for request in [
            RemoteRequest::Refresh,
            RemoteRequest::SetEnabled { enabled: false },
            RemoteRequest::SetPcName {
                name: "工作站".to_owned(),
            },
            RemoteRequest::SetKeepAwake { enabled: true },
            RemoteRequest::StartPairing,
            RemoteRequest::CancelPairing,
            RemoteRequest::RevokeDevice {
                device_id: "device-1".to_owned(),
            },
        ] {
            let payload = serde_json::to_value(&request).unwrap();
            let restored: RemoteRequest = serde_json::from_value(payload).unwrap();

            assert_eq!(restored, request);
        }
    }

    #[test]
    fn a_failing_operation_fails_the_job_with_the_hosts_message() {
        let port = RecordingPort {
            failure: Some("远控服务未启动".to_owned()),
            ..RecordingPort::default()
        };

        let error = run_operate_job(
            serde_json::to_value(RemoteRequest::StartPairing).unwrap(),
            &port,
        )
        .expect_err("a rejected operation fails the job");

        assert_eq!(error, "远控服务未启动");
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_operate_job(
            serde_json::json!({ "operation": "explode" }),
            &RecordingPort::default(),
        )
        .expect_err("an unknown operation cannot run");

        assert!(error.contains("invalid remote request"), "{error}");
    }
}

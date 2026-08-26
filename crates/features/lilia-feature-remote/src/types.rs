use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const REMOTE_PROTOCOL_VERSION: i64 = 1;
pub const REMOTE_MIN_PROTOCOL_VERSION: i64 = 1;
pub const REMOTE_ALPN: &str = "lilia.remote-control.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEndpointAddress {
    pub endpoint_id: String,
    #[serde(default)]
    pub relay_url: Option<String>,
    #[serde(default)]
    pub direct_addresses: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCapabilitySet {
    pub protocol_version: i64,
    pub min_protocol_version: i64,
    pub alpn: String,
    pub supports_pairing: bool,
    pub supports_task_inbox: bool,
    pub supports_timeline_subscription: bool,
    pub supports_timeline_pagination: bool,
    pub supports_chat_send: bool,
    pub supports_interaction_response: bool,
    pub supports_interrupt: bool,
    pub supports_agent_wire: bool,
    pub supports_session_fork: bool,
    pub supports_process_session: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePeerSummary {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub endpoint_id: String,
    pub protocol_version: i64,
    pub trusted: bool,
    pub first_paired_at: i64,
    pub last_seen_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePairingTicket {
    pub id: String,
    pub pc_name: String,
    pub pc_endpoint: RemoteEndpointAddress,
    pub protocol_version: i64,
    pub challenge: String,
    pub expires_at: i64,
    pub pairing_uri: String,
    pub bridge_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteControlStatus {
    pub host_enabled: bool,
    pub state: String,
    pub pc_name: String,
    pub keep_awake_enabled: bool,
    pub endpoint: Option<RemoteEndpointAddress>,
    pub active_ticket: Option<RemotePairingTicket>,
    pub trusted_devices: Vec<RemotePeerSummary>,
    pub capabilities: RemoteCapabilitySet,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePairDeviceInput {
    pub ticket_id: String,
    pub challenge: String,
    pub device_name: String,
    pub android_endpoint: RemoteEndpointAddress,
    pub protocol_version: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRequestEnvelope {
    pub id: String,
    pub protocol_version: i64,
    #[serde(default)]
    pub sent_at: Option<i64>,
    pub device_id: String,
    pub request: Value,
}

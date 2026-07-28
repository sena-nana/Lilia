//! Service-mode health report (#60).

use serde::Serialize;

use crate::writer_lease::WriterLeaseHealth;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceHealthStatus {
    Ready,
    Degraded,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentHealth {
    pub ok: bool,
    pub detail: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHealthReport {
    pub status: ServiceHealthStatus,
    pub generation: u64,
    pub mode: &'static str,
    pub desktop_exclusive_runtime: bool,
    pub shared_runtime_clients: bool,
    pub shared_projection_clients: bool,
    pub core: ComponentHealth,
    pub agentkit: ComponentHealth,
    pub credential: ComponentHealth,
    pub projection: ComponentHealth,
    pub writer: WriterLeaseHealth,
}

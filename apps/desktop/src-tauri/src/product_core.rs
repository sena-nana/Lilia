//! Product-core facade status exposed to Desktop.
//!
//! Native AgentKit is the default Desktop execution backend after Host pin alignment
//! (`MutsukiCore@8a02d74`). Legacy Node `agent-runner` is limited-time compatibility
//! only via `LILIA_AGENT_EXECUTION_BACKEND=node` until
//! [`crate::native_agent::LEGACY_NODE_RUNNER_COMPAT_UNTIL`] (#47).

use serde::Serialize;

use crate::native_agent::{
    self, BACKEND_NATIVE_AGENTKIT, LEGACY_NODE_RUNNER_COMPAT_UNTIL,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCoreStatus {
    pub cargo_workspace: bool,
    pub lilia_contracts: bool,
    pub lilia_core: bool,
    pub lilia_storage: bool,
    pub default_execution_backend: &'static str,
    pub active_execution_backend: &'static str,
    pub native_agentkit_crate: &'static str,
    pub native_agentkit_wired_in_desktop: bool,
    pub node_runner_is_default: bool,
    /// #47 — Node runner remains compile/runtime compatible only behind explicit env.
    pub node_runner_legacy_compatibility: bool,
    pub node_runner_compat_until: &'static str,
    /// #47 — default install resources exclude Codex app-server.
    pub default_bundle_includes_official_agent_server: bool,
    /// #47 — default install resources exclude Node agent-runner.
    pub default_bundle_includes_node_agent_runner: bool,
    pub agent_capabilities: lilia_core::NativeAgentCapabilitySnapshot,
    pub mutsuki_core_pin: &'static str,
    pub credential_broker_wired: bool,
    pub timeline_is_agentkit_projection: bool,
    pub product_timeline_store: &'static str,
    pub desktop_sqlite_is_ui_cache_only: bool,
    pub live_model_adapter_drives_turn: bool,
}

#[tauri::command]
pub fn product_core_status() -> ProductCoreStatus {
    let host = native_agent::host_status();
    ProductCoreStatus {
        cargo_workspace: true,
        lilia_contracts: true,
        lilia_core: true,
        lilia_storage: true,
        default_execution_backend: BACKEND_NATIVE_AGENTKIT,
        active_execution_backend: host.active_backend,
        native_agentkit_crate: "crates/lilia-agent-integration",
        native_agentkit_wired_in_desktop: host.wired,
        node_runner_is_default: false,
        node_runner_legacy_compatibility: host.node_runner_legacy_compatibility,
        node_runner_compat_until: LEGACY_NODE_RUNNER_COMPAT_UNTIL,
        default_bundle_includes_official_agent_server: host
            .default_bundle_includes_official_agent_server,
        default_bundle_includes_node_agent_runner: host.default_bundle_includes_node_agent_runner,
        agent_capabilities: host.capabilities,
        mutsuki_core_pin: "8a02d749b8fa93d7e0392e5ba5bbe80102999511",
        credential_broker_wired: host
            .diagnostics
            .as_ref()
            .map(|d| d.credential.broker_ready)
            .unwrap_or(false),
        timeline_is_agentkit_projection: host.timeline_is_agentkit_projection,
        product_timeline_store: host.product_timeline_store,
        desktop_sqlite_is_ui_cache_only: host.desktop_sqlite_is_ui_cache_only,
        live_model_adapter_drives_turn: host.live_model_adapter_drives_turn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_defaults_to_native_agentkit_after_cutover() {
        let previous = std::env::var("LILIA_AGENT_EXECUTION_BACKEND").ok();
        std::env::remove_var("LILIA_AGENT_EXECUTION_BACKEND");
        let status = product_core_status();
        assert_eq!(status.default_execution_backend, BACKEND_NATIVE_AGENTKIT);
        assert_eq!(status.active_execution_backend, BACKEND_NATIVE_AGENTKIT);
        assert!(status.native_agentkit_wired_in_desktop);
        assert!(!status.node_runner_is_default);
        assert!(!status.agent_capabilities.node_runner_default);
        assert!(status.node_runner_legacy_compatibility);
        assert_eq!(status.node_runner_compat_until, LEGACY_NODE_RUNNER_COMPAT_UNTIL);
        assert!(!status.default_bundle_includes_official_agent_server);
        assert!(!status.default_bundle_includes_node_agent_runner);
        assert!(status.timeline_is_agentkit_projection);
        assert!(status.desktop_sqlite_is_ui_cache_only);
        assert_eq!(
            status.product_timeline_store,
            lilia_contracts::PRODUCT_TIMELINE_STORE_ID
        );
        assert_eq!(
            native_agent::resolve_execution_backend(),
            native_agent::ExecutionBackend::NativeAgentkit
        );
        match previous {
            Some(value) => std::env::set_var("LILIA_AGENT_EXECUTION_BACKEND", value),
            None => std::env::remove_var("LILIA_AGENT_EXECUTION_BACKEND"),
        }
    }
}

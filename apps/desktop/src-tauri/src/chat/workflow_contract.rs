use std::sync::OnceLock;

use serde::Deserialize;

const RUNTIME_COMMAND_CONTRACT_JSON: &str =
    include_str!("../../../../../packages/contracts/src/runtime-command-contract.json");
const SESSION_MANAGEMENT_CONTRACT_JSON: &str =
    include_str!("../../../../../packages/contracts/src/session-management-contract.json");

static RUNTIME_COMMAND_CONTRACT: OnceLock<RuntimeCommandContract> = OnceLock::new();
static SESSION_MANAGEMENT_CONTRACT: OnceLock<SessionManagementContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeCommandContract {
    runtime_settings: RuntimeCommandTypeEntry,
    remote_environment: RuntimeCommandTypeEntry,
    sandbox_diagnostics: RuntimeCommandTypeEntry,
    session_fork: RuntimeCommandTypeEntry,
    process_session: RuntimeCommandTypeEntry,
}

#[derive(Debug, Deserialize)]
struct RuntimeCommandTypeEntry {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionManagementContract {
    runtime_command_type: String,
}

fn runtime_command_contract() -> &'static RuntimeCommandContract {
    RUNTIME_COMMAND_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            RUNTIME_COMMAND_CONTRACT_JSON,
            "runtime-command-contract.json",
        )
    })
}

fn session_management_contract() -> &'static SessionManagementContract {
    SESSION_MANAGEMENT_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            SESSION_MANAGEMENT_CONTRACT_JSON,
            "session-management-contract.json",
        )
    })
}

pub(super) fn session_fork_runtime_command_type() -> &'static str {
    &runtime_command_contract().session_fork.kind
}

pub(super) fn session_management_runtime_command_type() -> &'static str {
    &session_management_contract().runtime_command_type
}

pub(super) fn runtime_settings_command_type() -> &'static str {
    &runtime_command_contract().runtime_settings.kind
}

pub(super) fn remote_environment_command_type() -> &'static str {
    &runtime_command_contract().remote_environment.kind
}

pub(super) fn sandbox_diagnostics_command_type() -> &'static str {
    &runtime_command_contract().sandbox_diagnostics.kind
}

pub(super) fn process_session_command_type() -> &'static str {
    &runtime_command_contract().process_session.kind
}

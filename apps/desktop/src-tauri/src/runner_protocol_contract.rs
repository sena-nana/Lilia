use std::sync::OnceLock;

use serde::Deserialize;

const RUNNER_PROTOCOL_CONTRACT_JSON: &str =
    include_str!("../../../../packages/contracts/src/runner-protocol-contract.json");

static RUNNER_PROTOCOL_CONTRACT: OnceLock<RunnerProtocolContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunnerProtocolContract {
    runtime_event_types: RunnerRuntimeEventTypes,
    control_message_types: RunnerControlMessageTypes,
    // Read by build_runner_stdin_payload (legacy-runner / tests only).
    #[allow(dead_code)]
    stdin_payload_keys: RunnerStdinPayloadKeys,
    #[allow(dead_code)]
    stdin_turn_keys: RunnerStdinTurnKeys,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunnerRuntimeEventTypes {
    pub(crate) tool_use: String,
    pub(crate) todo_list: String,
    pub(crate) timeline: String,
    pub(crate) interaction_request: String,
    pub(crate) quota_usage_request: String,
    pub(crate) context_usage: String,
    pub(crate) done: String,
    pub(crate) prompt_suggestion: String,
    pub(crate) error: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunnerControlMessageTypes {
    pub(crate) interaction_response: String,
    pub(crate) settings_update: String,
    pub(crate) interrupt_turn: String,
    pub(crate) quota_usage_result: String,
    pub(crate) lilia_iab_result: String,
}

#[cfg_attr(not(any(feature = "legacy-runner", test)), allow(dead_code))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunnerStdinPayloadKeys {
    pub(crate) backend: String,
    pub(crate) turn: String,
    pub(crate) workflow: String,
    pub(crate) runtime_command: String,
    pub(crate) runtime_options: String,
    pub(crate) extensions: String,
}

#[cfg_attr(not(any(feature = "legacy-runner", test)), allow(dead_code))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunnerStdinTurnKeys {
    pub(crate) cwd: String,
    pub(crate) prompt: String,
    pub(crate) attachments: String,
    pub(crate) conversation_references: String,
    pub(crate) model: String,
    pub(crate) resume_session_id: String,
    pub(crate) plan_mode: String,
    pub(crate) goal_mode: String,
    pub(crate) permission: String,
}

fn runner_protocol_contract() -> &'static RunnerProtocolContract {
    RUNNER_PROTOCOL_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            RUNNER_PROTOCOL_CONTRACT_JSON,
            "runner-protocol-contract.json",
        )
    })
}

pub(crate) fn runner_runtime_event_types() -> &'static RunnerRuntimeEventTypes {
    &runner_protocol_contract().runtime_event_types
}

pub(crate) fn runner_control_message_types() -> &'static RunnerControlMessageTypes {
    &runner_protocol_contract().control_message_types
}

#[cfg(any(feature = "legacy-runner", test))]
pub(crate) fn runner_stdin_payload_keys() -> &'static RunnerStdinPayloadKeys {
    &runner_protocol_contract().stdin_payload_keys
}

#[cfg(any(feature = "legacy-runner", test))]
pub(crate) fn runner_stdin_turn_keys() -> &'static RunnerStdinTurnKeys {
    &runner_protocol_contract().stdin_turn_keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_protocol_stdin_keys_load_from_contract_manifest() {
        let payload = runner_stdin_payload_keys();
        assert_eq!(payload.turn, "turn");
        assert_eq!(payload.runtime_command, "runtimeCommand");
        assert_eq!(payload.runtime_options, "runtimeOptions");

        let turn = runner_stdin_turn_keys();
        assert_eq!(turn.prompt, "prompt");
        assert_eq!(turn.conversation_references, "conversationReferences");
        assert_eq!(turn.resume_session_id, "resumeSessionId");
    }
}

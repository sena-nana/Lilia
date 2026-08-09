use std::sync::OnceLock;

use serde::Deserialize;

const AGENT_INTERACTION_CONTRACT_JSON: &str =
    include_str!("../../../../packages/contracts/src/agent-interaction-contract.json");

static AGENT_INTERACTION_CONTRACT: OnceLock<AgentInteractionContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentInteractionContract {
    ask_user_interaction_kind: String,
    #[cfg(test)]
    plan_approval_interaction_kind: String,
    #[cfg(test)]
    tool_consent_interaction_kind: String,
    architecture_interaction_kind: String,
    #[cfg(test)]
    mcp_elicitation_interaction_kind: String,
    permission_approval_interaction_kind: String,
    #[cfg(test)]
    commands: AgentInteractionCommandsContract,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentInteractionCommandsContract {
    get_settings: String,
    set_settings: String,
    list_subagents: String,
    upsert_subagent: String,
    delete_subagent: String,
}

fn agent_interaction_contract() -> &'static AgentInteractionContract {
    AGENT_INTERACTION_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            AGENT_INTERACTION_CONTRACT_JSON,
            "agent-interaction-contract.json",
        )
    })
}

pub(crate) fn ask_user_interaction_kind() -> &'static str {
    &agent_interaction_contract().ask_user_interaction_kind
}

#[cfg(test)]
pub(crate) fn plan_approval_interaction_kind() -> &'static str {
    &agent_interaction_contract().plan_approval_interaction_kind
}

#[cfg(test)]
pub(crate) fn tool_consent_interaction_kind() -> &'static str {
    &agent_interaction_contract().tool_consent_interaction_kind
}

pub(crate) fn architecture_interaction_kind() -> &'static str {
    &agent_interaction_contract().architecture_interaction_kind
}

#[cfg(test)]
pub(crate) fn mcp_elicitation_interaction_kind() -> &'static str {
    &agent_interaction_contract().mcp_elicitation_interaction_kind
}

pub(crate) fn permission_approval_interaction_kind() -> &'static str {
    &agent_interaction_contract().permission_approval_interaction_kind
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{
        agent_interaction_delete_subagent, agent_interaction_get_settings,
        agent_interaction_list_subagents, agent_interaction_set_settings,
        agent_interaction_upsert_subagent,
    };

    #[test]
    fn agent_interaction_command_names_load_from_contract_manifest() {
        let commands = &agent_interaction_contract().commands;
        let _ = agent_interaction_get_settings;
        let _ = agent_interaction_set_settings;
        let _ = agent_interaction_list_subagents;
        let _ = agent_interaction_upsert_subagent;
        let _ = agent_interaction_delete_subagent;

        assert_eq!(
            commands.get_settings,
            stringify!(agent_interaction_get_settings)
        );
        assert_eq!(
            commands.set_settings,
            stringify!(agent_interaction_set_settings)
        );
        assert_eq!(
            commands.list_subagents,
            stringify!(agent_interaction_list_subagents)
        );
        assert_eq!(
            commands.upsert_subagent,
            stringify!(agent_interaction_upsert_subagent)
        );
        assert_eq!(
            commands.delete_subagent,
            stringify!(agent_interaction_delete_subagent)
        );
    }

    #[test]
    fn agent_interaction_kinds_load_from_contract_manifest() {
        assert_eq!(ask_user_interaction_kind(), "ask_user");
        assert_eq!(plan_approval_interaction_kind(), "plan_approval");
        assert_eq!(tool_consent_interaction_kind(), "tool_consent");
        assert_eq!(architecture_interaction_kind(), "architecture_change");
        assert_eq!(mcp_elicitation_interaction_kind(), "mcp_elicitation");
        assert_eq!(
            permission_approval_interaction_kind(),
            "permission_approval"
        );
    }
}

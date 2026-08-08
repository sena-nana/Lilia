use std::sync::OnceLock;

use serde::Deserialize;

const CHAT_EVENTS_CONTRACT_JSON: &str =
    include_str!("../../../../../packages/contracts/src/chat-events-contract.json");

static CHAT_EVENTS_CONTRACT: OnceLock<ChatEventsContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatEventsContract {
    #[allow(dead_code)] // legacy-runner emits; native path does not yet
    chat_turn_started_event_name: String,
    chat_done_event_name: String,
    chat_context_usage_event_name: String,
    chat_agent_interaction_request_event_name: String,
    agent_timeline_event_name: String,
    agent_timeline_batch_event_name: String,
}

fn chat_events_contract() -> &'static ChatEventsContract {
    CHAT_EVENTS_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            CHAT_EVENTS_CONTRACT_JSON,
            "chat-events-contract.json",
        )
    })
}

#[cfg(any(feature = "legacy-runner", test))]
pub(crate) fn turn_started_event_name() -> &'static str {
    &chat_events_contract().chat_turn_started_event_name
}

pub(crate) fn done_event_name() -> &'static str {
    &chat_events_contract().chat_done_event_name
}

pub(crate) fn context_usage_event_name() -> &'static str {
    &chat_events_contract().chat_context_usage_event_name
}

pub(crate) fn agent_interaction_request_event_name() -> &'static str {
    &chat_events_contract().chat_agent_interaction_request_event_name
}

pub(crate) fn agent_timeline_event_name() -> &'static str {
    &chat_events_contract().agent_timeline_event_name
}

pub(crate) fn agent_timeline_batch_event_name() -> &'static str {
    &chat_events_contract().agent_timeline_batch_event_name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_event_names_load_from_contract_manifest() {
        assert_eq!(turn_started_event_name(), "chat:turn-started");
        assert_eq!(done_event_name(), "chat:done");
        assert_eq!(context_usage_event_name(), "chat:context-usage");
        assert_eq!(
            agent_interaction_request_event_name(),
            "chat:agent-interaction-request"
        );
        assert_eq!(agent_timeline_event_name(), "agent:timeline");
        assert_eq!(agent_timeline_batch_event_name(), "agent:timeline-batch");
    }
}

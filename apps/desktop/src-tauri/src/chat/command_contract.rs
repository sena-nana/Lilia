use std::sync::OnceLock;

use serde::Deserialize;

const CHAT_COMMANDS_CONTRACT_JSON: &str =
    include_str!("../../../../../packages/contracts/src/chat-commands-contract.json");

static CHAT_COMMANDS_CONTRACT: OnceLock<ChatCommandsContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatCommandsContract {
    agent_timeline_list_command: String,
    agent_timeline_clear_task_command: String,
    chat_send_message_command: String,
    chat_interrupt_turn_command: String,
    chat_describe_attachments_command: String,
    chat_search_context_attachments_command: String,
    chat_search_slash_commands_command: String,
    chat_read_clipboard_file_paths_command: String,
    chat_save_clipboard_image_command: String,
    chat_save_clipboard_text_command: String,
    chat_get_composer_state_command: String,
    chat_get_composer_draft_command: String,
    chat_list_models_command: String,
    chat_get_runtime_snapshot_command: String,
    chat_set_composer_state_command: String,
    chat_set_composer_draft_command: String,
    chat_ack_restored_rollback_command: String,
    chat_respond_agent_interaction_command: String,
    chat_respond_title_update_command: String,
    lilia_iab_open_command: String,
    lilia_iab_submit_command: String,
}

fn chat_commands_contract() -> &'static ChatCommandsContract {
    CHAT_COMMANDS_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            CHAT_COMMANDS_CONTRACT_JSON,
            "chat-commands-contract.json",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_timeline::{agent_timeline_clear_task, agent_timeline_list};
    use crate::chat::attachments::{
        chat_describe_attachments, chat_read_clipboard_file_paths, chat_save_clipboard_image,
        chat_save_clipboard_text, chat_search_context_attachments,
    };
    use crate::chat::commands::{
        chat_ack_restored_rollback, chat_get_composer_draft, chat_get_composer_state,
        chat_get_runtime_snapshot, chat_interrupt_turn, chat_list_models,
        chat_respond_agent_interaction, chat_send_message, chat_set_composer_draft,
        chat_set_composer_state,
    };
    use crate::chat::slash_commands::chat_search_slash_commands;
    use crate::chat::title_update::chat_respond_title_update;
    use crate::lilia_iab::{lilia_iab_open, lilia_iab_submit};

    #[test]
    fn chat_command_names_load_from_contract_manifest() {
        let contract = chat_commands_contract();
        let _ = agent_timeline_list;
        let _ = agent_timeline_clear_task;
        let _ = chat_send_message;
        let _ = chat_interrupt_turn;
        let _ = chat_describe_attachments;
        let _ = chat_search_context_attachments;
        let _ = chat_search_slash_commands;
        let _ = chat_read_clipboard_file_paths;
        let _ = chat_save_clipboard_image;
        let _ = chat_save_clipboard_text;
        let _ = chat_get_composer_state;
        let _ = chat_get_composer_draft;
        let _ = chat_list_models;
        let _ = chat_get_runtime_snapshot;
        let _ = chat_set_composer_state;
        let _ = chat_set_composer_draft;
        let _ = chat_ack_restored_rollback;
        let _ = chat_respond_agent_interaction;
        let _ = chat_respond_title_update;
        let _ = lilia_iab_open;
        let _ = lilia_iab_submit;

        assert_eq!(
            contract.agent_timeline_list_command,
            stringify!(agent_timeline_list)
        );
        assert_eq!(
            contract.agent_timeline_clear_task_command,
            stringify!(agent_timeline_clear_task)
        );
        assert_eq!(
            contract.chat_send_message_command,
            stringify!(chat_send_message)
        );
        assert_eq!(
            contract.chat_interrupt_turn_command,
            stringify!(chat_interrupt_turn)
        );
        assert_eq!(
            contract.chat_describe_attachments_command,
            stringify!(chat_describe_attachments)
        );
        assert_eq!(
            contract.chat_search_context_attachments_command,
            stringify!(chat_search_context_attachments)
        );
        assert_eq!(
            contract.chat_search_slash_commands_command,
            stringify!(chat_search_slash_commands)
        );
        assert_eq!(
            contract.chat_read_clipboard_file_paths_command,
            stringify!(chat_read_clipboard_file_paths)
        );
        assert_eq!(
            contract.chat_save_clipboard_image_command,
            stringify!(chat_save_clipboard_image)
        );
        assert_eq!(
            contract.chat_save_clipboard_text_command,
            stringify!(chat_save_clipboard_text)
        );
        assert_eq!(
            contract.chat_get_composer_state_command,
            stringify!(chat_get_composer_state)
        );
        assert_eq!(
            contract.chat_get_composer_draft_command,
            stringify!(chat_get_composer_draft)
        );
        assert_eq!(
            contract.chat_list_models_command,
            stringify!(chat_list_models)
        );
        assert_eq!(
            contract.chat_get_runtime_snapshot_command,
            stringify!(chat_get_runtime_snapshot)
        );
        assert_eq!(
            contract.chat_set_composer_state_command,
            stringify!(chat_set_composer_state)
        );
        assert_eq!(
            contract.chat_set_composer_draft_command,
            stringify!(chat_set_composer_draft)
        );
        assert_eq!(
            contract.chat_ack_restored_rollback_command,
            stringify!(chat_ack_restored_rollback)
        );
        assert_eq!(
            contract.chat_respond_agent_interaction_command,
            stringify!(chat_respond_agent_interaction)
        );
        assert_eq!(
            contract.chat_respond_title_update_command,
            stringify!(chat_respond_title_update)
        );
        assert_eq!(contract.lilia_iab_open_command, stringify!(lilia_iab_open));
        assert_eq!(
            contract.lilia_iab_submit_command,
            stringify!(lilia_iab_submit)
        );
    }
}

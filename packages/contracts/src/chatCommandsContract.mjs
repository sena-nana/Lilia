import chatCommandsContract from "./chat-commands-contract.json" with { type: "json" };

const manifest = Object.freeze(chatCommandsContract);

export const CHAT_COMMANDS_CONTRACT = manifest;
export const AGENT_TIMELINE_LIST_COMMAND = manifest.agentTimelineListCommand;
export const AGENT_TIMELINE_CLEAR_TASK_COMMAND =
  manifest.agentTimelineClearTaskCommand;
export const CHAT_SEND_MESSAGE_COMMAND = manifest.chatSendMessageCommand;
export const CHAT_INTERRUPT_TURN_COMMAND = manifest.chatInterruptTurnCommand;
export const CHAT_DESCRIBE_ATTACHMENTS_COMMAND =
  manifest.chatDescribeAttachmentsCommand;
export const CHAT_SEARCH_CONTEXT_ATTACHMENTS_COMMAND =
  manifest.chatSearchContextAttachmentsCommand;
export const CHAT_SEARCH_SLASH_COMMANDS_COMMAND =
  manifest.chatSearchSlashCommandsCommand;
export const CHAT_READ_CLIPBOARD_FILE_PATHS_COMMAND =
  manifest.chatReadClipboardFilePathsCommand;
export const CHAT_SAVE_CLIPBOARD_IMAGE_COMMAND =
  manifest.chatSaveClipboardImageCommand;
export const CHAT_SAVE_CLIPBOARD_TEXT_COMMAND = manifest.chatSaveClipboardTextCommand;
export const CHAT_GET_COMPOSER_STATE_COMMAND = manifest.chatGetComposerStateCommand;
export const CHAT_GET_COMPOSER_DRAFT_COMMAND = manifest.chatGetComposerDraftCommand;
export const CHAT_LIST_MODELS_COMMAND = manifest.chatListModelsCommand;
export const CHAT_GET_RUNTIME_SNAPSHOT_COMMAND =
  manifest.chatGetRuntimeSnapshotCommand;
export const CHAT_SET_COMPOSER_STATE_COMMAND = manifest.chatSetComposerStateCommand;
export const CHAT_SET_COMPOSER_DRAFT_COMMAND = manifest.chatSetComposerDraftCommand;
export const CHAT_ACK_RESTORED_ROLLBACK_COMMAND =
  manifest.chatAckRestoredRollbackCommand;
export const CHAT_SEND_PROCESS_SESSION_COMMAND =
  manifest.chatSendProcessSessionCommand;
export const CHAT_RESPOND_AGENT_INTERACTION_COMMAND =
  manifest.chatRespondAgentInteractionCommand;
export const CHAT_RESPOND_TITLE_UPDATE_COMMAND =
  manifest.chatRespondTitleUpdateCommand;
export const LILIA_IAB_OPEN_COMMAND = manifest.liliaIabOpenCommand;
export const LILIA_IAB_SUBMIT_COMMAND = manifest.liliaIabSubmitCommand;

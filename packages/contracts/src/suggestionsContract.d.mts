type SuggestionSource = "provider" | "assistant-ai";
type SuggestionItemSource = "task" | "github" | "local-git" | "session-thread" | "provider";
type SuggestionStatus = "idle" | "loading" | "empty" | "error";

export const SUGGESTIONS_CONTRACT: Record<string, unknown>;
export const SUGGESTION_SOURCES: readonly SuggestionSource[];
export const DEFAULT_SUGGESTION_SOURCE: SuggestionSource;
export const CONVERSATION_SUGGESTIONS_GET_COMMAND: "conversation_suggestions_get";
export const CONVERSATION_SUGGESTIONS_GET_SOURCES_COMMAND: "conversation_suggestions_get_sources";
export const CONVERSATION_SUGGESTIONS_GET_SETTINGS_COMMAND: "conversation_suggestions_get_settings";
export const CONVERSATION_SUGGESTIONS_SET_SETTINGS_COMMAND: "conversation_suggestions_set_settings";
export const SUGGESTION_SOURCE_LABELS: Readonly<Record<SuggestionSource, string>>;
export const SUGGESTION_SOURCE_SETTING_ORDER: readonly SuggestionSource[];
export const SUGGESTION_ITEM_SOURCES: readonly SuggestionItemSource[];
export const SUGGESTION_LOADING_SOURCE_LABELS: Readonly<
  Record<"task" | "github" | "session-thread" | "provider", string>
>;
export const SUGGESTION_LOCAL_GIT_LOADING_LABELS: Readonly<{
  recentCommitsAndChangedFiles: string;
  recentCommits: string;
  changedFiles: string;
  default: string;
}>;
export const SUGGESTION_STATUSES: readonly SuggestionStatus[];
export const DEFAULT_SUGGESTION_STATUS: SuggestionStatus;
export const DEFAULT_SUGGESTION_LOADING_TEXT: string;
export const SUGGESTION_STATUS_DISPLAY_TEXT: Readonly<Partial<Record<SuggestionStatus, string>>>;

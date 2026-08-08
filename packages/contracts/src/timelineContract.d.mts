export const TIMELINE_CONTRACT: Record<string, unknown>;
export const AGENT_TIMELINE_RUNNING_STATUSES: readonly string[];
export const AGENT_TIMELINE_TERMINAL_STATUSES: readonly string[];
export const TIMELINE_STATUS_ALIASES: Readonly<Record<string, string>>;
export const DEFAULT_TIMELINE_STATUS: string;
export const AGENT_TIMELINE_ACTION_KIND_BY_EVENT_KIND: Readonly<
  Record<string, string>
>;
export const TITLE_UPDATE_ACTION_KIND: "title_update";
export const TIMELINE_DISPLAY_TEXT_LIMITS: Record<string, number>;
export const TIMELINE_DISPLAY_TITLE_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_SUMMARY_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_DETAIL_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_INLINE_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_SHORT_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_TINY_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_ONE_LINE_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_ERROR_SUMMARY_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_TODO_ITEM_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_TODO_STEP_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_PATH_PREVIEW_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_ASK_USER_HEADER_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_ASK_USER_QUESTION_PREVIEW_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_ALLOWED_PROMPT_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_PLAN_TEXT_LIMIT: number;
/** @deprecated Use TIMELINE_DISPLAY_PLAN_TEXT_LIMIT. */
export const TIMELINE_DISPLAY_CLAUDE_PLAN_TEXT_LIMIT: number;
export const TIMELINE_DISPLAY_COMMAND_INLINE_THRESHOLD: number;
export const TIMELINE_DISPLAY_FILE_CHANGE_PATH_TEXT_LIMIT: number;
export const TIMELINE_PAYLOAD_MAX_DEPTH: number;
export const TIMELINE_PAYLOAD_RESERVED_KEYS: readonly string[];
export const AGENT_TIMELINE_TOOL_WINDOW_KINDS: readonly string[];

export function isAgentTimelineRunningStatus(status: string): boolean;
export function isAgentTimelineTerminalStatus(status: string): boolean;
export function normalizeTimelineStatus(status: string | null | undefined): string;

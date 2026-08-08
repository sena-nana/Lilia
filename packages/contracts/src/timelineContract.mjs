import timelineContract from "./timeline-contract.json" with { type: "json" };

const manifest = deepFreeze(timelineContract);

export const TIMELINE_CONTRACT = manifest;
export const AGENT_TIMELINE_RUNNING_STATUSES = manifest.agentTimelineRunningStatuses;
export const AGENT_TIMELINE_TERMINAL_STATUSES = manifest.agentTimelineTerminalStatuses;
export const TIMELINE_STATUS_ALIASES = manifest.timelineStatusAliases;
export const DEFAULT_TIMELINE_STATUS = manifest.defaultTimelineStatus;
export const AGENT_TIMELINE_ACTION_KIND_BY_EVENT_KIND =
  manifest.agentTimelineActionKindByEventKind;
export const TITLE_UPDATE_ACTION_KIND =
  manifest.agentTimelineActionKindByEventKind.title_update;
export const TIMELINE_DISPLAY_TEXT_LIMITS = manifest.displayTextLimits;
export const TIMELINE_DISPLAY_TITLE_TEXT_LIMIT = manifest.displayTextLimits.title;
export const TIMELINE_DISPLAY_SUMMARY_TEXT_LIMIT = manifest.displayTextLimits.summary;
export const TIMELINE_DISPLAY_DETAIL_TEXT_LIMIT = manifest.displayTextLimits.detail;
export const TIMELINE_DISPLAY_INLINE_TEXT_LIMIT = manifest.displayTextLimits.inline;
export const TIMELINE_DISPLAY_SHORT_TEXT_LIMIT = manifest.displayTextLimits.short;
export const TIMELINE_DISPLAY_TINY_TEXT_LIMIT = manifest.displayTextLimits.tiny;
export const TIMELINE_DISPLAY_ONE_LINE_TEXT_LIMIT = manifest.displayTextLimits.oneLine;
export const TIMELINE_DISPLAY_ERROR_SUMMARY_TEXT_LIMIT =
  manifest.displayTextLimits.errorSummary;
export const TIMELINE_DISPLAY_TODO_ITEM_TEXT_LIMIT = manifest.displayTextLimits.todoItem;
export const TIMELINE_DISPLAY_TODO_STEP_TEXT_LIMIT = manifest.displayTextLimits.todoStep;
export const TIMELINE_DISPLAY_PATH_PREVIEW_TEXT_LIMIT = manifest.displayTextLimits.pathPreview;
export const TIMELINE_DISPLAY_ASK_USER_HEADER_TEXT_LIMIT =
  manifest.displayTextLimits.askUserHeader;
export const TIMELINE_DISPLAY_ASK_USER_QUESTION_PREVIEW_TEXT_LIMIT =
  manifest.displayTextLimits.askUserQuestionPreview;
export const TIMELINE_DISPLAY_ALLOWED_PROMPT_TEXT_LIMIT =
  manifest.displayTextLimits.allowedPrompt;
export const TIMELINE_DISPLAY_PLAN_TEXT_LIMIT = manifest.displayTextLimits.planText;
/** @deprecated Use TIMELINE_DISPLAY_PLAN_TEXT_LIMIT. */
export const TIMELINE_DISPLAY_CLAUDE_PLAN_TEXT_LIMIT = TIMELINE_DISPLAY_PLAN_TEXT_LIMIT;
export const TIMELINE_DISPLAY_COMMAND_INLINE_THRESHOLD =
  manifest.displayTextLimits.commandInlineThreshold;
export const TIMELINE_DISPLAY_FILE_CHANGE_PATH_TEXT_LIMIT =
  manifest.displayTextLimits.fileChangePath;
export const TIMELINE_PAYLOAD_MAX_DEPTH = manifest.payload.maxDepth;
export const TIMELINE_PAYLOAD_RESERVED_KEYS = manifest.payload.reservedKeys;
export const AGENT_TIMELINE_TOOL_WINDOW_KINDS = manifest.agentTimelineToolWindowKinds;

const runningStatusSet = new Set(AGENT_TIMELINE_RUNNING_STATUSES);
const terminalStatusSet = new Set(AGENT_TIMELINE_TERMINAL_STATUSES);

export function isAgentTimelineRunningStatus(status) {
  return runningStatusSet.has(status);
}

export function isAgentTimelineTerminalStatus(status) {
  return terminalStatusSet.has(status);
}

export function normalizeTimelineStatus(status) {
  if (!status) return DEFAULT_TIMELINE_STATUS;
  return TIMELINE_STATUS_ALIASES[status] ?? status;
}

function deepFreeze(value) {
  if (!value || typeof value !== "object") return value;
  for (const child of Object.values(value)) {
    deepFreeze(child);
  }
  return Object.freeze(value);
}

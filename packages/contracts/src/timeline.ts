import {
  chatAttachmentsToPayload,
  conversationReferencesToPayload,
  readChatAttachmentPayloadPaths,
  readChatAttachments,
  readConversationReferences,
  type ChatAttachment,
  type ChatBackendKind,
  type ChatConversationReference,
  type ChatMessage,
  type LiliaBatchApplyInput,
  type LiliaThreadGoal,
} from "./chat";
import { projectArchitectureChangeTextFromPayload } from "./architecture";
import {
  AGENT_TIMELINE_PENDING_INTERACTIONS,
  ARCHITECTURE_INTERACTION_KIND,
  ASK_USER_INTERACTION_KIND,
  PLAN_APPROVAL_INTERACTION_KIND,
  type AgentTimelinePendingInteraction,
} from "./agent-interaction";
import {
  AGENT_TIMELINE_ACTION_KIND_BY_EVENT_KIND as AGENT_TIMELINE_ACTION_KIND_BY_EVENT_KIND_IMPL,
  AGENT_TIMELINE_RUNNING_STATUSES,
  AGENT_TIMELINE_TERMINAL_STATUSES,
  AGENT_TIMELINE_TOOL_WINDOW_KINDS,
  DEFAULT_TIMELINE_STATUS,
  isAgentTimelineRunningStatus as isAgentTimelineRunningStatusImpl,
  isAgentTimelineTerminalStatus as isAgentTimelineTerminalStatusImpl,
  normalizeTimelineStatus as normalizeTimelineStatusImpl,
  TIMELINE_DISPLAY_ALLOWED_PROMPT_TEXT_LIMIT,
  TIMELINE_DISPLAY_ASK_USER_HEADER_TEXT_LIMIT,
  TIMELINE_DISPLAY_ASK_USER_QUESTION_PREVIEW_TEXT_LIMIT,
  TIMELINE_DISPLAY_PLAN_TEXT_LIMIT,
  TIMELINE_DISPLAY_CLAUDE_PLAN_TEXT_LIMIT,
  TIMELINE_DISPLAY_COMMAND_INLINE_THRESHOLD,
  TIMELINE_DISPLAY_DETAIL_TEXT_LIMIT,
  TIMELINE_DISPLAY_ERROR_SUMMARY_TEXT_LIMIT,
  TIMELINE_DISPLAY_FILE_CHANGE_PATH_TEXT_LIMIT,
  TIMELINE_DISPLAY_INLINE_TEXT_LIMIT,
  TIMELINE_DISPLAY_ONE_LINE_TEXT_LIMIT,
  TIMELINE_DISPLAY_PATH_PREVIEW_TEXT_LIMIT,
  TIMELINE_DISPLAY_SHORT_TEXT_LIMIT,
  TIMELINE_DISPLAY_SUMMARY_TEXT_LIMIT,
  TIMELINE_DISPLAY_TEXT_LIMITS,
  TIMELINE_DISPLAY_TINY_TEXT_LIMIT,
  TIMELINE_DISPLAY_TITLE_TEXT_LIMIT,
  TIMELINE_DISPLAY_TODO_ITEM_TEXT_LIMIT,
  TIMELINE_DISPLAY_TODO_STEP_TEXT_LIMIT,
  TIMELINE_PAYLOAD_MAX_DEPTH,
  TIMELINE_PAYLOAD_RESERVED_KEYS,
  TIMELINE_STATUS_ALIASES,
  TITLE_UPDATE_ACTION_KIND,
} from "./timelineContract.mjs";

export {
  AGENT_TIMELINE_PENDING_INTERACTIONS,
  type AgentTimelinePendingInteraction,
} from "./agent-interaction";

export {
  AGENT_TIMELINE_RUNNING_STATUSES,
  AGENT_TIMELINE_TERMINAL_STATUSES,
  AGENT_TIMELINE_TOOL_WINDOW_KINDS,
  DEFAULT_TIMELINE_STATUS,
  TIMELINE_DISPLAY_ALLOWED_PROMPT_TEXT_LIMIT,
  TIMELINE_DISPLAY_ASK_USER_HEADER_TEXT_LIMIT,
  TIMELINE_DISPLAY_ASK_USER_QUESTION_PREVIEW_TEXT_LIMIT,
  TIMELINE_DISPLAY_PLAN_TEXT_LIMIT,
  TIMELINE_DISPLAY_CLAUDE_PLAN_TEXT_LIMIT,
  TIMELINE_DISPLAY_COMMAND_INLINE_THRESHOLD,
  TIMELINE_DISPLAY_DETAIL_TEXT_LIMIT,
  TIMELINE_DISPLAY_ERROR_SUMMARY_TEXT_LIMIT,
  TIMELINE_DISPLAY_FILE_CHANGE_PATH_TEXT_LIMIT,
  TIMELINE_DISPLAY_INLINE_TEXT_LIMIT,
  TIMELINE_DISPLAY_ONE_LINE_TEXT_LIMIT,
  TIMELINE_DISPLAY_PATH_PREVIEW_TEXT_LIMIT,
  TIMELINE_DISPLAY_SHORT_TEXT_LIMIT,
  TIMELINE_DISPLAY_SUMMARY_TEXT_LIMIT,
  TIMELINE_DISPLAY_TEXT_LIMITS,
  TIMELINE_DISPLAY_TINY_TEXT_LIMIT,
  TIMELINE_DISPLAY_TITLE_TEXT_LIMIT,
  TIMELINE_DISPLAY_TODO_ITEM_TEXT_LIMIT,
  TIMELINE_DISPLAY_TODO_STEP_TEXT_LIMIT,
  TIMELINE_PAYLOAD_MAX_DEPTH,
  TIMELINE_PAYLOAD_RESERVED_KEYS,
  TIMELINE_STATUS_ALIASES,
  TITLE_UPDATE_ACTION_KIND,
};

export type AgentTimelineKnownEventKind =
  | "message"
  | "reasoning"
  | "plan"
  | "goal"
  | "todo_list"
  | "tool"
  | typeof ASK_USER_INTERACTION_KIND
  | typeof ARCHITECTURE_INTERACTION_KIND
  | "command"
  | "subagent"
  | "file_change"
  | "file_read"
  | "search"
  | "web_fetch"
  | "mcp"
  | "web_search"
  | "diagnostic"
  | typeof TITLE_UPDATE_ACTION_KIND
  | "error"
  | "turn";

export type AgentTimelineEventKind = AgentTimelineKnownEventKind | (string & {});

export type AgentTimelineEventStatus =
  | "pending"
  | "started"
  | "running"
  | "in_progress"
  | "completed"
  | "done"
  | "success"
  | "failed"
  | "error"
  | "cancelled"
  | "skipped"
  | "info"
  | "requires_action";

export type AgentTimelinePayload =
  | null
  | boolean
  | number
  | string
  | AgentTimelinePayload[]
  | { [key: string]: AgentTimelinePayload };

export type AgentTimelineDisplayIcon = string;

export type AgentTimelineDisplayBucket =
  | "command"
  | "file"
  | "plan"
  | "goal"
  | "todo"
  | "tool"
  | typeof ASK_USER_INTERACTION_KIND
  | "architecture"
  | "mcp"
  | "search"
  | "web_search"
  | "subagent"
  | "diagnostic"
  | "error"
  | "other"
  | (string & {});

export interface AgentTimelineDisplayGroup {
  key: string;
  bucket?: AgentTimelineDisplayBucket | null;
  unit?: string | null;
  count?: number | null;
}

export interface AgentTimelineDisplayField {
  label: string;
  value: string;
}

export interface AgentTimelineDisplayListItem {
  text: string;
  tone?: "default" | "muted" | "success" | "warning" | "error" | null;
}

export type AgentTimelineDisplayDetail =
  | {
      type: "line";
      text: string;
      tone?: "default" | "muted" | null;
    }
  | {
      type: "fields";
      fields: AgentTimelineDisplayField[];
    }
  | {
      type: "code";
      label?: string | null;
      content: string;
      language?: string | null;
    }
  | {
      type: "markdown";
      content: string;
      tone?: "default" | "muted" | null;
      singleLine?: boolean | null;
    }
  | {
      type: "list";
      items: AgentTimelineDisplayListItem[];
      ordered?: boolean | null;
    };

export interface AgentTimelineDisplay {
  icon?: AgentTimelineDisplayIcon | null;
  label?: string | null;
  action?: string | null;
  object?: string | null;
  objectInLabel?: boolean | null;
  preview?: string | null;
  details?: AgentTimelineDisplayDetail[] | null;
  group?: AgentTimelineDisplayGroup | null;
  defaultExpanded?: boolean | null;
}

export interface AgentTimelineEvent {
  id: string;
  taskId: string;
  turnId: string | null;
  backend: ChatBackendKind;
  kind: AgentTimelineEventKind;
  status: AgentTimelineEventStatus;
  title: string;
  summary: string | null;
  payload: AgentTimelinePayload;
  createdAt: number;
  updatedAt: number;
  turnSeq: number;
  intraTurnOrder: number;
}

export function timelineEventScopedKey(
  event: Pick<AgentTimelineEvent, "taskId" | "id">,
): string {
  return `${event.taskId}:${event.id}`;
}

export type TimelineChatMessage = ChatMessage & { queued?: boolean };

export function timelineChatMessageFromEvent(event: AgentTimelineEvent): TimelineChatMessage {
  const payload = agentTimelinePayloadRecord(event) ?? {};
  const role = payload.role === "system" ? "system" : "user";
  const content = typeof payload.content === "string"
    ? payload.content
    : event.summary ?? "";
  return {
    id: event.id,
    taskId: event.taskId,
    role,
    content,
    attachments: readChatAttachments(payload.attachments),
    conversationReferences: readConversationReferences(payload.conversationReferences),
    createdAt: event.createdAt,
    queued: payload.queued === true,
  };
}

export interface TimelineRetryContext {
  content: string;
  attachments: ChatAttachment[];
  conversationReferences?: ChatConversationReference[];
}

export interface CreateMessageTimelineEventInput {
  id: string;
  taskId: string;
  backend: ChatBackendKind;
  content: string;
  attachments?: ChatAttachment[];
  conversationReferences?: ChatConversationReference[];
  createdAt: number;
  queued?: boolean;
}

export interface CreateErrorTimelineEventInput {
  id: string;
  taskId: string;
  backend: ChatBackendKind;
  message: string;
  createdAt: number;
  retryContext?: TimelineRetryContext;
}

export interface AgentTimelineBatchEvent {
  taskId: string;
  events: AgentTimelineEvent[];
}

export type AgentTimelineActionKind =
  | typeof TITLE_UPDATE_ACTION_KIND
  | typeof PLAN_APPROVAL_INTERACTION_KIND
  | typeof ASK_USER_INTERACTION_KIND
  | typeof ARCHITECTURE_INTERACTION_KIND
  | AgentTimelinePendingInteraction;

export interface AgentTimelineActionMatchCandidate {
  kind: AgentTimelineActionKind;
  requestId?: string | null;
  turnId?: string | null;
}

export interface AgentTimelineActionDescriptor {
  kind: AgentTimelineActionKind;
  requestId: string;
}

export const AGENT_TIMELINE_ACTION_KIND_BY_EVENT_KIND =
  AGENT_TIMELINE_ACTION_KIND_BY_EVENT_KIND_IMPL as Partial<
    Record<AgentTimelineEventKind, AgentTimelineActionKind>
  >;

const AGENT_TIMELINE_PENDING_INTERACTION_SET = new Set<string>(
  AGENT_TIMELINE_PENDING_INTERACTIONS,
);

export interface TitleUpdateTimelinePayload {
  requestId: string;
  proposedTitle: string;
  previousTitle: string | null;
}

export interface AgentTimelinePlanPromptRow {
  index: number;
  text: string;
}

export interface AgentTimelinePlanArchitectureImpactRow {
  impactIndex: number;
  rowIndex: number;
  text: string;
}

export type TimelinePlanStatusKind =
  | "pending"
  | "revision"
  | "approved"
  | "rejected"
  | "cancelled"
  | "expired"
  | "neutral";

export const TIMELINE_PLAN_STATUS_LABELS: Record<TimelinePlanStatusKind, string> = {
  pending: "待确认",
  revision: "修改要求",
  approved: "已同意",
  rejected: "已拒绝",
  cancelled: "已取消",
  expired: "已失效",
  neutral: "计划",
};

export function agentTimelinePayloadValueRecord(
  value: AgentTimelinePayload | unknown,
): Record<string, AgentTimelinePayload> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, AgentTimelinePayload>
    : null;
}

export function agentTimelinePayloadRecord(
  event: Pick<AgentTimelineEvent, "payload">,
): Record<string, AgentTimelinePayload> | null {
  return agentTimelinePayloadValueRecord(event.payload);
}

export function readTimelineEventPayloadRecord(
  event: Pick<AgentTimelineEvent, "payload">,
): Record<string, AgentTimelinePayload> {
  return agentTimelinePayloadRecord(event) ?? {};
}

export function createMessageTimelineEvent(
  input: CreateMessageTimelineEventInput,
): AgentTimelineEvent {
  return {
    id: input.id,
    taskId: input.taskId,
    turnId: null,
    backend: input.backend,
    kind: "message",
    status: input.queued ? "pending" : "success",
    title: "用户输入",
    summary: input.content,
    payload: {
      role: "user",
      content: input.content,
      attachments: chatAttachmentsToPayload(input.attachments ?? []),
      conversationReferences: conversationReferencesToPayload(input.conversationReferences ?? []),
      queued: input.queued === true,
    },
    createdAt: input.createdAt,
    updatedAt: input.createdAt,
    turnSeq: Number.MAX_SAFE_INTEGER,
    intraTurnOrder: 0,
  };
}

export function createErrorTimelineEvent(
  input: CreateErrorTimelineEventInput,
): AgentTimelineEvent {
  const payload: Record<string, AgentTimelinePayload> = {
    message: input.message,
  };
  if (input.retryContext) {
    payload.retryContext = {
      content: input.retryContext.content,
      attachments: chatAttachmentsToPayload(input.retryContext.attachments),
      conversationReferences: conversationReferencesToPayload(
        input.retryContext.conversationReferences ?? [],
      ),
    };
  }
  return {
    id: input.id,
    taskId: input.taskId,
    turnId: null,
    backend: input.backend,
    kind: "error",
    status: "error",
    title: "错误",
    summary: input.message,
    payload,
    createdAt: input.createdAt,
    updatedAt: input.createdAt,
    turnSeq: Number.MAX_SAFE_INTEGER,
    intraTurnOrder: Number.MAX_SAFE_INTEGER,
  };
}

export function compareTimelineEvents(a: AgentTimelineEvent, b: AgentTimelineEvent): number {
  return a.turnSeq - b.turnSeq ||
    a.intraTurnOrder - b.intraTurnOrder ||
    a.createdAt - b.createdAt ||
    a.id.localeCompare(b.id);
}

export function upsertTimelineEventById(
  events: readonly AgentTimelineEvent[],
  event: AgentTimelineEvent,
): AgentTimelineEvent[] {
  const existingIndex = events.findIndex((item) => item.id === event.id);
  if (existingIndex < 0) return [...events, event];
  const next = events.slice();
  next[existingIndex] = event;
  return next;
}

export function upsertTimelineEventsById(
  events: readonly AgentTimelineEvent[],
  nextEvents: readonly AgentTimelineEvent[],
): AgentTimelineEvent[] {
  if (nextEvents.length === 0) return [...events];
  const byId = new Map<string, AgentTimelineEvent>();
  for (const event of events) byId.set(event.id, event);
  for (const event of nextEvents) byId.set(event.id, event);
  return [...byId.values()].sort(compareTimelineEvents);
}

export function mergeTimelineEvents(
  events: readonly AgentTimelineEvent[],
  current: readonly AgentTimelineEvent[],
): AgentTimelineEvent[] {
  const byId = new Map<string, AgentTimelineEvent>();
  for (const event of events) byId.set(event.id, event);
  for (const event of current) {
    if (!byId.has(event.id)) byId.set(event.id, event);
  }
  return [...byId.values()].sort(compareTimelineEvents);
}

export function mergeLoadedTimelineEvents(
  loaded: readonly AgentTimelineEvent[],
  current: readonly AgentTimelineEvent[],
  preserveEventIds: ReadonlySet<string> = new Set(),
): AgentTimelineEvent[] {
  const loadedKeys = new Set(
    loaded
      .filter(isTimelineUserMessage)
      .map(userMessageIdentityKey),
  );
  const preservedEvents = current.filter((event) =>
    preserveEventIds.has(event.id) ||
    (isQueuedUserMessageEvent(event) && !loadedKeys.has(userMessageIdentityKey(event)))
  );
  return upsertTimelineEventsById(loaded, preservedEvents);
}

export function markFirstQueuedUserMessageSuccessful(
  events: readonly AgentTimelineEvent[],
  now = Date.now(),
): AgentTimelineEvent[] {
  let cleared = false;
  return events.map((event) => {
    const payload = readTimelineEventPayloadRecord(event);
    if (!cleared && event.kind === "message" && payload.queued === true) {
      cleared = true;
      return {
        ...event,
        status: "success",
        payload: { ...payload, queued: false },
        updatedAt: now,
      };
    }
    return event;
  });
}

export function readTimelineRetryContext(value: unknown): TimelineRetryContext | null {
  const payload = agentTimelinePayloadValueRecord(value) ?? {};
  const content = typeof payload.content === "string" ? payload.content : "";
  const attachments = readChatAttachments(payload.attachments);
  const conversationReferences = readConversationReferences(payload.conversationReferences);
  if (!content.trim() && attachments.length === 0 && conversationReferences.length === 0) return null;
  return { content, attachments, conversationReferences };
}

export function retryContextForTimelineEvent(
  event: AgentTimelineEvent,
  timelineEvents: readonly AgentTimelineEvent[],
): TimelineRetryContext | null {
  if (event.kind !== "error") return null;
  const payload = readTimelineEventPayloadRecord(event);
  const embedded = readTimelineRetryContext(payload.retryContext);
  if (embedded) return embedded;
  if (!event.turnId) return null;
  const source = timelineEvents.find((candidate) => {
    if (candidate.kind !== "message" || candidate.turnId !== event.turnId) return false;
    return readTimelineEventPayloadRecord(candidate).role === "user";
  });
  if (!source) return null;
  const sourcePayload = readTimelineEventPayloadRecord(source);
  return readTimelineRetryContext({
    content: typeof sourcePayload.content === "string" ? sourcePayload.content : source.summary ?? "",
    attachments: sourcePayload.attachments,
    conversationReferences: sourcePayload.conversationReferences,
  });
}

export function isTimelineUserMessage(
  event: Pick<AgentTimelineEvent, "kind" | "payload">,
): boolean {
  if (!isTimelineMessageEvent(event)) return false;
  const payload = readTimelineEventPayloadRecord(event);
  return payload.role === "user" || payload.role === "system";
}

function isQueuedUserMessageEvent(event: AgentTimelineEvent): boolean {
  return isTimelineUserMessage(event) &&
    readTimelineEventPayloadRecord(event).queued === true;
}

function userMessageIdentityKey(event: AgentTimelineEvent): string {
  const payload = readTimelineEventPayloadRecord(event);
  const content = typeof payload.content === "string" ? payload.content : event.summary ?? "";
  const attachments = readChatAttachmentPayloadPaths(payload.attachments).join("\u001f");
  const conversationReferences = readConversationReferences(payload.conversationReferences)
    .map((reference) => reference.taskId)
    .join("\u001f");
  return `${payload.role ?? "user"}\u001f${content}\u001f${attachments}\u001f${conversationReferences}`;
}

export const isAgentTimelineRunningStatus = isAgentTimelineRunningStatusImpl as (
  status: AgentTimelineEventStatus,
) => boolean;

export const isAgentTimelineTerminalStatus = isAgentTimelineTerminalStatusImpl as (
  status: AgentTimelineEventStatus,
) => boolean;

export const normalizeTimelineStatus = normalizeTimelineStatusImpl as (
  status: AgentTimelineEventStatus | string | null | undefined,
) => AgentTimelineEventStatus;

export function isTimelineInterruptEvent(
  event: Pick<AgentTimelineEvent, "kind" | "payload">,
): boolean {
  return event.kind === "error" && readTimelineEventPayloadRecord(event).interrupted === true;
}

export function isTimelineErrorReply(
  event: Pick<AgentTimelineEvent, "kind" | "status" | "payload">,
): boolean {
  if (event.kind !== "error") return false;
  if (isTimelineInterruptEvent(event)) return false;
  return event.status === "error" || event.status === "failed";
}

export function isTimelineFinalReply(
  event: Pick<AgentTimelineEvent, "kind" | "status" | "payload">,
): boolean {
  if (isTimelineErrorReply(event)) return true;
  if (!isTimelineMessageEvent(event)) return false;
  return readTimelineEventPayloadRecord(event).role === "assistant";
}

export function isTimelineProcessAnchor(
  event: Pick<AgentTimelineEvent, "kind" | "status" | "payload">,
): boolean {
  return isTimelineFinalReply(event) || isTimelineInterruptEvent(event);
}

export function isHiddenTimelineEvent(
  event: Pick<AgentTimelineEvent, "kind">,
): boolean {
  return event.kind === "turn" || event.kind === "reasoning";
}

export function isTimelineMessageEvent(
  event: Pick<AgentTimelineEvent, "kind">,
): boolean {
  return event.kind === "message";
}

export function isTimelineFinalReplyStreaming(
  event: Pick<AgentTimelineEvent, "kind" | "payload" | "status">,
): boolean {
  return !isTimelineErrorReply(event) &&
    isTimelineFinalReply(event) &&
    isAgentTimelineRunningStatus(event.status);
}

export function timelineErrorReplyText(
  event: Pick<AgentTimelineEvent, "title" | "summary" | "payload">,
): string {
  const summary = event.summary?.trim() ?? "";
  if (summary) return summary;
  const payload = readTimelineEventPayloadRecord(event);
  for (const key of ["message", "error", "reason", "details", "stderr"]) {
    const value = payload[key];
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return event.title.trim();
}

export function readTimelineFinalText(
  event: Pick<AgentTimelineEvent, "kind" | "status" | "title" | "summary" | "payload">,
): string {
  if (isTimelineErrorReply(event)) return timelineErrorReplyText(event);
  if (!isTimelineMessageEvent(event)) return "";
  const payload = readTimelineEventPayloadRecord(event);
  if (payload.role !== "assistant") return "";
  const content = payload.content;
  return typeof content === "string" ? content : "";
}

export function aggregateTimelineStatus(
  events: ReadonlyArray<Pick<AgentTimelineEvent, "status">>,
): AgentTimelineEventStatus {
  if (events.some((event) =>
    event.status === "failed" || event.status === "error" || event.status === "cancelled"
  )) {
    return "failed";
  }
  if (events.some((event) => isAgentTimelineRunningStatus(event.status))) return "running";
  return "completed";
}

export function readAgentTimelinePayloadString(
  event: AgentTimelineEvent,
  key: string,
): string | null {
  const value = agentTimelinePayloadRecord(event)?.[key];
  return typeof value === "string" ? value : null;
}

function payloadString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function compactPayloadLine(value: unknown, max: number): string {
  const text = payloadString(value).replace(/\s+/g, " ").trim();
  return text.length > max ? `${text.slice(0, max)}...` : text;
}

export function timelinePlanStatusKind(
  event: Pick<AgentTimelineEvent, "status" | "payload">,
  actionExpired = false,
): TimelinePlanStatusKind {
  if (actionExpired) return "expired";
  const payload = readTimelineEventPayloadRecord(event);
  if (payloadString(payload.revisionRequest)) return "revision";
  if (payload.approved === null) return "pending";
  if (payload.approved === true) return "approved";
  if (payload.approved === false) return "rejected";
  if (event.status === "requires_action") return "pending";
  if (event.status === "cancelled") return "cancelled";
  return "neutral";
}

export function timelinePlanStatusLabel(kind: TimelinePlanStatusKind): string {
  return TIMELINE_PLAN_STATUS_LABELS[kind];
}

export function agentTimelinePlanAllowedPromptRows(
  payload: Record<string, unknown> | null | undefined,
): AgentTimelinePlanPromptRow[] {
  const prompts = payload?.allowedPrompts;
  if (!Array.isArray(prompts)) return [];
  return prompts.flatMap((item, index) => {
    const prompt = agentTimelinePayloadValueRecord(item);
    if (!prompt) return [];
    const tool = compactPayloadLine(prompt.tool, 80);
    const text = [
      tool,
      compactPayloadLine(prompt.prompt, 400),
    ].filter(Boolean).join("：");
    return text ? [{ index, text }] : [];
  });
}

export function agentTimelinePlanArchitectureImpactRows(
  payload: Record<string, unknown> | null | undefined,
): AgentTimelinePlanArchitectureImpactRow[] {
  const impacts = payload?.architectureImpacts;
  if (!Array.isArray(impacts)) return [];
  return impacts.flatMap((impact, impactIndex) => {
    const impactRecord = agentTimelinePayloadValueRecord(impact);
    if (!impactRecord) return [];
    const reason = compactPayloadLine(impactRecord.reason, 240);
    const changes = Array.isArray(impactRecord.changes) ? impactRecord.changes : [];
    const changeRows = changes
      .map((change) =>
        compactPayloadLine(projectArchitectureChangeTextFromPayload(change), 240)
      )
      .filter(Boolean);
    const rows = reason ? [reason, ...changeRows] : changeRows;
    return rows.map((text, rowIndex) => ({ impactIndex, rowIndex, text }));
  });
}

export function isAgentTimelinePendingInteraction(
  value: unknown,
): value is AgentTimelinePendingInteraction {
  return typeof value === "string" && AGENT_TIMELINE_PENDING_INTERACTION_SET.has(value);
}

export function agentTimelinePendingInteraction(
  event: AgentTimelineEvent,
): AgentTimelinePendingInteraction | null {
  const interaction = readAgentTimelinePayloadString(event, "interaction");
  return isAgentTimelinePendingInteraction(interaction) ? interaction : null;
}

export function titleUpdateTimelinePayload(
  event: AgentTimelineEvent,
): TitleUpdateTimelinePayload | null {
  if (event.kind !== TITLE_UPDATE_ACTION_KIND || event.status !== "requires_action") return null;
  const requestId = readAgentTimelinePayloadString(event, "requestId");
  const proposedTitle = readAgentTimelinePayloadString(event, "proposedTitle");
  if (!requestId || !proposedTitle) return null;
  return {
    requestId,
    proposedTitle,
    previousTitle: readAgentTimelinePayloadString(event, "previousTitle"),
  };
}

export function agentTimelineEventRequiresAction(event: AgentTimelineEvent): boolean {
  return agentTimelineActionKind(event) !== null;
}

export function agentTimelineActionKind(event: AgentTimelineEvent): AgentTimelineActionKind | null {
  if (event.status !== "requires_action") return null;
  const eventActionKind = AGENT_TIMELINE_ACTION_KIND_BY_EVENT_KIND[event.kind];
  if (eventActionKind === TITLE_UPDATE_ACTION_KIND) {
    return titleUpdateTimelinePayload(event) ? eventActionKind : null;
  }
  if (eventActionKind) return eventActionKind;
  return agentTimelinePendingInteraction(event);
}

export function agentTimelineEventRequestId(
  event: AgentTimelineEvent,
  rememberedRequestId?: string | null,
): string {
  return readAgentTimelinePayloadString(event, "requestId") ??
    rememberedRequestId ??
    `timeline:${event.id}`;
}

export function agentTimelineActionRequestId(event: AgentTimelineEvent): string | null {
  return agentTimelineActionDescriptor(event)?.requestId ?? null;
}

export function agentTimelineActionDescriptor(
  event: AgentTimelineEvent,
): AgentTimelineActionDescriptor | null {
  const kind = agentTimelineActionKind(event);
  if (!kind) return null;
  const requestId = readAgentTimelinePayloadString(event, "requestId");
  return requestId ? { kind, requestId } : null;
}

export function agentTimelineActionRequestIds(events: readonly AgentTimelineEvent[]): string[] {
  const requestIds = new Set<string>();
  for (const event of events) {
    const action = agentTimelineActionDescriptor(event);
    if (action) requestIds.add(action.requestId);
  }
  return [...requestIds];
}

export function agentTimelineActionMatches(
  event: AgentTimelineEvent,
  candidate: AgentTimelineActionMatchCandidate,
): boolean {
  const eventActionKind = agentTimelineActionKind(event);
  if (!eventActionKind || candidate.kind !== eventActionKind) return false;
  const eventRequestId = readAgentTimelinePayloadString(event, "requestId");
  if (eventRequestId) return !!candidate.requestId && eventRequestId === candidate.requestId;
  if (
    candidate.kind === PLAN_APPROVAL_INTERACTION_KIND &&
    candidate.turnId &&
    event.turnId === candidate.turnId
  ) {
    return true;
  }
  return false;
}

export function timelineFinalReplyBatchApplyInput(
  event: AgentTimelineEvent,
  sourceSummary: string,
): LiliaBatchApplyInput | null {
  if (event.status !== "success") return null;
  if (typeof event.turnId !== "string" || event.turnId.length === 0) return null;
  if (!sourceSummary.trim()) return null;
  const payload = agentTimelinePayloadRecord(event);
  const workflowSource = agentTimelinePayloadValueRecord(payload?.workflowSource);
  const sourceKind = workflowSource?.sourceKind;
  if (sourceKind !== "review" && sourceKind !== "fix_suggestion") return null;
  return {
    sourceTurnId: event.turnId,
    sourceKind,
    sourceSummary,
  };
}

export function latestLiliaGoalFromTimeline(
  events: readonly Pick<AgentTimelineEvent, "kind" | "payload" | "updatedAt">[],
): LiliaThreadGoal | null {
  let latest: Pick<AgentTimelineEvent, "payload" | "updatedAt"> | null = null;
  for (const event of events) {
    if (event.kind !== "goal") continue;
    if (!latest || event.updatedAt >= latest.updatedAt) latest = event;
  }
  if (!latest) return null;
  const payload = agentTimelinePayloadValueRecord(latest.payload);
  if (payload?.cleared === true) return null;
  const goal = payload?.goal;
  return goal && typeof goal === "object" && !Array.isArray(goal)
    ? goal as unknown as LiliaThreadGoal
    : null;
}

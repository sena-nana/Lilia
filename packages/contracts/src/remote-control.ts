import type { AgentInteractionRequest, AgentInteractionResponse } from "./agent-interaction";
import type {
  ChatAttachment,
  ChatComposerState,
  ChatConversationReference,
  ChatInterruptResult,
  ChatRuntimeCommand,
  ChatRuntimeSnapshot,
  ChatSendResult,
  ChatWorkflow,
  ProviderRuntimeOptions,
  SessionForkMode,
} from "./chat";
import type { Task, SidebarConversationSummary } from "./task";
import type { AgentTimelineEvent } from "./timeline";
import {
  REMOTE_CONTROL_CANCEL_PAIRING_COMMAND,
  REMOTE_CONTROL_DISPATCH_REQUEST_COMMAND,
  REMOTE_CONTROL_PAIR_DEVICE_COMMAND,
  REMOTE_CONTROL_REVOKE_DEVICE_COMMAND,
  REMOTE_CONTROL_SET_KEEP_AWAKE_ENABLED_COMMAND,
  REMOTE_CONTROL_SET_HOST_ENABLED_COMMAND,
  REMOTE_CONTROL_SET_PC_NAME_COMMAND,
  REMOTE_CONTROL_START_PAIRING_COMMAND,
  REMOTE_CONTROL_STATUS_COMMAND,
} from "./remoteControlCommandsContract.mjs";

export const REMOTE_CONTROL_PROTOCOL_VERSION = 1;
export const REMOTE_CONTROL_ALPN = "lilia.remote-control.v1";

export {
  REMOTE_CONTROL_CANCEL_PAIRING_COMMAND,
  REMOTE_CONTROL_DISPATCH_REQUEST_COMMAND,
  REMOTE_CONTROL_PAIR_DEVICE_COMMAND,
  REMOTE_CONTROL_REVOKE_DEVICE_COMMAND,
  REMOTE_CONTROL_SET_KEEP_AWAKE_ENABLED_COMMAND,
  REMOTE_CONTROL_SET_HOST_ENABLED_COMMAND,
  REMOTE_CONTROL_SET_PC_NAME_COMMAND,
  REMOTE_CONTROL_START_PAIRING_COMMAND,
  REMOTE_CONTROL_STATUS_COMMAND,
};

export type RemotePeerKind = "pc" | "android";
export type RemoteConnectionState =
  | "disabled"
  | "pairing"
  | "listening"
  | "connected"
  | "disconnected";

export type RemoteErrorCode =
  | "unauthenticated"
  | "unauthorized"
  | "unsupported"
  | "conflict"
  | "unavailable"
  | "transportClosed"
  | "invalidRequest"
  | "internal";

export interface RemoteEndpointAddress {
  endpointId: string;
  relayUrl?: string | null;
  directAddresses?: string[];
}

export interface RemotePeerSummary {
  id: string;
  kind: RemotePeerKind;
  displayName: string;
  endpointId: string;
  protocolVersion: number;
  trusted: boolean;
  firstPairedAt: number;
  lastSeenAt: number | null;
  revokedAt?: number | null;
}

export interface RemotePairingTicket {
  id: string;
  pcName: string;
  pcEndpoint: RemoteEndpointAddress;
  protocolVersion: number;
  challenge: string;
  expiresAt: number;
  pairingUri: string;
  bridgeUrl?: string | null;
}

export interface RemoteCapabilitySet {
  protocolVersion: number;
  minProtocolVersion: number;
  alpn: string;
  supportsPairing: boolean;
  supportsTaskInbox: boolean;
  supportsTimelineSubscription: boolean;
  supportsTimelinePagination: boolean;
  supportsChatSend: boolean;
  supportsInteractionResponse: boolean;
  supportsInterrupt: boolean;
  supportsAgentWire?: boolean;
  supportsSessionFork?: boolean;
  supportsProcessSession?: boolean;
}

export interface RemoteSessionForkResult {
  sessionId: string;
  sourceTurnId: string;
  mode: SessionForkMode;
}

export interface RemoteError {
  code: RemoteErrorCode;
  message: string;
  retryable?: boolean;
  details?: unknown;
}

export interface RemoteControlStatus {
  hostEnabled: boolean;
  state: RemoteConnectionState;
  pcName: string;
  keepAwakeEnabled: boolean;
  endpoint: RemoteEndpointAddress | null;
  activeTicket: RemotePairingTicket | null;
  trustedDevices: RemotePeerSummary[];
  capabilities: RemoteCapabilitySet;
}

export interface RemotePairDeviceInput {
  ticketId: string;
  challenge: string;
  deviceName: string;
  androidEndpoint: RemoteEndpointAddress;
  protocolVersion: number;
}

export interface RemoteHandshakeRequest {
  type: "connection.handshake";
  deviceName: string;
  androidEndpoint: RemoteEndpointAddress;
  protocolVersion: number;
  ticketId?: string;
  challenge?: string;
}

export interface RemoteReadCapabilitiesRequest {
  type: "connection.capabilities.read";
}

export interface RemoteResumeRequest {
  type: "connection.resume";
  androidEndpointId: string;
}

export interface RemoteListTasksRequest {
  type: "tasks.list";
  limit?: number;
}

export interface RemoteGetTaskRequest {
  type: "tasks.get";
  taskId: string;
}

export interface RemoteSubscribeTimelineRequest {
  type: "timeline.subscribe";
  taskId: string;
  afterEventId?: string | null;
  afterCursor?: string | null;
  limit?: number;
}

export interface RemoteTimelineSnapshotRequest {
  type: "timeline.snapshot";
  taskId: string;
  limit?: number;
  direction?: "latest" | "before";
  cursor?: string | null;
}

export interface RemoteTimelinePage {
  beforeCursor: string | null;
  afterCursor: string | null;
  hasMoreBefore: boolean;
  hasMoreAfter: boolean;
}

export interface RemoteSendChatRequest {
  type: "chat.send";
  taskId: string;
  content: string;
  composer?: ChatComposerState;
  projectCwd?: string;
  attachments?: ChatAttachment[];
  conversationReferences?: ChatConversationReference[];
  guideId?: string | null;
  workflow?: ChatWorkflow | null;
  runtimeCommand?: ChatRuntimeCommand | null;
  runtimeOptions?: ProviderRuntimeOptions | null;
}

export interface RemoteInterruptChatRequest {
  type: "chat.interrupt";
  taskId: string;
}

export interface RemoteRetryChatRequest {
  type: "chat.retry";
  taskId: string;
  eventId?: string | null;
}

export interface RemoteGetPendingInteractionRequest {
  type: "interaction.pending.read";
  taskId?: string | null;
}

export interface RemoteRespondInteractionRequest {
  type: "interaction.respond";
  response: AgentInteractionResponse;
}

export interface RemoteReadProviderStatusRequest {
  type: "provider.status.read";
}

export type RemoteRequest =
  | RemoteHandshakeRequest
  | RemoteReadCapabilitiesRequest
  | RemoteResumeRequest
  | RemoteListTasksRequest
  | RemoteGetTaskRequest
  | RemoteSubscribeTimelineRequest
  | RemoteTimelineSnapshotRequest
  | RemoteSendChatRequest
  | RemoteInterruptChatRequest
  | RemoteRetryChatRequest
  | RemoteGetPendingInteractionRequest
  | RemoteRespondInteractionRequest
  | RemoteReadProviderStatusRequest;

export interface RemoteRequestEnvelope {
  id: string;
  protocolVersion: number;
  sentAt: number;
  deviceId: string;
  request: RemoteRequest;
}

export type RemoteResponsePayload =
  | { type: "connection.handshake"; peer: RemotePeerSummary; capabilities: RemoteCapabilitySet }
  | { type: "connection.capabilities"; capabilities: RemoteCapabilitySet }
  | { type: "connection.resume"; accepted: boolean; peer: RemotePeerSummary | null }
  | { type: "tasks.list"; tasks: SidebarConversationSummary[] }
  | { type: "tasks.get"; task: Task | null; runtime: ChatRuntimeSnapshot | null }
  | { type: "timeline.snapshot"; taskId: string; events: AgentTimelineEvent[]; page?: RemoteTimelinePage }
  | { type: "timeline.subscribe"; taskId: string; events: AgentTimelineEvent[]; page?: RemoteTimelinePage }
  | { type: "chat.send"; result: ChatSendResult; sessionFork?: RemoteSessionForkResult | null }
  | { type: "chat.interrupt"; result: ChatInterruptResult }
  | { type: "chat.retry"; result: ChatSendResult }
  | { type: "interaction.pending"; interactions: AgentInteractionRequest[] }
  | { type: "interaction.respond"; accepted: true }
  | { type: "provider.status"; backend: string; ready: boolean; report?: unknown };

export interface RemoteResponseEnvelope {
  id: string;
  requestId: string;
  protocolVersion: number;
  sentAt: number;
  ok: boolean;
  payload?: RemoteResponsePayload;
  error?: RemoteError;
}

export type RemoteEventPayload =
  | { type: "connection.state"; state: RemoteConnectionState }
  | { type: "tasks.changed"; tasks: SidebarConversationSummary[] }
  | { type: "task.snapshot"; task: Task | null; runtime: ChatRuntimeSnapshot | null }
  | { type: "timeline.batch"; taskId: string; events: AgentTimelineEvent[] }
  | { type: "interaction.pending"; interaction: AgentInteractionRequest }
  | { type: "interaction.resolved"; taskId: string; requestId: string }
  | { type: "runner.lifecycle"; taskId: string; runtime: ChatRuntimeSnapshot }
  | { type: "provider.status"; backend: string; ready: boolean; report?: unknown };

export interface RemoteEventEnvelope {
  id: string;
  protocolVersion: number;
  sentAt: number;
  payload: RemoteEventPayload;
}

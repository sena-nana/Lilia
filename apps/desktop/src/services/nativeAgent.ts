/**
 * Native AgentKit Desktop glue (#44 / #50)：审批回灌与 Credential Broker 设置面。
 * 命令名与 Rust `native_agent` 模块对齐；secret 仅经 login/import 边界传入，不落前端状态。
 */

import {
  NATIVE_AGENT_HOST_STATUS_COMMAND,
  NATIVE_CREDENTIAL_DIAGNOSTICS_COMMAND,
  NATIVE_CREDENTIAL_IMPORT_COMMAND,
  NATIVE_CREDENTIAL_LOGIN_COMMAND,
  NATIVE_CREDENTIAL_PROVIDERS_COMMAND,
  NATIVE_CREDENTIAL_REVOKE_COMMAND,
  NATIVE_PRODUCT_ARTIFACTS_COMMAND,
  NATIVE_PRODUCT_PENDING_COMMAND,
  NATIVE_PRODUCT_TIMELINE_COMMAND,
  NATIVE_PRODUCT_TODOS_COMMAND,
  NATIVE_QUOTA_SURFACE_COMMAND,
  NATIVE_REBUILD_PRODUCT_TIMELINE_COMMAND,
  NATIVE_REBUILD_UI_TIMELINE_CACHE_COMMAND,
  NATIVE_RESPOND_APPROVAL_COMMAND,
  NATIVE_SHARED_CODE_INDEX_SEARCH_COMMAND,
  NATIVE_SHARED_CODING_SERVICES_STATUS_COMMAND,
  NATIVE_SHARED_GIT_STATUS_COMMAND,
  NATIVE_SHARED_LSP_OPEN_WORKSPACE_COMMAND,
  NATIVE_SHARED_LSP_STATUS_COMMAND,
  NATIVE_SHARED_MCP_LIST_SERVERS_COMMAND,
  NATIVE_SHARED_MEMORY_QUERY_COMMAND,
  NATIVE_SHARED_MEMORY_WRITE_COMMAND,
  NATIVE_SHARED_WORKSPACE_LIST_COMMAND,
  PRODUCT_CORE_STATUS_COMMAND,
} from "@lilia/contracts";
import { invoke } from "../tauri/runtime";

export {
  NATIVE_AGENT_HOST_STATUS_COMMAND,
  NATIVE_CREDENTIAL_DIAGNOSTICS_COMMAND,
  NATIVE_CREDENTIAL_IMPORT_COMMAND,
  NATIVE_CREDENTIAL_LOGIN_COMMAND,
  NATIVE_CREDENTIAL_PROVIDERS_COMMAND,
  NATIVE_CREDENTIAL_REVOKE_COMMAND,
  NATIVE_PRODUCT_ARTIFACTS_COMMAND,
  NATIVE_PRODUCT_PENDING_COMMAND,
  NATIVE_PRODUCT_TIMELINE_COMMAND,
  NATIVE_PRODUCT_TODOS_COMMAND,
  NATIVE_QUOTA_SURFACE_COMMAND,
  NATIVE_REBUILD_PRODUCT_TIMELINE_COMMAND,
  NATIVE_REBUILD_UI_TIMELINE_CACHE_COMMAND,
  NATIVE_RESPOND_APPROVAL_COMMAND,
  NATIVE_SHARED_CODE_INDEX_SEARCH_COMMAND,
  NATIVE_SHARED_CODING_SERVICES_STATUS_COMMAND,
  NATIVE_SHARED_GIT_STATUS_COMMAND,
  NATIVE_SHARED_LSP_OPEN_WORKSPACE_COMMAND,
  NATIVE_SHARED_LSP_STATUS_COMMAND,
  NATIVE_SHARED_MCP_LIST_SERVERS_COMMAND,
  NATIVE_SHARED_MEMORY_QUERY_COMMAND,
  NATIVE_SHARED_MEMORY_WRITE_COMMAND,
  NATIVE_SHARED_WORKSPACE_LIST_COMMAND,
  PRODUCT_CORE_STATUS_COMMAND,
};

export type NativeCredentialKind = "api_key" | "oauth_grant" | "generated_api_key" | "cloud_identity";

export type NativeCredentialStatus =
  | "active"
  | "expired"
  | "revoked"
  | "insufficient_scope"
  | "account_disabled"
  | "unsupported_for_custom_runtime"
  | "pending_refresh";

export type QuotaApiAvailability = "unavailable";

export interface ProductApprovalDecision {
  sessionId: string;
  turnId: string;
  actionId: string;
  version: number;
  approved: boolean;
}

export interface NativeCredentialProvider {
  providerId: string;
  displayName: string;
  protocolFamilies: string[];
  supportedKinds: NativeCredentialKind[];
  supportsBrowserLogin?: boolean;
  enterpriseIdentity?: boolean;
}

export interface NativeCredentialDescriptorView {
  credentialId: string;
  revision: number;
  providerId: string;
  kind: NativeCredentialKind;
  status: NativeCredentialStatus;
  accountLabel: string | null;
  source: string | null;
  modelInference: boolean;
}

export interface NativeCredentialHealthSnapshot {
  brokerReady: boolean;
  providerCount: number;
  credentialCount: number;
  activeCount: number;
  unavailableCount: number;
  hasUsableModelCredential: boolean;
  credentials: NativeCredentialDescriptorView[];
}

export interface NativeIndependentDiagnostics {
  credential: NativeCredentialHealthSnapshot;
  runtimeBackend: string;
  runtimeReady: boolean;
  officialAgentServer: boolean;
  nodeRunnerDefault: boolean;
  profileId: string | null;
  profileHasCredentialRefs: boolean;
  credentialAndRuntimeIndependent: boolean;
  liveModelAdapterDrivesTurn: boolean;
}

export interface KnownCapabilityLimit {
  kind: string;
  label: string;
  value: number | null;
  note: string;
}

export interface NativeProviderQuotaRow {
  providerId: string;
  displayName: string;
  adapterId: string | null;
  quotaApi: QuotaApiAvailability;
  credentialHealth: string;
  hasUsableCredential: boolean;
  knownLimits: KnownCapabilityLimit[];
  note: string;
}

/** Honest Credential Broker quota surface — never invents remote remaining quota. */
export interface NativeQuotaSurface {
  source: string;
  localUsageAvailable: boolean;
  localUsageNote: string;
  remoteQuotaApi: QuotaApiAvailability;
  remoteQuotaNote: string;
  subscriptionNotEquatedToApiQuota: boolean;
  credential: NativeCredentialHealthSnapshot;
  providers: NativeProviderQuotaRow[];
}

export interface NativeCredentialLoginInput {
  providerId: string;
  kind: NativeCredentialKind;
  secretMaterial: string;
  accountLabel?: string | null;
  source?: string | null;
}

export interface NativeCredentialImportInput extends NativeCredentialLoginInput {
  permissionsSummary?: string | null;
  independentRevokeUri?: string | null;
}

export interface NativeApprovalContext {
  sessionId: string;
  turnId: string;
  actionId: string;
  version: number;
  tool?: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

/** Extract Native ProductApprovalDecision fields from permission result providerContext. */
export function productApprovalDecisionFromPermissionResult(
  requestId: string,
  result: { action: string; providerContext?: unknown },
): ProductApprovalDecision | null {
  if (!isRecord(result.providerContext)) return null;
  const native = result.providerContext.native;
  if (!isRecord(native)) return null;
  const sessionId = typeof native.sessionId === "string" ? native.sessionId.trim() : "";
  const turnId = typeof native.turnId === "string" ? native.turnId.trim() : "";
  const actionId =
    (typeof native.actionId === "string" && native.actionId.trim()) ||
    requestId.trim();
  const version = typeof native.version === "number" && Number.isFinite(native.version)
    ? native.version
    : 1;
  if (!sessionId || !turnId || !actionId) return null;
  return {
    sessionId,
    turnId,
    actionId,
    version,
    approved: result.action === "approve",
  };
}

export function getNativeAgentHostStatus(): Promise<unknown> {
  return invoke(NATIVE_AGENT_HOST_STATUS_COMMAND);
}

export function listNativeCredentialProviders(): Promise<NativeCredentialProvider[]> {
  return invoke<NativeCredentialProvider[]>(NATIVE_CREDENTIAL_PROVIDERS_COMMAND);
}

export function nativeCredentialLogin(
  input: NativeCredentialLoginInput,
): Promise<NativeCredentialDescriptorView> {
  return invoke<NativeCredentialDescriptorView>(NATIVE_CREDENTIAL_LOGIN_COMMAND, { input });
}

export function nativeCredentialImport(
  input: NativeCredentialImportInput,
): Promise<NativeCredentialDescriptorView> {
  return invoke<NativeCredentialDescriptorView>(NATIVE_CREDENTIAL_IMPORT_COMMAND, { input });
}

export function nativeCredentialRevoke(
  credentialId: string,
  revision: number,
  reason?: string | null,
): Promise<NativeCredentialDescriptorView> {
  return invoke<NativeCredentialDescriptorView>(NATIVE_CREDENTIAL_REVOKE_COMMAND, {
    credentialId,
    revision,
    reason: reason ?? null,
  });
}

export function nativeCredentialDiagnostics(): Promise<NativeIndependentDiagnostics> {
  return invoke<NativeIndependentDiagnostics>(NATIVE_CREDENTIAL_DIAGNOSTICS_COMMAND);
}

export function getNativeQuotaSurface(): Promise<NativeQuotaSurface> {
  return invoke<NativeQuotaSurface>(NATIVE_QUOTA_SURFACE_COMMAND);
}

export function respondNativeApproval(
  taskId: string,
  decision: ProductApprovalDecision,
): Promise<unknown> {
  return invoke(NATIVE_RESPOND_APPROVAL_COMMAND, { taskId, decision });
}

/** Shared AgentKit Services inventory (#48) — same bundle Arc as Agent tools. */
export interface NativeSharedCodingServicesStatus {
  gitServiceId: string;
  codeIndexServiceId: string;
  lspServiceId: string;
  computerUseServiceId: string;
  mcpServiceId: string;
  memoryRunnerId: string;
  sharedIdentityOk: boolean;
  gitSameInstance: boolean;
  codeIndexSameInstance: boolean;
  lspSameInstance: boolean;
  mcpSameInstance: boolean;
  computerUseSameInstance: boolean;
  memorySharedRouter: boolean;
  mcpActiveServers: number;
  lspActiveWorkspaces: number;
  dataSource: string;
  officialAgentServer: boolean;
}

export function getNativeSharedCodingServicesStatus(): Promise<NativeSharedCodingServicesStatus> {
  return invoke<NativeSharedCodingServicesStatus>(NATIVE_SHARED_CODING_SERVICES_STATUS_COMMAND);
}

export function listNativeSharedMcpServers(): Promise<unknown[]> {
  return invoke<unknown[]>(NATIVE_SHARED_MCP_LIST_SERVERS_COMMAND);
}

export function getNativeSharedLspStatus(): Promise<{
  serviceId: string;
  activeWorkspaces: number;
  workspaces: unknown[];
  dataSource: string;
  sameInstance: boolean;
}> {
  return invoke(NATIVE_SHARED_LSP_STATUS_COMMAND);
}

export function queryNativeSharedMemory(input: {
  query: string;
  namespace?: string | null;
  scopeId?: string | null;
  limit?: number | null;
}): Promise<unknown> {
  return invoke(NATIVE_SHARED_MEMORY_QUERY_COMMAND, {
    query: input.query,
    namespace: input.namespace ?? null,
    scopeId: input.scopeId ?? null,
    limit: input.limit ?? null,
  });
}

export function writeNativeSharedMemory(input: {
  text: string;
  namespace?: string | null;
  scopeId?: string | null;
}): Promise<unknown> {
  return invoke(NATIVE_SHARED_MEMORY_WRITE_COMMAND, {
    text: input.text,
    namespace: input.namespace ?? null,
    scopeId: input.scopeId ?? null,
  });
}

/** Shared GitService Status — same Arc Agent tools hold. */
export function getNativeSharedGitStatus(path: string): Promise<unknown> {
  return invoke(NATIVE_SHARED_GIT_STATUS_COMMAND, { path });
}

/** Shared Code Index open → apply → search (product/UI probe path). */
export function searchNativeSharedCodeIndex(input: {
  workspaceId: string;
  root: string;
  query: string;
}): Promise<unknown> {
  return invoke(NATIVE_SHARED_CODE_INDEX_SEARCH_COMMAND, {
    workspaceId: input.workspaceId,
    root: input.root,
    query: input.query,
  });
}

export function listNativeSharedWorkspace(input: {
  workspaceId: string;
  root: string;
  path?: string;
}): Promise<unknown> {
  return invoke(NATIVE_SHARED_WORKSPACE_LIST_COMMAND, {
    workspaceId: input.workspaceId,
    root: input.root,
    path: input.path ?? "",
  });
}

export function openNativeSharedLspWorkspace(input: {
  workspaceId: string;
  root: string;
}): Promise<unknown> {
  return invoke(NATIVE_SHARED_LSP_OPEN_WORKSPACE_COMMAND, input);
}

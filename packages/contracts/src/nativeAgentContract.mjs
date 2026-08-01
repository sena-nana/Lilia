import nativeAgentContract from "./native-agent-contract.json" with { type: "json" };

const manifest = Object.freeze(nativeAgentContract);

export const NATIVE_AGENT_CONTRACT = manifest;
export const NATIVE_AGENT_HOST_STATUS_COMMAND = manifest.commands.hostStatus;
export const NATIVE_CREDENTIAL_PROVIDERS_COMMAND = manifest.commands.credentialProviders;
export const NATIVE_CREDENTIAL_LOGIN_COMMAND = manifest.commands.credentialLogin;
export const NATIVE_CREDENTIAL_IMPORT_COMMAND = manifest.commands.credentialImport;
export const NATIVE_CREDENTIAL_REVOKE_COMMAND = manifest.commands.credentialRevoke;
export const NATIVE_CREDENTIAL_DIAGNOSTICS_COMMAND = manifest.commands.credentialDiagnostics;
export const NATIVE_QUOTA_SURFACE_COMMAND = manifest.commands.quotaSurface;
export const NATIVE_RESPOND_APPROVAL_COMMAND = manifest.commands.respondApproval;
export const NATIVE_PRODUCT_TIMELINE_COMMAND = manifest.commands.productTimeline;
export const NATIVE_PRODUCT_ARTIFACTS_COMMAND = manifest.commands.productArtifacts;
export const NATIVE_PRODUCT_TODOS_COMMAND = manifest.commands.productTodos;
export const NATIVE_PRODUCT_PENDING_COMMAND = manifest.commands.productPending;
export const NATIVE_REBUILD_PRODUCT_TIMELINE_COMMAND =
  manifest.commands.rebuildProductTimeline;
export const NATIVE_REBUILD_UI_TIMELINE_CACHE_COMMAND =
  manifest.commands.rebuildUiTimelineCache;
export const NATIVE_SHARED_CODING_SERVICES_STATUS_COMMAND =
  manifest.commands.sharedCodingServicesStatus;
export const NATIVE_SHARED_GIT_STATUS_COMMAND = manifest.commands.sharedGitStatus;
export const NATIVE_SHARED_CODE_INDEX_SEARCH_COMMAND =
  manifest.commands.sharedCodeIndexSearch;
export const NATIVE_SHARED_WORKSPACE_LIST_COMMAND =
  manifest.commands.sharedWorkspaceList;
export const NATIVE_SHARED_MCP_LIST_SERVERS_COMMAND =
  manifest.commands.sharedMcpListServers;
export const NATIVE_SHARED_LSP_STATUS_COMMAND = manifest.commands.sharedLspStatus;
export const NATIVE_SHARED_LSP_OPEN_WORKSPACE_COMMAND =
  manifest.commands.sharedLspOpenWorkspace;
export const NATIVE_SHARED_MEMORY_QUERY_COMMAND = manifest.commands.sharedMemoryQuery;
export const NATIVE_SHARED_MEMORY_WRITE_COMMAND = manifest.commands.sharedMemoryWrite;
export const PRODUCT_CORE_STATUS_COMMAND = manifest.commands.productCoreStatus;
export const NATIVE_AGENT_STREAM_EVENT_NAME = manifest.events.stream;

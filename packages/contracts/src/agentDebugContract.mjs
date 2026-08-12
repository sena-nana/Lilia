import agentDebugContract from "./agent-debug-contract.json" with { type: "json" };

const manifest = Object.freeze(agentDebugContract);

export const AGENT_DEBUG_COMMANDS_CONTRACT = manifest;
export const AGENT_DEBUG_STATUS_COMMAND = manifest.commands.status;
export const AGENT_DEBUG_LOGS_COMMAND = manifest.commands.logs;
export const AGENT_DEBUG_RUNTIME_SNAPSHOT_COMMAND = manifest.commands.runtimeSnapshot;
export const AGENT_DEBUG_EQUIVALENCE_SNAPSHOT_COMMAND = manifest.commands.equivalenceSnapshot;
export const AGENT_DEBUG_RECORD_ACTION_COMMAND = manifest.commands.recordAction;
export const AGENT_DEBUG_RESET_STATE_COMMAND = manifest.commands.resetState;

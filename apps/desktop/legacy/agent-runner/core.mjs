import { buildPromptWithAttachments } from "./promptManager.mjs";
import { createProtocolEmitter } from "./protocol.mjs";
import { createInteractionBroker, emitAskUserTimeline } from "./interactions.mjs";
import { emitToolConsentTimeline } from "./toolConsentTimeline.mjs";
import { runClaude } from "./claude/runClaude.mjs";
import { runCodex } from "./codex/runCodex.mjs";
import { runDryRun } from "./dryRun.mjs";
import {
  liliaRuntimeCommandMetadata,
  liliaWorkflowMetadata,
} from "@lilia/contracts/liliaAgentProtocol.mjs";
import {
  RUNNER_ERROR_EVENT_TYPE,
  RUNNER_STDIN_PAYLOAD_KEYS,
  RUNNER_STDIN_TURN_KEYS,
} from "@lilia/contracts/runnerProtocolContract.mjs";

function isRecord(value) {
  return value && typeof value === "object" && !Array.isArray(value);
}

export function normalizeRunnerCommand(cmd) {
  const keys = RUNNER_STDIN_PAYLOAD_KEYS;
  const turn = isRecord(cmd?.[keys.turn]) ? cmd[keys.turn] : {};
  const runtimeOptions = isRecord(cmd?.[keys.runtimeOptions]) ? cmd[keys.runtimeOptions] : {};
  return {
    ...cmd,
    turn,
    workflow: isRecord(cmd?.[keys.workflow]) ? cmd[keys.workflow] : null,
    runtimeCommand: isRecord(cmd?.[keys.runtimeCommand]) ? cmd[keys.runtimeCommand] : null,
    runtimeOptions,
  };
}

export function createRunnerContext(deps = {}) {
  const protocol = deps.protocol || createProtocolEmitter({ write: deps.write });
  const emitConsentTimeline = (id, payload, status, decisionMessage = "") =>
    emitToolConsentTimeline(protocol, id, payload, status, decisionMessage);
  const emitAskTimeline = (id, spec, status, result = null, backend = "claude") =>
    emitAskUserTimeline(protocol, id, spec, status, result, backend);
  const interactions = deps.interactions || createInteractionBroker({
    protocol,
    emitToolConsentTimeline: emitConsentTimeline,
    emitAskUserTimeline: emitAskTimeline,
  });
  return {
    ...deps,
    protocol,
    interactions,
    emitToolConsentTimeline: emitConsentTimeline,
  };
}

export async function runAgentTurn(cmd, deps = {}) {
  const context = createRunnerContext(deps);
  cmd = normalizeRunnerCommand(cmd);
  const workflowType = typeof cmd?.workflow?.type === "string" ? cmd.workflow.type : "";
  const runtimeCommandType = typeof cmd?.runtimeCommand?.type === "string" ? cmd.runtimeCommand.type : "";
  const workflowMetadata = workflowType ? liliaWorkflowMetadata(workflowType) : null;
  const runtimeCommandMetadata = runtimeCommandType ? liliaRuntimeCommandMetadata(runtimeCommandType) : null;
  if (workflowType && !workflowMetadata) {
    context.protocol.emit({ type: RUNNER_ERROR_EVENT_TYPE, message: `unknown workflow: ${workflowType}` });
    return { ok: false, exitCode: 1 };
  }
  if (runtimeCommandType && !runtimeCommandMetadata) {
    context.protocol.emit({ type: RUNNER_ERROR_EVENT_TYPE, message: `unknown runtimeCommand: ${runtimeCommandType}` });
    return { ok: false, exitCode: 1 };
  }
  const turnKeys = RUNNER_STDIN_TURN_KEYS;
  const turn = isRecord(cmd.turn) ? cmd.turn : null;
  if (typeof turn?.[turnKeys.prompt] !== "string") {
    context.protocol.emit({ type: RUNNER_ERROR_EVENT_TYPE, message: "missing prompt" });
    return { ok: false, exitCode: 1 };
  }
  const turnAttachments = Array.isArray(turn[turnKeys.attachments])
    ? turn[turnKeys.attachments]
    : undefined;
  const nextCmd = {
    ...cmd,
    prompt: buildPromptWithAttachments(turn[turnKeys.prompt], turnAttachments),
    cwd: typeof turn[turnKeys.cwd] === "string" ? turn[turnKeys.cwd] : undefined,
    attachments: turnAttachments,
    model: typeof turn[turnKeys.model] === "string" ? turn[turnKeys.model] : undefined,
    resumeSessionId: typeof turn[turnKeys.resumeSessionId] === "string"
      ? turn[turnKeys.resumeSessionId]
      : undefined,
    planMode: typeof turn[turnKeys.planMode] === "boolean" ? turn[turnKeys.planMode] : undefined,
    goalMode: typeof turn[turnKeys.goalMode] === "boolean" ? turn[turnKeys.goalMode] : undefined,
    autoSessionFork: cmd.runtimeOptions?.common?.modelSelection?.sessionFork === true,
    permission: typeof turn[turnKeys.permission] === "string"
      ? turn[turnKeys.permission]
      : undefined,
  };
  const allowsEmptyWorkflowPrompt = workflowMetadata?.requiresPrompt === false;
  const allowsEmptyRuntimeCommandPrompt = runtimeCommandMetadata?.requiresPrompt === false;
  if (
    nextCmd.prompt.trim().length === 0 &&
    !allowsEmptyWorkflowPrompt &&
    !allowsEmptyRuntimeCommandPrompt
  ) {
    context.protocol.emit({ type: RUNNER_ERROR_EVENT_TYPE, message: "missing prompt" });
    return { ok: false, exitCode: 1 };
  }

  try {
    if ((context.env || process.env).LILIA_AGENT_DRY_RUN === "1") {
      await (context.runDryRun || runDryRun)(nextCmd, context);
      return { ok: true, exitCode: 0 };
    }

    const backend = nextCmd.backend === "codex" ? "codex" : "claude";
    if (backend === "codex") {
      await (context.runCodex || runCodex)(nextCmd, context);
    } else {
      await (context.runClaude || runClaude)(nextCmd, context);
    }
    return { ok: true, exitCode: 0 };
  } catch (err) {
    context.protocol.emit({ type: RUNNER_ERROR_EVENT_TYPE, message: err?.message || String(err) });
    return { ok: false, exitCode: 1 };
  }
}

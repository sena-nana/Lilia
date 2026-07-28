import {
  AGENT_INTERACTION_KINDS,
  ARCHITECTURE_INTERACTION_KIND,
  ASK_USER_INTERACTION_KIND,
  MCP_ELICITATION_INTERACTION_KIND,
  PERMISSION_APPROVAL_INTERACTION_KIND,
  PLAN_APPROVAL_INTERACTION_KIND,
  RUNTIME_INTERACTION_KINDS,
  TOOL_CONSENT_INTERACTION_KIND,
  normalizeRuntimeInteractionResult,
} from "@lilia/contracts/agentInteractionContract.mjs";
import {
  PROJECT_ARCHITECTURE_APPLIED_INTERACTION_STATUS,
  PROJECT_ARCHITECTURE_DEFAULT_INTERACTION_STATUS,
} from "@lilia/contracts/architectureContract.mjs";
import {
  ASK_USER_TOOL_NAME,
  PLAN_APPROVAL_INTENT,
} from "@lilia/contracts/askUserContract.mjs";
import {
  RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE,
  RUNNER_INTERACTION_REQUEST_EVENT_TYPE,
  RUNNER_INTERRUPT_TURN_CONTROL_TYPE,
  RUNNER_LILIA_IAB_RESULT_CONTROL_TYPE,
  RUNNER_QUOTA_USAGE_RESULT_CONTROL_TYPE,
  RUNNER_QUOTA_USAGE_REQUEST_EVENT_TYPE,
  RUNNER_SETTINGS_UPDATE_CONTROL_TYPE,
} from "@lilia/contracts/runnerProtocolContract.mjs";
import { normalizeAskUserResult } from "./askUser.mjs";
import { emitArchitectureTimeline } from "./architecture.mjs";
import { isRecord, oneLineSummary, stringOrNull } from "./utils.mjs";

export function normalizeToolConsentResult(value) {
  const row = value && typeof value === "object" && !Array.isArray(value) ? value : {};
  return {
    decision: row.decision === "allow" ? "allow" : "deny",
    message: stringOrNull(row.message) || "",
    updatedInput: row.updatedInput && typeof row.updatedInput === "object" && !Array.isArray(row.updatedInput)
      ? row.updatedInput
      : null,
    codexDecision: stringOrNull(row.codexDecision),
  };
}

const INTERACTION_RESPONSE_KINDS = new Set(AGENT_INTERACTION_KINDS);
const RUNTIME_INTERACTION_KIND_SET = new Set(RUNTIME_INTERACTION_KINDS);

function isRuntimeInteractionKind(kind) {
  return RUNTIME_INTERACTION_KIND_SET.has(kind);
}

export function createInteractionBroker({
  protocol,
  emitToolConsentTimeline,
  emitAskUserTimeline,
} = {}) {
  const consentPending = new Map();
  let consentSeq = 1;
  const askUserPending = new Map();
  let askUserSeq = 1;
  const codexPending = new Map();
  let codexSeq = 1;
  const quotaUsagePending = new Map();
  let quotaUsageSeq = 1;
  const architecturePending = new Map();
  let architectureSeq = 1;
  let settingsUpdateHandler = null;
  let liliaIabResultHandler = null;
  let processSessionCommandHandler = null;
  const interruptHandlers = new Set();

  function emitInteractionRequest(id, kind, payload, backend = "claude") {
    protocol.emit({
      type: RUNNER_INTERACTION_REQUEST_EVENT_TYPE,
      id,
      kind,
      backend,
      payload,
    });
  }

  function requestUserConsent(payload) {
    const id = `consent-${consentSeq++}`;
    emitToolConsentTimeline(id, payload, "requires_action");
    const backend = payload?.backend === "codex" ? "codex" : "claude";
    emitInteractionRequest(id, TOOL_CONSENT_INTERACTION_KIND, payload, backend);
    return new Promise((resolve) => {
      consentPending.set(id, {
        kind: TOOL_CONSENT_INTERACTION_KIND,
        resolve: (response) => resolve({ id, ...response }),
      });
    });
  }

  function requestAskUser(spec, options = {}) {
    const id = `ask-${askUserSeq++}`;
    const kind = spec?.intent === PLAN_APPROVAL_INTENT
      ? PLAN_APPROVAL_INTERACTION_KIND
      : ASK_USER_INTERACTION_KIND;
    const emitTimelineEvent =
      options.emitTimelineEvent !== false && spec?.intent !== PLAN_APPROVAL_INTENT;
    const backend = options.backend === "codex" ? "codex" : "claude";
    if (emitTimelineEvent) emitAskUserTimeline(id, spec, "requires_action", null, backend);
    return new Promise((resolve) => {
      askUserPending.set(id, {
        kind,
        resolve: (result) => {
          if (emitTimelineEvent) {
            emitAskUserTimeline(
              id,
              spec,
              result?.cancelled === true ? "cancelled" : "success",
              result,
              backend,
            );
          }
          resolve(result);
        },
      });
      emitInteractionRequest(id, kind, spec, backend);
    });
  }

  function emitRuntimeInteractionTimeline(id, kind, payload, status, result = null, backend = "codex") {
    const isMcp = kind === MCP_ELICITATION_INTERACTION_KIND;
    const accepted = result?.action === "accept";
    const label = backend === "claude" ? "Claude" : "Codex";
    protocol.emitTimeline({
      kind: isMcp ? "mcp" : "diagnostic",
      status,
      title: isMcp
        ? `${label} MCP elicitation`
        : `${label} permission approval`,
      summary: isMcp
        ? oneLineSummary(payload?.message || payload?.serverName || "MCP 请求用户输入")
        : oneLineSummary(payload?.reason || "Codex 请求额外权限"),
      payload: {
        backend,
        interaction: kind,
        requestId: id,
        ...(isMcp
          ? {
              subkind: MCP_ELICITATION_INTERACTION_KIND,
              serverName: stringOrNull(payload?.serverName),
              mode: stringOrNull(payload?.mode),
              message: stringOrNull(payload?.message),
              url: stringOrNull(payload?.url),
              requestedSchema: payload?.requestedSchema ?? null,
            }
          : {
              subkind: PERMISSION_APPROVAL_INTERACTION_KIND,
              reason: stringOrNull(payload?.reason),
              requestedAccess: payload?.requestedAccess ?? null,
              scopeSuggestion: payload?.scopeSuggestion ?? null,
              providerContext: payload?.providerContext ?? null,
            }),
        ...(result
          ? {
              result,
              accepted: isMcp ? accepted : status === "success",
            }
          : {}),
      },
      sourceId: id,
    });
  }

  function requestCodexInteraction(kind, payload) {
    return requestRuntimeInteraction(kind, payload, "codex");
  }

  function requestMcpElicitation(payload, options = {}) {
    const backend = options.backend === "codex" ? "codex" : "claude";
    return requestRuntimeInteraction(MCP_ELICITATION_INTERACTION_KIND, payload, backend);
  }

  function requestRuntimeInteraction(kind, payload, backend = "codex") {
    const id = `codex-${codexSeq++}`;
    emitRuntimeInteractionTimeline(id, kind, payload, "requires_action", null, backend);
    emitInteractionRequest(id, kind, payload, backend);
    return new Promise((resolve) => {
      codexPending.set(id, {
        kind,
        resolve: (result) => {
          emitRuntimeInteractionTimeline(
            id,
            kind,
            payload,
            codexInteractionCompletedStatus(kind, result),
            result,
            backend,
          );
          resolve(result);
        },
      });
    });
  }

  function requestArchitectureChange(payload, options = {}) {
    const id = `architecture-${architectureSeq++}`;
    const backend = options.backend === "codex" ? "codex" : "claude";
    const autoApply = options.autoApply === true;
    const requestPayload = {
      ...payload,
      requestId: id,
      status: autoApply
        ? PROJECT_ARCHITECTURE_APPLIED_INTERACTION_STATUS
        : PROJECT_ARCHITECTURE_DEFAULT_INTERACTION_STATUS,
      requiresConfirmation: !autoApply,
    };
    emitArchitectureTimeline({ protocol }, id, requestPayload, autoApply ? "info" : "requires_action");
    emitInteractionRequest(id, ARCHITECTURE_INTERACTION_KIND, requestPayload, backend);
    return new Promise((resolve) => {
      architecturePending.set(id, {
        kind: ARCHITECTURE_INTERACTION_KIND,
        resolve: (result) => {
          emitArchitectureTimeline(
            { protocol },
            id,
            requestPayload,
            result?.decision === "allow" ? "success" : "cancelled",
            result,
          );
          resolve(result);
        },
      });
    });
  }

  function requestQuotaUsage(payload = {}) {
    const id = `quota-${quotaUsageSeq++}`;
    protocol.emit({
      type: RUNNER_QUOTA_USAGE_REQUEST_EVENT_TYPE,
      id,
      payload,
    });
    return new Promise((resolve) => {
      quotaUsagePending.set(id, resolve);
    });
  }

  function handleControlLine(line) {
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      return;
    }
    if (!msg || typeof msg !== "object" || Array.isArray(msg)) return;
    if (msg.type === RUNNER_INTERACTION_RESPONSE_CONTROL_TYPE) {
      if (typeof msg.id !== "string") return;
      const kind = msg.kind;
      if (!INTERACTION_RESPONSE_KINDS.has(kind)) return;
      if (kind === TOOL_CONSENT_INTERACTION_KIND) {
        const pending = consentPending.get(msg.id);
        if (!pending || pending.kind !== kind) return;
        consentPending.delete(msg.id);
        pending.resolve(normalizeToolConsentResult(msg.result));
        return;
      }
      if (isRuntimeInteractionKind(kind)) {
        const pending = codexPending.get(msg.id);
        if (!pending || pending.kind !== kind) return;
        codexPending.delete(msg.id);
        pending.resolve(normalizeRuntimeInteractionResult(kind, msg.result));
        return;
      }
      if (kind === ARCHITECTURE_INTERACTION_KIND) {
        const pending = architecturePending.get(msg.id);
        if (!pending || pending.kind !== kind) return;
        architecturePending.delete(msg.id);
        pending.resolve(normalizeArchitectureChangeResult(msg.result));
        return;
      }
      const pending = askUserPending.get(msg.id);
      if (!pending || pending.kind !== kind) return;
      askUserPending.delete(msg.id);
      pending.resolve(normalizeAskUserResult(msg.result));
      return;
    }
    if (msg.type === RUNNER_SETTINGS_UPDATE_CONTROL_TYPE) {
      settingsUpdateHandler?.(msg);
      return;
    }
    if (msg.type === RUNNER_INTERRUPT_TURN_CONTROL_TYPE) {
      for (const handler of interruptHandlers) {
        handler?.();
      }
      return;
    }
    if (msg.type === RUNNER_QUOTA_USAGE_RESULT_CONTROL_TYPE) {
      if (typeof msg.id !== "string") return;
      const resolve = quotaUsagePending.get(msg.id);
      if (!resolve) return;
      quotaUsagePending.delete(msg.id);
      resolve({
        ok: msg.ok === true,
        result: msg.result ?? null,
        error: typeof msg.error === "string" ? msg.error : null,
      });
      return;
    }
    if (msg.type === RUNNER_LILIA_IAB_RESULT_CONTROL_TYPE) {
      liliaIabResultHandler?.(msg.snapshot);
      return;
    }
    if (msg.type === "process_session_command") {
      processSessionCommandHandler?.(msg.runtimeCommand);
      return;
    }
  }

  return {
    requestUserConsent,
    requestAskUser,
    requestCodexInteraction,
    requestMcpElicitation,
    requestArchitectureChange,
    requestQuotaUsage,
    handleControlLine,
    handleSettingsUpdate: (handler) => {
      settingsUpdateHandler = typeof handler === "function" ? handler : null;
    },
    handleLiliaIabResult: (handler) => {
      liliaIabResultHandler = typeof handler === "function" ? handler : null;
    },
    handleProcessSessionCommand: (handler) => {
      processSessionCommandHandler = typeof handler === "function" ? handler : null;
      return () => {
        if (processSessionCommandHandler === handler) processSessionCommandHandler = null;
      };
    },
    handleInterruptTurn: (handler) => {
      if (typeof handler !== "function") return () => {};
      interruptHandlers.add(handler);
      return () => interruptHandlers.delete(handler);
    },
    pendingCounts: () => ({
      consent: consentPending.size,
      askUser: askUserPending.size,
      codex: codexPending.size,
      quotaUsage: quotaUsagePending.size,
      architecture: architecturePending.size,
    }),
  };
}

function normalizeArchitectureChangeResult(value) {
  const row = value && typeof value === "object" && !Array.isArray(value) ? value : {};
  return {
    decision: row.decision === "allow" ? "allow" : "deny",
    graph: row.graph ?? null,
    event: row.event ?? null,
    message: stringOrNull(row.message),
  };
}

function codexInteractionCompletedStatus(kind, result) {
  if (kind === MCP_ELICITATION_INTERACTION_KIND) {
    return result?.action === "accept" ? "success" : "cancelled";
  }
  if (kind === PERMISSION_APPROVAL_INTERACTION_KIND) {
    return result?.action === "approve" ? "success" : "cancelled";
  }
  return "success";
}

export function askUserTimelineSummary(spec) {
  const questions = Array.isArray(spec?.questions) ? spec.questions : [];
  const first = questions[0];
  const question = oneLineSummary(first?.question || first?.header || spec?.title || "用户提问");
  if (questions.length <= 1) return question;
  return `${question} 等 ${questions.length} 个问题`;
}

export function emitAskUserTimeline(protocol, id, spec, status, result = null, backend = "claude") {
  const questions = Array.isArray(spec?.questions) ? spec.questions : [];
  protocol.emitTimeline({
    kind: ASK_USER_INTERACTION_KIND,
    status,
    title: stringOrNull(spec?.title) || ASK_USER_TOOL_NAME,
    summary: askUserTimelineSummary(spec),
    payload: {
      backend,
      interaction: ASK_USER_INTERACTION_KIND,
      requestId: id,
      title: stringOrNull(spec?.title),
      source: stringOrNull(spec?.source),
      questions,
      spec,
      ...(result ? { result } : {}),
      cancelled: result?.cancelled === true,
    },
    sourceId: id,
  });
}

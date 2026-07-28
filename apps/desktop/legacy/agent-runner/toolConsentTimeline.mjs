import { normalizeClaudeTool } from "@lilia/contracts/claudeTools.mjs";
import { TOOL_CONSENT_INTERACTION_KIND } from "@lilia/contracts/agentInteractionContract.mjs";
import { TIMELINE_DISPLAY_SHORT_TEXT_LIMIT } from "@lilia/contracts/timelineContract.mjs";
import { isRecord, oneLineSummary, shortText, stringOrNull } from "./utils.mjs";

export function stringifyCodexCommand(value) {
  if (Array.isArray(value)) {
    return value
      .map((part) => {
        if (typeof part === "string") return part;
        if (isRecord(part)) {
          return stringOrNull(part.text) ||
            stringOrNull(part.value) ||
            stringOrNull(part.arg) ||
            stringOrNull(part.command) ||
            "";
        }
        return stringOrNull(part) || "";
      })
      .filter(Boolean)
      .join(" ");
  }
  if (typeof value === "string") return value;
  if (isRecord(value)) {
    return stringifyCodexCommand(value.parsedCmd || value.command || value.cmd || value.args);
  }
  return "";
}

export function normalizeCodexConsentTool(toolName, input) {
  const name = String(toolName || "tool");
  if (
    name === "item/commandExecution/requestApproval" ||
    name === "commandExecution"
  ) {
    const command = stringifyCodexCommand(
      input?.parsedCmd ||
        input?.command ||
        input?.cmd ||
        input?.commandActions,
    ) || name;
    return {
      kind: "command",
      payload: { command },
      summary: shortText(command, TIMELINE_DISPLAY_SHORT_TEXT_LIMIT),
    };
  }
  if (
    name === "item/fileChange/requestApproval" ||
    name === "fileChange"
  ) {
    return {
      kind: "file_change",
      payload: {
        changes: Array.isArray(input?.fileChanges) ? input.fileChanges : undefined,
        grantRoot: stringOrNull(input?.grantRoot),
      },
      summary: shortText(
        stringOrNull(input?.grantRoot) || "Patch approval",
        TIMELINE_DISPLAY_SHORT_TEXT_LIMIT,
      ),
    };
  }
  return {
    kind: "tool",
    payload: {},
    summary: shortText(name, TIMELINE_DISPLAY_SHORT_TEXT_LIMIT),
  };
}

export function consentTimelineSourceId(id, payload) {
  return stringOrNull(payload?.toolUseID) || id;
}

export function emitToolConsentTimeline(protocol, id, payload, status, decisionMessage = "") {
  const toolName = stringOrNull(payload?.toolName) || "tool";
  const input = isRecord(payload?.input) ? payload.input : {};
  const backend = payload?.backend === "codex" ? "codex" : "claude";
  const normalized = backend === "claude"
    ? normalizeClaudeTool(toolName, input)
    : normalizeCodexConsentTool(toolName, input);
  const summary = decisionMessage ||
    oneLineSummary(payload?.description) ||
    normalized.summary ||
    oneLineSummary(payload?.title) ||
    oneLineSummary(payload?.displayName) ||
    toolName;
  const eventPayload = {
    backend,
    interaction: TOOL_CONSENT_INTERACTION_KIND,
    requestId: id,
    toolName,
    ...normalized.payload,
    input,
    toolUseId: stringOrNull(payload?.toolUseID),
    title: stringOrNull(payload?.title),
    displayName: stringOrNull(payload?.displayName),
    description: stringOrNull(payload?.description),
    blockedPath: stringOrNull(payload?.blockedPath),
    decisionReason: stringOrNull(payload?.decisionReason),
    additionalPermissions: payload?.additionalPermissions ?? null,
    availableDecisions: Array.isArray(payload?.availableDecisions) ? payload.availableDecisions : null,
    proposedExecpolicyAmendment: payload?.proposedExecpolicyAmendment ?? null,
    proposedNetworkPolicyAmendments: payload?.proposedNetworkPolicyAmendments ?? null,
    networkApprovalContext: payload?.networkApprovalContext ?? null,
    cwd: stringOrNull(payload?.cwd),
    reason: stringOrNull(payload?.reason),
    commandActions: payload?.commandActions ?? null,
  };
  if (payload?.commandEdited === true) {
    eventPayload.commandEdited = true;
    eventPayload.originalCommand = stringOrNull(payload?.originalCommand);
    eventPayload.modifiedCommand = stringOrNull(payload?.modifiedCommand);
  }
  if (normalized.subkind) eventPayload.subkind = normalized.subkind;
  if (decisionMessage) eventPayload.decisionMessage = decisionMessage;
  protocol.emitTimeline({
    kind: normalized.kind || "tool",
    status,
    title: toolName,
    summary,
    payload: eventPayload,
    sourceId: consentTimelineSourceId(id, payload),
  });
}

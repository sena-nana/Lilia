import { codexPermissionRuntime } from "@lilia/contracts/permissionModes.mjs";
import { TOOL_CONSENT_INTERACTION_KIND } from "@lilia/contracts/agentInteractionContract.mjs";
import { isRecord, oneLineSummary, stringOrNull } from "../utils.mjs";
import { stringifyCodexCommand } from "../toolConsentTimeline.mjs";
import {
  buildEditedCommandAdditionalContext,
  emitLiliaEditedCommandTimeline,
  executeEditedCodexCommand,
} from "./editedCommand.mjs";

export function mapCodexPermission(p) {
  return { sandboxMode: codexPermissionRuntime(p).sandboxMode };
}

export function codexPermissionProfileIdForMode(permission) {
  return codexPermissionRuntime(permission).permissionProfileId;
}

export function mapCodexSandboxMode(permission) {
  return mapCodexPermission(permission).sandboxMode;
}

export function mapCodexSandboxPolicy(permission) {
  return { type: codexPermissionRuntime(permission).sandboxPolicy };
}

export function mapCodexApprovalPolicy(permission) {
  return codexPermissionRuntime(permission).approvalPolicy;
}

export function codexPermissionRuntimeParams(permission) {
  const runtime = codexPermissionRuntime(permission);
  return {
    approvalPolicy: runtime.approvalPolicy,
    permissions: runtime.permissionProfileId,
    sandboxPolicy: { type: runtime.sandboxPolicy },
  };
}

export function codexLegacyPermissionRuntimeParams(permission) {
  const runtime = codexPermissionRuntime(permission);
  return {
    approvalPolicy: runtime.approvalPolicy,
    sandbox: runtime.sandboxMode,
  };
}

const CODEX_PERMISSION_PROFILE_BY_SANDBOX = Object.freeze({
  "danger-full-access": ":danger-no-sandbox",
  dangerFullAccess: ":danger-no-sandbox",
  "workspace-write": ":workspace",
  workspaceWrite: ":workspace",
  "read-only": ":read-only",
  readOnly: ":read-only",
});

function codexPermissionProfileIdForApprovedRequest(payload, fallbackPermission) {
  const amendment = isRecord(payload?.proposedExecpolicyAmendment)
    ? payload.proposedExecpolicyAmendment
    : {};
  const explicitProfile = stringOrNull(amendment?.permissionProfile) ||
    stringOrNull(amendment?.permissionProfileId);
  if (explicitProfile) return explicitProfile;
  const sandbox = stringOrNull(amendment?.sandbox) ||
    stringOrNull(amendment?.sandboxMode) ||
    stringOrNull(amendment?.sandboxPolicy) ||
    stringOrNull(amendment?.sandboxPolicy?.type);
  return CODEX_PERMISSION_PROFILE_BY_SANDBOX[sandbox] ||
    codexPermissionProfileIdForMode(fallbackPermission);
}

function codexApprovalDecisionFromConsent(response, availableDecisions) {
  const explicit = stringOrNull(response?.codexDecision);
  if (explicit) return explicit;
  if (response?.decision === "allow") return "accept";
  if (Array.isArray(availableDecisions) && !availableDecisions.includes("decline")) {
    return codexCancelDecision(availableDecisions);
  }
  return "decline";
}

function codexCancelDecision(availableDecisions) {
  if (Array.isArray(availableDecisions) && availableDecisions.includes("cancel")) {
    return "cancel";
  }
  return "decline";
}

function readAvailableDecisions(params) {
  return Array.isArray(params?.availableDecisions)
    ? params.availableDecisions.filter((item) => typeof item === "string")
    : ["accept", "decline"];
}

function buildCodexApprovalPayload(toolName, params, requestId, title, description) {
  const input = { ...params };
  const availableDecisions = readAvailableDecisions(params);
  return {
    backend: "codex",
    toolName,
    input,
    toolUseID: requestId,
    title,
    displayName: toolName,
    description,
    additionalPermissions: params.additionalPermissions ?? null,
    availableDecisions,
    proposedExecpolicyAmendment: params.proposedExecpolicyAmendment ?? null,
    proposedNetworkPolicyAmendments: params.proposedNetworkPolicyAmendments ?? null,
    networkApprovalContext: params.networkApprovalContext ?? null,
    cwd: stringOrNull(params.cwd),
    reason: stringOrNull(params.reason),
    commandActions: params.commandActions ?? null,
  };
}

function readCodexCommandFromInput(input) {
  return stringifyCodexCommand(
    input?.parsedCmd ||
      input?.command ||
      input?.cmd ||
      input?.commandActions,
  );
}

function readAllowedCodexCommandEdit(originalInput, updatedInput) {
  if (!isRecord(originalInput) || !isRecord(updatedInput)) return null;
  const originalCommand = readCodexCommandFromInput(originalInput);
  const modifiedCommand = stringifyCodexCommand(
    updatedInput.parsedCmd ||
      updatedInput.command ||
      updatedInput.cmd ||
      updatedInput.commandActions,
  );
  if (!modifiedCommand.trim() || modifiedCommand === originalCommand) return null;
  return {
    input: { ...originalInput, ...updatedInput, command: modifiedCommand },
    originalCommand,
    modifiedCommand,
  };
}

async function steerCodexWithEditedCommandResult(server, ctx, edit, result) {
  await server.request("turn/steer", {
    threadId: ctx.threadId,
    turnId: ctx.currentTurnId,
    additionalContext: buildEditedCommandAdditionalContext(edit, result),
  });
}

export async function maybeHandleCodexApprovalRequest(server, msg, ctx) {
  const method = stringOrNull(msg.method) || "";
  const params = msg.params && typeof msg.params === "object" && !Array.isArray(msg.params)
    ? msg.params
    : {};
  const isCommandApproval =
    method === "item/commandExecution/requestApproval";
  const isFileChangeApproval =
    method === "item/fileChange/requestApproval";
  if (!method.endsWith("/requestApproval")) return false;
  const requestId =
    stringOrNull(params.approvalId) ||
    stringOrNull(params.callId) ||
    stringOrNull(params.itemId) ||
    stringOrNull(msg.id) ||
    `codex-${method}`;
  const toolName = method;
  const command = stringifyCodexCommand(
    params.command ||
      params.parsedCmd ||
      params.cmd ||
      params.commandActions,
  );
  const description = isCommandApproval
    ? command
    : oneLineSummary(params.grantRoot || params.reason || method);
  const title = isCommandApproval
    ? "Codex command approval"
    : isFileChangeApproval
      ? "Codex patch approval"
      : "Codex tool approval";
  const payload = buildCodexApprovalPayload(toolName, params, requestId, title, description);
  const response = await (ctx.withCodexElicitation
    ? ctx.withCodexElicitation(TOOL_CONSENT_INTERACTION_KIND, () =>
      ctx.interactions.requestUserConsent(payload))
    : ctx.interactions.requestUserConsent(payload));
  const { id, decision, message, updatedInput } = response;
  if (isCommandApproval && decision === "allow" && !response.codexDecision) {
    const edit = readAllowedCodexCommandEdit(payload.input, updatedInput);
    if (edit) {
      try {
        const result = await executeEditedCodexCommand(
          server,
          edit,
          payload.cwd,
          codexPermissionProfileIdForApprovedRequest(payload, ctx?.executionPermission),
        );
        emitLiliaEditedCommandTimeline(ctx, payload, edit, result, result.exitCode === 0 ? "success" : "error");
        server.respond(msg.id, { decision: codexCancelDecision(payload.availableDecisions) });
        try {
          await steerCodexWithEditedCommandResult(server, ctx, edit, result);
        } catch (err) {
          ctx.protocol.emitTimeline({
            kind: "error",
            status: "error",
            title: "Codex edited command context",
            summary: err?.message || String(err),
            payload: {
              backend: "codex",
              executionOwner: "lilia",
              subkind: "lilia_edit_exec_context",
              originalCommand: edit.originalCommand,
              modifiedCommand: edit.modifiedCommand,
            },
            sourceId: `${payload.toolUseID || "codex-command"}:lilia-edit-exec-context`,
          });
        }
        ctx.emitToolConsentTimeline(id, {
          ...payload,
          input: edit.input,
          commandEdited: true,
          originalCommand: edit.originalCommand,
          modifiedCommand: edit.modifiedCommand,
        }, "success", "Lilia 已执行用户修改后的 Codex 命令");
        return true;
      } catch (err) {
        emitLiliaEditedCommandTimeline(ctx, payload, edit, {
          exitCode: null,
          stdout: "",
          stderr: err?.message || String(err),
          output: err?.message || String(err),
        }, "error", "Lilia 无法执行用户修改后的命令");
        server.respond(msg.id, { decision: codexCancelDecision(payload.availableDecisions) });
        return true;
      }
    }
  }
  const codexDecision = codexApprovalDecisionFromConsent(response, payload.availableDecisions);
  server.respond(msg.id, { decision: codexDecision });
  const accepted = codexDecision === "accept";
  ctx.emitToolConsentTimeline(id, payload, accepted ? "success" : "cancelled", accepted
    ? "用户已同意此次 Codex 操作"
    : message || "用户拒绝了此次 Codex 操作");
  return true;
}

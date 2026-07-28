import { normalizeClaudeTool } from "@lilia/contracts/claudeTools.mjs";
import { claudePermissionRuntime } from "@lilia/contracts/permissionModes.mjs";
import {
  buildPlanApprovalSpec,
  buildPlanRevisionDenyMessage,
  isClaudePlanTool,
  isPlanApprovalAccepted,
  isReadonlyDeniedClaudeTool,
  normalizeClaudePermissionMode,
  readPlanRevisionRequest,
} from "../claudePlan.mjs";
import { isLiliaAskUserTool } from "../askUser.mjs";
import { isLiliaArchitectureTool } from "../architecture.mjs";
import { normalizeRuntimePermission } from "../runtimeSettings.mjs";
import { isRecord, stringOrNull } from "../utils.mjs";
import { rememberClaudeTool } from "./state.mjs";

export function mapClaudeExecutionPermission(permission) {
  const runtime = claudePermissionRuntime(permission);
  const permissionMode = runtime?.permissionMode ?? normalizeClaudePermissionMode(permission);
  return {
    permissionMode,
    ...(runtime?.allowDangerouslySkipPermissions ? { allowDangerouslySkipPermissions: true } : {}),
  };
}

export function mapClaudeInitialPermission(permission, planMode) {
  const execution = mapClaudeExecutionPermission(permission);
  return {
    ...execution,
    permissionMode: planMode ? "plan" : execution.permissionMode,
  };
}

export function readAllowedCommandEdit(toolName, originalInput, updatedInput) {
  if (toolName !== "Bash" || !isRecord(originalInput) || !isRecord(updatedInput)) return null;
  const originalCommand = typeof originalInput.command === "string" ? originalInput.command : "";
  const modifiedCommand = typeof updatedInput.command === "string" ? updatedInput.command : "";
  if (!modifiedCommand.trim() || modifiedCommand === originalCommand) return null;
  return {
    input: { ...originalInput, command: modifiedCommand },
    originalCommand,
    modifiedCommand,
  };
}

export function commandEditFields(edit) {
  if (!edit) return {};
  return {
    commandEdited: true,
    originalCommand: edit.originalCommand,
    modifiedCommand: edit.modifiedCommand,
  };
}

export function withCommandEditPayload(payload, finalInput, edit) {
  if (!edit) return payload;
  return {
    ...payload,
    input: finalInput,
    ...commandEditFields(edit),
  };
}

export function commandFence(command) {
  let fence = "```";
  while (command.includes(fence)) fence += "`";
  return fence;
}

export function commandEditAdditionalContext(edit) {
  const command = edit?.modifiedCommand || "";
  const fence = commandFence(command);
  return [
    "用户修改了命令。",
    "修改后的命令是：",
    `${fence}shell`,
    command,
    fence,
  ].join("\n");
}

export function rememberCommandEdit(ctx, toolName, toolUseId, edit) {
  const id = stringOrNull(toolUseId);
  if (!id || !edit) return;
  ctx?.commandEdits?.set(id, edit);
  const normalized = normalizeClaudeTool(toolName, edit.input);
  rememberClaudeTool(ctx, id, {
    name: toolName,
    kind: normalized.kind,
    subkind: normalized.subkind,
    payload: { ...normalized.payload, ...commandEditFields(edit) },
  });
}

export function createCommandEditHook(ctx) {
  return async function commandEditHook(input, toolUseId) {
    const id = stringOrNull(input?.tool_use_id) || stringOrNull(toolUseId);
    const edit = id ? ctx?.commandEdits?.get(id) : null;
    if (!id || !edit) return { continue: true };
    ctx.commandEdits.delete(id);
    return {
      continue: true,
      hookSpecificOutput: {
        hookEventName: input?.hook_event_name === "PostToolUseFailure"
          ? "PostToolUseFailure"
          : "PostToolUse",
        additionalContext: commandEditAdditionalContext(edit),
      },
    };
  };
}

export function createClaudeHooks(ctx) {
  const commandEditHook = createCommandEditHook(ctx);
  return {
    PostToolUse: [{ matcher: "Bash", hooks: [commandEditHook] }],
    PostToolUseFailure: [{ matcher: "Bash", hooks: [commandEditHook] }],
  };
}

export function scheduleClaudePermissionModeRestore(ctx, mode) {
  if (typeof ctx.query?.setPermissionMode !== "function") return;
  setTimeout(() => {
    ctx.query.setPermissionMode(mode).catch((err) => {
      ctx.protocol.emitTimeline({
        kind: "error",
        status: "error",
        title: "Claude permission restore",
        summary: err?.message || String(err),
        payload: {
          backend: "claude",
          permissionMode: mode,
        },
      });
    });
  }, 0);
}

export function applyClaudeRuntimePermission(ctx, permission) {
  const normalized = normalizeRuntimePermission(permission);
  if (!normalized) return false;
  ctx.executionPermission = normalized;
  const mode = normalizeClaudePermissionMode(normalized);
  if (typeof ctx.query?.setPermissionMode === "function") {
    ctx.query.setPermissionMode(mode).catch((err) => {
      ctx.protocol.emitTimeline({
        kind: "diagnostic",
        status: "error",
        title: "Claude permission update failed",
        summary: err?.message || String(err),
        payload: {
          backend: "claude",
          subkind: "settings",
          permission: normalized,
          permissionMode: mode,
        },
        sourceId: "claude:settings:permission",
      });
    });
  }
  ctx.protocol.emitTimeline({
    kind: "diagnostic",
    status: "info",
    title: "Claude permission updated",
    summary: `权限已切换为 ${normalized}`,
    payload: {
      backend: "claude",
      subkind: "settings",
      permission: normalized,
      permissionMode: mode,
    },
    sourceId: "claude:settings:permission",
  });
  return true;
}

export async function handleClaudePlanPermission(toolName, input, opts, ctx) {
  const sourceId = stringOrNull(opts?.toolUseID);
  const executionPermission = ctx.executionPermission;
  const approvalSpec = buildPlanApprovalSpec();
  const pendingPayload = ctx.emitClaudePlanTimeline({
    ctx,
    toolName,
    input,
    fallbackPlan: ctx.latestAssistantText || "",
    approved: null,
    executionPermission,
    status: "requires_action",
    sourceId,
  });
  const result = await ctx.interactions.requestAskUser(approvalSpec, { emitTimelineEvent: false });
  const revisionRequest = readPlanRevisionRequest(result);
  if (revisionRequest) {
    ctx.emitClaudePlanTimeline({
      ctx,
      toolName,
      input: { ...pendingPayload, revisionRequest },
      approved: false,
      executionPermission,
      status: "cancelled",
      sourceId,
    });
    return {
      behavior: "deny",
      message: buildPlanRevisionDenyMessage(revisionRequest),
    };
  }
  if (!isPlanApprovalAccepted(result)) {
    ctx.emitClaudePlanTimeline({
      ctx,
      toolName,
      input: pendingPayload,
      approved: false,
      executionPermission,
      status: "cancelled",
      sourceId,
    });
    ctx?.activeTools?.delete(sourceId);
    return {
      behavior: "deny",
      message: "用户暂未确认计划",
      interrupt: true,
    };
  }

  const mode = normalizeClaudePermissionMode(executionPermission);
  scheduleClaudePermissionModeRestore(ctx, mode);
  ctx.emitClaudePlanTimeline({
    ctx,
    toolName,
    input: pendingPayload,
    approved: true,
    executionPermission,
    status: "success",
    sourceId,
  });
  ctx.approvedArchitectureImpacts = Array.isArray(pendingPayload.architectureImpacts)
    ? pendingPayload.architectureImpacts
    : [];
  return {
    behavior: "allow",
    updatedInput: input,
    updatedPermissions: [{ type: "setMode", mode, destination: "session" }],
  };
}

export function emitReadonlyDeniedClaudeTool(toolName, input, opts, ctx) {
  const sourceId = stringOrNull(opts?.toolUseID);
  const reason = "当前权限为只读，禁止写操作";
  const normalized = normalizeClaudeTool(toolName, input, {
    subagent_type: opts?.agentID,
  });
  const payload = {
    backend: "claude",
    toolName,
    ...normalized.payload,
    input,
    permissionDenied: true,
    reason: "readonly",
    message: reason,
  };
  if (normalized.subkind) payload.subkind = normalized.subkind;
  if (sourceId) {
    ctx.deniedTools.set(sourceId, { reason: "readonly", message: reason });
  }
  ctx.protocol.emitTimeline({
    kind: normalized.kind,
    status: "error",
    title: toolName,
    summary: reason,
    payload,
    sourceId,
  });
  ctx?.activeTools?.delete(sourceId);
}

export function createClaudeCanUseTool(ctx) {
  return async function claudeCanUseTool(toolName, input, opts) {
    const safeInput = isRecord(input) ? input : {};
    if (isLiliaAskUserTool(toolName) || isLiliaArchitectureTool(toolName)) {
      return { behavior: "allow", updatedInput: safeInput };
    }
    if (isClaudePlanTool(toolName)) {
      return handleClaudePlanPermission(toolName, safeInput, opts, ctx);
    }
    if (ctx.executionPermission === "full" || ctx.executionPermission === "free") {
      return { behavior: "allow", updatedInput: safeInput };
    }
    if (ctx.executionPermission === "readonly" && isReadonlyDeniedClaudeTool(toolName)) {
      emitReadonlyDeniedClaudeTool(toolName, safeInput, opts, ctx);
      return {
        behavior: "deny",
        message: "当前权限为只读，禁止写操作",
      };
    }
    const consentPayload = {
      toolName,
      input: safeInput,
      toolUseID: stringOrNull(opts?.toolUseID),
      title: stringOrNull(opts?.title),
      displayName: stringOrNull(opts?.displayName),
      description: stringOrNull(opts?.description),
      blockedPath: stringOrNull(opts?.blockedPath),
      decisionReason: stringOrNull(opts?.decisionReason),
    };
    const { id, decision, message, updatedInput } = await ctx.interactions.requestUserConsent(consentPayload);
    if (decision === "allow") {
      const commandEdit = readAllowedCommandEdit(toolName, safeInput, updatedInput);
      const finalInput = commandEdit?.input ?? safeInput;
      rememberCommandEdit(ctx, toolName, opts?.toolUseID, commandEdit);
      const finalConsentPayload = withCommandEditPayload(consentPayload, finalInput, commandEdit);
      ctx.emitToolConsentTimeline(id, finalConsentPayload, "success", "用户已同意此次工具调用");
      return { behavior: "allow", updatedInput: finalInput };
    }
    ctx.emitToolConsentTimeline(id, consentPayload, "cancelled", message || "用户拒绝了此次工具调用");
    return { behavior: "deny", message: message || "用户拒绝了此次工具调用" };
  };
}

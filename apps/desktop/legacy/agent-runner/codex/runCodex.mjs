import {
  askUserResultToCodexRequestUserInputResponse,
  codexAskUserDynamicTool,
  codexRequestUserInputQuestionsToSpec,
  createCodexAskUserHandler,
  isLiliaAskUserTool,
} from "../askUser.mjs";
import {
  codexQueryConversationContextDynamicTool,
  conversationContextEnabled,
  createConversationContextHandler,
  isLiliaConversationContextTool,
} from "../conversationContext.mjs";
import {
  architectureContextEnabled,
  codexUpdateProjectArchitectureDynamicTool,
  createArchitectureChangeHandler,
  isLiliaArchitectureTool,
} from "../architecture.mjs";
import {
  codexQueryQuotaUsageDynamicTool,
  createQuotaUsageHandler,
  isLiliaQuotaTool,
} from "../quotaUsage.mjs";
import {
  emitRuntimeExtensionWarnings,
  readCodexRuntimeExtensions,
} from "../runtimeExtensions.mjs";
import {
  readRunnerRuntimeCommand,
  readRunnerWorkflow,
} from "../runnerCommand.mjs";
import {
  buildCodexPlanPrompt,
  buildCodexWorkflowPrompt,
} from "../promptManager.mjs";
import {
  ensureProviderRuntimeOptions,
  handleExperimentalProviderOptions,
  readProviderRuntimeOptions,
  withProviderRuntimeOptions,
} from "../providerOptions.mjs";
import { normalizeRuntimePermission } from "../runtimeSettings.mjs";
import { isRecord, oneLineSummary, stringOrNull } from "../utils.mjs";
import { createCodexAppServer, initializeCodexAppServer } from "./appServer.mjs";
import {
  codexLegacyPermissionRuntimeParams,
  codexPermissionRuntimeParams,
  maybeHandleCodexApprovalRequest,
} from "./permissions.mjs";
import {
  createCodexRunContext,
  codexHistoryTimelineEvents,
  emitCodexPlanApprovalRequired,
  finalizeCodexRunContext,
  mapCodexEventToNdjson,
  normalizeCodexAppServerEvent,
  resetCodexContextForNextTurn,
  resolveCodexPlanApproval,
} from "./timeline.mjs";
import { buildPlanApprovalSpec } from "../planApproval.mjs";
import { runCodexSessionManagementRuntimeCommand } from "../sessionManagement.mjs";
import {
  ASK_USER_INTERACTION_KIND,
  MCP_ELICITATION_INTERACTION_KIND,
  PERMISSION_APPROVAL_INTERACTION_KIND,
  PLAN_APPROVAL_INTERACTION_KIND,
} from "@lilia/contracts/agentInteractionContract.mjs";
import { RUNNER_DONE_EVENT_TYPE } from "@lilia/contracts/runnerProtocolContract.mjs";
import { normalizeCodexProfileSettings } from "@lilia/contracts/liliaCodeCore.mjs";
import {
  REMOTE_ENVIRONMENT_ACTIONS,
  REMOTE_ENVIRONMENT_COMMAND_TYPE,
  SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
  normalizeProcessSessionCommand,
  normalizeRemoteEnvironmentCommand,
  normalizeRuntimeSettingsCommand,
  normalizeSandboxDiagnosticsCommand,
  normalizeSessionForkCommand,
} from "@lilia/contracts/runtimeCommandContract.mjs";
import {
  isLiliaMemoryResetWorkflow,
  isLiliaBackgroundTerminalsCleanWorkflow,
  isLiliaCompactWorkflow,
  normalizeLiliaBatchApplyWorkflow,
  normalizeLiliaGoalWorkflow,
  normalizeLiliaConfigDiagnosticsWorkflow,
  normalizeLiliaFixSuggestionWorkflow,
  normalizeLiliaMemoryModeWorkflow,
  normalizeLiliaReviewWorkflow,
  normalizeLiliaTaskWorkflow,
} from "@lilia/contracts/liliaWorkflowContract.mjs";

function codexThreadIdFromResult(result, fallback = null) {
  if (!isRecord(result)) return fallback;
  return stringOrNull(result.thread?.id) || stringOrNull(result.threadId) || fallback;
}

function codexModelFromResult(result, fallback = null) {
  if (!isRecord(result)) return fallback;
  return stringOrNull(result.model) || stringOrNull(result.thread?.model) || fallback;
}

function jsonArrayOrNull(value) {
  return Array.isArray(value) && value.length > 0 ? value.slice() : null;
}

function numberOrNull(value) {
  const number = typeof value === "number" ? value : Number(value);
  return Number.isFinite(number) ? number : null;
}

function booleanOrNull(value) {
  return typeof value === "boolean" ? value : null;
}

function normalizeCodexSettings(cmd) {
  const input = readProviderRuntimeOptions(cmd?.runtimeOptions, "codex").settings;
  const normalized = normalizeCodexProfileSettings({
    ...input,
    model: stringOrNull(input.model) || stringOrNull(cmd.model) || null,
  });
  return {
    ...normalized,
    environments: jsonArrayOrNull(input.environments),
    experimentalRawEvents: booleanOrNull(input.experimentalRawEvents),
  };
}

function ensureCodexRuntimeOptions(cmd) {
  return ensureProviderRuntimeOptions(cmd, "codex");
}

function assignCodexSettingsParams(params, settings, cmd, { includeSandbox = false } = {}) {
  if (settings.model) params.model = settings.model;
  if (settings.reasoningEffort) {
    params.reasoningEffort = settings.reasoningEffort;
    params.effort = settings.reasoningEffort;
  }
  if (settings.runtimeWorkspaceRoots.length > 0) {
    params.runtimeWorkspaceRoots = settings.runtimeWorkspaceRoots;
  }
  if (includeSandbox) {
    Object.assign(params, codexPermissionRuntimeParams(cmd.permission));
  }
}

function assignCodexAdvancedThreadParams(params, settings) {
  if (settings.persistExtendedHistory !== null) {
    params.persistExtendedHistory = settings.persistExtendedHistory;
  }
  if (settings.environments) {
    params.environments = settings.environments;
  }
  if (settings.experimentalRawEvents !== null) {
    params.experimentalRawEvents = settings.experimentalRawEvents;
  }
  if (settings.responsesApiClientMetadata) {
    params.responsesapiClientMetadata = settings.responsesApiClientMetadata;
  }
}

function assignCodexResumeParams(params, settings) {
  assignCodexAdvancedThreadParams(params, settings);
  if (settings.excludeTurns.length > 0) params.excludeTurns = settings.excludeTurns;
  if (settings.initialTurnsPage) params.initialTurnsPage = settings.initialTurnsPage;
}

function assignCodexTurnParams(params, settings) {
  if (settings.responsesApiClientMetadata) {
    params.responsesapiClientMetadata = settings.responsesApiClientMetadata;
  }
  if (settings.environments) {
    params.environments = settings.environments;
  }
  if (settings.experimentalRawEvents !== null) {
    params.experimentalRawEvents = settings.experimentalRawEvents;
  }
  if (settings.additionalContext) params.additionalContext = settings.additionalContext;
}

function buildCodexThreadSettingsParams(threadId, cmd) {
  const settings = normalizeCodexSettings(cmd);
  const params = {
    threadId,
  };
  assignCodexSettingsParams(params, settings, cmd, { includeSandbox: true });
  assignCodexAdvancedThreadParams(params, settings);
  return params;
}

function emitCodexSettingsUpdateDiagnostic(protocol, err, params) {
  protocol.emitTimeline({
    kind: "diagnostic",
    status: "error",
    title: "Codex settings update failed",
    summary: "thread/settings/update 失败，已降级为本轮 turn/start 参数。",
    payload: {
      backend: "codex",
      subkind: "settings",
      method: "thread/settings/update",
      error: err?.message || String(err),
      fallback: "turn/start",
      settingsKeys: Object.keys(params).filter((key) => key !== "threadId"),
    },
    sourceId: "codex:settings:update",
  });
}

export async function updateCodexThreadSettings(server, threadId, cmd, protocol) {
  const params = buildCodexThreadSettingsParams(threadId, cmd);
  try {
    await server.request("thread/settings/update", params);
    return { ok: true, fallback: false };
  } catch (err) {
    emitCodexSettingsUpdateDiagnostic(protocol, err, params);
    return { ok: false, fallback: true };
  }
}

export function applyCodexRuntimeSettings(server, threadId, cmd, ctx, update, protocol) {
  const normalizedPermission = normalizeRuntimePermission(update?.permission);
  const model = stringOrNull(update?.model)?.trim() || null;
  if (!normalizedPermission && !model) return null;
  if (normalizedPermission) {
    cmd.permission = normalizedPermission;
    ctx.executionPermission = normalizedPermission;
  }
  if (model) {
    cmd.model = model;
    ensureCodexRuntimeOptions(cmd).model = model;
  }
  const pending = updateCodexThreadSettings(server, threadId, cmd, protocol)
    .then((result) => {
      if (!result.ok) return result;
      const parts = [];
      if (normalizedPermission) parts.push(`权限已切换为 ${normalizedPermission}`);
      if (model) parts.push(`模型已切换为 ${model}`);
      protocol.emitTimeline({
        kind: "diagnostic",
        status: "info",
        title: "Codex settings updated",
        summary: parts.join("，"),
        payload: {
          backend: "codex",
          subkind: "settings",
          permission: normalizedPermission,
          model,
          fallback: false,
        },
        sourceId: "codex:settings:runtime",
      });
      return result;
    });
  ctx.settingsUpdatePromises.push(pending);
  return pending;
}

export async function flushCodexRuntimeSettings(ctx) {
  if (ctx.settingsUpdatePromises.length === 0) return;
  const pending = ctx.settingsUpdatePromises.splice(0);
  await Promise.allSettled(pending);
}

function emitCodexElicitationDiagnostic(ctx, method, kind, err) {
  ctx?.protocol?.emitTimeline?.({
    kind: "diagnostic",
    status: "error",
    title: "Codex elicitation update failed",
    summary: `${method} 失败，Lilia 已继续等待用户交互。`,
    payload: {
      backend: "codex",
      subkind: "elicitation",
      method,
      interactionKind: kind,
      error: err?.message || String(err),
    },
    sourceId: `codex:elicitation:${method}:${kind}`,
  });
}

export async function withCodexElicitation(server, threadId, ctx, kind, fn) {
  if (typeof fn !== "function") return undefined;
  if (!server || !threadId) return await fn();
  let incremented = false;
  try {
    await server.request("thread/increment_elicitation", { threadId });
    incremented = true;
  } catch (err) {
    emitCodexElicitationDiagnostic(ctx, "thread/increment_elicitation", kind, err);
  }
  try {
    return await fn();
  } finally {
    if (incremented) {
      try {
        await server.request("thread/decrement_elicitation", { threadId });
      } catch (err) {
        emitCodexElicitationDiagnostic(ctx, "thread/decrement_elicitation", kind, err);
      }
    }
  }
}

async function runCodexUiInteraction(ctx, kind, fn) {
  return ctx?.withCodexElicitation
    ? ctx.withCodexElicitation(kind, fn)
    : await fn();
}

export async function startCodexAppServerSession(server, cmd, cwdFn = process.cwd) {
  const { cwd, model, resumeSessionId, permission } = cmd;
  const settings = normalizeCodexSettings(cmd);
  const common = {
    model: settings.model || model || undefined,
    cwd: cwd || cwdFn(),
    ...codexLegacyPermissionRuntimeParams(permission),
  };
  assignCodexSettingsParams(common, settings, cmd);
  assignCodexAdvancedThreadParams(common, settings);
  const dynamicTools = [codexAskUserDynamicTool, codexQueryQuotaUsageDynamicTool];
  if (conversationContextEnabled(cmd.conversationContext)) {
    dynamicTools.push(codexQueryConversationContextDynamicTool);
  }
  if (architectureContextEnabled(cmd.conversationContext)) {
    dynamicTools.push(codexUpdateProjectArchitectureDynamicTool);
  }
  if (resumeSessionId) {
    const resumeParams = {
      threadId: resumeSessionId,
      ...common,
      dynamicTools,
    };
    assignCodexResumeParams(resumeParams, settings);
    const resumed = await server.request("thread/resume", resumeParams);
    return {
      threadId: codexThreadIdFromResult(resumed, resumeSessionId),
      model: codexModelFromResult(resumed, model || null),
    };
  }
  const started = await server.request("thread/start", {
    ...common,
    dynamicTools,
  });
  return {
    threadId: codexThreadIdFromResult(started),
    model: codexModelFromResult(started, model || null),
  };
}

export async function startCodexAppServerThread(server, cmd, cwdFn = process.cwd) {
  return (await startCodexAppServerSession(server, cmd, cwdFn)).threadId;
}

export function buildCodexPlanRevisionPrompt(revisionRequest) {
  return buildCodexPlanPrompt("revision", revisionRequest);
}

export function buildCodexPlanExecutionPrompt(plan) {
  return buildCodexPlanPrompt("execution", plan);
}

function codexTurnIdFromStartResult(result) {
  if (!isRecord(result)) return null;
  const turn = isRecord(result.turn) ? result.turn : null;
  return stringOrNull(turn?.id) || stringOrNull(result.turnId);
}

function readCodexWorkflow(cmd) {
  return readRunnerWorkflow(cmd);
}

function readCodexReviewWorkflow(cmd) {
  const workflow = normalizeLiliaReviewWorkflow(readCodexWorkflow(cmd));
  return workflow ? { ...workflow, target: codexReviewTargetForAppServer(workflow.target) } : null;
}

function readCodexFixSuggestionWorkflow(cmd) {
  const workflow = normalizeLiliaFixSuggestionWorkflow(readCodexWorkflow(cmd));
  return workflow ? { ...workflow, target: codexReviewTargetForAppServer(workflow.target) } : null;
}

function readCodexBatchApplyWorkflow(cmd) {
  return normalizeLiliaBatchApplyWorkflow(readCodexWorkflow(cmd));
}

function readCodexTaskWorkflow(cmd) {
  return normalizeLiliaTaskWorkflow(readCodexWorkflow(cmd));
}

function codexReviewTargetForAppServer(target) {
  return target?.type === "commit" ? { ...target, title: null } : target;
}

function codexFixSuggestionEffectiveTurnCmd(cmd, workflow) {
  if (workflow.mode !== "suggest") return cmd;
  return {
    ...cmd,
    permission: "readonly",
  };
}

function readCodexGoalWorkflow(cmd) {
  return normalizeLiliaGoalWorkflow(readCodexWorkflow(cmd));
}

function readCodexCompactWorkflow(cmd) {
  return isLiliaCompactWorkflow(readCodexWorkflow(cmd));
}

function readCodexBackgroundTerminalsCleanWorkflow(cmd) {
  return isLiliaBackgroundTerminalsCleanWorkflow(readCodexWorkflow(cmd));
}

function readCodexMemoryModeWorkflow(cmd) {
  return normalizeLiliaMemoryModeWorkflow(readCodexWorkflow(cmd));
}

function readCodexMemoryResetWorkflow(cmd) {
  return isLiliaMemoryResetWorkflow(readCodexWorkflow(cmd));
}

function readCodexThreadForkRuntimeCommand(cmd) {
  const command = normalizeSessionForkCommand(readRunnerRuntimeCommand(cmd));
  if (!command) return null;
  return {
    ...command,
    sourceTurnId: command.sourceTurnId || null,
    continueAfterFork: Boolean(command.sourceTurnId && stringOrNull(cmd?.prompt)?.trim()),
  };
}

function readCodexConfigDiagnosticsWorkflow(cmd) {
  return normalizeLiliaConfigDiagnosticsWorkflow(readCodexWorkflow(cmd));
}

function hasOwn(obj, key) {
  return Object.prototype.hasOwnProperty.call(obj, key);
}

function readCodexRuntimeSettingsCommand(cmd) {
  const normalizedCommand = normalizeRuntimeSettingsCommand(
    readRunnerRuntimeCommand(cmd),
  );
  if (!normalizedCommand) return null;
  const {
    common,
    settings: codex,
    ignoredProviderKeys,
  } = readProviderRuntimeOptions(cmd?.runtimeOptions, "codex");
  const normalizedCodex = normalizeCodexProfileSettings(codex);
  const updates = {};
  const model = stringOrNull(common.model)?.trim();
  const permission = normalizeRuntimePermission(common.permission);
  const environments = jsonArrayOrNull(codex.environments);

  if (model) updates.model = model;
  if (permission) updates.permission = permission;
  if (hasOwn(codex, "profile")) updates.profile = normalizedCodex.profile;
  if (hasOwn(codex, "reasoningEffort") && normalizedCodex.reasoningEffort) {
    updates.reasoningEffort = normalizedCodex.reasoningEffort;
  }
  if (hasOwn(codex, "runtimeWorkspaceRoots") && normalizedCodex.runtimeWorkspaceRoots.length > 0) {
    updates.runtimeWorkspaceRoots = normalizedCodex.runtimeWorkspaceRoots;
  }
  if (hasOwn(codex, "persistExtendedHistory")) {
    updates.persistExtendedHistory = normalizedCodex.persistExtendedHistory === true;
  }
  if (environments) updates.environments = environments;
  if (hasOwn(codex, "experimentalRawEvents")) {
    updates.experimentalRawEvents = codex.experimentalRawEvents === true;
  }
  if (hasOwn(codex, "responsesApiClientMetadata") && normalizedCodex.responsesApiClientMetadata) {
    updates.responsesApiClientMetadata = normalizedCodex.responsesApiClientMetadata;
  }
  if (normalizedCommand.action === "update" && Object.keys(updates).length === 0) {
    throw new Error("Lilia provider settings update requires at least one supported setting");
  }
  return {
    action: normalizedCommand.action,
    updates,
    ignoredProviderKeys,
  };
}

function readCodexRemoteEnvironmentCommand(cmd) {
  return normalizeRemoteEnvironmentCommand(readRunnerRuntimeCommand(cmd));
}

function readCodexSandboxDiagnosticsCommand(cmd) {
  return normalizeSandboxDiagnosticsCommand(readRunnerRuntimeCommand(cmd));
}

function readCodexProcessSessionCommand(value) {
  return normalizeProcessSessionCommand(
    isRecord(value?.runtimeCommand) ? value.runtimeCommand : value,
  );
}

function codexSettingsCmdFromRuntimeCommand(cmd, command) {
  const codexSettings = {
    ...normalizeCodexSettings(cmd),
  };
  const next = withProviderRuntimeOptions(cmd, "codex", codexSettings);
  if (command.updates.model) {
    next.model = command.updates.model;
    codexSettings.model = command.updates.model;
  }
  if (command.updates.permission) {
    next.permission = command.updates.permission;
  }
  if (command.updates.profile) {
    codexSettings.profile = command.updates.profile;
  }
  if (command.updates.reasoningEffort) {
    codexSettings.reasoningEffort = command.updates.reasoningEffort;
  }
  if (command.updates.runtimeWorkspaceRoots) {
    codexSettings.runtimeWorkspaceRoots = command.updates.runtimeWorkspaceRoots;
  }
  if (hasOwn(command.updates, "persistExtendedHistory")) {
    codexSettings.persistExtendedHistory = command.updates.persistExtendedHistory;
  }
  if (command.updates.environments) {
    codexSettings.environments = command.updates.environments;
  }
  if (hasOwn(command.updates, "experimentalRawEvents")) {
    codexSettings.experimentalRawEvents = command.updates.experimentalRawEvents;
  }
  if (command.updates.responsesApiClientMetadata) {
    codexSettings.responsesApiClientMetadata = command.updates.responsesApiClientMetadata;
  }
  return next;
}

function emitCodexWorkflowTimeline(ctx, config, threadId, status, error = null) {
  const failed = status === "error";
  const completed = status === "success";
  ctx.protocol.emitTimeline({
    kind: "diagnostic",
    status,
    title: failed
      ? config.errorTitle
      : completed
        ? config.successTitle
        : config.startedTitle,
    summary: failed
      ? error?.message || String(error)
      : completed
        ? config.successSummary
        : config.startedSummary,
    payload: {
      backend: "codex",
      subkind: config.subkind,
      method: config.method,
      threadId,
      ...(failed ? { error: error?.message || String(error) } : {}),
    },
    sourceId: `${config.sourcePrefix}:${failed ? "error" : completed ? "completed" : "start"}:${threadId}`,
  });
}

function emitCodexWorkflowDone(ctx, threadId) {
  ctx.protocol.emit({
    type: RUNNER_DONE_EVENT_TYPE,
    sessionId: threadId,
    subtype: "success",
  });
}

function compactConfigRequirements(requirements) {
  if (!isRecord(requirements)) return null;
  return {
    allowedApprovalsReviewers: requirements.allowedApprovalsReviewers ?? null,
    hooks: requirements.hooks ?? null,
    network: requirements.network ?? null,
    allowedApprovalPolicies: requirements.allowedApprovalPolicies ?? null,
    allowedSandboxModes: requirements.allowedSandboxModes ?? null,
    allowedPermissions: requirements.allowedPermissions ?? null,
  };
}

function compactCodexConfig(config) {
  if (!isRecord(config)) return null;
  return {
    model: config.model ?? null,
    modelProvider: config.model_provider ?? config.modelProvider ?? null,
    approvalPolicy: config.approval_policy ?? config.approvalPolicy ?? null,
    sandboxMode: config.sandbox_mode ?? config.sandboxMode ?? null,
    permissions: config.permissions ?? null,
    apps: config.apps ?? null,
  };
}

function codexThreadIdFromForkResult(result) {
  return codexThreadIdFromResult(result) || stringOrNull(result?.forkedThreadId);
}

function codexReviewTargetSummary(target) {
  if (target.type === "baseBranch") return `baseBranch:${target.branch}`;
  if (target.type === "commit") return `commit:${target.sha}`;
  return "uncommittedChanges";
}

function codexReviewTargetDescription(target) {
  if (target.type === "baseBranch") {
    return `Compare the current workspace against base branch: ${target.branch}`;
  }
  if (target.type === "commit") {
    return `Inspect the specified commit: ${target.sha}`;
  }
  return "Inspect the current uncommitted workspace changes.";
}

export function buildCodexFixSuggestionPrompt(workflow, cmd) {
  return buildCodexWorkflowPrompt("fixSuggestion", {
    workflow,
    cmd,
    targetSummary: codexReviewTargetSummary(workflow.target),
    targetDescription: codexReviewTargetDescription(workflow.target),
  });
}

export function buildCodexBatchApplyPrompt(workflow, cmd) {
  return buildCodexWorkflowPrompt("batchApply", { workflow, cmd });
}

export function buildCodexTaskWorkflowPrompt(workflow, cmd) {
  return buildCodexWorkflowPrompt("taskWorkflow", { workflow, cmd });
}

function readCollaborationModes(result) {
  if (!isRecord(result)) return [];
  return Array.isArray(result.data) ? result.data.filter(isRecord) : [];
}

export async function readCodexPlanModePreset(server) {
  let result;
  try {
    result = await server.request("collaborationMode/list", {});
  } catch (err) {
    throw new Error(`Codex plan mode unavailable: collaborationMode/list failed: ${err?.message || String(err)}`);
  }
  const preset = readCollaborationModes(result).find((mode) => mode.mode === "plan") || null;
  if (!preset) {
    throw new Error("Codex plan mode unavailable: plan collaboration preset is missing");
  }
  return preset;
}

async function requireCodexPlanModePreset(server, protocol, sourceId = "codex:plan-mode") {
  try {
    return await readCodexPlanModePreset(server);
  } catch (err) {
    protocol?.emitTimeline?.({
      kind: "diagnostic",
      status: "error",
      title: "Codex plan mode unavailable",
      summary: err?.message || String(err),
      payload: {
        backend: "codex",
        subkind: "plan_mode",
        method: "collaborationMode/list",
        error: err?.message || String(err),
      },
      sourceId,
    });
    throw err;
  }
}

function readCodexTurns(result) {
  if (!isRecord(result)) return [];
  return Array.isArray(result.data) ? result.data.filter(isRecord) : [];
}

export async function syncCodexThreadHistory(server, threadId, cmd, protocol) {
  if (!cmd.resumeSessionId || !threadId) return { ok: true, skipped: true, count: 0 };
  try {
    const result = await server.request("thread/turns/list", {
      threadId,
      limit: 50,
      sortDirection: "asc",
      itemsView: "full",
    });
    const events = codexHistoryTimelineEvents(threadId, readCodexTurns(result));
    for (const event of events) protocol.emitTimeline(event);
    protocol.emitTimeline({
      kind: "diagnostic",
      status: "info",
      title: "Codex history synced",
      summary: events.length > 0
        ? `已同步 ${events.length} 条 Codex 历史事件`
        : "没有需要同步的 Codex 历史事件",
      payload: {
        backend: "codex",
        subkind: "history_sync",
        threadId,
        eventCount: events.length,
        nextCursor: isRecord(result) ? result.nextCursor ?? null : null,
      },
      sourceId: `codex-history:${threadId}:sync`,
      turnIdOverride: `codex-history:${threadId}`,
    });
    return { ok: true, skipped: false, count: events.length };
  } catch (err) {
    protocol.emitTimeline({
      kind: "diagnostic",
      status: "error",
      title: "Codex history sync failed",
      summary: "thread/turns/list 失败，已跳过历史回补并继续当前 turn。",
      payload: {
        backend: "codex",
        subkind: "history_sync",
        method: "thread/turns/list",
        threadId,
        error: err?.message || String(err),
      },
      sourceId: `codex-history:${threadId}:sync-error`,
      turnIdOverride: `codex-history:${threadId}`,
    });
    return { ok: false, skipped: false, count: 0 };
  }
}

export function buildCodexCollaborationMode(kind, model, preset = null, reasoningEffort = null) {
  const fallbackModel = stringOrNull(model) || stringOrNull(preset?.model) || "gpt-5";
  return {
    mode: kind === "plan" ? "plan" : "default",
    settings: {
      model: fallbackModel,
      reasoning_effort:
        kind === "plan"
          ? stringOrNull(reasoningEffort) || stringOrNull(preset?.reasoning_effort) || "medium"
          : null,
      developer_instructions: null,
    },
  };
}

function readCodexSubagentInstructions(cmd) {
  const codex = readProviderRuntimeOptions(cmd?.runtimeOptions, "codex").settings;
  return stringOrNull(codex.subagentInstructions)?.trim() || null;
}

export async function startCodexAppServerTurn(
  server,
  threadId,
  prompt,
  cmd,
  cwdFn = process.cwd,
  options = {},
) {
  const settings = normalizeCodexSettings(cmd);
  const params = {
    threadId,
    input: [{ type: "text", text: prompt }],
    cwd: cmd.cwd || cwdFn(),
  };
  assignCodexSettingsParams(params, settings, cmd, { includeSandbox: true });
  assignCodexTurnParams(params, settings);
  if (options.collaborationMode) params.collaborationMode = options.collaborationMode;
  return server.request("turn/start", params);
}

export async function startCodexReview(server, threadId, review) {
  const params = {
    threadId,
    target: review.target,
    delivery: review.delivery,
  };
  return server.request("review/start", params);
}

export async function runCodexGoal(server, threadId, goal) {
  if (goal.action === "clear") {
    await server.request("thread/goal/clear", { threadId });
    return { action: goal.action, goal: null };
  }
  if (goal.action === "refresh") {
    const result = await server.request("thread/goal/get", { threadId });
    return { action: goal.action, goal: isRecord(result?.goal) ? result.goal : null };
  }
  const params = {
    threadId,
    objective: goal.objective,
    status: goal.status,
    tokenBudget: goal.tokenBudget ?? null,
  };
  const result = await server.request("thread/goal/set", params);
  return { action: goal.action, goal: isRecord(result?.goal) ? result.goal : null };
}

async function drainCodexTurnNotifications(server, ctx, timeoutMessage) {
  const startedAt = Date.now();
  while (!ctx.turnCompletedSeen) {
    const messages = server.drainNotifications();
    for (const msg of messages) {
      if (await maybeHandleCodexServerRequest(server, msg, ctx)) continue;
      const ev = normalizeCodexAppServerEvent(msg);
      if (ev) mapCodexEventToNdjson(ev, ctx);
    }
    if (Date.now() - startedAt > 1000 * 60 * 60) {
      throw new Error(timeoutMessage);
    }
    if (!ctx.turnCompletedSeen) {
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
  }
}

async function drainCodexCompactNotifications(server, ctx, threadId, timeoutMessage) {
  const startedAt = Date.now();
  while (true) {
    const messages = server.drainNotifications();
    for (const msg of messages) {
      if (await maybeHandleCodexServerRequest(server, msg, ctx)) continue;
      if (
        msg?.method === "thread/compacted" &&
        stringOrNull(msg?.params?.threadId) === threadId
      ) {
        return msg.params;
      }
      const ev = normalizeCodexAppServerEvent(msg);
      if (ev) mapCodexEventToNdjson(ev, ctx);
    }
    if (Date.now() - startedAt > 1000 * 60 * 60) {
      throw new Error(timeoutMessage);
    }
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}

async function runCodexTurnLoop(
  server,
  threadId,
  cmd,
  ctx,
  cwdFn,
  {
    initialPrompt,
    initialTurnKind,
    selectedModel,
    planPreset,
    timeoutMessage = "Codex app-server turn timed out",
  },
) {
  let nextPrompt = initialPrompt;
  let nextTurnKind = initialTurnKind === "plan" ? "plan" : "default";
  let shouldStartTurn = true;
  const subagentInstructions = readCodexSubagentInstructions(cmd);
  const selectedReasoningEffort = normalizeCodexSettings(cmd).reasoningEffort;
  while (shouldStartTurn) {
    shouldStartTurn = false;
    ctx.activeTurnKind = nextTurnKind;
    const collaborationMode =
      nextTurnKind === "plan"
        ? buildCodexCollaborationMode("plan", selectedModel, planPreset, selectedReasoningEffort)
        : buildCodexCollaborationMode("default", selectedModel, null);
    if (subagentInstructions) {
      collaborationMode.settings.developer_instructions = subagentInstructions;
    }
    await flushCodexRuntimeSettings(ctx);
    const startedTurn = await startCodexAppServerTurn(
      server,
      threadId,
      nextPrompt,
      cmd,
      cwdFn,
      { collaborationMode },
    );
    ctx.currentTurnId = codexTurnIdFromStartResult(startedTurn) || ctx.currentTurnId;
    if (ctx.interruptRequested) interruptActiveCodexTurn(ctx);
    await drainCodexTurnNotifications(server, ctx, timeoutMessage);
    if (ctx.turnInterruptedSeen) {
      continue;
    }
    if (ctx.turnFailedSeen) {
      continue;
    }
    if (nextTurnKind === "plan") {
      if (!emitCodexPlanApprovalRequired(ctx)) {
        throw new Error("Codex plan mode completed without a readable plan");
      }
      const result = await runCodexUiInteraction(ctx, PLAN_APPROVAL_INTERACTION_KIND, () =>
        ctx.interactions.requestAskUser(buildPlanApprovalSpec("codex"), {
          backend: "codex",
          emitTimelineEvent: false,
        }));
      resolveCodexPlanApproval(ctx, result);
      if (ctx.planRevisionRequest) {
        const revisionRequest = ctx.planRevisionRequest;
        resetCodexContextForNextTurn(ctx);
        nextPrompt = buildCodexPlanRevisionPrompt(revisionRequest);
        nextTurnKind = "plan";
        shouldStartTurn = true;
      } else if (ctx.planApproved) {
        const approvedPlan = ctx.pendingPlanPayload?.plan || "";
        resetCodexContextForNextTurn(ctx, { planMode: false });
        nextPrompt = buildCodexPlanExecutionPrompt(approvedPlan);
        nextTurnKind = "default";
        shouldStartTurn = true;
      } else if (ctx.planCancelled) {
        ctx.protocol.emit({
          type: RUNNER_DONE_EVENT_TYPE,
          sessionId: ctx.lastThreadId,
          subtype: "cancelled",
        });
      }
    }
  }
  return !ctx.planCancelled;
}

function interruptActiveCodexTurn(ctx) {
  if (!ctx) return;
  ctx.interruptRequested = true;
  const threadId = ctx.lastThreadId;
  if (ctx.interruptSent || ctx.turnCompletedSeen) return;
  if (!ctx.server || !threadId || !ctx.currentTurnId) return;
  ctx.interruptSent = true;
  ctx.server.request("turn/interrupt", {
    threadId,
    turnId: ctx.currentTurnId,
  }).catch((err) => {
    ctx.interruptSent = false;
    ctx.protocol.emitTimeline({
      kind: "error",
      status: "error",
      title: "Codex interrupt failed",
      summary: err?.message || String(err),
      payload: {
        backend: "codex",
        subkind: "interrupt",
        threadId,
        turnId: ctx.currentTurnId,
      },
      sourceId: `codex:interrupt:error:${ctx.currentTurnId || Date.now()}`,
    });
  });
}

function codexBatchApplyTimelinePayload(threadId, workflow, extra = {}) {
  return {
    backend: "codex",
    subkind: "batch_apply",
    method: "turn/start",
    threadId,
    sourceTurnId: workflow.sourceTurnId,
    sourceKind: workflow.sourceKind,
    forcedPlan: true,
    ...extra,
  };
}

function emitCodexDiagnostic(ctx, {
  status,
  title,
  summary,
  payload,
  sourceId,
  error = null,
}) {
  ctx.protocol.emitTimeline({
    kind: "diagnostic",
    status,
    title,
    summary: error ? error?.message || String(error) : summary,
    payload: {
      ...payload,
      ...(error ? { error: error?.message || String(error) } : {}),
    },
    sourceId,
  });
}

async function runCodexReviewWorkflow(server, threadId, cmd, ctx) {
  const review = readCodexReviewWorkflow(cmd);
  if (!review) return false;
  await flushCodexRuntimeSettings(ctx);
  const result = await startCodexReview(server, threadId, review);
  ctx.currentTurnId = codexTurnIdFromStartResult(result) || ctx.currentTurnId;
  ctx.workflowSource = {
    sourceKind: "review",
    codexTurnId: ctx.currentTurnId,
    target: review.target,
  };
  emitCodexDiagnostic(ctx, {
    status: "started",
    title: "Codex review started",
    summary: codexReviewTargetSummary(review.target),
    payload: {
      backend: "codex",
      subkind: "review",
      method: "review/start",
      target: review.target,
      delivery: review.delivery,
      hasInstructions: Boolean(review.instructions),
    },
    sourceId: `codex:review:start:${ctx.currentTurnId || codexReviewTargetSummary(review.target)}`,
  });
  await drainCodexTurnNotifications(server, ctx, "Codex review timed out");
  return true;
}

async function runCodexFixSuggestionWorkflow(server, threadId, cmd, ctx, cwdFn = process.cwd) {
  const workflow = readCodexFixSuggestionWorkflow(cmd);
  if (!workflow) return false;
  await flushCodexRuntimeSettings(ctx);
  const prompt = buildCodexFixSuggestionPrompt(workflow, {
    ...cmd,
    cwd: cmd.cwd || cwdFn(),
  });
  const turnCmd = codexFixSuggestionEffectiveTurnCmd(cmd, workflow);
  const targetSummary = codexReviewTargetSummary(workflow.target);
  const timelinePayload = {
    backend: "codex",
    subkind: "fix_suggestion",
    method: "turn/start",
    threadId,
    target: workflow.target,
    mode: workflow.mode,
  };
  emitCodexDiagnostic(ctx, {
    status: "started",
    title: "Codex fix suggestion started",
    summary: targetSummary,
    payload: {
      ...timelinePayload,
      hasInstructions: Boolean(workflow.instructions),
      effectivePermission: turnCmd.permission,
    },
    sourceId: `codex:fix-suggestion:start:${threadId}:${targetSummary}`,
  });
  try {
    const collaborationMode = buildCodexCollaborationMode(
      "default",
      normalizeCodexSettings(turnCmd).model,
      null,
    );
    const subagentInstructions = readCodexSubagentInstructions(turnCmd);
    if (subagentInstructions) {
      collaborationMode.settings.developer_instructions = subagentInstructions;
    }
    const startedTurn = await startCodexAppServerTurn(
      server,
      threadId,
      prompt,
      turnCmd,
      cwdFn,
      { collaborationMode },
    );
    ctx.currentTurnId = codexTurnIdFromStartResult(startedTurn) || ctx.currentTurnId;
    ctx.workflowSource = {
      sourceKind: "fix_suggestion",
      codexTurnId: ctx.currentTurnId,
      target: workflow.target,
      mode: workflow.mode,
    };
    await drainCodexTurnNotifications(server, ctx, "Codex fix suggestion timed out");
    if (ctx.turnFailedSeen) {
      throw new Error("Codex fix suggestion turn failed");
    }
  } catch (err) {
    emitCodexDiagnostic(ctx, {
      status: "error",
      title: "Codex fix suggestion failed",
      payload: timelinePayload,
      sourceId: `codex:fix-suggestion:error:${threadId}:${targetSummary}`,
      error: err,
    });
    throw err;
  }
  emitCodexDiagnostic(ctx, {
    status: "success",
    title: "Codex fix suggestion completed",
    summary: targetSummary,
    payload: timelinePayload,
    sourceId: `codex:fix-suggestion:completed:${threadId}:${targetSummary}`,
  });
  return true;
}

async function runCodexBatchApplyWorkflow(server, threadId, cmd, ctx, cwdFn = process.cwd) {
  const workflow = readCodexBatchApplyWorkflow(cmd);
  if (!workflow) return false;
  await flushCodexRuntimeSettings(ctx);
  const prompt = buildCodexBatchApplyPrompt(workflow, {
    ...cmd,
    cwd: cmd.cwd || cwdFn(),
  });
  const selectedModel = normalizeCodexSettings(cmd).model || ctx.selectedModel || null;
  const planPreset = await requireCodexPlanModePreset(
    server,
    ctx.protocol,
    `codex:batch-apply:plan-mode:${threadId}:${workflow.sourceTurnId}`,
  );
  emitCodexDiagnostic(ctx, {
    status: "started",
    title: "Codex batch apply started",
    summary: workflow.sourceKind,
    payload: codexBatchApplyTimelinePayload(threadId, workflow, {
      hasInstructions: Boolean(workflow.instructions),
    }),
    sourceId: `codex:batch-apply:start:${threadId}:${workflow.sourceTurnId}`,
  });
  try {
    const completed = await runCodexTurnLoop(
      server,
      threadId,
      cmd,
      ctx,
      cwdFn,
      {
        initialPrompt: prompt,
        initialTurnKind: "plan",
        selectedModel,
        planPreset,
        timeoutMessage: "Codex batch apply timed out",
      },
    );
    if (!completed) return true;
    if (ctx.turnFailedSeen) {
      throw new Error("Codex batch apply turn failed");
    }
  } catch (err) {
    emitCodexDiagnostic(ctx, {
      status: "error",
      title: "Codex batch apply failed",
      payload: codexBatchApplyTimelinePayload(threadId, workflow),
      sourceId: `codex:batch-apply:error:${threadId}:${workflow.sourceTurnId}`,
      error: err,
    });
    throw err;
  }
  emitCodexDiagnostic(ctx, {
    status: "success",
    title: "Codex batch apply completed",
    summary: workflow.sourceKind,
    payload: codexBatchApplyTimelinePayload(threadId, workflow),
    sourceId: `codex:batch-apply:completed:${threadId}:${workflow.sourceTurnId}`,
  });
  finalizeCodexRunContext(ctx);
  return true;
}

async function runCodexTaskWorkflow(server, threadId, cmd, ctx, cwdFn = process.cwd) {
  const workflow = readCodexTaskWorkflow(cmd);
  if (!workflow) return false;
  const turnCmd = {
    ...cmd,
    cwd: cmd.cwd || cwdFn(),
  };
  const prompt = buildCodexTaskWorkflowPrompt(workflow, turnCmd);
  const selectedModel = normalizeCodexSettings(turnCmd).model || ctx.selectedModel || null;
  const timelinePayload = {
    backend: "codex",
    subkind: "task_workflow",
    method: "turn/start",
    threadId,
    kind: workflow.kind,
    hasInstructions: Boolean(workflow.instructions),
  };
  emitCodexDiagnostic(ctx, {
    status: "started",
    title: "Codex task workflow started",
    summary: workflow.kind,
    payload: timelinePayload,
    sourceId: `codex:task-workflow:start:${threadId}:${workflow.kind}`,
  });
  try {
    const completed = await runCodexTurnLoop(
      server,
      threadId,
      turnCmd,
      ctx,
      cwdFn,
      {
        initialPrompt: prompt,
        initialTurnKind: "default",
        selectedModel,
        timeoutMessage: "Codex task workflow timed out",
      },
    );
    if (!completed) return true;
    if (ctx.turnFailedSeen) {
      throw new Error("Codex task workflow turn failed");
    }
  } catch (err) {
    emitCodexDiagnostic(ctx, {
      status: "error",
      title: "Codex task workflow failed",
      payload: timelinePayload,
      sourceId: `codex:task-workflow:error:${threadId}:${workflow.kind}`,
      error: err,
    });
    throw err;
  }
  emitCodexDiagnostic(ctx, {
    status: "success",
    title: "Codex task workflow completed",
    summary: workflow.kind,
    payload: timelinePayload,
    sourceId: `codex:task-workflow:completed:${threadId}:${workflow.kind}`,
  });
  return true;
}

function emitCodexGoalWorkflowTimeline(ctx, action, goal) {
  const cleared = action === "clear";
  ctx.protocol.emitTimeline({
    kind: "goal",
    status: cleared ? "cancelled" : "info",
    title: cleared ? "Lilia Goal cleared" : "Lilia Goal updated",
    summary: cleared
      ? "已清除 Lilia Goal"
      : stringOrNull(goal?.objective) || "Lilia Goal",
    payload: {
      backend: "codex",
      subkind: "thread_goal",
      action,
      cleared,
      goal: goal ?? null,
      goalStatus: stringOrNull(goal?.status),
      threadId: ctx.threadId,
    },
    sourceId: `codex:goal:${action}:${ctx.threadId}`,
  });
}

async function runCodexGoalWorkflow(server, threadId, cmd, ctx) {
  const goal = readCodexGoalWorkflow(cmd);
  if (!goal) return false;
  await flushCodexRuntimeSettings(ctx);
  const result = await runCodexGoal(server, threadId, goal);
  emitCodexGoalWorkflowTimeline(ctx, result.action, result.goal);
  return true;
}

const CODEX_COMPACT_TIMELINE = {
  subkind: "compact",
  method: "thread/compact/start",
  sourcePrefix: "codex:compact",
  startedTitle: "Codex compact started",
  successTitle: "Codex compact completed",
  errorTitle: "Codex compact failed",
  startedSummary: "正在压缩 Codex thread 上下文",
  successSummary: "Codex thread 上下文已压缩",
};

const CODEX_BACKGROUND_TERMINALS_CLEAN_TIMELINE = {
  subkind: "background_terminals_clean",
  method: "thread/backgroundTerminals/clean",
  sourcePrefix: "codex:background-terminals:clean",
  startedTitle: "Codex background terminals clean started",
  successTitle: "Codex background terminals cleaned",
  errorTitle: "Codex background terminals clean failed",
  startedSummary: "正在清理 Codex thread 后台终端",
  successSummary: "Codex thread 后台终端已清理",
};

async function runCodexCompactWorkflow(server, threadId, cmd, ctx) {
  if (!readCodexCompactWorkflow(cmd)) return false;
  await flushCodexRuntimeSettings(ctx);
  emitCodexWorkflowTimeline(ctx, CODEX_COMPACT_TIMELINE, threadId, "started");
  try {
    await server.request("thread/compact/start", { threadId });
    await drainCodexCompactNotifications(
      server,
      ctx,
      threadId,
      "Codex compact timed out",
    );
  } catch (err) {
    emitCodexWorkflowTimeline(ctx, CODEX_COMPACT_TIMELINE, threadId, "error", err);
    throw err;
  }
  emitCodexWorkflowTimeline(ctx, CODEX_COMPACT_TIMELINE, threadId, "success");
  emitCodexWorkflowDone(ctx, threadId);
  return true;
}

async function runCodexBackgroundTerminalsCleanWorkflow(server, threadId, cmd, ctx) {
  return runCodexRequestWorkflow(server, threadId, cmd, ctx, {
    read: readCodexBackgroundTerminalsCleanWorkflow,
    timeline: CODEX_BACKGROUND_TERMINALS_CLEAN_TIMELINE,
    params: () => ({ threadId }),
  });
}

const CODEX_MEMORY_MODE_TIMELINE = {
  subkind: "memory_mode",
  method: "thread/memoryMode/set",
  sourcePrefix: "codex:memory-mode",
  startedTitle: "Codex memory mode update started",
  successTitle: "Codex memory mode updated",
  errorTitle: "Codex memory mode update failed",
  startedSummary: "正在更新 Codex thread memory mode",
  successSummary: "Codex thread memory mode 已更新",
};

const CODEX_MEMORY_RESET_TIMELINE = {
  subkind: "memory_reset",
  method: "memory/reset",
  sourcePrefix: "codex:memory-reset",
  startedTitle: "Codex memory reset started",
  successTitle: "Codex memory reset completed",
  errorTitle: "Codex memory reset failed",
  startedSummary: "正在重置 Codex memory",
  successSummary: "Codex memory 已重置",
};

const CODEX_THREAD_FORK_TIMELINE = {
  subkind: "thread_fork",
  method: "thread/fork",
  sourcePrefix: "codex:thread-fork",
  startedTitle: "Codex thread fork started",
  successTitle: "Codex thread fork completed",
  errorTitle: "Codex thread fork failed",
  startedSummary: "正在 fork 当前 Codex thread",
  successSummary: "Codex thread 已 fork",
};

async function runCodexMemoryModeWorkflow(server, threadId, cmd, ctx) {
  return runCodexRequestWorkflow(server, threadId, cmd, ctx, {
    read: readCodexMemoryModeWorkflow,
    timeline: (workflow) => ({
      ...CODEX_MEMORY_MODE_TIMELINE,
      startedSummary: `正在${workflow.mode === "enabled" ? "启用" : "关闭"} Codex memory mode`,
      successSummary: `Codex memory mode 已${workflow.mode === "enabled" ? "启用" : "关闭"}`,
    }),
    params: (workflow) => ({ threadId, mode: workflow.mode }),
  });
}

async function runCodexMemoryResetWorkflow(server, threadId, cmd, ctx) {
  return runCodexRequestWorkflow(server, threadId, cmd, ctx, {
    read: readCodexMemoryResetWorkflow,
    timeline: CODEX_MEMORY_RESET_TIMELINE,
    params: () => null,
  });
}

function buildCodexThreadForkParams(threadId, cmd, command, cwdFn) {
  const settings = normalizeCodexSettings(cmd);
  const params = {
    threadId,
    cwd: cmd.cwd || cwdFn(),
    excludeTurns: command.excludeTurns,
  };
  if (command.sourceTurnId) params.turnId = command.sourceTurnId;
  assignCodexSettingsParams(params, settings, cmd, { includeSandbox: true });
  assignCodexAdvancedThreadParams(params, settings);
  return params;
}

async function forkCodexThreadForCommand(server, threadId, cmd, ctx, command, cwdFn, options = {}) {
  await flushCodexRuntimeSettings(ctx);
  emitCodexWorkflowTimeline(ctx, CODEX_THREAD_FORK_TIMELINE, threadId, "started");
  const params = buildCodexThreadForkParams(threadId, cmd, command, cwdFn);
  try {
    const result = await server.request("thread/fork", params);
    const forkedThreadId = codexThreadIdFromForkResult(result);
    if (!forkedThreadId) throw new Error("Codex thread/fork did not return a thread id");
    ctx.lastThreadId = forkedThreadId;
    ctx.threadId = forkedThreadId;
    if (options.updateCommandResume) cmd.resumeSessionId = forkedThreadId;
    ctx.protocol.emitTimeline({
      kind: "diagnostic",
      status: "success",
      title: CODEX_THREAD_FORK_TIMELINE.successTitle,
      summary: options.summary || CODEX_THREAD_FORK_TIMELINE.successSummary,
      payload: {
        backend: "codex",
        subkind: CODEX_THREAD_FORK_TIMELINE.subkind,
        method: CODEX_THREAD_FORK_TIMELINE.method,
        sourceThreadId: threadId,
        sourceTurnId: command.sourceTurnId,
        threadId: forkedThreadId,
        excludeTurns: command.excludeTurns,
        mode: command.mode,
      },
      sourceId: options.sourceId?.(forkedThreadId)
        || `${CODEX_THREAD_FORK_TIMELINE.sourcePrefix}:completed:${threadId}:${forkedThreadId}`,
    });
    return forkedThreadId;
  } catch (err) {
    emitCodexWorkflowTimeline(ctx, CODEX_THREAD_FORK_TIMELINE, threadId, "error", err);
    throw err;
  }
}

async function runCodexThreadForkRuntimeCommand(server, threadId, cmd, ctx, cwdFn = process.cwd) {
  const command = readCodexThreadForkRuntimeCommand(cmd);
  if (!command) return false;
  if (command.continueAfterFork) return false;
  const forkedThreadId = await forkCodexThreadForCommand(server, threadId, cmd, ctx, command, cwdFn);
  emitCodexWorkflowDone(ctx, forkedThreadId);
  return true;
}

async function runCodexPromptSessionFork(server, threadId, cmd, ctx, cwdFn = process.cwd) {
  const command = readCodexThreadForkRuntimeCommand(cmd);
  if (!command?.continueAfterFork) return threadId;
  if (!threadId) throw new Error("当前 Codex task 没有可 fork 的 thread");
  return forkCodexThreadForCommand(server, threadId, cmd, ctx, command, cwdFn, {
    updateCommandResume: true,
    summary: command.mode === "continue"
      ? "Codex thread 已从所选轮次继续"
      : "Codex thread 已从所选轮次 fork，本轮将在分叉 thread 中继续",
    sourceId: (forkedThreadId) =>
      `${CODEX_THREAD_FORK_TIMELINE.sourcePrefix}:anchored:${threadId}:${command.sourceTurnId}:${forkedThreadId}`,
  });
}

function shouldRunCodexAutoSessionFork(cmd) {
  return cmd?.autoSessionFork === true &&
    !normalizeSessionForkCommand(readRunnerRuntimeCommand(cmd));
}

async function runCodexAutoSessionFork(server, threadId, cmd, ctx, cwdFn = process.cwd) {
  if (!shouldRunCodexAutoSessionFork(cmd)) return threadId;
  if (!threadId) throw new Error("辅助模型建议会话分叉，但当前 Codex task 没有可 fork 的 thread");
  await flushCodexRuntimeSettings(ctx);
  emitCodexWorkflowTimeline(ctx, CODEX_THREAD_FORK_TIMELINE, threadId, "started");
  const settings = normalizeCodexSettings(cmd);
  const params = {
    threadId,
    cwd: cmd.cwd || cwdFn(),
    excludeTurns: true,
  };
  assignCodexSettingsParams(params, settings, cmd, { includeSandbox: true });
  assignCodexAdvancedThreadParams(params, settings);
  try {
    const result = await server.request("thread/fork", params);
    const forkedThreadId = codexThreadIdFromForkResult(result);
    if (!forkedThreadId) throw new Error("Codex thread/fork did not return a thread id");
    ctx.lastThreadId = forkedThreadId;
    ctx.threadId = forkedThreadId;
    cmd.resumeSessionId = forkedThreadId;
    ctx.protocol.emitTimeline({
      kind: "diagnostic",
      status: "success",
      title: CODEX_THREAD_FORK_TIMELINE.successTitle,
      summary: "Codex thread 已 fork，本轮将在分叉 thread 中继续",
      payload: {
        backend: "codex",
        subkind: CODEX_THREAD_FORK_TIMELINE.subkind,
        method: CODEX_THREAD_FORK_TIMELINE.method,
        sourceThreadId: threadId,
        threadId: forkedThreadId,
        excludeTurns: true,
        autoTurnDecision: true,
      },
      sourceId: `${CODEX_THREAD_FORK_TIMELINE.sourcePrefix}:auto:${threadId}:${forkedThreadId}`,
    });
    return forkedThreadId;
  } catch (err) {
    emitCodexWorkflowTimeline(ctx, CODEX_THREAD_FORK_TIMELINE, threadId, "error", err);
    throw err;
  }
}

async function runCodexConfigDiagnosticsWorkflow(server, threadId, cmd, ctx, cwdFn = process.cwd) {
  const workflow = readCodexConfigDiagnosticsWorkflow(cmd);
  if (!workflow) return false;
  await flushCodexRuntimeSettings(ctx);
  const configParams = {
    cwd: cmd.cwd || cwdFn(),
    includeLayers: workflow.includeLayers,
  };
  try {
    const [configResult, requirementsResult] = await Promise.all([
      server.request("config/read", configParams),
      server.request("configRequirements/read", null),
    ]);
    ctx.protocol.emitTimeline({
      kind: "diagnostic",
      status: "info",
      title: "Codex config diagnostics",
      summary: "已读取 Codex config 与 requirements",
      payload: {
        backend: "codex",
        subkind: "config_diagnostics",
        methods: ["config/read", "configRequirements/read"],
        threadId,
        includeLayers: workflow.includeLayers,
        config: compactCodexConfig(configResult?.config),
        apps: isRecord(configResult?.config) ? configResult.config.apps ?? null : null,
        origins: isRecord(configResult) ? configResult.origins ?? null : null,
        layers: workflow.includeLayers && isRecord(configResult) ? configResult.layers ?? null : null,
        requirements: compactConfigRequirements(requirementsResult?.requirements),
      },
      sourceId: `codex:config-diagnostics:${threadId}`,
    });
  } catch (err) {
    ctx.protocol.emitTimeline({
      kind: "diagnostic",
      status: "error",
      title: "Codex config diagnostics failed",
      summary: err?.message || String(err),
      payload: {
        backend: "codex",
        subkind: "config_diagnostics",
        methods: ["config/read", "configRequirements/read"],
        threadId,
        error: err?.message || String(err),
      },
      sourceId: `codex:config-diagnostics:error:${threadId}`,
    });
    throw err;
  }
  emitCodexWorkflowDone(ctx, threadId);
  return true;
}

async function runCodexRuntimeSettingsCommand(server, threadId, cmd, ctx) {
  const command = readCodexRuntimeSettingsCommand(cmd);
  if (!command) return false;
  await flushCodexRuntimeSettings(ctx);
  const effectiveCmd = codexSettingsCmdFromRuntimeCommand(cmd, command);
  const params = buildCodexThreadSettingsParams(threadId, effectiveCmd);
  const settingsKeys = Object.keys(params).filter((key) => key !== "threadId");
  if (command.action === "diagnose") {
    ctx.protocol.emitTimeline({
      kind: "diagnostic",
      status: "info",
      title: "Codex provider settings diagnostics",
      summary: settingsKeys.length
        ? `当前可应用 ${settingsKeys.length} 项 Codex provider settings`
        : "当前没有可应用的 Codex provider settings 更新",
      payload: {
        backend: "codex",
        subkind: "provider_settings",
        action: command.action,
        threadId,
        settingsKeys,
        ignoredProviderKeys: command.ignoredProviderKeys,
        params,
      },
      sourceId: `codex:provider-settings:diagnose:${threadId}`,
    });
    emitCodexWorkflowDone(ctx, threadId);
    return true;
  }

  try {
    await server.request("thread/settings/update", params);
    cmd.permission = effectiveCmd.permission;
    cmd.model = effectiveCmd.model;
    cmd.runtimeOptions = effectiveCmd.runtimeOptions;
    ctx.protocol.emitTimeline({
      kind: "diagnostic",
      status: "success",
      title: "Codex provider settings updated",
      summary: settingsKeys.length
        ? `已更新 ${settingsKeys.length} 项 Codex provider settings`
        : "Codex provider settings 已更新",
      payload: {
        backend: "codex",
        subkind: "provider_settings",
        action: command.action,
        method: "thread/settings/update",
        threadId,
        settingsKeys,
        ignoredProviderKeys: command.ignoredProviderKeys,
        params,
      },
      sourceId: `codex:provider-settings:update:${threadId}`,
    });
  } catch (err) {
    ctx.protocol.emitTimeline({
      kind: "diagnostic",
      status: "error",
      title: "Codex provider settings update failed",
      summary: err?.message || String(err),
      payload: {
        backend: "codex",
        subkind: "provider_settings",
        action: command.action,
        method: "thread/settings/update",
        threadId,
        settingsKeys,
        ignoredProviderKeys: command.ignoredProviderKeys,
        error: err?.message || String(err),
      },
      sourceId: `codex:provider-settings:error:${threadId}`,
    });
    throw err;
  }
  emitCodexWorkflowDone(ctx, threadId);
  return true;
}

function emitCodexRemoteEnvironmentTimeline(ctx, input) {
  const failed = input.status === "error";
  const errorMessage = failed ? input.error?.message || String(input.error) : null;
  ctx.protocol.emitTimeline({
    kind: "diagnostic",
    status: input.status,
    title: input.title,
    summary: errorMessage || input.summary,
    payload: {
      backend: "codex",
      subkind: REMOTE_ENVIRONMENT_COMMAND_TYPE,
      action: input.command.action,
      threadId: input.threadId,
      ...(input.method ? { method: input.method } : {}),
      ...(input.environmentId ? { environmentId: input.environmentId } : {}),
      ...(input.environments ? { environments: input.environments } : {}),
      ...(input.result !== undefined ? { result: input.result } : {}),
      ...(failed ? { error: errorMessage } : { native: true }),
      ...(input.extraPayload || {}),
    },
    sourceId: input.sourceId,
  });
}

function codexProcessIdFromSpawnResult(result) {
  if (!isRecord(result)) return "";
  return stringOrNull(result.processId) ||
    stringOrNull(result.processID) ||
    stringOrNull(result.pid) ||
    stringOrNull(result.id) ||
    "";
}

function processNotificationMethod(msg) {
  return stringOrNull(msg?.method) || stringOrNull(msg?.type) || "";
}

function processNotificationParams(msg) {
  return isRecord(msg?.params) ? msg.params : isRecord(msg) ? msg : {};
}

function processNotificationId(msg) {
  const params = processNotificationParams(msg);
  return stringOrNull(params.processId) ||
    stringOrNull(params.processID) ||
    stringOrNull(params.pid) ||
    stringOrNull(params.id) ||
    "";
}

function processOutputDelta(msg) {
  const params = processNotificationParams(msg);
  return stringOrNull(params.delta) ||
    stringOrNull(params.text) ||
    stringOrNull(params.chunk) ||
    stringOrNull(params.output) ||
    "";
}

function processSessionSourceId(action, processId, suffix = "") {
  return `codex:process-session:${action}:${processId || "unknown"}${suffix ? `:${suffix}` : ""}`;
}

function emitCodexProcessSessionTimeline(ctx, input) {
  const failed = input.status === "error";
  const errorMessage = failed ? input.error?.message || String(input.error) : null;
  ctx.protocol.emitTimeline({
    kind: "command",
    status: input.status,
    title: input.title,
    summary: errorMessage || input.summary || "",
    payload: {
      backend: "codex",
      subkind: "process_session",
      action: input.action,
      method: input.method,
      threadId: input.threadId,
      processId: input.processId || null,
      ...(input.command ? { command: input.command } : {}),
      ...(input.cwd ? { cwd: input.cwd } : {}),
      ...(input.stream ? { stream: input.stream } : {}),
      ...(input.delta ? { delta: input.delta } : {}),
      ...(typeof input.exitCode === "number" ? { exitCode: input.exitCode } : {}),
      ...(input.result !== undefined ? { result: input.result } : {}),
      ...(failed ? { error: errorMessage } : { native: true }),
    },
    sourceId: input.sourceId,
  });
}

function codexProcessSessionSpawnParams(command, cmd, cwdFn) {
  const params = {
    command: command.command,
    cwd: command.cwd || cmd.cwd || cwdFn(),
  };
  if (command.env) params.env = command.env;
  if (command.tty) params.tty = true;
  if (command.rows) params.rows = command.rows;
  if (command.cols) params.cols = command.cols;
  if (command.permissionProfile) params.permissionProfile = command.permissionProfile;
  return params;
}

function resolveCodexProcessId(command, ctx) {
  const processId = command.processId || ctx.activeProcessSession?.processId || "";
  if (!processId) {
    throw new Error("Lilia process session command requires processId");
  }
  return processId;
}

async function runCodexProcessSessionControl(server, command, ctx) {
  const processId = resolveCodexProcessId(command, ctx);
  if (command.action === "write_stdin") {
    return server.request("process/writeStdin", {
      processId,
      stdin: command.stdin,
    });
  }
  if (command.action === "kill") {
    return server.request("process/kill", { processId });
  }
  if (command.action === "resize_pty") {
    if (!command.rows || !command.cols) {
      throw new Error("Lilia process session resize_pty requires rows and cols");
    }
    return server.request("process/resizePty", {
      processId,
      rows: command.rows,
      cols: command.cols,
    });
  }
}

function registerCodexProcessSessionCommand(ctx, server, threadId) {
  return ctx.interactions?.handleProcessSessionCommand?.((rawCommand) => {
    let command;
    try {
      command = readCodexProcessSessionCommand(rawCommand);
      if (!command || command.action === "spawn") return;
    } catch (err) {
      emitCodexProcessSessionTimeline(ctx, {
        status: "error",
        title: "Codex process session command rejected",
        action: "control",
        method: "process/session",
        threadId,
        processId: ctx.activeProcessSession?.processId || "",
        error: err,
        sourceId: processSessionSourceId("control-error", ctx.activeProcessSession?.processId, Date.now()),
      });
      return;
    }
    const promise = runCodexProcessSessionControl(server, command, ctx)
      .catch((err) => {
        emitCodexProcessSessionTimeline(ctx, {
          status: "error",
          title: "Codex process session command failed",
          action: command.action,
          method: "process/session",
          threadId,
          processId: command.processId || ctx.activeProcessSession?.processId || "",
          error: err,
          sourceId: processSessionSourceId(command.action, command.processId || ctx.activeProcessSession?.processId, Date.now()),
        });
      });
    ctx.processSessionControlPromises.push(promise);
  }) ?? (() => {});
}

async function drainCodexProcessSessionNotifications(server, ctx, processId, threadId) {
  while (!ctx.activeProcessSession?.exited) {
    const messages = server.drainNotifications?.() || [];
    for (const msg of messages) {
      if (await maybeHandleCodexServerRequest(server, msg, ctx)) continue;
      const method = processNotificationMethod(msg);
      if (!method.startsWith("process/")) {
        const ev = normalizeCodexAppServerEvent(msg);
        if (ev) mapCodexEventToNdjson(ev, ctx);
        continue;
      }
      const params = processNotificationParams(msg);
      const eventProcessId = processNotificationId(msg);
      if (eventProcessId && eventProcessId !== processId) continue;
      if (method === "process/outputDelta") {
        const delta = processOutputDelta(msg);
        emitCodexProcessSessionTimeline(ctx, {
          status: "running",
          title: "Codex process output",
          summary: oneLineSummary(delta),
          action: "output",
          method,
          threadId,
          processId,
          stream: stringOrNull(params.stream) || (params.fd === 2 ? "stderr" : "stdout"),
          delta,
          sourceId: processSessionSourceId("output", processId, Date.now()),
        });
      } else if (method === "process/exited") {
        const exitCode = numberOrNull(params.exitCode) ?? numberOrNull(params.code) ?? 0;
        ctx.activeProcessSession.exited = true;
        emitCodexProcessSessionTimeline(ctx, {
          status: exitCode === 0 ? "success" : "error",
          title: "Codex process exited",
          summary: `exit ${exitCode}`,
          action: "exited",
          method,
          threadId,
          processId,
          exitCode,
          sourceId: processSessionSourceId("exited", processId),
        });
      }
    }
    if (!ctx.activeProcessSession?.exited) {
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
  }
  await Promise.allSettled(ctx.processSessionControlPromises);
}

async function runCodexProcessSessionCommand(server, threadId, cmd, ctx, cwdFn = process.cwd) {
  const command = readCodexProcessSessionCommand(cmd);
  if (!command) return false;
  if (command.action !== "spawn") {
    await runCodexProcessSessionControl(server, command, ctx);
    emitCodexWorkflowDone(ctx, threadId);
    return true;
  }
  await flushCodexRuntimeSettings(ctx);
  const params = codexProcessSessionSpawnParams(command, cmd, cwdFn);
  let result;
  try {
    result = await server.request("process/spawn", params);
  } catch (err) {
    emitCodexProcessSessionTimeline(ctx, {
      status: "error",
      title: "Codex process spawn failed",
      action: command.action,
      method: "process/spawn",
      threadId,
      command: command.command,
      cwd: params.cwd,
      error: err,
      sourceId: processSessionSourceId("spawn-error", "", Date.now()),
    });
    throw err;
  }
  const processId = codexProcessIdFromSpawnResult(result);
  ctx.activeProcessSession = { processId, exited: false };
  emitCodexProcessSessionTimeline(ctx, {
    status: "started",
    title: command.command,
    summary: "Codex process session started",
    action: command.action,
    method: "process/spawn",
    threadId,
    processId,
    command: command.command,
    cwd: params.cwd,
    result,
    sourceId: processSessionSourceId("spawn", processId || Date.now()),
  });
  if (!processId) {
    emitCodexWorkflowDone(ctx, threadId);
    return true;
  }
  await drainCodexProcessSessionNotifications(server, ctx, processId, threadId);
  emitCodexWorkflowDone(ctx, threadId);
  return true;
}

async function runCodexRemoteEnvironmentCommand(server, threadId, cmd, ctx) {
  const command = readCodexRemoteEnvironmentCommand(cmd);
  if (!command) return false;
  await flushCodexRuntimeSettings(ctx);
  if (command.action === "diagnose") {
    const environments = normalizeCodexSettings(cmd).environments || [];
    emitCodexRemoteEnvironmentTimeline(ctx, {
      status: "info",
      title: "Codex remote environment diagnostics",
      summary: environments.length
        ? `当前配置 ${environments.length} 个 Codex remote environment`
        : "当前没有配置 Codex remote environment",
      threadId,
      command,
      environments,
      extraPayload: {
        supportedActions: REMOTE_ENVIRONMENT_ACTIONS,
      },
      sourceId: `codex:remote-environment:diagnose:${threadId}`,
    });
    emitCodexWorkflowDone(ctx, threadId);
    return true;
  }

  if (command.action === "add") {
    try {
      const result = await server.request("environment/add", command.environment);
      emitCodexRemoteEnvironmentTimeline(ctx, {
        status: "success",
        title: "Codex remote environment added",
        summary: command.environmentId || "Codex remote environment 已注册",
        threadId,
        command,
        method: "environment/add",
        environmentId: command.environmentId || stringOrNull(result?.environmentId) || stringOrNull(result?.environment?.id),
        result,
        sourceId: `codex:remote-environment:add:${threadId}:${command.environmentId || Date.now()}`,
      });
    } catch (err) {
      emitCodexRemoteEnvironmentTimeline(ctx, {
        status: "error",
        title: "Codex remote environment add failed",
        threadId,
        command,
        method: "environment/add",
        environmentId: command.environmentId,
        error: err,
        sourceId: `codex:remote-environment:add:error:${threadId}`,
      });
      throw err;
    }
    emitCodexWorkflowDone(ctx, threadId);
    return true;
  }

  const environments = [{ id: command.environmentId }];
  try {
    await server.request("thread/settings/update", {
      threadId,
      environments,
    });
    emitCodexRemoteEnvironmentTimeline(ctx, {
      status: "success",
      title: "Codex remote environment selected",
      summary: command.environmentId,
      threadId,
      command,
      method: "thread/settings/update",
      environmentId: command.environmentId,
      environments,
      sourceId: `codex:remote-environment:select:${threadId}:${command.environmentId}`,
    });
  } catch (err) {
    emitCodexRemoteEnvironmentTimeline(ctx, {
      status: "error",
      title: "Codex remote environment select failed",
      threadId,
      command,
      method: "thread/settings/update",
      environmentId: command.environmentId,
      error: err,
      sourceId: `codex:remote-environment:select:error:${threadId}:${command.environmentId}`,
    });
    throw err;
  }
  emitCodexWorkflowDone(ctx, threadId);
  return true;
}

async function runCodexSandboxDiagnosticsCommand(server, threadId, cmd, ctx) {
  const command = readCodexSandboxDiagnosticsCommand(cmd);
  if (!command) return false;
  await flushCodexRuntimeSettings(ctx);
  try {
    const readiness = await server.request("windowsSandbox/readiness", null);
    ctx.protocol.emitTimeline({
      kind: "diagnostic",
      status: "info",
      title: "Codex sandbox diagnostics",
      summary: "已读取 Codex Windows sandbox readiness",
      payload: {
        backend: "codex",
        subkind: SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
        method: "windowsSandbox/readiness",
        threadId,
        includeDetails: command.includeDetails,
        readiness,
      },
      sourceId: `codex:sandbox-diagnostics:${threadId}`,
    });
  } catch (err) {
    ctx.protocol.emitTimeline({
      kind: "diagnostic",
      status: "error",
      title: "Codex sandbox diagnostics failed",
      summary: err?.message || String(err),
      payload: {
        backend: "codex",
        subkind: SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
        method: "windowsSandbox/readiness",
        threadId,
        includeDetails: command.includeDetails,
        error: err?.message || String(err),
      },
      sourceId: `codex:sandbox-diagnostics:error:${threadId}`,
    });
    throw err;
  }
  emitCodexWorkflowDone(ctx, threadId);
  return true;
}

async function runCodexRequestWorkflow(server, threadId, cmd, ctx, config) {
  const workflow = config.read(cmd);
  if (!workflow) return false;
  const timeline = typeof config.timeline === "function"
    ? config.timeline(workflow)
    : config.timeline;
  await flushCodexRuntimeSettings(ctx);
  emitCodexWorkflowTimeline(ctx, timeline, threadId, "started");
  try {
    await server.request(timeline.method, config.params(workflow));
  } catch (err) {
    emitCodexWorkflowTimeline(ctx, timeline, threadId, "error", err);
    throw err;
  }
  emitCodexWorkflowTimeline(ctx, timeline, threadId, "success");
  emitCodexWorkflowDone(ctx, threadId);
  return true;
}

function normalizeCodexMcpElicitationPayload(params) {
  if (!isRecord(params)) return null;
  const mode = stringOrNull(params.mode);
  const threadId = stringOrNull(params.threadId);
  const serverName = stringOrNull(params.serverName);
  const message = stringOrNull(params.message) || "";
  if (!threadId || !serverName || (mode !== "form" && mode !== "url")) return null;
  const payload = {
    threadId,
    turnId: stringOrNull(params.turnId),
    serverName,
    mode,
    message,
    _meta: params._meta ?? null,
  };
  if (mode === "form") payload.requestedSchema = params.requestedSchema ?? null;
  if (mode === "url") {
    payload.url = stringOrNull(params.url) || "";
    payload.elicitationId = stringOrNull(params.elicitationId) || "";
  }
  return payload;
}

function codexMcpElicitationResponse(result) {
  const action = result?.action === "accept" || result?.action === "decline"
    ? result.action
    : "cancel";
  const content = action === "accept" && isRecord(result?.content)
    ? result.content
    : null;
  return {
    action,
    content,
    _meta: isRecord(result?._meta) ? result._meta : null,
  };
}

function normalizeCodexPermissionApprovalPayload(params) {
  if (!isRecord(params)) return null;
  const threadId = stringOrNull(params.threadId);
  const turnId = stringOrNull(params.turnId);
  const itemId = stringOrNull(params.itemId);
  const cwd = stringOrNull(params.cwd);
  if (!threadId || !turnId || !itemId || !cwd) return null;
  return {
    reason: stringOrNull(params.reason),
    requestedAccess: isRecord(params.permissions) ? params.permissions : {},
    scopeSuggestion: "turn",
    providerContext: {
      codex: {
        threadId,
        turnId,
        itemId,
        startedAtMs: typeof params.startedAtMs === "number" && Number.isFinite(params.startedAtMs)
      ? params.startedAtMs
      : 0,
        cwd,
        permissions: isRecord(params.permissions) ? params.permissions : {},
      },
    },
  };
}

function codexGrantedPermissions(value) {
  const input = isRecord(value) ? value : {};
  const permissions = {};
  if (isRecord(input.network)) permissions.network = input.network;
  if (isRecord(input.fileSystem)) permissions.fileSystem = input.fileSystem;
  return permissions;
}

function codexPermissionApprovalResponse(result) {
  const permissions = codexGrantedPermissions(result?.grantedAccess);
  const action =
    result?.action === "approve" || result?.action === "decline" || result?.action === "cancel"
      ? result.action
      : result?.strictAutoReview === true
        ? "cancel"
        : Object.keys(permissions).length > 0
          ? "approve"
          : "decline";
  return {
    permissions: action === "approve" ? permissions : {},
    scope: result?.scope === "session" ? "session" : "turn",
    ...(action !== "approve"
      ? { strictAutoReview: true }
      : typeof result?.strictAutoReview === "boolean"
        ? { strictAutoReview: result.strictAutoReview }
        : {}),
  };
}

async function requestCodexRuntimeInteraction(ctx, kind, payload) {
  if (!ctx?.interactions?.requestCodexInteraction) {
    return kind === MCP_ELICITATION_INTERACTION_KIND
      ? { action: "cancel", content: null, _meta: null }
      : { action: "cancel", permissions: {}, scope: "turn", strictAutoReview: true };
  }
  return runCodexUiInteraction(ctx, kind, () =>
    ctx.interactions.requestCodexInteraction(kind, payload));
}

function normalizeLiliaIabSnapshot(value) {
  if (!isRecord(value)) return null;
  const taskId = stringOrNull(value.taskId);
  const url = stringOrNull(value.url) || "about:blank";
  const capturedAt = typeof value.capturedAt === "number" && Number.isFinite(value.capturedAt)
    ? value.capturedAt
    : Date.now();
  return {
    taskId,
    url,
    title: stringOrNull(value.title),
    note: stringOrNull(value.note),
    capturedAt,
    screenshotPath: stringOrNull(value.screenshotPath),
    status: value.status === "captured" ? "captured" : "metadata_only",
    warning: stringOrNull(value.warning),
  };
}

function liliaIabAdditionalContext(snapshot) {
  const lines = [
    "Lilia IAB interaction result:",
    `- URL: ${snapshot.url}`,
    snapshot.title ? `- Title: ${snapshot.title}` : null,
    snapshot.note ? `- User note: ${snapshot.note}` : null,
    `- Captured at: ${new Date(snapshot.capturedAt).toISOString()}`,
    `- Screenshot status: ${snapshot.status}`,
    snapshot.screenshotPath ? `- Screenshot path: ${snapshot.screenshotPath}` : null,
    snapshot.warning ? `- Warning: ${snapshot.warning}` : null,
  ].filter(Boolean);
  return lines.join("\n");
}

export async function handleLiliaIabResult(ctx, snapshotValue) {
  const snapshot = normalizeLiliaIabSnapshot(snapshotValue);
  if (!snapshot) return;
  ctx.protocol.emitTimeline({
    kind: "tool",
    status: snapshot.status === "captured" ? "success" : "info",
    title: "Lilia IAB snapshot",
    summary: oneLineSummary(snapshot.note || snapshot.title || snapshot.url),
    payload: {
      backend: "codex",
      subkind: "lilia_iab",
      taskId: snapshot.taskId,
      url: snapshot.url,
      title: snapshot.title,
      note: snapshot.note,
      capturedAt: snapshot.capturedAt,
      screenshotPath: snapshot.screenshotPath,
      status: snapshot.status,
      warning: snapshot.warning,
    },
    sourceId: `codex:iab:${snapshot.capturedAt}`,
  });
  if (!ctx.threadId || !ctx.currentTurnId) return;
  await ctx.server.request("turn/steer", {
    threadId: ctx.threadId,
    turnId: ctx.currentTurnId,
    additionalContext: liliaIabAdditionalContext(snapshot),
  });
}

const CODEX_WORKFLOW_RUNNERS = [
  { run: runCodexGoalWorkflow, finalize: true },
  { run: runCodexCompactWorkflow },
  { run: runCodexBackgroundTerminalsCleanWorkflow },
  { run: runCodexMemoryModeWorkflow },
  { run: runCodexMemoryResetWorkflow },
  { run: runCodexThreadForkRuntimeCommand, needsCwd: true },
  { run: runCodexConfigDiagnosticsWorkflow, needsCwd: true },
  { run: runCodexRuntimeSettingsCommand },
  { run: runCodexRemoteEnvironmentCommand },
  { run: runCodexSandboxDiagnosticsCommand },
  { run: runCodexProcessSessionCommand, needsCwd: true },
  { run: runCodexBatchApplyWorkflow, needsCwd: true },
  { run: runCodexFixSuggestionWorkflow, needsCwd: true, finalize: true },
  { run: runCodexReviewWorkflow, finalize: true },
  { run: runCodexTaskWorkflow, needsCwd: true, finalize: true },
];

async function runCodexWorkflowIfPresent(server, threadId, cmd, ctx, cwdFn) {
  for (const runner of CODEX_WORKFLOW_RUNNERS) {
    const handled = runner.needsCwd
      ? await runner.run(server, threadId, cmd, ctx, cwdFn)
      : await runner.run(server, threadId, cmd, ctx);
    if (!handled) continue;
    if (runner.finalize) finalizeCodexRunContext(ctx);
    return true;
  }
  return false;
}

export async function maybeHandleCodexServerRequest(server, msg, ctx = null) {
  if (!isRecord(msg)) return false;
  const method = stringOrNull(msg.method);
  if (method === "item/tool/call") {
    const params = isRecord(msg.params) ? msg.params : {};
    const toolName = stringOrNull(params.tool);
    const input = isRecord(params.arguments) ? params.arguments : {};
    let output = null;
    if (isLiliaAskUserTool(toolName)) {
      const requestAskUser = (spec, options) =>
        runCodexUiInteraction(ctx, ASK_USER_INTERACTION_KIND, () =>
          ctx.interactions.requestAskUser(spec, options));
      output = await createCodexAskUserHandler(requestAskUser)(input);
    } else if (isLiliaConversationContextTool(toolName)) {
      output = await createConversationContextHandler(ctx.cmd?.conversationContext)(input);
    } else if (isLiliaArchitectureTool(toolName)) {
      output = await createArchitectureChangeHandler({
        cmd: ctx.cmd,
        ctx,
        backend: "codex",
      })(input);
    } else if (isLiliaQuotaTool(toolName)) {
      output = await createQuotaUsageHandler(ctx.interactions?.requestQuotaUsage)(input);
    } else {
      return false;
    }
    server.respond(msg.id, {
      success: output.cancelled !== true && output.ok !== false,
      contentItems: [{ type: "inputText", text: JSON.stringify(output) }],
    });
    return true;
  }
  if (method === "item/tool/requestUserInput") {
    const params = isRecord(msg.params) ? msg.params : {};
    const spec = codexRequestUserInputQuestionsToSpec(params);
    if (spec.questions.length === 0) {
      server.respond(msg.id, { answers: {} });
      return true;
    }
    const result = await runCodexUiInteraction(ctx, ASK_USER_INTERACTION_KIND, () =>
      ctx.interactions.requestAskUser(spec, { backend: "codex" }));
    server.respond(msg.id, askUserResultToCodexRequestUserInputResponse(result, spec));
    return true;
  }
  if (method === "mcpServer/elicitation/request") {
    const payload = normalizeCodexMcpElicitationPayload(msg.params);
    if (!payload) {
      server.respond(msg.id, { action: "cancel", content: null, _meta: null });
      return true;
    }
    const result = await requestCodexRuntimeInteraction(ctx, MCP_ELICITATION_INTERACTION_KIND, payload);
    server.respond(
      msg.id,
      codexMcpElicitationResponse(result),
    );
    return true;
  }
  if (method === "item/permissions/requestApproval") {
    const payload = normalizeCodexPermissionApprovalPayload(msg.params);
    if (!payload) {
      server.respond(msg.id, {
        permissions: {},
        scope: "turn",
        strictAutoReview: true,
      });
      return true;
    }
    const result = await requestCodexRuntimeInteraction(ctx, PERMISSION_APPROVAL_INTERACTION_KIND, payload);
    server.respond(
      msg.id,
      codexPermissionApprovalResponse(result),
    );
    return true;
  }
  return maybeHandleCodexApprovalRequest(server, msg, ctx);
}

export async function runCodexAppServer(cmd, runtimeExtensions, context) {
  context.protocol.emitTimeline({
    kind: "diagnostic",
    status: "info",
    title: "Codex runtime starting",
    summary: "正在启动 Codex runtime",
    payload: {
      backend: "codex",
      subkind: "runtime_start",
      resumeSessionId: stringOrNull(cmd.resumeSessionId),
      cwd: stringOrNull(cmd.cwd),
    },
    sourceId: "codex:runtime:start",
  });
  const server = context.createCodexAppServer
    ? context.createCodexAppServer()
    : createCodexAppServer({ env: context.env || process.env });
  let unregisterInterrupt = null;
  let unregisterProcessSessionCommand = null;
  emitCodexRuntimeExtensionsTimeline(context.protocol, runtimeExtensions);
  try {
    await initializeCodexAppServer(server);
    if (await runCodexSessionManagementRuntimeCommand(
      server,
      stringOrNull(cmd.resumeSessionId),
      cmd,
      { protocol: context.protocol },
    )) {
      return;
    }
    const session = await startCodexAppServerSession(server, cmd, context.cwd || process.cwd);
    let threadId = session.threadId;
    if (!threadId) throw new Error("Codex app-server did not return a thread id");
    await updateCodexThreadSettings(
      server,
      threadId,
      cmd,
      context.protocol,
    );
    await syncCodexThreadHistory(server, threadId, cmd, context.protocol);
    const selectedModel = normalizeCodexSettings(cmd).model || session.model || null;
    const ctx = createCodexRunContext(cmd, context.protocol, threadId);
    ctx.cmd = cmd;
    ctx.selectedModel = selectedModel;
    ctx.interactions = context.interactions;
    ctx.server = server;
    ctx.emitToolConsentTimeline = context.emitToolConsentTimeline;
    ctx.settingsUpdatePromises = [];
    ctx.processSessionControlPromises = [];
    ctx.withCodexElicitation = (kind, fn) =>
      withCodexElicitation(server, threadId, ctx, kind, fn);
    unregisterInterrupt = context.interactions?.handleInterruptTurn?.(
      () => interruptActiveCodexTurn(ctx),
    ) ?? null;
    context.interactions?.handleSettingsUpdate?.((update) => {
      applyCodexRuntimeSettings(
        server,
        threadId,
        cmd,
        ctx,
        update,
        context.protocol,
      );
    });
    context.interactions?.handleLiliaIabResult?.((snapshot) => {
      ctx.settingsUpdatePromises.push(
        handleLiliaIabResult(ctx, snapshot).catch((err) => {
          context.protocol.emitTimeline({
            kind: "error",
            status: "error",
            title: "Lilia IAB result failed",
            summary: err?.message || String(err),
            payload: {
              backend: "codex",
              subkind: "lilia_iab_result",
            },
            sourceId: `codex:iab:error:${Date.now()}`,
          });
        }),
      );
    });
    unregisterProcessSessionCommand = registerCodexProcessSessionCommand(ctx, server, threadId);
    const planPreset = cmd.planMode === true
      ? await requireCodexPlanModePreset(server, context.protocol)
      : null;
    threadId = await runCodexAutoSessionFork(
      server,
      threadId,
      cmd,
      ctx,
      context.cwd || process.cwd,
    );
    threadId = await runCodexPromptSessionFork(
      server,
      threadId,
      cmd,
      ctx,
      context.cwd || process.cwd,
    );
    if (await runCodexWorkflowIfPresent(
      server,
      threadId,
      cmd,
      ctx,
      context.cwd || process.cwd,
    )) {
      return;
    }
    const completed = await runCodexTurnLoop(
      server,
      threadId,
      cmd,
      ctx,
      context.cwd || process.cwd,
      {
        initialPrompt: cmd.prompt,
        initialTurnKind: cmd.planMode === true ? "plan" : "default",
        selectedModel,
        planPreset,
      },
    );
    if (!completed) {
      return;
    }
    finalizeCodexRunContext(ctx);
  } finally {
    unregisterInterrupt?.();
    unregisterProcessSessionCommand?.();
    server.close();
  }
}

export async function runCodex(cmd, context) {
  handleExperimentalProviderOptions(cmd, context, "codex");
  const runtimeExtensions = readCodexRuntimeExtensions(cmd);
  await runCodexAppServer(cmd, runtimeExtensions, context);
}

export function emitCodexRuntimeExtensionsTimeline(protocol, extensions) {
  const count = extensions.mcpServers.length;
  const serverNames = extensions.mcpServers
    .map((server) => stringOrNull(server?.name))
    .filter(Boolean);
  protocol.emitTimeline({
    kind: "diagnostic",
    status: "info",
    title: "Codex MCP config",
    summary: count > 0
      ? `已注册 ${count} 个 MCP server`
      : "未发现 Codex MCP server",
    payload: {
      backend: "codex",
      subkind: "config",
      source: "config.toml",
      configPath: extensions.configPath,
      serverCount: count,
      servers: serverNames,
      warnings: extensions.warnings,
    },
    sourceId: "codex:mcp:runtime-config",
  });
  emitRuntimeExtensionWarnings(protocol, "codex", extensions.warnings);
}

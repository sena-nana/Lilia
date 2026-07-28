import { createSdkMcpServer, forkSession, query, tool } from "@anthropic-ai/claude-agent-sdk";
import { createClaudeStreamState, dispatchClaudeStreamEvent } from "../claudeStream.mjs";
import {
  askUserQuestionInputSchema,
  createClaudeAskUserHandler,
} from "../askUser.mjs";
import {
  architectureContextEnabled,
  createArchitectureChangeHandler,
  updateProjectArchitectureInputSchema,
} from "../architecture.mjs";
import {
  buildConversationContextToolDescription,
  conversationContextEnabled,
  createConversationContextHandler,
  queryConversationContextInputSchema,
} from "../conversationContext.mjs";
import {
  createQuotaUsageHandler,
  queryQuotaUsageInputSchema,
} from "../quotaUsage.mjs";
import {
  emitRuntimeExtensionWarnings,
  readClaudeRuntimeExtensions,
} from "../runtimeExtensions.mjs";
import {
  emitAssistantTextFragmentTimeline,
} from "../protocol.mjs";
import {
  applyClaudeRuntimePermission,
  mapClaudeInitialPermission,
  createClaudeCanUseTool,
  createClaudeHooks,
} from "./permissions.mjs";
import {
  PROCESS_SESSION_COMMAND_TYPE,
  REMOTE_ENVIRONMENT_COMMAND_TYPE,
  SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
  SESSION_FORK_COMMAND_TYPE,
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
  isLiliaQueryWorkflowType,
  normalizeLiliaBatchApplyWorkflow,
  normalizeLiliaGoalWorkflow,
  normalizeLiliaConfigDiagnosticsWorkflow,
  normalizeLiliaFixSuggestionWorkflow,
  normalizeLiliaMemoryModeWorkflow,
  normalizeLiliaReviewWorkflow,
  normalizeLiliaTaskWorkflow,
} from "@lilia/contracts/liliaWorkflowContract.mjs";
import {
  ASK_USER_CLAUDE_TOOL_NAME,
  ASK_USER_MCP_TOOL_NAME,
  ASK_USER_TOOL_NAME,
} from "@lilia/contracts/askUserContract.mjs";
import {
  ARCHITECTURE_CLAUDE_TOOL_NAME,
  ARCHITECTURE_MCP_TOOL_NAME,
  ARCHITECTURE_TOOL_NAME,
} from "@lilia/contracts/architectureContract.mjs";
import {
  CONVERSATION_CONTEXT_CLAUDE_TOOL_NAME,
  CONVERSATION_CONTEXT_MCP_TOOL_NAME,
  CONVERSATION_CONTEXT_TOOL_NAME,
} from "@lilia/contracts/conversationContextContract.mjs";
import {
  QUOTA_USAGE_CLAUDE_TOOL_NAME,
  QUOTA_USAGE_MCP_TOOL_NAME,
  QUOTA_USAGE_TOOL_NAME,
} from "@lilia/contracts/quotaContract.mjs";
import {
  RUNNER_DONE_EVENT_TYPE,
  RUNNER_PROMPT_SUGGESTION_EVENT_TYPE,
  RUNNER_TOOL_USE_EVENT_TYPE,
} from "@lilia/contracts/runnerProtocolContract.mjs";
import { normalizeReasoningEffortForBackend } from "@lilia/contracts/provider.mjs";
import { normalizeRuntimePermission } from "../runtimeSettings.mjs";
import { runClaudeSessionManagementRuntimeCommand } from "../sessionManagement.mjs";
import {
  handleExperimentalProviderOptions,
  readProviderRuntimeOptions,
} from "../providerOptions.mjs";
import {
  buildClaudeSystemPrompt,
  buildClaudeWorkflowPrompt as buildManagedClaudeWorkflowPrompt,
} from "../promptManager.mjs";
import {
  readRunnerRuntimeCommand,
  readRunnerWorkflow,
} from "../runnerCommand.mjs";
import {
  closeClaudeReasoningBlock,
  closeClaudeTextFragment,
  emitClaudePlanTimeline,
  emitClaudeTextResultFallback,
  emitClaudeToolResultTimeline,
  emitClaudeToolTimeline,
  finalizeClaudeReasoningTimeline,
  finalizeClaudeTextFragments,
  getOrCreateClaudeReasoningBlock,
  getOrCreateClaudeTextFragment,
  mapClaudeSystemTimeline,
  sweepActiveClaudeTools,
} from "./timeline.mjs";
import { isRecord, stringOrNull } from "../utils.mjs";

export {
  buildClaudePlatformAppend,
  buildClaudeSystemPrompt,
} from "../promptManager.mjs";

export async function* singleClaudePromptStream(prompt) {
  yield {
    type: "user",
    message: {
      role: "user",
      content: [{ type: "text", text: prompt }],
    },
    parent_tool_use_id: null,
  };
}

export function claudeCompactPromptStream() {
  return singleClaudePromptStream("/compact");
}

export function createLiliaAskUserServer({
  createServer = createSdkMcpServer,
  createTool = tool,
  requestAskUser,
  requestQuotaUsage,
  conversationContext = null,
  architectureHandler = null,
}) {
  const tools = [
    createTool(
      ASK_USER_CLAUDE_TOOL_NAME,
      "Ask the human user one or more multiple-choice questions through Lilia.",
      askUserQuestionInputSchema,
      createClaudeAskUserHandler(requestAskUser),
      { alwaysLoad: true },
    ),
    createTool(
      QUOTA_USAGE_CLAUDE_TOOL_NAME,
      "Query Lilia quota usage summaries through the Lilia internal quota plugin.",
      queryQuotaUsageInputSchema,
      createQuotaUsageHandler(requestQuotaUsage),
      { alwaysLoad: true },
    ),
  ];
  if (conversationContextEnabled(conversationContext)) {
    tools.push(createTool(
      CONVERSATION_CONTEXT_CLAUDE_TOOL_NAME,
      buildConversationContextToolDescription(),
      queryConversationContextInputSchema,
      createConversationContextHandler(conversationContext),
      { alwaysLoad: true },
    ));
  }
  if (architectureContextEnabled(conversationContext) && architectureHandler) {
    tools.push(createTool(
      ARCHITECTURE_CLAUDE_TOOL_NAME,
      "Update Lilia's project-level architecture graph with structured changes.",
      updateProjectArchitectureInputSchema,
      architectureHandler,
      { alwaysLoad: true },
    ));
  }
  return createServer({
    name: "lilia",
    version: "0.1.0",
    tools,
    alwaysLoad: true,
  });
}

function readLiliaWorkflow(cmd) {
  return readRunnerWorkflow(cmd);
}

function setClaudeResumeSessionAt(cmd, sourceTurnId) {
  if (!sourceTurnId) return;
  if (!isRecord(cmd.runtimeOptions)) cmd.runtimeOptions = {};
  if (!isRecord(cmd.runtimeOptions.provider)) cmd.runtimeOptions.provider = {};
  if (!isRecord(cmd.runtimeOptions.provider.claude)) cmd.runtimeOptions.provider.claude = {};
  cmd.runtimeOptions.provider.claude.resumeSessionAt = sourceTurnId;
}

async function runClaudeSessionForkRuntimeCommand(cmd, context, cwd) {
  const command = normalizeSessionForkCommand(readRunnerRuntimeCommand(cmd));
  if (!command) return false;
  const sourceTurnId = command.sourceTurnId || null;
  const mode = command.mode;
  const hasAnchoredPrompt = Boolean(sourceTurnId && stringOrNull(cmd?.prompt)?.trim());
  if (mode === "continue" && hasAnchoredPrompt) {
    setClaudeResumeSessionAt(cmd, sourceTurnId);
    return false;
  }
  const sourceSessionId = typeof cmd.resumeSessionId === "string" ? cmd.resumeSessionId.trim() : "";
  context.protocol.emitTimeline({
    kind: "diagnostic",
    status: "started",
    title: "Claude session fork started",
    summary: "正在分叉 Claude session",
    payload: {
      backend: "claude",
      subkind: SESSION_FORK_COMMAND_TYPE,
      sourceSessionId: sourceSessionId || null,
      sourceTurnId,
      mode,
    },
    sourceId: `claude:session-fork:start:${sourceSessionId || "missing"}`,
  });
  try {
    if (!sourceSessionId) throw new Error("当前 Claude task 没有可 fork 的 session");
    const forkClaudeSession = context.forkClaudeSession || forkSession;
    const result = await forkClaudeSession(sourceSessionId, {
      dir: cwd,
      ...(sourceTurnId ? { resumeSessionAt: sourceTurnId } : {}),
    });
    const sessionId = typeof result?.sessionId === "string" ? result.sessionId.trim() : "";
    if (!sessionId) throw new Error("Claude forkSession did not return a session id");
    setClaudeResumeSessionAt(cmd, sourceTurnId);
    cmd.resumeSessionId = sessionId;
    context.protocol.emitTimeline({
      kind: "diagnostic",
      status: "success",
      title: "Claude session fork completed",
      summary: "已分叉 Claude session",
      payload: {
        backend: "claude",
        subkind: SESSION_FORK_COMMAND_TYPE,
        sourceSessionId,
        sourceTurnId,
        sessionId,
        mode,
      },
      sourceId: `claude:session-fork:completed:${sourceSessionId}:${sessionId}`,
    });
    if (hasAnchoredPrompt) return false;
    context.protocol.emit({ type: RUNNER_DONE_EVENT_TYPE, sessionId, subtype: "success" });
  } catch (err) {
    context.protocol.emitTimeline({
      kind: "diagnostic",
      status: "error",
      title: "Claude session fork failed",
      summary: err?.message || String(err),
      payload: {
        backend: "claude",
        subkind: SESSION_FORK_COMMAND_TYPE,
        sourceSessionId: sourceSessionId || null,
        sourceTurnId,
        mode,
        error: err?.message || String(err),
      },
      sourceId: `claude:session-fork:error:${sourceSessionId || "missing"}`,
    });
    throw err;
  }
  return true;
}

async function runClaudeAutoSessionFork(cmd, context, cwd) {
  if (
    cmd?.autoSessionFork !== true ||
    normalizeSessionForkCommand(readRunnerRuntimeCommand(cmd))
  ) return;
  const sourceSessionId = typeof cmd.resumeSessionId === "string" ? cmd.resumeSessionId.trim() : "";
  context.protocol.emitTimeline({
    kind: "diagnostic",
    status: "started",
    title: "Claude session fork started",
    summary: "正在分叉 Claude session",
    payload: {
      backend: "claude",
      subkind: SESSION_FORK_COMMAND_TYPE,
      sourceSessionId: sourceSessionId || null,
      autoTurnDecision: true,
    },
    sourceId: `claude:session-fork:auto:start:${sourceSessionId || "missing"}`,
  });
  try {
    if (!sourceSessionId) throw new Error("辅助模型建议会话分叉，但当前 Claude task 没有可 fork 的 session");
    const forkClaudeSession = context.forkClaudeSession || forkSession;
    const result = await forkClaudeSession(sourceSessionId, { dir: cwd });
    const sessionId = typeof result?.sessionId === "string" ? result.sessionId.trim() : "";
    if (!sessionId) throw new Error("Claude forkSession did not return a session id");
    cmd.resumeSessionId = sessionId;
    context.protocol.emitTimeline({
      kind: "diagnostic",
      status: "success",
      title: "Claude session fork completed",
      summary: "已分叉 Claude session，本轮将在分叉 session 中继续",
      payload: {
        backend: "claude",
        subkind: SESSION_FORK_COMMAND_TYPE,
        sourceSessionId,
        sessionId,
        autoTurnDecision: true,
      },
      sourceId: `claude:session-fork:auto:completed:${sourceSessionId}:${sessionId}`,
    });
  } catch (err) {
    context.protocol.emitTimeline({
      kind: "diagnostic",
      status: "error",
      title: "Claude session fork failed",
      summary: err?.message || String(err),
      payload: {
        backend: "claude",
        subkind: SESSION_FORK_COMMAND_TYPE,
        sourceSessionId: sourceSessionId || null,
        autoTurnDecision: true,
        error: err?.message || String(err),
      },
      sourceId: `claude:session-fork:auto:error:${sourceSessionId || "missing"}`,
    });
    throw err;
  }
}

function liliaReviewTargetText(target) {
  if (target.type === "uncommittedChanges") return "当前工作区未提交改动";
  if (target.type === "baseBranch") return `当前工作区相对分支 ${target.branch} 的差异`;
  return `提交 ${target.sha}`;
}

function readLiliaReviewWorkflow(cmd) {
  return normalizeLiliaReviewWorkflow(readLiliaWorkflow(cmd));
}

function readLiliaFixSuggestionWorkflow(cmd) {
  return normalizeLiliaFixSuggestionWorkflow(readLiliaWorkflow(cmd));
}

function readLiliaBatchApplyWorkflow(cmd) {
  return normalizeLiliaBatchApplyWorkflow(readLiliaWorkflow(cmd));
}

function readLiliaTaskWorkflow(cmd) {
  return normalizeLiliaTaskWorkflow(readLiliaWorkflow(cmd));
}

function buildClaudeWorkflowPrompt(cmd, providerSettings = null) {
  const review = readLiliaReviewWorkflow(cmd);
  const taskWorkflow = readLiliaTaskWorkflow(cmd);
  const fix = readLiliaFixSuggestionWorkflow(cmd);
  const batch = readLiliaBatchApplyWorkflow(cmd);
  return buildManagedClaudeWorkflowPrompt({
    review,
    taskWorkflow,
    fix,
    batch,
    providerSettings,
    prompt: cmd.prompt,
    reviewTargetText: liliaReviewTargetText,
  });
}

const CLAUDE_PROVIDER_SETTING_KEYS = new Set([
  "allowedTools",
  "disallowedTools",
  "additionalDirectories",
  "additionalContext",
  "reasoningEffort",
  "thinking",
  "settingSources",
  "permissionPromptToolName",
  "resumeSessionAt",
  "sessionId",
  "managedSettings",
  "sandbox",
  "outputFormat",
  "sessionStore",
  "maxTurns",
  "maxBudgetUsd",
  "abortAfterMs",
  "includeHookEvents",
  "forwardSubagentText",
  "agentProgressSummaries",
  "continue",
  "tools",
  "settings",
]);
function emitClaudeWorkflowDone(context, sessionId = null) {
  context.protocol.emit({ type: RUNNER_DONE_EVENT_TYPE, sessionId, subtype: "success" });
}

function claudeUsageTokenCount(usage) {
  if (!isRecord(usage)) return null;
  const keys = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"];
  const total = keys.reduce((sum, key) => {
    const value = Number(usage[key]);
    return Number.isFinite(value) && value > 0 ? sum + value : sum;
  }, 0);
  return total > 0 ? total : null;
}

function emitClaudeContextUsage(context, usage) {
  const usedTokens = claudeUsageTokenCount(usage);
  if (usedTokens === null) return;
  context.protocol.emitContextUsage?.({
    usedTokens,
    source: "claude",
  });
}

function emitClaudeLocalWorkflowTimeline(context, workflow, config) {
  context.protocol.emitTimeline({
    kind: "diagnostic",
    status: config.status || "success",
    title: config.title,
    summary: config.summary,
    payload: {
      backend: "claude",
      source: "lilia",
      subkind: config.subkind,
      workflowType: workflow?.type ?? null,
      ...config.payload,
    },
    sourceId: `claude:lilia-${config.subkind || "workflow"}:${Date.now()}`,
  });
  emitClaudeWorkflowDone(context);
}

function claudeConfigDiagnosticsPayload(cmd) {
  const runtimeExtensions = readClaudeRuntimeExtensions(cmd);
  return {
    cwd: stringOrNull(cmd.cwd),
    model: stringOrNull(cmd.model),
    permission: normalizeRuntimePermission(cmd.permission) || "ask",
    planMode: cmd.planMode === true,
    hasResumeSession: Boolean(stringOrNull(cmd.resumeSessionId)),
    runtimeExtensions: {
      mcpServerCount: Object.keys(runtimeExtensions.mcpServers || {}).length,
      mcpServers: Object.keys(runtimeExtensions.mcpServers || {}),
      skillCount: runtimeExtensions.skills.length,
      skills: runtimeExtensions.skills,
      pluginCount: runtimeExtensions.plugins.length,
      plugins: runtimeExtensions.plugins,
      warningCount: runtimeExtensions.warnings.length,
      warnings: runtimeExtensions.warnings,
    },
  };
}

function readStringList(value) {
  if (!Array.isArray(value)) return [];
  const seen = new Set();
  const out = [];
  for (const item of value) {
    const text = stringOrNull(item)?.trim();
    if (!text || seen.has(text)) continue;
    seen.add(text);
    out.push(text);
  }
  return out;
}

function readStringOption(value) {
  return stringOrNull(value)?.trim() || null;
}

function readPlainObject(value) {
  return isRecord(value) && Object.keys(value).length > 0 ? { ...value } : null;
}

function readClaudeReasoningEffort(value) {
  return normalizeReasoningEffortForBackend("claude", readStringOption(value));
}

function readClaudeThinkingConfig(value) {
  if (!isRecord(value)) return null;
  if (value.type === "adaptive") return { type: "adaptive" };
  if (value.type === "disabled") return { type: "disabled" };
  if (value.type === "enabled") {
    const budgetTokens = Number(value.budgetTokens);
    if (Number.isFinite(budgetTokens) && budgetTokens > 0) {
      return { type: "enabled", budgetTokens: Math.trunc(budgetTokens) };
    }
  }
  return null;
}

function readClaudeManagedAgents(value) {
  if (!isRecord(value) || !isRecord(value.agents)) return null;
  return Object.keys(value.agents).length > 0 ? { ...value.agents } : null;
}

function readClaudeToolsOption(value) {
  const list = readStringList(value);
  if (list.length > 0) return list;
  if (isRecord(value) && value.type === "preset") {
    const preset = readStringOption(value.preset);
    if (preset) return { type: "preset", preset };
  }
  return null;
}

function readClaudeRuntimeSettingsCommand(command, runtimeOptions = {}) {
  const normalizedCommand = normalizeRuntimeSettingsCommand(command);
  if (!normalizedCommand) return null;
  const {
    common,
    settings: claude,
    ignoredProviderKeys,
  } = readProviderRuntimeOptions(runtimeOptions, "claude");
  const unsupportedKeys = Object.keys(claude)
    .filter((key) => !CLAUDE_PROVIDER_SETTING_KEYS.has(key));
  const supported = {};
  const options = {};
  const model = stringOrNull(common.model)?.trim();
  const permission = normalizeRuntimePermission(common.permission);
  const reasoningEffort = readClaudeReasoningEffort(claude.reasoningEffort) ||
    readClaudeReasoningEffort(common.reasoningEffort);
  const thinking = readClaudeThinkingConfig(claude.thinking);
  if (model) supported.model = model;
  if (permission) supported.permission = permission;
  if (reasoningEffort) options.effort = reasoningEffort;
  if (thinking) {
    options.thinking = thinking;
  } else if (reasoningEffort) {
    options.thinking = { type: "adaptive" };
  }

  for (const key of ["allowedTools", "disallowedTools", "additionalDirectories"]) {
    const values = readStringList(claude[key]);
    if (values.length > 0) options[key] = values;
  }
  const tools = readClaudeToolsOption(claude.tools);
  if (tools) options.tools = tools;
  const permissionPromptToolName = readStringOption(claude.permissionPromptToolName);
  if (permissionPromptToolName) options.permissionPromptToolName = permissionPromptToolName;
  const settingsPath = readStringOption(claude.settings);
  const settingsObject = readPlainObject(claude.settings);
  if (settingsPath || settingsObject) options.settings = settingsPath || settingsObject;
  const managedSettings = readPlainObject(claude.managedSettings);
  if (managedSettings) options.managedSettings = managedSettings;
  const settingSources = readStringList(claude.settingSources);
  if (settingSources.length > 0) options.settingSources = settingSources;
  for (const key of ["sandbox", "outputFormat", "sessionStore"]) {
    const object = readPlainObject(claude[key]);
    if (object) options[key] = object;
  }
  if (typeof claude.maxTurns === "number" && Number.isFinite(claude.maxTurns)) {
    options.maxTurns = claude.maxTurns;
  }
  if (typeof claude.maxBudgetUsd === "number" && Number.isFinite(claude.maxBudgetUsd)) {
    options.maxBudgetUsd = claude.maxBudgetUsd;
  }
  for (const key of ["includeHookEvents", "forwardSubagentText", "agentProgressSummaries", "continue"]) {
    if (typeof claude[key] === "boolean") options[key] = claude[key];
  }
  for (const key of ["resumeSessionAt", "sessionId"]) {
    const value = readStringOption(claude[key]);
    if (value) options[key] = value;
  }
  if (typeof claude.abortAfterMs === "number" && Number.isFinite(claude.abortAfterMs) && claude.abortAfterMs > 0) {
    options.abortAfterMs = Math.trunc(claude.abortAfterMs);
  }
  if (normalizedCommand.action === "update" && Object.keys(supported).length === 0 && Object.keys(options).length === 0) {
    throw new Error("Lilia provider settings update requires at least one valid setting");
  }
  return {
    action: normalizedCommand.action,
    supported,
    options,
    settingsKeys: [...Object.keys(supported), ...Object.keys(options)],
    ignoredProviderKeys,
    unsupportedKeys,
  };
}

async function runClaudeLocalLiliaWorkflow(cmd, context) {
  const workflow = readLiliaWorkflow(cmd);
  if (!workflow) return false;
  if (isLiliaQueryWorkflowType(workflow.type)) return false;
  const goalWorkflow = normalizeLiliaGoalWorkflow(workflow);
  if (goalWorkflow) {
    const { action, objective, status, tokenBudget } = goalWorkflow;
    const cleared = action === "clear";
    const goal = cleared
      ? null
      : {
        threadId: stringOrNull(cmd.resumeSessionId) || "claude-local",
        objective: objective || "Lilia Goal",
        status,
        tokenBudget,
        tokensUsed: 0,
        timeUsedSeconds: 0,
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };
    context.protocol.emitTimeline({
      kind: "goal",
      status: cleared ? "cancelled" : "info",
      title: cleared ? "Lilia Goal cleared" : "Lilia Goal updated",
      summary: cleared ? "已清除 Lilia Goal" : goal.objective,
      payload: {
        backend: "claude",
        subkind: "lilia_goal",
        action,
        cleared,
        goal,
        goalStatus: goal?.status ?? null,
      },
      sourceId: `claude:lilia-goal:${action}:${Date.now()}`,
    });
    emitClaudeWorkflowDone(context, stringOrNull(cmd.resumeSessionId));
    return true;
  }
  const memoryModeWorkflow = normalizeLiliaMemoryModeWorkflow(workflow);
  if (memoryModeWorkflow) {
    const { mode } = memoryModeWorkflow;
    emitClaudeLocalWorkflowTimeline(context, workflow, {
      subkind: "memory_mode",
      title: "Claude memory mode recorded",
      summary: `Lilia memory mode set to ${mode} for Claude UI state`,
      payload: {
        mode,
        native: false,
        reason: "Claude SDK has no equivalent Lilia-controllable memory mode.",
      },
    });
    return true;
  }
  if (isLiliaMemoryResetWorkflow(workflow)) {
    emitClaudeLocalWorkflowTimeline(context, workflow, {
      subkind: "memory_reset",
      title: "Claude memory reset recorded",
      summary: "Lilia memory reset completed for Claude UI state",
      payload: {
        native: false,
        reason: "Claude SDK has no equivalent global memory reset endpoint.",
      },
    });
    return true;
  }
  if (isLiliaBackgroundTerminalsCleanWorkflow(workflow)) {
    emitClaudeLocalWorkflowTimeline(context, workflow, {
      subkind: "background_terminals_clean",
      title: "Claude background terminals clean completed",
      summary: "No Lilia-managed Claude background terminals required cleanup",
      payload: {
        cleanedCount: 0,
        native: false,
      },
    });
    return true;
  }
  const configDiagnosticsWorkflow = normalizeLiliaConfigDiagnosticsWorkflow(workflow);
  if (configDiagnosticsWorkflow) {
    emitClaudeLocalWorkflowTimeline(context, workflow, {
      subkind: "config_diagnostics",
      title: "Claude config diagnostics",
      summary: "Lilia+Claude runtime diagnostics collected",
      payload: {
        includeLayers: configDiagnosticsWorkflow.includeLayers,
        ...claudeConfigDiagnosticsPayload(cmd),
      },
    });
    return true;
  }
  return false;
}

function readClaudeCompactWorkflow(cmd) {
  return isLiliaCompactWorkflow(readLiliaWorkflow(cmd));
}

function applyClaudeRuntimeSettingsCommand(cmd) {
  const command = readRunnerRuntimeCommand(cmd);
  const settings = readClaudeRuntimeSettingsCommand(
    command,
    isRecord(cmd?.runtimeOptions) ? cmd.runtimeOptions : {},
  );
  if (!settings) return null;
  if (settings.action === "update") {
    if (settings.supported.model) cmd.model = settings.supported.model;
    if (settings.supported.permission) cmd.permission = settings.supported.permission;
  }
  return settings;
}

function emitClaudeProviderSettingsTimeline(context, settings, status = "info") {
  const failed = status === "error";
  context.protocol.emitTimeline({
    kind: "diagnostic",
    status,
    title: failed
      ? "Claude provider settings failed"
      : settings.action === "diagnose"
        ? "Claude provider settings diagnostics"
        : "Claude provider settings applied",
    summary: settings.settingsKeys.length
      ? `当前可识别 ${settings.settingsKeys.length} 项 Claude provider settings`
      : "当前没有可应用的 Claude provider settings 更新",
    payload: {
      backend: "claude",
      subkind: "provider_settings",
      action: settings.action,
      settingsKeys: settings.settingsKeys,
      commonKeys: Object.keys(settings.supported),
      optionKeys: Object.keys(settings.options),
      ...(settings.ignoredProviderKeys.length ? { ignoredProviderKeys: settings.ignoredProviderKeys } : {}),
      ...(settings.unsupportedKeys.length ? { unsupportedKeys: settings.unsupportedKeys } : {}),
      native: true,
    },
    sourceId: `claude:provider-settings:${settings.action}:${Date.now()}`,
  });
}

function runClaudeProviderSettingsRuntimeCommand(cmd, context) {
  const command = readRunnerRuntimeCommand(cmd);
  const settings = readClaudeRuntimeSettingsCommand(
    command,
    isRecord(cmd?.runtimeOptions) ? cmd.runtimeOptions : {},
  );
  if (!settings || settings.action !== "diagnose") return false;
  emitClaudeProviderSettingsTimeline(context, settings, "info");
  emitClaudeWorkflowDone(context, stringOrNull(cmd.resumeSessionId));
  return true;
}

function runClaudeSandboxDiagnosticsRuntimeCommand(cmd, context) {
  const command = normalizeSandboxDiagnosticsCommand(
    readRunnerRuntimeCommand(cmd),
  );
  if (!command) return false;
  context.protocol.emitTimeline({
    kind: "diagnostic",
    status: "info",
    title: "Claude sandbox diagnostics unsupported",
    summary: "Claude SDK has no equivalent sandbox readiness endpoint.",
    payload: {
      backend: "claude",
      subkind: SANDBOX_DIAGNOSTICS_COMMAND_TYPE,
      includeDetails: command.includeDetails === true,
      native: false,
      unsupported: true,
      reason: "Claude SDK has no equivalent Lilia-controllable sandbox readiness endpoint.",
    },
    sourceId: `claude:sandbox-diagnostics:${Date.now()}`,
  });
  emitClaudeWorkflowDone(context, stringOrNull(cmd.resumeSessionId));
  return true;
}

function runClaudeProcessSessionRuntimeCommand(cmd, context) {
  const command = normalizeProcessSessionCommand(readRunnerRuntimeCommand(cmd));
  if (!command) return false;
  context.protocol.emitTimeline({
    kind: "diagnostic",
    status: "info",
    title: "Claude process session unsupported",
    summary: "Claude SDK has no equivalent independent process session endpoint.",
    payload: {
      backend: "claude",
      subkind: PROCESS_SESSION_COMMAND_TYPE,
      action: command.action,
      processId: command.processId || null,
      native: false,
      unsupported: true,
      reason: "Claude SDK has no equivalent Lilia-controllable process session endpoint.",
    },
    sourceId: `claude:process-session:${command.action}:${Date.now()}`,
  });
  emitClaudeWorkflowDone(context, stringOrNull(cmd.resumeSessionId));
  return true;
}

function runClaudeRemoteEnvironmentRuntimeCommand(cmd, context) {
  const command = normalizeRemoteEnvironmentCommand(
    readRunnerRuntimeCommand(cmd),
  );
  if (!command) return false;
  const environmentId = command.environmentId || null;
  context.protocol.emitTimeline({
    kind: "diagnostic",
    status: "info",
    title: "Claude remote environment unsupported",
    summary: "Claude SDK has no equivalent remote environment endpoint.",
    payload: {
      backend: "claude",
      subkind: REMOTE_ENVIRONMENT_COMMAND_TYPE,
      action: command.action,
      environmentId,
      native: false,
      unsupported: true,
      reason: "Claude SDK has no equivalent Lilia-controllable remote environment endpoint.",
    },
    sourceId: `claude:remote-environment:${command.action}:${Date.now()}`,
  });
  emitClaudeWorkflowDone(context, stringOrNull(cmd.resumeSessionId));
  return true;
}

function normalizeClaudeMcpElicitationPayload(request, ctx) {
  if (!isRecord(request)) return null;
  const serverName = stringOrNull(request.serverName) || stringOrNull(request.mcp_server_name);
  const mode = stringOrNull(request.mode) === "url" ? "url" : "form";
  if (!serverName) return null;
  const payload = {
    threadId: stringOrNull(ctx?.sessionId) || stringOrNull(request.sessionId) || stringOrNull(request.session_id) || "claude",
    turnId: stringOrNull(request.turnId) || stringOrNull(request.turn_id),
    serverName,
    mode,
    message: stringOrNull(request.message) || "",
    _meta: request._meta ?? null,
  };
  if (mode === "form") payload.requestedSchema = request.requestedSchema ?? request.requested_schema ?? null;
  if (mode === "url") {
    payload.url = stringOrNull(request.url) || "";
    payload.elicitationId = stringOrNull(request.elicitationId) || stringOrNull(request.elicitation_id) || "";
  }
  return payload;
}

function claudeMcpElicitationResponse(result) {
  const action = result?.action === "accept" || result?.action === "decline"
    ? result.action
    : "cancel";
  const content = action === "accept" && isRecord(result?.content)
    ? result.content
    : undefined;
  return {
    action,
    ...(content ? { content } : {}),
  };
}

async function requestClaudeMcpElicitation(request, ctx) {
  const payload = normalizeClaudeMcpElicitationPayload(request, ctx);
  if (!payload || !ctx?.interactions?.requestMcpElicitation) {
    return { action: "cancel" };
  }
  const result = await ctx.interactions.requestMcpElicitation(payload, { backend: "claude" });
  return claudeMcpElicitationResponse(result);
}

function emitClaudeCompactTimeline(context, status, sourceSessionId, err = null) {
  const failed = status === "error";
  const completed = status === "success";
  context.protocol.emitTimeline({
    kind: "diagnostic",
    status,
    title: failed
      ? "Claude compact failed"
      : completed
        ? "Claude compact completed"
        : "Claude compact started",
    summary: failed
      ? err?.message || String(err)
      : completed
        ? "Claude 原生上下文压缩已完成"
        : "正在通过 Claude 原生 /compact 压缩上下文",
    payload: {
      backend: "claude",
      subkind: "compact",
      sourceSessionId: sourceSessionId || null,
      method: "/compact",
      ...(failed ? { error: err?.message || String(err) } : {}),
    },
    sourceId: `claude:compact:${failed ? "error" : completed ? "completed" : "start"}:${sourceSessionId || "missing"}`,
  });
}

function readClaudeAdditionalContext(runtimeOptions = {}) {
  const claude = readProviderRuntimeOptions(runtimeOptions, "claude").settings;
  return readStringOption(claude.additionalContext);
}

function mergeClaudeHookMap(...maps) {
  const merged = {};
  for (const map of maps) {
    if (!map || typeof map !== "object") continue;
    for (const [eventName, entries] of Object.entries(map)) {
      if (!Array.isArray(entries) || entries.length === 0) continue;
      if (!Array.isArray(merged[eventName])) merged[eventName] = [];
      merged[eventName].push(...entries);
    }
  }
  return Object.keys(merged).length > 0 ? merged : undefined;
}

async function runClaudeCompactWorkflow(cmd, context, workingDir) {
  if (!readClaudeCompactWorkflow(cmd)) return false;
  const sourceSessionId = typeof cmd.resumeSessionId === "string" ? cmd.resumeSessionId.trim() : "";
  emitClaudeCompactTimeline(context, "started", sourceSessionId);
  try {
    if (!sourceSessionId) throw new Error("当前 Claude task 没有可 compact 的 session");
    const result = await runClaudeQueryTurn(
      {
        ...cmd,
        prompt: "",
        resumeSessionId: sourceSessionId,
        planMode: false,
      },
      context,
      workingDir,
      {
        promptStream: claudeCompactPromptStream(),
        emitRuntimeWarnings: false,
        skipPromptSuggestions: true,
        suppressDone: true,
      },
    );
    if (result?.compactResult === "failed") {
      throw new Error(result.compactError || "Claude compact failed");
    }
    emitClaudeCompactTimeline(context, "success", sourceSessionId);
    emitClaudeWorkflowDone(context, result?.sessionId || sourceSessionId);
  } catch (err) {
    emitClaudeCompactTimeline(context, "error", sourceSessionId, err);
    throw err;
  }
  return true;
}

async function runClaudeQueryTurn(cmd, context, workingDir, overrides = {}) {
  const providerSettings = applyClaudeRuntimeSettingsCommand(cmd);
  if (providerSettings?.action === "update") {
    emitClaudeProviderSettingsTimeline(context, providerSettings, "success");
  }
  const runtimeProviderSettings = readClaudeRuntimeSettingsCommand(
    { type: "runtime_settings", action: "diagnose" },
    isRecord(cmd?.runtimeOptions) ? cmd.runtimeOptions : {},
  );
  const { prompt, model, resumeSessionId } = cmd;
  const workflowPrompt = buildClaudeWorkflowPrompt(cmd, providerSettings);
  const runtimeExtensions = readClaudeRuntimeExtensions(cmd);
  const permission = normalizeRuntimePermission(cmd.permission) || "ask";
  const planMode = cmd.planMode === true;
  const permOpts = mapClaudeInitialPermission(permission, planMode);
  let lastSessionId = null;
  const providerOptions = {
    ...(runtimeProviderSettings?.options || {}),
    ...(providerSettings?.options || {}),
  };
  const additionalContext = readClaudeAdditionalContext(cmd.runtimeOptions);
  const managedAgents = readClaudeManagedAgents(providerOptions.managedSettings);
  if (managedAgents && !isRecord(providerOptions.agents)) {
    providerOptions.agents = managedAgents;
  }
  const abortAfterMs = providerOptions.abortAfterMs;
  delete providerOptions.abortAfterMs;
  let abortTimer = null;
  let interruptedByUser = false;
  const abortController = new AbortController();
  providerOptions.abortController = abortController;
  if (typeof abortAfterMs === "number" && abortAfterMs > 0) {
    abortTimer = setTimeout(() => {
      abortController.abort();
    }, abortAfterMs);
  }
  const ctx = {
    protocol: context.protocol,
    interactions: context.interactions,
    emitToolConsentTimeline: context.emitToolConsentTimeline,
    emitClaudePlanTimeline,
    claudeStream: createClaudeStreamState(),
    executionPermission: permission,
    query: null,
    latestAssistantText: "",
    deniedTools: new Map(),
    textFragments: new Map(),
    textFragmentsEmittedCount: 0,
    reasoningBlocks: new Map(),
    sawAssistantTextBlock: false,
    resultSeen: false,
    compactResult: null,
    compactError: null,
    activeTools: new Map(),
    commandEdits: new Map(),
    sessionId: stringOrNull(resumeSessionId),
  };
  const liliaAskUserServer = createLiliaAskUserServer({
    requestAskUser: context.interactions.requestAskUser,
    requestQuotaUsage: context.interactions.requestQuotaUsage,
    conversationContext: cmd.conversationContext,
    architectureHandler: createArchitectureChangeHandler({ cmd, ctx, backend: "claude" }),
    createServer: context.createSdkMcpServer || createSdkMcpServer,
    createTool: context.createClaudeTool || tool,
  });
  const options = {
    cwd: workingDir,
    model: model || undefined,
    resume: resumeSessionId || undefined,
    includePartialMessages: true,
    promptSuggestions: overrides.skipPromptSuggestions === true ? false : true,
    canUseTool: createClaudeCanUseTool(ctx),
    hooks: mergeClaudeHookMap(runtimeExtensions.hooks, createClaudeHooks(ctx)),
    onElicitation: (request) => requestClaudeMcpElicitation(request, ctx),
    systemPrompt: buildClaudeSystemPrompt(context.platform || process.platform, additionalContext),
    mcpServers: {
      lilia: liliaAskUserServer,
      ...runtimeExtensions.mcpServers,
    },
    toolAliases: {
      [ASK_USER_TOOL_NAME]: ASK_USER_MCP_TOOL_NAME,
      [QUOTA_USAGE_TOOL_NAME]: QUOTA_USAGE_MCP_TOOL_NAME,
      [CONVERSATION_CONTEXT_TOOL_NAME]: CONVERSATION_CONTEXT_MCP_TOOL_NAME,
      [ARCHITECTURE_TOOL_NAME]: ARCHITECTURE_MCP_TOOL_NAME,
    },
    toolConfig: {
      askUserQuestion: { previewFormat: "markdown" },
      queryConversationContext: { previewFormat: "markdown" },
      updateProjectArchitecture: { previewFormat: "markdown" },
    },
    ...(runtimeExtensions.skills.length > 0 ? { skills: runtimeExtensions.skills } : {}),
    ...(runtimeExtensions.plugins.length > 0 ? { plugins: runtimeExtensions.plugins } : {}),
    ...providerOptions,
    ...permOpts,
  };
  if (overrides.emitRuntimeWarnings !== false) {
    emitRuntimeExtensionWarnings(context.protocol, "claude", runtimeExtensions.warnings);
  }

  const createClaudeQuery = context.createClaudeQuery || query;
  const claudeQuery = createClaudeQuery({
    prompt: overrides.promptStream ?? singleClaudePromptStream(workflowPrompt || prompt),
    options,
  });
  ctx.query = claudeQuery;
  context.interactions?.handleSettingsUpdate?.((update) => {
    applyClaudeRuntimePermission(ctx, update?.permission);
  });
  const unregisterInterrupt = context.interactions?.handleInterruptTurn?.(() => {
    interruptedByUser = true;
    abortController.abort();
  }) ?? null;
  try {
    for await (const msg of claudeQuery) {
      if (msg.session_id) {
        lastSessionId = msg.session_id;
        ctx.sessionId = msg.session_id;
      }

      try {
        mapClaudeSystemTimeline(msg, ctx);
        switch (msg.type) {
          case "stream_event": {
            handleClaudeStreamEvent(msg, ctx);
            break;
          }
          case "assistant": {
            const content = msg?.message?.content;
            if (Array.isArray(content)) {
              for (const b of content) {
                if (b && b.type === "text") {
                  ctx.sawAssistantTextBlock = true;
                  if (typeof b.text === "string" && b.text.trim()) {
                    ctx.latestAssistantText = b.text;
                  }
                }
                if (b && b.type === "tool_use") {
                  context.protocol.emit({
                    type: RUNNER_TOOL_USE_EVENT_TYPE,
                    name: b.name,
                    input: b.input,
                  });
                  emitClaudeToolTimeline(b, msg, ctx);
                }
              }
            }
            break;
          }
          case "result": {
            ctx.resultSeen = true;
            const errorSummary = msg.is_error
              ? (Array.isArray(msg.errors) ? msg.errors.join("\n") : msg.subtype) || ""
              : "";
            const interrupted = interruptedByUser || msg.subtype === "interrupted";
            const status = interrupted ? "cancelled" : msg.is_error ? "error" : "success";
            const fragmentsEmitted = finalizeClaudeTextFragments(ctx, status);
            sweepActiveClaudeTools(ctx, status, msg?.session_id);
            finalizeClaudeReasoningTimeline(ctx, msg?.session_id || lastSessionId);
            emitClaudeTextResultFallback(ctx, msg, status, fragmentsEmitted, lastSessionId);
            emitClaudeContextUsage(context, msg.usage);
            context.protocol.emitTimeline({
              kind: "turn",
              status,
              title: interrupted
                ? "Claude turn interrupted"
                : msg.is_error ? "Claude turn failed" : "Claude turn completed",
              summary: errorSummary,
              payload: {
                backend: "claude",
                subtype: interrupted ? "interrupted" : msg.subtype,
                stopReason: msg.stop_reason,
                terminalReason: msg.terminal_reason,
                totalCostUsd: msg.total_cost_usd,
                usage: msg.usage,
                modelUsage: msg.modelUsage,
                permissionDenials: msg.permission_denials,
                errors: msg.errors,
                sessionId: msg.session_id || lastSessionId,
              },
              sourceId: msg.uuid,
            });
            if (overrides.suppressDone !== true) {
              context.protocol.emit({
                type: RUNNER_DONE_EVENT_TYPE,
                sessionId: msg.session_id || lastSessionId,
                subtype: interrupted ? "interrupted" : msg.subtype,
              });
            }
            break;
          }
          case "prompt_suggestion": {
            if (typeof msg.suggestion === "string" && msg.suggestion.trim()) {
              context.protocol.emit({
                type: RUNNER_PROMPT_SUGGESTION_EVENT_TYPE,
                suggestion: msg.suggestion,
                uuid: msg.uuid,
              });
            }
            break;
          }
          case "user":
          case "user_replay": {
            const content = msg?.message?.content;
            if (Array.isArray(content)) {
              for (const b of content) {
                if (b && b.type === "tool_result") {
                  emitClaudeToolResultTimeline(b, msg, ctx);
                }
              }
            }
            break;
          }
          case "system":
          default:
            break;
        }
      } catch (err) {
        context.protocol.emitTimeline({
          kind: "error",
          status: "error",
          title: "Claude event mapping",
          summary: err?.message || String(err),
          payload: {
            backend: "claude",
            rawType: msg?.type,
          },
          sourceId: msg?.uuid,
        });
      }
    }
  } catch (err) {
    if (!interruptedByUser || !isClaudeAbortError(err)) {
      throw err;
    }
  } finally {
    if (abortTimer) clearTimeout(abortTimer);
    unregisterInterrupt?.();
  }
  if ((lastSessionId || interruptedByUser) && !ctx.resultSeen) {
    const status = interruptedByUser ? "cancelled" : "success";
    const subtype = interruptedByUser ? "interrupted" : "success";
    const fragmentsEmitted = finalizeClaudeTextFragments(ctx, status);
    sweepActiveClaudeTools(ctx, status, lastSessionId);
    finalizeClaudeReasoningTimeline(ctx, lastSessionId);
    emitClaudeTextResultFallback(ctx, null, status, fragmentsEmitted, lastSessionId);
    context.protocol.emitTimeline({
      kind: "turn",
      status,
      title: interruptedByUser ? "Claude turn interrupted" : "Claude turn completed",
      summary: "",
      payload: {
        backend: "claude",
        subtype,
        sessionId: lastSessionId,
      },
      sourceId: `${lastSessionId}:turn:done`,
    });
    if (overrides.suppressDone !== true) {
      context.protocol.emit({ type: RUNNER_DONE_EVENT_TYPE, sessionId: lastSessionId, subtype });
    }
  }
  return {
    sessionId: lastSessionId,
    compactResult: ctx.compactResult,
    compactError: ctx.compactError,
  };
}

function isClaudeAbortError(err) {
  const name = typeof err?.name === "string" ? err.name : "";
  const message = typeof err?.message === "string" ? err.message : String(err ?? "");
  return name === "AbortError" || /abort|aborted|cancelled|canceled/i.test(message);
}

export async function runClaude(cmd, context) {
  handleExperimentalProviderOptions(cmd, context, "claude");
  const { cwd } = cmd;
  const workingDir = cwd || (context.cwd ? context.cwd() : process.cwd());
  if (await runClaudeSessionForkRuntimeCommand(cmd, context, workingDir)) return;
  await runClaudeAutoSessionFork(cmd, context, workingDir);
  if (await runClaudeCompactWorkflow(cmd, context, workingDir)) return;
  if (await runClaudeSessionManagementRuntimeCommand(cmd, context, workingDir)) return;
  if (await runClaudeProviderSettingsRuntimeCommand(cmd, context)) return;
  if (runClaudeRemoteEnvironmentRuntimeCommand(cmd, context)) return;
  if (runClaudeSandboxDiagnosticsRuntimeCommand(cmd, context)) return;
  if (runClaudeProcessSessionRuntimeCommand(cmd, context)) return;
  if (await runClaudeLocalLiliaWorkflow(cmd, context)) return;
  await runClaudeQueryTurn(cmd, context, workingDir);
}

export function handleClaudeStreamEvent(msg, ctx) {
  dispatchClaudeStreamEvent({
    event: msg?.event,
    state: ctx.claudeStream,
    onTextStart: ({ blockKey }) => {
      if (blockKey === null || blockKey === undefined) return;
      getOrCreateClaudeTextFragment(ctx, blockKey, msg?.session_id);
      emitAssistantTextFragmentTimeline(ctx.protocol, "", "running", msg?.session_id, blockKey);
    },
    onTextDelta: ({ blockKey, text }) => {
      if (blockKey === null || blockKey === undefined) return;
      const fragment = getOrCreateClaudeTextFragment(ctx, blockKey, msg?.session_id);
      fragment.accumulatedText += text;
      ctx.latestAssistantText = fragment.accumulatedText;
      fragment.sessionId = msg?.session_id || fragment.sessionId;
      fragment.pacer.push(text);
    },
    onTextClose: ({ blockKey }) => {
      if (blockKey === null || blockKey === undefined) return;
      closeClaudeTextFragment(ctx, blockKey, msg?.session_id);
    },
    onReasoning: ({ blockKey, text }) => {
      if (blockKey === null || blockKey === undefined) return;
      const entry = getOrCreateClaudeReasoningBlock(ctx, blockKey, msg?.session_id);
      entry.sessionId = msg?.session_id || entry.sessionId;
      entry.lastText = text;
      entry.pacer.push(text);
    },
    onReasoningClose: ({ blockKey }) => {
      if (blockKey === null || blockKey === undefined) return;
      closeClaudeReasoningBlock(ctx, blockKey, msg?.session_id);
    },
  });
}

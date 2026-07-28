import { normalizeClaudeTool } from "@lilia/contracts/claudeTools.mjs";
import { ASK_USER_INTERACTION_KIND } from "@lilia/contracts/agentInteractionContract.mjs";
import { TIMELINE_DISPLAY_ERROR_SUMMARY_TEXT_LIMIT } from "@lilia/contracts/timelineContract.mjs";
import { finalizeClaudeReasoningBlocks } from "../claudeStream.mjs";
import { buildPlanPayload, isClaudePlanTool } from "../claudePlan.mjs";
import { isLiliaAskUserTool } from "../askUser.mjs";
import {
  emitAssistantTextFragmentTimeline,
  createSnapshotPacer,
  createTextPacer,
} from "../protocol.mjs";
import {
  fullTextOrNull,
  isRecord,
  normalizeTimelineStatus,
  oneLineSummary,
  shortText,
  stringOrNull,
} from "../utils.mjs";
import { commandEditFields } from "./permissions.mjs";
import { rememberClaudeTool } from "./state.mjs";

export function emitClaudePlanTimeline({
  ctx,
  toolName = "ExitPlanMode",
  input,
  output,
  fallbackPlan = "",
  approved = null,
  executionPermission,
  status,
  sourceId,
  sessionId,
}) {
  const planPayload = buildPlanPayload({
    input,
    output,
    fallbackPlan,
    approved,
    executionPermission,
    source: "ExitPlanMode",
  });
  rememberClaudeTool(ctx, sourceId, {
    name: toolName,
    kind: "plan",
    subkind: null,
    status,
    payload: planPayload,
  });
  ctx.protocol.emitTimeline({
    kind: "plan",
    status,
    title: toolName,
    summary: oneLineSummary(planPayload.revisionRequest || planPayload.plan),
    payload: {
      backend: "claude",
      toolName,
      ...planPayload,
      sessionId,
    },
    sourceId,
  });
  return planPayload;
}

export function claudeReasoningSourceId(sessionId, blockKey) {
  return `${sessionId || "claude"}:thinking:${blockKey}`;
}

export function emitClaudeReasoningTimeline(ctx, text, status, sessionId, blockKey) {
  ctx.protocol.emitTimeline({
    kind: "reasoning",
    status,
    title: "思考中",
    summary: text,
    payload: {
      backend: "claude",
      sessionId,
      text,
    },
    sourceId: claudeReasoningSourceId(sessionId, blockKey),
  });
}

export function getOrCreateClaudeReasoningBlock(ctx, blockKey, sessionId) {
  let entry = ctx.reasoningBlocks.get(blockKey);
  if (entry) return entry;
  const sid = sessionId || "claude";
  const pacer = createSnapshotPacer({
    emit: (text) => emitClaudeReasoningTimeline(ctx, text, "running", entry.sessionId, blockKey),
  });
  entry = { pacer, lastText: "", sessionId: sid };
  ctx.reasoningBlocks.set(blockKey, entry);
  return entry;
}

export function closeClaudeReasoningBlock(ctx, blockKey, sessionId) {
  const entry = ctx.reasoningBlocks.get(blockKey);
  if (!entry) return;
  entry.pacer.cancel();
  emitClaudeReasoningTimeline(
    ctx,
    entry.lastText,
    "success",
    sessionId || entry.sessionId,
    blockKey,
  );
  ctx.reasoningBlocks.delete(blockKey);
}

export function finalizeClaudeReasoningTimeline(ctx, sessionId) {
  for (const [blockKey, entry] of ctx.reasoningBlocks) {
    entry.pacer.cancel();
    emitClaudeReasoningTimeline(
      ctx,
      entry.lastText,
      "success",
      sessionId || entry.sessionId,
      blockKey,
    );
  }
  ctx.reasoningBlocks.clear();
  finalizeClaudeReasoningBlocks(ctx.claudeStream, () => {});
}

export function emitClaudeToolTimeline(block, msg, ctx) {
  const name = stringOrNull(block?.name) || "tool";
  const input = isRecord(block?.input) ? block.input : {};
  const sourceId = stringOrNull(block?.id || block?.tool_use_id || msg?.uuid);
  if (isLiliaAskUserTool(name)) {
    rememberClaudeTool(ctx, sourceId, {
      name,
      kind: ASK_USER_INTERACTION_KIND,
      subkind: null,
      hiddenAskUserTool: true,
      payload: {},
    });
    return;
  }
  if (isClaudePlanTool(name)) {
    const cached = sourceId ? ctx?.activeTools?.get(sourceId) : null;
    const cachedPayload = isRecord(cached?.payload) ? cached.payload : {};
    const approved = typeof cached?.payload?.approved === "boolean"
      ? cached.payload.approved
      : null;
    const status = cached?.status || (
      approved === false ? "cancelled" : approved === true ? "success" : "requires_action"
    );
    emitClaudePlanTimeline({
      ctx,
      toolName: name,
      input: { ...input, ...cachedPayload },
      fallbackPlan: ctx?.latestAssistantText || "",
      approved,
      executionPermission: ctx?.executionPermission || "ask",
      status,
      sourceId,
      sessionId: msg?.session_id,
    });
    return;
  }
  const commandEdit = sourceId ? ctx?.commandEdits?.get(sourceId) : null;
  const finalInput = commandEdit?.input ?? input;
  const normalized = normalizeClaudeTool(name, finalInput, {
    subagent_type: msg?.subagent_type,
    task_description: msg?.task_description,
  });
  const normalizedPayload = { ...normalized.payload, ...commandEditFields(commandEdit) };
  const denied = sourceId ? ctx?.deniedTools?.get(sourceId) : null;
  const payload = {
    backend: "claude",
    toolName: name,
    ...normalizedPayload,
    ...(denied ? {
      permissionDenied: true,
      reason: denied.reason,
      message: denied.message,
    } : {}),
    sessionId: msg?.session_id,
  };
  if (normalized.subkind) payload.subkind = normalized.subkind;
  if (!denied) {
    rememberClaudeTool(ctx, sourceId, {
      name,
      kind: normalized.kind,
      subkind: normalized.subkind,
      payload: normalizedPayload,
    });
  }
  ctx.protocol.emitTimeline({
    kind: normalized.kind,
    status: denied ? "error" : "started",
    title: name,
    summary: denied?.message || normalized.summary,
    payload,
    sourceId,
  });
}

export function isAskUserCancelledOutput(text) {
  if (!text || typeof text !== "string") return false;
  try {
    const parsed = JSON.parse(text);
    return isRecord(parsed) && parsed.cancelled === true;
  } catch {
    return false;
  }
}

export function emitClaudeToolResultTimeline(block, msg, ctx) {
  const sourceId = stringOrNull(block?.tool_use_id);
  if (!sourceId) return;
  const cached = ctx?.activeTools?.get(sourceId);
  if (cached?.hiddenAskUserTool === true) {
    ctx?.activeTools?.delete(sourceId);
    return;
  }
  const name = cached?.name || "tool";
  const cachedPayload = cached?.payload || {};
  const kind = cached?.kind || "tool";
  const subkind = cached?.subkind || null;
  const isError = block?.is_error === true;
  let text = "";
  if (typeof block?.content === "string") {
    text = block.content;
  } else if (Array.isArray(block?.content)) {
    text = block.content
      .filter((c) => c && c.type === "text" && typeof c.text === "string")
      .map((c) => c.text)
      .join("\n");
  }
  const askUserCancelled = kind === ASK_USER_INTERACTION_KIND && isAskUserCancelledOutput(text);
  const summary = isError && !askUserCancelled
    ? shortText(text, TIMELINE_DISPLAY_ERROR_SUMMARY_TEXT_LIMIT) || ""
    : "";
  const planPayload = kind === "plan"
    ? buildPlanPayload({
        input: cachedPayload,
        output: text,
        fallbackPlan: cachedPayload.plan,
        approved: typeof cachedPayload.approved === "boolean"
          ? cachedPayload.approved
          : !isError,
        executionPermission: cachedPayload.executionPermission,
        source: cachedPayload.source || "ExitPlanMode",
      })
    : null;
  const status = kind === "plan" && planPayload?.revisionRequest
    ? "cancelled"
    : askUserCancelled ? "cancelled" : isError ? "error" : "success";
  const payload = {
    backend: "claude",
    toolName: name,
    ...(planPayload || cachedPayload),
    isError,
    output: text,
    sessionId: msg?.session_id,
  };
  if (subkind) payload.subkind = subkind;
  const timelineSummary = kind === "plan"
    ? planPayload?.revisionRequest
      ? oneLineSummary(planPayload.revisionRequest)
      : !isError ? oneLineSummary(payload.plan) : summary
    : summary;
  ctx.protocol.emitTimeline({
    kind,
    status,
    title: name,
    summary: timelineSummary,
    payload,
    sourceId,
  });
  ctx?.activeTools?.delete(sourceId);
  ctx?.deniedTools?.delete(sourceId);
}

export function sweepActiveClaudeTools(ctx, status, sessionId) {
  if (!ctx?.activeTools || ctx.activeTools.size === 0) return;
  for (const [sourceId, info] of ctx.activeTools) {
    const statusForTool = info.kind === "plan" && info.payload?.revisionRequest
      ? "cancelled"
      : status;
    const payload = {
      backend: "claude",
      toolName: info.name,
      ...(info.payload || {}),
      sweptByTurnEnd: true,
      sessionId,
    };
    if (info.subkind) payload.subkind = info.subkind;
    ctx.protocol.emitTimeline({
      kind: info.kind,
      status: statusForTool,
      title: info.name,
      summary: info.kind === "plan" ? oneLineSummary(payload.revisionRequest || payload.plan) : "",
      payload,
      sourceId,
    });
  }
  ctx.activeTools.clear();
}

export function mapClaudeSystemTimeline(msg, ctx) {
  if (!isRecord(msg)) return;
  const subtype = stringOrNull(msg.subtype) || "";
  const { emitTimeline } = ctx.protocol;

  if (msg.type === "tool_progress") {
    const name = msg.tool_name || "tool";
    const normalized = normalizeClaudeTool(name, {});
    const payload = {
      backend: "claude",
      toolName: name,
      elapsedTimeSeconds: msg.elapsed_time_seconds,
      sessionId: msg.session_id,
    };
    if (normalized.subkind) payload.subkind = normalized.subkind;
    emitTimeline({
      kind: normalized.kind,
      status: "running",
      title: name,
      summary: `${msg.elapsed_time_seconds ?? 0}s`,
      payload,
      sourceId: msg.tool_use_id || msg.uuid,
    });
    return;
  }

  if (msg.type === "tool_use_summary") {
    emitTimeline({
      kind: "tool",
      status: "success",
      title: "Tool summary",
      summary: msg.summary,
      payload: {
        backend: "claude",
        precedingToolUseIds: msg.preceding_tool_use_ids,
        sessionId: msg.session_id,
      },
      sourceId: msg.uuid,
    });
    return;
  }

  if (msg.type === "auth_status") {
    const text = Array.isArray(msg.output) ? msg.output.join("\n") : msg.error;
    emitTimeline({
      kind: msg.error ? "error" : "turn",
      status: msg.error ? "error" : msg.isAuthenticating ? "running" : "success",
      title: "Authentication",
      summary: msg.error || text || "",
      payload: {
        backend: "claude",
        isAuthenticating: msg.isAuthenticating,
        sessionId: msg.session_id,
      },
      sourceId: msg.uuid,
    });
    return;
  }

  if (msg.type === "system") {
    switch (subtype) {
      case "init":
        emitTimeline({
          kind: "turn",
          status: "started",
          title: "Claude session",
          summary: msg.model || "",
          payload: {
            backend: "claude",
            model: msg.model,
            cwd: msg.cwd,
            permissionMode: msg.permissionMode,
            tools: msg.tools,
            mcpServers: msg.mcp_servers,
          },
          sourceId: msg.uuid,
        });
        return;
      case "task_started":
        emitTimeline({
          kind: "subagent",
          status: "started",
          title: msg.subagent_type || msg.task_type || "Task",
          summary: msg.description || msg.prompt || "",
          payload: {
            backend: "claude",
            description: msg.description,
            subagentType: msg.subagent_type,
            taskType: msg.task_type,
            workflowName: msg.workflow_name,
            sessionId: msg.session_id,
          },
          sourceId: msg.tool_use_id || msg.uuid,
        });
        return;
      case "task_progress":
        emitTimeline({
          kind: "subagent",
          status: "running",
          title: msg.subagent_type || "Task progress",
          summary: msg.summary || msg.description || msg.last_tool_name || "",
          payload: {
            backend: "claude",
            description: msg.description,
            subagentType: msg.subagent_type,
            usage: msg.usage,
            lastToolName: msg.last_tool_name,
            sessionId: msg.session_id,
          },
          sourceId: msg.tool_use_id || msg.uuid,
        });
        return;
      case "task_updated": {
        const patch = isRecord(msg.patch) ? msg.patch : {};
        emitTimeline({
          kind: "subagent",
          status: normalizeTimelineStatus(patch.status || "running"),
          title: "Task updated",
          summary: patch.error || patch.description || "",
          payload: {
            backend: "claude",
            patch,
            sessionId: msg.session_id,
          },
          sourceId: msg.uuid,
        });
        return;
      }
      case "task_notification":
        emitTimeline({
          kind: msg.status === "failed" ? "error" : "subagent",
          status: normalizeTimelineStatus(msg.status || "success"),
          title: "Task notification",
          summary: msg.summary || "",
          payload: {
            backend: "claude",
            status: msg.status,
            outputFile: msg.output_file,
            usage: msg.usage,
            sessionId: msg.session_id,
          },
          sourceId: msg.tool_use_id || msg.uuid,
        });
        return;
      case "notification":
        emitTimeline({
          kind: "turn",
          status: msg.priority === "immediate" ? "requires_action" : "info",
          title: msg.key || "Notification",
          summary: msg.text || "",
          payload: {
            backend: "claude",
            priority: msg.priority,
            color: msg.color,
            timeoutMs: msg.timeout_ms,
            sessionId: msg.session_id,
          },
          sourceId: msg.uuid,
        });
        return;
      case "api_retry":
        emitTimeline({
          kind: "turn",
          status: "running",
          title: "API retry",
          summary: msg.error || "",
          payload: {
            backend: "claude",
            attempt: msg.attempt,
            maxRetries: msg.max_retries,
            retryDelayMs: msg.retry_delay_ms,
            errorStatus: msg.error_status,
            sessionId: msg.session_id,
          },
          sourceId: msg.uuid,
        });
        return;
      case "status":
        if (msg.compact_result) {
          ctx.compactResult = msg.compact_result;
          ctx.compactError = msg.compact_error;
        }
        emitTimeline({
          kind: "turn",
          status: msg.compact_result === "failed"
            ? "error"
            : msg.compact_result === "success"
              ? "success"
              : msg.status === "compacting"
                ? "running"
                : msg.status || "info",
          title: "Claude status",
          summary: msg.compact_error || msg.status || "",
          payload: {
            backend: "claude",
            subkind: msg.compact_result || msg.compact_error || msg.status === "compacting"
              ? "compact"
              : undefined,
            status: msg.status,
            permissionMode: msg.permissionMode,
            compactResult: msg.compact_result,
            compactError: msg.compact_error,
            sessionId: msg.session_id,
          },
          sourceId: msg.uuid,
        });
        return;
      case "compact_boundary": {
        const metadata = isRecord(msg.compact_metadata) ? msg.compact_metadata : {};
        const preTokens = typeof metadata.pre_tokens === "number" ? metadata.pre_tokens : null;
        const postTokens = typeof metadata.post_tokens === "number" ? metadata.post_tokens : null;
        const tokenSummary = preTokens !== null && postTokens !== null
          ? `${preTokens} -> ${postTokens} tokens`
          : preTokens !== null
            ? `${preTokens} tokens`
            : "";
        emitTimeline({
          kind: "diagnostic",
          status: "success",
          title: "Claude compact boundary",
          summary: tokenSummary,
          payload: {
            backend: "claude",
            subkind: "compact",
            trigger: metadata.trigger,
            preTokens,
            postTokens,
            durationMs: metadata.duration_ms,
            preservedSegment: metadata.preserved_segment,
            preservedMessages: metadata.preserved_messages,
            sessionId: msg.session_id,
          },
          sourceId: msg.uuid,
        });
        return;
      }
      case "session_state_changed":
        emitTimeline({
          kind: "turn",
          status: msg.state || "info",
          title: "Session state",
          summary: msg.state || "",
          payload: {
            backend: "claude",
            state: msg.state,
            sessionId: msg.session_id,
          },
          sourceId: msg.uuid,
        });
        return;
      case "hook_started":
      case "hook_progress":
      case "hook_response":
        emitTimeline({
          kind: "tool",
          status:
            subtype === "hook_started"
              ? "started"
              : msg.outcome === "error"
                ? "error"
                : normalizeTimelineStatus(msg.outcome || "running"),
          title: msg.hook_name || "Hook",
          summary: msg.output || msg.stderr || msg.stdout || msg.hook_event || "",
          payload: {
            backend: "claude",
            subkind: "hook",
            hookName: msg.hook_name,
            hookEvent: msg.hook_event,
            exitCode: msg.exit_code,
            stdout: msg.stdout,
            stderr: msg.stderr,
            output: msg.output,
            sessionId: msg.session_id,
          },
          sourceId: msg.uuid,
        });
        return;
      case "permission_denied":
        emitTimeline({
          kind: "error",
          status: "error",
          title: msg.tool_name || "Permission denied",
          summary: msg.message || msg.decision_reason || "",
          payload: {
            backend: "claude",
            toolName: msg.tool_name,
            decisionReasonType: msg.decision_reason_type,
            decisionReason: msg.decision_reason,
            sessionId: msg.session_id,
          },
          sourceId: msg.tool_use_id || msg.uuid,
        });
        return;
      case "mirror_error":
        emitTimeline({
          kind: "error",
          status: "error",
          title: "Mirror error",
          summary: msg.error || "",
          payload: {
            backend: "claude",
            key: msg.key,
            sessionId: msg.session_id,
          },
          sourceId: msg.uuid,
        });
        return;
      default:
        break;
    }
  }

  if (msg.error) {
    emitTimeline({
      kind: "error",
      status: "error",
      title: stringOrNull(msg.type) || "Claude error",
      summary: msg.error,
      payload: {
        backend: "claude",
        type: msg.type,
        subtype,
        sessionId: msg.session_id,
      },
      sourceId: msg.uuid,
    });
  }
}

export function getOrCreateClaudeTextFragment(ctx, blockKey, sessionId) {
  let fragment = ctx.textFragments.get(blockKey);
  if (fragment) return fragment;
  const sid = sessionId || "claude";
  const pacer = createTextPacer({
    emit: (text) => emitAssistantTextFragmentTimeline(ctx.protocol, text, "running", fragment.sessionId, blockKey),
  });
  fragment = { pacer, accumulatedText: "", sessionId: sid };
  ctx.textFragments.set(blockKey, fragment);
  return fragment;
}

export function finalizeClaudeTextFragments(ctx, status) {
  for (const [blockKey, fragment] of ctx.textFragments) {
    fragment.pacer.finishImmediate();
    emitAssistantTextFragmentTimeline(
      ctx.protocol,
      fragment.accumulatedText,
      status,
      fragment.sessionId,
      blockKey,
    );
    ctx.textFragmentsEmittedCount += 1;
  }
  ctx.textFragments.clear();
  return ctx.textFragmentsEmittedCount;
}

export function closeClaudeTextFragment(ctx, blockKey, sessionId) {
  const fragment = ctx.textFragments.get(blockKey);
  if (!fragment) return;
  fragment.pacer.finishImmediate();
  emitAssistantTextFragmentTimeline(
    ctx.protocol,
    fragment.accumulatedText,
    "success",
    sessionId || fragment.sessionId,
    blockKey,
  );
  ctx.textFragments.delete(blockKey);
  ctx.textFragmentsEmittedCount += 1;
}

export function emitClaudeTextResultFallback(ctx, msg, status, fragmentsEmitted, lastSessionId) {
  if (fragmentsEmitted > 0) return;
  const resultText = fullTextOrNull(msg?.result);
  const sessionId = msg?.session_id || lastSessionId || "claude";
  if (resultText) {
    emitAssistantTextFragmentTimeline(ctx.protocol, resultText, status, sessionId, "result");
  } else if (ctx.sawAssistantTextBlock) {
    emitAssistantTextFragmentTimeline(ctx.protocol, "", status, sessionId, "result");
  }
}

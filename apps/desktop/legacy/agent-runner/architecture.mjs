import { z } from "zod/v4";
import {
  ARCHITECTURE_TOOL_NAME,
  ARCHITECTURE_TOOL_NAMES as LILIA_ARCHITECTURE_TOOL_NAMES,
  PROJECT_ARCHITECTURE_APPLIED_INTERACTION_STATUS,
  PROJECT_ARCHITECTURE_DEFAULT_EDGE_TYPE,
  PROJECT_ARCHITECTURE_DEFAULT_INTERACTION_STATUS,
  PROJECT_ARCHITECTURE_DEFAULT_NODE_TYPE,
  PROJECT_ARCHITECTURE_READONLY_INTERACTION_STATUS,
  UPDATE_PROJECT_ARCHITECTURE_INPUT_SCHEMA,
  isLiliaArchitectureTool,
  projectArchitecturePermissionFromRuntimePermission,
} from "@lilia/contracts/architectureContract.mjs";
import { ARCHITECTURE_INTERACTION_KIND } from "@lilia/contracts/agentInteractionContract.mjs";
import { isRecord, oneLineSummary, stringOrNull } from "./utils.mjs";

export { LILIA_ARCHITECTURE_TOOL_NAMES, isLiliaArchitectureTool };

const architectureChangeSchema = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("upsert_node"),
    node: z.object({
      id: z.string().min(1),
      label: z.string().default(""),
      type: z.string().default(PROJECT_ARCHITECTURE_DEFAULT_NODE_TYPE),
      summary: z.string().default(""),
      paths: z.array(z.string()).default([]),
      tags: z.array(z.string()).default([]),
    }),
  }),
  z.object({
    type: z.literal("remove_node"),
    nodeId: z.string().min(1),
  }),
  z.object({
    type: z.literal("upsert_edge"),
    edge: z.object({
      id: z.string().min(1),
      from: z.string().min(1),
      to: z.string().min(1),
      type: z.string().default(PROJECT_ARCHITECTURE_DEFAULT_EDGE_TYPE),
      label: z.string().default(""),
      summary: z.string().default(""),
    }),
  }),
  z.object({
    type: z.literal("remove_edge"),
    edgeId: z.string().min(1),
  }),
  z.object({
    type: z.literal("set_summary"),
    summary: z.string().default(""),
  }),
]);

export const updateProjectArchitectureInputSchema = z.object({
  reason: z.string().default(""),
  changes: z.array(architectureChangeSchema).min(1),
});

export const codexUpdateProjectArchitectureDynamicTool = {
  name: ARCHITECTURE_TOOL_NAME,
  description: [
    "Update Lilia's project-level architecture graph with structured node/edge changes.",
    "Use it only when the conversation changes or clarifies architecture knowledge.",
  ].join(" "),
  inputSchema: UPDATE_PROJECT_ARCHITECTURE_INPUT_SCHEMA,
};

export function architectureContextEnabled(conversationContext) {
  return Boolean(stringOrNull(conversationContext?.projectId));
}

export function normalizeArchitectureToolInput(input) {
  const parsed = updateProjectArchitectureInputSchema.parse(isRecord(input) ? input : {});
  return {
    reason: parsed.reason.trim(),
    changes: parsed.changes,
  };
}

export function createArchitectureChangeHandler({
  cmd,
  ctx,
  backend,
}) {
  return async (input) => {
    const normalized = normalizeArchitectureToolInput(input);
    const projectId = stringOrNull(cmd?.conversationContext?.projectId);
    if (!projectId) {
      return {
        ok: false,
        cancelled: true,
        message: "当前对话不属于项目，无法更新项目架构图。",
      };
    }
    const permission = projectArchitecturePermissionFromRuntimePermission(cmd?.permission);
    const allowedByPlan = permission === "full" &&
      changesAllowedByPlan(normalized.changes, ctx?.approvedArchitectureImpacts);
    const status = permission === "readonly"
      ? PROJECT_ARCHITECTURE_READONLY_INTERACTION_STATUS
      : allowedByPlan
        ? PROJECT_ARCHITECTURE_APPLIED_INTERACTION_STATUS
        : PROJECT_ARCHITECTURE_DEFAULT_INTERACTION_STATUS;
    const payload = {
      projectId,
      taskId: stringOrNull(cmd?.taskId) || "",
      turnId: stringOrNull(ctx?.currentTurnId) || stringOrNull(cmd?.turnId),
      backend,
      permission,
      reason: normalized.reason,
      changes: normalized.changes,
      status,
      requiresConfirmation: status === PROJECT_ARCHITECTURE_DEFAULT_INTERACTION_STATUS,
    };

    if (permission === "readonly") {
      emitArchitectureTimeline(ctx, `arch-proposed-${Date.now()}`, payload, "info", null);
      return {
        ok: true,
        applied: false,
        requiresConfirmation: false,
        status: PROJECT_ARCHITECTURE_READONLY_INTERACTION_STATUS,
        message: "架构图变更已作为只读提议记录在时间线中。",
      };
    }

    if (allowedByPlan && ctx?.interactions?.requestArchitectureChange) {
      const result = await ctx.interactions.requestArchitectureChange(payload, {
        backend,
        autoApply: true,
      });
      return architectureToolResult(result);
    }

    if (!ctx?.interactions?.requestArchitectureChange) {
      return {
        ok: false,
        cancelled: true,
        message: "当前 runtime 不支持架构图确认。",
      };
    }
    const result = await ctx.interactions.requestArchitectureChange(payload, { backend });
    return architectureToolResult(result);
  };
}

export function emitArchitectureTimeline(ctx, id, payload, status, result = null) {
  ctx?.protocol?.emitTimeline?.({
    kind: ARCHITECTURE_INTERACTION_KIND,
    status,
    title: "项目架构图",
    summary: architectureSummary(payload),
    payload: {
      ...payload,
      interaction: ARCHITECTURE_INTERACTION_KIND,
      requestId: id,
      ...(result ? { result } : {}),
    },
    sourceId: id,
  });
}

function architectureToolResult(result) {
  if (result?.decision === "allow") {
    return {
      ok: true,
      applied: true,
      version: result?.graph?.version ?? result?.event?.afterVersion ?? null,
      message: result?.message || "架构图已更新。",
    };
  }
  return {
    ok: false,
    applied: false,
    cancelled: true,
    message: result?.message || "架构图变更未应用。",
  };
}

function architectureSummary(payload) {
  const reason = stringOrNull(payload?.reason);
  if (reason) return oneLineSummary(reason);
  const changes = Array.isArray(payload?.changes) ? payload.changes : [];
  if (changes.length === 0) return "无架构变更";
  return oneLineSummary(changes.map(changeLabel).slice(0, 2).join("；"));
}

function changeLabel(change) {
  if (!isRecord(change)) return "架构变更";
  if (change.type === "upsert_node") {
    const node = isRecord(change.node) ? change.node : {};
    return `节点 ${stringOrNull(node.label) || stringOrNull(node.id) || ""}`.trim();
  }
  if (change.type === "remove_node") return `移除节点 ${stringOrNull(change.nodeId) || ""}`.trim();
  if (change.type === "upsert_edge") {
    const edge = isRecord(change.edge) ? change.edge : {};
    return `关系 ${stringOrNull(edge.label) || stringOrNull(edge.id) || ""}`.trim();
  }
  if (change.type === "remove_edge") return `移除关系 ${stringOrNull(change.edgeId) || ""}`.trim();
  if (change.type === "set_summary") return "更新摘要";
  return "架构变更";
}

function changesAllowedByPlan(changes, impacts) {
  if (!Array.isArray(impacts) || impacts.length === 0) return false;
  const allowed = new Set();
  for (const impact of impacts) {
    const impactChanges = Array.isArray(impact?.changes) ? impact.changes : [];
    for (const change of impactChanges) allowed.add(stableChangeKey(change));
  }
  return changes.every((change) => allowed.has(stableChangeKey(change)));
}

function stableChangeKey(change) {
  return JSON.stringify(sortRecord(change));
}

function sortRecord(value) {
  if (Array.isArray(value)) return value.map(sortRecord);
  if (!isRecord(value)) return value;
  return Object.fromEntries(
    Object.entries(value)
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([key, item]) => [key, sortRecord(item)]),
  );
}

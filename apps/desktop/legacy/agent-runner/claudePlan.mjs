import { claudePermissionRuntime } from "@lilia/contracts/permissionModes.mjs";
import {
  TIMELINE_DISPLAY_ALLOWED_PROMPT_TEXT_LIMIT,
  TIMELINE_DISPLAY_CLAUDE_PLAN_TEXT_LIMIT,
  TIMELINE_DISPLAY_DETAIL_TEXT_LIMIT,
  TIMELINE_DISPLAY_FILE_CHANGE_PATH_TEXT_LIMIT,
  TIMELINE_DISPLAY_TINY_TEXT_LIMIT,
} from "@lilia/contracts/timelineContract.mjs";

export {
  PLAN_APPROVAL_QUESTION_ID,
  buildPlanApprovalSpec,
  buildPlanRevisionDenyMessage,
  isPlanApprovalAccepted,
  readPlanRevisionRequest,
} from "./planApproval.mjs";

import {
  compactLine,
  parseRecordJson,
  readArrayRecords,
  readFirstString,
  readFirstText,
} from "../../../packages/contracts/src/toolUtils.mjs";
export {
  isClaudePlanTool,
  isReadonlyDeniedClaudeTool,
} from "@lilia/contracts/claudePlanContract.mjs";

export function normalizeClaudePermissionMode(permission) {
  return claudePermissionRuntime(permission)?.permissionMode ?? "default";
}

export function readPlanAllowedPrompts(input) {
  return readArrayRecords(input?.allowedPrompts)
    .map((item) => ({
      tool: compactLine(item.tool, TIMELINE_DISPLAY_TINY_TEXT_LIMIT) || "tool",
      prompt: compactLine(item.prompt, TIMELINE_DISPLAY_ALLOWED_PROMPT_TEXT_LIMIT),
    }))
    .filter((item) => item.prompt);
}

export function readPlanArchitectureImpacts(input) {
  const raw = Array.isArray(input?.architectureImpacts)
    ? input.architectureImpacts
    : Array.isArray(input?.architecture_impacts)
      ? input.architecture_impacts
      : [];
  return raw
    .map((item) => {
      if (!item || typeof item !== "object" || Array.isArray(item)) return null;
      const changes = Array.isArray(item.changes) ? item.changes : [];
      return {
        reason: compactLine(item.reason, 800),
        changes,
      };
    })
    .filter((item) => item && item.changes.length > 0);
}

export function extractPlanTextFromInput(input) {
  return readFirstText(
    input,
    ["plan", "content", "text", "markdown"],
    TIMELINE_DISPLAY_CLAUDE_PLAN_TEXT_LIMIT,
  );
}

export function extractPlanResult(output) {
  const parsed = parseRecordJson(output) ?? {};
  const plan = readFirstText(
    parsed,
    ["plan", "content", "text", "markdown"],
    TIMELINE_DISPLAY_CLAUDE_PLAN_TEXT_LIMIT,
  );
  return {
    plan,
    filePath: readFirstString(
      parsed,
      ["filePath", "file_path"],
      TIMELINE_DISPLAY_FILE_CHANGE_PATH_TEXT_LIMIT,
    ) || undefined,
    isAgent: parsed.isAgent === true,
    hasTaskTool: parsed.hasTaskTool === true,
    planWasEdited: parsed.planWasEdited === true,
    awaitingLeaderApproval: parsed.awaitingLeaderApproval === true,
    revisionRequest: readFirstString(
      parsed,
      ["revisionRequest", "revision_request"],
      TIMELINE_DISPLAY_DETAIL_TEXT_LIMIT,
    ),
  };
}
export function buildPlanPayload({
  input,
  output,
  fallbackPlan = "",
  approved = null,
  executionPermission,
  source = "ExitPlanMode",
}) {
  const result = extractPlanResult(output);
  const inputPlan = extractPlanTextFromInput(input);
  const revisionRequest =
    result.revisionRequest ||
    readFirstString(
      input,
      ["revisionRequest", "revision_request"],
      TIMELINE_DISPLAY_DETAIL_TEXT_LIMIT,
    );
  const preserveInputPlan = Boolean(revisionRequest) || approved === false;
  const plan = preserveInputPlan
    ? inputPlan || fallbackPlan || result.plan || ""
    : result.plan || inputPlan || fallbackPlan || "";
  return {
    source,
    plan,
    allowedPrompts: readPlanAllowedPrompts(input),
    architectureImpacts: readPlanArchitectureImpacts(input),
    approved,
    executionPermission,
    ...(revisionRequest ? { revisionRequest } : {}),
    ...(result.filePath ? { filePath: result.filePath } : {}),
    ...(result.planWasEdited ? { planWasEdited: true } : {}),
    ...(result.awaitingLeaderApproval ? { awaitingLeaderApproval: true } : {}),
    ...(result.isAgent ? { isAgent: true } : {}),
    ...(result.hasTaskTool ? { hasTaskTool: true } : {}),
  };
}

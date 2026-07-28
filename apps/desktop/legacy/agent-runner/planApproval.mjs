import {
  ASK_USER_CANCEL_ANSWER_VALUE,
  ASK_USER_CONFIRM_ANSWER_VALUE,
  createPlanApprovalAskUserSpec,
} from "@lilia/contracts/askUserContract.mjs";
import { PLAN_APPROVAL_SPEC_DEFAULTS } from "@lilia/contracts/agentInteractionContract.mjs";
import { TIMELINE_DISPLAY_DETAIL_TEXT_LIMIT } from "@lilia/contracts/timelineContract.mjs";
import { compactLine, isRecord } from "../../../packages/contracts/src/toolUtils.mjs";

export const PLAN_APPROVAL_QUESTION_ID = PLAN_APPROVAL_SPEC_DEFAULTS.questionId;

export function planApprovalBackendLabel(backend = "claude") {
  return backend === "codex" ? "Codex" : "Claude";
}

export function buildPlanApprovalSpec(backend = "claude") {
  const label = planApprovalBackendLabel(backend);
  return createPlanApprovalAskUserSpec({
    title: renderPlanApprovalTemplate(PLAN_APPROVAL_SPEC_DEFAULTS.titleTemplate, label),
    source: renderPlanApprovalTemplate(PLAN_APPROVAL_SPEC_DEFAULTS.sourceTemplate, label),
  });
}

function renderPlanApprovalTemplate(template, backendLabel) {
  return String(template || "").replace("{backend}", backendLabel);
}

export function isPlanApprovalAccepted(result) {
  if (!isRecord(result) || result.cancelled === true) return false;
  const answers = isRecord(result.answers) ? result.answers : {};
  const answer = answers[PLAN_APPROVAL_QUESTION_ID];
  return isRecord(answer) && answer.value === ASK_USER_CONFIRM_ANSWER_VALUE;
}

export function readPlanRevisionRequest(result) {
  if (!isRecord(result) || result.cancelled === true) return "";
  const answers = isRecord(result.answers) ? result.answers : {};
  const answer = answers[PLAN_APPROVAL_QUESTION_ID];
  if (!isRecord(answer)) return "";
  if (answer.value === ASK_USER_CONFIRM_ANSWER_VALUE || answer.value === ASK_USER_CANCEL_ANSWER_VALUE) return "";
  return compactLine(answer.notes, TIMELINE_DISPLAY_DETAIL_TEXT_LIMIT);
}

export function buildPlanRevisionDenyMessage(revisionRequest) {
  const request = compactLine(revisionRequest, TIMELINE_DISPLAY_DETAIL_TEXT_LIMIT);
  return [
    "用户要求修改计划，暂不执行当前计划。",
    request ? `修改要求：${request}` : "",
    "请根据这条修改要求调整计划，然后再次请求确认。",
  ].filter(Boolean).join("\n");
}

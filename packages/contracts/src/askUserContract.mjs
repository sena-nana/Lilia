import askUserContract from "./ask-user-contract.json" with { type: "json" };
import { PLAN_APPROVAL_SPEC_DEFAULTS } from "./agentInteractionContract.mjs";

const manifest = deepFreeze(askUserContract);

export const ASK_USER_CONTRACT = manifest;
export const ASK_USER_MODES = manifest.askUserModes;
export const DEFAULT_ASK_USER_MODE = manifest.defaultAskUserMode;
export const DEFAULT_ASK_USER_MODE_WITH_OPTIONS = manifest.defaultAskUserModeWithOptions;
export const ASK_USER_SINGLE_SELECT_MODE = manifest.singleSelectAskUserMode;
export const ASK_USER_MULTI_SELECT_MODE = manifest.multiSelectAskUserMode;
export const MAX_ASK_USER_TOOL_QUESTIONS = manifest.maxAskUserToolQuestions;
export const MIN_ASK_USER_TOOL_OPTIONS = manifest.minAskUserToolOptions;
export const MAX_ASK_USER_TOOL_OPTIONS = manifest.maxAskUserToolOptions;
export const MAX_REQUEST_USER_INPUT_OPTIONS =
  manifest.maxRequestUserInputOptions ?? manifest.maxCodexRequestUserInputOptions;
/** @deprecated Use MAX_REQUEST_USER_INPUT_OPTIONS. */
export const MAX_CODEX_REQUEST_USER_INPUT_OPTIONS = MAX_REQUEST_USER_INPUT_OPTIONS;
export const MAX_ASK_USER_HEADER_TEXT = manifest.maxAskUserHeaderText;
export const MAX_ASK_USER_QUESTION_TEXT = manifest.maxAskUserQuestionText;
export const ASK_USER_TOOL_NAMES = manifest.askUserToolNames;
export const ASK_USER_TOOL_NAME = ASK_USER_TOOL_NAMES[0];
export const ASK_USER_CLAUDE_TOOL_NAME = ASK_USER_TOOL_NAMES[1];
export const ASK_USER_MCP_TOOL_NAME = ASK_USER_TOOL_NAMES[2];
export const ASK_USER_QUESTION_INPUT_SCHEMA = manifest.askUserQuestionInputSchema;
export const ASK_USER_INTENTS = manifest.askUserIntents;
export const PLAN_APPROVAL_INTENT = manifest.planApprovalIntent;
export const ASK_USER_CONFIRM_ANSWER_VALUE = manifest.confirmAskUserAnswerValue;
export const ASK_USER_CANCEL_ANSWER_VALUE = manifest.cancelAskUserAnswerValue;
export const PLAN_APPROVAL_REVISION_REQUEST_ANSWER_VALUE = manifest.planApprovalRevisionRequestAnswerValue;

const askUserModeSet = new Set(ASK_USER_MODES);
const askUserIntentSet = new Set(ASK_USER_INTENTS);
const askUserToolNameSet = new Set(ASK_USER_TOOL_NAMES);

export function isAskUserMode(value) {
  return typeof value === "string" && askUserModeSet.has(value);
}

export function normalizeAskUserMode(value, fallback = DEFAULT_ASK_USER_MODE) {
  return isAskUserMode(value) ? value : fallback;
}

export function isAskUserIntent(value) {
  return typeof value === "string" && askUserIntentSet.has(value);
}

export function normalizeAskUserIntent(value) {
  return isAskUserIntent(value) ? value : undefined;
}

export function isPlanApprovalAskUserSpec(spec) {
  return spec?.intent === PLAN_APPROVAL_INTENT;
}

export function isPlanApprovalConfirmAskUserSpec(spec) {
  const question = spec?.questions?.[0];
  return isPlanApprovalAskUserSpec(spec) &&
    spec.questions.length === 1 &&
    question?.mode === DEFAULT_ASK_USER_MODE;
}

export function createPlanApprovalAskUserSpec(input) {
  return {
    title: input.title,
    source: input.source,
    intent: PLAN_APPROVAL_INTENT,
    dismissable: PLAN_APPROVAL_SPEC_DEFAULTS.dismissable,
    questions: [
      {
        id: PLAN_APPROVAL_SPEC_DEFAULTS.questionId,
        header: PLAN_APPROVAL_SPEC_DEFAULTS.questionHeader,
        question: input.question ?? "",
        mode: DEFAULT_ASK_USER_MODE,
        confirmLabel: PLAN_APPROVAL_SPEC_DEFAULTS.confirmLabel,
        cancelLabel: PLAN_APPROVAL_SPEC_DEFAULTS.cancelLabel,
      },
    ],
  };
}

export function isLiliaAskUserTool(toolName) {
  return askUserToolNameSet.has(String(toolName || ""));
}

function deepFreeze(value) {
  if (!value || typeof value !== "object") return value;
  for (const child of Object.values(value)) {
    deepFreeze(child);
  }
  return Object.freeze(value);
}

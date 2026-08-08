import {
  ASK_USER_CLAUDE_TOOL_NAME,
  ASK_USER_CANCEL_ANSWER_VALUE,
  ASK_USER_CONFIRM_ANSWER_VALUE,
  ASK_USER_INTENTS,
  ASK_USER_MCP_TOOL_NAME,
  ASK_USER_MODES,
  ASK_USER_MULTI_SELECT_MODE,
  ASK_USER_QUESTION_INPUT_SCHEMA,
  ASK_USER_SINGLE_SELECT_MODE,
  ASK_USER_TOOL_NAME,
  ASK_USER_TOOL_NAMES,
  DEFAULT_ASK_USER_MODE,
  DEFAULT_ASK_USER_MODE_WITH_OPTIONS,
  isAskUserIntent as isAskUserIntentImpl,
  isAskUserMode as isAskUserModeImpl,
  isLiliaAskUserTool as isLiliaAskUserToolImpl,
  isPlanApprovalAskUserSpec as isPlanApprovalAskUserSpecImpl,
  isPlanApprovalConfirmAskUserSpec as isPlanApprovalConfirmAskUserSpecImpl,
  MAX_ASK_USER_HEADER_TEXT,
  MAX_ASK_USER_QUESTION_TEXT,
  MAX_ASK_USER_TOOL_OPTIONS,
  MAX_ASK_USER_TOOL_QUESTIONS,
  MAX_CODEX_REQUEST_USER_INPUT_OPTIONS,
  MAX_REQUEST_USER_INPUT_OPTIONS,
  MIN_ASK_USER_TOOL_OPTIONS,
  normalizeAskUserIntent as normalizeAskUserIntentImpl,
  normalizeAskUserMode as normalizeAskUserModeImpl,
  PLAN_APPROVAL_INTENT,
  PLAN_APPROVAL_REVISION_REQUEST_ANSWER_VALUE,
  createPlanApprovalAskUserSpec as createPlanApprovalAskUserSpecImpl,
  type AskUserCancelAnswerValue as AskUserContractCancelAnswerValue,
  type AskUserConfirmAnswerValue as AskUserContractConfirmAnswerValue,
  type AskUserIntent as AskUserContractIntent,
  type AskUserMode as AskUserContractMode,
  type AskUserToolName as AskUserContractToolName,
  type PlanApprovalRevisionRequestAnswerValue as ContractPlanApprovalRevisionRequestAnswerValue,
} from "./askUserContract.mjs";

export {
  ASK_USER_CLAUDE_TOOL_NAME,
  ASK_USER_CANCEL_ANSWER_VALUE,
  ASK_USER_CONFIRM_ANSWER_VALUE,
  ASK_USER_INTENTS,
  ASK_USER_MCP_TOOL_NAME,
  ASK_USER_MODES,
  ASK_USER_MULTI_SELECT_MODE,
  ASK_USER_QUESTION_INPUT_SCHEMA,
  ASK_USER_SINGLE_SELECT_MODE,
  ASK_USER_TOOL_NAME,
  ASK_USER_TOOL_NAMES,
  DEFAULT_ASK_USER_MODE,
  DEFAULT_ASK_USER_MODE_WITH_OPTIONS,
  MAX_ASK_USER_HEADER_TEXT,
  MAX_ASK_USER_QUESTION_TEXT,
  MAX_ASK_USER_TOOL_OPTIONS,
  MAX_ASK_USER_TOOL_QUESTIONS,
  MAX_CODEX_REQUEST_USER_INPUT_OPTIONS,
  MAX_REQUEST_USER_INPUT_OPTIONS,
  MIN_ASK_USER_TOOL_OPTIONS,
  PLAN_APPROVAL_INTENT,
  PLAN_APPROVAL_REVISION_REQUEST_ANSWER_VALUE,
};

export type AskUserMode = AskUserContractMode;
export type AskUserIntent = AskUserContractIntent;
export type AskUserToolName = AskUserContractToolName;
export type AskUserCancelAnswerValue = AskUserContractCancelAnswerValue;
export type AskUserConfirmAnswerValue = AskUserContractConfirmAnswerValue;
export type PlanApprovalRevisionRequestAnswerValue = ContractPlanApprovalRevisionRequestAnswerValue;

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function stringOrUndefined(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function booleanOrUndefined(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

function positiveIntegerOrUndefined(value: unknown): number | undefined {
  const numberValue = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(numberValue)) return undefined;
  const normalized = Math.trunc(numberValue);
  return normalized > 0 ? normalized : undefined;
}

export const isAskUserMode = isAskUserModeImpl as (
  value: unknown,
) => value is AskUserMode;

export const isAskUserIntent = isAskUserIntentImpl as (
  value: unknown,
) => value is AskUserIntent;

export const normalizeAskUserMode = normalizeAskUserModeImpl as (
  value: unknown,
  fallback?: AskUserMode,
) => AskUserMode;

export const normalizeAskUserIntent = normalizeAskUserIntentImpl as (
  value: unknown,
) => AskUserIntent | undefined;

export const isLiliaAskUserTool = isLiliaAskUserToolImpl as (
  toolName: unknown,
) => toolName is AskUserToolName;

export interface AskUserOption {
  id?: string;
  label: string;
  description?: string;
  preview?: string;
  icon?: string;
  recommended?: boolean;
  danger?: boolean;
}

export interface AskUserQuestion {
  id: string;
  header?: string;
  question: string;
  mode: AskUserMode;
  options?: AskUserOption[];
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  skippable?: boolean;
  allowOther?: boolean;
  minSelections?: number;
  maxSelections?: number;
}

export interface AskUserSpec {
  title?: string;
  source?: string;
  intent?: AskUserIntent;
  dismissable?: boolean;
  questions: AskUserQuestion[];
}

export interface AskUserAnswer {
  questionId: string;
  value: "yes" | "no" | "other" | string | string[];
  notes?: string;
  skipped?: boolean;
}

export interface AskUserResult {
  answers: Record<string, AskUserAnswer>;
  cancelled: boolean;
}

export interface CreatePlanApprovalAskUserSpecInput {
  title: string;
  source?: string;
  question?: string;
}

export function normalizeAskUserOption(value: unknown): AskUserOption | null {
  if (!isRecord(value)) return null;
  const label = stringOrUndefined(value.label);
  if (!label) return null;
  return {
    id: stringOrUndefined(value.id),
    label,
    description: stringOrUndefined(value.description),
    preview: stringOrUndefined(value.preview),
    icon: stringOrUndefined(value.icon),
    recommended: booleanOrUndefined(value.recommended),
    danger: booleanOrUndefined(value.danger),
  };
}

export function normalizeAskUserQuestion(value: unknown, index = 0): AskUserQuestion | null {
  if (!isRecord(value)) return null;
  const id = stringOrUndefined(value.id) ?? `question-${index + 1}`;
  const question = stringOrUndefined(value.question);
  if (!question && value.mode !== DEFAULT_ASK_USER_MODE) return null;
  const options = Array.isArray(value.options)
    ? value.options.map(normalizeAskUserOption).filter((option): option is AskUserOption => option !== null)
    : undefined;
  const mode = normalizeAskUserMode(
    value.mode,
    options?.length ? DEFAULT_ASK_USER_MODE_WITH_OPTIONS : DEFAULT_ASK_USER_MODE,
  );
  return {
    id,
    header: stringOrUndefined(value.header),
    question: question ?? "",
    mode,
    options,
    confirmLabel: stringOrUndefined(value.confirmLabel),
    cancelLabel: stringOrUndefined(value.cancelLabel),
    danger: booleanOrUndefined(value.danger),
    skippable: booleanOrUndefined(value.skippable),
    allowOther: booleanOrUndefined(value.allowOther),
    minSelections: positiveIntegerOrUndefined(value.minSelections),
    maxSelections: positiveIntegerOrUndefined(value.maxSelections),
  };
}

export function normalizeAskUserSpec(value: unknown): AskUserSpec | null {
  if (!isRecord(value) || !Array.isArray(value.questions)) return null;
  const questions = value.questions
    .map((question, index) => normalizeAskUserQuestion(question, index))
    .filter((question): question is AskUserQuestion => question !== null);
  if (questions.length === 0) return null;
  return {
    title: stringOrUndefined(value.title),
    source: stringOrUndefined(value.source),
    intent: normalizeAskUserIntent(value.intent),
    dismissable: booleanOrUndefined(value.dismissable),
    questions,
  };
}

function stableJsonValue(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(stableJsonValue);
  if (!isRecord(value)) return value;
  const output: Record<string, unknown> = {};
  for (const key of Object.keys(value).sort()) {
    const next = stableJsonValue(value[key]);
    if (next !== undefined) output[key] = next;
  }
  return output;
}

export function askUserSpecKey(spec: AskUserSpec): string {
  return JSON.stringify(stableJsonValue(normalizeAskUserSpec(spec) ?? spec));
}

export function isPlanApprovalAskUserSpec(
  spec: Pick<AskUserSpec, "intent">,
): boolean {
  return isPlanApprovalAskUserSpecImpl(spec);
}

export function isPlanApprovalConfirmAskUserSpec(
  spec: Pick<AskUserSpec, "intent" | "questions">,
): boolean {
  return isPlanApprovalConfirmAskUserSpecImpl(spec);
}

export function createPlanApprovalAskUserSpec(
  input: CreatePlanApprovalAskUserSpecInput,
): AskUserSpec {
  return createPlanApprovalAskUserSpecImpl(input) as AskUserSpec;
}

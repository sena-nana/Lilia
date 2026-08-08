export type AskUserMode = "confirm" | "single" | "multi";
export type AskUserIntent = "plan_approval";
export type AskUserConfirmAnswerValue = "yes";
export type AskUserCancelAnswerValue = "no";
export type PlanApprovalRevisionRequestAnswerValue = "revision_request";
export type AskUserToolName =
  | "AskUserQuestion"
  | "ask_user_question"
  | "mcp__lilia__ask_user_question";

export interface AskUserContractOption {
  id?: string;
  label: string;
  description?: string;
  preview?: string;
  icon?: string;
  recommended?: boolean;
  danger?: boolean;
}

export interface AskUserContractQuestion {
  id: string;
  header?: string;
  question: string;
  mode: AskUserMode;
  options?: ReadonlyArray<AskUserContractOption>;
  confirmLabel?: string;
  cancelLabel?: string;
}

export interface AskUserContractSpec {
  title?: string;
  source?: string;
  intent?: AskUserIntent;
  dismissable?: boolean;
  questions: ReadonlyArray<AskUserContractQuestion>;
}

export interface CreatePlanApprovalAskUserSpecInput {
  title: string;
  source?: string;
  question?: string;
}

export const ASK_USER_CONTRACT: {
  askUserModes: readonly AskUserMode[];
  defaultAskUserMode: AskUserMode;
  defaultAskUserModeWithOptions: AskUserMode;
  singleSelectAskUserMode: "single";
  multiSelectAskUserMode: "multi";
  maxAskUserToolQuestions: number;
  minAskUserToolOptions: number;
  maxAskUserToolOptions: number;
  maxCodexRequestUserInputOptions: number;
  maxAskUserHeaderText: number;
  maxAskUserQuestionText: number;
  askUserToolNames: readonly AskUserToolName[];
  askUserQuestionInputSchema: Record<string, unknown>;
  askUserIntents: readonly AskUserIntent[];
  planApprovalIntent: AskUserIntent;
  confirmAskUserAnswerValue: AskUserConfirmAnswerValue;
  cancelAskUserAnswerValue: AskUserCancelAnswerValue;
  planApprovalRevisionRequestAnswerValue: PlanApprovalRevisionRequestAnswerValue;
};

export const ASK_USER_MODES: readonly AskUserMode[];
export const DEFAULT_ASK_USER_MODE: AskUserMode;
export const DEFAULT_ASK_USER_MODE_WITH_OPTIONS: AskUserMode;
export const ASK_USER_SINGLE_SELECT_MODE: "single";
export const ASK_USER_MULTI_SELECT_MODE: "multi";
export const MAX_ASK_USER_TOOL_QUESTIONS: number;
export const MIN_ASK_USER_TOOL_OPTIONS: number;
export const MAX_ASK_USER_TOOL_OPTIONS: number;
export const MAX_REQUEST_USER_INPUT_OPTIONS: number;
/** @deprecated Use MAX_REQUEST_USER_INPUT_OPTIONS. */
export const MAX_CODEX_REQUEST_USER_INPUT_OPTIONS: number;
export const MAX_ASK_USER_HEADER_TEXT: number;
export const MAX_ASK_USER_QUESTION_TEXT: number;
export const ASK_USER_TOOL_NAMES: readonly AskUserToolName[];
export const ASK_USER_TOOL_NAME: "AskUserQuestion";
export const ASK_USER_CLAUDE_TOOL_NAME: "ask_user_question";
export const ASK_USER_MCP_TOOL_NAME: "mcp__lilia__ask_user_question";
export const ASK_USER_QUESTION_INPUT_SCHEMA: Record<string, unknown>;
export const ASK_USER_INTENTS: readonly AskUserIntent[];
export const PLAN_APPROVAL_INTENT: AskUserIntent;
export const ASK_USER_CONFIRM_ANSWER_VALUE: AskUserConfirmAnswerValue;
export const ASK_USER_CANCEL_ANSWER_VALUE: AskUserCancelAnswerValue;
export const PLAN_APPROVAL_REVISION_REQUEST_ANSWER_VALUE: PlanApprovalRevisionRequestAnswerValue;

export function isAskUserMode(value: unknown): value is AskUserMode;

export function normalizeAskUserMode(value: unknown, fallback?: AskUserMode): AskUserMode;

export function isAskUserIntent(value: unknown): value is AskUserIntent;

export function normalizeAskUserIntent(value: unknown): AskUserIntent | undefined;

export function isPlanApprovalAskUserSpec(spec: Pick<AskUserContractSpec, "intent">): boolean;

export function isPlanApprovalConfirmAskUserSpec(
  spec: Pick<AskUserContractSpec, "intent" | "questions">,
): boolean;

export function createPlanApprovalAskUserSpec(
  input: CreatePlanApprovalAskUserSpecInput,
): AskUserContractSpec;

export function isLiliaAskUserTool(toolName: unknown): toolName is AskUserToolName;

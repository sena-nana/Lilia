import {
  ASK_USER_MULTI_SELECT_MODE,
  ASK_USER_QUESTION_INPUT_SCHEMA,
  ASK_USER_SINGLE_SELECT_MODE,
  ASK_USER_TOOL_NAME,
  ASK_USER_TOOL_NAMES as LILIA_ASK_USER_TOOL_NAMES,
  MAX_ASK_USER_HEADER_TEXT,
  MAX_ASK_USER_QUESTION_TEXT,
  MAX_ASK_USER_TOOL_OPTIONS,
  MAX_ASK_USER_TOOL_QUESTIONS,
  MAX_CODEX_REQUEST_USER_INPUT_OPTIONS,
  MIN_ASK_USER_TOOL_OPTIONS,
  isLiliaAskUserTool,
} from "@lilia/contracts/askUserContract.mjs";
import { z } from "zod/v4";
import { isRecord, shortText, stringOrNull } from "./utils.mjs";

export { LILIA_ASK_USER_TOOL_NAMES, isLiliaAskUserTool };

export function normalizeAskUserResult(value) {
  if (!isRecord(value)) return { answers: {}, cancelled: true };
  return {
    answers: isRecord(value.answers) ? value.answers : {},
    cancelled: value.cancelled === true,
  };
}

export function stableOptionId(index) {
  return `o-${index + 1}`;
}

export function normalizeClaudeAskUserQuestions(input) {
  const rawQuestions = Array.isArray(input?.questions)
    ? input.questions.slice(0, MAX_ASK_USER_TOOL_QUESTIONS)
    : [];
  return rawQuestions
    .map((question, questionIndex) => {
      if (!isRecord(question)) return null;
      const options = Array.isArray(question.options)
        ? question.options.slice(0, MAX_ASK_USER_TOOL_OPTIONS)
        : [];
      const normalizedOptions = options
        .map((option, optionIndex) => {
          if (!isRecord(option)) return null;
          const label = stringOrNull(option.label) || `Option ${optionIndex + 1}`;
          const normalized = {
            id: stableOptionId(optionIndex),
            label,
          };
          const description = stringOrNull(option.description);
          const preview = stringOrNull(option.preview);
          if (description) normalized.description = description;
          if (preview) normalized.preview = preview;
          if (optionIndex === 0) normalized.recommended = true;
          return normalized;
        })
        .filter(Boolean);
      if (normalizedOptions.length < MIN_ASK_USER_TOOL_OPTIONS) return null;
      return {
        id: `q-${questionIndex + 1}`,
        header: shortText(question.header, MAX_ASK_USER_HEADER_TEXT) || `问题 ${questionIndex + 1}`,
        question: shortText(question.question, MAX_ASK_USER_QUESTION_TEXT) || "请选择一个选项。",
        mode: question.multiSelect === true ? ASK_USER_MULTI_SELECT_MODE : ASK_USER_SINGLE_SELECT_MODE,
        options: normalizedOptions,
        allowOther: true,
        skippable: false,
      };
    })
    .filter(Boolean);
}

export function claudeAskUserInputToSpec(input) {
  return {
    title: "Claude 想确认一下",
    source: "Claude",
    dismissable: true,
    questions: normalizeClaudeAskUserQuestions(input),
  };
}

export function answerValueToLabels(answer, question, originalQuestion) {
  if (!answer) return null;
  if (answer.skipped) return null;
  const sourceOptions = Array.isArray(originalQuestion?.options) ? originalQuestion.options : [];
  const labels = new Map(
    question.options.map((option, index) => [
      option.id,
      stringOrNull(sourceOptions[index]?.label) || option.label,
    ]),
  );
  const one = (value) => {
    if (value === "other") return stringOrNull(answer.notes) || "Other";
    return labels.get(value) || stringOrNull(value);
  };
  if (Array.isArray(answer.value)) {
    return answer.value.map(one).filter(Boolean).join(", ");
  }
  return one(answer.value);
}

export function askUserResultToClaudeOutput(input, spec, result) {
  const originalQuestions = Array.isArray(input?.questions) ? input.questions : [];
  const answers = {};
  const annotations = {};
  for (let index = 0; index < spec.questions.length; index += 1) {
    const question = spec.questions[index];
    const originalQuestion = originalQuestions[index];
    const questionText =
      stringOrNull(originalQuestion?.question) ||
      question.question ||
      `Question ${index + 1}`;
    const answer = result.answers?.[question.id];
    const labelText = answerValueToLabels(answer, question, originalQuestion);
    if (labelText) answers[questionText] = labelText;
    if (answer?.notes) annotations[questionText] = { notes: answer.notes };
  }
  return {
    questions: originalQuestions.slice(0, spec.questions.length),
    answers,
    annotations,
    cancelled: result.cancelled === true,
  };
}

export function createClaudeAskUserHandler(requestAskUser) {
  return async function handleClaudeAskUserQuestion(input) {
    const spec = claudeAskUserInputToSpec(input);
    if (spec.questions.length === 0) {
      return {
        content: [{ type: "text", text: "No valid questions were provided." }],
        isError: true,
      };
    }
    const result = await requestAskUser(spec);
    const output = askUserResultToClaudeOutput(input, spec, result);
    return {
      content: [{ type: "text", text: JSON.stringify(output) }],
      structuredContent: output,
      isError: output.cancelled,
    };
  };
}

export const askUserQuestionInputSchema = {
  questions: z
    .array(
      z.object({
        question: z.string(),
        header: z.string(),
        options: z
          .array(
            z.object({
              label: z.string(),
              description: z.string().optional().default(""),
              preview: z.string().optional(),
            }),
          )
          .min(MIN_ASK_USER_TOOL_OPTIONS)
          .max(MAX_ASK_USER_TOOL_OPTIONS),
        multiSelect: z.boolean().optional().default(false),
      }),
    )
    .min(1)
    .max(MAX_ASK_USER_TOOL_QUESTIONS),
};

export const askUserQuestionJsonSchema = ASK_USER_QUESTION_INPUT_SCHEMA;

export const codexAskUserDynamicTool = {
  name: ASK_USER_TOOL_NAME,
  description: "Ask the human user one or more multiple-choice questions through Lilia.",
  inputSchema: askUserQuestionJsonSchema,
};

export function askUserSpecToCodexOutput(input, spec, result) {
  return askUserResultToClaudeOutput(input, spec, result);
}

export function codexAskUserInputToSpec(input) {
  return {
    title: "Codex 想确认一下",
    source: "Codex",
    dismissable: true,
    questions: normalizeClaudeAskUserQuestions(input),
  };
}

export function createCodexAskUserHandler(requestAskUser) {
  return async function handleCodexAskUserQuestion(input) {
    const spec = codexAskUserInputToSpec(input);
    if (spec.questions.length === 0) {
      return {
        questions: Array.isArray(input?.questions) ? input.questions : [],
        answers: {},
        annotations: {},
        cancelled: true,
        error: "No valid questions were provided.",
      };
    }
    const result = await requestAskUser(spec, { backend: "codex" });
    return askUserSpecToCodexOutput(input, spec, result);
  };
}

export function codexRequestUserInputQuestionsToSpec(params) {
  const rawQuestions = Array.isArray(params?.questions)
    ? params.questions.slice(0, MAX_ASK_USER_TOOL_QUESTIONS)
    : [];
  const questions = rawQuestions
    .map((question, questionIndex) => {
      if (!isRecord(question)) return null;
      const options = Array.isArray(question.options)
        ? question.options.slice(0, MAX_CODEX_REQUEST_USER_INPUT_OPTIONS)
        : [];
      const normalizedOptions = options
        .map((option, optionIndex) => {
          if (!isRecord(option)) return null;
          const label = stringOrNull(option.label) || `Option ${optionIndex + 1}`;
          const normalized = {
            id: label,
            label,
          };
          const description = stringOrNull(option.description);
          if (description) normalized.description = description;
          if (optionIndex === 0) normalized.recommended = true;
          return normalized;
        })
        .filter(Boolean);
      const allowOther = question.isOther === true;
      const hasOptions = normalizedOptions.length >= MIN_ASK_USER_TOOL_OPTIONS;
      if (!hasOptions && !allowOther) return null;
      return {
        id: stringOrNull(question.id) || `q-${questionIndex + 1}`,
        header: shortText(question.header, MAX_ASK_USER_HEADER_TEXT) || `问题 ${questionIndex + 1}`,
        question: shortText(question.question, MAX_ASK_USER_QUESTION_TEXT) || "请补充信息。",
        mode: ASK_USER_SINGLE_SELECT_MODE,
        options: hasOptions ? normalizedOptions : [{ id: "other", label: "其他" }, { id: "skip", label: "跳过" }],
        allowOther,
        skippable: false,
      };
    })
    .filter(Boolean);
  return {
    title: "Codex 想确认一下",
    source: "Codex",
    dismissable: true,
    questions,
  };
}

export function askUserResultToCodexRequestUserInputResponse(result, spec) {
  const answers = {};
  for (const question of spec.questions) {
    const answer = result.answers?.[question.id];
    if (!answer || answer.skipped) continue;
    if (Array.isArray(answer.value)) {
      answers[question.id] = {
        answers: answer.value.map((value) => value === "other" ? answer.notes || "Other" : String(value)),
      };
      continue;
    }
    const value = answer.value === "other" ? answer.notes || "Other" : String(answer.value);
    if (value) answers[question.id] = { answers: [value] };
  }
  return { answers };
}

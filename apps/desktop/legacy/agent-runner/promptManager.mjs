import {
  PROMPT_CLAUDE,
  PROMPT_CODEX,
  PROMPT_MAIN_AGENT,
  PROMPT_RUNNER,
} from "@lilia/contracts/promptContract.mjs";
import { isRecord, stringOrNull } from "./utils.mjs";

export function composePromptParts(parts, separator = "\n") {
  return (Array.isArray(parts) ? parts : [])
    .map((part) => (typeof part === "string" ? part.trim() : ""))
    .filter(Boolean)
    .join(separator);
}

export function normalizePromptAttachments(value) {
  if (!Array.isArray(value)) return [];
  return value
    .map((item) => {
      if (!isRecord(item)) return null;
      const path = stringOrNull(item.path)?.trim();
      if (!path) return null;
      return {
        path,
        name: stringOrNull(item.name) || path,
        kind: stringOrNull(item.kind) || "unknown",
        mime: stringOrNull(item.mime),
        size: typeof item.size === "number" && Number.isFinite(item.size) ? item.size : null,
        directory: isRecord(item.directory) ? item.directory : null,
      };
    })
    .filter(Boolean);
}

export function attachmentSizeLabel(size) {
  if (typeof size !== "number" || !Number.isFinite(size) || size < 0) return "unknown size";
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${Math.round(size / 1024)} KB`;
  return `${Math.round(size / (1024 * 1024))} MB`;
}

export function attachmentDirectoryLabel(directory) {
  if (!directory) return "";
  const parts = [];
  if (typeof directory.fileCount === "number") parts.push(`${directory.fileCount} files`);
  if (typeof directory.directoryCount === "number") parts.push(`${directory.directoryCount} dirs`);
  if (directory.truncated === true) parts.push("truncated");
  return parts.join(", ");
}

export function buildAttachmentContext(attachments) {
  const normalized = normalizePromptAttachments(attachments);
  if (normalized.length === 0) return "";
  return [
    PROMPT_RUNNER.attachmentIntro,
    ...normalized.map((attachment, index) => {
      const meta = [
        attachment.kind,
        attachment.mime,
        attachmentSizeLabel(attachment.size),
        attachmentDirectoryLabel(attachment.directory),
      ].filter(Boolean).join(", ");
      return `${index + 1}. ${attachment.name}: ${attachment.path}${meta ? ` (${meta})` : ""}`;
    }),
  ].join("\n");
}

export function buildPromptWithAttachments(prompt, attachments) {
  const attachmentContext = buildAttachmentContext(attachments);
  const content = typeof prompt === "string" ? prompt.trim() : "";
  if (!attachmentContext) return typeof prompt === "string" ? prompt : "";
  return content
    ? `${attachmentContext}\n\n${PROMPT_RUNNER.attachmentUserMessageLabel}\n${content}`
    : attachmentContext;
}

export function buildClaudePlatformAppend(platform = process.platform) {
  if (platform !== "win32") return "";
  return PROMPT_CLAUDE.windowsPlatformAppend.join("\n");
}

export function buildClaudeSystemPrompt(platform = process.platform, additionalContext = null) {
  const append = composePromptParts([
    buildClaudePlatformAppend(platform),
    additionalContext,
  ], "\n\n");
  const base = { type: "preset", preset: PROMPT_CLAUDE.systemPromptPreset };
  return append ? { ...base, append } : base;
}

export function buildClaudeWorkflowPrompt({
  review = null,
  taskWorkflow = null,
  fix = null,
  batch = null,
  providerSettings = null,
  prompt = "",
  reviewTargetText,
} = {}) {
  const userPrompt = stringOrNull(prompt)?.trim();
  const targetText = typeof reviewTargetText === "function" ? reviewTargetText : () => "";
  if (taskWorkflow) {
    return buildTaskWorkflowPrompt(taskWorkflow, { prompt: userPrompt, backend: "Claude" });
  }
  if (review) {
    return composePromptParts([
      PROMPT_CLAUDE.workflows.review.title,
      `Review target: ${targetText(review.target)}.`,
      PROMPT_CLAUDE.workflows.review.focus,
      review.instructions ? `User instructions: ${review.instructions}` : null,
      userPrompt ? `Additional user message: ${userPrompt}` : null,
    ]);
  }
  if (fix) {
    return composePromptParts([
      PROMPT_CLAUDE.workflows.fixSuggestion.title,
      `Target: ${targetText(fix.target)}.`,
      fix.mode === "apply"
        ? PROMPT_CLAUDE.workflows.fixSuggestion.apply
        : PROMPT_CLAUDE.workflows.fixSuggestion.suggest,
      fix.instructions ? `User instructions: ${fix.instructions}` : null,
      userPrompt ? `Additional user message: ${userPrompt}` : null,
    ]);
  }
  if (batch) {
    return [
      PROMPT_CLAUDE.workflows.batchApply.title,
      `Source kind: ${batch.sourceKind}.`,
      `Source turn: ${batch.sourceTurnId}.`,
      PROMPT_CLAUDE.workflows.batchApply.goal,
      "",
      batch.sourceSummary,
      batch.instructions ? `\nUser instructions: ${batch.instructions}` : null,
      userPrompt ? `\nAdditional user message: ${userPrompt}` : null,
    ].filter(Boolean).join("\n");
  }
  if (providerSettings) {
    const optionKeys = Object.keys(providerSettings.options);
    const supportedKeys = Object.keys(providerSettings.supported);
    return composePromptParts([
      PROMPT_CLAUDE.workflows.runtimeSettings.title,
      `Action: ${providerSettings.action}.`,
      PROMPT_CLAUDE.workflows.runtimeSettings.goal,
      optionKeys.length > 0 ? `Claude option keys: ${optionKeys.join(", ")}.` : null,
      supportedKeys.length > 0 ? `Common setting keys: ${supportedKeys.join(", ")}.` : null,
      userPrompt ? `Additional user message: ${userPrompt}` : null,
    ]);
  }
  return null;
}

export function buildCodexPlanPrompt(kind, value) {
  if (kind === "revision") {
    return composePromptParts([
      PROMPT_CODEX.planRevision.header,
      `修改要求：${value}`,
      PROMPT_CODEX.planRevision.footer,
    ]);
  }
  if (kind === "execution") {
    return [
      PROMPT_CODEX.planExecution.header,
      PROMPT_CODEX.planExecution.instruction,
      "",
      PROMPT_CODEX.planExecution.label,
      value,
    ].join("\n");
  }
  return "";
}

export function buildCodexWorkflowPrompt(kind, input = {}) {
  if (kind === "taskWorkflow") {
    const { workflow, cmd } = input;
    return buildTaskWorkflowPrompt(workflow, {
      prompt: stringOrNull(cmd?.prompt)?.trim() || "",
      backend: "Codex",
      cwd: stringOrNull(cmd?.cwd) || "",
    });
  }
  if (kind === "fixSuggestion") {
    const { workflow, cmd, targetSummary, targetDescription } = input;
    const prompt = stringOrNull(cmd?.prompt)?.trim() || "";
    const instructions = workflow.instructions || "";
    const extraContext = prompt && prompt !== instructions ? prompt : "";
    const applying = workflow.mode === "apply";
    return [
      PROMPT_CODEX.workflows.fixSuggestion.title,
      "",
      `Target: ${targetSummary}`,
      `Target details: ${targetDescription}`,
      `Workspace cwd: ${stringOrNull(cmd?.cwd) || ""}`,
      `Mode: ${workflow.mode}`,
      "",
      applying
        ? PROMPT_CODEX.workflows.fixSuggestion.applyGoal
        : PROMPT_CODEX.workflows.fixSuggestion.suggestGoal,
      applying
        ? PROMPT_CODEX.workflows.fixSuggestion.applyPermission
        : PROMPT_CODEX.workflows.fixSuggestion.suggestPermission,
      PROMPT_CODEX.workflows.fixSuggestion.approval,
      "",
      "User instructions:",
      instructions || "(none)",
      "",
      "Additional composer context:",
      extraContext || "(none)",
    ].join("\n");
  }
  if (kind === "batchApply") {
    const { workflow, cmd } = input;
    const prompt = stringOrNull(cmd?.prompt)?.trim() || "";
    const instructions = workflow.instructions || "";
    const extraContext = prompt && prompt !== instructions ? prompt : "";
    return [
      PROMPT_CODEX.workflows.batchApply.title,
      "",
      `Source kind: ${workflow.sourceKind}`,
      `Source turn id: ${workflow.sourceTurnId}`,
      `Workspace cwd: ${stringOrNull(cmd?.cwd) || ""}`,
      "",
      PROMPT_CODEX.workflows.batchApply.goal,
      PROMPT_CODEX.workflows.batchApply.plan,
      PROMPT_CODEX.workflows.batchApply.apply,
      "",
      "Source suggestions:",
      workflow.sourceSummary,
      "",
      "User instructions:",
      instructions || "(none)",
      "",
      "Additional composer context:",
      extraContext || "(none)",
    ].join("\n");
  }
  return "";
}

export function mainAgentWorkflowDefinition(kind) {
  const key = stringOrNull(kind)?.trim() || "";
  if (!key) return null;
  return PROMPT_MAIN_AGENT.workflowTypes?.[key] ?? null;
}

export function buildTaskWorkflowPrompt(workflow, input = {}) {
  const definition = mainAgentWorkflowDefinition(workflow?.kind);
  if (!definition) throw new Error(`Unknown Lilia task workflow: ${workflow?.kind ?? ""}`);
  const prompt = stringOrNull(input.prompt)?.trim() || "";
  const instructions = stringOrNull(workflow?.instructions)?.trim() || "";
  return composePromptParts([
    `Lilia task workflow: ${workflow.kind}`,
    `Workflow title: ${definition.title.trim()}`,
    definition.summary ? `Summary: ${definition.summary.trim()}` : null,
    input.backend ? `Provider: ${input.backend}` : null,
    input.cwd ? `Workspace cwd: ${input.cwd}` : null,
    "Workflow instructions:",
    definition.prompt,
    instructions ? `User workflow instructions: ${instructions}` : null,
    prompt ? `Additional user message: ${prompt}` : null,
  ], "\n\n");
}

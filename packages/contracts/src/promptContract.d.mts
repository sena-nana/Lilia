export interface PromptRunnerContract {
  attachmentIntro: string;
  attachmentUserMessageLabel: string;
}

export interface PromptMainAgentWorkflow {
  title: string;
  summary: string;
  prompt: string;
}

export interface PromptMainAgentModes {
  conservative: string;
  aggressive: string;
}

export interface PromptMainAgentContract {
  basePrompt: string;
  toolsPrompt: string;
  workflowTypes: Record<string, PromptMainAgentWorkflow>;
  modes: PromptMainAgentModes;
  workflowOrder: readonly string[];
}

export interface PromptNativeContract {
  windowsPlatformAppend: readonly string[];
  compactPrompt: string;
  subagents: {
    delegationHeader: string;
    baselineIntro: string;
    baselinePrompt: string;
    baselineLabel: string;
    availableLabel: string;
    agentSpecificInstructionLabel: string;
    agentRoleLabel: string;
    delegationRulesLabel: string;
    delegationRules: readonly string[];
  };
  planRevision: Record<string, string>;
  planExecution: Record<string, string>;
  workflows: Record<string, Record<string, string>>;
}

/** @deprecated Use PromptNativeContract. */
export type PromptClaudeContract = PromptNativeContract;
/** @deprecated Use PromptNativeContract. */
export type PromptCodexContract = PromptNativeContract;

export interface PromptAssistantContract {
  promptRouter: {
    systemInstruction: string;
    requestInstruction: string;
    requirements: readonly string[];
    scenarios: readonly string[];
  };
  promptOptimize: {
    systemInstruction: string;
    requestInstruction: string;
    requirements: readonly string[];
  };
  autoTurnDecision: {
    systemInstruction: string;
    requestInstruction: string;
    tierPolicy: Record<"light" | "normal" | "deep", string>;
  };
  contextCompaction: {
    systemInstruction: string;
    requestInstruction: string;
    successMessage: string;
  };
}

export interface PromptSuggestionContract {
  systemInstruction: string;
  generationRules: readonly string[];
}

export interface PromptTitleContract {
  systemInstruction: string;
}

export interface PromptAutomationContract {
  defaultAgentPrompt: string;
  defaultHumanPrompt: string;
}

export interface PromptContract {
  runner: PromptRunnerContract;
  mainAgent: PromptMainAgentContract;
  native: PromptNativeContract;
  assistant: PromptAssistantContract;
  suggestion: PromptSuggestionContract;
  title: PromptTitleContract;
  automation: PromptAutomationContract;
}

export const PROMPT_CONTRACT: Readonly<PromptContract>;
export const PROMPT_RUNNER: Readonly<PromptRunnerContract>;
export const PROMPT_MAIN_AGENT: Readonly<PromptMainAgentContract>;
export const PROMPT_NATIVE: Readonly<PromptNativeContract>;
export const PROMPT_ASSISTANT: Readonly<PromptAssistantContract>;
export const PROMPT_SUGGESTION: Readonly<PromptSuggestionContract>;
export const PROMPT_TITLE: Readonly<PromptTitleContract>;
export const PROMPT_AUTOMATION: Readonly<PromptAutomationContract>;
export const DEFAULT_AUTOMATION_AGENT_PROMPT: string;
export const DEFAULT_AUTOMATION_HUMAN_PROMPT: string;

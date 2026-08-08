import promptContractManifest from "./prompt-contract.json" with { type: "json" };
import promptText from "./prompt-text.json" with { type: "json" };

if (promptContractManifest.textBundle !== "prompt-text.json") {
  throw new Error("Unsupported prompt contract text bundle.");
}

const manifest = Object.freeze(promptText);

export const PROMPT_CONTRACT = manifest;
export const PROMPT_RUNNER = manifest.runner;
export const PROMPT_MAIN_AGENT = manifest.mainAgent;
/** Native / Mutsuki AgentKit prompt pack (Lilia product protocol). */
export const PROMPT_NATIVE = manifest.native;
export const PROMPT_ASSISTANT = manifest.assistant;
export const PROMPT_SUGGESTION = manifest.suggestion;
export const PROMPT_TITLE = manifest.title;
export const PROMPT_AUTOMATION = manifest.automation;
export const DEFAULT_AUTOMATION_AGENT_PROMPT = manifest.automation.defaultAgentPrompt;
export const DEFAULT_AUTOMATION_HUMAN_PROMPT = manifest.automation.defaultHumanPrompt;

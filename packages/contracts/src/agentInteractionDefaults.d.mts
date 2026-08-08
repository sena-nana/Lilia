import type { RuntimePermissionMode } from "./permissionModes.mjs";

export type PermissionModeAvailability = Record<RuntimePermissionMode, boolean>;

export interface AgentSubagentModeSettings {
  enabled: boolean;
  forwardSubagentText: boolean;
  agentProgressSummaries: boolean;
}

/** @deprecated Use AgentSubagentModeSettings; brand backends removed. */
export type AgentSubagentBackendSettings = Pick<AgentSubagentModeSettings, "enabled">;

/** @deprecated Use AgentSubagentModeSettings; brand backends removed. */
export type ClaudeSubagentModeSettings = AgentSubagentModeSettings;

export interface AutoTurnDecisionSettings {
  enabled: boolean;
  allowModelTier: boolean;
  allowReasoningEffort: boolean;
  allowPlanMode: boolean;
  allowGoalMode: boolean;
  allowSessionFork: boolean;
}

export type AutoTurnDecisionPermissionKey = Exclude<
  keyof AutoTurnDecisionSettings,
  "enabled"
>;

export interface AutoTurnDecisionPermissionOption {
  key: AutoTurnDecisionPermissionKey;
  label: string;
  ariaLabel: string;
}

export type MainAgentPromptMode = "conservative" | "aggressive" | "custom";

export interface AgentInteractionSettings {
  nonInterruptMode: boolean;
  debug: boolean;
  permissionMode: RuntimePermissionMode;
  permissionModeAvailability: PermissionModeAvailability;
  mainAgentPromptMode: MainAgentPromptMode;
  mainAgentCustomPrompt: string;
  subagentMode: AgentSubagentModeSettings;
  autoTurnDecision: AutoTurnDecisionSettings;
}

export const AUTO_TURN_DECISION_PERMISSION_OPTIONS: readonly AutoTurnDecisionPermissionOption[];
export const AUTO_TURN_DECISION_PERMISSION_KEYS: readonly AutoTurnDecisionPermissionKey[];
export const LOCKED_PERMISSION_MODES: readonly RuntimePermissionMode[];
export const DEFAULT_PERMISSION_MODE_AVAILABILITY: Readonly<PermissionModeAvailability>;
export const DEFAULT_AGENT_SUBAGENT_MODE_SETTINGS: Readonly<AgentSubagentModeSettings>;
export const DEFAULT_AUTO_TURN_DECISION_SETTINGS: Readonly<AutoTurnDecisionSettings>;
export const DEFAULT_AGENT_INTERACTION_SETTINGS: Readonly<AgentInteractionSettings>;

export function normalizeMainAgentPromptMode(
  value: unknown,
  fallback?: MainAgentPromptMode,
): MainAgentPromptMode;

export function normalizeMainAgentCustomPrompt(
  value: unknown,
  fallback?: string,
): string;

export function normalizeAgentSubagentModeSettings(
  input: Partial<AgentSubagentModeSettings> | null | undefined,
  base?: AgentSubagentModeSettings,
): AgentSubagentModeSettings;

export function normalizeAutoTurnDecisionSettings(
  input: Partial<AutoTurnDecisionSettings> | null | undefined,
  base?: AutoTurnDecisionSettings,
): AutoTurnDecisionSettings;

export function normalizePermissionModeAvailability(
  input: Partial<PermissionModeAvailability> | null | undefined,
  base?: PermissionModeAvailability,
): PermissionModeAvailability;

export function enabledPermissionModes(
  availability?: Partial<PermissionModeAvailability> | null,
): RuntimePermissionMode[];

export function normalizeAvailablePermissionMode(
  value: unknown,
  availability?: Partial<PermissionModeAvailability> | null,
  fallback?: RuntimePermissionMode,
): RuntimePermissionMode;

export function normalizeAgentInteractionSettings(
  input: Partial<AgentInteractionSettings> | null | undefined,
  base?: AgentInteractionSettings,
): AgentInteractionSettings;

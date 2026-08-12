import { computed, readonly, ref } from "vue";
import {
  normalizeAgentInteractionSettings as normalizeAgentInteractionSettingsContract,
  type AgentInteractionSettings,
  type CustomSubagentDefinition,
  type CustomSubagentUpsertInput,
} from "@lilia/contracts";
import {
  deleteCustomSubagent,
  getAgentInteractionSettings,
  listCustomSubagents,
  setAgentInteractionSettings,
  upsertCustomSubagent,
} from "../services/chat";

export {
  normalizeAgentInteractionSettings,
  normalizePermissionMode,
} from "@lilia/contracts";

const settings = ref<AgentInteractionSettings>(normalizeAgentInteractionSettingsContract(null));
const subagents = ref<CustomSubagentDefinition[]>([]);

let loadPromise: Promise<AgentInteractionSettings> | null = null;

function sameJsonValue(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export function sameSubagentMode(a: unknown, b: unknown): boolean {
  return sameJsonValue(a, b);
}

export async function loadAgentInteractionSettings(): Promise<AgentInteractionSettings> {
  if (!loadPromise) {
    loadPromise = getAgentInteractionSettings()
      .then((next) => {
        settings.value = normalizeAgentInteractionSettingsContract(next);
        return settings.value;
      })
      .finally(() => {
        loadPromise = null;
      });
  }
  return loadPromise;
}

export async function loadCustomSubagentDefinitions(): Promise<CustomSubagentDefinition[]> {
  const next = await listCustomSubagents();
  subagents.value = [...next].sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));
  return subagents.value;
}

export async function saveCustomSubagentDefinition(
  input: CustomSubagentUpsertInput,
): Promise<CustomSubagentDefinition> {
  const saved = await upsertCustomSubagent(input);
  const next = [...subagents.value];
  const index = next.findIndex((item) => item.id === saved.id);
  if (index === -1) next.push(saved);
  else next[index] = saved;
  subagents.value = next.sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));
  return saved;
}

export async function removeCustomSubagentDefinition(id: string): Promise<void> {
  await deleteCustomSubagent(id);
  subagents.value = subagents.value.filter((item) => item.id !== id);
}

export async function updateAgentInteractionSettings(
  patch: Partial<AgentInteractionSettings>,
): Promise<AgentInteractionSettings> {
  const previous = settings.value;
  const next = normalizeAgentInteractionSettingsContract({ ...previous, ...patch });
  if (
    next.nonInterruptMode === previous.nonInterruptMode &&
    next.debug === previous.debug &&
    next.permissionMode === previous.permissionMode &&
    sameJsonValue(next.permissionModeAvailability, previous.permissionModeAvailability) &&
    next.mainAgentPromptMode === previous.mainAgentPromptMode &&
    next.mainAgentCustomPrompt === previous.mainAgentCustomPrompt &&
    sameSubagentMode(next.subagentMode, previous.subagentMode) &&
    sameJsonValue(next.autoTurnDecision, previous.autoTurnDecision)
  ) {
    return previous;
  }
  settings.value = next;
  try {
    await setAgentInteractionSettings(next);
    return next;
  } catch (err) {
    settings.value = previous;
    throw err;
  }
}

export function useAgentInteractionSettings() {
  return {
    settings: readonly(settings),
    subagents: readonly(subagents),
    nonInterruptMode: computed(() => settings.value.nonInterruptMode),
    debug: computed(() => settings.value.debug),
    permissionMode: computed(() => settings.value.permissionMode),
    permissionModeAvailability: computed(() => settings.value.permissionModeAvailability),
    mainAgentPromptMode: computed(() => settings.value.mainAgentPromptMode),
    mainAgentCustomPrompt: computed(() => settings.value.mainAgentCustomPrompt),
    load: loadAgentInteractionSettings,
    loadSubagents: loadCustomSubagentDefinitions,
    saveSubagent: saveCustomSubagentDefinition,
    deleteSubagent: removeCustomSubagentDefinition,
    update: updateAgentInteractionSettings,
  };
}


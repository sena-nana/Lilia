import { describe, expect, it } from "vitest";
import type {
  ChatAttachment,
  ChatComposerState,
  ChatModelOption,
  ModelFeatureSettings,
  ProviderRuntimeOptions,
} from "@lilia/contracts";
import {
  LILIA_COMPACT_WORKFLOW_TYPE,
  LILIA_REVIEW_WORKFLOW_TYPE,
  normalizeModelFeatureSettings,
} from "@lilia/contracts";
import { previewAutoModelSelection, selectModelForTurn } from "../src/services/modelSelection";

const nativeModels: ChatModelOption[] = [
  { id: "gpt-5.5", label: "GPT-5.5", backend: "native-agentkit" },
  { id: "gpt-5.4", label: "GPT-5.4", backend: "native-agentkit" },
  { id: "gpt-5.4-mini", label: "GPT-5.4 Mini", backend: "native-agentkit" },
  { id: "claude-opus-4-7", label: "Opus 4.7", backend: "native-agentkit" },
  { id: "claude-sonnet-4-6", label: "Sonnet 4.6", backend: "native-agentkit" },
  { id: "claude-haiku-4-5", label: "Haiku 4.5", backend: "native-agentkit" },
];

function composer(overrides: Partial<ChatComposerState> = {}): ChatComposerState {
  return {
    taskId: "t-1",
    backend: "native-agentkit",
    model: "gpt-5.5",
    modelSelectionMode: "auto",
    reasoningEffort: null,
    planMode: false,
    goalMode: false,
    permission: "ask",
    ...overrides,
  };
}

function attachment(id: string, partial: Partial<ChatAttachment> = {}): ChatAttachment {
  return {
    id,
    kind: "file",
    path: `C:\\repo\\${id}.md`,
    name: `${id}.md`,
    exists: true,
    size: 20,
    ...partial,
  };
}

function featureSettings(
  partial: Partial<ModelFeatureSettings> = {},
): ModelFeatureSettings {
  return normalizeModelFeatureSettings({
    chat: { light: null, normal: null, deep: null },
    title: null,
    suggestion: null,
    promptRouter: null,
    promptOptimize: null,
    autoTurnDecision: null,
    ...partial,
  });
}

describe("model selection (preset router)", () => {
  it("selects fast/default/plan presets by context scale", () => {
    expect(selectModelForTurn({
      backend: "native-agentkit",
      modelOptions: nativeModels,
      composer: composer(),
      prompt: "short",
    }).explanation).toMatchObject({
      model: "gpt-5.4-mini",
      reasoningEffort: "low",
      source: "auto",
      presetId: "fast",
    });

    expect(selectModelForTurn({
      backend: "native-agentkit",
      modelOptions: nativeModels,
      composer: composer(),
      prompt: "x".repeat(2100),
    }).explanation).toMatchObject({
      model: "gpt-5.4",
      reasoningEffort: "medium",
      presetId: "default",
    });

    expect(selectModelForTurn({
      backend: "native-agentkit",
      modelOptions: nativeModels,
      composer: composer(),
      prompt: "x".repeat(8100),
    }).explanation).toMatchObject({
      model: "gpt-5.5",
      reasoningEffort: "high",
      presetId: "plan",
    });
  });

  it("uses plan/review/fast presets for plan mode and workflows", () => {
    expect(selectModelForTurn({
      backend: "native-agentkit",
      modelOptions: nativeModels,
      composer: composer({ planMode: true }),
      prompt: "plan",
    }).explanation).toMatchObject({
      model: "gpt-5.5",
      reasoningEffort: "high",
      presetId: "plan",
    });

    expect(selectModelForTurn({
      backend: "native-agentkit",
      modelOptions: nativeModels,
      composer: composer(),
      prompt: "",
      workflow: { type: LILIA_REVIEW_WORKFLOW_TYPE, target: { type: "uncommittedChanges" } },
    }).explanation).toMatchObject({
      model: "gpt-5.5",
      presetId: "review",
    });

    expect(selectModelForTurn({
      backend: "native-agentkit",
      modelOptions: nativeModels,
      composer: composer(),
      prompt: "",
      workflow: { type: LILIA_COMPACT_WORKFLOW_TYPE },
    }).explanation).toMatchObject({
      model: "gpt-5.4-mini",
      reasoningEffort: "low",
      presetId: "fast",
    });
  });

  it("lets manual composer selection override auto and runtimeOptions override manual", () => {
    const manual = selectModelForTurn({
      backend: "native-agentkit",
      modelOptions: nativeModels,
      composer: composer({
        model: "claude-opus-4-7",
        modelSelectionMode: "manual",
        reasoningEffort: "max",
      }),
      prompt: "short",
    });
    expect(manual.explanation).toMatchObject({
      model: "claude-opus-4-7",
      reasoningEffort: "max",
      source: "manual",
    });
    expect(manual.runtimeOptions.provider?.["native-agentkit"]).toMatchObject({
      reasoningEffort: "max",
      thinking: { type: "adaptive" },
    });

    const runtimeOptions: ProviderRuntimeOptions = {
      common: { model: "claude-sonnet-4-6", reasoningEffort: "medium" },
    };
    const runtime = selectModelForTurn({
      backend: "native-agentkit",
      modelOptions: nativeModels,
      composer: composer({
        model: "claude-opus-4-7",
        modelSelectionMode: "manual",
        reasoningEffort: "max",
      }),
      prompt: "short",
      runtimeOptions,
    });
    expect(runtime.explanation).toMatchObject({
      model: "claude-sonnet-4-6",
      reasoningEffort: "medium",
      source: "runtimeOptions",
    });

    const customModel = selectModelForTurn({
      backend: "native-agentkit",
      modelOptions: nativeModels,
      composer: composer({
        model: "gpt-6-preview",
        modelSelectionMode: "manual",
      }),
      prompt: "short",
    });
    expect(customModel.explanation).toMatchObject({
      model: "gpt-6-preview",
      source: "manual",
    });
  });

  it("uses large signals for directory attachments and conversation references", () => {
    const preview = previewAutoModelSelection({
      backend: "native-agentkit",
      modelOptions: nativeModels,
      composer: composer(),
      prompt: "short",
      attachments: [
        attachment("repo", {
          kind: "directory",
          directory: {
            fileCount: 1200,
            directoryCount: 80,
            totalSize: 50 * 1024 * 1024,
            truncated: true,
            unreadableCount: 0,
          },
        }),
      ],
      conversationReferences: [
        { taskId: "a", title: "A", route: "/a" },
        { taskId: "b", title: "B", route: "/b" },
        { taskId: "c", title: "C", route: "/c" },
      ],
    });
    expect(preview).toMatchObject({
      model: "gpt-5.5",
      reasoningEffort: "high",
      presetId: "plan",
    });
  });

  it("uses saved preset model assignments for auto selection", () => {
    const preview = previewAutoModelSelection({
      backend: "native-agentkit",
      modelOptions: nativeModels,
      composer: composer(),
      prompt: "short",
      modelFeatureSettings: featureSettings({
        presets: [
          {
            id: "fast",
            label: "Fast",
            kind: "builtin",
            model: "gpt-5.4",
            reasoningEffort: null,
            enabled: true,
          },
          {
            id: "default",
            label: "Default",
            kind: "builtin",
            model: null,
            reasoningEffort: null,
            enabled: true,
          },
          {
            id: "plan",
            label: "Plan",
            kind: "builtin",
            model: null,
            reasoningEffort: null,
            enabled: true,
          },
          {
            id: "review",
            label: "Review",
            kind: "builtin",
            model: null,
            reasoningEffort: null,
            enabled: true,
          },
        ],
      }),
    });

    expect(preview).toMatchObject({
      model: "gpt-5.4",
      reasoningEffort: "low",
      source: "auto",
      presetId: "fast",
    });
  });

  it("migrates legacy chat tier settings into presets", () => {
    const migrated = normalizeModelFeatureSettings({
      chat: { light: "gpt-5.4", normal: null, deep: "gpt-5.5" },
      title: null,
      suggestion: null,
      promptRouter: null,
      promptOptimize: null,
      autoTurnDecision: null,
    });
    expect(migrated.presets.find((p) => p.id === "fast")?.model).toBe("gpt-5.4");
    expect(migrated.presets.find((p) => p.id === "plan")?.model).toBe("gpt-5.5");
    expect(migrated.presets.find((p) => p.id === "review")?.model).toBe("gpt-5.5");
    expect(migrated.chat.light).toBe("gpt-5.4");
  });

  it("keeps custom presets when normalizing", () => {
    const next = normalizeModelFeatureSettings({
      chat: { light: null, normal: null, deep: null },
      presets: [
        {
          id: "custom-nightly",
          label: "Nightly",
          kind: "custom",
          model: "gpt-5.4-mini",
          reasoningEffort: "low",
          enabled: true,
        },
      ],
      title: null,
      suggestion: null,
      promptRouter: null,
      promptOptimize: null,
      autoTurnDecision: null,
    });
    expect(next.presets.filter((p) => p.kind === "builtin")).toHaveLength(4);
    expect(next.presets.find((p) => p.id === "custom-nightly")).toMatchObject({
      label: "Nightly",
      model: "gpt-5.4-mini",
      kind: "custom",
    });
  });

  it("keeps an explicitly cleared builtin model cleared", () => {
    const next = normalizeModelFeatureSettings({
      chat: { light: "legacy-light", normal: null, deep: null },
      presets: [
        {
          id: "fast",
          label: "Fast",
          kind: "builtin",
          model: null,
          reasoningEffort: null,
          enabled: true,
        },
      ],
      title: null,
      suggestion: null,
      promptRouter: null,
      promptOptimize: null,
      autoTurnDecision: null,
    });

    expect(next.presets.find((preset) => preset.id === "fast")?.model).toBeNull();
    expect(next.chat.light).toBeNull();
  });
});

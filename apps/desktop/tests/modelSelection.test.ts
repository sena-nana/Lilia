import { describe, expect, it } from "vitest";
import type {
  ChatAttachment,
  ChatComposerState,
  ChatModelOption,
  ProviderRuntimeOptions,
} from "@lilia/contracts";
import {
  LILIA_COMPACT_WORKFLOW_TYPE,
  LILIA_REVIEW_WORKFLOW_TYPE,
} from "@lilia/contracts";
import { previewAutoModelSelection, selectModelForTurn } from "../src/services/modelSelection";

const codexModels: ChatModelOption[] = [
  { id: "gpt-5.5", label: "GPT-5.5", backend: "codex" },
  { id: "gpt-5.4", label: "GPT-5.4", backend: "codex" },
  { id: "gpt-5.4-mini", label: "GPT-5.4 Mini", backend: "codex" },
];

const claudeModels: ChatModelOption[] = [
  { id: "claude-opus-4-7", label: "Opus 4.7", backend: "claude" },
  { id: "claude-sonnet-4-6", label: "Sonnet 4.6", backend: "claude" },
  { id: "claude-haiku-4-5", label: "Haiku 4.5", backend: "claude" },
];

function composer(overrides: Partial<ChatComposerState> = {}): ChatComposerState {
  return {
    taskId: "t-1",
    backend: "codex",
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

describe("model selection", () => {
  it.skip("selects light, normal, and deep tiers by context scale", () => {
    expect(selectModelForTurn({
      backend: "codex",
      modelOptions: codexModels,
      composer: composer(),
      prompt: "short",
    }).explanation).toMatchObject({
      model: "gpt-5.4-mini",
      reasoningEffort: "low",
      source: "auto",
    });

    expect(selectModelForTurn({
      backend: "codex",
      modelOptions: codexModels,
      composer: composer(),
      prompt: "x".repeat(2100),
    }).explanation).toMatchObject({
      model: "gpt-5.4",
      reasoningEffort: "medium",
    });

    expect(selectModelForTurn({
      backend: "codex",
      modelOptions: codexModels,
      composer: composer(),
      prompt: "x".repeat(8100),
    }).explanation).toMatchObject({
      model: "gpt-5.5",
      reasoningEffort: "high",
    });
  });

  it.skip("uses deep for plan/review/fix/batch and light for compact/diagnostics", () => {
    expect(selectModelForTurn({
      backend: "claude",
      modelOptions: claudeModels,
      composer: composer({ backend: "claude", model: "claude-sonnet-4-6", planMode: true }),
      prompt: "plan",
    }).explanation).toMatchObject({
      model: "claude-opus-4-7",
      reasoningEffort: "high",
    });

    expect(selectModelForTurn({
      backend: "claude",
      modelOptions: claudeModels,
      composer: composer({ backend: "claude", model: "claude-sonnet-4-6" }),
      prompt: "",
      workflow: { type: LILIA_REVIEW_WORKFLOW_TYPE, target: { type: "uncommittedChanges" } },
    }).explanation.model).toBe("claude-opus-4-7");

    expect(selectModelForTurn({
      backend: "claude",
      modelOptions: claudeModels,
      composer: composer({ backend: "claude", model: "claude-sonnet-4-6" }),
      prompt: "",
      workflow: { type: LILIA_COMPACT_WORKFLOW_TYPE },
    }).explanation).toMatchObject({
      model: "claude-haiku-4-5",
      reasoningEffort: "low",
    });
  });

  it.skip("lets manual composer selection override auto and runtimeOptions override manual", () => {
    const manual = selectModelForTurn({
      backend: "claude",
      modelOptions: claudeModels,
      composer: composer({
        backend: "claude",
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
    expect(manual.runtimeOptions.provider?.claude).toMatchObject({
      reasoningEffort: "max",
      thinking: { type: "adaptive" },
    });

    const runtimeOptions: ProviderRuntimeOptions = {
      common: { model: "claude-sonnet-4-6", reasoningEffort: "medium" },
    };
    const runtime = selectModelForTurn({
      backend: "claude",
      modelOptions: claudeModels,
      composer: composer({
        backend: "claude",
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

    const customCodexModel = selectModelForTurn({
      backend: "codex",
      modelOptions: codexModels,
      composer: composer({
        model: "gpt-6-preview",
        modelSelectionMode: "manual",
      }),
      prompt: "short",
    });
    expect(customCodexModel.explanation).toMatchObject({
      model: "gpt-6-preview",
      source: "manual",
    });

    const incompatibleModel = selectModelForTurn({
      backend: "codex",
      modelOptions: codexModels,
      composer: composer({
        model: "claude-sonnet-4-6",
        modelSelectionMode: "manual",
      }),
      prompt: "short",
    });
    expect(incompatibleModel.explanation.model).toBe("gpt-5.4-mini");
    expect(incompatibleModel.explanation.signals).toContain(
      "模型 claude-sonnet-4-6 不属于当前后端，已回退自动档位",
    );
  });

  it.skip("maps provider options for Claude and downgrades Codex max", () => {
    const claude = selectModelForTurn({
      backend: "claude",
      modelOptions: claudeModels,
      composer: composer({ backend: "claude", model: "claude-sonnet-4-6" }),
      prompt: "short",
    });
    expect(claude.runtimeOptions.provider?.claude).toMatchObject({
      reasoningEffort: "low",
      thinking: { type: "adaptive" },
    });

    const codex = selectModelForTurn({
      backend: "codex",
      modelOptions: codexModels,
      composer: composer({
        modelSelectionMode: "manual",
        model: "gpt-5.5",
        reasoningEffort: "max",
      }),
      prompt: "short",
    });
    expect(codex.explanation.reasoningEffort).toBe("xhigh");
    expect(codex.runtimeOptions.provider?.codex?.reasoningEffort).toBe("xhigh");
    expect(codex.explanation.signals).toContain("Codex 不支持 max，已降级为 xhigh");
  });

  it.skip("uses large signals for directory attachments and conversation references", () => {
    const preview = previewAutoModelSelection({
      backend: "codex",
      modelOptions: codexModels,
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
    });
  });

  it.skip("uses saved chat tier model assignments for auto selection", () => {
    const preview = previewAutoModelSelection({
      backend: "codex",
      modelOptions: codexModels,
      composer: composer(),
      prompt: "short",
      modelFeatureSettings: {
        chat: { light: "gpt-5.4", normal: null, deep: null },
        title: null,
        suggestion: null,
        promptRouter: null,
        promptOptimize: null,
        autoTurnDecision: null,
      },
    });

    expect(preview).toMatchObject({
      model: "gpt-5.4",
      reasoningEffort: "low",
      source: "auto",
    });
  });
});


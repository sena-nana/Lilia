import { describe, expect, it } from "vitest";
import {
  AGENT_TIMELINE_BATCH_EVENT_NAME,
  AGENT_TIMELINE_EVENT_NAME,
} from "@lilia/contracts";
import { onAgentTimelineEvents, onModelFeatureSettingsChanged, setModelFeatureSettings } from "../src/services/chat";
import {
  emitMockTimelineBatchEvent,
  emitMockTimelineEvent,
  failNextMockListen,
  mockListenerCount,
} from "./tauriMock";

describe("chat service timeline events", () => {
  it("normalizes single and batch timeline event streams", async () => {
    const seen: Array<{ source: string; ids: string[] }> = [];
    const unlisten = await onAgentTimelineEvents((events, source) => {
      seen.push({ source, ids: events.map((event) => event.id) });
    });

    expect(mockListenerCount(AGENT_TIMELINE_EVENT_NAME)).toBe(1);
    expect(mockListenerCount(AGENT_TIMELINE_BATCH_EVENT_NAME)).toBe(1);

    emitMockTimelineEvent("task-service-events", { id: "single-1" });
    emitMockTimelineBatchEvent("task-service-events", [
      { id: "batch-1" },
      { id: "batch-2" },
    ]);

    expect(seen).toEqual([
      { source: "single", ids: ["single-1"] },
      { source: "batch", ids: ["batch-1", "batch-2"] },
    ]);

    unlisten();

    expect(mockListenerCount(AGENT_TIMELINE_EVENT_NAME)).toBe(0);
    expect(mockListenerCount(AGENT_TIMELINE_BATCH_EVENT_NAME)).toBe(0);

    emitMockTimelineEvent("task-service-events", { id: "single-2" });
    expect(seen).toHaveLength(2);
  });

  it("cleans up the single listener if batch listener registration fails", async () => {
    failNextMockListen(AGENT_TIMELINE_BATCH_EVENT_NAME, "batch listener failed");

    await expect(onAgentTimelineEvents(() => {})).rejects.toThrow("batch listener failed");

    expect(mockListenerCount(AGENT_TIMELINE_EVENT_NAME)).toBe(0);
    expect(mockListenerCount(AGENT_TIMELINE_BATCH_EVENT_NAME)).toBe(0);
  });

  it("notifies mounted consumers after model feature settings are saved", async () => {
    const seen: string[] = [];
    const unlisten = onModelFeatureSettingsChanged((settings) => {
      seen.push(settings.chat.light ?? "");
    });

    await setModelFeatureSettings({
      chat: { light: "gpt-5.4-mini", normal: null, deep: null },
      presets: [
        { id: "fast", label: "Fast", kind: "builtin", model: "gpt-5.4-mini", reasoningEffort: null, enabled: true },
        { id: "default", label: "Default", kind: "builtin", model: null, reasoningEffort: null, enabled: true },
        { id: "plan", label: "Plan", kind: "builtin", model: null, reasoningEffort: null, enabled: true },
        { id: "review", label: "Review", kind: "builtin", model: null, reasoningEffort: null, enabled: true },
      ],
      title: null,
      suggestion: null,
      promptRouter: null,
      promptOptimize: null,
      autoTurnDecision: null,
    });

    expect(seen).toEqual(["gpt-5.4-mini"]);

    unlisten();
    await setModelFeatureSettings({
      chat: { light: "gpt-5.4", normal: null, deep: null },
      presets: [
        { id: "fast", label: "Fast", kind: "builtin", model: "gpt-5.4", reasoningEffort: null, enabled: true },
        { id: "default", label: "Default", kind: "builtin", model: null, reasoningEffort: null, enabled: true },
        { id: "plan", label: "Plan", kind: "builtin", model: null, reasoningEffort: null, enabled: true },
        { id: "review", label: "Review", kind: "builtin", model: null, reasoningEffort: null, enabled: true },
      ],
      title: null,
      suggestion: null,
      promptRouter: null,
      promptOptimize: null,
      autoTurnDecision: null,
    });

    expect(seen).toEqual(["gpt-5.4-mini"]);
  });
});


import { describe, expect, it } from "vitest";
import {
  buildCodexCollaborationMode,
  readCodexPlanModePreset,
} from "../legacy/agent-runner/codex/runCodex.mjs";

describe("Codex plan helpers", () => {
  it("reads plan collaboration preset and builds fallback settings", async () => {
    const server = {
      request: async () => ({
        data: [
          { name: "chat", mode: "default", reasoning_effort: null },
          { name: "plan", mode: "plan", reasoning_effort: "high" },
        ],
      }),
    };

    await expect(readCodexPlanModePreset(server as any)).resolves.toMatchObject({
      mode: "plan",
      reasoning_effort: "high",
    });
    expect(buildCodexCollaborationMode("plan", "gpt-5.1", { reasoning_effort: "high" })).toEqual({
      mode: "plan",
      settings: {
        model: "gpt-5.1",
        reasoning_effort: "high",
        developer_instructions: null,
      },
    });
    expect(buildCodexCollaborationMode("plan", null, null)).toEqual({
      mode: "plan",
      settings: {
        model: "gpt-5",
        reasoning_effort: "medium",
        developer_instructions: null,
      },
    });
  });

  it("fails when plan collaboration preset is unavailable", async () => {
    await expect(readCodexPlanModePreset({
      request: async () => ({ data: [] }),
    } as any)).rejects.toThrow("plan collaboration preset is missing");
    await expect(readCodexPlanModePreset({
      request: async () => {
        throw new Error("unsupported");
      },
    } as any)).rejects.toThrow("collaborationMode/list failed");
  });

});


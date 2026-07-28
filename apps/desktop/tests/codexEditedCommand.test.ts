import { describe, expect, it } from "vitest";
import {
  buildEditedCommandAdditionalContext,
  executeEditedCodexCommand,
  normalizeEditedCommandExecResult,
} from "../legacy/agent-runner/codex/editedCommand.mjs";

describe("Codex edited command helpers", () => {
  it("normalizes direct and nested command exec results", () => {
    expect(normalizeEditedCommandExecResult({
      result: { exit_code: 2, stdout: "out", stderr: "err" },
    })).toEqual({
      exitCode: 2,
      stdout: "out",
      stderr: "err",
      output: "out\nerr",
    });

    expect(normalizeEditedCommandExecResult(null, { output: "fallback" })).toMatchObject({
      exitCode: 0,
      stdout: "fallback",
      output: "fallback",
    });
  });

  it("falls back to process/spawn and captures output deltas", async () => {
    const calls: any[] = [];
    let drained = false;
    const server = {
      request: async (method: string, params: any) => {
        calls.push([method, params]);
        if (method === "command/exec") throw new Error("method not found");
        if (method === "process/spawn") return { processId: "proc-1" };
        throw new Error(`unexpected request ${method}`);
      },
      drainNotifications: () => {
        if (drained) return [];
        drained = true;
        return [
          {
            method: "process/outputDelta",
            params: { processId: "proc-1", stream: "stdout", delta: "ok\n" },
          },
          {
            method: "process/exited",
            params: { processId: "proc-1", exitCode: 0 },
          },
        ];
      },
    };

    await expect(executeEditedCodexCommand(
      server as any,
      { modifiedCommand: "npm test -- --watch=false" },
      "C:/repo",
    )).resolves.toMatchObject({
      exitCode: 0,
      output: "ok\n",
    });
    expect(calls).toEqual([
      ["command/exec", { command: "npm test -- --watch=false", cwd: "C:/repo" }],
      ["process/spawn", { command: "npm test -- --watch=false", cwd: "C:/repo" }],
    ]);
  });

  it("builds model-visible context for edited command results", () => {
    const context = buildEditedCommandAdditionalContext(
      {
        originalCommand: "yarn test",
        modifiedCommand: "yarn test --runInBand",
      },
      {
        exitCode: 0,
        output: "tests passed",
      },
    );

    expect(context).toContain("不要再执行原始命令");
    expect(context).toContain("yarn test --runInBand");
    expect(context).toContain("退出码：0");
    expect(context).toContain("tests passed");
  });
});


// #47 LEGACY — Node agent-runner CLI entry (compat until product 1.0.0).
//
// NOT the default Desktop path. Only launched when
// `LILIA_AGENT_EXECUTION_BACKEND=node|legacy|agent-runner` is set explicitly.
// Default install resources do NOT package this file or Codex app-server.
//
// Rust/Tauri may still launch this file in legacy mode with `node agent-runner.mjs`.
// The implementation lives under `agent-runner/` so this file only wires stdio
// and process lifetime.

import { createInterface } from "node:readline";
import { createRunnerContext, runAgentTurn } from "./agent-runner/core.mjs";
import { runCodexSparkPromptCommand } from "./agent-runner/codex/sparkPrompt.mjs";

async function readInitialCommand(rl, protocol, context) {
  let cmd = null;
  let resolveCommand;
  const ready = new Promise((resolve) => {
    resolveCommand = resolve;
  });

  rl.on("line", (line) => {
    if (!cmd) {
      try {
        cmd = JSON.parse(line.replace(/^\uFEFF/, ""));
      } catch (err) {
        protocol.emit({ type: "error", message: `invalid stdin JSON: ${err.message}` });
        process.exit(1);
      }
      resolveCommand(cmd);
      return;
    }
    context.interactions.handleControlLine(line);
  });

  return ready;
}

async function main() {
  const context = createRunnerContext();
  const rl = createInterface({ input: process.stdin });
  const cmd = await readInitialCommand(rl, context.protocol, context);
  if (cmd?.kind === "codex_spark_prompt") {
    const result = await runCodexSparkPromptCommand(cmd);
    process.stdout.write(`${JSON.stringify(result)}\n`);
    rl.close();
    process.exit(0);
  }
  const result = await runAgentTurn(cmd, context);
  rl.close();
  process.exit(result.exitCode);
}

main().catch((err) => {
  const context = createRunnerContext();
  context.protocol.emit({ type: "error", message: err?.message || String(err) });
  process.exit(1);
});

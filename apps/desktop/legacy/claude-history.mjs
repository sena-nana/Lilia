// #47 LEGACY — Claude history utility (read-only provenance / migration aid).
import { createInterface } from "node:readline";
import {
  previewClaudeSession,
  previewClaudeSessionLite,
  searchClaudeSessions,
  syncClaudeSessionHistoryForTask,
} from "./agent-runner/claude/history.mjs";

async function readJsonLine() {
  const rl = createInterface({ input: process.stdin, crlfDelay: Infinity });
  try {
    for await (const line of rl) {
      if (!line.trim()) continue;
      return JSON.parse(line.replace(/^\uFEFF/, ""));
    }
  } finally {
    rl.close();
  }
  return {};
}

function writeJson(payload) {
  process.stdout.write(`${JSON.stringify(payload)}\n`);
}

async function main() {
  const input = await readJsonLine();
  const action = input?.action;
  if (action === "search") {
    writeJson(await searchClaudeSessions(input.input || {}));
    return;
  }
  if (action === "preview") {
    const detail = input.detail === "full" ? "full" : "lite";
    writeJson(detail === "full"
      ? await previewClaudeSession(String(input.sessionId || ""))
      : await previewClaudeSessionLite(String(input.sessionId || "")));
    return;
  }
  if (action === "sync") {
    writeJson(await syncClaudeSessionHistoryForTask({
      taskId: String(input.taskId || ""),
      sessionId: String(input.sessionId || ""),
      limit: input.limit,
      cursor: input.cursor,
    }));
    return;
  }
  throw new Error(`unknown claude history action: ${action || ""}`);
}

main().catch((err) => {
  writeJson({ error: err?.message || String(err) });
  process.exit(1);
});

// #47 LEGACY — Codex history utility (read-only provenance / migration aid).
import { createInterface } from "node:readline";
import {
  archiveCodexThread,
  cleanCodexThreadBackgroundTerminals,
  previewCodexThread,
  previewCodexThreadLite,
  renameCodexThread,
  searchCodexThreads,
  syncCodexThreadHistoryForTask,
} from "./agent-runner/codex/history.mjs";

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
    writeJson(await searchCodexThreads(input.input || {}));
    return;
  }
  if (action === "preview") {
    const detail = input.detail === "full" ? "full" : "lite";
    writeJson(detail === "full"
      ? await previewCodexThread(String(input.threadId || ""))
      : await previewCodexThreadLite(String(input.threadId || "")));
    return;
  }
  if (action === "sync") {
    writeJson(await syncCodexThreadHistoryForTask({
      taskId: String(input.taskId || ""),
      threadId: String(input.threadId || ""),
      limit: input.limit,
      cursor: input.cursor,
    }));
    return;
  }
  if (action === "cleanBackgroundTerminals") {
    writeJson(await cleanCodexThreadBackgroundTerminals(String(input.threadId || "")));
    return;
  }
  if (action === "archiveThread") {
    writeJson(await archiveCodexThread(String(input.threadId || "")));
    return;
  }
  if (action === "renameThread") {
    writeJson(await renameCodexThread(
      String(input.threadId || ""),
      String(input.name || ""),
    ));
    return;
  }
  throw new Error(`unknown codex history action: ${action || ""}`);
}

main().catch((err) => {
  writeJson({ error: err?.message || String(err) });
  process.exit(1);
});

import { createCodexAppServer, initializeCodexAppServer } from "./appServer.mjs";
import {
  normalizeCodexAppServerEvent,
  pickCodexAssistantText,
} from "./timeline.mjs";
import { PROMPT_CODEX } from "@lilia/contracts/promptContract.mjs";

export const CODEX_SPARK_MODEL = "gpt-5.3-codex-spark";
const DEFAULT_TIMEOUT_MS = 30_000;

function stringOrNull(value) {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function codexThreadIdFromResult(result) {
  if (!result || typeof result !== "object" || Array.isArray(result)) return null;
  const thread = result.thread && typeof result.thread === "object" ? result.thread : null;
  return stringOrNull(thread?.id) || stringOrNull(result.threadId) || stringOrNull(result.id);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export function updateCodexSparkPromptState(state, msg) {
  const ev = normalizeCodexAppServerEvent(msg);
  if (!ev) return state;
  if (ev.type === "agentMessage.delta") {
    state.deltaText += typeof ev.delta === "string" ? ev.delta : "";
    return state;
  }
  if (ev.type === "item.completed") {
    const item = ev.item || ev;
    if (item?.type === "agentMessage") {
      const text = pickCodexAssistantText(item);
      if (text) state.snapshotText = text;
    }
    return state;
  }
  if (ev.type === "turn.completed") {
    state.completed = true;
    return state;
  }
  if (ev.type === "turn.failed" || ev.type === "error") {
    state.completed = true;
    state.error = ev.error?.message || ev.message || "Codex Spark turn failed";
  }
  return state;
}

export async function runCodexSparkPrompt(cmd, context = {}) {
  const prompt = stringOrNull(cmd?.prompt);
  if (!prompt) throw new Error("Codex Spark prompt is empty");
  const instruction = stringOrNull(cmd?.instruction) || PROMPT_CODEX.sparkDefaultInstruction;
  const timeoutMs = Number.isFinite(cmd?.timeoutMs)
    ? Math.max(1, Number(cmd.timeoutMs))
    : DEFAULT_TIMEOUT_MS;
  const cwd = stringOrNull(cmd?.cwd) || context.cwd?.() || process.cwd();
  const createServer = context.createCodexAppServer || (() =>
    createCodexAppServer({ env: context.env || process.env }));
  const server = createServer();
  const state = {
    deltaText: "",
    snapshotText: "",
    completed: false,
    error: null,
  };
  try {
    await initializeCodexAppServer(server);
    const started = await server.request("thread/start", {
      model: CODEX_SPARK_MODEL,
      cwd,
      approvalPolicy: "never",
      sandbox: "read-only",
      dynamicTools: [],
    });
    const threadId = codexThreadIdFromResult(started);
    if (!threadId) throw new Error("Codex Spark thread/start did not return a thread id");
    await server.request("turn/start", {
      threadId,
      input: [{ type: "text", text: prompt }],
      cwd,
      approvalPolicy: "never",
      sandbox: "read-only",
      collaborationMode: {
        mode: "default",
        settings: {
          model: CODEX_SPARK_MODEL,
          reasoning_effort: null,
          developer_instructions: instruction,
        },
      },
    });
    const startedAt = Date.now();
    while (!state.completed) {
      for (const msg of server.drainNotifications()) {
        updateCodexSparkPromptState(state, msg);
      }
      if (state.error) throw new Error(state.error);
      if (Date.now() - startedAt > timeoutMs) {
        throw new Error("Codex Spark turn timed out");
      }
      if (!state.completed) await delay(20);
    }
    const text = stringOrNull(state.snapshotText) || stringOrNull(state.deltaText);
    if (!text) throw new Error("Codex Spark turn returned empty text");
    return text;
  } finally {
    server.close?.({ forceKillMs: 5000 });
  }
}

export async function runCodexSparkPromptCommand(cmd, context = {}) {
  try {
    return {
      ok: true,
      text: await runCodexSparkPrompt(cmd, context),
      error: null,
    };
  } catch (err) {
    return {
      ok: false,
      text: null,
      error: err?.message || String(err),
    };
  }
}

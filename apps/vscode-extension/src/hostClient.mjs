/**
 * AgentKit host client for VS Code compat (#40).
 *
 * Modes:
 * - deterministic: in-process stub matching `lilia-editor-compat` contract
 *   (used by unit tests and default extension config; no LiliaCore)
 * - process: spawn `lilia-editor-compat` JSONL binary
 */

import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

const DEFAULT_INSERT = ' println!("hi"); }';

/**
 * @returns {{ hostMode: string, hostBinary: string }}
 */
export function readHostConfig(env = process.env) {
  return {
    hostMode: (env.LILIA_AGENTKIT_HOST_MODE || "deterministic").trim(),
    hostBinary: (env.LILIA_AGENTKIT_HOST_BINARY || "").trim(),
  };
}

/**
 * @returns {Promise<{ ok: boolean, status?: object, error?: string }>}
 */
export async function hostStatus(config = readHostConfig()) {
  if (config.hostMode === "process") {
    return requestProcess(config.hostBinary, { op: "status" });
  }
  return {
    ok: true,
    status: {
      hostId: "lilia.editor-compat",
      requiresLiliaCore: false,
      requiresOfficialAgentServer: false,
      requiresNodeAgentRunner: false,
      completionService: "mutsuki.agent.service.code-completion",
      nextEditService: "mutsuki.agent.service.next-edit",
    },
  };
}

/**
 * @param {{ uri: string, languageId?: string, prefix: string, suffix?: string, generation?: number }} input
 */
export async function complete(input, config = readHostConfig()) {
  if (config.hostMode === "process") {
    return requestProcess(config.hostBinary, {
      op: "complete",
      uri: input.uri,
      language_id: input.languageId ?? "plaintext",
      prefix: input.prefix,
      suffix: input.suffix ?? "",
      generation: input.generation ?? 1,
    }).then((response) => {
      if (!response.ok) return response;
      const first = response.completion?.candidates?.[0] ?? {};
      const insertText = first.insertText ?? first.insert_text ?? "";
      return { ok: true, insertText, completion: response.completion };
    });
  }
  return {
    ok: true,
    insertText: DEFAULT_INSERT,
    completion: {
      status: "ready",
      candidates: [{ insertText: DEFAULT_INSERT, confidence: 0.9 }],
      generation: input.generation ?? 1,
    },
  };
}

/**
 * @param {{ uri: string, languageId?: string, content: string, summary?: string, generation?: number }} input
 */
export async function planNextEdit(input, config = readHostConfig()) {
  if (config.hostMode === "process") {
    return requestProcess(config.hostBinary, {
      op: "next_edit",
      uri: input.uri,
      language_id: input.languageId ?? "plaintext",
      content: input.content,
      summary: input.summary ?? "editor change",
      generation: input.generation ?? 1,
    }).then((response) => {
      if (!response.ok) return response;
      const next = response.nextEdit;
      const candidate =
        next && next.kind === "candidate" ? (next.candidate ?? null) : null;
      return { ok: true, candidate, nextEdit: next };
    });
  }
  return {
    ok: true,
    candidate: {
      candidateId: "next-edit-deterministic-1",
      proposal: {
        proposalId: "next-edit-proposal-deterministic-1",
        changes: [{ changeId: "chg-1", summary: input.summary ?? "editor change" }],
      },
    },
  };
}

/**
 * VS Code TextDocument adapter for Completion.
 * @param {{ uri: { toString(): string }, languageId: string, lineAt(line: number): { text: string }, offsetAt(pos: { line: number, character: number }): number, getText(): string }} document
 * @param {{ line: number, character: number }} position
 */
export async function completeFromDocument(document, position, config = readHostConfig()) {
  const line = document.lineAt(position.line).text;
  const prefix = line.slice(0, position.character);
  const suffix = line.slice(position.character);
  return complete(
    {
      uri: document.uri.toString(),
      languageId: document.languageId,
      prefix,
      suffix,
      generation: 1,
    },
    config,
  );
}

/**
 * @param {{ uri: { toString(): string }, languageId: string, getText(): string }} document
 */
export async function planNextEditFromDocument(document, config = readHostConfig()) {
  return planNextEdit(
    {
      uri: document.uri.toString(),
      languageId: document.languageId,
      content: document.getText(),
      summary: "vscode active editor change",
      generation: 1,
    },
    config,
  );
}

/**
 * @param {string} binary
 * @param {object} request
 */
async function requestProcess(binary, request) {
  if (!binary) {
    return {
      ok: false,
      error: "lilia.agentkit.hostBinary is empty while hostMode=process",
    };
  }
  return new Promise((resolve) => {
    const child = spawn(binary, [], {
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    let settled = false;
    const fail = (error) => {
      if (settled) return;
      settled = true;
      resolve({ ok: false, error: String(error) });
    };
    child.on("error", fail);
    const rl = createInterface({ input: child.stdout });
    rl.once("line", (line) => {
      if (settled) return;
      settled = true;
      try {
        resolve(JSON.parse(line));
      } catch (err) {
        resolve({ ok: false, error: `invalid host JSON: ${err}` });
      } finally {
        child.kill();
        rl.close();
      }
    });
    child.stdin.write(`${JSON.stringify(request)}\n`);
    child.stdin.end();
    setTimeout(() => fail("host request timed out"), 10_000);
  });
}

/**
 * AgentKit host client for VS Code compat (#40).
 *
 * Production uses the standalone Rust editor Host. The deterministic mode is
 * explicit test injection only and is not exposed as an extension setting.
 */

import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

const DEFAULT_INSERT = ' println!("hi"); }';
const processClients = new Map();

/**
 * @param {NodeJS.ProcessEnv} env
 * @param {{ hostMode?: string, hostBinary?: string }} settings
 * @returns {{ hostMode: string, hostBinary: string }}
 */
export function readHostConfig(env = process.env, settings = {}) {
  return {
    hostMode: String(
      settings.hostMode ?? env.LILIA_AGENTKIT_HOST_MODE ?? "process",
    ).trim(),
    hostBinary: String(
      settings.hostBinary ??
        env.LILIA_AGENTKIT_HOST_BINARY ??
        "lilia-editor-compat",
    ).trim(),
  };
}

/**
 * @returns {Promise<{ ok: boolean, status?: object, error?: string }>}
 */
export async function hostStatus(config = readHostConfig()) {
  if (config.hostMode === "deterministic") {
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
  return requestProcess(config.hostBinary, { op: "status" });
}

/**
 * @param {{ uri: string, languageId?: string, prefix: string, suffix?: string, generation?: number, signal?: AbortSignal }} input
 */
export async function complete(input, config = readHostConfig()) {
  if (config.hostMode === "deterministic") {
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
  return requestProcess(
    config.hostBinary,
    {
      op: "complete",
      uri: input.uri,
      language_id: input.languageId ?? "plaintext",
      prefix: input.prefix,
      suffix: input.suffix ?? "",
      generation: input.generation ?? 1,
    },
    input.signal,
  ).then((response) => {
    if (!response.ok) return response;
    const first = response.completion?.candidates?.[0] ?? {};
    const insertText = first.insertText ?? first.insert_text ?? "";
    return { ok: true, insertText, completion: response.completion };
  });
}

/**
 * @param {{ uri: string, languageId?: string, content: string, summary?: string, generation?: number }} input
 */
export async function planNextEdit(input, config = readHostConfig()) {
  if (config.hostMode === "deterministic") {
    return {
      ok: true,
      candidate: {
        candidateId: "next-edit-deterministic-1",
        generation: input.generation ?? 1,
        expectedDocumentVersions: [
          [{ workspaceId: "ws", uri: input.uri }, input.generation ?? 1],
        ],
        requiresPreview: false,
        proposal: {
          proposalId: "next-edit-proposal-deterministic-1",
          changes: [
            {
              changeId: "chg-1",
              document: { workspaceId: "ws", uri: input.uri },
              baseVersion: input.generation ?? 1,
              edits: [
                {
                  range: {
                    start: { line: 0, character: input.content.length },
                    end: { line: 0, character: input.content.length },
                  },
                  newText: "\n",
                },
              ],
            },
          ],
        },
      },
    };
  }
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
      next && next.kind === "candidate"
        ? normalizeCandidate(next.candidate ?? null)
        : null;
    return { ok: true, candidate, nextEdit: next };
  });
}

/**
 * Revalidate a candidate against the editor's current document versions.
 * @param {string} candidateId
 * @param {Array<[object, number]>} documentVersions
 * @param {{ commit: string, generation: number } | null} gitHead
 */
export async function validateNextEdit(
  candidateId,
  documentVersions,
  gitHead = null,
  config = readHostConfig(),
) {
  if (config.hostMode === "deterministic") {
    return { ok: true, valid: true, nextEdit: { kind: "valid", candidate_id: candidateId } };
  }
  return requestProcess(config.hostBinary, {
    op: "next_edit_validate",
    candidate_id: candidateId,
    document_versions: documentVersions.map(([document, version]) => [
      {
        workspace_id: document.workspaceId ?? document.workspace_id,
        uri: document.uri,
      },
      version,
    ]),
    git_head: gitHead,
    now_unix_ms: Date.now(),
  }).then((response) => ({
    ...response,
    valid: response.ok && response.nextEdit?.kind === "valid",
  }));
}

/**
 * @param {string} candidateId
 * @param {"accepted" | "rejected" | "skipped"} kind
 * @param {string | null} reasonCode
 */
export async function recordNextEditFeedback(
  candidateId,
  kind,
  reasonCode = null,
  config = readHostConfig(),
) {
  if (config.hostMode === "deterministic") {
    return { ok: true, nextEdit: { kind: "feedback_recorded" } };
  }
  return requestProcess(config.hostBinary, {
    op: "next_edit_feedback",
    candidate_id: candidateId,
    kind,
    reason_code: reasonCode,
    timestamp_unix_ms: Date.now(),
  });
}

/**
 * VS Code TextDocument adapter for Completion.
 * @param {{ uri: { toString(): string }, languageId: string, version?: number, offsetAt(pos: { line: number, character: number }): number, getText(): string }} document
 * @param {{ line: number, character: number }} position
 * @param {{ signal?: AbortSignal }} options
 */
export async function completeFromDocument(
  document,
  position,
  config = readHostConfig(),
  options = {},
) {
  const content = document.getText();
  const offset = document.offsetAt(position);
  return complete(
    {
      uri: document.uri.toString(),
      languageId: document.languageId,
      prefix: content.slice(0, offset),
      suffix: content.slice(offset),
      generation: document.version ?? 1,
      signal: options.signal,
    },
    config,
  );
}

/**
 * @param {{ uri: { toString(): string }, languageId: string, version?: number, getText(): string }} document
 */
export async function planNextEditFromDocument(document, config = readHostConfig()) {
  return planNextEdit(
    {
      uri: document.uri.toString(),
      languageId: document.languageId,
      content: document.getText(),
      summary: "active editor change",
      generation: document.version ?? 1,
    },
    config,
  );
}

export function disposeHostClients() {
  for (const client of processClients.values()) {
    client.dispose("editor host disposed");
  }
  processClients.clear();
}

function normalizeCandidate(candidate) {
  if (!candidate) return null;
  return {
    ...candidate,
    candidateId: candidate.candidateId ?? candidate.candidate_id,
    generation: candidate.generation,
    requiresPreview: candidate.requiresPreview ?? candidate.requires_preview ?? false,
    expectedDocumentVersions:
      candidate.expectedDocumentVersions ??
      candidate.expected_document_versions?.map(([document, version]) => [
        normalizeDocument(document),
        version,
      ]) ??
      [],
    proposal: normalizeProposal(candidate.proposal),
  };
}

function normalizeProposal(proposal = {}) {
  return {
    ...proposal,
    proposalId: proposal.proposalId ?? proposal.proposal_id,
    changes: (proposal.changes ?? []).map((change) => ({
      ...change,
      changeId: change.changeId ?? change.change_id,
      document: normalizeDocument(change.document),
      baseVersion: change.baseVersion ?? change.base_version,
      edits: (change.edits ?? []).map((edit) => ({
        range: edit.range,
        newText: edit.newText ?? edit.new_text ?? "",
      })),
    })),
  };
}

function normalizeDocument(document = {}) {
  return {
    workspaceId: document.workspaceId ?? document.workspace_id ?? "",
    uri: document.uri ?? "",
  };
}

function requestProcess(binary, request, signal) {
  if (!binary) {
    return Promise.resolve({
      ok: false,
      error: "lilia.agentkit.hostBinary is empty",
    });
  }
  let client = processClients.get(binary);
  if (!client) {
    client = new ProcessHostClient(binary);
    processClients.set(binary, client);
  }
  return client.request(request, signal);
}

class ProcessHostClient {
  constructor(binary) {
    this.binary = binary;
    this.child = null;
    this.readline = null;
    this.queue = [];
    this.active = null;
  }

  request(payload, signal) {
    if (signal?.aborted) {
      return Promise.resolve({ ok: false, error: "host request cancelled" });
    }
    return new Promise((resolve) => {
      const item = { payload, resolve, signal, abort: null, timeout: null };
      if (signal) {
        item.abort = () => {
          if (this.active === item) {
            this.dispose("host request cancelled");
          } else {
            const index = this.queue.indexOf(item);
            if (index >= 0) this.queue.splice(index, 1);
            resolve({ ok: false, error: "host request cancelled" });
          }
        };
        signal.addEventListener("abort", item.abort, { once: true });
      }
      this.queue.push(item);
      this.pump();
    });
  }

  pump() {
    if (this.active || this.queue.length === 0) return;
    if (!this.ensureProcess()) return;
    const item = this.queue.shift();
    this.active = item;
    item.timeout = setTimeout(() => {
      if (this.active === item) this.dispose("host request timed out");
    }, 30_000);
    this.child.stdin.write(`${JSON.stringify(item.payload)}\n`, (error) => {
      if (error && this.active === item) this.dispose(String(error));
    });
  }

  ensureProcess() {
    if (this.child && !this.child.killed) return true;
    try {
      const child = spawn(this.binary, [], {
        stdio: ["pipe", "pipe", "pipe"],
        windowsHide: true,
      });
      this.child = child;
      this.readline = createInterface({ input: child.stdout });
      this.readline.on("line", (line) => this.onLine(line));
      child.on("error", (error) => this.dispose(String(error)));
      child.on("exit", () => {
        if (this.child === child) this.dispose("editor host exited");
      });
      return true;
    } catch (error) {
      this.dispose(String(error));
      return false;
    }
  }

  onLine(line) {
    const item = this.active;
    if (!item) return;
    this.active = null;
    clearTimeout(item.timeout);
    if (item.abort) item.signal.removeEventListener("abort", item.abort);
    try {
      item.resolve(JSON.parse(line));
    } catch (error) {
      item.resolve({ ok: false, error: `invalid host JSON: ${error}` });
    }
    this.pump();
  }

  dispose(reason) {
    const pending = [this.active, ...this.queue].filter(Boolean);
    this.active = null;
    this.queue = [];
    for (const item of pending) {
      clearTimeout(item.timeout);
      if (item.abort) item.signal.removeEventListener("abort", item.abort);
      item.resolve({ ok: false, error: reason });
    }
    const child = this.child;
    this.child = null;
    this.readline?.close();
    this.readline = null;
    if (child && !child.killed) child.kill();
  }
}

/**
 * Host-mock proof for VS Code Extension behavior (#40).
 *
 * Does not start the VS Code extension host. Injects a stub `vscode` into
 * `extensionCore` and verifies InlineCompletionProvider + Next Edit command.
 *
 * Toolchain: runnable with plain Node (25+). Root `yarn verify:vscode-compat`
 * still requires Node 26 via check-toolchain — when unavailable, run:
 *   cargo test -p lilia-editor-compat --locked
 *   node apps/vscode-extension/tests/hostClient.test.mjs
 *   node apps/vscode-extension/tests/extensionBehavior.test.mjs
 */

import assert from "node:assert/strict";
import { activate } from "../src/extensionCore.mjs";

function createVscodeMock() {
  /** @type {Map<string, (...args: unknown[]) => unknown>} */
  const commands = new Map();
  /** @type {Array<object>} */
  const providers = [];
  /** @type {Array<{ kind: string, message: string }>} */
  const messages = [];
  /** @type {string[]} */
  const outputLines = [];

  class Range {
    /**
     * @param {{ line: number, character: number }} start
     * @param {{ line: number, character: number }} end
     */
    constructor(start, end) {
      this.start = start;
      this.end = end;
    }
  }

  return {
    Range,
    window: {
      createOutputChannel() {
        return {
          appendLine(line) {
            outputLines.push(String(line));
          },
          show() {},
          dispose() {},
        };
      },
      showInformationMessage(message) {
        messages.push({ kind: "info", message: String(message) });
        return Promise.resolve(undefined);
      },
      showWarningMessage(message) {
        messages.push({ kind: "warn", message: String(message) });
        return Promise.resolve(undefined);
      },
      showErrorMessage(message) {
        messages.push({ kind: "error", message: String(message) });
        return Promise.resolve(undefined);
      },
      get activeTextEditor() {
        return {
          document: {
            uri: { toString: () => "file:///workspace/main.rs" },
            languageId: "rust",
            getText: () => "fn main() {}",
            lineAt: () => ({ text: "fn main() {" }),
            offsetAt: (pos) => pos.character,
          },
        };
      },
    },
    languages: {
      registerInlineCompletionItemProvider(_selector, provider) {
        providers.push(provider);
        return { dispose() {} };
      },
    },
    commands: {
      registerCommand(id, handler) {
        commands.set(id, handler);
        return { dispose() {} };
      },
    },
    __test: { commands, providers, messages, outputLines },
  };
}

async function main() {
  process.env.LILIA_AGENTKIT_HOST_MODE = "deterministic";
  const vscode = createVscodeMock();
  const subscriptions = [];
  activate(vscode, {
    subscriptions: {
      push(...items) {
        subscriptions.push(...items);
      },
    },
  });

  const { commands, providers, messages, outputLines } = vscode.__test;
  assert.ok(subscriptions.length >= 3);
  assert.equal(providers.length, 1, "InlineCompletionProvider registered");
  assert.ok(commands.has("lilia.agentkit.status"));
  assert.ok(commands.has("lilia.agentkit.nextEdit"));

  const completion = await providers[0].provideInlineCompletionItems(
    {
      uri: { toString: () => "file:///workspace/main.rs" },
      languageId: "rust",
      getText: () => "fn main() {",
      lineAt: () => ({ text: "fn main() {" }),
      offsetAt: (pos) => pos.character,
    },
    { line: 0, character: 11 },
    {},
    { isCancellationRequested: false },
  );
  assert.ok(Array.isArray(completion.items));
  assert.ok(completion.items.length >= 1);
  assert.ok(String(completion.items[0].insertText).length > 0);
  assert.ok(completion.items[0].range instanceof vscode.Range);

  await commands.get("lilia.agentkit.status")();
  assert.ok(
    messages.some((item) => item.kind === "info" && item.message.includes("AgentKit host ready")),
  );
  assert.ok(outputLines.some((line) => line.includes("requiresLiliaCore")));

  await commands.get("lilia.agentkit.nextEdit")();
  assert.ok(
    messages.some(
      (item) =>
        item.kind === "info" &&
        item.message.includes("Next Edit proposal") &&
        item.message.includes("not applied automatically"),
    ),
  );

  console.log(
    "[vscode-extension extensionBehavior] OK — InlineCompletion + Next Edit via host mock (no VS Code host, no LiliaCore)",
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

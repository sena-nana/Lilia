/**
 * VS Code Extension core (#40) — injectable `vscode` for host-mock tests.
 *
 * `extension.mjs` wires the real `vscode` peer; unit tests pass a stub.
 */

import {
  completeFromDocument,
  hostStatus,
  planNextEditFromDocument,
} from "./hostClient.mjs";

/**
 * @param {typeof import('vscode')} vscode
 * @param {{ subscriptions: { push(...items: unknown[]): void } }} context
 */
export function activate(vscode, context) {
  const output = vscode.window.createOutputChannel("Lilia AgentKit");
  context.subscriptions.push(output);

  const provider = {
    /**
     * @param {import('vscode').TextDocument} document
     * @param {import('vscode').Position} position
     * @param {import('vscode').InlineCompletionContext} _context
     * @param {import('vscode').CancellationToken} _token
     */
    async provideInlineCompletionItems(document, position, _context, _token) {
      const result = await completeFromDocument(document, position);
      if (!result.ok || !result.insertText) {
        return { items: [] };
      }
      return {
        items: [
          {
            insertText: result.insertText,
            range: new vscode.Range(position, position),
          },
        ],
      };
    },
  };

  context.subscriptions.push(
    vscode.languages.registerInlineCompletionItemProvider({ pattern: "**" }, provider),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("lilia.agentkit.status", async () => {
      const status = await hostStatus();
      output.appendLine(JSON.stringify(status, null, 2));
      output.show(true);
      await vscode.window.showInformationMessage(
        status.ok
          ? `AgentKit host ready (requiresLiliaCore=${status.status?.requiresLiliaCore === true})`
          : `AgentKit host error: ${status.error ?? "unknown"}`,
      );
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("lilia.agentkit.nextEdit", async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) {
        await vscode.window.showWarningMessage("No active editor");
        return;
      }
      const result = await planNextEditFromDocument(editor.document);
      output.appendLine(JSON.stringify(result, null, 2));
      output.show(true);
      if (!result.ok) {
        await vscode.window.showErrorMessage(result.error ?? "next edit failed");
        return;
      }
      const proposalId = result.candidate?.proposal?.proposalId;
      await vscode.window.showInformationMessage(
        proposalId
          ? `Next Edit proposal ${proposalId} (WorkspaceEdit path; not applied automatically)`
          : "Next Edit returned no candidate",
      );
    }),
  );

  return { provider };
}

export function deactivate() {}

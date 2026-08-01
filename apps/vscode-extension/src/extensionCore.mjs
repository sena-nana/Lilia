/**
 * VS Code Extension core (#40) — injectable `vscode` for behavior tests.
 */

import {
  completeFromDocument,
  disposeHostClients,
  hostStatus,
  planNextEditFromDocument,
  readHostConfig,
  recordNextEditFeedback,
  validateNextEdit,
} from "./hostClient.mjs";

let activeHostConfig = null;

/**
 * @param {typeof import('vscode')} vscode
 * @param {{ subscriptions: { push(...items: unknown[]): void } }} context
 * @param {{ hostConfig?: { hostMode: string, hostBinary: string } }} options
 */
export function activate(vscode, context, options = {}) {
  const output = vscode.window.createOutputChannel("Lilia AgentKit");
  context.subscriptions.push(output);
  activeHostConfig = options.hostConfig ?? configFromVscode(vscode);

  const provider = {
    /**
     * @param {import('vscode').TextDocument} document
     * @param {import('vscode').Position} position
     * @param {import('vscode').InlineCompletionContext} _context
     * @param {import('vscode').CancellationToken} token
     */
    async provideInlineCompletionItems(document, position, _context, token) {
      const controller = new AbortController();
      const cancellation = token?.onCancellationRequested?.(() => controller.abort());
      try {
        const result = await completeFromDocument(
          document,
          position,
          activeHostConfig,
          { signal: controller.signal },
        );
        if (!result.ok || !result.insertText || token?.isCancellationRequested) {
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
      } finally {
        cancellation?.dispose?.();
      }
    },
  };

  context.subscriptions.push(
    vscode.languages.registerInlineCompletionItemProvider({ pattern: "**" }, provider),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("lilia.agentkit.status", async () => {
      const status = await hostStatus(activeHostConfig);
      output.appendLine(JSON.stringify(status, null, 2));
      output.show(true);
      await vscode.window.showInformationMessage(
        status.ok
          ? "AgentKit coding services are ready"
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
      const result = await planNextEditFromDocument(
        editor.document,
        activeHostConfig,
      );
      output.appendLine(JSON.stringify(result, null, 2));
      output.show(true);
      if (!result.ok) {
        await vscode.window.showErrorMessage(result.error ?? "Next Edit failed");
        return;
      }
      const candidate = result.candidate;
      if (!candidate) {
        await vscode.window.showInformationMessage("No Next Edit suggestion");
        return;
      }
      const editCount = candidate.proposal?.changes?.reduce(
        (count, change) => count + (change.edits?.length ?? 0),
        0,
      );
      if (!editCount) {
        await recordNextEditFeedback(
          candidate.candidateId,
          "rejected",
          "missing_concrete_edits",
          activeHostConfig,
        );
        await vscode.window.showErrorMessage(
          "Next Edit did not return an applicable edit",
        );
        return;
      }

      const action = await vscode.window.showInformationMessage(
        candidate.proposal.summary ?? "Apply the suggested edit?",
        { modal: candidate.requiresPreview === true },
        "Apply",
      );
      if (action !== "Apply") {
        await recordNextEditFeedback(
          candidate.candidateId,
          "skipped",
          "user_dismissed",
          activeHostConfig,
        );
        return;
      }

      const applied = await applyCandidate(vscode, candidate);
      if (!applied.ok) {
        await recordNextEditFeedback(
          candidate.candidateId,
          "rejected",
          applied.reasonCode,
          activeHostConfig,
        );
        await vscode.window.showWarningMessage(applied.error);
        return;
      }
      await recordNextEditFeedback(
        candidate.candidateId,
        "accepted",
        null,
        activeHostConfig,
      );
      await vscode.window.showInformationMessage("Next Edit applied");
    }),
  );

  return { provider };
}

async function applyCandidate(vscode, candidate) {
  const changes = candidate.proposal?.changes ?? [];
  const documents = [];
  for (const change of changes) {
    const uri = vscode.Uri.parse(change.document.uri);
    const document = await vscode.workspace.openTextDocument(uri);
    if (document.version !== change.baseVersion) {
      return {
        ok: false,
        reasonCode: "stale_document",
        error: "The document changed before Next Edit could be applied",
      };
    }
    documents.push([change, uri, document]);
  }

  const validation = await validateNextEdit(
    candidate.candidateId,
    documents.map(([change]) => [change.document, change.baseVersion]),
    null,
    activeHostConfig,
  );
  if (!validation.valid) {
    return {
      ok: false,
      reasonCode: "stale_candidate",
      error: "The Next Edit suggestion is stale",
    };
  }

  const workspaceEdit = new vscode.WorkspaceEdit();
  for (const [change, uri] of documents) {
    for (const edit of change.edits) {
      workspaceEdit.replace(
        uri,
        new vscode.Range(
          edit.range.start.line,
          edit.range.start.character,
          edit.range.end.line,
          edit.range.end.character,
        ),
        edit.newText,
      );
    }
  }
  const applied = await vscode.workspace.applyEdit(workspaceEdit);
  return applied
    ? { ok: true }
    : {
        ok: false,
        reasonCode: "workspace_edit_rejected",
        error: "VS Code did not apply the Next Edit suggestion",
      };
}

function configFromVscode(vscode) {
  const configuration = vscode.workspace?.getConfiguration?.(
    "lilia.agentkit",
  );
  return readHostConfig(process.env, {
    hostMode: configuration?.get?.("hostMode"),
    hostBinary: configuration?.get?.("hostBinary"),
  });
}

export function deactivate() {
  activeHostConfig = null;
  disposeHostClients();
}

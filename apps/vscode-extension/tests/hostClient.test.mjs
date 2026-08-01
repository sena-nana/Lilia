import assert from "node:assert/strict";
import {
  complete,
  completeFromDocument,
  hostStatus,
  planNextEdit,
  planNextEditFromDocument,
  readHostConfig,
} from "../src/hostClient.mjs";

function fakeDocument(text = "fn main() {", languageId = "rust") {
  return {
    uri: { toString: () => "file:///workspace/main.rs" },
    languageId,
    getText: () => text,
    lineAt: () => ({ text }),
    offsetAt: (pos) => pos.character,
  };
}

async function main() {
  const production = readHostConfig({});
  assert.equal(production.hostMode, "process");
  assert.equal(production.hostBinary, "lilia-editor-compat");

  const config = readHostConfig({ LILIA_AGENTKIT_HOST_MODE: "deterministic" });
  assert.equal(config.hostMode, "deterministic");

  const status = await hostStatus(config);
  assert.equal(status.ok, true);
  assert.equal(status.status.requiresLiliaCore, false);
  assert.equal(status.status.requiresOfficialAgentServer, false);
  assert.equal(status.status.requiresNodeAgentRunner, false);
  assert.equal(
    status.status.completionService,
    "mutsuki.agent.service.code-completion",
  );
  assert.equal(status.status.nextEditService, "mutsuki.agent.service.next-edit");

  const completion = await complete(
    {
      uri: "file:///workspace/main.rs",
      languageId: "rust",
      prefix: "fn main() {",
      suffix: "\n",
      generation: 1,
    },
    config,
  );
  assert.equal(completion.ok, true);
  assert.ok(completion.insertText.length > 0);

  const fromDoc = await completeFromDocument(
    fakeDocument("let x ="),
    { line: 0, character: 7 },
    config,
  );
  assert.equal(fromDoc.ok, true);

  const nextEdit = await planNextEdit(
    {
      uri: "file:///workspace/main.rs",
      languageId: "rust",
      content: "fn main() {}",
      summary: "opened function body",
      generation: 1,
    },
    config,
  );
  assert.equal(nextEdit.ok, true);
  assert.ok(nextEdit.candidate?.proposal?.proposalId);
  assert.equal(nextEdit.candidate.proposal.changes.length, 1);
  assert.equal(nextEdit.candidate.proposal.changes[0].edits.length, 1);

  const nextFromDoc = await planNextEditFromDocument(fakeDocument("fn main() {}"), config);
  assert.equal(nextFromDoc.ok, true);

  console.log("[vscode-extension hostClient] OK — Completion + Next Edit without LiliaCore");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

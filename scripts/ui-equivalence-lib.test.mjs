import assert from "node:assert/strict";
import test from "node:test";

import { assertEquivalent, firstDifference } from "./ui-equivalence-lib.mjs";

test("firstDifference reports the first stable nested field", () => {
  assert.deepEqual(
    firstDifference({ tasks: [{ id: "task", revision: 1 }] }, { tasks: [{ id: "task", revision: 2 }] }),
    { location: "snapshot.tasks[0].revision", left: 1, right: 2 },
  );
});

test("assertEquivalent accepts structurally identical snapshots", () => {
  assert.doesNotThrow(() =>
    assertEquivalent(
      { schemaVersion: 1, composers: [{ contentSha256: "abc" }] },
      { schemaVersion: 1, composers: [{ contentSha256: "abc" }] },
      "fixture",
    ),
  );
});

test("requestNativeDebug rejects malformed addresses before opening a socket", async () => {
  const { requestNativeDebug } = await import("./ui-equivalence-lib.mjs");
  await assert.rejects(
    requestNativeDebug("missing-port", { command: "observe" }),
    /Invalid Native debug address/,
  );
});

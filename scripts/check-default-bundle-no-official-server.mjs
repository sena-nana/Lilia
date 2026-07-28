#!/usr/bin/env node
/**
 * #47 acceptance gate: default Desktop package must not declare official
 * Codex app-server / Node agent-runner resources in tauri.conf.json.
 *
 * Legacy scripts may remain in the source tree for explicit
 * `LILIA_AGENT_EXECUTION_BACKEND=node` until product version 1.0.0.
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import process from "node:process";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const tauriConfPath = path.join(
  repoRoot,
  "apps",
  "desktop",
  "src-tauri",
  "tauri.conf.json",
);

const forbiddenResourceKeys = [
  "codex-account-quota.mjs",
  "agent-runner.mjs",
  "agent-runner/codex/accountQuota.mjs",
  "agent-runner/codex/appServer.mjs",
];

const forbiddenSourceSubstrings = [
  "appServer.mjs",
  "accountQuota.mjs",
  "codex-account-quota.mjs",
  "agent-runner.mjs",
];

function fail(message) {
  console.error(`[check-default-bundle-no-official-server] FAIL: ${message}`);
  process.exitCode = 1;
}

function main() {
  const raw = readFileSync(tauriConfPath, "utf8");
  const conf = JSON.parse(raw);
  const resources = conf?.bundle?.resources ?? {};
  const resourceValues = Object.values(resources).map(String);
  const resourceKeys = Object.keys(resources);

  if (resourceKeys.length > 0) {
    for (const key of resourceKeys) {
      const mapped = String(resources[key]);
      for (const forbidden of forbiddenSourceSubstrings) {
        if (key.includes(forbidden) || mapped.includes(forbidden)) {
          fail(
            `tauri.conf.json bundle.resources still maps official/legacy runtime: ${key} → ${mapped}`,
          );
        }
      }
    }
  }

  for (const name of forbiddenResourceKeys) {
    if (resourceValues.includes(name) || resourceKeys.some((k) => k.endsWith(name))) {
      fail(`forbidden packaged resource still declared: ${name}`);
    }
  }

  if (process.exitCode) {
    process.exit(process.exitCode);
  }
  console.log(
    "[check-default-bundle-no-official-server] OK: default tauri resources exclude official Agent Server / Node agent-runner",
  );
}

main();

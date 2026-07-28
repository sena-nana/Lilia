#!/usr/bin/env node
/**
 * #47 gate: default tree must not keep Node agent-runner / official Server at
 * Desktop root paths. Sources live under apps/desktop/legacy/ and require
 * Cargo feature `legacy-runner` to link.
 */
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import process from "node:process";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));

const forbiddenAtDesktopRoot = [
  "apps/desktop/agent-runner.mjs",
  "apps/desktop/agent-runner",
  "apps/desktop/claude-history.mjs",
  "apps/desktop/codex-history.mjs",
  "apps/desktop/codex-account-quota.mjs",
];

const requiredUnderLegacy = [
  "apps/desktop/legacy/LEGACY.md",
  "apps/desktop/legacy/agent-runner.mjs",
  "apps/desktop/legacy/agent-runner/codex/appServer.mjs",
  "apps/desktop/legacy/claude-history.mjs",
  "apps/desktop/legacy/codex-history.mjs",
  "apps/desktop/legacy/codex-account-quota.mjs",
];

function fail(message) {
  console.error(`[check-legacy-default-unreachable] FAIL: ${message}`);
  process.exitCode = 1;
}

function main() {
  for (const rel of forbiddenAtDesktopRoot) {
    const abs = path.join(repoRoot, rel);
    if (existsSync(abs)) {
      fail(`default Desktop path still present (must live under legacy/): ${rel}`);
    }
  }
  for (const rel of requiredUnderLegacy) {
    const abs = path.join(repoRoot, rel);
    if (!existsSync(abs)) {
      fail(`expected legacy source missing: ${rel}`);
    }
  }

  const cargoToml = readFileSync(
    path.join(repoRoot, "apps/desktop/src-tauri/Cargo.toml"),
    "utf8",
  );
  if (!cargoToml.includes("legacy-runner")) {
    fail("apps/desktop/src-tauri/Cargo.toml must declare feature `legacy-runner`");
  }
  // Default features must not enable legacy-runner.
  const defaultMatch = cargoToml.match(/default\s*=\s*\[([^\]]*)\]/);
  if (defaultMatch && defaultMatch[1].includes("legacy-runner")) {
    fail("feature `legacy-runner` must not be in default features");
  }

  const nativeAgent = readFileSync(
    path.join(repoRoot, "apps/desktop/src-tauri/src/native_agent.rs"),
    "utf8",
  );
  if (!nativeAgent.includes('cfg(feature = "legacy-runner")')) {
    fail("native_agent.rs must feature-gate NodeAgentRunner behind legacy-runner");
  }

  const runner = readFileSync(
    path.join(repoRoot, "apps/desktop/src-tauri/src/chat/runner.rs"),
    "utf8",
  );
  if (!runner.includes('cfg(feature = "legacy-runner")')) {
    fail("chat/runner.rs must feature-gate locate_agent_runner / Node path");
  }
  if (runner.includes('../../../agent-runner.mjs"') && !runner.includes("legacy/agent-runner")) {
    fail("locate_agent_runner must resolve apps/desktop/legacy/agent-runner.mjs");
  }

  if (process.exitCode) {
    process.exit(process.exitCode);
  }
  console.log(
    "[check-legacy-default-unreachable] OK: legacy sources under apps/desktop/legacy/; default feature off",
  );
}

main();

#!/usr/bin/env node
/**
 * #47 gate: `locate_agent_runner` must only be reachable from explicit legacy paths.
 *
 * Allowed call sites (and only when Cargo feature `legacy-runner` is enabled):
 * - definition in chat/runner.rs
 * - NodeAgentRunner branch in chat/runner.rs
 * - Codex Spark Node branch (gated by LILIA_AGENT_EXECUTION_BACKEND=node)
 *
 * History utilities must NOT call locate_agent_runner (default install has no agent-runner.mjs).
 * Default Desktop root must not host agent-runner sources (see check-legacy-default-unreachable.mjs).
 */
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import process from "node:process";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const srcRoot = path.join(repoRoot, "apps", "desktop", "src-tauri", "src");

const allowedFiles = new Set([
  path.join(srcRoot, "chat", "runner.rs"),
  path.join(srcRoot, "provider", "codex_spark.rs"),
]);

function walkRs(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const abs = path.join(dir, name);
    const st = statSync(abs);
    if (st.isDirectory()) {
      if (name === "target") continue;
      walkRs(abs, out);
    } else if (name.endsWith(".rs")) {
      out.push(abs);
    }
  }
  return out;
}

function fail(message) {
  console.error(`[check-legacy-runner-reachability] FAIL: ${message}`);
  process.exitCode = 1;
}

function main() {
  const files = walkRs(srcRoot);
  const hits = [];
  for (const file of files) {
    const text = readFileSync(file, "utf8");
    if (!text.includes("locate_agent_runner")) continue;
    hits.push(file);
    if (!allowedFiles.has(file)) {
      fail(
        `locate_agent_runner referenced outside allowed legacy gates: ${path.relative(repoRoot, file)}`,
      );
    }
  }

  // Codex Spark must gate Node path behind ExecutionBackend::NodeAgentRunner.
  const spark = readFileSync(path.join(srcRoot, "provider", "codex_spark.rs"), "utf8");
  if (!spark.includes("ExecutionBackend::NodeAgentRunner")) {
    fail("codex_spark.rs must gate Node runner behind ExecutionBackend::NodeAgentRunner");
  }
  if (!spark.includes("LEGACY_NODE_RUNNER_COMPAT_UNTIL")) {
    fail("codex_spark.rs must mention LEGACY_NODE_RUNNER_COMPAT_UNTIL");
  }
  if (!spark.includes('cfg(feature = "legacy-runner")')) {
    fail("codex_spark.rs must feature-gate Node path behind legacy-runner");
  }

  // Chat runner must only call locate after NodeAgentRunner match (feature-gated).
  const runner = readFileSync(path.join(srcRoot, "chat", "runner.rs"), "utf8");
  if (!runner.includes("ExecutionBackend::NodeAgentRunner")) {
    fail("chat/runner.rs must keep NodeAgentRunner branch for locate_agent_runner");
  }
  if (!runner.includes('cfg(feature = "legacy-runner")')) {
    fail("chat/runner.rs must feature-gate Node agent-runner behind legacy-runner");
  }
  if (!runner.includes("legacy/agent-runner")) {
    fail("locate_agent_runner must look under apps/desktop/legacy/");
  }

  // Default tree isolation.
  const desktopRoot = path.join(repoRoot, "apps", "desktop");
  for (const name of [
    "agent-runner.mjs",
    "agent-runner",
    "claude-history.mjs",
    "codex-history.mjs",
    "codex-account-quota.mjs",
  ]) {
    if (existsSync(path.join(desktopRoot, name))) {
      fail(`legacy source still at Desktop root: apps/desktop/${name}`);
    }
  }
  if (!existsSync(path.join(desktopRoot, "legacy", "agent-runner.mjs"))) {
    fail("expected apps/desktop/legacy/agent-runner.mjs");
  }

  if (process.exitCode) {
    process.exit(process.exitCode);
  }
  console.log(
    `[check-legacy-runner-reachability] OK: locate_agent_runner confined to ${hits.length} gated file(s)`,
  );
  for (const file of hits) {
    console.log(`  - ${path.relative(repoRoot, file)}`);
  }
}

main();

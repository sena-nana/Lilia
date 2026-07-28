#!/usr/bin/env node
/**
 * #47 — mark / inspect Legacy Node agent-runner + official app-server paths.
 *
 * Usage:
 *   node scripts/mark-legacy-agent-runner.mjs           # inspect + report
 *   node scripts/mark-legacy-agent-runner.mjs --check   # fail if markers missing
 */
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import process from "node:process";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const COMPAT_UNTIL = "1.0.0";
const MARKER = "#47 LEGACY";
const checkMode = process.argv.includes("--check");

const legacyEntries = [
  {
    id: "legacy-root",
    path: "apps/desktop/legacy",
    role: "Isolated legacy tree (default build does not package)",
    dir: true,
  },
  {
    id: "agent-runner-entry",
    path: "apps/desktop/legacy/agent-runner.mjs",
    role: "Node agent-runner CLI entry (feature legacy-runner + explicit env)",
  },
  {
    id: "agent-runner-dir",
    path: "apps/desktop/legacy/agent-runner",
    role: "Legacy Claude/Codex Node runtime modules",
    dir: true,
  },
  {
    id: "codex-app-server",
    path: "apps/desktop/legacy/agent-runner/codex/appServer.mjs",
    role: "Official Codex app-server launcher (must not ship in default install)",
  },
  {
    id: "codex-account-quota",
    path: "apps/desktop/legacy/codex-account-quota.mjs",
    role: "Codex account quota helper via app-server (source-only after #47)",
  },
  {
    id: "claude-history",
    path: "apps/desktop/legacy/claude-history.mjs",
    role: "Legacy Claude history utility",
  },
  {
    id: "codex-history",
    path: "apps/desktop/legacy/codex-history.mjs",
    role: "Legacy Codex history utility",
  },
];

const forbiddenAtDesktopRoot = [
  "apps/desktop/agent-runner.mjs",
  "apps/desktop/agent-runner",
  "apps/desktop/claude-history.mjs",
  "apps/desktop/codex-history.mjs",
  "apps/desktop/codex-account-quota.mjs",
];

function walkFiles(root, out = []) {
  if (!existsSync(root)) return out;
  const st = statSync(root);
  if (st.isFile()) {
    out.push(root);
    return out;
  }
  for (const name of readdirSync(root)) {
    if (name === "node_modules" || name === "dist") continue;
    walkFiles(path.join(root, name), out);
  }
  return out;
}

function hasMarker(filePath) {
  try {
    const text = readFileSync(filePath, "utf8").slice(0, 4000);
    return text.includes(MARKER) || text.includes("LEGACY_NODE_RUNNER_COMPAT_UNTIL");
  } catch {
    return false;
  }
}

function main() {
  const rows = [];
  let failures = 0;

  for (const rel of forbiddenAtDesktopRoot) {
    if (existsSync(path.join(repoRoot, rel))) {
      failures += 1;
      console.error(
        `[mark-legacy-agent-runner] forbidden default path still present: ${rel}`,
      );
    }
  }

  for (const entry of legacyEntries) {
    const abs = path.join(repoRoot, entry.path);
    const exists = existsSync(abs);
    let marked = false;
    let fileCount = 0;
    if (!exists) {
      failures += 1;
      rows.push({ ...entry, exists: false, marked: false, fileCount: 0 });
      continue;
    }
    if (entry.dir) {
      const files = walkFiles(abs).filter((f) => f.endsWith(".mjs") || f.endsWith(".md"));
      fileCount = files.length;
      const legacyMd = path.join(abs, "LEGACY.md");
      marked =
        existsSync(legacyMd) ||
        files.slice(0, 8).some((f) => hasMarker(f));
    } else {
      fileCount = 1;
      marked = hasMarker(abs);
    }
    if (checkMode && !marked) failures += 1;
    rows.push({ ...entry, exists: true, marked, fileCount });
  }

  console.log(`Legacy Node runner / official Server inventory (#47)`);
  console.log(`compatUntil: ${COMPAT_UNTIL}`);
  console.log(`defaultInstallBundlesOfficialServer: false`);
  console.log(`defaultInstallBundlesNodeAgentRunner: false`);
  console.log(`legacySourceRoot: apps/desktop/legacy/`);
  console.log(`cargoFeature: legacy-runner (default off)`);
  console.log("");
  for (const row of rows) {
    const status = !row.exists
      ? "MISSING"
      : row.marked
        ? "LEGACY-MARKED"
        : "NEEDS-MARKER";
    console.log(
      `- [${status}] ${row.path} (${row.role}; files=${row.fileCount})`,
    );
  }

  if (checkMode && failures > 0) {
    console.error(
      `\n[mark-legacy-agent-runner] ${failures} path(s) missing, unmarked, or still at Desktop root`,
    );
    process.exit(1);
  }
  console.log("\n[mark-legacy-agent-runner] done");
}

main();

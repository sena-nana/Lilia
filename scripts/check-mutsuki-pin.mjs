import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const upstream = "https://github.com/sena-nana/Mutsuki.git";

function read(relativePath) {
  return readFileSync(resolve(root, relativePath), "utf8");
}

function fail(message) {
  console.error(`[check-mutsuki-pin] ${message}`);
  process.exitCode = 1;
}

const manifest = read("Cargo.toml");
const workspaceDependencies = manifest.match(
  /\[workspace\.dependencies\]([\s\S]*?)(?=\n\[|$)/,
)?.[1];
if (!workspaceDependencies) {
  throw new Error("Cargo.toml is missing [workspace.dependencies]");
}

const dependencyLines = workspaceDependencies
  .split("\n")
  .map((line) => line.trim())
  .filter((line) => /^mutsuki-[a-z0-9-]+\s*=/.test(line));
const revisions = new Map();
for (const line of dependencyLines) {
  const name = line.match(/^(mutsuki-[a-z0-9-]+)\s*=/)?.[1];
  const git = line.match(/\bgit\s*=\s*"([^"]+)"/)?.[1];
  const revision = line.match(/\brev\s*=\s*"([0-9a-f]{40})"/)?.[1];
  if (!name || git !== upstream || !revision) {
    fail(`workspace dependency must use the canonical Mutsuki git + full rev: ${line}`);
    continue;
  }
  revisions.set(name, revision);
}

const uniqueRevisions = new Set(revisions.values());
if (revisions.size === 0 || revisions.size !== dependencyLines.length) {
  fail("not every active mutsuki-* workspace dependency has a valid git pin");
}
if (uniqueRevisions.size !== 1) {
  fail(`mutsuki-* workspace dependencies use mixed revisions: ${[...uniqueRevisions].join(", ")}`);
}
const [revision] = uniqueRevisions;

const productCore = read("apps/desktop/src-tauri/src/product_core.rs");
const productPin = productCore.match(/mutsuki_core_pin:\s*"([0-9a-f]{40})"/)?.[1];
if (productPin !== revision) {
  fail(`product_core.rs reports ${productPin ?? "no pin"}, expected ${revision}`);
}

const pinDocument = read("docs/design/mutsuki-dependency-pin.md");
const documentedRevisions = new Set(pinDocument.match(/[0-9a-f]{40}/g) ?? []);
if (documentedRevisions.size !== 1 || !documentedRevisions.has(revision)) {
  fail(
    `dependency-pin.md must document only ${revision}; found ${[...documentedRevisions].join(", ") || "none"}`,
  );
}

const lockfile = read("Cargo.lock");
const lockPins = [
  ...lockfile.matchAll(
    /git\+https:\/\/github\.com\/sena-nana\/Mutsuki\.git\?rev=([0-9a-f]{40})#([0-9a-f]{40})/g,
  ),
];
if (lockPins.length === 0) {
  fail("Cargo.lock has no resolved Mutsuki git source");
}
for (const [, declared, resolved] of lockPins) {
  if (declared !== revision || resolved !== revision) {
    fail(`Cargo.lock resolves Mutsuki ${declared}#${resolved}, expected ${revision}`);
    break;
  }
}

if (!process.exitCode) {
  console.log(
    `[check-mutsuki-pin] OK: ${revisions.size} dependencies, lockfile, product status, and docs use ${revision}`,
  );
}

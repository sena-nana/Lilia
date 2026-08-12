import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const upstream = "https://github.com/sena-nana/NanaUI.git";

function read(relativePath) {
  return readFileSync(resolve(root, relativePath), "utf8");
}

function fail(message) {
  console.error(`[check-nanaui-pin] ${message}`);
  process.exitCode = 1;
}

const manifest = read("apps/native-desktop/Cargo.toml");
const dependency = manifest
  .split("\n")
  .map((line) => line.trim())
  .find((line) => /^nana-ui\s*=/.test(line));

if (!dependency) {
  throw new Error("apps/native-desktop/Cargo.toml is missing nana-ui");
}
if (/\bpath\s*=/.test(dependency)) {
  fail("Native Preview must not use a local NanaUI path dependency");
}

const git = dependency.match(/\bgit\s*=\s*"([^"]+)"/)?.[1];
const revision = dependency.match(/\brev\s*=\s*"([0-9a-f]{40})"/)?.[1];
if (git !== upstream || !revision) {
  fail(`nana-ui must use ${upstream} with a full 40-character rev`);
}

const metadataResult = spawnSync(
  "cargo",
  ["metadata", "--locked", "--format-version", "1"],
  {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
    maxBuffer: 64 * 1024 * 1024,
  },
);
if (metadataResult.error) {
  throw metadataResult.error;
}
if (metadataResult.status !== 0) {
  fail(`cargo metadata --locked failed:\n${metadataResult.stderr.trim()}`);
} else {
  const metadata = JSON.parse(metadataResult.stdout);
  const resolvedNanaUi = metadata.packages.filter((entry) => entry.name === "nana-ui");
  if (
    resolvedNanaUi.length !== 1
    || resolvedNanaUi[0].source !== `git+${upstream}?rev=${revision}#${revision}`
  ) {
    fail(
      `resolved nana-ui must be the canonical Git revision; found ${resolvedNanaUi.map((entry) => entry.source ?? "path").join(", ") || "none"}`,
    );
  }
}

const lockfile = read("Cargo.lock");
const lockPins = [
  ...lockfile.matchAll(
    /git\+https:\/\/github\.com\/sena-nana\/NanaUI\.git\?rev=([0-9a-f]{40})#([0-9a-f]{40})/g,
  ),
];
if (lockPins.length === 0) {
  fail("Cargo.lock has no resolved NanaUI git source");
}
for (const [, declared, resolved] of lockPins) {
  if (declared !== revision || resolved !== revision) {
    fail(`Cargo.lock resolves NanaUI ${declared}#${resolved}, expected ${revision}`);
    break;
  }
}

if (!process.exitCode) {
  console.log(`[check-nanaui-pin] OK: manifest and lockfile use ${revision}`);
}

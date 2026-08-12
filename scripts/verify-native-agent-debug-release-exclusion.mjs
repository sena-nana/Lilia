#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const debugMarkers = [
  "LILIA_NATIVE_AGENT_DEBUG",
  "LILIA_NATIVE_AGENT_DEBUG_ADDR",
  "LILIA_NATIVE_AGENT_DEBUG_READY",
  "Native Agent debug service",
  "the UI did not answer the debug command",
  "recent-errors",
];

const binaries = resolveBinaries(process.argv.slice(2));
const scans = binaries.map((binary) => {
  const contents = fs.readFileSync(binary);
  return {
    binary,
    size: contents.byteLength,
    matches: findDebugMarkers(contents),
  };
});
const matches = scans.flatMap((scan) =>
  scan.matches.map((match) => ({ binary: scan.binary, ...match })),
);

if (matches.length > 0) {
  console.error(
    JSON.stringify(
      {
        success: false,
        binaries: scans.map(({ binary, size }) => ({ binary, size })),
        matches,
      },
      null,
      2,
    ),
  );
  process.exit(1);
}

console.log(
  JSON.stringify(
    {
      success: true,
      binaries: scans.map(({ binary, size }) => ({ binary, size })),
      markersChecked: debugMarkers.length,
      encodingsChecked: ["utf8", "utf16le"],
    },
    null,
    2,
  ),
);

function resolveBinaries(args) {
  const candidates = [];
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument !== "--binary") {
      throw new Error(`Unknown argument: ${argument}`);
    }
    const value = args[index + 1];
    if (!value) {
      throw new Error("--binary requires a path");
    }
    candidates.push(path.resolve(repoRoot, value));
    index += 1;
  }
  if (candidates.length === 0) {
    candidates.push(
      path.join(repoRoot, "target", "release", "lilia-native-preview.exe"),
      path.join(repoRoot, "target", "release", "lilia_native_host.dll"),
    );
  }
  for (const candidate of candidates) {
    if (!fs.existsSync(candidate)) {
      throw new Error(`Native Preview release artifact does not exist: ${candidate}`);
    }
    const stats = fs.statSync(candidate);
    if (!stats.isFile() || stats.size === 0) {
      throw new Error(`Native Preview release artifact is not a non-empty file: ${candidate}`);
    }
  }
  return [...new Set(candidates)];
}

function findDebugMarkers(contents) {
  const matches = [];
  for (const marker of debugMarkers) {
    const encodings = [];
    if (contents.indexOf(Buffer.from(marker, "utf8")) !== -1) {
      encodings.push("utf8");
    }
    if (contents.indexOf(Buffer.from(marker, "utf16le")) !== -1) {
      encodings.push("utf16le");
    }
    if (encodings.length > 0) {
      matches.push({ marker, encodings });
    }
  }
  return matches;
}

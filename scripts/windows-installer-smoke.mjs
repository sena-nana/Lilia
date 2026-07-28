import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawn, spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const processName = "lilia.exe";
const defaultTimeoutMs = 300_000;
// #47: default NSIS install must NOT ship official Agent Server / Node agent-runner.
const forbiddenOfficialRuntimeFiles = [
  "codex-account-quota.mjs",
  "agent-runner.mjs",
  path.join("agent-runner", "codex", "accountQuota.mjs"),
  path.join("agent-runner", "codex", "appServer.mjs"),
];

function getArgValue(name) {
  const index = process.argv.indexOf(name);
  if (index === -1) return undefined;
  return process.argv[index + 1];
}

function log(message) {
  console.log(`[release:smoke:windows] ${message}`);
}

function bufferOutput(buffer, source, chunk) {
  buffer.push(`[${source}] ${chunk.toString("utf8")}`);
  if (buffer.length > 200) {
    buffer.splice(0, buffer.length - 200);
  }
}

function logBufferedOutput(label, buffer) {
  if (buffer.length === 0) {
    log(`${label}: no output captured`);
    return;
  }
  log(`${label}:`);
  for (const line of buffer.join("").split(/\r?\n/).filter(Boolean).slice(-80)) {
    log(`  ${line}`);
  }
}

function fail(message) {
  throw new Error(message);
}

function run(command, args, options = {}) {
  const shell = options.shell ?? false;
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repoRoot,
    env: options.env ?? process.env,
    encoding: "utf8",
    shell,
    stdio: options.stdio ?? "pipe",
    windowsHide: true,
  });
  if (result.error) {
    fail(`${command} failed to start: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const stdout = result.stdout?.trim();
    const stderr = result.stderr?.trim();
    fail(`${command} ${args.join(" ")} exited ${result.status}.${stdout ? `\nstdout:\n${stdout}` : ""}${stderr ? `\nstderr:\n${stderr}` : ""}`);
  }
  return result;
}

function readWindowsPath(scope) {
  const result = run("powershell.exe", [
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-Command",
    `[Environment]::GetEnvironmentVariable('Path', '${scope}')`,
  ]);
  return result.stdout.trim();
}

function freshWindowsEnv(extra = {}) {
  const machinePath = readWindowsPath("Machine");
  const userPath = readWindowsPath("User");
  return {
    ...process.env,
    Path: [machinePath, userPath].filter(Boolean).join(";"),
    PATH: [machinePath, userPath].filter(Boolean).join(";"),
    ...extra,
  };
}

function waitUntil(description, predicate, timeoutMs = defaultTimeoutMs) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (predicate()) return;
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  fail(`Timed out waiting for ${description}.`);
}

function isProcessRunning() {
  const result = spawnSync("tasklist.exe", ["/FI", `IMAGENAME eq ${processName}`, "/NH"], {
    encoding: "utf8",
    stdio: "pipe",
    windowsHide: true,
  });
  if (result.error || result.status !== 0) return false;
  return result.stdout.toLowerCase().includes(processName.toLowerCase());
}

function stopAppIfRunning() {
  if (!isProcessRunning()) return;
  spawnSync("taskkill.exe", ["/F", "/IM", processName, "/T"], {
    encoding: "utf8",
    stdio: "pipe",
    windowsHide: true,
  });
  waitUntil(`${processName} to exit`, () => !isProcessRunning(), 15_000);
}

function pathKey(value) {
  let resolved = path.resolve(value);
  try {
    resolved = fs.realpathSync.native(resolved);
  } catch {
  }
  return resolved.replace(/[\\/]+$/, "").toLowerCase();
}

function quoteCmdArg(value) {
  return `"${value.replace(/"/g, '""')}"`;
}

function resolveLiliacode(env) {
  const result = spawnSync("cmd.exe", ["/d", "/s", "/c", "where liliacode"], {
    env,
    encoding: "utf8",
    stdio: "pipe",
    windowsHide: true,
  });
  if (result.status !== 0) return [];
  return result.stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function displayPath(value) {
  if (value.startsWith("\\\\?\\UNC\\")) return `\\\\${value.slice("\\\\?\\UNC\\".length)}`;
  if (value.startsWith("\\\\?\\")) return value.slice("\\\\?\\".length);
  return value;
}

function fileContainsText(filePath, needles) {
  try {
    const data = fs.readFileSync(filePath);
    return needles.some((needle) => data.includes(Buffer.from(needle, "utf8")));
  } catch {
    return false;
  }
}

function projectPathNeedles(projectPath) {
  const candidates = [path.resolve(projectPath)];
  try {
    candidates.push(fs.realpathSync.native(projectPath));
  } catch {
  }
  return Array.from(new Set(candidates.flatMap((candidate) => {
    const normalized = displayPath(candidate);
    return [normalized, normalized.replaceAll("/", "\\")];
  })));
}

function storeContainsProjectPath(liliaHome, projectPath) {
  const dbDir = path.join(liliaHome, "db");
  const dbFiles = ["lilia.db", "lilia.db-wal"].map((name) => path.join(dbDir, name));
  const needles = projectPathNeedles(projectPath);
  return dbFiles.some((filePath) => fileContainsText(filePath, needles));
}

function logStoreDiagnostics(label, liliaHome, projectPath) {
  const dbDir = path.join(liliaHome, "db");
  const dbFiles = ["lilia.db", "lilia.db-wal"].map((name) => path.join(dbDir, name));
  log(`${label} CLI project path candidates: ${projectPathNeedles(projectPath).join(" | ")}`);
  for (const filePath of dbFiles) {
    if (!fs.existsSync(filePath)) {
      log(`Store file missing: ${filePath}`);
      continue;
    }
    const stat = fs.statSync(filePath);
    log(`Store file size: ${filePath} (${stat.size} bytes)`);
  }
}

function storeDbPath(liliaHome) {
  return path.join(liliaHome, "db", "lilia.db");
}

function waitForProjectInStore(label, liliaHome, projectPath, appOutput) {
  try {
    waitUntil(`${label} CLI project path in the Lilia store`, () => storeContainsProjectPath(liliaHome, projectPath));
  } catch (err) {
    logBufferedOutput("Installed app output", appOutput);
    logStoreDiagnostics(label, liliaHome, projectPath);
    throw err;
  }
  log(`${label} CLI project path reached the Lilia store`);
}

function ensureInstallerPath() {
  const explicitInstaller = getArgValue("--installer") ?? process.env.RELEASE_INSTALLER_PATH;
  if (explicitInstaller) {
    const installer = path.resolve(explicitInstaller);
    if (!fs.existsSync(installer)) fail(`Installer not found: ${installer}`);
    return installer;
  }

  const tag = getArgValue("--tag") ?? process.env.RELEASE_TAG ?? process.env.GITHUB_REF_NAME;
  if (!tag) {
    fail("Pass --installer <path> for a local smoke run, or --tag <tag> to download from a draft Release.");
  }

  const downloadDir = path.join(os.tmpdir(), `lilia-release-smoke-assets-${process.pid}`);
  fs.mkdirSync(downloadDir, { recursive: true });
  log(`Downloading draft Release installer for ${tag} into ${downloadDir}`);
  const ghEnv = {
    ...process.env,
    GH_TOKEN: process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN,
  };
  const ghArgs = ["release", "download", tag, "--pattern", "LiliaCode_*_x64-setup.exe", "--dir", downloadDir, "--clobber"];
  const repo = getArgValue("--repo") ?? process.env.GITHUB_REPOSITORY;
  if (repo) ghArgs.push("--repo", repo);
  run("gh.exe", ghArgs, { env: ghEnv });

  const installers = fs.readdirSync(downloadDir)
    .filter((name) => /^LiliaCode_.+_x64-setup\.exe$/i.test(name))
    .map((name) => path.join(downloadDir, name));
  if (installers.length !== 1) {
    fail(`Expected exactly one LiliaCode_*_x64-setup.exe in ${downloadDir}, found ${installers.length}.`);
  }
  return installers[0];
}

function ensureTestProjects() {
  const explicitProject = getArgValue("--test-project");
  const ownedProjectRoot = path.join(os.tmpdir(), `lilia release smoke projects ${process.pid}`);
  const initialProject = explicitProject
    ? path.resolve(explicitProject)
    : path.join(ownedProjectRoot, "initial project with spaces");
  const secondProject = path.join(ownedProjectRoot, "second project with spaces");
  for (const projectPath of [initialProject, secondProject]) {
    fs.mkdirSync(projectPath, { recursive: true });
    fs.writeFileSync(path.join(projectPath, ".lilia-smoke"), "release smoke\n", "utf8");
  }
  return {
    initialProject,
    secondProject,
    ownedProjectRoot,
  };
}

function main() {
  if (process.argv.includes("--help")) {
    console.log("Usage: yarn release:smoke:windows --installer <path> [--test-project <path>]");
    console.log("   or: yarn release:smoke:windows --tag <tag> [--repo owner/repo]");
    return;
  }

  if (process.platform !== "win32") {
    fail("Windows installer smoke can only run on Windows.");
  }
  if (isProcessRunning()) {
    fail(`${processName} is already running. Close it before running this smoke script so cleanup is scoped to this run.`);
  }

  const installer = ensureInstallerPath();
  const installDir = path.join(os.tmpdir(), `lilia-release-smoke-install-${process.pid}`);
  const liliaHome = path.join(os.tmpdir(), `lilia-release-smoke-home-${process.pid}`);
  const {
    initialProject,
    secondProject,
    ownedProjectRoot,
  } = ensureTestProjects();
  const installedExe = path.join(installDir, processName);
  const cliCmd = path.join(installDir, "liliacode.cmd");
  const uninstaller = path.join(installDir, "uninstall.exe");
  let installed = false;
  const appOutput = [];

  log(`Installer: ${installer}`);
  log(`Install dir: ${installDir}`);
  log(`LILIA_HOME: ${liliaHome}`);
  log(`Initial CLI project: ${initialProject}`);
  log(`Second-instance CLI project: ${secondProject}`);

  try {
    const preInstallCli = resolveLiliacode(freshWindowsEnv());
    if (preInstallCli.length > 0) {
      fail(`liliacode already resolves before smoke install, so CLI cleanup cannot be scoped to this run:\n${preInstallCli.join("\n")}`);
    }

    log("Installing silently");
    run(installer, ["/S", `/D=${installDir}`]);
    installed = true;
    waitUntil("installed lilia.exe", () => fs.existsSync(installedExe));
    waitUntil("installed liliacode.cmd", () => fs.existsSync(cliCmd));
    for (const filename of forbiddenOfficialRuntimeFiles) {
      const resourcePath = path.join(installDir, filename);
      if (fs.existsSync(resourcePath)) {
        fail(
          `default install must not bundle official/legacy agent runtime (#47): ${filename}`,
        );
      }
    }
    log("Default install does not bundle official Agent Server / Node agent-runner");

    const runtimeEnv = freshWindowsEnv({ LILIA_HOME: liliaHome });
    const installedCli = resolveLiliacode(runtimeEnv);
    if (!installedCli.some((candidate) => pathKey(candidate) === pathKey(cliCmd))) {
      fail(`A fresh cmd environment did not resolve this smoke install's liliacode.cmd:\n${installedCli.join("\n")}`);
    }
    log("CLI command is available from fresh PATH");

    log("Launching installed app through liliacode with a spaced project path");
    const app = spawn("cmd.exe", ["/d", "/s", "/c", `liliacode ${quoteCmdArg(initialProject)}`], {
      env: runtimeEnv,
      detached: false,
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: false,
    });
    app.stdout?.on("data", (chunk) => bufferOutput(appOutput, "stdout", chunk));
    app.stderr?.on("data", (chunk) => bufferOutput(appOutput, "stderr", chunk));
    app.unref();
    waitUntil(`${processName} to start`, () => isProcessRunning());
    log("Installed app process started");
    waitUntil("Lilia store database initialization", () => fs.existsSync(storeDbPath(liliaHome)));
    log("Lilia store database initialized");
    waitForProjectInStore("Initial launch", liliaHome, initialProject, appOutput);

    log("Opening second project through liliacode while the app is already running");
    run("cmd.exe", ["/d", "/s", "/c", `liliacode ${quoteCmdArg(secondProject)}`], { env: runtimeEnv });
    waitForProjectInStore("Second-instance", liliaHome, secondProject, appOutput);

    log("Stopping installed app before uninstall");
    stopAppIfRunning();

    log("Uninstalling silently");
    waitUntil("uninstaller.exe", () => fs.existsSync(uninstaller));
    run(uninstaller, ["/S"]);
    waitUntil("liliacode.cmd removal", () => !fs.existsSync(cliCmd), 30_000);
    installed = false;

    const afterUninstallCli = resolveLiliacode(freshWindowsEnv({ LILIA_HOME: liliaHome }));
    if (afterUninstallCli.length > 0) {
      fail(`liliacode still resolves after uninstall:\n${afterUninstallCli.join("\n")}`);
    }
    log("CLI command is cleaned from fresh PATH after uninstall");
    log("Windows installer smoke passed");
  } finally {
    stopAppIfRunning();
    if (installed && fs.existsSync(uninstaller)) {
      spawnSync(uninstaller, ["/S"], { encoding: "utf8", stdio: "pipe", windowsHide: true });
    }
    fs.rmSync(liliaHome, { recursive: true, force: true });
    if (fs.existsSync(installDir)) {
      fs.rmSync(installDir, { recursive: true, force: true });
    }
    fs.rmSync(ownedProjectRoot, { recursive: true, force: true });
  }
}

try {
  main();
} catch (err) {
  console.error(`[release:smoke:windows] ${err.message}`);
  process.exitCode = 1;
}

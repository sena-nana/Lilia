import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawn, spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const nativePreview = process.argv.includes("--native-preview");
const product = nativePreview
  ? {
      label: "Native Preview",
      processName: "liliacode-native-preview.exe",
      cliCommand: "liliacode-native",
      cliFile: "liliacode-native.cmd",
      requiredFiles: ["lilia_native_host.dll"],
      installerPattern: "LiliaCodeNativePreview_*_x64-setup.exe",
      installerRegex: /^LiliaCodeNativePreview_.+_x64-setup\.exe$/i,
      homeEnv: "LILIA_NATIVE_PREVIEW_HOME",
      databaseFiles: ["product.db", "product.db-wal"],
      primaryDatabase: "product.db",
    }
  : {
      label: "LiliaCode",
      processName: "lilia.exe",
      cliCommand: "liliacode",
      cliFile: "liliacode.cmd",
      requiredFiles: [],
      installerPattern: "LiliaCode_*_x64-setup.exe",
      installerRegex: /^LiliaCode_.+_x64-setup\.exe$/i,
      homeEnv: "LILIA_HOME",
      databaseFiles: ["lilia.db", "lilia.db-wal"],
      primaryDatabase: "lilia.db",
    };
const processName = product.processName;
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
  console.log(`[release:smoke:windows:${nativePreview ? "native-preview" : "stable"}] ${message}`);
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

function runAsync(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd ?? repoRoot,
      env: options.env ?? process.env,
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });
    let stdout = "";
    let stderr = "";
    child.stdout?.on("data", (chunk) => { stdout += chunk.toString("utf8"); });
    child.stderr?.on("data", (chunk) => { stderr += chunk.toString("utf8"); });
    child.once("error", reject);
    child.once("exit", (code) => {
      if (code === 0) {
        resolve({ stdout, stderr });
      } else {
        reject(new Error(`${command} ${args.join(" ")} exited ${code ?? "unknown"}.${stdout.trim() ? `\nstdout:\n${stdout.trim()}` : ""}${stderr.trim() ? `\nstderr:\n${stderr.trim()}` : ""}`));
      }
    });
  });
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

function processRecords() {
  const result = spawnSync("tasklist.exe", ["/FO", "CSV", "/NH"], {
    encoding: "utf8",
    stdio: "pipe",
    windowsHide: true,
  });
  if (result.error || result.status !== 0) return [];
  return result.stdout
    .split(/\r?\n/)
    .map((line) => /^"([^"]+)","(\d+)"/.exec(line))
    .filter(Boolean)
    .map((match) => ({ imageName: match[1], pid: Number.parseInt(match[2], 10) }));
}

async function waitUntilAsync(description, predicate, timeoutMs = defaultTimeoutMs) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  fail(`Timed out waiting for ${description}.`);
}

function matchingProcessRecords() {
  const expected = processName.toLowerCase();
  return processRecords().filter((record) => record.imageName.toLowerCase() === expected);
}

function isProcessRunning() {
  return matchingProcessRecords().length > 0;
}

function isPidRunning(pid) {
  return processRecords().some((record) => record.pid === pid);
}

function stopAppIfRunning() {
  const records = matchingProcessRecords();
  for (const record of records) {
    spawnSync("taskkill.exe", ["/F", "/PID", String(record.pid), "/T"], {
      encoding: "utf8",
      stdio: "pipe",
      windowsHide: true,
    });
  }
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

function resolveCli(env) {
  const result = spawnSync("cmd.exe", ["/d", "/s", "/c", `where ${product.cliCommand}`], {
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

function storeContainsProjectPath(appHome, projectPath) {
  const dbDir = path.join(appHome, "db");
  const dbFiles = product.databaseFiles.map((name) => path.join(dbDir, name));
  const needles = projectPathNeedles(projectPath);
  return dbFiles.some((filePath) => fileContainsText(filePath, needles));
}

function storeContainsText(appHome, value) {
  const dbDir = path.join(appHome, "db");
  return product.databaseFiles
    .map((name) => path.join(dbDir, name))
    .some((filePath) => fileContainsText(filePath, [value]));
}

function logStoreDiagnostics(label, appHome, projectPath) {
  const dbDir = path.join(appHome, "db");
  const dbFiles = product.databaseFiles.map((name) => path.join(dbDir, name));
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

function storeDbPath(appHome) {
  return path.join(appHome, "db", product.primaryDatabase);
}

function waitForProjectInStore(label, appHome, projectPath, appOutput) {
  try {
    waitUntil(`${label} CLI project path in the product store`, () => storeContainsProjectPath(appHome, projectPath));
  } catch (err) {
    logBufferedOutput("Installed app output", appOutput);
    logStoreDiagnostics(label, appHome, projectPath);
    throw err;
  }
  log(`${label} CLI project path reached the product store`);
}

async function runNativeOverwriteUpdate(installer, runtimeEnv, sourcePid) {
  const output = [];
  let exited = false;
  let exitCode = null;
  const update = spawn(installer, ["/passive", "/UPDATE", `/UPDATEPID=${sourcePid}`], {
    env: runtimeEnv,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  update.stdout?.on("data", (chunk) => bufferOutput(output, "stdout", chunk));
  update.stderr?.on("data", (chunk) => bufferOutput(output, "stderr", chunk));
  update.once("exit", (code) => {
    exited = true;
    exitCode = code;
  });
  await new Promise((resolve) => setTimeout(resolve, 1_000));
  if (exited) {
    logBufferedOutput("Overwrite update installer output", output);
    fail(`Overwrite update installer exited ${exitCode ?? "unknown"} before the source Native process stopped.`);
  }

  const stop = spawnSync("taskkill.exe", ["/F", "/PID", String(sourcePid), "/T"], {
    encoding: "utf8",
    stdio: "pipe",
    windowsHide: true,
  });
  if (stop.status !== 0 && isPidRunning(sourcePid)) {
    fail(`Failed to stop source Native process ${sourcePid} for overwrite update: ${stop.stderr?.trim() ?? "unknown error"}`);
  }
  await waitUntilAsync("source Native process to exit for overwrite update", () => !isPidRunning(sourcePid), 15_000);
  await waitUntilAsync("overwrite update installer to finish", () => exited, 90_000);
  if (exitCode !== 0) {
    logBufferedOutput("Overwrite update installer output", output);
    fail(`Overwrite update installer exited ${exitCode ?? "unknown"}.`);
  }
  await waitUntilAsync(
    "updated Native Preview process to restart",
    () => matchingProcessRecords().some((record) => record.pid !== sourcePid),
    30_000,
  );
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

  const downloadDir = path.join(os.tmpdir(), `lilia-release-smoke-assets-${nativePreview ? "native-" : ""}${process.pid}`);
  fs.mkdirSync(downloadDir, { recursive: true });
  log(`Downloading draft Release installer for ${tag} into ${downloadDir}`);
  const ghEnv = {
    ...process.env,
    GH_TOKEN: process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN,
  };
  const ghArgs = ["release", "download", tag, "--pattern", product.installerPattern, "--dir", downloadDir, "--clobber"];
  const repo = getArgValue("--repo") ?? process.env.GITHUB_REPOSITORY;
  if (repo) ghArgs.push("--repo", repo);
  run("gh.exe", ghArgs, { env: ghEnv });

  const installers = fs.readdirSync(downloadDir)
    .filter((name) => product.installerRegex.test(name))
    .map((name) => path.join(downloadDir, name));
  if (installers.length !== 1) {
    fail(`Expected exactly one ${product.installerPattern} in ${downloadDir}, found ${installers.length}.`);
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
  const concurrentProjects = Array.from(
    { length: 10 },
    (_, index) => path.join(ownedProjectRoot, `concurrent project ${String(index + 1).padStart(2, "0")}`),
  );
  for (const projectPath of [initialProject, secondProject, ...concurrentProjects]) {
    fs.mkdirSync(projectPath, { recursive: true });
    fs.writeFileSync(path.join(projectPath, ".lilia-smoke"), "release smoke\n", "utf8");
  }
  const handoffId = `native-installer-smoke-handoff-${process.pid}`;
  const handoffPath = path.join(ownedProjectRoot, "task handoff with spaces.json");
  fs.writeFileSync(handoffPath, `${JSON.stringify({
    protocol: "lilia-code-task-handoff",
    version: 1,
    id: handoffId,
    createdAt: "2026-08-10T00:00:00Z",
    title: "验证 Native Preview 安装后任务交接",
    kind: "repository",
    repository: {
      fullName: "sena-nana/LiliaCode",
      worktreePath: secondProject,
      branch: "native-preview-smoke",
    },
    source: {
      application: "LiliaGithub",
      route: "/repositories/sena-nana/LiliaCode",
    },
    problem: "验证独立 CLI 将版本化 handoff 转发给正在运行的 Native Preview。",
    relatedFiles: [],
    acceptanceCriteria: ["生成 accepted 回执", "Product Core 持久化 handoff"],
  }, null, 2)}\n`, "utf8");
  return {
    initialProject,
    secondProject,
    concurrentProjects,
    ownedProjectRoot,
    handoffId,
    handoffPath,
    handoffReceiptPath: `${handoffPath}.receipt.json`,
  };
}

async function main() {
  if (process.argv.includes("--help")) {
    console.log("Usage: yarn release:smoke:windows [--native-preview] --installer <path> [--test-project <path>]");
    console.log("   or: yarn release:smoke:windows [--native-preview] --tag <tag> [--repo owner/repo]");
    return;
  }

  if (process.platform !== "win32") {
    fail("Windows installer smoke can only run on Windows.");
  }
  if (isProcessRunning()) {
    fail(`${processName} is already running. Close it before running this smoke script so cleanup is scoped to this run.`);
  }

  const installer = ensureInstallerPath();
  const installDir = path.join(os.tmpdir(), `lilia-release-smoke-install-${nativePreview ? "native-" : ""}${process.pid}`);
  const appHome = path.join(os.tmpdir(), `lilia-release-smoke-home-${nativePreview ? "native-" : ""}${process.pid}`);
  const {
    initialProject,
    secondProject,
    concurrentProjects,
    ownedProjectRoot,
    handoffId,
    handoffPath,
    handoffReceiptPath,
  } = ensureTestProjects();
  const installedExe = path.join(installDir, processName);
  const cliCmd = path.join(installDir, product.cliFile);
  const uninstaller = path.join(installDir, "uninstall.exe");
  let installed = false;
  const appOutput = [];

  log(`Installer: ${installer}`);
  log(`Install dir: ${installDir}`);
  log(`${product.homeEnv}: ${appHome}`);
  log(`Initial CLI project: ${initialProject}`);
  log(`Second-instance CLI project: ${secondProject}`);
  log(`Task handoff: ${handoffPath}`);

  try {
    const preInstallCli = resolveCli(freshWindowsEnv());
    if (preInstallCli.length > 0) {
      fail(`${product.cliCommand} already resolves before smoke install, so CLI cleanup cannot be scoped to this run:\n${preInstallCli.join("\n")}`);
    }

    log("Installing silently");
    run(installer, ["/S", `/D=${installDir}`]);
    installed = true;
    waitUntil(`installed ${processName}`, () => fs.existsSync(installedExe));
    waitUntil(`installed ${product.cliFile}`, () => fs.existsSync(cliCmd));
    for (const filename of product.requiredFiles) {
      waitUntil(`installed ${filename}`, () => fs.existsSync(path.join(installDir, filename)));
    }
    for (const filename of forbiddenOfficialRuntimeFiles) {
      const resourcePath = path.join(installDir, filename);
      if (fs.existsSync(resourcePath)) {
        fail(
          `default install must not bundle official/legacy agent runtime (#47): ${filename}`,
        );
      }
    }
    log("Default install does not bundle official Agent Server / Node agent-runner");

    const runtimeEnv = freshWindowsEnv({ [product.homeEnv]: appHome });
    const installedCli = resolveCli(runtimeEnv);
    if (!installedCli.some((candidate) => pathKey(candidate) === pathKey(cliCmd))) {
      fail(`A fresh cmd environment did not resolve this smoke install's ${product.cliFile}:\n${installedCli.join("\n")}`);
    }
    log("CLI command is available from fresh PATH");

    log(`Launching installed app with a spaced project path`);
    const app = nativePreview
      ? spawn(installedExe, [initialProject], {
          env: runtimeEnv,
          detached: false,
          stdio: ["ignore", "pipe", "pipe"],
          windowsHide: false,
        })
      : spawn("cmd.exe", ["/d", "/s", "/c", product.cliCommand, initialProject], {
          env: runtimeEnv,
          detached: false,
          stdio: ["ignore", "pipe", "pipe"],
          windowsHide: false,
        });
    app.stdout?.on("data", (chunk) => bufferOutput(appOutput, "stdout", chunk));
    app.stderr?.on("data", (chunk) => bufferOutput(appOutput, "stderr", chunk));
    let appExited = false;
    let appExitCode = null;
    app.once("exit", (code) => {
      appExited = true;
      appExitCode = code;
    });
    app.unref();
    await waitUntilAsync(
      `${processName} to start or the CLI process to exit`,
      () => (nativePreview ? isPidRunning(app.pid) : isProcessRunning()) || app.exitCode !== null,
      30_000,
    );
    await new Promise((resolve) => setTimeout(resolve, 2_000));
    if (!(nativePreview ? isPidRunning(app.pid) : isProcessRunning())) {
      await waitUntilAsync("installed app exit event", () => appExited, 5_000).catch(() => {});
      logBufferedOutput("Installed app output", appOutput);
      fail(`${processName} exited during startup (CLI exit code ${appExitCode ?? app.exitCode ?? "unknown"}).`);
    }
    log("Installed app process started");
    waitUntil("product store database initialization", () => fs.existsSync(storeDbPath(appHome)));
    log("Product store database initialized");
    waitForProjectInStore("Initial launch", appHome, initialProject, appOutput);

    log(`Opening second project through ${product.cliCommand} while the app is already running`);
    run("cmd.exe", ["/d", "/s", "/c", product.cliCommand, secondProject], { env: runtimeEnv });
    waitForProjectInStore("Second-instance", appHome, secondProject, appOutput);

    if (nativePreview) {
      log("Forwarding 10 concurrent project requests through independent CLI processes");
      await Promise.all(concurrentProjects.map((projectPath) => runAsync(
        "cmd.exe",
        ["/d", "/s", "/c", product.cliCommand, projectPath],
        { env: runtimeEnv },
      )));
      for (const projectPath of concurrentProjects) {
        waitForProjectInStore("Concurrent single-instance", appHome, projectPath, appOutput);
      }
      log("All 10 concurrent CLI requests reached the shared Product Core");

      log(`Forwarding a versioned task handoff through ${product.cliCommand}`);
      run("cmd.exe", ["/d", "/s", "/c", product.cliCommand, "--task-handoff", handoffPath], { env: runtimeEnv });
      waitUntil("accepted task handoff receipt", () => {
        if (!fs.existsSync(handoffReceiptPath)) return false;
        try {
          const receipt = JSON.parse(fs.readFileSync(handoffReceiptPath, "utf8"));
          return receipt.protocol === "lilia-code-task-handoff"
            && receipt.version === 1
            && receipt.handoffId === handoffId
            && receipt.status === "accepted"
            && typeof receipt.taskId === "string"
            && receipt.taskId.length > 0
            && typeof receipt.projectId === "string"
            && receipt.projectId.length > 0;
        } catch {
          return false;
        }
      });
      waitUntil("task handoff in the product store", () => storeContainsText(appHome, handoffId));
      log("Versioned task handoff receipt and Product Core record verified");

      log("Running overwrite update with source PID wait and automatic restart");
      await runNativeOverwriteUpdate(installer, runtimeEnv, app.pid);
      waitUntil("product store after overwrite update", () => fs.existsSync(storeDbPath(appHome)));
      log("Overwrite update waited for the source process and restarted Native Preview");
    }

    log("Stopping installed app before uninstall");
    stopAppIfRunning();

    log("Uninstalling silently");
    waitUntil("uninstaller.exe", () => fs.existsSync(uninstaller));
    run(uninstaller, ["/S"]);
    waitUntil(`${product.cliFile} removal`, () => !fs.existsSync(cliCmd), 90_000);
    installed = false;

    const afterUninstallCli = resolveCli(freshWindowsEnv({ [product.homeEnv]: appHome }));
    if (afterUninstallCli.length > 0) {
      fail(`${product.cliCommand} still resolves after uninstall:\n${afterUninstallCli.join("\n")}`);
    }
    log("CLI command is cleaned from fresh PATH after uninstall");
    log("Windows installer smoke passed");
  } finally {
    stopAppIfRunning();
    if (installed && fs.existsSync(uninstaller)) {
      spawnSync(uninstaller, ["/S"], { encoding: "utf8", stdio: "pipe", windowsHide: true });
    }
    fs.rmSync(appHome, { recursive: true, force: true });
    if (fs.existsSync(installDir)) {
      fs.rmSync(installDir, { recursive: true, force: true });
    }
    fs.rmSync(ownedProjectRoot, { recursive: true, force: true });
  }
}

main().catch((err) => {
  console.error(`[release:smoke:windows:${nativePreview ? "native-preview" : "stable"}] ${err.message}`);
  process.exitCode = 1;
});

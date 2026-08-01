import { spawn, spawnSync } from "node:child_process";
import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  createAgentDebugChildEnv,
  createAgentDebugDevServerPlan,
} from "./dev-server.mjs";

function resolveRepoRoot() {
  try {
    return path.resolve(fileURLToPath(new URL("../../..", import.meta.url)));
  } catch {
    let cursor = process.cwd();
    while (true) {
      if (existsSync(path.join(cursor, "package.json")) && existsSync(path.join(cursor, "apps", "desktop"))) {
        return cursor;
      }
      const next = path.dirname(cursor);
      if (next === cursor) return process.cwd();
      cursor = next;
    }
  }
}

const repoRoot = resolveRepoRoot();
const runsRoot = path.join(repoRoot, "agent-debug-runs");
const runId = new Date().toISOString().replace(/[:.]/g, "-");
const runDir = path.join(runsRoot, runId);
const driverPort = Number.parseInt(process.env.LILIA_AGENT_DEBUG_DRIVER_PORT ?? "4444", 10);
const driverUrl = `http://127.0.0.1:${driverPort}`;
const appBinary = process.env.LILIA_AGENT_DEBUG_APP ??
  path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", process.platform === "win32" ? "lilia.exe" : "lilia");
const toolDir = process.env.LILIA_AGENT_DEBUG_TOOL_DIR ??
  path.join(os.homedir(), ".lilia", "agent-debug", "bin");
const DEFAULT_NEXT_STEP = "Fix the preflight error, build the debug desktop binary, or set LILIA_AGENT_DEBUG_APP.";
let restoreAgentInteractionSettings = null;

const CHAT_SEND_MESSAGE_COMMAND = "chat_send_message";
const SLOW_INVOKE_WARNING_MS = 1000;
const SLOW_INVOKE_HIGH_MS = 5000;
const SEND_ERROR_TEXT_MARKERS = ["发生错误", "发送失败"];
const NATIVE_QUOTA_SURFACE_COMMAND = "native_quota_surface";
const PROVIDER_BLOCK_ERROR_MARKERS = [
  "辅助模型未配置",
  "Base URL",
  "API key",
  "Codex app-server 环境不满足",
  "Provider 设置",
  "Responses API",
];
const SCENARIOS = {
  ordinarySend: {
    id: "ordinary-send",
    label: "普通发送",
    artifact: "scenario-ordinary-send.png",
  },
  continueHistory: {
    id: "continue-history-session",
    label: "继续历史会话",
    artifact: "scenario-continue-history-session.png",
  },
  taskRecovery: {
    id: "task-route-recovery",
    label: "任务恢复",
    artifact: "scenario-task-route-recovery.png",
  },
  planPendingAction: {
    id: "plan-pending-action",
    label: "plan pending action",
    artifact: "scenario-plan-pending-action.png",
  },
  permissionPendingAction: {
    id: "permission-pending-action",
    label: "permission pending action",
    artifact: "scenario-permission-pending-action.png",
  },
  nativeQuotaSurface: {
    id: "native-quota-surface",
    label: "Native Provider quota diagnostics",
    artifact: "scenario-native-quota-surface.png",
  },
};
const requestedScenarioId = (process.env.LILIA_AGENT_DEBUG_SCENARIO ?? "").trim();
const SETTINGS_SURFACE_TABS = [
  "appearance",
  "window",
  "providers",
  "remote-control",
  "assistant",
  "model-config",
  "agent",
  "quota",
  "plugin-skills",
  "plugin-packages",
  "plugin-hooks",
  "plugin-mcp",
  "import",
  "project",
  "about",
];

class SmokeBlockedError extends Error {
  constructor(message, details = {}) {
    super(message);
    this.name = "SmokeBlockedError";
    this.details = details;
  }
}

class AgentDebugRunArtifacts {
  constructor({ runDir, runId }) {
    this.runDir = runDir;
    this.runId = runId;
    this.replay = [];
    this.scenarioResults = [];
    this.activeScenario = null;
  }

  path(name) {
    return path.join(this.runDir, name);
  }

  async writeJson(name, value) {
    const target = this.path(name);
    await writeFile(target, `${JSON.stringify(value, null, 2)}\n`, "utf8");
    return target;
  }

  async writeText(name, value) {
    const target = this.path(name);
    await writeFile(target, value, "utf8");
    return target;
  }

  beginScenario(scenario) {
    this.activeScenario = scenario;
  }

  appendScenarioResult(scenario, status, details = {}) {
    const result = {
      id: scenario.id,
      label: scenario.label,
      status,
      ...details,
    };
    this.scenarioResults.push(result);
    if (this.activeScenario?.id === scenario.id) {
      this.activeScenario = null;
    }
    return result;
  }

  ensureActiveScenarioResult(status, details = {}) {
    const scenario = this.activeScenario;
    if (!scenario) return null;
    if (this.scenarioResults.some((item) => item.id === scenario.id)) {
      this.activeScenario = null;
      return null;
    }
    return this.appendScenarioResult(scenario, status, details);
  }

  async writeRunArtifacts({ logs = [], driverOutput = [], devServerOutput = [] } = {}) {
    const logsPath = await this.writeJson("logs.json", logs);
    const replayPath = await this.writeJson("replay.json", this.replay);
    const scenarioResultsPath = await this.writeJson("scenario-results.json", this.scenarioResults);
    const driverLogPath = await this.writeText("tauri-driver.log", driverOutput.join(""));
    const devServerLogPath = await this.writeText("dev-server.log", devServerOutput.join(""));
    return {
      logsPath,
      replayPath,
      scenarioResultsPath,
      driverLogPath,
      devServerLogPath,
    };
  }
}

const artifacts = new AgentDebugRunArtifacts({ runDir, runId });
const replay = artifacts.replay;
const scenarioResults = artifacts.scenarioResults;

function commandExists(command, args = ["--version"]) {
  const result = spawnSync(command, args, { stdio: "ignore" });
  return result.status === 0;
}

function prependPath(dir) {
  const delimiter = process.platform === "win32" ? ";" : ":";
  const current = process.env.PATH ?? "";
  if (current.split(delimiter).some((entry) => path.resolve(entry) === path.resolve(dir))) return;
  process.env.PATH = `${dir}${delimiter}${current}`;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    ...options,
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed: ${result.stderr || result.stdout || result.error?.message || result.status}`);
  }
  return result.stdout.trim();
}

function runPowerShell(script) {
  return run("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script]);
}

async function downloadFile(url, target) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Download failed ${response.status}: ${url}`);
  }
  await writeFile(target, Buffer.from(await response.arrayBuffer()));
}

function parseVersion(text) {
  return text.match(/\d+\.\d+\.\d+\.\d+/)?.[0] ?? null;
}

function versionPrefix(version) {
  return version.split(".").slice(0, 3).join(".");
}

function edgeDriverVersion() {
  const result = spawnSync("msedgedriver", ["--version"], { encoding: "utf8" });
  if (result.status !== 0) return null;
  return parseVersion(`${result.stdout}\n${result.stderr}`);
}

function edgeVersion() {
  if (process.env.LILIA_AGENT_DEBUG_EDGE_VERSION) return process.env.LILIA_AGENT_DEBUG_EDGE_VERSION;
  if (process.platform !== "win32") return null;
  const script = [
    "$patterns = @(",
    "'C:\\Program Files (x86)\\Microsoft\\EdgeWebView\\Application\\*\\msedgewebview2.exe',",
    "'C:\\Program Files\\Microsoft\\EdgeWebView\\Application\\*\\msedgewebview2.exe',",
    "'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',",
    "'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe'",
    ");",
    "foreach ($pattern in $patterns) {",
    "  $items = Get-ChildItem -Path $pattern -ErrorAction SilentlyContinue | Sort-Object -Property FullName -Descending;",
    "  foreach ($item in $items) {",
    "    if ($item.VersionInfo.ProductVersion) { $item.VersionInfo.ProductVersion; exit 0 }",
    "  }",
    "}",
    "exit 1",
  ].join(" ");
  try {
    return parseVersion(runPowerShell(script));
  } catch {
    return null;
  }
}

async function installEdgeDriver(version, setup) {
  if (process.platform !== "win32") {
    throw new Error("Automatic EdgeDriver install is currently supported on Windows only.");
  }
  await mkdir(toolDir, { recursive: true });
  const tempDir = path.join(os.tmpdir(), `lilia-edgedriver-${version}-${Date.now()}`);
  const zipPath = path.join(tempDir, "edgedriver_win64.zip");
  const extractDir = path.join(tempDir, "extract");
  await mkdir(extractDir, { recursive: true });
  try {
    const url = `https://msedgedriver.microsoft.com/${version}/edgedriver_win64.zip`;
    await downloadFile(url, zipPath);
    runPowerShell(`Expand-Archive -LiteralPath '${zipPath.replace(/'/g, "''")}' -DestinationPath '${extractDir.replace(/'/g, "''")}' -Force`);
    const extracted = runPowerShell(`Get-ChildItem -Path '${extractDir.replace(/'/g, "''")}' -Recurse -Filter 'msedgedriver.exe' | Select-Object -First 1 -ExpandProperty FullName`);
    await copyFile(extracted, path.join(toolDir, "msedgedriver.exe"));
    prependPath(toolDir);
    setup.push(`installed EdgeDriver ${version}`);
  } finally {
    await rm(tempDir, { recursive: true, force: true });
  }
}

async function ensureAgentDebugTools() {
  const setup = [];
  const cargoBin = path.join(os.homedir(), ".cargo", "bin");
  prependPath(cargoBin);
  prependPath(toolDir);

  if (!commandExists("tauri-driver", ["--help"])) {
    if (!commandExists("cargo")) {
      throw new Error("cargo is required to auto-install tauri-driver.");
    }
    run("cargo", ["install", "tauri-driver"], { stdio: "pipe" });
    prependPath(cargoBin);
    setup.push("installed tauri-driver");
  }

  const browserVersion = edgeVersion();
  const driverVersion = edgeDriverVersion();
  const matchesEdge = browserVersion && driverVersion &&
    versionPrefix(browserVersion) === versionPrefix(driverVersion);
  if (!matchesEdge) {
    if (!browserVersion) {
      throw new Error("Microsoft Edge version could not be detected. Set LILIA_AGENT_DEBUG_EDGE_VERSION to install EdgeDriver automatically.");
    }
    await installEdgeDriver(browserVersion, setup);
  }

  return {
    setup,
    edgeVersion: browserVersion,
    edgeDriverVersion: edgeDriverVersion(),
    toolDir,
  };
}

async function writeJson(name, value) {
  return await artifacts.writeJson(name, value);
}

async function writeRunSummary(
  runArtifacts,
  {
    status,
    preflight,
    driverOutput = [],
    devServerOutput = [],
    logs = [],
    reason,
    message,
    details,
    nextStep,
    beforeScreenshotPath = null,
    afterScreenshotPath = null,
    failureScreenshotPath = null,
    failureDiagnosticsPath = null,
    observePath,
    slowInvokes = [],
  },
) {
  const paths = await runArtifacts.writeRunArtifacts({
    logs,
    driverOutput,
    devServerOutput,
  });
  const summary = {
    status,
    runId: runArtifacts.runId,
    preflight,
    ...(reason === undefined ? {} : { reason }),
    ...(message === undefined ? {} : { message }),
    ...(details === undefined ? {} : { details }),
    ...(nextStep === undefined ? {} : { nextStep }),
    ...(beforeScreenshotPath ? { beforeScreenshotPath } : {}),
    ...(afterScreenshotPath ? { afterScreenshotPath } : {}),
    ...(observePath ? { observePath } : {}),
    ...paths,
    failureScreenshotPath,
    failureDiagnosticsPath,
    slowInvokes,
    screenshots: {
      before: beforeScreenshotPath,
      after: afterScreenshotPath,
      failure: failureScreenshotPath,
    },
    replay: runArtifacts.replay,
    scenarios: runArtifacts.scenarioResults,
  };
  await runArtifacts.writeJson("summary.json", summary);
  return summary;
}

function request(method, pathname, body) {
  const payload = body === undefined ? null : JSON.stringify(body);
  return new Promise((resolve, reject) => {
    const req = http.request(
      `${driverUrl}${pathname}`,
      {
        method,
        headers: payload
          ? { "content-type": "application/json", "content-length": Buffer.byteLength(payload) }
          : undefined,
      },
      (res) => {
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () => {
          const text = Buffer.concat(chunks).toString("utf8");
          let json = null;
          try {
            json = text ? JSON.parse(text) : null;
          } catch {
            // Keep raw text in the error below.
          }
          if (res.statusCode && res.statusCode >= 200 && res.statusCode < 300) {
            resolve(json);
          } else {
            reject(new Error(`${method} ${pathname} failed: ${res.statusCode} ${text}`));
          }
        });
      },
    );
    req.on("error", reject);
    if (payload) req.write(payload);
    req.end();
  });
}

function getUrl(url) {
  return new Promise((resolve, reject) => {
    const req = http.get(url, (res) => {
      res.resume();
      res.on("end", () => {
        if (res.statusCode && res.statusCode >= 200 && res.statusCode < 400) {
          resolve();
        } else {
          reject(new Error(`GET ${url} failed: ${res.statusCode}`));
        }
      });
    });
    req.on("error", reject);
    req.setTimeout(1000, () => {
      req.destroy(new Error(`GET ${url} timed out`));
    });
  });
}

async function waitForUrl(url, timeoutMs, label, child = null, output = []) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (child && child.exitCode !== null) {
      throw new Error(`${label} exited before ready: ${output.join("").trim() || child.exitCode}`);
    }
    try {
      await getUrl(url);
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  }
  throw new Error(`${label} did not become ready within ${timeoutMs}ms`);
}

async function isUrlReady(url) {
  try {
    await getUrl(url);
    return true;
  } catch {
    return false;
  }
}

function stopProcessTree(child) {
  if (!child || child.killed) return;
  if (process.platform === "win32" && child.pid) {
    spawnSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore" });
    return;
  }
  child.kill();
}

function spawnYarn(args, options) {
  if (process.platform === "win32") {
    return spawn("cmd.exe", ["/d", "/s", "/c", "yarn", ...args], options);
  }
  return spawn("yarn", args, options);
}

async function waitForDriver() {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    try {
      await request("GET", "/status");
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  }
  throw new Error("tauri-driver did not become ready within 15s");
}

async function execute(sessionId, script, args = []) {
  const result = await request("POST", `/session/${sessionId}/execute/sync`, { script, args });
  return result?.value;
}

async function waitForDebugApi(sessionId) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const ready = await execute(
      sessionId,
      "return Boolean(window.__liliaAgentDebug?.observe?.().enabled);",
    ).catch(() => false);
    if (ready) return;
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error("window.__liliaAgentDebug did not become enabled within 30s");
}

async function waitForDebugUi(sessionId) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const ready = await execute(
      sessionId,
      `const observe = window.__liliaAgentDebug?.observe?.();
       return Boolean(observe?.elements?.some((element) =>
         element.id === "app.shell" ||
         element.id === "sidebar" ||
         element.id === "titlebar"
       ));`,
    ).catch(() => false);
    if (ready) return;
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error("agent debug UI tree did not become ready within 30s");
}

async function screenshot(sessionId, name) {
  const result = await request("GET", `/session/${sessionId}/screenshot`);
  const target = path.join(runDir, name);
  await writeFile(target, Buffer.from(result.value, "base64"));
  return target;
}

function elementById(snapshot, id) {
  return snapshot?.elements?.find((element) => element.id === id) ?? null;
}

function visibleEnabledElement(snapshot, id) {
  const element = elementById(snapshot, id);
  return element?.visible && element?.enabled ? element : null;
}

function visibleElement(snapshot, id) {
  const element = elementById(snapshot, id);
  return element?.visible ? element : null;
}

async function observe(sessionId) {
  return await execute(sessionId, "return window.__liliaAgentDebug?.observe?.() ?? null;");
}

async function waitForCondition(sessionId, label, predicate, timeoutMs = 15_000) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await observe(sessionId).catch((error) => ({ error: String(error?.message ?? error) }));
    if (predicate(last)) return last;
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
  throw new Error(`${label} did not become ready within ${timeoutMs}ms`);
}

async function waitForAgentElement(sessionId, id, timeoutMs = 15_000) {
  return await waitForCondition(
    sessionId,
    `agent debug element ${id}`,
    (snapshot) => !!visibleEnabledElement(snapshot, id),
    timeoutMs,
  );
}

async function waitForAnyAgentElement(sessionId, ids, timeoutMs = 15_000) {
  return await waitForCondition(
    sessionId,
    `one of agent debug elements ${ids.join(", ")}`,
    (snapshot) => ids.some((id) => !!visibleElement(snapshot, id)),
    timeoutMs,
  );
}

async function waitForVisibleText(sessionId, text, timeoutMs = 15_000) {
  return await waitForCondition(
    sessionId,
    `visible text ${text}`,
    (snapshot) => String(snapshot?.visibleText ?? "").includes(text),
    timeoutMs,
  );
}

async function waitForRouteChange(sessionId, previousRoute, timeoutMs = 15_000) {
  return await waitForCondition(
    sessionId,
    `route change from ${previousRoute}`,
    (snapshot) => !!snapshot?.route && snapshot.route !== previousRoute,
    timeoutMs,
  );
}

async function invokeCount(sessionId, command) {
  const snapshot = await observe(sessionId);
  return (snapshot?.invokes ?? []).filter((entry) => entry.command === command).length;
}

function commandInvokeAfter(snapshot, command, previousCount) {
  const matchingInvokes = (snapshot?.invokes ?? []).filter((entry) => entry.command === command);
  return matchingInvokes.slice(previousCount).at(-1) ?? matchingInvokes.at(-1) ?? null;
}

async function waitForCommandInvoke(sessionId, command, previousCount, timeoutMs = 20_000) {
  const snapshot = await waitForCondition(
    sessionId,
    `${command} invoke after ${previousCount} finished`,
    (next) => {
      const invoke = commandInvokeAfter(next, command, previousCount);
      return Boolean(invoke && invoke.status !== "started");
    },
    timeoutMs,
  );
  return {
    snapshot,
    invoke: commandInvokeAfter(snapshot, command, previousCount),
  };
}

function sendErrorText(snapshot) {
  const visibleText = String(snapshot?.visibleText ?? "");
  return SEND_ERROR_TEXT_MARKERS.find((marker) => visibleText.includes(marker)) ?? null;
}

function isProviderBlockError(message) {
  const text = String(message ?? "");
  return PROVIDER_BLOCK_ERROR_MARKERS.some((marker) => text.includes(marker));
}

function summarizeSlowInvokes(invokes) {
  return (invokes ?? [])
    .filter((entry) => Number(entry?.durationMs ?? 0) >= SLOW_INVOKE_WARNING_MS)
    .map((entry) => ({
      command: entry.command,
      status: entry.status,
      durationMs: entry.durationMs,
      severity: Number(entry.durationMs ?? 0) >= SLOW_INVOKE_HIGH_MS ? "high" : "warning",
      error: entry.error ?? null,
    }))
    .sort((a, b) => b.durationMs - a.durationMs);
}

async function collectInvokeLogs(sessionId) {
  return await execute(sessionId, "return window.__liliaAgentDebug?.observe?.().invokes ?? [];");
}

async function act(sessionId, action, scenarioId) {
  const result = await execute(
    sessionId,
    "return window.__liliaAgentDebug.act(arguments[0]);",
    [action],
  );
  replay.push({
    scenario: scenarioId,
    ...action,
    route: result?.route ?? null,
    capturedAt: result?.capturedAt ?? null,
  });
  return result;
}

async function executeInApp(sessionId, body, args = []) {
  return await execute(
    sessionId,
    `return (async () => {
      ${body}
    })();`,
    args,
  );
}

async function readAgentInteractionSettings(sessionId) {
  return await executeInApp(
    sessionId,
    `const chat = await import("/src/services/chat.ts");
     return await chat.getAgentInteractionSettings();`,
  );
}

async function writeAgentInteractionSettings(sessionId, settings) {
  return await executeInApp(
    sessionId,
    `const chat = await import("/src/services/chat.ts");
     return await chat.setAgentInteractionSettings(arguments[0]);`,
    [settings],
  );
}

async function configureAgentDebugSmokeSettings(sessionId) {
  const previous = await readAgentInteractionSettings(sessionId);
  const previousAutoTurn = previous?.autoTurnDecision ?? {};
  if (previousAutoTurn.enabled === false) {
    replay.push({ type: "settings", target: "agent.autoTurnDecision", changed: false });
    return;
  }
  const next = {
    ...previous,
    autoTurnDecision: {
      ...previousAutoTurn,
      enabled: false,
    },
  };
  await writeAgentInteractionSettings(sessionId, next);
  restoreAgentInteractionSettings = previous;
  replay.push({ type: "settings", target: "agent.autoTurnDecision", changed: true, enabled: false });
}

async function restoreAgentDebugSmokeSettings(sessionId) {
  if (!restoreAgentInteractionSettings) return;
  const previous = restoreAgentInteractionSettings;
  restoreAgentInteractionSettings = null;
  await writeAgentInteractionSettings(sessionId, previous);
  replay.push({ type: "settings", target: "agent.autoTurnDecision", restored: true });
}

async function readCodexProviderSettings(sessionId) {
  return await executeInApp(
    sessionId,
    `const chat = await import("/src/services/chat.ts");
     return {
       activeBackend: await chat.getActiveBackend(),
       codexRouterMode: await chat.getRouterMode("codex"),
     };`,
  );
}

async function restoreCodexProviderSettings(sessionId, settings) {
  if (!settings) return;
  await executeInApp(
    sessionId,
    `const chat = await import("/src/services/chat.ts");
     await chat.setRouterMode("codex", arguments[0].codexRouterMode);
     await chat.setActiveBackend(arguments[0].activeBackend);
     return true;`,
    [settings],
  );
  replay.push({ type: "settings", target: "provider.codex", restored: true });
}

function elementChecked(snapshot, id) {
  return elementById(snapshot, id)?.checked === true;
}

async function clickRadioUnlessChecked(sessionId, target, scenarioId) {
  const snapshot = await waitForAgentElement(sessionId, target, 20_000);
  if (elementChecked(snapshot, target)) return snapshot;
  await clickAgent(sessionId, target, scenarioId);
  return await waitForCondition(
    sessionId,
    `${target} checked`,
    (next) => elementChecked(next, target),
    20_000,
  );
}

async function refreshNativeQuotaPanel(sessionId, scenarioId) {
  await navigateRoute(sessionId, "/settings?tab=quota", scenarioId);
  await waitForAgentElement(sessionId, "settings.quota.refresh", 30_000);
  const beforeCount = await invokeCount(sessionId, NATIVE_QUOTA_SURFACE_COMMAND);
  await clickAgent(sessionId, "settings.quota.refresh", scenarioId);
  return await waitForCommandInvoke(
    sessionId,
    NATIVE_QUOTA_SURFACE_COMMAND,
    beforeCount,
    30_000,
  );
}

function assertVisibleText(snapshot, markers, label) {
  const visibleText = String(snapshot?.visibleText ?? "");
  const missing = markers.filter((marker) => !visibleText.includes(marker));
  if (missing.length) {
    throw new Error(`${label} missing visible text: ${missing.join(", ")}`);
  }
}

async function runNativeQuotaSurfaceScenario(sessionId) {
  const scenario = SCENARIOS.nativeQuotaSurface;
  await markScenario(sessionId, scenario);
  const result = await refreshNativeQuotaPanel(sessionId, scenario.id);
  await waitForAgentElement(sessionId, "settings.quota.native", 30_000);
  await waitForAgentElement(sessionId, "settings.quota.native.remote-api", 30_000);
  const snapshot = await waitForCondition(
    sessionId,
    "Native quota diagnostics visible",
    (next) => String(next?.visibleText ?? "").includes("Credential Broker / Native") &&
      String(next?.visibleText ?? "").includes("远程额度 API：不可用") &&
      String(next?.visibleText ?? "").includes("订阅 ≠ API 额度") &&
      String(next?.visibleText ?? "").includes("已隔离"),
    30_000,
  );
  assertVisibleText(
    snapshot,
    ["Credential Broker / Native", "远程额度 API：不可用", "订阅 ≠ API 额度", "已隔离"],
    "Native quota diagnostics",
  );
  return await recordScenarioResult(sessionId, scenario, "passed", {
    command: NATIVE_QUOTA_SURFACE_COMMAND,
    commandStatus: result.invoke?.status ?? null,
    remoteQuotaApi: "unavailable",
    subscriptionCredentialBoundary: "isolated",
  });
}

async function writePassedRun(sessionId, {
  preflight,
  driverOutput,
  devServerOutput,
  beforeScreenshotPath,
  observePath,
}) {
  const logs = await collectInvokeLogs(sessionId);
  const slowInvokes = summarizeSlowInvokes(logs);
  const afterScreenshotPath = await screenshot(sessionId, "after.png");
  await writeRunSummary(artifacts, {
    status: "passed",
    preflight,
    logs,
    driverOutput,
    devServerOutput,
    beforeScreenshotPath,
    afterScreenshotPath,
    observePath,
    slowInvokes,
  });
}

async function collectImplementedSurfaceTargets(sessionId) {
  return await executeInApp(
    sessionId,
    `const projectsStore = await import("/src/data/projects.ts");
     const tasksStore = await import("/src/data/tasks.ts");
     await projectsStore.ensureProjectsLoaded(true);
     const projects = projectsStore.listProjects();
     const firstProject = projects[0] ?? null;
     let firstProjectTask = null;
     if (firstProject) {
       await tasksStore.ensureProjectTasksLoaded(firstProject.id, true);
       firstProjectTask = tasksStore.listTasks(firstProject.id)[0] ?? null;
     }
     await tasksStore.ensureOrphansLoaded(true);
     const firstOrphan = tasksStore.listOrphanConversations()[0] ?? null;
     return {
       projectCount: projects.length,
       firstProject: firstProject ? { id: firstProject.id, name: firstProject.name } : null,
       firstProjectTask: firstProjectTask
         ? { id: firstProjectTask.id, title: firstProjectTask.title, projectId: firstProjectTask.projectId }
         : null,
       firstOrphan: firstOrphan ? { id: firstOrphan.id, title: firstOrphan.title } : null,
     };`,
  );
}

async function navigateRoute(sessionId, route, scenarioId) {
  await execute(
    sessionId,
    `window.history.pushState({}, "", arguments[0]);
     window.dispatchEvent(new PopStateEvent("popstate", { state: window.history.state }));
     return true;`,
    [route],
  );
  replay.push({ scenario: scenarioId, type: "navigate", route });
  return await waitForCondition(
    sessionId,
    `route ${route}`,
    (snapshot) => snapshot?.route === route,
    20_000,
  );
}

async function runSurfaceScenario(sessionId, surface, baselineErrorCount) {
  const scenario = {
    id: `surface-${surface.id}`,
    label: surface.label,
    artifact: `scenario-surface-${surface.id.replace(/[^a-z0-9_-]+/gi, "-")}.png`,
  };
  await markScenario(sessionId, scenario);
  await navigateRoute(sessionId, surface.route, scenario.id);
  const snapshot = await waitForCondition(
    sessionId,
    `${surface.label} surface ready`,
    (next) => {
      if (surface.expectedAny?.length) {
        return surface.expectedAny.some((id) => !!visibleElement(next, id));
      }
      return (surface.expected ?? []).every((id) => !!visibleElement(next, id));
    },
    surface.timeoutMs ?? 20_000,
  );
  if (snapshot.missingAgentIds?.length) {
    throw new Error(`${surface.label} has visible interactive elements without data-agent-id: ${snapshot.missingAgentIds.length}`);
  }
  const newErrors = (snapshot.errors ?? []).slice(baselineErrorCount);
  if (newErrors.length) {
    throw new Error(`${surface.label} produced frontend errors: ${newErrors.map((item) => item.message).join("; ")}`);
  }
  return await finishScenario(sessionId, scenario, {
    route: snapshot.route,
    expected: surface.expected ?? null,
    expectedAny: surface.expectedAny ?? null,
    elementCount: snapshot.elements?.length ?? 0,
  });
}

function buildImplementedSurfaceScenarios(targets) {
  const baseSurfaces = [
    {
      id: "home",
      label: "首页",
      route: "/",
      expected: ["app.shell", "app.main", "sidebar.new-chat"],
    },
    {
      id: "projects-overview",
      label: "项目总览",
      route: "/projects",
      expected: ["app.shell", "app.main", "sidebar.projects.overview"],
    },
    {
      id: "automations",
      label: "自动化",
      route: "/automations",
      expectedAny: ["automations.workspace", "automations.loading"],
      timeoutMs: 30_000,
    },
  ];

  const settingsSurfaces = SETTINGS_SURFACE_TABS.map((tab) => ({
    id: `settings-${tab}`,
    label: `设置 / ${tab}`,
    route: `/settings?tab=${encodeURIComponent(tab)}`,
    expected: [
      "settings.sidebar",
      `settings.tab.${tab}`,
      tab.startsWith("plugin-") || tab === "import"
        ? "settings.full-page-section"
        : `settings.page.${tab}`,
    ],
    timeoutMs: tab.startsWith("plugin-") || tab === "quota" ? 30_000 : 20_000,
  }));

  const projectId = targets.firstProject ? encodeURIComponent(targets.firstProject.id) : null;
  const projectSurfaces = projectId
    ? [
        {
          id: "project-sessions",
          label: "项目 Sessions",
          route: `/projects/${projectId}`,
          expected: ["app.shell", "view-tabs.sessions"],
        },
        {
          id: "project-roadmap",
          label: "项目路线图",
          route: `/projects/${projectId}/roadmap`,
          expected: ["app.shell", "view-tabs.roadmap", "roadmap.create.title"],
        },
        {
          id: "project-memory",
          label: "项目记忆",
          route: `/projects/${projectId}/memory`,
          expected: ["app.shell", "view-tabs.memory", "memory.page"],
        },
      ]
    : [];

  const taskSurfaces = [
    targets.firstProjectTask && {
      id: "project-task-detail",
      label: "项目任务详情",
      route: `/projects/${encodeURIComponent(targets.firstProjectTask.projectId)}/tasks/${encodeURIComponent(targets.firstProjectTask.id)}`,
      expected: ["app.shell", "chat.composer.input"],
      timeoutMs: 30_000,
    },
    targets.firstOrphan && {
      id: "orphan-chat-detail",
      label: "独立对话详情",
      route: `/chats/${encodeURIComponent(targets.firstOrphan.id)}`,
      expected: ["app.shell", "chat.composer.input"],
      timeoutMs: 30_000,
    },
  ].filter(Boolean);

  return [...baseSurfaces, ...settingsSurfaces, ...projectSurfaces, ...taskSurfaces];
}

async function runImplementedSurfaceScenarios(sessionId) {
  const scenario = {
    id: "surface-discovery",
    label: "页面覆盖发现",
    artifact: "scenario-surface-discovery.png",
  };
  await markScenario(sessionId, scenario);
  const targets = await collectImplementedSurfaceTargets(sessionId);
  await recordScenarioResult(sessionId, scenario, "passed", targets);
  const baselineErrorCount = (await observe(sessionId))?.errors?.length ?? 0;
  const surfaces = buildImplementedSurfaceScenarios(targets);
  for (const surface of surfaces) {
    await runSurfaceScenario(sessionId, surface, baselineErrorCount);
  }
}

async function clickAgent(sessionId, target, scenarioId) {
  await waitForAgentElement(sessionId, target);
  return await act(sessionId, { type: "click", target }, scenarioId);
}

async function typeAgent(sessionId, target, text, scenarioId, clear = true) {
  await waitForAgentElement(sessionId, target);
  return await act(sessionId, { type: "type", target, text, clear }, scenarioId);
}

async function markScenario(sessionId, scenario) {
  artifacts.beginScenario(scenario);
  await act(
    sessionId,
    { type: "mark", label: `scenario:${scenario.id}:start`, data: { label: scenario.label } },
    scenario.id,
  );
}

async function recordScenarioResult(sessionId, scenario, status, details = {}) {
  const screenshotPath = await screenshot(sessionId, scenario.artifact);
  const snapshot = await observe(sessionId);
  const result = artifacts.appendScenarioResult(scenario, status, {
    route: snapshot?.route ?? null,
    screenshotPath,
    ...details,
  });
  replay.push({ scenario: scenario.id, type: "assert", status, details });
  await act(
    sessionId,
    { type: "mark", label: `scenario:${scenario.id}:${status}`, data: result },
    scenario.id,
  );
  return result;
}

async function finishScenario(sessionId, scenario, details = {}) {
  return await recordScenarioResult(sessionId, scenario, "passed", details);
}

async function findDebugPanelOpener(sessionId) {
  return await waitForAnyAgentElement(
    sessionId,
    ["titlebar.chat-sidebar.toggle", "chat.sidebar.tab.debug", "debug.timeline.plan"],
    3_000,
  ).catch(() => null);
}

async function ensureFreshConversation(sessionId, scenarioId) {
  const before = await observe(sessionId);
  await clickAgent(sessionId, "sidebar.new-chat", scenarioId);
  const after = await waitForRouteChange(sessionId, before?.route ?? "", 15_000);
  await waitForAgentElement(sessionId, "chat.composer.input", 20_000);
  return after.route;
}

async function ensureDebugPanel(sessionId, scenarioId) {
  const snapshot = await observe(sessionId);
  if (visibleEnabledElement(snapshot, "debug.timeline.plan")) return true;

  if (!visibleEnabledElement(snapshot, "chat.sidebar.tab.debug")) {
    const opener = await findDebugPanelOpener(sessionId);
    if (!opener) return false;
    if (visibleEnabledElement(opener, "titlebar.chat-sidebar.toggle")) {
      await clickAgent(sessionId, "titlebar.chat-sidebar.toggle", scenarioId);
    }
  }
  const ready = await waitForAnyAgentElement(
    sessionId,
    ["chat.sidebar.tab.debug", "debug.timeline.plan"],
    20_000,
  );
  if (visibleEnabledElement(ready, "chat.sidebar.tab.debug")) {
    await clickAgent(sessionId, "chat.sidebar.tab.debug", scenarioId);
  }
  await waitForAgentElement(sessionId, "debug.timeline.plan", 20_000);
  return true;
}

async function clickFirstAvailable(sessionId, targets, scenarioId) {
  const snapshot = await waitForCondition(
    sessionId,
    `enabled agent debug element among ${targets.join(", ")}`,
    (next) => targets.some((id) => !!visibleEnabledElement(next, id)),
    20_000,
  );
  const target = targets.find((id) => visibleEnabledElement(snapshot, id));
  if (!target) throw new Error(`No enabled target among ${targets.join(", ")}`);
  await clickAgent(sessionId, target, scenarioId);
  return target;
}

async function settleRunningTurnForFollowup(sessionId, scenarioId) {
  const snapshot = await observe(sessionId);
  const sendButton = elementById(snapshot, "chat.composer.send");
  if (!String(sendButton?.text ?? "").includes("打断 Agent")) return;
  await clickAgent(sessionId, "chat.composer.send", scenarioId);
  await waitForCondition(
    sessionId,
    "running turn interruption",
    (next) => !String(elementById(next, "chat.composer.send")?.text ?? "").includes("打断 Agent"),
    15_000,
  ).catch(() => undefined);
}

async function runSendScenario(sessionId, scenario, input, options = {}) {
  await markScenario(sessionId, scenario);
  const beforeSnapshot = await observe(sessionId);
  const beforeErrorMarker = sendErrorText(beforeSnapshot);
  const beforeCount = await invokeCount(sessionId, CHAT_SEND_MESSAGE_COMMAND);
  await typeAgent(sessionId, "chat.composer.input", input, scenario.id);
  await clickAgent(sessionId, "chat.composer.send", scenario.id);
  const { snapshot, invoke } = await waitForCommandInvoke(sessionId, CHAT_SEND_MESSAGE_COMMAND, beforeCount);
  if (!invoke) {
    throw new Error(`${CHAT_SEND_MESSAGE_COMMAND} invoke was not captured for ${scenario.id}`);
  }
  if (invoke.status === "error" && isProviderBlockError(invoke.error)) {
    await recordScenarioResult(sessionId, scenario, "blocked", {
      reason: "provider is not ready for agent debug smoke",
      commandStatus: invoke.status,
      commandError: invoke.error,
      durationMs: invoke.durationMs,
    });
    throw new SmokeBlockedError(`Provider is not ready for agent debug smoke: ${invoke.error}`, {
      scenario: scenario.id,
      command: CHAT_SEND_MESSAGE_COMMAND,
      error: invoke.error,
    });
  }
  if (invoke.status !== "success") {
    throw new Error(`${CHAT_SEND_MESSAGE_COMMAND} returned ${invoke.status}: ${invoke.error ?? "unknown error"}`);
  }
  const marker = sendErrorText(snapshot);
  if (marker && marker !== beforeErrorMarker && !options.allowExistingSendError) {
    throw new Error(`${scenario.id} still shows send error text: ${marker}`);
  }
  return await finishScenario(sessionId, scenario, {
    expectedCommand: CHAT_SEND_MESSAGE_COMMAND,
    expectedCommandCount: beforeCount + 1,
    commandStatus: invoke.status,
    durationMs: invoke.durationMs,
  });
}

async function runOrdinarySendScenario(sessionId) {
  const scenario = SCENARIOS.ordinarySend;
  await ensureFreshConversation(sessionId, scenario.id);
  return await runSendScenario(
    sessionId,
    scenario,
    "Agent Debug v1.0 ordinary send regression",
  );
}

async function runContinueHistoryScenario(sessionId) {
  const scenario = SCENARIOS.continueHistory;
  await settleRunningTurnForFollowup(sessionId, scenario.id);
  await waitForAgentElement(sessionId, "chat.composer.input", 20_000);
  return await runSendScenario(
    sessionId,
    scenario,
    "Agent Debug v1.0 continue history regression",
    { allowExistingSendError: true },
  );
}

async function runTaskRecoveryScenario(sessionId, previousRoute) {
  const scenario = SCENARIOS.taskRecovery;
  await markScenario(sessionId, scenario);
  if (!previousRoute) throw new Error("Task recovery scenario requires an existing task route");
  await ensureFreshConversation(sessionId, scenario.id);
  await execute(sessionId, "window.history.back(); return true;");
  await waitForCondition(
    sessionId,
    `route recovery to ${previousRoute}`,
    (snapshot) => snapshot?.route === previousRoute,
    20_000,
  );
  await waitForAgentElement(sessionId, "chat.composer.input", 20_000);
  return await finishScenario(sessionId, scenario, { recoveredRoute: previousRoute });
}

async function runPendingActionScenario(sessionId, input) {
  const { scenario, triggerTarget, resolutionTargets, expectedText, details } = input;
  await markScenario(sessionId, scenario);
  if (!(await ensureDebugPanel(sessionId, scenario.id))) {
    const reason = "debug sidebar panel is not reachable on the current route";
    await recordScenarioResult(sessionId, scenario, "failed", {
      reason,
    });
    throw new Error(reason);
  }
  await clickAgent(sessionId, triggerTarget, scenario.id);
  await clickFirstAvailable(sessionId, resolutionTargets, scenario.id);
  await waitForVisibleText(sessionId, expectedText, 20_000);
  return await finishScenario(sessionId, scenario, details);
}

async function runCoreConversationScenarios(sessionId) {
  const ordinary = await runOrdinarySendScenario(sessionId);
  await runContinueHistoryScenario(sessionId);
  await runTaskRecoveryScenario(sessionId, ordinary.route);
  await ensureFreshConversation(sessionId, "pending-actions-setup");
  await runPendingActionScenario(sessionId, {
    scenario: SCENARIOS.planPendingAction,
    triggerTarget: "debug.timeline.plan",
    resolutionTargets: ["timeline.plan.accept", "chat.pending.plan.accept", "chat.composer.plan.accept", "ask-user.confirm"],
    expectedText: "Debug 计划已同意",
    details: { resolved: "accepted" },
  });
  await runPendingActionScenario(sessionId, {
    scenario: SCENARIOS.permissionPendingAction,
    triggerTarget: "debug.timeline.permission",
    resolutionTargets: ["timeline.permission.allow", "chat.pending.tool.allow", "chat.composer.tool.allow"],
    expectedText: "Debug 权限已同意",
    details: { resolved: "allowed" },
  });
}

async function main() {
  await mkdir(runDir, { recursive: true });
  const devServerPlan = await createAgentDebugDevServerPlan(process.env);
  const devServerOutput = [];
  const driverOutput = [];
  let setupInfo = null;
  let setupError = null;
  try {
    setupInfo = await ensureAgentDebugTools();
  } catch (error) {
    setupError = error?.message ?? String(error);
  }
  const preflight = {
    runId,
    runDir,
    appBinary,
    devUrl: devServerPlan.devUrl,
    devServer: devServerPlan,
    autoSetup: setupInfo?.setup ?? [],
    setupError,
    edgeVersion: setupInfo?.edgeVersion ?? null,
    edgeDriverVersion: setupInfo?.edgeDriverVersion ?? edgeDriverVersion(),
    toolDir,
    tauriDriver: commandExists("tauri-driver", ["--help"]),
    edgeDriver: commandExists("msedgedriver") || commandExists("MicrosoftWebDriver"),
    appBinaryExists: existsSync(appBinary),
    nativeQuotaSurfaceCommand: NATIVE_QUOTA_SURFACE_COMMAND,
    requiresLegacyAgentServerResources: false,
    requestedScenario: requestedScenarioId || null,
  };
  await writeJson("preflight.json", preflight);
  if (!preflight.tauriDriver || !preflight.edgeDriver || !preflight.appBinaryExists) {
    await writeRunSummary(artifacts, {
      status: "blocked",
      preflight,
      reason: setupError ?? "Missing tauri-driver, EdgeDriver, or debug app binary.",
      nextStep: DEFAULT_NEXT_STEP,
      driverOutput,
      devServerOutput,
    });
    process.exitCode = 2;
    return;
  }

  let devServer = null;
  let driver = null;
  let sessionId = null;
  try {
    const childEnv = createAgentDebugChildEnv(
      process.env,
      devServerPlan.devUrl,
      devServerPlan.port,
    );
    if (!(await isUrlReady(devServerPlan.devUrl))) {
      devServer = spawnYarn(["--cwd", "apps/desktop", "dev", "--host", "127.0.0.1"], {
        cwd: repoRoot,
        stdio: ["ignore", "pipe", "pipe"],
        env: childEnv,
      });
      devServer.stdout.on("data", (chunk) => devServerOutput.push(chunk.toString("utf8")));
      devServer.stderr.on("data", (chunk) => devServerOutput.push(chunk.toString("utf8")));
      await waitForUrl(devServerPlan.devUrl, 30_000, "Vite dev server", devServer, devServerOutput);
    }

    driver = spawn("tauri-driver", ["--port", String(driverPort)], {
      stdio: ["ignore", "pipe", "pipe"],
      env: {
        ...childEnv,
        MSEDGEDRIVER_TELEMETRY_OPTOUT: "1",
      },
    });
    driver.stdout.on("data", (chunk) => driverOutput.push(chunk.toString("utf8")));
    driver.stderr.on("data", (chunk) => driverOutput.push(chunk.toString("utf8")));

    await waitForDriver();
    const session = await request("POST", "/session", {
      capabilities: {
        alwaysMatch: {
          browserName: "wry",
          "tauri:options": {
            application: appBinary,
            args: [],
            env: childEnv,
          },
        },
      },
    });
    sessionId = session.value.sessionId;
    await waitForDebugApi(sessionId);
    await waitForDebugUi(sessionId);
    await configureAgentDebugSmokeSettings(sessionId);
    const beforeScreenshotPath = await screenshot(sessionId, "before.png");
    const observe = await execute(sessionId, "return window.__liliaAgentDebug?.observe?.() ?? null;");
    if (!observe?.enabled) {
      throw new Error("window.__liliaAgentDebug is not enabled in the app session");
    }
    await writeJson("observe.json", observe);
    if (!observe.elements.some((element) => element.id === "sidebar.new-chat" || element.id === "app.shell")) {
      throw new Error("Expected sidebar.new-chat or app.shell in agent debug element tree");
    }
    await writeJson("missing-agent-ids.json", observe.missingAgentIds ?? []);
    if (observe.missingAgentIds?.length) {
      throw new Error(`Visible interactive elements without data-agent-id: ${observe.missingAgentIds.length}`);
    }
    replay.push({ type: "observe", route: observe.route, capturedAt: observe.capturedAt });
    const typeResult = await execute(
      sessionId,
      "return window.__liliaAgentDebug.act({ type: 'mark', label: 'verify-agent-debug-smoke', data: { source: 'verify:agent-debug' } });",
    );
    replay.push({ type: "mark", label: "verify-agent-debug-smoke" });
    await writeJson("after-mark-observe.json", typeResult);
    const missingTargetError = await execute(
      sessionId,
      "return window.__liliaAgentDebug.act({ type: 'click', target: 'missing.debug.target' }).then(() => null, err => String(err && err.message || err));",
    );
    if (!String(missingTargetError).includes("missing.debug.target")) {
      throw new Error("Missing target scenario did not return a useful diagnostic");
    }
    replay.push({ type: "click", target: "missing.debug.target", expectedError: true });
    if (requestedScenarioId) {
      if (requestedScenarioId !== SCENARIOS.nativeQuotaSurface.id) {
        throw new Error(`Unknown agent-debug scenario filter: ${requestedScenarioId}`);
      }
      await runNativeQuotaSurfaceScenario(sessionId);
      await writePassedRun(sessionId, {
        preflight,
        driverOutput,
        devServerOutput,
        beforeScreenshotPath,
        observePath: path.join(runDir, "observe.json"),
      });
      return;
    }
    await runCoreConversationScenarios(sessionId);
    await runImplementedSurfaceScenarios(sessionId);
    await writePassedRun(sessionId, {
      preflight,
      driverOutput,
      devServerOutput,
      beforeScreenshotPath,
      observePath: path.join(runDir, "observe.json"),
    });
  } catch (error) {
    const blocked = error instanceof SmokeBlockedError;
    let failureDiagnosticsPath = null;
    let failureScreenshotPath = null;
    let failureSnapshot = null;
    let logs = [];
    let slowInvokes = [];
    if (sessionId) {
      failureScreenshotPath = await screenshot(sessionId, "failure.png").catch(() => null);
      failureSnapshot = await observe(sessionId).catch(() => null);
      logs = await collectInvokeLogs(sessionId).catch(() => []);
      slowInvokes = summarizeSlowInvokes(logs);
      const diagnostics = await execute(
        sessionId,
        `return {
          location: window.location.href,
          title: document.title,
          readyState: document.readyState,
          bodyText: document.body?.innerText?.slice(0, 2000) ?? "",
          hasAgentDebug: Boolean(window.__liliaAgentDebug),
        };`,
      ).catch((diagnosticError) => ({ diagnosticError: String(diagnosticError?.message ?? diagnosticError) }));
      failureDiagnosticsPath = await writeJson("failure-diagnostics.json", diagnostics).catch(() => null);
    }
    artifacts.ensureActiveScenarioResult(blocked ? "blocked" : "failed", {
      route: failureSnapshot?.route ?? null,
      screenshotPath: failureScreenshotPath,
      reason: blocked ? error.message : undefined,
      message: error?.message ?? String(error),
    });
    await writeRunSummary(artifacts, {
      status: blocked ? "blocked" : "failed",
      preflight,
      reason: blocked ? error.message : undefined,
      message: error?.message ?? String(error),
      details: blocked ? error.details : undefined,
      driverOutput,
      devServerOutput,
      logs,
      failureScreenshotPath,
      failureDiagnosticsPath,
      slowInvokes,
    });
    process.exitCode = blocked ? 2 : 1;
  } finally {
    if (sessionId) {
      await restoreAgentDebugSmokeSettings(sessionId).catch((error) => {
        replay.push({
          type: "settings",
          target: "agent.autoTurnDecision",
          restored: false,
          error: error?.message ?? String(error),
        });
      });
      await request("DELETE", `/session/${sessionId}`).catch(() => undefined);
    }
    driver?.kill();
    stopProcessTree(devServer);
    await artifacts.writeText("tauri-driver.log", driverOutput.join(""));
    await artifacts.writeText("dev-server.log", devServerOutput.join(""));
  }
}

function isDirectRun() {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
}

export {
  AgentDebugRunArtifacts,
  SmokeBlockedError,
  main,
  writeRunSummary,
};

if (isDirectRun()) {
  await main();
}

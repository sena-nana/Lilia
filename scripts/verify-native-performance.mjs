#!/usr/bin/env node

import { createHash } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import { existsSync, statSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { performance } from "node:perf_hooks";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import {
  createAgentDebugChildEnv,
  createAgentDebugDevServerPlan,
  findAvailablePort,
} from "../apps/desktop/agent-debug/dev-server.mjs";
import { ensureAgentDebugTools } from "../apps/desktop/agent-debug/verify-agent-debug.mjs";
import {
  evaluateNativePerformanceGate,
  isJavaScriptEntrypoint,
  processSampleDelta,
  summarizeSamples,
} from "./native-performance-lib.mjs";
import { requestNativeDebug, waitUntil } from "./ui-equivalence-lib.mjs";

const repoRoot = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const runId = new Date().toISOString().replace(/[:.]/g, "-");
const runDir = path.join(repoRoot, "agent-debug-runs", `native-performance-${runId}`);
const fixturePath = path.join(repoRoot, "tests", "equivalence", "performance-v1.json");
const fixture = JSON.parse(await readFile(fixturePath, "utf8"));
const fixtureId = fixture.fixtureId;
const projectId = `${fixtureId}-project`;
const taskId = `${fixtureId}-task`;
const expectedTimelineCount = 1_000;
const debugTauriBinary = binaryPath("debug", "lilia");
const debugNativeBinary = binaryPath("debug", "lilia-native-preview");
const releaseTauriBinary = binaryPath("release", "lilia");
const releaseNativeBinary = binaryPath("release", "lilia-native-preview");
const probeBinary = path.join(
  repoRoot,
  "target",
  "debug",
  "examples",
  process.platform === "win32" ? "native_performance_probe.exe" : "native_performance_probe",
);
const options = parseArguments(process.argv.slice(2));
const logs = new Map();
const children = new Set();

const summary = {
  schemaVersion: 2,
  runId,
  status: "running",
  mode: options.smoke ? "smoke" : options.interactionOnly ? "interaction-only" : "full",
  startedAt: new Date().toISOString(),
  finishedAt: null,
  fixtureId,
  fixturePath,
  options,
  environment: null,
  source: null,
  artifacts: {},
  tauri: null,
  native: null,
  gate: { status: "not-evaluated", reason: "run has not completed" },
};

function binaryPath(profile, name) {
  return path.join(repoRoot, "target", profile, process.platform === "win32" ? `${name}.exe` : name);
}

function parseArguments(arguments_) {
  const result = {
    samples: 30,
    coldStartSamples: 5,
    idleSamples: 5,
    idleWarmupMs: 5_000,
    idleIntervalMs: 1_000,
    skipBuild: false,
    interactionOnly: false,
    smoke: false,
  };
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    if (argument === "--skip-build") result.skipBuild = true;
    else if (argument === "--interaction-only") result.interactionOnly = true;
    else if (argument === "--smoke") {
      result.smoke = true;
      result.interactionOnly = true;
      result.samples = 3;
    } else if (["--samples", "--cold-start-samples", "--idle-samples", "--idle-warmup-ms", "--idle-interval-ms"].includes(argument)) {
      const value = Number.parseInt(arguments_[index + 1] ?? "", 10);
      if (!Number.isInteger(value) || value < 1) throw new Error(`${argument} requires a positive integer`);
      const field = {
        "--samples": "samples",
        "--cold-start-samples": "coldStartSamples",
        "--idle-samples": "idleSamples",
        "--idle-warmup-ms": "idleWarmupMs",
        "--idle-interval-ms": "idleIntervalMs",
      }[argument];
      result[field] = value;
      index += 1;
    } else {
      throw new Error(`Unknown argument: ${argument}`);
    }
  }
  return result;
}

function track(child, label) {
  children.add(child);
  const output = [];
  logs.set(label, output);
  child.stdout?.on("data", (chunk) => output.push(chunk.toString("utf8")));
  child.stderr?.on("data", (chunk) => output.push(chunk.toString("utf8")));
  child.once("exit", () => children.delete(child));
  return child;
}

function stopProcessTree(child) {
  if (!child || child.exitCode !== null || child.killed) return;
  if (process.platform === "win32" && child.pid) {
    spawnSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], {
      stdio: "ignore",
      windowsHide: true,
    });
  } else {
    child.kill();
  }
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function run(command, arguments_, runOptions = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, arguments_, {
      cwd: repoRoot,
      env: process.env,
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
      ...runOptions,
    });
    const output = [];
    child.stdout?.on("data", (chunk) => output.push(chunk.toString("utf8")));
    child.stderr?.on("data", (chunk) => output.push(chunk.toString("utf8")));
    child.once("error", reject);
    child.once("exit", (code) => {
      const text = output.join("");
      if (code === 0) resolve(text);
      else reject(new Error(`${command} ${arguments_.join(" ")} failed with exit code ${code}\n${text}`));
    });
  });
}

function yarnInvocation(arguments_) {
  const candidates = [];
  if (process.env.npm_execpath) candidates.push(process.env.npm_execpath);
  if (process.platform === "win32") {
    const located = spawnSync("where.exe", ["yarn.cmd"], {
      encoding: "utf8",
      windowsHide: true,
    });
    for (const shim of located.stdout?.split(/\r?\n/).filter(Boolean) ?? []) {
      candidates.push(path.join(path.dirname(shim), "node_modules", "corepack", "dist", "yarn.js"));
    }
  }
  const entry = candidates.find(
    (candidate) => isJavaScriptEntrypoint(candidate) && existsSync(candidate),
  );
  if (!entry) throw new Error("could not resolve the Corepack Yarn JavaScript entrypoint");
  return { command: process.execPath, arguments: [entry, ...arguments_] };
}

async function writeJson(name, value) {
  const target = path.join(runDir, name);
  await writeFile(target, `${JSON.stringify(value, null, 2)}\n`, "utf8");
  return target;
}

async function persistLogs() {
  for (const [label, entries] of logs) {
    await writeFile(path.join(runDir, `${label}.log`), entries.join(""), "utf8");
  }
}

async function seedHome(home, identity) {
  const output = await run("cargo", [
    "run",
    "--locked",
    "--quiet",
    "-p",
    "lilia-desktop-application",
    "--example",
    "equivalence_fixture",
    "--",
    "--manifest",
    fixturePath,
    "--home",
    home,
    "--identity",
    identity,
  ]);
  const result = JSON.parse(output.trim().split(/\r?\n/).at(-1));
  if (result.snapshot.timeline.length !== expectedTimelineCount) {
    throw new Error(`performance fixture seeded ${result.snapshot.timeline.length} timeline events`);
  }
  return result;
}

function getUrl(url) {
  return new Promise((resolve, reject) => {
    const request = http.get(url, (response) => {
      response.resume();
      response.once("end", () => {
        if (response.statusCode >= 200 && response.statusCode < 400) resolve(true);
        else reject(new Error(`GET ${url} failed: ${response.statusCode}`));
      });
    });
    request.once("error", reject);
    request.setTimeout(1_000, () => request.destroy(new Error(`GET ${url} timed out`)));
  });
}

function webdriverRequest(driverUrl, method, pathname, body) {
  const payload = body === undefined ? null : JSON.stringify(body);
  return new Promise((resolve, reject) => {
    const request = http.request(
      `${driverUrl}${pathname}`,
      {
        method,
        headers: payload
          ? { "content-type": "application/json", "content-length": Buffer.byteLength(payload) }
          : undefined,
      },
      (response) => {
        const chunks = [];
        response.on("data", (chunk) => chunks.push(chunk));
        response.on("end", () => {
          const text = Buffer.concat(chunks).toString("utf8");
          let value = null;
          try {
            value = text ? JSON.parse(text) : null;
          } catch {
            // Raw text remains attached to the protocol error below.
          }
          if (response.statusCode >= 200 && response.statusCode < 300) resolve(value);
          else reject(new Error(`${method} ${pathname} failed: ${response.statusCode} ${text}`));
        });
      },
    );
    request.once("error", reject);
    if (payload) request.write(payload);
    request.end();
  });
}

async function executeWebDriver(driverUrl, sessionId, script, args = []) {
  const result = await webdriverRequest(
    driverUrl,
    "POST",
    `/session/${sessionId}/execute/sync`,
    { script, args },
  );
  return result?.value;
}

async function webdriverElement(driverUrl, sessionId, selector) {
  const response = await webdriverRequest(
    driverUrl,
    "POST",
    `/session/${sessionId}/element`,
    { using: "css selector", value: selector },
  );
  const reference = response?.value;
  const elementId = reference?.["element-6066-11e4-a52e-4f735466cecf"] ??
    reference?.ELEMENT;
  if (!elementId) throw new Error(`WebDriver did not resolve element: ${selector}`);
  return { "element-6066-11e4-a52e-4f735466cecf": elementId };
}

function tauriObserve(driverUrl, sessionId) {
  return executeWebDriver(
    driverUrl,
    sessionId,
    "return window.__liliaAgentDebug?.observe?.() ?? null;",
  );
}

function tauriAct(driverUrl, sessionId, action) {
  return executeWebDriver(
    driverUrl,
    sessionId,
    "return window.__liliaAgentDebug.act(arguments[0]);",
    [action],
  );
}

function hasTauriTarget(observation, id) {
  return observation?.elements?.some((element) => element.id === id && element.visible && element.enabled);
}

async function prepareTauriFrameMeasurement(
  driverUrl,
  sessionId,
  targetId,
  eventName,
  eventScope = "target",
) {
  return executeWebDriver(
    driverUrl,
    sessionId,
    `const target = document.querySelector('[data-agent-id="' + arguments[0].replaceAll('\\\\', '\\\\\\\\').replaceAll('"', '\\\\"') + '"]');
     if (!(target instanceof HTMLElement)) throw new Error('performance target is unavailable: ' + arguments[0]);
     const id = 'perf-' + Date.now() + '-' + Math.random().toString(16).slice(2);
     window.__liliaPerformanceFrames ??= new Map();
     const promise = new Promise((resolve) => {
       const source = arguments[2] === 'window' ? window : target;
       const eventName = arguments[1];
       const onStart = (event) => {
         if (eventName === 'pointermove' && (event.buttons & 1) !== 1) return;
         source.removeEventListener(eventName, onStart);
         const started = performance.now();
         requestAnimationFrame(() => setTimeout(() => resolve({
           durationMs: performance.now() - started,
           value: target.getAttribute('aria-valuenow') ?? ('value' in target ? target.value : null),
         }), 0));
       };
       source.addEventListener(eventName, onStart);
     });
     window.__liliaPerformanceFrames.set(id, promise);
     const rect = target.getBoundingClientRect();
     return { id, x: rect.x + rect.width / 2, y: rect.y + rect.height / 2,
       before: target.getAttribute('aria-valuenow') ?? ('value' in target ? target.value : null) };`,
    [targetId, eventName, eventScope],
  );
}

async function finishTauriFrameMeasurement(driverUrl, sessionId, id) {
  return executeWebDriver(
    driverUrl,
    sessionId,
    `const frames = window.__liliaPerformanceFrames;
     const pending = frames?.get(arguments[0]);
     if (!pending) throw new Error('performance frame measurement is unavailable: ' + arguments[0]);
     return Promise.resolve(pending).then((result) => { frames.delete(arguments[0]); return result; });`,
    [id],
  );
}

async function measureTauriComposerFrame(driverUrl, sessionId, text) {
  const measurement = await prepareTauriFrameMeasurement(
    driverUrl,
    sessionId,
    "chat.composer.input",
    "input",
  );
  await tauriAct(driverUrl, sessionId, {
    type: "type",
    target: "chat.composer.input",
    text,
    clear: true,
  });
  return finishTauriFrameMeasurement(driverUrl, sessionId, measurement.id);
}

async function measureTauriPanelResize(driverUrl, sessionId, deltaX) {
  const measurement = await prepareTauriFrameMeasurement(
    driverUrl,
    sessionId,
    "app.sidebar.resizer",
    "pointermove",
    "window",
  );
  const resizer = await webdriverElement(
    driverUrl,
    sessionId,
    '[data-agent-id="app.sidebar.resizer"]',
  );
  const actions = [{
    type: "pointer",
    id: "performance-mouse",
    parameters: { pointerType: "mouse" },
    actions: [
      { type: "pointerMove", duration: 0, origin: resizer, x: 0, y: 0 },
      { type: "pointerDown", button: 0 },
      { type: "pointerMove", duration: 1, origin: "viewport", x: Math.round(measurement.x + deltaX), y: Math.round(measurement.y) },
      { type: "pointerUp", button: 0 },
    ],
  }];
  await webdriverRequest(driverUrl, "POST", `/session/${sessionId}/actions`, { actions });
  await webdriverRequest(driverUrl, "DELETE", `/session/${sessionId}/actions`).catch(() => undefined);
  const result = await finishTauriFrameMeasurement(driverUrl, sessionId, measurement.id);
  if (result.value === measurement.before) {
    throw new Error(`Tauri sidebar resize did not change aria-valuenow (${measurement.before})`);
  }
  return result;
}

async function runTauriInteraction(devServerPlan, home) {
  const driverPort = await findAvailablePort(4470);
  const driverUrl = `http://127.0.0.1:${driverPort}`;
  const childEnv = {
    ...createAgentDebugChildEnv(process.env, devServerPlan.devUrl, devServerPlan.port),
    LILIA_HOME: home,
    LILIA_EQUIVALENCE_FIXTURE_ID: fixtureId,
    TAURI_WEBDRIVER_PORT: String(driverPort),
  };
  let devServer = null;
  let driver = null;
  let sessionId = null;
  try {
    try {
      await getUrl(devServerPlan.devUrl);
    } catch {
      const vite = path.join(repoRoot, "node_modules", "vite", "bin", "vite.js");
      devServer = track(spawn(process.execPath, [vite, "--host", "127.0.0.1"], {
        cwd: path.join(repoRoot, "apps", "desktop"),
        env: childEnv,
        stdio: ["ignore", "pipe", "pipe"],
        windowsHide: true,
      }), "performance-tauri-vite");
      await waitUntil("Tauri Vite server", 30_000, async () => {
        if (devServer.exitCode !== null) throw new Error(logs.get("performance-tauri-vite").join(""));
        return getUrl(devServerPlan.devUrl).catch(() => false);
      });
    }

    driver = track(spawn("tauri-driver", ["--port", String(driverPort)], {
      env: { ...childEnv, MSEDGEDRIVER_TELEMETRY_OPTOUT: "1" },
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    }), "performance-tauri-driver");
    await waitUntil("tauri-driver", 20_000, async () => {
      if (driver.exitCode !== null) throw new Error(logs.get("performance-tauri-driver").join(""));
      return webdriverRequest(driverUrl, "GET", "/status").then(() => true, () => false);
    });
    const processStarted = performance.now();
    const session = await webdriverRequest(driverUrl, "POST", "/session", {
      capabilities: {
        alwaysMatch: {
          browserName: "wry",
          "tauri:options": { application: debugTauriBinary, args: [], env: childEnv },
        },
      },
    });
    sessionId = session?.value?.sessionId;
    if (!sessionId) throw new Error(`Tauri WebDriver did not return a session: ${JSON.stringify(session)}`);
    const harnessReady = await waitUntil("Tauri performance harness", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return observation?.enabled && hasTauriTarget(observation, "app.shell") ? observation : null;
    });
    const harnessReadyMs = performance.now() - processStarted;
    const taskStarted = performance.now();
    const projectTarget = `sidebar.project.${projectId}.row`;
    await waitUntil("Tauri performance project", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasTauriTarget(observation, projectTarget) ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: projectTarget });
    const taskTarget = `sidebar.task.${taskId}`;
    await waitUntil("Tauri performance task", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasTauriTarget(observation, taskTarget) ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: taskTarget });
    const timelineState = await waitUntil("Tauri thousand-event timeline", 60_000, async () => {
      return executeWebDriver(
        driverUrl,
        sessionId,
        `const surface = document.querySelector('[data-agent-id="timeline.surface"]');
         const composer = document.querySelector('[data-agent-id="chat.composer.input"]');
         if (!(surface instanceof HTMLElement) || !(composer instanceof HTMLElement)) return null;
         const total = Number(surface.dataset.agentTotalCount ?? 0);
         const rendered = Number(surface.dataset.agentRenderedCount ?? 0);
         return total === arguments[0] && rendered > 0 ? { total, rendered } : null;`,
        [expectedTimelineCount],
      );
    });
    const firstTaskUsableMs = performance.now() - processStarted;
    const thousandTimelineReadyMs = performance.now() - taskStarted;

    const composerFrameSamples = [];
    for (let index = 0; index < options.samples; index += 1) {
      const value = index % 2 === 0 ? `性能输入 ${index} 中文` : `perf-input-${index}`;
      const result = await measureTauriComposerFrame(driverUrl, sessionId, value);
      composerFrameSamples.push(result.durationMs);
    }
    const panelResizeFrameSamples = [];
    for (let index = 0; index < options.samples; index += 1) {
      const result = await measureTauriPanelResize(driverUrl, sessionId, index % 2 === 0 ? 8 : -8);
      panelResizeFrameSamples.push(result.durationMs);
    }
    return {
      harnessReadyMs,
      firstTaskUsableMs,
      thousandTimelineReadyMs,
      timelineState,
      composerFrameSamples,
      panelResizeFrameSamples,
      initialRoute: harnessReady.route,
    };
  } finally {
    if (sessionId) {
      await webdriverRequest(driverUrl, "DELETE", `/session/${sessionId}`).catch(() => undefined);
    }
    stopProcessTree(driver);
    stopProcessTree(devServer);
  }
}

async function runNativeInteraction(home) {
  const readyPath = path.join(runDir, "native-performance-ready.txt");
  const processStarted = performance.now();
  const child = track(spawn(debugNativeBinary, [], {
    cwd: repoRoot,
    env: {
      ...process.env,
      LILIA_NATIVE_PREVIEW_HOME: home,
      LILIA_NATIVE_AGENT_DEBUG: "1",
      LILIA_NATIVE_AGENT_DEBUG_ADDR: "127.0.0.1:0",
      LILIA_NATIVE_AGENT_DEBUG_READY: readyPath,
      LILIA_EQUIVALENCE_FIXTURE_ID: fixtureId,
    },
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  }), "performance-native");
  try {
    const address = await waitUntil("Native performance endpoint", 30_000, async () => {
      if (child.exitCode !== null) throw new Error(logs.get("performance-native").join(""));
      if (!existsSync(readyPath)) return null;
      return (await readFile(readyPath, "utf8")).trim() || null;
    });
    await waitUntil("Native performance shell", 30_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.visibleTargetIds.includes("native-preview.app")
        ? response.observation
        : null;
    });
    const harnessReadyMs = performance.now() - processStarted;
    const taskStarted = performance.now();
    await requestNativeDebug(address, {
      command: "click",
      targetId: `native-preview.project.${projectId}`,
    });
    await waitUntil("Native performance task", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.visibleTargetIds.includes(`native-preview.task.${taskId}`)
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: `native-preview.task.${taskId}`,
    });
    let observation = await waitUntil("Native first timeline page", 30_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      const value = response.observation;
      return value.visibleTargetIds.includes("native-preview.task-session.composer.input") &&
          value.timelineEventCount > 0
        ? value
        : null;
    });
    const firstTaskUsableMs = performance.now() - processStarted;
    while (observation.timelineHasMoreBefore) {
      const response = await requestNativeDebug(address, {
        command: "click",
        targetId: "native-preview.task-session.timeline.load-earlier",
      });
      observation = response.observation;
    }
    if (observation.timelineEventCount !== expectedTimelineCount) {
      throw new Error(`Native loaded ${observation.timelineEventCount}/${expectedTimelineCount} timeline events`);
    }
    const thousandTimelineReadyMs = performance.now() - taskStarted;

    const composerFrameSamples = [];
    for (let index = 0; index < options.samples; index += 1) {
      const response = await requestNativeDebug(address, {
        command: "input-frame",
        targetId: "native-preview.task-session.composer.input",
        text: index % 2 === 0 ? `性能输入 ${index} 中文` : `perf-input-${index}`,
      });
      composerFrameSamples.push(response.durationMs);
    }
    if (!observation.visibleTargetIds.includes("native-preview.task-session.inspector")) {
      observation = (await requestNativeDebug(address, {
        command: "click",
        targetId: "native-preview.task-session.inspector.toggle",
      })).observation;
    }
    const panelResizeFrameSamples = [];
    for (let index = 0; index < options.samples; index += 1) {
      const expectedExtent = index % 2 === 0 ? 368 : 352;
      const response = await requestNativeDebug(address, {
        command: "resize-panel-frame",
        extent: String(expectedExtent),
      });
      if (Math.abs(response.observation.inspectorRegionExtent - expectedExtent) > 0.01) {
        throw new Error(
          `Native panel resize observed ${response.observation.inspectorRegionExtent}, ` +
            `expected ${expectedExtent}`,
        );
      }
      panelResizeFrameSamples.push(response.durationMs);
    }
    return {
      harnessReadyMs,
      firstTaskUsableMs,
      thousandTimelineReadyMs,
      timelineState: {
        total: observation.timelineEventCount,
        hasMoreBefore: observation.timelineHasMoreBefore,
      },
      composerFrameSamples,
      panelResizeFrameSamples,
    };
  } finally {
    stopProcessTree(child);
  }
}

async function startProbeServer(label) {
  const child = track(spawn(probeBinary, ["server"], {
    cwd: repoRoot,
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  }), `${label}-probe`);
  const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });
  const iterator = lines[Symbol.asyncIterator]();

  async function readResponse(timeoutMs) {
    let timeout;
    try {
      const next = await Promise.race([
        iterator.next(),
        new Promise((_, reject) => {
          timeout = setTimeout(
            () => reject(new Error("performance probe response timed out")),
            timeoutMs,
          );
        }),
      ]);
      if (next.done) throw new Error("performance probe exited before returning a response");
      return JSON.parse(next.value);
    } finally {
      clearTimeout(timeout);
    }
  }

  const ready = await readResponse(5_000);
  if (ready.ready !== true) throw new Error("performance probe did not report readiness");
  return {
    async request(request, timeoutMs = 35_000) {
      child.stdin.write(`${JSON.stringify(request)}\n`);
      const response = await readResponse(timeoutMs);
      if (!response.ok) throw new Error(`performance probe failed: ${response.error}`);
      return response.result;
    },
    close() {
      if (child.exitCode === null && !child.killed) {
        child.stdin.end(`${JSON.stringify({ command: "exit" })}\n`);
      }
      lines.close();
      stopProcessTree(child);
    },
  };
}

async function measureReleaseHost(label, binary, homeEnvironment) {
  const coldStartSamples = [];
  const idleCpuSamples = [];
  const idleWorkingSetSamples = [];
  const idlePrivateBytesSamples = [];
  const processCounts = [];
  const probe = await startProbeServer(label);
  try {
    for (let index = 0; index < options.coldStartSamples; index += 1) {
      const home = path.join(runDir, `${label}-release-home-${index}`);
      await seedHome(home, `${label}-release-${index}`);
      const started = performance.now();
      const child = track(spawn(binary, [], {
        cwd: repoRoot,
        env: { ...process.env, [homeEnvironment]: home },
        stdio: ["ignore", "pipe", "pipe"],
        windowsHide: true,
      }), `${label}-release-${index}`);
      try {
        const window = await probe.request({
          command: "wait-window",
          pid: child.pid,
          timeout_ms: 30_000,
        });
        coldStartSamples.push(performance.now() - started);
        if (index === options.coldStartSamples - 1) {
          await delay(options.idleWarmupMs);
          for (let sampleIndex = 0; sampleIndex < options.idleSamples; sampleIndex += 1) {
            const before = await probe.request({ command: "sample-tree", pid: child.pid });
            await delay(options.idleIntervalMs);
            const after = await probe.request({ command: "sample-tree", pid: child.pid });
            if (before.inaccessiblePids.length > 0 || after.inaccessiblePids.length > 0) {
              throw new Error(
                `${label} process tree sampling was incomplete: ` +
                  `${[...new Set([...before.inaccessiblePids, ...after.inaccessiblePids])].join(", ")}`,
              );
            }
            const delta = processSampleDelta(before, after, os.cpus().length);
            idleCpuSamples.push(delta.cpuPercentOfMachine);
            idleWorkingSetSamples.push(delta.workingSetBytes);
            idlePrivateBytesSamples.push(delta.privateBytes);
            processCounts.push(delta.processCount);
          }
        }
        if (window.width <= 0 || window.height <= 0) {
          throw new Error(`${label} release window had no area`);
        }
      } finally {
        stopProcessTree(child);
      }
    }
  } finally {
    probe.close();
  }
  const binaryArtifacts = [binary];
  if (label === "native") {
    const hostLibrary = path.join(path.dirname(binary), "lilia_native_host.dll");
    if (!existsSync(hostLibrary)) {
      throw new Error(`native host library is missing: ${hostLibrary}`);
    }
    binaryArtifacts.push(hostLibrary);
  }
  return {
    coldStartMeasurement: "prestarted-native-probe-observed-visible-window",
    coldStartSamples,
    idleCpuSamples,
    idleWorkingSetSamples,
    idlePrivateBytesSamples,
    processCounts,
    binaryArtifacts: binaryArtifacts.map((artifact) => ({
      path: artifact,
      bytes: statSync(artifact).size,
    })),
    executableBytes: binaryArtifacts.reduce(
      (total, artifact) => total + statSync(artifact).size,
      0,
    ),
  };
}

async function buildApplications(devUrl) {
  const configPath = path.join(repoRoot, "apps", "desktop", "src-tauri", "tauri.conf.json");
  const config = JSON.parse(await readFile(configPath, "utf8"));
  const debugConfig = {
    ...config,
    identifier: "com.lilia.desktop.performance.debug",
    build: { ...config.build, devUrl },
  };
  await run("cargo", [
    "build", "--locked", "-p", "lilia", "--features", "agent-debug-webdriver",
  ], { env: { ...process.env, TAURI_CONFIG: JSON.stringify(debugConfig) } });
  await run("cargo", ["build", "--locked", "-p", "lilia-native-preview"]);
  if (!options.interactionOnly) {
    const yarn = yarnInvocation(["verify:desktop:build"]);
    await run(yarn.command, yarn.arguments);
    const releaseConfig = {
      ...config,
      identifier: "com.lilia.desktop.performance.release",
    };
    await run("cargo", ["build", "--locked", "--release", "-p", "lilia"], {
      env: { ...process.env, TAURI_CONFIG: JSON.stringify(releaseConfig) },
    });
    await run("cargo", ["build", "--locked", "--release", "-p", "lilia-native-preview"]);
    await run("cargo", [
      "build", "--locked", "-p", "lilia-native-preview", "--example", "native_performance_probe",
    ]);
  }
}

function ensureBinaries() {
  const required = [debugTauriBinary, debugNativeBinary];
  if (!options.interactionOnly) required.push(releaseTauriBinary, releaseNativeBinary, probeBinary);
  const missing = required.filter((candidate) => !existsSync(candidate));
  if (missing.length > 0) throw new Error(`performance binaries are missing: ${missing.join("; ")}`);
}

async function sourceFingerprint(root) {
  const revision = spawnSync("git", ["rev-parse", "HEAD"], {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
  });
  const status = spawnSync("git", ["status", "--porcelain=v1"], {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
  });
  if (revision.status !== 0 || status.status !== 0) {
    return { root, revision: null, dirty: null, fingerprint: null };
  }
  const listed = spawnSync("git", ["ls-files", "--cached", "--others", "--exclude-standard"], {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
    maxBuffer: 16 * 1024 * 1024,
  });
  const hash = createHash("sha256");
  if (listed.status === 0) {
    for (const relative of listed.stdout.split(/\r?\n/).filter(Boolean).sort()) {
      const file = path.join(root, relative);
      if (!existsSync(file) || !statSync(file).isFile()) continue;
      hash.update(relative.replaceAll("\\", "/"));
      hash.update("\0");
      hash.update(await readFile(file));
      hash.update("\0");
    }
  }
  return {
    root,
    revision: revision.stdout.trim(),
    dirty: status.stdout.trim().length > 0,
    fingerprint: listed.status === 0 ? hash.digest("hex") : null,
  };
}

function windowsEnvironment() {
  if (process.platform !== "win32") return { supported: false, platform: process.platform };
  const script = [
    "$os=Get-CimInstance Win32_OperatingSystem",
    "$cpu=Get-CimInstance Win32_Processor | Select-Object -First 1",
    "$gpu=Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion,DriverDate",
    "$power=(powercfg /getactivescheme | Out-String).Trim()",
    "[pscustomobject]@{osCaption=$os.Caption;osVersion=$os.Version;osBuild=$os.BuildNumber;cpu=$cpu.Name;logicalProcessors=$cpu.NumberOfLogicalProcessors;gpu=$gpu;powerPlan=$power}|ConvertTo-Json -Depth 4 -Compress",
  ].join("; ");
  const result = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {
    cwd: repoRoot,
    encoding: "utf8",
    timeout: 30_000,
    windowsHide: true,
  });
  if (result.status !== 0) throw new Error(`failed to capture Windows environment: ${result.stderr}`);
  return { supported: true, ...JSON.parse(result.stdout.trim().split(/\r?\n/).at(-1)) };
}

function installerArtifactBytes(kind) {
  const directory = path.join(repoRoot, "target", "release", "bundle", "nsis");
  if (!existsSync(directory)) return null;
  const result = spawnSync("powershell.exe", [
    "-NoProfile", "-NonInteractive", "-Command",
    `$item=Get-ChildItem -LiteralPath $env:LILIA_PERF_BUNDLE -Filter '*${kind}*.exe' | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1; if($item){[pscustomobject]@{path=$item.FullName;bytes=$item.Length}|ConvertTo-Json -Compress}`,
  ], {
    cwd: repoRoot,
    env: { ...process.env, LILIA_PERF_BUNDLE: directory },
    encoding: "utf8",
    windowsHide: true,
  });
  return result.status === 0 && result.stdout.trim() ? JSON.parse(result.stdout.trim()) : null;
}

function hostReport(interaction, release) {
  return {
    harnessReadyMs: interaction.harnessReadyMs,
    firstTaskUsableMs: interaction.firstTaskUsableMs,
    thousandTimelineReadyMs: interaction.thousandTimelineReadyMs,
    timelineState: interaction.timelineState,
    composerFrameMs: summarizeSamples(interaction.composerFrameSamples),
    panelResizeFrameMs: summarizeSamples(interaction.panelResizeFrameSamples),
    raw: {
      composerFrameMs: interaction.composerFrameSamples,
      panelResizeFrameMs: interaction.panelResizeFrameSamples,
    },
    ...(release ? {
      coldStartMs: summarizeSamples(release.coldStartSamples),
      idleCpuPercent: summarizeSamples(release.idleCpuSamples),
      idleWorkingSetBytes: summarizeSamples(release.idleWorkingSetSamples),
      idlePrivateBytes: summarizeSamples(release.idlePrivateBytesSamples),
      processCount: summarizeSamples(release.processCounts),
      executableBytes: release.executableBytes,
      rawRelease: release,
    } : {}),
  };
}

async function main() {
  if (process.platform !== "win32") throw new Error("Native performance gate currently requires Windows 11");
  await mkdir(runDir, { recursive: true });
  summary.environment = windowsEnvironment();
  summary.source = {
    lilia: await sourceFingerprint(repoRoot),
    nanaUi: await sourceFingerprint(path.resolve(repoRoot, "..", "sena-nana", "NanaUI")),
  };
  const devServerPlan = await createAgentDebugDevServerPlan(process.env);
  summary.environment.devUrl = devServerPlan.devUrl;
  summary.environment.agentDebugTools = await ensureAgentDebugTools();
  if (!options.skipBuild) await buildApplications(devServerPlan.devUrl);
  ensureBinaries();

  const tauriHome = path.join(runDir, "tauri-interaction-home");
  const nativeHome = path.join(runDir, "native-interaction-home");
  const tauriSeed = await seedHome(tauriHome, "tauri-performance");
  const nativeSeed = await seedHome(nativeHome, "native-performance");
  if (tauriSeed.manifestSha256 !== nativeSeed.manifestSha256) {
    throw new Error("performance homes were not seeded from the same fixture manifest");
  }
  summary.manifestSha256 = tauriSeed.manifestSha256;
  const tauriInteraction = await runTauriInteraction(devServerPlan, tauriHome);
  const nativeInteraction = await runNativeInteraction(nativeHome);
  let tauriRelease = null;
  let nativeRelease = null;
  if (!options.interactionOnly) {
    tauriRelease = await measureReleaseHost("tauri", releaseTauriBinary, "LILIA_HOME");
    nativeRelease = await measureReleaseHost(
      "native",
      releaseNativeBinary,
      "LILIA_NATIVE_PREVIEW_HOME",
    );
  }
  summary.tauri = hostReport(tauriInteraction, tauriRelease);
  summary.native = hostReport(nativeInteraction, nativeRelease);
  summary.artifacts.installers = {
    tauri: installerArtifactBytes("Lilia"),
    native: installerArtifactBytes("Native"),
  };

  const officialSampleVolume = !options.interactionOnly &&
    options.samples >= 30 && options.coldStartSamples >= 5 && options.idleSamples >= 5;
  if (officialSampleVolume) {
    summary.gate = evaluateNativePerformanceGate({ tauri: summary.tauri, native: summary.native });
    summary.status = summary.gate.status;
    if (summary.gate.status === "failed") {
      const failedChecks = summary.gate.checks
        .filter((check) => !check.passed)
        .map((check) => `${check.id}: ${check.actual} ${check.unit} (limit ${check.limit} ${check.unit})`);
      summary.message = `performance gate failed: ${failedChecks.join("; ")}`;
    }
  } else {
    summary.gate = {
      status: "not-evaluated",
      reason: "official gate requires full mode, at least 30 interaction samples, 5 cold starts and 5 idle intervals",
    };
    summary.status = "passed";
  }
}

await mkdir(runDir, { recursive: true });
try {
  await main();
} catch (error) {
  summary.status = "failed";
  summary.message = error instanceof Error ? error.message : String(error);
  process.exitCode = 1;
} finally {
  for (const child of children) stopProcessTree(child);
  await persistLogs();
  summary.finishedAt = new Date().toISOString();
  summary.artifacts.summary = await writeJson("summary.json", summary);
}

if (summary.status === "failed") {
  throw new Error(`${summary.message ?? "performance gate failed without a diagnostic"}\nPerformance artifacts: ${runDir}`);
}
console.log(
  options.smoke || options.interactionOnly
    ? `Native performance harness smoke passed (official gate not evaluated): ${runDir}`
    : `Native performance gate ${summary.status}: ${runDir}`,
);

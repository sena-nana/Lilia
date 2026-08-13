import { spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  createAgentDebugChildEnv,
  createAgentDebugDevServerPlan,
  findAvailablePort,
} from "../apps/desktop/agent-debug/dev-server.mjs";
import { ensureAgentDebugTools } from "../apps/desktop/agent-debug/verify-agent-debug.mjs";
import {
  assertEquivalent,
  requestNativeDebug,
  waitUntil,
} from "./ui-equivalence-lib.mjs";

const repoRoot = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const runId = new Date().toISOString().replace(/[:.]/g, "-");
const runDir = path.join(repoRoot, "agent-debug-runs", `equivalence-p0-${runId}`);
const fixturePath = path.join(repoRoot, "tests", "equivalence", "p0-v1.json");
const fixture = JSON.parse(await readFile(fixturePath, "utf8"));
const fixtureId = fixture.fixtureId;
const projectId = `${fixtureId}-project-alpha`;
const taskId = `${fixtureId}-task-root`;
const goalInteraction = fixture.interactions.goal;
const todoInteraction = fixture.interactions.todo;
const roadmapInteraction = fixture.interactions.roadmap;
const memoryInteraction = fixture.interactions.memory;
const memorySettingsInteraction = fixture.interactions.memorySettings;
const conversationSuggestionsInteraction = fixture.interactions.conversationSuggestions;
const automationInteraction = fixture.interactions.automation;
const skillInteraction = fixture.interactions.skill;
const pluginInteraction = fixture.interactions.plugin;
const hookInteraction = fixture.interactions.hook;
const mcpInteraction = fixture.interactions.mcp;
const memoryTagsText = memoryInteraction.tags.join(", ");
const draft = "同语料 Composer 草稿：中文 IME / markdown **fixed**";
const discardedConversationDraft = "同语料未发送新对话草稿：关闭后不得落库";
const expectedDraftBytes = Buffer.byteLength(draft, "utf8");
const tauriHome = path.join(runDir, "tauri-home");
const nativeHome = path.join(runDir, "native-home");
const tauriBinary = path.join(repoRoot, "target", "debug", process.platform === "win32" ? "lilia.exe" : "lilia");
const nativeBinary = path.join(
  repoRoot,
  "target",
  "debug",
  process.platform === "win32" ? "lilia-native-preview.exe" : "lilia-native-preview",
);
const logs = new Map();
const summary = {
  status: "running",
  runId,
  runDir,
  fixtureId,
  fixturePath,
  draftBytes: expectedDraftBytes,
  checks: [],
  artifacts: {},
};
let seededMcpConfigurationSha256 = null;

class VisualGateBlockedError extends Error {
  constructor(message) {
    super(message);
    this.name = "VisualGateBlockedError";
  }
}

function track(child, label) {
  const entries = [];
  logs.set(label, entries);
  child.stdout?.on("data", (chunk) => entries.push(chunk.toString("utf8")));
  child.stderr?.on("data", (chunk) => entries.push(chunk.toString("utf8")));
  return child;
}

function stopProcessTree(child) {
  if (!child || child.exitCode !== null || child.killed) return;
  if (process.platform === "win32" && child.pid) {
    spawn("taskkill", ["/PID", String(child.pid), "/T", "/F"], {
      stdio: "ignore",
      windowsHide: true,
    });
    return;
  }
  child.kill();
}

function run(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: repoRoot,
      env: process.env,
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
      ...options,
    });
    const output = [];
    child.stdout?.on("data", (chunk) => output.push(chunk.toString("utf8")));
    child.stderr?.on("data", (chunk) => output.push(chunk.toString("utf8")));
    child.once("error", reject);
    child.once("exit", (code) => {
      const text = output.join("");
      if (code === 0) resolve(text);
      else reject(new Error(`${command} ${args.join(" ")} failed with exit code ${code}\n${text}`));
    });
  });
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
  await writeJson(`${identity}-seed.json`, result);
  return result;
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
            // The raw response is retained in the error below.
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

async function buildApplications(devUrl) {
  const configPath = path.join(repoRoot, "apps", "desktop", "src-tauri", "tauri.conf.json");
  const config = JSON.parse(await readFile(configPath, "utf8"));
  config.identifier = "com.lilia.desktop.equivalence.p0";
  config.build = { ...config.build, devUrl };
  await run(
    "cargo",
    ["build", "--locked", "-p", "lilia", "--features", "agent-debug-webdriver"],
    { env: { ...process.env, TAURI_CONFIG: JSON.stringify(config) } },
  );
  await run("cargo", ["build", "--locked", "-p", "lilia-native-preview"]);
  if (!existsSync(tauriBinary) || !existsSync(nativeBinary)) {
    throw new Error(`debug binaries missing: ${tauriBinary}; ${nativeBinary}`);
  }
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

async function tauriNavigate(driverUrl, sessionId, route) {
  await executeWebDriver(
    driverUrl,
    sessionId,
    `
      window.history.pushState({}, "", arguments[0]);
      window.dispatchEvent(new PopStateEvent("popstate"));
      return window.location.pathname + window.location.search;
    `,
    [route],
  );
}

async function tauriObserve(driverUrl, sessionId) {
  return executeWebDriver(
    driverUrl,
    sessionId,
    "return window.__liliaAgentDebug?.observe?.() ?? null;",
  );
}

async function tauriAct(driverUrl, sessionId, action) {
  return executeWebDriver(
    driverUrl,
    sessionId,
    "return window.__liliaAgentDebug.act(arguments[0]);",
    [action],
  );
}

async function tauriSnapshot(driverUrl, sessionId) {
  return executeWebDriver(
    driverUrl,
    sessionId,
    "return window.__liliaAgentDebug.equivalenceSnapshot(arguments[0]);",
    [fixtureId],
  );
}

async function tauriInvoke(driverUrl, sessionId, command, args = {}) {
  return executeWebDriver(
    driverUrl,
    sessionId,
    "return window.__TAURI_INTERNALS__.invoke(arguments[0], arguments[1]);",
    [command, args],
  );
}

async function tauriScreenshot(driverUrl, sessionId, name) {
  const response = await webdriverRequest(driverUrl, "GET", `/session/${sessionId}/screenshot`);
  const target = path.join(runDir, name);
  await writeFile(target, Buffer.from(response.value, "base64"));
  return target;
}

function hasTauriTarget(observation, id) {
  return observation?.elements?.some((element) => element.id === id && element.visible);
}

function hasEnabledTauriTarget(observation, id) {
  return observation?.elements?.some(
    (element) => element.id === id && element.visible && element.enabled,
  );
}

function visibleTauriTargetMatching(observation, predicate) {
  return observation?.elements?.find(
    (element) => element.visible && typeof element.id === "string" && predicate(element.id),
  )?.id ?? null;
}

function hasRoadmapAndMemoryFacts(snapshot) {
  const milestone = snapshot.roadmap.find(
    (item) => item.projectId === projectId && item.title === roadmapInteraction.title,
  );
  const memory = snapshot.memories.find(
    (item) => item.title === memoryInteraction.title &&
      item.scope === memoryInteraction.scope &&
      item.projectId === (memoryInteraction.scope === "project" ? projectId : null),
  );
  return milestone?.taskIds.includes(roadmapInteraction.taskId) &&
    memory?.tags.join("\u0000") === [...memoryInteraction.tags].sort().join("\u0000") &&
    memory?.bodySha256?.length === 64;
}

function hasMemorySettingsFacts(snapshot) {
  return snapshot.memorySettings?.enabled === memorySettingsInteraction.enabled &&
    snapshot.memorySettings?.baselineInjectionEnabled ===
      memorySettingsInteraction.baselineInjectionEnabled &&
    snapshot.memorySettings?.cooldownTurns === memorySettingsInteraction.cooldownTurns;
}

function hasInitialConversationSuggestionSettingsFacts(snapshot) {
  return snapshot.conversationSuggestions?.enabled !==
      conversationSuggestionsInteraction.enabled &&
    snapshot.conversationSuggestions?.source === conversationSuggestionsInteraction.source;
}

function hasConversationSuggestionSettingsFacts(snapshot) {
  return snapshot.conversationSuggestions?.enabled ===
      conversationSuggestionsInteraction.enabled &&
    snapshot.conversationSuggestions?.source === conversationSuggestionsInteraction.source;
}

function hasSameConversationAuthority(left, right) {
  return JSON.stringify(left.tasks) === JSON.stringify(right.tasks) &&
    JSON.stringify(left.conversations) === JSON.stringify(right.conversations);
}

function hasAutomationDraftFacts(snapshot, { published = false, enabled = false } = {}) {
  const workflow = snapshot.automations.find(
    (item) => item.name === automationInteraction.name,
  );
  const [node] = workflow?.nodes ?? [];
  return workflow?.published === published &&
    workflow?.enabled === enabled &&
    workflow?.scope.includeInbox === automationInteraction.includeInbox &&
    workflow?.nodes.length === 1 &&
    workflow?.edges.length === 0 &&
    node?.kind === "trigger" &&
    node?.title === automationInteraction.nodeTitle &&
    node?.configSha256?.length === 64;
}

function hasSeededGoalTodoFacts(snapshot) {
  const goalFixture = fixture.goals.find((item) => item.taskId === goalInteraction.taskId);
  const todoFixture = fixture.todos.find((item) => item.taskId === todoInteraction.taskId);
  const goal = snapshot.goals.find((item) => item.taskId === goalInteraction.taskId);
  const todo = snapshot.todos.find((item) => item.taskId === todoInteraction.taskId);
  return goalFixture &&
    todoFixture &&
    goal?.objectiveSha256?.length === 64 &&
    goal?.status === "active" &&
    goal?.tokenBudget === goalFixture.tokenBudget &&
    goal?.tokensUsed === 0 &&
    todo?.textSha256?.length === 64 &&
    todo?.done === false &&
    todo?.source === "lilia" &&
    todo?.priority === todoFixture.priority &&
    todo?.guideStatus === "pending" &&
    todo?.attachmentCount === 0;
}

function hasSeededMcpFacts(snapshot) {
  const server = snapshot.mcpServers?.find(
    (item) => item.serverId === mcpInteraction.serverId,
  );
  return snapshot.mcpRegistryRevision === 1 &&
    server?.transport === "stdio" &&
    server?.enabled === false &&
    server?.registered === true &&
    server?.editable === true &&
    server?.configurationSha256?.length === 64 &&
    server?.credentials?.length === 0;
}

function hasSeededSkillFacts(snapshot) {
  const skill = snapshot.skills?.find((item) => item.skillId === skillInteraction.skillId);
  return snapshot.skillsRegistryRevision === 1 &&
    skill?.scope === "user" &&
    skill?.enabled === true &&
    skill?.editable === true &&
    skill?.runtimeAvailable === true &&
    skill?.descriptionSha256?.length === 64;
}

function hasUpdatedSkillFacts(snapshot) {
  const skill = snapshot.skills?.find((item) => item.skillId === skillInteraction.skillId);
  return snapshot.skillsRegistryRevision === 2 &&
    skill?.enabled === skillInteraction.enabled &&
    skill?.editable === true &&
    skill?.runtimeAvailable === false &&
    skill?.descriptionSha256?.length === 64;
}

function hasSeededPluginFacts(snapshot) {
  const plugin = snapshot.plugins?.find((item) => item.pluginId === pluginInteraction.pluginId);
  return snapshot.pluginsRegistryRevision === 2 &&
    plugin?.enabled === true &&
    plugin?.runtimeAvailable === true &&
    plugin?.packageSha256?.length === 64 &&
    plugin?.skillCount === 1 &&
    plugin?.hookCount === 0 &&
    plugin?.mcpServerCount === 0;
}

function hasUpdatedPluginFacts(snapshot) {
  const plugin = snapshot.plugins?.find((item) => item.pluginId === pluginInteraction.pluginId);
  return snapshot.pluginsRegistryRevision === 3 &&
    plugin?.enabled === pluginInteraction.enabled &&
    plugin?.runtimeAvailable === false &&
    plugin?.packageSha256?.length === 64;
}

function hasSeededHookFacts(snapshot) {
  const source = snapshot.hookSources?.find((item) => item.scope === hookInteraction.scope);
  const handler = source?.handlers?.find(
    (item) => item.id === `${fixtureId}-hook-prompt`,
  );
  return source?.revision === 3 &&
    source?.enabled === true &&
    handler?.event === "UserPromptSubmit" &&
    handler?.matcher === "*equivalence*" &&
    handler?.configurationSha256?.length === 64;
}

function hasUpdatedHookFacts(snapshot) {
  const source = snapshot.hookSources?.find((item) => item.scope === hookInteraction.scope);
  return source?.revision === 4 &&
    source?.enabled === hookInteraction.enabled &&
    source?.handlers?.length === 1 &&
    source.handlers[0]?.configurationSha256?.length === 64;
}

function hasUpdatedMcpFacts(snapshot) {
  const server = snapshot.mcpServers?.find(
    (item) => item.serverId === mcpInteraction.serverId,
  );
  return snapshot.mcpRegistryRevision === 2 &&
    server?.transport === "stdio" &&
    server?.enabled === false &&
    server?.configurationSha256?.length === 64 &&
    server.configurationSha256 !== seededMcpConfigurationSha256;
}

function hasClearedGoalTodoFacts(snapshot) {
  return !snapshot.goals.some((item) => item.taskId === goalInteraction.taskId) &&
    !snapshot.todos.some((item) => item.taskId === todoInteraction.taskId);
}

function hasFinalEquivalenceFacts(snapshot) {
  return hasClearedGoalTodoFacts(snapshot) &&
    hasRoadmapAndMemoryFacts(snapshot) &&
    hasMemorySettingsFacts(snapshot) &&
    hasConversationSuggestionSettingsFacts(snapshot) &&
    hasAutomationDraftFacts(snapshot, { published: true, enabled: true }) &&
    hasUpdatedSkillFacts(snapshot) &&
    hasUpdatedPluginFacts(snapshot) &&
    hasUpdatedHookFacts(snapshot) &&
    hasUpdatedMcpFacts(snapshot);
}

async function runTauri(devServerPlan) {
  const driverPort = await findAvailablePort(4450);
  const driverUrl = `http://127.0.0.1:${driverPort}`;
  const childEnv = {
    ...createAgentDebugChildEnv(process.env, devServerPlan.devUrl, devServerPlan.port),
    LILIA_HOME: tauriHome,
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
      devServer = track(
        spawn(process.execPath, [vite, "--host", "127.0.0.1"], {
          cwd: path.join(repoRoot, "apps", "desktop"),
          env: childEnv,
          stdio: ["ignore", "pipe", "pipe"],
          windowsHide: true,
        }),
        "tauri-vite",
      );
      await waitUntil("Tauri Vite server", 30_000, async () => {
        if (devServer.exitCode !== null) throw new Error(logs.get("tauri-vite").join(""));
        return getUrl(devServerPlan.devUrl).catch(() => false);
      });
    }

    driver = track(
      spawn("tauri-driver", ["--port", String(driverPort)], {
        env: { ...childEnv, MSEDGEDRIVER_TELEMETRY_OPTOUT: "1" },
        stdio: ["ignore", "pipe", "pipe"],
        windowsHide: true,
      }),
      "tauri-driver",
    );
    await waitUntil("tauri-driver", 20_000, async () => {
      if (driver.exitCode !== null) throw new Error(logs.get("tauri-driver").join(""));
      return webdriverRequest(driverUrl, "GET", "/status").then(() => true, () => false);
    });
    const session = await webdriverRequest(driverUrl, "POST", "/session", {
      capabilities: {
        alwaysMatch: {
          browserName: "wry",
          "tauri:options": { application: tauriBinary, args: [], env: childEnv },
        },
      },
    });
    sessionId = session?.value?.sessionId;
    if (!sessionId) throw new Error(`Tauri WebDriver did not return a session: ${JSON.stringify(session)}`);
    await webdriverRequest(driverUrl, "POST", `/session/${sessionId}/window/rect`, {
      width: 1600,
      height: 1000,
    });

    await waitUntil("Tauri Agent Debug harness", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return observation?.enabled ? observation : null;
    });
    const todoFixture = fixture.todos.find((item) => item.taskId === todoInteraction.taskId);
    if (!todoFixture) throw new Error("fixture is missing the Todo seed input");
    const startupSnapshot = await tauriSnapshot(driverUrl, sessionId);
    if (!startupSnapshot.todos.some((item) => item.taskId === todoInteraction.taskId)) {
      await tauriInvoke(driverUrl, sessionId, "todo_create", {
        taskId: todoFixture.taskId,
        text: todoFixture.text,
        priority: todoFixture.priority,
        attachments: [],
      });
      await waitUntil("Tauri Todo fixture authority", 20_000, async () => {
        const snapshot = await tauriSnapshot(driverUrl, sessionId);
        return hasSeededGoalTodoFacts(snapshot) ? snapshot : null;
      });
    }
    const initial = await tauriSnapshot(driverUrl, sessionId);
    await writeJson("tauri-initial.json", initial);

    const projectTarget = `sidebar.project.${projectId}.row`;
    await waitUntil("Tauri new conversation entry", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "sidebar.new-chat") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: "sidebar.new-chat" });
    await waitUntil("Tauri transient conversation draft", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "chat.composer.input") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "type",
      target: "chat.composer.input",
      text: discardedConversationDraft,
      clear: true,
    });
    const tauriDraftSnapshot = await tauriSnapshot(driverUrl, sessionId);
    if (!hasSameConversationAuthority(initial, tauriDraftSnapshot)) {
      throw new Error("Tauri unsent conversation draft changed Product authority");
    }
    await waitUntil("Tauri fixture project", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasTauriTarget(observation, projectTarget) ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: projectTarget });
    const tauriDiscardedDraftSnapshot = await tauriSnapshot(driverUrl, sessionId);
    if (!hasSameConversationAuthority(initial, tauriDiscardedDraftSnapshot)) {
      throw new Error("Tauri discarded conversation draft changed Product authority");
    }
    const taskTarget = `sidebar.task.${taskId}`;
    await waitUntil("Tauri fixture task", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasTauriTarget(observation, taskTarget) ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: taskTarget });
    const timelineTargets = fixture.timeline.map((event) => `timeline.event.${event.id}`);
    await waitUntil("Tauri fixed timeline", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      const ids = new Set(observation?.elements?.map((element) => element.id));
      return timelineTargets.every((id) => ids.has(id)) && hasTauriTarget(observation, "chat.composer.input")
        ? observation
        : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "type",
      target: "chat.composer.input",
      text: draft,
      clear: true,
    });
    await waitUntil("Tauri composer authority", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      const composer = snapshot.composers.find((item) => item.taskId === taskId);
      return composer?.contentBytes === expectedDraftBytes ? snapshot : null;
    });

    await waitUntil("Tauri Goal and Todo controls", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      const todoDeleteTarget = visibleTauriTargetMatching(
        observation,
        (id) => id.startsWith("todo.guide.") && id.endsWith(".delete"),
      );
      return hasEnabledTauriTarget(observation, "todo.goal.clear") && todoDeleteTarget
        ? { observation, todoDeleteTarget }
        : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: "todo.goal.clear" });
    await waitUntil("Tauri Goal clear authority", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return snapshot.goals.some((item) => item.taskId === goalInteraction.taskId)
        ? null
        : snapshot;
    });
    const tauriTodoDeleteTarget = await waitUntil("Tauri Todo delete control", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return visibleTauriTargetMatching(
        observation,
        (id) => id.startsWith("todo.guide.") && id.endsWith(".delete"),
      );
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: tauriTodoDeleteTarget });
    await waitUntil("Tauri Goal and Todo authority", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasClearedGoalTodoFacts(snapshot) ? snapshot : null;
    });

    const projectMoreTarget = `sidebar.project.${projectId}.more`;
    await tauriAct(driverUrl, sessionId, { type: "focus", target: projectMoreTarget });
    await waitUntil("Tauri project menu trigger", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasTauriTarget(observation, projectMoreTarget) ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: projectMoreTarget });
    await waitUntil("Tauri project menu", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasTauriTarget(observation, "context-menu.item.open-project") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: "context-menu.item.open-project",
    });
    await waitUntil("Tauri Roadmap tab", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasTauriTarget(observation, "view-tabs.roadmap") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: "view-tabs.roadmap" });
    await waitUntil("Tauri Roadmap editor", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasTauriTarget(observation, "roadmap.create.title") &&
        hasTauriTarget(observation, "roadmap.create.submit")
        ? observation
        : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "type",
      target: "roadmap.create.title",
      text: roadmapInteraction.title,
      clear: true,
    });
    await waitUntil("Tauri Roadmap create enabled", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "roadmap.create.submit") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: "roadmap.create.submit" });
    await waitUntil("Tauri Roadmap milestone authority", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return snapshot.roadmap.some(
        (item) => item.projectId === projectId && item.title === roadmapInteraction.title,
      )
        ? snapshot
        : null;
    });
    const roadmapTaskTarget = await waitUntil("Tauri Roadmap task link", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return visibleTauriTargetMatching(
        observation,
        (id) => id.startsWith("roadmap.milestone.") &&
          id.endsWith(`.task.${roadmapInteraction.taskId}.toggle`),
      );
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: roadmapTaskTarget });

    await waitUntil("Tauri Roadmap authority", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      const milestone = snapshot.roadmap.find(
        (item) => item.projectId === projectId && item.title === roadmapInteraction.title,
      );
      return milestone?.taskIds.includes(roadmapInteraction.taskId) ? snapshot : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: "view-tabs.memory" });
    const tauriMemoryAddTarget = `memory.${memoryInteraction.scope}.add`;
    await waitUntil("Tauri Memory editor", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasTauriTarget(observation, tauriMemoryAddTarget) &&
        hasTauriTarget(observation, "memory.form.title") &&
        hasTauriTarget(observation, "memory.form.body")
        ? observation
        : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: tauriMemoryAddTarget });
    for (const [target, text] of [
      ["memory.form.title", memoryInteraction.title],
      ["memory.form.body", memoryInteraction.body],
      ["memory.form.tags", memoryTagsText],
    ]) {
      await tauriAct(driverUrl, sessionId, { type: "type", target, text, clear: true });
    }
    await waitUntil("Tauri Memory save enabled", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "memory.form.save") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: "memory.form.save" });
    await waitUntil("Tauri Roadmap and Memory authority", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasRoadmapAndMemoryFacts(snapshot) ? snapshot : null;
    });
    const tauriMemoryToggleTarget = await waitUntil(
      "Tauri Memory toggle target",
      20_000,
      async () => visibleTauriTargetMatching(
        await tauriObserve(driverUrl, sessionId),
        (id) => id.startsWith("memory.item.") && id.endsWith(".toggle"),
      ),
    );
    await tauriAct(driverUrl, sessionId, { type: "click", target: tauriMemoryToggleTarget });
    await waitUntil("Tauri Memory disabled", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return snapshot.memories.some(
        (item) => item.title === memoryInteraction.title && item.enabled === false,
      ) ? snapshot : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: tauriMemoryToggleTarget });
    await waitUntil("Tauri Memory enabled", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasRoadmapAndMemoryFacts(snapshot) ? snapshot : null;
    });
    const tauriMemoryDeleteTarget = await waitUntil(
      "Tauri Memory delete target",
      20_000,
      async () => visibleTauriTargetMatching(
        await tauriObserve(driverUrl, sessionId),
        (id) => id.startsWith("memory.item.") && id.endsWith(".delete"),
      ),
    );
    await tauriAct(driverUrl, sessionId, { type: "click", target: tauriMemoryDeleteTarget });
    await waitUntil("Tauri Memory deleted", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return snapshot.memories.every((item) => item.title !== memoryInteraction.title)
        ? snapshot
        : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: tauriMemoryAddTarget });
    for (const [target, text] of [
      ["memory.form.title", memoryInteraction.title],
      ["memory.form.body", memoryInteraction.body],
      ["memory.form.tags", memoryTagsText],
    ]) {
      await tauriAct(driverUrl, sessionId, { type: "type", target, text, clear: true });
    }
    await waitUntil("Tauri recreated Memory save enabled", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "memory.form.save") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: "memory.form.save" });
    await waitUntil("Tauri Memory recreated", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasRoadmapAndMemoryFacts(snapshot) ? snapshot : null;
    });

    await waitUntil("Tauri Memory settings", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "memory.settings.baseline-injection") &&
          hasEnabledTauriTarget(observation, "memory.settings.enabled") &&
          hasEnabledTauriTarget(observation, "memory.settings.cooldown-turns")
        ? observation
        : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "type",
      target: "memory.settings.cooldown-turns",
      text: String(memorySettingsInteraction.cooldownTurns),
      clear: true,
    });
    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: "memory.settings.baseline-injection",
    });
    await waitUntil("Tauri Memory baseline setting", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return snapshot.memorySettings?.baselineInjectionEnabled ===
          memorySettingsInteraction.baselineInjectionEnabled
        ? snapshot
        : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: "memory.settings.enabled",
    });
    await waitUntil("Tauri Memory settings authority", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasMemorySettingsFacts(snapshot) ? snapshot : null;
    });

    await tauriNavigate(driverUrl, sessionId, "/settings?tab=model-config");
    const tauriSuggestionTarget = conversationSuggestionsInteraction.enabled
      ? "settings.suggestions.enabled.on"
      : "settings.suggestions.enabled.off";
    await waitUntil("Tauri conversation suggestion setting", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, tauriSuggestionTarget) ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: tauriSuggestionTarget,
    });
    await waitUntil("Tauri conversation suggestion authority", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasConversationSuggestionSettingsFacts(snapshot) ? snapshot : null;
    });

    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: "sidebar.footer.automations",
    });
    await waitUntil("Tauri Automation workspace", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "automations.new") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: "automations.new" });
    await waitUntil("Tauri Automation workflow name", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasTauriTarget(observation, "automations.workflow.name") ? observation : null;
    });
    await waitUntil("Tauri Automation trigger action", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "automations.node.add.trigger")
        ? observation
        : null;
    });
    let latestAutomationObservation = null;
    try {
      await waitUntil("Tauri Automation scope inspector", 30_000, async () => {
        latestAutomationObservation = await tauriObserve(driverUrl, sessionId);
        return hasEnabledTauriTarget(
            latestAutomationObservation,
            "automations.scope.include-inbox",
          )
          ? latestAutomationObservation
          : null;
      });
    } catch (error) {
      await writeJson("tauri-automation-inspector-timeout.json", latestAutomationObservation);
      throw error;
    }
    await tauriAct(driverUrl, sessionId, {
      type: "type",
      target: "automations.workflow.name",
      text: automationInteraction.name,
      clear: true,
    });
    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: "automations.node.add.trigger",
    });
    await waitUntil("Tauri Automation trigger inspector", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasTauriTarget(observation, "automations.node.title") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "type",
      target: "automations.node.title",
      text: automationInteraction.nodeTitle,
      clear: true,
    });
    await waitUntil("Tauri Automation save enabled", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "automations.workflow.save-draft")
        ? observation
        : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: "automations.workflow.save-draft",
    });
    await waitUntil("Tauri Automation draft authority", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasAutomationDraftFacts(snapshot) ? snapshot : null;
    });
    await waitUntil("Tauri Automation publish enabled", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "automations.workflow.publish")
        ? observation
        : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: "automations.workflow.publish",
    });
    await waitUntil("Tauri Automation published authority", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasAutomationDraftFacts(snapshot, { published: true }) ? snapshot : null;
    });
    await waitUntil("Tauri Automation toggle enabled", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "automations.workflow.toggle-enabled")
        ? observation
        : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: "automations.workflow.toggle-enabled",
    });
    await tauriNavigate(driverUrl, sessionId, "/settings?tab=plugin-skills");
    const tauriSkillEntry = await waitUntil("Tauri seeded Skill entry", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return observation.elements.find(
        (element) => element.id?.startsWith("plugins.entry.skill:user:") && element.enabled,
      )?.id ?? null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: tauriSkillEntry });
    await waitUntil("Tauri Skill toggle action", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "plugins.detail.toggle") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: "plugins.detail.toggle",
    });
    await waitUntil("Tauri Skill authority update", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasUpdatedSkillFacts(snapshot) ? snapshot : null;
    });
    await tauriNavigate(driverUrl, sessionId, "/settings?tab=plugin-packages");
    const tauriPluginEntry = await waitUntil("Tauri seeded Plugin entry", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return observation.elements.find(
        (element) => element.id?.startsWith("plugins.entry.plugin:") && element.enabled,
      )?.id ?? null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: tauriPluginEntry });
    await waitUntil("Tauri Plugin toggle action", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "plugins.detail.toggle") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: "plugins.detail.toggle",
    });
    await waitUntil("Tauri Plugin authority update", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasUpdatedPluginFacts(snapshot) ? snapshot : null;
    });
    await tauriNavigate(driverUrl, sessionId, "/settings?tab=plugin-hooks");
    const tauriHookEntry = "plugins.entry.hook:native-agentkit:user";
    await waitUntil("Tauri seeded Hook source", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, tauriHookEntry) ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: tauriHookEntry });
    await waitUntil("Tauri Hook toggle action", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "plugins.detail.toggle") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: "plugins.detail.toggle",
    });
    await waitUntil("Tauri Hook authority update", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasUpdatedHookFacts(snapshot) ? snapshot : null;
    });
    await tauriNavigate(driverUrl, sessionId, "/settings?tab=plugin-mcp");
    const tauriMcpEntry = `plugins.entry.mcp:${mcpInteraction.serverId}`;
    await waitUntil("Tauri seeded MCP entry", 30_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, tauriMcpEntry) ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: tauriMcpEntry });
    await waitUntil("Tauri MCP edit action", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasEnabledTauriTarget(observation, "plugins.detail.edit") ? observation : null;
    });
    await tauriAct(driverUrl, sessionId, { type: "click", target: "plugins.detail.edit" });
    const tauriMcpEditor = await waitUntil("Tauri MCP editor", 20_000, async () => {
      const observation = await tauriObserve(driverUrl, sessionId);
      return hasTauriTarget(observation, "plugins.mcp-editor.command") &&
          hasTauriTarget(observation, "plugins.mcp-editor.args") &&
          hasEnabledTauriTarget(observation, "plugins.mcp-editor.save")
        ? observation
        : null;
    });
    const tauriMcpId = tauriMcpEditor.elements.find(
      (element) => element.id === "plugins.mcp-editor.name",
    );
    if (!tauriMcpId || tauriMcpId.enabled) {
      throw new Error("Tauri MCP editor did not keep the persisted server ID immutable");
    }
    await tauriAct(driverUrl, sessionId, {
      type: "type",
      target: "plugins.mcp-editor.command",
      text: mcpInteraction.command,
      clear: true,
    });
    await tauriAct(driverUrl, sessionId, {
      type: "type",
      target: "plugins.mcp-editor.args",
      text: mcpInteraction.args.join("\n"),
      clear: true,
    });
    await tauriAct(driverUrl, sessionId, {
      type: "click",
      target: "plugins.mcp-editor.save",
    });
    await waitUntil("Tauri MCP authority update", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasUpdatedMcpFacts(snapshot) ? snapshot : null;
    });
    const final = await waitUntil("Tauri final authority", 20_000, async () => {
      const snapshot = await tauriSnapshot(driverUrl, sessionId);
      return hasFinalEquivalenceFacts(snapshot) ? snapshot : null;
    });
    const screenshot = await tauriScreenshot(driverUrl, sessionId, "tauri-task.png");
    await writeJson("tauri-final.json", final);
    summary.artifacts.tauriScreenshot = screenshot;
    summary.checks.push(
      "Tauri real UI cleared Goal/Todo and persisted the fixed Composer, Roadmap, Memory and conversation suggestion settings, published Automation, disabled the live AgentKit Skill, Plugin and Hook source, and edited Native AgentKit MCP",
    );
    return { initial, final };
  } finally {
    if (sessionId) {
      await webdriverRequest(driverUrl, "DELETE", `/session/${sessionId}`).catch(() => undefined);
    }
    stopProcessTree(driver);
    stopProcessTree(devServer);
  }
}

async function captureNativeWindow(processId, outputPath) {
  if (process.platform !== "win32") throw new Error("Native GPU capture currently requires Windows");
  const script = String.raw`
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class LiliaEquivalenceCapture {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int command);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr dpiContext);
  public static IntPtr Find(int processId, string requiredTitle) {
    IntPtr result = IntPtr.Zero;
    EnumWindows((hWnd, _) => {
      uint owner;
      GetWindowThreadProcessId(hWnd, out owner);
      if (owner != processId || !IsWindowVisible(hWnd)) return true;
      int length = GetWindowTextLength(hWnd);
      if (length <= 0) return true;
      var title = new StringBuilder(length + 1);
      GetWindowText(hWnd, title, title.Capacity);
      if (!title.ToString().Contains(requiredTitle)) return true;
      result = hWnd;
      return false;
    }, IntPtr.Zero);
    return result;
  }
}
"@
$previousDpiContext = [LiliaEquivalenceCapture]::SetThreadDpiAwarenessContext([IntPtr](-4))
if ($previousDpiContext -eq [IntPtr]::Zero) { throw "failed to enable per-monitor DPI awareness" }
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
$targetProcessId = [int]$env:LILIA_EQUIVALENCE_CAPTURE_PID
$deadline = [DateTime]::UtcNow.AddSeconds(15)
$handle = [IntPtr]::Zero
while ([DateTime]::UtcNow -lt $deadline) {
  $handle = [LiliaEquivalenceCapture]::Find($targetProcessId, "LiliaCode Native Preview")
  if ($handle -ne [IntPtr]::Zero) { break }
  Start-Sleep -Milliseconds 100
}
if ($handle -eq [IntPtr]::Zero) { throw "Native Preview has no visible titled window" }
[void][LiliaEquivalenceCapture]::ShowWindow($handle, 9)
[void][LiliaEquivalenceCapture]::SetWindowPos($handle, [IntPtr](-1), 0, 0, 0, 0, 0x0043)
[void][LiliaEquivalenceCapture]::SetForegroundWindow($handle)
$shell = New-Object -ComObject WScript.Shell
[void]$shell.AppActivate($targetProcessId)
Start-Sleep -Milliseconds 800
$rect = New-Object LiliaEquivalenceCapture+RECT
if (-not [LiliaEquivalenceCapture]::GetWindowRect($handle, [ref]$rect)) { throw "GetWindowRect failed" }
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
if ($width -le 0 -or $height -le 0) { throw "Native Preview window geometry is invalid" }
$virtual = [System.Windows.Forms.SystemInformation]::VirtualScreen
if ($rect.Left -lt $virtual.Left -or $rect.Top -lt $virtual.Top -or $rect.Right -gt $virtual.Right -or $rect.Bottom -gt $virtual.Bottom) {
  throw "Native Preview window is not fully inside the virtual screen"
}
$bitmap = New-Object System.Drawing.Bitmap($width, $height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$nonBlackSamples = 0
$neutralSamples = 0
try {
  $source = New-Object System.Drawing.Point($rect.Left, $rect.Top)
  $destination = [System.Drawing.Point]::Empty
  $graphics.CopyFromScreen($source, $destination, $bitmap.Size)
  for ($sampleY = 0; $sampleY -lt 18; $sampleY += 1) {
    $pixelY = [Math]::Min($height - 1, [int](($sampleY + 0.5) * $height / 18))
    for ($sampleX = 0; $sampleX -lt 32; $sampleX += 1) {
      $pixelX = [Math]::Min($width - 1, [int](($sampleX + 0.5) * $width / 32))
      $pixel = $bitmap.GetPixel($pixelX, $pixelY)
      if ($pixel.R -gt 5 -or $pixel.G -gt 5 -or $pixel.B -gt 5) { $nonBlackSamples += 1 }
      $maximum = [Math]::Max($pixel.R, [Math]::Max($pixel.G, $pixel.B))
      $minimum = [Math]::Min($pixel.R, [Math]::Min($pixel.G, $pixel.B))
      $average = ($pixel.R + $pixel.G + $pixel.B) / 3
      if (($maximum - $minimum) -le 18 -and $average -gt 18) { $neutralSamples += 1 }
    }
  }
  $bitmap.Save($env:LILIA_EQUIVALENCE_CAPTURE_PATH, [System.Drawing.Imaging.ImageFormat]::Png)
} finally {
  $graphics.Dispose()
  $bitmap.Dispose()
  [void][LiliaEquivalenceCapture]::SetWindowPos($handle, [IntPtr](-2), 0, 0, 0, 0, 0x0043)
}
[pscustomobject]@{ width = $width; height = $height; nonBlackSamples = $nonBlackSamples; neutralSamples = $neutralSamples } | ConvertTo-Json -Compress
`;
  const result = spawnSync(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script],
    {
      cwd: repoRoot,
      encoding: "utf8",
      timeout: 30_000,
      windowsHide: true,
      env: {
        ...process.env,
        LILIA_EQUIVALENCE_CAPTURE_PID: String(processId),
        LILIA_EQUIVALENCE_CAPTURE_PATH: outputPath,
      },
    },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`Native GPU screenshot failed: ${result.stderr || result.stdout}`);
  }
  const capture = JSON.parse(result.stdout.trim().split(/\r?\n/).at(-1));
  if (capture.nonBlackSamples < 8 || capture.neutralSamples < 32) {
    throw new Error(
      `Native screenshot did not contain the UI surface: ` +
        `${capture.nonBlackSamples} non-black, ${capture.neutralSamples} neutral samples`,
    );
  }
  return { path: outputPath, ...capture };
}

async function runNative() {
  const readyPath = path.join(runDir, "native-ready.txt");
  const child = track(
    spawn(nativeBinary, [], {
      cwd: repoRoot,
      env: {
        ...process.env,
        LILIA_NATIVE_PREVIEW_HOME: nativeHome,
        LILIA_NATIVE_AGENT_DEBUG: "1",
        LILIA_NATIVE_AGENT_DEBUG_ADDR: "127.0.0.1:0",
        LILIA_NATIVE_AGENT_DEBUG_READY: readyPath,
        LILIA_EQUIVALENCE_FIXTURE_ID: fixtureId,
      },
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    }),
    "native-preview",
  );
  try {
    const address = await waitUntil("Native debug endpoint", 30_000, async () => {
      if (child.exitCode !== null) throw new Error(logs.get("native-preview").join(""));
      if (!existsSync(readyPath)) return null;
      const value = (await readFile(readyPath, "utf8")).trim();
      return value || null;
    });
    const initialResponse = await requestNativeDebug(address, {
      command: "equivalence-snapshot",
      fixtureId,
    });
    const initial = initialResponse.snapshot;
    await writeJson("native-initial.json", initial);

    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.new-conversation",
    });
    await waitUntil("Native transient conversation draft", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      const ids = response.observation.visibleTargetIds;
      return ids.includes("native-preview.new-conversation.close") &&
        ids.includes("native-preview.task-session.composer.input")
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "input",
      targetId: "native-preview.task-session.composer.input",
      text: discardedConversationDraft,
    });
    const nativeDraftSnapshot = (await requestNativeDebug(address, {
      command: "equivalence-snapshot",
      fixtureId,
    })).snapshot;
    if (!hasSameConversationAuthority(initial, nativeDraftSnapshot)) {
      throw new Error("Native unsent conversation draft changed Product authority");
    }
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.new-conversation.close",
    });
    const nativeDiscardedDraftSnapshot = (await requestNativeDebug(address, {
      command: "equivalence-snapshot",
      fixtureId,
    })).snapshot;
    if (!hasSameConversationAuthority(initial, nativeDiscardedDraftSnapshot)) {
      throw new Error("Native discarded conversation draft changed Product authority");
    }

    await requestNativeDebug(address, {
      command: "click",
      targetId: `native-preview.project.${projectId}`,
    });
    await waitUntil("Native fixture task", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.visibleTargetIds.includes(`native-preview.task.${taskId}`)
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: `native-preview.task.${taskId}`,
    });
    const timelineTargets = fixture.timeline.map(
      (event) => `native-preview.task-session.timeline.${event.id}`,
    );
    await waitUntil("Native fixed timeline", 30_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      const ids = new Set(response.observation.visibleTargetIds);
      return timelineTargets.every((id) => ids.has(id)) &&
        ids.has("native-preview.task-session.composer.input")
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "input",
      targetId: "native-preview.task-session.composer.input",
      text: draft,
    });
    await waitUntil("Native composer authority", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      const composer = response.snapshot.composers.find((item) => item.taskId === taskId);
      return composer?.contentBytes === expectedDraftBytes ? response.snapshot : null;
    });

    const nativeTodoDeleteTarget = await waitUntil("Native Goal and Todo controls", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      const ids = response.observation.visibleTargetIds;
      const todoDeleteTarget = ids.find(
        (id) => id.startsWith("native-preview.task-session.todo.") && id.endsWith(".delete"),
      );
      return ids.includes("native-preview.task-session.goal.clear") && todoDeleteTarget
        ? todoDeleteTarget
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.task-session.goal.clear",
    });
    await waitUntil("Native Goal clear authority", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return response.snapshot.goals.some((item) => item.taskId === goalInteraction.taskId)
        ? null
        : response.snapshot;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: nativeTodoDeleteTarget,
    });
    await waitUntil("Native Goal and Todo authority", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasClearedGoalTodoFacts(response.snapshot) ? response.snapshot : null;
    });

    await requestNativeDebug(address, {
      command: "click",
      targetId: `native-preview.project.${projectId}`,
    });
    await waitUntil("Native Roadmap entry", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.visibleTargetIds.includes("native-preview.project.roadmap")
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.project.roadmap",
    });
    await waitUntil("Native Roadmap editor", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.visibleTargetIds.includes("native-preview.roadmap.create")
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.roadmap.create",
    });
    const nativeRoadmapTaskTarget = await waitUntil("Native Roadmap task link", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.visibleTargetIds.find(
        (id) => id.startsWith("native-preview.roadmap.milestone.") &&
          id.endsWith(`.task.${roadmapInteraction.taskId}`),
      ) ?? null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: nativeRoadmapTaskTarget,
    });
    await waitUntil("Native Roadmap authority", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      const milestone = response.snapshot.roadmap.find(
        (item) => item.projectId === projectId && item.title === roadmapInteraction.title,
      );
      return milestone?.taskIds.includes(roadmapInteraction.taskId) ? response.snapshot : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.project.memory",
    });
    await waitUntil("Native Memory editor", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      const ids = new Set(response.observation.visibleTargetIds);
      return ids.has("native-preview.memory.new") &&
        ids.has("native-preview.memory.title") &&
        ids.has("native-preview.memory.body")
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.memory.new",
    });
    if (memoryInteraction.scope === "user") {
      await requestNativeDebug(address, {
        command: "click",
        targetId: "native-preview.memory.scope",
      });
    }
    for (const [targetId, text] of [
      ["native-preview.memory.title", memoryInteraction.title],
      ["native-preview.memory.body", memoryInteraction.body],
      ["native-preview.memory.tags", memoryTagsText],
    ]) {
      await requestNativeDebug(address, { command: "input", targetId, text });
    }
    await waitUntil("Native Memory save action", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.visibleTargetIds.includes("native-preview.memory.save")
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.memory.save",
    });
    await waitUntil("Native Roadmap and Memory authority", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasRoadmapAndMemoryFacts(response.snapshot) ? response.snapshot : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.memory.toggle",
    });
    await waitUntil("Native Memory disabled", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return response.snapshot.memories.some(
        (item) => item.title === memoryInteraction.title && item.enabled === false,
      ) ? response.snapshot : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.memory.toggle",
    });
    await waitUntil("Native Memory enabled", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasRoadmapAndMemoryFacts(response.snapshot) ? response.snapshot : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.memory.delete",
    });
    await waitUntil("Native Memory deleted", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return response.snapshot.memories.every(
        (item) => item.title !== memoryInteraction.title,
      ) ? response.snapshot : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.memory.new",
    });
    if (memoryInteraction.scope === "user") {
      await requestNativeDebug(address, {
        command: "click",
        targetId: "native-preview.memory.scope",
      });
    }
    for (const [targetId, text] of [
      ["native-preview.memory.title", memoryInteraction.title],
      ["native-preview.memory.body", memoryInteraction.body],
      ["native-preview.memory.tags", memoryTagsText],
    ]) {
      await requestNativeDebug(address, { command: "input", targetId, text });
    }
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.memory.save",
    });
    await waitUntil("Native Memory recreated", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasRoadmapAndMemoryFacts(response.snapshot) ? response.snapshot : null;
    });
    await requestNativeDebug(address, {
      command: "input",
      targetId: "native-preview.memory.settings.cooldown.input",
      text: String(memorySettingsInteraction.cooldownTurns),
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.memory.settings.cooldown.save",
    });

    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.memory.settings.baseline",
    });
    await waitUntil("Native Memory baseline setting", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return response.snapshot.memorySettings?.baselineInjectionEnabled ===
          memorySettingsInteraction.baselineInjectionEnabled
        ? response.snapshot
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.memory.settings.enabled",
    });
    await waitUntil("Native Memory settings authority", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasMemorySettingsFacts(response.snapshot) ? response.snapshot : null;
    });

    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.settings.open",
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.settings.provider",
    });
    const nativeSuggestionTarget = conversationSuggestionsInteraction.enabled
      ? "native-preview.settings.provider.conversation-suggestions.enable"
      : "native-preview.settings.provider.conversation-suggestions.disable";
    await waitUntil("Native conversation suggestion setting", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.visibleTargetIds.includes(nativeSuggestionTarget)
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: nativeSuggestionTarget,
    });
    await waitUntil("Native conversation suggestion authority", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasConversationSuggestionSettingsFacts(response.snapshot)
        ? response.snapshot
        : null;
    });

    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.automations.open",
    });
    await waitUntil("Native Automation workspace", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.visibleTargetIds.includes("native-preview.automations.create")
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.automations.create",
    });
    await waitUntil("Native Automation draft editor", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      const ids = new Set(response.observation.visibleTargetIds);
      return ids.has("native-preview.automations.name") &&
          ids.has("native-preview.automations.save-draft") &&
          ids.has("native-preview.automations.scope.include-inbox") &&
          ids.has("graph.lilia-automation.node.trigger")
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "input",
      targetId: "native-preview.automations.name",
      text: automationInteraction.name,
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.automations.save-draft",
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.automations.scope.include-inbox",
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "graph.lilia-automation.node.trigger",
    });
    await waitUntil("Native Automation trigger inspector", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.visibleTargetIds.includes("native-preview.automations.node.title")
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "input",
      targetId: "native-preview.automations.node.title",
      text: automationInteraction.nodeTitle,
    });
    await waitUntil("Native Automation node save", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.visibleTargetIds.includes("native-preview.automations.node.save")
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.automations.node.save",
    });
    await waitUntil("Native Automation draft authority", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasAutomationDraftFacts(response.snapshot) ? response.snapshot : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.automations.publish",
    });
    await waitUntil("Native Automation published authority", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasAutomationDraftFacts(response.snapshot, { published: true })
        ? response.snapshot
        : null;
    });
    await waitUntil("Native Automation toggle", 20_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.visibleTargetIds.includes("native-preview.automations.toggle")
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.automations.toggle",
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.automations.back",
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.settings.open",
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.settings.extensions",
    });
    const nativeSkillToggle =
      `native-preview.settings.extensions.skill.${skillInteraction.skillId}.toggle`;
    await waitUntil("Native seeded Skill entry", 30_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.extensionsBusy === false &&
          response.observation.visibleTargetIds.includes(nativeSkillToggle)
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: nativeSkillToggle,
    });
    await waitUntil("Native Skill authority update", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasUpdatedSkillFacts(response.snapshot) ? response.snapshot : null;
    });
    const nativePluginToggle =
      `native-preview.settings.extensions.plugin.${pluginInteraction.pluginId}.toggle`;
    await waitUntil("Native seeded Plugin entry", 30_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.extensionsBusy === false &&
          response.observation.visibleTargetIds.includes(nativePluginToggle)
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: nativePluginToggle,
    });
    await waitUntil("Native Plugin authority update", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasUpdatedPluginFacts(response.snapshot) ? response.snapshot : null;
    });
    const nativeHookToggle =
      "native-preview.settings.extensions.hook.native-agentkit:user.toggle";
    await waitUntil("Native seeded Hook source", 30_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.extensionsBusy === false &&
          response.observation.visibleTargetIds.includes(nativeHookToggle)
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: nativeHookToggle,
    });
    await waitUntil("Native Hook authority update", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasUpdatedHookFacts(response.snapshot) ? response.snapshot : null;
    });
    const nativeMcpEdit = `native-preview.settings.extensions.mcp.${mcpInteraction.serverId}.edit`;
    await waitUntil("Native seeded MCP entry", 30_000, async () => {
      const response = await requestNativeDebug(address, { command: "observe" });
      return response.observation.extensionsBusy === false &&
          response.observation.visibleTargetIds.includes(nativeMcpEdit)
        ? response.observation
        : null;
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: nativeMcpEdit,
    });
    await requestNativeDebug(address, {
      command: "input",
      targetId: "native-preview.settings.extensions.mcp.editor.location",
      text: mcpInteraction.command,
    });
    await requestNativeDebug(address, {
      command: "input",
      targetId: "native-preview.settings.extensions.mcp.editor.args",
      text: JSON.stringify(mcpInteraction.args),
    });
    await requestNativeDebug(address, {
      command: "click",
      targetId: "native-preview.settings.extensions.mcp.editor.save",
    });
    await waitUntil("Native MCP authority update", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasUpdatedMcpFacts(response.snapshot) ? response.snapshot : null;
    });
    const final = await waitUntil("Native final authority", 20_000, async () => {
      const response = await requestNativeDebug(address, {
        command: "equivalence-snapshot",
        fixtureId,
      });
      return hasFinalEquivalenceFacts(response.snapshot) ? response.snapshot : null;
    });
    await writeJson("native-final.json", final);
    summary.checks.push(
      "Native real UI cleared Goal/Todo and persisted the fixed Composer, Roadmap, Memory and conversation suggestion settings, published Automation, disabled the live AgentKit Skill, Plugin and Hook source, and edited Native AgentKit MCP",
    );
    let screenshot = null;
    let screenshotError = null;
    try {
      screenshot = await captureNativeWindow(
        child.pid,
        path.join(runDir, "native-task.png"),
      );
      summary.artifacts.nativeScreenshot = screenshot.path;
      summary.nativeScreenshot = screenshot;
    } catch (error) {
      screenshotError = error instanceof Error ? error.message : String(error);
      summary.nativeScreenshot = { status: "blocked", message: screenshotError };
    }
    return { initial, final, screenshotError };
  } finally {
    stopProcessTree(child);
  }
}

async function main() {
  await mkdir(runDir, { recursive: true });
  const tauriSeed = await seedHome(tauriHome, "tauri-equivalence");
  const nativeSeed = await seedHome(nativeHome, "native-equivalence");
  if (tauriSeed.manifestSha256 !== nativeSeed.manifestSha256) {
    throw new Error("isolated homes were not seeded from the same fixture manifest");
  }
  assertEquivalent(tauriSeed.snapshot, nativeSeed.snapshot, "seed snapshots");
  if (!hasSeededGoalTodoFacts(tauriSeed.snapshot)) {
    throw new Error("seed snapshot is missing the fixed Goal/Todo facts");
  }
  if (!hasSeededMcpFacts(tauriSeed.snapshot)) {
    throw new Error("seed snapshot is missing the fixed Native AgentKit MCP facts");
  }
  if (!hasSeededSkillFacts(tauriSeed.snapshot)) {
    throw new Error("seed snapshot is missing the fixed Native AgentKit Skill facts");
  }
  if (!hasSeededPluginFacts(tauriSeed.snapshot)) {
    throw new Error("seed snapshot is missing the fixed Native AgentKit Plugin facts");
  }
  if (!hasSeededHookFacts(tauriSeed.snapshot)) {
    throw new Error("seed snapshot is missing the fixed Native AgentKit Hook facts");
  }
  if (!hasInitialConversationSuggestionSettingsFacts(tauriSeed.snapshot)) {
    throw new Error("seed snapshot is missing the default conversation suggestion settings");
  }
  seededMcpConfigurationSha256 = tauriSeed.snapshot.mcpServers.find(
    (item) => item.serverId === mcpInteraction.serverId,
  ).configurationSha256;
  summary.manifestSha256 = tauriSeed.manifestSha256;
  summary.checks.push(
    "isolated Tauri and Native homes were seeded through typed APIs from one manifest, including Goal/Todo, default conversation suggestion settings, exact-package Skill, revisioned Hook and secret-free Native AgentKit MCP facts",
  );

  const devServerPlan = await createAgentDebugDevServerPlan(process.env);
  summary.devUrl = devServerPlan.devUrl;
  summary.agentDebugTools = await ensureAgentDebugTools();
  await buildApplications(devServerPlan.devUrl);
  summary.checks.push("debug Tauri/WebDriver and Native binaries built from the current workspace");

  const tauri = await runTauri(devServerPlan);
  const native = await runNative();
  assertEquivalent(tauri.initial, native.initial, "initial UI authority snapshots");
  assertEquivalent(tauri.final, native.final, "final UI authority snapshots");
  summary.checks.push(
    "both real UIs kept an edited but unsent new conversation out of Product authority and produced identical normalized snapshots before and after the remaining input, including conversation suggestion settings, Skill runtime state, Hook revision and MCP registry revision with hashed configuration",
  );
  summary.businessEquivalence = "passed";
  if (native.screenshotError) {
    throw new VisualGateBlockedError(
      `Native GPU visual evidence was blocked: ${native.screenshotError}`,
    );
  }
  summary.status = "passed";
}

await mkdir(runDir, { recursive: true });
try {
  await main();
} catch (error) {
  summary.status = error instanceof VisualGateBlockedError ? "blocked" : "failed";
  summary.message = error instanceof Error ? error.message : String(error);
  process.exitCode = summary.status === "blocked" ? 2 : 1;
} finally {
  await persistLogs();
  summary.artifacts.summary = await writeJson("summary.json", summary);
}

if (summary.status === "failed") {
  throw new Error(`${summary.message}\nEquivalence artifacts: ${runDir}`);
}

if (summary.status === "blocked") {
  console.error(`P0 UI equivalence visual gate blocked: ${runDir}`);
}

if (summary.status === "passed") {
  console.log(`P0 UI equivalence passed: ${runDir}`);
}

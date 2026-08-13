import fs from "node:fs";
import http from "node:http";
import net from "node:net";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const runId = new Date().toISOString().replaceAll(":", "-").replaceAll(".", "-");
const runDir = path.join(repoRoot, "agent-debug-runs", `native-${runId}`);
const readyPath = path.join(runDir, "ready.txt");
const transcriptPath = path.join(runDir, "protocol.jsonl");
const summaryPath = path.join(runDir, "summary.json");
const screenshotPath = path.join(runDir, "window.png");
const markdownImageScreenshotPath = path.join(runDir, "markdown-image.png");
const providerScreenshotPath = path.join(runDir, "provider.png");
const agentScreenshotPath = path.join(runDir, "agent.png");
const automationScreenshotPath = path.join(runDir, "automation.png");
const roadmapScreenshotPath = path.join(runDir, "roadmap.png");
const memoryScreenshotPath = path.join(runDir, "memory.png");
const githubScreenshotPath = path.join(runDir, "github-repositories.png");
const codingToolsScreenshotPath = path.join(runDir, "coding-tools.png");
const iabScreenshotPath = path.join(runDir, "iab.png");
const architectureScreenshotPath = path.join(runDir, "architecture.png");
const architectureApprovalScreenshotPath = path.join(runDir, "architecture-approval.png");
const quotaScreenshotPath = path.join(runDir, "quota.png");
const extensionsScreenshotPath = path.join(runDir, "extensions.png");
const remoteScreenshotPath = path.join(runDir, "remote.png");
const mcpElicitationScreenshotPath = path.join(runDir, "mcp-elicitation.png");
const projectRemovalScreenshotPath = path.join(runDir, "project-removal-confirmation.png");
const projectOrderingScreenshotPath = path.join(runDir, "project-ordering.png");
const taskOrderingScreenshotPath = path.join(runDir, "task-ordering.png");
const taskLocationDropScreenshotPath = path.join(runDir, "task-location-drop.png");
const stdoutPath = path.join(runDir, "stdout.log");
const stderrPath = path.join(runDir, "stderr.log");
const buildLogPath = path.join(runDir, "build.log");
const previewHome = path.join(runDir, "home");
const mainWindowStatePath = path.join(previewHome, "main-window-state.json");
const conversationStatusStatePath = path.join(
  previewHome,
  "conversation-status-window.json",
);
const workspaceTopologyPath = path.join(previewHome, "workspace-topology-state.json");
const previewWorkspace = path.join(runDir, "workspace");
const cloneParent = path.join(runDir, "clone-parent");
const importSource = path.join(runDir, "legacy-import-source");
const executable = path.join(
  repoRoot,
  "target",
  "debug",
  process.platform === "win32" ? "lilia-native-preview.exe" : "lilia-native-preview",
);
const providerSecretCanary = "sk-native-provider-debug-secret-0123456789abcdef";
const mcpSecretCanary = "native-mcp-keyring-secret-0123456789abcdef";
const githubTokenCanary = "native-github-keyring-token-0123456789abcdef";
const mcpToolPrompt = "native-mcp-tool-probe";
const mcpFixturePath = path.join(repoRoot, "scripts", "fixtures", "native-mcp-server.mjs");
const mcpFixtureMarkerPath = path.join(runDir, "mcp-tool-call.json");
const pluginFixtureRoot = path.join(runDir, "native-plugin-fixture");
const pluginId = "native-debug-plugin";
const pluginSkillId = "native-debug-plugin-skill";
const debugProjectId = "native-agent-debug-project";
const debugTaskId = "native-agent-debug-task";
const debugPlanReplayTaskId = "native-agent-debug-plan-replay-task";
const debugPlanCancelTaskId = "native-agent-debug-plan-cancel-task";
const debugQuestionReplayTaskId = "native-agent-debug-question-replay-task";
const debugMcpElicitationTaskId = "native-agent-debug-mcp-elicitation-task";
const debugArchitectureApprovalTaskId = "native-agent-debug-architecture-approval-task";
const architectureApprovalPrompt = "native-architecture-approval";
const composerRestartDraft = "native-composer-draft-restart";
const workflowReviewBranch = "native-debug-review-base";
const retryFailurePrompt = "native-retry-failure";
const fifoActivePrompt = "native-fifo-active-approval";
const fifoFirstPrompt = "native-fifo-queued-first";
const fifoSecondPrompt = "native-fifo-queued-second";
const guideCancelActivePrompt = "native-guide-cancel-active-approval";
const guideCancelQueuedPrompt = "native-guide-cancel-queued";
const databaseBusyPrompt = "native-database-busy-retry";
const interruptedToolTurnId = "native-debug-interrupted-tool";
const debugSeedTaskIds = [
  debugTaskId,
  debugPlanReplayTaskId,
  debugPlanCancelTaskId,
  debugQuestionReplayTaskId,
  debugMcpElicitationTaskId,
  debugArchitectureApprovalTaskId,
];

fs.mkdirSync(previewHome, { recursive: true });
fs.mkdirSync(previewWorkspace, { recursive: true });
fs.mkdirSync(cloneParent, { recursive: true });
fs.mkdirSync(path.join(importSource, "db"), { recursive: true });
fs.writeFileSync(path.join(importSource, "db", "writer.lock"), "agent-debug-import-source\n");
fs.mkdirSync(path.join(pluginFixtureRoot, "skills", pluginSkillId), { recursive: true });
fs.writeFileSync(
  path.join(pluginFixtureRoot, "skills", pluginSkillId, "SKILL.md"),
  `---\nname: ${pluginSkillId}\ndescription: Native Plugin debug Skill\n---\nInspect the Native Plugin debug fixture.\n`,
);
fs.writeFileSync(
  path.join(pluginFixtureRoot, "hooks.json"),
  `${JSON.stringify({
    version: 1,
    revision: 1,
    enabled: true,
    handlers: [{
      id: "native-debug-plugin-prompt",
      event: "UserPromptSubmit",
      matcher: "*",
      type: "command",
      command: "printf 'plugin-hook-ran\\n' >> native-agent-debug-plugin-hook.txt",
      commandWindows: "echo plugin-hook-ran>>native-agent-debug-plugin-hook.txt",
      timeoutSeconds: 5,
      statusMessage: "Plugin prompt hook",
    }],
  }, null, 2)}\n`,
);
fs.copyFileSync(mcpFixturePath, path.join(pluginFixtureRoot, "native-mcp-server.mjs"));
fs.writeFileSync(
  path.join(pluginFixtureRoot, "native-mcp-server.cmd"),
  `@echo off\r\n"${process.execPath}" "%~dp0native-mcp-server.mjs" %*\r\n`,
);
fs.writeFileSync(
  path.join(pluginFixtureRoot, "mcp.json"),
  `${JSON.stringify({
    version: 1,
    revision: 1,
    secretFree: true,
    servers: [{
      serverId: "debug-mcp",
      source: "plugin",
      transport: "stdio",
      command: "native-mcp-server.cmd",
      envSecretNames: ["NATIVE_DEBUG_TOKEN"],
      registeredFrom: "native-debug-plugin",
      enabled: true,
    }],
  }, null, 2)}\n`,
);
fs.writeFileSync(
  path.join(pluginFixtureRoot, "lilia-plugin.json"),
  `${JSON.stringify({
    schemaVersion: 1,
    pluginId,
    name: "Native Debug Plugin",
    pluginVersion: "1.0.0",
    description: "Native Plugin debug fixture",
    contributions: {
      skills: [`skills/${pluginSkillId}`],
      hooks: ["hooks.json"],
      mcp: ["mcp.json"],
    },
  }, null, 2)}\n`,
);

let child;
let modelServer;
let mcpHttpFixture;
let githubFixture;
let hangingGitFixture;
let debugAddress;
let stdoutFd;
let stderrFd;
let clipboardGateError;
const summary = {
  success: false,
  runDir,
  executable,
  readyPath,
  transcriptPath,
  workspaceTopologyPath,
  screenshotPath,
  markdownImageScreenshotPath,
  providerScreenshotPath,
  agentScreenshotPath,
  automationScreenshotPath,
  roadmapScreenshotPath,
  memoryScreenshotPath,
  githubScreenshotPath,
  codingToolsScreenshotPath,
  iabScreenshotPath,
  architectureScreenshotPath,
  architectureApprovalScreenshotPath,
  quotaScreenshotPath,
  extensionsScreenshotPath,
  remoteScreenshotPath,
  mcpElicitationScreenshotPath,
  projectRemovalScreenshotPath,
  projectOrderingScreenshotPath,
  taskOrderingScreenshotPath,
  taskLocationDropScreenshotPath,
  checks: [],
  restarts: [],
  screenshotGateErrors: [],
  screenshotSkippedSurfaces: [],
};

try {
  const build = spawnSync("cargo", ["build", "--locked", "-p", "lilia-native-preview"], {
    cwd: repoRoot,
    encoding: "utf8",
    timeout: 600_000,
    windowsHide: true,
  });
  fs.writeFileSync(
    buildLogPath,
    `${build.stdout ?? ""}${build.stderr ?? ""}`,
    "utf8",
  );
  if (build.error) throw build.error;
  if (build.status !== 0) {
    throw new Error(`cargo build failed with exit code ${build.status}`);
  }
  summary.checks.push("debug build passed");
  initializeGitWorkspace(previewWorkspace);
  summary.checks.push("isolated Native workspace is a real Git repository");

  const modelFixture = await startModelFixture();
  modelServer = modelFixture.server;
  summary.modelEndpoint = modelFixture.endpoint;
  mcpHttpFixture = await startMcpHttpFixture(mcpSecretCanary);
  githubFixture = await startGitHubFixture(githubTokenCanary);
  summary.githubRequests = githubFixture.requests;
  summary.mcpHttpRequests = mcpHttpFixture.requests;
  summary.mcpHttpEndpoints = {
    streamableHttp: mcpHttpFixture.streamableEndpoint,
    sse: mcpHttpFixture.sseEndpoint,
  };

  const previewEnvironment = {
    ...process.env,
    LILIA_NATIVE_AGENT_DEBUG: "1",
    LILIA_NATIVE_AGENT_DEBUG_ADDR: "127.0.0.1:0",
    LILIA_NATIVE_AGENT_DEBUG_READY: readyPath,
    LILIA_NATIVE_AGENT_DEBUG_SEED: "1",
    LILIA_NATIVE_AGENT_DEBUG_CORRUPT_ARCHITECTURE: "1",
    LILIA_NATIVE_AGENT_DEBUG_MODEL_ENDPOINT: modelFixture.endpoint,
    LILIA_NATIVE_AGENT_DEBUG_WORKSPACE: previewWorkspace,
    LILIA_NATIVE_AGENT_DEBUG_CLONE_PARENT: cloneParent,
    LILIA_NATIVE_AGENT_DEBUG_IMPORT_SOURCE: importSource,
    LILIA_NATIVE_AGENT_DEBUG_IMPORT_ASSUME_EMPTY: "1",
    LILIA_DESKTOP_GITHUB_FIXTURE_URL: githubFixture.baseUrl,
    LILIA_NATIVE_PREVIEW_HOME: previewHome,
  };

  fs.rmSync(readyPath, { force: true });
  stdoutFd = fs.openSync(stdoutPath, "w");
  stderrFd = fs.openSync(stderrPath, "w");
  child = spawn(executable, [], {
    cwd: repoRoot,
    env: previewEnvironment,
    stdio: ["ignore", stdoutFd, stderrFd],
    windowsHide: true,
  });
  summary.pid = child.pid;

  await waitForReady(child, readyPath, 30_000);
  let address = fs.readFileSync(readyPath, "utf8").trim();
  debugAddress = address;
  summary.address = address;

  const initial = await request(address, { command: "observe" });
  assertSuccess(initial, "observe");
  assertEqual(initial.observation.page, "projects", "initial page");
  assertTarget(initial, "native-preview.settings.open");
  assertTarget(initial, "native-preview.conversation-status.open");
  summary.checks.push("observe returned projects and visible targets");
  const initialMcpRecovery = await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpCount === 1 &&
      observation.extensionsEnabledMcpCount === 1 &&
      observation.extensionsActiveMcpCount === 0 &&
      observation.extensionsActivationErrorCount === 1,
    30_000,
  );
  assertEqual(
    initialMcpRecovery.observation.page,
    "projects",
    "startup MCP recovery does not navigate to Extensions",
  );
  const marked = await request(address, {
    command: "mark",
    label: "native-agent-debug:started",
    data: JSON.stringify({ runDir }),
  });
  assertSuccess(marked, "record Native debug mark");
  if (
    !marked.observation.logs.some(
      (entry) => entry.kind === "mark" && entry.message === "native-agent-debug:started",
    )
  ) {
    throw new Error("Native Agent Debug mark was not retained in the bounded log");
  }
  const initialErrors = await request(address, { command: "recent-errors" });
  assertSuccess(initialErrors, "read initial Native debug errors");
  if (
    !initialErrors.observation.errors.some(
      (entry) => entry.source === "mcp:native-debug-invalid",
    ) ||
    initialErrors.observation.errors.some(
      (entry) => entry.source !== "mcp:native-debug-invalid",
    )
  ) {
    throw new Error("startup MCP recovery did not isolate the seeded per-server activation error");
  }
  summary.checks.push(
    "startup restored enabled MCP registrations off the first frame and retained a bounded per-server error without opening Extensions",
  );
  const mainWindowStateBeforeToolWindow = await waitForJsonFile(mainWindowStatePath, 10_000);

  const mainTaskCountBeforeDraft = initial.observation.taskCount;
  const mainDraftOpened = await request(address, {
    command: "click",
    targetId: "native-preview.new-conversation",
  });
  assertSuccess(mainDraftOpened, "open a transient conversation draft in the main window");
  assertTarget(mainDraftOpened, "native-preview.new-conversation.close");
  assertTarget(mainDraftOpened, "native-preview.task-session.composer.input");
  assertEqual(
    mainDraftOpened.observation.taskCount,
    mainTaskCountBeforeDraft,
    "main draft task count before input",
  );
  const mainDraftEdited = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: "关闭前不应保存的主窗口草稿",
  });
  assertSuccess(mainDraftEdited, "edit the transient main-window draft");
  assertTarget(mainDraftEdited, "native-preview.task-session.composer.send");
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.new-conversation.close",
    }),
    "close the unsent main-window draft",
  );
  const mainDraftDiscarded = await waitForObservation(
    address,
    (observation) =>
      !observation.visibleTargetIds.includes("native-preview.new-conversation.close") &&
      observation.taskCount === mainTaskCountBeforeDraft,
    10_000,
  );
  assertTarget(mainDraftDiscarded, "native-preview.new-conversation");
  summary.checks.push(
    "the main new-conversation entry kept typed content transient and closing it created no task",
  );

  const statusOpening = await request(address, {
    command: "click",
    targetId: "native-preview.conversation-status.open",
  });
  assertSuccess(statusOpening, "open Native conversation-status window");
  const statusReady = await waitForObservation(
    address,
    (observation) =>
      observation.conversationStatusWindowOpen === true &&
      observation.conversationStatusWindowReady === true &&
      observation.conversationStatusTaskCount === debugSeedTaskIds.length,
    10_000,
  );
  assertTarget(statusReady, "native-preview.conversation-status.window");
  assertTarget(statusReady, `native-preview.conversation-status.task.${debugTaskId}`);
  assertTarget(statusReady, "native-preview.conversation-status.pin");
  assertTarget(statusReady, "native-preview.conversation-status.opacity");
  assertTarget(statusReady, "native-preview.conversation-status.new-chat");
  const statusPinned = await request(address, {
    command: "click",
    targetId: "native-preview.conversation-status.pin",
  });
  assertSuccess(statusPinned, "pin the Native conversation-status window");
  const statusOpacity = await request(address, {
    command: "input",
    targetId: "native-preview.conversation-status.opacity",
    text: "0.72",
  });
  assertSuccess(statusOpacity, "change the Native conversation-status opacity");
  const persistedStatusState = await waitForJsonFile(conversationStatusStatePath, 10_000);
  assertEqual(persistedStatusState.alwaysOnTop, true, "persisted status-window pin");
  assertEqual(persistedStatusState.opacity, 0.72, "persisted status-window opacity");

  const taskCountBeforeDraft = statusReady.observation.conversationStatusTaskCount;
  const draftOpened = await request(address, {
    command: "click",
    targetId: "native-preview.conversation-status.new-chat",
  });
  assertSuccess(draftOpened, "open a transient Native conversation draft");
  const draftReady = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 1 &&
      observation.taskPopupReadyCount === 1 &&
      observation.taskPopupTaskIds?.[0] === "" &&
      observation.conversationStatusTaskCount === taskCountBeforeDraft,
    10_000,
  );
  const firstDraftComposer = draftReady.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".composer.input"),
  );
  const firstDraftClose = draftReady.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".close"),
  );
  if (!firstDraftComposer || !firstDraftClose) {
    throw new Error("transient Native conversation draft did not expose its composer and close action");
  }
  const draftClosed = await request(address, {
    command: "click",
    targetId: firstDraftClose,
  });
  assertSuccess(draftClosed, "close the unsent Native conversation draft");
  await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 0 &&
      observation.conversationStatusTaskCount === taskCountBeforeDraft,
    10_000,
  );

  const promotedDraftOpened = await request(address, {
    command: "click",
    targetId: "native-preview.conversation-status.new-chat",
  });
  assertSuccess(promotedDraftOpened, "open a second transient Native conversation draft");
  const promotedDraftReady = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 1 && observation.taskPopupTaskIds?.[0] === "",
    10_000,
  );
  const promotedComposer = promotedDraftReady.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".composer.input"),
  );
  if (!promotedComposer) throw new Error("second Native conversation draft has no composer");
  const drafted = await request(address, {
    command: "input",
    targetId: promotedComposer,
    text: "验证首次发送才保存对话",
  });
  assertSuccess(drafted, "edit the transient Native conversation draft");
  const promotedSend = drafted.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".composer.send"),
  );
  if (!promotedSend) throw new Error("send action is unavailable for the transient conversation draft");
  const promoted = await request(address, {
    command: "click",
    targetId: promotedSend,
  });
  assertSuccess(promoted, "materialize the Native conversation on first send");
  const promotedReady = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 1 &&
      Boolean(observation.taskPopupTaskIds?.[0]) &&
      observation.conversationStatusTaskCount === taskCountBeforeDraft + 1,
    10_000,
  );
  const promotedClose = promotedReady.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".close"),
  );
  if (!promotedClose) throw new Error("materialized Native conversation has no close action");
  assertSuccess(
    await request(address, { command: "click", targetId: promotedClose }),
    "close the materialized Native conversation window",
  );
  await waitForObservation(
    address,
    (observation) => observation.taskPopupWindowCount === 0,
    10_000,
  );
  summary.checks.push(
    "conversation-status pin and opacity persisted; closing an unsent Native draft created no task, while first send materialized exactly one task",
  );
  const statusTaskOpened = await request(address, {
    command: "click",
    targetId: `native-preview.conversation-status.task.${debugTaskId}`,
  });
  assertSuccess(statusTaskOpened, "open a task window from the conversation-status window");
  const statusTaskWindowReady = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 1 &&
      observation.taskPopupReadyCount === 1 &&
      observation.taskPopupTaskIds?.[0] === debugTaskId,
    10_000,
  );
  const statusTaskClose = statusTaskWindowReady.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".close"),
  );
  const statusTaskFocusMain = statusTaskWindowReady.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".focus-main"),
  );
  const statusTaskNewChat = statusTaskWindowReady.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".new-chat"),
  );
  if (!statusTaskClose || !statusTaskFocusMain || !statusTaskNewChat) {
    throw new Error("conversation-status task popup has incomplete titlebar actions");
  }
  assertSuccess(
    await request(address, { command: "click", targetId: statusTaskNewChat }),
    "replace the task popup with a transient new conversation",
  );
  const titlebarDraftReady = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 1 &&
      observation.taskPopupTaskIds?.[0] === "" &&
      observation.conversationStatusTaskCount === taskCountBeforeDraft + 1,
    10_000,
  );
  const titlebarDraftClose = titlebarDraftReady.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".close"),
  );
  if (!titlebarDraftClose) throw new Error("titlebar conversation draft has no close action");
  assertSuccess(
    await request(address, { command: "click", targetId: titlebarDraftClose }),
    "close the unsent titlebar conversation draft",
  );
  await waitForObservation(address, (observation) => observation.taskPopupWindowCount === 0, 10_000);

  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.conversation-status.task.${debugTaskId}`,
    }),
    "reopen a task window to verify return-to-main",
  );
  const reopenedTaskWindow = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 1 && observation.taskPopupTaskIds?.[0] === debugTaskId,
    10_000,
  );
  const reopenedFocusMain = reopenedTaskWindow.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".focus-main"),
  );
  if (!reopenedFocusMain) throw new Error("reopened task popup has no return-to-main action");
  assertSuccess(
    await request(address, { command: "click", targetId: reopenedFocusMain }),
    "return from the task popup to the existing main-window task view",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 0 && observation.selectedTask === debugTaskId,
    10_000,
  );
  assertTarget(statusTaskOpened, "native-preview.conversation-status.close");
  closeWindowByTitle(child.pid, "LiliaCode 会话状态");
  const afterStatusClose = await waitForObservation(
    address,
    (observation) =>
      observation.conversationStatusWindowOpen === false &&
      observation.conversationStatusWindowReady === false,
    10_000,
  );
  assertEqual(afterStatusClose.observation.selectedTask, debugTaskId, "main window survived close");
  const mainWindowStateAfterToolWindow = await waitForJsonFile(mainWindowStatePath, 10_000);
  assertEqual(
    JSON.stringify(mainWindowStateAfterToolWindow),
    JSON.stringify(mainWindowStateBeforeToolWindow),
    "auxiliary window must not overwrite main-window geometry",
  );
  summary.checks.push(
    "a real NanaUI status window opened a task independently; its titlebar created an unsaved transient conversation and returned to the existing main task view without duplicating Product state",
  );

  const mcpTaskList = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.back",
  });
  assertSuccess(mcpTaskList, "return to seeded task list for MCP elicitation");
  assertTarget(mcpTaskList, `native-preview.task.${debugMcpElicitationTaskId}`);
  const mcpTask = await request(address, {
    command: "click",
    targetId: `native-preview.task.${debugMcpElicitationTaskId}`,
  });
  assertSuccess(mcpTask, "open seeded MCP elicitation task");
  assertEqual(
    mcpTask.observation.selectedTask,
    debugMcpElicitationTaskId,
    "MCP elicitation task selection",
  );
  const mcpTargetPrefix =
    "native-preview.task-session.mcp.native-agent-debug-mcp-request";
  const mcpReady = await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "waiting_interaction" &&
      observation.taskActionError === null &&
      observation.visibleTargetIds.includes(`${mcpTargetPrefix}.field.0.toggle`) &&
      observation.visibleTargetIds.includes(`${mcpTargetPrefix}.field.1.option.1`) &&
      observation.visibleTargetIds.includes(`${mcpTargetPrefix}.field.2.option.0`) &&
      observation.visibleTargetIds.includes(`${mcpTargetPrefix}.field.3`) &&
      observation.visibleTargetIds.includes(`${mcpTargetPrefix}.field.4`) &&
      !observation.visibleTargetIds.includes(`${mcpTargetPrefix}.accept`),
    30_000,
  );
  assertTarget(mcpReady, `${mcpTargetPrefix}.decline`);
  assertTarget(mcpReady, `${mcpTargetPrefix}.cancel`);
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${mcpTargetPrefix}.field.0.toggle`,
    }),
    "toggle MCP boolean field",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${mcpTargetPrefix}.field.1.option.1`,
    }),
    "select MCP enum field",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${mcpTargetPrefix}.field.2.option.0`,
    }),
    "select first MCP multi-select option",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${mcpTargetPrefix}.field.2.option.1`,
    }),
    "select second MCP multi-select option",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: `${mcpTargetPrefix}.field.3`,
      text: "Native MCP 表单已接入真实应用响应",
    }),
    "enter required MCP text field",
  );
  const mcpTags = await request(address, {
    command: "input",
    targetId: `${mcpTargetPrefix}.field.4`,
    text: "native, mcp",
  });
  assertSuccess(mcpTags, "enter MCP free-form array field");
  assertTarget(mcpTags, `${mcpTargetPrefix}.accept`);
  await recordRenderedWindow(
    child.pid,
    mcpElicitationScreenshotPath,
    "MCP elicitation",
    "mcpElicitationWindowBounds",
    "mcpElicitationPngSize",
  );
  summary.checks.push(
    "a persisted MCP form used stable Native targets for boolean, enum, multi-select, text and free-form arrays, enabled the production accept action only after validation, and produced a real GPU screenshot",
  );
  const mainTaskList = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.back",
  });
  assertSuccess(mainTaskList, "return to seeded task list after MCP elicitation");
  assertTarget(mainTaskList, `native-preview.task.${debugTaskId}`);
  const mainTaskReady = await request(address, {
    command: "click",
    targetId: `native-preview.task.${debugTaskId}`,
  });
  assertSuccess(mainTaskReady, "restore the primary seeded task after MCP elicitation");
  const markdownImageReady = await waitForObservation(
    address,
    (observation) =>
      observation.visibleTargets?.some(
        (target) =>
          target.startsWith("native-preview.task-window.0.markdown-image.") &&
          !target.endsWith(".close"),
      ),
    10_000,
  );
  const markdownImageTarget = markdownImageReady.observation.visibleTargets.find(
    (target) =>
      target.startsWith("native-preview.task-window.0.markdown-image.") &&
      !target.endsWith(".close"),
  );
  const markdownImageOpened = await request(address, {
    command: "click",
    targetId: markdownImageTarget,
  });
  assertSuccess(markdownImageOpened, "open a real Native Markdown image");
  assertTarget(markdownImageOpened, "native-preview.task-window.0.markdown-image.close");
  await recordRenderedWindow(
    child.pid,
    markdownImageScreenshotPath,
    "Markdown image viewer",
    "markdownImageWindowBounds",
    "markdownImagePngSize",
  );
  const markdownImageClosed = await request(address, {
    command: "click",
    targetId: "native-preview.task-window.0.markdown-image.close",
  });
  assertSuccess(markdownImageClosed, "close the Native Markdown image viewer");
  summary.checks.push(
    "Native Markdown loaded an in-memory image, opened the NanaUI zoom and pan viewer, and produced a real GPU screenshot",
  );
  const mcpWorkspaceTab =
    `native-preview.workspace.tab.task:${debugMcpElicitationTaskId}`;
  assertTarget(mainTaskReady, mcpWorkspaceTab);
  const mcpWorkspaceTabActive = await request(address, {
    command: "click",
    targetId: mcpWorkspaceTab,
  });
  assertSuccess(mcpWorkspaceTabActive, "reactivate the MCP debug Workspace view for close");
  const mcpWorkspaceTabClose =
    `native-preview.workspace.tab.task:${debugMcpElicitationTaskId}.close`;
  assertTarget(mcpWorkspaceTabActive, mcpWorkspaceTabClose);
  const mainTaskIsolated = await request(address, {
    command: "click",
    targetId: mcpWorkspaceTabClose,
  });
  assertSuccess(mainTaskIsolated, "close the completed MCP debug Workspace view");
  assertEqual(
    mainTaskIsolated.observation.selectedTask,
    debugTaskId,
    "primary task after closing MCP Workspace view",
  );

  assertTarget(mainTaskIsolated, "native-preview.task-session.popup.open");
  const taskPopupOpening = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.popup.open",
  });
  assertSuccess(taskPopupOpening, "open selected task in a Native window");
  const taskPopupReady = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 1 &&
      observation.taskPopupReadyCount === 1 &&
      observation.taskPopupTaskIds?.[0] === debugTaskId &&
      observation.taskPopupSessionIds?.length === 1,
    10_000,
  );
  const taskPopupSessionId = taskPopupReady.observation.taskPopupSessionIds[0];
  if (
    !taskPopupSessionId.startsWith(`native-preview.popup.task.${debugTaskId}.`) ||
    taskPopupSessionId === taskPopupReady.observation.workspaceSessionId
  ) {
    throw new Error("task popup did not own an independent typed Workspace session");
  }
  const mainTaskViewId = mainTaskIsolated.observation.activeWorkspaceItemIds?.[0];
  const popupTaskViewId = taskPopupReady.observation.taskPopupWorkspaceItemIds?.[0];
  if (!mainTaskViewId || !popupTaskViewId || popupTaskViewId === mainTaskViewId) {
    throw new Error("task popup did not create an independent view instance");
  }
  assertEqual(
    taskPopupReady.observation.taskPopupWorkspaceResourceIds?.[0],
    `task:${debugTaskId}`,
    "task popup shared task resource identity",
  );
  const taskPopupComposerTarget = taskPopupReady.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".composer.input"),
  );
  const taskPopupPasteTarget = taskPopupReady.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".paste-text"),
  );
  const taskPopupPasteImageTarget = taskPopupReady.observation.visibleTargetIds.find(
    (target) => target.startsWith("native-preview.task-popup.") && target.endsWith(".paste-image"),
  );
  if (!taskPopupComposerTarget) {
    throw new Error("ready task popup did not expose its real composer target");
  }
  if (!taskPopupPasteTarget) {
    throw new Error("ready task popup did not expose its clipboard target");
  }
  if (!taskPopupPasteImageTarget) {
    throw new Error("ready task popup did not expose its clipboard image target");
  }
  const clipboardText = "Windows 剪贴板到任务窗口";
  try {
    setClipboardText(clipboardText);
    const taskPopupPasted = await request(address, {
      command: "click",
      targetId: taskPopupPasteTarget,
    });
    assertSuccess(taskPopupPasted, "paste real Windows clipboard text in the Native task window");
    assertEqual(
      taskPopupPasted.observation.taskPopupComposerLengths[0],
      clipboardText.length,
      "task popup clipboard text length",
    );
    const taskPopupClearedFromMain = await request(address, {
      command: "input",
      targetId: "native-preview.task-session.composer.input",
      text: "",
    });
    assertSuccess(
      taskPopupClearedFromMain,
      "clear the shared task-window draft from the main window",
    );
    await waitForObservation(
      address,
      (observation) => observation.taskPopupComposerLengths?.[0] === 0,
      10_000,
    );
    summary.checks.push(
      "the task window read real Windows clipboard text and the main window cleared the same shared Composer state",
    );
  } catch (error) {
    clipboardGateError = error instanceof Error ? error : new Error(String(error));
    summary.clipboardGateError = clipboardGateError.message;
  }
  const taskPopupDraft = "验证任务窗口共享草稿";
  const taskPopupDrafted = await request(address, {
    command: "input",
    targetId: taskPopupComposerTarget,
    text: taskPopupDraft,
  });
  assertSuccess(taskPopupDrafted, "edit composer from the Native task window");
  assertEqual(
    taskPopupDrafted.observation.taskPopupComposerLengths[0],
    taskPopupDraft.length,
    "task popup composer length",
  );
  const taskPopupReused = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.popup.open",
  });
  assertSuccess(taskPopupReused, "focus the existing Native task window");
  assertEqual(taskPopupReused.observation.taskPopupWindowCount, 1, "reused task popup count");
  assertEqual(
    taskPopupReused.observation.taskPopupSessionIds[0],
    taskPopupSessionId,
    "reused task popup session",
  );
  const taskPopupCleared = await request(address, {
    command: "input",
    targetId: taskPopupComposerTarget,
    text: "",
  });
  assertSuccess(taskPopupCleared, "clear composer from the Native task window");
  assertEqual(taskPopupCleared.observation.taskPopupComposerLengths[0], 0, "cleared task popup");
  closeWindowByTitle(
    child.pid,
    `LiliaCode · ${taskPopupReady.observation.selectedTaskTitle}`,
  );
  const afterTaskPopupClose = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 0 &&
      observation.taskPopupReadyCount === 0 &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    10_000,
  );
  assertEqual(
    afterTaskPopupClose.observation.selectedTask,
    debugTaskId,
    "main window survived task popup close",
  );
  const mainWindowStateAfterTaskPopup = await waitForJsonFile(mainWindowStatePath, 10_000);
  assertEqual(
    JSON.stringify(mainWindowStateAfterTaskPopup),
    JSON.stringify(mainWindowStateBeforeToolWindow),
    "task popup must not overwrite main-window geometry",
  );
  summary.checks.push(
    "a real NanaUI task window owned an independent Workspace session and view instance over the shared task resource, edited shared composer state, reused its existing window, and closed without exiting or overwriting main-window state",
  );

  assertTarget(afterTaskPopupClose, "native-preview.task-session.popup.move-selected");
  const itemMovedOut = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.popup.move-selected",
  });
  assertSuccess(itemMovedOut, "move the active Workspace item to a Native task window");
  const itemOwnedByPopup = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 1 &&
      observation.taskPopupReadyCount === 1 &&
      observation.taskPopupWorkspaceItemIds?.[0] === mainTaskViewId &&
      observation.taskPopupWorkspaceResourceIds?.[0] === `task:${debugTaskId}` &&
      observation.taskPopupGeometries?.[0] != null &&
      !observation.workspaceItemIds?.includes(mainTaskViewId) &&
      observation.selectedTask === null &&
      observation.workspacePersistedRevision === observation.workspaceRevision &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    10_000,
  );
  const transferredPopupSessionId = itemOwnedByPopup.observation.taskPopupSessionIds[0];
  const transferredPopupGeometry = itemOwnedByPopup.observation.taskPopupGeometries[0];
  assertEqual(
    JSON.stringify(itemOwnedByPopup.observation.workspaceWindowItemIds?.[0]),
    JSON.stringify([mainTaskViewId]),
    "single-item workspace window ownership",
  );
  const secondTaskOpened = await request(address, {
    command: "click",
    targetId: `native-preview.task.${debugPlanCancelTaskId}`,
  });
  assertSuccess(secondTaskOpened, "open a second task view in the main Workspace");
  const secondMainViewId = secondTaskOpened.observation.activeWorkspaceItemIds?.find(
    (itemId) => itemId !== mainTaskViewId,
  );
  if (!secondMainViewId) {
    throw new Error("second task did not create a main Workspace item");
  }
  const moveSecondToWindowTarget = secondTaskOpened.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith(`native-preview.workspace.tab.${secondMainViewId}.drag-to-window.`),
  );
  if (!moveSecondToWindowTarget) {
    throw new Error("workspace window did not expose its external Tab drop target");
  }
  const secondItemMovedOut = await request(address, {
    command: "click",
    targetId: moveSecondToWindowTarget,
  });
  assertSuccess(secondItemMovedOut, "drag a second main Workspace tab into the existing window");
  const multiItemWindow = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 1 &&
      observation.workspaceWindowItemIds?.[0]?.length === 2 &&
      observation.workspaceWindowItemIds[0].includes(mainTaskViewId) &&
      observation.workspaceWindowItemIds[0].includes(secondMainViewId) &&
      observation.workspaceWindowActiveItemIds?.[0] === secondMainViewId &&
      !observation.workspaceItemIds?.includes(mainTaskViewId) &&
      !observation.workspaceItemIds?.includes(secondMainViewId) &&
      observation.workspacePersistedRevision === observation.workspaceRevision &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    10_000,
  );
  const workspaceWindow = multiItemWindow.observation.workspaceWindows?.[0];
  if (!workspaceWindow || workspaceWindow.panes?.length !== 1) {
    throw new Error("multi-item workspace window did not expose its initial Pane tree");
  }
  const workspaceWindowId = workspaceWindow.windowId;
  const windowPrimaryPaneId = workspaceWindow.activePaneId;
  const splitWorkspaceWindowTarget =
    `native-preview.workspace-window.${workspaceWindowId}.pane.${windowPrimaryPaneId}.split-horizontal`;
  assertTarget(multiItemWindow, splitWorkspaceWindowTarget);
  const workspaceWindowSplit = await request(address, {
    command: "click",
    targetId: splitWorkspaceWindowTarget,
  });
  assertSuccess(workspaceWindowSplit, "split the auxiliary Workspace window Pane");
  const splitWindow = workspaceWindowSplit.observation.workspaceWindows?.[0];
  const windowSecondaryPane = splitWindow?.panes?.find(
    (pane) => pane.id !== windowPrimaryPaneId,
  );
  if (!splitWindow || !windowSecondaryPane || splitWindow.splits?.length !== 1) {
    throw new Error("auxiliary Workspace window split did not create a second Pane");
  }
  const windowSecondaryPaneId = windowSecondaryPane.id;
  const moveSecondWithinWindowTarget =
    `native-preview.workspace-window.${workspaceWindowId}.tab.${secondMainViewId}.drag-to-pane.${windowSecondaryPaneId}`;
  assertTarget(workspaceWindowSplit, moveSecondWithinWindowTarget);
  const secondItemMovedWithinWindow = await request(address, {
    command: "click",
    targetId: moveSecondWithinWindowTarget,
  });
  assertSuccess(
    secondItemMovedWithinWindow,
    "drag a Workspace-window Tab into its second Pane",
  );
  const growWorkspaceWindowSplitTarget =
    `native-preview.workspace-window.${workspaceWindowId}.split.${windowPrimaryPaneId}.${windowSecondaryPaneId}.grow`;
  assertTarget(secondItemMovedWithinWindow, growWorkspaceWindowSplitTarget);
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: growWorkspaceWindowSplitTarget,
    }),
    "resize the auxiliary Workspace-window split",
  );
  const multiPaneWindow = await waitForObservation(
    address,
    (observation) => {
      const window = observation.workspaceWindows?.[0];
      const primary = window?.panes?.find((pane) => pane.id === windowPrimaryPaneId);
      const secondary = window?.panes?.find((pane) => pane.id === windowSecondaryPaneId);
      const split = window?.splits?.find(
        (candidate) =>
          candidate.firstPaneId === windowPrimaryPaneId &&
          candidate.secondPaneId === windowSecondaryPaneId,
      );
      return (
        window?.windowId === workspaceWindowId &&
        window.activePaneId === windowSecondaryPaneId &&
        window.activeItemId === secondMainViewId &&
        primary?.itemIds?.length === 1 &&
        primary.itemIds[0] === mainTaskViewId &&
        secondary?.itemIds?.length === 1 &&
        secondary.itemIds[0] === secondMainViewId &&
        Math.abs(split?.ratio - 0.6) <= 0.001 &&
        observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision
      );
    },
    10_000,
  );
  const transferredTopology = await waitForJsonFile(workspaceTopologyPath, 10_000);
  assertEqual(transferredTopology.schemaVersion, 3, "workspace topology schema");
  assertEqual(
    transferredTopology.revision,
    multiPaneWindow.observation.workspaceTopologyRevision,
    "workspace topology committed revision",
  );
  if (
    transferredTopology.primaryWorkspace?.workspaceItems?.some(
      (item) => item.id === mainTaskViewId || item.id === secondMainViewId,
    )
  ) {
    throw new Error("committed workspace topology duplicated transferred items in the main window");
  }
  if (
    transferredTopology.windows?.[0]?.taskId !== undefined ||
    transferredTopology.windows?.[0]?.workspaceItemId !== undefined
  ) {
    throw new Error("schema v3 workspace window retained a single-task descriptor");
  }
  if (
    ![mainTaskViewId, secondMainViewId].every((itemId) =>
      transferredTopology.windows?.[0]?.workspace?.workspaceItems?.some(
        (item) => item.id === itemId,
      ),
    )
  ) {
    throw new Error("committed workspace topology omitted a transferred window item");
  }
  const persistedWindowLayout = transferredTopology.windows?.[0]?.workspace?.panelLayout;
  if (
    persistedWindowLayout?.activePane !== windowSecondaryPaneId ||
    persistedWindowLayout?.panes?.kind !== "split" ||
    persistedWindowLayout.panes.axis !== "horizontal" ||
    Math.abs(persistedWindowLayout.panes.ratio - 0.6) > 0.001 ||
    persistedWindowLayout.panes.first?.id !== windowPrimaryPaneId ||
    persistedWindowLayout.panes.second?.id !== windowSecondaryPaneId
  ) {
    throw new Error("committed workspace topology omitted the auxiliary Pane tree or ratio");
  }
  assertEqual(
    JSON.stringify(transferredTopology.windows?.[0]?.geometry),
    JSON.stringify(transferredPopupGeometry),
    "workspace topology task-window geometry",
  );
  const workspaceWindowProcessId = child.pid;
  ({ child, address } = await restartPreviewProcess(child, previewEnvironment));
  summary.restarts.push({
    kind: "workspace-window",
    firstProcessId: workspaceWindowProcessId,
    restoredProcessId: child.pid,
  });
  debugAddress = address;
  const restoredWindowOwnership = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 1 &&
      observation.taskPopupReadyCount === 1 &&
      observation.taskPopupSessionIds?.[0] === transferredPopupSessionId &&
      observation.workspaceWindowItemIds?.[0]?.length === 2 &&
      observation.workspaceWindowItemIds[0].includes(mainTaskViewId) &&
      observation.workspaceWindowItemIds[0].includes(secondMainViewId) &&
      observation.workspaceWindowActiveItemIds?.[0] === secondMainViewId &&
      observation.workspaceWindows?.[0]?.windowId === workspaceWindowId &&
      observation.workspaceWindows[0].activePaneId === windowSecondaryPaneId &&
      observation.workspaceWindows[0].activeItemId === secondMainViewId &&
      observation.workspaceWindows[0].panes?.some(
        (pane) => pane.id === windowPrimaryPaneId && pane.itemIds?.[0] === mainTaskViewId,
      ) &&
      observation.workspaceWindows[0].panes?.some(
        (pane) => pane.id === windowSecondaryPaneId && pane.itemIds?.[0] === secondMainViewId,
      ) &&
      observation.workspaceWindows[0].splits?.some(
        (split) =>
          split.firstPaneId === windowPrimaryPaneId &&
          split.secondPaneId === windowSecondaryPaneId &&
          Math.abs(split.ratio - 0.6) <= 0.001,
      ) &&
      JSON.stringify(observation.taskPopupGeometries?.[0]) ===
        JSON.stringify(transferredPopupGeometry) &&
      !observation.workspaceItemIds?.includes(mainTaskViewId) &&
      !observation.workspaceItemIds?.includes(secondMainViewId) &&
      observation.workspacePersistedRevision === observation.workspaceRevision &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    15_000,
  );
  const activateRestoredMainItem = await request(address, {
    command: "click",
    targetId: `native-preview.workspace-window.${workspaceWindowId}.tab.${mainTaskViewId}`,
  });
  assertSuccess(activateRestoredMainItem, "focus the restored first Workspace-window Pane");
  const restoredMainItemFocused = await waitForObservation(
    address,
    (observation) =>
      observation.workspaceWindows?.[0]?.activePaneId === windowPrimaryPaneId &&
      observation.workspaceWindows[0].activeItemId === mainTaskViewId,
    10_000,
  );
  const moveSecondToMainTarget = restoredMainItemFocused.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.task-popup.") &&
      target.includes(`.tab.${secondMainViewId}.drag-to-main-pane.`),
  );
  if (!moveSecondToMainTarget) {
    throw new Error("restored multi-item window did not expose every Tab as a drag source");
  }
  const secondItemMovedBack = await request(address, {
    command: "click",
    targetId: moveSecondToMainTarget,
  });
  assertSuccess(secondItemMovedBack, "drag one item out while keeping a non-empty Workspace window");
  const oneItemWindow = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 1 &&
      observation.workspaceWindowItemIds?.[0]?.length === 1 &&
      observation.workspaceWindowItemIds[0][0] === mainTaskViewId &&
      observation.workspaceWindowActiveItemIds?.[0] === mainTaskViewId &&
      observation.workspaceWindows?.[0]?.panes?.length === 2 &&
      observation.workspaceWindows[0].activePaneId === windowPrimaryPaneId &&
      observation.workspaceWindows[0].activeItemId === mainTaskViewId &&
      observation.workspaceWindows[0].panes.some(
        (pane) => pane.id === windowSecondaryPaneId && pane.itemIds.length === 0,
      ) &&
      observation.workspaceWindows[0].splits.some(
        (split) => Math.abs(split.ratio - 0.6) <= 0.001,
      ) &&
      observation.workspaceItemIds?.includes(secondMainViewId) &&
      !observation.workspaceItemIds?.includes(mainTaskViewId) &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    10_000,
  );
  const moveToMainTarget = oneItemWindow.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.task-popup.") &&
      target.includes(`.tab.${mainTaskViewId}.drag-to-main-pane.`),
  );
  if (!moveToMainTarget) {
    throw new Error("transferred task popup did not expose its cross-window tab drag target");
  }
  const itemMovedBack = await request(address, {
    command: "click",
    targetId: moveToMainTarget,
  });
  assertSuccess(itemMovedBack, "drag the exact Workspace tab back to the main window surface");
  const afterCrossWindowTransfer = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === 0 &&
      observation.workspaceItemIds?.includes(mainTaskViewId) &&
      observation.workspaceItemIds?.includes(secondMainViewId) &&
      observation.activeWorkspaceItemIds?.includes(mainTaskViewId) &&
      observation.selectedTask === debugTaskId &&
      observation.workspacePersistedRevision === observation.workspaceRevision &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    10_000,
  );
  summary.checks.push(
    "schema v3 persisted a generic multi-item auxiliary Workspace with a recursive Pane tree, active Pane, resized split and geometry; restart restored the exact topology, non-empty source ownership survived the first transfer, and the window closed only after the final atomic transfer",
  );

  assertEqual(
    afterCrossWindowTransfer.observation.workspacePaneCount,
    1,
    "initial workspace pane count",
  );
  const primaryPaneId = afterCrossWindowTransfer.observation.activeWorkspacePaneId;
  if (!primaryPaneId) throw new Error("Native Workspace did not expose an active pane id");
  assertTarget(
    afterTaskPopupClose,
    `native-preview.workspace.pane.${primaryPaneId}.split-horizontal`,
  );
  const paneSplit = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.pane.${primaryPaneId}.split-horizontal`,
  });
  assertSuccess(paneSplit, "split the active Native workspace pane");
  assertEqual(paneSplit.observation.workspacePaneCount, 2, "split workspace pane count");
  const secondaryPane = paneSplit.observation.workspacePanes.find(
    (pane) => pane.id !== primaryPaneId,
  );
  if (!secondaryPane) throw new Error("Native Workspace split did not create a second pane");
  const splitDescriptor = paneSplit.observation.workspaceSplits.find(
    (split) =>
      split.firstPaneId === primaryPaneId && split.secondPaneId === secondaryPane.id,
  );
  if (!splitDescriptor || Math.abs(splitDescriptor.ratio - 0.5) > 0.001) {
    throw new Error("Native Workspace did not expose the persisted initial split ratio");
  }
  const growSplitTarget = `native-preview.workspace.split.${primaryPaneId}.${secondaryPane.id}.grow`;
  assertTarget(paneSplit, growSplitTarget);
  const paneResized = await request(address, {
    command: "click",
    targetId: growSplitTarget,
  });
  assertSuccess(paneResized, "resize the Native workspace split through the real layout command");
  const resizedSplit = paneResized.observation.workspaceSplits.find(
    (split) =>
      split.firstPaneId === primaryPaneId && split.secondPaneId === secondaryPane.id,
  );
  if (!resizedSplit || Math.abs(resizedSplit.ratio - 0.6) > 0.001) {
    throw new Error("Native Workspace split ratio was not updated to 0.6");
  }
  await waitForObservation(
    address,
    (observation) =>
      observation.workspacePersistedRevision === observation.workspaceRevision &&
      observation.workspaceSplits.some(
        (split) =>
          split.firstPaneId === primaryPaneId &&
          split.secondPaneId === secondaryPane.id &&
          Math.abs(split.ratio - 0.6) <= 0.001,
      ),
    10_000,
  );
  const crossPaneTabDragTarget =
    `native-preview.workspace.tab.${mainTaskViewId}.drag-to-pane.${secondaryPane.id}`;
  assertTarget(paneResized, crossPaneTabDragTarget);
  const paneItemMoved = await request(address, {
    command: "click",
    targetId: crossPaneTabDragTarget,
  });
  assertSuccess(paneItemMoved, "drag the active NanaUI tab to the second Native workspace pane");
  assertEqual(
    paneItemMoved.observation.activeWorkspacePaneId,
    secondaryPane.id,
    "pane focus after moving the task tab",
  );
  const movedPane = paneItemMoved.observation.workspacePanes.find(
    (pane) => pane.id === secondaryPane.id,
  );
  if (!movedPane?.itemIds.includes(`task:${debugTaskId}`)) {
    throw new Error("moved Native task tab did not belong to the target pane");
  }
  const sourcePaneAfterDrag = paneItemMoved.observation.workspacePanes.find(
    (pane) => pane.id === primaryPaneId,
  );
  if (sourcePaneAfterDrag?.itemIds.includes(mainTaskViewId)) {
    throw new Error("cross-pane NanaUI tab drag left duplicate ownership in the source pane");
  }
  const paneItemMovedPersisted = await waitForObservation(
    address,
    (observation) =>
      observation.workspacePersistedRevision === observation.workspaceRevision &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision &&
      observation.workspacePanes.some(
        (pane) => pane.id === secondaryPane.id && pane.itemIds.includes(mainTaskViewId),
      ) &&
      observation.workspacePanes.some(
        (pane) => pane.id === primaryPaneId && !pane.itemIds.includes(mainTaskViewId),
      ),
    10_000,
  );
  const secondCrossPaneTabDragTarget =
    `native-preview.workspace.tab.${secondMainViewId}.drag-to-pane.${secondaryPane.id}`;
  assertTarget(paneItemMovedPersisted, secondCrossPaneTabDragTarget);
  const secondPaneItemMoved = await request(address, {
    command: "click",
    targetId: secondCrossPaneTabDragTarget,
  });
  assertSuccess(secondPaneItemMoved, "drag the second NanaUI tab to the target pane");
  const emptySourcePane = await waitForObservation(
    address,
    (observation) =>
      observation.workspacePanes.some(
        (pane) => pane.id === primaryPaneId && pane.itemIds.length === 0,
      ) &&
      observation.workspacePanes.some(
        (pane) =>
          pane.id === secondaryPane.id &&
          pane.itemIds.includes(mainTaskViewId) &&
          pane.itemIds.includes(secondMainViewId),
      ) &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    10_000,
  );
  const mainTaskReactivated = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.tab.${mainTaskViewId}`,
  });
  assertSuccess(mainTaskReactivated, "reactivate the original task Tab in the target pane");
  assertEqual(mainTaskReactivated.observation.selectedTask, debugTaskId, "reactivated task");
  assertTarget(
    emptySourcePane,
    `native-preview.workspace.pane.${primaryPaneId}.focus`,
  );
  const emptyPaneFocused = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.pane.${primaryPaneId}.focus`,
  });
  assertSuccess(emptyPaneFocused, "focus the empty Native workspace pane");
  assertEqual(
    emptyPaneFocused.observation.activeWorkspacePaneId,
    primaryPaneId,
    "focused empty pane id",
  );
  assertEqual(emptyPaneFocused.observation.selectedTask, null, "empty pane task selection");
  assertTarget(emptyPaneFocused, `native-preview.workspace.pane.${primaryPaneId}.close`);
  const emptyPaneClosed = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.pane.${primaryPaneId}.close`,
  });
  assertSuccess(emptyPaneClosed, "close the empty Native workspace pane");
  assertEqual(emptyPaneClosed.observation.workspacePaneCount, 1, "collapsed workspace pane count");
  assertEqual(
    emptyPaneClosed.observation.selectedTask,
    debugTaskId,
    "task selection after closing the empty pane",
  );
  summary.checks.push(
    "Native Workspace recursively rendered real panes while stable targets split, persisted a resized ratio, routed a NanaUI tab drag across strips, focused, and collapsed an empty pane",
  );

  const projectOverview = await request(address, {
    command: "click",
    targetId: "native-preview.workspace.tab.overview",
  });
  assertSuccess(projectOverview, "open the selected Native project overview");
  assertTarget(projectOverview, "native-preview.project.workspace.pick");
  const seededWorkspacePicked = await request(address, {
    command: "click",
    targetId: "native-preview.project.workspace.pick",
  });
  assertSuccess(seededWorkspacePicked, "pick the seeded Native project workspace directory");
  assertTarget(seededWorkspacePicked, "native-preview.project.workspace.clear");
  const seededWorkspaceCleared = await request(address, {
    command: "click",
    targetId: "native-preview.project.workspace.clear",
  });
  assertSuccess(seededWorkspaceCleared, "clear the seeded Native project workspace directory");
  assertNotIncludes(
    seededWorkspaceCleared.observation.visibleTargetIds,
    "native-preview.project.workspace.clear",
    "seeded workspace clear target after clearing directory",
  );
  const seededWorkspaceRestored = await request(address, {
    command: "click",
    targetId: "native-preview.project.workspace.pick",
  });
  assertSuccess(seededWorkspaceRestored, "restore the seeded Native project workspace directory");
  assertTarget(seededWorkspaceRestored, "native-preview.project.workspace.clear");
  summary.checks.push(
    "the Native project editor selected and cleared a real directory through the DesktopHost file-dialog contract",
  );
  const seededProjectId = seededWorkspaceRestored.observation.selectedProject;
  if (!seededProjectId) throw new Error("seeded Native project was not selected before clone");
  const projectCountBeforeClone = seededWorkspaceRestored.observation.projectCount;
  hangingGitFixture = await startHangingGitFixture();
  summary.hangingGitCloneUrl = hangingGitFixture.repositoryUrl;
  const cloneOpened = await request(address, {
    command: "click",
    targetId: "native-preview.projects.clone",
  });
  assertSuccess(cloneOpened, "open Native project clone surface");
  assertEqual(cloneOpened.observation.page, "projects/clone", "project clone page");
  assertTarget(cloneOpened, "native-preview.project-clone.github.bind");
  const githubBindingStarted = await request(address, {
    command: "click",
    targetId: "native-preview.project-clone.github.bind",
  });
  assertSuccess(githubBindingStarted, "start Native GitHub device authorization");
  const githubDeviceFlow = await waitForObservation(
    address,
    (observation) =>
      observation.githubBindingBusy === true &&
      observation.githubDeviceFlowActive === true,
    10_000,
  );
  assertTarget(githubDeviceFlow, "native-preview.project-clone.github.verification.open");
  assertTarget(githubDeviceFlow, "native-preview.project-clone.github.user-code.copy");
  assertTarget(githubDeviceFlow, "native-preview.project-clone.github.bind.cancel");
  const githubBindingCancelled = await request(address, {
    command: "click",
    targetId: "native-preview.project-clone.github.bind.cancel",
  });
  assertSuccess(githubBindingCancelled, "cancel Native GitHub device authorization");
  assertEqual(
    githubBindingCancelled.observation.githubBindingState,
    "unbound",
    "GitHub state after cancelling device authorization",
  );
  assertEqual(
    githubBindingCancelled.observation.githubDeviceFlowActive,
    false,
    "GitHub device flow after cancellation",
  );
  const githubBindingRestarted = await request(address, {
    command: "click",
    targetId: "native-preview.project-clone.github.bind",
  });
  assertSuccess(githubBindingRestarted, "restart Native GitHub device authorization");
  const restartedGitHubDeviceFlow = await waitForObservation(
    address,
    (observation) =>
      observation.githubBindingBusy === true &&
      observation.githubDeviceFlowActive === true,
    10_000,
  );
  assertTarget(restartedGitHubDeviceFlow, "native-preview.project-clone.github.user-code.copy");
  const githubCodeCopied = await request(address, {
    command: "click",
    targetId: "native-preview.project-clone.github.user-code.copy",
  });
  assertSuccess(githubCodeCopied, "copy the Native GitHub device authorization code");
  const githubBound = await waitForObservation(
    address,
    (observation) =>
      observation.githubBindingState === "bound" &&
      observation.githubBindingLogin === "native-debug" &&
      observation.githubBindingBusy === false &&
      observation.githubRepositoryBusy === false &&
      observation.githubRepositoryCount === 2,
    20_000,
  );
  assertTarget(githubBound, "native-preview.project-clone.github.repos.load-more");
  assertTarget(
    githubBound,
    "native-preview.project-clone.github.repo.native-debug/private-repo",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.project-clone.github.repos.load-more",
    }),
    "load the next Native GitHub repository page",
  );
  const githubAllRepos = await waitForObservation(
    address,
    (observation) =>
      observation.githubRepositoryBusy === false &&
      observation.githubRepositoryCount === 3,
    10_000,
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.project-clone.github.repo.native-debug/private-repo",
    }),
    "select a private Native GitHub repository",
  );
  const githubSelected = await request(address, { command: "observe" });
  assertSuccess(githubSelected, "observe selected Native GitHub repository");
  assertEqual(
    githubSelected.observation.selectedGitHubRepository,
    "native-debug/private-repo",
    "selected GitHub repository",
  );
  await recordRenderedWindow(
    child.pid,
    githubScreenshotPath,
    "github-repositories",
    "githubWindowBounds",
    "githubPngSize",
  );
  if (
    !githubFixture.requests.some((entry) => entry.kind === "device-code") ||
    !githubFixture.requests.some((entry) => entry.kind === "access-token") ||
    !githubFixture.requests.some((entry) => entry.kind === "user" && entry.authorized) ||
    githubFixture.requests.filter((entry) => entry.kind === "repositories" && entry.authorized)
      .length !== 2
  ) {
    throw new Error("Native GitHub binding did not complete the expected authenticated API flow");
  }
  const githubUnbound = await request(address, {
    command: "click",
    targetId: "native-preview.project-clone.github.unbind",
  });
  assertSuccess(githubUnbound, "unbind the Native GitHub account");
  assertEqual(githubUnbound.observation.githubBindingState, "unbound", "GitHub state after unbind");
  assertEqual(githubUnbound.observation.githubRepositoryCount, 0, "repositories after unbind");
  assertEqual(
    githubUnbound.observation.selectedGitHubRepository,
    null,
    "selected repository after unbind",
  );
  summary.checks.push(
    "Native GitHub device authorization cancelled and restarted through stable targets, stored its token only through the instance Keyring, loaded two authenticated repository pages, selected a private repository, and removed the binding",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.project-clone.repository",
      text: hangingGitFixture.repositoryUrl,
    }),
    "input hanging Native clone repository",
  );
  const cloneParentPicked = await request(address, {
    command: "click",
    targetId: "native-preview.project-clone.pick-parent",
  });
  assertSuccess(cloneParentPicked, "pick Native clone parent");
  assertTarget(cloneParentPicked, "native-preview.project-clone.start");
  const cloneStarted = await request(address, {
    command: "click",
    targetId: "native-preview.project-clone.start",
  });
  assertSuccess(cloneStarted, "start cancellable Native Git clone");
  const cloneBusy = await waitForObservation(
    address,
    (observation) =>
      observation.projectCloneBusy === true &&
      observation.projectCloneOutcome === "running" &&
      hangingGitFixture.requests.length > 0,
    10_000,
  );
  assertTarget(cloneBusy, "native-preview.project-clone.cancel");
  if (
    typeof cloneBusy.observation.projectClonePhase !== "string" ||
    cloneBusy.observation.projectClonePhase.length === 0
  ) {
    throw new Error("busy Native Git clone did not expose its current phase");
  }
  if (
    !Number.isInteger(cloneBusy.observation.projectClonePercent) ||
    cloneBusy.observation.projectClonePercent < 0 ||
    cloneBusy.observation.projectClonePercent > 100
  ) {
    throw new Error("busy Native Git clone did not expose a valid progress percent");
  }
  const cancelledCloneTarget = cloneBusy.observation.projectCloneTarget;
  if (typeof cancelledCloneTarget !== "string" || cancelledCloneTarget.length === 0) {
    throw new Error("busy Native Git clone did not expose its reserved target directory");
  }
  const cloneCancelled = await request(address, {
    command: "click",
    targetId: "native-preview.project-clone.cancel",
  });
  assertSuccess(cloneCancelled, "cancel hanging Native Git clone");
  const cloneCancelledObservation = await waitForObservation(
    address,
    (observation) =>
      observation.projectCloneBusy === false &&
      observation.projectCloneOutcome === "cancelled",
    10_000,
  );
  assertEqual(
    cloneCancelledObservation.observation.projectCount,
    projectCountBeforeClone,
    "project count after cancelled clone",
  );
  await waitForPathAbsent(cancelledCloneTarget, 10_000);
  summary.hangingGitCloneRequests = hangingGitFixture.requests;
  summary.projectCloneCancellation = {
    target: cancelledCloneTarget,
    runningPhase: cloneBusy.observation.projectClonePhase,
    runningPercent: cloneBusy.observation.projectClonePercent,
    outcome: cloneCancelledObservation.observation.projectCloneOutcome,
    projectCount: cloneCancelledObservation.observation.projectCount,
  };
  await stopHangingGitFixture(hangingGitFixture);
  hangingGitFixture = undefined;
  summary.checks.push(
    "a real hanging Git HTTP clone exposed progress, cancelled through its stable target, removed its reservation, and did not create a project",
  );

  assertTarget(cloneCancelledObservation, "native-preview.project-clone.repository");
  assertTarget(cloneCancelledObservation, "native-preview.project-clone.start");
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.project-clone.repository",
      text: previewWorkspace,
    }),
    "replace cancelled clone repository with the local Git fixture",
  );
  const cloneRetried = await request(address, {
    command: "click",
    targetId: "native-preview.project-clone.start",
  });
  assertSuccess(cloneRetried, "retry Native Git clone immediately after cancellation");
  const clonedWorkspace = path.join(cloneParent, path.basename(previewWorkspace));
  const cloneCompleted = await waitForObservation(
    address,
    (observation) =>
      observation.projectCloneBusy === false &&
      observation.projectCloneOutcome === "completed" &&
      observation.projectClonePercent === 100 &&
      observation.projectCount === projectCountBeforeClone + 1 &&
      observation.selectedProjectWorkspace === clonedWorkspace,
    30_000,
  );
  if (!fs.existsSync(path.join(clonedWorkspace, ".git"))) {
    throw new Error("Native Git clone did not create a real repository worktree");
  }
  const clonedProjectId = cloneCompleted.observation.selectedProject;
  if (!clonedProjectId || clonedProjectId === seededProjectId) {
    throw new Error("Native Git clone did not select a distinct Product Core project");
  }
  assertEqual(cloneCompleted.observation.page, "projects", "project page after clone");
  summary.checks.push(
    "project clone immediately retried after cancellation, used the shared background Git service, and created a Product Core project for the real worktree",
  );
  const seededProjectRestored = await request(address, {
    command: "click",
    targetId: `native-preview.project.${seededProjectId}`,
  });
  assertSuccess(seededProjectRestored, "return to the seeded Native project after clone");
  assertEqual(
    seededProjectRestored.observation.selectedProject,
    seededProjectId,
    "selected seeded project after clone",
  );
  const projectOrderBeforeDrag = seededProjectRestored.observation.projectOrder;
  if (!Array.isArray(projectOrderBeforeDrag)) {
    throw new Error("Native project order observation is missing");
  }
  const cloneBeforeSeedTarget = `native-preview.project-reorder.${clonedProjectId}.before.${seededProjectId}`;
  assertTarget(seededProjectRestored, cloneBeforeSeedTarget);
  const clonedProjectDraggedBeforeSeed = await request(address, {
    command: "click",
    targetId: cloneBeforeSeedTarget,
  });
  assertSuccess(clonedProjectDraggedBeforeSeed, "drag cloned Native project before seeded project");
  if (
    clonedProjectDraggedBeforeSeed.observation.projectOrder.indexOf(clonedProjectId) >=
    clonedProjectDraggedBeforeSeed.observation.projectOrder.indexOf(seededProjectId)
  ) {
    throw new Error("Native project drag did not persist the before-project order");
  }
  await recordRenderedWindow(
    child.pid,
    projectOrderingScreenshotPath,
    "project drag ordering",
    "projectOrderingWindowBounds",
    "projectOrderingPngSize",
  );
  const cloneToEndTarget = `native-preview.project-reorder.${clonedProjectId}.before.end`;
  assertTarget(clonedProjectDraggedBeforeSeed, cloneToEndTarget);
  const clonedProjectDraggedBack = await request(address, {
    command: "click",
    targetId: cloneToEndTarget,
  });
  assertSuccess(clonedProjectDraggedBack, "restore cloned Native project order by drag");
  assertEqual(
    JSON.stringify(clonedProjectDraggedBack.observation.projectOrder),
    JSON.stringify(projectOrderBeforeDrag),
    "project order after drag restoration",
  );
  summary.checks.push(
    "project drag ordering used NanaUI before-value targets and persisted through the shared Product Core sort order",
  );
  const taskOrderBeforeDrag = clonedProjectDraggedBack.observation.taskOrder;
  if (!Array.isArray(taskOrderBeforeDrag) || taskOrderBeforeDrag.length < 2) {
    throw new Error("Native task order observation is missing or cannot be reordered");
  }
  const taskIndexBeforeDrag = taskOrderBeforeDrag.indexOf(debugTaskId);
  if (taskIndexBeforeDrag < 0) {
    throw new Error("seeded Native task is missing from the task order observation");
  }
  const dragBeforeTaskId =
    taskIndexBeforeDrag === taskOrderBeforeDrag.length - 1 ? taskOrderBeforeDrag[0] : null;
  const taskDragTarget = `native-preview.task-reorder.${debugTaskId}.before.${dragBeforeTaskId ?? "end"}`;
  assertTarget(clonedProjectDraggedBack, taskDragTarget);
  const seededTaskDragged = await request(address, {
    command: "click",
    targetId: taskDragTarget,
  });
  assertSuccess(seededTaskDragged, "drag seeded Native task to a relative target");
  if (JSON.stringify(seededTaskDragged.observation.taskOrder) === JSON.stringify(taskOrderBeforeDrag)) {
    throw new Error("Native task drag did not persist a changed task order");
  }
  await recordRenderedWindow(
    child.pid,
    taskOrderingScreenshotPath,
    "task drag ordering",
    "taskOrderingWindowBounds",
    "taskOrderingPngSize",
  );
  const restoreBeforeTaskId = taskOrderBeforeDrag[taskIndexBeforeDrag + 1] ?? null;
  const taskRestoreTarget = `native-preview.task-reorder.${debugTaskId}.before.${restoreBeforeTaskId ?? "end"}`;
  assertTarget(seededTaskDragged, taskRestoreTarget);
  const seededTaskDragRestored = await request(address, {
    command: "click",
    targetId: taskRestoreTarget,
  });
  assertSuccess(seededTaskDragRestored, "restore seeded Native task order by drag");
  assertEqual(
    JSON.stringify(seededTaskDragRestored.observation.taskOrder),
    JSON.stringify(taskOrderBeforeDrag),
    "task order after drag restoration",
  );
  summary.checks.push(
    "task drag ordering used NanaUI before-value targets and one atomic Product Core reorder command",
  );
  const seededTaskForReorder = await request(address, {
    command: "click",
    targetId: `native-preview.task.${debugTaskId}`,
  });
  assertSuccess(seededTaskForReorder, "open seeded Native task for reorder");
  assertTarget(seededTaskForReorder, "native-preview.task-session.task.move-down");
  assertNotIncludes(
    seededTaskForReorder.observation.visibleTargetIds,
    "native-preview.task-session.task.move-up",
    "seeded task move-up target before reorder",
  );
  const seededTaskMovedDown = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.task.move-down",
  });
  assertSuccess(seededTaskMovedDown, "move seeded Native task down");
  assertTarget(seededTaskMovedDown, "native-preview.task-session.task.move-up");
  const seededTaskMovedUp = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.task.move-up",
  });
  assertSuccess(seededTaskMovedUp, "restore seeded Native task order");
  assertNotIncludes(
    seededTaskMovedUp.observation.visibleTargetIds,
    "native-preview.task-session.task.move-up",
    "seeded task move-up target after restoring order",
  );
  assertTarget(seededTaskMovedUp, "native-preview.task-session.task.parent-target");
  const parentTargetSelected = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.task.parent-target",
  });
  assertSuccess(parentTargetSelected, "select a Native parent task");
  assertTarget(parentTargetSelected, "native-preview.task-session.task.reparent");
  const taskReparented = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.task.reparent",
  });
  assertSuccess(taskReparented, "reparent the seeded Native task");
  if (!taskReparented.observation.selectedTaskParent) {
    throw new Error("Native task reparent did not persist a Product Core parent id");
  }
  assertTarget(taskReparented, "native-preview.task-session.task.parent-clear");
  const taskParentCleared = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.task.parent-clear",
  });
  assertSuccess(taskParentCleared, "restore the seeded Native task to root");
  assertEqual(taskParentCleared.observation.selectedTaskParent, null, "restored task parent");
  const cloneRootDropTarget =
    `native-preview.task-drop.${debugTaskId}.project.${clonedProjectId}.parent.root`;
  assertTarget(taskParentCleared, cloneRootDropTarget);
  const taskMovedToClone = await request(address, {
    command: "click",
    targetId: cloneRootDropTarget,
  });
  assertSuccess(taskMovedToClone, "drag seeded task to cloned Native project root");
  assertEqual(
    taskMovedToClone.observation.selectedProject,
    clonedProjectId,
    "task target project after Native move",
  );
  assertEqual(
    taskMovedToClone.observation.selectedTask,
    debugTaskId,
    "task selection after Native move",
  );
  assertTarget(taskMovedToClone, "native-preview.task-session.task.drop-search");
  const taskParentSearch = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.task.drop-search",
    text: debugPlanReplayTaskId,
  });
  assertSuccess(taskParentSearch, "search Native task drag parent destinations");
  const seededNestedDropTarget =
    `native-preview.task-drop.${debugTaskId}.project.${seededProjectId}.parent.${debugPlanReplayTaskId}`;
  assertTarget(taskParentSearch, seededNestedDropTarget);
  await recordRenderedWindow(
    child.pid,
    taskLocationDropScreenshotPath,
    "task cross-project nested drag destination",
    "taskLocationDropWindowBounds",
    "taskLocationDropPngSize",
  );
  const taskMovedBack = await request(address, {
    command: "click",
    targetId: seededNestedDropTarget,
  });
  assertSuccess(taskMovedBack, "drag seeded task under a searched Native parent");
  assertEqual(
    taskMovedBack.observation.selectedProject,
    seededProjectId,
    "task project after Native move restoration",
  );
  assertEqual(
    taskMovedBack.observation.selectedTask,
    debugTaskId,
    "task selection after Native move restoration",
  );
  assertEqual(
    taskMovedBack.observation.selectedTaskParent,
    debugPlanReplayTaskId,
    "task parent after searched nested drop",
  );
  assertTarget(taskMovedBack, "native-preview.task-session.task.parent-clear");
  const nestedTaskRestoredToRoot = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.task.parent-clear",
  });
  assertSuccess(nestedTaskRestoredToRoot, "restore nested Native task to project root");
  assertEqual(
    nestedTaskRestoredToRoot.observation.selectedTaskParent,
    null,
    "task parent after nested drag restoration",
  );
  const inboxRootDropTarget =
    `native-preview.task-drop.${debugTaskId}.project.inbox.parent.root`;
  assertTarget(nestedTaskRestoredToRoot, inboxRootDropTarget);
  const taskMovedToInbox = await request(address, {
    command: "click",
    targetId: inboxRootDropTarget,
  });
  assertSuccess(taskMovedToInbox, "drag seeded task to the Native inbox root");
  assertEqual(taskMovedToInbox.observation.inboxSelected, true, "inbox task location");
  assertEqual(taskMovedToInbox.observation.selectedProject, null, "inbox project identity");
  assertEqual(taskMovedToInbox.observation.selectedTask, debugTaskId, "inbox task selection");
  const inboxOverview = await request(address, {
    command: "click",
    targetId: "native-preview.inbox",
  });
  assertSuccess(inboxOverview, "open the Native inbox");
  assertEqual(inboxOverview.observation.page, "inbox", "Native inbox page");
  assertEqual(inboxOverview.observation.selectedTask, null, "Native inbox overview selection");
  assertTarget(inboxOverview, `native-preview.task.${debugTaskId}`);
  const inboxTaskReopened = await request(address, {
    command: "click",
    targetId: `native-preview.task.${debugTaskId}`,
  });
  assertSuccess(inboxTaskReopened, "reopen an orphan task from the Native inbox");
  assertEqual(inboxTaskReopened.observation.inboxSelected, true, "reopened inbox task location");
  const seededRootDropTarget =
    `native-preview.task-drop.${debugTaskId}.project.${seededProjectId}.parent.root`;
  assertTarget(inboxTaskReopened, seededRootDropTarget);
  const taskRestoredFromInbox = await request(address, {
    command: "click",
    targetId: seededRootDropTarget,
  });
  assertSuccess(taskRestoredFromInbox, "drag inbox task back to the seeded project root");
  assertEqual(
    taskRestoredFromInbox.observation.selectedProject,
    seededProjectId,
    "task project after inbox restoration",
  );
  assertEqual(
    taskRestoredFromInbox.observation.selectedTask,
    debugTaskId,
    "task selection after inbox restoration",
  );
  const projectOverviewAfterTaskReorder = await request(address, {
    command: "click",
    targetId: "native-preview.workspace.tab.overview",
  });
  assertSuccess(projectOverviewAfterTaskReorder, "return to project overview after task reorder");
  assertTarget(projectOverviewAfterTaskReorder, "native-preview.projects");
  const allProjectsOverview = await request(address, {
    command: "click",
    targetId: "native-preview.projects",
  });
  assertSuccess(allProjectsOverview, "open the aggregate Native projects overview");
  assertEqual(allProjectsOverview.observation.page, "projects/overview", "projects overview page");
  assertApplicationWorkspaceItem(
    allProjectsOverview,
    "projects-workspace",
    "application:projects",
  );
  assertTarget(allProjectsOverview, `native-preview.project.${seededProjectId}`);
  const selectedProjectFromOverview = await request(address, {
    command: "click",
    targetId: `native-preview.project.${seededProjectId}`,
  });
  assertSuccess(selectedProjectFromOverview, "open a project from the aggregate overview");
  assertEqual(
    selectedProjectFromOverview.observation.selectedProject,
    seededProjectId,
    "project selected from aggregate overview",
  );
  summary.checks.push("task ordering persisted through the shared Product Core sort order");
  summary.checks.push(
    "the projects overview used a persistent Application Workspace Item and shared Product Core status, session, activity, and usage aggregation",
  );
  summary.checks.push(
    "task hierarchy used Product Core parent ids, rejected descendant targets, and restored the selected task to root",
  );
  summary.checks.push(
    "cross-project, inbox, and searched nested task drag destinations used NanaUI passive drop targets and one typed Product Core aggregate command while retaining Workspace focus and selection",
  );

  const settings = await request(address, {
    command: "click",
    targetId: "native-preview.settings.open",
  });
  assertSuccess(settings, "open settings");
  assertEqual(settings.observation.page, "settings/appearance", "settings page");
  const settingsWorkspaceItemId = assertApplicationWorkspaceItem(
    settings,
    "settings-workspace",
    "application:settings",
  );
  assertTarget(settings, "native-preview.settings.appearance.theme.light");
  assertTarget(settings, "native-preview.settings.appearance.theme.dark");
  assertTarget(settings, "native-preview.settings.appearance.sidebar.grouped");
  assertTarget(settings, "native-preview.settings.appearance.sidebar.unified");
  summary.checks.push("settings click entered the real settings state");

  const targetTheme = settings.observation.theme === "dark" ? "light" : "dark";
  const theme = await request(address, {
    command: "click",
    targetId: `native-preview.settings.appearance.theme.${targetTheme}`,
  });
  assertSuccess(theme, "change theme");
  assertEqual(theme.observation.theme, targetTheme, "theme state");
  summary.checks.push("theme click changed the real theme state");

  const unifiedSidebar = await request(address, {
    command: "click",
    targetId: "native-preview.settings.appearance.sidebar.unified",
  });
  assertSuccess(unifiedSidebar, "switch to unified sidebar");
  assertEqual(
    unifiedSidebar.observation.sidebarDisplayMode,
    "unified",
    "unified sidebar preference",
  );
  const groupedSidebar = await request(address, {
    command: "click",
    targetId: "native-preview.settings.appearance.sidebar.grouped",
  });
  assertSuccess(groupedSidebar, "restore grouped sidebar");
  assertEqual(
    groupedSidebar.observation.sidebarDisplayMode,
    "grouped",
    "grouped sidebar preference",
  );
  summary.checks.push(
    "sidebar appearance switched between the persisted grouped project tree and the global conversation list",
  );

  const settingsWindowCount = groupedSidebar.observation.taskPopupWindowCount;
  const moveSettingsToWindowTarget =
    `native-preview.workspace.tab.${settingsWorkspaceItemId}.move-to-new-window`;
  assertTarget(theme, moveSettingsToWindowTarget);
  const settingsMovedToWindow = await request(address, {
    command: "click",
    targetId: moveSettingsToWindowTarget,
  });
  assertSuccess(settingsMovedToWindow, "move Settings Workspace Item to a new window");
  const settingsWindow = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === settingsWindowCount + 1 &&
      !observation.workspaceItemIds?.includes(settingsWorkspaceItemId) &&
      observation.workspaceWindows?.some(
        (window) =>
          window.activeItemId === settingsWorkspaceItemId &&
          window.panes?.some((pane) => pane.itemIds?.includes(settingsWorkspaceItemId)),
      ) &&
      observation.visibleTargetIds.includes("native-preview.settings.provider") &&
      observation.visibleTargetIds.includes("native-preview.projects.create") &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    10_000,
  );
  if (settingsWindow.observation.page.startsWith("settings/")) {
    throw new Error("main window remained on Settings after transferring its Workspace Item");
  }
  const providerInSettingsWindow = await request(address, {
    command: "click",
    targetId: "native-preview.settings.provider",
  });
  assertSuccess(providerInSettingsWindow, "open Provider inside the Settings workspace window");
  assertTarget(providerInSettingsWindow, "native-preview.settings.provider.refresh");
  assertNotIncludes(
    providerInSettingsWindow.observation.visibleTargetIds,
    "native-preview.settings.appearance.theme.light",
    "inactive Settings appearance controls",
  );
  const settingsReturnedFromWindow = await request(address, {
    command: "click",
    targetId: "native-preview.settings.back",
  });
  assertSuccess(settingsReturnedFromWindow, "return Settings from its workspace window");
  const settingsWindowClosed = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === settingsWindowCount &&
      observation.workspaceItemIds?.includes(settingsWorkspaceItemId) &&
      observation.workspaceWindows?.every(
        (window) => !window.panes?.some((pane) => pane.itemIds?.includes(settingsWorkspaceItemId)),
      ) &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    10_000,
  );
  assertTarget(
    settingsWindowClosed,
    `native-preview.workspace.tab.${settingsWorkspaceItemId}`,
  );
  const settingsRestoredFromWindow = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.tab.${settingsWorkspaceItemId}`,
  });
  assertSuccess(settingsRestoredFromWindow, "restore Settings after its window was closed");
  assertEqual(
    settingsRestoredFromWindow.observation.page,
    "settings/provider",
    "Settings workspace-window route persistence",
  );
  summary.checks.push(
    "Settings moved atomically into a real auxiliary Workspace window, stayed interactive alongside the main project surface, preserved its selected tab and returned without duplicating ownership",
  );

  const desktopSettings = await request(address, {
    command: "click",
    targetId: "native-preview.settings.desktop",
  });
  assertSuccess(desktopSettings, "open desktop settings");
  assertEqual(desktopSettings.observation.page, "settings/desktop", "desktop settings page");
  assertTarget(desktopSettings, "native-preview.settings.desktop.shortcut");
  assertTarget(desktopSettings, "native-preview.settings.desktop.shortcut.save");
  assertTarget(desktopSettings, "native-preview.settings.desktop.shortcut.clear");
  assertTarget(desktopSettings, "native-preview.settings.desktop.update.releases");
  if (typeof desktopSettings.observation.updateConfigured !== "boolean") {
    throw new Error("Native updater configuration state is missing");
  }
  if (typeof desktopSettings.observation.updateState !== "string") {
    throw new Error("Native updater state is missing");
  }
  if (desktopSettings.observation.updateConfigured && !desktopSettings.observation.updateBusy) {
    assertTarget(desktopSettings, "native-preview.settings.desktop.update.check");
  }
  assertEqual(desktopSettings.observation.trayActive, true, "Native tray registration");
  const shortcutCapture = await request(address, {
    command: "click",
    targetId: "native-preview.settings.desktop.shortcut",
  });
  assertSuccess(shortcutCapture, "begin Native global shortcut capture");
  assertEqual(
    shortcutCapture.observation.shellShortcutCapturing,
    true,
    "shortcut capture state",
  );
  const shortcutInput = await request(address, {
    command: "input",
    targetId: "native-preview.settings.desktop.shortcut",
    text: "Ctrl+Shift+F24",
  });
  assertSuccess(shortcutInput, "enter Native global shortcut");
  assertEqual(shortcutInput.observation.shellShortcutCapturing, false, "captured shortcut state");
  const shortcutSaved = await request(address, {
    command: "click",
    targetId: "native-preview.settings.desktop.shortcut.save",
  });
  assertSuccess(shortcutSaved, "save Native global shortcut");
  assertEqual(shortcutSaved.observation.shellShortcut, "Ctrl+Shift+F24", "saved shortcut");
  assertEqual(shortcutSaved.observation.shellShortcutActive, true, "registered shortcut");
  assertEqual(shortcutSaved.observation.shellError, null, "shortcut registration error");
  const shortcutCleared = await request(address, {
    command: "click",
    targetId: "native-preview.settings.desktop.shortcut.clear",
  });
  assertSuccess(shortcutCleared, "clear Native global shortcut");
  assertEqual(shortcutCleared.observation.shellShortcut, null, "cleared shortcut");
  assertEqual(shortcutCleared.observation.shellShortcutActive, false, "unregistered shortcut");
  summary.checks.push(
    "Native desktop settings exposed the real updater state and registered and unregistered a real global shortcut while the tray stayed active",
  );

  const dataImportSettings = await request(address, {
    command: "click",
    targetId: "native-preview.settings.data",
  });
  assertSuccess(dataImportSettings, "open Native data import settings");
  assertEqual(
    dataImportSettings.observation.page,
    "settings/data",
    "data import settings page",
  );
  assertTarget(dataImportSettings, "native-preview.settings.data.pick-source");
  const dataImportPlanning = await request(address, {
    command: "click",
    targetId: "native-preview.settings.data.pick-source",
  });
  assertSuccess(dataImportPlanning, "select isolated legacy import source");
  const dataImportPlanned = await waitForObservation(
    address,
    (observation) =>
      observation.page === "settings/data" &&
      observation.dataImportBusy === false &&
      observation.dataImportPlanStatus === "empty",
    10_000,
  );
  assertEqual(dataImportPlanned.observation.dataImportHasSource, true, "import source state");
  assertEqual(dataImportPlanned.observation.dataImportError, null, "import plan error");
  assertTarget(dataImportPlanned, "native-preview.settings.data.execute");
  const dataImportExecuting = await request(address, {
    command: "click",
    targetId: "native-preview.settings.data.execute",
  });
  assertSuccess(dataImportExecuting, "execute isolated data import plan");
  const dataImportFinished = await waitForObservation(
    address,
    (observation) =>
      observation.page === "settings/data" &&
      observation.dataImportBusy === false &&
      observation.dataImportReportStatus === "nothing_to_import",
    10_000,
  );
  assertEqual(
    dataImportFinished.observation.dataImportRestartRequired,
    false,
    "empty import restart state",
  );
  assertTarget(dataImportFinished, "native-preview.settings.data.reset");
  const dataImportReset = await request(address, {
    command: "click",
    targetId: "native-preview.settings.data.reset",
  });
  assertSuccess(dataImportReset, "reset completed data import");
  assertEqual(dataImportReset.observation.dataImportHasSource, false, "reset import source");
  assertEqual(dataImportReset.observation.dataImportPlanStatus, null, "reset import plan");
  summary.checks.push(
    "Native data migration used stable debug targets to select, plan, execute, report, and reset a real isolated import without touching the source",
  );

  const aboutSettings = await request(address, {
    command: "click",
    targetId: "native-preview.settings.about",
  });
  assertSuccess(aboutSettings, "open Native about settings");
  assertEqual(aboutSettings.observation.page, "settings/about", "about settings page");
  assertTarget(aboutSettings, "native-preview.settings.about");
  summary.checks.push(
    "Native About settings exposed the application version and generated third-party license manifest",
  );

  const providerSettings = await request(address, {
    command: "click",
    targetId: "native-preview.settings.provider",
  });
  assertSuccess(providerSettings, "open provider settings");
  assertEqual(providerSettings.observation.page, "settings/provider", "provider settings page");
  const openAiProviderTarget =
    "native-preview.settings.provider.mutsuki.credential.openai";
  assertTarget(providerSettings, openAiProviderTarget);
  const openAiProvider = await request(address, {
    command: "click",
    targetId: openAiProviderTarget,
  });
  assertSuccess(openAiProvider, "select OpenAI provider");
  assertEqual(
    openAiProvider.observation.providerId,
    "mutsuki.credential.openai",
    "selected provider",
  );
  assertTarget(openAiProvider, "native-preview.settings.provider.runtime.model");
  assertTarget(openAiProvider, "native-preview.settings.provider.runtime.openai-endpoint");
  assertTarget(openAiProvider, "native-preview.settings.provider.runtime.anthropic-endpoint");
  const initialProviderRuntimeRevision = openAiProvider.observation.providerRuntimeRevision;
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.settings.provider.runtime.openai-endpoint",
      text: modelFixture.endpoint,
    }),
    "enter OpenAI-compatible runtime endpoint",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.settings.provider.runtime.anthropic-endpoint",
      text: "https://anthropic.example.test/v1/messages",
    }),
    "enter Anthropic runtime endpoint",
  );
  const providerModelInput = await request(address, {
    command: "input",
    targetId: "native-preview.settings.provider.runtime.model",
    text: "native-debug-model",
  });
  assertSuccess(providerModelInput, "enter default Native runtime model");
  assertEqual(providerModelInput.observation.providerRuntimeDirty, true, "runtime settings draft");
  assertTarget(providerModelInput, "native-preview.settings.provider.runtime.save");
  const providerRuntimeSaved = await request(address, {
    command: "click",
    targetId: "native-preview.settings.provider.runtime.save",
  });
  assertSuccess(providerRuntimeSaved, "save default Native runtime model");
  assertEqual(
    providerRuntimeSaved.observation.providerRuntimeRevision,
    initialProviderRuntimeRevision + 1,
    "provider runtime revision",
  );
  assertEqual(
    providerRuntimeSaved.observation.providerRuntimeModel,
    "native-debug-model",
    "saved Native runtime model",
  );
  assertEqual(
    providerRuntimeSaved.observation.providerOpenAiEndpoint,
    modelFixture.endpoint,
    "saved OpenAI-compatible endpoint",
  );
  assertEqual(
    providerRuntimeSaved.observation.providerAnthropicEndpoint,
    "https://anthropic.example.test/v1/messages",
    "saved Anthropic endpoint",
  );
  assertEqual(providerRuntimeSaved.observation.providerRuntimeDirty, false, "saved runtime draft");
  assertTarget(providerRuntimeSaved, "native-preview.settings.provider.runtime.reset");
  assertTarget(openAiProvider, "native-preview.settings.provider.secret");
  assertTarget(providerSettings, "native-preview.settings.provider.refresh");
  if (providerSettings.observation.providerActiveCredentialCount < 1) {
    throw new Error("debug model credential was not visible through the typed provider snapshot");
  }
  const initialProviderCredentialCount = providerSettings.observation.providerCredentialCount;
  const initialProviderActiveCredentialCount =
    providerSettings.observation.providerActiveCredentialCount;
  const initialRevokeTargets = new Set(
    openAiProvider.observation.visibleTargetIds.filter(
      (target) =>
        target.startsWith("native-preview.settings.provider.credential.") &&
        target.endsWith(".revoke"),
    ),
  );
  const providerInput = await request(address, {
    command: "input",
    targetId: "native-preview.settings.provider.secret",
    text: providerSecretCanary,
  });
  assertSuccess(providerInput, "enter provider credential");
  if (JSON.stringify(providerInput).includes(providerSecretCanary)) {
    throw new Error("provider secret leaked into the Agent Debug response");
  }
  if (fs.readFileSync(transcriptPath, "utf8").includes(providerSecretCanary)) {
    throw new Error("provider secret leaked into the Agent Debug transcript");
  }
  assertTarget(providerInput, "native-preview.settings.provider.save");
  const providerSaved = await request(address, {
    command: "click",
    targetId: "native-preview.settings.provider.save",
  });
  assertSuccess(providerSaved, "save provider credential");
  const providerReady = await waitForObservation(
    address,
    (observation) =>
      !observation.providerBusy &&
      observation.providerError === null &&
      observation.providerCredentialCount === initialProviderCredentialCount + 1 &&
      observation.providerProfileHasCredentialRefs &&
      observation.providerLiveModelAdapter,
    30_000,
  );
  const savedCredentialRevoke = providerReady.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.settings.provider.credential.") &&
      target.endsWith(".revoke") &&
      !initialRevokeTargets.has(target),
  );
  if (!savedCredentialRevoke) {
    throw new Error("saved provider credential has no stable revoke target");
  }
  const providerRevoked = await request(address, {
    command: "click",
    targetId: savedCredentialRevoke,
  });
  assertSuccess(providerRevoked, "revoke provider credential");
  await waitForObservation(
    address,
    (observation) =>
      !observation.providerBusy &&
      observation.providerError === null &&
      observation.providerActiveCredentialCount === initialProviderActiveCredentialCount,
    30_000,
  );
  summary.checks.push(
    "Provider settings saved and revoked a real Broker credential without exposing secret material",
  );

  const agentSettings = await request(address, {
    command: "click",
    targetId: "native-preview.settings.agent",
  });
  assertSuccess(agentSettings, "open Agent settings");
  assertEqual(agentSettings.observation.page, "settings/agent", "Agent settings page");
  assertTarget(agentSettings, "native-preview.settings.agent.subagents");
  assertTarget(agentSettings, "native-preview.settings.agent.non-interrupt");
  assertTarget(agentSettings, "native-preview.settings.agent.debug");
  assertTarget(agentSettings, "native-preview.settings.agent.auto-turn");
  assertTarget(agentSettings, "native-preview.settings.agent.custom.new");
  const initialAgentRevision = agentSettings.observation.agentInteractionRevision;
  const initialAgentCount = agentSettings.observation.customAgentCount;
  const initialAgentEnabledCount = agentSettings.observation.customAgentEnabledCount;
  const initialCustomAgentTargets = new Set(
    agentSettings.observation.visibleTargetIds.filter((target) =>
      target.startsWith("native-preview.settings.agent.custom."),
    ),
  );
  const initialSubagentsEnabled = agentSettings.observation.agentSubagentsEnabled;
  const initialNonInterruptMode = agentSettings.observation.agentNonInterruptMode;
  const initialAgentDebugEnabled = agentSettings.observation.agentDebugEnabled;
  const initialAutoTurnEnabled = agentSettings.observation.agentAutoTurnEnabled;
  if (!initialAgentDebugEnabled) {
    const debugEnabled = await request(address, {
      command: "click",
      targetId: "native-preview.settings.agent.debug",
    });
    assertSuccess(debugEnabled, "enable Native Debug timeline panel");
    assertEqual(debugEnabled.observation.agentDebugEnabled, true, "Debug panel enabled");
  }
  const nonInterruptToggled = await request(address, {
    command: "click",
    targetId: "native-preview.settings.agent.non-interrupt",
  });
  assertSuccess(nonInterruptToggled, "toggle Native non-interrupt mode");
  assertEqual(
    nonInterruptToggled.observation.agentNonInterruptMode,
    !initialNonInterruptMode,
    "toggled Native non-interrupt mode",
  );
  const nonInterruptRestored = await request(address, {
    command: "click",
    targetId: "native-preview.settings.agent.non-interrupt",
  });
  assertSuccess(nonInterruptRestored, "restore Native non-interrupt mode");
  assertEqual(
    nonInterruptRestored.observation.agentNonInterruptMode,
    initialNonInterruptMode,
    "restored Native non-interrupt mode",
  );
  const subagentModeToggled = await request(address, {
    command: "click",
    targetId: "native-preview.settings.agent.subagents",
  });
  assertSuccess(subagentModeToggled, "toggle Native subagent mode");
  assertEqual(
    subagentModeToggled.observation.agentSubagentsEnabled,
    !initialSubagentsEnabled,
    "toggled Native subagent mode",
  );
  const subagentModeRestored = await request(address, {
    command: "click",
    targetId: "native-preview.settings.agent.subagents",
  });
  assertSuccess(subagentModeRestored, "restore Native subagent mode");
  assertEqual(
    subagentModeRestored.observation.agentSubagentsEnabled,
    initialSubagentsEnabled,
    "restored Native subagent mode",
  );
  const autoTurnToggled = await request(address, {
    command: "click",
    targetId: "native-preview.settings.agent.auto-turn",
  });
  assertSuccess(autoTurnToggled, "toggle Native auto-turn decision");
  assertEqual(
    autoTurnToggled.observation.agentAutoTurnEnabled,
    !initialAutoTurnEnabled,
    "toggled Native auto-turn decision",
  );
  const autoTurnRestored = await request(address, {
    command: "click",
    targetId: "native-preview.settings.agent.auto-turn",
  });
  assertSuccess(autoTurnRestored, "restore Native auto-turn decision");
  assertEqual(
    autoTurnRestored.observation.agentAutoTurnEnabled,
    initialAutoTurnEnabled,
    "restored Native auto-turn decision",
  );
  const customAgentNew = await request(address, {
    command: "click",
    targetId: "native-preview.settings.agent.custom.new",
  });
  assertSuccess(customAgentNew, "open custom Agent editor");
  assertEqual(customAgentNew.observation.customAgentEditorOpen, true, "custom Agent editor");
  assertTarget(customAgentNew, "native-preview.settings.agent.custom.name");
  assertTarget(customAgentNew, "native-preview.settings.agent.custom.description");
  assertTarget(customAgentNew, "native-preview.settings.agent.custom.instruction");
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.settings.agent.custom.name",
      text: "Native Researcher",
    }),
    "enter custom Agent name",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.settings.agent.custom.description",
      text: "Finds code ownership and evidence",
    }),
    "enter custom Agent description",
  );
  const customAgentInstruction = "Inspect the workspace read-only and return concise evidence.";
  const customAgentDraft = await request(address, {
    command: "input",
    targetId: "native-preview.settings.agent.custom.instruction",
    text: customAgentInstruction,
  });
  assertSuccess(customAgentDraft, "enter custom Agent instruction");
  assertEqual(
    customAgentDraft.observation.customAgentInstructionLength,
    customAgentInstruction.length,
    "custom Agent instruction length",
  );
  assertTarget(customAgentDraft, "native-preview.settings.agent.custom.save");
  await recordRenderedWindow(
    child.pid,
    agentScreenshotPath,
    "Agent settings",
    "agentWindowBounds",
    "agentPngSize",
  );
  const customAgentSaved = await request(address, {
    command: "click",
    targetId: "native-preview.settings.agent.custom.save",
  });
  assertSuccess(customAgentSaved, "save custom Agent");
  assertEqual(
    customAgentSaved.observation.customAgentCount,
    initialAgentCount + 1,
    "saved custom Agent count",
  );
  assertEqual(customAgentSaved.observation.customAgentEditorOpen, false, "closed Agent editor");
  if (customAgentSaved.observation.agentInteractionRevision < initialAgentRevision + 5) {
    throw new Error("Agent interaction revision did not advance for real setting/catalog writes");
  }
  const customAgentEditTarget = customAgentSaved.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.settings.agent.custom.") &&
      target.endsWith(".edit") &&
      !initialCustomAgentTargets.has(target),
  );
  const customAgentToggleTarget = customAgentSaved.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.settings.agent.custom.") &&
      target.endsWith(".toggle") &&
      !initialCustomAgentTargets.has(target),
  );
  const customAgentDeleteTarget = customAgentSaved.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.settings.agent.custom.") &&
      target.endsWith(".delete") &&
      !initialCustomAgentTargets.has(target),
  );
  if (!customAgentEditTarget || !customAgentToggleTarget || !customAgentDeleteTarget) {
    throw new Error("saved custom Agent did not expose stable edit/toggle/delete targets");
  }
  const customAgentToggled = await request(address, {
    command: "click",
    targetId: customAgentToggleTarget,
  });
  assertSuccess(customAgentToggled, "toggle custom Agent");
  assertEqual(
    customAgentToggled.observation.customAgentEnabledCount,
    initialAgentEnabledCount,
    "disabled custom Agent",
  );
  const customAgentEditing = await request(address, {
    command: "click",
    targetId: customAgentEditTarget,
  });
  assertSuccess(customAgentEditing, "edit custom Agent");
  assertEqual(
    customAgentEditing.observation.customAgentNameDraft,
    "Native Researcher",
    "custom Agent edit draft",
  );
  const customAgentDeleted = await request(address, {
    command: "click",
    targetId: customAgentDeleteTarget,
  });
  assertSuccess(customAgentDeleted, "delete custom Agent");
  assertEqual(customAgentDeleted.observation.customAgentCount, initialAgentCount, "Agent cleanup");
  assertEqual(customAgentDeleted.observation.agentInteractionError, null, "Agent settings error");
  summary.checks.push(
    "Agent settings used stable targets for persisted mode toggles and real custom Agent create/edit/enable/delete operations",
  );

  const providerClosedForRestart = await request(address, {
    command: "click",
    targetId: "native-preview.settings.back",
  });
  assertSuccess(providerClosedForRestart, "close provider settings before restart recovery");
  const approvalProject = await request(address, {
    command: "click",
    targetId: "native-preview.project.native-agent-debug-project",
  });
  assertSuccess(approvalProject, "open seeded project for restart recovery");
  assertTarget(approvalProject, "native-preview.task.native-agent-debug-task");
  const approvalTask = await request(address, {
    command: "click",
    targetId: "native-preview.task.native-agent-debug-task",
  });
  assertSuccess(approvalTask, "open seeded task for restart recovery");
  for (const action of [
    "plan",
    "ask-user",
    "ask-user-multi",
    "ask-user-preview",
    "ask-user-flow",
    "permission",
    "todo-tool",
    "todo",
    "command",
    "file-read",
    "file-change",
  ]) {
    assertTarget(approvalTask, `native-preview.debug.timeline.${action}`);
  }
  const timelineCountBeforeDebug = approvalTask.observation.timelineEventCount;
  const debugCommand = await request(address, {
    command: "click",
    targetId: "native-preview.debug.timeline.command",
  });
  assertSuccess(debugCommand, "inject Native Debug command card");
  assertEqual(
    debugCommand.observation.timelineEventCount,
    timelineCountBeforeDebug + 1,
    "Debug command card visible in timeline",
  );
  const debugPlan = await request(address, {
    command: "click",
    targetId: "native-preview.debug.timeline.plan",
  });
  assertSuccess(debugPlan, "inject Native Debug plan interaction");
  const debugPlanRequestId = debugPlan.observation.pendingInteractionIds.find((requestId) =>
    requestId.startsWith("native-debug:request:"),
  );
  if (!debugPlanRequestId) {
    throw new Error("Debug plan did not enter the real pending interaction surface");
  }
  const debugPlanApproved = await request(address, {
    command: "click",
    targetId: `native-preview.task-session.plan.${debugPlanRequestId}.approve`,
  });
  assertSuccess(debugPlanApproved, "approve Native Debug plan interaction");
  assertNotIncludes(
    debugPlanApproved.observation.pendingInteractionIds,
    debugPlanRequestId,
    "resolved Debug plan interaction",
  );
  const debugAskFlow = await request(address, {
    command: "click",
    targetId: "native-preview.debug.timeline.ask-user-flow",
  });
  assertSuccess(debugAskFlow, "inject Native Debug multi-question interaction");
  const debugAskRequestId = debugAskFlow.observation.pendingInteractionIds.find((requestId) =>
    requestId.startsWith("native-debug:request:"),
  );
  if (!debugAskRequestId) {
    throw new Error("Debug multi-question interaction did not enter the pending surface");
  }
  const debugAskFirstChoice = await request(address, {
    command: "click",
    targetId: `native-preview.task-session.interaction.${debugAskRequestId}.option.1`,
  });
  assertSuccess(debugAskFirstChoice, "select the first Debug AskUser answer");
  assertTarget(
    debugAskFirstChoice,
    `native-preview.task-session.interaction.${debugAskRequestId}.submit`,
  );
  const debugAskNext = await request(address, {
    command: "click",
    targetId: `native-preview.task-session.interaction.${debugAskRequestId}.submit`,
  });
  assertSuccess(debugAskNext, "advance the Native Debug AskUser flow");
  assertIncludes(
    debugAskNext.observation.pendingInteractionIds,
    debugAskRequestId,
    "pending Debug AskUser second question",
  );
  assertTarget(
    debugAskNext,
    `native-preview.task-session.interaction.${debugAskRequestId}.back`,
  );
  const debugAskMultiChoice = await request(address, {
    command: "click",
    targetId: `native-preview.task-session.interaction.${debugAskRequestId}.option.0`,
  });
  assertSuccess(debugAskMultiChoice, "select a multi-choice Debug AskUser answer");
  const debugAskCompleted = await request(address, {
    command: "click",
    targetId: `native-preview.task-session.interaction.${debugAskRequestId}.submit`,
  });
  assertSuccess(debugAskCompleted, "complete the Native Debug AskUser flow");
  assertNotIncludes(
    debugAskCompleted.observation.pendingInteractionIds,
    debugAskRequestId,
    "resolved Debug AskUser interaction",
  );
  summary.checks.push(
    "the persisted Debug setting exposed all 11 Native timeline fixtures, injected a real ephemeral command card, and completed plan plus structured multi-question AskUser interactions through the shared pending-action surface",
  );
  assertTarget(approvalTask, "native-preview.task-session.iab.open");
  const iabOpened = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.iab.open",
  });
  assertSuccess(iabOpened, "open Native IAB Dock");
  assertTarget(iabOpened, "native-preview.task-session.iab.url");
  assertTarget(iabOpened, "native-preview.task-session.iab.navigate");
  const iabUrl = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.iab.url",
    text: modelFixture.endpoint,
  });
  assertSuccess(iabUrl, "enter Native IAB fixture URL");
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.iab.navigate",
    }),
    "navigate the Native IAB",
  );
  const iabReady = await waitForObservation(
    address,
    (observation) =>
      observation.iabDockOpen &&
      observation.iabBrowserAttached &&
      observation.iabBrowserReady &&
      observation.iabUrl === modelFixture.endpoint &&
      observation.iabError === null,
    30_000,
  );
  assertTarget(iabReady, "native-preview.task-session.iab.close");
  assertTarget(iabReady, "native-preview.task-session.iab.open-window");
  await recordRenderedWindow(
    child.pid,
    iabScreenshotPath,
    "IAB",
    "iabWindowBounds",
    "iabPngSize",
  );
  const iabWindowOpening = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.iab.open-window",
  });
  assertSuccess(iabWindowOpening, "open the Native IAB task window");
  const iabWindowReady = await waitForObservation(
    address,
    (observation) =>
      observation.iabWindowCount === 1 &&
      observation.iabWindowReadyCount === 1 &&
      observation.iabWindowTaskIds?.[0] === debugTaskId &&
      observation.iabWindowUrls?.[0] === modelFixture.endpoint &&
      observation.iabWindowError === null,
    30_000,
  );
  const iabWindowTarget = iabWindowReady.observation.visibleTargetIds.find((target) =>
    /^native-preview\.iab-window\.\d+$/.test(target),
  );
  if (!iabWindowTarget) {
    throw new Error("ready Native IAB task window did not expose its structured target");
  }
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: `${iabWindowTarget}.note`,
      text: "确认本地页面状态",
    }),
    "enter the Native IAB result note",
  );
  assertTarget(iabWindowReady, `${iabWindowTarget}.submit`);
  const iabSubmitted = await request(address, {
    command: "click",
    targetId: `${iabWindowTarget}.submit`,
  });
  assertSuccess(iabSubmitted, "capture and submit the Native IAB result");
  const iabSubmissionComplete = await waitForObservation(
    address,
    (observation) =>
      observation.iabWindowCapturePendingCount === 0 &&
      typeof observation.iabWindowNotice === "string" &&
      observation.iabWindowNotice.length > 0 &&
      observation.iabWindowError === null,
    30_000,
  );
  const submittedIabSnapshot = await waitForFirstPng(
    path.join(previewHome, "cache", "iab-snapshots"),
    10_000,
  );
  summary.iabSnapshotPath = submittedIabSnapshot;
  summary.iabSnapshotPngSize = fs.statSync(submittedIabSnapshot).size;
  assertTarget(iabSubmissionComplete, `${iabWindowTarget}.close`);
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${iabWindowTarget}.close`,
    }),
    "close the Native IAB task window",
  );
  await waitForObservation(address, (observation) => observation.iabWindowCount === 0, 10_000);
  const iabClosed = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.iab.close",
  });
  assertSuccess(iabClosed, "close Native IAB Dock");
  if (iabClosed.observation.iabDockOpen) {
    throw new Error("Native IAB Dock remained open after close");
  }
  summary.checks.push(
    "the Native IAB Dock and task window navigated a local fixture, captured a PNG, submitted durable Agent context and closed without losing the task",
  );
  const draftBeforeRestart = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: composerRestartDraft,
  });
  assertSuccess(draftBeforeRestart, "enter unsent Composer restart draft");
  assertEqual(
    draftBeforeRestart.observation.composerLength,
    composerRestartDraft.length,
    "unsent Composer draft length",
  );
  const draftRevision = draftBeforeRestart.observation.composerRevision;
  if (!Number.isInteger(draftRevision) || draftRevision <= 0) {
    throw new Error(`unsent Composer draft has invalid revision ${draftRevision}`);
  }
  const draftProcessId = child.pid;
  ({ child, address } = await restartPreviewProcess(child, previewEnvironment));
  summary.restarts.push({
    kind: "composer-draft",
    firstProcessId: draftProcessId,
    restoredProcessId: child.pid,
  });
  debugAddress = address;
  const restoredDraft = await waitForObservation(
    address,
    (observation) =>
      observation.selectedTask === debugTaskId &&
      observation.composerLength === composerRestartDraft.length &&
      observation.composerRevision === draftRevision &&
      observation.visibleTargetIds.includes("native-preview.task-session.composer.input"),
    30_000,
  );
  assertEqual(restoredDraft.observation.composerRevision, draftRevision, "restored draft revision");
  const clearedRestartDraft = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: "",
  });
  assertSuccess(clearedRestartDraft, "clear restored Composer draft");
  assertEqual(clearedRestartDraft.observation.composerLength, 0, "cleared Composer draft length");
  assertEqual(
    clearedRestartDraft.observation.composerRevision,
    draftRevision + 1,
    "cleared Composer draft revision",
  );
  summary.checks.push(
    "an unsent main Composer draft retained its exact length and revision across a forced process restart, then cleared through the real input target",
  );
  const slashQuery = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: "/sta",
  });
  assertSuccess(slashQuery, "enter Native slash command query");
  await waitForObservation(
    address,
    (observation) =>
      observation.composerLength === "/sta".length &&
      observation.visibleTargetIds.includes("native-preview.task-session.composer.slash.status"),
    10_000,
  );
  const slashSelected = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.slash.status",
  });
  assertSuccess(slashSelected, "select Native status slash command");
  assertEqual(slashSelected.observation.composerLength, "/status".length, "slash command length");
  const slashRevision = slashSelected.observation.composerRevision;
  const slashTimelineTarget =
    `native-preview.task-session.timeline.desktop-slash:${debugTaskId}:${slashRevision}`;
  const slashSent = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.send",
  });
  assertSuccess(slashSent, "execute Native status slash command");
  assertEqual(slashSent.observation.composerLength, 0, "cleared slash command length");
  assertTarget(slashSent, slashTimelineTarget);
  if (slashSent.observation.turnState !== null) {
    throw new Error(`Native slash command unexpectedly started an Agent turn: ${slashSent.observation.turnState}`);
  }
  summary.checks.push(
    "slash command search, stable selection, revisioned Composer clear and Product timeline projection used the real Native application path",
  );
  const frontendSlash = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: "/front",
  });
  assertSuccess(frontendSlash, "enter Native frontend workflow slash query");
  const frontendSlashTarget = "native-preview.task-session.composer.slash.frontend";
  const frontendSlashReady = await waitForObservation(
    address,
    (observation) => observation.visibleTargetIds.includes(frontendSlashTarget),
    10_000,
  );
  assertTarget(frontendSlashReady, frontendSlashTarget);
  assertSuccess(
    await request(address, { command: "click", targetId: frontendSlashTarget }),
    "start the Native frontend workflow from its slash command",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "completed" &&
      observation.composerLength === 0 &&
      observation.taskActionError === null,
    30_000,
  );
  const reviewSlash = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: "/review",
  });
  assertSuccess(reviewSlash, "enter Native review workflow slash query");
  const reviewSlashTarget = "native-preview.task-session.composer.slash.review";
  const reviewSlashReady = await waitForObservation(
    address,
    (observation) => observation.visibleTargetIds.includes(reviewSlashTarget),
    10_000,
  );
  assertTarget(reviewSlashReady, reviewSlashTarget);
  const reviewTargetReady = await request(address, {
    command: "click",
    targetId: reviewSlashTarget,
  });
  assertSuccess(reviewTargetReady, "open the Native review target selector");
  const reviewBranchTarget = "native-preview.task-window.0.review-workflow.target.branch";
  assertTarget(reviewTargetReady, reviewBranchTarget);
  const reviewBranchSelected = await request(address, {
    command: "click",
    targetId: reviewBranchTarget,
  });
  assertSuccess(reviewBranchSelected, "select the Native review base-branch target");
  const reviewTargetInput = "native-preview.task-window.0.review-workflow.target-input";
  assertTarget(reviewBranchSelected, reviewTargetInput);
  const reviewBranchEntered = await request(address, {
    command: "input",
    targetId: reviewTargetInput,
    text: workflowReviewBranch,
  });
  assertSuccess(reviewBranchEntered, "enter the Native review base branch");
  const reviewSubmitTarget = "native-preview.task-window.0.review-workflow.submit";
  assertTarget(reviewBranchEntered, reviewSubmitTarget);
  assertSuccess(
    await request(address, { command: "click", targetId: reviewSubmitTarget }),
    "start the Native base-branch review workflow",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "completed" &&
      observation.composerLength === 0 &&
      observation.taskActionError === null,
    30_000,
  );
  if (!summary.modelFixtureRequests?.some((entry) => entry.reviewBranchSeen)) {
    throw new Error("Native review workflow did not carry the selected base branch to the model request");
  }
  const retryFailureEntered = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: retryFailurePrompt,
  });
  assertSuccess(retryFailureEntered, "enter the Native retry failure fixture prompt");
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.send",
    }),
    "send the Native retry failure fixture prompt",
  );
  const retryReady = await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "failed" &&
      observation.visibleTargetIds.some(
        (target) =>
          target.startsWith("native-preview.task-window.0.timeline.") &&
          target.endsWith(".retry"),
      ),
    30_000,
  );
  const retryTarget = retryReady.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.task-window.0.timeline.") && target.endsWith(".retry"),
  );
  if (!retryTarget) throw new Error("Native failed turn did not expose its retry target");
  assertSuccess(
    await request(address, { command: "click", targetId: retryTarget }),
    "retry the failed Native timeline event",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "completed" && observation.taskActionError === null,
    30_000,
  );
  const retryRequests = summary.modelFixtureRequests?.filter(
    (entry) => entry.retryFailurePromptSeen,
  ).length ?? 0;
  if (retryRequests < 2) {
    throw new Error(`Native retry sent ${retryRequests} matching model requests instead of two`);
  }
  summary.checks.push(
    "Native workflow slash commands submitted typed task/review workflows with a real review target, and failed timeline events retried their original durable message without replacing the Composer draft",
  );
  const conversationQuery = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: "#计划",
  });
  assertSuccess(conversationQuery, "enter Native conversation reference query");
  const conversationCandidate =
    `native-preview.task-session.composer.conversation.${debugPlanReplayTaskId}`;
  const conversationResults = await waitForObservation(
    address,
    (observation) => observation.visibleTargetIds.includes(conversationCandidate),
    10_000,
  );
  assertTarget(conversationResults, conversationCandidate);
  const conversationSelected = await request(address, {
    command: "click",
    targetId: conversationCandidate,
  });
  assertSuccess(conversationSelected, "select Native conversation reference");
  assertEqual(
    conversationSelected.observation.composerConversationReferenceCount,
    1,
    "selected conversation reference count",
  );
  assertEqual(
    conversationSelected.observation.composerLength,
    0,
    "conversation query removed from Composer",
  );
  assertTarget(
    conversationSelected,
    `native-preview.task-session.composer.conversation.${debugPlanReplayTaskId}.remove`,
  );
  const contextQuery = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: "@README",
  });
  assertSuccess(contextQuery, "enter Native project context query");
  const contextCandidate = "native-preview.task-session.composer.context.README.md";
  const contextResults = await waitForObservation(
    address,
    (observation) => observation.visibleTargetIds.includes(contextCandidate),
    10_000,
  );
  assertTarget(contextResults, contextCandidate);
  const contextSelected = await request(address, {
    command: "click",
    targetId: contextCandidate,
  });
  assertSuccess(contextSelected, "select Native project context attachment");
  assertEqual(
    contextSelected.observation.composerAttachmentCount,
    1,
    "selected project context attachment count",
  );
  assertEqual(
    contextSelected.observation.composerConversationReferenceCount,
    1,
    "retained conversation reference count",
  );
  assertEqual(contextSelected.observation.composerLength, 0, "context query removed from Composer");
  const referenceRequestBaseline = summary.modelFixtureRequests?.length ?? 0;
  const referencePrompt = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: "native-context-reference",
  });
  assertSuccess(referencePrompt, "enter Native referenced turn prompt");
  const referenceSent = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.send",
  });
  assertSuccess(referenceSent, "send Native referenced turn");
  await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "completed" &&
      observation.taskActionError === null &&
      observation.composerLength === 0 &&
      observation.composerAttachmentCount === 0 &&
      observation.composerConversationReferenceCount === 0 &&
      observation.contextUsageUsedTokens === 4 &&
      observation.contextUsageLimitTokens === null &&
      observation.contextUsageUsedPercent === null &&
      !observation.visibleTargetIds.includes("native-preview.task-session.composer.interrupt"),
    30_000,
  );
  const referencedFixtureRequest = summary.modelFixtureRequests
    ?.slice(referenceRequestBaseline)
    .find((entry) => entry.contextReferencePromptSeen);
  if (
    !referencedFixtureRequest?.conversationReferenceSeen ||
    !referencedFixtureRequest?.contextAttachmentSeen
  ) {
    throw new Error("Native referenced turn did not preserve both conversation and project context references");
  }
  const automaticDecisionRequest = summary.modelFixtureRequests
    ?.slice(referenceRequestBaseline)
    .find((entry) => entry.autoTurnDecisionSeen);
  if (automaticDecisionRequest?.reasoningEffort !== "low") {
    throw new Error("Native automatic turn decision did not use the low-cost control-model effort");
  }
  if (referencedFixtureRequest?.reasoningEffort !== "medium") {
    throw new Error("Native Agent turn did not receive the reasoning effort selected by auto-turn");
  }
  summary.checks.push(
    "conversation and project-context search, stable selection, revision-safe Composer persistence and Agent dispatch used the real Native path",
  );
  summary.checks.push(
    "Native Composer used a real control-model decision and durably applied its model tier, reasoning effort and mode selection before Agent dispatch",
  );
  summary.checks.push(
    "Native context usage came from the persisted AgentKit usage event without inventing a context limit",
  );
  const markdownReady = await waitForObservation(
    address,
    (observation) =>
      observation.markdownTableCount >= 1 &&
      observation.markdownCopyTargetCount >= 1 &&
      observation.visibleTargetIds.some(
        (target) =>
          target.startsWith("native-preview.task-session.timeline.") && target.endsWith(".copy"),
      ),
    10_000,
  );
  const markdownCopyTarget = markdownReady.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.task-session.timeline.") && target.endsWith(".copy"),
  );
  if (!markdownCopyTarget) {
    throw new Error("Native Markdown event did not expose a stable complete-copy target");
  }
  const markdownEventId = markdownCopyTarget.slice(
    "native-preview.task-session.timeline.".length,
    -".copy".length,
  );
  const markdownCopied = await request(address, {
    command: "click",
    targetId: markdownCopyTarget,
  });
  assertSuccess(markdownCopied, "copy complete Native Markdown event");
  if (markdownCopied.observation.lastCopiedMarkdownEventId === null) {
    const copyError = markdownCopied.observation.taskActionError ?? "clipboard write did not complete";
    summary.markdownClipboardGateError = copyError;
    clipboardGateError ??= new Error(`Native Markdown clipboard write failed: ${copyError}`);
  } else {
    assertEqual(
      markdownCopied.observation.lastCopiedMarkdownEventId,
      markdownEventId,
      "copied Markdown event identity",
    );
    if (markdownCopied.observation.lastCopiedMarkdownBytes <= 0) {
      throw new Error("Native Markdown complete-copy action reported an empty clipboard payload");
    }
  }
  summary.checks.push(
    "Native Markdown rendered a structured GFM table and exposed a real host-backed complete-document copy action",
  );
  const approvalInput = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: "native-approval",
  });
  assertSuccess(approvalInput, "enter approval prompt");
  const approvalSent = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.send",
  });
  assertSuccess(approvalSent, "send approval turn");
  const waitingApproval = await waitForObservation(
    address,
    (observation) =>
      observation.visibleTargetIds.some((target) =>
        target.startsWith("native-preview.task-session.approval.") && target.endsWith(".deny"),
      ),
    30_000,
  );
  const denyTarget = waitingApproval.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.task-session.approval.") && target.endsWith(".deny"),
  );
  if (!denyTarget) throw new Error("Native approval deny target was not found");
  await waitForObservation(
    address,
    (observation) =>
      observation.workspaceRevision >= 3 &&
      observation.workspacePersistedRevision === observation.workspaceRevision,
    10_000,
  );

  const firstProcessId = child.pid;
  ({ child, address } = await restartPreviewProcess(child, previewEnvironment));
  summary.restart = { firstProcessId, restoredProcessId: child.pid };
  summary.restarts.push({ kind: "permission", ...summary.restart });
  debugAddress = address;

  const restartedWorkspace = await request(address, { command: "observe" });
  assertSuccess(restartedWorkspace, "observe restored Native workspace session");
  assertEqual(
    restartedWorkspace.observation.selectedTask,
    debugTaskId,
    "persisted workspace task selection",
  );
  assertEqual(
    restartedWorkspace.observation.workspaceSessionId,
    "native-preview.primary",
    "workspace session identity",
  );
  if (!restartedWorkspace.observation.workspaceItemIds.includes(`task:${debugTaskId}`)) {
    throw new Error("restored Native workspace did not retain the selected task item");
  }
  const restoredTaskItem = restartedWorkspace.observation.workspaceItems.find(
    (item) => item.id === `task:${debugTaskId}`,
  );
  if (!restoredTaskItem) throw new Error("restored Native workspace item descriptor is missing");
  assertEqual(restoredTaskItem.kind, "task", "workspace item kind");
  assertEqual(
    restoredTaskItem.resourceId,
    `task:${debugTaskId}`,
    "workspace item resource identity",
  );
  assertEqual(
    restoredTaskItem.title,
    "验证 Native Composer 与时间线",
    "workspace item title from Product Core",
  );
  assertEqual(restoredTaskItem.focusTarget, "composer", "workspace item focus target");
  assertEqual(restoredTaskItem.closable, true, "workspace item close capability");
  assertEqual(restoredTaskItem.splittable, true, "workspace item split capability");
  assertEqual(restoredTaskItem.persistent, true, "workspace item persistence capability");

  const restartedProject = await request(address, {
    command: "click",
    targetId: "native-preview.project.native-agent-debug-project",
  });
  assertSuccess(restartedProject, "open seeded project after Native restart");
  assertTarget(restartedProject, "native-preview.task.native-agent-debug-task");
  const restartedTask = await request(address, {
    command: "click",
    targetId: "native-preview.task.native-agent-debug-task",
  });
  assertSuccess(restartedTask, "open pending task after Native restart");
  const restoredApproval = await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "waiting_approval" &&
      observation.taskActionError === null &&
      observation.visibleTargetIds.includes(denyTarget),
    30_000,
  );
  assertEqual(
    restoredApproval.observation.selectedTask,
    "native-agent-debug-task",
    "restored approval task",
  );
  const denied = await request(address, { command: "click", targetId: denyTarget });
  assertSuccess(denied, "deny Native permission approval");
  const approvalCompleted = await waitForObservation(
    address,
    (observation) =>
      !observation.visibleTargetIds.some((target) =>
        target.startsWith("native-preview.task-session.approval."),
      ) &&
      observation.turnState === "cancelled" &&
      observation.taskActionError === null &&
      !observation.visibleTargetIds.includes("native-preview.task-session.composer.interrupt"),
    30_000,
  );
  summary.checks.push(
    "process restart restored the persisted permission approval and denial cancelled the real Native turn without an application error",
  );

  const fifoActiveInput = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: fifoActivePrompt,
  });
  assertSuccess(fifoActiveInput, "enter FIFO active approval prompt");
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.send",
    }),
    "send FIFO active approval turn",
  );
  const fifoWaitingApproval = await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "waiting_approval" &&
      typeof observation.activeTurnId === "string" &&
      observation.activeTurnDurableState === "claimed" &&
      Number.isInteger(observation.activeTurnClaimAttempts) &&
      observation.activeTurnClaimAttempts >= 1 &&
      observation.activeTurnOwnedByCurrentEpoch === true &&
      observation.durableTurnCount === 1 &&
      observation.queueDepth === 0 &&
      observation.queuedTurnIds?.length === 0 &&
      observation.visibleTargetIds.some(
        (target) =>
          target.startsWith("native-preview.task-session.approval.") &&
          target.endsWith(".approve"),
      ),
    30_000,
  );
  const fifoActiveTurnId = fifoWaitingApproval.observation.activeTurnId;
  const fifoActiveClaimAttempts = fifoWaitingApproval.observation.activeTurnClaimAttempts;
  const fifoGuideBaseline = {
    pending: fifoWaitingApproval.observation.pendingGuideCount,
    queued: fifoWaitingApproval.observation.queuedGuideCount,
    sent: fifoWaitingApproval.observation.sentGuideCount,
  };
  const fifoQuarantineBaseline = fifoWaitingApproval.observation.quarantinedTurnCount ?? 0;
  const fifoApproveTarget = fifoWaitingApproval.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.task-session.approval.") && target.endsWith(".approve"),
  );
  if (!fifoActiveTurnId || !fifoApproveTarget) {
    throw new Error("FIFO approval fixture did not expose its active turn and approval target");
  }

  const fifoQueuedIds = [];
  for (const [index, prompt] of [fifoFirstPrompt, fifoSecondPrompt].entries()) {
    const drafted = await request(address, {
      command: "input",
      targetId: "native-preview.task-session.composer.input",
      text: prompt,
    });
    assertSuccess(drafted, `enter FIFO queued prompt ${index + 1}`);
    assertEqual(drafted.observation.composerLength, prompt.length, `FIFO draft ${index + 1} length`);
    assertTarget(drafted, "native-preview.task-session.composer.send");
    assertSuccess(
      await request(address, {
        command: "click",
        targetId: "native-preview.task-session.composer.send",
      }),
      `enqueue FIFO turn ${index + 1}`,
    );
    const queued = await waitForObservation(
      address,
      (observation) =>
        observation.activeTurnId === fifoActiveTurnId &&
        observation.activeTurnDurableState === "claimed" &&
        observation.activeTurnOwnedByCurrentEpoch === true &&
        observation.durableTurnCount === index + 2 &&
        observation.queueDepth === index + 1 &&
        observation.queuedTurnIds?.length === index + 1 &&
        observation.pendingGuideCount === fifoGuideBaseline.pending &&
        observation.queuedGuideCount === fifoGuideBaseline.queued + index + 1 &&
        observation.sentGuideCount === fifoGuideBaseline.sent,
      10_000,
    );
    assertEqual(queued.observation.composerLength, 0, `cleared FIFO draft ${index + 1}`);
    const queuedId = queued.observation.queuedTurnIds[index];
    if (typeof queuedId !== "string" || queuedId.length === 0) {
      throw new Error(`FIFO turn ${index + 1} did not expose a stable turn id`);
    }
    if (
      index > 0 &&
      JSON.stringify(queued.observation.queuedTurnIds.slice(0, index)) !==
        JSON.stringify(fifoQueuedIds)
    ) {
      throw new Error("enqueue changed the order or identity of earlier FIFO turns");
    }
    fifoQueuedIds.push(queuedId);
  }
  if (new Set([fifoActiveTurnId, ...fifoQueuedIds]).size !== 3) {
    throw new Error("FIFO active and queued turns did not receive distinct stable ids");
  }
  const corruptedQueuedTurn = await request(address, {
    command: "corrupt-queued-turn",
    turnId: fifoQueuedIds[0],
  });
  assertSuccess(corruptedQueuedTurn, "inject a corrupt queued Native turn before restart");
  assertEqual(
    corruptedQueuedTurn.observation.durableTurnCount,
    3,
    "fault injection preserves the durable row until recovery",
  );

  const fifoProcessId = child.pid;
  ({ child, address } = await restartPreviewProcess(child, previewEnvironment));
  summary.restarts.push({
    kind: "turn-queue",
    firstProcessId: fifoProcessId,
    restoredProcessId: child.pid,
  });
  debugAddress = address;
  const restoredFifo = await waitForObservation(
    address,
    (observation) =>
      observation.selectedTask === debugTaskId &&
      observation.turnState === "waiting_approval" &&
      observation.activeTurnId === fifoActiveTurnId &&
      observation.activeTurnDurableState === "claimed" &&
      observation.activeTurnOwnedByCurrentEpoch === true &&
      observation.activeTurnClaimAttempts > fifoActiveClaimAttempts &&
      observation.durableTurnCount === 2 &&
      observation.queueDepth === 1 &&
      JSON.stringify(observation.queuedTurnIds) === JSON.stringify([fifoQueuedIds[1]]) &&
      observation.quarantinedTurnCount === fifoQuarantineBaseline + 1 &&
      observation.quarantinedTurnIds?.includes(fifoQueuedIds[0]) &&
      observation.quarantineReasonCodes?.includes("invalid_request_json") &&
      observation.pendingGuideCount === fifoGuideBaseline.pending + 1 &&
      observation.queuedGuideCount === fifoGuideBaseline.queued + 1 &&
      observation.sentGuideCount === fifoGuideBaseline.sent &&
      observation.visibleTargetIds.includes(fifoApproveTarget),
    30_000,
  );
  assertEqual(restoredFifo.observation.activeTurnId, fifoActiveTurnId, "restored FIFO active turn");
  assertEqual(
    JSON.stringify(restoredFifo.observation.queuedTurnIds),
    JSON.stringify([fifoQueuedIds[1]]),
    "restored FIFO order after corrupt-row isolation",
  );
  assertSuccess(
    await request(address, { command: "click", targetId: fifoApproveTarget }),
    "approve restored FIFO active turn",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.activeTurnId === fifoQueuedIds[1] &&
      observation.activeTurnDurableState === "claimed" &&
      observation.activeTurnOwnedByCurrentEpoch === true &&
      observation.durableTurnCount === 1 &&
      observation.queueDepth === 0 &&
      observation.queuedTurnIds?.length === 0 &&
      observation.pendingGuideCount === fifoGuideBaseline.pending + 1 &&
      observation.queuedGuideCount === fifoGuideBaseline.queued &&
      observation.sentGuideCount === fifoGuideBaseline.sent + 1,
    30_000,
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.activeTurnId === null &&
      observation.activeTurnDurableState === null &&
      observation.activeTurnClaimAttempts === null &&
      observation.durableTurnCount === 0 &&
      observation.queueDepth === 0 &&
      observation.queuedTurnIds?.length === 0 &&
      observation.turnState === "completed" &&
      observation.pendingGuideCount === fifoGuideBaseline.pending &&
      observation.queuedGuideCount === fifoGuideBaseline.queued &&
      observation.sentGuideCount === fifoGuideBaseline.sent + 2 &&
      observation.quarantinedTurnCount === fifoQuarantineBaseline + 1 &&
      observation.taskActionError === null,
    30_000,
  );
  const fifoFirstRequestIndex = summary.modelFixtureRequests.findIndex(
    (entry) => entry.fifoFirstPromptSeen,
  );
  const fifoSecondRequestIndex = summary.modelFixtureRequests.findIndex(
    (entry) => entry.fifoSecondPromptSeen,
  );
  if (
    fifoFirstRequestIndex < 0 ||
    fifoSecondRequestIndex < 0 ||
    fifoSecondRequestIndex >= fifoFirstRequestIndex
  ) {
    throw new Error(
      `model fixture did not execute the surviving FIFO prompt before the recovered Guide retry: ${fifoFirstRequestIndex}, ${fifoSecondRequestIndex}`,
    );
  }
  summary.checks.push(
    "a corrupt queued Composer Guide was transactionally quarantined across a forced restart, the following valid Guide resumed first, and the isolated Guide retried later as a new durable turn",
  );

  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.task-session.composer.input",
      text: guideCancelActivePrompt,
    }),
    "enter Guide cancellation approval prompt",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.send",
    }),
    "send Guide cancellation approval turn",
  );
  const guideCancelWaiting = await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "waiting_approval" &&
      typeof observation.activeTurnId === "string" &&
      observation.activeTurnDurableState === "claimed" &&
      observation.activeTurnOwnedByCurrentEpoch === true &&
      observation.durableTurnCount === 1 &&
      observation.queueDepth === 0,
    30_000,
  );
  const guideCancelActiveTurnId = guideCancelWaiting.observation.activeTurnId;
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.task-session.composer.input",
      text: guideCancelQueuedPrompt,
    }),
    "enter queued Guide that will be cancelled",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.send",
    }),
    "queue Guide before explicit cancellation",
  );
  const guideQueuedBeforeCancel = await waitForObservation(
    address,
    (observation) =>
      observation.activeTurnId === guideCancelActiveTurnId &&
      observation.activeTurnDurableState === "claimed" &&
      observation.activeTurnOwnedByCurrentEpoch === true &&
      observation.durableTurnCount === 2 &&
      observation.queueDepth === 1 &&
      observation.queuedGuideCount === fifoGuideBaseline.queued + 1 &&
      observation.sentGuideCount === fifoGuideBaseline.sent + 2 &&
      observation.visibleTargetIds.includes("native-preview.task-session.composer.interrupt"),
    10_000,
  );
  const pendingBeforeGuideCancel = guideQueuedBeforeCancel.observation.pendingGuideCount;
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.interrupt",
    }),
    "explicitly cancel active turn and queued Guide",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.activeTurnId === null &&
      observation.activeTurnDurableState === null &&
      observation.durableTurnCount === 0 &&
      observation.queueDepth === 0 &&
      observation.queuedTurnIds?.length === 0 &&
      observation.turnState === "cancelled" &&
      observation.pendingGuideCount === pendingBeforeGuideCancel + 1 &&
      observation.queuedGuideCount === fifoGuideBaseline.queued &&
      observation.sentGuideCount === fifoGuideBaseline.sent + 2,
    30_000,
  );
  summary.checks.push(
    "durable claim ownership survived restart, FIFO ack claimed each next turn, and explicit cancellation cleared all durable rows without promotion while restoring its queued Guide to pending",
  );

  const modelRequestsBeforeToolRecovery = summary.modelFixtureRequests.length;
  const seededInterruptedTool = await request(address, {
    command: "seed-interrupted-tool",
    taskId: debugTaskId,
    turnId: interruptedToolTurnId,
  });
  assertSuccess(seededInterruptedTool, "seed a durable interrupted tool boundary");
  assertEqual(
    seededInterruptedTool.observation.activeTurnId,
    interruptedToolTurnId,
    "seeded interrupted tool turn id",
  );
  assertEqual(
    seededInterruptedTool.observation.durableTurnCount,
    1,
    "seeded interrupted tool durable row",
  );
  const toolRecoveryProcessId = child.pid;
  ({ child, address } = await restartPreviewProcess(child, previewEnvironment));
  summary.restarts.push({
    kind: "tool-side-effect-recovery",
    firstProcessId: toolRecoveryProcessId,
    restoredProcessId: child.pid,
  });
  debugAddress = address;
  const restoredToolRecovery = await waitForObservation(
    address,
    (observation) =>
      observation.selectedTask === debugTaskId &&
      observation.activeTurnId === interruptedToolTurnId &&
      observation.activeTurnDurableState === "claimed" &&
      observation.activeTurnOwnedByCurrentEpoch === true &&
      observation.turnState === "waiting_interaction" &&
      observation.pendingInteractionIds?.includes(`${interruptedToolTurnId}:tool`) &&
      observation.pendingInteractionKinds?.includes("ask_user") &&
      observation.visibleTargetIds.includes(
        `native-preview.task-session.interaction.${interruptedToolTurnId}:tool.option.0`,
      ),
    30_000,
  );
  assertEqual(
    summary.modelFixtureRequests.length,
    modelRequestsBeforeToolRecovery,
    "interrupted tool recovery does not call the Provider before confirmation",
  );
  const interruptedToolOutput = path.join(
    previewWorkspace,
    "native-agent-debug-tool-recovery.txt",
  );
  if (fs.existsSync(interruptedToolOutput)) {
    throw new Error("interrupted tool was replayed before the user confirmed its state");
  }
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.task-session.interaction.${interruptedToolTurnId}:tool.option.0`,
    }),
    "confirm that the interrupted tool had already completed",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.activeTurnId === null &&
      observation.activeTurnDurableState === null &&
      observation.durableTurnCount === 0 &&
      observation.turnState === "completed" &&
      !observation.pendingInteractionIds?.includes(`${interruptedToolTurnId}:tool`) &&
      observation.taskActionError === null,
    30_000,
  );
  if (fs.existsSync(interruptedToolOutput)) {
    throw new Error("user-confirmed recovery unexpectedly replayed the interrupted tool");
  }
  const toolRecoveryRequests = summary.modelFixtureRequests
    .slice(modelRequestsBeforeToolRecovery)
    .filter((request) => request.toolRecoveryCompletionSeen);
  assertEqual(
    toolRecoveryRequests.length,
    1,
    "confirmed tool recovery performs one Provider continuation for its tool result",
  );
  if (!toolRecoveryRequests[0]?.toolRecoveryCompletionSeen) {
    throw new Error("Provider continuation did not receive the user-confirmed tool result");
  }
  summary.checks.push(
    "a forced restart after a durable ToolCallStarted boundary surfaced a real Native recovery interaction, never replayed the workspace write, and continued the Provider exactly once only after user confirmation",
  );

  const busyDraft = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: databaseBusyPrompt,
  });
  assertSuccess(busyDraft, "enter Composer draft for SQLite busy recovery");
  assertEqual(
    busyDraft.observation.composerLength,
    databaseBusyPrompt.length,
    "SQLite busy draft length",
  );
  assertSuccess(
    await request(address, {
      command: "hold-database-writer",
      durationMs: "6500",
    }),
    "hold an external SQLite writer lock",
  );
  const blockedSubmission = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.send",
  });
  assertSuccess(blockedSubmission, "attempt Composer submission while SQLite is busy");
  assertEqual(
    blockedSubmission.observation.composerLength,
    databaseBusyPrompt.length,
    "SQLite busy submission preserves Composer content",
  );
  assertEqual(
    blockedSubmission.observation.taskActionError,
    "无法发送消息，请重试。",
    "SQLite busy submission uses a user-safe diagnostic",
  );
  assertEqual(
    blockedSubmission.observation.durableTurnCount,
    0,
    "SQLite busy submission does not leave a partial durable turn",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.send",
    }),
    "retry Composer submission after SQLite writer release",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.composerLength === 0 &&
      observation.activeTurnId === null &&
      observation.durableTurnCount === 0 &&
      observation.turnState === "completed" &&
      observation.taskActionError === null,
    30_000,
  );
  summary.checks.push(
    "an external SQLite writer exhausted the busy timeout without clearing the Composer or partially enqueueing a turn, then the unchanged draft retried successfully after release",
  );

  const planTaskList = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.back",
  });
  assertSuccess(planTaskList, "return to seeded task list for plan recovery");
  assertTarget(planTaskList, `native-preview.task.${debugPlanReplayTaskId}`);
  const planTask = await request(address, {
    command: "click",
    targetId: `native-preview.task.${debugPlanReplayTaskId}`,
  });
  assertSuccess(planTask, "open seeded plan replay task");
  assertEqual(planTask.observation.selectedTask, debugPlanReplayTaskId, "plan replay task");
  const approvalWorkspaceTab = `native-preview.workspace.tab.task:${debugTaskId}`;
  const planWorkspaceTab = `native-preview.workspace.tab.task:${debugPlanReplayTaskId}`;
  const planWorkspaceTabDragLeft = `${planWorkspaceTab}.drag-left`;
  assertTarget(planTask, approvalWorkspaceTab);
  assertTarget(planTask, planWorkspaceTab);
  assertTarget(planTask, planWorkspaceTabDragLeft);
  const planPaneBeforeReorder = planTask.observation.workspacePanes.find((pane) =>
    pane.itemIds.includes(`task:${debugPlanReplayTaskId}`),
  );
  const planIndexBeforeReorder = planPaneBeforeReorder?.itemIds.indexOf(
    `task:${debugPlanReplayTaskId}`,
  );
  if (!planPaneBeforeReorder || !Number.isInteger(planIndexBeforeReorder) || planIndexBeforeReorder < 1) {
    throw new Error("plan Workspace tab did not have a real left neighbor");
  }
  const expectedReorderedItems = [...planPaneBeforeReorder.itemIds];
  [expectedReorderedItems[planIndexBeforeReorder - 1], expectedReorderedItems[planIndexBeforeReorder]] =
    [expectedReorderedItems[planIndexBeforeReorder], expectedReorderedItems[planIndexBeforeReorder - 1]];
  const planTabDraggedLeft = await request(address, {
    command: "click",
    targetId: planWorkspaceTabDragLeft,
  });
  assertSuccess(planTabDraggedLeft, "reorder the plan Workspace tab through the drag contract");
  const reorderedTaskPane = planTabDraggedLeft.observation.workspacePanes.find(
    (pane) =>
      pane.itemIds.includes(`task:${debugTaskId}`) &&
      pane.itemIds.includes(`task:${debugPlanReplayTaskId}`),
  );
  assertEqual(
    JSON.stringify(reorderedTaskPane?.itemIds),
    JSON.stringify(expectedReorderedItems),
    "reordered Workspace tab order",
  );
  if (
    planTabDraggedLeft.observation.workspacePersistedRevision !==
    planTabDraggedLeft.observation.workspaceRevision
  ) {
    await waitForObservation(
      address,
      (observation) => observation.workspacePersistedRevision === observation.workspaceRevision,
      10_000,
    );
  }
  const approvalTabActivated = await request(address, {
    command: "click",
    targetId: approvalWorkspaceTab,
  });
  assertSuccess(approvalTabActivated, "activate the persisted approval task workspace tab");
  assertEqual(
    approvalTabActivated.observation.selectedTask,
    debugTaskId,
    "workspace tab task selection",
  );
  const approvalTabClose = `${approvalWorkspaceTab}.close`;
  assertTarget(approvalTabActivated, approvalTabClose);
  const approvalPaneBeforeClose = approvalTabActivated.observation.workspacePanes.find((pane) =>
    pane.itemIds.includes(`task:${debugTaskId}`),
  );
  const approvalIndexBeforeClose = approvalPaneBeforeClose?.itemIds.indexOf(`task:${debugTaskId}`);
  if (!approvalPaneBeforeClose || !Number.isInteger(approvalIndexBeforeClose)) {
    throw new Error("active approval Workspace tab did not have a pane owner");
  }
  const itemsAfterApprovalClose = approvalPaneBeforeClose.itemIds.filter(
    (itemId) => itemId !== `task:${debugTaskId}`,
  );
  const expectedNeighborItemId =
    itemsAfterApprovalClose[
      Math.min(approvalIndexBeforeClose, itemsAfterApprovalClose.length - 1)
    ];
  const approvalTabClosed = await request(address, {
    command: "click",
    targetId: approvalTabClose,
  });
  assertSuccess(approvalTabClosed, "close the active approval task workspace tab");
  if (approvalTabClosed.observation.workspaceItemIds.includes(`task:${debugTaskId}`)) {
    throw new Error("closed Native workspace tab remained registered in the pane tree");
  }
  if (!approvalTabClosed.observation.activeWorkspaceItemIds.includes(expectedNeighborItemId)) {
    throw new Error("closing the active Native tab did not activate its pane neighbor");
  }
  const planTabReactivated = await request(address, {
    command: "click",
    targetId: planWorkspaceTab,
  });
  assertSuccess(planTabReactivated, "reactivate plan task after neighbor selection check");
  assertEqual(
    planTabReactivated.observation.selectedTask,
    debugPlanReplayTaskId,
    "plan task after neighbor selection check",
  );
  summary.checks.push(
    "NanaUI Tabs reordered the real pane item sequence through the shared drag contract, persisted it, activated cross-task state, and close selected the real pane neighbor",
  );
  const planModeEnabled = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.plan-mode",
  });
  assertSuccess(planModeEnabled, "enable plan mode for restart replay");
  assertEqual(planModeEnabled.observation.composerPlanMode, true, "plan replay mode");
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.task-session.composer.input",
      text: "native-plan-restart",
    }),
    "enter plan restart prompt",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.send",
    }),
    "send plan restart turn",
  );
  const waitingPlan = await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "waiting_interaction" &&
      observation.visibleTargetIds.some(
        (target) =>
          target.startsWith("native-preview.task-session.plan.") && target.endsWith(".approve"),
      ),
    30_000,
  );
  const approvePlanTarget = waitingPlan.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.task-session.plan.") && target.endsWith(".approve"),
  );
  if (!approvePlanTarget) throw new Error("Native plan approve target was not found");
  await waitForObservation(
    address,
    (observation) =>
      observation.workspaceRevision >= 3 &&
      observation.workspacePersistedRevision === observation.workspaceRevision,
    10_000,
  );

  const planProcessId = child.pid;
  ({ child, address } = await restartPreviewProcess(child, previewEnvironment));
  summary.restarts.push({
    kind: "plan",
    firstProcessId: planProcessId,
    restoredProcessId: child.pid,
  });
  debugAddress = address;
  const restartedPlanWorkspace = await request(address, { command: "observe" });
  assertSuccess(restartedPlanWorkspace, "observe restored plan workspace session");
  assertEqual(
    restartedPlanWorkspace.observation.selectedTask,
    debugPlanReplayTaskId,
    "persisted plan workspace task selection",
  );
  if (
    !restartedPlanWorkspace.observation.activeWorkspaceItemIds.includes(
      `task:${debugPlanReplayTaskId}`,
    )
  ) {
    throw new Error("restored Native workspace did not reactivate the plan task item");
  }
  const restoredPlanItem = restartedPlanWorkspace.observation.workspaceItems.find(
    (item) => item.id === `task:${debugPlanReplayTaskId}`,
  );
  assertEqual(
    restoredPlanItem?.title,
    "验证 Native 计划重启回放",
    "restored plan item title from Product Core",
  );
  assertEqual(
    restoredPlanItem?.resourceId,
    `task:${debugPlanReplayTaskId}`,
    "restored plan item resource identity",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.project.${debugProjectId}`,
    }),
    "open seeded project after plan restart",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.task.${debugPlanReplayTaskId}`,
    }),
    "open pending plan task after restart",
  );
  const restoredPlan = await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "waiting_interaction" &&
      observation.taskActionError === null &&
      observation.visibleTargetIds.includes(approvePlanTarget),
    30_000,
  );
  assertTarget(restoredPlan, approvePlanTarget);
  const approvedPlan = await request(address, {
    command: "click",
    targetId: approvePlanTarget,
  });
  assertSuccess(approvedPlan, "approve restored Native plan");
  await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "completed" &&
      observation.taskActionError === null &&
      !observation.visibleTargetIds.some((target) =>
        target.startsWith("native-preview.task-session.plan."),
      ),
    30_000,
  );
  summary.checks.push(
    "process restart restored a persisted plan interaction and approval resumed the same Native Agent turn to completion",
  );
  summary.checks.push(
    "window-scoped Native workspace state restored the selected task independently of Product and Agent runtime recovery",
  );

  const questionTaskList = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.back",
  });
  assertSuccess(questionTaskList, "return to seeded task list for question recovery");
  assertTarget(questionTaskList, `native-preview.task.${debugQuestionReplayTaskId}`);
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.task.${debugQuestionReplayTaskId}`,
    }),
    "open seeded question replay task",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.task-session.composer.input",
      text: "native-question-restart",
    }),
    "enter question restart prompt",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.send",
    }),
    "send question restart turn",
  );
  const waitingQuestion = await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "waiting_interaction" &&
      observation.visibleTargetIds.some(
        (target) =>
          target.startsWith("native-preview.task-session.interaction.") &&
          target.endsWith(".option.1"),
      ),
    30_000,
  );
  const answerQuestionTarget = waitingQuestion.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.task-session.interaction.") &&
      target.endsWith(".option.1"),
  );
  if (!answerQuestionTarget) throw new Error("Native question answer target was not found");
  await waitForObservation(
    address,
    (observation) =>
      observation.workspaceRevision >= 3 &&
      observation.workspacePersistedRevision === observation.workspaceRevision,
    10_000,
  );

  const questionProcessId = child.pid;
  ({ child, address } = await restartPreviewProcess(child, previewEnvironment));
  summary.restarts.push({
    kind: "question",
    firstProcessId: questionProcessId,
    restoredProcessId: child.pid,
  });
  debugAddress = address;
  const restartedQuestionWorkspace = await request(address, { command: "observe" });
  assertSuccess(restartedQuestionWorkspace, "observe restored question workspace session");
  assertEqual(
    restartedQuestionWorkspace.observation.selectedTask,
    debugQuestionReplayTaskId,
    "persisted question workspace task selection",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.project.${debugProjectId}`,
    }),
    "open seeded project after question restart",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.task.${debugQuestionReplayTaskId}`,
    }),
    "open pending question task after restart",
  );
  const restoredQuestion = await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "waiting_interaction" &&
      observation.taskActionError === null &&
      observation.visibleTargetIds.includes(answerQuestionTarget),
    30_000,
  );
  assertTarget(restoredQuestion, answerQuestionTarget);
  assertSuccess(
    await request(address, { command: "click", targetId: answerQuestionTarget }),
    "answer restored Native question",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "completed" &&
      observation.taskActionError === null &&
      !observation.visibleTargetIds.some((target) =>
        target.startsWith("native-preview.task-session.interaction."),
      ),
    30_000,
  );
  if (!summary.modelFixtureRequests.some((entry) => entry.questionAnswerSeen)) {
    throw new Error("model fixture did not receive the restored Native question answer");
  }
  summary.checks.push(
    "process restart restored a persisted AskUser interaction and the selected answer resumed the same Native Agent turn to completion",
  );

  const cancelPlanTaskList = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.back",
  });
  assertSuccess(cancelPlanTaskList, "return to seeded task list for plan cancellation");
  assertTarget(cancelPlanTaskList, `native-preview.task.${debugPlanCancelTaskId}`);
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.task.${debugPlanCancelTaskId}`,
    }),
    "open seeded plan cancellation task",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.plan-mode",
    }),
    "enable plan mode for cancellation",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.task-session.composer.input",
      text: "native-plan-cancel",
    }),
    "enter plan cancellation prompt",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.send",
    }),
    "send plan cancellation turn",
  );
  const waitingPlanCancellation = await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "waiting_interaction" &&
      observation.visibleTargetIds.some(
        (target) =>
          target.startsWith("native-preview.task-session.plan.") &&
          target.endsWith(".cancel-turn"),
      ),
    30_000,
  );
  const cancelPlanTarget = waitingPlanCancellation.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.task-session.plan.") && target.endsWith(".cancel-turn"),
  );
  if (!cancelPlanTarget) throw new Error("Native plan cancel-turn target was not found");
  assertSuccess(
    await request(address, { command: "click", targetId: cancelPlanTarget }),
    "cancel Native plan turn",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "cancelled" &&
      observation.taskActionError === null &&
      !observation.visibleTargetIds.some((target) =>
        target.startsWith("native-preview.task-session.plan."),
      ),
    30_000,
  );
  summary.checks.push(
    "plan cancellation used a dedicated stable target and terminated the exact Native Agent turn without submitting a decline response",
  );

  const architectureTaskList = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.back",
  });
  assertSuccess(architectureTaskList, "return to seeded task list for architecture approval");
  assertTarget(
    architectureTaskList,
    `native-preview.task.${debugArchitectureApprovalTaskId}`,
  );
  const architectureBaseline = await request(address, {
    command: "click",
    targetId: "native-preview.project.architecture",
  });
  assertSuccess(architectureBaseline, "load the authoritative architecture baseline");
  assertEqual(
    architectureBaseline.observation.architectureQuarantineCount,
    1,
    "architecture quarantine count",
  );
  summary.checks.push(
    "a corrupt persisted architecture snapshot was quarantined and atomically recovered from the newest valid history before the Native graph loaded",
  );
  const architectureVersionBefore = architectureBaseline.observation.architectureVersion;
  const architectureNodeCountBefore = architectureBaseline.observation.architectureNodeCount;
  const architectureHistoryCountBefore = architectureBaseline.observation.architectureHistoryCount;
  const architectureTaskListRestored = await request(address, {
    command: "click",
    targetId: "native-preview.project.tasks",
  });
  assertSuccess(architectureTaskListRestored, "return from architecture baseline to tasks");
  assertTarget(
    architectureTaskListRestored,
    `native-preview.task.${debugArchitectureApprovalTaskId}`,
  );
  const architectureTask = await request(address, {
    command: "click",
    targetId: `native-preview.task.${debugArchitectureApprovalTaskId}`,
  });
  assertSuccess(architectureTask, "open seeded architecture approval task");
  assertEqual(
    architectureTask.observation.composerPermission,
    "ask",
    "architecture approval permission",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.task-session.composer.input",
      text: architectureApprovalPrompt,
    }),
    "enter Native architecture approval prompt",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.send",
    }),
    "send Native architecture approval turn",
  );
  const architecturePending = await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "waiting_interaction" &&
      observation.pendingInteractionKinds?.includes("architecture_change") &&
      observation.visibleTargetIds.some(
        (target) =>
          target.startsWith("native-preview.task-session.architecture.") &&
          target.endsWith(".allow"),
      ),
    30_000,
  );
  const architectureAllowTarget = architecturePending.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.task-session.architecture.") &&
      target.endsWith(".allow"),
  );
  if (!architectureAllowTarget) throw new Error("Native architecture allow target was not found");
  const architectureDenyTarget = architectureAllowTarget.replace(/\.allow$/, ".deny");
  assertTarget(architecturePending, architectureDenyTarget);
  await recordRenderedWindow(
    child.pid,
    architectureApprovalScreenshotPath,
    "architecture approval",
    "architectureApprovalWindowBounds",
    "architectureApprovalPngSize",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: architectureAllowTarget,
    }),
    "apply Native architecture approval",
  );
  const architectureApplied = await waitForObservation(
    address,
    (observation) =>
      observation.architectureVersion === architectureVersionBefore + 1 &&
      observation.architectureNodeCount === architectureNodeCountBefore + 1 &&
      observation.architectureHistoryCount === architectureHistoryCountBefore + 1 &&
      !observation.pendingInteractionKinds?.includes("architecture_change") &&
      !observation.visibleTargetIds.some((target) =>
        target.startsWith("native-preview.task-session.architecture."),
      ),
    30_000,
  );
  assertEqual(architectureApplied.observation.taskActionError, null, "architecture apply error");
  if (!summary.modelFixtureRequests.some((entry) => entry.architectureResolutionSeen)) {
    throw new Error("model fixture did not receive the typed Native architecture decision");
  }
  summary.checks.push(
    "a real model tool call produced a typed Native architecture proposal, stable NanaUI targets applied it atomically, and the same Agent turn received the decision before the graph and history advanced",
  );

  const settingsAfterRestart = await request(address, {
    command: "click",
    targetId: "native-preview.settings.open",
  });
  assertSuccess(settingsAfterRestart, "open settings after restart recovery");
  const providerAfterRestart = await request(address, {
    command: "click",
    targetId: "native-preview.settings.provider",
  });
  assertSuccess(providerAfterRestart, "open provider settings after restart recovery");
  const openAiAfterRestart = await request(address, {
    command: "click",
    targetId: openAiProviderTarget,
  });
  assertSuccess(openAiAfterRestart, "select OpenAI provider after restart recovery");
  assertEqual(
    openAiAfterRestart.observation.providerRuntimeModel,
    "native-debug-model",
    "provider runtime model after restart",
  );
  assertEqual(
    openAiAfterRestart.observation.providerRuntimeRevision,
    initialProviderRuntimeRevision + 1,
    "provider runtime revision after restart",
  );
  assertEqual(
    openAiAfterRestart.observation.providerOpenAiEndpoint,
    modelFixture.endpoint,
    "OpenAI-compatible endpoint after restart",
  );
  assertEqual(
    openAiAfterRestart.observation.providerAnthropicEndpoint,
    "https://anthropic.example.test/v1/messages",
    "Anthropic endpoint after restart",
  );
  await recordRenderedWindow(
    child.pid,
    providerScreenshotPath,
    "provider settings",
    "providerWindowBounds",
    "providerPngSize",
  );
  const providerRuntimeReset = await request(address, {
    command: "click",
    targetId: "native-preview.settings.provider.runtime.reset",
  });
  assertSuccess(providerRuntimeReset, "reset Native provider runtime defaults");
  assertEqual(providerRuntimeReset.observation.providerRuntimeModel, null, "reset runtime model");
  assertEqual(
    providerRuntimeReset.observation.providerOpenAiEndpoint,
    null,
    "reset OpenAI-compatible endpoint",
  );
  assertEqual(
    providerRuntimeReset.observation.providerAnthropicEndpoint,
    null,
    "reset Anthropic endpoint",
  );
  assertEqual(providerRuntimeReset.observation.providerRuntimeDirty, false, "reset runtime draft");
  summary.checks.push(
    "Provider runtime settings used stable input/save/reset targets, persisted the selected default model and both endpoint families across process restart, and remained separate from OS-keyring credentials",
  );

  const appearanceAgain = await request(address, {
    command: "click",
    targetId: "native-preview.settings.appearance",
  });
  assertSuccess(appearanceAgain, "return to appearance settings");

  const input = await request(address, {
    command: "input",
    targetId: "native-preview.settings.appearance",
    text: "not-applicable",
  });
  if (input.ok || input.error?.code !== "target_not_available") {
    throw new Error("input did not report structured unavailable state");
  }
  summary.checks.push("input reported structured unavailable state");

  let returned = await request(address, {
    command: "click",
    targetId: "native-preview.settings.back",
  });
  assertSuccess(returned, "return from settings");
  if (returned.observation.page.startsWith("tasks/")) {
    assertTarget(returned, "native-preview.task-session.back");
    returned = await request(address, {
      command: "click",
      targetId: "native-preview.task-session.back",
    });
    assertSuccess(returned, "return from restored task after settings");
  }
  assertEqual(returned.observation.page, "projects", "returned page");
  assertTarget(returned, `native-preview.workspace.tab.${settingsWorkspaceItemId}`);
  const settingsTabRestored = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.tab.${settingsWorkspaceItemId}`,
  });
  assertSuccess(settingsTabRestored, "restore settings Workspace Item tab");
  assertEqual(
    settingsTabRestored.observation.page,
    "settings/appearance",
    "restored settings route",
  );
  assertApplicationWorkspaceItem(
    settingsTabRestored,
    "settings-workspace",
    "application:settings",
  );
  returned = await request(address, {
    command: "click",
    targetId: "native-preview.settings.back",
  });
  assertSuccess(returned, "return from restored settings Workspace Item");
  assertEqual(returned.observation.page, "projects", "restored settings return page");
  summary.checks.push(
    "Settings used one persistent application Workspace Item, restored its selected tab and returned through real tab routing",
  );

  assertTarget(returned, "native-preview.automations.open");
  const automations = await request(address, {
    command: "click",
    targetId: "native-preview.automations.open",
  });
  assertSuccess(automations, "open automations");
  assertEqual(automations.observation.page, "automations", "automations page");
  const automationWorkspaceItemId = assertApplicationWorkspaceItem(
    automations,
    "automation-workspace",
    "application:automations",
  );
  assertTarget(automations, "native-preview.automations.create");
  const automationCreated = await request(address, {
    command: "click",
    targetId: "native-preview.automations.create",
  });
  assertSuccess(automationCreated, "create Native automation");
  if (!automationCreated.observation.page.startsWith("automations/")) {
    throw new Error("created automation was not selected");
  }
  assertEqual(automationCreated.observation.automationCount, 1, "automation count");
  assertEqual(automationCreated.observation.automationNodeCount, 1, "automation node count");
  assertEqual(automationCreated.observation.automationEdgeCount, 0, "automation edge count");
  assertTarget(automationCreated, "graph.lilia-automation.canvas");
  assertTarget(automationCreated, "graph.lilia-automation.node.trigger");
  assertTarget(automationCreated, "native-preview.automations.add.human");
  assertTarget(automationCreated, "native-preview.automations.scope.include-inbox");
  const automationInboxScope = await request(address, {
    command: "click",
    targetId: "native-preview.automations.scope.include-inbox",
  });
  assertSuccess(automationInboxScope, "include inbox in Native automation scope");
  assertEqual(
    automationInboxScope.observation.automationScope.includeInbox,
    true,
    "automation inbox scope",
  );
  const automationStatusScope = await request(address, {
    command: "click",
    targetId: "native-preview.automations.scope.task-status.waiting",
  });
  assertSuccess(automationStatusScope, "add status to Native automation scope");
  assertIncludes(
    automationStatusScope.observation.automationScope.taskStatuses,
    "waiting",
    "automation task-status scope",
  );
  const automationBackendScope = await request(address, {
    command: "click",
    targetId: "native-preview.automations.scope.backend.native-agentkit",
  });
  assertSuccess(automationBackendScope, "add backend to Native automation scope");
  assertIncludes(
    automationBackendScope.observation.automationScope.backends,
    "native-agentkit",
    "automation backend scope",
  );
  const automationEventScope = await request(address, {
    command: "click",
    targetId: "native-preview.automations.scope.event-kind.task_created",
  });
  assertSuccess(automationEventScope, "add event kind to Native automation scope");
  assertIncludes(
    automationEventScope.observation.automationScope.eventKinds,
    "task_created",
    "automation event-kind scope",
  );
  const automationNodeAdded = await request(address, {
    command: "click",
    targetId: "native-preview.automations.add.human",
  });
  assertSuccess(automationNodeAdded, "add Native automation node");
  assertEqual(automationNodeAdded.observation.automationNodeCount, 2, "automation node count");
  assertTarget(automationNodeAdded, "graph.lilia-automation.node.human-2");
  const humanNodeSelected = await request(address, {
    command: "click",
    targetId: "graph.lilia-automation.node.human-2",
  });
  assertSuccess(humanNodeSelected, "select Native automation human node");
  assertEqual(
    humanNodeSelected.observation.automationSelectedNodeKind,
    "human",
    "selected automation node kind",
  );
  assertTarget(humanNodeSelected, "native-preview.automations.node.config.prompt");
  const humanPromptChanged = await request(address, {
    command: "input",
    targetId: "native-preview.automations.node.config.prompt",
    text: "请确认 Native 类型化 Inspector",
  });
  assertSuccess(humanPromptChanged, "edit Native automation human prompt");
  assertEqual(
    humanPromptChanged.observation.automationSelectedNodeConfigDraft.prompt,
    "请确认 Native 类型化 Inspector",
    "automation node config draft",
  );
  const humanNodeSaved = await request(address, {
    command: "click",
    targetId: "native-preview.automations.node.save",
  });
  assertSuccess(humanNodeSaved, "save Native automation human node");
  assertEqual(
    humanNodeSaved.observation.automationSelectedNodeConfig.prompt,
    "请确认 Native 类型化 Inspector",
    "saved automation human prompt",
  );
  const graphSelected = await request(address, {
    command: "click",
    targetId: "graph.lilia-automation.node.trigger",
  });
  assertSuccess(graphSelected, "select Native automation graph node");
  assertTarget(graphSelected, "native-preview.automations.node.title");
  assertTarget(graphSelected, "native-preview.automations.node.config");
  assertTarget(graphSelected, "native-preview.automations.node.config.triggerKind");
  const automationTitleChanged = await request(address, {
    command: "input",
    targetId: "native-preview.automations.node.title",
    text: "手动开始",
  });
  assertSuccess(automationTitleChanged, "edit Native automation node title");
  let automationConfigChanged;
  for (let index = 0; index < 5; index += 1) {
    automationConfigChanged = await request(address, {
      command: "click",
      targetId: "native-preview.automations.node.config.triggerKind",
    });
    assertSuccess(automationConfigChanged, "cycle Native automation trigger kind");
  }
  assertEqual(
    automationConfigChanged.observation.automationSelectedNodeConfigDraft.triggerKind,
    "manual",
    "automation trigger kind cycle",
  );
  assertTarget(automationConfigChanged, "native-preview.automations.node.save");
  const automationNodeSaved = await request(address, {
    command: "click",
    targetId: "native-preview.automations.node.save",
  });
  assertSuccess(automationNodeSaved, "save Native automation node inspector");
  assertEqual(
    automationNodeSaved.observation.automationSelectedNodeTitle,
    "手动开始",
    "saved automation node title",
  );
  const automationRefreshed = await request(address, {
    command: "click",
    targetId: "native-preview.automations.refresh",
  });
  assertSuccess(automationRefreshed, "reload Native automation persistence");
  assertEqual(
    automationRefreshed.observation.automationScope.includeInbox,
    true,
    "persisted automation inbox scope",
  );
  assertIncludes(
    automationRefreshed.observation.automationScope.taskStatuses,
    "waiting",
    "persisted automation task-status scope",
  );
  assertEqual(
    automationRefreshed.observation.automationSelectedNodeConfig.triggerKind,
    "manual",
    "persisted automation node config",
  );
  const automationPublished = await request(address, {
    command: "click",
    targetId: "native-preview.automations.publish",
  });
  assertSuccess(automationPublished, "publish Native automation");
  assertEqual(automationPublished.observation.automationPublished, true, "published state");
  assertTarget(automationPublished, "native-preview.automations.toggle");
  const automationEnabled = await request(address, {
    command: "click",
    targetId: "native-preview.automations.toggle",
  });
  assertSuccess(automationEnabled, "enable Native automation");
  assertEqual(automationEnabled.observation.automationEnabled, true, "automation enabled state");
  assertTarget(automationEnabled, "native-preview.automations.run");
  const automationRun = await request(address, {
    command: "click",
    targetId: "native-preview.automations.run",
  });
  assertSuccess(automationRun, "run Native automation");
  assertEqual(automationRun.observation.automationRunCount, 1, "automation run count");
  assertEqual(
    automationRun.observation.automationRunStatus,
    "waiting_user",
    "automation waiting status",
  );
  assertEqual(
    automationRun.observation.automationWaitingHumanNode,
    "human-2",
    "automation waiting human node",
  );
  assertTarget(automationRun, "native-preview.automations.run.human-response");
  assertTarget(automationRun, "native-preview.automations.run.resume");
  assertTarget(automationRun, "native-preview.automations.run.cancel");
  const automationCancelled = await request(address, {
    command: "click",
    targetId: "native-preview.automations.run.cancel",
  });
  assertSuccess(automationCancelled, "cancel Native automation");
  assertEqual(
    automationCancelled.observation.automationRunStatus,
    "cancelled",
    "automation cancelled status",
  );
  assertEqual(automationCancelled.observation.automationRunCount, 1, "cancelled run count");
  assertNotIncludes(
    automationCancelled.observation.visibleTargetIds,
    "native-preview.automations.run.cancel",
    "cancelled automation target",
  );
  const automationRetry = await request(address, {
    command: "click",
    targetId: "native-preview.automations.run",
  });
  assertSuccess(automationRetry, "run Native automation after cancellation");
  assertEqual(automationRetry.observation.automationRunCount, 2, "automation retry run count");
  assertEqual(
    automationRetry.observation.automationRunStatus,
    "waiting_user",
    "automation retry waiting status",
  );
  assertTarget(automationRetry, "native-preview.automations.run.cancel");
  const automationResponse = await request(address, {
    command: "input",
    targetId: "native-preview.automations.run.human-response",
    text: "Agent Debug 已确认",
  });
  assertSuccess(automationResponse, "enter Native automation human response");
  const automationResumed = await request(address, {
    command: "click",
    targetId: "native-preview.automations.run.resume",
  });
  assertSuccess(automationResumed, "resume Native automation");
  assertEqual(
    automationResumed.observation.automationRunStatus,
    "succeeded",
    "automation completed status",
  );
  await recordRenderedWindow(
    child.pid,
    automationScreenshotPath,
    "automation",
    "automationWindowBounds",
    "automationPngSize",
  );
  summary.checks.push(
    "Automation create, scope and typed Inspector persistence, graph selection, publish, enable, exact cancellation, retry and human resume used the shared engine, SQLite state and real WGPU canvas",
  );
  let automationsBack = await request(address, {
    command: "click",
    targetId: "native-preview.automations.back",
  });
  assertSuccess(automationsBack, "return from automations");
  assertEqual(automationsBack.observation.page, "projects", "automation return page");
  assertTarget(
    automationsBack,
    `native-preview.workspace.tab.${automationWorkspaceItemId}`,
  );
  const automationTabRestored = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.tab.${automationWorkspaceItemId}`,
  });
  assertSuccess(automationTabRestored, "restore automation Workspace Item tab");
  if (!automationTabRestored.observation.page.startsWith("automations/")) {
    throw new Error("automation Workspace Item did not restore its selected workflow route");
  }
  assertApplicationWorkspaceItem(
    automationTabRestored,
    "automation-workspace",
    "application:automations",
  );
  const automationWindowCount = automationTabRestored.observation.taskPopupWindowCount;
  const moveAutomationToWindowTarget =
    `native-preview.workspace.tab.${automationWorkspaceItemId}.move-to-new-window`;
  assertTarget(automationTabRestored, moveAutomationToWindowTarget);
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: moveAutomationToWindowTarget,
    }),
    "move Automation Workspace Item to a new window",
  );
  const automationWindow = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === automationWindowCount + 1 &&
      !observation.workspaceItemIds?.includes(automationWorkspaceItemId) &&
      observation.workspaceWindows?.some(
        (window) =>
          window.activeItemId === automationWorkspaceItemId &&
          window.panes?.some((pane) => pane.itemIds?.includes(automationWorkspaceItemId)),
      ) &&
      observation.visibleTargetIds.includes("native-preview.automations.refresh") &&
      observation.visibleTargetIds.includes("native-preview.projects.create") &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    10_000,
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.automations.refresh",
    }),
    "refresh Automation inside its workspace window",
  );
  assertTarget(automationWindow, "native-preview.automations.back");
  automationsBack = await request(address, {
    command: "click",
    targetId: "native-preview.automations.back",
  });
  assertSuccess(automationsBack, "return Automation from its workspace window");
  automationsBack = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === automationWindowCount &&
      observation.workspaceItemIds?.includes(automationWorkspaceItemId) &&
      observation.workspaceWindows?.every(
        (window) =>
          !window.panes?.some((pane) => pane.itemIds?.includes(automationWorkspaceItemId)),
      ) &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    10_000,
  );
  if (automationsBack.observation.page.startsWith("automations")) {
    throw new Error("main window remained on Automation after transferring its Workspace Item");
  }
  summary.checks.push(
    "Automation used one persistent application Workspace Item with stable resource identity, moved into a real auxiliary window, refreshed there and returned without duplicate ownership",
  );

  const initialProjectCount = automationsBack.observation.projectCount;
  const projectCreated = await request(address, {
    command: "click",
    targetId: "native-preview.projects.create",
  });
  assertSuccess(projectCreated, "create Native project");
  assertEqual(
    projectCreated.observation.projectCount,
    initialProjectCount + 1,
    "project count after create",
  );
  const createdProjectId = projectCreated.observation.selectedProject;
  if (!createdProjectId) throw new Error("created Native project was not selected");
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.project.name",
      text: "Agent Debug 原生项目",
    }),
    "edit Native project name",
  );
  const workspacePicked = await request(address, {
    command: "click",
    targetId: "native-preview.project.workspace.pick",
  });
  assertSuccess(workspacePicked, "pick Native project workspace directory");
  assertIncludes(
    workspacePicked.observation.visibleTargetIds,
    "native-preview.project.workspace.clear",
    "workspace clear target after directory selection",
  );
  const workspaceCleared = await request(address, {
    command: "click",
    targetId: "native-preview.project.workspace.clear",
  });
  assertSuccess(workspaceCleared, "clear Native project workspace directory");
  assertNotIncludes(
    workspaceCleared.observation.visibleTargetIds,
    "native-preview.project.workspace.clear",
    "workspace clear target after clearing directory",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.project.workspace.pick",
    }),
    "pick Native project workspace directory again",
  );
  const projectSaved = await request(address, {
    command: "click",
    targetId: "native-preview.project.save",
  });
  assertSuccess(projectSaved, "save Native project");
  assertEqual(
    projectSaved.observation.selectedProjectName,
    "Agent Debug 原生项目",
    "saved project name",
  );
  assertEqual(
    projectSaved.observation.selectedProjectWorkspace,
    previewWorkspace,
    "saved project workspace",
  );
  assertIncludes(
    projectSaved.observation.visibleTargetIds,
    "native-preview.project.move-up",
    "new project move-up target",
  );
  assertNotIncludes(
    projectSaved.observation.visibleTargetIds,
    "native-preview.project.move-down",
    "new project move-down target before reorder",
  );
  const projectMovedUp = await request(address, {
    command: "click",
    targetId: "native-preview.project.move-up",
  });
  assertSuccess(projectMovedUp, "move Native project up");
  assertIncludes(
    projectMovedUp.observation.visibleTargetIds,
    "native-preview.project.move-down",
    "project move-down target after moving up",
  );
  const projectMovedDown = await request(address, {
    command: "click",
    targetId: "native-preview.project.move-down",
  });
  assertSuccess(projectMovedDown, "move Native project down");
  assertNotIncludes(
    projectMovedDown.observation.visibleTargetIds,
    "native-preview.project.move-down",
    "project move-down target after restoring order",
  );
  const projectPinned = await request(address, {
    command: "click",
    targetId: "native-preview.project.pin",
  });
  assertSuccess(projectPinned, "pin Native project");
  assertEqual(projectPinned.observation.selectedProjectPinned, true, "project pinned state");

  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.tasks.create.title",
      text: "Agent Debug 原生任务",
    }),
    "enter Native task title",
  );
  const taskCreated = await request(address, {
    command: "click",
    targetId: "native-preview.tasks.create",
  });
  assertSuccess(taskCreated, "create Native task");
  const createdTaskId = taskCreated.observation.selectedTask;
  if (!createdTaskId) throw new Error("created Native task was not selected");
  assertEqual(taskCreated.observation.taskCount, 1, "task count after create");
  assertEqual(
    taskCreated.observation.selectedTaskTitle,
    "Agent Debug 原生任务",
    "created task title",
  );
  const initialTaskStatus = taskCreated.observation.selectedTaskStatus;
  const initialTaskPriority = taskCreated.observation.selectedTaskPriority;
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.task-session.task.title",
      text: "Agent Debug 原生任务已编辑",
    }),
    "edit Native task title",
  );
  const taskSaved = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.task.save",
  });
  assertSuccess(taskSaved, "save Native task");
  assertEqual(
    taskSaved.observation.selectedTaskTitle,
    "Agent Debug 原生任务已编辑",
    "saved task title",
  );
  const taskStatusChanged = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.task.status",
  });
  assertSuccess(taskStatusChanged, "advance Native task status");
  if (taskStatusChanged.observation.selectedTaskStatus === initialTaskStatus) {
    throw new Error("Native task status did not change");
  }
  const taskPriorityChanged = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.task.priority",
  });
  assertSuccess(taskPriorityChanged, "advance Native task priority");
  if (taskPriorityChanged.observation.selectedTaskPriority === initialTaskPriority) {
    throw new Error("Native task priority did not change");
  }
  const taskPinned = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.task.pin",
  });
  assertSuccess(taskPinned, "pin Native task");
  assertEqual(taskPinned.observation.selectedTaskPinned, true, "task pinned state");

  const taskList = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.back",
  });
  assertSuccess(taskList, "return to Native task list");
  const createdTaskTarget = `native-preview.task.${createdTaskId}`;
  assertTarget(taskList, createdTaskTarget);
  const taskSearch = await request(address, {
    command: "input",
    targetId: "native-preview.tasks.search",
    text: "已编辑",
  });
  assertSuccess(taskSearch, "search Native task");
  assertEqual(taskSearch.observation.visibleTaskCount, 1, "matching task count");
  assertTarget(taskSearch, createdTaskTarget);
  const taskSearchMiss = await request(address, {
    command: "input",
    targetId: "native-preview.tasks.search",
    text: "不存在的任务",
  });
  assertSuccess(taskSearchMiss, "search missing Native task");
  assertEqual(taskSearchMiss.observation.visibleTaskCount, 0, "missing task count");
  if (taskSearchMiss.observation.visibleTargetIds.includes(createdTaskTarget)) {
    throw new Error("filtered Native task remained exposed as a visible debug target");
  }
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.tasks.search",
      text: "",
    }),
    "clear Native task search",
  );
  assertSuccess(
    await request(address, { command: "click", targetId: createdTaskTarget }),
    "reopen Native task",
  );
  const taskArchived = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.task.archive",
  });
  assertSuccess(taskArchived, "archive Native task");
  assertEqual(taskArchived.observation.archivedTaskCount, 1, "archived task count");
  const restoreTaskTarget = `native-preview.task.${createdTaskId}.restore`;
  assertTarget(taskArchived, restoreTaskTarget);
  const taskRestored = await request(address, {
    command: "click",
    targetId: restoreTaskTarget,
  });
  assertSuccess(taskRestored, "restore Native task");
  assertEqual(taskRestored.observation.archivedTaskCount, 0, "restored task count");

  const projectRemovalRequested = await request(address, {
    command: "click",
    targetId: "native-preview.project.remove",
  });
  assertSuccess(projectRemovalRequested, "request Native project removal");
  assertEqual(
    projectRemovalRequested.observation.pendingProjectRemoval,
    createdProjectId,
    "pending project removal id",
  );
  assertEqual(
    projectRemovalRequested.observation.archivedProjectCount,
    0,
    "project remains active before removal confirmation",
  );
  assertTarget(projectRemovalRequested, "native-preview.project.remove.dialog");
  assertTarget(projectRemovalRequested, "native-preview.project.remove.confirm");
  assertTarget(projectRemovalRequested, "native-preview.project.remove.cancel");
  await recordRenderedWindow(
    child.pid,
    projectRemovalScreenshotPath,
    "project removal confirmation",
    "projectRemovalWindowBounds",
    "projectRemovalPngSize",
  );
  const projectRemovalCancelled = await request(address, {
    command: "click",
    targetId: "native-preview.project.remove.cancel",
  });
  assertSuccess(projectRemovalCancelled, "cancel Native project removal");
  assertEqual(
    projectRemovalCancelled.observation.pendingProjectRemoval,
    null,
    "cancelled project removal state",
  );
  assertEqual(
    projectRemovalCancelled.observation.archivedProjectCount,
    0,
    "cancelled project remains active",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.project.remove",
    }),
    "request Native project removal again",
  );
  const projectRemoved = await request(address, {
    command: "click",
    targetId: "native-preview.project.remove.confirm",
  });
  assertSuccess(projectRemoved, "confirm Native project removal");
  assertEqual(
    projectRemoved.observation.archivedProjectCount,
    1,
    "removed project archive count",
  );
  assertEqual(projectRemoved.observation.inboxSelected, true, "Inbox selected after removal");
  assertTarget(projectRemoved, `native-preview.task.${createdTaskId}`);
  if (!fs.existsSync(previewWorkspace)) {
    throw new Error("Native project removal deleted the workspace directory");
  }
  const projectRemovalPersisted = await waitForObservation(
    address,
    (observation) =>
      observation.archivedProjectCount === 1 &&
      observation.inboxSelected === true &&
      observation.workspacePersistedRevision === observation.workspaceRevision,
    30_000,
  );
  const projectRemovalProcessId = child.pid;
  ({ child, address } = await restartPreviewProcess(child, previewEnvironment));
  summary.restarts.push({
    kind: "project-removal",
    firstProcessId: projectRemovalProcessId,
    restoredProcessId: child.pid,
  });
  debugAddress = address;
  const restoredProjectRemoval = await waitForObservation(
    address,
    (observation) =>
      observation.archivedProjectCount === 1 &&
      observation.inboxSelected === true &&
      observation.pendingProjectRemoval === null &&
      observation.visibleTargetIds.includes(`native-preview.task.${createdTaskId}`),
    30_000,
  );
  assertEqual(
    restoredProjectRemoval.observation.workspaceRevision,
    projectRemovalPersisted.observation.workspaceRevision,
    "project removal workspace revision after restart",
  );
  const restoreProjectTarget = `native-preview.project.${createdProjectId}.restore`;
  assertTarget(restoredProjectRemoval, restoreProjectTarget);
  const projectRestored = await request(address, {
    command: "click",
    targetId: restoreProjectTarget,
  });
  assertSuccess(projectRestored, "restore Native project");
  assertEqual(projectRestored.observation.archivedProjectCount, 0, "restored project count");
  assertEqual(projectRestored.observation.taskCount, 0, "restored project keeps removed tasks in Inbox");
  assertNotIncludes(
    projectRestored.observation.visibleTargetIds,
    `native-preview.task.${createdTaskId}`,
    "restored project task list",
  );
  summary.checks.push(
    "Project removal required a real NanaUI confirmation, atomically moved active tasks and conversations to Inbox, preserved the workspace directory and topology across restart, and restore did not silently reattach tasks",
  );

  const project = await request(address, {
    command: "click",
    targetId: "native-preview.project.native-agent-debug-project",
  });
  assertSuccess(project, "open seeded project");
  assertTarget(project, "native-preview.task.native-agent-debug-task");
  assertTarget(project, "native-preview.project.roadmap");
  assertTarget(project, "native-preview.project.memory");
  assertTarget(project, "native-preview.project.coding-tools");

  const codingToolsOpened = await request(address, {
    command: "click",
    targetId: "native-preview.project.coding-tools",
  });
  assertSuccess(codingToolsOpened, "open Native coding tools");
  if (!codingToolsOpened.observation.codingToolsDockOpen) {
    throw new Error("Native coding tools did not become the active right Dock panel");
  }
  const codingToolsReady = await waitForObservation(
    address,
    (observation) =>
      !observation.codingToolsBusy &&
      observation.codingToolsSharedIdentity &&
      observation.codingToolsHasGit &&
      observation.codingToolsHasWorkspace,
    30_000,
  );
  assertTarget(codingToolsReady, "native-preview.coding-tools.query");
  assertTarget(codingToolsReady, "native-preview.coding-tools.close");
  assertTarget(codingToolsReady, "native-preview.coding-tools.open-workspace");
  assertTarget(codingToolsReady, "native-preview.coding-tools.open-terminal");
  if (!(codingToolsReady.observation.codingToolsPanelExtent >= 240)) {
    throw new Error("Native coding tools Dock did not expose its persisted extent");
  }
  const memoryCountBeforeCodingCapture = codingToolsReady.observation.memoryCount;
  const codingQuery = await request(address, {
    command: "input",
    targetId: "native-preview.coding-tools.query",
    text: "Native Agent Debug",
  });
  assertSuccess(codingQuery, "enter Native Code Index query");
  assertTarget(codingQuery, "native-preview.coding-tools.search");
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.coding-tools.search",
    }),
    "search Native shared Code Index",
  );
  const codingSearchReady = await waitForObservation(
    address,
    (observation) => !observation.codingToolsBusy && observation.codingToolsHasSearch,
    30_000,
  );
  const codingSearchHitTarget = codingSearchReady.observation.visibleTargetIds.find((target) =>
    target.startsWith("native-preview.coding-tools.search-hit."),
  );
  if (!codingSearchHitTarget) {
    throw new Error("Native Code Index search did not expose a structured result target");
  }
  const codingDocumentOpened = await request(address, {
    command: "click",
    targetId: codingSearchHitTarget,
  });
  assertSuccess(codingDocumentOpened, "open Native Code Index result in the document editor");
  if (
    !codingDocumentOpened.observation.visibleTargetIds.some((target) =>
      /^native-preview\.document-editor\.[0-9a-f]+\.definition$/.test(target),
    )
  ) {
    throw new Error("Native document editor did not expose its real definition action");
  }
  assertTarget(codingDocumentOpened, "native-preview.command-palette.open");
  const commandPaletteOpened = await request(address, {
    command: "click",
    targetId: "native-preview.command-palette.open",
  });
  assertSuccess(commandPaletteOpened, "open Native command palette");
  assertTarget(commandPaletteOpened, "native-preview.command-palette.input");
  assertTarget(
    commandPaletteOpened,
    "native-preview.command-palette.action.document.save",
  );
  const commandPaletteFiltered = await request(address, {
    command: "input",
    targetId: "native-preview.command-palette.input",
    text: "保存",
  });
  assertSuccess(commandPaletteFiltered, "filter Native document commands");
  assertTarget(
    commandPaletteFiltered,
    "native-preview.command-palette.action.document.save",
  );
  const commandPaletteSaved = await request(address, {
    command: "click",
    targetId: "native-preview.command-palette.action.document.save",
  });
  assertSuccess(commandPaletteSaved, "dispatch Native document save command");
  if (commandPaletteSaved.observation.visibleTargetIds.includes(
    "native-preview.command-palette.input",
  )) {
    throw new Error("Native command palette remained open after dispatch");
  }
  assertTarget(codingSearchReady, "native-preview.coding-tools.save-memory");
  const codingMemorySaved = await request(address, {
    command: "click",
    targetId: "native-preview.coding-tools.save-memory",
  });
  assertSuccess(codingMemorySaved, "save Native Code Index result to project Memory");
  assertEqual(
    codingMemorySaved.observation.memoryCount,
    memoryCountBeforeCodingCapture + 1,
    "Code Index project Memory count",
  );
  await recordRenderedWindow(
    child.pid,
    codingToolsScreenshotPath,
    "coding tools",
    "codingToolsWindowBounds",
    "codingToolsPngSize",
  );
  const codingToolsClosed = await request(address, {
    command: "click",
    targetId: "native-preview.coding-tools.close",
  });
  assertSuccess(codingToolsClosed, "close Native coding tools Dock");
  if (codingToolsClosed.observation.codingToolsDockOpen) {
    throw new Error("Native coding tools Dock remained open after close");
  }
  const codingToolsReopened = await request(address, {
    command: "click",
    targetId: "native-preview.project.coding-tools",
  });
  assertSuccess(codingToolsReopened, "reopen Native coding tools Dock");
  if (!codingToolsReopened.observation.codingToolsDockOpen) {
    throw new Error("Native coding tools Dock did not reopen");
  }
  const capturedMemoryId = codingMemorySaved.observation.selectedMemory;
  if (!capturedMemoryId) {
    throw new Error("Native Code Index Memory write did not select the persisted record");
  }
  const codingMemoryPage = await request(address, {
    command: "click",
    targetId: "native-preview.project.memory",
  });
  assertSuccess(codingMemoryPage, "open project Memory after Code Index capture");
  assertTarget(codingMemoryPage, `native-preview.memory.${capturedMemoryId}`);
  assertTarget(codingMemoryPage, "native-preview.memory.delete");
  const codingMemoryCleaned = await request(address, {
    command: "click",
    targetId: "native-preview.memory.delete",
  });
  assertSuccess(codingMemoryCleaned, "remove Code Index Agent Debug Memory record");
  assertEqual(
    codingMemoryCleaned.observation.memoryCount,
    memoryCountBeforeCodingCapture,
    "Code Index Memory cleanup count",
  );
  summary.checks.push(
    "Workspace tools used a persistent right Dock, shared Native AgentKit services, opened a Code Index hit in the syntax-highlighted document editor, dispatched its context-scoped save action through the NanaUI command palette, exposed a real definition action, and persisted project Memory",
  );

  const architectureOpened = await request(address, {
    command: "click",
    targetId: "native-preview.project.architecture",
  });
  assertSuccess(architectureOpened, "open Native architecture");
  if (!architectureOpened.observation.page.startsWith("architecture/")) {
    throw new Error("Native architecture did not become the active project surface");
  }
  assertEqual(architectureOpened.observation.architectureVersion, 2, "architecture version");
  assertEqual(architectureOpened.observation.architectureNodeCount, 3, "architecture node count");
  assertEqual(architectureOpened.observation.architectureEdgeCount, 1, "architecture edge count");
  assertEqual(
    architectureOpened.observation.architectureQuarantineCount,
    1,
    "architecture quarantine count after restart",
  );
  const architectureWorkspaceItemId = assertProjectWorkspaceItem(
    architectureOpened,
    "architecture",
  );
  assertEqual(
    architectureOpened.observation.architectureHistoryCount,
    2,
    "architecture history count",
  );
  assertTarget(architectureOpened, "graph.lilia-architecture.canvas");
  assertTarget(architectureOpened, "graph.lilia-architecture.node.native-ui");
  assertTarget(
    architectureOpened,
    "graph.lilia-architecture.node.agent-architecture-approval",
  );
  const architectureSelected = await request(address, {
    command: "click",
    targetId: "graph.lilia-architecture.node.native-ui",
  });
  assertSuccess(architectureSelected, "select Native architecture node");
  assertEqual(
    architectureSelected.observation.architectureSelectedNode,
    "native-ui",
    "selected architecture node",
  );
  await recordRenderedWindow(
    child.pid,
    architectureScreenshotPath,
    "architecture",
    "architectureWindowBounds",
    "architecturePngSize",
  );
  assertTarget(architectureSelected, "native-preview.architecture.rollback");
  const architectureRolledBack = await request(address, {
    command: "click",
    targetId: "native-preview.architecture.rollback",
  });
  assertSuccess(architectureRolledBack, "rollback Native architecture");
  assertEqual(
    architectureRolledBack.observation.architectureVersion,
    3,
    "rolled back architecture version",
  );
  assertEqual(
    architectureRolledBack.observation.architectureNodeCount,
    2,
    "rolled back architecture node count",
  );
  assertEqual(
    architectureRolledBack.observation.architectureHistoryCount,
    3,
    "rolled back architecture history count",
  );
  if (
    architectureRolledBack.observation.visibleTargetIds.includes(
      "native-preview.architecture.rollback",
    )
  ) {
    throw new Error("Native architecture exposed a second rollback after history was consumed");
  }
  summary.checks.push(
    "Architecture snapshot, WGPU graph selection, version history and rollback used the shared idempotent service",
  );

  const roadmapOpened = await request(address, {
    command: "click",
    targetId: "native-preview.project.roadmap",
  });
  assertSuccess(roadmapOpened, "open Native roadmap");
  if (!roadmapOpened.observation.page.startsWith("roadmap/")) {
    throw new Error("Native roadmap did not become the active project surface");
  }
  const roadmapWorkspaceItemId = assertProjectWorkspaceItem(roadmapOpened, "roadmap");
  const milestoneCreated = await request(address, {
    command: "click",
    targetId: "native-preview.roadmap.create",
  });
  assertSuccess(milestoneCreated, "create Native milestone");
  assertEqual(milestoneCreated.observation.milestoneCount, 1, "milestone count");
  const milestoneId = milestoneCreated.observation.selectedMilestone;
  if (!milestoneId) throw new Error("created milestone was not selected");
  for (const [targetId, text, label] of [
    ["native-preview.roadmap.milestone.title", "原生发布门禁", "milestone title"],
    ["native-preview.roadmap.milestone.description", "完成 Windows 原生验收", "milestone description"],
    ["native-preview.roadmap.milestone.due-date", "2028-02-29", "milestone due date"],
  ]) {
    const changed = await request(address, { command: "input", targetId, text });
    assertSuccess(changed, `edit ${label}`);
  }
  const milestoneSaved = await request(address, {
    command: "click",
    targetId: "native-preview.roadmap.milestone.save",
  });
  assertSuccess(milestoneSaved, "save Native milestone");
  assertEqual(
    milestoneSaved.observation.selectedMilestoneTitle,
    "原生发布门禁",
    "saved milestone title",
  );
  assertEqual(
    milestoneSaved.observation.selectedMilestoneDescription,
    "完成 Windows 原生验收",
    "saved milestone description",
  );
  assertEqual(
    milestoneSaved.observation.selectedMilestoneDueDate,
    "2028-02-29",
    "saved milestone due date",
  );
  assertEqual(
    milestoneSaved.observation.selectedMilestoneStatus,
    "upcoming",
    "saved milestone status",
  );
  const invalidMilestoneDate = await request(address, {
    command: "input",
    targetId: "native-preview.roadmap.milestone.due-date",
    text: "2027-02-29",
  });
  assertSuccess(invalidMilestoneDate, "enter invalid Native milestone due date");
  const invalidMilestoneSave = await request(address, {
    command: "click",
    targetId: "native-preview.roadmap.milestone.save",
  });
  assertSuccess(invalidMilestoneSave, "reject invalid Native milestone due date");
  assertEqual(
    invalidMilestoneSave.observation.roadmapError,
    "截止日期不是有效的日历日期。",
    "invalid milestone due date error",
  );
  assertEqual(
    invalidMilestoneSave.observation.selectedMilestoneDueDate,
    "2028-02-29",
    "invalid milestone due date did not persist",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.roadmap.milestone.due-date",
      text: "",
    }),
    "clear Native milestone due date draft",
  );
  const clearedMilestoneDate = await request(address, {
    command: "click",
    targetId: "native-preview.roadmap.milestone.save",
  });
  assertSuccess(clearedMilestoneDate, "clear Native milestone due date");
  assertEqual(
    clearedMilestoneDate.observation.selectedMilestoneDueDate,
    null,
    "cleared milestone due date",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.roadmap.milestone.due-date",
      text: "2028-02-29",
    }),
    "restore Native milestone due date draft",
  );
  const restoredMilestoneDate = await request(address, {
    command: "click",
    targetId: "native-preview.roadmap.milestone.save",
  });
  assertSuccess(restoredMilestoneDate, "restore Native milestone due date");
  assertEqual(
    restoredMilestoneDate.observation.selectedMilestoneDueDate,
    "2028-02-29",
    "restored milestone due date",
  );
  const milestoneTaskTarget = `native-preview.roadmap.milestone.${milestoneId}.task.native-agent-debug-task`;
  assertTarget(restoredMilestoneDate, milestoneTaskTarget);
  const milestoneTaskLinked = await request(address, {
    command: "click",
    targetId: milestoneTaskTarget,
  });
  assertSuccess(milestoneTaskLinked, "link task to Native milestone");
  assertEqual(
    milestoneTaskLinked.observation.selectedMilestoneTaskCount,
    1,
    "linked milestone task count",
  );
  const milestoneStatusAdvanced = await request(address, {
    command: "click",
    targetId: "native-preview.roadmap.milestone.status",
  });
  assertSuccess(milestoneStatusAdvanced, "advance Native milestone status");
  assertEqual(
    milestoneStatusAdvanced.observation.selectedMilestoneStatus,
    "in-progress",
    "advanced milestone status",
  );
  await recordRenderedWindow(
    child.pid,
    roadmapScreenshotPath,
    "roadmap",
    "roadmapWindowBounds",
    "roadmapPngSize",
  );
  summary.checks.push(
    "Roadmap CRUD, description, due-date validation/clear/restore, status and task linking used shared SQLite state",
  );

  const roadmapWindowCount = milestoneStatusAdvanced.observation.taskPopupWindowCount;
  const moveRoadmapToWindowTarget =
    `native-preview.workspace.tab.${roadmapWorkspaceItemId}.move-to-new-window`;
  assertTarget(milestoneStatusAdvanced, moveRoadmapToWindowTarget);
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: moveRoadmapToWindowTarget,
    }),
    "move Roadmap Workspace Item to a new window",
  );
  const roadmapWindow = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === roadmapWindowCount + 1 &&
      !observation.workspaceItemIds?.includes(roadmapWorkspaceItemId) &&
      observation.workspaceWindows?.some(
        (window) =>
          window.activeItemId === roadmapWorkspaceItemId &&
          window.panes?.some((pane) => pane.itemIds?.includes(roadmapWorkspaceItemId)),
      ) &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    10_000,
  );
  const roadmapWorkspaceWindow = roadmapWindow.observation.workspaceWindows.find(
    (window) => window.activeItemId === roadmapWorkspaceItemId,
  );
  if (!roadmapWorkspaceWindow) {
    throw new Error("Roadmap auxiliary Workspace window was not observable");
  }
  const roadmapWindowTarget = (targetId) =>
    `native-preview.workspace-window.${roadmapWorkspaceWindow.windowId}.project-action.${targetId}`;
  const roadmapDescriptionTarget = roadmapWindowTarget(
    "native-preview.roadmap.milestone.description",
  );
  const roadmapSaveTarget = roadmapWindowTarget("native-preview.roadmap.milestone.save");
  assertTarget(roadmapWindow, roadmapDescriptionTarget);
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: roadmapDescriptionTarget,
      text: "完成 Windows 原生验收与跨窗口",
    }),
    "edit Roadmap inside its workspace window",
  );
  assertSuccess(
    await request(address, { command: "click", targetId: roadmapSaveTarget }),
    "save Roadmap inside its workspace window",
  );
  const roadmapWindowProcessId = child.pid;
  ({ child, address } = await restartPreviewProcess(child, previewEnvironment));
  summary.restarts.push({
    kind: "project-workspace-window",
    firstProcessId: roadmapWindowProcessId,
    restoredProcessId: child.pid,
  });
  debugAddress = address;
  const restoredRoadmapWindow = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === roadmapWindowCount + 1 &&
      observation.workspaceWindows?.some(
        (window) =>
          window.windowId === roadmapWorkspaceWindow.windowId &&
          window.activeItemId === roadmapWorkspaceItemId &&
          window.panes?.some((pane) => pane.itemIds?.includes(roadmapWorkspaceItemId)),
      ) &&
      observation.visibleTargetIds.includes(roadmapDescriptionTarget) &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    15_000,
  );
  const moveRoadmapToMainTarget = (
    restoredRoadmapWindow
  ).observation.visibleTargetIds.find(
    (target) =>
      target.startsWith(`native-preview.task-popup.${roadmapWorkspaceWindow.windowId}.`) &&
      target.includes(`.tab.${roadmapWorkspaceItemId}.drag-to-main-pane.`),
  );
  if (!moveRoadmapToMainTarget) {
    throw new Error("Roadmap workspace window did not expose its move-to-main target");
  }
  assertSuccess(
    await request(address, { command: "click", targetId: moveRoadmapToMainTarget }),
    "move Roadmap Workspace Item back to the main window",
  );
  const roadmapReturned = await waitForObservation(
    address,
    (observation) =>
      observation.taskPopupWindowCount === roadmapWindowCount &&
      observation.workspaceItemIds?.includes(roadmapWorkspaceItemId) &&
      observation.workspaceTopologyPersistedRevision === observation.workspaceTopologyRevision,
    10_000,
  );
  assertTarget(roadmapReturned, `native-preview.workspace.tab.${roadmapWorkspaceItemId}`);
  const roadmapRestoredFromWindow = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.tab.${roadmapWorkspaceItemId}`,
  });
  assertSuccess(roadmapRestoredFromWindow, "restore Roadmap after cross-window editing");
  assertEqual(
    roadmapRestoredFromWindow.observation.selectedMilestoneDescription,
    "完成 Windows 原生验收与跨窗口",
    "Roadmap cross-window persisted description",
  );
  summary.checks.push(
    "Roadmap used an item-owned editor state in a real auxiliary window, accepted stable scoped input and save targets, restored the same window/item after process restart, persisted through the shared service and returned atomically",
  );

  const memoryOpened = await request(address, {
    command: "click",
    targetId: "native-preview.project.memory",
  });
  assertSuccess(memoryOpened, "open Native Memory");
  if (!memoryOpened.observation.page.startsWith("memory/")) {
    throw new Error("Native Memory did not become the active project surface");
  }
  const memoryWorkspaceItemId = assertProjectWorkspaceItem(memoryOpened, "memory");
  assertTarget(memoryOpened, "native-preview.memory.settings.enabled");
  assertTarget(memoryOpened, "native-preview.memory.settings.baseline");
  assertTarget(memoryOpened, "native-preview.memory.settings.cooldown");
  assertTarget(memoryOpened, "native-preview.memory.settings.cooldown.input");
  assertTarget(memoryOpened, "native-preview.memory.settings.cooldown.save");
  const memoryGlobalDisabled = await request(address, {
    command: "click",
    targetId: "native-preview.memory.settings.enabled",
  });
  assertSuccess(memoryGlobalDisabled, "disable global Native Memory");
  assertEqual(memoryGlobalDisabled.observation.memoryGlobalEnabled, false, "global Memory state");
  const memoryGlobalEnabled = await request(address, {
    command: "click",
    targetId: "native-preview.memory.settings.enabled",
  });
  assertSuccess(memoryGlobalEnabled, "restore global Native Memory");
  assertEqual(memoryGlobalEnabled.observation.memoryGlobalEnabled, true, "global Memory state");
  const memoryBaselineDisabled = await request(address, {
    command: "click",
    targetId: "native-preview.memory.settings.baseline",
  });
  assertSuccess(memoryBaselineDisabled, "disable Native Memory baseline injection");
  assertEqual(
    memoryBaselineDisabled.observation.memoryBaselineEnabled,
    false,
    "baseline Memory state",
  );
  const memoryCooldown = await request(address, {
    command: "click",
    targetId: "native-preview.memory.settings.cooldown",
  });
  assertSuccess(memoryCooldown, "cycle Native Memory cooldown");
  assertEqual(memoryCooldown.observation.memoryCooldownTurns, 10, "Memory cooldown");
  const memoryCooldownDraft = await request(address, {
    command: "input",
    targetId: "native-preview.memory.settings.cooldown.input",
    text: "37",
  });
  assertSuccess(memoryCooldownDraft, "input exact Native Memory cooldown");
  const memoryCooldownSaved = await request(address, {
    command: "click",
    targetId: "native-preview.memory.settings.cooldown.save",
  });
  assertSuccess(memoryCooldownSaved, "save exact Native Memory cooldown");
  assertEqual(memoryCooldownSaved.observation.memoryCooldownTurns, 37, "exact Memory cooldown");
  assertSuccess(
    await request(address, { command: "click", targetId: "native-preview.memory.new" }),
    "start Native Memory draft",
  );
  const memoryScopeChanged = await request(address, {
    command: "click",
    targetId: "native-preview.memory.scope",
  });
  assertSuccess(memoryScopeChanged, "switch Native Memory draft to user scope");
  assertEqual(memoryScopeChanged.observation.memoryScope, "user", "Memory draft scope");
  const multilineMemoryBody = [
    "所有可见操作必须接入真实应用服务。",
    "长文本在 NanaUI 原生多行编辑器中保留换行。",
    "主窗口与辅助 Workspace Window 共享同一持久化合同。",
  ].join("\n");
  for (const [targetId, text, label] of [
    ["native-preview.memory.title", "原生迁移约束", "Memory title"],
    ["native-preview.memory.body", multilineMemoryBody, "multiline Memory body"],
    ["native-preview.memory.tags", "native，migration", "Memory tags"],
  ]) {
    const changed = await request(address, { command: "input", targetId, text });
    assertSuccess(changed, `edit ${label}`);
  }
  const memorySaved = await request(address, {
    command: "click",
    targetId: "native-preview.memory.save",
  });
  assertSuccess(memorySaved, "save Native Memory");
  assertEqual(memorySaved.observation.memoryCount, 1, "Memory count");
  assertEqual(memorySaved.observation.memoryScope, "user", "saved Memory scope");
  assertEqual(
    memorySaved.observation.selectedMemoryBodyLineCount,
    3,
    "persisted Memory body line count",
  );
  assertEqual(
    memorySaved.observation.memoryDraftBodyLineCount,
    3,
    "visible Memory textarea line count",
  );
  assertEqual(
    memorySaved.observation.selectedMemoryTitle,
    "原生迁移约束",
    "saved Memory title",
  );
  const memoryDisabled = await request(address, {
    command: "click",
    targetId: "native-preview.memory.toggle",
  });
  assertSuccess(memoryDisabled, "disable Native Memory");
  assertEqual(memoryDisabled.observation.memoryEnabled, false, "Memory enabled state");
  await recordRenderedWindow(
    child.pid,
    memoryScreenshotPath,
    "memory",
    "memoryWindowBounds",
    "memoryPngSize",
  );
  const architectureTabRestored = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.tab.${architectureWorkspaceItemId}`,
  });
  assertSuccess(architectureTabRestored, "reactivate Native architecture workspace item");
  if (!architectureTabRestored.observation.page.startsWith("architecture/")) {
    throw new Error("Native architecture workspace item did not restore its content route");
  }
  assertProjectWorkspaceItem(architectureTabRestored, "architecture");
  if (!architectureTabRestored.observation.workspaceItemIds.includes(roadmapWorkspaceItemId)) {
    throw new Error("Native roadmap workspace item was lost while switching project surfaces");
  }
  const memoryTabRestored = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.tab.${memoryWorkspaceItemId}`,
  });
  assertSuccess(memoryTabRestored, "reactivate Native Memory workspace item");
  if (!memoryTabRestored.observation.page.startsWith("memory/")) {
    throw new Error("Native Memory workspace item did not restore its content route");
  }
  assertEqual(
    memoryTabRestored.observation.selectedMemoryBodyLineCount,
    3,
    "restored multiline Memory body",
  );
  summary.checks.push(
    "Memory CRUD, user/project scope, tags, enabled state and multiline body used shared storage and the native textarea",
  );
  summary.checks.push(
    "Roadmap, Memory and Architecture used persistent project Workspace Items with stable resource identities and real tab routing",
  );

  const projectTasks = await request(address, {
    command: "click",
    targetId: "native-preview.project.tasks",
  });
  assertSuccess(projectTasks, "return to Native project tasks");
  assertTarget(projectTasks, "native-preview.task.native-agent-debug-task");
  const task = await request(address, {
    command: "click",
    targetId: "native-preview.task.native-agent-debug-task",
  });
  assertSuccess(task, "open seeded task");
  assertEqual(task.observation.page, "tasks/native-agent-debug-task", "task page");
  assertTarget(task, "native-preview.task-session.composer.input");
  assertTarget(task, "native-preview.task-session.composer.plan-mode");
  assertTarget(task, "native-preview.task-session.composer.goal-mode");
  assertTarget(task, "native-preview.task-session.composer.permission");
  assertTarget(task, "native-preview.task-session.goal.input");
  assertTarget(task, "native-preview.task-session.todo.input");
  assertTarget(task, "native-preview.task-session.worktree.create");
  assertTarget(task, "native-preview.task-session.memory.toggle");
  const taskMemoryDisabled = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.memory.toggle",
  });
  assertSuccess(taskMemoryDisabled, "disable task Memory injection");
  assertEqual(taskMemoryDisabled.observation.taskMemoryEnabled, false, "task Memory state");
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.memory.reset-cooldown",
    }),
    "reset task Memory cooldown",
  );

  const worktreeStarted = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.worktree.create",
  });
  assertSuccess(worktreeStarted, "create task worktree");
  assertEqual(worktreeStarted.observation.worktreeBusy, true, "worktree operation state");
  const worktreeCreated = await waitForObservation(
    address,
    (observation) => Boolean(observation.worktreePath) && !observation.worktreeBusy,
    30_000,
  );
  if (!worktreeCreated.observation.worktreeBranch?.startsWith("lilia/")) {
    throw new Error("Native task worktree did not use the managed branch namespace");
  }
  assertTarget(worktreeCreated, "native-preview.task-session.worktree.open");
  assertTarget(worktreeCreated, "native-preview.task-session.worktree.clear");
  assertTarget(worktreeCreated, "native-preview.task-session.worktree.request-merge");
  summary.worktreePath = worktreeCreated.observation.worktreePath;
  summary.checks.push("Worktree creation used real Git and persisted the task binding");

  const initialEditableTodoCount = worktreeCreated.observation.editableTodoCount;
  const initialCompletedTodoCount = worktreeCreated.observation.completedTodoCount;
  const initialTodoTargets = new Set(worktreeCreated.observation.visibleTargetIds);
  const todoInput = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.todo.input",
    text: "验证原生 Todo",
  });
  assertSuccess(todoInput, "enter Native Todo");
  assertTarget(todoInput, "native-preview.task-session.todo.save");
  const todoCreated = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.todo.save",
  });
  assertSuccess(todoCreated, "create Native Todo");
  assertEqual(
    todoCreated.observation.editableTodoCount,
    initialEditableTodoCount + 1,
    "editable Todo count increment",
  );
  if (!todoCreated.observation.todoTitles.includes("验证原生 Todo")) {
    throw new Error("created Native Todo was not returned by the application snapshot");
  }
  const createdTodoTargets = todoCreated.observation.visibleTargetIds.filter(
    (target) => !initialTodoTargets.has(target),
  );
  const todoToggleTarget = createdTodoTargets.find(
    (target) => target.startsWith("native-preview.task-session.todo.") && target.endsWith(".toggle"),
  );
  const todoPriorityTarget = createdTodoTargets.find(
    (target) => target.startsWith("native-preview.task-session.todo.") && target.endsWith(".priority"),
  );
  const todoEditTarget = createdTodoTargets.find(
    (target) => target.startsWith("native-preview.task-session.todo.") && target.endsWith(".edit"),
  );
  if (!todoToggleTarget || !todoPriorityTarget || !todoEditTarget) {
    throw new Error("created Native Todo did not expose its functional targets");
  }
  const todoCompleted = await request(address, {
    command: "click",
    targetId: todoToggleTarget,
  });
  assertSuccess(todoCompleted, "complete Native Todo");
  assertEqual(
    todoCompleted.observation.completedTodoCount,
    initialCompletedTodoCount + 1,
    "completed Todo count increment",
  );
  const todoPrioritized = await request(address, {
    command: "click",
    targetId: todoPriorityTarget,
  });
  assertSuccess(todoPrioritized, "change Native Todo priority");
  if (!todoPrioritized.observation.todoPriorities.includes("low")) {
    throw new Error("Native Todo priority mutation did not reach persisted state");
  }
  const todoEditing = await request(address, {
    command: "click",
    targetId: todoEditTarget,
  });
  assertSuccess(todoEditing, "edit Native Todo");
  assertEqual(todoEditing.observation.todoEditing, true, "Todo editing state");
  await request(address, {
    command: "input",
    targetId: "native-preview.task-session.todo.input",
    text: "验证原生 Todo 已修改",
  });
  const todoSaved = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.todo.save",
  });
  assertSuccess(todoSaved, "save edited Native Todo");
  assertEqual(todoSaved.observation.todoEditing, false, "saved Todo editing state");
  if (!todoSaved.observation.todoTitles.includes("验证原生 Todo 已修改")) {
    throw new Error("edited Native Todo was not returned by the application snapshot");
  }
  summary.checks.push("Todo create, complete, priority and edit used the real isolated SQLite state");

  const goalInput = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.goal.input",
    text: "完成 Native 功能对齐",
  });
  assertSuccess(goalInput, "enter Native Goal");
  assertTarget(goalInput, "native-preview.task-session.goal.set");
  const goalSet = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.goal.set",
  });
  assertSuccess(goalSet, "set Native Goal");
  assertEqual(goalSet.observation.goalObjective, "完成 Native 功能对齐", "Goal objective");
  assertEqual(goalSet.observation.goalStatus, "active", "initial Goal status");
  assertTarget(goalSet, "native-preview.task-session.goal.refresh");
  assertTarget(goalSet, "native-preview.task-session.goal.clear");
  const goalRefreshed = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.goal.refresh",
  });
  assertSuccess(goalRefreshed, "refresh Native Goal");
  const expectedGoalStatus =
    goalSet.observation.todoCount > 0 &&
    goalSet.observation.todoCount === goalSet.observation.completedTodoCount
      ? "complete"
      : "active";
  assertEqual(goalRefreshed.observation.goalStatus, expectedGoalStatus, "refreshed Goal status");
  summary.checks.push("Goal set and refresh derived status from the real task timeline and Todo state");

  const planEnabled = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.plan-mode",
  });
  assertSuccess(planEnabled, "enable composer plan mode");
  assertEqual(planEnabled.observation.composerPlanMode, true, "plan mode state");
  const goalEnabled = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.goal-mode",
  });
  assertSuccess(goalEnabled, "enable composer goal mode");
  assertEqual(goalEnabled.observation.composerGoalMode, true, "goal mode state");
  const readonly = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.permission",
  });
  assertSuccess(readonly, "cycle composer permission");
  assertEqual(readonly.observation.composerPermission, "readonly", "permission state");
  await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.plan-mode",
  });
  await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.goal-mode",
  });
  await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.permission",
  });
  const permissionRestored = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.permission",
  });
  assertEqual(permissionRestored.observation.composerPermission, "ask", "restored permission state");
  summary.checks.push("composer plan, goal and permission controls changed real request state");

  const composerInput = await request(address, {
    command: "input",
    targetId: "native-preview.task-session.composer.input",
    text: "验证原生消息发送",
  });
  assertSuccess(composerInput, "enter composer text");
  assertTarget(composerInput, "native-preview.task-session.composer.send");
  const sent = await request(address, {
    command: "click",
    targetId: "native-preview.task-session.composer.send",
  });
  assertSuccess(sent, "send Native Agent turn");
  assertTarget(sent, "native-preview.task-session.composer.interrupt");
  const completed = await waitForObservation(
    address,
    (observation) =>
      observation.page === "tasks/native-agent-debug-task" &&
      observation.visibleTargetIds.some((target) =>
        target.startsWith("native-preview.task-session.timeline."),
      ) &&
      observation.turnState === "completed" &&
      observation.taskActionError === null &&
      !observation.visibleTargetIds.includes("native-preview.task-session.composer.interrupt"),
    30_000,
  );
  assertTarget(completed, "native-preview.task-session.composer.input");
  if (!summary.modelFixtureRequests?.some((request) => request.worktreePathSeen)) {
    throw new Error("Native Agent turn did not receive the bound worktree path");
  }
  summary.checks.push(
    "composer input and send used the bound worktree and reached the real Native Agent timeline with Markdown, math and Mermaid content",
  );

  const quotaSettingsOpen = await request(address, {
    command: "click",
    targetId: "native-preview.settings.open",
  });
  assertSuccess(quotaSettingsOpen, "open settings for Native quota");
  const quotaOpened = await request(address, {
    command: "click",
    targetId: "native-preview.settings.quota",
  });
  assertSuccess(quotaOpened, "open Native quota settings");
  assertEqual(quotaOpened.observation.page, "settings/quota", "quota settings page");
  const quotaReady = await waitForObservation(
    address,
    (observation) =>
      observation.page === "settings/quota" &&
      !observation.quotaBusy &&
      observation.quotaError === null &&
      observation.quotaRecordCount >= 1 &&
      observation.quotaTotalTokens > 0,
    30_000,
  );
  assertEqual(quotaReady.observation.quotaKnownCost, false, "unknown provider cost state");
  assertTarget(quotaReady, "native-preview.settings.quota.refresh");
  assertTarget(quotaReady, "native-preview.settings.quota.days");
  assertTarget(quotaReady, "native-preview.settings.quota.backend");
  await recordRenderedWindow(
    child.pid,
    quotaScreenshotPath,
    "quota",
    "quotaWindowBounds",
    "quotaPngSize",
  );

  const previousQuotaDays = quotaReady.observation.quotaDays;
  const quotaDaysChanged = await request(address, {
    command: "click",
    targetId: "native-preview.settings.quota.days",
  });
  assertSuccess(quotaDaysChanged, "cycle Native quota range");
  await waitForObservation(
    address,
    (observation) => !observation.quotaBusy && observation.quotaDays !== previousQuotaDays,
    30_000,
  );
  const quotaBackendChanged = await request(address, {
    command: "click",
    targetId: "native-preview.settings.quota.backend",
  });
  assertSuccess(quotaBackendChanged, "cycle Native quota backend");
  await waitForObservation(
    address,
    (observation) =>
      !observation.quotaBusy && observation.quotaBackend === "native-agentkit",
    30_000,
  );
  const quotaClosed = await request(address, {
    command: "click",
    targetId: "native-preview.settings.back",
  });
  assertSuccess(quotaClosed, "close Native quota settings");
  assertEqual(quotaClosed.observation.page, "tasks/native-agent-debug-task", "return from quota");
  summary.checks.push(
    "Native quota aggregated Product Core usage, rendered a real Canvas chart and preserved unknown remote cost semantics",
  );

  const extensionsSettingsOpen = await request(address, {
    command: "click",
    targetId: "native-preview.settings.open",
  });
  assertSuccess(extensionsSettingsOpen, "open settings for Native extensions");
  const extensionsOpened = await request(address, {
    command: "click",
    targetId: "native-preview.settings.extensions",
  });
  assertSuccess(extensionsOpened, "open Native extensions settings");
  assertEqual(
    extensionsOpened.observation.page,
    "settings/extensions",
    "extensions settings page",
  );
  const extensionsReady = await waitForObservation(
    address,
    (observation) =>
      observation.page === "settings/extensions" &&
      !observation.extensionsBusy &&
      observation.extensionsError === null &&
      observation.extensionsSharedIdentity &&
      observation.extensionsRuntimeServiceCount === 6 &&
      observation.extensionsSkillCount === 1 &&
      observation.extensionsMcpCount === 1,
    30_000,
  );
  assertTarget(extensionsReady, "native-preview.settings.extensions.refresh");
  assertTarget(extensionsReady, "native-preview.settings.extensions.activate-mcp");
  assertTarget(extensionsReady, "native-preview.settings.extensions.mcp.add");
  assertTarget(extensionsReady, "native-preview.settings.extensions.skill.id");
  assertTarget(extensionsReady, "native-preview.settings.extensions.skill.description");
  const skillsRevision = extensionsReady.observation.extensionsSkillsRegistryRevision;
  const baselineSkillCount = extensionsReady.observation.extensionsSkillCount;
  const baselineEnabledSkillCount = extensionsReady.observation.extensionsEnabledSkillCount;
  const baselineRuntimeSkillCount = extensionsReady.observation.extensionsRuntimeSkillCount;
  const managedSkillId = "native-debug-managed";
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.settings.extensions.skill.id",
      text: managedSkillId,
    }),
    "input Native Skill ID",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.settings.extensions.skill.description",
      text: "Review the current native change set.",
    }),
    "input Native Skill instructions",
  );
  const skillCreate = await request(address, {
    command: "click",
    targetId: "native-preview.settings.extensions.skill.create",
  });
  assertSuccess(skillCreate, "create Native managed Skill");
  const skillCreated = await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsSkillsRegistryRevision === skillsRevision + 1 &&
      observation.extensionsSkillCount === baselineSkillCount + 1 &&
      observation.extensionsEditableSkillCount === 1 &&
      observation.extensionsEnabledSkillCount === baselineEnabledSkillCount + 1 &&
      observation.extensionsRuntimeSkillCount === baselineRuntimeSkillCount + 1,
    30_000,
  );
  assertTarget(
    skillCreated,
    `native-preview.settings.extensions.skill.${managedSkillId}.toggle`,
  );
  assertEqual(
    fs.existsSync(path.join(previewHome, "skills", managedSkillId, "SKILL.md")),
    true,
    "managed Skill document exists",
  );

  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.skill.${managedSkillId}.toggle`,
    }),
    "disable Native managed Skill",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsSkillsRegistryRevision === skillsRevision + 2 &&
      observation.extensionsEnabledSkillCount === baselineEnabledSkillCount &&
      observation.extensionsRuntimeSkillCount === baselineRuntimeSkillCount,
    30_000,
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.skill.${managedSkillId}.toggle`,
    }),
    "re-enable Native managed Skill",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsSkillsRegistryRevision === skillsRevision + 3 &&
      observation.extensionsRuntimeSkillCount === baselineRuntimeSkillCount + 1,
    30_000,
  );
  const requestedSkillDelete = await request(address, {
    command: "click",
    targetId: `native-preview.settings.extensions.skill.${managedSkillId}.delete`,
  });
  assertSuccess(requestedSkillDelete, "request Native managed Skill deletion");
  assertEqual(
    requestedSkillDelete.observation.extensionsSkillDeleteConfirmation,
    managedSkillId,
    "managed Skill delete confirmation",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.skill.${managedSkillId}.delete.cancel`,
    }),
    "cancel Native managed Skill deletion",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.skill.${managedSkillId}.delete`,
    }),
    "request Native managed Skill deletion again",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.skill.${managedSkillId}.delete.confirm`,
    }),
    "confirm Native managed Skill deletion",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsSkillsRegistryRevision === skillsRevision + 4 &&
      observation.extensionsSkillCount === baselineSkillCount &&
      observation.extensionsEditableSkillCount === 0 &&
      observation.extensionsRuntimeSkillCount === baselineRuntimeSkillCount &&
      observation.extensionsSkillDeleteConfirmation === null,
    30_000,
  );
  assertEqual(
    fs.existsSync(path.join(previewHome, "skills", managedSkillId)),
    false,
    "managed Skill directory removed",
  );
  const pluginsRevision = extensionsReady.observation.extensionsPluginsRegistryRevision;
  const baselinePluginCount = extensionsReady.observation.extensionsPluginCount;
  const baselineEnabledPluginCount = extensionsReady.observation.extensionsEnabledPluginCount;
  const baselineRuntimePluginCount = extensionsReady.observation.extensionsRuntimePluginCount;
  const baselineMcpCount = extensionsReady.observation.extensionsMcpCount;
  const baselineActiveMcpCount = extensionsReady.observation.extensionsActiveMcpCount;
  const baselineMcpToolCount = extensionsReady.observation.extensionsMcpToolCount;
  const baselineMcpResourceCount = extensionsReady.observation.extensionsMcpResourceCount;
  const baselineMcpPromptCount = extensionsReady.observation.extensionsMcpPromptCount;
  const baselineMcpCredentialCount = extensionsReady.observation.extensionsMcpCredentialCount;
  const baselineConfiguredMcpCredentialCount =
    extensionsReady.observation.extensionsMcpConfiguredCredentialCount;
  const baselineMcpActivationErrorCount =
    extensionsReady.observation.extensionsActivationErrorCount;
  assertTarget(extensionsReady, "native-preview.settings.extensions.plugin.source");
  assertTarget(extensionsReady, "native-preview.settings.extensions.plugin.pick");
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.settings.extensions.plugin.source",
      text: pluginFixtureRoot,
    }),
    "input Native Plugin source",
  );
  const pluginInstall = await request(address, {
    command: "click",
    targetId: "native-preview.settings.extensions.plugin.install",
  });
  assertSuccess(pluginInstall, "install Native Plugin package");
  const pluginInstalled = await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsPluginsRegistryRevision === pluginsRevision + 1 &&
      observation.extensionsPluginCount === baselinePluginCount + 1 &&
      observation.extensionsEnabledPluginCount === baselineEnabledPluginCount &&
      observation.extensionsRuntimePluginCount === baselineRuntimePluginCount &&
      observation.extensionsPluginSourceInput === "",
    30_000,
  );
  assertTarget(
    pluginInstalled,
    `native-preview.settings.extensions.plugin.${pluginId}.toggle`,
  );
  assertTarget(
    pluginInstalled,
    `native-preview.settings.extensions.plugin.${pluginId}.delete`,
  );
  assertEqual(
    fs.existsSync(path.join(previewHome, "plugins", pluginId, "lilia-plugin.json")),
    true,
    "managed Plugin manifest exists",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.plugin.${pluginId}.toggle`,
    }),
    "enable Native Plugin package",
  );
  const pluginEnabled = await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsPluginsRegistryRevision === pluginsRevision + 2 &&
      observation.extensionsEnabledPluginCount === baselineEnabledPluginCount + 1 &&
      observation.extensionsRuntimePluginCount === baselineRuntimePluginCount + 1 &&
      observation.extensionsSkillCount === baselineSkillCount + 1 &&
      observation.extensionsRuntimeSkillCount === baselineRuntimeSkillCount + 1 &&
      observation.extensionsMcpCount === baselineMcpCount + 1 &&
      observation.extensionsMcpCredentialCount === baselineMcpCredentialCount + 1,
    30_000,
  );
  const pluginMcpServerId = `plugin.${pluginId}.debug-mcp`;
  const pluginMcpCredentialTarget =
    `native-preview.settings.extensions.mcp.${pluginMcpServerId}.credential.env.NATIVE_DEBUG_TOKEN`;
  assertTarget(pluginEnabled, pluginMcpCredentialTarget);
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: pluginMcpCredentialTarget,
      text: mcpSecretCanary,
    }),
    "input Native Plugin MCP Keyring credential",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${pluginMcpCredentialTarget}.save`,
    }),
    "save Native Plugin MCP Keyring credential",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpConfiguredCredentialCount ===
        baselineConfiguredMcpCredentialCount + 1,
    30_000,
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions.activate-mcp",
    }),
    "activate Native Plugin MCP server",
  );
  const pluginMcpActive = await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsActivationErrorCount === baselineMcpActivationErrorCount &&
      observation.extensionsActiveMcpCount === baselineActiveMcpCount + 1 &&
      observation.extensionsMcpToolCount === baselineMcpToolCount + 1 &&
      observation.extensionsMcpResourceCount === baselineMcpResourceCount + 1 &&
      observation.extensionsMcpPromptCount === baselineMcpPromptCount + 1,
    30_000,
  );
  const pluginMcpResourceTarget =
    `native-preview.settings.extensions.mcp.${pluginMcpServerId}.resource.mcp://native-debug/credential-status.read`;
  assertTarget(pluginMcpActive, pluginMcpResourceTarget);
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: pluginMcpResourceTarget,
    }),
    "read Native Plugin MCP resource",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpContentKind === "resource" &&
      observation.extensionsMcpContentText?.includes('"credentialPresent":true'),
    30_000,
  );
  const userHookSourceId = "native-agentkit:user";
  const hookTargetPrefix = `native-preview.settings.extensions.hook.${userHookSourceId}`;
  const baselineHookSourceCount = pluginEnabled.observation.extensionsHookSourceCount;
  const baselineExistingHookSourceCount =
    pluginEnabled.observation.extensionsExistingHookSourceCount;
  const baselineEnabledHookSourceCount =
    pluginEnabled.observation.extensionsEnabledHookSourceCount;
  assertTarget(pluginEnabled, `${hookTargetPrefix}.create`);
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${hookTargetPrefix}.create`,
    }),
    "create Native user Hook source",
  );
  const hookCreated = await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsHookSourceCount === baselineHookSourceCount &&
      observation.extensionsExistingHookSourceCount === baselineExistingHookSourceCount + 1 &&
      observation.extensionsHookRevisions?.[userHookSourceId] === 1,
    30_000,
  );
  assertTarget(hookCreated, `${hookTargetPrefix}.draft`);
  const hookHandlerFields = [
    ["event", "UserPromptSubmit"],
    ["matcher", "*"],
    ["type", "command"],
    ["command", "printf 'hook-ran\\n' >> native-agent-debug-hook.txt"],
    ["command-windows", "echo hook-ran>>native-agent-debug-hook.txt"],
    ["timeout", "5"],
    ["status-message", "Checking prompt"],
  ];
  for (const [field, text] of hookHandlerFields) {
    const targetId = `${hookTargetPrefix}.handler.0.${field}`;
    assertTarget(hookCreated, targetId);
    assertSuccess(
      await request(address, { command: "input", targetId, text }),
      `input Native Hook handler ${field}`,
    );
  }
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${hookTargetPrefix}.save`,
    }),
    "save Native Hook handlers",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsHookRevisions?.[userHookSourceId] === 2 &&
      observation.extensionsHookHandlerCount === 1,
    30_000,
  );
  const hookDocumentPath = path.join(previewHome, "config", "agentkit-hooks.json");
  const persistedHookDocument = JSON.parse(fs.readFileSync(hookDocumentPath, "utf8"));
  assertEqual(persistedHookDocument.revision, 2, "persisted Hook revision");
  assertEqual(persistedHookDocument.enabled, false, "Hook source defaults disabled");
  assertEqual(persistedHookDocument.handlers?.[0]?.event, "UserPromptSubmit", "persisted Hook event");
  assertEqual(persistedHookDocument.handlers?.[0]?.type, "command", "persisted Hook type");
  assertEqual(persistedHookDocument.handlers?.[0]?.timeoutSeconds, 5, "persisted Hook timeout");
  if (!persistedHookDocument.handlers?.[0]?.id) {
    throw new Error("persisted Hook handler did not receive a stable id");
  }
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${hookTargetPrefix}.toggle`,
    }),
    "enable Native Hook source",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsHookRevisions?.[userHookSourceId] === 3 &&
      observation.extensionsEnabledHookSourceCount === baselineEnabledHookSourceCount + 1,
    30_000,
  );
  const hookMarkerPath = path.join(summary.worktreePath, "native-agent-debug-hook.txt");
  const pluginHookMarkerPath = path.join(
    summary.worktreePath,
    "native-agent-debug-plugin-hook.txt",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.back",
    }),
    "close settings for Native Hook execution",
  );
  const hookPrompt = "native Hook execution probe";
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.task-session.composer.input",
      text: hookPrompt,
    }),
    "input Native Hook execution turn",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.send",
    }),
    "send Native Hook execution turn",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.activeTurnId === null &&
      observation.turnState === "completed" &&
      observation.taskActionError === null &&
      fs.existsSync(hookMarkerPath) &&
      fs.existsSync(pluginHookMarkerPath),
    30_000,
  );
  assertEqual(
    fs.readFileSync(hookMarkerPath, "utf8").trim(),
    "hook-ran",
    "Native UserPromptSubmit Hook execution marker",
  );
  assertEqual(
    fs.readFileSync(pluginHookMarkerPath, "utf8").trim(),
    "plugin-hook-ran",
    "Native Plugin UserPromptSubmit Hook execution marker",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.open",
    }),
    "reopen settings after Native Hook execution",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions",
    }),
    "reopen Native extensions after Hook execution",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.page === "settings/extensions" &&
      !observation.extensionsBusy &&
      observation.extensionsHookRevisions?.[userHookSourceId] === 3 &&
      observation.visibleTargetIds.includes(`${hookTargetPrefix}.toggle`),
    30_000,
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${hookTargetPrefix}.toggle`,
    }),
    "disable Native Hook source",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsHookRevisions?.[userHookSourceId] === 4 &&
      observation.extensionsEnabledHookSourceCount === baselineEnabledHookSourceCount,
    30_000,
  );
  const requestedHookDelete = await request(address, {
    command: "click",
    targetId: `${hookTargetPrefix}.delete`,
  });
  assertSuccess(requestedHookDelete, "request Native Hook source deletion");
  assertEqual(
    requestedHookDelete.observation.extensionsHookDeleteConfirmation,
    userHookSourceId,
    "Hook source delete confirmation",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${hookTargetPrefix}.delete.cancel`,
    }),
    "cancel Native Hook source deletion",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${hookTargetPrefix}.delete`,
    }),
    "request Native Hook source deletion again",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${hookTargetPrefix}.delete.confirm`,
    }),
    "confirm Native Hook source deletion",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsExistingHookSourceCount === baselineExistingHookSourceCount &&
      observation.extensionsHookRevisions?.[userHookSourceId] === 0 &&
      observation.extensionsHookDeleteConfirmation === null,
    30_000,
  );
  assertEqual(fs.existsSync(hookDocumentPath), false, "managed Hook source removed");
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.plugin.${pluginId}.toggle`,
    }),
    "disable Native Plugin package",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsPluginsRegistryRevision === pluginsRevision + 3 &&
      observation.extensionsEnabledPluginCount === baselineEnabledPluginCount &&
      observation.extensionsRuntimePluginCount === baselineRuntimePluginCount &&
      observation.extensionsSkillCount === baselineSkillCount &&
      observation.extensionsRuntimeSkillCount === baselineRuntimeSkillCount &&
      observation.extensionsMcpCount === baselineMcpCount &&
      observation.extensionsActiveMcpCount === baselineActiveMcpCount,
    30_000,
  );
  const requestedPluginDelete = await request(address, {
    command: "click",
    targetId: `native-preview.settings.extensions.plugin.${pluginId}.delete`,
  });
  assertSuccess(requestedPluginDelete, "request Native Plugin deletion");
  assertEqual(
    requestedPluginDelete.observation.extensionsPluginDeleteConfirmation,
    pluginId,
    "Plugin delete confirmation",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.plugin.${pluginId}.delete.cancel`,
    }),
    "cancel Native Plugin deletion",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.plugin.${pluginId}.delete`,
    }),
    "request Native Plugin deletion again",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.plugin.${pluginId}.delete.confirm`,
    }),
    "confirm Native Plugin deletion",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsPluginsRegistryRevision === pluginsRevision + 4 &&
      observation.extensionsPluginCount === baselinePluginCount &&
      observation.extensionsPluginDeleteConfirmation === null,
    30_000,
  );
  assertEqual(
    fs.existsSync(path.join(previewHome, "plugins", pluginId)),
    false,
    "managed Plugin directory removed",
  );
  summary.checks.push(
    "Native Hooks created a disabled revisioned source, persisted handlers, executed UserPromptSubmit once in the bound worktree, toggled trust explicitly and confirmed deletion through stable debug targets",
    "Native Plugin installed from a validated directory, enabled Skill, Hook and credential-bound MCP contributions in the shared runtime, executed the Plugin Hook and MCP resource, then disabled and deleted the managed package through revisioned stable debug targets",
  );
  const registryRevision = extensionsReady.observation.extensionsMcpRegistryRevision;
  assertEqual(
    extensionsReady.observation.extensionsEditableMcpCount,
    1,
    "editable MCP fixture count",
  );
  assertEqual(
    extensionsReady.observation.extensionsEnabledMcpCount,
    1,
    "enabled MCP fixture count",
  );

  const mcpAdd = await request(address, {
    command: "click",
    targetId: "native-preview.settings.extensions.mcp.add",
  });
  assertSuccess(mcpAdd, "open Native MCP editor");
  assertEqual(mcpAdd.observation.extensionsMcpEditorOpen, true, "MCP editor opens");
  assertTarget(mcpAdd, "native-preview.settings.extensions.mcp.editor.id");
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.settings.extensions.mcp.editor.id",
      text: "native-debug-crud",
    }),
    "enter MCP server id",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.settings.extensions.mcp.editor.location",
      text: process.execPath,
    }),
    "enter MCP command",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.settings.extensions.mcp.editor.args",
      text: JSON.stringify([mcpFixturePath, mcpFixtureMarkerPath]),
    }),
    "enter MCP arguments as a JSON array",
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.settings.extensions.mcp.editor.credential-names",
      text: '["NATIVE_DEBUG_TOKEN"]',
    }),
    "register MCP Keyring environment credential name",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions.mcp.editor.enabled",
    }),
    "create MCP as disabled",
  );
  const mcpSaved = await request(address, {
    command: "click",
    targetId: "native-preview.settings.extensions.mcp.editor.save",
  });
  assertSuccess(mcpSaved, "persist new Native MCP server");
  const mcpCreated = await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      !observation.extensionsMcpEditorOpen &&
      observation.extensionsMcpRegistryRevision === registryRevision + 1 &&
      observation.extensionsEditableMcpCount === 2 &&
      observation.extensionsEnabledMcpCount === 1 &&
      observation.extensionsMcpCredentialCount === 1 &&
      observation.extensionsMcpConfiguredCredentialCount === 0,
    30_000,
  );
  assertTarget(
    mcpCreated,
    "native-preview.settings.extensions.mcp.native-debug-crud.edit",
  );
  const registryPath = path.join(previewHome, "config", "agentkit-mcp-registry.json");
  const createdRegistry = JSON.parse(fs.readFileSync(registryPath, "utf8"));
  const createdServer = createdRegistry.servers.find(
    (server) => server.serverId === "native-debug-crud",
  );
  if (
    createdRegistry.secretFree !== true ||
    createdServer?.enabled !== false ||
    JSON.stringify(createdServer?.envSecretNames) !== JSON.stringify(["NATIVE_DEBUG_TOKEN"]) ||
    JSON.stringify(createdServer?.args) !==
      JSON.stringify([mcpFixturePath, mcpFixtureMarkerPath])
  ) {
    throw new Error("Native MCP create did not persist the exact secret-free typed registry entry");
  }
  if (fs.readFileSync(registryPath, "utf8").includes(mcpSecretCanary)) {
    throw new Error("Native MCP secret leaked into the secret-free registry before Keyring write");
  }
  assertSuccess(
    await request(address, {
      command: "input",
      targetId:
        "native-preview.settings.extensions.mcp.native-debug-crud.credential.env.NATIVE_DEBUG_TOKEN",
      text: mcpSecretCanary,
    }),
    "enter MCP credential through the secure Native input",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId:
        "native-preview.settings.extensions.mcp.native-debug-crud.credential.env.NATIVE_DEBUG_TOKEN.save",
    }),
    "save MCP credential to OS Keyring",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpRegistryRevision === registryRevision + 1 &&
      observation.extensionsMcpConfiguredCredentialCount === 1,
    30_000,
  );
  if (fs.readFileSync(registryPath, "utf8").includes(mcpSecretCanary)) {
    throw new Error("Native MCP secret leaked from OS Keyring into the registry");
  }

  const mcpEdit = await request(address, {
    command: "click",
    targetId: "native-preview.settings.extensions.mcp.native-debug-crud.edit",
  });
  assertSuccess(mcpEdit, "edit registered Native MCP server");
  assertEqual(
    mcpEdit.observation.extensionsEditingMcpId,
    "native-debug-crud",
    "MCP editor retains immutable id",
  );
  if (
    mcpEdit.observation.visibleTargetIds.includes(
      "native-preview.settings.extensions.mcp.editor.id",
    )
  ) {
    throw new Error("Native MCP edit exposed the immutable server id as an input target");
  }
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.settings.extensions.mcp.editor.args",
      text: JSON.stringify([mcpFixturePath, mcpFixtureMarkerPath, "--edited"]),
    }),
    "edit MCP arguments",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions.mcp.editor.save",
    }),
    "persist edited Native MCP server",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpRegistryRevision === registryRevision + 2,
    30_000,
  );

  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions.mcp.native-debug-crud.toggle",
    }),
    "enable registered Native MCP server",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpRegistryRevision === registryRevision + 3 &&
      observation.extensionsEnabledMcpCount === 2 &&
      observation.extensionsActiveMcpCount === 1 &&
      observation.extensionsMcpToolCount === 1 &&
      observation.extensionsMcpResourceCount === 1 &&
      observation.extensionsMcpPromptCount === 1 &&
      observation.extensionsActivationErrorCount === 0,
    30_000,
  );
  const mcpTaskBeforeRestart = await request(address, {
    command: "click",
    targetId: "native-preview.settings.back",
  });
  assertSuccess(mcpTaskBeforeRestart, "return to task before MCP startup recovery");
  assertEqual(
    mcpTaskBeforeRestart.observation.page,
    `tasks/${debugTaskId}`,
    "MCP restart source page",
  );
  const mcpTaskWorkspaceActive = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.tab.task:${debugTaskId}`,
  });
  assertSuccess(mcpTaskWorkspaceActive, "activate task Workspace item before MCP restart");
  await waitForObservation(
    address,
    (observation) =>
      observation.page === `tasks/${debugTaskId}` &&
      observation.activeWorkspaceItemIds.includes(`task:${debugTaskId}`) &&
      observation.workspacePersistedRevision === observation.workspaceRevision,
    10_000,
  );
  const mcpProcessId = child.pid;
  ({ child, address } = await restartPreviewProcess(child, previewEnvironment));
  summary.restarts.push({
    kind: "mcp-keyring",
    firstProcessId: mcpProcessId,
    restoredProcessId: child.pid,
  });
  debugAddress = address;
  const restoredMcp = await waitForObservation(
    address,
    (observation) =>
      observation.page === `tasks/${debugTaskId}` &&
      !observation.extensionsBusy &&
      observation.extensionsEnabledMcpCount === 2 &&
      observation.extensionsActiveMcpCount === 1 &&
      observation.extensionsMcpConfiguredCredentialCount === 1 &&
      observation.extensionsMcpToolCount === 1 &&
      observation.extensionsMcpResourceCount === 1 &&
      observation.extensionsMcpPromptCount === 1 &&
      observation.extensionsActivationErrorCount === 1,
    30_000,
  );
  if (restoredMcp.observation.page === "settings/extensions") {
    throw new Error("MCP startup recovery required navigating to Extensions");
  }
  assertSuccess(
    await request(address, { command: "click", targetId: "native-preview.settings.open" }),
    "open settings after MCP startup recovery",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions",
    }),
    "open Extensions for live MCP content",
  );
  const resourceReadTarget =
    "native-preview.settings.extensions.mcp.native-debug-crud.resource.mcp://native-debug/credential-status.read";
  const promptArgumentsTarget =
    "native-preview.settings.extensions.mcp.native-debug-crud.prompt.credential_summary.arguments";
  const promptGetTarget =
    "native-preview.settings.extensions.mcp.native-debug-crud.prompt.credential_summary.get";
  const mcpContentReady = await waitForObservation(
    address,
    (observation) =>
      observation.page === "settings/extensions" &&
      !observation.extensionsBusy &&
      observation.extensionsActiveMcpCount === 1 &&
      observation.visibleTargetIds.includes(resourceReadTarget) &&
      observation.visibleTargetIds.includes(promptArgumentsTarget) &&
      observation.visibleTargetIds.includes(promptGetTarget),
    30_000,
  );
  assertTarget(mcpContentReady, resourceReadTarget);
  assertSuccess(
    await request(address, { command: "click", targetId: resourceReadTarget }),
    "read live MCP resource",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpContentKind === "resource" &&
      observation.extensionsMcpContentTitle === "mcp://native-debug/credential-status" &&
      observation.extensionsMcpContentText === '{"credentialPresent":true}',
    30_000,
  );
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: promptArgumentsTarget,
      text: '{"scope":"restart"}',
    }),
    "enter live MCP prompt arguments",
  );
  assertSuccess(
    await request(address, { command: "click", targetId: promptGetTarget }),
    "materialize live MCP prompt",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpContentKind === "prompt" &&
      observation.extensionsMcpContentTitle === "native-debug-crud/credential_summary" &&
      observation.extensionsMcpContentText === "Native credential scope: restart",
    30_000,
  );
  const liveMcpTask = await request(address, {
    command: "click",
    targetId: "native-preview.settings.back",
  });
  assertSuccess(liveMcpTask, "return to task after live MCP content reads");
  assertEqual(liveMcpTask.observation.page, `tasks/${debugTaskId}`, "MCP content return page");
  assertTarget(liveMcpTask, "native-preview.task-session.composer.input");
  const mcpTimelineBefore = liveMcpTask.observation.timelineEventCount;
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: "native-preview.task-session.composer.input",
      text: mcpToolPrompt,
    }),
    "enter live MCP tool prompt",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.composer.send",
    }),
    "send live MCP tool turn",
  );
  const mcpApproval = await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "waiting_approval" &&
      observation.visibleTargetIds.includes(
        "native-preview.task-session.approval.native-debug-mcp-tool.approve",
      ),
    30_000,
  );
  assertTarget(
    mcpApproval,
    "native-preview.task-session.approval.native-debug-mcp-tool.deny",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.task-session.approval.native-debug-mcp-tool.approve",
    }),
    "approve live MCP tool call",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.turnState === "completed" &&
      observation.timelineEventCount >= mcpTimelineBefore + 3 &&
      fs.existsSync(mcpFixtureMarkerPath),
    30_000,
  );
  const mcpToolMarker = JSON.parse(fs.readFileSync(mcpFixtureMarkerPath, "utf8"));
  if (
    mcpToolMarker.called !== true ||
    mcpToolMarker.credentialPresent !== true ||
    mcpToolMarker.text !== "Native AgentKit MCP tool call"
  ) {
    throw new Error("Native Agent turn did not execute the live Keyring-backed MCP tool");
  }
  if (fs.readFileSync(mcpFixtureMarkerPath, "utf8").includes(mcpSecretCanary)) {
    throw new Error("Native MCP fixture leaked the Keyring secret into its tool marker");
  }
  if (
    !summary.modelFixtureRequests?.some(
      (entry) => entry.mcpToolPromptSeen && entry.mcpToolAvailable,
    )
  ) {
    throw new Error("Native Agent model request did not expose the live MCP tool descriptor");
  }
  if (!summary.modelFixtureRequests?.some((entry) => entry.mcpToolResultSeen)) {
    throw new Error("Native Agent did not return the real MCP tool result to the same model turn");
  }
  assertSuccess(
    await request(address, { command: "click", targetId: "native-preview.settings.open" }),
    "reopen settings after live MCP tool call",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions",
    }),
    "return to Native extensions after live MCP tool call",
  );
  await waitForObservation(
    address,
    (observation) =>
      observation.page === "settings/extensions" &&
      !observation.extensionsBusy &&
      observation.extensionsActiveMcpCount === 1,
    30_000,
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions.mcp.native-debug-crud.toggle",
    }),
    "disable registered Native MCP server",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpRegistryRevision === registryRevision + 4 &&
      observation.extensionsEnabledMcpCount === 1,
    30_000,
  );

  const mcpDelete = await request(address, {
    command: "click",
    targetId: "native-preview.settings.extensions.mcp.native-debug-crud.delete",
  });
  assertSuccess(mcpDelete, "request Native MCP deletion");
  assertEqual(
    mcpDelete.observation.extensionsMcpDeleteConfirmation,
    "native-debug-crud",
    "MCP deletion requires confirmation",
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions.mcp.native-debug-crud.delete-confirm",
    }),
    "confirm Native MCP deletion",
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpRegistryRevision === registryRevision + 5 &&
      observation.extensionsEditableMcpCount === 1 &&
      observation.extensionsMcpDeleteConfirmation === null,
    30_000,
  );
  const deletedRegistry = JSON.parse(fs.readFileSync(registryPath, "utf8"));
  if (deletedRegistry.servers.some((server) => server.serverId === "native-debug-crud")) {
    throw new Error("Native MCP delete left the removed server in the registry");
  }
  if (deletedRegistry.secretFree !== true) {
    throw new Error("Native MCP delete changed the secret-free registry invariant");
  }

  await verifyAuthenticatedHttpMcp(address, {
    fixture: mcpHttpFixture,
    registryPath,
    serverId: "native-debug-http",
    transport: "streamable_http",
    transportClicks: 1,
    endpoint: mcpHttpFixture.streamableEndpoint,
    resourceUri: "mcp://native-debug-http/credential-status",
    expectedContent: '{"authorized":true,"transport":"streamable_http"}',
    expectedResponseKind: "application/json",
    credential: mcpSecretCanary,
    verifyRecovery: true,
    expectedRecoveryErrors: 1,
  });
  await verifyAuthenticatedHttpMcp(address, {
    fixture: mcpHttpFixture,
    registryPath,
    serverId: "native-debug-sse",
    transport: "sse",
    transportClicks: 2,
    endpoint: mcpHttpFixture.sseEndpoint,
    resourceUri: "mcp://native-debug-sse/credential-status",
    expectedContent: '{"authorized":true,"transport":"sse"}',
    expectedResponseKind: "legacy_sse_message",
    credential: mcpSecretCanary,
  });
  summary.mcpHttpRequests = mcpHttpFixture.requests;

  const mcpActivated = await request(address, {
    command: "click",
    targetId: "native-preview.settings.extensions.activate-mcp",
  });
  assertSuccess(mcpActivated, "activate registered Native MCP servers");
  const mcpActivationFinished = await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy && observation.extensionsActivationErrorCount === 1,
    30_000,
  );
  assertEqual(
    mcpActivationFinished.observation.extensionsActiveMcpCount,
    0,
    "invalid MCP remains disconnected",
  );
  const mcpErrors = await request(address, { command: "recent-errors" });
  assertSuccess(mcpErrors, "read Native MCP activation errors");
  if (!mcpErrors.observation.errors.some((entry) => entry.source.startsWith("mcp:"))) {
    throw new Error("Native recent-errors did not retain the real MCP activation failure");
  }
  await recordRenderedWindow(
    child.pid,
    extensionsScreenshotPath,
    "extensions",
    "extensionsWindowBounds",
    "extensionsPngSize",
  );
  const extensionsClosed = await request(address, {
    command: "click",
    targetId: "native-preview.settings.back",
  });
  assertSuccess(extensionsClosed, "close Native extensions settings");
  assertEqual(
    extensionsClosed.observation.page,
    "tasks/native-agent-debug-task",
    "return from extensions",
  );
  summary.checks.push(
    "Native extensions created, disabled, re-enabled and confirmed deletion of a revision-safe managed Skill while the exact AgentKit package catalog followed each state; it also persisted an enabled MCP registration, restarted the process, restored its OS Keyring credential and live tool/resource/prompt catalog without opening Extensions, read the real resource and materialized the parameterized prompt, approved and executed its read-only tool through a normal Agent turn, exercised authenticated Streamable HTTP JSON and legacy SSE GET/endpoint/POST traffic through Keyring-backed headers, then used revision-safe secret-free edit, disable and confirmed delete operations while retaining per-server activation errors",
  );

  const remoteSettingsOpen = await request(address, {
    command: "click",
    targetId: "native-preview.settings.open",
  });
  assertSuccess(remoteSettingsOpen, "open settings for Native remote control");
  const remoteOpened = await request(address, {
    command: "click",
    targetId: "native-preview.settings.remote",
  });
  assertSuccess(remoteOpened, "open Native remote control settings");
  assertEqual(remoteOpened.observation.page, "settings/remote", "remote settings page");
  const remoteReady = await waitForObservation(
    address,
    (observation) =>
      observation.page === "settings/remote" &&
      !observation.remoteBusy &&
      observation.remoteError === null &&
      observation.remoteHostEnabled &&
      observation.remoteTrustedDeviceCount === 1,
    30_000,
  );
  assertTarget(remoteReady, "native-preview.settings.remote.refresh");
  assertTarget(remoteReady, "native-preview.settings.remote.host-toggle");
  assertTarget(remoteReady, "native-preview.settings.remote.pc-name");
  assertTarget(remoteReady, "native-preview.settings.remote.pc-name-save");
  assertTarget(remoteReady, "native-preview.settings.remote.keep-awake");
  assertTarget(remoteReady, "native-preview.settings.remote.start-pairing");

  const trustedDeviceRevoke = remoteReady.observation.visibleTargetIds.find(
    (target) =>
      target.startsWith("native-preview.settings.remote.device.") &&
      target.endsWith(".revoke"),
  );
  if (!trustedDeviceRevoke) throw new Error("Native trusted remote device revoke target was not found");

  const renamedRemote = await request(address, {
    command: "input",
    targetId: "native-preview.settings.remote.pc-name",
    text: "Agent Debug Native PC",
  });
  assertSuccess(renamedRemote, "enter Native remote PC name");
  const savedRemoteName = await request(address, {
    command: "click",
    targetId: "native-preview.settings.remote.pc-name-save",
  });
  assertSuccess(savedRemoteName, "save Native remote PC name");
  await waitForObservation(
    address,
    (observation) =>
      !observation.remoteBusy &&
      observation.remoteError === null &&
      observation.remotePcName === "Agent Debug Native PC",
    30_000,
  );

  const previousKeepAwake = remoteReady.observation.remoteKeepAwakeEnabled;
  const keepAwakeChanged = await request(address, {
    command: "click",
    targetId: "native-preview.settings.remote.keep-awake",
  });
  assertSuccess(keepAwakeChanged, "toggle Native remote keep-awake");
  await waitForObservation(
    address,
    (observation) =>
      !observation.remoteBusy &&
      observation.remoteError === null &&
      observation.remoteKeepAwakeEnabled !== previousKeepAwake,
    30_000,
  );

  const revokedRemoteDevice = await request(address, {
    command: "click",
    targetId: trustedDeviceRevoke,
  });
  assertSuccess(revokedRemoteDevice, "revoke Native trusted remote device");
  const remoteDeviceRevoked = await waitForObservation(
    address,
    (observation) =>
      !observation.remoteBusy &&
      observation.remoteError === null &&
      observation.remoteTrustedDeviceCount === 0,
    30_000,
  );
  if (remoteDeviceRevoked.observation.visibleTargetIds.includes(trustedDeviceRevoke)) {
    throw new Error("revoked Native remote device remains actionable");
  }

  const pairingStarted = await request(address, {
    command: "click",
    targetId: "native-preview.settings.remote.start-pairing",
  });
  assertSuccess(pairingStarted, "start Native remote pairing");
  const remotePairing = await waitForObservation(
    address,
    (observation) =>
      !observation.remoteBusy &&
      observation.remoteError === null &&
      observation.remotePairingActive,
    30_000,
  );
  assertTarget(remotePairing, "native-preview.settings.remote.copy-pairing");
  assertTarget(remotePairing, "native-preview.settings.remote.cancel-pairing");
  await recordRenderedWindow(
    child.pid,
    remoteScreenshotPath,
    "remote control",
    "remoteWindowBounds",
    "remotePngSize",
  );

  const pairingCopied = await request(address, {
    command: "click",
    targetId: "native-preview.settings.remote.copy-pairing",
  });
  assertSuccess(pairingCopied, "copy Native remote pairing URI");
  if (pairingCopied.observation.remoteError !== null) {
    summary.remoteClipboardGateError = pairingCopied.observation.remoteError;
    clipboardGateError ??= new Error(
      `remote pairing clipboard write failed: ${pairingCopied.observation.remoteError}`,
    );
  }
  const pairingCancelled = await request(address, {
    command: "click",
    targetId: "native-preview.settings.remote.cancel-pairing",
  });
  assertSuccess(pairingCancelled, "cancel Native remote pairing");
  await waitForObservation(
    address,
    (observation) =>
      !observation.remoteBusy &&
      observation.remoteError === null &&
      !observation.remotePairingActive,
    30_000,
  );
  const remoteClosed = await request(address, {
    command: "click",
    targetId: "native-preview.settings.back",
  });
  assertSuccess(remoteClosed, "close Native remote control settings");
  assertEqual(
    remoteClosed.observation.page,
    "tasks/native-agent-debug-task",
    "return from remote control",
  );
  summary.checks.push(
    "Native remote control changed persisted host state, revoked a trusted device and rendered a real scanner-safe pairing QR without WebView",
  );

  const finalPaneId = remoteClosed.observation.activeWorkspacePaneId;
  if (!finalPaneId) throw new Error("Native Workspace lost its active pane before visual capture");
  const finalPaneSplit = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.pane.${finalPaneId}.split-vertical`,
  });
  assertSuccess(finalPaneSplit, "split the Native workspace for final visual capture");
  const finalPaneMove = await request(address, {
    command: "click",
    targetId: `native-preview.workspace.pane.${finalPaneId}.move-next`,
  });
  assertSuccess(finalPaneMove, "move the active task into the final visual pane");
  assertEqual(finalPaneMove.observation.workspacePaneCount, 2, "final visual pane count");
  if (
    finalPaneMove.observation.workspacePanes.filter((pane) => pane.activeItemId !== null).length < 2
  ) {
    throw new Error("final Native Workspace capture does not have two populated panes");
  }

  const finalWindowCapture = await recordRenderedWindow(
    child.pid,
    screenshotPath,
    "final workspace",
    "windowBounds",
    "pngSize",
  );
  if (finalWindowCapture) {
    summary.checks.push("real Win32 window screenshot captured with two populated Native panes");
  }
  summary.finalObservation = finalPaneMove.observation;
  const hardGateErrors = [];
  if (clipboardGateError) {
    hardGateErrors.push(`Windows clipboard gate failed: ${clipboardGateError.message}`);
  }
  if (summary.screenshotGateErrors.length > 0) {
    const firstScreenshotError = summary.screenshotGateErrors[0];
    hardGateErrors.push(
      `Native GPU screenshot gate failed at ${firstScreenshotError.surface}: ` +
        `${firstScreenshotError.error}; skipped ${summary.screenshotSkippedSurfaces.length} later surfaces`,
    );
  }
  if (hardGateErrors.length > 0) {
    throw new Error(hardGateErrors.join("; "));
  }
  summary.success = true;
} catch (error) {
  summary.error = error instanceof Error ? error.stack ?? error.message : String(error);
  process.exitCode = 1;
} finally {
  if (debugAddress && child?.exitCode === null) {
    try {
      await cleanupProviderCredentials(debugAddress);
      summary.checks.push("Agent Debug credentials were removed from the Preview Keyring namespace");
    } catch (cleanupError) {
      summary.credentialCleanupError =
        cleanupError instanceof Error ? cleanupError.message : String(cleanupError);
      summary.success = false;
      process.exitCode = 1;
    }
  } else if (summary.success) {
    summary.credentialCleanupError =
      "Native Preview exited before Agent Debug credentials could be cleaned up";
    summary.success = false;
    process.exitCode = 1;
  }
  await stopChild(child);
  await stopHangingGitFixture(hangingGitFixture);
  await mcpHttpFixture?.close?.();
  await stopServer(githubFixture?.server);
  await stopServer(modelServer);
  if (stdoutFd !== undefined) fs.closeSync(stdoutFd);
  if (stderrFd !== undefined) fs.closeSync(stderrFd);
  const secretLeakPaths = [transcriptPath, stdoutPath, stderrPath].filter(
    (artifact) =>
      fs.existsSync(artifact) &&
      [providerSecretCanary, mcpSecretCanary, githubTokenCanary].some((canary) =>
        fs.readFileSync(artifact, "utf8").includes(canary),
      ),
  );
  if (
    [providerSecretCanary, mcpSecretCanary, githubTokenCanary].some((canary) =>
      JSON.stringify(summary).includes(canary),
    )
  ) {
    secretLeakPaths.push("summary-memory");
  }
  if (secretLeakPaths.length > 0) {
    summary.secretLeakPaths = secretLeakPaths;
    summary.success = false;
    summary.error = "Credential secret leaked into Native Agent Debug artifacts";
    process.exitCode = 1;
  }
  fs.writeFileSync(summaryPath, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
  process.stdout.write(`${summaryPath}\n`);
}

async function waitForReady(processHandle, file, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (fs.existsSync(file) && fs.readFileSync(file, "utf8").trim()) return;
    if (processHandle.exitCode !== null) {
      throw new Error(`Native Preview exited before ready with code ${processHandle.exitCode}`);
    }
    await delay(100);
  }
  throw new Error(`timed out waiting for ${file}`);
}

async function waitForJsonFile(file, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      return JSON.parse(fs.readFileSync(file, "utf8"));
    } catch (error) {
      lastError = error;
      await delay(50);
    }
  }
  throw new Error(`timed out reading JSON file ${file}: ${lastError?.message ?? "unknown"}`);
}

async function waitForPathAbsent(target, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (!fs.existsSync(target)) return;
    await delay(100);
  }
  throw new Error(`timed out waiting for cancelled clone target cleanup: ${target}`);
}

async function waitForFirstPng(directory, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (fs.existsSync(directory)) {
      const candidate = fs
        .readdirSync(directory)
        .filter((entry) => entry.toLowerCase().endsWith(".png"))
        .map((entry) => path.join(directory, entry))
        .find((entry) => {
          const bytes = fs.readFileSync(entry);
          return (
            bytes.length > 1_000 &&
            bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))
          );
        });
      if (candidate) return candidate;
    }
    await delay(100);
  }
  throw new Error(`timed out waiting for a captured PNG in ${directory}`);
}

function request(address, payload) {
  appendTranscript("request", redactDebugPayload(payload));
  const separator = address.lastIndexOf(":");
  const host = address.slice(0, separator);
  const port = Number(address.slice(separator + 1));
  if (host !== "127.0.0.1" || !Number.isInteger(port) || port <= 0) {
    throw new Error(`invalid loopback ready address: ${address}`);
  }

  return new Promise((resolve, reject) => {
    const socket = net.createConnection({ host, port });
    socket.setEncoding("utf8");
    socket.setTimeout(15_000);
    let buffer = "";
    socket.on("connect", () => socket.write(`${JSON.stringify(payload)}\n`));
    socket.on("data", (chunk) => {
      buffer += chunk;
      const newline = buffer.indexOf("\n");
      if (newline === -1) return;
      const line = buffer.slice(0, newline);
      socket.end();
      try {
        const response = JSON.parse(line);
        appendTranscript("response", response);
        resolve(response);
      } catch (error) {
        reject(new Error(`invalid JSONL response: ${line}\n${error}`));
      }
    });
    socket.on("timeout", () => socket.destroy(new Error("debug request timed out")));
    socket.on("error", reject);
  });
}

function redactDebugPayload(payload) {
  if (
    payload?.command === "input" &&
    (payload?.targetId === "native-preview.settings.provider.secret" ||
      payload?.targetId?.startsWith("native-preview.settings.extensions.mcp.") &&
        payload.targetId.includes(".credential."))
  ) {
    return { ...payload, text: "[REDACTED]" };
  }
  return payload;
}

async function startGitHubFixture(expectedToken) {
  const requests = [];
  let tokenPolls = 0;
  const server = http.createServer((request, response) => {
    let requestBody = "";
    request.setEncoding("utf8");
    request.on("data", (chunk) => {
      requestBody += chunk;
    });
    request.on("end", () => {
      const requestUrl = new URL(request.url ?? "/", `http://${request.headers.host}`);
      const authorized = request.headers.authorization === `Bearer ${expectedToken}`;
      const respond = (status, payload, headers = {}) => {
        const body = Buffer.from(JSON.stringify(payload), "utf8");
        response.writeHead(status, {
          "Content-Type": "application/json",
          "Content-Length": body.length,
          Connection: "close",
          ...headers,
        });
        response.end(body);
      };
      if (request.method === "POST" && requestUrl.pathname === "/login/device/code") {
        requests.push({ kind: "device-code", method: request.method });
        respond(200, {
          device_code: "native-debug-device-code",
          user_code: "LILIA-DEBUG",
          verification_uri: `${requestUrl.origin}/verify`,
          expires_in: 60,
          interval: 1,
        });
        return;
      }
      if (
        request.method === "POST" &&
        requestUrl.pathname === "/login/oauth/access_token"
      ) {
        tokenPolls += 1;
        requests.push({ kind: "access-token", method: request.method, poll: tokenPolls });
        const params = new URLSearchParams(requestBody);
        if (params.get("device_code") !== "native-debug-device-code") {
          respond(400, { error: "incorrect_device_code" });
        } else if (tokenPolls === 1) {
          respond(200, { error: "authorization_pending" });
        } else {
          respond(200, {
            access_token: expectedToken,
            token_type: "bearer",
            scope: "repo read:user",
          });
        }
        return;
      }
      if (request.method === "GET" && requestUrl.pathname === "/user") {
        requests.push({ kind: "user", method: request.method, authorized });
        if (!authorized) {
          respond(401, { message: "Bad credentials" });
          return;
        }
        respond(200, {
          login: "native-debug",
          avatar_url: `${requestUrl.origin}/avatar.png`,
        });
        return;
      }
      if (request.method === "GET" && requestUrl.pathname === "/user/repos") {
        const page = Number(requestUrl.searchParams.get("page") ?? "1");
        requests.push({ kind: "repositories", method: request.method, page, authorized });
        if (!authorized) {
          respond(401, { message: "Bad credentials" });
          return;
        }
        const repository = (id, name, isPrivate) => ({
          id,
          name,
          full_name: `native-debug/${name}`,
          private: isPrivate,
          description: `${name} Native repository fixture`,
          default_branch: "main",
          updated_at: `2026-08-${String(12 - id).padStart(2, "0")}T00:00:00Z`,
          clone_url: `https://github.com/native-debug/${name}.git`,
          html_url: `https://github.com/native-debug/${name}`,
          owner: { login: "native-debug" },
        });
        if (page === 1) {
          respond(
            200,
            [repository(1, "private-repo", true), repository(2, "public-repo", false)],
            {
              Link: `<${requestUrl.origin}/user/repos?page=2>; rel="next", <${requestUrl.origin}/user/repos?page=2>; rel="last"`,
            },
          );
        } else {
          respond(200, [repository(3, "tools-repo", false)]);
        }
        return;
      }
      if (request.method === "GET" && requestUrl.pathname === "/verify") {
        const body = Buffer.from("GitHub fixture authorization page", "utf8");
        response.writeHead(200, {
          "Content-Type": "text/plain; charset=utf-8",
          "Content-Length": body.length,
          Connection: "close",
        });
        response.end(body);
        return;
      }
      respond(404, { error: "not_found" });
    });
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") {
    server.close();
    throw new Error("GitHub fixture did not bind a TCP port");
  }
  return {
    server,
    requests,
    baseUrl: `http://127.0.0.1:${address.port}`,
  };
}

async function cleanupProviderCredentials(address) {
  let observation = await request(address, { command: "observe" });
  assertSuccess(observation, "observe before credential cleanup");
  if (!observation.observation.page.startsWith("settings/")) {
    observation = await request(address, {
      command: "click",
      targetId: "native-preview.settings.open",
    });
    assertSuccess(observation, "open settings for credential cleanup");
  }
  if (observation.observation.page !== "settings/provider") {
    observation = await request(address, {
      command: "click",
      targetId: "native-preview.settings.provider",
    });
    assertSuccess(observation, "open provider settings for credential cleanup");
  }
  const providerIds = observation.observation.providerIds;
  if (!Array.isArray(providerIds) || providerIds.length === 0) {
    throw new Error("credential cleanup could not observe the Provider catalog");
  }
  let revoked = 0;
  for (const providerId of providerIds) {
    if (observation.observation.providerId !== providerId) {
      const selected = await request(address, {
        command: "click",
        targetId: `native-preview.settings.provider.${providerId}`,
      });
      assertSuccess(selected, `select ${providerId} for credential cleanup`);
      observation = await waitForObservation(
        address,
        (candidate) =>
          candidate.page === "settings/provider" &&
          candidate.providerId === providerId &&
          !candidate.providerBusy,
        30_000,
      );
    }
    while (true) {
      const revokeTarget = observation.observation.visibleTargetIds.find(
        (target) =>
          target.startsWith("native-preview.settings.provider.credential.") &&
          target.endsWith(".revoke"),
      );
      if (!revokeTarget) break;
      if (revoked >= 32) {
        throw new Error("credential cleanup exceeded the expected credential count");
      }
      const activeBefore = observation.observation.providerActiveCredentialCount;
      const clicked = await request(address, { command: "click", targetId: revokeTarget });
      assertSuccess(clicked, "revoke credential during cleanup");
      observation = await waitForObservation(
        address,
        (candidate) =>
          !candidate.providerBusy &&
          candidate.providerError === null &&
          candidate.providerActiveCredentialCount < activeBefore,
        30_000,
      );
      revoked += 1;
    }
  }
  if (observation.observation.providerActiveCredentialCount !== 0) {
    throw new Error(
      `credential cleanup left ${observation.observation.providerActiveCredentialCount} active credentials`,
    );
  }
}

async function startHangingGitFixture() {
  const sockets = new Set();
  const requests = [];
  const server = http.createServer((request, response) => {
    const entry = {
      method: request.method,
      url: request.url,
      acceptedAt: new Date().toISOString(),
    };
    requests.push(entry);
    request.resume();
    response.on("close", () => {
      if (entry.closedAt === undefined) entry.closedAt = new Date().toISOString();
    });
    // Deliberately do not send headers or a body. Git remains blocked in a real
    // HTTP request until the product cancellation path terminates its process.
  });
  server.requestTimeout = 0;
  server.headersTimeout = 0;
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") {
    for (const socket of sockets) socket.destroy();
    server.close();
    throw new Error("hanging Git fixture did not bind a TCP port");
  }
  return {
    server,
    sockets,
    requests,
    repositoryUrl: `http://127.0.0.1:${address.port}/native-agent-debug-hanging.git`,
  };
}

async function stopHangingGitFixture(fixture) {
  if (!fixture) return;
  for (const socket of fixture.sockets) socket.destroy();
  if (!fixture.server.listening) return;
  await new Promise((resolve) => fixture.server.close(resolve));
}

function latestUserMessageText(requestBody) {
  try {
    const payload = JSON.parse(requestBody);
    const messages = Array.isArray(payload?.messages) ? payload.messages : [];
    const latest = messages.findLast((message) => message?.role === "user");
    if (typeof latest?.content === "string") return latest.content;
    return latest?.content === undefined ? "" : JSON.stringify(latest.content);
  } catch {
    return "";
  }
}

function hasCompletedToolRecovery(requestPayload) {
  const messages = Array.isArray(requestPayload?.messages) ? requestPayload.messages : [];
  const recoveryIndex = messages.findIndex((message) => {
    if (
      message?.role !== "tool" ||
      message?.tool_call_id !== `${interruptedToolTurnId}:tool`
    ) {
      return false;
    }
    if (typeof message.content !== "string") return message.content?.status === "completed";
    try {
      const content = JSON.parse(message.content);
      return content?.status === "completed" || content?.answer?.status === "completed";
    } catch {
      return false;
    }
  });
  return (
    recoveryIndex >= 0 &&
    !messages.slice(recoveryIndex + 1).some((message) => message?.role === "user")
  );
}

async function startModelFixture() {
  let approvalIssued = false;
  let fifoApprovalIssued = false;
  let guideCancelApprovalIssued = false;
  let planReplayIssued = false;
  let planCancelIssued = false;
  let questionReplayIssued = false;
  let architectureApprovalIssued = false;
  let mcpToolIssued = false;
  let retryFailureIssued = false;
  let awaitingQuestionResolution = false;
  const server = http.createServer((request, response) => {
    let requestBody = "";
    request.setEncoding("utf8");
    request.on("data", (chunk) => {
      requestBody += chunk;
    });
    request.on("end", () => {
      const latestUserText = latestUserMessageText(requestBody);
      let requestPayload = null;
      try {
        requestPayload = JSON.parse(requestBody);
      } catch {
        requestPayload = null;
      }
      const isAutoTurnDecision =
        requestBody.includes("回合策略决策辅助 Agent") &&
        latestUserText.includes('"tierPolicy"') &&
        latestUserText.includes('"promptPreview"');
      if (isAutoTurnDecision) {
        let decisionInput = {};
        try {
          decisionInput = JSON.parse(latestUserText);
        } catch {
          decisionInput = {};
        }
        summary.modelFixtureRequests ??= [];
        summary.modelFixtureRequests.push({
          bodyLength: requestBody.length,
          autoTurnDecisionSeen: true,
          reasoningEffort: requestPayload?.reasoning_effort ?? null,
          decisionPromptLength: decisionInput?.promptLength ?? null,
        });
        const decision = JSON.stringify({
          tier: "normal",
          reasoningEffort: "medium",
          planMode: decisionInput?.current?.planMode === true,
          goalMode: decisionInput?.current?.goalMode === true,
          sessionFork: false,
          summary: "Native 自动回合策略验证",
          signals: ["真实控制模型请求"],
        });
        const body = JSON.stringify({
          choices: [
            {
              finish_reason: "stop",
              message: { role: "assistant", content: decision },
            },
          ],
          usage: { prompt_tokens: 2, completion_tokens: 1, total_tokens: 3 },
        });
        response.writeHead(200, {
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(body),
          Connection: "close",
        });
        response.end(body);
        return;
      }
      const shouldRequestApproval =
        requestBody.includes("native-approval") && !approvalIssued;
      const shouldRequestFifoApproval =
        latestUserText.includes(fifoActivePrompt) && !fifoApprovalIssued;
      const shouldRequestGuideCancelApproval =
        latestUserText.includes(guideCancelActivePrompt) && !guideCancelApprovalIssued;
      const shouldRequestPlanReplay =
        requestBody.includes("native-plan-restart") && !planReplayIssued;
      const shouldRequestPlanCancel =
        requestBody.includes("native-plan-cancel") && !planCancelIssued;
      const shouldRequestQuestionReplay =
        requestBody.includes("native-question-restart") && !questionReplayIssued;
      const shouldRequestArchitectureApproval =
        latestUserText.includes(architectureApprovalPrompt) && !architectureApprovalIssued;
      const shouldRequestMcpTool = latestUserText.includes(mcpToolPrompt) && !mcpToolIssued;
      const shouldFailRetry = latestUserText.includes(retryFailurePrompt) && !retryFailureIssued;
      const questionAnswerSeen = requestBody.includes("native-choice-b");
      const fifoFirstPromptSeen = latestUserText.includes(fifoFirstPrompt);
      const fifoSecondPromptSeen = latestUserText.includes(fifoSecondPrompt);
      const contextReferencePromptSeen = latestUserText.includes("native-context-reference");
      summary.modelFixtureRequests ??= [];
      summary.modelFixtureRequests.push({
        bodyLength: requestBody.length,
        approvalPromptSeen: requestBody.includes("native-approval"),
        planReplayPromptSeen: requestBody.includes("native-plan-restart"),
        planCancelPromptSeen: requestBody.includes("native-plan-cancel"),
        questionReplayPromptSeen: requestBody.includes("native-question-restart"),
        architectureApprovalPromptSeen: latestUserText.includes(architectureApprovalPrompt),
        mcpToolPromptSeen: latestUserText.includes(mcpToolPrompt),
        mcpToolAvailable: requestPayload?.tools?.some(
          (tool) => tool?.function?.name === "native-debug-crud/credential_probe",
        ),
        mcpToolResultSeen: requestBody.includes("credentialPresent"),
        architectureResolutionSeen:
          requestBody.includes("architecture_change") &&
          requestBody.includes("native-debug-architecture-approval") &&
          requestBody.includes("allow"),
        questionAnswerSeen,
        fifoActivePromptSeen: latestUserText.includes(fifoActivePrompt),
        fifoFirstPromptSeen,
        fifoSecondPromptSeen,
        contextReferencePromptSeen,
        toolRecoveryCompletionSeen: hasCompletedToolRecovery(requestPayload),
        conversationReferenceSeen:
          latestUserText.includes("[对话引用: 验证 Native 计划重启回放 |") &&
          latestUserText.includes(debugPlanReplayTaskId),
        contextAttachmentSeen:
          latestUserText.includes("[文件引用: README.md |") &&
          latestUserText.includes(previewWorkspace),
        guideCancelActivePromptSeen: latestUserText.includes(guideCancelActivePrompt),
        guideCancelQueuedPromptSeen: latestUserText.includes(guideCancelQueuedPrompt),
        reviewBranchSeen: requestBody.includes(workflowReviewBranch),
        retryFailurePromptSeen: latestUserText.includes(retryFailurePrompt),
        issuedToolCall:
          shouldRequestApproval ||
          shouldRequestFifoApproval ||
          shouldRequestGuideCancelApproval ||
          shouldRequestPlanReplay ||
          shouldRequestPlanCancel ||
          shouldRequestQuestionReplay ||
          shouldRequestArchitectureApproval ||
          shouldRequestMcpTool,
        worktreePathSeen:
          typeof summary.worktreePath === "string" && requestBody.includes(summary.worktreePath),
        reasoningEffort: requestPayload?.reasoning_effort ?? null,
      });
      if (shouldRequestApproval) approvalIssued = true;
      if (shouldRequestFifoApproval) fifoApprovalIssued = true;
      if (shouldRequestGuideCancelApproval) guideCancelApprovalIssued = true;
      if (shouldRequestPlanReplay) planReplayIssued = true;
      if (shouldRequestPlanCancel) planCancelIssued = true;
      if (shouldRequestArchitectureApproval) architectureApprovalIssued = true;
      if (shouldRequestMcpTool) mcpToolIssued = true;
      if (shouldFailRetry) {
        retryFailureIssued = true;
        const body = JSON.stringify({ error: "native retry fixture failure" });
        response.writeHead(500, {
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(body),
          Connection: "close",
        });
        response.end(body);
        return;
      }
      if (awaitingQuestionResolution && !questionAnswerSeen) {
        const body = JSON.stringify({ error: "restored question answer was not forwarded" });
        response.writeHead(409, {
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(body),
          Connection: "close",
        });
        response.end(body);
        return;
      }
      if (awaitingQuestionResolution) awaitingQuestionResolution = false;
      if (shouldRequestQuestionReplay) {
        questionReplayIssued = true;
        awaitingQuestionResolution = true;
      }
      const toolCall =
        shouldRequestApproval || shouldRequestFifoApproval || shouldRequestGuideCancelApproval
        ? {
            id: shouldRequestFifoApproval
              ? "native-debug-fifo-write"
              : shouldRequestGuideCancelApproval
                ? "native-debug-guide-cancel-write"
                : "native-debug-write",
            type: "function",
            function: {
              name: "computer.fs.write",
              arguments:
                shouldRequestFifoApproval || shouldRequestGuideCancelApproval
                  ? '{"path":"native-debug-fifo-created.txt","content":"fifo","create":true}'
                  : '{"path":"native-debug-created.txt","content":"debug","create":true}',
            },
          }
        : shouldRequestMcpTool
          ? {
              id: "native-debug-mcp-tool",
              type: "function",
              function: {
                name: "native-debug-crud/credential_probe",
                arguments: JSON.stringify({ text: "Native AgentKit MCP tool call" }),
              },
            }
        : shouldRequestArchitectureApproval
          ? {
              id: "native-debug-architecture-approval",
              type: "function",
              function: {
                name: "update_project_architecture",
                arguments: JSON.stringify({
                  reason: "将 Agent 架构审批纳入 Native 应用边界",
                  changes: [
                    {
                      type: "upsert_node",
                      node: {
                        id: "agent-architecture-approval",
                        label: "Agent Architecture Approval",
                        type: "workflow",
                        summary: "由共享应用层原子审批并更新项目架构",
                        paths: ["crates/lilia-desktop-application/src/agent.rs"],
                        tags: ["native", "agent"],
                      },
                    },
                    {
                      type: "set_summary",
                      summary: "Native UI 与产品服务保持单向依赖，Agent 架构变更经类型化审批落库。",
                    },
                  ],
                }),
              },
            }
          : shouldRequestQuestionReplay
          ? {
              id: "native-debug-question-replay",
              type: "function",
              function: {
                name: "ask_user_question",
                arguments: JSON.stringify({
                  title: "Native 提问恢复",
                  question: "请选择恢复后继续使用的验证目标",
                  options: [
                    { label: "目标 A", value: "native-choice-a" },
                    { label: "目标 B", value: "native-choice-b" },
                  ],
                }),
              },
            }
          : shouldRequestPlanReplay || shouldRequestPlanCancel
          ? {
              id: shouldRequestPlanReplay
                ? "native-debug-plan-replay"
                : "native-debug-plan-cancel",
              type: "function",
              function: {
                name: "confirm_plan",
                arguments: JSON.stringify({
                  title: "Native 计划确认",
                  question: "是否按此计划继续？",
                  plan: "1. 恢复等待状态\n2. 处理用户决策\n3. 验证同一 turn",
                }),
              },
            }
          : null;
      const body = JSON.stringify(
        toolCall
        ? {
            choices: [
              {
                finish_reason: "tool_calls",
                message: {
                  role: "assistant",
                  content: null,
                  tool_calls: [toolCall],
                },
              },
            ],
            usage: { prompt_tokens: 2, completion_tokens: 1, total_tokens: 3 },
          }
        : {
            choices: [
              {
                finish_reason: "stop",
                message: {
                  role: "assistant",
                  content:
                    "### Native Markdown\n\n- **粗体**、`code` 与 $E = mc^2$\n\n| 名称 | 数量 | 比例 |\n| :--- | ---: | :---: |\n| **Alpha** | 42 | $1/2$ |\n\n```mermaid\nflowchart LR\nA[Composer] --> B[Timeline]\n```",
                },
              },
            ],
            usage: { prompt_tokens: 2, completion_tokens: 2, total_tokens: 4 },
          },
      );
      const responseDelayMs = fifoFirstPromptSeen || fifoSecondPromptSeen ? 750 : 0;
      setTimeout(() => {
        response.writeHead(200, {
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(body),
          Connection: "close",
        });
        response.end(body);
      }, responseDelayMs);
    });
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("model fixture did not bind a TCP port");
  }
  return {
    server,
    endpoint: `http://127.0.0.1:${address.port}/v1/chat/completions`,
  };
}

async function stopServer(server) {
  if (!server) return;
  await new Promise((resolve) => server.close(resolve));
}

async function startMcpHttpFixture(expectedCredential) {
  const requests = [];
  const available = {
    streamable_http: true,
    sse: true,
  };
  let sseResponse;
  const server = http.createServer((request, response) => {
    let requestBody = "";
    request.setEncoding("utf8");
    request.on("data", (chunk) => {
      requestBody += chunk;
    });
    request.on("end", () => {
      const transport = request.url?.startsWith("/sse") ? "sse" : "streamable_http";
      const authorized = request.headers.authorization === expectedCredential;
      let payload;
      try {
        payload = JSON.parse(requestBody);
      } catch {
        payload = {};
      }
      const rpcMethod = typeof payload.method === "string" ? payload.method : null;
      const entry = {
        transport,
        method: request.method,
        path: request.url,
        rpcMethod,
        authorized,
        accept: request.headers.accept ?? null,
        responseKind: null,
      };
      requests.push(entry);
      if (!authorized) {
        entry.responseKind = "unauthorized";
        const body = Buffer.from(
          JSON.stringify({ error: "missing Native Keyring credential" }),
          "utf8",
        );
        response.writeHead(401, {
          "Content-Type": "application/json",
          "Content-Length": body.length,
          Connection: "close",
        });
        response.end(body);
        return;
      }
      if (transport === "sse" && request.method === "GET" && request.url === "/sse") {
        if (!available.sse) {
          entry.responseKind = "unavailable";
          response.writeHead(503, { "Content-Length": 0, Connection: "close" });
          response.end();
          return;
        }
        sseResponse?.end();
        sseResponse = response;
        entry.responseKind = "legacy_sse_stream";
        response.writeHead(200, {
          "Content-Type": "text/event-stream",
          "Cache-Control": "no-cache",
          Connection: "keep-alive",
        });
        response.write("event: endpoint\ndata: /sse/messages\n\n");
        response.on("close", () => {
          if (sseResponse === response) sseResponse = undefined;
        });
        return;
      }
      if (
        request.method !== "POST" ||
        (transport === "sse" && request.url !== "/sse/messages")
      ) {
        entry.responseKind = "method_not_allowed";
        response.writeHead(405, { Connection: "close" });
        response.end();
        return;
      }
      if (!available[transport]) {
        entry.responseKind = "unavailable";
        const body = Buffer.from(JSON.stringify({ error: "fixture transport unavailable" }));
        response.writeHead(503, {
          "Content-Type": "application/json",
          "Content-Length": body.length,
          Connection: "close",
        });
        response.end(body);
        return;
      }
      if (payload.id === undefined || payload.id === null) {
        entry.responseKind = "accepted";
        response.writeHead(202, { "Content-Length": 0, Connection: "close" });
        response.end();
        return;
      }
      if (transport === "sse" && !sseResponse) {
        entry.responseKind = "stream_unavailable";
        response.writeHead(409, { "Content-Length": 0, Connection: "close" });
        response.end();
        return;
      }
      const serverId = transport === "sse" ? "native-debug-sse" : "native-debug-http";
      const resourceUri = `mcp://${serverId}/credential-status`;
      let result;
      switch (rpcMethod) {
        case "initialize":
          result = {
            protocolVersion: "2024-11-05",
            capabilities: { tools: {}, resources: {}, prompts: {} },
            serverInfo: { name: `${serverId}-fixture`, version: "1.0.0" },
          };
          break;
        case "tools/list":
          result = {
            tools: [
              {
                name: "credential_probe",
                description: `Authenticated ${transport} credential probe.`,
                inputSchema: { type: "object", properties: {} },
                annotations: {
                  readOnlyHint: true,
                  destructiveHint: false,
                  idempotentHint: true,
                  openWorldHint: false,
                },
              },
            ],
          };
          break;
        case "resources/list":
          result = {
            resources: [
              {
                uri: resourceUri,
                name: `${transport} credential status`,
                description: "Secret-free HTTP credential injection status.",
                mimeType: "application/json",
              },
            ],
          };
          break;
        case "prompts/list":
          result = {
            prompts: [
              {
                name: "credential_summary",
                description: `Summarizes the ${transport} credential probe.`,
                arguments: [],
              },
            ],
          };
          break;
        case "tools/call":
          result = {
            content: [{ type: "text", text: JSON.stringify({ authorized, transport }) }],
            isError: false,
          };
          break;
        case "resources/read":
          result = {
            contents: [
              {
                uri: resourceUri,
                mimeType: "application/json",
                text: JSON.stringify({ authorized, transport }),
              },
            ],
          };
          break;
        case "prompts/get":
          result = {
            messages: [
              {
                role: "user",
                content: { type: "text", text: `${transport} credential authorized` },
              },
            ],
          };
          break;
        default:
          result = {};
          break;
      }
      const message = Buffer.from(
        JSON.stringify({ jsonrpc: "2.0", id: payload.id, result }),
        "utf8",
      );
      if (transport === "sse") {
        const event = Buffer.concat([
          Buffer.from("event: message\ndata: ", "utf8"),
          message,
          Buffer.from("\n\n", "utf8"),
        ]);
        entry.responseKind = "legacy_sse_message";
        response.writeHead(202, { "Content-Length": 0, Connection: "close" });
        response.end();
        sseResponse.write(event);
        return;
      }
      entry.responseKind = "application/json";
      response.writeHead(200, {
        "Content-Type": "application/json",
        "Content-Length": message.length,
        Connection: "close",
      });
      response.end(message);
    });
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("MCP HTTP fixture did not bind a TCP port");
  }
  return {
    server,
    requests,
    setAvailable(transport, value) {
      available[transport] = value;
    },
    async close() {
      sseResponse?.end();
      await stopServer(server);
    },
    streamableEndpoint: `http://127.0.0.1:${address.port}/streamable`,
    sseEndpoint: `http://127.0.0.1:${address.port}/sse`,
  };
}

async function verifyAuthenticatedHttpMcp(address, options) {
  const initial = await request(address, { command: "observe" });
  assertSuccess(initial, `observe before ${options.transport} MCP verification`);
  assertEqual(initial.observation.page, "settings/extensions", `${options.transport} page`);
  const baseline = {
    revision: initial.observation.extensionsMcpRegistryRevision,
    editable: initial.observation.extensionsEditableMcpCount,
    enabled: initial.observation.extensionsEnabledMcpCount,
    active: initial.observation.extensionsActiveMcpCount,
    tools: initial.observation.extensionsMcpToolCount,
    resources: initial.observation.extensionsMcpResourceCount,
    prompts: initial.observation.extensionsMcpPromptCount,
    credentials: initial.observation.extensionsMcpCredentialCount,
    configuredCredentials: initial.observation.extensionsMcpConfiguredCredentialCount,
  };
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions.mcp.add",
    }),
    `open ${options.transport} MCP editor`,
  );
  let editor;
  for (let index = 0; index < options.transportClicks; index += 1) {
    editor = await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions.mcp.editor.transport",
    });
    assertSuccess(editor, `select ${options.transport} MCP transport`);
  }
  assertEqual(
    editor.observation.extensionsMcpEditorTransport,
    options.transport,
    `${options.transport} editor transport`,
  );
  for (const [targetId, text, label] of [
    ["native-preview.settings.extensions.mcp.editor.id", options.serverId, "server id"],
    ["native-preview.settings.extensions.mcp.editor.location", options.endpoint, "endpoint"],
    [
      "native-preview.settings.extensions.mcp.editor.credential-names",
      '["Authorization"]',
      "Keyring header name",
    ],
  ]) {
    assertSuccess(
      await request(address, { command: "input", targetId, text }),
      `enter ${options.transport} MCP ${label}`,
    );
  }
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions.mcp.editor.enabled",
    }),
    `create ${options.transport} MCP disabled`,
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: "native-preview.settings.extensions.mcp.editor.save",
    }),
    `persist ${options.transport} MCP`,
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      !observation.extensionsMcpEditorOpen &&
      observation.extensionsMcpRegistryRevision === baseline.revision + 1 &&
      observation.extensionsEditableMcpCount === baseline.editable + 1 &&
      observation.extensionsMcpCredentialCount === baseline.credentials + 1,
    30_000,
  );
  const registered = JSON.parse(fs.readFileSync(options.registryPath, "utf8"));
  const registeredServer = registered.servers.find(
    (server) => server.serverId === options.serverId,
  );
  if (
    registered.secretFree !== true ||
    registeredServer?.transport !== options.transport ||
    registeredServer?.url !== options.endpoint ||
    registeredServer?.enabled !== false ||
    JSON.stringify(registeredServer?.headerSecretNames) !== JSON.stringify(["Authorization"])
  ) {
    throw new Error(`${options.transport} MCP registry entry was not exact and secret-free`);
  }
  if (fs.readFileSync(options.registryPath, "utf8").includes(options.credential)) {
    throw new Error(`${options.transport} MCP credential leaked into the registry`);
  }
  const credentialTarget =
    `native-preview.settings.extensions.mcp.${options.serverId}.credential.header.Authorization`;
  assertSuccess(
    await request(address, {
      command: "input",
      targetId: credentialTarget,
      text: options.credential,
    }),
    `enter ${options.transport} MCP header credential`,
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `${credentialTarget}.save`,
    }),
    `save ${options.transport} MCP header credential`,
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpConfiguredCredentialCount ===
        baseline.configuredCredentials + 1,
    30_000,
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.mcp.${options.serverId}.toggle`,
    }),
    `enable ${options.transport} MCP`,
  );
  const active = await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpRegistryRevision === baseline.revision + 2 &&
      observation.extensionsEnabledMcpCount === baseline.enabled + 1 &&
      observation.extensionsActiveMcpCount === baseline.active + 1 &&
      observation.extensionsMcpToolCount === baseline.tools + 1 &&
      observation.extensionsMcpResourceCount === baseline.resources + 1 &&
      observation.extensionsMcpPromptCount === baseline.prompts + 1 &&
      observation.extensionsActivationErrorCount === 0,
    30_000,
  );
  const resourceTarget =
    `native-preview.settings.extensions.mcp.${options.serverId}.resource.${options.resourceUri}.read`;
  assertTarget(active, resourceTarget);
  assertSuccess(
    await request(address, { command: "click", targetId: resourceTarget }),
    `read ${options.transport} MCP resource`,
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpContentKind === "resource" &&
      observation.extensionsMcpContentTitle === options.resourceUri &&
      observation.extensionsMcpContentText === options.expectedContent,
    30_000,
  );
  const transportRequests = options.fixture.requests.filter(
    (entry) => entry.transport === options.transport,
  );
  const requiredMethods = [
    "initialize",
    "tools/list",
    "resources/list",
    "prompts/list",
    "resources/read",
  ];
  if (
    requiredMethods.some(
      (method) => !transportRequests.some((entry) => entry.rpcMethod === method),
    ) ||
    transportRequests.some((entry) => entry.authorized !== true) ||
    transportRequests
      .filter(
        (entry) =>
          entry.rpcMethod && entry.rpcMethod !== "notifications/initialized",
      )
      .some((entry) => entry.responseKind !== options.expectedResponseKind)
  ) {
    throw new Error(`${options.transport} MCP did not use the authenticated real transport`);
  }
  if (
    options.transport === "sse" &&
    !transportRequests.some(
      (entry) =>
        entry.method === "GET" &&
        entry.path === "/sse" &&
        entry.responseKind === "legacy_sse_stream",
    )
  ) {
    throw new Error("legacy SSE MCP did not negotiate a GET event stream endpoint");
  }
  if (options.verifyRecovery) {
    options.fixture.setAvailable(options.transport, false);
    assertSuccess(
      await request(address, { command: "click", targetId: resourceTarget }),
      `start failed ${options.transport} MCP resource request`,
    );
    await waitForObservation(
      address,
      (observation) =>
        !observation.extensionsBusy &&
        typeof observation.extensionsError === "string" &&
        observation.extensionsError.length > 0,
      30_000,
    );
    assertSuccess(
      await request(address, {
        command: "click",
        targetId: "native-preview.settings.extensions.refresh",
      }),
      `refresh failed ${options.transport} MCP state`,
    );
    await waitForObservation(
      address,
      (observation) =>
        !observation.extensionsBusy &&
        observation.extensionsActiveMcpCount === baseline.active,
      30_000,
    );
    options.fixture.setAvailable(options.transport, true);
    assertSuccess(
      await request(address, {
        command: "click",
        targetId: "native-preview.settings.extensions.activate-mcp",
      }),
      `recover ${options.transport} MCP`,
    );
    await waitForObservation(
      address,
      (observation) =>
        !observation.extensionsBusy &&
        observation.extensionsActiveMcpCount === baseline.active + 1 &&
        observation.extensionsActivationErrorCount === options.expectedRecoveryErrors,
      30_000,
    );
    const recoveredRequests = options.fixture.requests.filter(
      (entry) => entry.transport === options.transport,
    );
    if (
      !recoveredRequests.some((entry) => entry.responseKind === "unavailable") ||
      recoveredRequests.filter((entry) => entry.rpcMethod === "initialize").length < 2
    ) {
      throw new Error(`${options.transport} MCP did not expose failure and reconnect cleanly`);
    }
  }
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.mcp.${options.serverId}.toggle`,
    }),
    `disable ${options.transport} MCP`,
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpRegistryRevision === baseline.revision + 3 &&
      observation.extensionsActiveMcpCount === baseline.active,
    30_000,
  );
  const requestedDelete = await request(address, {
    command: "click",
    targetId: `native-preview.settings.extensions.mcp.${options.serverId}.delete`,
  });
  assertSuccess(requestedDelete, `request ${options.transport} MCP deletion`);
  assertEqual(
    requestedDelete.observation.extensionsMcpDeleteConfirmation,
    options.serverId,
    `${options.transport} deletion confirmation`,
  );
  assertSuccess(
    await request(address, {
      command: "click",
      targetId: `native-preview.settings.extensions.mcp.${options.serverId}.delete-confirm`,
    }),
    `confirm ${options.transport} MCP deletion`,
  );
  await waitForObservation(
    address,
    (observation) =>
      !observation.extensionsBusy &&
      observation.extensionsMcpRegistryRevision === baseline.revision + 4 &&
      observation.extensionsEditableMcpCount === baseline.editable &&
      observation.extensionsMcpCredentialCount === baseline.credentials &&
      observation.extensionsMcpConfiguredCredentialCount === baseline.configuredCredentials,
    30_000,
  );
  const deleted = JSON.parse(fs.readFileSync(options.registryPath, "utf8"));
  if (
    deleted.secretFree !== true ||
    deleted.servers.some((server) => server.serverId === options.serverId) ||
    fs.readFileSync(options.registryPath, "utf8").includes(options.credential)
  ) {
    throw new Error(`${options.transport} MCP delete violated the secret-free registry`);
  }
}

function initializeGitWorkspace(workspace) {
  fs.writeFileSync(path.join(workspace, "README.md"), "Native Agent Debug\n", "utf8");
  const commands = [
    ["init", "-b", "main"],
    ["config", "user.email", "native-agent-debug@example.invalid"],
    ["config", "user.name", "Native Agent Debug"],
    ["config", "core.autocrlf", "false"],
    ["add", "README.md"],
    ["commit", "-m", "native agent debug fixture"],
  ];
  for (const args of commands) {
    const result = spawnSync("git", args, {
      cwd: workspace,
      encoding: "utf8",
      timeout: 30_000,
      windowsHide: true,
      env: { ...process.env, GIT_TERMINAL_PROMPT: "0" },
    });
    if (result.error) throw result.error;
    if (result.status !== 0) {
      throw new Error(`git ${args.join(" ")} failed: ${result.stderr || result.stdout}`);
    }
  }
}

async function waitForObservation(address, predicate, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let latest;
  while (Date.now() < deadline) {
    latest = await request(address, { command: "observe" });
    assertSuccess(latest, "observe task progress");
    if (predicate(latest.observation)) return latest;
    await delay(100);
  }
  throw new Error(`timed out waiting for Native observation: ${JSON.stringify(latest)}`);
}

function appendTranscript(direction, payload) {
  fs.appendFileSync(
    transcriptPath,
    `${JSON.stringify({ at: new Date().toISOString(), direction, payload })}\n`,
    "utf8",
  );
}

function assertSuccess(response, step) {
  if (!response?.ok) {
    throw new Error(`${step} failed: ${JSON.stringify(response)}`);
  }
}

function assertEqual(actual, expected, label) {
  if (actual !== expected) {
    throw new Error(`${label}: expected ${expected}, received ${actual}`);
  }
}

function assertIncludes(values, expected, label) {
  if (!values.includes(expected)) {
    throw new Error(`${label}: expected ${expected} to be present`);
  }
}

function assertNotIncludes(values, expected, label) {
  if (values.includes(expected)) {
    throw new Error(`${label}: expected ${expected} to be absent`);
  }
}

function assertTarget(response, targetId) {
  if (!response.observation.visibleTargetIds.includes(targetId)) {
    throw new Error(`target is not visible: ${targetId}`);
  }
}

function assertProjectWorkspaceItem(response, surface) {
  const kind = `project-${surface}`;
  const itemId = `${kind}:${debugProjectId}`;
  const item = response.observation.workspaceItems.find((candidate) => candidate.id === itemId);
  if (!item) {
    throw new Error(`Native ${surface} workspace item is missing: ${itemId}`);
  }
  assertEqual(item.kind, kind, `${surface} workspace item kind`);
  assertEqual(item.resourceId, itemId, `${surface} workspace resource identity`);
  assertEqual(item.closable, true, `${surface} workspace closable capability`);
  assertEqual(item.splittable, true, `${surface} workspace splittable capability`);
  assertEqual(item.movableAcrossWindows, true, `${surface} workspace window capability`);
  assertEqual(item.persistent, true, `${surface} workspace persistence capability`);
  assertIncludes(
    response.observation.activeWorkspaceItemIds,
    itemId,
    `${surface} active workspace item`,
  );
  assertTarget(response, `native-preview.workspace.tab.${itemId}`);
  return itemId;
}

function assertApplicationWorkspaceItem(response, kind, itemId) {
  const item = response.observation.workspaceItems.find((candidate) => candidate.id === itemId);
  if (!item) {
    throw new Error(`Native application workspace item is missing: ${itemId}`);
  }
  assertEqual(item.kind, kind, "application workspace item kind");
  assertEqual(item.resourceId, itemId, "application workspace resource identity");
  assertEqual(item.closable, true, "application workspace closable capability");
  assertEqual(item.splittable, true, "application workspace splittable capability");
  assertEqual(item.movableAcrossWindows, true, "application workspace window capability");
  assertEqual(item.persistent, true, "application workspace persistence capability");
  assertIncludes(
    response.observation.activeWorkspaceItemIds,
    itemId,
    "application active workspace item",
  );
  assertTarget(response, `native-preview.workspace.tab.${itemId}`);
  return itemId;
}

function closeWindowByTitle(processId, title) {
  const closeScript = String.raw`
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class NativeWindowCloser {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")]
  public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")]
  public static extern bool PostMessage(IntPtr hWnd, uint message, IntPtr wParam, IntPtr lParam);
  public static IntPtr Find(int processId, string exactTitle) {
    IntPtr result = IntPtr.Zero;
    EnumWindows((hWnd, _) => {
      uint owner;
      GetWindowThreadProcessId(hWnd, out owner);
      if (owner != processId || !IsWindowVisible(hWnd)) return true;
      int length = GetWindowTextLength(hWnd);
      var text = new StringBuilder(length + 1);
      GetWindowText(hWnd, text, text.Capacity);
      if (!string.Equals(text.ToString(), exactTitle, StringComparison.Ordinal)) return true;
      result = hWnd;
      return false;
    }, IntPtr.Zero);
    return result;
  }
}
"@
$handle = [NativeWindowCloser]::Find([int]$env:LILIA_NATIVE_CLOSE_PID, $env:LILIA_NATIVE_CLOSE_TITLE)
if ($handle -eq [IntPtr]::Zero) { throw "target Native window was not found" }
if (-not [NativeWindowCloser]::PostMessage($handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) {
  throw "failed to post WM_CLOSE"
}
`;
  const result = spawnSync(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", closeScript],
    {
      cwd: repoRoot,
      encoding: "utf8",
      timeout: 15_000,
      windowsHide: true,
      env: {
        ...process.env,
        LILIA_NATIVE_CLOSE_PID: String(processId),
        LILIA_NATIVE_CLOSE_TITLE: title,
      },
    },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`failed to close Native window: ${result.stderr || result.stdout}`);
  }
}

function setClipboardText(value) {
  const result = spawnSync(
    "powershell.exe",
    [
      "-NoProfile",
      "-NonInteractive",
      "-STA",
      "-Command",
      "$value = [Environment]::GetEnvironmentVariable('LILIA_NATIVE_CLIPBOARD_TEXT'); $last = $null; for ($attempt = 0; $attempt -lt 20; $attempt++) { try { Set-Clipboard -Value $value -ErrorAction Stop; exit 0 } catch { $last = $_; Start-Sleep -Milliseconds 100 } }; throw $last",
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
      timeout: 15_000,
      windowsHide: true,
      env: {
        ...process.env,
        LILIA_NATIVE_CLIPBOARD_TEXT: value,
      },
    },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`failed to seed Windows clipboard: ${result.stderr || result.stdout}`);
  }
}

async function captureRenderedWindow(processId, outputPath) {
  const deadline = Date.now() + 10_000;
  let lastError;
  do {
    try {
      return captureWindow(processId, outputPath);
    } catch (error) {
      lastError = error;
      await delay(500);
    }
  } while (Date.now() < deadline);
  throw lastError;
}

async function recordRenderedWindow(processId, outputPath, surface, boundsKey, pngSizeKey) {
  if (summary.screenshotGateErrors.length > 0) {
    summary.screenshotSkippedSurfaces.push(surface);
    return null;
  }
  try {
    const windowBounds = await captureRenderedWindow(processId, outputPath);
    if (!fs.existsSync(outputPath) || fs.statSync(outputPath).size === 0) {
      throw new Error("window screenshot was not created");
    }
    const pngSize = readPngSize(outputPath);
    if (pngSize.width !== windowBounds.width || pngSize.height !== windowBounds.height) {
      throw new Error(
        `screenshot geometry mismatch: PNG ${pngSize.width}x${pngSize.height}, ` +
          `Win32 ${windowBounds.width}x${windowBounds.height}`,
      );
    }
    summary[boundsKey] = windowBounds;
    summary[pngSizeKey] = pngSize;
    return { windowBounds, pngSize };
  } catch (error) {
    summary.screenshotGateErrors.push({
      surface,
      outputPath,
      error: error instanceof Error ? error.message : String(error),
    });
    return null;
  }
}

function captureWindow(processId, outputPath) {
  const captureScript = String.raw`
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class NativePreviewWindow {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [StructLayout(LayoutKind.Sequential)]
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")]
  public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")]
  public static extern bool ShowWindow(IntPtr hWnd, int command);
  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")]
  public static extern bool SetWindowPos(IntPtr hWnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")]
  public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr dpiContext);
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
$previousDpiContext = [NativePreviewWindow]::SetThreadDpiAwarenessContext([IntPtr](-4))
if ($previousDpiContext -eq [IntPtr]::Zero) { throw "failed to enable per-monitor DPI awareness" }
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
$targetProcessId = [int]$env:LILIA_NATIVE_CAPTURE_PID
$deadline = [DateTime]::UtcNow.AddSeconds(15)
$handle = [IntPtr]::Zero
while ([DateTime]::UtcNow -lt $deadline) {
  $handle = [NativePreviewWindow]::Find($targetProcessId, "LiliaCode Native Preview")
  if ($handle -ne [IntPtr]::Zero) { break }
  Start-Sleep -Milliseconds 100
}
if ($handle -eq [IntPtr]::Zero) { throw "Native Preview has no visible titled window" }
[void][NativePreviewWindow]::ShowWindow($handle, 9)
[void][NativePreviewWindow]::SetWindowPos($handle, [IntPtr](-1), 0, 0, 0, 0, 0x0043)
[void][NativePreviewWindow]::SetForegroundWindow($handle)
$shell = New-Object -ComObject WScript.Shell
[void]$shell.AppActivate($targetProcessId)
Start-Sleep -Milliseconds 800
$rect = New-Object NativePreviewWindow+RECT
if (-not [NativePreviewWindow]::GetWindowRect($handle, [ref]$rect)) { throw "GetWindowRect failed" }
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
if ($width -le 0 -or $height -le 0) { throw "Native Preview window has invalid geometry" }
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
  $bitmap.Save($env:LILIA_NATIVE_CAPTURE_PATH, [System.Drawing.Imaging.ImageFormat]::Png)
} finally {
  $graphics.Dispose()
  $bitmap.Dispose()
  [void][NativePreviewWindow]::SetWindowPos($handle, [IntPtr](-2), 0, 0, 0, 0, 0x0043)
}
[pscustomobject]@{ left = $rect.Left; top = $rect.Top; width = $width; height = $height; nonBlackSamples = $nonBlackSamples; neutralSamples = $neutralSamples } | ConvertTo-Json -Compress
`;
  const result = spawnSync(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", captureScript],
    {
      cwd: repoRoot,
      encoding: "utf8",
      timeout: 30_000,
      windowsHide: true,
      env: {
        ...process.env,
        LILIA_NATIVE_CAPTURE_PID: String(processId),
        LILIA_NATIVE_CAPTURE_PATH: outputPath,
      },
    },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`window capture failed: ${result.stderr || result.stdout}`);
  }
  const output = result.stdout.trim().split(/\r?\n/).at(-1);
  const capture = JSON.parse(output);
  if (!Number.isInteger(capture.nonBlackSamples) || capture.nonBlackSamples < 8) {
    throw new Error(
      `window capture contains no visible GPU content: ${capture.nonBlackSamples ?? "unknown"} samples`,
    );
  }
  if (!Number.isInteger(capture.neutralSamples) || capture.neutralSamples < 32) {
    throw new Error(
      `window capture does not contain the expected Native UI surface: ${capture.neutralSamples ?? "unknown"} neutral samples`,
    );
  }
  return capture;
}

function readPngSize(file) {
  const header = fs.readFileSync(file).subarray(0, 24);
  if (header.length < 24 || header.toString("ascii", 1, 4) !== "PNG") {
    throw new Error("window screenshot is not a PNG file");
  }
  return { width: header.readUInt32BE(16), height: header.readUInt32BE(20) };
}

async function stopChild(processHandle) {
  if (!processHandle || processHandle.exitCode !== null) return;
  processHandle.kill();
  await Promise.race([new Promise((resolve) => processHandle.once("exit", resolve)), delay(2_000)]);
  if (processHandle.exitCode === null) {
    spawnSync("taskkill.exe", ["/PID", String(processHandle.pid), "/T", "/F"], {
      windowsHide: true,
      stdio: "ignore",
    });
  }
}

async function restartPreviewProcess(processHandle, environment) {
  await stopChild(processHandle);
  fs.rmSync(readyPath, { force: true });
  const restarted = spawn(executable, [], {
    cwd: repoRoot,
    env: environment,
    stdio: ["ignore", stdoutFd, stderrFd],
    windowsHide: true,
  });
  await waitForReady(restarted, readyPath, 30_000);
  return {
    child: restarted,
    address: fs.readFileSync(readyPath, "utf8").trim(),
  };
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

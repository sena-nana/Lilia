import { isRecord, stringOrNull } from "../utils.mjs";

const EDIT_EXEC_OUTPUT_LIMIT = 6000;
const EDIT_EXEC_WAIT_MS = 1000 * 60 * 30;

function trimOutput(value, limit = EDIT_EXEC_OUTPUT_LIMIT) {
  const text = typeof value === "string" ? value : "";
  if (text.length <= limit) return text;
  return `${text.slice(0, limit)}\n...输出已截断，完整输出过长`;
}

function numberOrNull(value) {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

export function normalizeEditedCommandExecResult(result, fallback = {}) {
  const row = isRecord(result) ? result : {};
  const nested = isRecord(row.result) ? row.result : {};
  const output = stringOrNull(row.output) ||
    stringOrNull(row.text) ||
    stringOrNull(row.content) ||
    stringOrNull(nested.output) ||
    stringOrNull(fallback.output) ||
    "";
  const stdout = stringOrNull(row.stdout) ||
    stringOrNull(nested.stdout) ||
    stringOrNull(fallback.stdout) ||
    output;
  const stderr = stringOrNull(row.stderr) ||
    stringOrNull(nested.stderr) ||
    stringOrNull(fallback.stderr) ||
    "";
  const exitCode = numberOrNull(row.exitCode) ??
    numberOrNull(row.exit_code) ??
    numberOrNull(row.code) ??
    numberOrNull(nested.exitCode) ??
    numberOrNull(nested.exit_code) ??
    numberOrNull(fallback.exitCode) ??
    0;
  return {
    exitCode,
    stdout: trimOutput(stdout),
    stderr: trimOutput(stderr),
    output: trimOutput(output || [stdout, stderr].filter(Boolean).join("\n")),
  };
}

function processIdFromSpawnResult(result) {
  if (!isRecord(result)) return null;
  return stringOrNull(result.processId) ||
    stringOrNull(result.processID) ||
    stringOrNull(result.pid) ||
    stringOrNull(result.id);
}

function notificationMethod(msg) {
  return stringOrNull(msg?.method) || stringOrNull(msg?.type) || "";
}

function notificationParams(msg) {
  return isRecord(msg?.params) ? msg.params : isRecord(msg) ? msg : {};
}

function outputDeltaFromNotification(msg) {
  const params = notificationParams(msg);
  return stringOrNull(params.delta) ||
    stringOrNull(params.text) ||
    stringOrNull(params.chunk) ||
    stringOrNull(params.output) ||
    "";
}

function notificationProcessId(msg) {
  const params = notificationParams(msg);
  return stringOrNull(params.processId) ||
    stringOrNull(params.processID) ||
    stringOrNull(params.pid) ||
    stringOrNull(params.id);
}

function notificationMatchesProcess(msg, processId) {
  if (!processId) return true;
  const id = notificationProcessId(msg);
  return !id || id === processId;
}

async function waitForSpawnedProcess(server, processId) {
  const startedAt = Date.now();
  let stdout = "";
  let stderr = "";
  while (Date.now() - startedAt < EDIT_EXEC_WAIT_MS) {
    for (const msg of server.drainNotifications?.() || []) {
      const method = notificationMethod(msg);
      if (!method.startsWith("process/") || !notificationMatchesProcess(msg, processId)) {
        continue;
      }
      if (method === "process/outputDelta") {
        const params = notificationParams(msg);
        const delta = outputDeltaFromNotification(msg);
        if (params.stream === "stderr" || params.fd === 2) stderr += delta;
        else stdout += delta;
        stdout = trimOutput(stdout);
        stderr = trimOutput(stderr);
      }
      if (method === "process/exited") {
        const params = notificationParams(msg);
        return normalizeEditedCommandExecResult({
          exitCode: numberOrNull(params.exitCode) ?? numberOrNull(params.code) ?? 0,
          stdout,
          stderr,
        });
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  throw new Error("Edited Codex command execution timed out");
}

export async function executeEditedCodexCommand(server, edit, cwd, permissionProfile = null) {
  const params = {
    command: edit.modifiedCommand,
    cwd: cwd || undefined,
  };
  if (permissionProfile) params.permissionProfile = permissionProfile;
  try {
    return normalizeEditedCommandExecResult(await server.request("command/exec", params));
  } catch (commandExecError) {
    try {
      const spawned = await server.request("process/spawn", params);
      return await waitForSpawnedProcess(server, processIdFromSpawnResult(spawned));
    } catch (spawnError) {
      const err = new Error(spawnError?.message || commandExecError?.message || "Edited Codex command execution failed");
      err.commandExecError = commandExecError;
      err.spawnError = spawnError;
      throw err;
    }
  }
}

export function emitLiliaEditedCommandTimeline(ctx, payload, edit, result, status = "success", message = "") {
  const exitCode = numberOrNull(result?.exitCode);
  const summary = message ||
    (exitCode === 0 ? "Lilia 已执行用户修改后的命令" : `Lilia 执行用户修改后的命令失败，退出码 ${exitCode ?? "unknown"}`);
  ctx.protocol.emitTimeline({
    kind: "command",
    status,
    title: edit.modifiedCommand,
    summary,
    payload: {
      backend: "codex",
      executionOwner: "lilia",
      subkind: "lilia_edit_exec",
      originalCommand: edit.originalCommand,
      modifiedCommand: edit.modifiedCommand,
      command: edit.modifiedCommand,
      cwd: stringOrNull(payload?.cwd),
      exitCode,
      stdout: stringOrNull(result?.stdout),
      stderr: stringOrNull(result?.stderr),
      output: stringOrNull(result?.output),
    },
    sourceId: `${payload.toolUseID || "codex-command"}:lilia-edit-exec`,
  });
}

function commandFence(command) {
  let fence = "```";
  while (command.includes(fence)) fence += "`";
  return fence;
}

export function buildEditedCommandAdditionalContext(edit, result) {
  const fence = commandFence(edit.modifiedCommand);
  const output = trimOutput(result?.output || [result?.stdout, result?.stderr].filter(Boolean).join("\n"), 4000);
  return [
    "用户编辑了 Codex 请求审批的命令，Lilia 已执行编辑后的命令；不要再执行原始命令。",
    "",
    "原始命令：",
    `${fence}shell`,
    edit.originalCommand,
    fence,
    "",
    "编辑后由 Lilia 执行的命令：",
    `${fence}shell`,
    edit.modifiedCommand,
    fence,
    "",
    `退出码：${numberOrNull(result?.exitCode) ?? "unknown"}`,
    output ? `输出摘要：\n${output}` : "输出摘要：无输出",
  ].join("\n");
}

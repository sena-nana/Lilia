// #47 LEGACY — Official Codex app-server launcher (compat until product 1.0.0).
// Must NOT be packaged in default Desktop install resources.
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";

const DEFAULT_HOME_SUBDIR = ".lilia";
const REDIRECT_FILE = ".redirect";

function trimmedString(value) {
  return typeof value === "string" ? value.trim() : "";
}

function resolveLiliaHome({
  env = process.env,
  homeDir = homedir(),
  fileExists = existsSync,
  readTextFile = readFileSync,
} = {}) {
  const envHome = trimmedString(env.LILIA_HOME);
  if (envHome) return envHome;

  const defaultHome = join(homeDir || ".", DEFAULT_HOME_SUBDIR);
  const redirect = join(defaultHome, REDIRECT_FILE);
  if (fileExists(redirect)) {
    try {
      const redirected = trimmedString(readTextFile(redirect, "utf8"));
      if (redirected) return redirected;
    } catch {
      // Ignore unreadable redirect files and fall back to the default home.
    }
  }
  return defaultHome;
}

function codexCandidateFilenames(platform = process.platform) {
  return platform === "win32"
    ? ["codex.cmd", "codex.exe", "codex.bat", "codex"]
    : ["codex"];
}

export function codexAppServerBinary({
  env = process.env,
  platform = process.platform,
  homeDir = homedir(),
  fileExists = existsSync,
  readTextFile = readFileSync,
} = {}) {
  const installDir = join(
    resolveLiliaHome({ env, homeDir, fileExists, readTextFile }),
    "runtime",
    "codex",
    "bin",
  );
  for (const filename of codexCandidateFilenames(platform)) {
    const candidate = join(installDir, filename);
    if (fileExists(candidate)) return candidate;
  }
  throw new Error(
    "Codex app-server 环境不满足：Lilia 内置 Codex CLI 未安装或不可用。请在 Provider 设置中安装或更新 Codex app-server。",
  );
}

export function isWindowsCommandScript(binary, platform = process.platform) {
  return platform === "win32" && /\.(cmd|bat)$/i.test(binary);
}

export function resolveWindowsCommandScript(
  binary,
  {
    platform = process.platform,
    fileExists = existsSync,
  } = {},
) {
  if (platform !== "win32") return null;
  if (isWindowsCommandScript(binary, platform)) return binary;
  if (/\.[^\\/]+$/i.test(binary)) return null;
  for (const candidate of [`${binary}.cmd`, `${binary}.bat`]) {
    if (fileExists(candidate)) return candidate;
  }
  return null;
}

export function windowsCommandLineToken(value) {
  const text = String(value);
  if (!/[\s"&|<>^]/.test(text)) return text;
  return `"${text.replace(/"/g, '\\"')}"`;
}

export function windowsCommandLine(binary, args) {
  return [binary, ...args].map(windowsCommandLineToken).join(" ");
}

export function codexAppServerSpawnCommand(
  binary,
  {
    env = process.env,
    platform = process.platform,
    fileExists = existsSync,
  } = {},
) {
  const commandScript = resolveWindowsCommandScript(binary, { platform, fileExists });
  if (!commandScript) return { command: binary, args: ["app-server"] };
  return {
    command: env.ComSpec || "cmd.exe",
    args: [
      "/d",
      "/s",
      "/c",
      windowsCommandLine(commandScript, ["app-server"]),
    ],
  };
}

export function spawnCodexAppServer(binary, options) {
  const { command, args } = codexAppServerSpawnCommand(binary);
  return spawn(command, args, options);
}

export function codexAppServerExitError(stderr, code, signal) {
  const status = signal ? `signal ${signal}` : `code ${code ?? "unknown"}`;
  const detail = stderr.trim();
  if (detail) return new Error(`Codex app-server exited (${status}): ${detail}`);
  return new Error(
    `Codex app-server exited (${status})，但没有输出 stderr；请检查 Codex CLI 配置或认证状态。`,
  );
}

export function codexAppServerClosedError() {
  return new Error("Codex app-server request was cancelled because Lilia closed the app-server.");
}

export async function initializeCodexAppServer(server) {
  await server.request("initialize", {
    clientInfo: {
      name: "lilia",
      title: "LiliaCode",
      version: "0.1.0",
    },
    capabilities: { experimentalApi: true },
  });
  server.notify("initialized", {});
}

export function createCodexAppServer({
  env = process.env,
  resolveBinary = codexAppServerBinary,
  spawnServer = spawnCodexAppServer,
} = {}) {
  const binary = resolveBinary({ env });
  const child = spawnServer(binary, {
    stdio: ["pipe", "pipe", "pipe"],
    env: {
      ...env,
      ...(env.OPENAI_BASE_URL ? { OPENAI_BASE_URL: env.OPENAI_BASE_URL } : {}),
      ...(env.OPENAI_API_KEY ? { CODEX_API_KEY: env.OPENAI_API_KEY } : {}),
    },
  });
  const rl = createInterface({ input: child.stdout, crlfDelay: Infinity });
  const pending = new Map();
  const notifications = [];
  let seq = 1;
  let stderr = "";
  let closed = false;
  let closeRequested = false;
  let closeTimer = null;
  let closeForceKillMs = 30_000;
  let shutdownStarted = false;
  child.stderr?.on("data", (chunk) => {
    stderr += chunk.toString("utf8");
  });
  child.once("error", (err) => {
    closed = true;
    clearCloseTimer();
    for (const { reject } of pending.values()) {
      reject(err);
    }
    pending.clear();
  });
  child.once("exit", (code, signal) => {
    closed = true;
    if (closeRequested) {
      return;
    }
    for (const { reject } of pending.values()) {
      reject(codexAppServerExitError(stderr, code, signal));
    }
    pending.clear();
  });
  child.once("close", () => {
    closed = true;
    clearCloseTimer();
    if (closeRequested) rejectPendingRequests();
  });
  rl.on("line", (line) => {
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      return;
    }
    if (Object.prototype.hasOwnProperty.call(msg, "id")) {
      const entry = pending.get(msg.id);
      if (!entry) {
        notifications.push(msg);
        return;
      }
      pending.delete(msg.id);
      if (msg.error) entry.reject(new Error(msg.error.message || "Codex app-server request failed"));
      else entry.resolve(msg.result ?? null);
      if (closeRequested && pending.size === 0) shutdown({ forceKillMs: closeForceKillMs });
      return;
    }
    notifications.push(msg);
  });
  function request(method, params = null) {
    if (closed || !child.stdin) {
      return Promise.reject(new Error("Codex app-server is not running"));
    }
    const id = seq++;
    const payload = { method, id, params: params ?? null };
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      child.stdin.write(`${JSON.stringify(payload)}\n`);
    });
  }
  function notify(method, params = {}) {
    if (!closed && child.stdin) child.stdin.write(`${JSON.stringify({ method, params })}\n`);
  }
  function respond(id, result) {
    if (!closed && child.stdin) child.stdin.write(`${JSON.stringify({ id, result })}\n`);
  }
  function drainNotifications() {
    return notifications.splice(0, notifications.length);
  }

  function clearCloseTimer() {
    if (!closeTimer) return;
    clearTimeout(closeTimer);
    closeTimer = null;
  }

  function rejectPendingRequests() {
    for (const { reject } of pending.values()) {
      reject(codexAppServerClosedError());
    }
    pending.clear();
  }

  function shutdown({ forceKillMs = 30_000, cancelPending = false } = {}) {
    if (shutdownStarted) return;
    shutdownStarted = true;
    closeRequested = true;
    closed = true;
    clearCloseTimer();
    if (cancelPending) rejectPendingRequests();
    try {
      child.stdin?.end?.();
    } catch {
      // noop
    }
    if (forceKillMs <= 0) {
      try {
        child.kill();
      } catch {
        // noop
      }
      return;
    }
    closeTimer = setTimeout(() => {
      if (pending.size > 0) rejectPendingRequests();
      try {
        child.kill();
      } catch {
        // noop
      }
    }, forceKillMs);
    closeTimer.unref?.();
  }

  function close({ forceKillMs = 30_000 } = {}) {
    if (closeRequested) return;
    closeRequested = true;
    closed = true;
    closeForceKillMs = forceKillMs;
    if (pending.size === 0 || forceKillMs <= 0) {
      shutdown({ forceKillMs, cancelPending: pending.size > 0 });
    } else {
      closeTimer = setTimeout(() => {
        shutdown({ forceKillMs: 0, cancelPending: true });
      }, forceKillMs);
      closeTimer.unref?.();
    }
  }
  return { binary, child, request, notify, respond, drainNotifications, close };
}

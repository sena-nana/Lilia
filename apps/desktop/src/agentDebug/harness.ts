import type { Router } from "vue-router";
import {
  AGENT_DEBUG_EQUIVALENCE_SNAPSHOT_COMMAND,
  AGENT_DEBUG_RECORD_ACTION_COMMAND,
  type AgentDebugAction,
  type AgentDebugElement,
  type AgentDebugElementRole,
  type AgentDebugErrorEntry,
  type AgentDebugInvokeEntry,
  type AgentDebugLogEntry,
  type AgentDebugMissingElement,
  type AgentDebugSnapshot,
} from "@lilia/contracts";
import { invoke as rawInvoke } from "@tauri-apps/api/core";

type AgentDebugWindow = Window & {
  __liliaAgentDebug?: {
    observe: () => AgentDebugSnapshot;
    act: (action: AgentDebugAction) => Promise<AgentDebugSnapshot>;
    mark: (label: string, data?: unknown) => Promise<AgentDebugSnapshot>;
    getRecentErrors: () => AgentDebugErrorEntry[];
    equivalenceSnapshot: (fixtureId: string) => Promise<unknown>;
  };
};

const MAX_ERRORS = 80;
const MAX_INVOKES = 160;
const MAX_LOGS = 240;
const INTERACTIVE_SELECTOR = [
  "button",
  "input",
  "textarea",
  "select",
  "a[href]",
  "[role='button']",
  "[role='menuitem']",
  "[role='menuitemradio']",
  "[role='menuitemcheckbox']",
  "[role='tab']",
  "[role='checkbox']",
  "[role='radio']",
  "[role='switch']",
  "[contenteditable='true']",
].join(",");
const errors: AgentDebugErrorEntry[] = [];
const invokes: AgentDebugInvokeEntry[] = [];
const logs: AgentDebugLogEntry[] = [];
let installed = false;
let routerRef: Router | null = null;
let seq = 0;

export function isAgentDebugFrontendEnabled(): boolean {
  return import.meta.env.VITE_LILIA_AGENT_DEBUG === "1" &&
    (import.meta.env.DEV || import.meta.env.MODE === "agent-debug");
}

function nextId(prefix: string): string {
  seq += 1;
  return `${prefix}-${Date.now()}-${seq}`;
}

function keepBounded<T>(items: T[], max: number): void {
  if (items.length > max) items.splice(0, items.length - max);
}

function stringifyError(value: unknown): { message: string; stack: string | null } {
  if (value instanceof Error) {
    return { message: value.message || String(value), stack: value.stack ?? null };
  }
  if (typeof value === "string") return { message: value, stack: null };
  try {
    return { message: JSON.stringify(value), stack: null };
  } catch {
    return { message: String(value), stack: null };
  }
}

function pushError(kind: AgentDebugErrorEntry["kind"], value: unknown): void {
  if (!isAgentDebugFrontendEnabled()) return;
  const normalized = stringifyError(value);
  errors.push({
    id: nextId("error"),
    kind,
    message: normalized.message,
    stack: normalized.stack,
    createdAt: Date.now(),
  });
  keepBounded(errors, MAX_ERRORS);
}

function summarize(value: unknown): unknown {
  if (value == null) return value;
  if (typeof value === "string") {
    return value.length > 240 ? `${value.slice(0, 240)}...` : value;
  }
  if (typeof value === "number" || typeof value === "boolean") return value;
  if (Array.isArray(value)) return { type: "array", length: value.length };
  if (typeof value === "object") {
    const keys = Object.keys(value).slice(0, 12);
    return { type: "object", keys };
  }
  return String(value);
}

function pushLog(entry: Omit<AgentDebugLogEntry, "id" | "createdAt">): AgentDebugLogEntry {
  const saved = {
    id: nextId("log"),
    createdAt: Date.now(),
    ...entry,
  };
  logs.push(saved);
  keepBounded(logs, MAX_LOGS);
  return saved;
}

async function recordBackendLog(entry: AgentDebugLogEntry): Promise<void> {
  if (!isAgentDebugFrontendEnabled()) return;
  try {
    await rawInvoke(AGENT_DEBUG_RECORD_ACTION_COMMAND, { entry });
  } catch {
    // The Rust command is dev-only; frontend collection remains useful without it.
  }
}

function safeCssEscape(value: string): string {
  // jsdom's CSS.escape is not a free function; always use a local escaper.
  return value.replace(/["\\]/g, "\\$&");
}

function selectorFor(id: string): string {
  return `[data-agent-id="${safeCssEscape(id)}"]`;
}

function cssEscape(value: string): string {
  return safeCssEscape(value);
}

function readableText(element: HTMLElement): string {
  return (
    element.innerText ||
    element.getAttribute("aria-label") ||
    element.getAttribute("title") ||
    element.getAttribute("placeholder") ||
    element.textContent ||
    ""
  ).trim().replace(/\s+/g, " ").slice(0, 240);
}

function diagnosticSelectorFor(element: HTMLElement): string {
  const parts: string[] = [];
  let cursor: HTMLElement | null = element;
  while (cursor && cursor !== document.body && parts.length < 4) {
    const tag = cursor.tagName.toLowerCase();
    const id = cursor.id ? `#${cssEscape(cursor.id)}` : "";
    const agentId = cursor.dataset.agentId ? `[data-agent-id="${cssEscape(cursor.dataset.agentId)}"]` : "";
    const classes = [...cursor.classList].slice(0, 3).map((item) => `.${cssEscape(item)}`).join("");
    let nth = "";
    if (!id && !agentId && cursor.parentElement) {
      const siblings = [...cursor.parentElement.children].filter((item) => item.tagName === cursor?.tagName);
      const index = siblings.indexOf(cursor) + 1;
      if (index > 1) nth = `:nth-of-type(${index})`;
    }
    parts.unshift(`${tag}${id}${agentId}${classes}${nth}`);
    if (id || agentId) break;
    cursor = cursor.parentElement;
  }
  return parts.join(" > ");
}

function readRole(element: HTMLElement): AgentDebugElementRole {
  const explicit = element.getAttribute("role");
  if (explicit === "button" || explicit === "menuitem" || explicit === "tab" ||
    explicit === "menuitemradio" || explicit === "menuitemcheckbox" ||
    explicit === "checkbox" || explicit === "radio" || explicit === "switch" ||
    explicit === "link" || explicit === "region") {
    return explicit;
  }
  const tag = element.tagName.toLowerCase();
  if (tag === "button") return "button";
  if (tag === "a") return "link";
  if (tag === "input") {
    const type = (element.getAttribute("type") ?? "text").toLowerCase();
    if (type === "checkbox") return "checkbox";
    if (type === "radio") return "radio";
    return "input";
  }
  if (tag === "textarea") return "textarea";
  if (tag === "select") return "select";
  if (element.isContentEditable) return "textarea";
  return "unknown";
}

function isVisible(element: HTMLElement, rect: DOMRect): boolean {
  const style = window.getComputedStyle(element);
  return rect.width > 0 &&
    rect.height > 0 &&
    style.visibility !== "hidden" &&
    style.display !== "none" &&
    Number.parseFloat(style.opacity || "1") > 0;
}

export function collectAgentElements(): AgentDebugElement[] {
  if (typeof document === "undefined") return [];
  return Array.from(document.querySelectorAll<HTMLElement>("[data-agent-id]"))
    .map((element) => {
      const id = element.dataset.agentId ?? "";
      const rect = element.getBoundingClientRect();
      return {
        id,
        role: readRole(element),
        text: readableText(element),
        tagName: element.tagName.toLowerCase(),
        selector: selectorFor(id),
        rect: {
          x: rect.x,
          y: rect.y,
          width: rect.width,
          height: rect.height,
        },
        visible: isVisible(element, rect),
        enabled: !element.hasAttribute("disabled") && element.getAttribute("aria-disabled") !== "true",
        checked: element.getAttribute("aria-checked") === null
          ? null
          : element.getAttribute("aria-checked") === "true",
        expanded: element.getAttribute("aria-expanded") === null
          ? null
          : element.getAttribute("aria-expanded") === "true",
      };
    });
}

export function collectMissingAgentIds(): AgentDebugMissingElement[] {
  if (typeof document === "undefined") return [];
  return Array.from(document.querySelectorAll<HTMLElement>(INTERACTIVE_SELECTOR))
    .filter((element) => !element.dataset.agentId)
    .map((element) => {
      const rect = element.getBoundingClientRect();
      if (!isVisible(element, rect)) return null;
      return {
        role: readRole(element),
        text: readableText(element),
        tagName: element.tagName.toLowerCase(),
        selector: diagnosticSelectorFor(element),
        rect: {
          x: rect.x,
          y: rect.y,
          width: rect.width,
          height: rect.height,
        },
        nearestAgentId: element.closest<HTMLElement>("[data-agent-id]")?.dataset.agentId ?? null,
      };
    })
    .filter((item): item is AgentDebugMissingElement => item !== null);
}

function currentRoute(): string {
  return routerRef?.currentRoute.value.fullPath ?? window.location.pathname + window.location.search + window.location.hash;
}

export function observeAgentDebugSnapshot(): AgentDebugSnapshot {
  const activeElement = document.activeElement instanceof HTMLElement
    ? document.activeElement.dataset.agentId ?? null
    : null;
  return {
    enabled: isAgentDebugFrontendEnabled(),
    route: currentRoute(),
    title: document.title,
    capturedAt: Date.now(),
    viewport: {
      x: 0,
      y: 0,
      width: window.innerWidth,
      height: window.innerHeight,
    },
    activeElement,
    visibleText: (document.body?.innerText ?? "").trim().slice(0, 6000),
    elements: collectAgentElements(),
    missingAgentIds: collectMissingAgentIds(),
    errors: [...errors],
    invokes: [...invokes],
  };
}

function requireTarget(target: string): HTMLElement {
  const element = document.querySelector<HTMLElement>(selectorFor(target));
  if (!element) throw new Error(`Agent debug target not found: ${target}`);
  return element;
}

async function act(action: AgentDebugAction): Promise<AgentDebugSnapshot> {
  const actionId = "actionId" in action && action.actionId ? action.actionId : nextId("act");
  const started = pushLog({
    actionId,
    kind: action.type === "mark" ? "mark" : "frontend",
    level: "info",
    message: `agent-debug:${action.type}:start`,
    data: action,
  });
  await recordBackendLog(started);
  try {
    if (action.type === "click") {
      requireTarget(action.target).click();
    } else if (action.type === "focus") {
      requireTarget(action.target).focus();
    } else if (action.type === "type") {
      const element = requireTarget(action.target);
      element.focus();
      if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) {
        if (action.clear !== false) element.value = "";
        element.value += action.text;
        element.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: action.text }));
        element.dispatchEvent(new Event("change", { bubbles: true }));
      } else if (element.isContentEditable) {
        if (action.clear !== false) element.textContent = "";
        element.textContent = `${element.textContent ?? ""}${action.text}`;
        element.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: action.text }));
      } else {
        throw new Error(`Agent debug target is not text editable: ${action.target}`);
      }
    } else if (action.type === "hotkey") {
      const key = action.keys[action.keys.length - 1] ?? "";
      const event = new KeyboardEvent("keydown", {
        key,
        ctrlKey: action.keys.some((item) => item.toLowerCase() === "control" || item.toLowerCase() === "ctrl"),
        altKey: action.keys.some((item) => item.toLowerCase() === "alt"),
        shiftKey: action.keys.some((item) => item.toLowerCase() === "shift"),
        metaKey: action.keys.some((item) => item.toLowerCase() === "meta" || item.toLowerCase() === "cmd"),
        bubbles: true,
      });
      (document.activeElement ?? document.body).dispatchEvent(event);
    }
    const finished = pushLog({
      actionId,
      kind: action.type === "mark" ? "mark" : "frontend",
      level: "info",
      message: `agent-debug:${action.type}:success`,
      data: action,
    });
    await recordBackendLog(finished);
  } catch (err) {
    pushError("error", err);
    const failed = pushLog({
      actionId,
      kind: "frontend",
      level: "error",
      message: `agent-debug:${action.type}:error`,
      data: stringifyError(err),
    });
    await recordBackendLog(failed);
    throw err;
  }
  return observeAgentDebugSnapshot();
}

export async function markAgentDebug(label: string, data?: unknown): Promise<AgentDebugSnapshot> {
  return act({ type: "mark", label, data });
}

export function recordAgentDebugInvokeStart(command: string, args: unknown): AgentDebugInvokeEntry | null {
  if (!isAgentDebugFrontendEnabled() || command.startsWith("agent_debug_")) return null;
  const entry: AgentDebugInvokeEntry = {
    id: nextId("invoke"),
    command,
    actionId: nextId("invoke-action"),
    status: "started",
    argsSummary: summarize(args),
    resultSummary: null,
    error: null,
    startedAt: Date.now(),
    finishedAt: null,
    durationMs: null,
  };
  invokes.push(entry);
  keepBounded(invokes, MAX_INVOKES);
  void recordBackendLog(pushLog({
    actionId: entry.actionId,
    kind: "invoke",
    level: "info",
    message: `invoke:${command}:start`,
    data: entry.argsSummary,
  }));
  return entry;
}

export function recordAgentDebugInvokeEnd(
  entry: AgentDebugInvokeEntry | null,
  status: "success" | "error",
  value: unknown,
): void {
  if (!entry) return;
  entry.status = status;
  entry.finishedAt = Date.now();
  entry.durationMs = entry.finishedAt - entry.startedAt;
  if (status === "success") {
    entry.resultSummary = summarize(value);
  } else {
    entry.error = stringifyError(value).message;
  }
  void recordBackendLog(pushLog({
    actionId: entry.actionId,
    kind: "invoke",
    level: status === "success" ? "info" : "error",
    message: `invoke:${entry.command}:${status}`,
    data: status === "success" ? entry.resultSummary : entry.error,
  }));
}

export function installAgentDebugHarness(router: Router): void {
  if (installed || !isAgentDebugFrontendEnabled()) return;
  installed = true;
  routerRef = router;
  const originalConsoleError = console.error.bind(console);
  console.error = (...args: unknown[]) => {
    pushError("console", args.map((item) => stringifyError(item).message).join(" "));
    originalConsoleError(...args);
  };
  window.addEventListener("error", (event) => pushError("error", event.error ?? event.message));
  window.addEventListener("unhandledrejection", (event) => pushError("unhandledrejection", event.reason));
  (window as AgentDebugWindow).__liliaAgentDebug = {
    observe: observeAgentDebugSnapshot,
    act,
    mark: markAgentDebug,
    getRecentErrors: () => [...errors],
    equivalenceSnapshot: (fixtureId: string) =>
      rawInvoke(AGENT_DEBUG_EQUIVALENCE_SNAPSHOT_COMMAND, { fixtureId }),
  };
  void markAgentDebug("agent-debug-installed", { route: currentRoute() });
}


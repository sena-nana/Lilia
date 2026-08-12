import {
  HOOK_BACKENDS,
  HOOK_SCOPES,
  HOOK_SCOPE_LABELS,
  HOOK_SOURCE_EDIT_LABELS,
  HOOK_SOURCE_FORMATS,
  HOOK_SOURCE_FORMAT_LABELS,
  HOOK_SOURCE_STATE_LABELS,
  HOOK_TRUST_STATES,
  HOOK_TRUST_STATE_LABELS,
} from "./hooksContract.mjs";

export type HookBackendKind = typeof HOOK_BACKENDS[number];

export type HookScope = typeof HOOK_SCOPES[number];

export type HookSourceFormat = typeof HOOK_SOURCE_FORMATS[number];

export type HookTrustState = typeof HOOK_TRUST_STATES[number];

export {
  HOOK_BACKENDS,
  HOOK_SCOPES,
  HOOK_SCOPE_LABELS,
  HOOK_SOURCE_EDIT_LABELS,
  HOOK_SOURCE_FORMATS,
  HOOK_SOURCE_FORMAT_LABELS,
  HOOK_SOURCE_STATE_LABELS,
  HOOK_TRUST_STATES,
  HOOK_TRUST_STATE_LABELS,
};

const HOOK_BACKEND_SET = new Set<string>(HOOK_BACKENDS);
const HOOK_SCOPE_SET = new Set<string>(HOOK_SCOPES);
const HOOK_SOURCE_FORMAT_SET = new Set<string>(HOOK_SOURCE_FORMATS);
const HOOK_TRUST_STATE_SET = new Set<string>(HOOK_TRUST_STATES);

export function isHookBackendKind(value: unknown): value is HookBackendKind {
  return typeof value === "string" && HOOK_BACKEND_SET.has(value);
}

export function isHookScope(value: unknown): value is HookScope {
  return typeof value === "string" && HOOK_SCOPE_SET.has(value);
}

export function isHookSourceFormat(value: unknown): value is HookSourceFormat {
  return typeof value === "string" && HOOK_SOURCE_FORMAT_SET.has(value);
}

export function isHookTrustState(value: unknown): value is HookTrustState {
  return typeof value === "string" && HOOK_TRUST_STATE_SET.has(value);
}

export function hookScopeLabel(scope: HookScope): string {
  return HOOK_SCOPE_LABELS[scope];
}

export function hookSourceFormatLabel(format: HookSourceFormat): string {
  return HOOK_SOURCE_FORMAT_LABELS[format];
}

export function hookTrustStateLabel(trustState: HookTrustState): string {
  return HOOK_TRUST_STATE_LABELS[trustState];
}

export function hookSourceStateLabel(source: Pick<HookSourceSummary, "exists" | "handlerCount">): string {
  if (!source.exists) return HOOK_SOURCE_STATE_LABELS.missing;
  if (source.handlerCount === 0) return HOOK_SOURCE_STATE_LABELS.empty;
  return HOOK_SOURCE_STATE_LABELS.handlerCount.replace("{count}", String(source.handlerCount));
}

export function hookSourceEditLabel(source: Pick<HookSourceSummary, "editable" | "exists">): string {
  if (!source.editable) return HOOK_SOURCE_EDIT_LABELS.readonly;
  return source.exists ? HOOK_SOURCE_EDIT_LABELS.writable : HOOK_SOURCE_EDIT_LABELS.creatable;
}

export interface HookSourceSummary {
  id: string;
  backend: HookBackendKind;
  scope: HookScope;
  format: HookSourceFormat;
  name: string;
  path: string;
  projectCwd?: string | null;
  exists: boolean;
  editable: boolean;
  managed: boolean;
  enabled: boolean;
  revision: number;
  handlerCount: number;
  warnings: string[];
  limitations: string[];
  trustState: HookTrustState;
  description?: string | null;
}

export interface HookHandlerView {
  id: string;
  event: string;
  matcher: string | null;
  type: string;
  command: string | null;
  commandWindows: string | null;
  timeoutSeconds: number | null;
  statusMessage: string | null;
  supported: boolean;
  executable: boolean;
  groupAdvancedJson: string | null;
  advancedJson: string | null;
  warnings: string[];
}

export interface HookDocumentView {
  source: HookSourceSummary;
  handlers: HookHandlerView[];
  rawDocument: string | null;
  rawFormat: "json" | "toml" | "text";
  warnings: string[];
  limitations: string[];
}

export interface HooksOverview {
  sources: HookSourceSummary[];
  warnings: string[];
}

export interface HookHandlerUpdateInput {
  id?: string;
  event: string;
  matcher?: string | null;
  type: string;
  command?: string | null;
  commandWindows?: string | null;
  timeoutSeconds?: number | null;
  statusMessage?: string | null;
  groupAdvancedJson?: string | null;
  advancedJson?: string | null;
}

export interface HookDocumentUpdateInput {
  expectedRevision: number;
  handlers: HookHandlerUpdateInput[];
}

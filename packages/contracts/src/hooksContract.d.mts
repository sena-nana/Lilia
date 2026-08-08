type HookBackendKind = "native-agentkit";
type HookScope = "managed" | "user" | "project" | "local" | "plugin" | "system";
type HookSourceFormat =
  | "settings_json"
  | "hooks_json"
  | "config_toml"
  | "managed_settings"
  | "requirements_toml"
  | "plugin_manifest";
type HookTrustState = "unknown" | "required" | "managed" | "n_a";

export const HOOKS_CONTRACT: Record<string, unknown>;
export const HOOK_BACKENDS: readonly HookBackendKind[];
export const HOOK_SCOPES: readonly HookScope[];
export const HOOK_SOURCE_FORMATS: readonly HookSourceFormat[];
export const HOOK_TRUST_STATES: readonly HookTrustState[];
export const HOOK_SCOPE_LABELS: Readonly<Record<HookScope, string>>;
export const HOOK_SOURCE_FORMAT_LABELS: Readonly<Record<HookSourceFormat, string>>;
export const HOOK_TRUST_STATE_LABELS: Readonly<Record<HookTrustState, string>>;
export const HOOK_SOURCE_STATE_LABELS: Readonly<{
  missing: string;
  empty: string;
  handlerCount: string;
}>;
export const HOOK_SOURCE_EDIT_LABELS: Readonly<{
  readonly: string;
  writable: string;
  creatable: string;
}>;

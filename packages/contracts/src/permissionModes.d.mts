export type RuntimePermissionMode = "full" | "ask" | "readonly" | "free";
export type RuntimePermissionBackend = "native-agentkit";

export interface PermissionModeDisplay {
  label: string;
  description: string;
}

export interface NativePermissionRuntimeMapping {
  mode: "full_access" | "default" | "readonly";
  requireApproval: boolean;
}

export const PERMISSION_MODES_MANIFEST: {
  permissionModes: readonly RuntimePermissionMode[];
  defaultPermissionMode: RuntimePermissionMode;
  display: Readonly<Record<RuntimePermissionMode, PermissionModeDisplay>>;
  displayOrder: readonly RuntimePermissionMode[];
  runtimeMappings: {
    "native-agentkit": Record<RuntimePermissionMode, NativePermissionRuntimeMapping>;
  };
};

export const PERMISSION_MODES: readonly RuntimePermissionMode[];
export const DEFAULT_PERMISSION_MODE: RuntimePermissionMode;
export const PERMISSION_MODE_DISPLAY: Readonly<Record<RuntimePermissionMode, PermissionModeDisplay>>;
export const PERMISSION_MODE_DISPLAY_ORDER: readonly RuntimePermissionMode[];

export function isRuntimePermissionMode(value: unknown): value is RuntimePermissionMode;

export function normalizeRuntimePermissionMode(
  value: unknown,
  fallback?: RuntimePermissionMode,
): RuntimePermissionMode;

export function runtimePermissionMapping(
  backend: "native-agentkit",
  permission: unknown,
): NativePermissionRuntimeMapping | null;

export function runtimePermissionMapping(
  backend: RuntimePermissionBackend,
  permission: unknown,
): NativePermissionRuntimeMapping | null;

export function nativePermissionRuntime(permission: unknown): NativePermissionRuntimeMapping | null;

export function createPermissionModeHelpers(permissionModesJson) {
  const manifest = deepFreeze(permissionModesJson);
  const modes = manifest.permissionModes;
  const defaultMode = manifest.defaultPermissionMode;
  const modeSet = new Set(modes);

  function isRuntimePermissionMode(value) {
    return typeof value === "string" && modeSet.has(value);
  }

  function normalizeRuntimePermissionMode(value, fallback = defaultMode) {
    return isRuntimePermissionMode(value) ? value : fallback;
  }

  function runtimePermissionMapping(backend, permission) {
    const normalized = normalizeRuntimePermissionMode(permission);
    return manifest.runtimeMappings[backend]?.[normalized] ??
      manifest.runtimeMappings[backend]?.[defaultMode] ??
      null;
  }

  function nativePermissionRuntime(permission) {
    return runtimePermissionMapping("native-agentkit", permission);
  }

  return {
    PERMISSION_MODES_MANIFEST: manifest,
    PERMISSION_MODES: modes,
    DEFAULT_PERMISSION_MODE: defaultMode,
    PERMISSION_MODE_DISPLAY: manifest.display,
    PERMISSION_MODE_DISPLAY_ORDER: manifest.displayOrder,
    isRuntimePermissionMode,
    normalizeRuntimePermissionMode,
    runtimePermissionMapping,
    nativePermissionRuntime,
  };
}

function deepFreeze(value) {
  if (!value || typeof value !== "object") return value;
  for (const child of Object.values(value)) {
    deepFreeze(child);
  }
  return Object.freeze(value);
}

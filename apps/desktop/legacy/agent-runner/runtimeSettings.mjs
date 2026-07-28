import { isRuntimePermissionMode } from "@lilia/contracts/permissionModes.mjs";

export function normalizeRuntimePermission(permission) {
  return isRuntimePermissionMode(permission) ? permission : null;
}

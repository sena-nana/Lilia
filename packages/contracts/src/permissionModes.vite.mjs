import permissionModesJson from "./permission-modes.json" with { type: "json" };
import { createPermissionModeHelpers } from "./permissionModesCore.mjs";

const helpers = createPermissionModeHelpers(permissionModesJson);

export const PERMISSION_MODES_MANIFEST = helpers.PERMISSION_MODES_MANIFEST;
export const PERMISSION_MODES = helpers.PERMISSION_MODES;
export const DEFAULT_PERMISSION_MODE = helpers.DEFAULT_PERMISSION_MODE;
export const PERMISSION_MODE_DISPLAY = helpers.PERMISSION_MODE_DISPLAY;
export const PERMISSION_MODE_DISPLAY_ORDER = helpers.PERMISSION_MODE_DISPLAY_ORDER;
export const isRuntimePermissionMode = helpers.isRuntimePermissionMode;
export const normalizeRuntimePermissionMode = helpers.normalizeRuntimePermissionMode;
export const runtimePermissionMapping = helpers.runtimePermissionMapping;
export const nativePermissionRuntime = helpers.nativePermissionRuntime;


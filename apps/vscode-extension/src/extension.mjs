/**
 * Minimal VS Code compat extension (#40).
 *
 * Calls AgentKit Completion / Next Edit via `hostClient` — never LiliaCore,
 * never Node agent-runner, never official Agent Server.
 *
 * Loaded as ESM (`.mjs`) so root Node 26 workspace can unit-test without vsce.
 */

import * as vscode from "vscode";
import { activate as activateCore, deactivate as deactivateCore } from "./extensionCore.mjs";

/**
 * @param {import('vscode').ExtensionContext} context
 */
export function activate(context) {
  return activateCore(vscode, context);
}

export function deactivate() {
  return deactivateCore();
}

import { isRecord } from "./utils.mjs";

export function readRunnerWorkflow(cmd) {
  return isRecord(cmd?.workflow) ? cmd.workflow : null;
}

export function readRunnerRuntimeCommand(cmd) {
  return isRecord(cmd?.runtimeCommand) ? cmd.runtimeCommand : null;
}

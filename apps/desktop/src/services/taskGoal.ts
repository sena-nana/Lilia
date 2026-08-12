import type { LiliaThreadGoal } from "@lilia/contracts";
import {
  TASK_GOAL_CLEAR_COMMAND,
  TASK_GOAL_GET_COMMAND,
  TASK_GOAL_REFRESH_COMMAND,
  TASK_GOAL_SET_COMMAND,
} from "@lilia/contracts";
import { invoke } from "../tauri/runtime";

export function getTaskGoal(taskId: string): Promise<LiliaThreadGoal | null> {
  return invoke<LiliaThreadGoal | null>(TASK_GOAL_GET_COMMAND, { taskId });
}

export function setTaskGoal(
  taskId: string,
  objective: string,
  tokenBudget: number | null = null,
): Promise<LiliaThreadGoal> {
  return invoke<LiliaThreadGoal>(TASK_GOAL_SET_COMMAND, { taskId, objective, tokenBudget });
}

export function refreshTaskGoal(taskId: string): Promise<LiliaThreadGoal> {
  return invoke<LiliaThreadGoal>(TASK_GOAL_REFRESH_COMMAND, { taskId });
}

export function clearTaskGoal(taskId: string): Promise<boolean> {
  return invoke<boolean>(TASK_GOAL_CLEAR_COMMAND, { taskId });
}

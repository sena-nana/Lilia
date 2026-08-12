import taskCommandContract from "./task-command-contract.json" with { type: "json" };

const manifest = Object.freeze(taskCommandContract);

export const TASK_COMMANDS_CONTRACT = manifest;
export const TASK_LIST_COMMAND = manifest.commands.list;
export const TASK_LIST_SIDEBAR_CONVERSATIONS_COMMAND =
  manifest.commands.listSidebarConversations;
export const TASK_GET_COMMAND = manifest.commands.get;
export const TASK_HANDOFF_GET_COMMAND = manifest.commands.handoffGet;
export const TASK_CREATE_COMMAND = manifest.commands.create;
export const TASK_UPDATE_COMMAND = manifest.commands.update;
export const TASK_DELETE_COMMAND = manifest.commands.delete;
export const TASK_PROMOTE_COMMAND = manifest.commands.promote;
export const TASK_ARCHIVE_PROJECT_COMMAND = manifest.commands.archiveProject;
export const TASK_ARCHIVE_COMMAND = manifest.commands.archive;
export const TASK_TOGGLE_PIN_COMMAND = manifest.commands.togglePin;
export const TASK_REORDER_COMMAND = manifest.commands.reorder;
export const TASK_REPARENT_COMMAND = manifest.commands.reparent;
export const TASK_UPDATE_DEPENDENCIES_COMMAND = manifest.commands.updateDependencies;
export const TASK_GOAL_GET_COMMAND = manifest.commands.goalGet;
export const TASK_GOAL_SET_COMMAND = manifest.commands.goalSet;
export const TASK_GOAL_REFRESH_COMMAND = manifest.commands.goalRefresh;
export const TASK_GOAL_CLEAR_COMMAND = manifest.commands.goalClear;

use std::sync::OnceLock;

use serde::Deserialize;

const TASK_COMMAND_CONTRACT_JSON: &str =
    include_str!("../../../../packages/contracts/src/task-command-contract.json");

static TASK_COMMAND_CONTRACT: OnceLock<TaskCommandContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct TaskCommandContract {
    commands: TaskCommandsContract,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskCommandsContract {
    list: String,
    list_sidebar_conversations: String,
    search_sessions: String,
    get: String,
    handoff_get: String,
    create: String,
    update: String,
    delete: String,
    promote: String,
    archive_project: String,
    archive: String,
    toggle_pin: String,
    reorder: String,
    reparent: String,
    update_dependencies: String,
    goal_get: String,
    goal_set: String,
    goal_refresh: String,
    goal_clear: String,
}

fn task_command_contract() -> &'static TaskCommandContract {
    TASK_COMMAND_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            TASK_COMMAND_CONTRACT_JSON,
            "task-command-contract.json",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects_tasks::{
        search_sessions, task_archive, task_archive_project, task_create, task_delete, task_get,
        task_list, task_list_sidebar_conversations, task_promote, task_reorder, task_reparent,
        task_toggle_pin, task_update, task_update_dependencies,
    };
    use crate::task_goal::{task_goal_clear, task_goal_get, task_goal_refresh, task_goal_set};
    use crate::task_handoff::task_handoff_get;

    #[test]
    fn task_command_names_load_from_contract_manifest() {
        let commands = &task_command_contract().commands;
        let _ = task_list;
        let _ = task_list_sidebar_conversations;
        let _ = search_sessions;
        let _ = task_get;
        let _ = task_handoff_get;
        let _ = task_create;
        let _ = task_update;
        let _ = task_delete;
        let _ = task_promote;
        let _ = task_archive_project;
        let _ = task_archive;
        let _ = task_toggle_pin;
        let _ = task_reorder;
        let _ = task_reparent;
        let _ = task_update_dependencies;
        let _ = task_goal_get;
        let _ = task_goal_set;
        let _ = task_goal_refresh;
        let _ = task_goal_clear;

        assert_eq!(commands.list, stringify!(task_list));
        assert_eq!(
            commands.list_sidebar_conversations,
            stringify!(task_list_sidebar_conversations)
        );
        assert_eq!(commands.search_sessions, stringify!(search_sessions));
        assert_eq!(commands.get, stringify!(task_get));
        assert_eq!(commands.handoff_get, stringify!(task_handoff_get));
        assert_eq!(commands.create, stringify!(task_create));
        assert_eq!(commands.update, stringify!(task_update));
        assert_eq!(commands.delete, stringify!(task_delete));
        assert_eq!(commands.promote, stringify!(task_promote));
        assert_eq!(commands.archive_project, stringify!(task_archive_project));
        assert_eq!(commands.archive, stringify!(task_archive));
        assert_eq!(commands.toggle_pin, stringify!(task_toggle_pin));
        assert_eq!(commands.reorder, stringify!(task_reorder));
        assert_eq!(commands.reparent, stringify!(task_reparent));
        assert_eq!(
            commands.update_dependencies,
            stringify!(task_update_dependencies)
        );
        assert_eq!(commands.goal_get, stringify!(task_goal_get));
        assert_eq!(commands.goal_set, stringify!(task_goal_set));
        assert_eq!(commands.goal_refresh, stringify!(task_goal_refresh));
        assert_eq!(commands.goal_clear, stringify!(task_goal_clear));
    }
}

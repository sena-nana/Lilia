use lilia_contracts::TaskId;
use lilia_desktop_application::{DesktopApplication, DesktopGoalSnapshot};
use tauri::State;

fn parse_task_id(value: String) -> Result<TaskId, String> {
    TaskId::new(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn task_goal_get(
    task_id: String,
    desktop: State<'_, DesktopApplication>,
) -> Result<Option<DesktopGoalSnapshot>, String> {
    desktop
        .task_goal(&parse_task_id(task_id)?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn task_goal_set(
    task_id: String,
    objective: String,
    token_budget: Option<u64>,
    desktop: State<'_, DesktopApplication>,
) -> Result<DesktopGoalSnapshot, String> {
    desktop
        .set_task_goal(&parse_task_id(task_id)?, objective, token_budget)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn task_goal_refresh(
    task_id: String,
    desktop: State<'_, DesktopApplication>,
) -> Result<DesktopGoalSnapshot, String> {
    desktop
        .refresh_task_goal(&parse_task_id(task_id)?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn task_goal_clear(
    task_id: String,
    desktop: State<'_, DesktopApplication>,
) -> Result<bool, String> {
    desktop
        .clear_task_goal(&parse_task_id(task_id)?)
        .map_err(|error| error.to_string())
}

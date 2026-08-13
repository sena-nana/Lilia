//! Thin Tauri adapter for the shared title-update coordinator.

use lilia_contracts::TaskId;
use lilia_desktop_application::DesktopApplication;
use tauri::{AppHandle, Manager, Runtime};

/// Schedule automatic title generation after a completed turn.
pub(crate) fn spawn_title_update<R: Runtime>(
    app: AppHandle<R>,
    task_id: String,
    turn_id: Option<String>,
) {
    let Some(application) = app
        .try_state::<DesktopApplication>()
        .map(|state| state.inner().clone())
    else {
        eprintln!("[title-update] skipped: DesktopApplication unavailable");
        return;
    };
    let Ok(task_id) = TaskId::new(task_id) else {
        eprintln!("[title-update] skipped: invalid task id");
        return;
    };
    application.spawn_title_update_after_turn(task_id, turn_id);
}

/// Accept or decline a previously proposed title-update interaction.
#[tauri::command]
pub fn chat_respond_title_update(
    app: AppHandle,
    task_id: String,
    request_id: String,
    decision: String,
) -> Result<(), String> {
    let accepted = match decision.as_str() {
        "accept" => true,
        "decline" => false,
        _ => return Err("invalid title update decision".to_owned()),
    };
    let application = app
        .try_state::<DesktopApplication>()
        .map(|state| state.inner().clone())
        .ok_or_else(|| "DesktopApplication unavailable".to_string())?;
    let task_id = TaskId::new(task_id).map_err(|error| format!("invalid task id: {error}"))?;
    application
        .respond_title_update_review(&task_id, &request_id, accepted)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub(crate) fn shutdown_title_update(app: &AppHandle) {
    if let Some(application) = app.try_state::<DesktopApplication>() {
        application.title_update_coordinator().shutdown();
    }
}

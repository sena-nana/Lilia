use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use lilia_contracts::TaskId;
use lilia_desktop_application::{DesktopApplication, DesktopImportedTaskHandoff};
use rusqlite::OptionalExtension;
use tauri::{AppHandle, Manager, Runtime, State};

use crate::store::LiliaStore;

const APPLICATION_READY_TIMEOUT: Duration = Duration::from_secs(5);
const APPLICATION_READY_POLL: Duration = Duration::from_millis(50);

pub(crate) type ImportedTaskHandoff = DesktopImportedTaskHandoff;

pub(crate) struct TaskHandoffOpenPayload {
    pub(crate) project_id: String,
    pub(crate) cwd: String,
    pub(crate) task_id: String,
    pub(crate) handoff_id: String,
}

#[tauri::command]
pub(crate) fn task_handoff_get(
    task_id: String,
    application: State<'_, DesktopApplication>,
    legacy_store: State<'_, LiliaStore>,
) -> Result<Option<ImportedTaskHandoff>, String> {
    let typed_task_id = TaskId::new(task_id.clone()).map_err(|error| error.to_string())?;
    if let Some(imported) = application
        .imported_task_handoff(&typed_task_id)
        .map_err(|error| error.to_string())?
    {
        return Ok(Some(imported));
    }
    let payload = legacy_store
        .conn()?
        .query_row(
            "SELECT payload_json FROM task_handoffs WHERE task_id = ?1",
            [task_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("task_handoff_get: {error}"))?;
    payload
        .map(|payload| {
            lilia_desktop_application::describe_task_handoff(&task_id, &payload)
                .map_err(|error| error.to_string())
        })
        .transpose()
}

pub(crate) fn resolve_task_handoff<R: Runtime>(
    app: &AppHandle<R>,
    handoff_path: &Path,
    cwd: &Path,
) -> Result<TaskHandoffOpenPayload, String> {
    let started = Instant::now();
    loop {
        if let Some(application) = app.try_state::<DesktopApplication>() {
            let opened = application
                .accept_task_handoff_file(handoff_path, Some(cwd))
                .map_err(|error| error.to_string())?;
            return Ok(TaskHandoffOpenPayload {
                project_id: opened.project_id.as_str().to_owned(),
                cwd: opened.cwd,
                task_id: opened.task_id.as_str().to_owned(),
                handoff_id: opened.handoff_id,
            });
        }
        if started.elapsed() >= APPLICATION_READY_TIMEOUT {
            return Err("桌面应用服务尚未初始化".to_owned());
        }
        thread::sleep(APPLICATION_READY_POLL);
    }
}

#[cfg(feature = "runtime-domain-reference")]
pub(crate) fn runtime_reference_prepare_handoff(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    lilia_desktop_application::prepare_task_handoff_reference(payload)
        .map_err(|error| error.to_string())
}

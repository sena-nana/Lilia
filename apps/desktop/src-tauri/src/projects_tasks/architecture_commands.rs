use lilia_contracts::{ProjectId, TaskId};
use lilia_desktop_application::DesktopApplication;
use lilia_desktop_application::{
    ArchitectureBackend, ProjectArchitectureApplyInput, ProjectArchitectureApplyResult,
    ProjectArchitectureChangeEvent, ProjectArchitectureChangeRecord, ProjectArchitectureGraph,
    ProjectArchitectureRejectInput, ProjectArchitectureRollbackResult,
};
use tauri::State;

#[tauri::command]
pub fn project_architecture_get(
    project_id: String,
    application: State<'_, DesktopApplication>,
) -> Result<ProjectArchitectureGraph, String> {
    let project_id = ProjectId::new(project_id).map_err(|error| error.to_string())?;
    application
        .project_architecture(&project_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn project_architecture_list_changes(
    project_id: String,
    limit: Option<i64>,
    application: State<'_, DesktopApplication>,
) -> Result<Vec<ProjectArchitectureChangeRecord>, String> {
    let project_id = ProjectId::new(project_id).map_err(|error| error.to_string())?;
    application
        .project_architecture_changes(&project_id, limit.unwrap_or(20).clamp(1, 200) as usize)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn project_architecture_apply(
    input: ProjectArchitectureApplyInput,
    application: State<'_, DesktopApplication>,
) -> Result<ProjectArchitectureApplyResult, String> {
    application
        .apply_project_architecture(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn project_architecture_reject(
    input: ProjectArchitectureRejectInput,
    application: State<'_, DesktopApplication>,
) -> Result<ProjectArchitectureChangeEvent, String> {
    application
        .reject_project_architecture(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn project_architecture_rollback(
    project_id: String,
    task_id: String,
    backend: ArchitectureBackend,
    application: State<'_, DesktopApplication>,
) -> Result<ProjectArchitectureRollbackResult, String> {
    let project_id = ProjectId::new(project_id).map_err(|error| error.to_string())?;
    let task_id = TaskId::new(task_id).map_err(|error| error.to_string())?;
    application
        .rollback_project_architecture(&project_id, &task_id, backend)
        .map_err(|error| error.to_string())
}

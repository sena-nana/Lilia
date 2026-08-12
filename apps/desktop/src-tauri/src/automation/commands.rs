use tauri::{AppHandle, Runtime, State};

use lilia_desktop_application::{DesktopApplication, DesktopAutomationService};

use super::{emit_changed, emit_execution_result, run_workflow};
use crate::automation::signals::manual_signal;
use crate::automation::types::{
    AutomationResumeRunInput, AutomationRun, AutomationRunDetail, AutomationRunOnceInput,
    AutomationRunSummary, AutomationSaveDraftInput, AutomationWorkflow, AutomationWorkflowVersion,
};
use crate::chat::state::ChatStore;
use crate::store::LiliaStore;

#[tauri::command]
pub fn automation_list_workflows(
    automation: State<'_, DesktopAutomationService>,
) -> Result<Vec<AutomationWorkflow>, String> {
    automation
        .list_workflows()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn automation_save_draft<R: Runtime>(
    input: AutomationSaveDraftInput,
    app: AppHandle<R>,
    automation: State<'_, DesktopAutomationService>,
) -> Result<AutomationWorkflow, String> {
    let workflow = automation
        .save_draft(input)
        .map_err(|error| error.to_string())?;
    emit_changed(&app, Some(workflow.id.clone()));
    Ok(workflow)
}

#[tauri::command]
pub fn automation_publish<R: Runtime>(
    id: String,
    app: AppHandle<R>,
    automation: State<'_, DesktopAutomationService>,
) -> Result<AutomationWorkflowVersion, String> {
    let version = automation.publish(&id).map_err(|error| error.to_string())?;
    emit_changed(&app, Some(id));
    Ok(version)
}

#[tauri::command]
pub fn automation_delete_workflow<R: Runtime>(
    id: String,
    app: AppHandle<R>,
    automation: State<'_, DesktopAutomationService>,
) -> Result<(), String> {
    automation
        .delete_workflow(&id)
        .map_err(|error| error.to_string())?;
    emit_changed(&app, Some(id));
    Ok(())
}

#[tauri::command]
pub fn automation_set_enabled<R: Runtime>(
    id: String,
    enabled: bool,
    app: AppHandle<R>,
    automation: State<'_, DesktopAutomationService>,
) -> Result<(), String> {
    automation
        .set_enabled(&id, enabled)
        .map_err(|error| error.to_string())?;
    emit_changed(&app, Some(id));
    Ok(())
}

#[tauri::command]
pub fn automation_run_once<R: Runtime>(
    id: String,
    input: Option<AutomationRunOnceInput>,
    app: AppHandle<R>,
    _store: State<'_, LiliaStore>,
    chat_store: State<'_, ChatStore>,
) -> Result<AutomationRun, String> {
    let payload = input.and_then(|input| input.payload);
    run_workflow(&app, &chat_store, &id, manual_signal(payload))
}

#[tauri::command]
pub fn automation_resume_run<R: Runtime>(
    run_id: String,
    input: Option<AutomationResumeRunInput>,
    app: AppHandle<R>,
    desktop: State<'_, DesktopApplication>,
) -> Result<AutomationRun, String> {
    let result = desktop
        .resume_automation_run(&run_id, input.unwrap_or_default())
        .map_err(|error| format!("automation_resume_run: {error}"))?;
    emit_execution_result(&app, &result);
    Ok(result.detail.run)
}

#[tauri::command]
pub fn automation_list_runs(
    workflow_id: Option<String>,
    automation: State<'_, DesktopAutomationService>,
) -> Result<Vec<AutomationRunSummary>, String> {
    automation
        .list_runs(workflow_id.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn automation_get_run(
    run_id: String,
    automation: State<'_, DesktopAutomationService>,
) -> Result<Option<AutomationRunDetail>, String> {
    automation
        .run_detail(&run_id)
        .map_err(|error| error.to_string())
}

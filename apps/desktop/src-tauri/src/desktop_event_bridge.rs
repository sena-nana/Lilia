use lilia_desktop_application::{
    AutomationRunStatus, DesktopApplication, DesktopEvent, DesktopEventKind,
};
use tauri::{AppHandle, Emitter};

pub(crate) fn start(app: &AppHandle, desktop: &DesktopApplication) -> Result<(), std::io::Error> {
    let app = app.clone();
    let desktop = desktop.clone();
    let events = desktop.subscribe_events();
    std::thread::Builder::new()
        .name("lilia-tauri-desktop-events".to_owned())
        .spawn(move || {
            while let Ok(event) = events.recv() {
                if let Err(error) = forward(&app, &desktop, event) {
                    eprintln!("[desktop-event-bridge] {error}");
                }
            }
        })
        .map(|_| ())
}

fn forward(
    app: &AppHandle,
    desktop: &DesktopApplication,
    event: DesktopEvent,
) -> Result<(), String> {
    match event.kind {
        DesktopEventKind::ProjectsChanged => {
            emit_tasks_changed(app, None);
        }
        DesktopEventKind::TasksChanged { project_id, .. } => {
            emit_tasks_changed(app, project_id.as_ref().map(|project| project.as_str()));
        }
        DesktopEventKind::TimelineChanged { task_id, cursor } => {
            let projected = desktop
                .authority()
                .projection_timeline_for_task(&task_id)
                .into_iter()
                .filter(|timeline| cursor.is_none_or(|cursor| timeline.sequence == cursor))
                .collect::<Vec<_>>();
            crate::native_agent::mirror_product_timeline_to_ui_cache(app, &projected)?;
        }
        DesktopEventKind::TodosChanged { task_id } => {
            app.emit(
                crate::todos::contract::changed_event_name(),
                crate::todos::contract::changed_event_payload(task_id.as_str()),
            )
            .map_err(|error| error.to_string())?;
        }
        DesktopEventKind::AutomationChanged { automation_id } => {
            crate::automation::emit_changed(app, automation_id);
        }
        DesktopEventKind::AutomationRunChanged { run_id, status, .. } => {
            let Some(detail) = desktop
                .automation_service()
                .run_detail(&run_id)
                .map_err(|error| error.to_string())?
            else {
                return Ok(());
            };
            crate::automation::emit_run(
                app,
                crate::automation::contract::run_updated_event_name(),
                detail.run.clone(),
            );
            if matches!(
                status,
                AutomationRunStatus::Succeeded
                    | AutomationRunStatus::Failed
                    | AutomationRunStatus::Skipped
                    | AutomationRunStatus::Cancelled
            ) {
                crate::automation::emit_run(
                    app,
                    crate::automation::contract::run_finished_event_name(),
                    detail.run,
                );
            }
        }
        DesktopEventKind::RoadmapChanged { project_id, .. } => {
            emit_tasks_changed(app, Some(project_id.as_str()));
        }
        DesktopEventKind::ArchitectureChanged { project_id, .. } => {
            emit_tasks_changed(app, Some(project_id.as_str()));
        }
        DesktopEventKind::MemoryChanged { project_id, .. } => {
            emit_tasks_changed(app, project_id.as_ref().map(|project| project.as_str()));
        }
        DesktopEventKind::MemoryInjectionChanged { task_id }
        | DesktopEventKind::GoalChanged { task_id }
        | DesktopEventKind::WorktreeChanged { task_id }
        | DesktopEventKind::WorktreeOperationCompleted { task_id } => {
            emit_tasks_changed_for_task(app, desktop, &task_id);
        }
        DesktopEventKind::WorktreeOperationFailed {
            task_id, message, ..
        } => {
            eprintln!(
                "[desktop-event-bridge] worktree operation failed for {}: {message}",
                task_id.as_str()
            );
            emit_tasks_changed_for_task(app, desktop, &task_id);
        }
        DesktopEventKind::ComposerChanged { .. }
        | DesktopEventKind::ProviderChanged { .. }
        | DesktopEventKind::CredentialChanged { .. }
        | DesktopEventKind::GitHubBindingChanged { .. }
        | DesktopEventKind::AgentInteractionChanged { .. }
        | DesktopEventKind::TurnStateChanged { .. }
        | DesktopEventKind::TurnRecoveryIssue { .. }
        | DesktopEventKind::ApprovalChanged { .. }
        | DesktopEventKind::InteractionChanged { .. }
        | DesktopEventKind::MemorySettingsChanged
        | DesktopEventKind::NavigationRequested { .. }
        | DesktopEventKind::UpdateStateChanged { .. } => {}
    }
    Ok(())
}

fn emit_tasks_changed_for_task(
    app: &AppHandle,
    desktop: &DesktopApplication,
    task_id: &lilia_contracts::TaskId,
) {
    let project_id = desktop
        .get_task(task_id)
        .ok()
        .and_then(|task| task.project_id)
        .map(|project| project.into_inner());
    emit_tasks_changed(app, project_id.as_deref());
}

fn emit_tasks_changed(app: &AppHandle, project_id: Option<&str>) {
    let _ = app.emit(
        crate::task_contract::tasks_changed_event_name(),
        crate::task_contract::tasks_changed_event_payload(project_id),
    );
}

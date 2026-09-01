use std::sync::{Arc, Mutex};

use lilia_contracts::{ProjectId, TaskId};

use crate::application::{
    DesktopApplication, DesktopApplicationError, DesktopCommand, DesktopCommandOutcome,
    ProjectQuery, TaskQuery, WorkspaceItem, WorkspaceItemRestoration,
};

#[cfg(test)]
use crate::application::{PanelLayoutSnapshot, WorkspaceItemError, WorkspaceItemId};

pub use lilia_feature_workspace::{
    DesktopWorkspaceProject, DesktopWorkspaceSession, DesktopWorkspaceSessionId,
    DesktopWorkspaceSessionIdError, DesktopWorkspaceSessionState,
    DesktopWorkspaceSessionStateError, DesktopWorkspaceSnapshot, DesktopWorkspaceState,
    DesktopWorkspaceTask, DesktopWorkspaceTransferOutcome, WorkspaceCatalog, WorkspaceSessionError,
    WorkspaceTaskRef,
};

impl WorkspaceCatalog for DesktopApplication {
    fn list_projects(&self) -> Result<Vec<DesktopWorkspaceProject>, WorkspaceSessionError> {
        let mut projects = self
            .query_projects(ProjectQuery::default())
            .map_err(catalog_error)?
            .into_iter()
            .map(|project| DesktopWorkspaceProject {
                id: project.id,
                name: project.name,
                workspace_path: project.workspace_path,
                pinned: project.pinned,
                sort_order: project.sort_order,
            })
            .collect::<Vec<_>>();
        projects.sort_by(|left, right| {
            right
                .pinned
                .cmp(&left.pinned)
                .then_with(|| left.sort_order.cmp(&right.sort_order))
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.as_str().cmp(right.id.as_str()))
        });
        Ok(projects)
    }

    fn list_project_tasks(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<DesktopWorkspaceTask>, WorkspaceSessionError> {
        Ok(self
            .query_tasks(TaskQuery::for_project(project_id.clone()))
            .map_err(catalog_error)?
            .into_iter()
            .map(workspace_task)
            .collect())
    }

    fn list_inbox_tasks(&self) -> Result<Vec<DesktopWorkspaceTask>, WorkspaceSessionError> {
        Ok(self
            .query_tasks(TaskQuery::for_inbox())
            .map_err(catalog_error)?
            .into_iter()
            .map(workspace_task)
            .collect())
    }

    fn restore_item(
        &self,
        restoration: &WorkspaceItemRestoration,
    ) -> Result<Option<WorkspaceItem>, WorkspaceSessionError> {
        self.restore_workspace_item(restoration)
            .map_err(catalog_error)
    }

    fn lookup_task(&self, task_id: &TaskId) -> Result<WorkspaceTaskRef, WorkspaceSessionError> {
        let task = self.get_task(task_id).map_err(catalog_error)?;
        Ok(WorkspaceTaskRef {
            archived: task.archived,
            project_id: task.project_id,
        })
    }

    fn host_ptr(&self) -> usize {
        Arc::as_ptr(&self.inner) as usize
    }
}

fn workspace_task(task: lilia_contracts::ProductTask) -> DesktopWorkspaceTask {
    DesktopWorkspaceTask {
        id: task.id,
        title: task.title,
        parent_id: task.parent_id,
        status: task.status,
        priority: task.priority,
        pinned: task.pinned,
        sort_order: task.sort_order,
    }
}

fn catalog_error(error: DesktopApplicationError) -> WorkspaceSessionError {
    match error {
        DesktopApplicationError::InvalidInput { field, message } => {
            WorkspaceSessionError::InvalidInput { field, message }
        }
        DesktopApplicationError::StateUnavailable(state) => {
            WorkspaceSessionError::StateUnavailable(state)
        }
        DesktopApplicationError::StateRevisionOverflow(state) => {
            WorkspaceSessionError::StateRevisionOverflow(state)
        }
        DesktopApplicationError::WorkspaceItem(error) => WorkspaceSessionError::Item(error),
        DesktopApplicationError::PanelLayout(error) => WorkspaceSessionError::Panel(error),
        other => WorkspaceSessionError::Catalog(other.to_string()),
    }
}

impl DesktopApplication {
    pub fn create_workspace_session(
        &self,
        id: DesktopWorkspaceSessionId,
    ) -> DesktopWorkspaceSession {
        DesktopWorkspaceSession::new(
            id,
            Arc::new(self.clone()),
            Arc::new(Mutex::new(DesktopWorkspaceState::default())),
        )
    }

    pub fn default_workspace_session(&self) -> DesktopWorkspaceSession {
        DesktopWorkspaceSession::new(
            DesktopWorkspaceSessionId::new("default").expect("default workspace id"),
            Arc::new(self.clone()),
            Arc::clone(&self.inner.workspace),
        )
    }

    pub fn workspace_snapshot(&self) -> Result<DesktopWorkspaceSnapshot, DesktopApplicationError> {
        Ok(self.default_workspace_session().snapshot()?)
    }

    pub fn execute_command(
        &self,
        command: DesktopCommand,
    ) -> Result<DesktopCommandOutcome, DesktopApplicationError> {
        Ok(self.default_workspace_session().execute(command)?)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use lilia_contracts::{ProductEntity, ProductTask, Project};
    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::application::{
        ApplicationWorkspaceSurface, DesktopApplicationConfig, DesktopHost, DesktopHostAction,
        DesktopHostContext, DesktopHostError, DesktopHostResult, DesktopProjectPatch,
        ProjectWorkspaceSurface,
    };

    struct NoopHost;

    static NEXT_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    fn application() -> DesktopApplication {
        let id = NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:desktop-workspace:{id}"),
            format!("desktop-workspace-test:{id}"),
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new("C:/lilia/workspace", "liliacode.test").unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap()
    }

    #[test]
    fn commands_keep_selection_valid_as_product_facts_change() {
        let app = application();
        let project_a = ProjectId::new("project-a").unwrap();
        let project_b = ProjectId::new("project-b").unwrap();
        let task_a = TaskId::new("task-a").unwrap();
        let client = app.authority().client().unwrap();
        client
            .products()
            .create_entity(ProductEntity::Project(
                Project::new(project_a.clone(), "A").unwrap(),
            ))
            .unwrap();
        client
            .products()
            .create_entity(ProductEntity::Project(
                Project::new(project_b.clone(), "B").unwrap(),
            ))
            .unwrap();
        client
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task_a.clone(), Some(project_a.clone()), "Task A").unwrap(),
            ))
            .unwrap();

        let initial = app
            .execute_command(DesktopCommand::RefreshWorkspace)
            .unwrap();
        assert_eq!(initial.workspace.selected_project, Some(project_a.clone()));

        let selected = app
            .execute_command(DesktopCommand::SelectTask(task_a.clone()))
            .unwrap();
        assert_eq!(selected.workspace.selected_task, Some(task_a));
        assert!(
            selected
                .workspace
                .panel_layout
                .active_panel(crate::application::DockSlot::Right)
                .is_none(),
            "opening a conversation must keep optional inspector panels closed"
        );

        let switched = app
            .execute_command(DesktopCommand::SelectProject(project_b))
            .unwrap();
        assert_eq!(switched.workspace.selected_task, None);
        assert!(switched.workspace.tasks.is_empty());
        assert!(switched.workspace.revision > selected.workspace.revision);
    }

    #[test]
    fn project_surface_items_drive_project_selection_and_restore_current_product_facts() {
        let app = application();
        let project_a = ProjectId::new("surface-project-a").unwrap();
        let project_b = ProjectId::new("surface-project-b").unwrap();
        let client = app.authority().client().unwrap();
        for (project_id, name) in [(&project_a, "A"), (&project_b, "B")] {
            client
                .products()
                .create_entity(ProductEntity::Project(
                    Project::new(project_id.clone(), name).unwrap(),
                ))
                .unwrap();
        }

        let session = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:project-surface-source").unwrap(),
        );
        session
            .execute(DesktopCommand::SelectProject(project_a))
            .unwrap();
        let item = app
            .project_workspace_item(&project_b, ProjectWorkspaceSurface::Memory)
            .unwrap();
        assert!(item.capabilities.movable_across_windows);
        let item_id = item.id.clone();
        let opened = session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: crate::application::PaneId::new("primary").unwrap(),
                item,
            })
            .unwrap();
        assert_eq!(opened.workspace.selected_project, Some(project_b.clone()));
        assert_eq!(opened.workspace.selected_task, None);
        assert_eq!(
            opened.workspace.workspace_items[0]
                .project_surface()
                .unwrap(),
            Some((project_b.clone(), ProjectWorkspaceSurface::Memory))
        );

        session
            .execute(DesktopCommand::UpdateWorkspaceItemState {
                item_id: item_id.clone(),
                serialized_state: Some(serde_json::json!({ "selectedMemoryId": "memory-7" })),
            })
            .unwrap();
        let target = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:project-surface-target").unwrap(),
        );
        let transferred = session
            .transfer_item_to(
                &target,
                &item_id,
                &crate::application::PaneId::new("primary").unwrap(),
                None,
            )
            .unwrap();
        assert!(transferred.source.workspace_items.is_empty());
        assert_eq!(transferred.target.workspace_items.len(), 1);
        assert_eq!(
            transferred.target.workspace_items[0].serialized_state,
            Some(serde_json::json!({ "selectedMemoryId": "memory-7" }))
        );
        let persisted = target.persisted_state().unwrap();
        app.update_project(
            &project_b,
            DesktopProjectPatch {
                name: Some("B renamed".to_owned()),
                ..DesktopProjectPatch::default()
            },
        )
        .unwrap();

        let restored = app
            .create_workspace_session(
                DesktopWorkspaceSessionId::new("window:project-surface-restored").unwrap(),
            )
            .restore(&persisted)
            .unwrap();
        assert_eq!(restored.workspace.selected_project, Some(project_b.clone()));
        assert_eq!(restored.workspace.workspace_items.len(), 1);
        assert_eq!(restored.workspace.workspace_items[0].id, item_id);
        assert_eq!(
            restored.workspace.workspace_items[0].title,
            "B renamed · 记忆"
        );
        assert_eq!(
            restored.workspace.workspace_items[0].serialized_state,
            Some(serde_json::json!({ "selectedMemoryId": "memory-7" }))
        );

        app.update_project(
            &project_b,
            DesktopProjectPatch {
                archived: Some(true),
                ..DesktopProjectPatch::default()
            },
        )
        .unwrap();
        let after_archive = app
            .create_workspace_session(
                DesktopWorkspaceSessionId::new("window:project-surface-archived").unwrap(),
            )
            .restore(&persisted)
            .unwrap();
        assert!(after_archive.workspace.workspace_items.is_empty());
        assert_eq!(
            after_archive
                .workspace
                .panel_layout
                .active_workspace_item()
                .unwrap(),
            None
        );
    }

    #[test]
    fn application_surface_items_transfer_and_restore_identity_and_view_state() {
        let app = application();
        let session = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:application-surface-source").unwrap(),
        );
        let item = app
            .application_workspace_item(ApplicationWorkspaceSurface::Automations)
            .unwrap();
        let item_id = item.id.clone();
        let opened = session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: crate::application::PaneId::new("primary").unwrap(),
                item,
            })
            .unwrap();
        assert_eq!(
            opened.workspace.workspace_items[0]
                .application_surface()
                .unwrap(),
            Some(ApplicationWorkspaceSurface::Automations)
        );
        assert_eq!(opened.workspace.selected_task, None);

        session
            .execute(DesktopCommand::UpdateWorkspaceItemState {
                item_id: item_id.clone(),
                serialized_state: Some(serde_json::json!({ "selectedWorkflowId": "workflow-1" })),
            })
            .unwrap();
        let reopened = session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: crate::application::PaneId::new("primary").unwrap(),
                item: app
                    .application_workspace_item(ApplicationWorkspaceSurface::Automations)
                    .unwrap(),
            })
            .unwrap();
        assert_eq!(
            reopened.workspace.workspace_items[0].serialized_state,
            Some(serde_json::json!({ "selectedWorkflowId": "workflow-1" }))
        );
        let target = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:application-surface-target").unwrap(),
        );
        let transferred = session
            .transfer_item_to(
                &target,
                &item_id,
                &crate::application::PaneId::new("primary").unwrap(),
                None,
            )
            .unwrap();
        assert!(transferred.source.workspace_items.is_empty());
        assert_eq!(transferred.target.workspace_items.len(), 1);
        assert_eq!(
            transferred.target.workspace_items[0].serialized_state,
            Some(serde_json::json!({ "selectedWorkflowId": "workflow-1" }))
        );
        let persisted = target.persisted_state().unwrap();
        let restored = app
            .create_workspace_session(
                DesktopWorkspaceSessionId::new("window:application-surface-restored").unwrap(),
            )
            .restore(&persisted)
            .unwrap();
        let restored_item = &restored.workspace.workspace_items[0];
        assert_eq!(restored_item.id, item_id);
        assert_eq!(
            restored_item.resource_id.as_str(),
            "application:automations"
        );
        assert_eq!(
            restored_item.serialized_state,
            Some(serde_json::json!({ "selectedWorkflowId": "workflow-1" }))
        );
        assert!(restored_item.capabilities.movable_across_windows);
    }

    #[test]
    fn inbox_selection_and_orphan_task_tabs_restore_without_a_synthetic_project() {
        let app = application();
        let project_id = ProjectId::new("inbox-project").unwrap();
        let task_id = TaskId::new("inbox-task").unwrap();
        let client = app.authority().client().unwrap();
        client
            .products()
            .create_entity(ProductEntity::Project(
                Project::new(project_id.clone(), "Project").unwrap(),
            ))
            .unwrap();
        client
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task_id.clone(), None, "Inbox task").unwrap(),
            ))
            .unwrap();

        let session = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:inbox-source").unwrap(),
        );
        session
            .execute(DesktopCommand::SelectProject(project_id))
            .unwrap();
        let inbox = session.execute(DesktopCommand::SelectInbox).unwrap();
        assert!(inbox.workspace.inbox_selected);
        assert_eq!(inbox.workspace.selected_project, None);
        assert_eq!(inbox.workspace.tasks.len(), 1);
        assert_eq!(inbox.workspace.tasks[0].id, task_id);

        let item = app.task_workspace_item(&task_id).unwrap();
        let opened = session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: crate::application::PaneId::new("primary").unwrap(),
                item,
            })
            .unwrap();
        assert!(opened.workspace.inbox_selected);
        assert_eq!(opened.workspace.selected_project, None);
        assert_eq!(opened.workspace.selected_task, Some(task_id.clone()));

        let persisted = session.persisted_state().unwrap();
        assert!(persisted.inbox_selected);
        let restored = app
            .create_workspace_session(
                DesktopWorkspaceSessionId::new("window:inbox-restored").unwrap(),
            )
            .restore(&persisted)
            .unwrap();
        assert!(restored.workspace.inbox_selected);
        assert_eq!(restored.workspace.selected_project, None);
        assert_eq!(restored.workspace.selected_task, Some(task_id));
    }

    #[test]
    fn task_tabs_drive_cross_project_selection_and_neighbor_activation() {
        let app = application();
        let project_a = ProjectId::new("tab-project-a").unwrap();
        let project_b = ProjectId::new("tab-project-b").unwrap();
        let task_a = TaskId::new("tab-task-a").unwrap();
        let task_b = TaskId::new("tab-task-b").unwrap();
        let client = app.authority().client().unwrap();
        for (project_id, name) in [
            (project_a.clone(), "Tab project A"),
            (project_b.clone(), "Tab project B"),
        ] {
            client
                .products()
                .create_entity(ProductEntity::Project(
                    Project::new(project_id, name).unwrap(),
                ))
                .unwrap();
        }
        for (task_id, project_id, title) in [
            (task_a.clone(), project_a.clone(), "Tab task A"),
            (task_b.clone(), project_b.clone(), "Tab task B"),
        ] {
            client
                .products()
                .create_entity(ProductEntity::Task(
                    ProductTask::new(task_id, Some(project_id), title).unwrap(),
                ))
                .unwrap();
        }
        let session =
            app.create_workspace_session(DesktopWorkspaceSessionId::new("window:tabs").unwrap());
        let pane_id = crate::application::PaneId::new("primary").unwrap();
        session
            .execute(DesktopCommand::SelectProject(project_a.clone()))
            .unwrap();
        let item_a = app.task_workspace_item(&task_a).unwrap();
        let item_a_id = item_a.id.clone();
        session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: pane_id.clone(),
                item: item_a,
            })
            .unwrap();
        let item_b = app.task_workspace_item(&task_b).unwrap();
        let item_b_id = item_b.id.clone();
        let opened_b = session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: pane_id.clone(),
                item: item_b,
            })
            .unwrap();
        assert_eq!(opened_b.workspace.selected_project, Some(project_b.clone()));
        assert_eq!(opened_b.workspace.selected_task, Some(task_b.clone()));

        let activated_a = session
            .execute(DesktopCommand::ActivateWorkspaceItem {
                pane_id: pane_id.clone(),
                item_id: item_a_id.clone(),
            })
            .unwrap();
        assert_eq!(activated_a.workspace.selected_project, Some(project_a));
        assert_eq!(activated_a.workspace.selected_task, Some(task_a));
        assert_eq!(
            activated_a.workspace.panel_layout.active_workspace_item(),
            Ok(Some(&item_a_id))
        );

        let closed_a = session
            .execute(DesktopCommand::CloseWorkspaceItem {
                pane_id: pane_id.clone(),
                item_id: item_a_id,
            })
            .unwrap();
        assert_eq!(closed_a.workspace.selected_project, Some(project_b));
        assert_eq!(closed_a.workspace.selected_task, Some(task_b.clone()));
        assert_eq!(
            closed_a.workspace.panel_layout.active_workspace_item(),
            Ok(Some(&item_b_id))
        );

        let overview = session.execute(DesktopCommand::BackToTaskList).unwrap();
        assert_eq!(overview.workspace.selected_task, None);
        assert_eq!(
            overview.workspace.panel_layout.active_workspace_item(),
            Ok(None)
        );
        assert_eq!(overview.workspace.workspace_items.len(), 1);

        let reopened = session
            .execute(DesktopCommand::ActivateWorkspaceItem {
                pane_id,
                item_id: item_b_id,
            })
            .unwrap();
        assert_eq!(reopened.workspace.selected_task, Some(task_b));
    }

    #[test]
    fn pane_focus_and_item_move_drive_the_visible_task_without_copying_items() {
        let app = application();
        let project = ProjectId::new("pane-project").unwrap();
        let task_a = TaskId::new("pane-task-a").unwrap();
        let task_b = TaskId::new("pane-task-b").unwrap();
        let client = app.authority().client().unwrap();
        client
            .products()
            .create_entity(ProductEntity::Project(
                Project::new(project.clone(), "Pane project").unwrap(),
            ))
            .unwrap();
        for (task_id, title) in [
            (task_a.clone(), "Pane task A"),
            (task_b.clone(), "Pane task B"),
        ] {
            client
                .products()
                .create_entity(ProductEntity::Task(
                    ProductTask::new(task_id, Some(project.clone()), title).unwrap(),
                ))
                .unwrap();
        }
        let session =
            app.create_workspace_session(DesktopWorkspaceSessionId::new("window:panes").unwrap());
        let primary = crate::application::PaneId::new("primary").unwrap();
        let secondary = crate::application::PaneId::new("secondary").unwrap();
        let item_a = app.task_workspace_item(&task_a).unwrap();
        let item_a_id = item_a.id.clone();
        let item_b = app.task_workspace_item(&task_b).unwrap();
        let item_b_id = item_b.id.clone();
        session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: primary.clone(),
                item: item_a,
            })
            .unwrap();
        session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: primary.clone(),
                item: item_b,
            })
            .unwrap();
        session
            .execute(DesktopCommand::SplitPane {
                pane_id: primary.clone(),
                new_pane_id: secondary.clone(),
                axis: crate::application::SplitAxis::Horizontal,
                ratio: 0.5,
            })
            .unwrap();

        let moved = session
            .execute(DesktopCommand::MoveWorkspaceItem {
                item_id: item_b_id.clone(),
                target_pane_id: secondary.clone(),
                before: None,
            })
            .unwrap();
        assert_eq!(moved.workspace.selected_task, Some(task_b.clone()));
        assert_eq!(moved.workspace.workspace_items.len(), 2);
        assert_eq!(moved.workspace.panel_layout.active_pane(), &secondary);

        let focused_primary = session.execute(DesktopCommand::FocusPane(primary)).unwrap();
        assert_eq!(focused_primary.workspace.selected_task, Some(task_a));
        assert_eq!(
            focused_primary
                .workspace
                .panel_layout
                .active_workspace_item(),
            Ok(Some(&item_a_id))
        );
        let focused_secondary = session
            .execute(DesktopCommand::FocusPane(secondary))
            .unwrap();
        assert_eq!(focused_secondary.workspace.selected_task, Some(task_b));
        assert_eq!(
            focused_secondary
                .workspace
                .panel_layout
                .active_workspace_item(),
            Ok(Some(&item_b_id))
        );
    }

    #[test]
    fn multiple_view_instances_share_one_task_resource_and_restore_independently() {
        let app = application();
        let project_id = ProjectId::new("multi-view-project").unwrap();
        let task_id = TaskId::new("multi-view-task").unwrap();
        let client = app.authority().client().unwrap();
        client
            .products()
            .create_entity(ProductEntity::Project(
                Project::new(project_id.clone(), "Multi view").unwrap(),
            ))
            .unwrap();
        client
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task_id.clone(), Some(project_id), "Shared task").unwrap(),
            ))
            .unwrap();
        let session = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:multi-view-source").unwrap(),
        );
        let primary = crate::application::PaneId::new("primary").unwrap();
        let secondary = crate::application::PaneId::new("secondary").unwrap();
        let first = app.task_workspace_item(&task_id).unwrap();
        let mut second = first.clone();
        second.id = WorkspaceItemId::new("task-view:multi-view-task:second").unwrap();
        session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: primary.clone(),
                item: first,
            })
            .unwrap();
        session
            .execute(DesktopCommand::SplitPane {
                pane_id: primary,
                new_pane_id: secondary.clone(),
                axis: crate::application::SplitAxis::Horizontal,
                ratio: 0.5,
            })
            .unwrap();
        session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: secondary,
                item: second,
            })
            .unwrap();

        let persisted = session.persisted_state().unwrap();
        assert_eq!(persisted.workspace_items.len(), 2);
        assert!(persisted.workspace_items.iter().all(|item| {
            item.resource_id
                .as_ref()
                .is_some_and(|resource_id| resource_id.as_str() == "task:multi-view-task")
        }));
        let restored = app
            .create_workspace_session(
                DesktopWorkspaceSessionId::new("window:multi-view-restored").unwrap(),
            )
            .restore(&persisted)
            .unwrap();
        assert_eq!(restored.workspace.workspace_items.len(), 2);
        assert!(restored.workspace.workspace_items.iter().all(|item| {
            item.resource_id.as_str() == "task:multi-view-task"
                && item.task_id() == Ok(Some(task_id.clone()))
        }));
    }

    #[test]
    fn document_editor_views_share_one_buffer_and_restore_ui_state_only() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("lilia-editor-share-{stamp}.txt"));
        fs::write(&path, "shared").unwrap();
        let path = fs::canonicalize(&path).unwrap();

        let app = application();
        let (document, created) = app.open_document_at_path(&path).unwrap();
        assert!(created);
        let first = app.document_workspace_item(document.id).unwrap();
        let second = app
            .document_workspace_item_view(
                document.id,
                crate::application::WorkspaceItemId::new("document-view:shared:second").unwrap(),
            )
            .unwrap();
        assert_eq!(first.resource_id, second.resource_id);
        assert_ne!(first.id, second.id);

        let session = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:document-share").unwrap(),
        );
        let primary = crate::application::PaneId::new("primary").unwrap();
        let secondary = crate::application::PaneId::new("secondary").unwrap();
        session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: primary.clone(),
                item: first.clone(),
            })
            .unwrap();
        session
            .execute(DesktopCommand::SplitPane {
                pane_id: primary,
                new_pane_id: secondary.clone(),
                axis: crate::application::SplitAxis::Horizontal,
                ratio: 0.5,
            })
            .unwrap();
        session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: secondary,
                item: second.with_serialized_state(Some(serde_json::json!({
                    "scrollOffset": 24.0,
                    "selectionStart": 1,
                    "selectionEnd": 3
                }))),
            })
            .unwrap();

        let revision = app
            .edit_document(
                document.id,
                document.buffer.revision,
                vec![crate::application::TextEdit::new(0..6, "edited")],
            )
            .unwrap();
        assert_eq!(
            app.document_snapshot(document.id).unwrap().buffer.text,
            "edited"
        );
        app.save_document(document.id, revision).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "edited");
        assert!(!app
            .document_snapshot(document.id)
            .unwrap()
            .buffer
            .is_dirty());

        let persisted = session.persisted_state().unwrap();
        assert_eq!(persisted.workspace_items.len(), 2);
        assert!(persisted.workspace_items.iter().all(|item| {
            item.kind.as_str() == crate::application::DOCUMENT_WORKSPACE_ITEM_KIND
                && item
                    .resource_id
                    .as_ref()
                    .is_some_and(|resource| resource == &first.resource_id)
        }));
        let restored = app
            .create_workspace_session(
                DesktopWorkspaceSessionId::new("window:document-share-restored").unwrap(),
            )
            .restore(&persisted)
            .unwrap();
        assert_eq!(restored.workspace.workspace_items.len(), 2);
        let restored_second = restored
            .workspace
            .workspace_items
            .iter()
            .find(|item| item.id.as_str() == "document-view:shared:second")
            .unwrap();
        assert_eq!(
            restored_second.serialized_state,
            Some(serde_json::json!({
                "scrollOffset": 24.0,
                "selectionStart": 1,
                "selectionEnd": 3
            }))
        );
        assert_eq!(
            restored_second
                .serialized_state
                .as_ref()
                .and_then(|state| state.get("text")),
            None
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn invalid_task_selection_does_not_mutate_workspace_state() {
        let app = application();
        let project = ProjectId::new("project").unwrap();
        app.authority()
            .client()
            .unwrap()
            .products()
            .create_entity(ProductEntity::Project(
                Project::new(project, "Project").unwrap(),
            ))
            .unwrap();
        let before = app
            .execute_command(DesktopCommand::RefreshWorkspace)
            .unwrap()
            .workspace;

        assert!(app
            .execute_command(DesktopCommand::SelectTask(
                TaskId::new("missing-task").unwrap()
            ))
            .is_err());
        assert_eq!(app.workspace_snapshot().unwrap(), before);
    }

    #[test]
    fn window_scoped_sessions_keep_selection_and_layout_independent() {
        let app = application();
        let project_a = ProjectId::new("project-session-a").unwrap();
        let project_b = ProjectId::new("project-session-b").unwrap();
        let client = app.authority().client().unwrap();
        for (id, name) in [(project_a.clone(), "A"), (project_b.clone(), "B")] {
            client
                .products()
                .create_entity(ProductEntity::Project(Project::new(id, name).unwrap()))
                .unwrap();
        }
        let first =
            app.create_workspace_session(DesktopWorkspaceSessionId::new("window:first").unwrap());
        let second =
            app.create_workspace_session(DesktopWorkspaceSessionId::new("window:second").unwrap());
        first.execute(DesktopCommand::RefreshWorkspace).unwrap();
        second.execute(DesktopCommand::RefreshWorkspace).unwrap();

        first
            .execute(DesktopCommand::SelectProject(project_a.clone()))
            .unwrap();
        second
            .execute(DesktopCommand::SelectProject(project_b.clone()))
            .unwrap();
        let mut layout = PanelLayoutSnapshot::default();
        layout.panels[0].extent = 384.0;
        first
            .execute(DesktopCommand::ReplacePanelLayout(layout.clone()))
            .unwrap();

        assert_eq!(first.id().as_str(), "window:first");
        assert_eq!(first.snapshot().unwrap().selected_project, Some(project_a));
        assert_eq!(first.snapshot().unwrap().panel_layout, layout);
        assert_eq!(second.snapshot().unwrap().selected_project, Some(project_b));
        assert_eq!(
            second.snapshot().unwrap().panel_layout,
            PanelLayoutSnapshot::default()
        );
    }

    #[test]
    fn dock_commands_are_persisted_per_workspace_session() {
        let app = application();
        let first =
            app.create_workspace_session(DesktopWorkspaceSessionId::new("dock:first").unwrap());
        let second =
            app.create_workspace_session(DesktopWorkspaceSessionId::new("dock:second").unwrap());
        let tools =
            crate::application::PanelId::new(crate::application::CODING_TOOLS_PANEL_ID).unwrap();

        first
            .execute(DesktopCommand::ActivatePanel(tools.clone()))
            .unwrap();
        first
            .execute(DesktopCommand::ResizePanel {
                panel_id: tools.clone(),
                extent: 418.0,
            })
            .unwrap();

        let first_snapshot = first.snapshot().unwrap();
        assert_eq!(
            first_snapshot
                .panel_layout
                .active_panel(crate::application::DockSlot::Right)
                .map(|panel| panel.id.as_str()),
            Some(crate::application::CODING_TOOLS_PANEL_ID)
        );
        assert_eq!(
            first_snapshot.panel_layout.panel(&tools).unwrap().extent,
            418.0
        );
        assert!(second
            .snapshot()
            .unwrap()
            .panel_layout
            .active_panel(crate::application::DockSlot::Right)
            .is_none());

        first
            .execute(DesktopCommand::SetPanelVisible {
                panel_id: tools,
                visible: false,
            })
            .unwrap();
        assert!(first
            .snapshot()
            .unwrap()
            .panel_layout
            .active_panel(crate::application::DockSlot::Right)
            .is_none());
    }

    #[test]
    fn persisted_session_state_restores_only_valid_ui_references() {
        let app = application();
        let project = ProjectId::new("project-restored").unwrap();
        let task = TaskId::new("task-restored").unwrap();
        let client = app.authority().client().unwrap();
        client
            .products()
            .create_entity(ProductEntity::Project(
                Project::new(project.clone(), "Restored").unwrap(),
            ))
            .unwrap();
        client
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task.clone(), Some(project.clone()), "Restored task").unwrap(),
            ))
            .unwrap();
        let mut layout = PanelLayoutSnapshot::default();
        layout.panels[0].extent = 320.0;
        layout
            .open_item(
                &crate::application::PaneId::new("primary").unwrap(),
                WorkspaceItemId::new("task:task-restored").unwrap(),
            )
            .unwrap();
        let persisted = DesktopWorkspaceSessionState {
            schema_version: 1,
            revision: 7,
            selected_project: Some(project.clone()),
            inbox_selected: false,
            selected_task: Some(task.clone()),
            workspace_items: Vec::new(),
            panel_layout: layout.clone(),
        };
        let restored = app
            .create_workspace_session(DesktopWorkspaceSessionId::new("window:restored").unwrap());

        let outcome = restored.restore(&persisted).unwrap();
        assert_eq!(outcome.workspace.selected_project, Some(project));
        assert_eq!(outcome.workspace.selected_task, Some(task));
        assert_eq!(outcome.workspace.panel_layout, layout);
        assert_eq!(outcome.workspace.revision, 8);
        assert_eq!(outcome.workspace.workspace_items.len(), 1);
        let migrated = restored.persisted_state().unwrap();
        assert_eq!(migrated.revision, persisted.revision + 1);
        assert_eq!(migrated.workspace_items.len(), 1);
        assert_eq!(migrated.workspace_items[0].kind.as_str(), "task");
        assert_eq!(
            migrated.workspace_items[0]
                .resource_id
                .as_ref()
                .map(crate::application::WorkspaceResourceId::as_str),
            Some("task:task-restored")
        );

        let missing =
            app.create_workspace_session(DesktopWorkspaceSessionId::new("window:missing").unwrap());
        let missing_state = DesktopWorkspaceSessionState {
            selected_project: Some(ProjectId::new("missing-project").unwrap()),
            selected_task: Some(TaskId::new("missing-task").unwrap()),
            ..DesktopWorkspaceSessionState::default()
        };
        let outcome = missing.restore(&missing_state).unwrap();
        assert_eq!(
            outcome.workspace.selected_project,
            outcome
                .workspace
                .projects
                .first()
                .map(|project| project.id.clone())
        );
        assert_eq!(outcome.workspace.selected_task, None);
        assert_eq!(outcome.workspace.revision, 1);
    }

    #[test]
    fn task_workspace_items_restore_current_product_title_and_ui_state() {
        let app = application();
        let project = ProjectId::new("project-item").unwrap();
        let task = TaskId::new("task-item").unwrap();
        let client = app.authority().client().unwrap();
        client
            .products()
            .create_entity(ProductEntity::Project(
                Project::new(project.clone(), "Items").unwrap(),
            ))
            .unwrap();
        client
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task.clone(), Some(project), "Original title").unwrap(),
            ))
            .unwrap();
        let first =
            app.create_workspace_session(DesktopWorkspaceSessionId::new("window:item-a").unwrap());
        first.execute(DesktopCommand::RefreshWorkspace).unwrap();
        first
            .execute(DesktopCommand::SelectTask(task.clone()))
            .unwrap();
        let item = app.task_workspace_item(&task).unwrap();
        assert_eq!(
            item.kind.as_str(),
            crate::application::TASK_WORKSPACE_ITEM_KIND
        );
        assert_eq!(item.resource_id.as_str(), "task:task-item");
        assert_eq!(item.focus_target.as_str(), "composer");
        assert_eq!(item.title, "Original title");
        assert_eq!(
            item.capabilities,
            crate::application::WorkspaceItemCapabilities::dockable()
        );
        let item_id = item.id.clone();
        first
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: crate::application::PaneId::new("primary").unwrap(),
                item,
            })
            .unwrap();
        first
            .execute(DesktopCommand::UpdateWorkspaceItemState {
                item_id: item_id.clone(),
                serialized_state: Some(serde_json::json!({ "scrollOffset": 24 })),
            })
            .unwrap();
        let persisted = first.persisted_state().unwrap();
        assert_eq!(persisted.workspace_items.len(), 1);

        app.update_task(
            &task,
            crate::application::DesktopTaskPatch {
                title: Some("Renamed in Product Core".to_owned()),
                ..crate::application::DesktopTaskPatch::default()
            },
        )
        .unwrap();
        let restored =
            app.create_workspace_session(DesktopWorkspaceSessionId::new("window:item-b").unwrap());
        let snapshot = restored.restore(&persisted).unwrap().workspace;
        assert_eq!(snapshot.workspace_items.len(), 1);
        assert_eq!(snapshot.workspace_items[0].title, "Renamed in Product Core");
        assert_eq!(
            snapshot.workspace_items[0].serialized_state,
            Some(serde_json::json!({ "scrollOffset": 24 }))
        );
        assert_eq!(
            snapshot
                .panel_layout
                .active_item(&crate::application::PaneId::new("primary").unwrap()),
            Ok(Some(&item_id))
        );
    }

    #[test]
    fn workspace_items_transfer_atomically_between_window_sessions() {
        let app = application();
        let project = ProjectId::new("transfer-project").unwrap();
        let task = TaskId::new("transfer-task").unwrap();
        let client = app.authority().client().unwrap();
        client
            .products()
            .create_entity(ProductEntity::Project(
                Project::new(project.clone(), "Transfer").unwrap(),
            ))
            .unwrap();
        client
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task.clone(), Some(project), "Transfer task").unwrap(),
            ))
            .unwrap();

        let source = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:transfer-source").unwrap(),
        );
        let target = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:transfer-target").unwrap(),
        );
        source.execute(DesktopCommand::RefreshWorkspace).unwrap();
        target.execute(DesktopCommand::RefreshWorkspace).unwrap();
        let pane_id = crate::application::PaneId::new("primary").unwrap();
        let item_id = WorkspaceItemId::new("task-view:transfer-detached").unwrap();
        let item = app
            .task_workspace_item_view(&task, item_id.clone())
            .unwrap();
        source
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: pane_id.clone(),
                item,
            })
            .unwrap();
        source
            .execute(DesktopCommand::UpdateWorkspaceItemState {
                item_id: item_id.clone(),
                serialized_state: Some(serde_json::json!({ "scrollOffset": 48 })),
            })
            .unwrap();
        let source_before = source.snapshot().unwrap();
        let target_before = target.snapshot().unwrap();

        let transferred = source
            .transfer_item_to(&target, &item_id, &pane_id, None)
            .unwrap();

        assert_eq!(transferred.source.revision, source_before.revision + 1);
        assert_eq!(transferred.target.revision, target_before.revision + 1);
        assert!(transferred.source.workspace_items.is_empty());
        assert!(!transferred.source.panel_layout.contains_item(&item_id));
        assert_eq!(transferred.source.selected_task, None);
        assert_eq!(transferred.target.selected_task, Some(task));
        assert_eq!(transferred.target.workspace_items.len(), 1);
        assert_eq!(transferred.target.workspace_items[0].id, item_id);
        assert_eq!(
            transferred.target.workspace_items[0].resource_id.as_str(),
            "task:transfer-task"
        );
        assert_eq!(
            transferred.target.workspace_items[0].serialized_state,
            Some(serde_json::json!({ "scrollOffset": 48 }))
        );
        assert_eq!(
            target.persisted_state().unwrap().workspace_items[0].id,
            transferred.target.workspace_items[0].id
        );
    }

    #[test]
    fn failed_cross_window_transfer_keeps_both_sessions_unchanged() {
        let app = application();
        let task = TaskId::new("transfer-atomic-task").unwrap();
        app.authority()
            .client()
            .unwrap()
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task.clone(), None, "Atomic transfer").unwrap(),
            ))
            .unwrap();
        let source = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:atomic-source").unwrap(),
        );
        let target = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:atomic-target").unwrap(),
        );
        let primary = crate::application::PaneId::new("primary").unwrap();
        let item = app.task_workspace_item(&task).unwrap();
        let item_id = item.id.clone();
        source
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: primary,
                item,
            })
            .unwrap();
        let source_before = source.snapshot().unwrap();
        let target_before = target.snapshot().unwrap();

        assert!(source
            .transfer_item_to(
                &target,
                &item_id,
                &crate::application::PaneId::new("missing").unwrap(),
                None,
            )
            .is_err());
        assert_eq!(source.snapshot().unwrap(), source_before);
        assert_eq!(target.snapshot().unwrap(), target_before);
    }

    #[test]
    fn workspace_item_capabilities_are_enforced_by_command_routing() {
        let app = application();
        let session =
            app.create_workspace_session(DesktopWorkspaceSessionId::new("window:locked").unwrap());
        let item = WorkspaceItem::new(
            WorkspaceItemId::new("tool:locked").unwrap(),
            crate::application::WorkspaceResourceId::new("tool:locked").unwrap(),
            crate::application::WorkspaceItemKind::new("tool").unwrap(),
            "Locked tool",
            crate::application::WorkspaceFocusTarget::new("primary").unwrap(),
            crate::application::WorkspaceItemCapabilities {
                closable: false,
                splittable: false,
                movable_across_windows: false,
                persistent: false,
            },
        )
        .unwrap();
        let item_id = item.id.clone();
        let pane_id = crate::application::PaneId::new("primary").unwrap();
        session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: pane_id.clone(),
                item,
            })
            .unwrap();
        let persisted = session.persisted_state().unwrap();
        assert!(persisted.workspace_items.is_empty());
        assert!(persisted.panel_layout.item_ids().is_empty());

        assert!(matches!(
            session.execute(DesktopCommand::CloseWorkspaceItem {
                pane_id: pane_id.clone(),
                item_id: item_id.clone(),
            }),
            Err(WorkspaceSessionError::Item(
                WorkspaceItemError::NotClosable(_)
            ))
        ));
        assert!(matches!(
            session.execute(DesktopCommand::SplitPane {
                pane_id,
                new_pane_id: crate::application::PaneId::new("secondary").unwrap(),
                axis: crate::application::SplitAxis::Horizontal,
                ratio: 0.5,
            }),
            Err(WorkspaceSessionError::Item(
                WorkspaceItemError::NotSplittable(_)
            ))
        ));

        let move_session = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:locked-move").unwrap(),
        );
        let primary = crate::application::PaneId::new("primary").unwrap();
        let secondary = crate::application::PaneId::new("secondary").unwrap();
        move_session
            .execute(DesktopCommand::SplitPane {
                pane_id: primary.clone(),
                new_pane_id: secondary.clone(),
                axis: crate::application::SplitAxis::Horizontal,
                ratio: 0.5,
            })
            .unwrap();
        let locked_item = WorkspaceItem::new(
            WorkspaceItemId::new("tool:locked-move").unwrap(),
            crate::application::WorkspaceResourceId::new("tool:locked-move").unwrap(),
            crate::application::WorkspaceItemKind::new("tool").unwrap(),
            "Locked move tool",
            crate::application::WorkspaceFocusTarget::new("primary").unwrap(),
            crate::application::WorkspaceItemCapabilities {
                closable: true,
                splittable: false,
                movable_across_windows: false,
                persistent: false,
            },
        )
        .unwrap();
        let locked_item_id = locked_item.id.clone();
        move_session
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: primary,
                item: locked_item,
            })
            .unwrap();
        assert!(matches!(
            move_session.execute(DesktopCommand::MoveWorkspaceItem {
                item_id: locked_item_id,
                target_pane_id: secondary,
                before: None,
            }),
            Err(WorkspaceSessionError::Item(
                WorkspaceItemError::NotSplittable(_)
            ))
        ));

        let target = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:locked-transfer-target").unwrap(),
        );
        assert!(matches!(
            move_session.transfer_item_to(
                &target,
                &WorkspaceItemId::new("tool:locked-move").unwrap(),
                &crate::application::PaneId::new("primary").unwrap(),
                None,
            ),
            Err(WorkspaceSessionError::Item(
                WorkspaceItemError::NotMovableAcrossWindows(_)
            ))
        ));
    }
}

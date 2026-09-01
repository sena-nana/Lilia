//! Delegation onto the project and task feature.
//!
//! The domain rules live in `lilia-feature-task`; these wrappers exist only
//! while the shell still calls through `DesktopApplication`, and disappear with
//! it once every caller resolves the service from the kernel.

use std::sync::Arc;

use lilia_contracts::{
    ProductProjectArchiveOutcome, ProductProjectRemovalOutcome, ProductTask,
    ProductTaskArchiveOutcome, ProjectId, TaskId,
};
use lilia_feature_task::{
    DesktopProjectCreate, DesktopProjectPatch, DesktopProjectRemovalPreview, DesktopTaskCreate,
    DesktopTaskMove, DesktopTaskPatch, DesktopTaskRunBlock, ProjectTaskService,
};

use crate::application::{DesktopApplication, DesktopApplicationError};

impl DesktopApplication {
    pub(crate) fn project_tasks(&self) -> &ProjectTaskService {
        &self.inner.project_tasks
    }

    /// Hands the one project/task service and its event fanout to the kernel so
    /// `TaskFeature` publishes this instance instead of building a second one.
    pub fn project_task_services(
        &self,
    ) -> (
        ProjectTaskService,
        Arc<lilia_feature_task::ProjectTaskEventFanout>,
    ) {
        (
            self.inner.project_tasks.clone(),
            self.inner.project_task_events.clone(),
        )
    }

    /// The log the application already writes to. Handed to the kernel so
    /// lifecycle, job, event and mutation records share one sequence.
    pub fn journal(&self) -> lilia_kernel::Journal {
        self.inner.journal.clone()
    }

    pub fn task_run_block(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopTaskRunBlock>, DesktopApplicationError> {
        Ok(self.project_tasks().task_run_block(task_id)?)
    }

    pub fn ensure_task_runnable(&self, task_id: &TaskId) -> Result<(), DesktopApplicationError> {
        Ok(self.project_tasks().ensure_task_runnable(task_id)?)
    }

    pub fn create_project(
        &self,
        input: DesktopProjectCreate,
    ) -> Result<lilia_contracts::Project, DesktopApplicationError> {
        Ok(self.project_tasks().create_project(input)?)
    }

    pub fn update_project(
        &self,
        project_id: &ProjectId,
        patch: DesktopProjectPatch,
    ) -> Result<lilia_contracts::Project, DesktopApplicationError> {
        Ok(self.project_tasks().update_project(project_id, patch)?)
    }

    pub fn project_removal_preview(
        &self,
        project_id: &ProjectId,
    ) -> Result<DesktopProjectRemovalPreview, DesktopApplicationError> {
        Ok(self.project_tasks().project_removal_preview(project_id)?)
    }

    pub fn remove_project(
        &self,
        project_id: &ProjectId,
    ) -> Result<ProductProjectRemovalOutcome, DesktopApplicationError> {
        Ok(self.project_tasks().remove_project(project_id)?)
    }

    pub fn archive_project_conversations(
        &self,
        project_id: &ProjectId,
    ) -> Result<ProductProjectArchiveOutcome, DesktopApplicationError> {
        Ok(self
            .project_tasks()
            .archive_project_conversations(project_id)?)
    }

    pub fn reorder_projects(
        &self,
        ordered: &[ProjectId],
    ) -> Result<Vec<lilia_contracts::Project>, DesktopApplicationError> {
        Ok(self.project_tasks().reorder_projects(ordered)?)
    }

    pub fn create_task(
        &self,
        input: DesktopTaskCreate,
    ) -> Result<ProductTask, DesktopApplicationError> {
        Ok(self.project_tasks().create_task(input)?)
    }

    pub fn update_task(
        &self,
        task_id: &TaskId,
        patch: DesktopTaskPatch,
    ) -> Result<ProductTask, DesktopApplicationError> {
        Ok(self.project_tasks().update_task(task_id, patch)?)
    }

    pub fn set_task_archived(
        &self,
        task_id: &TaskId,
        archived: bool,
    ) -> Result<ProductTaskArchiveOutcome, DesktopApplicationError> {
        Ok(self.project_tasks().set_task_archived(task_id, archived)?)
    }

    pub fn update_task_dependencies(
        &self,
        task_id: &TaskId,
        dependencies: Vec<TaskId>,
    ) -> Result<ProductTask, DesktopApplicationError> {
        Ok(self
            .project_tasks()
            .update_task_dependencies(task_id, dependencies)?)
    }

    pub fn reorder_tasks(
        &self,
        project_id: Option<ProjectId>,
        ordered: &[TaskId],
    ) -> Result<Vec<ProductTask>, DesktopApplicationError> {
        Ok(self.project_tasks().reorder_tasks(project_id, ordered)?)
    }

    pub fn move_task(
        &self,
        task_id: &TaskId,
        target: DesktopTaskMove,
    ) -> Result<ProductTask, DesktopApplicationError> {
        Ok(self.project_tasks().move_task(task_id, target)?)
    }

    pub(crate) fn ensure_task_conversation(
        &self,
        task: &ProductTask,
        title: &str,
    ) -> Result<(), DesktopApplicationError> {
        Ok(self.project_tasks().ensure_task_conversation(task, title)?)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use lilia_contracts::{
        ExpectedRevision, ProductConversationStatus, ProductEntity, ProductEntityKind,
        ProductTaskStatus, ProjectArchiveState, TaskId,
    };
    use lilia_feature_task::update_meta;
    use lilia_service::ServiceAuthority;
    use tempfile::tempdir;

    use crate::application::{
        DesktopApplication, DesktopApplicationConfig, DesktopApplicationError, DesktopCommand,
        DesktopHost, DesktopHostAction, DesktopHostContext, DesktopHostError, DesktopHostResult,
        DesktopProjectCreate, DesktopProjectPatch, DesktopTaskCreate, DesktopTaskMove,
        DesktopTaskPatch, DesktopTaskRunBlock, ProjectQuery, ProjectsChanged, TaskQuery,
        TasksChanged,
    };

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    struct NoopHost;

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
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:product-management:{id}"),
            format!("product-management-test:{id}"),
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                "C:/lilia/product-management",
                format!("liliacode.product-management-test.{id}"),
            )
            .unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap()
    }

    #[test]
    fn project_and_task_commands_update_workspace_without_ui_owned_rows() {
        let app = application();
        let project = app
            .create_project(DesktopProjectCreate::new("Native IDE"))
            .unwrap();
        let task = app
            .create_task(DesktopTaskCreate::new(
                Some(project.id.clone()),
                "Build editor item",
            ))
            .unwrap();

        let workspace = app
            .execute_command(DesktopCommand::RefreshWorkspace)
            .unwrap()
            .workspace;
        assert_eq!(workspace.projects.len(), 1);
        assert_eq!(workspace.tasks.len(), 1);
        assert_eq!(workspace.tasks[0].id, task.id);
        assert_eq!(
            app.authority()
                .client()
                .unwrap()
                .products()
                .list_entities(ProductEntityKind::Conversation)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn mutations_are_noop_safe_and_archives_leave_product_facts_durable() {
        let app = application();
        let project = app
            .create_project(DesktopProjectCreate::new("Initial"))
            .unwrap();
        let renamed = app
            .update_project(
                &project.id,
                DesktopProjectPatch {
                    name: Some("Renamed".to_owned()),
                    pinned: Some(true),
                    ..DesktopProjectPatch::default()
                },
            )
            .unwrap();
        let replay = app
            .update_project(
                &project.id,
                DesktopProjectPatch {
                    name: Some("Renamed".to_owned()),
                    pinned: Some(true),
                    ..DesktopProjectPatch::default()
                },
            )
            .unwrap();
        assert_eq!(replay.revision, renamed.revision);

        let task = app
            .create_task(DesktopTaskCreate::new(Some(project.id.clone()), "Task"))
            .unwrap();
        let archived = app.set_task_archived(&task.id, true).unwrap();
        assert!(archived.task.archived);
        assert!(archived.conversations.iter().all(|conversation| {
            conversation.archived && conversation.status == ProductConversationStatus::Closed
        }));
        assert!(app
            .query_tasks(TaskQuery::for_project(project.id.clone()))
            .unwrap()
            .is_empty());
        assert_eq!(
            app.query_tasks(TaskQuery::for_project(project.id.clone()).including_archived())
                .unwrap()
                .len(),
            1
        );

        app.update_project(
            &project.id,
            DesktopProjectPatch {
                archived: Some(true),
                ..DesktopProjectPatch::default()
            },
        )
        .unwrap();
        assert!(app
            .query_projects(ProjectQuery::default())
            .unwrap()
            .is_empty());
        assert_eq!(
            app.query_projects(ProjectQuery {
                include_archived: true,
            })
            .unwrap()
            .len(),
            1
        );
    }

    #[test]
    fn project_conversation_archive_updates_only_the_current_active_fact_set() {
        let app = application();
        let project = app
            .create_project(DesktopProjectCreate::new("Archive conversations"))
            .unwrap();
        let first = app
            .create_task(DesktopTaskCreate::new(Some(project.id.clone()), "First"))
            .unwrap();
        let second = app
            .create_task(DesktopTaskCreate::new(Some(project.id.clone()), "Second"))
            .unwrap();
        let other_project = app
            .create_project(DesktopProjectCreate::new("Keep conversations"))
            .unwrap();
        let other = app
            .create_task(DesktopTaskCreate::new(
                Some(other_project.id.clone()),
                "Other",
            ))
            .unwrap();

        let outcome = app.archive_project_conversations(&project.id).unwrap();

        assert_eq!(outcome.archived_tasks.len(), 2);
        assert_eq!(outcome.archived_conversations.len(), 2);
        assert!(outcome.archived_tasks.iter().all(|task| task.archived));
        assert!(outcome.archived_conversations.iter().all(|conversation| {
            conversation.archived && conversation.status == ProductConversationStatus::Closed
        }));
        assert!(app
            .query_tasks(TaskQuery::for_project(project.id.clone()))
            .unwrap()
            .is_empty());
        assert!(app.get_task(&first.id).unwrap().archived);
        assert!(app.get_task(&second.id).unwrap().archived);
        assert!(!app.get_task(&other.id).unwrap().archived);
        assert_eq!(
            app.get_project(&project.id).unwrap().archive,
            ProjectArchiveState::Active
        );

        let replay = app.archive_project_conversations(&project.id).unwrap();
        assert!(replay.archived_tasks.is_empty());
        assert!(replay.archived_conversations.is_empty());

        let restored = app.set_task_archived(&first.id, false).unwrap();
        assert!(!restored.task.archived);
        assert!(restored.conversations.iter().all(|conversation| {
            !conversation.archived && conversation.status == ProductConversationStatus::Active
        }));
    }

    #[test]
    fn remove_project_atomically_moves_active_facts_to_inbox_and_preserves_workspace() {
        let app = application();
        let workspace = tempdir().unwrap();
        let sentinel = workspace.path().join("keep.txt");
        std::fs::write(&sentinel, "keep").unwrap();
        let project = app
            .create_project(DesktopProjectCreate {
                workspace_path: Some(workspace.path().display().to_string()),
                ..DesktopProjectCreate::new("Remove me")
            })
            .unwrap();
        let parent = app
            .create_task(DesktopTaskCreate {
                id: TaskId::new("task-parent").unwrap(),
                project_id: Some(project.id.clone()),
                parent_id: None,
                title: "Parent".to_owned(),
            })
            .unwrap();
        let child = app
            .create_task(DesktopTaskCreate {
                id: TaskId::new("task-child").unwrap(),
                project_id: Some(project.id.clone()),
                parent_id: None,
                title: "Child".to_owned(),
            })
            .unwrap();
        app.move_task(
            &child.id,
            DesktopTaskMove {
                target_project_id: Some(project.id.clone()),
                target_parent_id: Some(parent.id.clone()),
            },
        )
        .unwrap();
        app.authority()
            .client()
            .unwrap()
            .products()
            .update_task_dependencies(
                &child.id,
                vec![parent.id.clone()],
                ExpectedRevision::new(app.get_task(&child.id).unwrap().revision.get()).unwrap(),
            )
            .unwrap();
        let archived_task = app
            .create_task(DesktopTaskCreate {
                id: TaskId::new("task-archived").unwrap(),
                project_id: Some(project.id.clone()),
                parent_id: None,
                title: "Archived".to_owned(),
            })
            .unwrap();
        app.update_task(
            &archived_task.id,
            DesktopTaskPatch {
                archived: Some(true),
                ..DesktopTaskPatch::default()
            },
        )
        .unwrap();
        let mut archived_conversation = app
            .authority()
            .client()
            .unwrap()
            .products()
            .list_entities(ProductEntityKind::Conversation)
            .unwrap()
            .into_iter()
            .find_map(|entity| match entity {
                ProductEntity::Conversation(conversation)
                    if conversation.task_id.as_ref() == Some(&archived_task.id) =>
                {
                    Some(conversation)
                }
                _ => None,
            })
            .unwrap();
        let archived_conversation_revision = archived_conversation.revision;
        archived_conversation.archived = true;
        app.authority()
            .client()
            .unwrap()
            .update_product_entity(
                &update_meta(
                    "conversation",
                    archived_conversation.id.as_str(),
                    archived_conversation_revision,
                )
                .unwrap(),
                ProductEntity::Conversation(archived_conversation.clone()),
                "test_archive_conversation",
            )
            .unwrap();

        let preview = app.project_removal_preview(&project.id).unwrap();
        assert_eq!(preview.active_task_count, 2);
        assert_eq!(preview.active_conversation_count, 2);
        assert_eq!(preview.workspace_path, project.workspace_path);
        let events = app.subscribe_events();
        let outcome = app.remove_project(&project.id).unwrap();

        assert!(!outcome.already_removed);
        assert_eq!(
            outcome.moved_task_ids,
            vec![child.id.clone(), parent.id.clone()]
        );
        assert_eq!(outcome.moved_conversation_ids.len(), 2);
        assert_eq!(
            app.get_project(&project.id).unwrap().archive,
            ProjectArchiveState::Archived
        );
        let moved_parent = app.get_task(&parent.id).unwrap();
        let moved_child = app.get_task(&child.id).unwrap();
        assert_eq!(moved_parent.project_id, None);
        assert_eq!(moved_child.project_id, None);
        assert_eq!(moved_child.parent_id, Some(parent.id.clone()));
        assert_eq!(moved_child.depends_on, vec![parent.id.clone()]);
        assert_eq!(
            app.get_task(&archived_task.id).unwrap().project_id,
            Some(project.id.clone())
        );
        let conversations = app
            .authority()
            .client()
            .unwrap()
            .products()
            .list_entities(ProductEntityKind::Conversation)
            .unwrap();
        assert!(conversations.iter().all(|entity| match entity {
            ProductEntity::Conversation(conversation) if conversation.archived => {
                conversation.project_id.as_ref() == Some(&project.id)
            }
            ProductEntity::Conversation(conversation) => conversation.project_id.is_none(),
            _ => true,
        }));
        assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "keep");
        let changed = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        assert_eq!(changed.len(), 3);
        assert!(changed[0].is::<ProjectsChanged>());
        assert!(matches!(
            changed[1].downcast::<TasksChanged>(),
            Some(TasksChanged {
                project_id: Some(id),
                task_id: None,
            }) if id == &project.id
        ));
        assert!(matches!(
            changed[2].downcast::<TasksChanged>(),
            Some(TasksChanged {
                project_id: None,
                task_id: None,
            })
        ));

        let replay = app.remove_project(&project.id).unwrap();
        assert!(replay.already_removed);
        assert!(replay.moved_task_ids.is_empty());
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn project_reorder_requires_a_complete_pinned_group_and_persists_order() {
        let app = application();
        let first = app
            .create_project(DesktopProjectCreate::new("First"))
            .unwrap();
        let second = app
            .create_project(DesktopProjectCreate::new("Second"))
            .unwrap();
        let pinned = app
            .create_project(DesktopProjectCreate::new("Pinned"))
            .unwrap();
        app.update_project(
            &pinned.id,
            DesktopProjectPatch {
                pinned: Some(true),
                ..DesktopProjectPatch::default()
            },
        )
        .unwrap();

        let reordered = app
            .reorder_projects(&[second.id.clone(), first.id.clone()])
            .unwrap();
        assert_eq!(
            reordered
                .iter()
                .map(|project| project.id.clone())
                .collect::<Vec<_>>(),
            vec![pinned.id.clone(), second.id.clone(), first.id.clone()]
        );
        assert_eq!(app.get_project(&second.id).unwrap().sort_order, 0);
        assert_eq!(app.get_project(&first.id).unwrap().sort_order, 1);
        let second_revision = app.get_project(&second.id).unwrap().revision;
        let first_revision = app.get_project(&first.id).unwrap().revision;
        app.reorder_projects(&[second.id.clone(), first.id.clone()])
            .unwrap();
        assert_eq!(
            app.get_project(&second.id).unwrap().revision,
            second_revision
        );
        assert_eq!(app.get_project(&first.id).unwrap().revision, first_revision);

        let error = app
            .reorder_projects(std::slice::from_ref(&first.id))
            .unwrap_err();
        assert!(matches!(
            error,
            DesktopApplicationError::InvalidInput {
                field: "ordered_project_ids",
                ..
            }
        ));
        let duplicate = app
            .reorder_projects(&[second.id.clone(), second.id])
            .unwrap_err();
        assert!(matches!(
            duplicate,
            DesktopApplicationError::InvalidInput {
                field: "ordered_project_ids",
                ..
            }
        ));
    }

    #[test]
    fn task_reorder_requires_a_complete_pinned_group_and_persists_order() {
        let app = application();
        let project = app
            .create_project(DesktopProjectCreate::new("Project"))
            .unwrap();
        let first = app
            .create_task(DesktopTaskCreate::new(Some(project.id.clone()), "First"))
            .unwrap();
        let second = app
            .create_task(DesktopTaskCreate::new(Some(project.id.clone()), "Second"))
            .unwrap();
        let pinned = app
            .create_task(DesktopTaskCreate::new(Some(project.id.clone()), "Pinned"))
            .unwrap();
        app.update_task(
            &pinned.id,
            DesktopTaskPatch {
                pinned: Some(true),
                ..DesktopTaskPatch::default()
            },
        )
        .unwrap();

        let reordered = app
            .reorder_tasks(
                Some(project.id.clone()),
                &[second.id.clone(), first.id.clone()],
            )
            .unwrap();
        assert_eq!(
            reordered
                .iter()
                .map(|task| task.id.clone())
                .collect::<Vec<_>>(),
            vec![pinned.id, second.id.clone(), first.id.clone()]
        );
        assert_eq!(app.get_task(&second.id).unwrap().sort_order, 0);
        assert_eq!(app.get_task(&first.id).unwrap().sort_order, 1);
        let second_revision = app.get_task(&second.id).unwrap().revision;
        let first_revision = app.get_task(&first.id).unwrap().revision;
        app.reorder_tasks(
            Some(project.id.clone()),
            &[second.id.clone(), first.id.clone()],
        )
        .unwrap();
        assert_eq!(app.get_task(&second.id).unwrap().revision, second_revision);
        assert_eq!(app.get_task(&first.id).unwrap().revision, first_revision);

        let error = app
            .reorder_tasks(Some(project.id), std::slice::from_ref(&first.id))
            .unwrap_err();
        assert!(matches!(
            error,
            DesktopApplicationError::InvalidInput {
                field: "ordered_task_ids",
                ..
            }
        ));
    }

    #[test]
    fn inbox_order_and_move_scope_never_include_project_tasks() {
        let app = application();
        let project = app
            .create_project(DesktopProjectCreate::new("Project"))
            .unwrap();
        let project_task = app
            .create_task(DesktopTaskCreate::new(
                Some(project.id.clone()),
                "Project task",
            ))
            .unwrap();
        let first = app
            .create_task(DesktopTaskCreate::new(None, "Inbox first"))
            .unwrap();
        let second = app
            .create_task(DesktopTaskCreate::new(None, "Inbox second"))
            .unwrap();

        let reordered = app
            .reorder_tasks(None, &[second.id.clone(), first.id.clone()])
            .unwrap();
        assert_eq!(
            reordered
                .iter()
                .map(|task| task.id.clone())
                .collect::<Vec<_>>(),
            vec![second.id.clone(), first.id.clone()]
        );
        assert_eq!(app.get_task(&project_task.id).unwrap().sort_order, 0);

        let moved = app
            .move_task(
                &project_task.id,
                DesktopTaskMove {
                    target_project_id: None,
                    target_parent_id: None,
                },
            )
            .unwrap();
        assert_eq!(moved.project_id, None);
        assert_eq!(moved.sort_order, 2);
        let inbox = app.query_tasks(TaskQuery::for_inbox()).unwrap();
        assert_eq!(inbox.len(), 3);
        assert!(inbox.iter().all(|task| task.project_id.is_none()));
    }

    #[test]
    fn task_run_gate_uses_product_dependencies_for_every_host() {
        let app = application();
        let project = app
            .create_project(DesktopProjectCreate::new("Run gate"))
            .unwrap();
        let dependency = app
            .create_task(DesktopTaskCreate::new(
                Some(project.id.clone()),
                "Dependency",
            ))
            .unwrap();
        let task = app
            .create_task(DesktopTaskCreate::new(Some(project.id), "Target"))
            .unwrap();
        let updated = app
            .update_task_dependencies(&task.id, vec![dependency.id.clone()])
            .unwrap();
        assert_eq!(updated.depends_on, vec![dependency.id.clone()]);

        assert!(matches!(
            app.task_run_block(&task.id).unwrap(),
            Some(DesktopTaskRunBlock::DependencyIncomplete {
                task_id,
                status: ProductTaskStatus::Draft,
                ..
            }) if task_id == dependency.id
        ));
        assert!(matches!(
            app.task_session_snapshot(&task.id).unwrap().run_block,
            Some(DesktopTaskRunBlock::DependencyIncomplete { task_id, .. })
                if task_id == dependency.id
        ));
        assert!(app.ensure_task_runnable(&task.id).is_err());

        app.update_task(
            &dependency.id,
            DesktopTaskPatch {
                status: Some(ProductTaskStatus::Done),
                ..DesktopTaskPatch::default()
            },
        )
        .unwrap();
        assert_eq!(app.task_run_block(&task.id).unwrap(), None);
        assert_eq!(app.task_session_snapshot(&task.id).unwrap().run_block, None);

        app.update_task(
            &task.id,
            DesktopTaskPatch {
                status: Some(ProductTaskStatus::Blocked),
                ..DesktopTaskPatch::default()
            },
        )
        .unwrap();
        assert!(matches!(
            app.task_run_block(&task.id).unwrap(),
            Some(DesktopTaskRunBlock::Blocked { .. })
        ));
    }

    #[test]
    fn task_move_updates_conversation_and_rejects_invalid_or_cyclic_parents() {
        let app = application();
        let source = app
            .create_project(DesktopProjectCreate::new("Source"))
            .unwrap();
        let target = app
            .create_project(DesktopProjectCreate::new("Target"))
            .unwrap();
        let root = app
            .create_task(DesktopTaskCreate::new(Some(source.id.clone()), "Root"))
            .unwrap();
        let child = app
            .create_task(DesktopTaskCreate::new(Some(source.id.clone()), "Child"))
            .unwrap();

        let invalid = app
            .move_task(
                &child.id,
                DesktopTaskMove {
                    target_project_id: Some(target.id.clone()),
                    target_parent_id: Some(root.id.clone()),
                },
            )
            .unwrap_err();
        assert!(matches!(
            invalid,
            DesktopApplicationError::InvalidInput {
                field: "target_parent_id",
                ..
            }
        ));
        assert_eq!(
            app.get_task(&child.id).unwrap().project_id,
            Some(source.id.clone())
        );

        app.move_task(
            &child.id,
            DesktopTaskMove {
                target_project_id: Some(source.id.clone()),
                target_parent_id: Some(root.id.clone()),
            },
        )
        .unwrap();
        let cycle = app
            .move_task(
                &root.id,
                DesktopTaskMove {
                    target_project_id: Some(source.id.clone()),
                    target_parent_id: Some(child.id.clone()),
                },
            )
            .unwrap_err();
        assert!(matches!(
            cycle,
            DesktopApplicationError::InvalidInput {
                field: "target_parent_id",
                ..
            }
        ));

        let moved = app
            .move_task(
                &child.id,
                DesktopTaskMove {
                    target_project_id: Some(target.id.clone()),
                    target_parent_id: None,
                },
            )
            .unwrap();
        assert_eq!(moved.project_id, Some(target.id.clone()));
        assert_eq!(moved.parent_id, None);
        let conversations = app.project_tasks().task_conversations(&child.id).unwrap();
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].project_id, Some(target.id));
    }

    #[test]
    fn child_task_creation_persists_the_parent_and_rejects_cross_project_parents() {
        let app = application();
        let source = app
            .create_project(DesktopProjectCreate::new("Source"))
            .unwrap();
        let target = app
            .create_project(DesktopProjectCreate::new("Target"))
            .unwrap();
        let parent = app
            .create_task(DesktopTaskCreate::new(Some(source.id.clone()), "Parent"))
            .unwrap();

        let child = app
            .create_task(
                DesktopTaskCreate::new(Some(source.id.clone()), "Child")
                    .with_parent(parent.id.clone()),
            )
            .unwrap();
        assert_eq!(child.parent_id, Some(parent.id.clone()));
        assert_eq!(child.project_id, Some(source.id));
        assert_eq!(
            app.project_tasks()
                .task_conversations(&child.id)
                .unwrap()
                .len(),
            1
        );

        let invalid = app
            .create_task(
                DesktopTaskCreate::new(Some(target.id), "Foreign child").with_parent(parent.id),
            )
            .unwrap_err();
        assert!(matches!(
            invalid,
            DesktopApplicationError::InvalidInput {
                field: "target_parent_id",
                ..
            }
        ));
    }
}

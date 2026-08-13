use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use lilia_contracts::{
    AgentSessionBinding, ConflictKind, ExpectedRevision, Page, PageRequest, ProductCommandMeta,
    ProductCommandResult, ProductConversationStatus, ProductEntity, ProductEntityKind,
    ProductError, ProductEvent, ProductEventSequence, ProductProjectArchiveInput,
    ProductProjectArchiveOutcome, ProductProjectRemovalOutcome, ProductProjectReorderEntry,
    ProductProjectReorderOutcome, ProductResult, ProductTask, ProductTaskArchiveInput,
    ProductTaskArchiveOutcome, ProductTaskHandoffImport, ProductTaskHandoffRecord,
    ProductTaskMoveInput, ProductTaskMoveOutcome, ProductTaskReorderEntry,
    ProductTaskReorderOutcome, Project, ProjectArchiveState, ProjectId, TaskDependencyGraph,
    TaskId,
};

use crate::domain::ensure_expected_revision;

/// Host-neutral Product Core persistence port.
///
/// Application services own validation and concurrency semantics; repositories
/// provide one durable fact surface for Desktop, CLI, Remote, and Service hosts.
pub trait ProductRepository: Send + Sync {
    fn create_entity(&self, entity: ProductEntity) -> ProductResult<ProductEntity>;
    fn update_entity(
        &self,
        entity: ProductEntity,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductEntity>;
    fn get_entity(&self, kind: ProductEntityKind, id: &str) -> ProductResult<ProductEntity>;
    fn list_entities(&self, kind: ProductEntityKind) -> ProductResult<Vec<ProductEntity>>;
    fn create_entity_command(
        &self,
        meta: &ProductCommandMeta,
        entity: ProductEntity,
        action: &str,
    ) -> ProductResult<ProductCommandResult<ProductEntity>>;
    fn update_entity_command(
        &self,
        meta: &ProductCommandMeta,
        entity: ProductEntity,
        action: &str,
    ) -> ProductResult<ProductCommandResult<ProductEntity>>;
    fn remove_project_command(
        &self,
        meta: &ProductCommandMeta,
        project_id: &ProjectId,
        removed_at: i64,
    ) -> ProductResult<ProductCommandResult<ProductProjectRemovalOutcome>>;
    fn archive_project_command(
        &self,
        meta: &ProductCommandMeta,
        input: &ProductProjectArchiveInput,
    ) -> ProductResult<ProductCommandResult<ProductProjectArchiveOutcome>>;
    fn set_task_archived_command(
        &self,
        meta: &ProductCommandMeta,
        input: &ProductTaskArchiveInput,
    ) -> ProductResult<ProductCommandResult<ProductTaskArchiveOutcome>>;
    fn reorder_projects_command(
        &self,
        meta: &ProductCommandMeta,
        entries: &[ProductProjectReorderEntry],
    ) -> ProductResult<ProductCommandResult<ProductProjectReorderOutcome>>;
    fn reorder_tasks_command(
        &self,
        meta: &ProductCommandMeta,
        entries: &[ProductTaskReorderEntry],
    ) -> ProductResult<ProductCommandResult<ProductTaskReorderOutcome>>;
    fn move_task_command(
        &self,
        meta: &ProductCommandMeta,
        input: &ProductTaskMoveInput,
    ) -> ProductResult<ProductCommandResult<ProductTaskMoveOutcome>>;
    fn product_events(&self, request: &PageRequest) -> ProductResult<Page<ProductEvent>>;
    fn create_project(&self, project: Project) -> ProductResult<Project>;
    fn create_task(&self, task: ProductTask) -> ProductResult<ProductTask>;
    fn update_task_dependencies(
        &self,
        task_id: &TaskId,
        depends_on: Vec<TaskId>,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductTask>;
    fn get_project(&self, project_id: &ProjectId) -> ProductResult<Project>;
    fn list_projects(&self) -> ProductResult<Vec<Project>>;
    fn get_task(&self, task_id: &TaskId) -> ProductResult<ProductTask>;
    fn list_tasks(&self) -> ProductResult<Vec<ProductTask>>;
    fn accept_task_handoff(
        &self,
        import: ProductTaskHandoffImport,
    ) -> ProductResult<ProductTaskHandoffRecord>;
    fn task_handoff_for_task(
        &self,
        task_id: &TaskId,
    ) -> ProductResult<Option<ProductTaskHandoffRecord>>;
    fn record_binding(&self, binding: AgentSessionBinding) -> ProductResult<AgentSessionBinding>;
    fn list_bindings_for_task(&self, task_id: &TaskId) -> ProductResult<Vec<AgentSessionBinding>>;
    fn replace_binding_for_task(
        &self,
        binding: AgentSessionBinding,
    ) -> ProductResult<AgentSessionBinding>;
    /// Drop product Agent session bindings for a task (e.g. user session reset).
    fn clear_bindings_for_task(&self, task_id: &TaskId) -> ProductResult<usize>;
}

#[derive(Default)]
pub struct InMemoryProductStore {
    pub projects: HashMap<String, Project>,
    pub tasks: HashMap<String, ProductTask>,
    pub bindings: HashMap<String, AgentSessionBinding>,
    pub task_handoffs: HashMap<String, ProductTaskHandoffRecord>,
    entities: HashMap<String, ProductEntity>,
    command_results: HashMap<String, ProductCommandResult<ProductEntity>>,
    project_removal_results: HashMap<String, ProductCommandResult<ProductProjectRemovalOutcome>>,
    project_archive_results: HashMap<String, ProductCommandResult<ProductProjectArchiveOutcome>>,
    task_archive_results: HashMap<String, ProductCommandResult<ProductTaskArchiveOutcome>>,
    project_reorder_results: HashMap<String, ProductCommandResult<ProductProjectReorderOutcome>>,
    task_reorder_results: HashMap<String, ProductCommandResult<ProductTaskReorderOutcome>>,
    task_move_results: HashMap<String, ProductCommandResult<ProductTaskMoveOutcome>>,
    product_events: Vec<ProductEvent>,
    dependency_graph: TaskDependencyGraph,
}

impl InMemoryProductStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace in-memory rows (used by Service crash-restart hydrate from SQLite).
    pub fn replace_snapshot(
        &mut self,
        projects: Vec<Project>,
        tasks: Vec<ProductTask>,
        bindings: Vec<AgentSessionBinding>,
    ) {
        self.projects.clear();
        self.tasks.clear();
        self.bindings.clear();
        self.task_handoffs.clear();
        self.entities.clear();
        self.command_results.clear();
        self.project_removal_results.clear();
        self.project_archive_results.clear();
        self.task_archive_results.clear();
        self.project_reorder_results.clear();
        self.task_reorder_results.clear();
        self.task_move_results.clear();
        self.product_events.clear();
        for project in projects {
            self.projects
                .insert(project.id.as_str().to_string(), project);
        }
        for task in tasks {
            self.tasks.insert(task.id.as_str().to_string(), task);
        }
        for binding in bindings {
            self.bindings
                .insert(binding.binding_id.as_str().to_string(), binding);
        }
        self.rebuild_graph();
    }

    fn rebuild_graph(&mut self) {
        let mut graph = TaskDependencyGraph::new();
        for task in self.tasks.values() {
            graph.register_task(&task.id, task.project_id.as_ref(), &task.depends_on);
        }
        self.dependency_graph = graph;
    }
}

impl ProductRepository for Mutex<InMemoryProductStore> {
    fn create_entity(&self, entity: ProductEntity) -> ProductResult<ProductEntity> {
        let mut store = lock_store(self)?;
        if lookup_entity(&store, entity.kind(), entity.id()).is_some() {
            return Err(duplicate_entity_error(&entity));
        }
        store_entity(&mut store, entity.clone());
        store.rebuild_graph();
        Ok(entity)
    }

    fn update_entity(
        &self,
        mut entity: ProductEntity,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductEntity> {
        let mut store = lock_store(self)?;
        let current = lookup_entity(&store, entity.kind(), entity.id()).ok_or_else(|| {
            ProductError::NotFound {
                entity: entity.kind().as_str().into(),
                id: entity.id().into(),
            }
        })?;
        ensure_expected_revision(expected, current.revision())?;
        entity.set_revision(current.revision().next());
        store_entity(&mut store, entity.clone());
        store.rebuild_graph();
        Ok(entity)
    }

    fn get_entity(&self, kind: ProductEntityKind, id: &str) -> ProductResult<ProductEntity> {
        let store = lock_store(self)?;
        lookup_entity(&store, kind, id).ok_or_else(|| ProductError::NotFound {
            entity: kind.as_str().into(),
            id: id.into(),
        })
    }

    fn list_entities(&self, kind: ProductEntityKind) -> ProductResult<Vec<ProductEntity>> {
        let store = lock_store(self)?;
        let mut entities: Vec<ProductEntity> = match kind {
            ProductEntityKind::Project => store
                .projects
                .values()
                .cloned()
                .map(ProductEntity::Project)
                .collect(),
            ProductEntityKind::Task => store
                .tasks
                .values()
                .cloned()
                .map(ProductEntity::Task)
                .collect(),
            ProductEntityKind::Binding => store
                .bindings
                .values()
                .cloned()
                .map(ProductEntity::Binding)
                .collect(),
            _ => store
                .entities
                .values()
                .filter(|entity| entity.kind() == kind)
                .cloned()
                .collect(),
        };
        entities.sort_by(|left, right| left.id().cmp(right.id()));
        Ok(entities)
    }

    fn create_entity_command(
        &self,
        meta: &ProductCommandMeta,
        entity: ProductEntity,
        action: &str,
    ) -> ProductResult<ProductCommandResult<ProductEntity>> {
        let mut store = lock_store(self)?;
        if let Some(result) = store
            .command_results
            .get(meta.idempotency_key.as_str())
            .cloned()
        {
            return duplicate_command_result(meta, result);
        }
        if lookup_entity(&store, entity.kind(), entity.id()).is_some() {
            return Err(duplicate_entity_error(&entity));
        }
        store_entity(&mut store, entity.clone());
        store.rebuild_graph();
        append_command_result(&mut store, meta, entity, action)
    }

    fn update_entity_command(
        &self,
        meta: &ProductCommandMeta,
        mut entity: ProductEntity,
        action: &str,
    ) -> ProductResult<ProductCommandResult<ProductEntity>> {
        let mut store = lock_store(self)?;
        if let Some(result) = store
            .command_results
            .get(meta.idempotency_key.as_str())
            .cloned()
        {
            return duplicate_command_result(meta, result);
        }
        let expected = meta
            .expected_revision
            .ok_or_else(|| ProductError::InvalidInput {
                field: "expected_revision".into(),
                message: "update command requires expected_revision".into(),
            })?;
        let current = lookup_entity(&store, entity.kind(), entity.id()).ok_or_else(|| {
            ProductError::NotFound {
                entity: entity.kind().as_str().into(),
                id: entity.id().into(),
            }
        })?;
        ensure_expected_revision(expected, current.revision())?;
        entity.set_revision(current.revision().next());
        store_entity(&mut store, entity.clone());
        store.rebuild_graph();
        append_command_result(&mut store, meta, entity, action)
    }

    fn remove_project_command(
        &self,
        meta: &ProductCommandMeta,
        project_id: &ProjectId,
        removed_at: i64,
    ) -> ProductResult<ProductCommandResult<ProductProjectRemovalOutcome>> {
        let mut store = lock_store(self)?;
        if let Some(result) = store
            .project_removal_results
            .get(meta.idempotency_key.as_str())
            .cloned()
        {
            return duplicate_command_result(meta, result);
        }
        let expected = meta
            .expected_revision
            .ok_or_else(|| ProductError::InvalidInput {
                field: "expected_revision".into(),
                message: "remove project command requires expected_revision".into(),
            })?;
        let mut project = store
            .projects
            .get(project_id.as_str())
            .cloned()
            .ok_or_else(|| ProductError::NotFound {
                entity: "project".into(),
                id: project_id.as_str().into(),
            })?;
        ensure_expected_revision(expected, project.revision)?;
        if project.archive == ProjectArchiveState::Archived {
            return Err(ProductError::InvalidState {
                message: format!("project `{project_id}` is already archived"),
            });
        }

        let mut conversation_keys = store
            .entities
            .iter()
            .filter_map(|(key, entity)| match entity {
                ProductEntity::Conversation(conversation)
                    if conversation.project_id.as_ref() == Some(project_id)
                        && !conversation.archived =>
                {
                    Some(key.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        conversation_keys.sort();
        let mut moved_conversation_ids = Vec::with_capacity(conversation_keys.len());
        let mut detached = Vec::with_capacity(conversation_keys.len());
        for key in conversation_keys {
            let Some(ProductEntity::Conversation(conversation)) = store.entities.get_mut(&key)
            else {
                continue;
            };
            conversation.project_id = None;
            conversation.updated_at = conversation.updated_at.max(removed_at);
            conversation.revision = conversation.revision.next();
            moved_conversation_ids.push(conversation.id.clone());
            detached.push(ProductEntity::Conversation(conversation.clone()));
        }

        let mut task_ids = store
            .tasks
            .values()
            .filter(|task| task.project_id.as_ref() == Some(project_id) && !task.archived)
            .map(|task| task.id.clone())
            .collect::<Vec<_>>();
        task_ids.sort();
        let mut moved_task_ids = Vec::with_capacity(task_ids.len());
        for task_id in task_ids {
            let Some(task) = store.tasks.get_mut(task_id.as_str()) else {
                continue;
            };
            task.project_id = None;
            task.updated_at = task.updated_at.max(removed_at);
            task.revision = task.revision.next();
            moved_task_ids.push(task.id.clone());
            detached.push(ProductEntity::Task(task.clone()));
        }
        for entity in &detached {
            append_product_event(&mut store, meta, entity, "detached_from_project");
        }

        project.archive = ProjectArchiveState::Archived;
        project.revision = project.revision.next();
        store
            .projects
            .insert(project.id.as_str().to_owned(), project.clone());
        store.rebuild_graph();
        let outcome = ProductProjectRemovalOutcome {
            project: project.clone(),
            moved_task_ids,
            moved_conversation_ids,
            already_removed: false,
        };
        append_project_removal_result(&mut store, meta, outcome, "project_removed")
    }

    fn archive_project_command(
        &self,
        meta: &ProductCommandMeta,
        input: &ProductProjectArchiveInput,
    ) -> ProductResult<ProductCommandResult<ProductProjectArchiveOutcome>> {
        let mut store = lock_store(self)?;
        if let Some(result) = store
            .project_archive_results
            .get(meta.idempotency_key.as_str())
            .cloned()
        {
            return duplicate_command_result(meta, result);
        }
        validate_project_archive_input(input)?;
        let project = store
            .projects
            .get(input.project_id.as_str())
            .ok_or_else(|| ProductError::NotFound {
                entity: "project".into(),
                id: input.project_id.as_str().into(),
            })?;
        ensure_expected_revision(input.expected_project_revision, project.revision)?;
        if project.archive == ProjectArchiveState::Archived {
            return Err(ProductError::InvalidState {
                message: format!("project `{}` is archived", input.project_id),
            });
        }

        let mut active_task_ids = store
            .tasks
            .values()
            .filter(|task| task.project_id.as_ref() == Some(&input.project_id) && !task.archived)
            .map(|task| task.id.clone())
            .collect::<Vec<_>>();
        active_task_ids.sort();
        let mut requested_task_ids = input
            .tasks
            .iter()
            .map(|entry| entry.task_id.clone())
            .collect::<Vec<_>>();
        requested_task_ids.sort();
        if active_task_ids != requested_task_ids {
            return Err(stale_project_archive("active task set changed"));
        }
        for entry in &input.tasks {
            let task =
                store
                    .tasks
                    .get(entry.task_id.as_str())
                    .ok_or_else(|| ProductError::NotFound {
                        entity: "task".into(),
                        id: entry.task_id.as_str().into(),
                    })?;
            ensure_expected_revision(entry.expected_revision, task.revision)?;
        }

        let active_task_ids = active_task_ids
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        let mut active_conversation_ids = store
            .entities
            .values()
            .filter_map(|entity| match entity {
                ProductEntity::Conversation(conversation)
                    if !conversation.archived
                        && conversation
                            .task_id
                            .as_ref()
                            .is_some_and(|task_id| active_task_ids.contains(task_id)) =>
                {
                    Some(conversation.id.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        active_conversation_ids.sort();
        let mut requested_conversation_ids = input
            .conversations
            .iter()
            .map(|entry| entry.conversation_id.clone())
            .collect::<Vec<_>>();
        requested_conversation_ids.sort();
        if active_conversation_ids != requested_conversation_ids {
            return Err(stale_project_archive("active conversation set changed"));
        }
        for entry in &input.conversations {
            let conversation = lookup_entity(
                &store,
                ProductEntityKind::Conversation,
                entry.conversation_id.as_str(),
            )
            .and_then(|entity| match entity {
                ProductEntity::Conversation(conversation) => Some(conversation),
                _ => None,
            })
            .ok_or_else(|| ProductError::NotFound {
                entity: "conversation".into(),
                id: entry.conversation_id.as_str().into(),
            })?;
            ensure_expected_revision(entry.expected_revision, conversation.revision)?;
        }

        let mut archived_tasks = Vec::with_capacity(input.tasks.len());
        for entry in &input.tasks {
            let mut task = store
                .tasks
                .get(entry.task_id.as_str())
                .cloned()
                .expect("archive task set was validated");
            task.archived = true;
            task.updated_at = task.updated_at.max(input.archived_at);
            task.revision = task.revision.next();
            store_entity(&mut store, ProductEntity::Task(task.clone()));
            append_product_event(
                &mut store,
                meta,
                &ProductEntity::Task(task.clone()),
                "archived",
            );
            archived_tasks.push(task);
        }
        let mut archived_conversations = Vec::with_capacity(input.conversations.len());
        for entry in &input.conversations {
            let mut conversation = lookup_entity(
                &store,
                ProductEntityKind::Conversation,
                entry.conversation_id.as_str(),
            )
            .and_then(|entity| match entity {
                ProductEntity::Conversation(conversation) => Some(conversation),
                _ => None,
            })
            .expect("archive conversation set was validated");
            conversation.archived = true;
            conversation.status = ProductConversationStatus::Closed;
            conversation.updated_at = conversation.updated_at.max(input.archived_at);
            conversation.revision = conversation.revision.next();
            store_entity(
                &mut store,
                ProductEntity::Conversation(conversation.clone()),
            );
            append_product_event(
                &mut store,
                meta,
                &ProductEntity::Conversation(conversation.clone()),
                "archived",
            );
            archived_conversations.push(conversation);
        }
        store.rebuild_graph();
        append_project_archive_result(
            &mut store,
            meta,
            ProductProjectArchiveOutcome {
                archived_tasks,
                archived_conversations,
            },
        )
    }

    fn set_task_archived_command(
        &self,
        meta: &ProductCommandMeta,
        input: &ProductTaskArchiveInput,
    ) -> ProductResult<ProductCommandResult<ProductTaskArchiveOutcome>> {
        let mut store = lock_store(self)?;
        if let Some(result) = store
            .task_archive_results
            .get(meta.idempotency_key.as_str())
            .cloned()
        {
            return duplicate_command_result(meta, result);
        }
        validate_task_archive_input(input)?;
        let mut task = store
            .tasks
            .get(input.task_id.as_str())
            .cloned()
            .ok_or_else(|| ProductError::NotFound {
                entity: "task".into(),
                id: input.task_id.as_str().into(),
            })?;
        ensure_expected_revision(input.expected_revision, task.revision)?;
        let mut conversations = store
            .entities
            .values()
            .filter_map(|entity| match entity {
                ProductEntity::Conversation(conversation)
                    if conversation.task_id.as_ref() == Some(&input.task_id) =>
                {
                    Some(conversation.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        conversations.sort_by(|left, right| left.id.cmp(&right.id));
        let mut requested_ids = input
            .conversations
            .iter()
            .map(|entry| entry.conversation_id.clone())
            .collect::<Vec<_>>();
        requested_ids.sort();
        if conversations
            .iter()
            .map(|conversation| &conversation.id)
            .ne(requested_ids.iter())
        {
            return Err(stale_task_archive("bound conversation set changed"));
        }
        for entry in &input.conversations {
            let conversation = conversations
                .iter()
                .find(|conversation| conversation.id == entry.conversation_id)
                .expect("task conversation set was validated");
            ensure_expected_revision(entry.expected_revision, conversation.revision)?;
        }

        let action = if input.archived {
            "archived"
        } else {
            "restored"
        };
        let mut changed = false;
        if task.archived != input.archived {
            task.archived = input.archived;
            task.updated_at = task.updated_at.max(input.updated_at);
            task.revision = task.revision.next();
            store_entity(&mut store, ProductEntity::Task(task.clone()));
            append_product_event(&mut store, meta, &ProductEntity::Task(task.clone()), action);
            changed = true;
        }
        let desired_status = if input.archived {
            ProductConversationStatus::Closed
        } else {
            ProductConversationStatus::Active
        };
        for conversation in &mut conversations {
            if conversation.archived == input.archived && conversation.status == desired_status {
                continue;
            }
            conversation.archived = input.archived;
            conversation.status = desired_status;
            conversation.updated_at = conversation.updated_at.max(input.updated_at);
            conversation.revision = conversation.revision.next();
            store_entity(
                &mut store,
                ProductEntity::Conversation(conversation.clone()),
            );
            append_product_event(
                &mut store,
                meta,
                &ProductEntity::Conversation(conversation.clone()),
                action,
            );
            changed = true;
        }
        if !changed {
            return Err(ProductError::InvalidState {
                message: "task archive state already matches the requested value".into(),
            });
        }
        store.rebuild_graph();
        append_task_archive_result(
            &mut store,
            meta,
            ProductTaskArchiveOutcome {
                task,
                conversations,
            },
        )
    }

    fn reorder_projects_command(
        &self,
        meta: &ProductCommandMeta,
        entries: &[ProductProjectReorderEntry],
    ) -> ProductResult<ProductCommandResult<ProductProjectReorderOutcome>> {
        let mut store = lock_store(self)?;
        if let Some(result) = store
            .project_reorder_results
            .get(meta.idempotency_key.as_str())
            .cloned()
        {
            return duplicate_command_result(meta, result);
        }
        validate_project_reorder_entries(entries)?;
        let mut projects = entries
            .iter()
            .map(|entry| {
                store
                    .projects
                    .get(entry.project_id.as_str())
                    .cloned()
                    .ok_or_else(|| ProductError::NotFound {
                        entity: "project".into(),
                        id: entry.project_id.as_str().into(),
                    })
            })
            .collect::<ProductResult<Vec<_>>>()?;
        let pinned = projects[0].pinned;
        if projects.iter().any(|project| {
            project.archive == ProjectArchiveState::Archived || project.pinned != pinned
        }) {
            return Err(ProductError::InvalidInput {
                field: "ordered_project_ids".into(),
                message: "project order must contain active projects from one pinned group".into(),
            });
        }
        let complete_group = store
            .projects
            .values()
            .filter(|project| {
                project.archive == ProjectArchiveState::Active && project.pinned == pinned
            })
            .collect::<Vec<_>>();
        if complete_group.len() != entries.len()
            || complete_group
                .iter()
                .any(|project| !entries.iter().any(|entry| entry.project_id == project.id))
        {
            return Err(ProductError::InvalidInput {
                field: "ordered_project_ids".into(),
                message: "project order must contain one complete pinned group".into(),
            });
        }
        for (project, entry) in projects.iter().zip(entries) {
            ensure_expected_revision(entry.expected_revision, project.revision)?;
        }
        for (sort_order, project) in projects.iter_mut().enumerate() {
            project.sort_order = sort_order as i64;
            project.revision = project.revision.next();
            store
                .projects
                .insert(project.id.as_str().to_owned(), project.clone());
            append_product_event(
                &mut store,
                meta,
                &ProductEntity::Project(project.clone()),
                "projects_reordered",
            );
        }
        append_project_reorder_result(&mut store, meta, ProductProjectReorderOutcome { projects })
    }

    fn reorder_tasks_command(
        &self,
        meta: &ProductCommandMeta,
        entries: &[ProductTaskReorderEntry],
    ) -> ProductResult<ProductCommandResult<ProductTaskReorderOutcome>> {
        let mut store = lock_store(self)?;
        if let Some(result) = store
            .task_reorder_results
            .get(meta.idempotency_key.as_str())
            .cloned()
        {
            return duplicate_command_result(meta, result);
        }
        validate_task_reorder_entries(entries)?;
        let mut tasks = entries
            .iter()
            .map(|entry| {
                store
                    .tasks
                    .get(entry.task_id.as_str())
                    .cloned()
                    .ok_or_else(|| ProductError::NotFound {
                        entity: "task".into(),
                        id: entry.task_id.as_str().into(),
                    })
            })
            .collect::<ProductResult<Vec<_>>>()?;
        let project_id = tasks[0].project_id.clone();
        let pinned = tasks[0].pinned;
        if tasks
            .iter()
            .any(|task| task.archived || task.project_id != project_id || task.pinned != pinned)
        {
            return Err(ProductError::InvalidInput {
                field: "ordered_task_ids".into(),
                message: "task order must contain active tasks from one pinned group".into(),
            });
        }
        let complete_group = store
            .tasks
            .values()
            .filter(|task| !task.archived && task.project_id == project_id && task.pinned == pinned)
            .collect::<Vec<_>>();
        if complete_group.len() != entries.len()
            || complete_group
                .iter()
                .any(|task| !entries.iter().any(|entry| entry.task_id == task.id))
        {
            return Err(ProductError::InvalidInput {
                field: "ordered_task_ids".into(),
                message: "task order must contain one complete pinned group".into(),
            });
        }
        for (task, entry) in tasks.iter().zip(entries) {
            ensure_expected_revision(entry.expected_revision, task.revision)?;
        }
        for (sort_order, task) in tasks.iter_mut().enumerate() {
            task.sort_order = sort_order as i64;
            task.revision = task.revision.next();
            store
                .tasks
                .insert(task.id.as_str().to_owned(), task.clone());
            append_product_event(
                &mut store,
                meta,
                &ProductEntity::Task(task.clone()),
                "tasks_reordered",
            );
        }
        append_task_reorder_result(&mut store, meta, ProductTaskReorderOutcome { tasks })
    }

    fn move_task_command(
        &self,
        meta: &ProductCommandMeta,
        input: &ProductTaskMoveInput,
    ) -> ProductResult<ProductCommandResult<ProductTaskMoveOutcome>> {
        let mut store = lock_store(self)?;
        if let Some(result) = store
            .task_move_results
            .get(meta.idempotency_key.as_str())
            .cloned()
        {
            return duplicate_command_result(meta, result);
        }
        let mut task = store
            .tasks
            .get(input.task_id.as_str())
            .cloned()
            .ok_or_else(|| ProductError::NotFound {
                entity: "task".into(),
                id: input.task_id.as_str().into(),
            })?;
        ensure_expected_revision(input.expected_revision, task.revision)?;
        if task.archived {
            return Err(ProductError::InvalidInput {
                field: "task_id".into(),
                message: "archived tasks cannot be moved".into(),
            });
        }
        if let Some(project_id) = &input.target_project_id {
            let project =
                store
                    .projects
                    .get(project_id.as_str())
                    .ok_or_else(|| ProductError::NotFound {
                        entity: "project".into(),
                        id: project_id.as_str().into(),
                    })?;
            if project.archive == ProjectArchiveState::Archived {
                return Err(ProductError::InvalidInput {
                    field: "target_project_id".into(),
                    message: "target project must be active".into(),
                });
            }
        }
        let tasks = store.tasks.values().cloned().collect::<Vec<_>>();
        validate_task_move_parent(
            &tasks,
            &input.task_id,
            input.target_project_id.as_ref(),
            input.target_parent_id.as_ref(),
        )?;
        if task.project_id == input.target_project_id && task.parent_id == input.target_parent_id {
            return Err(ProductError::InvalidInput {
                field: "target_parent_id".into(),
                message: "task is already at the target location".into(),
            });
        }
        let subtree_ids = task_subtree_ids(&tasks, &input.task_id);
        let location_changed = task.project_id != input.target_project_id;
        let moved_task_ids = if location_changed {
            subtree_ids.clone()
        } else {
            vec![input.task_id.clone()]
        };

        let mut moved_conversation_ids = Vec::new();
        let mut conversations = store
            .entities
            .values()
            .filter_map(|entity| match entity {
                ProductEntity::Conversation(conversation)
                    if conversation
                        .task_id
                        .as_ref()
                        .is_some_and(|task_id| moved_task_ids.contains(task_id))
                        && conversation.project_id != input.target_project_id =>
                {
                    Some(conversation.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        conversations.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        for mut conversation in conversations {
            conversation.project_id = input.target_project_id.clone();
            conversation.updated_at = conversation.updated_at.max(input.moved_at);
            conversation.revision = conversation.revision.next();
            moved_conversation_ids.push(conversation.id.clone());
            let entity = ProductEntity::Conversation(conversation);
            store_entity(&mut store, entity.clone());
            append_product_event(&mut store, meta, &entity, "conversation_moved_with_task");
        }

        if location_changed {
            task.sort_order = tasks
                .iter()
                .filter(|candidate| {
                    !candidate.archived
                        && !subtree_ids.contains(&candidate.id)
                        && candidate.project_id == input.target_project_id
                })
                .map(|candidate| candidate.sort_order)
                .max()
                .unwrap_or(-1)
                .saturating_add(1);
            for descendant_id in subtree_ids.iter().skip(1) {
                let mut descendant = tasks
                    .iter()
                    .find(|candidate| &candidate.id == descendant_id)
                    .cloned()
                    .expect("task subtree ids come from the task snapshot");
                descendant.project_id = input.target_project_id.clone();
                descendant.updated_at = descendant.updated_at.max(input.moved_at);
                descendant.revision = descendant.revision.next();
                store
                    .tasks
                    .insert(descendant.id.as_str().to_owned(), descendant.clone());
                append_product_event(
                    &mut store,
                    meta,
                    &ProductEntity::Task(descendant),
                    "task_moved_with_parent",
                );
            }
        }
        task.project_id = input.target_project_id.clone();
        task.parent_id = input.target_parent_id.clone();
        task.updated_at = task.updated_at.max(input.moved_at);
        task.revision = task.revision.next();
        store
            .tasks
            .insert(task.id.as_str().to_owned(), task.clone());
        store.rebuild_graph();
        append_product_event(
            &mut store,
            meta,
            &ProductEntity::Task(task.clone()),
            "task_moved",
        );
        append_task_move_result(
            &mut store,
            meta,
            ProductTaskMoveOutcome {
                task,
                moved_task_ids,
                moved_conversation_ids,
            },
        )
    }

    fn product_events(&self, request: &PageRequest) -> ProductResult<Page<ProductEvent>> {
        let store = lock_store(self)?;
        Ok(page_events(&store.product_events, request))
    }

    fn create_project(&self, project: Project) -> ProductResult<Project> {
        let mut store = lock_store(self)?;
        if store.projects.contains_key(project.id.as_str()) {
            return Err(ProductError::Conflict {
                conflict: ConflictKind::DuplicateIdempotency,
                message: format!("project `{}` already exists", project.id),
            });
        }
        store
            .projects
            .insert(project.id.as_str().to_string(), project.clone());
        Ok(project)
    }

    fn create_task(&self, task: ProductTask) -> ProductResult<ProductTask> {
        let mut store = lock_store(self)?;
        if store.tasks.contains_key(task.id.as_str()) {
            return Err(ProductError::Conflict {
                conflict: ConflictKind::DuplicateIdempotency,
                message: format!("task `{}` already exists", task.id),
            });
        }
        store
            .tasks
            .insert(task.id.as_str().to_string(), task.clone());
        store.rebuild_graph();
        Ok(task)
    }

    fn accept_task_handoff(
        &self,
        import: ProductTaskHandoffImport,
    ) -> ProductResult<ProductTaskHandoffRecord> {
        let mut store = lock_store(self)?;
        if let Some(existing) = store.task_handoffs.get(&import.handoff.id) {
            let mut existing = existing.clone();
            existing.duplicate = true;
            return Ok(existing);
        }
        if import.task.project_id.as_ref() != Some(&import.project.id) {
            return Err(ProductError::InvalidInput {
                field: "task.project_id".into(),
                message: "handoff task must belong to the imported project".into(),
            });
        }
        if store.tasks.contains_key(import.task.id.as_str()) {
            return Err(ProductError::Conflict {
                conflict: ConflictKind::DuplicateIdempotency,
                message: format!("task `{}` already exists", import.task.id),
            });
        }
        let project = store
            .projects
            .entry(import.project.id.as_str().to_owned())
            .or_insert_with(|| import.project.clone())
            .clone();
        store
            .tasks
            .insert(import.task.id.as_str().to_owned(), import.task.clone());
        let record = ProductTaskHandoffRecord {
            handoff: import.handoff,
            payload_json: import.payload_json,
            project,
            task: import.task,
            accepted_at: import.accepted_at,
            duplicate: false,
        };
        store
            .task_handoffs
            .insert(record.handoff.id.clone(), record.clone());
        store.rebuild_graph();
        Ok(record)
    }

    fn task_handoff_for_task(
        &self,
        task_id: &TaskId,
    ) -> ProductResult<Option<ProductTaskHandoffRecord>> {
        let store = lock_store(self)?;
        Ok(store
            .task_handoffs
            .values()
            .find(|record| &record.task.id == task_id)
            .cloned())
    }

    fn update_task_dependencies(
        &self,
        task_id: &TaskId,
        depends_on: Vec<TaskId>,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductTask> {
        let mut store = lock_store(self)?;
        let task =
            store
                .tasks
                .get(task_id.as_str())
                .cloned()
                .ok_or_else(|| ProductError::NotFound {
                    entity: "task".into(),
                    id: task_id.as_str().to_string(),
                })?;
        ensure_expected_revision(expected, task.revision)?;
        store.rebuild_graph();
        let validated = store.dependency_graph.validate_dependencies(
            &task.id,
            task.project_id.as_ref(),
            &depends_on,
        )?;
        let mut updated = task;
        updated.depends_on = validated;
        updated.revision = updated.revision.next();
        store
            .tasks
            .insert(updated.id.as_str().to_string(), updated.clone());
        store.rebuild_graph();
        Ok(updated)
    }

    fn get_project(&self, project_id: &ProjectId) -> ProductResult<Project> {
        let store = lock_store(self)?;
        store
            .projects
            .get(project_id.as_str())
            .cloned()
            .ok_or_else(|| ProductError::NotFound {
                entity: "project".into(),
                id: project_id.as_str().to_string(),
            })
    }

    fn list_projects(&self) -> ProductResult<Vec<Project>> {
        let store = lock_store(self)?;
        let mut projects = store.projects.values().cloned().collect::<Vec<_>>();
        projects.sort_by(|left, right| {
            right
                .pinned
                .cmp(&left.pinned)
                .then(left.sort_order.cmp(&right.sort_order))
                .then(left.id.cmp(&right.id))
        });
        Ok(projects)
    }

    fn get_task(&self, task_id: &TaskId) -> ProductResult<ProductTask> {
        let store = lock_store(self)?;
        store
            .tasks
            .get(task_id.as_str())
            .cloned()
            .ok_or_else(|| ProductError::NotFound {
                entity: "task".into(),
                id: task_id.as_str().to_string(),
            })
    }

    fn list_tasks(&self) -> ProductResult<Vec<ProductTask>> {
        let store = lock_store(self)?;
        let mut tasks = store.tasks.values().cloned().collect::<Vec<_>>();
        tasks.sort_by(|left, right| {
            right
                .pinned
                .cmp(&left.pinned)
                .then(left.sort_order.cmp(&right.sort_order))
                .then(left.id.cmp(&right.id))
        });
        Ok(tasks)
    }

    fn record_binding(&self, binding: AgentSessionBinding) -> ProductResult<AgentSessionBinding> {
        let mut store = lock_store(self)?;
        if !store.tasks.contains_key(binding.task_id.as_str()) {
            return Err(ProductError::NotFound {
                entity: "task".into(),
                id: binding.task_id.as_str().to_string(),
            });
        }
        if store.bindings.contains_key(binding.binding_id.as_str()) {
            return Err(ProductError::Conflict {
                conflict: ConflictKind::DuplicateBinding,
                message: format!("binding `{}` already exists", binding.binding_id),
            });
        }
        store
            .bindings
            .insert(binding.binding_id.as_str().to_string(), binding.clone());
        Ok(binding)
    }

    fn list_bindings_for_task(&self, task_id: &TaskId) -> ProductResult<Vec<AgentSessionBinding>> {
        let store = lock_store(self)?;
        let mut bindings = store
            .bindings
            .values()
            .filter(|binding| binding.task_id == *task_id)
            .cloned()
            .collect::<Vec<_>>();
        bindings.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
        Ok(bindings)
    }

    fn replace_binding_for_task(
        &self,
        binding: AgentSessionBinding,
    ) -> ProductResult<AgentSessionBinding> {
        let mut store = lock_store(self)?;
        if !store.tasks.contains_key(binding.task_id.as_str()) {
            return Err(ProductError::NotFound {
                entity: "task".into(),
                id: binding.task_id.as_str().to_string(),
            });
        }
        store
            .bindings
            .retain(|_, existing| existing.task_id != binding.task_id);
        store
            .bindings
            .insert(binding.binding_id.as_str().to_owned(), binding.clone());
        Ok(binding)
    }

    fn clear_bindings_for_task(&self, task_id: &TaskId) -> ProductResult<usize> {
        let mut store = lock_store(self)?;
        let before = store.bindings.len();
        store
            .bindings
            .retain(|_, binding| binding.task_id != *task_id);
        Ok(before.saturating_sub(store.bindings.len()))
    }
}

pub struct ProductServices {
    repository: Arc<dyn ProductRepository>,
}

impl ProductServices {
    pub fn new(store: Arc<Mutex<InMemoryProductStore>>) -> Self {
        Self::with_repository(store)
    }

    pub fn with_repository(repository: Arc<dyn ProductRepository>) -> Self {
        Self { repository }
    }

    pub fn create_project(&self, id: ProjectId, name: impl Into<String>) -> ProductResult<Project> {
        let project = Project::new(id, name)?;
        self.repository.create_project(project)
    }

    pub fn create_task(
        &self,
        id: TaskId,
        project_id: Option<ProjectId>,
        title: impl Into<String>,
    ) -> ProductResult<ProductTask> {
        let task = ProductTask::new(id, project_id, title)?;
        self.repository.create_task(task)
    }

    pub fn update_task_dependencies(
        &self,
        task_id: &TaskId,
        depends_on: Vec<TaskId>,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductTask> {
        self.repository
            .update_task_dependencies(task_id, depends_on, expected)
    }

    pub fn get_task(&self, task_id: &TaskId) -> ProductResult<ProductTask> {
        self.repository.get_task(task_id)
    }

    pub fn get_project(&self, project_id: &ProjectId) -> ProductResult<Project> {
        self.repository.get_project(project_id)
    }

    pub fn list_projects(&self) -> ProductResult<Vec<Project>> {
        self.repository.list_projects()
    }

    pub fn list_tasks(&self) -> ProductResult<Vec<ProductTask>> {
        self.repository.list_tasks()
    }

    pub fn accept_task_handoff(
        &self,
        import: ProductTaskHandoffImport,
    ) -> ProductResult<ProductTaskHandoffRecord> {
        self.repository.accept_task_handoff(import)
    }

    pub fn task_handoff_for_task(
        &self,
        task_id: &TaskId,
    ) -> ProductResult<Option<ProductTaskHandoffRecord>> {
        self.repository.task_handoff_for_task(task_id)
    }

    pub fn list_bindings_for_task(
        &self,
        task_id: &TaskId,
    ) -> ProductResult<Vec<AgentSessionBinding>> {
        self.repository.list_bindings_for_task(task_id)
    }

    pub fn clear_bindings_for_task(&self, task_id: &TaskId) -> ProductResult<usize> {
        self.repository.clear_bindings_for_task(task_id)
    }

    pub fn replace_binding_for_task(
        &self,
        binding: AgentSessionBinding,
    ) -> ProductResult<AgentSessionBinding> {
        self.repository.replace_binding_for_task(binding)
    }

    pub fn record_binding(
        &self,
        binding: AgentSessionBinding,
    ) -> ProductResult<AgentSessionBinding> {
        self.repository.record_binding(binding)
    }

    pub fn create_entity(&self, entity: ProductEntity) -> ProductResult<ProductEntity> {
        self.repository.create_entity(entity)
    }

    pub fn update_entity(
        &self,
        entity: ProductEntity,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductEntity> {
        self.repository.update_entity(entity, expected)
    }

    pub fn get_entity(&self, kind: ProductEntityKind, id: &str) -> ProductResult<ProductEntity> {
        self.repository.get_entity(kind, id)
    }

    pub fn list_entities(&self, kind: ProductEntityKind) -> ProductResult<Vec<ProductEntity>> {
        self.repository.list_entities(kind)
    }

    pub fn create_entity_command(
        &self,
        meta: &ProductCommandMeta,
        entity: ProductEntity,
        action: &str,
    ) -> ProductResult<ProductCommandResult<ProductEntity>> {
        self.repository.create_entity_command(meta, entity, action)
    }

    pub fn update_entity_command(
        &self,
        meta: &ProductCommandMeta,
        entity: ProductEntity,
        action: &str,
    ) -> ProductResult<ProductCommandResult<ProductEntity>> {
        self.repository.update_entity_command(meta, entity, action)
    }

    pub fn remove_project_command(
        &self,
        meta: &ProductCommandMeta,
        project_id: &ProjectId,
        removed_at: i64,
    ) -> ProductResult<ProductCommandResult<ProductProjectRemovalOutcome>> {
        self.repository
            .remove_project_command(meta, project_id, removed_at)
    }

    pub fn archive_project_command(
        &self,
        meta: &ProductCommandMeta,
        input: &ProductProjectArchiveInput,
    ) -> ProductResult<ProductCommandResult<ProductProjectArchiveOutcome>> {
        self.repository.archive_project_command(meta, input)
    }

    pub fn set_task_archived_command(
        &self,
        meta: &ProductCommandMeta,
        input: &ProductTaskArchiveInput,
    ) -> ProductResult<ProductCommandResult<ProductTaskArchiveOutcome>> {
        self.repository.set_task_archived_command(meta, input)
    }

    pub fn reorder_projects_command(
        &self,
        meta: &ProductCommandMeta,
        entries: &[ProductProjectReorderEntry],
    ) -> ProductResult<ProductCommandResult<ProductProjectReorderOutcome>> {
        self.repository.reorder_projects_command(meta, entries)
    }

    pub fn reorder_tasks_command(
        &self,
        meta: &ProductCommandMeta,
        entries: &[ProductTaskReorderEntry],
    ) -> ProductResult<ProductCommandResult<ProductTaskReorderOutcome>> {
        self.repository.reorder_tasks_command(meta, entries)
    }

    pub fn move_task_command(
        &self,
        meta: &ProductCommandMeta,
        input: &ProductTaskMoveInput,
    ) -> ProductResult<ProductCommandResult<ProductTaskMoveOutcome>> {
        self.repository.move_task_command(meta, input)
    }

    pub fn product_events(&self, request: &PageRequest) -> ProductResult<Page<ProductEvent>> {
        self.repository.product_events(request)
    }
}

fn lock_store(
    store: &Mutex<InMemoryProductStore>,
) -> ProductResult<std::sync::MutexGuard<'_, InMemoryProductStore>> {
    store.lock().map_err(|_| ProductError::Unavailable {
        message: "product store lock poisoned".into(),
    })
}

fn entity_key(kind: ProductEntityKind, id: &str) -> String {
    format!("{}:{id}", kind.as_str())
}

fn lookup_entity(
    store: &InMemoryProductStore,
    kind: ProductEntityKind,
    id: &str,
) -> Option<ProductEntity> {
    match kind {
        ProductEntityKind::Project => store.projects.get(id).cloned().map(ProductEntity::Project),
        ProductEntityKind::Task => store.tasks.get(id).cloned().map(ProductEntity::Task),
        ProductEntityKind::Binding => store.bindings.get(id).cloned().map(ProductEntity::Binding),
        _ => store.entities.get(&entity_key(kind, id)).cloned(),
    }
}

fn store_entity(store: &mut InMemoryProductStore, entity: ProductEntity) {
    match entity {
        ProductEntity::Project(project) => {
            store.projects.insert(project.id.to_string(), project);
        }
        ProductEntity::Task(task) => {
            store.tasks.insert(task.id.to_string(), task);
        }
        ProductEntity::Binding(binding) => {
            store
                .bindings
                .insert(binding.binding_id.to_string(), binding);
        }
        entity => {
            store
                .entities
                .insert(entity_key(entity.kind(), entity.id()), entity);
        }
    }
}

fn duplicate_entity_error(entity: &ProductEntity) -> ProductError {
    ProductError::Conflict {
        conflict: if entity.kind() == ProductEntityKind::Binding {
            ConflictKind::DuplicateBinding
        } else {
            ConflictKind::DuplicateIdempotency
        },
        message: format!(
            "{} `{}` already exists",
            entity.kind().as_str(),
            entity.id()
        ),
    }
}

fn duplicate_command_result<T>(
    meta: &ProductCommandMeta,
    mut result: ProductCommandResult<T>,
) -> ProductResult<ProductCommandResult<T>> {
    if result.command_id != meta.command_id {
        return Err(ProductError::Conflict {
            conflict: ConflictKind::DuplicateIdempotency,
            message: "idempotency key was already used by another command".into(),
        });
    }
    result.duplicate = true;
    Ok(result)
}

fn append_command_result(
    store: &mut InMemoryProductStore,
    meta: &ProductCommandMeta,
    entity: ProductEntity,
    action: &str,
) -> ProductResult<ProductCommandResult<ProductEntity>> {
    let sequence = ProductEventSequence::new(store.product_events.len() as u64 + 1);
    let event = ProductEvent {
        sequence,
        command_id: meta.command_id.clone(),
        entity: entity.kind().as_str().into(),
        entity_id: entity.id().into(),
        action: action.into(),
        revision: Some(entity.revision().get()),
    };
    let result = ProductCommandResult {
        command_id: meta.command_id.clone(),
        event_sequence: sequence,
        value: entity,
        duplicate: false,
    };
    store.product_events.push(event);
    store
        .command_results
        .insert(meta.idempotency_key.as_str().into(), result.clone());
    Ok(result)
}

fn append_product_event(
    store: &mut InMemoryProductStore,
    meta: &ProductCommandMeta,
    entity: &ProductEntity,
    action: &str,
) {
    store.product_events.push(ProductEvent {
        sequence: ProductEventSequence::new(store.product_events.len() as u64 + 1),
        command_id: meta.command_id.clone(),
        entity: entity.kind().as_str().into(),
        entity_id: entity.id().into(),
        action: action.into(),
        revision: Some(entity.revision().get()),
    });
}

fn append_project_removal_result(
    store: &mut InMemoryProductStore,
    meta: &ProductCommandMeta,
    outcome: ProductProjectRemovalOutcome,
    action: &str,
) -> ProductResult<ProductCommandResult<ProductProjectRemovalOutcome>> {
    let sequence = ProductEventSequence::new(store.product_events.len() as u64 + 1);
    store.product_events.push(ProductEvent {
        sequence,
        command_id: meta.command_id.clone(),
        entity: ProductEntityKind::Project.as_str().into(),
        entity_id: outcome.project.id.as_str().into(),
        action: action.into(),
        revision: Some(outcome.project.revision.get()),
    });
    let result = ProductCommandResult {
        command_id: meta.command_id.clone(),
        event_sequence: sequence,
        value: outcome,
        duplicate: false,
    };
    store
        .project_removal_results
        .insert(meta.idempotency_key.as_str().into(), result.clone());
    Ok(result)
}

fn append_project_archive_result(
    store: &mut InMemoryProductStore,
    meta: &ProductCommandMeta,
    outcome: ProductProjectArchiveOutcome,
) -> ProductResult<ProductCommandResult<ProductProjectArchiveOutcome>> {
    let sequence = store
        .product_events
        .last()
        .map(|event| event.sequence)
        .ok_or_else(|| ProductError::InvalidState {
            message: "project archive command did not publish an event".into(),
        })?;
    let result = ProductCommandResult {
        command_id: meta.command_id.clone(),
        event_sequence: sequence,
        value: outcome,
        duplicate: false,
    };
    store
        .project_archive_results
        .insert(meta.idempotency_key.as_str().into(), result.clone());
    Ok(result)
}

fn validate_project_archive_input(input: &ProductProjectArchiveInput) -> ProductResult<()> {
    if input.tasks.is_empty() {
        return Err(ProductError::InvalidInput {
            field: "tasks".into(),
            message: "project archive must include at least one active task".into(),
        });
    }
    if input.tasks.iter().enumerate().any(|(index, entry)| {
        input.tasks[..index]
            .iter()
            .any(|candidate| candidate.task_id == entry.task_id)
    }) {
        return Err(ProductError::InvalidInput {
            field: "tasks".into(),
            message: "project archive tasks must not contain duplicate ids".into(),
        });
    }
    if input
        .conversations
        .iter()
        .enumerate()
        .any(|(index, entry)| {
            input.conversations[..index]
                .iter()
                .any(|candidate| candidate.conversation_id == entry.conversation_id)
        })
    {
        return Err(ProductError::InvalidInput {
            field: "conversations".into(),
            message: "project archive conversations must not contain duplicate ids".into(),
        });
    }
    Ok(())
}

fn append_task_archive_result(
    store: &mut InMemoryProductStore,
    meta: &ProductCommandMeta,
    outcome: ProductTaskArchiveOutcome,
) -> ProductResult<ProductCommandResult<ProductTaskArchiveOutcome>> {
    let sequence = store
        .product_events
        .last()
        .map(|event| event.sequence)
        .ok_or_else(|| ProductError::InvalidState {
            message: "task archive command did not publish an event".into(),
        })?;
    let result = ProductCommandResult {
        command_id: meta.command_id.clone(),
        event_sequence: sequence,
        value: outcome,
        duplicate: false,
    };
    store
        .task_archive_results
        .insert(meta.idempotency_key.as_str().into(), result.clone());
    Ok(result)
}

fn validate_task_archive_input(input: &ProductTaskArchiveInput) -> ProductResult<()> {
    if input
        .conversations
        .iter()
        .enumerate()
        .any(|(index, entry)| {
            input.conversations[..index]
                .iter()
                .any(|candidate| candidate.conversation_id == entry.conversation_id)
        })
    {
        return Err(ProductError::InvalidInput {
            field: "conversations".into(),
            message: "task archive conversations must not contain duplicate ids".into(),
        });
    }
    Ok(())
}

fn stale_task_archive(message: &str) -> ProductError {
    ProductError::Conflict {
        conflict: ConflictKind::StaleRevision,
        message: message.into(),
    }
}

fn stale_project_archive(message: &str) -> ProductError {
    ProductError::Conflict {
        conflict: ConflictKind::StaleRevision,
        message: message.into(),
    }
}

fn append_project_reorder_result(
    store: &mut InMemoryProductStore,
    meta: &ProductCommandMeta,
    outcome: ProductProjectReorderOutcome,
) -> ProductResult<ProductCommandResult<ProductProjectReorderOutcome>> {
    let sequence = store
        .product_events
        .last()
        .map(|event| event.sequence)
        .ok_or_else(|| ProductError::InvalidState {
            message: "project reorder command did not publish an event".into(),
        })?;
    let result = ProductCommandResult {
        command_id: meta.command_id.clone(),
        event_sequence: sequence,
        value: outcome,
        duplicate: false,
    };
    store
        .project_reorder_results
        .insert(meta.idempotency_key.as_str().into(), result.clone());
    Ok(result)
}

fn validate_project_reorder_entries(entries: &[ProductProjectReorderEntry]) -> ProductResult<()> {
    if entries.is_empty() {
        return Err(ProductError::InvalidInput {
            field: "ordered_project_ids".into(),
            message: "project order must not be empty".into(),
        });
    }
    if entries.iter().enumerate().any(|(index, entry)| {
        entries[..index]
            .iter()
            .any(|candidate| candidate.project_id == entry.project_id)
    }) {
        return Err(ProductError::InvalidInput {
            field: "ordered_project_ids".into(),
            message: "project order must not contain duplicate ids".into(),
        });
    }
    Ok(())
}

fn append_task_reorder_result(
    store: &mut InMemoryProductStore,
    meta: &ProductCommandMeta,
    outcome: ProductTaskReorderOutcome,
) -> ProductResult<ProductCommandResult<ProductTaskReorderOutcome>> {
    let sequence = store
        .product_events
        .last()
        .map(|event| event.sequence)
        .ok_or_else(|| ProductError::InvalidState {
            message: "task reorder command did not publish an event".into(),
        })?;
    let result = ProductCommandResult {
        command_id: meta.command_id.clone(),
        event_sequence: sequence,
        value: outcome,
        duplicate: false,
    };
    store
        .task_reorder_results
        .insert(meta.idempotency_key.as_str().into(), result.clone());
    Ok(result)
}

fn validate_task_reorder_entries(entries: &[ProductTaskReorderEntry]) -> ProductResult<()> {
    if entries.is_empty() {
        return Err(ProductError::InvalidInput {
            field: "ordered_task_ids".into(),
            message: "task order must not be empty".into(),
        });
    }
    if entries.iter().enumerate().any(|(index, entry)| {
        entries[..index]
            .iter()
            .any(|candidate| candidate.task_id == entry.task_id)
    }) {
        return Err(ProductError::InvalidInput {
            field: "ordered_task_ids".into(),
            message: "task order must not contain duplicate ids".into(),
        });
    }
    Ok(())
}

fn append_task_move_result(
    store: &mut InMemoryProductStore,
    meta: &ProductCommandMeta,
    outcome: ProductTaskMoveOutcome,
) -> ProductResult<ProductCommandResult<ProductTaskMoveOutcome>> {
    let sequence = store
        .product_events
        .last()
        .map(|event| event.sequence)
        .ok_or_else(|| ProductError::InvalidState {
            message: "task move command did not publish an event".into(),
        })?;
    let result = ProductCommandResult {
        command_id: meta.command_id.clone(),
        event_sequence: sequence,
        value: outcome,
        duplicate: false,
    };
    store
        .task_move_results
        .insert(meta.idempotency_key.as_str().into(), result.clone());
    Ok(result)
}

fn validate_task_move_parent(
    tasks: &[ProductTask],
    task_id: &TaskId,
    target_project_id: Option<&ProjectId>,
    target_parent_id: Option<&TaskId>,
) -> ProductResult<()> {
    let mut cursor = target_parent_id.cloned();
    let mut visited = Vec::new();
    while let Some(parent_id) = cursor {
        if &parent_id == task_id || visited.contains(&parent_id) {
            return Err(ProductError::InvalidInput {
                field: "target_parent_id".into(),
                message: "task parent would create a cycle".into(),
            });
        }
        let parent = tasks
            .iter()
            .find(|task| task.id == parent_id)
            .ok_or_else(|| ProductError::NotFound {
                entity: "task".into(),
                id: parent_id.as_str().into(),
            })?;
        if parent.archived || parent.project_id.as_ref() != target_project_id {
            return Err(ProductError::InvalidInput {
                field: "target_parent_id".into(),
                message: "task parent must be active in the target project".into(),
            });
        }
        visited.push(parent_id);
        cursor = parent.parent_id.clone();
    }
    Ok(())
}

fn task_subtree_ids(tasks: &[ProductTask], root_id: &TaskId) -> Vec<TaskId> {
    let mut ids = vec![root_id.clone()];
    loop {
        let mut added = tasks
            .iter()
            .filter(|task| {
                !ids.contains(&task.id)
                    && task
                        .parent_id
                        .as_ref()
                        .is_some_and(|parent_id| ids.contains(parent_id))
            })
            .map(|task| task.id.clone())
            .collect::<Vec<_>>();
        if added.is_empty() {
            break;
        }
        added.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids.extend(added);
    }
    ids
}

fn page_events(events: &[ProductEvent], request: &PageRequest) -> Page<ProductEvent> {
    let after = request.after.unwrap_or(ProductEventSequence::ORIGIN).get();
    let items = events
        .iter()
        .filter(|event| event.sequence.get() > after)
        .take(request.normalized_limit())
        .cloned()
        .collect::<Vec<_>>();
    let next = items.last().map(|event| event.sequence);
    Page { items, next }
}

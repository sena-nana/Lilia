use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use lilia_contracts::{
    AgentSessionBinding, ConflictKind, ExpectedRevision, Page, PageRequest, ProductCommandMeta,
    ProductCommandResult, ProductEntity, ProductEntityKind, ProductError, ProductEvent,
    ProductEventSequence, ProductResult, ProductTask, Project, ProjectId, TaskDependencyGraph,
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
    fn record_binding(&self, binding: AgentSessionBinding) -> ProductResult<AgentSessionBinding>;
    fn list_bindings_for_task(&self, task_id: &TaskId) -> ProductResult<Vec<AgentSessionBinding>>;
    /// Drop product Agent session bindings for a task (e.g. user session reset).
    fn clear_bindings_for_task(&self, task_id: &TaskId) -> ProductResult<usize>;
}

#[derive(Default)]
pub struct InMemoryProductStore {
    pub projects: HashMap<String, Project>,
    pub tasks: HashMap<String, ProductTask>,
    pub bindings: HashMap<String, AgentSessionBinding>,
    entities: HashMap<String, ProductEntity>,
    command_results: HashMap<String, ProductCommandResult<ProductEntity>>,
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
        self.entities.clear();
        self.command_results.clear();
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

    pub fn list_bindings_for_task(
        &self,
        task_id: &TaskId,
    ) -> ProductResult<Vec<AgentSessionBinding>> {
        self.repository.list_bindings_for_task(task_id)
    }

    pub fn clear_bindings_for_task(&self, task_id: &TaskId) -> ProductResult<usize> {
        self.repository.clear_bindings_for_task(task_id)
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

fn duplicate_command_result(
    meta: &ProductCommandMeta,
    mut result: ProductCommandResult<ProductEntity>,
) -> ProductResult<ProductCommandResult<ProductEntity>> {
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

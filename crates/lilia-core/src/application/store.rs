use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use lilia_contracts::{
    AgentSessionBinding, ConflictKind, ExpectedRevision, ProductError, ProductResult, ProductTask,
    Project, ProjectId, TaskDependencyGraph, TaskId,
};

use crate::domain::ensure_expected_revision;

#[derive(Default)]
pub struct InMemoryProductStore {
    pub projects: HashMap<String, Project>,
    pub tasks: HashMap<String, ProductTask>,
    pub bindings: HashMap<String, AgentSessionBinding>,
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

pub struct ProductServices {
    store: Arc<Mutex<InMemoryProductStore>>,
}

impl ProductServices {
    pub fn new(store: Arc<Mutex<InMemoryProductStore>>) -> Self {
        Self { store }
    }

    pub fn create_project(&self, id: ProjectId, name: impl Into<String>) -> ProductResult<Project> {
        let project = Project::new(id, name)?;
        let mut store = self.store.lock().expect("product store lock");
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

    pub fn create_task(
        &self,
        id: TaskId,
        project_id: Option<ProjectId>,
        title: impl Into<String>,
    ) -> ProductResult<ProductTask> {
        let task = ProductTask::new(id, project_id, title)?;
        let mut store = self.store.lock().expect("product store lock");
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

    pub fn update_task_dependencies(
        &self,
        task_id: &TaskId,
        depends_on: Vec<TaskId>,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductTask> {
        let mut store = self.store.lock().expect("product store lock");
        let task = store
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

    pub fn get_task(&self, task_id: &TaskId) -> ProductResult<ProductTask> {
        let store = self.store.lock().expect("product store lock");
        store
            .tasks
            .get(task_id.as_str())
            .cloned()
            .ok_or_else(|| ProductError::NotFound {
                entity: "task".into(),
                id: task_id.as_str().to_string(),
            })
    }

    pub fn get_project(&self, project_id: &ProjectId) -> ProductResult<Project> {
        let store = self.store.lock().expect("product store lock");
        store
            .projects
            .get(project_id.as_str())
            .cloned()
            .ok_or_else(|| ProductError::NotFound {
                entity: "project".into(),
                id: project_id.as_str().to_string(),
            })
    }

    pub fn list_bindings_for_task(&self, task_id: &TaskId) -> Vec<AgentSessionBinding> {
        let store = self.store.lock().expect("product store lock");
        store
            .bindings
            .values()
            .filter(|binding| binding.task_id == *task_id)
            .cloned()
            .collect()
    }

    pub fn record_binding(
        &self,
        binding: AgentSessionBinding,
    ) -> ProductResult<AgentSessionBinding> {
        let mut store = self.store.lock().expect("product store lock");
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
}

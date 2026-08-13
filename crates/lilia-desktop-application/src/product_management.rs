use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::{
    ConversationId, ExpectedRevision, IdempotencyKey, ProductCommandMeta, ProductConversation,
    ProductEntity, ProductEntityKind, ProductProjectRemovalOutcome, ProductProjectReorderEntry,
    ProductRevision, ProductTask, ProductTaskMoveInput, ProductTaskPriority,
    ProductTaskReorderEntry, ProductTaskStatus, Project, ProjectArchiveState, ProjectId, TaskId,
};
use uuid::Uuid;

use crate::{DesktopApplication, DesktopApplicationError, DesktopEventKind, TaskQuery};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DesktopOptionalTextUpdate {
    #[default]
    Unchanged,
    Set(String),
    Clear,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopProjectCreate {
    pub id: ProjectId,
    pub name: String,
    pub workspace_path: Option<String>,
}

impl DesktopProjectCreate {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: ProjectId::new(format!("project-{}", Uuid::new_v4()))
                .expect("UUID-backed project ids are valid"),
            name: name.into(),
            workspace_path: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesktopProjectPatch {
    pub name: Option<String>,
    pub workspace_path: DesktopOptionalTextUpdate,
    pub pinned: Option<bool>,
    pub sort_order: Option<i64>,
    pub archived: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopProjectRemovalPreview {
    pub project_id: ProjectId,
    pub project_name: String,
    pub workspace_path: Option<String>,
    pub active_task_count: usize,
    pub active_conversation_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopTaskCreate {
    pub id: TaskId,
    pub project_id: Option<ProjectId>,
    pub parent_id: Option<TaskId>,
    pub title: String,
}

impl DesktopTaskCreate {
    pub fn new(project_id: Option<ProjectId>, title: impl Into<String>) -> Self {
        Self {
            id: TaskId::new(format!("task-{}", Uuid::new_v4()))
                .expect("UUID-backed task ids are valid"),
            project_id,
            parent_id: None,
            title: title.into(),
        }
    }

    pub fn with_parent(mut self, parent_id: TaskId) -> Self {
        self.parent_id = Some(parent_id);
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesktopTaskPatch {
    pub title: Option<String>,
    pub description: DesktopOptionalTextUpdate,
    pub status: Option<ProductTaskStatus>,
    pub priority: Option<ProductTaskPriority>,
    pub pinned: Option<bool>,
    pub sort_order: Option<i64>,
    pub archived: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopTaskMove {
    pub target_project_id: Option<ProjectId>,
    pub target_parent_id: Option<TaskId>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DesktopTaskRunBlock {
    Archived {
        task_id: TaskId,
    },
    Blocked {
        task_id: TaskId,
        title: String,
    },
    DependencyIncomplete {
        task_id: TaskId,
        title: String,
        status: ProductTaskStatus,
    },
    DependencyCycle {
        task_id: TaskId,
    },
}

impl std::fmt::Display for DesktopTaskRunBlock {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Archived { .. } => formatter.write_str("任务已归档，暂不能启动会话"),
            Self::Blocked { title, .. } => {
                write!(formatter, "任务已标记为阻塞，暂不能启动会话：{title}")
            }
            Self::DependencyIncomplete { title, status, .. } => write!(
                formatter,
                "任务依赖未完成，暂不能启动会话：{title}（{}）",
                task_status_label(*status)
            ),
            Self::DependencyCycle { .. } => formatter.write_str("任务依赖存在循环，暂不能启动会话"),
        }
    }
}

impl DesktopApplication {
    pub fn task_run_block(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopTaskRunBlock>, DesktopApplicationError> {
        let task = self.get_task(task_id)?;
        if task.archived {
            return Ok(Some(DesktopTaskRunBlock::Archived { task_id: task.id }));
        }
        if task.status == ProductTaskStatus::Blocked {
            return Ok(Some(DesktopTaskRunBlock::Blocked {
                task_id: task.id,
                title: task.title,
            }));
        }
        let tasks = self
            .query_tasks(TaskQuery::default().including_archived())?
            .into_iter()
            .map(|task| (task.id.clone(), task))
            .collect::<BTreeMap<_, _>>();
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        Ok(dependency_run_block(
            &task,
            &tasks,
            &mut visiting,
            &mut visited,
        ))
    }

    pub fn ensure_task_runnable(&self, task_id: &TaskId) -> Result<(), DesktopApplicationError> {
        if let Some(block) = self.task_run_block(task_id)? {
            return Err(DesktopApplicationError::InvalidInput {
                field: "task_id",
                message: block.to_string(),
            });
        }
        Ok(())
    }

    pub fn create_project(
        &self,
        input: DesktopProjectCreate,
    ) -> Result<Project, DesktopApplicationError> {
        let mut project = Project::new(input.id.clone(), input.name)?;
        project.workspace_path = normalized_optional_text(input.workspace_path);
        project.sort_order = self
            .query_projects(crate::ProjectQuery {
                include_archived: true,
            })?
            .into_iter()
            .map(|project| project.sort_order)
            .max()
            .unwrap_or(-1)
            .saturating_add(1);
        let key = format!("desktop:create-project:{}", input.id.as_str());
        let result = self.authority().client()?.create_product_entity(
            &create_meta(&key)?,
            ProductEntity::Project(project),
            "desktop_create_project",
        )?;
        let project = project_entity(result.value)?;
        if !result.duplicate {
            self.emit_event(DesktopEventKind::ProjectsChanged);
        }
        Ok(project)
    }

    pub fn update_project(
        &self,
        project_id: &ProjectId,
        patch: DesktopProjectPatch,
    ) -> Result<Project, DesktopApplicationError> {
        let mut project = self.get_project(project_id)?;
        if let Some(name) = patch.name {
            validate_required_text("name", &name)?;
            project.name = name.trim().to_owned();
        }
        apply_optional_text(&mut project.workspace_path, patch.workspace_path);
        if let Some(pinned) = patch.pinned {
            project.pinned = pinned;
        }
        if let Some(sort_order) = patch.sort_order {
            project.sort_order = sort_order;
        }
        if let Some(archived) = patch.archived {
            project.archive = if archived {
                ProjectArchiveState::Archived
            } else {
                ProjectArchiveState::Active
            };
        }
        let current = self.get_project(project_id)?;
        if project == current {
            return Ok(current);
        }
        let meta = update_meta("project", project.id.as_str(), current.revision)?;
        let result = self.authority().client()?.update_product_entity(
            &meta,
            ProductEntity::Project(project),
            "desktop_update_project",
        )?;
        let project = project_entity(result.value)?;
        if !result.duplicate {
            self.emit_event(DesktopEventKind::ProjectsChanged);
        }
        Ok(project)
    }

    pub fn project_removal_preview(
        &self,
        project_id: &ProjectId,
    ) -> Result<DesktopProjectRemovalPreview, DesktopApplicationError> {
        let project = self.get_project(project_id)?;
        let active_task_count = self
            .query_tasks(TaskQuery::for_project(project_id.clone()))?
            .len();
        let active_conversation_count = self
            .authority()
            .client()?
            .products()
            .list_entities(ProductEntityKind::Conversation)?
            .into_iter()
            .filter(|entity| {
                matches!(
                    entity,
                    ProductEntity::Conversation(conversation)
                        if conversation.project_id.as_ref() == Some(project_id)
                            && !conversation.archived
                )
            })
            .count();
        Ok(DesktopProjectRemovalPreview {
            project_id: project.id,
            project_name: project.name,
            workspace_path: project.workspace_path,
            active_task_count,
            active_conversation_count,
        })
    }

    pub fn remove_project(
        &self,
        project_id: &ProjectId,
    ) -> Result<ProductProjectRemovalOutcome, DesktopApplicationError> {
        let project = self.get_project(project_id)?;
        if project.archive == ProjectArchiveState::Archived {
            return Ok(ProductProjectRemovalOutcome {
                project,
                moved_task_ids: Vec::new(),
                moved_conversation_ids: Vec::new(),
                already_removed: true,
            });
        }
        let meta = update_meta("remove-project", project.id.as_str(), project.revision)?;
        let result = self
            .authority()
            .client()?
            .remove_project(&meta, project_id, now_millis())?;
        if !result.duplicate {
            self.emit_event(DesktopEventKind::ProjectsChanged);
            self.emit_event(DesktopEventKind::TasksChanged {
                project_id: Some(project_id.clone()),
                task_id: None,
            });
            self.emit_event(DesktopEventKind::TasksChanged {
                project_id: None,
                task_id: None,
            });
        }
        Ok(result.value)
    }

    pub fn reorder_projects(
        &self,
        ordered_ids: &[ProjectId],
    ) -> Result<Vec<Project>, DesktopApplicationError> {
        if ordered_ids.is_empty() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "ordered_project_ids",
                message: "project order must not be empty".to_owned(),
            });
        }
        if ordered_ids
            .iter()
            .enumerate()
            .any(|(index, id)| ordered_ids[..index].contains(id))
        {
            return Err(DesktopApplicationError::InvalidInput {
                field: "ordered_project_ids",
                message: "project order must not contain duplicate ids".to_owned(),
            });
        }

        let active = self.query_projects(crate::ProjectQuery::default())?;
        let Some(first) = active.iter().find(|project| project.id == ordered_ids[0]) else {
            return Err(DesktopApplicationError::InvalidInput {
                field: "ordered_project_ids",
                message: format!("project `{}` is not active", ordered_ids[0].as_str()),
            });
        };
        let pinned = first.pinned;
        let group = active
            .iter()
            .filter(|project| project.pinned == pinned)
            .collect::<Vec<_>>();
        if group.len() != ordered_ids.len()
            || group
                .iter()
                .any(|project| !ordered_ids.contains(&project.id))
        {
            return Err(DesktopApplicationError::InvalidInput {
                field: "ordered_project_ids",
                message: "project order must contain one complete pinned group".to_owned(),
            });
        }
        if group
            .iter()
            .map(|project| &project.id)
            .eq(ordered_ids.iter())
        {
            return Ok(active);
        }

        let entries = ordered_ids
            .iter()
            .map(|project_id| {
                let project = group
                    .iter()
                    .find(|project| project.id == *project_id)
                    .expect("complete project group was validated");
                Ok(ProductProjectReorderEntry {
                    project_id: project_id.clone(),
                    expected_revision: ExpectedRevision::new(project.revision.get())?,
                })
            })
            .collect::<Result<Vec<_>, DesktopApplicationError>>()?;
        let key = format!(
            "desktop:reorder-projects:{}",
            entries
                .iter()
                .map(|entry| format!(
                    "{}@{}",
                    entry.project_id.as_str(),
                    entry.expected_revision.get()
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        self.authority()
            .client()?
            .reorder_projects(&create_meta(&key)?, &entries)?;
        self.query_projects(crate::ProjectQuery::default())
    }

    pub fn create_task(
        &self,
        input: DesktopTaskCreate,
    ) -> Result<ProductTask, DesktopApplicationError> {
        if let Some(project_id) = &input.project_id {
            self.get_project(project_id)?;
        }
        self.validate_task_parent(
            &input.id,
            input.project_id.as_ref(),
            input.parent_id.as_ref(),
        )?;
        let mut task = ProductTask::new(
            input.id.clone(),
            input.project_id.clone(),
            input.title.clone(),
        )?;
        task.parent_id = input.parent_id.clone();
        task.sort_order = self
            .query_tasks(
                TaskQuery::for_project_or_inbox(input.project_id.clone()).including_archived(),
            )?
            .into_iter()
            .map(|task| task.sort_order)
            .max()
            .unwrap_or(-1)
            .saturating_add(1);
        task.created_at = now_millis();
        task.updated_at = task.created_at;
        let key = format!("desktop:create-task:{}", input.id.as_str());
        let result = self.authority().client()?.create_product_entity(
            &create_meta(&key)?,
            ProductEntity::Task(task),
            "desktop_create_task",
        )?;
        let task = task_entity(result.value)?;
        self.ensure_task_conversation(&task, &input.title)?;
        if !result.duplicate {
            self.emit_event(DesktopEventKind::TasksChanged {
                project_id: task.project_id.clone(),
                task_id: Some(task.id.clone()),
            });
        }
        Ok(task)
    }

    pub fn update_task(
        &self,
        task_id: &TaskId,
        patch: DesktopTaskPatch,
    ) -> Result<ProductTask, DesktopApplicationError> {
        let mut task = self.get_task(task_id)?;
        let current = task.clone();
        if let Some(title) = patch.title {
            validate_required_text("title", &title)?;
            task.title = title.trim().to_owned();
        }
        apply_optional_text(&mut task.description, patch.description);
        if let Some(status) = patch.status {
            task.status = status;
        }
        if let Some(priority) = patch.priority {
            task.priority = priority;
        }
        if let Some(pinned) = patch.pinned {
            task.pinned = pinned;
        }
        if let Some(sort_order) = patch.sort_order {
            task.sort_order = sort_order;
        }
        if let Some(archived) = patch.archived {
            task.archived = archived;
        }
        if task == current {
            return Ok(current);
        }
        task.updated_at = now_millis().max(current.updated_at);
        let meta = update_meta("task", task.id.as_str(), current.revision)?;
        let result = self.authority().client()?.update_product_entity(
            &meta,
            ProductEntity::Task(task),
            "desktop_update_task",
        )?;
        let task = task_entity(result.value)?;
        if !result.duplicate {
            self.emit_event(DesktopEventKind::TasksChanged {
                project_id: task.project_id.clone(),
                task_id: Some(task.id.clone()),
            });
        }
        Ok(task)
    }

    pub fn update_task_dependencies(
        &self,
        task_id: &TaskId,
        depends_on: Vec<TaskId>,
    ) -> Result<ProductTask, DesktopApplicationError> {
        let current = self.get_task(task_id)?;
        if current.depends_on == depends_on {
            return Ok(current);
        }
        let task = self.authority().client()?.update_task_dependencies(
            task_id,
            depends_on,
            ExpectedRevision::new(current.revision.get())?,
        )?;
        self.emit_event(DesktopEventKind::TasksChanged {
            project_id: task.project_id.clone(),
            task_id: Some(task.id.clone()),
        });
        Ok(task)
    }

    pub fn reorder_tasks(
        &self,
        project_id: Option<ProjectId>,
        ordered_ids: &[TaskId],
    ) -> Result<Vec<ProductTask>, DesktopApplicationError> {
        if ordered_ids.is_empty() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "ordered_task_ids",
                message: "task order must not be empty".to_owned(),
            });
        }
        if ordered_ids
            .iter()
            .enumerate()
            .any(|(index, id)| ordered_ids[..index].contains(id))
        {
            return Err(DesktopApplicationError::InvalidInput {
                field: "ordered_task_ids",
                message: "task order must not contain duplicate ids".to_owned(),
            });
        }

        let query = TaskQuery::for_project_or_inbox(project_id.clone());
        let active = self.query_tasks(query.clone())?;
        let Some(first) = active.iter().find(|task| task.id == ordered_ids[0]) else {
            return Err(DesktopApplicationError::InvalidInput {
                field: "ordered_task_ids",
                message: format!("task `{}` is not active", ordered_ids[0].as_str()),
            });
        };
        let pinned = first.pinned;
        let group = active
            .iter()
            .filter(|task| task.pinned == pinned)
            .collect::<Vec<_>>();
        if group.len() != ordered_ids.len()
            || group.iter().any(|task| !ordered_ids.contains(&task.id))
        {
            return Err(DesktopApplicationError::InvalidInput {
                field: "ordered_task_ids",
                message: "task order must contain one complete pinned group".to_owned(),
            });
        }
        if group.iter().map(|task| &task.id).eq(ordered_ids.iter()) {
            return Ok(active);
        }

        let entries = ordered_ids
            .iter()
            .map(|task_id| {
                let task = group
                    .iter()
                    .find(|task| task.id == *task_id)
                    .expect("complete task group was validated");
                Ok(ProductTaskReorderEntry {
                    task_id: task_id.clone(),
                    expected_revision: ExpectedRevision::new(task.revision.get())?,
                })
            })
            .collect::<Result<Vec<_>, DesktopApplicationError>>()?;
        let key = format!(
            "desktop:reorder-tasks:{}",
            entries
                .iter()
                .map(|entry| format!(
                    "{}@{}",
                    entry.task_id.as_str(),
                    entry.expected_revision.get()
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        let result = self
            .authority()
            .client()?
            .reorder_tasks(&create_meta(&key)?, &entries)?;
        if !result.duplicate {
            self.emit_event(DesktopEventKind::TasksChanged {
                project_id: project_id.clone(),
                task_id: None,
            });
        }
        self.query_tasks(query)
    }

    pub fn move_task(
        &self,
        task_id: &TaskId,
        request: DesktopTaskMove,
    ) -> Result<ProductTask, DesktopApplicationError> {
        let current = self.get_task(task_id)?;
        if current.archived {
            return Err(DesktopApplicationError::InvalidInput {
                field: "task_id",
                message: "archived tasks cannot be moved".to_owned(),
            });
        }
        if let Some(project_id) = &request.target_project_id {
            let project = self.get_project(project_id)?;
            if project.archive == ProjectArchiveState::Archived {
                return Err(DesktopApplicationError::InvalidInput {
                    field: "target_project_id",
                    message: "target project is archived".to_owned(),
                });
            }
        }
        self.validate_task_parent(
            task_id,
            request.target_project_id.as_ref(),
            request.target_parent_id.as_ref(),
        )?;
        if current.project_id == request.target_project_id
            && current.parent_id == request.target_parent_id
        {
            return Ok(current);
        }

        let meta = update_meta("task", current.id.as_str(), current.revision)?;
        let result = self.authority().client()?.move_task(
            &meta,
            &ProductTaskMoveInput {
                task_id: task_id.clone(),
                target_project_id: request.target_project_id,
                target_parent_id: request.target_parent_id,
                expected_revision: ExpectedRevision::new(current.revision.get())?,
                moved_at: now_millis(),
            },
        )?;
        let moved = result.value.task;
        let source_project_id = current.project_id.clone();
        if !result.duplicate {
            self.emit_event(DesktopEventKind::TasksChanged {
                project_id: source_project_id.clone(),
                task_id: Some(moved.id.clone()),
            });
            if moved.project_id != source_project_id {
                self.emit_event(DesktopEventKind::TasksChanged {
                    project_id: moved.project_id.clone(),
                    task_id: Some(moved.id.clone()),
                });
            }
        }
        Ok(moved)
    }

    fn validate_task_parent(
        &self,
        task_id: &TaskId,
        target_project_id: Option<&ProjectId>,
        target_parent_id: Option<&TaskId>,
    ) -> Result<(), DesktopApplicationError> {
        let mut cursor = target_parent_id.cloned();
        let mut visited = Vec::new();
        while let Some(parent_id) = cursor {
            if &parent_id == task_id || visited.contains(&parent_id) {
                return Err(DesktopApplicationError::InvalidInput {
                    field: "target_parent_id",
                    message: "task parent would create a cycle".to_owned(),
                });
            }
            let parent = self.get_task(&parent_id)?;
            if parent.archived || parent.project_id.as_ref() != target_project_id {
                return Err(DesktopApplicationError::InvalidInput {
                    field: "target_parent_id",
                    message: "task parent must be active in the target project".to_owned(),
                });
            }
            visited.push(parent_id);
            cursor = parent.parent_id;
        }
        Ok(())
    }

    #[cfg(test)]
    fn task_conversations(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<ProductConversation>, DesktopApplicationError> {
        let mut conversations = self
            .authority()
            .client()?
            .products()
            .list_entities(ProductEntityKind::Conversation)?
            .into_iter()
            .filter_map(|entity| match entity {
                ProductEntity::Conversation(conversation)
                    if conversation.task_id.as_ref() == Some(task_id) =>
                {
                    Some(conversation)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        conversations.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        Ok(conversations)
    }

    pub(crate) fn ensure_task_conversation(
        &self,
        task: &ProductTask,
        title: &str,
    ) -> Result<(), DesktopApplicationError> {
        let conversation_id = ConversationId::new(format!("conversation:{}", task.id.as_str()))?;
        let conversation = ProductConversation::new(
            conversation_id,
            task.project_id.clone(),
            Some(task.id.clone()),
            title,
        )?;
        let key = format!("desktop:create-task-conversation:{}", task.id.as_str());
        self.authority().client()?.create_product_entity(
            &create_meta(&key)?,
            ProductEntity::Conversation(conversation),
            "desktop_create_task_conversation",
        )?;
        Ok(())
    }
}

fn dependency_run_block(
    task: &ProductTask,
    tasks: &BTreeMap<TaskId, ProductTask>,
    visiting: &mut BTreeSet<TaskId>,
    visited: &mut BTreeSet<TaskId>,
) -> Option<DesktopTaskRunBlock> {
    if visited.contains(&task.id) {
        return None;
    }
    if !visiting.insert(task.id.clone()) {
        return Some(DesktopTaskRunBlock::DependencyCycle {
            task_id: task.id.clone(),
        });
    }
    for dependency_id in &task.depends_on {
        let Some(dependency) = tasks
            .get(dependency_id)
            .filter(|dependency| !dependency.archived)
        else {
            continue;
        };
        if visiting.contains(dependency_id) {
            return Some(DesktopTaskRunBlock::DependencyCycle {
                task_id: dependency_id.clone(),
            });
        }
        if dependency.status != ProductTaskStatus::Done {
            return Some(DesktopTaskRunBlock::DependencyIncomplete {
                task_id: dependency.id.clone(),
                title: dependency.title.clone(),
                status: dependency.status,
            });
        }
        if let Some(block) = dependency_run_block(dependency, tasks, visiting, visited) {
            return Some(block);
        }
    }
    visiting.remove(&task.id);
    visited.insert(task.id.clone());
    None
}

fn task_status_label(status: ProductTaskStatus) -> &'static str {
    match status {
        ProductTaskStatus::Draft => "草稿",
        ProductTaskStatus::Waiting => "等待中",
        ProductTaskStatus::Running => "运行中",
        ProductTaskStatus::Blocked => "阻塞",
        ProductTaskStatus::Done => "完成",
        ProductTaskStatus::Cancelled => "已取消",
    }
}

fn create_meta(key: &str) -> Result<ProductCommandMeta, DesktopApplicationError> {
    Ok(ProductCommandMeta::create(key, IdempotencyKey::new(key)?)?)
}

fn update_meta(
    kind: &str,
    id: &str,
    revision: ProductRevision,
) -> Result<ProductCommandMeta, DesktopApplicationError> {
    let key = format!("desktop:update-{kind}:{id}:revision:{}", revision.get());
    Ok(ProductCommandMeta::update(
        key.clone(),
        IdempotencyKey::new(key)?,
        ExpectedRevision::new(revision.get())?,
    )?)
}

fn validate_required_text(field: &'static str, value: &str) -> Result<(), DesktopApplicationError> {
    if value.trim().is_empty() {
        return Err(DesktopApplicationError::InvalidInput {
            field,
            message: format!("{field} must not be empty"),
        });
    }
    Ok(())
}

fn normalized_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn apply_optional_text(target: &mut Option<String>, update: DesktopOptionalTextUpdate) {
    match update {
        DesktopOptionalTextUpdate::Unchanged => {}
        DesktopOptionalTextUpdate::Set(value) => *target = normalized_optional_text(Some(value)),
        DesktopOptionalTextUpdate::Clear => *target = None,
    }
}

fn project_entity(entity: ProductEntity) -> Result<Project, DesktopApplicationError> {
    match entity {
        ProductEntity::Project(project) => Ok(project),
        _ => Err(DesktopApplicationError::InvalidInput {
            field: "entity",
            message: "product command returned a non-project entity".to_owned(),
        }),
    }
}

fn task_entity(entity: ProductEntity) -> Result<ProductTask, DesktopApplicationError> {
    match entity {
        ProductEntity::Task(task) => Ok(task),
        _ => Err(DesktopApplicationError::InvalidInput {
            field: "entity",
            message: "product command returned a non-task entity".to_owned(),
        }),
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use lilia_contracts::{ExpectedRevision, ProductEntityKind};
    use lilia_service::ServiceAuthority;
    use tempfile::tempdir;

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopCommand, DesktopHost, DesktopHostAction,
        DesktopHostContext, DesktopHostError, DesktopHostResult, ProjectQuery,
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
        let archived = app
            .update_task(
                &task.id,
                DesktopTaskPatch {
                    archived: Some(true),
                    ..DesktopTaskPatch::default()
                },
            )
            .unwrap();
        assert!(archived.archived);
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
        assert!(matches!(changed[0].kind, DesktopEventKind::ProjectsChanged));
        assert!(matches!(
            changed[1].kind,
            DesktopEventKind::TasksChanged {
                project_id: Some(ref id),
                task_id: None,
            } if id == &project.id
        ));
        assert!(matches!(
            changed[2].kind,
            DesktopEventKind::TasksChanged {
                project_id: None,
                task_id: None,
            }
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
        let conversations = app.task_conversations(&child.id).unwrap();
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
        assert_eq!(app.task_conversations(&child.id).unwrap().len(), 1);

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

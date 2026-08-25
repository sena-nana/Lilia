//! Project and task commands and queries.
//!
//! Every mutation goes through the shared product authority and reports the
//! resulting fact change on [`ProjectTaskEvents`]; the service holds no cached
//! copy of a project or task.

use std::sync::Arc;

use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::{
    ConversationId, ExpectedRevision, IdempotencyKey, ProductCommandMeta, ProductConversation,
    ProductConversationStatus, ProductEntity, ProductEntityKind,
    ProductProjectArchiveConversationEntry, ProductProjectArchiveInput,
    ProductProjectArchiveOutcome, ProductProjectArchiveTaskEntry, ProductProjectRemovalOutcome,
    ProductProjectReorderEntry, ProductRevision, ProductTask, ProductTaskArchiveConversationEntry,
    ProductTaskArchiveInput, ProductTaskArchiveOutcome, ProductTaskMoveInput, ProductTaskPriority,
    ProductTaskReorderEntry, ProductTaskStatus, Project, ProjectArchiveState, ProjectId, TaskId,
};
use lilia_service::ServiceAuthority;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::events::ProjectTaskEvents;
use crate::query::{DesktopTaskScope, ProjectQuery, TaskQuery};
use crate::TaskError;

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

/// Authority-backed project and task service.
#[derive(Clone)]
pub struct ProjectTaskService {
    authority: ServiceAuthority,
    events: Arc<dyn ProjectTaskEvents>,
}

impl ProjectTaskService {
    pub fn new(authority: ServiceAuthority, events: Arc<dyn ProjectTaskEvents>) -> Self {
        Self { authority, events }
    }

    pub fn authority(&self) -> &ServiceAuthority {
        &self.authority
    }

    pub fn query_projects(&self, query: ProjectQuery) -> Result<Vec<Project>, TaskError> {
        let mut projects = self.authority.client()?.products().list_projects()?;
        if !query.include_archived {
            projects.retain(|project| project.archive == ProjectArchiveState::Active);
        }
        Ok(projects)
    }

    pub fn get_project(&self, project_id: &ProjectId) -> Result<Project, TaskError> {
        Ok(self.authority.client()?.products().get_project(project_id)?)
    }

    pub fn query_tasks(&self, query: TaskQuery) -> Result<Vec<ProductTask>, TaskError> {
        let mut tasks = self.authority.client()?.products().list_tasks()?;
        match query.scope {
            DesktopTaskScope::All => {}
            DesktopTaskScope::Project(project_id) => {
                tasks.retain(|task| task.project_id.as_ref() == Some(&project_id));
            }
            DesktopTaskScope::Inbox => {
                tasks.retain(|task| task.project_id.is_none());
            }
        }
        if !query.include_archived {
            tasks.retain(|task| !task.archived);
        }
        Ok(tasks)
    }

    pub fn get_task(&self, task_id: &TaskId) -> Result<ProductTask, TaskError> {
        Ok(self.authority.client()?.products().get_task(task_id)?)
    }

    pub fn task_run_block(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopTaskRunBlock>, TaskError> {
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

    pub fn ensure_task_runnable(&self, task_id: &TaskId) -> Result<(), TaskError> {
        if let Some(block) = self.task_run_block(task_id)? {
            return Err(TaskError::InvalidInput {
                field: "task_id",
                message: block.to_string(),
            });
        }
        Ok(())
    }

    pub fn create_project(
        &self,
        input: DesktopProjectCreate,
    ) -> Result<Project, TaskError> {
        let mut project = Project::new(input.id.clone(), input.name)?;
        project.workspace_path = normalized_optional_text(input.workspace_path);
        project.sort_order = self
            .query_projects(ProjectQuery {
                include_archived: true,
            })?
            .into_iter()
            .map(|project| project.sort_order)
            .max()
            .unwrap_or(-1)
            .saturating_add(1);
        let key = format!("desktop:create-project:{}", input.id.as_str());
        let result = self.authority.client()?.create_product_entity(
            &create_meta(&key)?,
            ProductEntity::Project(project),
            "desktop_create_project",
        )?;
        let project = project_entity(result.value)?;
        if !result.duplicate {
            self.events.projects_changed();
        }
        Ok(project)
    }

    pub fn update_project(
        &self,
        project_id: &ProjectId,
        patch: DesktopProjectPatch,
    ) -> Result<Project, TaskError> {
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
        let result = self.authority.client()?.update_product_entity(
            &meta,
            ProductEntity::Project(project),
            "desktop_update_project",
        )?;
        let project = project_entity(result.value)?;
        if !result.duplicate {
            self.events.projects_changed();
        }
        Ok(project)
    }

    pub fn project_removal_preview(
        &self,
        project_id: &ProjectId,
    ) -> Result<DesktopProjectRemovalPreview, TaskError> {
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
    ) -> Result<ProductProjectRemovalOutcome, TaskError> {
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
            self.events.projects_changed();
            self.events.tasks_changed(Some(project_id.clone()), None);
            self.events.tasks_changed(None, None);
        }
        Ok(result.value)
    }

    pub fn archive_project_conversations(
        &self,
        project_id: &ProjectId,
    ) -> Result<ProductProjectArchiveOutcome, TaskError> {
        let project = self.get_project(project_id)?;
        if project.archive == ProjectArchiveState::Archived {
            return Err(TaskError::InvalidInput {
                field: "project_id",
                message: "archived project conversations cannot be archived again".to_owned(),
            });
        }
        let tasks = self.query_tasks(TaskQuery::for_project(project_id.clone()))?;
        if tasks.is_empty() {
            return Ok(ProductProjectArchiveOutcome {
                archived_tasks: Vec::new(),
                archived_conversations: Vec::new(),
            });
        }
        let task_ids = tasks
            .iter()
            .map(|task| task.id.clone())
            .collect::<BTreeSet<_>>();
        let mut conversations = self
            .authority()
            .client()?
            .products()
            .list_entities(ProductEntityKind::Conversation)?
            .into_iter()
            .filter_map(|entity| match entity {
                ProductEntity::Conversation(conversation)
                    if !conversation.archived
                        && conversation
                            .task_id
                            .as_ref()
                            .is_some_and(|task_id| task_ids.contains(task_id)) =>
                {
                    Some(conversation)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        conversations.sort_by(|left, right| left.id.cmp(&right.id));
        let mut fingerprint = Sha256::new();
        fingerprint.update(project.id.as_str().as_bytes());
        fingerprint.update(project.revision.get().to_le_bytes());
        for task in &tasks {
            fingerprint.update(task.id.as_str().as_bytes());
            fingerprint.update(task.revision.get().to_le_bytes());
        }
        for conversation in &conversations {
            fingerprint.update(conversation.id.as_str().as_bytes());
            fingerprint.update(conversation.revision.get().to_le_bytes());
        }
        let key = format!(
            "desktop:archive-project-conversations:{}:{:x}",
            project.id,
            fingerprint.finalize()
        );
        let result = self.authority.client()?.archive_project(
            &create_meta(&key)?,
            &ProductProjectArchiveInput {
                project_id: project.id.clone(),
                expected_project_revision: ExpectedRevision::new(project.revision.get())?,
                tasks: tasks
                    .iter()
                    .map(|task| {
                        Ok(ProductProjectArchiveTaskEntry {
                            task_id: task.id.clone(),
                            expected_revision: ExpectedRevision::new(task.revision.get())?,
                        })
                    })
                    .collect::<Result<Vec<_>, TaskError>>()?,
                conversations: conversations
                    .iter()
                    .map(|conversation| {
                        Ok(ProductProjectArchiveConversationEntry {
                            conversation_id: conversation.id.clone(),
                            expected_revision: ExpectedRevision::new(conversation.revision.get())?,
                        })
                    })
                    .collect::<Result<Vec<_>, TaskError>>()?,
                archived_at: now_millis(),
            },
        )?;
        if !result.duplicate {
            self.events.tasks_changed(Some(project_id.clone()), None);
        }
        Ok(result.value)
    }

    pub fn reorder_projects(
        &self,
        ordered_ids: &[ProjectId],
    ) -> Result<Vec<Project>, TaskError> {
        if ordered_ids.is_empty() {
            return Err(TaskError::InvalidInput {
                field: "ordered_project_ids",
                message: "project order must not be empty".to_owned(),
            });
        }
        if ordered_ids
            .iter()
            .enumerate()
            .any(|(index, id)| ordered_ids[..index].contains(id))
        {
            return Err(TaskError::InvalidInput {
                field: "ordered_project_ids",
                message: "project order must not contain duplicate ids".to_owned(),
            });
        }

        let active = self.query_projects(ProjectQuery::default())?;
        let Some(first) = active.iter().find(|project| project.id == ordered_ids[0]) else {
            return Err(TaskError::InvalidInput {
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
            return Err(TaskError::InvalidInput {
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
            .collect::<Result<Vec<_>, TaskError>>()?;
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
        self.authority
            .client()?
            .reorder_projects(&create_meta(&key)?, &entries)?;
        self.query_projects(ProjectQuery::default())
    }

    pub fn create_task(
        &self,
        input: DesktopTaskCreate,
    ) -> Result<ProductTask, TaskError> {
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
        let result = self.authority.client()?.create_product_entity(
            &create_meta(&key)?,
            ProductEntity::Task(task),
            "desktop_create_task",
        )?;
        let task = task_entity(result.value)?;
        self.ensure_task_conversation(&task, &input.title)?;
        if !result.duplicate {
            self.events.tasks_changed(task.project_id.clone(), Some(task.id.clone()));
        }
        Ok(task)
    }

    pub fn update_task(
        &self,
        task_id: &TaskId,
        patch: DesktopTaskPatch,
    ) -> Result<ProductTask, TaskError> {
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
        let result = self.authority.client()?.update_product_entity(
            &meta,
            ProductEntity::Task(task),
            "desktop_update_task",
        )?;
        let task = task_entity(result.value)?;
        if !result.duplicate {
            self.events.tasks_changed(task.project_id.clone(), Some(task.id.clone()));
        }
        Ok(task)
    }

    pub fn set_task_archived(
        &self,
        task_id: &TaskId,
        archived: bool,
    ) -> Result<ProductTaskArchiveOutcome, TaskError> {
        let task = self.get_task(task_id)?;
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
        conversations.sort_by(|left, right| left.id.cmp(&right.id));
        let desired_status = if archived {
            ProductConversationStatus::Closed
        } else {
            ProductConversationStatus::Active
        };
        if task.archived == archived
            && conversations.iter().all(|conversation| {
                conversation.archived == archived && conversation.status == desired_status
            })
        {
            return Ok(ProductTaskArchiveOutcome {
                task,
                conversations,
            });
        }

        let mut fingerprint = Sha256::new();
        fingerprint.update(task.id.as_str().as_bytes());
        fingerprint.update(task.revision.get().to_le_bytes());
        fingerprint.update([u8::from(archived)]);
        for conversation in &conversations {
            fingerprint.update(conversation.id.as_str().as_bytes());
            fingerprint.update(conversation.revision.get().to_le_bytes());
        }
        let key = format!(
            "desktop:set-task-archived:{}:{:x}",
            task.id,
            fingerprint.finalize()
        );
        let result = self.authority.client()?.set_task_archived(
            &create_meta(&key)?,
            &ProductTaskArchiveInput {
                task_id: task.id.clone(),
                expected_revision: ExpectedRevision::new(task.revision.get())?,
                conversations: conversations
                    .iter()
                    .map(|conversation| {
                        Ok(ProductTaskArchiveConversationEntry {
                            conversation_id: conversation.id.clone(),
                            expected_revision: ExpectedRevision::new(conversation.revision.get())?,
                        })
                    })
                    .collect::<Result<Vec<_>, TaskError>>()?,
                archived,
                updated_at: now_millis(),
            },
        )?;
        if !result.duplicate {
            self.events.tasks_changed(result.value.task.project_id.clone(), Some(result.value.task.id.clone()));
        }
        Ok(result.value)
    }

    pub fn update_task_dependencies(
        &self,
        task_id: &TaskId,
        depends_on: Vec<TaskId>,
    ) -> Result<ProductTask, TaskError> {
        let current = self.get_task(task_id)?;
        if current.depends_on == depends_on {
            return Ok(current);
        }
        let task = self.authority.client()?.update_task_dependencies(
            task_id,
            depends_on,
            ExpectedRevision::new(current.revision.get())?,
        )?;
        self.events.tasks_changed(task.project_id.clone(), Some(task.id.clone()));
        Ok(task)
    }

    pub fn reorder_tasks(
        &self,
        project_id: Option<ProjectId>,
        ordered_ids: &[TaskId],
    ) -> Result<Vec<ProductTask>, TaskError> {
        if ordered_ids.is_empty() {
            return Err(TaskError::InvalidInput {
                field: "ordered_task_ids",
                message: "task order must not be empty".to_owned(),
            });
        }
        if ordered_ids
            .iter()
            .enumerate()
            .any(|(index, id)| ordered_ids[..index].contains(id))
        {
            return Err(TaskError::InvalidInput {
                field: "ordered_task_ids",
                message: "task order must not contain duplicate ids".to_owned(),
            });
        }

        let query = TaskQuery::for_project_or_inbox(project_id.clone());
        let active = self.query_tasks(query.clone())?;
        let Some(first) = active.iter().find(|task| task.id == ordered_ids[0]) else {
            return Err(TaskError::InvalidInput {
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
            return Err(TaskError::InvalidInput {
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
            .collect::<Result<Vec<_>, TaskError>>()?;
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
            self.events.tasks_changed(project_id.clone(), None);
        }
        self.query_tasks(query)
    }

    pub fn move_task(
        &self,
        task_id: &TaskId,
        request: DesktopTaskMove,
    ) -> Result<ProductTask, TaskError> {
        let current = self.get_task(task_id)?;
        if current.archived {
            return Err(TaskError::InvalidInput {
                field: "task_id",
                message: "archived tasks cannot be moved".to_owned(),
            });
        }
        if let Some(project_id) = &request.target_project_id {
            let project = self.get_project(project_id)?;
            if project.archive == ProjectArchiveState::Archived {
                return Err(TaskError::InvalidInput {
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
        let result = self.authority.client()?.move_task(
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
            self.events.tasks_changed(source_project_id.clone(), Some(moved.id.clone()));
            if moved.project_id != source_project_id {
                self.events.tasks_changed(moved.project_id.clone(), Some(moved.id.clone()));
            }
        }
        Ok(moved)
    }

    fn validate_task_parent(
        &self,
        task_id: &TaskId,
        target_project_id: Option<&ProjectId>,
        target_parent_id: Option<&TaskId>,
    ) -> Result<(), TaskError> {
        let mut cursor = target_parent_id.cloned();
        let mut visited = Vec::new();
        while let Some(parent_id) = cursor {
            if &parent_id == task_id || visited.contains(&parent_id) {
                return Err(TaskError::InvalidInput {
                    field: "target_parent_id",
                    message: "task parent would create a cycle".to_owned(),
                });
            }
            let parent = self.get_task(&parent_id)?;
            if parent.archived || parent.project_id.as_ref() != target_project_id {
                return Err(TaskError::InvalidInput {
                    field: "target_parent_id",
                    message: "task parent must be active in the target project".to_owned(),
                });
            }
            visited.push(parent_id);
            cursor = parent.parent_id;
        }
        Ok(())
    }

    /// Conversations attached to `task_id`, ordered by id.
    pub fn task_conversations(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<ProductConversation>, TaskError> {
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

    pub fn ensure_task_conversation(
        &self,
        task: &ProductTask,
        title: &str,
    ) -> Result<(), TaskError> {
        let conversation_id = ConversationId::new(format!("conversation:{}", task.id.as_str()))?;
        let conversation = ProductConversation::new(
            conversation_id,
            task.project_id.clone(),
            Some(task.id.clone()),
            title,
        )?;
        let key = format!("desktop:create-task-conversation:{}", task.id.as_str());
        self.authority.client()?.create_product_entity(
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

/// Idempotency-keyed metadata for a product create command.
pub fn create_meta(key: &str) -> Result<ProductCommandMeta, TaskError> {
    Ok(ProductCommandMeta::create(key, IdempotencyKey::new(key)?)?)
}

/// Idempotency-keyed metadata for a product update command at `revision`.
pub fn update_meta(
    kind: &str,
    id: &str,
    revision: ProductRevision,
) -> Result<ProductCommandMeta, TaskError> {
    let key = format!("desktop:update-{kind}:{id}:revision:{}", revision.get());
    Ok(ProductCommandMeta::update(
        key.clone(),
        IdempotencyKey::new(key)?,
        ExpectedRevision::new(revision.get())?,
    )?)
}

fn validate_required_text(field: &'static str, value: &str) -> Result<(), TaskError> {
    if value.trim().is_empty() {
        return Err(TaskError::InvalidInput {
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

fn project_entity(entity: ProductEntity) -> Result<Project, TaskError> {
    match entity {
        ProductEntity::Project(project) => Ok(project),
        _ => Err(TaskError::InvalidInput {
            field: "entity",
            message: "product command returned a non-project entity".to_owned(),
        }),
    }
}

fn task_entity(entity: ProductEntity) -> Result<ProductTask, TaskError> {
    match entity {
        ProductEntity::Task(task) => Ok(task),
        _ => Err(TaskError::InvalidInput {
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


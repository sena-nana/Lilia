use lilia_contracts::{ProductError, ProductTask, Project, ProjectArchiveState, ProjectId, TaskId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    document_resource_key, path_from_document_resource_key, DesktopApplication,
    DesktopApplicationError, DocumentId, DocumentSnapshot, WorkspaceItemId,
};

pub const TASK_WORKSPACE_ITEM_KIND: &str = "task";
pub const ROADMAP_WORKSPACE_ITEM_KIND: &str = "project-roadmap";
pub const MEMORY_WORKSPACE_ITEM_KIND: &str = "project-memory";
pub const ARCHITECTURE_WORKSPACE_ITEM_KIND: &str = "project-architecture";
pub const PROJECT_FILES_WORKSPACE_ITEM_KIND: &str = "project-files";
pub const DOCUMENT_WORKSPACE_ITEM_KIND: &str = "document-editor";
pub const TERMINAL_WORKSPACE_ITEM_KIND: &str = "terminal";
pub const AUTOMATION_WORKSPACE_ITEM_KIND: &str = "automation-workspace";
pub const SETTINGS_WORKSPACE_ITEM_KIND: &str = "settings-workspace";
pub const PROJECTS_WORKSPACE_ITEM_KIND: &str = "projects-workspace";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationWorkspaceSurface {
    Projects,
    Automations,
    Settings,
}

impl ApplicationWorkspaceSurface {
    pub const ALL: [Self; 3] = [Self::Projects, Self::Automations, Self::Settings];

    pub const fn kind(self) -> &'static str {
        match self {
            Self::Projects => PROJECTS_WORKSPACE_ITEM_KIND,
            Self::Automations => AUTOMATION_WORKSPACE_ITEM_KIND,
            Self::Settings => SETTINGS_WORKSPACE_ITEM_KIND,
        }
    }

    const fn resource_id(self) -> &'static str {
        match self {
            Self::Projects => "application:projects",
            Self::Automations => "application:automations",
            Self::Settings => "application:settings",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Projects => "项目",
            Self::Automations => "自动化",
            Self::Settings => "设置",
        }
    }

    const fn icon(self) -> &'static str {
        match self {
            Self::Projects => "workspace",
            Self::Automations => "automation",
            Self::Settings => "settings",
        }
    }

    const fn focus_target(self) -> &'static str {
        match self {
            Self::Projects => "projects-overview",
            Self::Automations => "automation-canvas",
            Self::Settings => "settings-content",
        }
    }

    fn from_kind(kind: &WorkspaceItemKind) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|surface| surface.kind() == kind.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectWorkspaceSurface {
    Roadmap,
    Memory,
    Architecture,
    Files,
}

impl ProjectWorkspaceSurface {
    pub const ALL: [Self; 4] = [Self::Roadmap, Self::Memory, Self::Architecture, Self::Files];

    pub const fn kind(self) -> &'static str {
        match self {
            Self::Roadmap => ROADMAP_WORKSPACE_ITEM_KIND,
            Self::Memory => MEMORY_WORKSPACE_ITEM_KIND,
            Self::Architecture => ARCHITECTURE_WORKSPACE_ITEM_KIND,
            Self::Files => PROJECT_FILES_WORKSPACE_ITEM_KIND,
        }
    }

    const fn resource_prefix(self) -> &'static str {
        match self {
            Self::Roadmap => "project-roadmap:",
            Self::Memory => "project-memory:",
            Self::Architecture => "project-architecture:",
            Self::Files => "project-files:",
        }
    }

    const fn title_suffix(self) -> &'static str {
        match self {
            Self::Roadmap => "路线图",
            Self::Memory => "记忆",
            Self::Architecture => "架构",
            Self::Files => "文件",
        }
    }

    const fn icon(self) -> &'static str {
        match self {
            Self::Roadmap => "roadmap",
            Self::Memory => "memory",
            Self::Architecture => "architecture",
            Self::Files => "folder",
        }
    }

    const fn focus_target(self) -> &'static str {
        match self {
            Self::Roadmap => "roadmap",
            Self::Memory => "memory-search",
            Self::Architecture => "architecture-canvas",
            Self::Files => "project-files",
        }
    }

    fn from_kind(kind: &WorkspaceItemKind) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|surface| surface.kind() == kind.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceResourceId(String);

impl WorkspaceResourceId {
    pub fn new(value: impl Into<String>) -> Result<Self, WorkspaceItemError> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err(WorkspaceItemError::InvalidResourceId);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceItemKind(String);

impl WorkspaceItemKind {
    pub fn new(value: impl Into<String>) -> Result<Self, WorkspaceItemError> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err(WorkspaceItemError::InvalidKind);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceFocusTarget(String);

impl WorkspaceFocusTarget {
    pub fn new(value: impl Into<String>) -> Result<Self, WorkspaceItemError> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err(WorkspaceItemError::InvalidFocusTarget);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceItemCapabilities {
    pub closable: bool,
    pub splittable: bool,
    pub movable_across_windows: bool,
    pub persistent: bool,
}

impl WorkspaceItemCapabilities {
    pub const fn dockable() -> Self {
        Self {
            closable: true,
            splittable: true,
            movable_across_windows: true,
            persistent: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceItem {
    pub id: WorkspaceItemId,
    pub resource_id: WorkspaceResourceId,
    pub kind: WorkspaceItemKind,
    pub title: String,
    pub icon: Option<String>,
    pub focus_target: WorkspaceFocusTarget,
    pub capabilities: WorkspaceItemCapabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serialized_state: Option<Value>,
}

impl WorkspaceItem {
    pub fn new(
        id: WorkspaceItemId,
        resource_id: WorkspaceResourceId,
        kind: WorkspaceItemKind,
        title: impl Into<String>,
        focus_target: WorkspaceFocusTarget,
        capabilities: WorkspaceItemCapabilities,
    ) -> Result<Self, WorkspaceItemError> {
        let title = title.into();
        if title.trim().is_empty() || title.chars().any(char::is_control) {
            return Err(WorkspaceItemError::InvalidTitle);
        }
        Ok(Self {
            id,
            resource_id,
            kind,
            title,
            icon: None,
            focus_target,
            capabilities,
            serialized_state: None,
        })
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Result<Self, WorkspaceItemError> {
        let icon = icon.into();
        if icon.trim().is_empty() || icon.chars().any(char::is_control) {
            return Err(WorkspaceItemError::InvalidIcon);
        }
        self.icon = Some(icon);
        Ok(self)
    }

    pub fn with_serialized_state(mut self, state: Option<Value>) -> Self {
        self.serialized_state = state;
        self
    }

    pub fn restoration(&self) -> Option<WorkspaceItemRestoration> {
        self.capabilities
            .persistent
            .then(|| WorkspaceItemRestoration {
                id: self.id.clone(),
                resource_id: Some(self.resource_id.clone()),
                kind: self.kind.clone(),
                serialized_state: self.serialized_state.clone(),
            })
    }

    pub fn task_id(&self) -> Result<Option<TaskId>, WorkspaceItemError> {
        if self.kind.as_str() != TASK_WORKSPACE_ITEM_KIND {
            return Ok(None);
        }
        task_id_from_resource_identity(&self.resource_id).map(Some)
    }

    pub fn project_surface(
        &self,
    ) -> Result<Option<(ProjectId, ProjectWorkspaceSurface)>, WorkspaceItemError> {
        let Some(surface) = ProjectWorkspaceSurface::from_kind(&self.kind) else {
            return Ok(None);
        };
        project_id_from_resource_identity(&self.resource_id, surface)
            .map(|project_id| Some((project_id, surface)))
    }

    pub fn application_surface(
        &self,
    ) -> Result<Option<ApplicationWorkspaceSurface>, WorkspaceItemError> {
        let Some(surface) = ApplicationWorkspaceSurface::from_kind(&self.kind) else {
            return Ok(None);
        };
        if self.resource_id.as_str() != surface.resource_id() {
            return Err(WorkspaceItemError::InvalidRestorationIdentity {
                item_id: self.resource_id.as_str().to_owned(),
                kind: surface.kind().to_owned(),
            });
        }
        Ok(Some(surface))
    }

    pub fn document_path(&self) -> Result<Option<std::path::PathBuf>, WorkspaceItemError> {
        if self.kind.as_str() != DOCUMENT_WORKSPACE_ITEM_KIND {
            return Ok(None);
        }
        path_from_document_resource_key(self.resource_id.as_str())
            .map(Some)
            .map_err(|_| WorkspaceItemError::InvalidRestorationIdentity {
                item_id: self.resource_id.as_str().to_owned(),
                kind: DOCUMENT_WORKSPACE_ITEM_KIND.to_owned(),
            })
    }

    pub fn terminal_session_id(
        &self,
    ) -> Result<Option<crate::DesktopTerminalSessionId>, WorkspaceItemError> {
        if self.kind.as_str() != TERMINAL_WORKSPACE_ITEM_KIND {
            return Ok(None);
        }
        terminal_session_id_from_resource_identity(&self.resource_id).map(Some)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceItemRestoration {
    pub id: WorkspaceItemId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<WorkspaceResourceId>,
    pub kind: WorkspaceItemKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serialized_state: Option<Value>,
}

impl WorkspaceItemRestoration {
    pub fn from_legacy_id(id: WorkspaceItemId) -> Option<Self> {
        let is_legacy_task = id
            .as_str()
            .strip_prefix("task:")
            .filter(|value| !value.is_empty())
            .is_some();
        is_legacy_task.then(|| Self {
            id,
            resource_id: None,
            kind: WorkspaceItemKind(TASK_WORKSPACE_ITEM_KIND.to_owned()),
            serialized_state: None,
        })
    }
}

impl DesktopApplication {
    pub fn task_workspace_item(
        &self,
        task_id: &TaskId,
    ) -> Result<WorkspaceItem, DesktopApplicationError> {
        let task = self.get_task(task_id)?;
        if task.archived {
            return Err(DesktopApplicationError::InvalidInput {
                field: "taskId",
                message: format!("task `{}` is archived", task_id.as_str()),
            });
        }
        task_item(task).map_err(DesktopApplicationError::from)
    }

    pub fn task_workspace_item_view(
        &self,
        task_id: &TaskId,
        instance_id: WorkspaceItemId,
    ) -> Result<WorkspaceItem, DesktopApplicationError> {
        let task = self.get_task(task_id)?;
        if task.archived {
            return Err(DesktopApplicationError::InvalidInput {
                field: "taskId",
                message: format!("task `{}` is archived", task_id.as_str()),
            });
        }
        task_item_with_instance_id(task, instance_id).map_err(DesktopApplicationError::from)
    }

    pub fn project_workspace_item(
        &self,
        project_id: &ProjectId,
        surface: ProjectWorkspaceSurface,
    ) -> Result<WorkspaceItem, DesktopApplicationError> {
        let project = self.get_project(project_id)?;
        if project.archive == ProjectArchiveState::Archived {
            return Err(DesktopApplicationError::InvalidInput {
                field: "projectId",
                message: format!("project `{}` is archived", project_id.as_str()),
            });
        }
        project_item(project, surface).map_err(DesktopApplicationError::from)
    }

    pub fn project_workspace_item_view(
        &self,
        project_id: &ProjectId,
        surface: ProjectWorkspaceSurface,
        instance_id: WorkspaceItemId,
    ) -> Result<WorkspaceItem, DesktopApplicationError> {
        let project = self.get_project(project_id)?;
        if project.archive == ProjectArchiveState::Archived {
            return Err(DesktopApplicationError::InvalidInput {
                field: "projectId",
                message: format!("project `{}` is archived", project_id.as_str()),
            });
        }
        project_item_with_instance_id(project, instance_id, surface)
            .map_err(DesktopApplicationError::from)
    }

    pub fn application_workspace_item(
        &self,
        surface: ApplicationWorkspaceSurface,
    ) -> Result<WorkspaceItem, DesktopApplicationError> {
        application_item(surface).map_err(DesktopApplicationError::from)
    }

    pub fn document_workspace_item(
        &self,
        document_id: DocumentId,
    ) -> Result<WorkspaceItem, DesktopApplicationError> {
        let snapshot = self.document_snapshot(document_id)?;
        document_item(&snapshot).map_err(DesktopApplicationError::from)
    }

    pub fn document_workspace_item_view(
        &self,
        document_id: DocumentId,
        instance_id: WorkspaceItemId,
    ) -> Result<WorkspaceItem, DesktopApplicationError> {
        let snapshot = self.document_snapshot(document_id)?;
        document_item_with_instance_id(&snapshot, instance_id)
            .map_err(DesktopApplicationError::from)
    }

    pub fn terminal_workspace_item(
        &self,
        snapshot: &crate::DesktopTerminalSnapshot,
    ) -> Result<WorkspaceItem, DesktopApplicationError> {
        terminal_item(snapshot).map_err(DesktopApplicationError::from)
    }

    pub fn open_project_document_workspace_item(
        &self,
        project_id: &ProjectId,
        relative_path: &str,
    ) -> Result<(DocumentSnapshot, WorkspaceItem), DesktopApplicationError> {
        let snapshot = self.open_project_file(project_id, relative_path)?;
        let item = self.document_workspace_item(snapshot.id)?;
        Ok((snapshot, item))
    }

    pub(crate) fn restore_workspace_item(
        &self,
        restoration: &WorkspaceItemRestoration,
    ) -> Result<Option<WorkspaceItem>, DesktopApplicationError> {
        if restoration.kind.as_str() == DOCUMENT_WORKSPACE_ITEM_KIND {
            let resource_id = restoration
                .resource_id
                .clone()
                .unwrap_or_else(|| WorkspaceResourceId(restoration.id.as_str().to_owned()));
            let path = match path_from_document_resource_key(resource_id.as_str()) {
                Ok(path) => path,
                Err(_) => {
                    return Err(WorkspaceItemError::InvalidRestorationIdentity {
                        item_id: resource_id.as_str().to_owned(),
                        kind: DOCUMENT_WORKSPACE_ITEM_KIND.to_owned(),
                    }
                    .into());
                }
            };
            let (snapshot, _) = match self.open_document_at_path(&path) {
                Ok(opened) => opened,
                Err(DesktopApplicationError::Document(crate::DocumentError::Io { .. }))
                | Err(DesktopApplicationError::Document(crate::DocumentError::NotAFile(_))) => {
                    return Ok(None);
                }
                Err(error) => return Err(error),
            };
            return document_item_with_instance_id(&snapshot, restoration.id.clone())
                .map(|item| item.with_serialized_state(restoration.serialized_state.clone()))
                .map(Some)
                .map_err(DesktopApplicationError::from);
        }

        if restoration.kind.as_str() == TERMINAL_WORKSPACE_ITEM_KIND {
            let resource_id = restoration
                .resource_id
                .clone()
                .unwrap_or_else(|| WorkspaceResourceId(restoration.id.as_str().to_owned()));
            let session_id = terminal_session_id_from_resource_identity(&resource_id)?;
            let state = restoration.serialized_state.clone().ok_or_else(|| {
                WorkspaceItemError::InvalidRestorationIdentity {
                    item_id: resource_id.as_str().to_owned(),
                    kind: TERMINAL_WORKSPACE_ITEM_KIND.to_owned(),
                }
            })?;
            let terminal: crate::DesktopTerminalRestoration = serde_json::from_value(state)
                .map_err(|_| WorkspaceItemError::InvalidRestorationIdentity {
                    item_id: resource_id.as_str().to_owned(),
                    kind: TERMINAL_WORKSPACE_ITEM_KIND.to_owned(),
                })?;
            if terminal.id != session_id {
                return Err(WorkspaceItemError::InvalidRestorationIdentity {
                    item_id: resource_id.as_str().to_owned(),
                    kind: TERMINAL_WORKSPACE_ITEM_KIND.to_owned(),
                }
                .into());
            }
            return terminal_item_with_instance_id(&terminal.snapshot(), restoration.id.clone())
                .map(Some)
                .map_err(DesktopApplicationError::from);
        }

        if restoration.kind.as_str() == TASK_WORKSPACE_ITEM_KIND {
            let resource_id = restoration
                .resource_id
                .clone()
                .unwrap_or_else(|| WorkspaceResourceId(restoration.id.as_str().to_owned()));
            let task_id = task_id_from_resource_identity(&resource_id)?;
            let task = match self.get_task(&task_id) {
                Ok(task) => task,
                Err(DesktopApplicationError::Product(ProductError::NotFound { .. })) => {
                    return Ok(None);
                }
                Err(error) => return Err(error),
            };
            if task.archived {
                return Ok(None);
            }
            return task_item_with_instance_id(task, restoration.id.clone())
                .map(|item| item.with_serialized_state(restoration.serialized_state.clone()))
                .map(Some)
                .map_err(DesktopApplicationError::from);
        }

        if let Some(surface) = ApplicationWorkspaceSurface::from_kind(&restoration.kind) {
            let resource_id = restoration
                .resource_id
                .clone()
                .unwrap_or_else(|| WorkspaceResourceId(restoration.id.as_str().to_owned()));
            if resource_id.as_str() != surface.resource_id() {
                return Err(WorkspaceItemError::InvalidRestorationIdentity {
                    item_id: resource_id.as_str().to_owned(),
                    kind: surface.kind().to_owned(),
                }
                .into());
            }
            return application_item_with_instance_id(surface, restoration.id.clone())
                .map(|item| item.with_serialized_state(restoration.serialized_state.clone()))
                .map(Some)
                .map_err(DesktopApplicationError::from);
        }

        let Some(surface) = ProjectWorkspaceSurface::from_kind(&restoration.kind) else {
            return Ok(None);
        };
        let resource_id = restoration
            .resource_id
            .clone()
            .unwrap_or_else(|| WorkspaceResourceId(restoration.id.as_str().to_owned()));
        let project_id = project_id_from_resource_identity(&resource_id, surface)?;
        let project = match self.get_project(&project_id) {
            Ok(project) => project,
            Err(DesktopApplicationError::Product(ProductError::NotFound { .. })) => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        if project.archive == ProjectArchiveState::Archived {
            return Ok(None);
        }
        project_item_with_instance_id(project, restoration.id.clone(), surface)
            .map(|item| item.with_serialized_state(restoration.serialized_state.clone()))
            .map(Some)
            .map_err(DesktopApplicationError::from)
    }
}

pub(crate) fn has_workspace_item_restorer(kind: &WorkspaceItemKind) -> bool {
    kind.as_str() == TASK_WORKSPACE_ITEM_KIND
        || kind.as_str() == DOCUMENT_WORKSPACE_ITEM_KIND
        || kind.as_str() == TERMINAL_WORKSPACE_ITEM_KIND
        || ProjectWorkspaceSurface::from_kind(kind).is_some()
        || ApplicationWorkspaceSurface::from_kind(kind).is_some()
}

fn terminal_session_id_from_resource_identity(
    id: &WorkspaceResourceId,
) -> Result<crate::DesktopTerminalSessionId, WorkspaceItemError> {
    let Some(session_id) = id
        .as_str()
        .strip_prefix("terminal:")
        .filter(|value| !value.is_empty())
    else {
        return Err(WorkspaceItemError::InvalidRestorationIdentity {
            item_id: id.as_str().to_owned(),
            kind: TERMINAL_WORKSPACE_ITEM_KIND.to_owned(),
        });
    };
    serde_json::from_value(serde_json::Value::String(session_id.to_owned())).map_err(|_| {
        WorkspaceItemError::InvalidRestorationIdentity {
            item_id: id.as_str().to_owned(),
            kind: TERMINAL_WORKSPACE_ITEM_KIND.to_owned(),
        }
    })
}

fn task_id_from_resource_identity(id: &WorkspaceResourceId) -> Result<TaskId, WorkspaceItemError> {
    let Some(task_id) = id.as_str().strip_prefix("task:") else {
        return Err(WorkspaceItemError::InvalidRestorationIdentity {
            item_id: id.as_str().to_owned(),
            kind: TASK_WORKSPACE_ITEM_KIND.to_owned(),
        });
    };
    TaskId::new(task_id.to_owned()).map_err(|_| WorkspaceItemError::InvalidRestorationIdentity {
        item_id: id.as_str().to_owned(),
        kind: TASK_WORKSPACE_ITEM_KIND.to_owned(),
    })
}

fn project_id_from_resource_identity(
    id: &WorkspaceResourceId,
    surface: ProjectWorkspaceSurface,
) -> Result<ProjectId, WorkspaceItemError> {
    let Some(project_id) = id
        .as_str()
        .strip_prefix(surface.resource_prefix())
        .filter(|value| !value.is_empty())
    else {
        return Err(WorkspaceItemError::InvalidRestorationIdentity {
            item_id: id.as_str().to_owned(),
            kind: surface.kind().to_owned(),
        });
    };
    ProjectId::new(project_id.to_owned()).map_err(|_| {
        WorkspaceItemError::InvalidRestorationIdentity {
            item_id: id.as_str().to_owned(),
            kind: surface.kind().to_owned(),
        }
    })
}

fn task_item(task: ProductTask) -> Result<WorkspaceItem, WorkspaceItemError> {
    let instance_id = WorkspaceItemId::new(format!("task:{}", task.id.as_str())).map_err(|_| {
        WorkspaceItemError::InvalidRestorationIdentity {
            item_id: task.id.as_str().to_owned(),
            kind: TASK_WORKSPACE_ITEM_KIND.to_owned(),
        }
    })?;
    task_item_with_instance_id(task, instance_id)
}

fn task_item_with_instance_id(
    task: ProductTask,
    instance_id: WorkspaceItemId,
) -> Result<WorkspaceItem, WorkspaceItemError> {
    WorkspaceItem::new(
        instance_id,
        WorkspaceResourceId::new(format!("task:{}", task.id.as_str()))?,
        WorkspaceItemKind::new(TASK_WORKSPACE_ITEM_KIND)?,
        task.title,
        WorkspaceFocusTarget::new("composer")?,
        WorkspaceItemCapabilities::dockable(),
    )?
    .with_icon("task")
}

fn project_item(
    project: Project,
    surface: ProjectWorkspaceSurface,
) -> Result<WorkspaceItem, WorkspaceItemError> {
    let instance_id = WorkspaceItemId::new(format!(
        "{}{}",
        surface.resource_prefix(),
        project.id.as_str()
    ))
    .map_err(|_| WorkspaceItemError::InvalidRestorationIdentity {
        item_id: project.id.as_str().to_owned(),
        kind: surface.kind().to_owned(),
    })?;
    project_item_with_instance_id(project, instance_id, surface)
}

fn project_item_with_instance_id(
    project: Project,
    instance_id: WorkspaceItemId,
    surface: ProjectWorkspaceSurface,
) -> Result<WorkspaceItem, WorkspaceItemError> {
    WorkspaceItem::new(
        instance_id,
        WorkspaceResourceId::new(format!(
            "{}{}",
            surface.resource_prefix(),
            project.id.as_str()
        ))?,
        WorkspaceItemKind::new(surface.kind())?,
        format!("{} · {}", project.name, surface.title_suffix()),
        WorkspaceFocusTarget::new(surface.focus_target())?,
        WorkspaceItemCapabilities {
            closable: true,
            splittable: true,
            movable_across_windows: true,
            persistent: true,
        },
    )?
    .with_icon(surface.icon())
}

fn application_item(
    surface: ApplicationWorkspaceSurface,
) -> Result<WorkspaceItem, WorkspaceItemError> {
    let instance_id = WorkspaceItemId::new(surface.resource_id()).map_err(|_| {
        WorkspaceItemError::InvalidRestorationIdentity {
            item_id: surface.resource_id().to_owned(),
            kind: surface.kind().to_owned(),
        }
    })?;
    application_item_with_instance_id(surface, instance_id)
}

fn application_item_with_instance_id(
    surface: ApplicationWorkspaceSurface,
    instance_id: WorkspaceItemId,
) -> Result<WorkspaceItem, WorkspaceItemError> {
    WorkspaceItem::new(
        instance_id,
        WorkspaceResourceId::new(surface.resource_id())?,
        WorkspaceItemKind::new(surface.kind())?,
        surface.title(),
        WorkspaceFocusTarget::new(surface.focus_target())?,
        WorkspaceItemCapabilities {
            closable: true,
            splittable: true,
            movable_across_windows: true,
            persistent: true,
        },
    )?
    .with_icon(surface.icon())
}

fn document_item(snapshot: &DocumentSnapshot) -> Result<WorkspaceItem, WorkspaceItemError> {
    let resource = document_resource_key(&snapshot.canonical_path).map_err(|_| {
        WorkspaceItemError::InvalidRestorationIdentity {
            item_id: snapshot.canonical_path.display().to_string(),
            kind: DOCUMENT_WORKSPACE_ITEM_KIND.to_owned(),
        }
    })?;
    let instance_id = WorkspaceItemId::new(resource.clone()).map_err(|_| {
        WorkspaceItemError::InvalidRestorationIdentity {
            item_id: resource.clone(),
            kind: DOCUMENT_WORKSPACE_ITEM_KIND.to_owned(),
        }
    })?;
    document_item_with_resource(snapshot, instance_id, resource)
}

fn document_item_with_instance_id(
    snapshot: &DocumentSnapshot,
    instance_id: WorkspaceItemId,
) -> Result<WorkspaceItem, WorkspaceItemError> {
    let resource = document_resource_key(&snapshot.canonical_path).map_err(|_| {
        WorkspaceItemError::InvalidRestorationIdentity {
            item_id: snapshot.canonical_path.display().to_string(),
            kind: DOCUMENT_WORKSPACE_ITEM_KIND.to_owned(),
        }
    })?;
    document_item_with_resource(snapshot, instance_id, resource)
}

fn document_item_with_resource(
    snapshot: &DocumentSnapshot,
    instance_id: WorkspaceItemId,
    resource: String,
) -> Result<WorkspaceItem, WorkspaceItemError> {
    let title = snapshot
        .canonical_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("未命名文档")
        .to_owned();
    WorkspaceItem::new(
        instance_id,
        WorkspaceResourceId::new(resource)?,
        WorkspaceItemKind::new(DOCUMENT_WORKSPACE_ITEM_KIND)?,
        title,
        WorkspaceFocusTarget::new("editor")?,
        WorkspaceItemCapabilities {
            closable: true,
            splittable: true,
            movable_across_windows: true,
            persistent: true,
        },
    )?
    .with_icon("document")
}

fn terminal_item(
    snapshot: &crate::DesktopTerminalSnapshot,
) -> Result<WorkspaceItem, WorkspaceItemError> {
    let instance_id =
        WorkspaceItemId::new(format!("terminal:{}", snapshot.id.as_str())).map_err(|_| {
            WorkspaceItemError::InvalidRestorationIdentity {
                item_id: snapshot.id.as_str().to_owned(),
                kind: TERMINAL_WORKSPACE_ITEM_KIND.to_owned(),
            }
        })?;
    terminal_item_with_instance_id(snapshot, instance_id)
}

fn terminal_item_with_instance_id(
    snapshot: &crate::DesktopTerminalSnapshot,
    instance_id: WorkspaceItemId,
) -> Result<WorkspaceItem, WorkspaceItemError> {
    let title = snapshot
        .cwd
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map_or_else(|| "终端".to_owned(), |name| format!("终端 · {name}"));
    WorkspaceItem::new(
        instance_id,
        WorkspaceResourceId::new(format!("terminal:{}", snapshot.id.as_str()))?,
        WorkspaceItemKind::new(TERMINAL_WORKSPACE_ITEM_KIND)?,
        title,
        WorkspaceFocusTarget::new("terminal-input")?,
        WorkspaceItemCapabilities::dockable(),
    )?
    .with_icon("terminal")
    .map(|item| {
        item.with_serialized_state(
            serde_json::to_value(crate::DesktopTerminalRestoration::from_snapshot(snapshot)).ok(),
        )
    })
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WorkspaceItemError {
    #[error("workspace resource id must not be empty or contain control characters")]
    InvalidResourceId,
    #[error("workspace item kind must not be empty or contain control characters")]
    InvalidKind,
    #[error("workspace item title must not be empty or contain control characters")]
    InvalidTitle,
    #[error("workspace item icon must not be empty or contain control characters")]
    InvalidIcon,
    #[error("workspace focus target must not be empty or contain control characters")]
    InvalidFocusTarget,
    #[error("workspace item `{item_id}` is not a valid `{kind}` restoration identity")]
    InvalidRestorationIdentity { item_id: String, kind: String },
    #[error("workspace item `{item_id}` changed kind from `{existing}` to `{requested}`")]
    KindMismatch {
        item_id: String,
        existing: String,
        requested: String,
    },
    #[error("workspace item `{item_id}` changed resource from `{existing}` to `{requested}`")]
    ResourceMismatch {
        item_id: String,
        existing: String,
        requested: String,
    },
    #[error("workspace item `{0}` is not registered in this workspace session")]
    UnknownItem(String),
    #[error("workspace item `{0}` cannot be closed")]
    NotClosable(String),
    #[error("workspace item `{0}` cannot be split")]
    NotSplittable(String),
    #[error("workspace item `{0}` cannot move across windows")]
    NotMovableAcrossWindows(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restoration_does_not_duplicate_presentation_or_product_facts() {
        let item = WorkspaceItem::new(
            WorkspaceItemId::new("task-view:one").unwrap(),
            WorkspaceResourceId::new("task:one").unwrap(),
            WorkspaceItemKind::new(TASK_WORKSPACE_ITEM_KIND).unwrap(),
            "Current title",
            WorkspaceFocusTarget::new("composer").unwrap(),
            WorkspaceItemCapabilities::dockable(),
        )
        .unwrap()
        .with_icon("task")
        .unwrap()
        .with_serialized_state(Some(serde_json::json!({ "scrollOffset": 12 })));

        assert_eq!(
            serde_json::to_value(item.restoration().unwrap()).unwrap(),
            serde_json::json!({
                "id": "task-view:one",
                "resourceId": "task:one",
                "kind": "task",
                "serializedState": { "scrollOffset": 12 }
            })
        );
    }

    #[test]
    fn transient_items_are_not_serialized_for_session_restore() {
        let item = WorkspaceItem::new(
            WorkspaceItemId::new("search:temporary").unwrap(),
            WorkspaceResourceId::new("search:query").unwrap(),
            WorkspaceItemKind::new("search").unwrap(),
            "Search",
            WorkspaceFocusTarget::new("query").unwrap(),
            WorkspaceItemCapabilities {
                closable: true,
                splittable: true,
                movable_across_windows: true,
                persistent: false,
            },
        )
        .unwrap();

        assert_eq!(item.restoration(), None);
    }

    #[test]
    fn project_surfaces_keep_product_identity_out_of_serialized_view_state() {
        let project = Project::new(ProjectId::new("native-project").unwrap(), "Native").unwrap();
        for surface in ProjectWorkspaceSurface::ALL {
            let item = project_item(project.clone(), surface).unwrap();
            assert_eq!(item.kind.as_str(), surface.kind());
            assert_eq!(
                item.project_surface().unwrap(),
                Some((project.id.clone(), surface))
            );
            assert!(item.capabilities.closable);
            assert!(item.capabilities.splittable);
            assert!(item.capabilities.movable_across_windows);
            assert!(item.capabilities.persistent);
            assert_eq!(item.serialized_state, None);
        }
    }

    #[test]
    fn project_surface_rejects_a_kind_resource_mismatch() {
        let item = WorkspaceItem::new(
            WorkspaceItemId::new("project-roadmap:view").unwrap(),
            WorkspaceResourceId::new("project-memory:native-project").unwrap(),
            WorkspaceItemKind::new(ROADMAP_WORKSPACE_ITEM_KIND).unwrap(),
            "Native · 路线图",
            WorkspaceFocusTarget::new("roadmap").unwrap(),
            WorkspaceItemCapabilities::dockable(),
        )
        .unwrap();

        assert!(matches!(
            item.project_surface(),
            Err(WorkspaceItemError::InvalidRestorationIdentity { kind, .. })
                if kind == ROADMAP_WORKSPACE_ITEM_KIND
        ));
    }

    #[test]
    fn application_surface_has_stable_identity_and_cross_window_capabilities() {
        for (surface, resource_id) in [
            (
                ApplicationWorkspaceSurface::Projects,
                "application:projects",
            ),
            (
                ApplicationWorkspaceSurface::Automations,
                "application:automations",
            ),
            (
                ApplicationWorkspaceSurface::Settings,
                "application:settings",
            ),
        ] {
            let item = application_item(surface).unwrap();
            assert_eq!(item.id.as_str(), resource_id);
            assert_eq!(item.resource_id.as_str(), resource_id);
            assert_eq!(item.kind.as_str(), surface.kind());
            assert_eq!(item.application_surface().unwrap(), Some(surface));
            assert!(item.capabilities.closable);
            assert!(item.capabilities.splittable);
            assert!(item.capabilities.movable_across_windows);
            assert!(item.capabilities.persistent);
        }
    }

    #[test]
    fn application_surface_rejects_a_kind_resource_mismatch() {
        let item = WorkspaceItem::new(
            WorkspaceItemId::new("application:automations").unwrap(),
            WorkspaceResourceId::new("application:settings").unwrap(),
            WorkspaceItemKind::new(AUTOMATION_WORKSPACE_ITEM_KIND).unwrap(),
            "自动化",
            WorkspaceFocusTarget::new("automation-canvas").unwrap(),
            WorkspaceItemCapabilities::dockable(),
        )
        .unwrap();

        assert!(matches!(
            item.application_surface(),
            Err(WorkspaceItemError::InvalidRestorationIdentity { kind, .. })
                if kind == AUTOMATION_WORKSPACE_ITEM_KIND
        ));
    }

    #[test]
    fn document_editor_items_share_canonical_resource_identity() {
        let snapshot = DocumentSnapshot {
            id: DocumentId::new(3),
            canonical_path: std::env::current_dir().unwrap().join("src/lib.rs"),
            language: None,
            read_only: false,
            buffer: crate::BufferSnapshot {
                id: crate::BufferId::new(1),
                text: "fn ok() {}".into(),
                revision: crate::BufferRevision::INITIAL,
                saved_revision: crate::BufferRevision::INITIAL,
            },
            disk_fingerprint: 1,
        };
        let first = document_item(&snapshot).unwrap();
        let second = document_item_with_instance_id(
            &snapshot,
            WorkspaceItemId::new("document-view:lib:second").unwrap(),
        )
        .unwrap();
        assert_eq!(first.kind.as_str(), DOCUMENT_WORKSPACE_ITEM_KIND);
        assert_eq!(first.resource_id, second.resource_id);
        assert_ne!(first.id, second.id);
        assert_eq!(
            first.document_path().unwrap(),
            Some(snapshot.canonical_path.clone())
        );
        assert_eq!(first.serialized_state, None);
        assert!(first.capabilities.persistent);
    }
}

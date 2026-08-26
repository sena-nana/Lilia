use lilia_contracts::{ProjectId, TaskId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::WorkspaceItemId;

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

    pub const fn resource_id(self) -> &'static str {
        match self {
            Self::Projects => "application:projects",
            Self::Automations => "application:automations",
            Self::Settings => "application:settings",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Projects => "项目",
            Self::Automations => "自动化",
            Self::Settings => "设置",
        }
    }

    pub const fn icon(self) -> &'static str {
        match self {
            Self::Projects => "workspace",
            Self::Automations => "automation",
            Self::Settings => "settings",
        }
    }

    pub const fn focus_target(self) -> &'static str {
        match self {
            Self::Projects => "projects-overview",
            Self::Automations => "automation-canvas",
            Self::Settings => "settings-content",
        }
    }

    pub fn from_kind(kind: &WorkspaceItemKind) -> Option<Self> {
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

    pub const fn resource_prefix(self) -> &'static str {
        match self {
            Self::Roadmap => "project-roadmap:",
            Self::Memory => "project-memory:",
            Self::Architecture => "project-architecture:",
            Self::Files => "project-files:",
        }
    }

    pub const fn title_suffix(self) -> &'static str {
        match self {
            Self::Roadmap => "路线图",
            Self::Memory => "记忆",
            Self::Architecture => "架构",
            Self::Files => "文件",
        }
    }

    pub const fn icon(self) -> &'static str {
        match self {
            Self::Roadmap => "roadmap",
            Self::Memory => "memory",
            Self::Architecture => "architecture",
            Self::Files => "folder",
        }
    }

    pub const fn focus_target(self) -> &'static str {
        match self {
            Self::Roadmap => "roadmap",
            Self::Memory => "memory-search",
            Self::Architecture => "architecture-canvas",
            Self::Files => "project-files",
        }
    }

    pub fn from_kind(kind: &WorkspaceItemKind) -> Option<Self> {
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

pub fn has_workspace_item_restorer(kind: &WorkspaceItemKind) -> bool {
    kind.as_str() == TASK_WORKSPACE_ITEM_KIND
        || kind.as_str() == DOCUMENT_WORKSPACE_ITEM_KIND
        || kind.as_str() == TERMINAL_WORKSPACE_ITEM_KIND
        || ProjectWorkspaceSurface::from_kind(kind).is_some()
        || ApplicationWorkspaceSurface::from_kind(kind).is_some()
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

}

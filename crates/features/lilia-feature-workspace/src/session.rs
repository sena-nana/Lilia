use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use lilia_contracts::{ProductTaskPriority, ProductTaskStatus, ProjectId, TaskId};
use serde::{Deserialize, Serialize};

use crate::{
    has_workspace_item_restorer, DesktopCommand, DesktopCommandOutcome, PanelLayoutError,
    PanelLayoutSnapshot, WorkspaceItem, WorkspaceItemError, WorkspaceItemId,
    WorkspaceItemRestoration,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorkspaceProject {
    pub id: ProjectId,
    pub name: String,
    pub workspace_path: Option<String>,
    pub pinned: bool,
    pub sort_order: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorkspaceTask {
    pub id: TaskId,
    pub title: String,
    pub parent_id: Option<TaskId>,
    pub status: ProductTaskStatus,
    pub priority: ProductTaskPriority,
    pub pinned: bool,
    pub sort_order: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorkspaceSnapshot {
    pub revision: u64,
    pub projects: Vec<DesktopWorkspaceProject>,
    pub tasks: Vec<DesktopWorkspaceTask>,
    pub selected_project: Option<ProjectId>,
    pub inbox_selected: bool,
    pub selected_task: Option<TaskId>,
    pub workspace_items: Vec<WorkspaceItem>,
    pub panel_layout: PanelLayoutSnapshot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorkspaceTransferOutcome {
    pub source: DesktopWorkspaceSnapshot,
    pub target: DesktopWorkspaceSnapshot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorkspaceSessionState {
    pub schema_version: u32,
    #[serde(default)]
    pub revision: u64,
    pub selected_project: Option<ProjectId>,
    #[serde(default)]
    pub inbox_selected: bool,
    pub selected_task: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspace_items: Vec<WorkspaceItemRestoration>,
    pub panel_layout: PanelLayoutSnapshot,
}

impl Default for DesktopWorkspaceSessionState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            selected_project: None,
            inbox_selected: false,
            selected_task: None,
            workspace_items: Vec::new(),
            panel_layout: PanelLayoutSnapshot::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DesktopWorkspaceSessionId(String);

impl DesktopWorkspaceSessionId {
    pub fn new(value: impl Into<String>) -> Result<Self, DesktopWorkspaceSessionIdError> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err(DesktopWorkspaceSessionIdError::InvalidIdentifier);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone)]
pub struct DesktopWorkspaceSession {
    id: DesktopWorkspaceSessionId,
    catalog: std::sync::Arc<dyn WorkspaceCatalog>,
    state: Arc<Mutex<DesktopWorkspaceState>>,
}

/// Product catalog and item restoration the session does not own.
pub trait WorkspaceCatalog: Send + Sync + 'static {
    fn list_projects(&self) -> Result<Vec<DesktopWorkspaceProject>, WorkspaceSessionError>;
    fn list_project_tasks(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<DesktopWorkspaceTask>, WorkspaceSessionError>;
    fn list_inbox_tasks(&self) -> Result<Vec<DesktopWorkspaceTask>, WorkspaceSessionError>;
    fn restore_item(
        &self,
        restoration: &WorkspaceItemRestoration,
    ) -> Result<Option<WorkspaceItem>, WorkspaceSessionError>;
    fn lookup_task(&self, task_id: &TaskId) -> Result<WorkspaceTaskRef, WorkspaceSessionError>;
    fn host_ptr(&self) -> usize;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceTaskRef {
    pub archived: bool,
    pub project_id: Option<ProjectId>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WorkspaceSessionError {
    #[error("invalid workspace input `{field}`: {message}")]
    InvalidInput {
        field: &'static str,
        message: String,
    },
    #[error("workspace {0} state is unavailable")]
    StateUnavailable(&'static str),
    #[error("workspace {0} state revision overflowed")]
    StateRevisionOverflow(&'static str),
    #[error("{0}")]
    Catalog(String),
    #[error(transparent)]
    SessionId(#[from] DesktopWorkspaceSessionIdError),
    #[error(transparent)]
    SessionState(#[from] DesktopWorkspaceSessionStateError),
    #[error(transparent)]
    Item(#[from] WorkspaceItemError),
    #[error(transparent)]
    Panel(#[from] PanelLayoutError),
}

impl DesktopWorkspaceSession {
    pub fn new(
        id: DesktopWorkspaceSessionId,
        catalog: Arc<dyn WorkspaceCatalog>,
        state: Arc<Mutex<DesktopWorkspaceState>>,
    ) -> Self {
        Self { id, catalog, state }
    }

    pub fn id(&self) -> &DesktopWorkspaceSessionId {
        &self.id
    }

    pub fn snapshot(&self) -> Result<DesktopWorkspaceSnapshot, WorkspaceSessionError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| WorkspaceSessionError::StateUnavailable("workspace session"))?
            .snapshot())
    }

    pub fn persisted_state(&self) -> Result<DesktopWorkspaceSessionState, WorkspaceSessionError> {
        let state = self
            .state
            .lock()
            .map_err(|_| WorkspaceSessionError::StateUnavailable("workspace session"))?;
        let workspace_items = state
            .workspace_items
            .values()
            .filter_map(WorkspaceItem::restoration)
            .collect::<Vec<_>>();
        let retained_items = workspace_items.iter().map(|item| item.id.clone()).collect();
        let mut panel_layout = state.panel_layout.clone();
        panel_layout.retain_items(&retained_items);
        Ok(DesktopWorkspaceSessionState {
            schema_version: 1,
            revision: state.revision,
            selected_project: state.selected_project.clone(),
            inbox_selected: state.inbox_selected,
            selected_task: state.selected_task.clone(),
            workspace_items,
            panel_layout,
        })
    }

    pub fn restore(
        &self,
        persisted: &DesktopWorkspaceSessionState,
    ) -> Result<DesktopCommandOutcome, WorkspaceSessionError> {
        if persisted.schema_version != 1 {
            return Err(DesktopWorkspaceSessionStateError::UnsupportedSchemaVersion(
                persisted.schema_version,
            )
            .into());
        }
        persisted.panel_layout.validate()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| WorkspaceSessionError::StateUnavailable("workspace session"))?;
        let mut next = DesktopWorkspaceState {
            panel_layout: persisted.panel_layout.clone(),
            ..DesktopWorkspaceState::default()
        };
        next.reload(
            self.catalog.as_ref(),
            persisted.selected_project.clone(),
            persisted.inbox_selected,
            true,
            false,
        )?;
        next.panel_layout = persisted.panel_layout.clone();
        next.selected_task = persisted
            .selected_task
            .clone()
            .filter(|task_id| next.tasks.iter().any(|task| &task.id == task_id));
        let layout_item_ids = persisted
            .panel_layout
            .item_ids()
            .into_iter()
            .collect::<BTreeSet<_>>();
        let legacy_items_migrated =
            persisted.workspace_items.is_empty() && !layout_item_ids.is_empty();
        let restorations = if persisted.workspace_items.is_empty() {
            layout_item_ids
                .iter()
                .cloned()
                .filter_map(WorkspaceItemRestoration::from_legacy_id)
                .collect::<Vec<_>>()
        } else {
            persisted.workspace_items.clone()
        };
        let mut restored_ids = BTreeSet::new();
        let mut resource_identities_migrated = false;
        for restoration in restorations {
            resource_identities_migrated |= restoration.resource_id.is_none();
            if !restored_ids.insert(restoration.id.clone()) {
                return Err(DesktopWorkspaceSessionStateError::DuplicateWorkspaceItem(
                    restoration.id.as_str().to_owned(),
                )
                .into());
            }
            if !layout_item_ids.contains(&restoration.id) {
                continue;
            }
            if let Some(item) = self.catalog.restore_item(&restoration)? {
                next.workspace_items.insert(item.id.clone(), item);
            }
        }
        let retained_items = next.workspace_items.keys().cloned().collect();
        next.panel_layout.retain_items(&retained_items);
        next.sync_selection(self.catalog.as_ref())?;
        let references_sanitized = next.selected_project != persisted.selected_project
            || next.inbox_selected != persisted.inbox_selected
            || next.selected_task != persisted.selected_task
            || next.panel_layout != persisted.panel_layout
            || legacy_items_migrated
            || resource_identities_migrated;
        next.revision = if references_sanitized {
            persisted.revision.checked_add(1).ok_or(
                WorkspaceSessionError::StateRevisionOverflow("workspace session restore"),
            )?
        } else {
            persisted.revision
        };
        let changed = !state.same_content(&next);
        if changed || state.revision != next.revision {
            *state = next;
        }
        Ok(DesktopCommandOutcome {
            changed,
            workspace: state.snapshot(),
        })
    }

    pub fn transfer_item_to(
        &self,
        target: &DesktopWorkspaceSession,
        item_id: &WorkspaceItemId,
        target_pane_id: &crate::PaneId,
        before: Option<&WorkspaceItemId>,
    ) -> Result<DesktopWorkspaceTransferOutcome, WorkspaceSessionError> {
        if Arc::ptr_eq(&self.state, &target.state) || self.id == target.id {
            return Err(DesktopWorkspaceSessionStateError::TransferRequiresDistinctSessions.into());
        }
        if self.catalog.host_ptr() != target.catalog.host_ptr() {
            return Err(DesktopWorkspaceSessionStateError::TransferApplicationMismatch.into());
        }

        let source_key = Arc::as_ptr(&self.state) as usize;
        let target_key = Arc::as_ptr(&target.state) as usize;
        if source_key < target_key {
            let mut source_state = self.state.lock().map_err(|_| {
                WorkspaceSessionError::StateUnavailable("source workspace session")
            })?;
            let mut target_state = target.state.lock().map_err(|_| {
                WorkspaceSessionError::StateUnavailable("target workspace session")
            })?;
            transfer_workspace_item_locked(
                self.catalog.as_ref(),
                &mut source_state,
                &mut target_state,
                item_id,
                target_pane_id,
                before,
            )
        } else {
            let mut target_state = target.state.lock().map_err(|_| {
                WorkspaceSessionError::StateUnavailable("target workspace session")
            })?;
            let mut source_state = self.state.lock().map_err(|_| {
                WorkspaceSessionError::StateUnavailable("source workspace session")
            })?;
            transfer_workspace_item_locked(
                self.catalog.as_ref(),
                &mut source_state,
                &mut target_state,
                item_id,
                target_pane_id,
                before,
            )
        }
    }

    pub fn execute(
        &self,
        command: DesktopCommand,
    ) -> Result<DesktopCommandOutcome, WorkspaceSessionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WorkspaceSessionError::StateUnavailable("workspace session"))?;
        let mut next = state.clone();
        match command {
            DesktopCommand::RefreshWorkspace => {
                let inbox_selected = next.inbox_selected;
                next.reload(self.catalog.as_ref(), None, inbox_selected, false, false)?;
                next.sync_selection(self.catalog.as_ref())?;
            }
            DesktopCommand::SelectProject(project_id) => {
                next.reload(self.catalog.as_ref(), Some(project_id), false, true, true)?;
                next.panel_layout.deactivate_active_item()?;
            }
            DesktopCommand::SelectInbox => {
                next.reload(self.catalog.as_ref(), None, true, true, false)?;
                next.panel_layout.deactivate_active_item()?;
            }
            DesktopCommand::SelectTask(task_id) => {
                let selected_project = next.selected_project.clone();
                let inbox_selected = next.inbox_selected;
                next.reload(self.catalog.as_ref(), selected_project, inbox_selected, false, false)?;
                if !next.tasks.iter().any(|task| task.id == task_id) {
                    return Err(WorkspaceSessionError::InvalidInput {
                        field: "taskId",
                        message: format!(
                            "task `{}` is not part of the selected project",
                            task_id.as_str()
                        ),
                    });
                }
                let item_id = next.workspace_items.values().find_map(|item| {
                    (item.task_id().ok().flatten().as_ref() == Some(&task_id))
                        .then(|| item.id.clone())
                });
                if let Some((item_id, pane_id)) = item_id.and_then(|item_id| {
                    next.panel_layout
                        .pane_for_item(&item_id)
                        .cloned()
                        .map(|pane_id| (item_id, pane_id))
                }) {
                    next.panel_layout.activate_item(&pane_id, &item_id)?;
                } else {
                    next.panel_layout.deactivate_active_item()?;
                }
                next.selected_task = Some(task_id);
            }
            DesktopCommand::BackToTaskList => {
                next.selected_task = None;
                next.panel_layout.deactivate_active_item()?;
            }
            DesktopCommand::ReplacePanelLayout(layout) => {
                layout.validate()?;
                if let Some(unknown) = layout
                    .item_ids()
                    .into_iter()
                    .find(|item_id| !next.workspace_items.contains_key(item_id))
                {
                    return Err(WorkspaceItemError::UnknownItem(unknown.as_str().to_owned()).into());
                }
                next.panel_layout = layout;
                next.sync_selection(self.catalog.as_ref())?;
            }
            DesktopCommand::ActivatePanel(panel_id) => {
                next.panel_layout.activate_panel(&panel_id)?;
            }
            DesktopCommand::SetPanelVisible { panel_id, visible } => {
                next.panel_layout.set_panel_visible(&panel_id, visible)?;
            }
            DesktopCommand::ResizePanel { panel_id, extent } => {
                next.panel_layout.resize_panel(&panel_id, extent)?;
            }
            DesktopCommand::OpenWorkspaceItem { pane_id, mut item } => {
                if let Some(existing) = next.workspace_items.get(&item.id) {
                    if existing.kind != item.kind {
                        return Err(WorkspaceItemError::KindMismatch {
                            item_id: item.id.as_str().to_owned(),
                            existing: existing.kind.as_str().to_owned(),
                            requested: item.kind.as_str().to_owned(),
                        }
                        .into());
                    }
                    if existing.resource_id != item.resource_id {
                        return Err(WorkspaceItemError::ResourceMismatch {
                            item_id: item.id.as_str().to_owned(),
                            existing: existing.resource_id.as_str().to_owned(),
                            requested: item.resource_id.as_str().to_owned(),
                        }
                        .into());
                    }
                    if item.serialized_state.is_none() {
                        item.serialized_state = existing.serialized_state.clone();
                    }
                }
                next.panel_layout.open_item(&pane_id, item.id.clone())?;
                next.workspace_items.insert(item.id.clone(), item);
                next.sync_selection(self.catalog.as_ref())?;
            }
            DesktopCommand::ActivateWorkspaceItem { pane_id, item_id } => {
                if !next.workspace_items.contains_key(&item_id) {
                    return Err(WorkspaceItemError::UnknownItem(item_id.as_str().to_owned()).into());
                }
                next.panel_layout.activate_item(&pane_id, &item_id)?;
                next.sync_selection(self.catalog.as_ref())?;
            }
            DesktopCommand::FocusPane(pane_id) => {
                next.panel_layout.focus_pane(&pane_id)?;
                next.sync_selection(self.catalog.as_ref())?;
            }
            DesktopCommand::MoveWorkspaceItem {
                item_id,
                target_pane_id,
                before,
            } => {
                let item = next
                    .workspace_items
                    .get(&item_id)
                    .ok_or_else(|| WorkspaceItemError::UnknownItem(item_id.as_str().to_owned()))?;
                let source_pane_id = next
                    .panel_layout
                    .pane_for_item(&item_id)
                    .ok_or_else(|| WorkspaceItemError::UnknownItem(item_id.as_str().to_owned()))?;
                if source_pane_id != &target_pane_id && !item.capabilities.splittable {
                    return Err(
                        WorkspaceItemError::NotSplittable(item_id.as_str().to_owned()).into(),
                    );
                }
                next.panel_layout
                    .move_item(&item_id, &target_pane_id, before.as_ref())?;
                next.sync_selection(self.catalog.as_ref())?;
            }
            DesktopCommand::CloseWorkspaceItem { pane_id, item_id } => {
                let item = next
                    .workspace_items
                    .get(&item_id)
                    .ok_or_else(|| WorkspaceItemError::UnknownItem(item_id.as_str().to_owned()))?;
                if !item.capabilities.closable {
                    return Err(WorkspaceItemError::NotClosable(item_id.as_str().to_owned()).into());
                }
                next.panel_layout.close_item(&pane_id, &item_id)?;
                if !next.panel_layout.contains_item(&item_id) {
                    next.workspace_items.remove(&item_id);
                }
                next.sync_selection(self.catalog.as_ref())?;
            }
            DesktopCommand::UpdateWorkspaceItemState {
                item_id,
                serialized_state,
            } => {
                let item = next
                    .workspace_items
                    .get_mut(&item_id)
                    .ok_or_else(|| WorkspaceItemError::UnknownItem(item_id.as_str().to_owned()))?;
                item.serialized_state = serialized_state;
            }
            DesktopCommand::SplitPane {
                pane_id,
                new_pane_id,
                axis,
                ratio,
            } => {
                if let Some(item_id) = next.panel_layout.active_item(&pane_id)? {
                    let item = next.workspace_items.get(item_id).ok_or_else(|| {
                        WorkspaceItemError::UnknownItem(item_id.as_str().to_owned())
                    })?;
                    if !item.capabilities.splittable {
                        return Err(
                            WorkspaceItemError::NotSplittable(item_id.as_str().to_owned()).into(),
                        );
                    }
                }
                next.panel_layout
                    .split_pane(&pane_id, new_pane_id, axis, ratio)?;
            }
            DesktopCommand::ResizePaneSplit {
                first_pane_id,
                second_pane_id,
                ratio,
            } => {
                next.panel_layout
                    .resize_split(&first_pane_id, &second_pane_id, ratio)?;
            }
            DesktopCommand::ClosePane { pane_id } => {
                next.panel_layout.close_empty_pane(&pane_id)?;
                next.sync_selection(self.catalog.as_ref())?;
            }
        }

        let changed = !state.same_content(&next);
        if changed {
            next.revision = state.revision.checked_add(1).ok_or(
                WorkspaceSessionError::StateRevisionOverflow("workspace session"),
            )?;
            *state = next;
        }
        Ok(DesktopCommandOutcome {
            changed,
            workspace: state.snapshot(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DesktopWorkspaceSessionIdError {
    #[error("workspace session identifier must not be empty or contain control characters")]
    InvalidIdentifier,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DesktopWorkspaceSessionStateError {
    #[error("unsupported workspace session state schema version {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("workspace item `{0}` has more than one restoration record")]
    DuplicateWorkspaceItem(String),
    #[error("workspace item transfer requires two distinct workspace sessions")]
    TransferRequiresDistinctSessions,
    #[error("workspace item transfer requires sessions owned by the same desktop application")]
    TransferApplicationMismatch,
    #[error("workspace item `{0}` is already open in the target workspace session")]
    TransferTargetContainsItem(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesktopWorkspaceState {
    revision: u64,
    projects: Vec<DesktopWorkspaceProject>,
    tasks: Vec<DesktopWorkspaceTask>,
    selected_project: Option<ProjectId>,
    inbox_selected: bool,
    selected_task: Option<TaskId>,
    workspace_items: BTreeMap<WorkspaceItemId, WorkspaceItem>,
    panel_layout: PanelLayoutSnapshot,
}

impl DesktopWorkspaceState {
    fn snapshot(&self) -> DesktopWorkspaceSnapshot {
        DesktopWorkspaceSnapshot {
            revision: self.revision,
            projects: self.projects.clone(),
            tasks: self.tasks.clone(),
            selected_project: self.selected_project.clone(),
            inbox_selected: self.inbox_selected,
            selected_task: self.selected_task.clone(),
            workspace_items: self.workspace_items.values().cloned().collect(),
            panel_layout: self.panel_layout.clone(),
        }
    }

    fn same_content(&self, other: &Self) -> bool {
        self.projects == other.projects
            && self.tasks == other.tasks
            && self.selected_project == other.selected_project
            && self.inbox_selected == other.inbox_selected
            && self.selected_task == other.selected_task
            && self.workspace_items == other.workspace_items
            && self.panel_layout == other.panel_layout
    }
}

fn transfer_workspace_item_locked(
    catalog: &dyn WorkspaceCatalog,
    source: &mut DesktopWorkspaceState,
    target: &mut DesktopWorkspaceState,
    item_id: &WorkspaceItemId,
    target_pane_id: &crate::PaneId,
    before: Option<&WorkspaceItemId>,
) -> Result<DesktopWorkspaceTransferOutcome, WorkspaceSessionError> {
    let item = source
        .workspace_items
        .get(item_id)
        .cloned()
        .ok_or_else(|| WorkspaceItemError::UnknownItem(item_id.as_str().to_owned()))?;
    if !item.capabilities.movable_across_windows {
        return Err(
            WorkspaceItemError::NotMovableAcrossWindows(item_id.as_str().to_owned()).into(),
        );
    }
    if target.workspace_items.contains_key(item_id) || target.panel_layout.contains_item(item_id) {
        return Err(
            DesktopWorkspaceSessionStateError::TransferTargetContainsItem(
                item_id.as_str().to_owned(),
            )
            .into(),
        );
    }
    let source_pane_id = source
        .panel_layout
        .pane_for_item(item_id)
        .cloned()
        .ok_or_else(|| WorkspaceItemError::UnknownItem(item_id.as_str().to_owned()))?;

    let mut next_source = source.clone();
    let mut next_target = target.clone();
    next_target
        .panel_layout
        .open_item(target_pane_id, item_id.clone())?;
    if before.is_some() {
        next_target
            .panel_layout
            .move_item(item_id, target_pane_id, before)?;
    }
    next_target.workspace_items.insert(item_id.clone(), item);
    next_source
        .panel_layout
        .close_item(&source_pane_id, item_id)?;
    if !next_source.panel_layout.contains_item(item_id) {
        next_source.workspace_items.remove(item_id);
    }

    next_source.sync_selection(catalog)?;
    next_target.sync_selection(catalog)?;
    next_source.revision =
        source
            .revision
            .checked_add(1)
            .ok_or(WorkspaceSessionError::StateRevisionOverflow(
                "source workspace session",
            ))?;
    next_target.revision =
        target
            .revision
            .checked_add(1)
            .ok_or(WorkspaceSessionError::StateRevisionOverflow(
                "target workspace session",
            ))?;
    *source = next_source;
    *target = next_target;
    Ok(DesktopWorkspaceTransferOutcome {
        source: source.snapshot(),
        target: target.snapshot(),
    })
}

impl DesktopWorkspaceState {
    pub fn reload(
        &mut self,
        catalog: &dyn WorkspaceCatalog,
        requested_project: Option<ProjectId>,
        requested_inbox: bool,
        clear_task: bool,
        require_requested_project: bool,
    ) -> Result<(), WorkspaceSessionError> {
        let mut projects = catalog.list_projects()?;
        projects.sort_by(|left, right| {
            right
                .pinned
                .cmp(&left.pinned)
                .then_with(|| left.sort_order.cmp(&right.sort_order))
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.as_str().cmp(right.id.as_str()))
        });

        let requested_project = if requested_inbox {
            None
        } else {
            requested_project.or_else(|| self.selected_project.clone())
        };
        let selected_project = if requested_inbox {
            None
        } else {
            match requested_project {
                Some(project_id) if projects.iter().any(|project| project.id == project_id) => {
                    Some(project_id)
                }
                Some(project_id) if require_requested_project => {
                    return Err(WorkspaceSessionError::InvalidInput {
                        field: "projectId",
                        message: format!(
                            "project `{}` is not available in this workspace",
                            project_id.as_str()
                        ),
                    });
                }
                Some(_) => projects.first().map(|project| project.id.clone()),
                None => projects.first().map(|project| project.id.clone()),
            }
        };
        let inbox_selected = requested_inbox || selected_project.is_none();

        let mut tasks = match (selected_project.clone(), inbox_selected) {
            (Some(project_id), _) => catalog.list_project_tasks(&project_id)?,
            (None, true) => catalog.list_inbox_tasks()?,
            (None, false) => Vec::new(),
        };
        tasks.sort_by(|left, right| {
            right
                .pinned
                .cmp(&left.pinned)
                .then_with(|| left.sort_order.cmp(&right.sort_order))
                .then_with(|| left.title.cmp(&right.title))
                .then_with(|| left.id.as_str().cmp(right.id.as_str()))
        });
        let selected_task = (!clear_task)
            .then(|| self.selected_task.clone())
            .flatten()
            .filter(|task_id| tasks.iter().any(|task| &task.id == task_id));

        self.projects = projects;
        self.tasks = tasks;
        self.selected_project = selected_project;
        self.inbox_selected = inbox_selected;
        self.selected_task = selected_task;
        self.refresh_items(catalog)?;
        Ok(())
    }

    pub fn sync_selection(
        &mut self,
        catalog: &dyn WorkspaceCatalog,
    ) -> Result<(), WorkspaceSessionError> {
        let active_item = self.panel_layout.active_workspace_item()?.cloned();
        let active_workspace_item = active_item
            .as_ref()
            .and_then(|item_id| self.workspace_items.get(item_id));
        let task_id = active_workspace_item
            .map(WorkspaceItem::task_id)
            .transpose()?
            .flatten();
        if let Some(task_id) = task_id {
            let task = catalog.lookup_task(&task_id)?;
            if task.archived {
                return Err(WorkspaceSessionError::InvalidInput {
                    field: "workspaceItemId",
                    message: format!(
                        "task workspace item `{}` references an archived task",
                        active_item
                            .as_ref()
                            .expect("task items always have an identity")
                            .as_str()
                    ),
                });
            }
            match task.project_id {
                Some(project_id) => {
                    self.reload(catalog, Some(project_id), false, true, true)?;
                }
                None => {
                    self.reload(catalog, None, true, true, false)?;
                }
            }
            self.selected_task = Some(task_id);
            return Ok(());
        }

        let project_surface = active_workspace_item
            .map(WorkspaceItem::project_surface)
            .transpose()?
            .flatten();
        if let Some((project_id, _)) = project_surface {
            self.reload(catalog, Some(project_id), false, true, true)?;
            self.selected_task = None;
            return Ok(());
        }

        self.selected_task = None;
        Ok(())
    }

    pub fn refresh_items(
        &mut self,
        catalog: &dyn WorkspaceCatalog,
    ) -> Result<(), WorkspaceSessionError> {
        let mut refreshed = BTreeMap::new();
        for item in self.workspace_items.values() {
            let restoration = WorkspaceItemRestoration {
                id: item.id.clone(),
                resource_id: Some(item.resource_id.clone()),
                kind: item.kind.clone(),
                serialized_state: item.serialized_state.clone(),
            };
            match catalog.restore_item(&restoration)? {
                Some(item) => {
                    refreshed.insert(item.id.clone(), item);
                }
                None if !has_workspace_item_restorer(&item.kind) => {
                    refreshed.insert(item.id.clone(), item.clone());
                }
                None => {}
            }
        }
        let retained = refreshed.keys().cloned().collect();
        self.panel_layout.retain_items(&retained);
        self.workspace_items = refreshed;
        Ok(())
    }
}

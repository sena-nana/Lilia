use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use lilia_contracts::{ProductTaskPriority, ProductTaskStatus, ProjectId, TaskId};
use serde::{Deserialize, Serialize};

use crate::workspace_item::has_workspace_item_restorer;
use crate::{
    DesktopApplication, DesktopApplicationError, DesktopCommand, DesktopCommandOutcome,
    PanelLayoutSnapshot, ProjectQuery, TaskQuery, WorkspaceItem, WorkspaceItemError,
    WorkspaceItemId, WorkspaceItemRestoration,
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
    application: DesktopApplication,
    state: Arc<Mutex<DesktopWorkspaceState>>,
}

impl DesktopWorkspaceSession {
    pub fn id(&self) -> &DesktopWorkspaceSessionId {
        &self.id
    }

    pub fn snapshot(&self) -> Result<DesktopWorkspaceSnapshot, DesktopApplicationError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("workspace session"))?
            .snapshot())
    }

    pub fn persisted_state(&self) -> Result<DesktopWorkspaceSessionState, DesktopApplicationError> {
        let state = self
            .state
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("workspace session"))?;
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
    ) -> Result<DesktopCommandOutcome, DesktopApplicationError> {
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
            .map_err(|_| DesktopApplicationError::StateUnavailable("workspace session"))?;
        let mut next = DesktopWorkspaceState {
            panel_layout: persisted.panel_layout.clone(),
            ..DesktopWorkspaceState::default()
        };
        self.application.reload_workspace(
            &mut next,
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
            if let Some(item) = self.application.restore_workspace_item(&restoration)? {
                next.workspace_items.insert(item.id.clone(), item);
            }
        }
        let retained_items = next.workspace_items.keys().cloned().collect();
        next.panel_layout.retain_items(&retained_items);
        self.application.sync_selection_to_active_item(&mut next)?;
        let references_sanitized = next.selected_project != persisted.selected_project
            || next.inbox_selected != persisted.inbox_selected
            || next.selected_task != persisted.selected_task
            || next.panel_layout != persisted.panel_layout
            || legacy_items_migrated
            || resource_identities_migrated;
        next.revision = if references_sanitized {
            persisted.revision.checked_add(1).ok_or(
                DesktopApplicationError::StateRevisionOverflow("workspace session restore"),
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
    ) -> Result<DesktopWorkspaceTransferOutcome, DesktopApplicationError> {
        if Arc::ptr_eq(&self.state, &target.state) || self.id == target.id {
            return Err(DesktopWorkspaceSessionStateError::TransferRequiresDistinctSessions.into());
        }
        if !Arc::ptr_eq(&self.application.inner, &target.application.inner) {
            return Err(DesktopWorkspaceSessionStateError::TransferApplicationMismatch.into());
        }

        let source_key = Arc::as_ptr(&self.state) as usize;
        let target_key = Arc::as_ptr(&target.state) as usize;
        if source_key < target_key {
            let mut source_state = self.state.lock().map_err(|_| {
                DesktopApplicationError::StateUnavailable("source workspace session")
            })?;
            let mut target_state = target.state.lock().map_err(|_| {
                DesktopApplicationError::StateUnavailable("target workspace session")
            })?;
            transfer_workspace_item_locked(
                &self.application,
                &mut source_state,
                &mut target_state,
                item_id,
                target_pane_id,
                before,
            )
        } else {
            let mut target_state = target.state.lock().map_err(|_| {
                DesktopApplicationError::StateUnavailable("target workspace session")
            })?;
            let mut source_state = self.state.lock().map_err(|_| {
                DesktopApplicationError::StateUnavailable("source workspace session")
            })?;
            transfer_workspace_item_locked(
                &self.application,
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
    ) -> Result<DesktopCommandOutcome, DesktopApplicationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("workspace session"))?;
        let mut next = state.clone();
        match command {
            DesktopCommand::RefreshWorkspace => {
                let inbox_selected = next.inbox_selected;
                self.application
                    .reload_workspace(&mut next, None, inbox_selected, false, false)?;
                self.application.sync_selection_to_active_item(&mut next)?;
            }
            DesktopCommand::SelectProject(project_id) => {
                self.application.reload_workspace(
                    &mut next,
                    Some(project_id),
                    false,
                    true,
                    true,
                )?;
                next.panel_layout.deactivate_active_item()?;
            }
            DesktopCommand::SelectInbox => {
                self.application
                    .reload_workspace(&mut next, None, true, true, false)?;
                next.panel_layout.deactivate_active_item()?;
            }
            DesktopCommand::SelectTask(task_id) => {
                let selected_project = next.selected_project.clone();
                let inbox_selected = next.inbox_selected;
                self.application.reload_workspace(
                    &mut next,
                    selected_project,
                    inbox_selected,
                    false,
                    false,
                )?;
                if !next.tasks.iter().any(|task| task.id == task_id) {
                    return Err(DesktopApplicationError::InvalidInput {
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
                self.application.sync_selection_to_active_item(&mut next)?;
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
                self.application.sync_selection_to_active_item(&mut next)?;
            }
            DesktopCommand::ActivateWorkspaceItem { pane_id, item_id } => {
                if !next.workspace_items.contains_key(&item_id) {
                    return Err(WorkspaceItemError::UnknownItem(item_id.as_str().to_owned()).into());
                }
                next.panel_layout.activate_item(&pane_id, &item_id)?;
                self.application.sync_selection_to_active_item(&mut next)?;
            }
            DesktopCommand::FocusPane(pane_id) => {
                next.panel_layout.focus_pane(&pane_id)?;
                self.application.sync_selection_to_active_item(&mut next)?;
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
                self.application.sync_selection_to_active_item(&mut next)?;
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
                self.application.sync_selection_to_active_item(&mut next)?;
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
                self.application.sync_selection_to_active_item(&mut next)?;
            }
        }

        if next.selected_task.is_some() && next.selected_task != state.selected_task {
            let task_inspector = crate::PanelId::new(crate::TASK_INSPECTOR_PANEL_ID)?;
            if next.panel_layout.panel(&task_inspector).is_some() {
                next.panel_layout.activate_panel(&task_inspector)?;
            }
        }

        let changed = !state.same_content(&next);
        if changed {
            next.revision = state.revision.checked_add(1).ok_or(
                DesktopApplicationError::StateRevisionOverflow("workspace session"),
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
pub(crate) struct DesktopWorkspaceState {
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
    application: &DesktopApplication,
    source: &mut DesktopWorkspaceState,
    target: &mut DesktopWorkspaceState,
    item_id: &WorkspaceItemId,
    target_pane_id: &crate::PaneId,
    before: Option<&WorkspaceItemId>,
) -> Result<DesktopWorkspaceTransferOutcome, DesktopApplicationError> {
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

    application.sync_selection_to_active_item(&mut next_source)?;
    application.sync_selection_to_active_item(&mut next_target)?;
    next_source.revision =
        source
            .revision
            .checked_add(1)
            .ok_or(DesktopApplicationError::StateRevisionOverflow(
                "source workspace session",
            ))?;
    next_target.revision =
        target
            .revision
            .checked_add(1)
            .ok_or(DesktopApplicationError::StateRevisionOverflow(
                "target workspace session",
            ))?;
    *source = next_source;
    *target = next_target;
    Ok(DesktopWorkspaceTransferOutcome {
        source: source.snapshot(),
        target: target.snapshot(),
    })
}

impl DesktopApplication {
    pub fn create_workspace_session(
        &self,
        id: DesktopWorkspaceSessionId,
    ) -> DesktopWorkspaceSession {
        DesktopWorkspaceSession {
            id,
            application: self.clone(),
            state: Arc::new(Mutex::new(DesktopWorkspaceState::default())),
        }
    }

    pub fn default_workspace_session(&self) -> DesktopWorkspaceSession {
        DesktopWorkspaceSession {
            id: DesktopWorkspaceSessionId("default".to_owned()),
            application: self.clone(),
            state: Arc::clone(&self.inner.workspace),
        }
    }

    pub fn workspace_snapshot(&self) -> Result<DesktopWorkspaceSnapshot, DesktopApplicationError> {
        self.default_workspace_session().snapshot()
    }

    pub fn execute_command(
        &self,
        command: DesktopCommand,
    ) -> Result<DesktopCommandOutcome, DesktopApplicationError> {
        self.default_workspace_session().execute(command)
    }

    fn reload_workspace(
        &self,
        state: &mut DesktopWorkspaceState,
        requested_project: Option<ProjectId>,
        requested_inbox: bool,
        clear_task: bool,
        require_requested_project: bool,
    ) -> Result<(), DesktopApplicationError> {
        let mut projects = self
            .query_projects(ProjectQuery::default())?
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

        let requested_project = if requested_inbox {
            None
        } else {
            requested_project.or_else(|| state.selected_project.clone())
        };
        let selected_project = if requested_inbox {
            None
        } else {
            match requested_project {
                Some(project_id) if projects.iter().any(|project| project.id == project_id) => {
                    Some(project_id)
                }
                Some(project_id) if require_requested_project => {
                    return Err(DesktopApplicationError::InvalidInput {
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
            (Some(project_id), _) => self
                .query_tasks(TaskQuery::for_project(project_id))?
                .into_iter()
                .map(|task| DesktopWorkspaceTask {
                    id: task.id,
                    title: task.title,
                    parent_id: task.parent_id,
                    status: task.status,
                    priority: task.priority,
                    pinned: task.pinned,
                    sort_order: task.sort_order,
                })
                .collect::<Vec<_>>(),
            (None, true) => self
                .query_tasks(TaskQuery::for_inbox())?
                .into_iter()
                .map(|task| DesktopWorkspaceTask {
                    id: task.id,
                    title: task.title,
                    parent_id: task.parent_id,
                    status: task.status,
                    priority: task.priority,
                    pinned: task.pinned,
                    sort_order: task.sort_order,
                })
                .collect::<Vec<_>>(),
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
            .then(|| state.selected_task.clone())
            .flatten()
            .filter(|task_id| tasks.iter().any(|task| &task.id == task_id));

        state.projects = projects;
        state.tasks = tasks;
        state.selected_project = selected_project;
        state.inbox_selected = inbox_selected;
        state.selected_task = selected_task;
        self.refresh_workspace_items(state)?;
        Ok(())
    }

    fn sync_selection_to_active_item(
        &self,
        state: &mut DesktopWorkspaceState,
    ) -> Result<(), DesktopApplicationError> {
        let active_item = state.panel_layout.active_workspace_item()?.cloned();
        let active_workspace_item = active_item
            .as_ref()
            .and_then(|item_id| state.workspace_items.get(item_id));
        let task_id = active_workspace_item
            .map(WorkspaceItem::task_id)
            .transpose()?
            .flatten();
        if let Some(task_id) = task_id {
            let task = self.get_task(&task_id)?;
            if task.archived {
                return Err(DesktopApplicationError::InvalidInput {
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
                    self.reload_workspace(state, Some(project_id), false, true, true)?;
                }
                None => {
                    self.reload_workspace(state, None, true, true, false)?;
                }
            }
            state.selected_task = Some(task_id);
            return Ok(());
        }

        let project_surface = active_workspace_item
            .map(WorkspaceItem::project_surface)
            .transpose()?
            .flatten();
        if let Some((project_id, _)) = project_surface {
            self.reload_workspace(state, Some(project_id), false, true, true)?;
            state.selected_task = None;
            return Ok(());
        }

        state.selected_task = None;
        Ok(())
    }

    fn refresh_workspace_items(
        &self,
        state: &mut DesktopWorkspaceState,
    ) -> Result<(), DesktopApplicationError> {
        let mut refreshed = BTreeMap::new();
        for item in state.workspace_items.values() {
            let restoration = WorkspaceItemRestoration {
                id: item.id.clone(),
                resource_id: Some(item.resource_id.clone()),
                kind: item.kind.clone(),
                serialized_state: item.serialized_state.clone(),
            };
            match self.restore_workspace_item(&restoration)? {
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
        state.panel_layout.retain_items(&retained);
        state.workspace_items = refreshed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use lilia_contracts::{ProductEntity, ProductTask, Project};
    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::{
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
                pane_id: crate::PaneId::new("primary").unwrap(),
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
                &crate::PaneId::new("primary").unwrap(),
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
                pane_id: crate::PaneId::new("primary").unwrap(),
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
                pane_id: crate::PaneId::new("primary").unwrap(),
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
                &crate::PaneId::new("primary").unwrap(),
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
                pane_id: crate::PaneId::new("primary").unwrap(),
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
        let pane_id = crate::PaneId::new("primary").unwrap();
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
        let primary = crate::PaneId::new("primary").unwrap();
        let secondary = crate::PaneId::new("secondary").unwrap();
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
                axis: crate::SplitAxis::Horizontal,
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
        let primary = crate::PaneId::new("primary").unwrap();
        let secondary = crate::PaneId::new("secondary").unwrap();
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
                axis: crate::SplitAxis::Horizontal,
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
        let tools = crate::PanelId::new(crate::CODING_TOOLS_PANEL_ID).unwrap();

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
                .active_panel(crate::DockSlot::Right)
                .map(|panel| panel.id.as_str()),
            Some(crate::CODING_TOOLS_PANEL_ID)
        );
        assert_eq!(
            first_snapshot.panel_layout.panel(&tools).unwrap().extent,
            418.0
        );
        assert!(second
            .snapshot()
            .unwrap()
            .panel_layout
            .active_panel(crate::DockSlot::Right)
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
            .active_panel(crate::DockSlot::Right)
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
                &crate::PaneId::new("primary").unwrap(),
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
                .map(crate::WorkspaceResourceId::as_str),
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
        assert_eq!(item.kind.as_str(), crate::TASK_WORKSPACE_ITEM_KIND);
        assert_eq!(item.resource_id.as_str(), "task:task-item");
        assert_eq!(item.focus_target.as_str(), "composer");
        assert_eq!(item.title, "Original title");
        assert_eq!(
            item.capabilities,
            crate::WorkspaceItemCapabilities::dockable()
        );
        let item_id = item.id.clone();
        first
            .execute(DesktopCommand::OpenWorkspaceItem {
                pane_id: crate::PaneId::new("primary").unwrap(),
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
            crate::DesktopTaskPatch {
                title: Some("Renamed in Product Core".to_owned()),
                ..crate::DesktopTaskPatch::default()
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
                .active_item(&crate::PaneId::new("primary").unwrap()),
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
        let pane_id = crate::PaneId::new("primary").unwrap();
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
        let primary = crate::PaneId::new("primary").unwrap();
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
                &crate::PaneId::new("missing").unwrap(),
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
            crate::WorkspaceResourceId::new("tool:locked").unwrap(),
            crate::WorkspaceItemKind::new("tool").unwrap(),
            "Locked tool",
            crate::WorkspaceFocusTarget::new("primary").unwrap(),
            crate::WorkspaceItemCapabilities {
                closable: false,
                splittable: false,
                movable_across_windows: false,
                persistent: false,
            },
        )
        .unwrap();
        let item_id = item.id.clone();
        let pane_id = crate::PaneId::new("primary").unwrap();
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
            Err(DesktopApplicationError::WorkspaceItem(
                WorkspaceItemError::NotClosable(_)
            ))
        ));
        assert!(matches!(
            session.execute(DesktopCommand::SplitPane {
                pane_id,
                new_pane_id: crate::PaneId::new("secondary").unwrap(),
                axis: crate::SplitAxis::Horizontal,
                ratio: 0.5,
            }),
            Err(DesktopApplicationError::WorkspaceItem(
                WorkspaceItemError::NotSplittable(_)
            ))
        ));

        let move_session = app.create_workspace_session(
            DesktopWorkspaceSessionId::new("window:locked-move").unwrap(),
        );
        let primary = crate::PaneId::new("primary").unwrap();
        let secondary = crate::PaneId::new("secondary").unwrap();
        move_session
            .execute(DesktopCommand::SplitPane {
                pane_id: primary.clone(),
                new_pane_id: secondary.clone(),
                axis: crate::SplitAxis::Horizontal,
                ratio: 0.5,
            })
            .unwrap();
        let locked_item = WorkspaceItem::new(
            WorkspaceItemId::new("tool:locked-move").unwrap(),
            crate::WorkspaceResourceId::new("tool:locked-move").unwrap(),
            crate::WorkspaceItemKind::new("tool").unwrap(),
            "Locked move tool",
            crate::WorkspaceFocusTarget::new("primary").unwrap(),
            crate::WorkspaceItemCapabilities {
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
            Err(DesktopApplicationError::WorkspaceItem(
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
                &crate::PaneId::new("primary").unwrap(),
                None,
            ),
            Err(DesktopApplicationError::WorkspaceItem(
                WorkspaceItemError::NotMovableAcrossWindows(_)
            ))
        ));
    }
}

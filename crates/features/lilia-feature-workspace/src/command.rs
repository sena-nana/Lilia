use lilia_contracts::{ProjectId, TaskId};

use crate::{PaneId, PanelId, PanelLayoutSnapshot, SplitAxis, WorkspaceItem, WorkspaceItemId};

#[derive(Clone, Debug, PartialEq)]
pub enum DesktopCommand {
    RefreshWorkspace,
    SelectProject(ProjectId),
    SelectInbox,
    SelectTask(TaskId),
    BackToTaskList,
    ReplacePanelLayout(PanelLayoutSnapshot),
    ActivatePanel(PanelId),
    SetPanelVisible { panel_id: PanelId, visible: bool },
    ResizePanel { panel_id: PanelId, extent: f32 },
    OpenWorkspaceItem {
        pane_id: PaneId,
        item: WorkspaceItem,
    },
    ActivateWorkspaceItem {
        pane_id: PaneId,
        item_id: WorkspaceItemId,
    },
    FocusPane(PaneId),
    MoveWorkspaceItem {
        item_id: WorkspaceItemId,
        target_pane_id: PaneId,
        before: Option<WorkspaceItemId>,
    },
    CloseWorkspaceItem {
        pane_id: PaneId,
        item_id: WorkspaceItemId,
    },
    UpdateWorkspaceItemState {
        item_id: WorkspaceItemId,
        serialized_state: Option<serde_json::Value>,
    },
    SplitPane {
        pane_id: PaneId,
        new_pane_id: PaneId,
        axis: SplitAxis,
        ratio: f32,
    },
    ResizePaneSplit {
        first_pane_id: PaneId,
        second_pane_id: PaneId,
        ratio: f32,
    },
    ClosePane { pane_id: PaneId },
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesktopCommandOutcome {
    pub changed: bool,
    pub workspace: crate::DesktopWorkspaceSnapshot,
}

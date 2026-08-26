//! Workspace domain feature.
//!
//! Owns the per-window session: project/task catalogs as the session sees them,
//! selection, pane layout and workspace items. The host supplies a
//! [`WorkspaceCatalog`] for product queries and item restoration.

mod command;
mod item;
mod panel;
mod session;

pub use command::{DesktopCommand, DesktopCommandOutcome};
pub use item::{
    has_workspace_item_restorer, ApplicationWorkspaceSurface, ProjectWorkspaceSurface,
    WorkspaceFocusTarget, WorkspaceItem, WorkspaceItemCapabilities, WorkspaceItemError,
    WorkspaceItemKind, WorkspaceItemRestoration, WorkspaceResourceId, ARCHITECTURE_WORKSPACE_ITEM_KIND,
    AUTOMATION_WORKSPACE_ITEM_KIND, DOCUMENT_WORKSPACE_ITEM_KIND, MEMORY_WORKSPACE_ITEM_KIND,
    PROJECT_FILES_WORKSPACE_ITEM_KIND, PROJECTS_WORKSPACE_ITEM_KIND, ROADMAP_WORKSPACE_ITEM_KIND,
    SETTINGS_WORKSPACE_ITEM_KIND, TASK_WORKSPACE_ITEM_KIND, TERMINAL_WORKSPACE_ITEM_KIND,
};
pub use panel::{
    default_panel_states, DockSlot, PaneId, PaneNode, PanelId, PanelLayoutError, PanelLayoutSnapshot,
    PanelState, SplitAxis, WorkspaceItemId, CODING_TOOLS_PANEL_ID, DIAGNOSTICS_PANEL_ID, IAB_PANEL_ID,
    RESOURCES_PANEL_ID, TASK_INSPECTOR_PANEL_ID,
};
pub use session::{
    DesktopWorkspaceProject, DesktopWorkspaceSession, DesktopWorkspaceSessionId,
    DesktopWorkspaceSessionIdError, DesktopWorkspaceSessionState, DesktopWorkspaceSessionStateError,
    DesktopWorkspaceSnapshot, DesktopWorkspaceState, DesktopWorkspaceTask,
    DesktopWorkspaceTransferOutcome, WorkspaceCatalog, WorkspaceSessionError, WorkspaceTaskRef,
};

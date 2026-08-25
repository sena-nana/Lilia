//! Service slots the shell owns rather than a feature crate.
//!
//! A UI module reaches shared shell facts through the kernel, not through a
//! handle threaded down from `DesktopProgram`. Publishing them as slots is what
//! lets a module be constructed knowing only `&Kernel` and its own window,
//! which in turn is what keeps its state private instead of living as another
//! field on the shell.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use lilia_kernel::{
    Feature, FeatureContext, FeatureId, Kernel, KernelError, ServiceKey, ServiceRef,
};
use nana_ui_platform::WindowId;

use crate::application::DesktopWorkspaceSession;

/// Service slot for every window's workspace session.
///
/// A session owns the projects, tasks, selection and pane layout its window
/// renders, and it is the only writer of them. Modules resolve their own rather
/// than mirroring rows into their fields.
pub enum WorkspaceSessionsKey {}

impl ServiceKey for WorkspaceSessionsKey {
    type Value = Arc<WorkspaceSessions>;

    const NAME: &'static str = "lilia.shell.workspace_sessions";
}

/// The sessions the process currently has open, by window.
///
/// Workspace windows each carry their own selection and pane layout, so a domain
/// is per-window rather than global; this is what lets a module read "my"
/// session instead of the shell swapping state in and out around it.
///
/// Interior mutability because windows open and close long after mount, and a
/// slot is provided exactly once. The alternative — re-providing on every window
/// change — is what `ServiceRegistry` refuses by design.
#[derive(Default)]
pub struct WorkspaceSessions {
    sessions: Mutex<HashMap<WindowId, DesktopWorkspaceSession>>,
}

impl WorkspaceSessions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or replaces a window's session.
    pub fn install(&self, window: WindowId, session: DesktopWorkspaceSession) {
        self.lock().insert(window, session);
    }

    /// Drops a closed window's session so a later window with a recycled id
    /// cannot inherit it.
    pub fn remove(&self, window: WindowId) {
        self.lock().remove(&window);
    }

    pub fn get(&self, window: WindowId) -> Option<DesktopWorkspaceSession> {
        self.lock().get(&window).cloned()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<WindowId, DesktopWorkspaceSession>> {
        self.sessions.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Publishes the sessions the kernel cannot build for itself.
///
/// The primary session is created during application bootstrap, before the
/// kernel exists, because restoring persisted panes has to happen while the
/// shell is still deciding what to show.
pub struct WorkspaceSessionFeature {
    sessions: Arc<WorkspaceSessions>,
}

impl WorkspaceSessionFeature {
    pub fn new(sessions: Arc<WorkspaceSessions>) -> Self {
        Self { sessions }
    }
}

impl Feature for WorkspaceSessionFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.shell.workspace")
            .expect("the workspace shell feature id is not blank")
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<WorkspaceSessionsKey>()]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        cx.provide::<WorkspaceSessionsKey>(Arc::clone(&self.sessions))
    }
}

/// The session registry every module and the shell share. An empty slot is a
/// composition bug rather than a runtime condition, so this panics instead of
/// letting a module silently fall back to its own copy.
pub fn workspace_sessions(kernel: &Kernel) -> Arc<WorkspaceSessions> {
    kernel
        .service::<WorkspaceSessionsKey>()
        .expect("the workspace sessions slot is filled while features mount")
}

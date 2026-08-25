//! Service slots the shell owns rather than a feature crate.
//!
//! A UI module reaches shared shell facts through the kernel, not through a
//! handle threaded down from `DesktopProgram`. Publishing them as slots is what
//! lets a module be constructed knowing only `&Kernel`, which in turn is what
//! keeps its state private instead of living as another field on the shell.

use lilia_kernel::{
    Feature, FeatureContext, FeatureId, Kernel, KernelError, ServiceKey, ServiceRef,
};

use crate::application::DesktopWorkspaceSession;

/// Service slot for the primary window's workspace session.
///
/// The session owns the projects, tasks, selection and pane layout every domain
/// reads, and it is the only writer of them. Modules resolve it instead of
/// mirroring its rows into their own fields.
pub enum WorkspaceSessionKey {}

impl ServiceKey for WorkspaceSessionKey {
    type Value = DesktopWorkspaceSession;

    const NAME: &'static str = "lilia.shell.workspace_session";
}

/// Publishes the shell-owned session the kernel cannot build for itself.
///
/// The session is created during application bootstrap, before the kernel
/// exists, because restoring persisted panes has to happen while the shell is
/// still deciding what to show.
pub struct WorkspaceSessionFeature {
    session: DesktopWorkspaceSession,
}

impl WorkspaceSessionFeature {
    pub fn new(session: DesktopWorkspaceSession) -> Self {
        Self { session }
    }
}

impl Feature for WorkspaceSessionFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.shell.workspace")
            .expect("the workspace shell feature id is not blank")
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<WorkspaceSessionKey>()]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        cx.provide::<WorkspaceSessionKey>(self.session.clone())
    }
}

/// The one session every module and the shell share. An empty slot is a
/// composition bug rather than a runtime condition, so this panics instead of
/// letting a module silently fall back to its own copy.
pub fn workspace_session(kernel: &Kernel) -> DesktopWorkspaceSession {
    kernel
        .service::<WorkspaceSessionKey>()
        .expect("the workspace session slot is filled while features mount")
}

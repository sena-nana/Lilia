//! Terminal domain feature.
//!
//! Owns PTY sessions: spawning them, parsing their screens, and their
//! lifecycle. Which directory a session starts in is a project or worktree
//! fact, so the caller resolves the working directory and passes it in.

mod session;

use std::sync::Arc;

use lilia_kernel::{Feature, FeatureContext, FeatureId, KernelError, ServiceKey, ServiceRef};

pub use session::{
    canonical_directory,
    DesktopTerminalColor, DesktopTerminalCommand, DesktopTerminalError, DesktopTerminalLaunch,
    DesktopTerminalProcessState, DesktopTerminalRestoration, DesktopTerminalRow,
    DesktopTerminalScope, DesktopTerminalService, DesktopTerminalSessionId,
    DesktopTerminalSnapshot, DesktopTerminalStyle, DesktopTerminalStyleSpan,
};

/// Where a session reports that its screen advanced.
///
/// Output arrives on the reader thread the PTY owns, so the host decides how
/// that reaches the UI.
pub trait TerminalEvents: Send + Sync + 'static {
    fn changed(&self, session_id: &DesktopTerminalSessionId, revision: u64);
}

/// Drops every terminal notification. Used where a caller drives sessions
/// directly and polls snapshots.
pub struct SilentTerminalEvents;

impl TerminalEvents for SilentTerminalEvents {
    fn changed(&self, _session_id: &DesktopTerminalSessionId, _revision: u64) {}
}

/// Service slot for [`DesktopTerminalService`].
pub enum TerminalServiceKey {}

impl ServiceKey for TerminalServiceKey {
    type Value = Arc<DesktopTerminalService>;

    const NAME: &'static str = "lilia.terminal";
}

pub struct TerminalFeature {
    service: Arc<DesktopTerminalService>,
}

impl TerminalFeature {
    pub fn new(service: Arc<DesktopTerminalService>) -> Self {
        Self { service }
    }
}

impl Feature for TerminalFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.terminal").expect("the terminal feature id is not blank")
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<TerminalServiceKey>()]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        cx.provide::<TerminalServiceKey>(self.service.clone())
    }
}

use crate::runtime_compat::HostedWindowId;

#[derive(Debug, Clone)]
pub struct IabPanelState {
    active_url: String,
}

impl Default for IabPanelState {
    fn default() -> Self {
        Self::new("about:blank")
    }
}

impl IabPanelState {
    pub fn new(initial_url: impl Into<String>) -> Self {
        Self {
            active_url: initial_url.into(),
        }
    }

    pub const fn browser_attached(&self) -> bool {
        false
    }

    pub const fn browser_ready(&self) -> bool {
        false
    }

    pub fn active_url(&self) -> &str {
        &self.active_url
    }

    pub fn error(&self) -> Option<&str> {
        None
    }

    pub fn set_panel_visible(&mut self, _visible: bool, _window_id: HostedWindowId) {}
}

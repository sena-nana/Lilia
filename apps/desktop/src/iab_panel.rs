use crate::runtime_compat::HostedWindowId;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct HostedBrowserId(pub u64);

pub const SIDEBAR_BROWSER_ID: HostedBrowserId = HostedBrowserId(1);

#[derive(Debug, Clone)]
pub struct IabPanelState {
    browser_id: HostedBrowserId,
    draft_url: String,
    active_url: String,
}

impl Default for IabPanelState {
    fn default() -> Self {
        Self::new(SIDEBAR_BROWSER_ID, "about:blank")
    }
}

impl IabPanelState {
    pub fn new(browser_id: HostedBrowserId, initial_url: impl Into<String>) -> Self {
        let initial_url = initial_url.into();
        Self {
            browser_id,
            draft_url: initial_url.clone(),
            active_url: initial_url,
        }
    }

    pub const fn browser_id(&self) -> HostedBrowserId {
        self.browser_id
    }

    pub const fn browser_attached(&self) -> bool {
        false
    }

    pub const fn browser_ready(&self) -> bool {
        false
    }

    pub fn draft_url(&self) -> &str {
        &self.draft_url
    }

    pub fn active_url(&self) -> &str {
        &self.active_url
    }

    pub fn error(&self) -> Option<&str> {
        None
    }

    pub fn set_panel_visible(&mut self, _visible: bool, _window_id: HostedWindowId) {}
}

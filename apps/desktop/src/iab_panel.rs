use crate::runtime_compat::HostedWindowId;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct HostedBrowserId(pub u64);

pub const SIDEBAR_BROWSER_ID: HostedBrowserId = HostedBrowserId(1);

#[derive(Debug, Clone, PartialEq)]
pub enum IabPanelMessage {
    DraftUrlChanged(String),
    Navigate,
    OpenWindow,
    Close,
}

#[derive(Debug, Clone)]
pub struct IabPanelState {
    browser_id: HostedBrowserId,
    draft_url: String,
    active_url: String,
    title: Option<String>,
    error: Option<String>,
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
            title: None,
            error: None,
        }
    }

    pub const fn browser_id(&self) -> HostedBrowserId {
        self.browser_id
    }

    pub fn browser_attached(&self) -> bool {
        false
    }

    pub fn browser_ready(&self) -> bool {
        false
    }

    pub fn draft_url(&self) -> &str {
        &self.draft_url
    }

    pub fn active_url(&self) -> &str {
        &self.active_url
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn update(&mut self, message: IabPanelMessage, _window_id: HostedWindowId) {
        match message {
            IabPanelMessage::DraftUrlChanged(value) => {
                self.draft_url = value;
                self.error = None;
            }
            IabPanelMessage::Navigate => {
                self.active_url = self.draft_url.clone();
            }
            IabPanelMessage::OpenWindow | IabPanelMessage::Close => {}
        }
    }

    pub fn set_panel_visible(&mut self, _visible: bool, _window_id: HostedWindowId) {}
}

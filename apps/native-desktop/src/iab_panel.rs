use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length, Padding};
use nana_ui::widgets::{button_style, canvas_style};
use nana_ui::{
    icon, ui_font, ButtonKind, ControlSize, HostedBrowserBounds, HostedBrowserCommand,
    HostedBrowserCommandKind, HostedBrowserEvent, HostedBrowserId, HostedBrowserLoadState,
    HostedWindowId, Icon, Input, LayoutBounds, LayoutProbe, ThemeTokens,
};
use url::Url;

pub const SIDEBAR_BROWSER_ID: HostedBrowserId = HostedBrowserId(1);
const DEFAULT_URL: &str = "about:blank";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum IabLoadState {
    #[default]
    Idle,
    Loading,
    Ready,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IabPanelMessage {
    DraftUrlChanged(String),
    Navigate,
    BoundsChanged(LayoutBounds),
    OpenWindow,
    Close,
}

#[derive(Debug, Clone)]
pub struct IabPanelState {
    browser_id: HostedBrowserId,
    draft_url: String,
    active_url: String,
    title: Option<String>,
    load_state: IabLoadState,
    error: Option<String>,
    browser_bounds: Option<HostedBrowserBounds>,
    browser_attached: bool,
    browser_visible: bool,
    panel_visible: bool,
}

impl Default for IabPanelState {
    fn default() -> Self {
        Self::new(SIDEBAR_BROWSER_ID, DEFAULT_URL)
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
            load_state: IabLoadState::Idle,
            error: None,
            browser_bounds: None,
            browser_attached: false,
            browser_visible: false,
            panel_visible: false,
        }
    }

    pub const fn browser_id(&self) -> HostedBrowserId {
        self.browser_id
    }

    pub fn browser_attached(&self) -> bool {
        self.browser_attached
    }

    pub fn browser_ready(&self) -> bool {
        self.load_state == IabLoadState::Ready
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

    pub fn update(
        &mut self,
        message: IabPanelMessage,
        window_id: HostedWindowId,
    ) -> Vec<HostedBrowserCommand> {
        match message {
            IabPanelMessage::DraftUrlChanged(value) => {
                self.draft_url = value;
                self.error = None;
                Vec::new()
            }
            IabPanelMessage::Navigate => self.navigate(window_id),
            IabPanelMessage::BoundsChanged(bounds) => self.update_bounds(bounds, window_id),
            IabPanelMessage::OpenWindow | IabPanelMessage::Close => Vec::new(),
        }
    }

    pub fn set_panel_visible(
        &mut self,
        visible: bool,
        window_id: HostedWindowId,
    ) -> Vec<HostedBrowserCommand> {
        self.panel_visible = visible;
        if visible && !self.browser_attached {
            return self
                .browser_bounds
                .map(|bounds| vec![self.attach(window_id, bounds)])
                .unwrap_or_default();
        }
        if !self.browser_attached || self.browser_visible == visible {
            return Vec::new();
        }
        self.browser_visible = visible;
        vec![HostedBrowserCommand::SetVisible {
            id: self.browser_id,
            visible,
        }]
    }

    pub fn handle_browser_event(&mut self, event: HostedBrowserEvent) -> bool {
        if event.id() != self.browser_id {
            return false;
        }
        match event {
            HostedBrowserEvent::PageLoad { state, url, .. } => {
                self.error = None;
                self.active_url = url.clone();
                self.load_state = match state {
                    HostedBrowserLoadState::Started => IabLoadState::Loading,
                    HostedBrowserLoadState::Finished => {
                        self.draft_url = url;
                        IabLoadState::Ready
                    }
                };
            }
            HostedBrowserEvent::DocumentTitleChanged { title, .. } => {
                self.title = (!title.trim().is_empty()).then_some(title);
            }
            HostedBrowserEvent::CommandFailed { command, .. } => {
                self.error = Some(browser_error_message(command).to_owned());
                if matches!(command, HostedBrowserCommandKind::Attach) {
                    self.browser_attached = false;
                    self.browser_visible = false;
                }
                if matches!(command, HostedBrowserCommandKind::Navigate) {
                    self.load_state = IabLoadState::Idle;
                }
            }
        }
        true
    }

    pub fn view(
        &self,
        tokens: ThemeTokens,
        allow_detached_window: bool,
    ) -> Element<'static, IabPanelMessage> {
        let colors = tokens.colors;
        let detached = button(text("独立窗口").size(10).color(colors.text))
            .on_press(IabPanelMessage::OpenWindow)
            .style(button_style(tokens, ButtonKind::Text));
        let close = button(icon(Icon::Close, 13.0, colors.muted))
            .on_press(IabPanelMessage::Close)
            .style(button_style(tokens, ButtonKind::Text));
        let mut heading = row![column![
            text(
                self.title
                    .clone()
                    .unwrap_or_else(|| "网页浏览器".to_owned())
            )
            .size(15)
            .font(ui_font(iced::font::Weight::Semibold))
            .color(colors.text),
            text(self.status_label()).size(10).color(colors.muted),
        ]
        .spacing(2)
        .width(Length::Fill),]
        .spacing(8)
        .align_y(Alignment::Center);
        if allow_detached_window {
            heading = heading.push(detached);
        }
        heading = heading.push(close);

        let address = Input::new("https://example.com", self.draft_url.clone())
            .on_input(IabPanelMessage::DraftUrlChanged)
            .on_submit(IabPanelMessage::Navigate)
            .invalid(self.error.is_some())
            .size(ControlSize::Small)
            .view(tokens);
        let open = button(
            row![icon(Icon::Eye, 12.0, colors.text), text("打开").size(10),]
                .spacing(4)
                .align_y(Alignment::Center),
        )
        .on_press(IabPanelMessage::Navigate)
        .style(button_style(tokens, ButtonKind::Primary));
        let address_bar = row![address, open]
            .spacing(6)
            .align_y(Alignment::Center)
            .width(Length::Fill);

        let placeholder = if self.error.is_some() {
            "当前无法打开网页"
        } else if self.browser_attached {
            ""
        } else {
            "正在准备网页浏览器…"
        };
        let browser = LayoutProbe::new(
            container(text(placeholder).size(11).color(colors.muted))
                .width(Length::Fill)
                .height(Length::Fill)
                .center(Length::Fill)
                .style(canvas_style(tokens)),
            IabPanelMessage::BoundsChanged,
        );

        let mut content = column![heading, address_bar]
            .spacing(8)
            .width(Length::Fill)
            .height(Length::Fill);
        if let Some(error) = &self.error {
            content = content.push(text(error.clone()).size(10).color(colors.danger));
        }
        content = content.push(browser);
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::from([12, 12]))
            .style(canvas_style(tokens))
            .into()
    }

    fn navigate(&mut self, window_id: HostedWindowId) -> Vec<HostedBrowserCommand> {
        let url = match normalize_url(&self.draft_url) {
            Ok(url) => url,
            Err(message) => {
                self.error = Some(message.to_owned());
                return Vec::new();
            }
        };
        self.draft_url = url.clone();
        self.active_url = url.clone();
        self.title = None;
        self.load_state = IabLoadState::Loading;
        self.error = None;
        if self.browser_attached {
            return vec![HostedBrowserCommand::Navigate {
                id: self.browser_id,
                url,
            }];
        }
        if self.panel_visible {
            return self
                .browser_bounds
                .map(|bounds| vec![self.attach(window_id, bounds)])
                .unwrap_or_default();
        }
        Vec::new()
    }

    fn update_bounds(
        &mut self,
        bounds: LayoutBounds,
        window_id: HostedWindowId,
    ) -> Vec<HostedBrowserCommand> {
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return Vec::new();
        }
        let bounds = HostedBrowserBounds::from(bounds);
        let changed = self.browser_bounds != Some(bounds);
        self.browser_bounds = Some(bounds);
        if !self.panel_visible {
            return Vec::new();
        }
        if !self.browser_attached {
            return vec![self.attach(window_id, bounds)];
        }
        changed
            .then_some(HostedBrowserCommand::SetBounds {
                id: self.browser_id,
                bounds,
            })
            .into_iter()
            .collect()
    }

    fn attach(
        &mut self,
        window_id: HostedWindowId,
        bounds: HostedBrowserBounds,
    ) -> HostedBrowserCommand {
        self.browser_attached = true;
        self.browser_visible = true;
        self.load_state = IabLoadState::Loading;
        HostedBrowserCommand::Attach {
            id: self.browser_id,
            window_id,
            url: self.active_url.clone(),
            bounds,
        }
    }

    fn status_label(&self) -> String {
        match self.load_state {
            IabLoadState::Idle => "输入完整地址后打开".to_owned(),
            IabLoadState::Loading => "正在加载".to_owned(),
            IabLoadState::Ready => self.active_url.clone(),
        }
    }
}

fn normalize_url(value: &str) -> Result<String, &'static str> {
    let raw = value.trim();
    let raw = if raw.is_empty() { DEFAULT_URL } else { raw };
    let url = Url::parse(raw).map_err(|_| "请输入完整的 HTTP(S) 地址。")?;
    let supported = match url.scheme() {
        "http" | "https" => url.host_str().is_some(),
        "about" => url.as_str() == DEFAULT_URL,
        _ => false,
    };
    if !supported {
        return Err("请输入完整的 HTTP(S) 地址。");
    }
    Ok(url.to_string())
}

fn browser_error_message(command: HostedBrowserCommandKind) -> &'static str {
    match command {
        HostedBrowserCommandKind::Navigate => "无法打开这个地址，请检查后重试。",
        HostedBrowserCommandKind::Attach => "无法打开网页浏览器，请重试。",
        HostedBrowserCommandKind::SetBounds
        | HostedBrowserCommandKind::SetVisible
        | HostedBrowserCommandKind::Focus
        | HostedBrowserCommandKind::Detach => "网页浏览器暂时不可用，请关闭后重试。",
    }
}

#[cfg(test)]
mod tests {
    use super::{IabLoadState, IabPanelMessage, IabPanelState, SIDEBAR_BROWSER_ID};
    use nana_ui::{
        HostedBrowserCommand, HostedBrowserCommandKind, HostedBrowserEvent, HostedBrowserLoadState,
        HostedWindowId, LayoutBounds,
    };

    #[test]
    fn browser_attaches_only_after_visible_bounds_are_measured() {
        let mut state = IabPanelState::default();
        let bounds = LayoutBounds::new(24.0, 80.0, 420.0, 560.0);

        assert!(state
            .update(
                IabPanelMessage::BoundsChanged(bounds),
                HostedWindowId::PRIMARY,
            )
            .is_empty());
        let commands = state.set_panel_visible(true, HostedWindowId::PRIMARY);

        assert!(matches!(
            commands.as_slice(),
            [HostedBrowserCommand::Attach {
                id: SIDEBAR_BROWSER_ID,
                window_id: HostedWindowId::PRIMARY,
                ..
            }]
        ));
    }

    #[test]
    fn browser_visibility_tracks_panel_lifecycle_without_losing_session() {
        let mut state = IabPanelState::default();
        state.update(
            IabPanelMessage::BoundsChanged(LayoutBounds::new(0.0, 40.0, 400.0, 500.0)),
            HostedWindowId::PRIMARY,
        );
        state.set_panel_visible(true, HostedWindowId::PRIMARY);

        let hidden = state.set_panel_visible(false, HostedWindowId::PRIMARY);
        let visible = state.set_panel_visible(true, HostedWindowId::PRIMARY);

        assert!(matches!(
            hidden.as_slice(),
            [HostedBrowserCommand::SetVisible {
                id: SIDEBAR_BROWSER_ID,
                visible: false,
            }]
        ));
        assert!(matches!(
            visible.as_slice(),
            [HostedBrowserCommand::SetVisible {
                id: SIDEBAR_BROWSER_ID,
                visible: true,
            }]
        ));
    }

    #[test]
    fn navigation_rejects_non_web_schemes_before_replacing_active_page() {
        let mut state = IabPanelState::default();
        state.update(
            IabPanelMessage::DraftUrlChanged("file:///tmp/private".to_owned()),
            HostedWindowId::PRIMARY,
        );

        let commands = state.update(IabPanelMessage::Navigate, HostedWindowId::PRIMARY);

        assert!(commands.is_empty());
        assert_eq!(state.active_url, "about:blank");
        assert!(state.error.is_some());
    }

    #[test]
    fn navigation_retries_attach_after_the_host_reports_a_failure() {
        let mut state = IabPanelState::default();
        state.update(
            IabPanelMessage::BoundsChanged(LayoutBounds::new(0.0, 40.0, 400.0, 500.0)),
            HostedWindowId::PRIMARY,
        );
        state.set_panel_visible(true, HostedWindowId::PRIMARY);
        state.handle_browser_event(HostedBrowserEvent::CommandFailed {
            id: SIDEBAR_BROWSER_ID,
            command: HostedBrowserCommandKind::Attach,
            message: "host failure".to_owned(),
        });
        state.update(
            IabPanelMessage::DraftUrlChanged("https://example.com".to_owned()),
            HostedWindowId::PRIMARY,
        );

        let commands = state.update(IabPanelMessage::Navigate, HostedWindowId::PRIMARY);

        assert!(matches!(
            commands.as_slice(),
            [HostedBrowserCommand::Attach {
                id: SIDEBAR_BROWSER_ID,
                window_id: HostedWindowId::PRIMARY,
                url,
                ..
            }] if url == "https://example.com/"
        ));
    }

    #[test]
    fn completed_redirect_updates_the_visible_address_and_title() {
        let mut state = IabPanelState::default();

        assert!(state.handle_browser_event(HostedBrowserEvent::PageLoad {
            id: SIDEBAR_BROWSER_ID,
            state: HostedBrowserLoadState::Finished,
            url: "https://example.com/final".to_owned(),
        }));
        assert!(
            state.handle_browser_event(HostedBrowserEvent::DocumentTitleChanged {
                id: SIDEBAR_BROWSER_ID,
                title: "Example".to_owned(),
            })
        );

        assert_eq!(state.load_state, IabLoadState::Ready);
        assert_eq!(state.active_url, "https://example.com/final");
        assert_eq!(state.draft_url, state.active_url);
        assert_eq!(state.title.as_deref(), Some("Example"));
    }
}

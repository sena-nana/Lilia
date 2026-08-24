use nana_ui::{
    RuntimeProgramContext, RuntimeProgramUpdate, WindowChromeAction,
};
use nana_ui_platform::{WindowCommand, WindowEvent, WindowGeometry, WindowId, WindowSettings};

pub type HostedProgramContext<M> = RuntimeProgramContext<M>;
pub type HostedProgramUpdate = RuntimeProgramUpdate;

pub trait HostedUpdateExt {
    fn redraw_primary() -> Self;
    fn redraw_window(id: WindowId) -> Self;
    fn with_window_commands(self, commands: impl IntoIterator<Item = WindowCommand>) -> Self;
}

impl HostedUpdateExt for RuntimeProgramUpdate {
    fn redraw_primary() -> Self {
        Self::redraw(WindowId::PRIMARY)
    }

    fn redraw_window(id: WindowId) -> Self {
        Self::redraw(id)
    }

    fn with_window_commands(mut self, commands: impl IntoIterator<Item = WindowCommand>) -> Self {
        self.window_commands.extend(commands);
        self
    }
}

pub type HostedWindowId = WindowId;
pub type HostedWindowEvent = WindowEvent;
pub type HostedWindowCommand = WindowCommand;
pub type HostedWindowGeometry = WindowGeometry;
pub type HostedWindowSettings = WindowSettings;

#[derive(Debug, Clone, PartialEq)]
pub enum HostedUiCommand {
    Focus {
        window_id: WindowId,
        target: String,
    },
    ScrollBy {
        window_id: WindowId,
        target: String,
        x: f32,
        y: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostedWindowAction {
    pub id: WindowId,
    pub action: WindowChromeAction,
}

impl HostedWindowAction {
    pub fn into_window_command(self) -> Option<WindowCommand> {
        match self.action {
            WindowChromeAction::Drag => Some(WindowCommand::Drag(self.id)),
            WindowChromeAction::Minimize => Some(WindowCommand::SetMinimized {
                id: self.id,
                minimized: true,
            }),
            WindowChromeAction::Close => Some(WindowCommand::Close(self.id)),
            WindowChromeAction::ToggleMaximize => Some(WindowCommand::SetMaximized {
                id: self.id,
                maximized: true,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HostedWindowCaptureId(pub u64);

pub fn window_event_id(event: &WindowEvent) -> WindowId {
    match event {
        WindowEvent::Ready { id, .. }
        | WindowEvent::Resized { id, .. }
        | WindowEvent::Moved { id, .. }
        | WindowEvent::VisibilityChanged { id, .. }
        | WindowEvent::FocusChanged { id, .. }
        | WindowEvent::Ime { id, .. }
        | WindowEvent::CloseRequested { id }
        | WindowEvent::Closed { id }
        | WindowEvent::FileHovered { id, .. }
        | WindowEvent::FileDropped { id, .. }
        | WindowEvent::FileHoverCancelled { id } => *id,
    }
}

pub fn tool_window_settings(title: impl Into<String>, width: f64, height: f64, min_width: f64, min_height: f64) -> WindowSettings {
    let mut settings = WindowSettings::new(title)
        .initial_size(width, height)
        .minimum_size(min_width, min_height);
    settings.role = nana_ui_platform::WindowRole::Tool;
    settings.parent = Some(WindowId::PRIMARY);
    settings
}

pub fn startup_document(status: &str) -> Result<nana_ui::runtime::RuntimeDocument, nana_ui::runtime::FrameworkError> {
    let document_id = nana_ui::runtime::DocumentId::new(1).expect("startup document id");
    let mut document = nana_ui::runtime::RuntimeDocument::new(document_id);
    let title = document
        .context_mut()
        .create_component(document_id, nana_ui::runtime::Text::new("LiliaCode"))?;
    let message = document
        .context_mut()
        .create_component(document_id, nana_ui::runtime::Text::new(status))?;
    document.context_mut().append_child(title, message)?;
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn window_event_id_covers_file_drop_events() {
        let id = WindowId(7);
        assert_eq!(
            window_event_id(&WindowEvent::FileHovered {
                id,
                paths: vec![PathBuf::from("/tmp/project")],
                position: Some((12.0, 40.0)),
            }),
            id
        );
        assert_eq!(
            window_event_id(&WindowEvent::FileDropped {
                id,
                paths: vec![PathBuf::from("/tmp/note.md")],
                position: None,
            }),
            id
        );
        assert_eq!(
            window_event_id(&WindowEvent::FileHoverCancelled { id }),
            id
        );
    }
}

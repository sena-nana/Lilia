use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use lilia_contracts::TaskId;
use nana_ui::runtime::{
    AppContext, Button, DesktopShell, DocumentId, Entity, FrameworkError, IconButton, List,
    NativeMarkdown, ScrollAxes, ScrollView, Text, TextArea, TextChanged,
};
use nana_ui::{ButtonKind, ControlSize, ThemeMode, WindowChrome};
use nana_ui_platform::WindowId;

use crate::runtime_layout::{
    composer_interrupt_button, composer_send_button, reconcile_children, HostStack,
};
use crate::runtime_shell::{bind_activate, emit, ShellIntent, ShellTimelineRow};

const CONVERSATION_STATUS_DOCUMENT: u64 = 10_001;

type IntentSink = Arc<dyn Fn(ShellIntent) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct ConversationStatusRow {
    pub task_id: TaskId,
    pub title: String,
    pub project_name: String,
    pub status: String,
    pub phase: String,
    pub can_stop: bool,
}

#[derive(Debug, Clone)]
pub struct ConversationStatusSnapshot {
    pub theme: ThemeMode,
    pub pinned: bool,
    pub error: Option<String>,
    pub entries: Vec<ConversationStatusRow>,
}

#[derive(Debug, Clone)]
pub struct TaskPopupSnapshot {
    pub window_id: WindowId,
    pub theme: ThemeMode,
    pub title: String,
    pub heading: String,
    pub error: Option<String>,
    pub timeline: Vec<ShellTimelineRow>,
    pub composer: String,
    pub composer_disabled: bool,
    pub can_send: bool,
    pub can_interrupt: bool,
}

pub struct ConversationStatusHandles {
    sink: IntentSink,
    shell: Entity<DesktopShell>,
    title: Entity<Text>,
    error: Entity<Text>,
    list: Entity<List>,
    rows: HashMap<String, Entity<HostStack>>,
    pin: Entity<Button>,
}

pub struct TaskPopupHandles {
    shell: Entity<DesktopShell>,
    heading: Entity<Text>,
    error: Entity<Text>,
    timeline_scroll: Entity<ScrollView>,
    timeline_items: HashMap<String, Entity<NativeMarkdown>>,
    timeline_sources: HashMap<String, u64>,
    composer: Entity<TextArea>,
    send: Entity<IconButton>,
    interrupt: Entity<IconButton>,
}

fn action_button(label: &str, kind: ButtonKind) -> Button {
    Button::new(label).kind(kind).size(ControlSize::Small)
}

pub fn mount_conversation_status(
    snapshot: &ConversationStatusSnapshot,
    sink: IntentSink,
) -> Result<(nana_ui::runtime::RuntimeDocument, ConversationStatusHandles), FrameworkError> {
    let document_id = DocumentId::new(CONVERSATION_STATUS_DOCUMENT).expect("status document");
    let mut document = nana_ui::runtime::RuntimeDocument::new(document_id);
    let context = document.context_mut();
    let _ = context.set_theme(snapshot.theme);

    let title = context.create_detached_component(document_id, Text::new("会话状态"))?;
    let error = context.create_detached_component(
        document_id,
        Text::new(snapshot.error.clone().unwrap_or_default()),
    )?;
    let list = context.create_detached_component(document_id, List::new())?;
    let actions = context.create_detached_component(document_id, HostStack::leading_row(8.0))?;
    let pin = context.create_detached_component(
        document_id,
        action_button(
            if snapshot.pinned {
                "取消置顶"
            } else {
                "置顶"
            },
            ButtonKind::Subtle,
        ),
    )?;
    let new_chat = context
        .create_detached_component(document_id, action_button("新会话", ButtonKind::Primary))?;
    let close = context
        .create_detached_component(document_id, action_button("关闭", ButtonKind::Subtle))?;
    bind_activate(
        context,
        pin,
        Arc::clone(&sink),
        ShellIntent::ToggleConversationStatusPin,
    )?;
    bind_activate(
        context,
        new_chat,
        Arc::clone(&sink),
        ShellIntent::OpenConversationStatusNewChat,
    )?;
    bind_activate(
        context,
        close,
        Arc::clone(&sink),
        ShellIntent::CloseConversationStatus,
    )?;
    context.append_child(actions, pin)?;
    context.append_child(actions, new_chat)?;
    context.append_child(actions, close)?;

    let page = context
        .create_detached_component(document_id, HostStack::fill_column(10.0).padding(16.0))?;
    context.append_child(page, title)?;
    context.append_child(page, error)?;
    context.append_child(page, list)?;
    context.append_child(page, actions)?;

    let title_trailing = context.create_detached_component(document_id, HostStack::row(6.0))?;
    if WindowChrome::platform_default().uses_custom_controls() {
        let close_win = context.create_detached_component(
            document_id,
            nana_ui::runtime::IconButton::new(nana_ui::Icon::Close, "关闭"),
        )?;
        context.append_child(title_trailing, close_win)?;
        bind_activate(
            context,
            close_win,
            Arc::clone(&sink),
            ShellIntent::CloseConversationStatus,
        )?;
    }

    let shell = context.create_component(
        document_id,
        DesktopShell::from_model(nana_ui::WorkspaceModel::new())
            .title("会话状态")
            .title_center(title.stable_id())
            .title_trailing(title_trailing.stable_id())
            .primary(page.stable_id()),
    )?;
    context.assemble_desktop_shell(shell)?;

    let mut handles = ConversationStatusHandles {
        sink,
        shell,
        title,
        error,
        list,
        rows: HashMap::new(),
        pin,
    };
    handles.sync_rows(context, document_id, snapshot)?;
    Ok((document, handles))
}

impl ConversationStatusHandles {
    pub fn sync(
        &mut self,
        document: &mut nana_ui::runtime::RuntimeDocument,
        snapshot: &ConversationStatusSnapshot,
    ) -> Result<(), FrameworkError> {
        let document_id = document.document();
        let context = document.context_mut();
        let _ = context.set_theme(snapshot.theme);
        context.update_component(self.title, |title, _| {
            *title = Text::new("会话状态");
        })?;
        context.update_component(self.error, |error, _| {
            *error = Text::new(snapshot.error.clone().unwrap_or_default());
        })?;
        context.update_component(self.pin, |button, _| {
            *button = action_button(
                if snapshot.pinned {
                    "取消置顶"
                } else {
                    "置顶"
                },
                ButtonKind::Subtle,
            );
        })?;
        self.sync_rows(context, document_id, snapshot)?;
        context.assemble_desktop_shell(self.shell)?;
        Ok(())
    }

    fn sync_rows(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &ConversationStatusSnapshot,
    ) -> Result<(), FrameworkError> {
        let mut keep = HashSet::new();
        let mut order = Vec::new();
        for entry in &snapshot.entries {
            let key = entry.task_id.as_str().to_owned();
            keep.insert(key.clone());
            let label = format!(
                "{} · {} · {} · {}",
                entry.title, entry.project_name, entry.status, entry.phase
            );
            let row = if let Some(row) = self.rows.get(&key).copied() {
                row
            } else {
                let row =
                    context.create_detached_component(document_id, HostStack::fill_column(4.0))?;
                let text =
                    context.create_detached_component(document_id, Text::new(label.clone()))?;
                let open = context.create_detached_component(
                    document_id,
                    action_button("打开", ButtonKind::Subtle),
                )?;
                bind_activate(
                    context,
                    open,
                    Arc::clone(&self.sink),
                    ShellIntent::OpenStatusTask(entry.task_id.clone()),
                )?;
                context.append_child(row, text)?;
                context.append_child(row, open)?;
                if entry.can_stop {
                    let stop = context.create_detached_component(
                        document_id,
                        action_button("停止", ButtonKind::Danger),
                    )?;
                    bind_activate(
                        context,
                        stop,
                        Arc::clone(&self.sink),
                        ShellIntent::StopStatusTask(entry.task_id.clone()),
                    )?;
                    context.append_child(row, stop)?;
                }
                self.rows.insert(key, row);
                row
            };
            let children = context
                .world()
                .node(row.stable_id())
                .map(|node| node.children.clone())
                .unwrap_or_default();
            if let Some(first) = children.first() {
                let _ =
                    context.update_component(Entity::<Text>::from_stable_id(*first), |text, _| {
                        *text = Text::new(label);
                    });
            }
            order.push(row.stable_id());
        }
        let stale: Vec<_> = self
            .rows
            .keys()
            .filter(|key| !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(row) = self.rows.remove(&key) {
                let _ = context.remove_view(row);
            }
        }
        reconcile_children(context, self.list.stable_id(), &order)
    }
}

pub fn mount_task_popup(
    snapshot: &TaskPopupSnapshot,
    sink: IntentSink,
) -> Result<(nana_ui::runtime::RuntimeDocument, TaskPopupHandles), FrameworkError> {
    let document_id =
        DocumentId::new(10_000u64.saturating_add(snapshot.window_id.0)).expect("popup document");
    let mut document = nana_ui::runtime::RuntimeDocument::new(document_id);
    let context = document.context_mut();
    let _ = context.set_theme(snapshot.theme);

    let heading =
        context.create_detached_component(document_id, Text::new(snapshot.heading.clone()))?;
    let error = context.create_detached_component(
        document_id,
        Text::new(snapshot.error.clone().unwrap_or_default()),
    )?;
    let timeline_scroll =
        context.create_detached_component(document_id, ScrollView::new(ScrollAxes::Vertical))?;
    let composer = context.create_detached_component(
        document_id,
        TextArea::new(snapshot.composer.clone()).height(96.0),
    )?;
    let composer_sink = Arc::clone(&sink);
    let window_id = snapshot.window_id;
    context.on(composer, move |_, event: &TextChanged, _| {
        emit(
            &composer_sink,
            ShellIntent::TaskPopupComposerChanged {
                window_id,
                value: event.value.clone(),
            },
        );
    })?;
    let send =
        context.create_detached_component(document_id, composer_send_button(snapshot.can_send))?;
    let interrupt = context.create_detached_component(
        document_id,
        composer_interrupt_button(snapshot.can_interrupt),
    )?;
    let close = context
        .create_detached_component(document_id, action_button("关闭窗口", ButtonKind::Subtle))?;
    bind_activate(
        context,
        send,
        Arc::clone(&sink),
        ShellIntent::TaskPopupSubmit(window_id),
    )?;
    bind_activate(
        context,
        interrupt,
        Arc::clone(&sink),
        ShellIntent::TaskPopupInterrupt(window_id),
    )?;
    bind_activate(
        context,
        close,
        Arc::clone(&sink),
        ShellIntent::CloseTaskPopup(window_id),
    )?;
    if snapshot.composer_disabled {
        context.update_component(composer, |editor, _| {
            editor.disabled = true;
        })?;
    }

    let actions = context.create_detached_component(document_id, HostStack::leading_row(8.0))?;
    context.append_child(actions, send)?;
    context.append_child(actions, interrupt)?;
    context.append_child(actions, close)?;
    let page = context
        .create_detached_component(document_id, HostStack::fill_column(10.0).padding(16.0))?;
    context.append_child(page, heading)?;
    context.append_child(page, error)?;
    context.append_child(page, timeline_scroll)?;
    context.append_child(page, composer)?;
    context.append_child(page, actions)?;

    let title =
        context.create_detached_component(document_id, Text::new(snapshot.title.clone()))?;
    let shell = context.create_component(
        document_id,
        DesktopShell::from_model(nana_ui::WorkspaceModel::new())
            .title(snapshot.title.clone())
            .title_center(title.stable_id())
            .primary(page.stable_id()),
    )?;
    context.assemble_desktop_shell(shell)?;

    let mut handles = TaskPopupHandles {
        shell,
        heading,
        error,
        timeline_scroll,
        timeline_items: HashMap::new(),
        timeline_sources: HashMap::new(),
        composer,
        send,
        interrupt,
    };
    handles.sync_timeline(context, document_id, snapshot)?;
    Ok((document, handles))
}

impl TaskPopupHandles {
    pub fn sync(
        &mut self,
        document: &mut nana_ui::runtime::RuntimeDocument,
        snapshot: &TaskPopupSnapshot,
    ) -> Result<(), FrameworkError> {
        let document_id = document.document();
        let context = document.context_mut();
        let _ = context.set_theme(snapshot.theme);
        context.update_component(self.heading, |text, _| {
            *text = Text::new(snapshot.heading.clone());
        })?;
        context.update_component(self.error, |text, _| {
            *text = Text::new(snapshot.error.clone().unwrap_or_default());
        })?;
        context.update_component(self.composer, |editor, _| {
            if editor.state.value != snapshot.composer {
                editor.state.replace_value(snapshot.composer.clone());
            }
            editor.disabled = snapshot.composer_disabled;
        })?;
        context.update_component(self.send, |button, _| {
            *button = composer_send_button(snapshot.can_send);
        })?;
        context.update_component(self.interrupt, |button, _| {
            *button = composer_interrupt_button(snapshot.can_interrupt);
        })?;
        self.sync_timeline(context, document_id, snapshot)?;
        context.assemble_desktop_shell(self.shell)?;
        Ok(())
    }

    fn sync_timeline(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &TaskPopupSnapshot,
    ) -> Result<(), FrameworkError> {
        let mut keep = HashSet::new();
        let mut order = Vec::new();
        for item in &snapshot.timeline {
            keep.insert(item.id.clone());
            let source = crate::runtime_shell::content_hash(&item.markdown);
            let entity = if let Some(entity) = self.timeline_items.get(&item.id).copied() {
                if self.timeline_sources.get(&item.id) != Some(&source) {
                    context.update_component(entity, |markdown, _| {
                        *markdown = NativeMarkdown::parse(&item.markdown);
                    })?;
                    context.assemble_markdown(entity)?;
                    self.timeline_sources.insert(item.id.clone(), source);
                }
                entity
            } else {
                let entity = context.create_detached_component(
                    document_id,
                    NativeMarkdown::parse(&item.markdown),
                )?;
                context.assemble_markdown(entity)?;
                self.timeline_items.insert(item.id.clone(), entity);
                self.timeline_sources.insert(item.id.clone(), source);
                entity
            };
            order.push(entity.stable_id());
        }
        let stale: Vec<_> = self
            .timeline_items
            .keys()
            .filter(|key| !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(entity) = self.timeline_items.remove(&key) {
                let _ = context.remove_view(entity);
            }
            self.timeline_sources.remove(&key);
        }
        reconcile_children(context, self.timeline_scroll.stable_id(), &order)
    }
}

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use lilia_contracts::TaskId;
use nana_ui::runtime::{
    AppContext, Button, DesktopShell, DocumentId, Entity, FrameworkError, IconButton, List,
    NativeMarkdown, ScrollAxes, ScrollView, Stack, Text, TextArea, TextChanged,
};
use nana_ui::{ButtonKind, ControlSize, ThemeMode, WindowChrome};
use nana_ui_platform::WindowId;

use crate::runtime_layout::{
    composer_interrupt_button, composer_send_button, pending_actions_row, pending_interaction_card,
    reconcile_children, window_control,
};
use crate::runtime_shell::{
    bind_activate, composer_is_focused, emit, pending_action_specs, ComposerGeneration,
    ShellIntent, ShellPendingKind, ShellTimelineRow,
};

const CONVERSATION_STATUS_DOCUMENT: u64 = 10_001;

type IntentSink = Arc<dyn Fn(ShellIntent) + Send + Sync>;

fn popup_pending_intent(window_id: WindowId, intent: ShellIntent) -> ShellIntent {
    match &intent {
        ShellIntent::RespondApproval { .. }
        | ShellIntent::RespondPlan { .. }
        | ShellIntent::RespondToolConsent { .. }
        | ShellIntent::RespondArchitecture { .. }
        | ShellIntent::RespondTitle { .. }
        | ShellIntent::RespondMcp { .. }
        | ShellIntent::McpFieldChanged { .. }
        | ShellIntent::McpRawJsonChanged { .. }
        | ShellIntent::McpToggleOption { .. }
        | ShellIntent::McpToggleBoolean { .. }
        | ShellIntent::ToolConsentDraftChanged { .. }
        | ShellIntent::PendingDraftChanged { .. }
        | ShellIntent::SelectPendingOption { .. }
        | ShellIntent::AskUserPending { .. }
        | ShellIntent::InterruptTurn => ShellIntent::TaskPopupPending {
            window_id,
            intent: Box::new(intent),
        },
        _ => intent,
    }
}

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
    pub composer_task_id: Option<String>,
    pub composer_revision: u64,
    pub composer_disabled: bool,
    pub can_send: bool,
    pub can_interrupt: bool,
    pub pending: Option<crate::runtime_shell::ShellPending>,
}

pub struct ConversationStatusHandles {
    sink: IntentSink,
    shell: Entity<DesktopShell>,
    title: Entity<Text>,
    error: Entity<Text>,
    list: Entity<List>,
    rows: HashMap<String, Entity<Stack>>,
    pin: Entity<Button>,
}

pub struct TaskPopupHandles {
    window_id: WindowId,
    sink: IntentSink,
    shell: Entity<DesktopShell>,
    heading: Entity<Text>,
    error: Entity<Text>,
    timeline_scroll: Entity<ScrollView>,
    timeline_items: HashMap<String, Entity<NativeMarkdown>>,
    timeline_sources: HashMap<String, u64>,
    page: Entity<Stack>,
    pending_panel: Entity<Stack>,
    pending_title: Entity<Text>,
    pending_prompt: Entity<Text>,
    pending_draft: Entity<TextArea>,
    pending_tool_command: Entity<TextArea>,
    pending_tool_message: Entity<TextArea>,
    pending_actions: Entity<Stack>,
    pending_buttons: HashMap<String, Entity<Button>>,
    pending_fields: HashMap<String, Entity<TextArea>>,
    pending_request: Arc<Mutex<String>>,
    pending_tool_command_value: Arc<Mutex<String>>,
    pending_tool_message_value: Arc<Mutex<String>>,
    composer: Entity<TextArea>,
    composer_actions: Entity<Stack>,
    composer_generation: ComposerGeneration,
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
    let actions = context.create_detached_component(document_id, Stack::row(8.0))?;
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
        .create_detached_component(document_id, Stack::fill_column(10.0).padding(16.0))?;
    context.append_child(page, title)?;
    context.append_child(page, error)?;
    context.append_child(page, list)?;
    context.append_child(page, actions)?;

    let title_trailing = context.create_detached_component(document_id, Stack::row(6.0))?;
    if WindowChrome::platform_default().uses_custom_controls() {
        let close_win = context.create_detached_component(
            document_id,
            window_control(nana_ui::Icon::Close, "关闭", ButtonKind::Text),
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
                    context.create_detached_component(document_id, Stack::fill_column(4.0))?;
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

    let actions = context.create_detached_component(document_id, Stack::row(8.0))?;
    context.append_child(actions, send)?;
    context.append_child(actions, interrupt)?;
    context.append_child(actions, close)?;
    let pending_panel =
        context.create_detached_component(document_id, pending_interaction_card())?;
    let pending_title = context.create_detached_component(document_id, Text::new(String::new()))?;
    let pending_prompt =
        context.create_detached_component(document_id, Text::new(String::new()))?;
    let pending_draft = context
        .create_detached_component(document_id, TextArea::new(String::new()).height(48.0))?;
    let pending_request = Arc::new(Mutex::new(
        snapshot
            .pending
            .as_ref()
            .map(|pending| pending.request_id.clone())
            .unwrap_or_default(),
    ));
    context.on(pending_draft, {
        let sink = Arc::clone(&sink);
        let pending_request = Arc::clone(&pending_request);
        move |_, event: &TextChanged, _| {
            let request_id = pending_request
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            emit(
                &sink,
                popup_pending_intent(
                    window_id,
                    ShellIntent::PendingDraftChanged {
                        request_id,
                        value: event.value.clone(),
                    },
                ),
            );
        }
    })?;
    let pending_tool_command_value = Arc::new(Mutex::new(String::new()));
    let pending_tool_message_value = Arc::new(Mutex::new(String::new()));
    let pending_tool_command = context
        .create_detached_component(document_id, TextArea::new(String::new()).height(48.0))?;
    context.on(pending_tool_command, {
        let sink = Arc::clone(&sink);
        let pending_request = Arc::clone(&pending_request);
        let pending_tool_command_value = Arc::clone(&pending_tool_command_value);
        let pending_tool_message_value = Arc::clone(&pending_tool_message_value);
        move |_, event: &TextChanged, _| {
            let request_id = pending_request
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            if let Ok(mut guard) = pending_tool_command_value.lock() {
                *guard = event.value.clone();
            }
            let message = pending_tool_message_value
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            emit(
                &sink,
                popup_pending_intent(
                    window_id,
                    ShellIntent::ToolConsentDraftChanged {
                        request_id,
                        command: event.value.clone(),
                        message,
                    },
                ),
            );
        }
    })?;
    let pending_tool_message = context
        .create_detached_component(document_id, TextArea::new(String::new()).height(48.0))?;
    context.on(pending_tool_message, {
        let sink = Arc::clone(&sink);
        let pending_request = Arc::clone(&pending_request);
        let pending_tool_command_value = Arc::clone(&pending_tool_command_value);
        let pending_tool_message_value = Arc::clone(&pending_tool_message_value);
        move |_, event: &TextChanged, _| {
            let request_id = pending_request
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            if let Ok(mut guard) = pending_tool_message_value.lock() {
                *guard = event.value.clone();
            }
            let command = pending_tool_command_value
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            emit(
                &sink,
                popup_pending_intent(
                    window_id,
                    ShellIntent::ToolConsentDraftChanged {
                        request_id,
                        command,
                        message: event.value.clone(),
                    },
                ),
            );
        }
    })?;
    let pending_actions = context.create_detached_component(document_id, pending_actions_row())?;
    context.append_child(pending_panel, pending_title)?;
    context.append_child(pending_panel, pending_prompt)?;
    let page = context
        .create_detached_component(document_id, Stack::fill_column(10.0).padding(16.0))?;
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
        window_id: snapshot.window_id,
        sink,
        shell,
        heading,
        error,
        timeline_scroll,
        timeline_items: HashMap::new(),
        timeline_sources: HashMap::new(),
        page,
        pending_panel,
        pending_title,
        pending_prompt,
        pending_draft,
        pending_tool_command,
        pending_tool_message,
        pending_actions,
        pending_buttons: HashMap::new(),
        pending_fields: HashMap::new(),
        pending_request,
        pending_tool_command_value,
        pending_tool_message_value,
        composer,
        composer_actions: actions,
        composer_generation: ComposerGeneration::default(),
        send,
        interrupt,
    };
    handles.sync_timeline(context, document_id, snapshot)?;
    handles.sync_pending(context, document_id, snapshot)?;
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
        let composer_generation = ComposerGeneration::new(
            snapshot.composer_task_id.clone(),
            snapshot.composer_revision,
        );
        let write_composer = !composer_is_focused(context, self.composer)
            || self.composer_generation != composer_generation;
        context.update_component(self.composer, |editor, _| {
            if write_composer && editor.state.value != snapshot.composer {
                editor.state.replace_value(snapshot.composer.clone());
            }
            editor.disabled = snapshot.composer_disabled;
        })?;
        self.composer_generation = composer_generation;
        context.update_component(self.send, |button, _| {
            *button = composer_send_button(snapshot.can_send);
        })?;
        context.update_component(self.interrupt, |button, _| {
            *button = composer_interrupt_button(snapshot.can_interrupt);
        })?;
        self.sync_timeline(context, document_id, snapshot)?;
        self.sync_pending(context, document_id, snapshot)?;
        context.assemble_desktop_shell(self.shell)?;
        Ok(())
    }

    fn sync_pending(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &TaskPopupSnapshot,
    ) -> Result<(), FrameworkError> {
        let Some(pending) = &snapshot.pending else {
            if let Ok(mut guard) = self.pending_request.lock() {
                guard.clear();
            }
            for field in self.pending_fields.drain() {
                let _ = context.remove_view(field.1);
            }
            reconcile_children(context, self.pending_panel.stable_id(), &[])?;
            return reconcile_children(
                context,
                self.page.stable_id(),
                &[
                    self.heading.stable_id(),
                    self.error.stable_id(),
                    self.timeline_scroll.stable_id(),
                    self.composer.stable_id(),
                    self.composer_actions.stable_id(),
                ],
            );
        };
        context.update_component(self.pending_title, |text, _| {
            *text = Text::new(pending.title.clone());
        })?;
        context.update_component(self.pending_prompt, |text, _| {
            *text = Text::new(pending.prompt.clone());
        })?;
        if let Ok(mut guard) = self.pending_request.lock() {
            *guard = pending.request_id.clone();
        }
        let show_draft = pending.kind == ShellPendingKind::PlanApproval
            || (pending.kind == ShellPendingKind::AskUser
                && pending.ask.as_ref().is_some_and(|ask| ask.show_freeform));
        context.update_component(self.pending_draft, |editor, _| {
            if editor.state.value != pending.draft {
                editor.state.replace_value(pending.draft.clone());
            }
        })?;
        let mut panel = vec![
            self.pending_title.stable_id(),
            self.pending_prompt.stable_id(),
        ];
        if show_draft {
            panel.push(self.pending_draft.stable_id());
        }
        if pending.kind == ShellPendingKind::ToolConsent {
            if let Some(tool) = &pending.tool {
                if let Ok(mut guard) = self.pending_tool_command_value.lock() {
                    *guard = tool.command.clone();
                }
                if let Ok(mut guard) = self.pending_tool_message_value.lock() {
                    *guard = tool.message.clone();
                }
                if tool.command_editable {
                    context.update_component(self.pending_tool_command, |editor, _| {
                        editor.placeholder = Arc::from("确认执行的命令");
                        if editor.state.value != tool.command {
                            editor.state.replace_value(tool.command.clone());
                        }
                    })?;
                    panel.push(self.pending_tool_command.stable_id());
                }
                context.update_component(self.pending_tool_message, |editor, _| {
                    editor.placeholder = Arc::from("拒绝理由");
                    if editor.state.value != tool.message {
                        editor.state.replace_value(tool.message.clone());
                    }
                })?;
                panel.push(self.pending_tool_message.stable_id());
            }
        }
        let mut keep = HashSet::new();
        let mut field_keep = HashSet::new();
        if pending.kind == ShellPendingKind::McpElicitation {
            if let Some(mcp) = &pending.mcp {
                let request_id = pending.request_id.clone();
                if let Some(url) = &mcp.url {
                    let id = format!("popup-mcp-url-{request_id}");
                    keep.insert(id.clone());
                    let button = self.upsert_pending_button(
                        context,
                        document_id,
                        &id,
                        "打开链接",
                        ButtonKind::Subtle,
                        ShellIntent::OpenMarkdownLink(url.clone()),
                    )?;
                    panel.push(button.stable_id());
                }
                if let Some(raw) = &mcp.raw_json {
                    let field = self.upsert_pending_field(
                        context,
                        document_id,
                        &mut field_keep,
                        &format!("popup-mcp-raw-{request_id}"),
                        "原始 JSON",
                        raw,
                        {
                            let request_id = request_id.clone();
                            move |value| ShellIntent::McpRawJsonChanged {
                                request_id: request_id.clone(),
                                value,
                            }
                        },
                    )?;
                    panel.push(field.stable_id());
                }
                for field in &mcp.fields {
                    if field.options.is_empty() && field.kind != "boolean" {
                        let editor = self.upsert_pending_field(
                            context,
                            document_id,
                            &mut field_keep,
                            &format!("popup-mcp-field-{request_id}-{}", field.key),
                            &field.label,
                            &field.value,
                            {
                                let request_id = request_id.clone();
                                let field_key = field.key.clone();
                                move |value| ShellIntent::McpFieldChanged {
                                    request_id: request_id.clone(),
                                    field_key: field_key.clone(),
                                    value,
                                }
                            },
                        )?;
                        panel.push(editor.stable_id());
                    } else if field.kind == "boolean" {
                        let id = format!("popup-mcp-bool-{request_id}-{}", field.key);
                        keep.insert(id.clone());
                        let button = self.upsert_pending_button(
                            context,
                            document_id,
                            &id,
                            if field.enabled {
                                "已开启"
                            } else {
                                "已关闭"
                            },
                            if field.enabled {
                                ButtonKind::Primary
                            } else {
                                ButtonKind::Subtle
                            },
                            ShellIntent::McpToggleBoolean {
                                request_id: request_id.clone(),
                                field_key: field.key.clone(),
                            },
                        )?;
                        panel.push(button.stable_id());
                    } else {
                        for option in &field.options {
                            let id = format!(
                                "popup-mcp-opt-{request_id}-{}-{}",
                                field.key, option.value
                            );
                            keep.insert(id.clone());
                            let button = self.upsert_pending_button(
                                context,
                                document_id,
                                &id,
                                &option.label,
                                if option.selected {
                                    ButtonKind::Primary
                                } else {
                                    ButtonKind::Subtle
                                },
                                ShellIntent::McpToggleOption {
                                    request_id: request_id.clone(),
                                    field_key: field.key.clone(),
                                    value: option.value.clone(),
                                    multi: option.multi,
                                },
                            )?;
                            panel.push(button.stable_id());
                        }
                    }
                }
            }
        }
        let stale_fields: Vec<_> = self
            .pending_fields
            .keys()
            .filter(|key| !field_keep.contains(*key))
            .cloned()
            .collect();
        for key in stale_fields {
            if let Some(field) = self.pending_fields.remove(&key) {
                let _ = context.remove_view(field);
            }
        }
        let mut action_order = Vec::new();
        for option in &pending.options {
            let id = format!("popup-opt-{}-{}", pending.request_id, option.id);
            keep.insert(id.clone());
            let button = self.upsert_pending_button(
                context,
                document_id,
                &id,
                &option.label,
                if option.selected {
                    ButtonKind::Primary
                } else if option.danger {
                    ButtonKind::Danger
                } else {
                    ButtonKind::Subtle
                },
                ShellIntent::SelectPendingOption {
                    request_id: pending.request_id.clone(),
                    option_id: option.id.clone(),
                },
            )?;
            action_order.push(button.stable_id());
        }
        for (id, label, kind, intent, disabled) in pending_action_specs(pending) {
            keep.insert(id.clone());
            let button =
                self.upsert_pending_button(context, document_id, &id, &label, kind, intent)?;
            context.update_component(button, |button, _| {
                button.disabled = disabled;
            })?;
            action_order.push(button.stable_id());
        }
        let stale: Vec<_> = self
            .pending_buttons
            .keys()
            .filter(|key| !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(button) = self.pending_buttons.remove(&key) {
                let _ = context.remove_view(button);
            }
        }
        reconcile_children(context, self.pending_actions.stable_id(), &action_order)?;
        if !action_order.is_empty() {
            panel.push(self.pending_actions.stable_id());
        }
        reconcile_children(context, self.pending_panel.stable_id(), &panel)?;
        reconcile_children(
            context,
            self.page.stable_id(),
            &[
                self.heading.stable_id(),
                self.error.stable_id(),
                self.timeline_scroll.stable_id(),
                self.pending_panel.stable_id(),
                self.composer.stable_id(),
                self.composer_actions.stable_id(),
            ],
        )
    }

    fn upsert_pending_button(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        id: &str,
        label: &str,
        kind: ButtonKind,
        intent: ShellIntent,
    ) -> Result<Entity<Button>, FrameworkError> {
        if let Some(button) = self.pending_buttons.get(id).copied() {
            context.update_component(button, |button, _| {
                *button = action_button(label, kind);
            })?;
            Ok(button)
        } else {
            let button =
                context.create_detached_component(document_id, action_button(label, kind))?;
            bind_activate(
                context,
                button,
                Arc::clone(&self.sink),
                popup_pending_intent(self.window_id, intent),
            )?;
            self.pending_buttons.insert(id.to_owned(), button);
            Ok(button)
        }
    }

    fn upsert_pending_field(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        keep: &mut HashSet<String>,
        id: &str,
        placeholder: &str,
        value: &str,
        intent: impl Fn(String) -> ShellIntent + Send + Sync + 'static,
    ) -> Result<Entity<TextArea>, FrameworkError> {
        keep.insert(id.to_owned());
        if let Some(field) = self.pending_fields.get(id).copied() {
            context.update_component(field, |editor, _| {
                editor.placeholder = Arc::from(placeholder);
                if editor.state.value != value {
                    editor.state.replace_value(value.to_owned());
                }
            })?;
            Ok(field)
        } else {
            let field = context.create_detached_component(
                document_id,
                TextArea::new(value.to_owned()).height(40.0),
            )?;
            context.update_component(field, |editor, _| {
                editor.placeholder = Arc::from(placeholder);
            })?;
            let sink = Arc::clone(&self.sink);
            let window_id = self.window_id;
            context.on(field, move |_, event: &TextChanged, _| {
                emit(
                    &sink,
                    popup_pending_intent(window_id, intent(event.value.clone())),
                );
            })?;
            self.pending_fields.insert(id.to_owned(), field);
            Ok(field)
        }
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

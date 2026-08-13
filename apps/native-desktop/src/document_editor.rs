//! Native document editor surface backed by DocumentStore.
//!
//! View state owns selection/scroll/focus presentation only. Buffer text and
//! dirty/conflict facts come from the shared application DocumentStore.

use iced::keyboard::{key, Key};
use iced::widget::{button, column, container, row, text, text_editor};
use iced::{Element, Length, Padding};
use lilia_contracts::ProjectId;
use lilia_desktop_application::{
    BufferRevision, DesktopDocumentDefinitionTarget, DesktopDocumentDiagnosticsSnapshot,
    DesktopDocumentDiagnosticsState, Diagnostic, DiagnosticSeverity, DocumentId, DocumentSnapshot,
    WorkspaceItem, DOCUMENT_WORKSPACE_ITEM_KIND,
};
use nana_ui::widgets::{button_style, canvas_style};
use nana_ui::{
    ui_font, ButtonKind, EmptyState, HostedTextarea, HostedTextareaState, Icon, KeyModifiers,
    KeyStroke, ListItem, ThemeTokens,
};

pub fn select_document_editor_range(
    state: &DocumentEditorViewState,
    start_line: u32,
    start_character: u32,
    end_line: u32,
    end_character: u32,
) -> bool {
    select_hosted_textarea_range(
        &state.editor,
        start_line,
        start_character,
        end_line,
        end_character,
    )
}

pub fn select_document_editor_offsets(
    state: &DocumentEditorViewState,
    start_offset: usize,
    end_offset: usize,
) -> bool {
    let text = state.editor.text();
    let Some((start_line, start_character)) = byte_position(&text, start_offset) else {
        return false;
    };
    let Some((end_line, end_character)) = byte_position(&text, end_offset) else {
        return false;
    };
    select_hosted_textarea_range(
        &state.editor,
        start_line,
        start_character,
        end_line,
        end_character,
    )
}

pub fn document_editor_cursor_offset(state: &DocumentEditorViewState) -> Option<usize> {
    let cursor = state.editor.cursor().position;
    let text = state.editor.text();
    let mut lines = text.split('\n');
    let mut offset = 0usize;
    for line_index in 0..=cursor.line {
        let line = lines.next()?;
        if line_index == cursor.line {
            if cursor.column > line.len() || !line.is_char_boundary(cursor.column) {
                return None;
            }
            return Some(offset.saturating_add(cursor.column));
        }
        offset = offset.saturating_add(line.len()).saturating_add(1);
    }
    None
}

fn byte_position(text: &str, offset: usize) -> Option<(u32, u32)> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return None;
    }
    let prefix = &text[..offset];
    let line = u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count()).ok()?;
    let line_start = prefix
        .rfind('\n')
        .map_or(0, |index| index.saturating_add(1));
    let character = u32::try_from(offset.saturating_sub(line_start)).ok()?;
    Some((line, character))
}

fn select_hosted_textarea_range(
    editor: &HostedTextareaState,
    start_line: u32,
    start_character: u32,
    end_line: u32,
    end_character: u32,
) -> bool {
    let text = editor.text();
    let lines = text.split('\n').collect::<Vec<_>>();
    let Some(start_text) = lines.get(start_line as usize) else {
        return false;
    };
    let Some(end_text) = lines.get(end_line as usize) else {
        return false;
    };
    if (end_line, end_character) < (start_line, start_character) {
        return false;
    }
    let start_column = valid_byte_column(start_text, start_character);
    let end_column = valid_byte_column(end_text, end_character);
    editor.move_to(text_editor::Cursor {
        position: text_editor::Position {
            line: end_line as usize,
            column: end_column,
        },
        selection: ((start_line, start_column) != (end_line, end_column)).then_some(
            text_editor::Position {
                line: start_line as usize,
                column: start_column,
            },
        ),
    });
    true
}

fn valid_byte_column(line: &str, offset: u32) -> usize {
    let mut offset = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(line.len());
    while !line.is_char_boundary(offset) {
        offset = offset.saturating_sub(1);
    }
    offset
}

#[derive(Clone)]
pub struct DocumentEditorViewState {
    pub document_id: DocumentId,
    pub path_label: String,
    pub language_label: String,
    pub revision: BufferRevision,
    pub dirty: bool,
    pub read_only: bool,
    pub conflict_message: Option<String>,
    pub status_message: Option<String>,
    pub diagnostics_state: DesktopDocumentDiagnosticsState,
    pub diagnostics: Vec<Diagnostic>,
    pub definition_operation: Option<u64>,
    pub definition_project_id: Option<ProjectId>,
    pub definition_targets: Vec<DesktopDocumentDefinitionTarget>,
    pub definition_message: Option<String>,
    pub definition_error: Option<String>,
    pub editor: HostedTextareaState,
}

impl DocumentEditorViewState {
    pub fn from_snapshot(snapshot: &DocumentSnapshot) -> Self {
        Self {
            document_id: snapshot.id,
            path_label: snapshot.canonical_path.display().to_string(),
            language_label: snapshot
                .language
                .as_ref()
                .map(|language| language.as_str().to_owned())
                .unwrap_or_else(|| "plaintext".to_owned()),
            revision: snapshot.buffer.revision,
            dirty: snapshot.buffer.is_dirty(),
            read_only: snapshot.read_only,
            conflict_message: None,
            status_message: None,
            diagnostics_state: DesktopDocumentDiagnosticsState::Idle,
            diagnostics: Vec::new(),
            definition_operation: None,
            definition_project_id: None,
            definition_targets: Vec::new(),
            definition_message: None,
            definition_error: None,
            editor: HostedTextareaState::with_text(&snapshot.buffer.text),
        }
    }

    pub fn sync_from_snapshot(&mut self, snapshot: &DocumentSnapshot) {
        self.document_id = snapshot.id;
        self.path_label = snapshot.canonical_path.display().to_string();
        self.language_label = snapshot
            .language
            .as_ref()
            .map(|language| language.as_str().to_owned())
            .unwrap_or_else(|| "plaintext".to_owned());
        if self.revision != snapshot.buffer.revision {
            self.definition_targets.clear();
            self.definition_message = None;
            self.definition_error = None;
        }
        self.revision = snapshot.buffer.revision;
        self.dirty = snapshot.buffer.is_dirty();
        self.read_only = snapshot.read_only;
        if self.editor.text() != snapshot.buffer.text {
            self.editor.set_text(&snapshot.buffer.text);
        }
    }

    pub fn mark_diagnostics_checking(&mut self) {
        self.diagnostics_state = DesktopDocumentDiagnosticsState::Checking;
    }

    pub fn mark_diagnostics_unavailable(&mut self) {
        self.diagnostics_state = DesktopDocumentDiagnosticsState::Unavailable;
    }

    pub fn note_text_changed(&mut self) {
        self.diagnostics_state = DesktopDocumentDiagnosticsState::Idle;
        self.diagnostics.clear();
    }

    pub fn sync_diagnostics(&mut self, snapshot: &DesktopDocumentDiagnosticsSnapshot) {
        if snapshot.document_id != self.document_id || snapshot.buffer_revision != self.revision {
            return;
        }
        self.diagnostics_state = snapshot.state;
        self.diagnostics = snapshot.diagnostics.clone();
    }
}

pub fn is_document_editor_item(item: &WorkspaceItem) -> bool {
    item.kind.as_str() == DOCUMENT_WORKSPACE_ITEM_KIND
}

pub struct DocumentEditorActions<Message> {
    pub on_edit: Box<dyn Fn(text_editor::Action) -> Message>,
    pub on_diagnostic: Box<dyn Fn(usize) -> Message>,
    pub on_definition: Option<Message>,
    pub on_definition_target: Box<dyn Fn(usize) -> Message>,
    pub on_command_key: Box<dyn Fn(KeyStroke) -> Message>,
    pub on_save: Message,
    pub on_discard: Message,
}

pub fn document_editor_content<Message: Clone + 'static>(
    state: &DocumentEditorViewState,
    editor_id: String,
    tokens: ThemeTokens,
    actions: DocumentEditorActions<Message>,
    syntax_theme: iced::highlighter::Theme,
) -> Element<'static, Message> {
    let DocumentEditorActions {
        on_edit,
        on_diagnostic,
        on_definition,
        on_definition_target,
        on_command_key,
        on_save,
        on_discard,
    } = actions;
    let colors = tokens.colors;
    let dirty_label = if state.dirty {
        "未保存"
    } else {
        "已保存"
    };
    let language = state.language_label.clone();
    let path = state.path_label.clone();
    let mut header = column![
        text(path)
            .size(13)
            .font(ui_font(iced::font::Weight::Semibold))
            .color(colors.text),
        row![
            text(format!("语言 {language}"))
                .size(10)
                .color(colors.muted),
            text(format!("修订 {}", state.revision.get()))
                .size(10)
                .color(colors.muted),
            text(dirty_label).size(10).color(if state.dirty {
                colors.warning
            } else {
                colors.muted
            }),
        ]
        .spacing(12),
    ]
    .spacing(4)
    .width(Length::Fill);

    if let Some(conflict) = &state.conflict_message {
        header = header.push(text(conflict.clone()).size(11).color(colors.danger));
    } else if let Some(status) = &state.status_message {
        header = header.push(text(status.clone()).size(10).color(colors.success));
    }

    let mut actions = row![].spacing(8).align_y(iced::Alignment::Center);
    if !state.read_only {
        let mut save =
            button(text("保存").size(11)).style(button_style(tokens, ButtonKind::Primary));
        if state.dirty {
            save = save.on_press(on_save);
        }
        let mut discard =
            button(text("丢弃更改").size(11)).style(button_style(tokens, ButtonKind::Subtle));
        if state.dirty {
            discard = discard.on_press(on_discard);
        }
        actions = actions.push(save).push(discard);
    }

    let definition_enabled = on_definition.is_some() && state.definition_operation.is_none();
    let mut definition = button(
        text(if state.definition_operation.is_some() {
            "正在查找"
        } else {
            "转到定义"
        })
        .size(11),
    )
    .style(button_style(tokens, ButtonKind::Subtle));
    if definition_enabled {
        definition = definition.on_press(on_definition.clone().expect("definition is enabled"));
    }
    actions = actions.push(definition);

    let mut editor = HostedTextarea::new(&state.editor)
        .id(editor_id)
        .placeholder("开始编辑…")
        .height(420.0)
        .disabled(state.read_only)
        .on_action(on_edit)
        .syntax_highlighting(syntax_token(&state.language_label), syntax_theme);
    if let Some(on_definition) = on_definition {
        editor = editor.key_binding(move |key_press| {
            if let Some(stroke) = document_command_stroke(&key_press) {
                return Some(text_editor::Binding::Custom(on_command_key(stroke)));
            }
            if matches!(key_press.modified_key, Key::Named(key::Named::F12))
                && matches!(key_press.status, text_editor::Status::Focused { .. })
            {
                Some(text_editor::Binding::Custom(on_definition.clone()))
            } else {
                text_editor::Binding::from_key_press(key_press)
            }
        });
    } else {
        editor = editor.key_binding(move |key_press| {
            document_command_stroke(&key_press)
                .map(|stroke| text_editor::Binding::Custom(on_command_key(stroke)))
                .or_else(|| text_editor::Binding::from_key_press(key_press))
        });
    }
    let editor = editor.view(tokens);

    let diagnostics_summary = match state.diagnostics_state {
        DesktopDocumentDiagnosticsState::Idle => "编辑后保存以更新问题检查".to_owned(),
        DesktopDocumentDiagnosticsState::Checking => "正在检查问题…".to_owned(),
        DesktopDocumentDiagnosticsState::Ready if state.diagnostics.is_empty() => {
            "未发现问题".to_owned()
        }
        DesktopDocumentDiagnosticsState::Ready => {
            format!("发现 {} 个问题", state.diagnostics.len())
        }
        DesktopDocumentDiagnosticsState::Unavailable => "暂时无法检查问题".to_owned(),
    };
    let mut diagnostics = column![text(diagnostics_summary).size(10).color(colors.muted)]
        .spacing(5)
        .width(Length::Fill);
    for (index, diagnostic) in state.diagnostics.iter().take(4).enumerate() {
        let (label, color) = match diagnostic.severity {
            DiagnosticSeverity::Error => ("错误", colors.danger),
            DiagnosticSeverity::Warning => ("警告", colors.warning),
            DiagnosticSeverity::Information => ("信息", colors.text),
            DiagnosticSeverity::Hint => ("提示", colors.muted),
        };
        diagnostics = diagnostics.push(
            ListItem::new(
                row![
                    text(label).size(10).color(color),
                    text(diagnostic.message.clone())
                        .size(10)
                        .color(colors.text)
                        .width(Length::Fill),
                ]
                .spacing(8),
            )
            .auto_height()
            .on_select(on_diagnostic(index))
            .view(tokens),
        );
    }

    let mut definitions = column![].spacing(5).width(Length::Fill);
    if let Some(message) = &state.definition_message {
        definitions = definitions.push(text(message.clone()).size(10).color(colors.muted));
    }
    if let Some(error) = &state.definition_error {
        definitions = definitions.push(text(error.clone()).size(10).color(colors.danger));
    }
    for (index, target) in state.definition_targets.iter().take(8).enumerate() {
        definitions = definitions.push(
            ListItem::new(
                text(target.relative_path.clone())
                    .size(10)
                    .color(colors.text)
                    .width(Length::Fill),
            )
            .auto_height()
            .on_select(on_definition_target(index))
            .view(tokens),
        );
    }

    container(
        column![header, actions, editor, definitions, diagnostics]
            .spacing(10)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(Padding::from([14, 12]))
    .style(canvas_style(tokens))
    .into()
}

fn document_command_stroke(key_press: &text_editor::KeyPress) -> Option<KeyStroke> {
    let stroke = KeyStroke::from_iced(&key_press.key, key_press.modifiers)?;
    let primary = KeyModifiers::primary();
    let command = stroke.modifiers == primary && matches!(stroke.key.as_str(), "s" | "b");
    let command_palette = stroke.modifiers == primary.with_shift() && stroke.key == "p";
    (command || command_palette).then_some(stroke)
}

fn syntax_token(language: &str) -> &'static str {
    match language {
        "markdown" => "md",
        "rust" => "rs",
        "toml" => "toml",
        "json" => "json",
        "javascript" => "js",
        "typescript" => "ts",
        "tsx" => "tsx",
        "jsx" => "jsx",
        "python" => "py",
        "css" => "css",
        "html" => "html",
        "yaml" => "yaml",
        "shell" => "sh",
        _ => "txt",
    }
}

#[cfg(test)]
mod tests {
    use iced::widget::text_editor::{Action, Cursor, Edit, Position};
    use lilia_desktop_application::{
        BufferId, BufferRevision, BufferSnapshot, DocumentId, DocumentSnapshot,
    };
    use nana_ui::HostedTextareaState;

    use super::{
        byte_position, document_editor_cursor_offset, select_hosted_textarea_range,
        DocumentEditorViewState,
    };

    #[test]
    fn code_index_byte_range_selects_unicode_text_without_changing_the_buffer() {
        let editor = HostedTextareaState::with_text("one\n你好 value\nthree");
        assert!(select_hosted_textarea_range(&editor, 1, 0, 1, 6));
        assert_eq!(editor.text(), "one\n你好 value\nthree");

        editor.perform(Action::Edit(Edit::Insert('X')));
        assert_eq!(editor.text(), "one\nX value\nthree");
    }

    #[test]
    fn invalid_code_index_range_does_not_move_or_edit_the_buffer() {
        let editor = HostedTextareaState::with_text("one\ntwo");
        assert!(!select_hosted_textarea_range(&editor, 4, 0, 4, 1));
        assert_eq!(editor.text(), "one\ntwo");
    }

    #[test]
    fn diagnostic_offsets_convert_to_the_same_unicode_range_as_the_buffer() {
        let text = "one\n你好 value\nthree";
        let editor = HostedTextareaState::with_text(text);
        let (start_line, start_character) = byte_position(text, 4).unwrap();
        let (end_line, end_character) = byte_position(text, 10).unwrap();
        assert!(select_hosted_textarea_range(
            &editor,
            start_line,
            start_character,
            end_line,
            end_character,
        ));

        editor.perform(Action::Edit(Edit::Insert('X')));
        assert_eq!(editor.text(), "one\nX value\nthree");
    }

    #[test]
    fn cursor_offset_reads_the_retained_unicode_cursor_without_mutating_text() {
        let state = DocumentEditorViewState::from_snapshot(&DocumentSnapshot {
            id: DocumentId::new(1),
            canonical_path: "document.rs".into(),
            language: None,
            buffer: BufferSnapshot {
                id: BufferId::new(1),
                text: "零😀a\nsecond".to_owned(),
                revision: BufferRevision::INITIAL,
                saved_revision: BufferRevision::INITIAL,
            },
            read_only: false,
            disk_fingerprint: 0,
        });
        let offset = state.editor.text().find('a').unwrap();
        state.editor.move_to(Cursor {
            position: Position {
                line: 0,
                column: offset,
            },
            selection: None,
        });

        assert_eq!(document_editor_cursor_offset(&state), Some(offset));
        assert_eq!(state.editor.text(), "零😀a\nsecond");
    }
}

pub fn document_editor_inactive_preview<Message: 'static>(
    item: &WorkspaceItem,
    state: Option<&DocumentEditorViewState>,
    tokens: ThemeTokens,
) -> Element<'static, Message> {
    let colors = tokens.colors;
    let dirty = state.is_some_and(|state| state.dirty);
    let summary = if dirty {
        "有未保存更改 · 聚焦窗格后继续编辑"
    } else {
        "只读预览 · 聚焦窗格后可编辑"
    };
    container(
        column![
            text(item.title.clone())
                .size(15)
                .font(ui_font(iced::font::Weight::Semibold))
                .color(colors.text),
            text(summary).size(10).color(colors.muted),
            EmptyState::new("文档已打开")
                .message("聚焦此窗格后可继续编辑。")
                .icon(Icon::Workspace)
                .view(tokens),
        ]
        .spacing(8)
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(Padding::from([16, 14]))
    .style(canvas_style(tokens))
    .into()
}

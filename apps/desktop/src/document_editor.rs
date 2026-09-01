//! Native document editor surface backed by DocumentStore.
//!
//! View state owns selection/scroll/focus presentation only. Buffer text and
//! dirty/conflict facts come from the shared application DocumentStore.

use crate::application::{
    BufferRevision, DesktopDocumentDefinitionTarget, DesktopDocumentDiagnosticsSnapshot,
    DesktopDocumentDiagnosticsState, Diagnostic, DocumentId, DocumentSnapshot, WorkspaceItem,
    DOCUMENT_WORKSPACE_ITEM_KIND,
};
use crate::text_editor_state::{TextEditorCursor, TextEditorPosition, TextEditorState};
use lilia_contracts::ProjectId;

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
    editor: &TextEditorState,
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
    editor.move_to(TextEditorCursor {
        position: TextEditorPosition {
            line: end_line as usize,
            column: end_column,
        },
        selection: ((start_line, start_column) != (end_line, end_column)).then_some(
            TextEditorPosition {
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
    pub definition_job: Option<lilia_kernel::JobId>,
    pub definition_project_id: Option<ProjectId>,
    pub definition_targets: Vec<DesktopDocumentDefinitionTarget>,
    pub definition_message: Option<String>,
    pub definition_error: Option<String>,
    pub editor: TextEditorState,
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
            definition_job: None,
            definition_project_id: None,
            definition_targets: Vec::new(),
            definition_message: None,
            definition_error: None,
            editor: TextEditorState::with_text(&snapshot.buffer.text),
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

#[cfg(test)]
mod tests {
    use crate::application::{
        BufferId, BufferRevision, BufferSnapshot, DocumentId, DocumentSnapshot,
    };

    use super::{
        byte_position, document_editor_cursor_offset, select_hosted_textarea_range,
        DocumentEditorViewState,
    };
    use crate::text_editor_state::{TextEditorCursor, TextEditorPosition, TextEditorState};

    #[test]
    fn code_index_byte_range_selects_unicode_text_without_changing_the_buffer() {
        let editor = TextEditorState::with_text("one\n你好 value\nthree");
        assert!(select_hosted_textarea_range(&editor, 1, 0, 1, 6));
        assert_eq!(editor.text(), "one\n你好 value\nthree");
    }

    #[test]
    fn invalid_code_index_range_does_not_move_or_edit_the_buffer() {
        let editor = TextEditorState::with_text("one\ntwo");
        assert!(!select_hosted_textarea_range(&editor, 4, 0, 4, 1));
        assert_eq!(editor.text(), "one\ntwo");
    }

    #[test]
    fn diagnostic_offsets_convert_to_the_same_unicode_range_as_the_buffer() {
        let text = "one\n你好 value\nthree";
        let editor = TextEditorState::with_text(text);
        let (start_line, start_character) = byte_position(text, 4).unwrap();
        let (end_line, end_character) = byte_position(text, 10).unwrap();
        assert!(select_hosted_textarea_range(
            &editor,
            start_line,
            start_character,
            end_line,
            end_character,
        ));
        assert_eq!(editor.text(), text);
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
        state.editor.move_to(TextEditorCursor {
            position: TextEditorPosition {
                line: 0,
                column: offset,
            },
            selection: None,
        });

        assert_eq!(document_editor_cursor_offset(&state), Some(offset));
        assert_eq!(state.editor.text(), "零😀a\nsecond");
    }
}

use std::collections::BTreeMap;
use std::ops::Range;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BufferId(u64);

impl BufferId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BufferRevision(u64);

impl BufferRevision {
    pub const INITIAL: Self = Self(0);

    pub const fn get(self) -> u64 {
        self.0
    }

    fn next(self) -> Result<Self, BufferError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(BufferError::RevisionOverflow)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub range: Range<usize>,
    pub replacement: String,
}

impl TextEdit {
    pub fn new(range: Range<usize>, replacement: impl Into<String>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BufferSnapshot {
    pub id: BufferId,
    pub text: String,
    pub revision: BufferRevision,
    pub saved_revision: BufferRevision,
}

impl BufferSnapshot {
    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }
}

#[derive(Clone, Debug)]
pub struct TextBuffer {
    snapshot: BufferSnapshot,
}

#[derive(Clone, Debug, Default)]
pub struct BufferStore {
    next_id: u64,
    buffers: BTreeMap<BufferId, TextBuffer>,
}

impl BufferStore {
    pub fn open(&mut self, text: impl Into<String>) -> Result<BufferId, BufferError> {
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(BufferError::IdentifierOverflow)?;
        self.next_id = next_id;
        let id = BufferId::new(next_id);
        self.buffers.insert(id, TextBuffer::new(id, text));
        Ok(id)
    }

    pub fn get(&self, id: BufferId) -> Result<&TextBuffer, BufferError> {
        self.buffers.get(&id).ok_or(BufferError::NotFound(id))
    }

    pub fn get_mut(&mut self, id: BufferId) -> Result<&mut TextBuffer, BufferError> {
        self.buffers.get_mut(&id).ok_or(BufferError::NotFound(id))
    }

    pub fn close(&mut self, id: BufferId) -> Result<TextBuffer, BufferError> {
        self.buffers.remove(&id).ok_or(BufferError::NotFound(id))
    }
}

impl TextBuffer {
    pub fn new(id: BufferId, text: impl Into<String>) -> Self {
        Self {
            snapshot: BufferSnapshot {
                id,
                text: text.into(),
                revision: BufferRevision::INITIAL,
                saved_revision: BufferRevision::INITIAL,
            },
        }
    }

    pub fn snapshot(&self) -> BufferSnapshot {
        self.snapshot.clone()
    }

    pub fn text(&self) -> &str {
        &self.snapshot.text
    }

    pub fn revision(&self) -> BufferRevision {
        self.snapshot.revision
    }

    pub fn is_dirty(&self) -> bool {
        self.snapshot.is_dirty()
    }

    pub fn apply_transaction(
        &mut self,
        mut edits: Vec<TextEdit>,
    ) -> Result<BufferRevision, BufferError> {
        if edits.is_empty() {
            return Ok(self.snapshot.revision);
        }
        edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
        let mut previous_end = 0;
        for edit in &edits {
            if edit.range.start > edit.range.end || edit.range.end > self.snapshot.text.len() {
                return Err(BufferError::RangeOutOfBounds);
            }
            if !self.snapshot.text.is_char_boundary(edit.range.start)
                || !self.snapshot.text.is_char_boundary(edit.range.end)
            {
                return Err(BufferError::InvalidUtf8Boundary);
            }
            if edit.range.start < previous_end {
                return Err(BufferError::OverlappingEdits);
            }
            previous_end = edit.range.end;
        }
        let next_revision = self.snapshot.revision.next()?;
        for edit in edits.into_iter().rev() {
            self.snapshot
                .text
                .replace_range(edit.range, &edit.replacement);
        }
        self.snapshot.revision = next_revision;
        Ok(next_revision)
    }

    pub fn mark_saved(&mut self, revision: BufferRevision) -> Result<(), BufferError> {
        if revision != self.snapshot.revision {
            return Err(BufferError::StaleSave {
                expected: self.snapshot.revision,
                actual: revision,
            });
        }
        self.snapshot.saved_revision = revision;
        Ok(())
    }

    pub fn replace_from_disk(
        &mut self,
        text: impl Into<String>,
    ) -> Result<BufferRevision, BufferError> {
        if self.is_dirty() {
            return Err(BufferError::DirtyReload);
        }
        let next_revision = self.snapshot.revision.next()?;
        self.snapshot.text = text.into();
        self.snapshot.revision = next_revision;
        self.snapshot.saved_revision = next_revision;
        Ok(next_revision)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BufferError {
    #[error("buffer identifier overflowed")]
    IdentifierOverflow,
    #[error("buffer {0:?} does not exist")]
    NotFound(BufferId),
    #[error("text edit range is outside the buffer")]
    RangeOutOfBounds,
    #[error("text edit range splits a UTF-8 code point")]
    InvalidUtf8Boundary,
    #[error("text edits in one transaction overlap")]
    OverlappingEdits,
    #[error("buffer revision overflowed")]
    RevisionOverflow,
    #[error("save completed for revision {actual:?}, but current revision is {expected:?}")]
    StaleSave {
        expected: BufferRevision,
        actual: BufferRevision,
    },
    #[error("cannot replace a dirty buffer from disk")]
    DirtyReload,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_is_atomic_and_preserves_unicode_boundaries() {
        let mut buffer = TextBuffer::new(BufferId::new(1), "a中c");
        let before = buffer.snapshot();

        assert_eq!(
            buffer.apply_transaction(vec![TextEdit::new(2..3, "x")]),
            Err(BufferError::InvalidUtf8Boundary)
        );
        assert_eq!(buffer.snapshot(), before);

        let revision = buffer
            .apply_transaction(vec![TextEdit::new(0..1, "A"), TextEdit::new(4..5, "C")])
            .unwrap();
        assert_eq!(buffer.text(), "A中C");
        assert_eq!(revision.get(), 1);
        assert!(buffer.is_dirty());
    }

    #[test]
    fn stale_save_cannot_clear_newer_edits() {
        let mut buffer = TextBuffer::new(BufferId::new(1), "one");
        let first = buffer
            .apply_transaction(vec![TextEdit::new(0..3, "two")])
            .unwrap();
        buffer
            .apply_transaction(vec![TextEdit::new(0..3, "three")])
            .unwrap();

        assert!(matches!(
            buffer.mark_saved(first),
            Err(BufferError::StaleSave { .. })
        ));
        assert!(buffer.is_dirty());
    }
}

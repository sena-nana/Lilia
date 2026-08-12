use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    BufferError, BufferId, BufferRevision, BufferSnapshot, BufferStore, DesktopApplication,
    DesktopApplicationError, LanguageId, TextEdit,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentId(u64);

impl DocumentId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSnapshot {
    pub id: DocumentId,
    pub canonical_path: PathBuf,
    pub language: Option<LanguageId>,
    pub read_only: bool,
    pub buffer: BufferSnapshot,
}

#[derive(Clone, Debug)]
struct DocumentRecord {
    id: DocumentId,
    canonical_path: PathBuf,
    language: Option<LanguageId>,
    read_only: bool,
    buffer_id: BufferId,
}

#[derive(Clone, Debug, Default)]
pub struct DocumentStore {
    next_id: u64,
    by_path: BTreeMap<String, DocumentId>,
    records: BTreeMap<DocumentId, DocumentRecord>,
    buffers: BufferStore,
}

impl DocumentStore {
    pub fn open_file(
        &mut self,
        canonical_path: impl Into<PathBuf>,
        text: impl Into<String>,
        language: Option<LanguageId>,
        read_only: bool,
    ) -> Result<(DocumentSnapshot, bool), DocumentError> {
        let canonical_path = canonical_path.into();
        let key = path_key(&canonical_path)?;
        if let Some(id) = self.by_path.get(&key).copied() {
            return Ok((self.snapshot(id)?, false));
        }
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(DocumentError::IdentifierOverflow)?;
        self.next_id = next_id;
        let id = DocumentId(next_id);
        let buffer_id = self.buffers.open(text)?;
        let record = DocumentRecord {
            id,
            canonical_path,
            language,
            read_only,
            buffer_id,
        };
        self.by_path.insert(key, id);
        self.records.insert(id, record);
        Ok((self.snapshot(id)?, true))
    }

    pub fn snapshot(&self, id: DocumentId) -> Result<DocumentSnapshot, DocumentError> {
        let record = self.records.get(&id).ok_or(DocumentError::NotFound(id))?;
        Ok(DocumentSnapshot {
            id: record.id,
            canonical_path: record.canonical_path.clone(),
            language: record.language.clone(),
            read_only: record.read_only,
            buffer: self.buffers.get(record.buffer_id)?.snapshot(),
        })
    }

    pub fn apply_edits(
        &mut self,
        id: DocumentId,
        edits: Vec<TextEdit>,
    ) -> Result<BufferRevision, DocumentError> {
        let record = self.records.get(&id).ok_or(DocumentError::NotFound(id))?;
        if record.read_only {
            return Err(DocumentError::ReadOnly(id));
        }
        Ok(self
            .buffers
            .get_mut(record.buffer_id)?
            .apply_transaction(edits)?)
    }

    pub fn mark_saved(
        &mut self,
        id: DocumentId,
        revision: BufferRevision,
    ) -> Result<(), DocumentError> {
        let record = self.records.get(&id).ok_or(DocumentError::NotFound(id))?;
        self.buffers
            .get_mut(record.buffer_id)?
            .mark_saved(revision)?;
        Ok(())
    }

    pub fn close(&mut self, id: DocumentId, discard_dirty: bool) -> Result<(), DocumentError> {
        let record = self.records.get(&id).ok_or(DocumentError::NotFound(id))?;
        if self.buffers.get(record.buffer_id)?.is_dirty() && !discard_dirty {
            return Err(DocumentError::DirtyClose(id));
        }
        let record = self
            .records
            .remove(&id)
            .expect("document was checked above");
        self.by_path.remove(&path_key(&record.canonical_path)?);
        self.buffers.close(record.buffer_id)?;
        Ok(())
    }
}

impl DesktopApplication {
    pub fn open_document(
        &self,
        canonical_path: impl Into<PathBuf>,
        text: impl Into<String>,
        language: Option<LanguageId>,
        read_only: bool,
    ) -> Result<(DocumentSnapshot, bool), DesktopApplicationError> {
        let canonical_path = canonical_path.into();
        let language = match language {
            Some(language) => Some(language),
            None => self
                .inner
                .languages
                .read()
                .map_err(|_| DesktopApplicationError::StateUnavailable("language registry"))?
                .language_for_path(&canonical_path)
                .map(|definition| definition.id.clone()),
        };
        Ok(self
            .inner
            .documents
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("document store"))?
            .open_file(canonical_path, text, language, read_only)?)
    }

    pub fn document_snapshot(
        &self,
        id: DocumentId,
    ) -> Result<DocumentSnapshot, DesktopApplicationError> {
        Ok(self
            .inner
            .documents
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("document store"))?
            .snapshot(id)?)
    }

    pub fn edit_document(
        &self,
        id: DocumentId,
        edits: Vec<TextEdit>,
    ) -> Result<BufferRevision, DesktopApplicationError> {
        Ok(self
            .inner
            .documents
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("document store"))?
            .apply_edits(id, edits)?)
    }

    pub fn mark_document_saved(
        &self,
        id: DocumentId,
        revision: BufferRevision,
    ) -> Result<(), DesktopApplicationError> {
        self.inner
            .documents
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("document store"))?
            .mark_saved(id, revision)?;
        Ok(())
    }

    pub fn close_document(
        &self,
        id: DocumentId,
        discard_dirty: bool,
    ) -> Result<(), DesktopApplicationError> {
        self.inner
            .documents
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("document store"))?
            .close(id, discard_dirty)?;
        Ok(())
    }
}

fn path_key(path: &Path) -> Result<String, DocumentError> {
    if !path.is_absolute() {
        return Err(DocumentError::PathMustBeCanonical(path.to_path_buf()));
    }
    let key = path.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    let key = key.to_ascii_lowercase();
    Ok(key)
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DocumentError {
    #[error("document identifier overflowed")]
    IdentifierOverflow,
    #[error("document path must be an absolute canonical path: `{0:?}`")]
    PathMustBeCanonical(PathBuf),
    #[error("document {0:?} does not exist")]
    NotFound(DocumentId),
    #[error("document {0:?} is read-only")]
    ReadOnly(DocumentId),
    #[error("document {0:?} has unsaved changes")]
    DirtyClose(DocumentId),
    #[error(transparent)]
    Buffer(#[from] BufferError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> PathBuf {
        std::env::current_dir().unwrap().join(name)
    }

    #[test]
    fn opening_the_same_canonical_path_reuses_the_document_and_buffer() {
        let mut store = DocumentStore::default();
        let (first, created) = store
            .open_file(path("README.md"), "first", None, false)
            .unwrap();
        let (second, created_again) = store
            .open_file(path("README.md"), "ignored", None, false)
            .unwrap();

        assert!(created);
        assert!(!created_again);
        assert_eq!(first.id, second.id);
        assert_eq!(second.buffer.text, "first");
    }

    #[test]
    fn dirty_documents_require_an_explicit_discard_before_close() {
        let mut store = DocumentStore::default();
        let (document, _) = store
            .open_file(path("src/main.rs"), "fn main() {}", None, false)
            .unwrap();
        store
            .apply_edits(document.id, vec![TextEdit::new(3..7, "entry")])
            .unwrap();

        assert_eq!(
            store.close(document.id, false),
            Err(DocumentError::DirtyClose(document.id))
        );
        store.close(document.id, true).unwrap();
        assert_eq!(
            store.snapshot(document.id),
            Err(DocumentError::NotFound(document.id))
        );
    }
}

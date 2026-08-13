use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use lilia_contracts::ProjectId;
use serde::{Deserialize, Serialize};

use crate::{
    BufferError, BufferId, BufferRevision, BufferSnapshot, BufferStore, DesktopApplication,
    DesktopApplicationError, LanguageId, ProjectContext, TextEdit,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentId(u64);

impl DocumentId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

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
    pub disk_fingerprint: u64,
}

#[derive(Clone, Debug)]
struct DocumentRecord {
    id: DocumentId,
    canonical_path: PathBuf,
    language: Option<LanguageId>,
    read_only: bool,
    buffer_id: BufferId,
    disk_fingerprint: u64,
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
        let text = text.into();
        let disk_fingerprint = content_fingerprint(&text);
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
            disk_fingerprint,
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
            disk_fingerprint: record.disk_fingerprint,
        })
    }

    pub fn find_by_path(&self, canonical_path: &Path) -> Result<Option<DocumentId>, DocumentError> {
        let key = path_key(canonical_path)?;
        Ok(self.by_path.get(&key).copied())
    }

    pub fn apply_edits(
        &mut self,
        id: DocumentId,
        expected_revision: BufferRevision,
        edits: Vec<TextEdit>,
    ) -> Result<BufferRevision, DocumentError> {
        let record = self.records.get(&id).ok_or(DocumentError::NotFound(id))?;
        if record.read_only {
            return Err(DocumentError::ReadOnly(id));
        }
        Ok(self
            .buffers
            .get_mut(record.buffer_id)?
            .apply_transaction_at(expected_revision, edits)?)
    }

    pub fn replace_text(
        &mut self,
        id: DocumentId,
        expected_revision: BufferRevision,
        text: String,
    ) -> Result<BufferRevision, DocumentError> {
        let record = self.records.get(&id).ok_or(DocumentError::NotFound(id))?;
        if record.read_only {
            return Err(DocumentError::ReadOnly(id));
        }
        let buffer = self.buffers.get(record.buffer_id)?;
        if buffer.revision() != expected_revision {
            return Err(BufferError::RevisionMismatch {
                expected: expected_revision,
                actual: buffer.revision(),
            }
            .into());
        }
        if buffer.text() == text {
            return Ok(expected_revision);
        }
        let previous_len = buffer.text().len();
        Ok(self
            .buffers
            .get_mut(record.buffer_id)?
            .apply_transaction_at(
                expected_revision,
                vec![TextEdit::new(0..previous_len, text)],
            )?)
    }

    pub fn mark_saved(
        &mut self,
        id: DocumentId,
        revision: BufferRevision,
        disk_fingerprint: u64,
    ) -> Result<(), DocumentError> {
        let record = self
            .records
            .get_mut(&id)
            .ok_or(DocumentError::NotFound(id))?;
        self.buffers
            .get_mut(record.buffer_id)?
            .mark_saved(revision)?;
        record.disk_fingerprint = disk_fingerprint;
        Ok(())
    }

    pub fn prepare_save(
        &self,
        id: DocumentId,
        expected_revision: BufferRevision,
        disk_text: &str,
    ) -> Result<DocumentSavePlan, DocumentError> {
        let snapshot = self.snapshot(id)?;
        if snapshot.read_only {
            return Err(DocumentError::ReadOnly(id));
        }
        if snapshot.buffer.revision != expected_revision {
            return Err(DocumentError::SaveConflict {
                id,
                expected_revision,
                current_revision: snapshot.buffer.revision,
                disk_changed: content_fingerprint(disk_text) != snapshot.disk_fingerprint,
            });
        }
        let disk_changed = content_fingerprint(disk_text) != snapshot.disk_fingerprint;
        if disk_changed {
            return Err(DocumentError::SaveConflict {
                id,
                expected_revision,
                current_revision: snapshot.buffer.revision,
                disk_changed: true,
            });
        }
        Ok(DocumentSavePlan {
            id,
            path: snapshot.canonical_path,
            revision: snapshot.buffer.revision,
            text: snapshot.buffer.text,
        })
    }

    pub fn force_reload(
        &mut self,
        id: DocumentId,
        text: impl Into<String>,
    ) -> Result<DocumentSnapshot, DocumentError> {
        let text = text.into();
        let fingerprint = content_fingerprint(&text);
        let record = self
            .records
            .get_mut(&id)
            .ok_or(DocumentError::NotFound(id))?;
        self.buffers.get_mut(record.buffer_id)?.force_reload(text)?;
        record.disk_fingerprint = fingerprint;
        self.snapshot(id)
    }

    pub fn reload_from_disk_text(
        &mut self,
        id: DocumentId,
        text: impl Into<String>,
    ) -> Result<DocumentSnapshot, DocumentError> {
        let text = text.into();
        let fingerprint = content_fingerprint(&text);
        let record = self
            .records
            .get_mut(&id)
            .ok_or(DocumentError::NotFound(id))?;
        self.buffers
            .get_mut(record.buffer_id)?
            .replace_from_disk(text)?;
        record.disk_fingerprint = fingerprint;
        self.snapshot(id)
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentSavePlan {
    pub id: DocumentId,
    pub path: PathBuf,
    pub revision: BufferRevision,
    pub text: String,
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

    pub fn open_document_at_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<(DocumentSnapshot, bool), DesktopApplicationError> {
        let canonical_path = canonicalize_existing_file(path.as_ref())?;
        {
            let store = self
                .inner
                .documents
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("document store"))?;
            if let Some(existing) = store.find_by_path(&canonical_path)? {
                return Ok((store.snapshot(existing)?, false));
            }
        }
        let text = fs::read_to_string(&canonical_path).map_err(|error| DocumentError::Io {
            path: canonical_path.clone(),
            message: error.to_string(),
        })?;
        self.open_document(canonical_path, text, None, false)
    }

    pub fn open_project_document(
        &self,
        project_id: &ProjectId,
        relative_path: impl AsRef<Path>,
    ) -> Result<(DocumentSnapshot, bool), DesktopApplicationError> {
        let context = self.project_context(project_id)?;
        let resolved = context.resolve_relative(relative_path.as_ref())?;
        self.open_document_at_path(resolved)
    }

    pub fn open_document_in_context(
        &self,
        context: &ProjectContext,
        relative_path: impl AsRef<Path>,
    ) -> Result<(DocumentSnapshot, bool), DesktopApplicationError> {
        let resolved = context.resolve_relative(relative_path.as_ref())?;
        self.open_document_at_path(resolved)
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
        expected_revision: BufferRevision,
        edits: Vec<TextEdit>,
    ) -> Result<BufferRevision, DesktopApplicationError> {
        let revision = self
            .inner
            .documents
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("document store"))?
            .apply_edits(id, expected_revision, edits)?;
        if let Err(error) = self.notify_document_language_service_changed(id) {
            eprintln!("failed to synchronize language document change: {error}");
        }
        Ok(revision)
    }

    pub fn replace_document_text(
        &self,
        id: DocumentId,
        expected_revision: BufferRevision,
        text: impl Into<String>,
    ) -> Result<BufferRevision, DesktopApplicationError> {
        let revision = self
            .inner
            .documents
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("document store"))?
            .replace_text(id, expected_revision, text.into())?;
        if revision != expected_revision {
            if let Err(error) = self.notify_document_language_service_changed(id) {
                eprintln!("failed to synchronize language document change: {error}");
            }
        }
        Ok(revision)
    }

    pub fn save_document(
        &self,
        id: DocumentId,
        expected_revision: BufferRevision,
    ) -> Result<DocumentSnapshot, DesktopApplicationError> {
        let mut documents = self
            .inner
            .documents
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("document store"))?;
        let snapshot = documents.snapshot(id)?;
        let disk_text = read_document_disk_text(&snapshot.canonical_path)?;
        let plan = documents.prepare_save(id, expected_revision, &disk_text)?;
        let fingerprint = content_fingerprint(&plan.text);
        let staged = stage_document_replacement(&plan.path, plan.text.as_bytes())?;
        let latest_disk_text = read_document_disk_text(&plan.path)?;
        documents.prepare_save(id, expected_revision, &latest_disk_text)?;
        persist_document_replacement(staged, &plan.path)?;
        documents.mark_saved(id, plan.revision, fingerprint)?;
        let snapshot = documents.snapshot(id)?;
        drop(documents);
        if let Err(error) = self.notify_document_language_service_saved(id) {
            eprintln!("failed to synchronize language document save: {error}");
        }
        Ok(snapshot)
    }

    pub fn discard_document_changes(
        &self,
        id: DocumentId,
    ) -> Result<DocumentSnapshot, DesktopApplicationError> {
        let path = self.document_snapshot(id)?.canonical_path;
        let text = fs::read_to_string(&path).map_err(|error| DocumentError::Io {
            path: path.clone(),
            message: error.to_string(),
        })?;
        let snapshot = self
            .inner
            .documents
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("document store"))?
            .force_reload(id, text)?;
        if let Err(error) = self.notify_document_language_service_changed(id) {
            eprintln!("failed to synchronize reloaded language document: {error}");
        }
        Ok(snapshot)
    }

    pub fn mark_document_saved(
        &self,
        id: DocumentId,
        revision: BufferRevision,
    ) -> Result<(), DesktopApplicationError> {
        let fingerprint = content_fingerprint(&self.document_snapshot(id)?.buffer.text);
        self.inner
            .documents
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("document store"))?
            .mark_saved(id, revision, fingerprint)?;
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
        if let Err(error) = self.close_document_language_service(id) {
            eprintln!("failed to close language document: {error}");
        }
        Ok(())
    }
}

pub fn document_resource_key(path: &Path) -> Result<String, DocumentError> {
    Ok(format!("document:{}", path_key(path)?))
}

pub fn path_from_document_resource_key(resource_id: &str) -> Result<PathBuf, DocumentError> {
    let Some(raw) = resource_id.strip_prefix("document:") else {
        return Err(DocumentError::InvalidResourceId(resource_id.to_owned()));
    };
    if raw.is_empty() {
        return Err(DocumentError::InvalidResourceId(resource_id.to_owned()));
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(DocumentError::PathMustBeCanonical(path));
    }
    Ok(path)
}

pub(crate) fn path_key(path: &Path) -> Result<String, DocumentError> {
    if !path.is_absolute() {
        return Err(DocumentError::PathMustBeCanonical(path.to_path_buf()));
    }
    let key = path.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    let key = key.to_ascii_lowercase();
    Ok(key)
}

fn content_fingerprint(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn read_document_disk_text(path: &Path) -> Result<String, DocumentError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(document_io_error(path, error)),
    }
}

fn stage_document_replacement(
    path: &Path,
    contents: &[u8],
) -> Result<tempfile::NamedTempFile, DocumentError> {
    let parent = path.parent().ok_or_else(|| DocumentError::Io {
        path: path.to_path_buf(),
        message: "document has no parent directory".to_owned(),
    })?;
    let mut staged = tempfile::Builder::new()
        .prefix(".lilia-save-")
        .tempfile_in(parent)
        .map_err(|error| document_io_error(path, error))?;
    staged
        .write_all(contents)
        .and_then(|()| staged.flush())
        .map_err(|error| document_io_error(path, error))?;
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(staged.path(), metadata.permissions())
            .map_err(|error| document_io_error(path, error))?;
    }
    staged
        .as_file()
        .sync_all()
        .map_err(|error| document_io_error(path, error))?;
    Ok(staged)
}

fn persist_document_replacement(
    staged: tempfile::NamedTempFile,
    path: &Path,
) -> Result<(), DocumentError> {
    staged
        .persist(path)
        .map(|_| ())
        .map_err(|error| document_io_error(path, error.error))
}

fn document_io_error(path: &Path, error: io::Error) -> DocumentError {
    DocumentError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

fn canonicalize_existing_file(path: &Path) -> Result<PathBuf, DocumentError> {
    let canonical = fs::canonicalize(path).map_err(|error| DocumentError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if !canonical.is_file() {
        return Err(DocumentError::NotAFile(canonical));
    }
    Ok(canonical)
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DocumentError {
    #[error("document identifier overflowed")]
    IdentifierOverflow,
    #[error("document path must be an absolute canonical path: `{0:?}`")]
    PathMustBeCanonical(PathBuf),
    #[error("document path is not a file: `{0:?}`")]
    NotAFile(PathBuf),
    #[error("document {0:?} does not exist")]
    NotFound(DocumentId),
    #[error("document {0:?} is read-only")]
    ReadOnly(DocumentId),
    #[error("document {0:?} has unsaved changes")]
    DirtyClose(DocumentId),
    #[error(
        "document {id:?} save conflict: expected revision {expected_revision:?}, current {current_revision:?}, disk_changed={disk_changed}"
    )]
    SaveConflict {
        id: DocumentId,
        expected_revision: BufferRevision,
        current_revision: BufferRevision,
        disk_changed: bool,
    },
    #[error("invalid document resource id `{0}`")]
    InvalidResourceId(String),
    #[error("document io failed for `{path:?}`: {message}")]
    Io { path: PathBuf, message: String },
    #[error(transparent)]
    Buffer(#[from] BufferError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn path(name: &str) -> PathBuf {
        std::env::current_dir().unwrap().join(name)
    }

    fn temporary_file(name: &str, contents: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("lilia-document-{name}-{stamp}.txt"));
        fs::write(&path, contents).unwrap();
        fs::canonicalize(&path).unwrap()
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
            .apply_edits(
                document.id,
                document.buffer.revision,
                vec![TextEdit::new(3..7, "entry")],
            )
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

    #[test]
    fn edit_requires_matching_revision() {
        let mut store = DocumentStore::default();
        let (document, _) = store
            .open_file(path("notes.txt"), "alpha", None, false)
            .unwrap();
        let first = store
            .apply_edits(
                document.id,
                document.buffer.revision,
                vec![TextEdit::new(0..5, "beta")],
            )
            .unwrap();
        assert!(matches!(
            store.apply_edits(
                document.id,
                document.buffer.revision,
                vec![TextEdit::new(0..4, "gamma")]
            ),
            Err(DocumentError::Buffer(BufferError::RevisionMismatch { .. }))
        ));
        store
            .apply_edits(document.id, first, vec![TextEdit::new(0..4, "gamma")])
            .unwrap();
        assert_eq!(store.snapshot(document.id).unwrap().buffer.text, "gamma");
    }

    #[test]
    fn replacing_with_identical_text_preserves_revision_and_clean_state() {
        let mut store = DocumentStore::default();
        let (document, _) = store
            .open_file(path("same.txt"), "unchanged", None, false)
            .unwrap();

        let revision = store
            .replace_text(
                document.id,
                document.buffer.revision,
                "unchanged".to_owned(),
            )
            .unwrap();
        let snapshot = store.snapshot(document.id).unwrap();
        assert_eq!(revision, document.buffer.revision);
        assert_eq!(snapshot.buffer.revision, document.buffer.revision);
        assert!(!snapshot.buffer.is_dirty());
    }

    #[test]
    fn save_detects_external_disk_changes() {
        let path = temporary_file("conflict", "one");
        let mut store = DocumentStore::default();
        let (document, _) = store.open_file(path.clone(), "one", None, false).unwrap();
        let revision = store
            .apply_edits(
                document.id,
                document.buffer.revision,
                vec![TextEdit::new(0..3, "two")],
            )
            .unwrap();
        fs::write(&path, "external").unwrap();
        assert!(matches!(
            store.prepare_save(document.id, revision, "external"),
            Err(DocumentError::SaveConflict {
                disk_changed: true,
                ..
            })
        ));
        let _ = fs::remove_file(path);
    }
}

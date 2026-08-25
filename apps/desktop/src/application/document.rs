use std::fs;
use std::path::{Path, PathBuf};

use lilia_contracts::ProjectId;
use lilia_feature_document::{
    canonicalize_existing_file, content_fingerprint, persist_document_replacement,
    read_document_disk_text, stage_document_replacement,
};

use crate::application::{
    BufferRevision, DesktopApplication, DesktopApplicationError, LanguageDefinition, LanguageId,
    ProjectContext, TextEdit,
};

pub use lilia_feature_document::{
    document_resource_key, path_from_document_resource_key, DocumentError, DocumentId,
    DocumentSavePlan, DocumentSnapshot, DocumentStore,
};
impl DesktopApplication {
    pub fn register_language(
        &self,
        definition: LanguageDefinition,
    ) -> Result<(), DesktopApplicationError> {
        self.inner
            .languages
            .write()
            .map_err(|_| DesktopApplicationError::StateUnavailable("language registry"))?
            .register(definition)?;
        Ok(())
    }

    pub fn language_for_path(
        &self,
        path: &Path,
    ) -> Result<Option<LanguageDefinition>, DesktopApplicationError> {
        Ok(self
            .inner
            .languages
            .read()
            .map_err(|_| DesktopApplicationError::StateUnavailable("language registry"))?
            .language_for_path(path)
            .cloned())
    }

    pub fn project_context(
        &self,
        project_id: &ProjectId,
    ) -> Result<ProjectContext, DesktopApplicationError> {
        Ok(ProjectContext::from_project(
            &self.get_project(project_id)?,
        )?)
    }

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

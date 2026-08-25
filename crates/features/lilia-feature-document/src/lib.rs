//! Document domain feature.
//!
//! Owns the editable side of a workspace: text buffers with revisions, the
//! language registry that classifies a path, the project roots a relative path
//! resolves against, and the document store that binds a canonical path to a
//! buffer plus its on-disk fingerprint.

mod buffer;
mod document;
mod jobs;
mod language;
mod project;

use std::sync::{Arc, Mutex, RwLock};

use lilia_kernel::{
    Feature, FeatureContext, FeatureId, JobProtocol, KernelError, ServiceKey, ServiceRef,
};

pub use buffer::{
    BufferError, BufferId, BufferRevision, BufferSnapshot, BufferStore, TextBuffer, TextEdit,
};
pub use document::{
    canonicalize_existing_file, content_fingerprint, document_resource_key, path_key,
    path_from_document_resource_key, persist_document_replacement, read_document_disk_text,
    stage_document_replacement, DocumentError, DocumentId, DocumentSavePlan, DocumentSnapshot,
    DocumentStore,
};
pub use jobs::{
    definition_slot, diagnostics_slot, DefinitionRequest, DiagnosticsRequest, LanguagePort,
    DEFINITION_PROTOCOL, DIAGNOSTICS_PROTOCOL,
};
pub use language::{LanguageDefinition, LanguageId, LanguageRegistry, LanguageRegistryError};
pub use project::{ProjectContext, ProjectContextError};

/// Open documents and the buffers behind them.
pub type SharedDocumentStore = Arc<Mutex<DocumentStore>>;

/// Language definitions the editor resolves paths against.
pub type SharedLanguageRegistry = Arc<RwLock<LanguageRegistry>>;

/// Service slot for the document store.
pub enum DocumentStoreKey {}

impl ServiceKey for DocumentStoreKey {
    type Value = SharedDocumentStore;

    const NAME: &'static str = "lilia.document.store";
}

/// Service slot for the language registry.
pub enum LanguageRegistryKey {}

impl ServiceKey for LanguageRegistryKey {
    type Value = SharedLanguageRegistry;

    const NAME: &'static str = "lilia.document.languages";
}

pub struct DocumentFeature {
    documents: SharedDocumentStore,
    languages: SharedLanguageRegistry,
    language_port: Arc<dyn LanguagePort>,
}

impl DocumentFeature {
    pub fn new(
        documents: SharedDocumentStore,
        languages: SharedLanguageRegistry,
        language_port: Arc<dyn LanguagePort>,
    ) -> Self {
        Self {
            documents,
            languages,
            language_port,
        }
    }
}

impl Feature for DocumentFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.document").expect("the document feature id is not blank")
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        jobs::language_protocols(Arc::clone(&self.language_port))
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![
            ServiceRef::of::<DocumentStoreKey>(),
            ServiceRef::of::<LanguageRegistryKey>(),
        ]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        cx.provide::<DocumentStoreKey>(self.documents.clone())?;
        cx.provide::<LanguageRegistryKey>(self.languages.clone())
    }
}

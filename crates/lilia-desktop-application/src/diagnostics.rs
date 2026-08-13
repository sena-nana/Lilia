//! Reserved application boundary for language diagnostics.
//!
//! Editor views may consume typed diagnostics from this store. Language server
//! processes and transport remain outside the view layer and are not implemented
//! here yet.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::DocumentId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub start_offset: usize,
    pub end_offset: usize,
    pub source: Option<String>,
    pub code: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct DiagnosticStore {
    by_document: BTreeMap<DocumentId, Vec<Diagnostic>>,
}

impl DiagnosticStore {
    pub fn diagnostics_for(&self, document_id: DocumentId) -> &[Diagnostic] {
        self.by_document
            .get(&document_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn replace(
        &mut self,
        document_id: DocumentId,
        diagnostics: impl IntoIterator<Item = Diagnostic>,
    ) {
        let diagnostics = diagnostics.into_iter().collect::<Vec<_>>();
        if diagnostics.is_empty() {
            self.by_document.remove(&document_id);
        } else {
            self.by_document.insert(document_id, diagnostics);
        }
    }

    pub fn clear(&mut self, document_id: DocumentId) {
        self.by_document.remove(&document_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_replaces_and_clears_diagnostics_per_document() {
        let mut store = DiagnosticStore::default();
        let document = DocumentId::new(7);
        store.replace(
            document,
            [Diagnostic {
                message: "unused".into(),
                severity: DiagnosticSeverity::Warning,
                start_offset: 0,
                end_offset: 4,
                source: Some("preview".into()),
                code: None,
            }],
        );
        assert_eq!(store.diagnostics_for(document).len(), 1);
        store.clear(document);
        assert!(store.diagnostics_for(document).is_empty());
    }
}

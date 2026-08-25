//! Kernel job lanes for the two language-server queries an editor issues.
//!
//! Both were bare threads in the shell. Diagnostics had no de-duplication at
//! all, so two refreshes of the same document raced and the slower reply won;
//! definition lookups shared one `document_definition_operation_sequence`
//! across every open editor. A slot per document and a slot per editor state
//! both intents directly.

use std::sync::Arc;

use lilia_kernel::{JobContext, JobProtocol, JobSlot};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{BufferRevision, DocumentId};

pub const DIAGNOSTICS_PROTOCOL: &str = "lilia.document/diagnostics@1";
pub const DEFINITION_PROTOCOL: &str = "lilia.document/definition@1";

/// Payload of [`DIAGNOSTICS_PROTOCOL`]. A document outside a project workspace
/// has no project id and is checked standalone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsRequest {
    pub document_id: DocumentId,
    pub project_id: Option<String>,
}

/// Payload of [`DEFINITION_PROTOCOL`].
///
/// The revision travels with the request so the surface can tell a stale answer
/// from a current one: the user may edit while the language server thinks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionRequest {
    pub document_id: DocumentId,
    pub revision: BufferRevision,
    pub source_offset: usize,
}

/// Answers the two queries against the host's language service.
pub trait LanguagePort: Send + Sync + 'static {
    fn diagnostics(&self, request: DiagnosticsRequest) -> Result<Value, String>;
    /// Resolves the document's project first, so the reply carries both the
    /// project the targets belong to and the targets themselves.
    fn definitions(&self, request: DefinitionRequest) -> Result<Value, String>;
}

/// One lane per document: checking one file must not cancel the check running
/// on another, but a second check of the same file replaces the first.
pub fn diagnostics_slot(document: DocumentId) -> JobSlot {
    JobSlot::new(format!("lilia.document.diagnostics.{}", document.get()))
        .expect("the diagnostics slot name is not blank")
}

/// One lane per editor. Two editors can show the same document and look up
/// definitions independently, so the editor — not the document — is the
/// discriminator.
pub fn definition_slot(editor: &str) -> JobSlot {
    JobSlot::new(format!("lilia.document.definition.{editor}"))
        .expect("the definition slot name is not blank")
}

pub(crate) fn language_protocols(port: Arc<dyn LanguagePort>) -> Vec<JobProtocol> {
    let diagnostics = Arc::clone(&port);
    vec![
        JobProtocol::new(
            DIAGNOSTICS_PROTOCOL,
            Arc::new(move |payload, _context: &JobContext| {
                run_diagnostics_job(payload, diagnostics.as_ref())
            }),
        ),
        JobProtocol::new(
            DEFINITION_PROTOCOL,
            Arc::new(move |payload, _context: &JobContext| {
                run_definition_job(payload, port.as_ref())
            }),
        ),
    ]
}

fn run_diagnostics_job(payload: Value, port: &dyn LanguagePort) -> Result<Value, String> {
    let request: DiagnosticsRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid diagnostics request: {error}"))?;
    port.diagnostics(request)
}

fn run_definition_job(payload: Value, port: &dyn LanguagePort) -> Result<Value, String> {
    let request: DefinitionRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid definition request: {error}"))?;
    port.definitions(request)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        diagnostics: Mutex<Vec<DiagnosticsRequest>>,
        definitions: Mutex<Vec<DefinitionRequest>>,
        failure: Option<String>,
    }

    impl LanguagePort for RecordingPort {
        fn diagnostics(&self, request: DiagnosticsRequest) -> Result<Value, String> {
            self.diagnostics.lock().unwrap().push(request);
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(serde_json::json!({ "diagnostics": [] })),
            }
        }

        fn definitions(&self, request: DefinitionRequest) -> Result<Value, String> {
            self.definitions.lock().unwrap().push(request);
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(serde_json::json!({ "targets": [] })),
            }
        }
    }

    fn diagnostics_request() -> DiagnosticsRequest {
        DiagnosticsRequest {
            document_id: DocumentId::new(7),
            project_id: Some("project-1".to_owned()),
        }
    }

    fn definition_request() -> DefinitionRequest {
        DefinitionRequest {
            document_id: DocumentId::new(7),
            revision: BufferRevision::INITIAL,
            source_offset: 340,
        }
    }

    #[test]
    fn the_diagnostics_job_forwards_the_document_and_its_project() {
        let port = RecordingPort::default();

        run_diagnostics_job(serde_json::to_value(diagnostics_request()).unwrap(), &port).unwrap();

        assert_eq!(
            port.diagnostics.lock().unwrap().as_slice(),
            [diagnostics_request()]
        );
    }

    #[test]
    fn the_definition_job_carries_the_revision_the_cursor_was_on() {
        let port = RecordingPort::default();

        run_definition_job(serde_json::to_value(definition_request()).unwrap(), &port).unwrap();

        assert_eq!(
            port.definitions.lock().unwrap().as_slice(),
            [definition_request()]
        );
    }

    #[test]
    fn a_language_server_failure_fails_the_job_with_its_message() {
        let port = RecordingPort {
            failure: Some("语言服务未就绪".to_owned()),
            ..RecordingPort::default()
        };

        let error = run_definition_job(serde_json::to_value(definition_request()).unwrap(), &port)
            .expect_err("an unavailable language server fails the job");

        assert_eq!(error, "语言服务未就绪");
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_diagnostics_job(
            serde_json::json!({ "documentId": "seven" }),
            &RecordingPort::default(),
        )
        .expect_err("a malformed request cannot run");

        assert!(error.contains("invalid diagnostics request"), "{error}");
    }

    #[test]
    fn each_document_checks_in_its_own_lane() {
        assert_ne!(
            diagnostics_slot(DocumentId::new(1)),
            diagnostics_slot(DocumentId::new(2))
        );
    }

    #[test]
    fn two_editors_on_one_document_look_up_definitions_independently() {
        assert_ne!(definition_slot("editor-a"), definition_slot("editor-b"));
    }
}

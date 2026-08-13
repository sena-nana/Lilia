use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use lilia_contracts::ProjectId;
use mutsuki_agent_contracts::{LspDiagnostic, LspDocumentId, LspPosition};
use serde::{Deserialize, Serialize};

use crate::{
    BufferRevision, DesktopApplication, DesktopApplicationError, Diagnostic, DiagnosticSeverity,
    DiagnosticStore, DocumentId,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopDocumentDiagnosticsState {
    #[default]
    Idle,
    Checking,
    Ready,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopDocumentDiagnosticsSnapshot {
    pub document_id: DocumentId,
    pub buffer_revision: BufferRevision,
    pub state: DesktopDocumentDiagnosticsState,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopDocumentDefinitionTarget {
    pub document_id: DocumentId,
    pub canonical_path: PathBuf,
    pub relative_path: String,
    pub start_offset: usize,
    pub end_offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopDocumentDefinitionResult {
    pub source_document_id: DocumentId,
    pub source_revision: BufferRevision,
    pub targets: Vec<DesktopDocumentDefinitionTarget>,
}

#[derive(Clone)]
struct DocumentLanguageBinding {
    document: LspDocumentId,
    version: i64,
}

#[derive(Default)]
pub(crate) struct DesktopLanguageServiceState {
    bindings: BTreeMap<DocumentId, DocumentLanguageBinding>,
    diagnostics: DiagnosticStore,
    states: BTreeMap<DocumentId, DesktopDocumentDiagnosticsState>,
}

impl DesktopLanguageServiceState {
    fn snapshot(
        &self,
        document_id: DocumentId,
        buffer_revision: BufferRevision,
    ) -> DesktopDocumentDiagnosticsSnapshot {
        DesktopDocumentDiagnosticsSnapshot {
            document_id,
            buffer_revision,
            state: self.states.get(&document_id).copied().unwrap_or_default(),
            diagnostics: self.diagnostics.diagnostics_for(document_id).to_vec(),
        }
    }
}

impl DesktopApplication {
    pub fn document_project_id(
        &self,
        document_id: DocumentId,
    ) -> Result<Option<ProjectId>, DesktopApplicationError> {
        let document = self.document_snapshot(document_id)?;
        let roots = self
            .query_projects(crate::ProjectQuery::default())?
            .into_iter()
            .filter_map(|project| {
                let context = self.project_context(&project.id).ok()?;
                let root = fs::canonicalize(context.active_root()).ok()?;
                Some((project.id, root))
            })
            .collect::<Vec<_>>();
        Ok(project_for_path(&document.canonical_path, roots))
    }

    pub fn document_diagnostics(
        &self,
        document_id: DocumentId,
    ) -> Result<DesktopDocumentDiagnosticsSnapshot, DesktopApplicationError> {
        let snapshot = self.document_snapshot(document_id)?;
        Ok(self
            .inner
            .language_services
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("language services"))?
            .snapshot(document_id, snapshot.buffer.revision))
    }

    pub fn refresh_project_document_diagnostics(
        &self,
        project_id: &ProjectId,
        document_id: DocumentId,
    ) -> Result<DesktopDocumentDiagnosticsSnapshot, DesktopApplicationError> {
        self.ensure_project_document_language_binding(project_id, document_id)?;
        self.refresh_document_diagnostics(document_id)
    }

    pub fn project_document_definitions(
        &self,
        project_id: &ProjectId,
        document_id: DocumentId,
        expected_revision: BufferRevision,
        source_offset: usize,
    ) -> Result<DesktopDocumentDefinitionResult, DesktopApplicationError> {
        let source = self.document_snapshot(document_id)?;
        if source.buffer.revision != expected_revision {
            return Err(DesktopApplicationError::InvalidInput {
                field: "buffer_revision",
                message: format!(
                    "definition expected revision {}, actual {}",
                    expected_revision.get(),
                    source.buffer.revision.get()
                ),
            });
        }
        if source_offset > source.buffer.text.len()
            || !source.buffer.text.is_char_boundary(source_offset)
        {
            return Err(DesktopApplicationError::InvalidInput {
                field: "source_offset",
                message: "definition offset must be a UTF-8 boundary inside the document".into(),
            });
        }
        let binding = self.ensure_project_document_language_binding(project_id, document_id)?;
        let locations = {
            let _operation = self.inner.language_service_operations.lock().map_err(|_| {
                DesktopApplicationError::StateUnavailable("language service operation")
            })?;
            self.authority()
                .shared_runtime()
                .inner()
                .shared_lsp_definition(
                    &binding.document,
                    lsp_position_for_offset(&source.buffer.text, source_offset),
                )
                .map_err(language_service_error)?
        };
        let current = self.document_snapshot(document_id)?;
        if current.buffer.revision != source.buffer.revision {
            return Err(DesktopApplicationError::InvalidInput {
                field: "buffer_revision",
                message: format!(
                    "document changed while resolving definition: expected {}, actual {}",
                    source.buffer.revision.get(),
                    current.buffer.revision.get()
                ),
            });
        }

        let context = self.project_context(project_id)?;
        let root = fs::canonicalize(context.active_root()).map_err(|error| {
            DesktopApplicationError::Agent(format!(
                "cannot canonicalize definition workspace root: {error}"
            ))
        })?;
        let mut seen = BTreeSet::new();
        let mut targets = Vec::new();
        for location in locations.into_iter().take(64) {
            let Some(path) = workspace_file_from_uri(&root, &location.uri) else {
                continue;
            };
            let relative_path = path
                .strip_prefix(&root)
                .expect("workspace URI was fenced to the canonical root")
                .to_string_lossy()
                .replace('\\', "/");
            let (target, _) = self.open_document_at_path(&path)?;
            let start_offset = offset_for_lsp_position(&target.buffer.text, location.range.start);
            let end_offset =
                offset_for_lsp_position(&target.buffer.text, location.range.end).max(start_offset);
            if seen.insert((target.id, start_offset, end_offset)) {
                targets.push(DesktopDocumentDefinitionTarget {
                    document_id: target.id,
                    canonical_path: target.canonical_path,
                    relative_path,
                    start_offset,
                    end_offset,
                });
            }
        }
        Ok(DesktopDocumentDefinitionResult {
            source_document_id: document_id,
            source_revision: expected_revision,
            targets,
        })
    }

    pub fn refresh_document_diagnostics(
        &self,
        document_id: DocumentId,
    ) -> Result<DesktopDocumentDiagnosticsSnapshot, DesktopApplicationError> {
        let _operation =
            self.inner.language_service_operations.lock().map_err(|_| {
                DesktopApplicationError::StateUnavailable("language service operation")
            })?;
        let requested = self.document_snapshot(document_id)?;
        let binding = {
            let mut state = self
                .inner
                .language_services
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("language services"))?;
            let Some(binding) = state.bindings.get(&document_id).cloned() else {
                state
                    .states
                    .insert(document_id, DesktopDocumentDiagnosticsState::Unavailable);
                return Err(DesktopApplicationError::InvalidInput {
                    field: "document_id",
                    message: "document is not attached to a language service".to_owned(),
                });
            };
            state
                .states
                .insert(document_id, DesktopDocumentDiagnosticsState::Checking);
            binding
        };
        let diagnostics = match self
            .authority()
            .shared_runtime()
            .inner()
            .shared_lsp_document_diagnostics(&binding.document)
        {
            Ok(diagnostics) => diagnostics,
            Err(error) => {
                self.mark_document_diagnostics_unavailable(document_id)?;
                return Err(language_service_error(error));
            }
        };
        let current = self.document_snapshot(document_id)?;
        let mut state = self
            .inner
            .language_services
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("language services"))?;
        if current.buffer.revision == requested.buffer.revision {
            state.diagnostics.replace(
                document_id,
                diagnostics
                    .into_iter()
                    .map(|diagnostic| map_lsp_diagnostic(&requested.buffer.text, diagnostic)),
            );
            state
                .states
                .insert(document_id, DesktopDocumentDiagnosticsState::Ready);
        }
        Ok(state.snapshot(document_id, current.buffer.revision))
    }

    pub(crate) fn notify_document_language_service_changed(
        &self,
        document_id: DocumentId,
    ) -> Result<(), DesktopApplicationError> {
        let _operation =
            self.inner.language_service_operations.lock().map_err(|_| {
                DesktopApplicationError::StateUnavailable("language service operation")
            })?;
        let snapshot = self.document_snapshot(document_id)?;
        let version = lsp_version(snapshot.buffer.revision)?;
        let binding = {
            let state = self
                .inner
                .language_services
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("language services"))?;
            state.bindings.get(&document_id).cloned()
        };
        let Some(binding) = binding else {
            return Ok(());
        };
        if version <= binding.version {
            return Ok(());
        }
        if let Err(error) = self
            .authority()
            .shared_runtime()
            .inner()
            .shared_lsp_change_document(&binding.document, version, snapshot.buffer.text)
        {
            self.mark_document_diagnostics_unavailable(document_id)?;
            return Err(language_service_error(error));
        }
        let mut state = self
            .inner
            .language_services
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("language services"))?;
        if let Some(current) = state.bindings.get_mut(&document_id) {
            current.version = current.version.max(version);
        }
        state
            .states
            .insert(document_id, DesktopDocumentDiagnosticsState::Idle);
        Ok(())
    }

    pub(crate) fn notify_document_language_service_saved(
        &self,
        document_id: DocumentId,
    ) -> Result<(), DesktopApplicationError> {
        let _operation =
            self.inner.language_service_operations.lock().map_err(|_| {
                DesktopApplicationError::StateUnavailable("language service operation")
            })?;
        let snapshot = self.document_snapshot(document_id)?;
        let binding = self
            .inner
            .language_services
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("language services"))?
            .bindings
            .get(&document_id)
            .cloned();
        let Some(binding) = binding else {
            return Ok(());
        };
        self.authority()
            .shared_runtime()
            .inner()
            .shared_lsp_save_document(&binding.document, snapshot.buffer.text)
            .map_err(language_service_error)
    }

    pub(crate) fn close_document_language_service(
        &self,
        document_id: DocumentId,
    ) -> Result<(), DesktopApplicationError> {
        let _operation =
            self.inner.language_service_operations.lock().map_err(|_| {
                DesktopApplicationError::StateUnavailable("language service operation")
            })?;
        let binding = {
            let mut state = self
                .inner
                .language_services
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("language services"))?;
            state.diagnostics.clear(document_id);
            state.states.remove(&document_id);
            state.bindings.remove(&document_id)
        };
        let Some(binding) = binding else {
            return Ok(());
        };
        self.authority()
            .shared_runtime()
            .inner()
            .shared_lsp_close_document(&binding.document)
            .map_err(language_service_error)
    }

    fn mark_document_diagnostics_unavailable(
        &self,
        document_id: DocumentId,
    ) -> Result<(), DesktopApplicationError> {
        self.inner
            .language_services
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("language services"))?
            .states
            .insert(document_id, DesktopDocumentDiagnosticsState::Unavailable);
        Ok(())
    }

    fn ensure_project_document_language_binding(
        &self,
        project_id: &ProjectId,
        document_id: DocumentId,
    ) -> Result<DocumentLanguageBinding, DesktopApplicationError> {
        let _operation =
            self.inner.language_service_operations.lock().map_err(|_| {
                DesktopApplicationError::StateUnavailable("language service operation")
            })?;
        if let Some(binding) = self
            .inner
            .language_services
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("language services"))?
            .bindings
            .get(&document_id)
            .cloned()
        {
            return Ok(binding);
        }
        let context = self.project_context(project_id)?;
        let snapshot = self.document_snapshot(document_id)?;
        let language_id = snapshot
            .language
            .as_ref()
            .map(|language| language.as_str())
            .unwrap_or("plaintext");
        let version = lsp_version(snapshot.buffer.revision)?;
        self.inner
            .language_services
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("language services"))?
            .states
            .insert(document_id, DesktopDocumentDiagnosticsState::Checking);
        let document = match self
            .authority()
            .shared_runtime()
            .inner()
            .shared_lsp_open_document(
                &context.active_root().to_string_lossy(),
                &snapshot.canonical_path,
                language_id,
                version,
                snapshot.buffer.text,
            ) {
            Ok(document) => document,
            Err(error) => {
                self.mark_document_diagnostics_unavailable(document_id)?;
                return Err(language_service_error(error));
            }
        };
        let binding = DocumentLanguageBinding { document, version };
        self.inner
            .language_services
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("language services"))?
            .bindings
            .insert(document_id, binding.clone());
        Ok(binding)
    }
}

fn project_for_path(
    document: &Path,
    roots: impl IntoIterator<Item = (ProjectId, PathBuf)>,
) -> Option<ProjectId> {
    roots
        .into_iter()
        .filter(|(_, root)| document.starts_with(root))
        .max_by_key(|(_, root)| root.components().count())
        .map(|(project_id, _)| project_id)
}

fn lsp_version(revision: BufferRevision) -> Result<i64, DesktopApplicationError> {
    i64::try_from(revision.get()).map_err(|_| DesktopApplicationError::InvalidInput {
        field: "buffer_revision",
        message: "buffer revision exceeds the language service range".to_owned(),
    })
}

fn map_lsp_diagnostic(text: &str, diagnostic: LspDiagnostic) -> Diagnostic {
    let start_offset = offset_for_lsp_position(text, diagnostic.range.start);
    let end_offset = offset_for_lsp_position(text, diagnostic.range.end).max(start_offset);
    Diagnostic {
        message: diagnostic.message,
        severity: match diagnostic.severity {
            Some(1) => DiagnosticSeverity::Error,
            Some(2) => DiagnosticSeverity::Warning,
            Some(4) => DiagnosticSeverity::Hint,
            _ => DiagnosticSeverity::Information,
        },
        start_offset,
        end_offset,
        source: None,
        code: diagnostic.code.and_then(|code| match code {
            serde_json::Value::String(code) => Some(code),
            serde_json::Value::Number(code) => Some(code.to_string()),
            _ => None,
        }),
    }
}

fn offset_for_lsp_position(text: &str, position: LspPosition) -> usize {
    let mut line_start = 0;
    let mut current_line = 0_u32;
    for (offset, byte) in text.bytes().enumerate() {
        if current_line == position.line {
            break;
        }
        if byte == b'\n' {
            current_line = current_line.saturating_add(1);
            line_start = offset.saturating_add(1);
        }
    }
    if current_line < position.line {
        return text.len();
    }
    let line = text[line_start..]
        .split_once('\n')
        .map_or(&text[line_start..], |(line, _)| line);
    let target = position.character as usize;
    let mut utf16_offset = 0;
    for (byte_offset, character) in line.char_indices() {
        let next = utf16_offset + character.len_utf16();
        if target < next {
            return line_start + byte_offset;
        }
        utf16_offset = next;
    }
    line_start + line.len()
}

fn lsp_position_for_offset(text: &str, offset: usize) -> LspPosition {
    let prefix = &text[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let line_start = prefix
        .rfind('\n')
        .map_or(0, |index| index.saturating_add(1));
    LspPosition {
        line: u32::try_from(line).unwrap_or(u32::MAX),
        character: u32::try_from(prefix[line_start..].encode_utf16().count()).unwrap_or(u32::MAX),
    }
}

fn workspace_file_from_uri(root: &Path, uri: &str) -> Option<PathBuf> {
    let url = reqwest::Url::parse(uri).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    let path = fs::canonicalize(url.to_file_path().ok()?).ok()?;
    (path.is_file() && path.starts_with(root)).then_some(path)
}

fn language_service_error(error: impl std::fmt::Display) -> DesktopApplicationError {
    DesktopApplicationError::Agent(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use lilia_service::ServiceAuthority;
    use mutsuki_agent_contracts::LspRange;

    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, DocumentError,
    };

    struct NoopHost;

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    fn application() -> DesktopApplication {
        let identity = format!("language-service-test-{}", uuid::Uuid::new_v4());
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new("/tmp/lilia-language-service-test", identity.clone())
                .unwrap(),
            ServiceAuthority::bootstrap_in_memory_named(identity, "language-service-test").unwrap(),
            Arc::new(NoopHost),
        )
        .unwrap()
    }

    #[test]
    fn rejected_dirty_close_keeps_document_diagnostics_state() {
        let app = application();
        let path = std::env::current_dir()
            .unwrap()
            .join("dirty-language-document.rs");
        let (document, _) = app
            .open_document(path, "fn main() {}", None, false)
            .unwrap();
        app.inner
            .language_services
            .lock()
            .unwrap()
            .states
            .insert(document.id, DesktopDocumentDiagnosticsState::Ready);
        app.replace_document_text(document.id, document.buffer.revision, "fn changed() {}")
            .unwrap();

        assert!(matches!(
            app.close_document(document.id, false),
            Err(DesktopApplicationError::Document(DocumentError::DirtyClose(id)))
                if id == document.id
        ));
        assert_eq!(
            app.document_diagnostics(document.id).unwrap().state,
            DesktopDocumentDiagnosticsState::Ready
        );
    }

    #[test]
    fn diagnostics_map_utf16_positions_to_utf8_buffer_offsets() {
        let text = "零😀a\nsecond";
        let diagnostic = map_lsp_diagnostic(
            text,
            LspDiagnostic {
                range: LspRange {
                    start: LspPosition {
                        line: 0,
                        character: 1,
                    },
                    end: LspPosition {
                        line: 0,
                        character: 3,
                    },
                },
                severity: Some(2),
                code: Some(serde_json::json!("unicode")),
                message: "emoji".to_owned(),
            },
        );

        assert_eq!(&text[diagnostic.start_offset..diagnostic.end_offset], "😀");
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
        assert_eq!(diagnostic.code.as_deref(), Some("unicode"));
    }

    #[test]
    fn definition_positions_round_trip_unicode_and_fence_workspace_files() {
        let text = "零😀a\nsecond";
        let offset = text.find('a').unwrap();
        let position = lsp_position_for_offset(text, offset);
        assert_eq!(position.line, 0);
        assert_eq!(position.character, 3);
        assert_eq!(offset_for_lsp_position(text, position), offset);

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let inside_path = workspace.path().join("inside.rs");
        let outside_path = outside.path().join("outside.rs");
        fs::write(&inside_path, "fn inside() {}\n").unwrap();
        fs::write(&outside_path, "fn outside() {}\n").unwrap();
        let root = fs::canonicalize(workspace.path()).unwrap();
        let inside_uri = reqwest::Url::from_file_path(&inside_path).unwrap();
        let outside_uri = reqwest::Url::from_file_path(&outside_path).unwrap();

        assert_eq!(
            workspace_file_from_uri(&root, inside_uri.as_str()),
            Some(fs::canonicalize(inside_path).unwrap())
        );
        assert_eq!(workspace_file_from_uri(&root, outside_uri.as_str()), None);
        assert_eq!(
            workspace_file_from_uri(&root, "https://example.com/definition.rs"),
            None
        );
    }

    #[test]
    fn document_project_resolution_prefers_the_nearest_active_root() {
        let outer = ProjectId::new("outer").unwrap();
        let inner = ProjectId::new("inner").unwrap();
        let document = Path::new("/workspace/packages/editor/src/lib.rs");

        assert_eq!(
            project_for_path(
                document,
                [
                    (outer, PathBuf::from("/workspace")),
                    (inner.clone(), PathBuf::from("/workspace/packages/editor")),
                ],
            ),
            Some(inner)
        );
    }
}

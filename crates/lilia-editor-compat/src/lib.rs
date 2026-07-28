//! Minimal editor-side AgentKit Completion / Next Edit host (#40).
//!
//! Proves VS Code (or any editor) can reuse AgentKit coding services without
//! `lilia-core`, Desktop Tauri, Node `agent-runner`, or official Agent Server.

use std::sync::Arc;

use mutsuki_agent_contracts::{
    CodeCompletionResponse, DocumentVersion, EditorDocumentRef, EditorWorkspaceRef, NextEditRequest,
    NextEditServiceRequest, NextEditServiceResponse, RecentEditEvent, RecentEditKind, TextPosition,
    TextSelection,
};
use mutsuki_agent_plugin_code_completion::{
    CodeCompletionConfig, CodeCompletionService, DeterministicCompletionAdapter,
    request_from_snapshot, test_provider,
};
use mutsuki_agent_plugin_next_edit::{NextEditServiceConfig, SharedNextEditService};
use mutsuki_agent_runtime::AgentResourceStore;
use mutsuki_agent_testkit::FakeEditorContextService;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const HOST_ID: &str = "lilia.editor-compat";
pub const REQUIRES_LILIA_CORE: bool = false;
pub const REQUIRES_OFFICIAL_AGENT_SERVER: bool = false;
pub const REQUIRES_NODE_AGENT_RUNNER: bool = false;

#[derive(Debug, Error)]
pub enum EditorCompatError {
    #[error(transparent)]
    Agent(#[from] mutsuki_agent_contracts::AgentError),
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorCompatHostStatus {
    pub host_id: &'static str,
    pub requires_lilia_core: bool,
    pub requires_official_agent_server: bool,
    pub requires_node_agent_runner: bool,
    pub completion_service: &'static str,
    pub next_edit_service: &'static str,
}

pub fn host_status() -> EditorCompatHostStatus {
    EditorCompatHostStatus {
        host_id: HOST_ID,
        requires_lilia_core: REQUIRES_LILIA_CORE,
        requires_official_agent_server: REQUIRES_OFFICIAL_AGENT_SERVER,
        requires_node_agent_runner: REQUIRES_NODE_AGENT_RUNNER,
        completion_service: mutsuki_agent_plugin_code_completion::SERVICE_ID,
        next_edit_service: mutsuki_agent_plugin_next_edit::SERVICE_ID,
    }
}

/// In-process deterministic host used by tests and the JSONL CLI.
pub struct EditorCompatHost {
    editor: FakeEditorContextService,
    completion: CodeCompletionService,
    next_edit: SharedNextEditService,
}

impl Default for EditorCompatHost {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorCompatHost {
    pub fn new() -> Self {
        Self {
            editor: FakeEditorContextService::default(),
            completion: CodeCompletionService::new(
                Arc::new(DeterministicCompletionAdapter::new()),
                test_provider("memory://editor-compat"),
                CodeCompletionConfig::default(),
            ),
            next_edit: SharedNextEditService::with_config(
                AgentResourceStore::default(),
                NextEditServiceConfig {
                    debounce_ms: 0,
                    ..NextEditServiceConfig::default()
                },
            ),
        }
    }

    pub fn status(&self) -> EditorCompatHostStatus {
        host_status()
    }

    pub fn complete(
        &self,
        uri: &str,
        language_id: &str,
        prefix: &str,
        suffix: &str,
        generation: u64,
    ) -> Result<CodeCompletionResponse, EditorCompatError> {
        let content = format!("{prefix}{suffix}");
        self.editor
            .open_document(uri, language_id, &content, true)
            .map_err(EditorCompatError::from)?;
        let snapshot = self
            .editor
            .freeze_snapshot(None)
            .map_err(EditorCompatError::from)?;
        let document = snapshot.active_document.clone().unwrap_or(EditorDocumentRef {
            workspace_id: snapshot.workspace.workspace_id.clone(),
            uri: uri.into(),
        });
        let version = snapshot
            .documents
            .iter()
            .find(|doc| doc.document.uri == document.uri)
            .map(|doc| doc.version)
            .unwrap_or(DocumentVersion(1));
        let language = snapshot
            .documents
            .iter()
            .find(|doc| doc.document.uri == document.uri)
            .and_then(|doc| doc.language_id.clone());
        let response = self
            .completion
            .complete(request_from_snapshot(
                format!("vscode-{generation}"),
                generation,
                snapshot.workspace.workspace_id.clone(),
                document,
                language,
                version,
                TextPosition {
                    line: 0,
                    character: prefix.len() as u32,
                },
                prefix,
                suffix,
            ))
            .map_err(EditorCompatError::from)?;
        Ok(response)
    }

    pub fn plan_next_edit(
        &self,
        uri: &str,
        language_id: &str,
        content: &str,
        summary: &str,
        generation: u64,
    ) -> Result<NextEditServiceResponse, EditorCompatError> {
        self.editor
            .open_document(uri, language_id, content, true)
            .map_err(EditorCompatError::from)?;
        let bumped = self
            .editor
            .edit_unsaved(uri, &format!("{content}\n"))
            .map_err(EditorCompatError::from)?;
        let snapshot = self
            .editor
            .freeze_snapshot(Some(format!("turn-{generation}")))
            .map_err(EditorCompatError::from)?;
        // Always plan against the live editor generation so late/superseded
        // results cannot cover a newer buffer.
        let live_generation = snapshot.generation.max(generation);
        let document = EditorDocumentRef {
            workspace_id: snapshot.workspace.workspace_id.clone(),
            uri: uri.into(),
        };
        let event = RecentEditEvent {
            event_id: format!("edit-{live_generation}"),
            document: document.clone(),
            version: bumped,
            editor_generation: live_generation,
            timestamp_unix_ms: 1_000,
            kind: RecentEditKind::Replaced,
            range: Some(TextSelection {
                start: TextPosition {
                    line: 0,
                    character: 0,
                },
                end: TextPosition {
                    line: 1,
                    character: 0,
                },
            }),
            summary: summary.into(),
            byte_delta: 1,
        };
        self.next_edit
            .call_typed(NextEditServiceRequest::IngestRecentEdit {
                event: event.clone(),
            })
            .map_err(EditorCompatError::from)?;

        let request = NextEditRequest {
            request_id: format!("next-edit-{live_generation}"),
            workspace: EditorWorkspaceRef {
                workspace_id: snapshot.workspace.workspace_id.clone(),
                folders: snapshot.workspace.folders.clone(),
                metadata: snapshot.workspace.metadata.clone(),
            },
            generation: live_generation,
            editor_generation: live_generation,
            document_versions: vec![(document, bumped)],
            recent_edits: vec![event],
            diagnostics: Vec::new(),
            related_paths: Vec::new(),
            git_diff: Vec::new(),
            expected_git_head: None,
            intent: None,
            path: mutsuki_agent_contracts::NextEditPlanningPath::Lightweight,
            min_confidence: 0.55,
            allow_multi_file: false,
            deadline_unix_ms: Some(10_000),
            now_unix_ms: 2_000,
            metadata: serde_json::json!({ "host": HOST_ID }),
        };
        self.next_edit
            .call_typed(NextEditServiceRequest::Plan { request })
            .map_err(EditorCompatError::from)
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HostRequest {
    Status,
    Complete {
        uri: String,
        #[serde(default = "default_language")]
        language_id: String,
        prefix: String,
        #[serde(default)]
        suffix: String,
        #[serde(default = "default_generation")]
        generation: u64,
    },
    NextEdit {
        uri: String,
        #[serde(default = "default_language")]
        language_id: String,
        content: String,
        #[serde(default = "default_summary")]
        summary: String,
        #[serde(default = "default_generation")]
        generation: u64,
    },
}

fn default_language() -> String {
    "plaintext".into()
}

fn default_generation() -> u64 {
    1
}

fn default_summary() -> String {
    "editor change".into()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<EditorCompatHostStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion: Option<CodeCompletionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_edit: Option<NextEditServiceResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn handle_request(host: &EditorCompatHost, request: HostRequest) -> HostResponse {
    match request {
        HostRequest::Status => HostResponse {
            ok: true,
            status: Some(host.status()),
            completion: None,
            next_edit: None,
            error: None,
        },
        HostRequest::Complete {
            uri,
            language_id,
            prefix,
            suffix,
            generation,
        } => match host.complete(&uri, &language_id, &prefix, &suffix, generation) {
            Ok(completion) => HostResponse {
                ok: true,
                status: None,
                completion: Some(completion),
                next_edit: None,
                error: None,
            },
            Err(err) => HostResponse {
                ok: false,
                status: None,
                completion: None,
                next_edit: None,
                error: Some(err.to_string()),
            },
        },
        HostRequest::NextEdit {
            uri,
            language_id,
            content,
            summary,
            generation,
        } => match host.plan_next_edit(&uri, &language_id, &content, &summary, generation) {
            Ok(next_edit) => HostResponse {
                ok: true,
                status: None,
                completion: None,
                next_edit: Some(next_edit),
                error: None,
            },
            Err(err) => HostResponse {
                ok: false,
                status: None,
                completion: None,
                next_edit: None,
                error: Some(err.to_string()),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mutsuki_agent_contracts::CodeCompletionStatus;

    #[test]
    fn host_does_not_require_lilia_core_or_official_server() {
        let status = host_status();
        assert!(!status.requires_lilia_core);
        assert!(!status.requires_official_agent_server);
        assert!(!status.requires_node_agent_runner);
        assert_eq!(status.host_id, HOST_ID);
    }

    #[test]
    fn completion_ready_without_agent_loop() {
        let host = EditorCompatHost::new();
        let response = host
            .complete(
                "file:///workspace/main.rs",
                "rust",
                "fn main() {",
                "\n",
                1,
            )
            .expect("completion");
        assert_eq!(response.status, CodeCompletionStatus::Ready);
        assert!(!response.candidates.is_empty());
        assert!(response.may_display(DocumentVersion(1), 1));
    }

    #[test]
    fn next_edit_returns_workspace_edit_proposal() {
        let host = EditorCompatHost::new();
        let response = host
            .plan_next_edit(
                "file:///workspace/main.rs",
                "rust",
                "fn main() {}",
                "opened function body",
                1,
            )
            .expect("next edit");
        let NextEditServiceResponse::Candidate {
            candidate: Some(candidate),
        } = response
        else {
            panic!("expected candidate, got {response:?}");
        };
        assert_eq!(candidate.proposal.changes.len(), 1);
        assert!(!candidate.proposal.proposal_id.is_empty());
    }

    #[test]
    fn json_protocol_status_and_complete() {
        let host = EditorCompatHost::new();
        let status = handle_request(&host, HostRequest::Status);
        assert!(status.ok);
        assert_eq!(
            status.status.as_ref().map(|s| s.requires_lilia_core),
            Some(false)
        );

        let complete = handle_request(
            &host,
            HostRequest::Complete {
                uri: "file:///workspace/lib.rs".into(),
                language_id: "rust".into(),
                prefix: "let x =".into(),
                suffix: "".into(),
                generation: 2,
            },
        );
        assert!(complete.ok);
        assert_eq!(
            complete.completion.as_ref().map(|c| c.status.clone()),
            Some(CodeCompletionStatus::Ready)
        );
    }
}

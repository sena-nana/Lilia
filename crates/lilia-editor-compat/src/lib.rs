//! Standalone editor-side AgentKit Completion / Next Edit host (#40).
//!
//! Production calls protocol model adapters directly. It never starts
//! `lilia-core`, Desktop, Node `agent-runner`, or an official Agent Server.

use std::collections::BTreeMap;
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mutsuki_agent_adapter_anthropic::AnthropicMessagesAdapter;
use mutsuki_agent_adapter_api::{
    CredentialBroker, CredentialFuture, CredentialValue, ModelProtocolAdapter,
};
use mutsuki_agent_adapter_openai::OpenAiCompatibleAdapter;
use mutsuki_agent_contracts::{
    AgentError, CodeCompletionResponse, CredentialRef, DocumentVersion, EditorDocumentRef,
    EditorWorkspaceRef, GitHeadIdentity, ModelCapability, ModelProtocolAdapterDescriptor,
    NextEditDocumentContext, NextEditFeedback, NextEditFeedbackKind, NextEditPlanningPath,
    NextEditRequest, NextEditServiceRequest, NextEditServiceResponse, ProviderInstanceDescriptor,
    RecentEditEvent, RecentEditKind, TextPosition, TextSelection,
};
use mutsuki_agent_plugin_code_completion::{
    request_from_snapshot, CodeCompletionConfig, CodeCompletionService,
};
use mutsuki_agent_plugin_next_edit::{NextEditServiceConfig, SharedNextEditService};
use mutsuki_agent_runtime::AgentResourceStore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

pub const HOST_ID: &str = "lilia.editor-compat";
pub const REQUIRES_LILIA_CORE: bool = false;
pub const REQUIRES_OFFICIAL_AGENT_SERVER: bool = false;
pub const REQUIRES_NODE_AGENT_RUNNER: bool = false;

const DEFAULT_OPENAI_ENDPOINT: &str = "https://api.openai.com/v1";
const PROVIDER_CREDENTIAL_ID: &str = "lilia-editor-host-credential";

#[derive(Debug, Error)]
pub enum EditorCompatError {
    #[error(transparent)]
    Agent(#[from] AgentError),
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

pub struct EditorCompatHost {
    completion: CodeCompletionService,
    next_edit: SharedNextEditService,
    next_generation: AtomicU64,
}

impl EditorCompatHost {
    pub fn from_environment() -> Result<Self, EditorCompatError> {
        let provider_kind = required_env("LILIA_AGENTKIT_PROVIDER")?;
        let model = required_env("LILIA_AGENTKIT_MODEL")?;
        let secret = required_env("LILIA_AGENTKIT_API_KEY")?;
        let credentials: Arc<dyn CredentialBroker> = Arc::new(HostCredentialBroker::new(secret));

        let (adapter, endpoint): (Arc<dyn ModelProtocolAdapter>, String) =
            match provider_kind.as_str() {
                "openai-compatible" => {
                    let mut descriptor = ModelProtocolAdapterDescriptor {
                        adapter_id: "openai-compatible".into(),
                        protocol: "openai.chat-completions".into(),
                        version: "1".into(),
                        runner_id: mutsuki_agent_adapter_openai::RUNNER_ID.into(),
                        capability: ModelCapability::default(),
                    };
                    descriptor.capability.code_completion = true;
                    let adapter = OpenAiCompatibleAdapter::new(descriptor, credentials)
                        .map_err(|error| EditorCompatError::Message(error.message))?;
                    (
                        Arc::new(adapter),
                        optional_env("LILIA_AGENTKIT_ENDPOINT")
                            .unwrap_or_else(|| DEFAULT_OPENAI_ENDPOINT.into()),
                    )
                }
                "anthropic-messages" => {
                    let mut descriptor = AnthropicMessagesAdapter::default_descriptor();
                    descriptor.capability.code_completion = true;
                    let adapter = AnthropicMessagesAdapter::new(descriptor, credentials)
                        .map_err(|error| EditorCompatError::Message(error.message))?;
                    (
                        Arc::new(adapter),
                        optional_env("LILIA_AGENTKIT_ENDPOINT").unwrap_or_else(|| {
                            mutsuki_agent_adapter_anthropic::DEFAULT_ENDPOINT.into()
                        }),
                    )
                }
                other => {
                    return Err(EditorCompatError::Message(format!(
                        "unsupported LILIA_AGENTKIT_PROVIDER `{other}`"
                    )));
                }
            };

        let mut capability = adapter.descriptor().capability.clone();
        capability.code_completion = true;
        let provider = ProviderInstanceDescriptor {
            provider_id: "lilia-editor-provider".into(),
            adapter_id: adapter.descriptor().adapter_id.clone(),
            endpoint,
            credential: CredentialRef {
                credential_id: PROVIDER_CREDENTIAL_ID.into(),
                revision: 1,
            },
            models: BTreeMap::from([(model.clone(), capability)]),
            headers: BTreeMap::new(),
            compatibility: BTreeMap::from([
                ("timeout_ms".into(), json!(10_000)),
                ("max_retries".into(), json!(1)),
            ]),
            remote_execution_allowed: true,
        };
        let completion = CodeCompletionService::new(
            adapter.clone(),
            provider.clone(),
            CodeCompletionConfig {
                model: model.clone(),
                ..CodeCompletionConfig::default()
            },
        );
        let next_edit = SharedNextEditService::with_protocol_model(
            AgentResourceStore::default(),
            NextEditServiceConfig {
                debounce_ms: 0,
                ..NextEditServiceConfig::default()
            },
            adapter,
            provider,
            model,
        )?;
        Ok(Self {
            completion,
            next_edit,
            next_generation: AtomicU64::new(1),
        })
    }

    #[cfg(test)]
    fn new_test() -> Self {
        use mutsuki_agent_contracts::WorkspaceTextEdit;
        use mutsuki_agent_plugin_code_completion::{test_provider, DeterministicCompletionAdapter};
        use mutsuki_agent_plugin_next_edit::{NextEditPlan, NextEditPlanner, PlannedNextEdit};

        struct TestPlanner;
        impl NextEditPlanner for TestPlanner {
            fn plan(
                &self,
                _request: &NextEditRequest,
                targets: &[mutsuki_agent_contracts::NextEditTarget],
            ) -> Result<Option<NextEditPlan>, AgentError> {
                let Some(target) = targets.first() else {
                    return Ok(None);
                };
                Ok(Some(NextEditPlan {
                    reason: "test next edit".into(),
                    confidence: 0.9,
                    edits: vec![PlannedNextEdit {
                        document: target.document.clone(),
                        edit: WorkspaceTextEdit {
                            range: target.range.unwrap_or(TextSelection {
                                start: TextPosition {
                                    line: 0,
                                    character: 0,
                                },
                                end: TextPosition {
                                    line: 0,
                                    character: 0,
                                },
                            }),
                            new_text: "\n".into(),
                        },
                    }],
                }))
            }
        }

        Self {
            completion: CodeCompletionService::new(
                Arc::new(DeterministicCompletionAdapter::new()),
                test_provider("memory://editor-compat"),
                CodeCompletionConfig::default(),
            ),
            next_edit: SharedNextEditService::with_planner(
                AgentResourceStore::default(),
                NextEditServiceConfig {
                    debounce_ms: 0,
                    ..NextEditServiceConfig::default()
                },
                Arc::new(TestPlanner),
            ),
            next_generation: AtomicU64::new(1),
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
        let workspace = workspace_for_uri(uri);
        let document = EditorDocumentRef {
            workspace_id: workspace.workspace_id.clone(),
            uri: uri.into(),
        };
        let version = DocumentVersion(generation.max(1));
        self.completion
            .observe_document_version(workspace.workspace_id.clone(), uri, version);
        self.completion
            .complete(request_from_snapshot(
                format!("vscode-{generation}"),
                generation.max(1),
                workspace.workspace_id,
                document,
                Some(language_id.into()),
                version,
                text_end_position(prefix),
                (prefix, suffix),
            ))
            .map_err(EditorCompatError::from)
    }

    pub fn plan_next_edit(
        &self,
        uri: &str,
        language_id: &str,
        content: &str,
        summary: &str,
        document_version: u64,
    ) -> Result<NextEditServiceResponse, EditorCompatError> {
        let workspace = workspace_for_uri(uri);
        let document = EditorDocumentRef {
            workspace_id: workspace.workspace_id.clone(),
            uri: uri.into(),
        };
        let version = DocumentVersion(document_version.max(1));
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let now_unix_ms = unix_time_ms();
        let cursor = text_end_position(content);
        let recent_edit = RecentEditEvent {
            event_id: format!("edit-{generation}"),
            document: document.clone(),
            version,
            editor_generation: generation,
            timestamp_unix_ms: now_unix_ms.saturating_sub(1),
            kind: RecentEditKind::Replaced,
            range: Some(TextSelection {
                start: cursor,
                end: cursor,
            }),
            summary: summary.into(),
            byte_delta: 0,
        };
        self.next_edit
            .call_typed(NextEditServiceRequest::IngestRecentEdit {
                event: recent_edit.clone(),
            })?;
        self.next_edit
            .call_typed(NextEditServiceRequest::Plan {
                request: Box::new(NextEditRequest {
                    request_id: format!("next-edit-{generation}"),
                    workspace,
                    generation,
                    editor_generation: generation,
                    document_versions: vec![(document.clone(), version)],
                    document_contexts: vec![NextEditDocumentContext {
                        document,
                        version,
                        language_id: Some(language_id.into()),
                        selection: Some(TextSelection {
                            start: cursor,
                            end: cursor,
                        }),
                        inline_text: Some(content.into()),
                        content_ref: None,
                    }],
                    recent_edits: vec![recent_edit],
                    diagnostics: Vec::new(),
                    related_paths: Vec::new(),
                    git_diff: Vec::new(),
                    expected_git_head: None,
                    intent: Some(summary.into()),
                    path: NextEditPlanningPath::Lightweight,
                    min_confidence: 0.55,
                    allow_multi_file: false,
                    deadline_unix_ms: Some(now_unix_ms.saturating_add(10_000)),
                    now_unix_ms,
                    metadata: json!({ "host": HOST_ID }),
                }),
            })
            .map_err(EditorCompatError::from)
    }

    pub fn validate_next_edit(
        &self,
        candidate_id: String,
        document_versions: Vec<(EditorDocumentRef, DocumentVersion)>,
        git_head: Option<GitHeadIdentity>,
        now_unix_ms: u64,
    ) -> Result<NextEditServiceResponse, EditorCompatError> {
        self.next_edit
            .call_typed(NextEditServiceRequest::Validate {
                candidate_id,
                document_versions,
                git_head,
                now_unix_ms,
            })
            .map_err(EditorCompatError::from)
    }

    pub fn record_next_edit_feedback(
        &self,
        feedback: NextEditFeedback,
    ) -> Result<NextEditServiceResponse, EditorCompatError> {
        self.next_edit
            .call_typed(NextEditServiceRequest::Feedback { feedback })
            .map_err(EditorCompatError::from)
    }
}

struct HostCredentialBroker {
    secret: Arc<str>,
}

impl HostCredentialBroker {
    fn new(secret: String) -> Self {
        Self {
            secret: Arc::from(secret),
        }
    }
}

impl CredentialBroker for HostCredentialBroker {
    fn resolve(&self, credential: CredentialRef) -> CredentialFuture {
        let secret = self.secret.clone();
        Box::pin(async move {
            if credential.credential_id != PROVIDER_CREDENTIAL_ID {
                return Err(mutsuki_agent_contracts::ProtocolError {
                    code: "agent.credential.unavailable".into(),
                    class: mutsuki_agent_contracts::ProtocolErrorClass::Authentication,
                    message: "credential reference is unavailable".into(),
                    retry_after_ms: None,
                });
            }
            CredentialValue::new(secret.to_string())
        })
    }
}

fn required_env(name: &str) -> Result<String, EditorCompatError> {
    optional_env(name).ok_or_else(|| {
        EditorCompatError::Message(format!("{name} is required for the AgentKit editor host"))
    })
}

fn optional_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn workspace_for_uri(uri: &str) -> EditorWorkspaceRef {
    let folder = uri
        .rsplit_once('/')
        .map(|(folder, _)| folder)
        .unwrap_or(uri);
    EditorWorkspaceRef {
        workspace_id: format!("vscode:{folder}"),
        folders: vec![folder.into()],
        metadata: json!({}),
    }
}

fn text_end_position(value: &str) -> TextPosition {
    let mut line = 0u32;
    let mut character = 0u32;
    for ch in value.chars() {
        if ch == '\n' {
            line = line.saturating_add(1);
            character = 0;
        } else {
            character = character.saturating_add(ch.len_utf16() as u32);
        }
    }
    TextPosition { line, character }
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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
    NextEditValidate {
        candidate_id: String,
        document_versions: Vec<(EditorDocumentRef, DocumentVersion)>,
        #[serde(default)]
        git_head: Option<GitHeadIdentity>,
        #[serde(default)]
        now_unix_ms: u64,
    },
    NextEditFeedback {
        candidate_id: String,
        kind: NextEditFeedbackKind,
        #[serde(default)]
        reason_code: Option<String>,
        #[serde(default)]
        timestamp_unix_ms: u64,
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

impl HostResponse {
    pub fn error(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            status: None,
            completion: None,
            next_edit: None,
            error: Some(error.into()),
        }
    }
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
            Err(error) => HostResponse::error(error.to_string()),
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
            Err(error) => HostResponse::error(error.to_string()),
        },
        HostRequest::NextEditValidate {
            candidate_id,
            document_versions,
            git_head,
            now_unix_ms,
        } => {
            match host.validate_next_edit(candidate_id, document_versions, git_head, now_unix_ms) {
                Ok(next_edit) => HostResponse {
                    ok: true,
                    status: None,
                    completion: None,
                    next_edit: Some(next_edit),
                    error: None,
                },
                Err(error) => HostResponse::error(error.to_string()),
            }
        }
        HostRequest::NextEditFeedback {
            candidate_id,
            kind,
            reason_code,
            timestamp_unix_ms,
        } => match host.record_next_edit_feedback(NextEditFeedback {
            candidate_id,
            kind,
            timestamp_unix_ms,
            reason_code,
        }) {
            Ok(next_edit) => HostResponse {
                ok: true,
                status: None,
                completion: None,
                next_edit: Some(next_edit),
                error: None,
            },
            Err(error) => HostResponse::error(error.to_string()),
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
        let host = EditorCompatHost::new_test();
        let response = host
            .complete("file:///workspace/main.rs", "rust", "fn main() {", "\n", 1)
            .expect("completion");
        assert_eq!(response.status, CodeCompletionStatus::Ready);
        assert!(!response.candidates.is_empty());
        assert!(response.may_display(DocumentVersion(1), 1));
    }

    #[test]
    fn next_edit_returns_applicable_workspace_edit_and_validates_version() {
        let host = EditorCompatHost::new_test();
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
        assert_eq!(candidate.proposal.changes[0].edits.len(), 1);
        let valid = host
            .validate_next_edit(
                candidate.candidate_id,
                vec![(
                    candidate.proposal.changes[0].document.clone(),
                    DocumentVersion(1),
                )],
                None,
                unix_time_ms(),
            )
            .expect("validate");
        assert!(matches!(valid, NextEditServiceResponse::Valid { .. }));
    }

    #[test]
    fn json_protocol_status_complete_and_feedback() {
        let host = EditorCompatHost::new_test();
        let status = handle_request(&host, HostRequest::Status);
        assert!(status.ok);
        assert_eq!(
            status
                .status
                .as_ref()
                .map(|value| value.requires_lilia_core),
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
            complete.completion.as_ref().map(|value| value.status),
            Some(CodeCompletionStatus::Ready)
        );

        let feedback = handle_request(
            &host,
            HostRequest::NextEditFeedback {
                candidate_id: "candidate".into(),
                kind: NextEditFeedbackKind::Skipped,
                reason_code: Some("test".into()),
                timestamp_unix_ms: 1,
            },
        );
        assert!(feedback.ok);
        assert!(matches!(
            feedback.next_edit,
            Some(NextEditServiceResponse::FeedbackRecorded { .. })
        ));
    }
}

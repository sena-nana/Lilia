use lilia_agent::{
    NativeContextCompactionSource, NativeControlModelRequest, NativeControlModelResult,
};
use lilia_contracts::{
    context_compaction_request_instruction, context_compaction_success_message,
    context_compaction_system_instruction, TaskId,
};
use mutsuki_agent_contracts::{AgentEventEnvelope, AgentRole};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::application::TimelineChanged;
use crate::application::{DesktopApplication, DesktopApplicationError};

const CONTEXT_COMPACTION_INPUT_TOKEN_BUDGET: u64 = 48_000;
const CONTEXT_COMPACTION_OUTPUT_TOKEN_BUDGET: u64 = 2_048;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopContextCompactionResult {
    pub source_session_id: String,
    pub session_id: String,
    pub profile_id: String,
    pub provider_id: String,
    pub model: String,
    pub source_message_count: usize,
    pub omitted_message_count: usize,
    pub events: Vec<AgentEventEnvelope>,
}

impl DesktopApplication {
    pub fn compact_task_agent_context(
        &self,
        task_id: &TaskId,
        turn_id: &str,
    ) -> Result<DesktopContextCompactionResult, DesktopApplicationError> {
        self.compact_task_agent_context_with_model(task_id, turn_id, None)
    }

    pub fn compact_task_agent_context_with_model(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        model: Option<&str>,
    ) -> Result<DesktopContextCompactionResult, DesktopApplicationError> {
        self.compact_task_agent_context_with_commit_guard(task_id, turn_id, model, || true)
    }

    pub(crate) fn compact_task_agent_context_with_commit_guard(
        &self,
        task_id: &TaskId,
        turn_id: &str,
        model: Option<&str>,
        should_commit: impl Fn() -> bool,
    ) -> Result<DesktopContextCompactionResult, DesktopApplicationError> {
        let turn_id = turn_id.trim();
        if turn_id.is_empty() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "turn_id",
                message: "context compaction turn id must not be empty".into(),
            });
        }
        let source_binding = self
            .authority()
            .list_session_bindings(task_id)?
            .into_iter()
            .next()
            .ok_or_else(|| DesktopApplicationError::InvalidInput {
                field: "agent_session",
                message: "task has no Agent session to compact".into(),
            })?;
        let runtime = self.authority().shared_runtime();
        let source = runtime
            .inner()
            .prepare_product_session_compaction(
                source_binding.agent_session.as_str(),
                CONTEXT_COMPACTION_INPUT_TOKEN_BUDGET,
            )
            .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        if !source.budget_satisfied {
            return Err(DesktopApplicationError::InvalidInput {
                field: "agent_session",
                message: "the latest Agent turn exceeds the safe context compaction budget".into(),
            });
        }
        let generated = runtime
            .inner()
            .generate_control_text(NativeControlModelRequest {
                system_instruction: context_compaction_system_instruction().to_owned(),
                prompt: context_compaction_prompt(&source),
                model: model.map(str::to_owned),
                max_output_tokens: CONTEXT_COMPACTION_OUTPUT_TOKEN_BUDGET,
                reasoning: Some("low".into()),
            })
            .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        if !should_commit() {
            return Err(DesktopApplicationError::Agent(
                "context compaction was cancelled before commit".into(),
            ));
        }
        let target_session_id = format!(
            "native-{}-compact-{}",
            task_id.as_str(),
            uuid::Uuid::new_v4()
        );
        let compacted = runtime
            .inner()
            .create_compacted_product_session(
                task_id,
                &source,
                &target_session_id,
                turn_id,
                &generated,
                context_compaction_success_message(),
            )
            .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        if !should_commit() {
            return Err(DesktopApplicationError::Agent(
                "context compaction was cancelled before binding replacement".into(),
            ));
        }
        self.replace_session_binding(task_id, &compacted.session_id, &compacted.profile_id)?;
        self.emit_event(TimelineChanged {
            task_id: task_id.clone(),
            cursor: compacted.events.last().map(|event| event.sequence),
        });
        Ok(compaction_result(source, generated, compacted))
    }
}

fn context_compaction_prompt(source: &NativeContextCompactionSource) -> String {
    let transcript = source
        .messages
        .iter()
        .map(|message| {
            json!({
                "role": role_name(&message.role),
                "name": message.name,
                "content": message.content,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "instruction": context_compaction_request_instruction(),
        "sourceMessageCount": source.source_message_count,
        "omittedMessageCount": source.omitted_message_count,
        "transcript": transcript,
    })
    .to_string()
}

fn role_name(role: &AgentRole) -> &'static str {
    match role {
        AgentRole::System => "system",
        AgentRole::User => "user",
        AgentRole::Assistant => "assistant",
        AgentRole::Tool => "tool",
    }
}

fn compaction_result(
    source: NativeContextCompactionSource,
    generated: NativeControlModelResult,
    compacted: mutsuki_agent_contracts::AgentSession,
) -> DesktopContextCompactionResult {
    DesktopContextCompactionResult {
        source_session_id: source.source_session_id,
        session_id: compacted.session_id,
        profile_id: compacted.profile_id,
        provider_id: generated.provider_id,
        model: generated.model,
        source_message_count: source.source_message_count,
        omitted_message_count: source.omitted_message_count,
        events: compacted.events,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;

    use lilia_agent::ProductCredentialLoginInput;
    use lilia_contracts::{AgentSessionRef, ProductEntity, ProductTask};
    use lilia_service::ServiceAuthority;
    use mutsuki_agent_contracts::{AgentMessage, CredentialKind, OPENAI_CREDENTIAL_PROVIDER_ID};

    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
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

    fn model_response(content: &str) -> serde_json::Value {
        json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {"role": "assistant", "content": content}
            }],
            "usage": {"prompt_tokens": 30, "completion_tokens": 12, "total_tokens": 42}
        })
    }

    fn model_server(responses: Vec<serde_json::Value>) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        );
        let server = std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut bytes = [0_u8; 32_768];
                let _ = stream.read(&mut bytes).unwrap();
                let body = response.to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        (endpoint, server)
    }

    fn application_with_source_session(
        label: &str,
        endpoint: String,
    ) -> (DesktopApplication, TaskId, String) {
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:context-compaction:{label}"),
            format!("context-compaction:{label}"),
        )
        .unwrap();
        let runtime = authority.shared_runtime();
        runtime
            .inner()
            .credentials()
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-context-compaction-0123456789".into(),
                account_label: None,
                source: Some("context-compaction-test".into()),
            })
            .unwrap();
        let task_id = TaskId::new(format!("task-context-compaction-{label}")).unwrap();
        authority
            .client()
            .unwrap()
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task_id.clone(), None, "Context compaction").unwrap(),
            ))
            .unwrap();
        let application = DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                "C:/lilia/context-compaction-test",
                format!("liliacode.context-compaction.{label}"),
            )
            .unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();
        runtime.inner().set_model_endpoint_override(Some(endpoint));
        runtime.inner().refresh_product_profile(None).unwrap();
        let session = application.open_task_agent_wire_session(&task_id).unwrap();
        runtime
            .inner()
            .submit_turn_with_context_streaming(
                &AgentSessionRef::new(session.session_id.clone()).unwrap(),
                "preserve the current implementation state",
                "turn-before-compaction",
                None,
            )
            .unwrap();
        (application, task_id, session.session_id)
    }

    #[test]
    fn compaction_prompt_keeps_roles_and_content_but_not_runtime_metadata() {
        let mut message = AgentMessage::user("keep the dirty worktree");
        message.metadata = Some(json!({"privateRuntimeState": true}));
        let prompt = context_compaction_prompt(&NativeContextCompactionSource {
            source_session_id: "source".into(),
            profile_id: "profile".into(),
            title: None,
            messages: vec![message, AgentMessage::assistant("understood")],
            source_message_count: 4,
            omitted_message_count: 2,
            estimated_tokens: 20,
            budget_satisfied: true,
        });
        let prompt: serde_json::Value = serde_json::from_str(&prompt).unwrap();

        assert_eq!(prompt["sourceMessageCount"], 4);
        assert_eq!(prompt["omittedMessageCount"], 2);
        assert_eq!(prompt["transcript"][0]["role"], "user");
        assert_eq!(
            prompt["transcript"][0]["content"],
            "keep the dirty worktree"
        );
        assert!(prompt["transcript"][0].get("metadata").is_none());
        assert_eq!(prompt["transcript"][1]["role"], "assistant");
    }

    #[test]
    fn compaction_replaces_the_product_binding_only_after_the_new_session_is_durable() {
        let label = uuid::Uuid::new_v4().to_string();
        let (endpoint, server) = model_server(vec![
            model_response("initial response"),
            model_response("目标：保留当前实现。状态：初始回合已完成。下一步：继续迁移。"),
        ]);
        let (application, task_id, source_session_id) =
            application_with_source_session(&label, endpoint);
        let source_before = application
            .authority()
            .shared_runtime()
            .inner()
            .session_snapshot(&source_session_id)
            .unwrap();

        let result = application
            .compact_task_agent_context(&task_id, "turn-compaction")
            .unwrap();
        server.join().unwrap();

        assert_ne!(result.session_id, source_session_id);
        let bindings = application
            .authority()
            .list_session_bindings(&task_id)
            .unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].agent_session.as_str(), result.session_id);
        assert_eq!(
            application
                .authority()
                .shared_runtime()
                .inner()
                .session_snapshot(&source_session_id)
                .unwrap(),
            source_before
        );
        assert!(result.events.iter().any(|event| matches!(
            &event.event,
            mutsuki_agent_contracts::AgentEvent::FinalResponse { summary, .. }
                if summary == context_compaction_success_message()
        )));
    }

    #[test]
    fn compaction_model_failure_keeps_the_existing_product_binding() {
        let label = uuid::Uuid::new_v4().to_string();
        let (endpoint, server) =
            model_server(vec![model_response("initial response"), model_response("")]);
        let (application, task_id, source_session_id) =
            application_with_source_session(&label, endpoint);

        assert!(application
            .compact_task_agent_context(&task_id, "turn-compaction-failure")
            .is_err());
        server.join().unwrap();

        let bindings = application
            .authority()
            .list_session_bindings(&task_id)
            .unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].agent_session.as_str(), source_session_id);
    }
}

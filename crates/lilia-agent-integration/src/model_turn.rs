//! Live Model Adapter turn driver (#50 / Mutsuki#121).
//!
//! Product path: Credential Broker resolve → protocol HTTP Adapter
//! (`openai-compatible`) → AgentKit stream/generate events.
//! Without a usable CredentialRef, callers keep the reference coding tool path.

use std::collections::BTreeMap;
use std::sync::Arc;

use mutsuki_agent_adapter_api::{
    CredentialBroker, CredentialFuture, CredentialValue, ModelProtocolAdapter,
};
use mutsuki_agent_adapter_openai::OpenAiCompatibleAdapter;
use mutsuki_agent_contracts::{
    AgentEvent, AgentEventMeta, AgentMessage, AgentModelGenerateRequest, AgentModelStopReason,
    AgentRuntimeProfile, AgentToolCall, AgentToolDescriptor, CredentialRef, ModelCapability,
    ModelGenerateRequest, ModelProtocolAdapterDescriptor, ModelStreamEvent, PermissionRequest,
    ProtocolError, ProtocolErrorClass, ProviderInstanceDescriptor, ToolSideEffect,
};
use mutsuki_agent_runtime::CredentialBrokerService;
use serde_json::{json, Value};

use crate::anthropic_adapter::{
    anthropic_provider_descriptor, resolve_anthropic_endpoint, AnthropicMessagesAdapter,
    ANTHROPIC_MESSAGES_ADAPTER_ID, DEFAULT_ANTHROPIC_MODEL,
};
use crate::profile::{ANTHROPIC_MESSAGES_PROTOCOL_FAMILY, OPENAI_COMPATIBLE_ADAPTER_ID};

/// Public OpenAI Chat Completions base (no secret). Override with env / runtime.
pub const DEFAULT_OPENAI_COMPATIBLE_ENDPOINT: &str = "https://api.openai.com/v1";
pub const ENV_MODEL_ENDPOINT: &str = "LILIA_MODEL_ENDPOINT";
pub const ENV_MODEL_ID: &str = "LILIA_MODEL_ID";
const PRODUCT_DEFAULT_MODEL_PLACEHOLDER: &str = "product-default";
const DEFAULT_OPENAI_MODEL: &str = "gpt-4.1-mini";

/// Bridges AgentKit CredentialBrokerService into the adapter-facing CredentialBroker.
#[derive(Clone)]
struct ProductAdapterCredentialBroker {
    service: CredentialBrokerService,
}

impl CredentialBroker for ProductAdapterCredentialBroker {
    fn resolve(&self, credential: CredentialRef) -> CredentialFuture {
        let service = self.service.clone();
        Box::pin(async move {
            match service.resolve_secret(&credential) {
                Ok(secret) => CredentialValue::new(secret).map_err(|error| ProtocolError {
                    code: error.code.clone(),
                    class: ProtocolErrorClass::Authentication,
                    message: error.message,
                    retry_after_ms: None,
                }),
                Err(error) => Err(ProtocolError {
                    code: if error.code.is_empty() {
                        "agent.credential.unavailable".into()
                    } else {
                        error.code
                    },
                    class: ProtocolErrorClass::Authentication,
                    message: error.message,
                    retry_after_ms: None,
                }),
            }
        })
    }
}

pub fn adapter_credential_broker(service: CredentialBrokerService) -> Arc<dyn CredentialBroker> {
    Arc::new(ProductAdapterCredentialBroker { service })
}

#[derive(Clone, Debug)]
pub struct LiveModelTurnPlan {
    pub provider: ProviderInstanceDescriptor,
    pub model: String,
    pub driver: LiveModelDriver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveModelDriver {
    OpenAiCompatible,
    AnthropicMessages,
}

impl LiveModelDriver {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai-compatible",
            Self::AnthropicMessages => "anthropic-messages",
        }
    }
}

/// Tool call waiting for product approval before real execution.
#[derive(Clone, Debug)]
pub struct PendingToolApproval {
    pub call_id: String,
    pub name: String,
    pub input: Value,
    pub version: u64,
}

#[derive(Clone, Debug)]
pub struct LiveModelTurnResult {
    pub events: Vec<(AgentEventMeta, AgentEvent)>,
    pub tool_summary: Value,
    pub waiting_approval: bool,
    pub pending_approvals: Vec<PendingToolApproval>,
}

/// Resolve HTTPS (or loopback) endpoint for the openai-compatible Adapter.
pub fn resolve_model_endpoint(override_endpoint: Option<&str>) -> String {
    override_endpoint
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var(ENV_MODEL_ENDPOINT)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| DEFAULT_OPENAI_COMPATIBLE_ENDPOINT.to_string())
}

fn resolve_model_id(profile_model: &str) -> String {
    if let Ok(value) = std::env::var(ENV_MODEL_ID) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if profile_model.trim().is_empty() || profile_model == PRODUCT_DEFAULT_MODEL_PLACEHOLDER {
        DEFAULT_OPENAI_MODEL.to_string()
    } else {
        profile_model.to_string()
    }
}

pub fn openai_live_eligible(profile: &AgentRuntimeProfile) -> bool {
    profile.providers.iter().any(|provider| {
        provider.adapter_id == OPENAI_COMPATIBLE_ADAPTER_ID && provider.credential_ref.is_some()
    })
}

pub fn anthropic_live_eligible(profile: &AgentRuntimeProfile) -> bool {
    profile.providers.iter().any(|provider| {
        provider.adapter_id == ANTHROPIC_MESSAGES_ADAPTER_ID && provider.credential_ref.is_some()
    })
}

/// True when the product profile can drive turns via a protocol HTTP Adapter.
pub fn live_model_adapter_eligible(profile: &AgentRuntimeProfile) -> bool {
    openai_live_eligible(profile) || anthropic_live_eligible(profile)
}

pub fn build_live_turn_plan(
    profile: &AgentRuntimeProfile,
    openai_endpoint: &str,
    anthropic_endpoint: Option<&str>,
) -> Option<LiveModelTurnPlan> {
    if openai_live_eligible(profile) {
        return build_openai_turn_plan(profile, openai_endpoint);
    }
    if anthropic_live_eligible(profile) {
        return build_anthropic_turn_plan(profile, anthropic_endpoint);
    }
    None
}

fn build_openai_turn_plan(
    profile: &AgentRuntimeProfile,
    endpoint: &str,
) -> Option<LiveModelTurnPlan> {
    let selection = profile
        .adapters
        .iter()
        .find(|adapter| adapter.adapter_id == OPENAI_COMPATIBLE_ADAPTER_ID)?;
    let provider_inst = profile
        .providers
        .iter()
        .find(|provider| provider.instance_id == selection.provider_instance_id)
        .filter(|provider| provider.credential_ref.is_some())
        .or_else(|| {
            profile.providers.iter().find(|provider| {
                provider.adapter_id == OPENAI_COMPATIBLE_ADAPTER_ID
                    && provider.credential_ref.is_some()
            })
        })?;
    let credential = provider_inst.credential_ref.clone()?;
    let model = resolve_model_id(&selection.model);
    let capability = ModelCapability {
        context_window: 128_000,
        streaming: true,
        tools: true,
        structured_output: true,
        ..ModelCapability::default()
    };
    Some(LiveModelTurnPlan {
        provider: ProviderInstanceDescriptor {
            provider_id: provider_inst.instance_id.clone(),
            adapter_id: OPENAI_COMPATIBLE_ADAPTER_ID.into(),
            endpoint: endpoint.to_string(),
            credential,
            models: BTreeMap::from([(model.clone(), capability)]),
            headers: BTreeMap::new(),
            compatibility: BTreeMap::from([
                ("timeout_ms".into(), json!(30_000)),
                ("max_retries".into(), json!(1)),
            ]),
            remote_execution_allowed: true,
        },
        model,
        driver: LiveModelDriver::OpenAiCompatible,
    })
}

fn build_anthropic_turn_plan(
    profile: &AgentRuntimeProfile,
    endpoint_override: Option<&str>,
) -> Option<LiveModelTurnPlan> {
    let selection = profile.adapters.iter().find(|adapter| {
        adapter.adapter_id == ANTHROPIC_MESSAGES_ADAPTER_ID
            || adapter.protocol_family == ANTHROPIC_MESSAGES_PROTOCOL_FAMILY
    });
    let provider_inst = selection
        .and_then(|sel| {
            profile
                .providers
                .iter()
                .find(|provider| provider.instance_id == sel.provider_instance_id)
                .filter(|provider| provider.credential_ref.is_some())
        })
        .or_else(|| {
            profile.providers.iter().find(|provider| {
                provider.adapter_id == ANTHROPIC_MESSAGES_ADAPTER_ID
                    && provider.credential_ref.is_some()
            })
        })?;
    let credential = provider_inst.credential_ref.clone()?;
    let model = selection
        .map(|sel| resolve_anthropic_model_id(&sel.model))
        .unwrap_or_else(|| DEFAULT_ANTHROPIC_MODEL.to_string());
    let endpoint = resolve_anthropic_endpoint(endpoint_override);
    Some(LiveModelTurnPlan {
        provider: anthropic_provider_descriptor(
            provider_inst.instance_id.clone(),
            &endpoint,
            credential,
            &model,
        ),
        model,
        driver: LiveModelDriver::AnthropicMessages,
    })
}

fn resolve_anthropic_model_id(profile_model: &str) -> String {
    if let Ok(value) = std::env::var(ENV_MODEL_ID) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if profile_model.trim().is_empty() || profile_model == PRODUCT_DEFAULT_MODEL_PLACEHOLDER {
        DEFAULT_ANTHROPIC_MODEL.to_string()
    } else {
        profile_model.to_string()
    }
}

fn coding_tools() -> Vec<AgentToolDescriptor> {
    let mut tool = AgentToolDescriptor::new(
        "native.coding.fix",
        "lilia.native.coding.fix@1",
        "Apply the reference native coding fix tool",
    );
    tool.side_effect = ToolSideEffect::WorkspaceWrite;
    tool.requires_approval = true;
    tool.input_schema = json!({
        "type": "object",
        "properties": { "prompt": { "type": "string" } }
    });
    vec![tool]
}

fn openai_adapter_descriptor() -> ModelProtocolAdapterDescriptor {
    ModelProtocolAdapterDescriptor {
        adapter_id: OPENAI_COMPATIBLE_ADAPTER_ID.into(),
        protocol: "openai.chat-completions".into(),
        version: "1".into(),
        runner_id: "agent.adapter.openai-compatible".into(),
        capability: ModelCapability {
            context_window: 128_000,
            streaming: true,
            tools: true,
            structured_output: true,
            ..ModelCapability::default()
        },
    }
}

fn block_on_adapter<T>(future: impl std::future::Future<Output = T>) -> Result<T, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| err.to_string())?;
    Ok(runtime.block_on(future))
}

fn map_protocol_error(error: ProtocolError) -> String {
    format!("{}: {}", error.code, error.message)
}

fn append_tool_and_approval_events(
    events: &mut Vec<(AgentEventMeta, AgentEvent)>,
    session_id: &str,
    turn_id: &str,
    tool_calls: &[AgentToolCall],
) -> Vec<PendingToolApproval> {
    let mut pending = Vec::new();
    for (index, call) in tool_calls.iter().enumerate() {
        events.push((
            AgentEventMeta::new(
                format!("evt-tool-start-{turn_id}-{index}"),
                "tool call started",
            )
            .with_turn(turn_id),
            AgentEvent::ToolCallStarted {
                turn_id: turn_id.into(),
                call_id: call.call_id.clone(),
                name: call.name.clone(),
                input: call.input.clone(),
            },
        ));
        let requires_approval = coding_tools()
            .iter()
            .find(|tool| tool.name == call.name)
            .map(|tool| tool.requires_approval)
            .unwrap_or(true);
        if requires_approval {
            let version = 1;
            events.push((
                AgentEventMeta::new(
                    format!("evt-approval-{turn_id}-{index}"),
                    "approval requested",
                )
                .with_turn(turn_id),
                AgentEvent::ApprovalRequest {
                    request: PermissionRequest {
                        session_id: session_id.into(),
                        turn_id: turn_id.into(),
                        action_id: call.call_id.clone(),
                        tool: call.name.clone(),
                        side_effect: ToolSideEffect::WorkspaceWrite,
                        summary: format!("Allow `{}` for this coding action", call.name),
                        version,
                    },
                },
            ));
            pending.push(PendingToolApproval {
                call_id: call.call_id.clone(),
                name: call.name.clone(),
                input: call.input.clone(),
                version,
            });
        }
    }
    pending
}

fn events_from_generate(
    session_id: &str,
    turn_id: &str,
    result: mutsuki_agent_contracts::AgentModelGenerateResult,
    driver: LiveModelDriver,
) -> LiveModelTurnResult {
    let mut events = Vec::new();
    if !result.message.content.is_empty() {
        events.push((
            AgentEventMeta::new(format!("evt-delta-{turn_id}"), "model delta").with_turn(turn_id),
            AgentEvent::ModelDelta {
                turn_id: turn_id.into(),
                text: result.message.content.clone(),
            },
        ));
    }
    let pending_approvals =
        append_tool_and_approval_events(&mut events, session_id, turn_id, &result.tool_calls);
    let waiting_approval = !pending_approvals.is_empty();
    let summary = if waiting_approval {
        format!(
            "Live Model Adapter requested approval for {} tool call(s)",
            result.tool_calls.len()
        )
    } else if result.message.content.is_empty() {
        "Live Model Adapter turn complete".into()
    } else {
        result.message.content.clone()
    };
    // When waiting on approval, defer FinalResponse until the decision is applied.
    if !waiting_approval {
        events.push((
            AgentEventMeta::new(format!("evt-final-{turn_id}"), "final response")
                .with_turn(turn_id),
            AgentEvent::FinalResponse {
                turn_id: turn_id.into(),
                summary: summary.clone(),
                result: None,
            },
        ));
    }
    let status = if waiting_approval {
        "waiting_approval"
    } else {
        "completed"
    };
    events.push((
        AgentEventMeta::new(format!("evt-turn-done-{turn_id}"), "turn completed")
            .with_turn(turn_id),
        AgentEvent::TurnState {
            turn_id: turn_id.into(),
            status: status.into(),
        },
    ));
    LiveModelTurnResult {
        events,
        tool_summary: json!({
            "driver": driver.as_str(),
            "official_servers": 0,
            "stop_reason": result.stop_reason,
            "tool_calls": result.tool_calls.len(),
            "waiting_approval": waiting_approval,
            "usage": result.usage,
        }),
        waiting_approval,
        pending_approvals,
    }
}

fn events_from_stream(
    session_id: &str,
    turn_id: &str,
    stream_events: Vec<ModelStreamEvent>,
    driver: LiveModelDriver,
) -> Result<LiveModelTurnResult, String> {
    let mut events = Vec::new();
    let mut assembled = String::new();
    let mut tool_calls = Vec::new();
    let mut stop_reason = AgentModelStopReason::Stop;
    let mut usage = mutsuki_agent_contracts::AgentUsage::default();
    for event in stream_events {
        match event {
            ModelStreamEvent::MessageDelta { text, .. } => {
                assembled.push_str(&text);
                events.push((
                    AgentEventMeta::new(
                        format!("evt-delta-{turn_id}-{}", events.len()),
                        "model delta",
                    )
                    .with_turn(turn_id),
                    AgentEvent::ModelDelta {
                        turn_id: turn_id.into(),
                        text,
                    },
                ));
            }
            ModelStreamEvent::ReasoningDelta { text, .. } => {
                events.push((
                    AgentEventMeta::new(
                        format!("evt-reason-{turn_id}-{}", events.len()),
                        "reasoning delta",
                    )
                    .with_turn(turn_id),
                    AgentEvent::ReasoningDelta {
                        turn_id: turn_id.into(),
                        text,
                    },
                ));
            }
            ModelStreamEvent::ToolCallDelta { value, .. } => {
                if let Ok(call) = serde_json::from_value::<AgentToolCall>(value) {
                    tool_calls.push(call);
                }
            }
            ModelStreamEvent::Usage { value, .. } => {
                usage = value;
            }
            ModelStreamEvent::Completed { result, .. } => {
                stop_reason = result.stop_reason;
                tool_calls = result.tool_calls;
                usage = result.usage;
            }
            ModelStreamEvent::Failed { error, .. } => {
                return Err(map_protocol_error(error));
            }
        }
    }
    let pending_approvals =
        append_tool_and_approval_events(&mut events, session_id, turn_id, &tool_calls);
    let waiting_approval = !pending_approvals.is_empty();
    let summary = if waiting_approval {
        format!(
            "Live Model Adapter requested approval for {} tool call(s)",
            tool_calls.len()
        )
    } else if assembled.is_empty() {
        "Live Model Adapter stream complete".into()
    } else {
        assembled
    };
    if !waiting_approval {
        events.push((
            AgentEventMeta::new(format!("evt-final-{turn_id}"), "final response")
                .with_turn(turn_id),
            AgentEvent::FinalResponse {
                turn_id: turn_id.into(),
                summary: summary.clone(),
                result: None,
            },
        ));
    }
    let status = if waiting_approval {
        "waiting_approval"
    } else {
        "completed"
    };
    events.push((
        AgentEventMeta::new(format!("evt-turn-done-{turn_id}"), "turn completed")
            .with_turn(turn_id),
        AgentEvent::TurnState {
            turn_id: turn_id.into(),
            status: status.into(),
        },
    ));
    Ok(LiveModelTurnResult {
        events,
        tool_summary: json!({
            "driver": driver.as_str(),
            "official_servers": 0,
            "stop_reason": stop_reason,
            "tool_calls": tool_calls.len(),
            "waiting_approval": waiting_approval,
            "usage": usage,
            "streamed": true,
        }),
        waiting_approval,
        pending_approvals,
    })
}

/// Drive one turn through the selected protocol-level HTTP Adapter.
pub fn drive_live_model_turn(
    broker: &CredentialBrokerService,
    plan: &LiveModelTurnPlan,
    session_id: &str,
    turn_id: &str,
    prompt: &str,
) -> Result<LiveModelTurnResult, String> {
    match plan.driver {
        LiveModelDriver::OpenAiCompatible => {
            drive_openai_turn(broker, plan, session_id, turn_id, prompt)
        }
        LiveModelDriver::AnthropicMessages => {
            drive_anthropic_turn(broker, plan, session_id, turn_id, prompt)
        }
    }
}

fn drive_openai_turn(
    broker: &CredentialBrokerService,
    plan: &LiveModelTurnPlan,
    session_id: &str,
    turn_id: &str,
    prompt: &str,
) -> Result<LiveModelTurnResult, String> {
    let credentials = adapter_credential_broker(broker.clone());
    let adapter = OpenAiCompatibleAdapter::new(openai_adapter_descriptor(), credentials)
        .map_err(map_protocol_error)?;
    let request = ModelGenerateRequest {
        request: AgentModelGenerateRequest {
            model: plan.model.clone(),
            messages: vec![AgentMessage::user(prompt)],
            temperature: None,
            max_output_tokens: Some(1_024),
            provider_hint: Some(plan.provider.provider_id.clone()),
            metadata: None,
            result_protocol_id: None,
            result_context: None,
            session_id: Some(session_id.to_string()),
        },
        tools: coding_tools(),
        structured_output: None,
        reasoning: None,
    };
    let generated = block_on_adapter(adapter.generate(plan.provider.clone(), request))?
        .map_err(map_protocol_error)?;
    Ok(events_from_generate(
        session_id,
        turn_id,
        generated,
        LiveModelDriver::OpenAiCompatible,
    ))
}

fn drive_anthropic_turn(
    broker: &CredentialBrokerService,
    plan: &LiveModelTurnPlan,
    session_id: &str,
    turn_id: &str,
    prompt: &str,
) -> Result<LiveModelTurnResult, String> {
    let credentials = adapter_credential_broker(broker.clone());
    let adapter =
        AnthropicMessagesAdapter::new(AnthropicMessagesAdapter::default_descriptor(), credentials)
            .map_err(map_protocol_error)?;
    let request = ModelGenerateRequest {
        request: AgentModelGenerateRequest {
            model: plan.model.clone(),
            messages: vec![AgentMessage::user(prompt)],
            temperature: None,
            max_output_tokens: Some(1_024),
            provider_hint: Some(plan.provider.provider_id.clone()),
            metadata: None,
            result_protocol_id: None,
            result_context: None,
            session_id: Some(session_id.to_string()),
        },
        tools: coding_tools(),
        structured_output: None,
        reasoning: None,
    };
    let generated = block_on_adapter(adapter.generate(plan.provider.clone(), request))?
        .map_err(map_protocol_error)?;
    Ok(events_from_generate(
        session_id,
        turn_id,
        generated,
        LiveModelDriver::AnthropicMessages,
    ))
}

/// Drive a turn via Adapter SSE stream (loopback / recorded SSE fixtures).
pub fn drive_live_model_turn_streaming(
    broker: &CredentialBrokerService,
    plan: &LiveModelTurnPlan,
    session_id: &str,
    turn_id: &str,
    prompt: &str,
) -> Result<LiveModelTurnResult, String> {
    if plan.driver != LiveModelDriver::OpenAiCompatible {
        return Err("streaming is only wired for openai-compatible in this slice".into());
    }
    let credentials = adapter_credential_broker(broker.clone());
    let adapter = OpenAiCompatibleAdapter::new(openai_adapter_descriptor(), credentials)
        .map_err(map_protocol_error)?;
    let request = ModelGenerateRequest {
        request: AgentModelGenerateRequest {
            model: plan.model.clone(),
            messages: vec![AgentMessage::user(prompt)],
            temperature: None,
            max_output_tokens: Some(1_024),
            provider_hint: Some(plan.provider.provider_id.clone()),
            metadata: None,
            result_protocol_id: None,
            result_context: None,
            session_id: Some(session_id.to_string()),
        },
        tools: coding_tools(),
        structured_output: None,
        reasoning: None,
    };
    let stream_events = block_on_adapter(adapter.stream(plan.provider.clone(), request))?
        .map_err(map_protocol_error)?;
    events_from_stream(
        session_id,
        turn_id,
        stream_events,
        LiveModelDriver::OpenAiCompatible,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::{ProductCredentialBridge, ProductCredentialLoginInput};
    use crate::profile::build_product_coding_profile;
    use mutsuki_agent_contracts::{CredentialKind, OPENAI_CREDENTIAL_PROVIDER_ID};
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn endpoint_prefers_override_then_env_then_default() {
        let previous = std::env::var(ENV_MODEL_ENDPOINT).ok();
        std::env::remove_var(ENV_MODEL_ENDPOINT);
        assert_eq!(
            resolve_model_endpoint(None),
            DEFAULT_OPENAI_COMPATIBLE_ENDPOINT
        );
        assert_eq!(
            resolve_model_endpoint(Some("http://127.0.0.1:9/v1")),
            "http://127.0.0.1:9/v1"
        );
        std::env::set_var(ENV_MODEL_ENDPOINT, "http://127.0.0.1:8/v1");
        assert_eq!(resolve_model_endpoint(None), "http://127.0.0.1:8/v1");
        match previous {
            Some(value) => std::env::set_var(ENV_MODEL_ENDPOINT, value),
            None => std::env::remove_var(ENV_MODEL_ENDPOINT),
        }
    }

    #[test]
    fn live_http_adapter_generates_model_delta_via_loopback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 16_384];
            let _ = stream.read(&mut bytes).unwrap();
            let payload = r#"{"choices":[{"message":{"role":"assistant","content":"live adapter hello"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}"#;
            let body = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
            stream.write_all(body.as_bytes()).unwrap();
        });

        let bridge = ProductCredentialBridge::new();
        bridge
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: Some("openai".into()),
                source: Some("user_api_key".into()),
            })
            .unwrap();
        let profile = build_product_coding_profile(&bridge, None).unwrap();
        assert!(live_model_adapter_eligible(&profile));
        let plan = build_live_turn_plan(
            &profile,
            &format!("http://{address}/v1/chat/completions"),
            None,
        )
        .expect("live plan");
        assert_eq!(plan.driver, LiveModelDriver::OpenAiCompatible);
        let result = drive_live_model_turn(
            bridge.broker(),
            &plan,
            "sess-live",
            "turn-live",
            "hello from product",
        )
        .unwrap();
        assert!(!result.waiting_approval);
        assert!(result.events.iter().any(|(_, event)| matches!(
            event,
            AgentEvent::ModelDelta { text, .. } if text.contains("live adapter hello")
        )));
        assert_eq!(result.tool_summary["official_servers"], 0);
        assert_eq!(result.tool_summary["driver"], "openai-compatible");
        server.join().unwrap();
    }

    #[test]
    fn live_http_adapter_stream_deltas_via_loopback_sse() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 16_384];
            let _ = stream.read(&mut bytes).unwrap();
            let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"stream \"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\n\
data: [DONE]\n\n";
            let body = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",
                sse.len()
            );
            stream.write_all(body.as_bytes()).unwrap();
        });

        let bridge = ProductCredentialBridge::new();
        bridge
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: Some("openai-stream".into()),
                source: Some("user_api_key".into()),
            })
            .unwrap();
        let profile = build_product_coding_profile(&bridge, None).unwrap();
        let plan = build_live_turn_plan(
            &profile,
            &format!("http://{address}/v1/chat/completions"),
            None,
        )
        .expect("live plan");
        let result = drive_live_model_turn_streaming(
            bridge.broker(),
            &plan,
            "sess-stream",
            "turn-stream",
            "stream please",
        )
        .unwrap();
        assert!(!result.waiting_approval);
        assert_eq!(result.tool_summary["streamed"], true);
        assert_eq!(result.tool_summary["official_servers"], 0);
        let deltas: String = result
            .events
            .iter()
            .filter_map(|(_, event)| match event {
                AgentEvent::ModelDelta { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            deltas.contains("stream") && deltas.contains("hello"),
            "expected streamed deltas, got {deltas:?}"
        );
        server.join().unwrap();
    }
}

use std::time::Duration;

use lilia_agent::NativeControlModelRequest;
use reqwest::blocking::Client;
use serde_json::{json, Value as JsonValue};

use super::settings::DesktopConversationSuggestionSource;
use crate::application::{DesktopApplication, ASSISTANT_AI_CREDENTIAL_KEY};
use lilia_feature_suggestions::generation::suggestion_system_instruction;
use lilia_feature_suggestions::types::DesktopSuggestionModelRequest;

const AGENTKIT_CONTROL_MODEL_ENDPOINT: &str = "agentkit://control-model";
const AGENTKIT_DEFAULT_MODEL: &str = "product-default";

/// Host-supplied model resolver + HTTP caller for conversation suggestions.
pub trait ConversationSuggestionModelPort: Send + Sync {
    fn resolve_requests(&self) -> Vec<DesktopSuggestionModelRequest>;

    fn request_completion(
        &self,
        model: &DesktopSuggestionModelRequest,
        prompt: &str,
    ) -> Result<String, String> {
        request_model_completion(model, prompt)
    }
}

/// Host-neutral model port used by Native surfaces.
///
/// Assistant AI keeps its dedicated product configuration. Provider-backed
/// suggestions use AgentKit's control-model path so secrets stay behind the
/// Credential Broker and protocol selection remains runtime-owned.
#[derive(Clone)]
pub struct DesktopApplicationSuggestionModelPort {
    application: DesktopApplication,
}

impl DesktopApplicationSuggestionModelPort {
    pub fn new(application: DesktopApplication) -> Self {
        Self { application }
    }

    fn assistant_ai_request(&self) -> Option<DesktopSuggestionModelRequest> {
        let assistant = self.application.assistant_ai_settings().ok()?;
        let features = self.application.model_feature_settings().ok()?;
        let base_url = assistant.base_url?.trim().trim_end_matches('/').to_owned();
        let model = features.suggestion.or(assistant.model)?.trim().to_owned();
        let api_key = self
            .application
            .read_host_credential_text(ASSISTANT_AI_CREDENTIAL_KEY)?
            .trim()
            .to_owned();
        if base_url.is_empty() || model.is_empty() || api_key.is_empty() {
            return None;
        }
        Some(DesktopSuggestionModelRequest {
            source: DesktopConversationSuggestionSource::AssistantAi,
            backend: None,
            model,
            base_url,
            api_key,
        })
    }

    fn provider_request(&self) -> Option<DesktopSuggestionModelRequest> {
        let provider = self.application.provider_snapshot();
        if !provider.runtime.live_model_adapter_drives_turn {
            return None;
        }
        let settings = self.application.provider_runtime_settings().ok()?;
        let mut active_credentials = provider
            .credentials
            .iter()
            .filter(|credential| credential.model_inference)
            .map(|credential| {
                format!(
                    "{}:{}:{}",
                    credential.provider_id, credential.credential_id, credential.revision
                )
            })
            .collect::<Vec<_>>();
        active_credentials.sort();
        Some(DesktopSuggestionModelRequest {
            source: DesktopConversationSuggestionSource::Provider,
            backend: Some(format!("agentkit:{}", active_credentials.join(","))),
            model: settings
                .model
                .unwrap_or_else(|| AGENTKIT_DEFAULT_MODEL.to_owned()),
            base_url: AGENTKIT_CONTROL_MODEL_ENDPOINT.to_owned(),
            api_key: String::new(),
        })
    }
}

impl ConversationSuggestionModelPort for DesktopApplicationSuggestionModelPort {
    fn resolve_requests(&self) -> Vec<DesktopSuggestionModelRequest> {
        let request = match self.application.conversation_suggestion_settings() {
            Ok(settings) => match settings.source {
                DesktopConversationSuggestionSource::AssistantAi => self.assistant_ai_request(),
                DesktopConversationSuggestionSource::Provider => self.provider_request(),
            },
            Err(_) => None,
        };
        request.into_iter().collect()
    }

    fn request_completion(
        &self,
        model: &DesktopSuggestionModelRequest,
        prompt: &str,
    ) -> Result<String, String> {
        if model.source == DesktopConversationSuggestionSource::Provider
            && model.base_url == AGENTKIT_CONTROL_MODEL_ENDPOINT
        {
            return self
                .application
                .authority()
                .shared_runtime()
                .inner()
                .generate_control_text(NativeControlModelRequest {
                    system_instruction: suggestion_system_instruction().to_owned(),
                    prompt: prompt.to_owned(),
                    model: None,
                    max_output_tokens: 400,
                    reasoning: Some("low".to_owned()),
                })
                .map(|result| result.text)
                .map_err(|error| error.to_string());
        }
        request_model_completion(model, prompt)
    }
}

pub fn request_model_completion(
    model: &DesktopSuggestionModelRequest,
    prompt: &str,
) -> Result<String, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("HTTP client failed: {error}"))?;
    if model.backend.as_deref() == Some("claude") {
        request_anthropic(&client, model, prompt)
    } else {
        request_openai_compatible(&client, model, prompt)
    }
}

fn request_openai_compatible(
    client: &Client,
    model: &DesktopSuggestionModelRequest,
    prompt: &str,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", model.base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .bearer_auth(&model.api_key)
        .json(&json!({
            "model": model.model,
            "messages": [
                { "role": "system", "content": suggestion_system_instruction() },
                { "role": "user", "content": prompt }
            ],
            "temperature": 0.2,
            "max_tokens": 400
        }))
        .send()
        .map_err(|error| format!("suggestion request failed: {error}"))?;
    if !resp.status().is_success() {
        return Err(format!("suggestion request HTTP {}", resp.status()));
    }
    let value = resp
        .json::<JsonValue>()
        .map_err(|error| format!("suggestion response parse failed: {error}"))?;
    value
        .get("choices")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "suggestion response missing content".to_string())
}

fn request_anthropic(
    client: &Client,
    model: &DesktopSuggestionModelRequest,
    prompt: &str,
) -> Result<String, String> {
    let url = format!("{}/v1/messages", model.base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .header("x-api-key", &model.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&json!({
            "model": model.model,
            "max_tokens": 400,
            "system": suggestion_system_instruction(),
            "messages": [
                { "role": "user", "content": prompt }
            ]
        }))
        .send()
        .map_err(|error| format!("suggestion request failed: {error}"))?;
    if !resp.status().is_success() {
        return Err(format!("suggestion request HTTP {}", resp.status()));
    }
    let value = resp
        .json::<JsonValue>()
        .map_err(|error| format!("suggestion response parse failed: {error}"))?;
    value
        .get("content")
        .and_then(|value| value.as_array())
        .and_then(|items| {
            items.iter().find_map(|item| {
                if item.get("type").and_then(|value| value.as_str()) == Some("text") {
                    item.get("text").and_then(|value| value.as_str())
                } else {
                    None
                }
            })
        })
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "suggestion response missing content".to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tempfile::TempDir;

    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopAssistantAiSettingsUpdate, DesktopCredentialAction,
        DesktopHost, DesktopHostAction, DesktopHostContext, DesktopHostError, DesktopHostResult,
        DesktopSecret,
    };

    struct SecretHost;

    impl DesktopHost for SecretHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            match action {
                DesktopHostAction::Credential(DesktopCredentialAction::Read { key })
                    if key == ASSISTANT_AI_CREDENTIAL_KEY =>
                {
                    Ok(DesktopHostResult::Credential(Some(DesktopSecret::new(
                        b"test-secret".to_vec(),
                    ))))
                }
                _ => Ok(DesktopHostResult::Completed),
            }
        }
    }

    #[test]
    fn application_port_resolves_assistant_ai_from_shared_settings_and_host_secret() {
        let home = TempDir::new().unwrap();
        let application = DesktopApplication::bootstrap(
            DesktopApplicationConfig::new(home.path(), "suggestion-model-port").unwrap(),
            Arc::new(SecretHost),
        )
        .unwrap();
        application
            .save_assistant_ai_settings(DesktopAssistantAiSettingsUpdate {
                expected_revision: 1,
                base_url: Some("https://assistant.example.test/v1/".to_owned()),
                model: Some("suggestion-model".to_owned()),
                model_pool: Vec::new(),
                codex_account_spark_enabled: false,
            })
            .unwrap();

        let requests = DesktopApplicationSuggestionModelPort::new(application).resolve_requests();

        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].source,
            DesktopConversationSuggestionSource::AssistantAi
        );
        assert_eq!(requests[0].base_url, "https://assistant.example.test/v1");
        assert_eq!(requests[0].model, "suggestion-model");
        assert_eq!(requests[0].api_key, "test-secret");
    }
}

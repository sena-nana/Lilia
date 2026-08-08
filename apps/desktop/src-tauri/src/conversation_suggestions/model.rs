use std::time::Duration;

use reqwest::blocking::Client;
use serde_json::{json, Value as JsonValue};
use tauri::AppHandle;

use crate::chat::state::default_model_for_backend;
use crate::prompt_contract;
use crate::provider::{
    assistant_ai_secret, backend_api_key_env, backend_direct_url, load_active_backend,
    load_assistant_ai_config, load_model_feature_settings, resolve_connection_for,
    AssistantAIConfig, BackendConnectionPlan, ConnectionMode,
};
use crate::BACKEND_CLAUDE;

use super::types::{ModelRequest, SuggestionSettings, SuggestionSource};

pub(super) fn resolve_model_requests(
    app: &AppHandle,
    settings: &SuggestionSettings,
) -> Vec<ModelRequest> {
    let mut requests = Vec::new();
    match &settings.source {
        SuggestionSource::AssistantAi => {
            requests.extend(assistant_ai_model_request(app));
        }
        SuggestionSource::Provider => {
            requests.extend(provider_model_request(app));
        }
    }
    requests
}

fn assistant_ai_model_request(app: &AppHandle) -> Option<ModelRequest> {
    let cfg: AssistantAIConfig = load_assistant_ai_config(app);
    let base_url = cfg.base_url?.trim().trim_end_matches('/').to_string();
    let api_key = assistant_ai_secret().ok().flatten()?;
    let model = load_model_feature_settings(app)
        .suggestion
        .or(cfg.model)?
        .trim()
        .to_string();
    if base_url.is_empty() || api_key.is_empty() || model.is_empty() {
        return None;
    }
    Some(ModelRequest {
        source: SuggestionSource::AssistantAi,
        backend: None,
        model,
        base_url,
        api_key,
    })
}

fn provider_model_request(app: &AppHandle) -> Option<ModelRequest> {
    let backend = load_active_backend(app);
    let plan = resolve_connection_for(app, &backend);
    if plan.mode == ConnectionMode::CodexAccount {
        return None;
    }
    let base_url = effective_base_url(&backend, &plan)?;
    let api_key = provider_api_key(&backend, plan.api_key.as_deref())?;
    Some(ModelRequest {
        source: SuggestionSource::Provider,
        model: default_model_for_backend(&backend).to_string(),
        backend: Some(backend),
        base_url,
        api_key,
    })
}

fn provider_api_key(backend: &str, plan_api_key: Option<&str>) -> Option<String> {
    plan_api_key
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var(backend_api_key_env(backend))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|key| !key.is_empty())
        })
}

fn effective_base_url(backend: &str, plan: &BackendConnectionPlan) -> Option<String> {
    let base_url = plan
        .base_url
        .clone()
        .unwrap_or_else(|| backend_direct_url(backend).to_string())
        .trim()
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        None
    } else {
        Some(base_url)
    }
}

pub(super) fn request_model(
    app: &AppHandle,
    model: &ModelRequest,
    prompt: &str,
) -> Result<String, String> {
    let _ = app;
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTP client failed: {e}"))?;
    if model.backend.as_deref() == Some(BACKEND_CLAUDE) {
        request_anthropic(&client, model, prompt)
    } else {
        request_openai_compatible(&client, model, prompt)
    }
}

fn request_openai_compatible(
    client: &Client,
    model: &ModelRequest,
    prompt: &str,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", model.base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .bearer_auth(&model.api_key)
        .json(&json!({
            "model": model.model,
            "messages": [
                { "role": "system", "content": prompt_contract::suggestion_system_instruction() },
                { "role": "user", "content": prompt }
            ],
            "temperature": 0.2,
            "max_tokens": 400
        }))
        .send()
        .map_err(|e| format!("suggestion request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("suggestion request HTTP {}", resp.status()));
    }
    let value = resp
        .json::<JsonValue>()
        .map_err(|e| format!("suggestion response parse failed: {e}"))?;
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
    model: &ModelRequest,
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
            "system": prompt_contract::suggestion_system_instruction(),
            "messages": [
                { "role": "user", "content": prompt }
            ]
        }))
        .send()
        .map_err(|e| format!("suggestion request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("suggestion request HTTP {}", resp.status()));
    }
    let value = resp
        .json::<JsonValue>()
        .map_err(|e| format!("suggestion response parse failed: {e}"))?;
    value
        .get("content")
        .and_then(|value| value.as_array())
        .and_then(|items| {
            items.iter().find_map(|item| {
                if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                    item.get("text").and_then(|v| v.as_str())
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

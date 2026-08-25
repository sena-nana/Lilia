use std::time::Duration;

use reqwest::blocking::Client;
use serde_json::{json, Value as JsonValue};

#[derive(Clone, Debug)]
pub(crate) struct DesktopAuxiliaryModelRequest {
    pub(crate) base_url: String,
    pub(crate) model: String,
    pub(crate) api_key: String,
}

pub(crate) fn request_auxiliary_model_text(
    model: &DesktopAuxiliaryModelRequest,
    system_instruction: &str,
    prompt: &str,
    max_tokens: u32,
    timeout: Duration,
    purpose: &str,
) -> Result<String, String> {
    let client = Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|error| format!("{purpose} HTTP client failed: {error}"))?;
    let url = format!("{}/chat/completions", model.base_url.trim_end_matches('/'));
    let response = client
        .post(url)
        .bearer_auth(&model.api_key)
        .json(&json!({
            "model": model.model,
            "messages": [
                { "role": "system", "content": system_instruction },
                { "role": "user", "content": prompt }
            ],
            "temperature": 0.2,
            "max_tokens": max_tokens
        }))
        .send()
        .map_err(|error| format!("{purpose} request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("{purpose} request HTTP {}", response.status()));
    }
    let value = response
        .json::<JsonValue>()
        .map_err(|error| format!("{purpose} response parse failed: {error}"))?;
    value
        .get("choices")
        .and_then(JsonValue::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{purpose} response missing content"))
}

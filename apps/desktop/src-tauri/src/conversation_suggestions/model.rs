use lilia_desktop_application::{ConversationSuggestionModelPort, DesktopSuggestionModelRequest};
use tauri::AppHandle;

use super::types::{SuggestionSettings, SuggestionSource};
use crate::chat::state::default_model_for_backend;
use crate::provider::{
    assistant_ai_secret, backend_api_key_env, backend_direct_url, load_active_backend,
    load_assistant_ai_config, load_model_feature_settings, resolve_connection_for, ConnectionMode,
};

pub(super) struct TauriSuggestionModelPort {
    app: AppHandle,
    settings: SuggestionSettings,
}

impl TauriSuggestionModelPort {
    pub(super) fn new(app: AppHandle, settings: &SuggestionSettings) -> Self {
        Self {
            app,
            settings: settings.clone(),
        }
    }
}

impl ConversationSuggestionModelPort for TauriSuggestionModelPort {
    fn resolve_requests(&self) -> Vec<DesktopSuggestionModelRequest> {
        match self.settings.source {
            SuggestionSource::AssistantAi => {
                assistant_ai_model_request(&self.app).into_iter().collect()
            }
            SuggestionSource::Provider => provider_model_request(&self.app).into_iter().collect(),
        }
    }
}

fn assistant_ai_model_request(app: &AppHandle) -> Option<DesktopSuggestionModelRequest> {
    let cfg = load_assistant_ai_config(app);
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
    Some(DesktopSuggestionModelRequest {
        source: lilia_desktop_application::DesktopConversationSuggestionSource::AssistantAi,
        backend: None,
        model,
        base_url,
        api_key,
    })
}

fn provider_model_request(app: &AppHandle) -> Option<DesktopSuggestionModelRequest> {
    let backend = load_active_backend(app);
    let plan = resolve_connection_for(app, &backend);
    if plan.mode == ConnectionMode::CodexAccount {
        return None;
    }
    let base_url = plan
        .base_url
        .clone()
        .unwrap_or_else(|| backend_direct_url(&backend).to_string())
        .trim()
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        return None;
    }
    let api_key = plan
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var(backend_api_key_env(&backend))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|key| !key.is_empty())
        })?;
    Some(DesktopSuggestionModelRequest {
        source: lilia_desktop_application::DesktopConversationSuggestionSource::Provider,
        model: default_model_for_backend(&backend).to_string(),
        backend: Some(backend),
        base_url,
        api_key,
    })
}

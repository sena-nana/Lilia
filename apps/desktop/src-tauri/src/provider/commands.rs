use std::collections::HashMap;

use tauri::AppHandle;

use crate::chat::state::{chat_backend_supported, chat_backends};
use crate::settings_store::save_store_value;

use super::assistant_ai;
use super::config::{
    known_provider_key_for_backend, load_active_backend, load_agent_interaction_settings,
    load_model_feature_settings, load_router_mode, normalize_agent_interaction_settings,
    public_assistant_ai_config, public_provider_config, router_key_for_backend,
    router_mode_supported_for_backend, save_assistant_ai_config_metadata,
    save_model_feature_settings, save_provider_config_metadata, AGENT_INTERACTION_KEY,
    PROVIDER_ACTIVE_BACKEND_KEY,
};
use super::connection::build_backend_env_status;
use super::credentials::{apply_secret_update, assistant_ai_account, provider_account};
use super::subagents::{
    delete_custom_subagent, load_custom_subagents, upsert_custom_subagent,
    CustomSubagentUpsertInput,
};
use super::types::{
    AgentInteractionSettings, AssistantAIConfig, AssistantAIModelPoolItem, AssistantAIModelsResult,
    AssistantAITestResult, CodexAppServerStatus, CustomSubagentDefinition, EnvStatusReport,
    ModelFeatureSettings, ProviderConfig,
};

fn removed_codex_app_server_status() -> CodexAppServerStatus {
    CodexAppServerStatus {
        version: None,
        install_path: None,
        managed: false,
        available: false,
        supports_required_protocol: false,
        failure_kind: Some("removed".to_string()),
        issues: vec!["官方 Codex app-server 已移除".to_string()],
        latest_version: None,
        update_available: false,
        release_notes: Vec::new(),
        update_error: Some("官方 Codex app-server 已移除".to_string()),
        update_state: "idle".to_string(),
        prepared_version: None,
        update_progress_percent: None,
    }
}

fn chat_check_env_sync(app: AppHandle, _force_refresh: bool) -> EnvStatusReport {
    let mut backends = HashMap::new();
    for backend in chat_backends() {
        backends.insert(backend.clone(), build_backend_env_status(&app, backend));
    }

    let mut router_modes = HashMap::new();
    for backend in chat_backends() {
        router_modes.insert(backend.clone(), load_router_mode(&app, backend));
    }

    EnvStatusReport {
        node_available: false,
        codex_cli_available: false,
        codex_app_server: removed_codex_app_server_status(),
        router_modes,
        backends,
    }
}

#[tauri::command]
pub async fn chat_check_env(app: AppHandle, force_refresh: Option<bool>) -> EnvStatusReport {
    let force_refresh = force_refresh.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || chat_check_env_sync(app, force_refresh))
        .await
        .expect("chat_check_env blocking task panicked")
}

#[tauri::command]
pub fn provider_get_config(app: AppHandle, backend: String) -> ProviderConfig {
    public_provider_config(&app, &backend)
}

#[tauri::command]
pub fn provider_set_config(app: AppHandle, config: ProviderConfig) -> Result<(), String> {
    let key = known_provider_key_for_backend(&config.backend)?;
    let account = provider_account(&config.backend)?;
    apply_secret_update(&account, config.api_key.as_deref(), config.clear_api_key)?;
    save_provider_config_metadata(&app, key, config.backend, config.base_url)
}

#[tauri::command]
pub fn provider_get_active_backend(app: AppHandle) -> String {
    load_active_backend(&app)
}

#[tauri::command]
pub fn provider_set_active_backend(app: AppHandle, backend: String) -> Result<(), String> {
    if chat_backend_supported(&backend) {
        save_store_value(&app, PROVIDER_ACTIVE_BACKEND_KEY, &backend)
    } else {
        Err(format!("未知 backend: {backend}"))
    }
}

#[tauri::command]
pub fn assistant_ai_get_config(app: AppHandle) -> AssistantAIConfig {
    public_assistant_ai_config(&app)
}

#[tauri::command]
pub fn assistant_ai_set_config(app: AppHandle, config: AssistantAIConfig) -> Result<(), String> {
    apply_secret_update(
        assistant_ai_account(),
        config.api_key.as_deref(),
        config.clear_api_key,
    )?;
    save_assistant_ai_config_metadata(
        &app,
        config.base_url,
        config.model,
        config.model_pool,
        config.codex_account_spark_enabled,
    )
}

#[tauri::command]
pub fn assistant_ai_fetch_models(config: AssistantAIConfig) -> AssistantAIModelsResult {
    assistant_ai::fetch_models(config)
}

#[tauri::command]
pub fn model_feature_list_model_options(app: AppHandle) -> Vec<AssistantAIModelPoolItem> {
    public_assistant_ai_config(&app).model_pool
}

#[tauri::command]
pub fn model_feature_get_settings(app: AppHandle) -> ModelFeatureSettings {
    load_model_feature_settings(&app)
}

#[tauri::command]
pub fn model_feature_set_settings(
    app: AppHandle,
    settings: ModelFeatureSettings,
) -> Result<(), String> {
    save_model_feature_settings(&app, settings)
}

#[tauri::command]
pub fn agent_interaction_get_settings(app: AppHandle) -> AgentInteractionSettings {
    load_agent_interaction_settings(&app)
}

#[tauri::command]
pub fn agent_interaction_set_settings(
    app: AppHandle,
    settings: AgentInteractionSettings,
) -> Result<(), String> {
    let settings = normalize_agent_interaction_settings(Some(settings));
    save_store_value(&app, AGENT_INTERACTION_KEY, &settings)
}

#[tauri::command]
pub fn agent_interaction_list_subagents() -> Result<Vec<CustomSubagentDefinition>, String> {
    load_custom_subagents()
}

#[tauri::command]
pub fn agent_interaction_upsert_subagent(
    input: CustomSubagentUpsertInput,
) -> Result<CustomSubagentDefinition, String> {
    upsert_custom_subagent(input)
}

#[tauri::command]
pub fn agent_interaction_delete_subagent(id: String) -> Result<(), String> {
    delete_custom_subagent(&id)
}

#[tauri::command]
pub fn assistant_ai_test_connection(config: AssistantAIConfig) -> AssistantAITestResult {
    assistant_ai::test_connection(config)
}

#[tauri::command]
pub(crate) fn assistant_ai_optimize_prompt(
    app: AppHandle,
    input: assistant_ai::PromptOptimizeInput,
) -> Result<assistant_ai::PromptOptimizeResult, String> {
    assistant_ai::optimize_prompt(app, input)
}

#[tauri::command]
pub fn router_get_mode(app: AppHandle, backend: String) -> String {
    load_router_mode(&app, &backend)
}

#[tauri::command]
pub fn router_set_mode(app: AppHandle, backend: String, mode: String) -> Result<(), String> {
    let key = router_key_for_backend(&backend)?;
    if !router_mode_supported_for_backend(&backend, &mode) {
        return Err(format!("{backend} 不支持路由模式: {mode}"));
    }
    save_store_value(&app, key, &mode)
}

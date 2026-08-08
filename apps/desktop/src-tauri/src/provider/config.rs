use std::collections::HashMap;

use serde::Serialize;
#[cfg(feature = "legacy-runner")]
use serde_json::Map as JsonMap;
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Runtime};

use crate::chat::state::{default_backend, normalize_backend, try_normalize_backend};
use crate::chat_backends_contract::chat_backends_contract;
use crate::settings_store::{load_store_value, save_store_value};
use crate::BACKEND_CODEX;

use super::config_contract;
use super::credentials::{
    assistant_ai_account, has_secret, normalize_secret, provider_account, read_secret, write_secret,
};
#[cfg(feature = "legacy-runner")]
use super::subagents::{claude_managed_subagents, codex_subagent_instructions};
use super::types::{
    AgentInteractionSettings, AssistantAIConfig, AssistantAIModelPoolItem, CodexProfileSettings,
    ConnectionMode, ModelFeatureChatSettings, ModelFeatureSettings, ProviderConfig,
    SubagentModeSettings,
};

fn manifest_contains(values: &[String], value: &str) -> bool {
    values.iter().any(|item| item == value)
}

pub(crate) const PROVIDER_ACTIVE_BACKEND_KEY: &str = "provider.activeBackend";
pub(crate) const CC_SWITCH_KEY: &str = "cc-switch.config";
pub(crate) const ASSISTANT_AI_KEY: &str = "assistant-ai.config";
pub(crate) const MODEL_FEATURE_KEY: &str = "model-feature.config";
pub(crate) const AGENT_INTERACTION_KEY: &str = "agent-interaction.config";
pub(crate) const ROUTER_API: &str = "api";
pub(crate) const ROUTER_CODEX_ACCOUNT: &str = "codex-account";
const ROUTER_CC_SWITCH_LEGACY: &str = "cc-switch";
const ROUTER_DIRECT_LEGACY: &str = "direct";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderConfigMetadata {
    backend: String,
    base_url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AssistantAIConfigMetadata {
    base_url: Option<String>,
    model: Option<String>,
    model_pool: Vec<AssistantAIModelPoolItem>,
    codex_account_spark_enabled: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyApiSourceConfig {
    base_url: Option<String>,
}

pub(crate) fn provider_key_for_backend(backend: &str) -> &'static str {
    let backend = normalize_backend(backend);
    chat_backends_contract()
        .provider_store_keys
        .get(backend)
        .map(String::as_str)
        .expect("chat-backends.json missing providerStoreKeys backend")
}

pub(crate) fn known_provider_key_for_backend(backend: &str) -> Result<&'static str, String> {
    match try_normalize_backend(backend) {
        Some(backend) => Ok(provider_key_for_backend(backend)),
        None => Err(format!("未知 backend: {backend}")),
    }
}

pub(crate) fn router_key_for_backend(backend: &str) -> Result<&'static str, String> {
    match try_normalize_backend(backend) {
        Some(backend) => chat_backends_contract()
            .router_store_keys
            .get(backend)
            .map(String::as_str)
            .ok_or_else(|| {
                format!("chat-backends.json missing routerStoreKeys backend: {backend}")
            }),
        None => Err(format!("未知 backend: {backend}")),
    }
}

fn backend_for_provider_key(key: &str) -> Option<&'static str> {
    chat_backends_contract()
        .provider_store_keys
        .iter()
        .find_map(|(backend, provider_key)| (provider_key == key).then_some(backend.as_str()))
}

pub(crate) fn backend_api_key_env(backend: &str) -> String {
    let backend = normalize_backend(backend);
    chat_backends_contract()
        .api_key_env
        .get(backend)
        .cloned()
        .expect("chat-backends.json missing apiKeyEnv backend")
}

pub(crate) fn backend_direct_url(backend: &str) -> &'static str {
    let backend = normalize_backend(backend);
    chat_backends_contract()
        .direct_urls
        .get(backend)
        .map(String::as_str)
        .expect("chat-backends.json missing directUrls backend")
}

pub(crate) fn load_provider_config<R: Runtime>(
    app: &AppHandle<R>,
    key: &str,
) -> Option<ProviderConfig> {
    if let Err(err) = migrate_provider_config(app, key) {
        eprintln!("[provider] migrate provider config secret failed for {key}: {err}");
    }
    load_store_value(app, key)
}

pub(crate) fn save_provider_config_metadata<R: Runtime>(
    app: &AppHandle<R>,
    key: &str,
    backend: String,
    base_url: Option<String>,
) -> Result<(), String> {
    save_store_value(app, key, &ProviderConfigMetadata { backend, base_url })
}

pub(crate) fn public_provider_config<R: Runtime>(
    app: &AppHandle<R>,
    backend: &str,
) -> ProviderConfig {
    let mut config =
        load_provider_config(app, provider_key_for_backend(backend)).unwrap_or_else(|| {
            ProviderConfig {
                backend: backend.to_string(),
                base_url: None,
                api_key: None,
                has_api_key: false,
                clear_api_key: false,
            }
        });
    config.backend = normalize_backend(backend).to_string();
    config.has_api_key = provider_has_api_key(backend).unwrap_or(false);
    config.api_key = None;
    config.clear_api_key = false;
    config
}

pub(crate) fn public_assistant_ai_config<R: Runtime>(app: &AppHandle<R>) -> AssistantAIConfig {
    let mut config = load_assistant_ai_config(app);
    config.model_pool = normalize_model_pool(config.model_pool);
    config.has_api_key = assistant_ai_has_api_key().unwrap_or(false);
    config.api_key = None;
    config.clear_api_key = false;
    config
}

pub(crate) fn provider_api_key(backend: &str) -> Result<Option<String>, String> {
    read_secret(&provider_account(normalize_backend(backend))?)
}

pub(crate) fn assistant_ai_api_key() -> Result<Option<String>, String> {
    read_secret(assistant_ai_account())
}

pub(crate) fn assistant_ai_secret() -> Result<Option<String>, String> {
    Ok(assistant_ai_api_key()?
        .as_deref()
        .and_then(normalize_secret)
        .map(str::to_string))
}

pub(crate) fn provider_has_api_key(backend: &str) -> Result<bool, String> {
    has_secret(&provider_account(normalize_backend(backend))?)
}

pub(crate) fn assistant_ai_has_api_key() -> Result<bool, String> {
    has_secret(assistant_ai_account())
}

pub(crate) fn load_active_backend<R: Runtime>(app: &AppHandle<R>) -> String {
    load_store_value::<String, _>(app, PROVIDER_ACTIVE_BACKEND_KEY)
        .map(|s| normalize_backend(&s).to_string())
        .unwrap_or_else(|| default_backend().to_string())
}

pub(crate) fn load_legacy_cc_switch_base_url<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    load_store_value::<LegacyApiSourceConfig, _>(app, CC_SWITCH_KEY)
        .and_then(|config| normalize_optional_string(config.base_url))
}

pub(crate) fn uses_legacy_cc_switch_mode<R: Runtime>(app: &AppHandle<R>, backend: &str) -> bool {
    let backend = normalize_backend(backend);
    let key = router_key_for_backend(backend).expect("normalized backend must have a router key");
    load_store_value::<String, _>(app, key).as_deref() == Some(ROUTER_CC_SWITCH_LEGACY)
}

pub(crate) fn load_assistant_ai_config<R: Runtime>(app: &AppHandle<R>) -> AssistantAIConfig {
    if let Err(err) = migrate_assistant_ai_config(app) {
        eprintln!("[provider] migrate assistant-ai config secret failed: {err}");
    }
    let mut config: AssistantAIConfig = load_store_value(app, ASSISTANT_AI_KEY).unwrap_or_default();
    config.model_pool = normalize_model_pool(config.model_pool);
    config
}

pub(crate) fn save_assistant_ai_config_metadata<R: Runtime>(
    app: &AppHandle<R>,
    base_url: Option<String>,
    model: Option<String>,
    model_pool: Vec<AssistantAIModelPoolItem>,
    codex_account_spark_enabled: bool,
) -> Result<(), String> {
    save_store_value(
        app,
        ASSISTANT_AI_KEY,
        &AssistantAIConfigMetadata {
            base_url,
            model,
            model_pool: normalize_model_pool(model_pool),
            codex_account_spark_enabled,
        },
    )
}

pub(crate) fn load_model_feature_settings<R: Runtime>(app: &AppHandle<R>) -> ModelFeatureSettings {
    normalize_model_feature_settings(load_store_value(app, MODEL_FEATURE_KEY))
}

pub(crate) fn save_model_feature_settings<R: Runtime>(
    app: &AppHandle<R>,
    settings: ModelFeatureSettings,
) -> Result<(), String> {
    save_store_value(
        app,
        MODEL_FEATURE_KEY,
        &normalize_model_feature_settings(Some(settings)),
    )
}

pub(crate) fn normalize_model_pool(
    items: Vec<AssistantAIModelPoolItem>,
) -> Vec<AssistantAIModelPoolItem> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for item in items {
        let id = item.id.trim().to_string();
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        let label = item.label.trim();
        out.push(AssistantAIModelPoolItem {
            id: id.clone(),
            label: if label.is_empty() {
                id
            } else {
                label.to_string()
            },
            source: match item.source.trim() {
                "legacy" => "legacy".to_string(),
                _ => "remote".to_string(),
            },
            backend: normalize_backend(&item.backend).to_string(),
        });
    }
    out
}

pub(crate) fn normalize_model_feature_settings(
    settings: Option<ModelFeatureSettings>,
) -> ModelFeatureSettings {
    let settings = settings.unwrap_or_default();
    ModelFeatureSettings {
        chat: ModelFeatureChatSettings {
            light: normalize_optional_string(settings.chat.light),
            normal: normalize_optional_string(settings.chat.normal),
            deep: normalize_optional_string(settings.chat.deep),
        },
        title: normalize_optional_string(settings.title),
        suggestion: normalize_optional_string(settings.suggestion),
        prompt_router: normalize_optional_string(settings.prompt_router),
        prompt_optimize: normalize_optional_string(settings.prompt_optimize),
        auto_turn_decision: normalize_optional_string(settings.auto_turn_decision),
    }
}

fn migrate_provider_config<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<(), String> {
    let Some(backend) = backend_for_provider_key(key) else {
        return Ok(());
    };
    let Some(config) = load_store_value::<ProviderConfig, _>(app, key) else {
        return Ok(());
    };
    let Some(api_key) = config.api_key.as_deref().and_then(normalize_secret) else {
        return Ok(());
    };
    let account = provider_account(backend)?;
    if !has_secret(&account)? {
        write_secret(&account, api_key)?;
    }
    save_provider_config_metadata(app, key, config.backend, config.base_url)
}

fn migrate_assistant_ai_config<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let Some(config) = load_store_value::<AssistantAIConfig, _>(app, ASSISTANT_AI_KEY) else {
        return Ok(());
    };
    let Some(api_key) = config.api_key.as_deref().and_then(normalize_secret) else {
        return Ok(());
    };
    if !has_secret(assistant_ai_account())? {
        write_secret(assistant_ai_account(), api_key)?;
    }
    save_assistant_ai_config_metadata(
        app,
        config.base_url,
        config.model,
        config.model_pool,
        config.codex_account_spark_enabled,
    )
}

pub(crate) fn load_agent_interaction_settings<R: Runtime>(
    app: &AppHandle<R>,
) -> AgentInteractionSettings {
    normalize_agent_interaction_settings(load_store_value(app, AGENT_INTERACTION_KEY))
}

pub(crate) fn normalize_agent_interaction_settings(
    settings: Option<AgentInteractionSettings>,
) -> AgentInteractionSettings {
    let settings = settings.unwrap_or_default();
    let permission_mode_availability =
        normalize_permission_mode_availability(settings.permission_mode_availability);
    AgentInteractionSettings {
        non_interrupt_mode: settings.non_interrupt_mode,
        debug: settings.debug,
        permission_mode: normalize_available_permission_mode(
            &settings.permission_mode,
            &permission_mode_availability,
        ),
        permission_mode_availability,
        main_agent_prompt_mode: normalize_main_agent_prompt_mode(&settings.main_agent_prompt_mode),
        main_agent_custom_prompt: normalize_optional_string(Some(
            settings.main_agent_custom_prompt,
        ))
        .unwrap_or_default(),
        codex_profile: normalize_codex_profile_settings(settings.codex_profile),
        subagent_mode: normalize_subagent_mode_settings(settings.subagent_mode),
        auto_turn_decision: settings.auto_turn_decision,
    }
}

pub(crate) fn normalize_main_agent_prompt_mode(value: &str) -> String {
    match value {
        "aggressive" => "aggressive".to_string(),
        "custom" => "custom".to_string(),
        _ => "conservative".to_string(),
    }
}

pub(crate) fn normalize_permission_mode(value: &str) -> String {
    if manifest_contains(config_contract::permission_modes(), value) {
        value.to_string()
    } else {
        config_contract::default_permission_mode().to_string()
    }
}

pub(crate) fn normalize_permission_mode_availability(
    settings: HashMap<String, bool>,
) -> HashMap<String, bool> {
    let mut normalized = HashMap::new();
    for mode in config_contract::permission_modes() {
        let enabled = match mode.as_str() {
            "ask" | "readonly" => true,
            _ => settings.get(mode).copied().unwrap_or(true),
        };
        normalized.insert(mode.clone(), enabled);
    }
    normalized
}

pub(crate) fn normalize_available_permission_mode(
    value: &str,
    availability: &HashMap<String, bool>,
) -> String {
    let normalized = normalize_permission_mode(value);
    if availability.get(&normalized).copied().unwrap_or(false) {
        return normalized;
    }
    config_contract::default_permission_mode().to_string()
}

pub(crate) fn normalize_subagent_mode_settings(
    settings: SubagentModeSettings,
) -> SubagentModeSettings {
    SubagentModeSettings {
        enabled: settings.enabled,
        codex: settings.codex,
        claude: settings.claude,
    }
}

pub(crate) fn normalize_codex_profile_settings(
    settings: CodexProfileSettings,
) -> CodexProfileSettings {
    CodexProfileSettings {
        profile: if manifest_contains(
            config_contract::codex_settings_profiles(),
            &settings.profile,
        ) {
            settings.profile
        } else {
            config_contract::default_codex_settings_profile().to_string()
        },
        model: normalize_optional_string(settings.model),
        reasoning_effort: normalize_reasoning_effort(settings.reasoning_effort),
        runtime_workspace_roots: normalize_runtime_workspace_roots(
            settings.runtime_workspace_roots,
        ),
        responses_api_client_metadata: normalize_json_object(
            settings.responses_api_client_metadata,
        ),
        additional_context: normalize_optional_string(settings.additional_context),
        persist_extended_history: settings.persist_extended_history,
        initial_turns_page: normalize_json_object(settings.initial_turns_page),
        exclude_turns: normalize_string_list(settings.exclude_turns),
    }
}

pub(crate) fn normalize_json_object(value: Option<JsonValue>) -> Option<JsonValue> {
    match value {
        Some(JsonValue::Object(map)) if !map.is_empty() => Some(JsonValue::Object(map)),
        _ => None,
    }
}

pub(crate) fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub(crate) fn normalize_reasoning_effort(value: Option<String>) -> Option<String> {
    let value = normalize_optional_string(value)?;
    if chat_backends_contract()
        .backend_reasoning_efforts
        .get(BACKEND_CODEX)
        .is_some_and(|efforts| manifest_contains(efforts, &value))
    {
        Some(value)
    } else {
        None
    }
}

#[cfg(any(feature = "legacy-runner", test))]
pub(crate) fn normalize_codex_settings_profile(value: Option<String>) -> Option<String> {
    let value = normalize_optional_string(value)?;
    manifest_contains(config_contract::codex_settings_profiles(), &value).then_some(value)
}

pub(crate) fn normalize_runtime_workspace_roots(roots: Vec<String>) -> Vec<String> {
    normalize_string_list(roots)
}

pub(crate) fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || normalized.iter().any(|seen| seen == trimmed) {
            continue;
        }
        normalized.push(trimmed.to_string());
    }
    normalized
}

#[cfg(feature = "legacy-runner")]
pub(crate) fn build_effective_claude_settings<R: Runtime>(app: &AppHandle<R>) -> Option<JsonValue> {
    let settings = load_agent_interaction_settings(app).subagent_mode;
    if !settings.enabled || !settings.claude.enabled {
        return None;
    }
    let mut claude = JsonMap::new();
    claude.insert(
        "forwardSubagentText".to_string(),
        JsonValue::Bool(settings.claude.forward_subagent_text),
    );
    claude.insert(
        "agentProgressSummaries".to_string(),
        JsonValue::Bool(settings.claude.agent_progress_summaries),
    );
    match claude_managed_subagents() {
        Ok(Some(managed)) => {
            claude.insert("managedSettings".to_string(), managed);
        }
        Ok(None) => {}
        Err(err) => {
            eprintln!("[provider] load Claude subagents failed: {err}");
        }
    }
    Some(JsonValue::Object(claude))
}

#[cfg(feature = "legacy-runner")]
pub(crate) fn build_effective_codex_subagent_settings<R: Runtime>(
    app: &AppHandle<R>,
) -> Option<JsonValue> {
    let settings = load_agent_interaction_settings(app).subagent_mode;
    if !settings.enabled || !settings.codex.enabled {
        return None;
    }
    match codex_subagent_instructions() {
        Ok(Some(instructions)) => Some(serde_json::json!({
            "subagentInstructions": instructions,
        })),
        Ok(None) => None,
        Err(err) => {
            eprintln!("[provider] load Codex subagents failed: {err}");
            None
        }
    }
}

pub(crate) fn load_router_mode<R: Runtime>(app: &AppHandle<R>, backend: &str) -> String {
    let backend = normalize_backend(backend);
    let key = router_key_for_backend(backend).expect("normalized backend must have a router key");
    normalize_router_mode_value(backend, load_store_value::<String, _>(app, key).as_deref())
}

pub(crate) fn router_mode_supported_for_backend(backend: &str, mode: &str) -> bool {
    let backend = normalize_backend(backend);
    chat_backends_contract()
        .backend_router_modes
        .get(backend)
        .is_some_and(|modes| manifest_contains(modes, mode))
}

pub(crate) fn connection_mode_uses_api_key(mode: ConnectionMode) -> bool {
    manifest_contains(
        &chat_backends_contract().connection_modes_using_api_key,
        mode.as_contract_value(),
    )
}

pub(crate) fn connection_mode_uses_default_api(mode: ConnectionMode) -> bool {
    manifest_contains(
        &chat_backends_contract().connection_modes_using_default_api,
        mode.as_contract_value(),
    )
}

pub(crate) fn connection_mode_uses_custom_url(mode: ConnectionMode) -> bool {
    manifest_contains(
        &chat_backends_contract().connection_modes_using_custom_url,
        mode.as_contract_value(),
    )
}

pub(crate) fn connection_mode_uses_codex_account(mode: ConnectionMode) -> bool {
    manifest_contains(
        &chat_backends_contract().connection_modes_using_codex_account,
        mode.as_contract_value(),
    )
}

fn normalize_router_mode_value(backend: &str, value: Option<&str>) -> String {
    let backend = normalize_backend(backend);
    let value = match value {
        Some(ROUTER_DIRECT_LEGACY | ROUTER_CC_SWITCH_LEGACY) => Some(ROUTER_API),
        other => other,
    };
    let manifest = chat_backends_contract();
    if let Some(value) = value {
        if manifest
            .backend_router_modes
            .get(backend)
            .is_some_and(|modes| manifest_contains(modes, value))
        {
            return value.to_string();
        }
    }
    manifest
        .default_router_modes
        .get(backend)
        .cloned()
        .unwrap_or_else(|| ROUTER_API.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BACKEND_CLAUDE;

    #[test]
    fn codex_profile_settings_are_normalized_to_controlled_values() {
        assert_eq!(
            config_contract::permission_modes(),
            ["full", "ask", "readonly", "free"]
        );
        assert_eq!(config_contract::default_permission_mode(), "ask");
        assert_eq!(normalize_permission_mode("free"), "free");
        assert_eq!(normalize_permission_mode("danger"), "ask");
        assert_eq!(
            chat_backends_contract()
                .backend_reasoning_efforts
                .get(BACKEND_CODEX),
            Some(&vec![
                "low".to_string(),
                "medium".to_string(),
                "high".to_string(),
                "xhigh".to_string()
            ])
        );
        assert_eq!(
            config_contract::codex_settings_profiles(),
            ["default", "fast", "balanced", "deep"]
        );
        assert_eq!(config_contract::default_codex_settings_profile(), "default");
        assert_eq!(
            normalize_reasoning_effort(Some(" xhigh ".to_string())).as_deref(),
            Some("xhigh")
        );
        let normalized = normalize_codex_profile_settings(CodexProfileSettings {
            profile: "unknown".to_string(),
            model: Some("  gpt-5.5  ".to_string()),
            reasoning_effort: Some("ultra".to_string()),
            runtime_workspace_roots: vec![
                " C:/repo ".to_string(),
                "".to_string(),
                "C:/repo".to_string(),
                "D:/shared".to_string(),
            ],
            responses_api_client_metadata: Some(serde_json::json!({ "surface": "lilia" })),
            additional_context: Some("  extra context  ".to_string()),
            persist_extended_history: Some(true),
            initial_turns_page: Some(serde_json::json!([])),
            exclude_turns: vec![" turn-1 ".to_string(), "turn-1".to_string(), "".to_string()],
        });

        assert_eq!(normalized.profile, "default");
        assert_eq!(normalized.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(normalized.reasoning_effort, None);
        assert_eq!(
            normalized.runtime_workspace_roots,
            vec!["C:/repo", "D:/shared"]
        );
        assert_eq!(
            normalized.responses_api_client_metadata,
            Some(serde_json::json!({ "surface": "lilia" }))
        );
        assert_eq!(
            normalized.additional_context.as_deref(),
            Some("extra context")
        );
        assert_eq!(normalized.persist_extended_history, Some(true));
        assert_eq!(normalized.initial_turns_page, None);
        assert_eq!(normalized.exclude_turns, vec!["turn-1"]);
        assert_eq!(
            normalize_codex_settings_profile(Some(" balanced ".to_string())).as_deref(),
            Some("balanced")
        );
        assert_eq!(
            normalize_codex_settings_profile(Some("bad".to_string())),
            None
        );
    }

    #[test]
    fn subagent_mode_defaults_keep_backend_toggles_enabled() {
        let normalized = normalize_subagent_mode_settings(SubagentModeSettings::default());
        assert!(!normalized.enabled);
        assert!(normalized.codex.enabled);
        assert!(normalized.claude.enabled);
        assert!(normalized.claude.forward_subagent_text);
        assert!(normalized.claude.agent_progress_summaries);
    }

    #[test]
    fn main_agent_prompt_mode_normalizes_to_known_modes() {
        assert_eq!(normalize_main_agent_prompt_mode("aggressive"), "aggressive");
        assert_eq!(normalize_main_agent_prompt_mode("custom"), "custom");
        assert_eq!(normalize_main_agent_prompt_mode("unknown"), "conservative");
    }

    #[test]
    fn agent_interaction_custom_prompt_trims_outer_whitespace() {
        let normalized = normalize_agent_interaction_settings(Some(AgentInteractionSettings {
            main_agent_prompt_mode: "custom".to_string(),
            main_agent_custom_prompt: "  custom strategy\nwith details  ".to_string(),
            ..AgentInteractionSettings::default()
        }));

        assert_eq!(normalized.main_agent_prompt_mode, "custom");
        assert_eq!(
            normalized.main_agent_custom_prompt,
            "custom strategy\nwith details"
        );
    }

    #[test]
    fn provider_metadata_serialization_excludes_secret_fields() {
        let value = serde_json::to_value(ProviderConfigMetadata {
            backend: BACKEND_CODEX.to_string(),
            base_url: Some("https://api.example.com/v1".to_string()),
        })
        .unwrap();

        assert_eq!(
            value.get("backend").and_then(JsonValue::as_str),
            Some(BACKEND_CODEX)
        );
        assert_eq!(
            value.get("baseUrl").and_then(JsonValue::as_str),
            Some("https://api.example.com/v1")
        );
        assert!(value.get("apiKey").is_none());
        assert!(value.get("hasApiKey").is_none());
        assert!(value.get("clearApiKey").is_none());
    }

    #[test]
    fn assistant_ai_metadata_serialization_excludes_secret_fields() {
        let value = serde_json::to_value(AssistantAIConfigMetadata {
            base_url: Some("https://api.example.com/v1".to_string()),
            model: Some("mini".to_string()),
            model_pool: vec![AssistantAIModelPoolItem {
                id: "remote-mini".to_string(),
                label: "Remote Mini".to_string(),
                source: "remote".to_string(),
                backend: BACKEND_CODEX.to_string(),
            }],
            codex_account_spark_enabled: true,
        })
        .unwrap();

        assert_eq!(
            value.get("baseUrl").and_then(JsonValue::as_str),
            Some("https://api.example.com/v1")
        );
        assert_eq!(value.get("model").and_then(JsonValue::as_str), Some("mini"));
        assert_eq!(
            value
                .get("modelPool")
                .and_then(JsonValue::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            value
                .get("codexAccountSparkEnabled")
                .and_then(JsonValue::as_bool),
            Some(true)
        );
        assert!(value.get("apiKey").is_none());
        assert!(value.get("hasApiKey").is_none());
        assert!(value.get("clearApiKey").is_none());
    }

    #[test]
    fn router_mode_defaults_and_legacy_values_normalize_to_current_modes() {
        assert_eq!(
            crate::chat::state::chat_backends(),
            &[BACKEND_CLAUDE.to_string(), BACKEND_CODEX.to_string()]
        );
        assert!(crate::chat::state::chat_backend_supported(BACKEND_CLAUDE));
        assert!(crate::chat::state::chat_backend_supported(BACKEND_CODEX));
        assert!(!crate::chat::state::chat_backend_supported("unknown"));
        assert_eq!(
            provider_key_for_backend("unknown"),
            provider_key_for_backend(default_backend())
        );
        assert_eq!(
            normalize_router_mode_value("unknown", None),
            normalize_router_mode_value(default_backend(), None)
        );
        assert_eq!(
            backend_direct_url(BACKEND_CLAUDE),
            "https://api.anthropic.com"
        );
        assert_eq!(
            backend_direct_url(BACKEND_CODEX),
            "https://api.openai.com/v1"
        );
        assert_eq!(backend_api_key_env(BACKEND_CLAUDE), "ANTHROPIC_API_KEY");
        assert_eq!(backend_api_key_env(BACKEND_CODEX), "OPENAI_API_KEY");
        assert_eq!(provider_key_for_backend(BACKEND_CLAUDE), "provider.claude");
        assert_eq!(provider_key_for_backend(BACKEND_CODEX), "provider.codex");
        assert_eq!(
            known_provider_key_for_backend(&format!(" {BACKEND_CODEX} ")).unwrap(),
            "provider.codex"
        );
        assert!(known_provider_key_for_backend("unknown").is_err());
        assert_eq!(
            router_key_for_backend(BACKEND_CLAUDE).unwrap(),
            "router.claude"
        );
        assert_eq!(
            router_key_for_backend(BACKEND_CODEX).unwrap(),
            "router.codex"
        );
        assert!(router_key_for_backend("unknown").is_err());
        assert_eq!(
            backend_for_provider_key("provider.claude"),
            Some(BACKEND_CLAUDE)
        );
        assert_eq!(
            backend_for_provider_key("provider.codex"),
            Some(BACKEND_CODEX)
        );
        assert_eq!(
            chat_backends_contract()
                .backend_router_modes
                .get(BACKEND_CLAUDE),
            Some(&vec![ROUTER_API.to_string()])
        );
        assert_eq!(
            chat_backends_contract()
                .backend_router_modes
                .get(BACKEND_CODEX),
            Some(&vec![
                ROUTER_API.to_string(),
                ROUTER_CODEX_ACCOUNT.to_string()
            ])
        );
        assert_eq!(
            chat_backends_contract()
                .default_router_modes
                .get(BACKEND_CODEX),
            Some(&ROUTER_CODEX_ACCOUNT.to_string())
        );
        assert!(connection_mode_uses_api_key(ConnectionMode::Api));
        assert!(connection_mode_uses_api_key(ConnectionMode::CustomBaseUrl));
        assert!(!connection_mode_uses_api_key(ConnectionMode::CodexAccount));
        assert!(connection_mode_uses_default_api(ConnectionMode::Api));
        assert!(connection_mode_uses_custom_url(
            ConnectionMode::CustomBaseUrl
        ));
        assert!(connection_mode_uses_codex_account(
            ConnectionMode::CodexAccount
        ));
        assert!(!connection_mode_uses_default_api(
            ConnectionMode::Unconfigured
        ));
        assert_eq!(
            normalize_router_mode_value(BACKEND_CLAUDE, None),
            ROUTER_API
        );
        assert_eq!(
            normalize_router_mode_value(BACKEND_CODEX, None),
            ROUTER_CODEX_ACCOUNT
        );
        assert_eq!(
            normalize_router_mode_value(BACKEND_CLAUDE, Some(ROUTER_DIRECT_LEGACY)),
            ROUTER_API
        );
        assert_eq!(
            normalize_router_mode_value(BACKEND_CODEX, Some(ROUTER_CC_SWITCH_LEGACY)),
            ROUTER_API
        );
        assert_eq!(
            normalize_router_mode_value(BACKEND_CLAUDE, Some(ROUTER_CODEX_ACCOUNT)),
            ROUTER_API
        );
        assert_eq!(
            normalize_router_mode_value(BACKEND_CODEX, Some(ROUTER_CODEX_ACCOUNT)),
            ROUTER_CODEX_ACCOUNT
        );
        assert!(router_mode_supported_for_backend(
            BACKEND_CODEX,
            ROUTER_CODEX_ACCOUNT
        ));
        assert!(!router_mode_supported_for_backend(
            BACKEND_CLAUDE,
            ROUTER_CODEX_ACCOUNT
        ));
        assert!(!router_mode_supported_for_backend(BACKEND_CODEX, "unknown"));
    }
}

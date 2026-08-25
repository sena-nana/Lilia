//! Host-neutral Model feature / Assistant AI / Router mode settings.
//!
//! Credentials stay in the OS Keyring via host credential actions; only
//! non-secret metadata is persisted here with optimistic revision checks.

use std::collections::BTreeMap;

use lilia_storage::SqliteAgentRuntimeStateStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::application::{
    DesktopApplication, DesktopApplicationError, DesktopCredentialAction, DesktopEventKind,
    DesktopHostAction, DesktopHostResult, DesktopSecret,
};

pub const MODEL_FEATURE_SETTINGS_KEY: &str = "desktop.model-feature.settings.v1";
pub const ASSISTANT_AI_SETTINGS_KEY: &str = "desktop.assistant-ai.settings.v1";
pub const ASSISTANT_AI_CREDENTIAL_KEY: &str = "assistant-ai";
pub const ROUTER_MODE_SETTINGS_KEY: &str = "desktop.router-mode.settings.v1";

const MODEL_FEATURE_SCHEMA_VERSION: u32 = 1;
const ASSISTANT_AI_SCHEMA_VERSION: u32 = 1;
const ROUTER_MODE_SCHEMA_VERSION: u32 = 1;

const BUILTIN_PRESET_SPECS: &[(&str, &str, &str)] = &[
    ("fast", "Fast", "light"),
    ("default", "Default", "normal"),
    ("plan", "Plan", "deep"),
    ("review", "Review", "deep"),
];

#[derive(Debug, Error)]
pub enum DesktopProviderUiSettingsError {
    #[error("provider UI settings persistence failed: {0}")]
    Persistence(String),
    #[error("provider UI settings payload is corrupt: {0}")]
    Corrupt(String),
    #[error("unsupported provider UI settings schema {0}")]
    UnsupportedSchema(u32),
    #[error("provider UI settings revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("provider UI settings revision overflowed")]
    RevisionOverflow,
}

impl From<DesktopProviderUiSettingsError> for DesktopApplicationError {
    fn from(error: DesktopProviderUiSettingsError) -> Self {
        match error {
            DesktopProviderUiSettingsError::Persistence(message)
            | DesktopProviderUiSettingsError::Corrupt(message) => Self::InvalidInput {
                field: "provider_ui_settings",
                message,
            },
            DesktopProviderUiSettingsError::UnsupportedSchema(version) => Self::InvalidInput {
                field: "provider_ui_settings",
                message: format!("unsupported schema {version}"),
            },
            DesktopProviderUiSettingsError::RevisionConflict { expected, actual } => {
                Self::InvalidInput {
                    field: "provider_ui_settings",
                    message: format!("revision conflict: expected {expected}, actual {actual}"),
                }
            }
            DesktopProviderUiSettingsError::RevisionOverflow => {
                Self::StateRevisionOverflow("provider_ui_settings")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DesktopModelFeatureChatSettings {
    #[serde(default)]
    pub light: Option<String>,
    #[serde(default)]
    pub normal: Option<String>,
    #[serde(default)]
    pub deep: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopModelPresetGroup {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopModelFeatureSettings {
    pub revision: u64,
    #[serde(default)]
    pub chat: DesktopModelFeatureChatSettings,
    #[serde(default)]
    pub presets: Vec<DesktopModelPresetGroup>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub suggestion: Option<String>,
    #[serde(default)]
    pub prompt_router: Option<String>,
    #[serde(default)]
    pub prompt_optimize: Option<String>,
    #[serde(default)]
    pub auto_turn_decision: Option<String>,
}

impl Default for DesktopModelFeatureSettings {
    fn default() -> Self {
        normalize_model_feature_settings(Self {
            revision: 1,
            chat: DesktopModelFeatureChatSettings::default(),
            presets: Vec::new(),
            title: None,
            suggestion: None,
            prompt_router: None,
            prompt_optimize: None,
            auto_turn_decision: None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopModelFeatureSettingsUpdate {
    pub expected_revision: u64,
    #[serde(default)]
    pub chat: DesktopModelFeatureChatSettings,
    #[serde(default)]
    pub presets: Vec<DesktopModelPresetGroup>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub suggestion: Option<String>,
    #[serde(default)]
    pub prompt_router: Option<String>,
    #[serde(default)]
    pub prompt_optimize: Option<String>,
    #[serde(default)]
    pub auto_turn_decision: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredModelFeatureSettings {
    schema_version: u32,
    settings: DesktopModelFeatureSettings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAssistantAiModelPoolItem {
    pub id: String,
    pub label: String,
    #[serde(default = "default_model_pool_source")]
    pub source: String,
    #[serde(default = "default_model_pool_backend")]
    pub backend: String,
}

fn default_model_pool_source() -> String {
    "remote".to_owned()
}

fn default_model_pool_backend() -> String {
    "native-agentkit".to_owned()
}

/// Non-secret Assistant AI metadata. API keys stay in Keyring.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAssistantAiSettings {
    pub revision: u64,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_pool: Vec<DesktopAssistantAiModelPoolItem>,
    #[serde(default)]
    pub codex_account_spark_enabled: bool,
}

impl Default for DesktopAssistantAiSettings {
    fn default() -> Self {
        Self {
            revision: 1,
            base_url: None,
            model: None,
            model_pool: Vec::new(),
            codex_account_spark_enabled: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAssistantAiSettingsUpdate {
    pub expected_revision: u64,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_pool: Vec<DesktopAssistantAiModelPoolItem>,
    #[serde(default)]
    pub codex_account_spark_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopAssistantAiSecretUpdate {
    Keep,
    Set(DesktopSecret),
    Clear,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopAssistantAiConfigurationUpdate {
    pub settings: DesktopAssistantAiSettingsUpdate,
    pub secret: DesktopAssistantAiSecretUpdate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAssistantAiSettings {
    schema_version: u32,
    settings: DesktopAssistantAiSettings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopRouterModeSettings {
    pub revision: u64,
    #[serde(default)]
    pub modes: BTreeMap<String, String>,
}

impl Default for DesktopRouterModeSettings {
    fn default() -> Self {
        Self {
            revision: 1,
            modes: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopRouterModeSettingsUpdate {
    pub expected_revision: u64,
    #[serde(default)]
    pub modes: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRouterModeSettings {
    schema_version: u32,
    settings: DesktopRouterModeSettings,
}

impl DesktopApplication {
    pub fn model_feature_settings(
        &self,
    ) -> Result<DesktopModelFeatureSettings, DesktopApplicationError> {
        Ok(self.load_model_feature_settings()?)
    }

    pub fn save_model_feature_settings(
        &self,
        update: DesktopModelFeatureSettingsUpdate,
    ) -> Result<DesktopModelFeatureSettings, DesktopApplicationError> {
        let current = self.load_model_feature_settings()?;
        ensure_revision(update.expected_revision, current.revision)?;
        let next = normalize_model_feature_settings(DesktopModelFeatureSettings {
            revision: next_revision(current.revision)?,
            chat: update.chat,
            presets: update.presets,
            title: update.title,
            suggestion: update.suggestion,
            prompt_router: update.prompt_router,
            prompt_optimize: update.prompt_optimize,
            auto_turn_decision: update.auto_turn_decision,
        });
        self.persist_model_feature_settings(&next)?;
        self.emit_event(DesktopEventKind::ModelFeatureSettingsChanged {
            revision: next.revision,
        });
        Ok(next)
    }

    pub fn assistant_ai_settings(
        &self,
    ) -> Result<DesktopAssistantAiSettings, DesktopApplicationError> {
        Ok(self.load_assistant_ai_settings()?)
    }

    pub fn assistant_ai_secret_configured(&self) -> Result<bool, DesktopApplicationError> {
        Ok(self
            .read_host_credential_text_result(ASSISTANT_AI_CREDENTIAL_KEY)?
            .is_some_and(|secret| !secret.trim().is_empty()))
    }

    pub fn save_assistant_ai_settings(
        &self,
        update: DesktopAssistantAiSettingsUpdate,
    ) -> Result<DesktopAssistantAiSettings, DesktopApplicationError> {
        let current = self.load_assistant_ai_settings()?;
        ensure_revision(update.expected_revision, current.revision)?;
        let next = normalize_assistant_ai_settings(DesktopAssistantAiSettings {
            revision: next_revision(current.revision)?,
            base_url: update.base_url,
            model: update.model,
            model_pool: update.model_pool,
            codex_account_spark_enabled: update.codex_account_spark_enabled,
        });
        self.persist_assistant_ai_settings(&next)?;
        self.emit_event(DesktopEventKind::AssistantAiSettingsChanged {
            revision: next.revision,
        });
        Ok(next)
    }

    pub fn save_assistant_ai_configuration(
        &self,
        update: DesktopAssistantAiConfigurationUpdate,
    ) -> Result<DesktopAssistantAiSettings, DesktopApplicationError> {
        let original_secret = if matches!(&update.secret, DesktopAssistantAiSecretUpdate::Keep) {
            None
        } else {
            Some(self.read_host_credential_text_result(ASSISTANT_AI_CREDENTIAL_KEY)?)
        };
        if !matches!(&update.secret, DesktopAssistantAiSecretUpdate::Keep) {
            self.apply_assistant_ai_secret_update(&update.secret)?;
        }
        match self.save_assistant_ai_settings(update.settings) {
            Ok(settings) => Ok(settings),
            Err(settings_error) => {
                if let Some(original_secret) = original_secret {
                    if let Err(rollback_error) =
                        self.restore_assistant_ai_secret(original_secret.as_deref())
                    {
                        return Err(DesktopApplicationError::InvalidInput {
                            field: "assistant_ai",
                            message: format!(
                                "settings save failed: {settings_error}; credential rollback failed: {rollback_error}"
                            ),
                        });
                    }
                }
                Err(settings_error)
            }
        }
    }

    fn apply_assistant_ai_secret_update(
        &self,
        update: &DesktopAssistantAiSecretUpdate,
    ) -> Result<(), DesktopApplicationError> {
        let action = match update {
            DesktopAssistantAiSecretUpdate::Keep => return Ok(()),
            DesktopAssistantAiSecretUpdate::Set(secret) => DesktopCredentialAction::Write {
                key: ASSISTANT_AI_CREDENTIAL_KEY.to_owned(),
                secret: secret.clone(),
            },
            DesktopAssistantAiSecretUpdate::Clear => DesktopCredentialAction::Delete {
                key: ASSISTANT_AI_CREDENTIAL_KEY.to_owned(),
            },
        };
        match self.inner.host.execute(
            &self.inner.host_context,
            DesktopHostAction::Credential(action),
        )? {
            DesktopHostResult::Completed => Ok(()),
            _ => Err(DesktopApplicationError::InvalidInput {
                field: "assistant_ai",
                message: "credential update returned an unexpected host result".to_owned(),
            }),
        }
    }

    fn restore_assistant_ai_secret(
        &self,
        original: Option<&str>,
    ) -> Result<(), DesktopApplicationError> {
        let update = original.map_or(DesktopAssistantAiSecretUpdate::Clear, |secret| {
            DesktopAssistantAiSecretUpdate::Set(DesktopSecret::new(secret.as_bytes().to_vec()))
        });
        self.apply_assistant_ai_secret_update(&update)
    }

    pub fn router_mode_settings(
        &self,
    ) -> Result<DesktopRouterModeSettings, DesktopApplicationError> {
        Ok(self.load_router_mode_settings()?)
    }

    pub fn router_mode_for_backend(
        &self,
        backend: &str,
    ) -> Result<Option<String>, DesktopApplicationError> {
        let backend = backend.trim();
        if backend.is_empty() {
            return Ok(None);
        }
        Ok(self
            .load_router_mode_settings()?
            .modes
            .get(backend)
            .cloned())
    }

    pub fn save_router_mode_settings(
        &self,
        update: DesktopRouterModeSettingsUpdate,
    ) -> Result<DesktopRouterModeSettings, DesktopApplicationError> {
        let current = self.load_router_mode_settings()?;
        ensure_revision(update.expected_revision, current.revision)?;
        let next = normalize_router_mode_settings(DesktopRouterModeSettings {
            revision: next_revision(current.revision)?,
            modes: update.modes,
        });
        self.persist_router_mode_settings(&next)?;
        self.emit_event(DesktopEventKind::RouterModeSettingsChanged {
            revision: next.revision,
        });
        Ok(next)
    }

    pub fn set_router_mode_for_backend(
        &self,
        backend: &str,
        mode: &str,
    ) -> Result<DesktopRouterModeSettings, DesktopApplicationError> {
        let backend = backend.trim();
        let mode = mode.trim();
        if backend.is_empty() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "backend",
                message: "backend must not be empty".to_owned(),
            });
        }
        if mode.is_empty() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "mode",
                message: "mode must not be empty".to_owned(),
            });
        }
        let current = self.load_router_mode_settings()?;
        let mut modes = current.modes;
        modes.insert(backend.to_owned(), mode.to_owned());
        self.save_router_mode_settings(DesktopRouterModeSettingsUpdate {
            expected_revision: current.revision,
            modes,
        })
    }

    fn load_model_feature_settings(
        &self,
    ) -> Result<DesktopModelFeatureSettings, DesktopProviderUiSettingsError> {
        let value = self
            .provider_ui_settings_store()?
            .setting(MODEL_FEATURE_SETTINGS_KEY)
            .map_err(|error| DesktopProviderUiSettingsError::Persistence(error.to_string()))?;
        let Some(value) = value else {
            return Ok(DesktopModelFeatureSettings::default());
        };
        let stored = serde_json::from_value::<StoredModelFeatureSettings>(value)
            .map_err(|error| DesktopProviderUiSettingsError::Corrupt(error.to_string()))?;
        if stored.schema_version != MODEL_FEATURE_SCHEMA_VERSION {
            return Err(DesktopProviderUiSettingsError::UnsupportedSchema(
                stored.schema_version,
            ));
        }
        if stored.settings.revision == 0 {
            return Err(DesktopProviderUiSettingsError::Corrupt(
                "revision must be positive".to_owned(),
            ));
        }
        Ok(normalize_model_feature_settings(stored.settings))
    }

    fn persist_model_feature_settings(
        &self,
        settings: &DesktopModelFeatureSettings,
    ) -> Result<(), DesktopProviderUiSettingsError> {
        let value = serde_json::to_value(StoredModelFeatureSettings {
            schema_version: MODEL_FEATURE_SCHEMA_VERSION,
            settings: settings.clone(),
        })
        .map_err(|error| DesktopProviderUiSettingsError::Persistence(error.to_string()))?;
        self.provider_ui_settings_store()?
            .put_setting(MODEL_FEATURE_SETTINGS_KEY, &value)
            .map_err(|error| DesktopProviderUiSettingsError::Persistence(error.to_string()))
    }

    fn load_assistant_ai_settings(
        &self,
    ) -> Result<DesktopAssistantAiSettings, DesktopProviderUiSettingsError> {
        let value = self
            .provider_ui_settings_store()?
            .setting(ASSISTANT_AI_SETTINGS_KEY)
            .map_err(|error| DesktopProviderUiSettingsError::Persistence(error.to_string()))?;
        let Some(value) = value else {
            return Ok(DesktopAssistantAiSettings::default());
        };
        let stored = serde_json::from_value::<StoredAssistantAiSettings>(value)
            .map_err(|error| DesktopProviderUiSettingsError::Corrupt(error.to_string()))?;
        if stored.schema_version != ASSISTANT_AI_SCHEMA_VERSION {
            return Err(DesktopProviderUiSettingsError::UnsupportedSchema(
                stored.schema_version,
            ));
        }
        if stored.settings.revision == 0 {
            return Err(DesktopProviderUiSettingsError::Corrupt(
                "revision must be positive".to_owned(),
            ));
        }
        Ok(normalize_assistant_ai_settings(stored.settings))
    }

    fn persist_assistant_ai_settings(
        &self,
        settings: &DesktopAssistantAiSettings,
    ) -> Result<(), DesktopProviderUiSettingsError> {
        let value = serde_json::to_value(StoredAssistantAiSettings {
            schema_version: ASSISTANT_AI_SCHEMA_VERSION,
            settings: settings.clone(),
        })
        .map_err(|error| DesktopProviderUiSettingsError::Persistence(error.to_string()))?;
        self.provider_ui_settings_store()?
            .put_setting(ASSISTANT_AI_SETTINGS_KEY, &value)
            .map_err(|error| DesktopProviderUiSettingsError::Persistence(error.to_string()))
    }

    fn load_router_mode_settings(
        &self,
    ) -> Result<DesktopRouterModeSettings, DesktopProviderUiSettingsError> {
        let value = self
            .provider_ui_settings_store()?
            .setting(ROUTER_MODE_SETTINGS_KEY)
            .map_err(|error| DesktopProviderUiSettingsError::Persistence(error.to_string()))?;
        let Some(value) = value else {
            return Ok(DesktopRouterModeSettings::default());
        };
        let stored = serde_json::from_value::<StoredRouterModeSettings>(value)
            .map_err(|error| DesktopProviderUiSettingsError::Corrupt(error.to_string()))?;
        if stored.schema_version != ROUTER_MODE_SCHEMA_VERSION {
            return Err(DesktopProviderUiSettingsError::UnsupportedSchema(
                stored.schema_version,
            ));
        }
        if stored.settings.revision == 0 {
            return Err(DesktopProviderUiSettingsError::Corrupt(
                "revision must be positive".to_owned(),
            ));
        }
        Ok(normalize_router_mode_settings(stored.settings))
    }

    fn persist_router_mode_settings(
        &self,
        settings: &DesktopRouterModeSettings,
    ) -> Result<(), DesktopProviderUiSettingsError> {
        let value = serde_json::to_value(StoredRouterModeSettings {
            schema_version: ROUTER_MODE_SCHEMA_VERSION,
            settings: settings.clone(),
        })
        .map_err(|error| DesktopProviderUiSettingsError::Persistence(error.to_string()))?;
        self.provider_ui_settings_store()?
            .put_setting(ROUTER_MODE_SETTINGS_KEY, &value)
            .map_err(|error| DesktopProviderUiSettingsError::Persistence(error.to_string()))
    }

    fn provider_ui_settings_store(
        &self,
    ) -> Result<SqliteAgentRuntimeStateStore, DesktopProviderUiSettingsError> {
        self.config()
            .data_paths()
            .ensure_layout()
            .map_err(|error| DesktopProviderUiSettingsError::Persistence(error.to_string()))?;
        SqliteAgentRuntimeStateStore::open(self.config().data_paths().agent_runtime_db())
            .map_err(|error| DesktopProviderUiSettingsError::Persistence(error.to_string()))
    }
}

fn ensure_revision(expected: u64, actual: u64) -> Result<(), DesktopProviderUiSettingsError> {
    if expected != actual {
        return Err(DesktopProviderUiSettingsError::RevisionConflict { expected, actual });
    }
    Ok(())
}

fn next_revision(current: u64) -> Result<u64, DesktopProviderUiSettingsError> {
    current
        .checked_add(1)
        .ok_or(DesktopProviderUiSettingsError::RevisionOverflow)
}

pub fn normalize_model_feature_settings(
    settings: DesktopModelFeatureSettings,
) -> DesktopModelFeatureSettings {
    let chat = DesktopModelFeatureChatSettings {
        light: normalize_optional_string(settings.chat.light),
        normal: normalize_optional_string(settings.chat.normal),
        deep: normalize_optional_string(settings.chat.deep),
    };
    let presets = normalize_model_presets(settings.presets, &chat);
    let chat = mirror_presets_into_chat_tiers(&presets, &chat);
    DesktopModelFeatureSettings {
        revision: settings.revision.max(1),
        chat,
        presets,
        title: normalize_optional_string(settings.title),
        suggestion: normalize_optional_string(settings.suggestion),
        prompt_router: normalize_optional_string(settings.prompt_router),
        prompt_optimize: normalize_optional_string(settings.prompt_optimize),
        auto_turn_decision: normalize_optional_string(settings.auto_turn_decision),
    }
}

pub fn normalize_assistant_ai_settings(
    settings: DesktopAssistantAiSettings,
) -> DesktopAssistantAiSettings {
    DesktopAssistantAiSettings {
        revision: settings.revision.max(1),
        base_url: normalize_optional_string(settings.base_url),
        model: normalize_optional_string(settings.model),
        model_pool: normalize_model_pool(settings.model_pool),
        codex_account_spark_enabled: settings.codex_account_spark_enabled,
    }
}

pub fn normalize_router_mode_settings(
    settings: DesktopRouterModeSettings,
) -> DesktopRouterModeSettings {
    let mut modes = BTreeMap::new();
    for (backend, mode) in settings.modes {
        let backend = backend.trim();
        let mode = mode.trim();
        if backend.is_empty() || mode.is_empty() {
            continue;
        }
        modes.insert(backend.to_owned(), mode.to_owned());
    }
    DesktopRouterModeSettings {
        revision: settings.revision.max(1),
        modes,
    }
}

pub fn normalize_model_pool(
    items: Vec<DesktopAssistantAiModelPoolItem>,
) -> Vec<DesktopAssistantAiModelPoolItem> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for item in items {
        let id = item.id.trim().to_owned();
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        let label = item.label.trim();
        out.push(DesktopAssistantAiModelPoolItem {
            id: id.clone(),
            label: if label.is_empty() {
                id
            } else {
                label.to_owned()
            },
            source: match item.source.trim() {
                "legacy" => "legacy".to_owned(),
                _ => "remote".to_owned(),
            },
            backend: {
                let backend = item.backend.trim();
                if backend.is_empty() {
                    default_model_pool_backend()
                } else {
                    backend.to_owned()
                }
            },
        });
    }
    out
}

fn normalize_model_presets(
    raw: Vec<DesktopModelPresetGroup>,
    chat: &DesktopModelFeatureChatSettings,
) -> Vec<DesktopModelPresetGroup> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut builtin_overrides: BTreeMap<String, DesktopModelPresetGroup> = BTreeMap::new();
    let mut customs: Vec<DesktopModelPresetGroup> = Vec::new();
    let mut seen_custom: BTreeSet<String> = BTreeSet::new();

    for item in raw {
        let id = item.id.trim().to_owned();
        if id.is_empty() {
            continue;
        }
        if is_builtin_preset_id(&id) {
            let label = builtin_preset_label(&id).to_owned();
            builtin_overrides.insert(
                id.clone(),
                DesktopModelPresetGroup {
                    id,
                    label,
                    kind: "builtin".to_owned(),
                    model: normalize_optional_string(item.model),
                    reasoning_effort: normalize_optional_string(item.reasoning_effort),
                    enabled: item.enabled,
                },
            );
            continue;
        }
        if !seen_custom.insert(id.clone()) {
            continue;
        }
        let label = {
            let trimmed = item.label.trim();
            if trimmed.is_empty() {
                id.clone()
            } else {
                trimmed.to_owned()
            }
        };
        customs.push(DesktopModelPresetGroup {
            id,
            label,
            kind: "custom".to_owned(),
            model: normalize_optional_string(item.model),
            reasoning_effort: normalize_optional_string(item.reasoning_effort),
            enabled: item.enabled,
        });
    }

    let mut builtins = Vec::with_capacity(BUILTIN_PRESET_SPECS.len());
    for (id, label, tier) in BUILTIN_PRESET_SPECS {
        let from_chat = match *tier {
            "light" => chat.light.clone(),
            "normal" => chat.normal.clone(),
            "deep" => chat.deep.clone(),
            _ => None,
        };
        if let Some(override_preset) = builtin_overrides.remove(*id) {
            builtins.push(override_preset);
            continue;
        }
        builtins.push(DesktopModelPresetGroup {
            id: (*id).to_owned(),
            label: (*label).to_owned(),
            kind: "builtin".to_owned(),
            model: from_chat,
            reasoning_effort: None,
            enabled: true,
        });
    }

    builtins.extend(customs);
    builtins
}

fn mirror_presets_into_chat_tiers(
    presets: &[DesktopModelPresetGroup],
    chat: &DesktopModelFeatureChatSettings,
) -> DesktopModelFeatureChatSettings {
    let mut light = chat.light.clone();
    let mut normal = chat.normal.clone();
    let mut deep = chat.deep.clone();
    for preset in presets {
        match preset.id.as_str() {
            "fast" => light = preset.model.clone(),
            "default" => normal = preset.model.clone(),
            "plan" => deep = preset.model.clone(),
            _ => {}
        }
    }
    DesktopModelFeatureChatSettings {
        light,
        normal,
        deep,
    }
}

fn is_builtin_preset_id(id: &str) -> bool {
    matches!(id, "fast" | "default" | "plan" | "review")
}

fn builtin_preset_label(id: &str) -> String {
    match id {
        "fast" => "Fast".to_owned(),
        "default" => "Default".to_owned(),
        "plan" => "Plan".to_owned(),
        "review" => "Review".to_owned(),
        other => other.to_owned(),
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };
    use lilia_service::ServiceAuthority;

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

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

    #[derive(Default)]
    struct CredentialHost {
        secret: Mutex<Option<Vec<u8>>>,
    }

    impl CredentialHost {
        fn secret(&self) -> Option<String> {
            self.secret
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .and_then(|secret| String::from_utf8(secret.clone()).ok())
        }
    }

    impl DesktopHost for CredentialHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            let mut secret = self
                .secret
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match action {
                DesktopHostAction::Credential(DesktopCredentialAction::Read { key })
                    if key == ASSISTANT_AI_CREDENTIAL_KEY =>
                {
                    Ok(DesktopHostResult::Credential(
                        secret.clone().map(DesktopSecret::new),
                    ))
                }
                DesktopHostAction::Credential(DesktopCredentialAction::Write {
                    key,
                    secret: next,
                }) if key == ASSISTANT_AI_CREDENTIAL_KEY => {
                    *secret = Some(next.into_inner());
                    Ok(DesktopHostResult::Completed)
                }
                DesktopHostAction::Credential(DesktopCredentialAction::Delete { key })
                    if key == ASSISTANT_AI_CREDENTIAL_KEY =>
                {
                    *secret = None;
                    Ok(DesktopHostResult::Completed)
                }
                _ => Ok(DesktopHostResult::Completed),
            }
        }
    }

    fn application() -> DesktopApplication {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("lilia-provider-ui-settings-{id}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let config =
            DesktopApplicationConfig::new(&root, format!("provider-ui-settings-{id}")).unwrap();
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:provider-ui-settings:{id}"),
            format!("provider-ui-settings-test:{id}"),
        )
        .unwrap();
        DesktopApplication::from_authority(config, authority, Arc::new(NoopHost)).unwrap()
    }

    fn application_with_credential_host() -> (DesktopApplication, Arc<CredentialHost>) {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("lilia-provider-ui-credential-{id}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let config =
            DesktopApplicationConfig::new(&root, format!("provider-ui-settings-credential-{id}"))
                .unwrap();
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:provider-ui-credential:{id}"),
            format!("provider-ui-settings-credential-test:{id}"),
        )
        .unwrap();
        let host = Arc::new(CredentialHost::default());
        let app = DesktopApplication::from_authority(config, authority, host.clone()).unwrap();
        (app, host)
    }

    #[test]
    fn model_feature_settings_round_trip_with_revision() {
        let app = application();
        let defaults = app.model_feature_settings().unwrap();
        assert_eq!(defaults.revision, 1);

        let saved = app
            .save_model_feature_settings(DesktopModelFeatureSettingsUpdate {
                expected_revision: defaults.revision,
                chat: DesktopModelFeatureChatSettings {
                    light: Some("  mini  ".to_owned()),
                    normal: None,
                    deep: None,
                },
                presets: Vec::new(),
                title: Some(" title-model ".to_owned()),
                suggestion: None,
                prompt_router: None,
                prompt_optimize: None,
                auto_turn_decision: None,
            })
            .unwrap();
        assert_eq!(saved.revision, 2);
        assert_eq!(saved.chat.light.as_deref(), Some("mini"));
        assert_eq!(saved.title.as_deref(), Some("title-model"));
        assert!(saved.presets.iter().any(|preset| preset.id == "fast"));
        assert_eq!(app.model_feature_settings().unwrap(), saved);

        let conflict = app.save_model_feature_settings(DesktopModelFeatureSettingsUpdate {
            expected_revision: 1,
            chat: DesktopModelFeatureChatSettings::default(),
            presets: Vec::new(),
            title: None,
            suggestion: None,
            prompt_router: None,
            prompt_optimize: None,
            auto_turn_decision: None,
        });
        assert!(conflict.is_err());
    }

    #[test]
    fn explicit_builtin_model_clear_does_not_restore_legacy_chat_value() {
        let normalized = normalize_model_feature_settings(DesktopModelFeatureSettings {
            revision: 3,
            chat: DesktopModelFeatureChatSettings {
                light: Some("legacy-light".into()),
                normal: None,
                deep: None,
            },
            presets: vec![DesktopModelPresetGroup {
                id: "fast".into(),
                label: "Fast".into(),
                kind: "builtin".into(),
                model: None,
                reasoning_effort: None,
                enabled: true,
            }],
            title: None,
            suggestion: None,
            prompt_router: None,
            prompt_optimize: None,
            auto_turn_decision: None,
        });

        assert_eq!(normalized.chat.light, None);
        assert_eq!(
            normalized
                .presets
                .iter()
                .find(|preset| preset.id == "fast")
                .and_then(|preset| preset.model.as_deref()),
            None
        );
    }

    #[test]
    fn assistant_ai_settings_exclude_secrets_and_normalize_pool() {
        let app = application();
        let defaults = app.assistant_ai_settings().unwrap();
        let saved = app
            .save_assistant_ai_settings(DesktopAssistantAiSettingsUpdate {
                expected_revision: defaults.revision,
                base_url: Some(" https://api.example.com/v1 ".to_owned()),
                model: Some(" mini ".to_owned()),
                model_pool: vec![
                    DesktopAssistantAiModelPoolItem {
                        id: " remote-mini ".to_owned(),
                        label: " Remote Mini ".to_owned(),
                        source: "remote".to_owned(),
                        backend: "native-agentkit".to_owned(),
                    },
                    DesktopAssistantAiModelPoolItem {
                        id: "remote-mini".to_owned(),
                        label: "dup".to_owned(),
                        source: "legacy".to_owned(),
                        backend: "native-agentkit".to_owned(),
                    },
                ],
                codex_account_spark_enabled: true,
            })
            .unwrap();
        assert_eq!(saved.revision, 2);
        assert_eq!(
            saved.base_url.as_deref(),
            Some("https://api.example.com/v1")
        );
        assert_eq!(saved.model_pool.len(), 1);
        assert_eq!(saved.model_pool[0].id, "remote-mini");
        assert_eq!(saved.model_pool[0].label, "Remote Mini");
        let encoded = serde_json::to_value(&saved).unwrap();
        assert!(encoded.get("apiKey").is_none());
        assert!(encoded.get("hasApiKey").is_none());
        assert!(encoded.get("clearApiKey").is_none());
    }

    #[test]
    fn assistant_ai_configuration_rolls_back_the_secret_on_revision_conflict() {
        let (app, host) = application_with_credential_host();
        let saved = app
            .save_assistant_ai_configuration(DesktopAssistantAiConfigurationUpdate {
                settings: DesktopAssistantAiSettingsUpdate {
                    expected_revision: 1,
                    base_url: Some("https://assistant.example.test/v1".to_owned()),
                    model: Some("assistant-model".to_owned()),
                    model_pool: Vec::new(),
                    codex_account_spark_enabled: false,
                },
                secret: DesktopAssistantAiSecretUpdate::Set(DesktopSecret::new(
                    b"current-secret".to_vec(),
                )),
            })
            .unwrap();
        assert_eq!(saved.revision, 2);
        assert_eq!(host.secret().as_deref(), Some("current-secret"));

        let conflict = app.save_assistant_ai_configuration(DesktopAssistantAiConfigurationUpdate {
            settings: DesktopAssistantAiSettingsUpdate {
                expected_revision: 1,
                base_url: Some("https://wrong.example.test/v1".to_owned()),
                model: Some("wrong-model".to_owned()),
                model_pool: Vec::new(),
                codex_account_spark_enabled: false,
            },
            secret: DesktopAssistantAiSecretUpdate::Set(DesktopSecret::new(
                b"replacement-secret".to_vec(),
            )),
        });

        assert!(conflict.is_err());
        assert_eq!(app.assistant_ai_settings().unwrap(), saved);
        assert_eq!(host.secret().as_deref(), Some("current-secret"));
    }

    #[test]
    fn router_mode_settings_set_per_backend() {
        let app = application();
        let saved = app
            .set_router_mode_for_backend("native-agentkit", "api")
            .unwrap();
        assert_eq!(saved.revision, 2);
        assert_eq!(
            saved.modes.get("native-agentkit").map(String::as_str),
            Some("api")
        );
        assert_eq!(
            app.router_mode_for_backend("native-agentkit")
                .unwrap()
                .as_deref(),
            Some("api")
        );
    }
}

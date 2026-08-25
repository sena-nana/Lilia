use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CONVERSATION_SUGGESTION_SETTINGS_KEY: &str =
    "desktop.conversation-suggestions.settings.v1";
pub const CONVERSATION_SUGGESTION_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DesktopConversationSuggestionSource {
    Provider,
    #[default]
    #[serde(rename = "assistant-ai")]
    AssistantAi,
}

impl DesktopConversationSuggestionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::AssistantAi => "assistant-ai",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim() {
            "assistant-ai" => Self::AssistantAi,
            _ => Self::Provider,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopConversationSuggestionSettings {
    pub enabled: bool,
    pub source: DesktopConversationSuggestionSource,
}

impl Default for DesktopConversationSuggestionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            source: DesktopConversationSuggestionSource::AssistantAi,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredConversationSuggestionSettings {
    pub schema_version: u32,
    pub settings: DesktopConversationSuggestionSettings,
}

#[derive(Debug, Error)]
pub enum DesktopConversationSuggestionError {
    #[error("conversation suggestion settings persistence failed: {0}")]
    Persistence(String),
    #[error("conversation suggestion settings payload is corrupt: {0}")]
    Corrupt(String),
    #[error("unsupported conversation suggestion settings schema {0}")]
    UnsupportedSchema(u32),
}

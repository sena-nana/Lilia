use lilia_storage::SqliteAgentRuntimeStateStore;

use crate::application::ConversationSuggestionSettingsChanged;
use crate::application::{DesktopApplication, DesktopApplicationError};

pub use lilia_feature_suggestions::settings::{
    DesktopConversationSuggestionError, DesktopConversationSuggestionSettings,
    DesktopConversationSuggestionSource, StoredConversationSuggestionSettings,
    CONVERSATION_SUGGESTION_SETTINGS_KEY, CONVERSATION_SUGGESTION_SETTINGS_SCHEMA_VERSION,
};

/// The suggestions domain reports settings failures as invalid input, which is
/// what the settings surface already renders.
impl From<DesktopConversationSuggestionError> for DesktopApplicationError {
    fn from(error: DesktopConversationSuggestionError) -> Self {
        match error {
            DesktopConversationSuggestionError::Persistence(message)
            | DesktopConversationSuggestionError::Corrupt(message) => Self::InvalidInput {
                field: "conversation_suggestions",
                message,
            },
            DesktopConversationSuggestionError::UnsupportedSchema(version) => Self::InvalidInput {
                field: "conversation_suggestions",
                message: format!("unsupported schema {version}"),
            },
        }
    }
}

impl DesktopApplication {
    pub fn conversation_suggestion_settings(
        &self,
    ) -> Result<DesktopConversationSuggestionSettings, DesktopApplicationError> {
        Ok(self.load_conversation_suggestion_settings()?)
    }

    pub fn save_conversation_suggestion_settings(
        &self,
        settings: DesktopConversationSuggestionSettings,
    ) -> Result<DesktopConversationSuggestionSettings, DesktopApplicationError> {
        let normalized = normalize_conversation_suggestion_settings(settings);
        self.persist_conversation_suggestion_settings(&normalized)?;
        self.emit_event(ConversationSuggestionSettingsChanged);
        Ok(normalized)
    }

    fn load_conversation_suggestion_settings(
        &self,
    ) -> Result<DesktopConversationSuggestionSettings, DesktopConversationSuggestionError> {
        let value = self
            .conversation_suggestion_store()?
            .setting(CONVERSATION_SUGGESTION_SETTINGS_KEY)
            .map_err(|error| DesktopConversationSuggestionError::Persistence(error.to_string()))?;
        let Some(value) = value else {
            return Ok(DesktopConversationSuggestionSettings::default());
        };
        let stored = serde_json::from_value::<StoredConversationSuggestionSettings>(value)
            .map_err(|error| DesktopConversationSuggestionError::Corrupt(error.to_string()))?;
        if stored.schema_version != CONVERSATION_SUGGESTION_SETTINGS_SCHEMA_VERSION {
            return Err(DesktopConversationSuggestionError::UnsupportedSchema(
                stored.schema_version,
            ));
        }
        Ok(normalize_conversation_suggestion_settings(stored.settings))
    }

    fn persist_conversation_suggestion_settings(
        &self,
        settings: &DesktopConversationSuggestionSettings,
    ) -> Result<(), DesktopConversationSuggestionError> {
        let stored = StoredConversationSuggestionSettings {
            schema_version: CONVERSATION_SUGGESTION_SETTINGS_SCHEMA_VERSION,
            settings: settings.clone(),
        };
        let value = serde_json::to_value(stored)
            .map_err(|error| DesktopConversationSuggestionError::Persistence(error.to_string()))?;
        self.conversation_suggestion_store()?
            .put_setting(CONVERSATION_SUGGESTION_SETTINGS_KEY, &value)
            .map_err(|error| DesktopConversationSuggestionError::Persistence(error.to_string()))
    }

    fn conversation_suggestion_store(
        &self,
    ) -> Result<SqliteAgentRuntimeStateStore, DesktopConversationSuggestionError> {
        self.config()
            .data_paths()
            .ensure_layout()
            .map_err(|error| DesktopConversationSuggestionError::Persistence(error.to_string()))?;
        SqliteAgentRuntimeStateStore::open(self.config().data_paths().agent_runtime_db())
            .map_err(|error| DesktopConversationSuggestionError::Persistence(error.to_string()))
    }
}

pub fn normalize_conversation_suggestion_settings(
    settings: DesktopConversationSuggestionSettings,
) -> DesktopConversationSuggestionSettings {
    DesktopConversationSuggestionSettings {
        enabled: settings.enabled,
        source: settings.source,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

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

    fn application() -> DesktopApplication {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("lilia-conversation-suggestions-{id}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let config =
            DesktopApplicationConfig::new(&root, format!("conversation-suggestions-{id}")).unwrap();
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:conversation-suggestions:{id}"),
            format!("conversation-suggestions-test:{id}"),
        )
        .unwrap();
        DesktopApplication::from_authority(config, authority, Arc::new(NoopHost)).unwrap()
    }

    #[test]
    fn conversation_suggestion_settings_persist_enabled_and_source() {
        let app = application();
        assert_eq!(
            app.conversation_suggestion_settings().unwrap(),
            DesktopConversationSuggestionSettings::default()
        );
        let saved = app
            .save_conversation_suggestion_settings(DesktopConversationSuggestionSettings {
                enabled: false,
                source: DesktopConversationSuggestionSource::AssistantAi,
            })
            .unwrap();
        assert!(!saved.enabled);
        assert_eq!(
            saved.source,
            DesktopConversationSuggestionSource::AssistantAi
        );
        assert_eq!(app.conversation_suggestion_settings().unwrap(), saved);
    }
}

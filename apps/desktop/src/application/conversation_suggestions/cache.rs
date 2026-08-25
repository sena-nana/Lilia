use crate::application::{DesktopApplication, DesktopApplicationError, DesktopConversationSuggestionError};
use lilia_feature_suggestions::types::{now_millis, DesktopSuggestionItem, SUGGESTION_CACHE_KEY};
use lilia_storage::SqliteAgentRuntimeStateStore;

pub use lilia_feature_suggestions::cache::{
    build_cache_key, cache_entry_is_valid, cache_scope_key, SuggestionCache, SuggestionCacheEntry,
};

impl DesktopApplication {
    pub(crate) fn load_suggestion_cache_hit(
        &self,
        scope: &str,
        cache_key: &str,
    ) -> Result<Option<SuggestionCacheEntry>, DesktopApplicationError> {
        let cache = self.load_suggestion_cache()?;
        let Some(hit) = cache.get(scope).cloned() else {
            return Ok(None);
        };
        Ok(cache_entry_is_valid(&hit, cache_key, now_millis()).then_some(hit))
    }

    pub(crate) fn save_suggestion_cache(
        &self,
        scope: String,
        cache_key: String,
        items: Vec<DesktopSuggestionItem>,
    ) -> Result<(), DesktopApplicationError> {
        let mut cache = self.load_suggestion_cache()?;
        cache.insert(
            scope,
            SuggestionCacheEntry {
                cache_key,
                generated_at: now_millis(),
                items,
            },
        );
        let value = serde_json::to_value(cache)
            .map_err(|error| DesktopConversationSuggestionError::Persistence(error.to_string()))?;
        self.suggestion_cache_store()?
            .put_setting(SUGGESTION_CACHE_KEY, &value)
            .map_err(|error| DesktopConversationSuggestionError::Persistence(error.to_string()))?;
        Ok(())
    }

    fn load_suggestion_cache(&self) -> Result<SuggestionCache, DesktopApplicationError> {
        let value = self
            .suggestion_cache_store()?
            .setting(SUGGESTION_CACHE_KEY)
            .map_err(|error| DesktopConversationSuggestionError::Persistence(error.to_string()))?;
        let Some(value) = value else {
            return Ok(SuggestionCache::default());
        };
        serde_json::from_value(value)
            .map_err(|error| DesktopConversationSuggestionError::Corrupt(error.to_string()).into())
    }

    fn suggestion_cache_store(
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

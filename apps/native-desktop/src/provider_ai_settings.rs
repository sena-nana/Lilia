use std::collections::BTreeSet;

use lilia_contracts::auto_model_for_provider_family_tier;
use lilia_desktop_application::{
    normalize_model_pool, DesktopApplication, DesktopAssistantAiConfigurationUpdate,
    DesktopAssistantAiModelPoolItem, DesktopAssistantAiSecretUpdate, DesktopAssistantAiSettings,
    DesktopAssistantAiSettingsUpdate, DesktopConversationSuggestionSettings,
    DesktopModelFeatureSettings, DesktopModelFeatureSettingsUpdate, DesktopModelPresetGroup,
    DesktopSecret,
};

#[derive(Clone, Debug)]
pub(crate) struct ProviderAiSettingsState {
    assistant: DesktopAssistantAiSettings,
    model_features: DesktopModelFeatureSettings,
    conversation_suggestions: DesktopConversationSuggestionSettings,
    pub(crate) assistant_base_url: String,
    pub(crate) assistant_model: String,
    pub(crate) assistant_secret: String,
    pub(crate) assistant_secret_configured: bool,
    pub(crate) title_model: String,
    pub(crate) suggestion_model: String,
    pub(crate) prompt_router_model: String,
    pub(crate) prompt_optimize_model: String,
    pub(crate) auto_turn_decision_model: String,
    assistant_model_pool_drafts: Vec<DesktopAssistantAiModelPoolItem>,
    preset_drafts: Vec<DesktopModelPresetGroup>,
}

impl ProviderAiSettingsState {
    pub(crate) fn load(application: &DesktopApplication) -> Result<Self, String> {
        let assistant = application
            .assistant_ai_settings()
            .map_err(|error| error.to_string())?;
        let model_features = application
            .model_feature_settings()
            .map_err(|error| error.to_string())?;
        let conversation_suggestions = application
            .conversation_suggestion_settings()
            .map_err(|error| error.to_string())?;
        let assistant_secret_configured = application
            .assistant_ai_secret_configured()
            .map_err(|error| error.to_string())?;
        Ok(Self::new(
            assistant,
            model_features,
            conversation_suggestions,
            assistant_secret_configured,
        ))
    }

    fn new(
        assistant: DesktopAssistantAiSettings,
        model_features: DesktopModelFeatureSettings,
        conversation_suggestions: DesktopConversationSuggestionSettings,
        assistant_secret_configured: bool,
    ) -> Self {
        Self {
            assistant_base_url: assistant.base_url.clone().unwrap_or_default(),
            assistant_model: assistant.model.clone().unwrap_or_default(),
            assistant_secret: String::new(),
            assistant_secret_configured,
            conversation_suggestions,
            title_model: model_features.title.clone().unwrap_or_default(),
            suggestion_model: model_features.suggestion.clone().unwrap_or_default(),
            prompt_router_model: model_features.prompt_router.clone().unwrap_or_default(),
            prompt_optimize_model: model_features.prompt_optimize.clone().unwrap_or_default(),
            auto_turn_decision_model: model_features
                .auto_turn_decision
                .clone()
                .unwrap_or_default(),
            assistant_model_pool_drafts: assistant.model_pool.clone(),
            preset_drafts: model_features.presets.clone(),
            assistant,
            model_features,
        }
    }

    pub(crate) fn assistant_dirty(&self) -> bool {
        normalized(&self.assistant_base_url) != self.assistant.base_url.as_deref()
            || normalized(&self.assistant_model) != self.assistant.model.as_deref()
            || !self.assistant_secret.trim().is_empty()
            || normalize_model_pool(self.assistant_model_pool_drafts.clone())
                != self.assistant.model_pool
    }

    pub(crate) fn model_features_dirty(&self) -> bool {
        normalized(&self.title_model) != self.model_features.title.as_deref()
            || normalized(&self.suggestion_model) != self.model_features.suggestion.as_deref()
            || normalized(&self.prompt_router_model) != self.model_features.prompt_router.as_deref()
            || normalized(&self.prompt_optimize_model)
                != self.model_features.prompt_optimize.as_deref()
            || normalized(&self.auto_turn_decision_model)
                != self.model_features.auto_turn_decision.as_deref()
            || self.preset_drafts != self.model_features.presets
    }

    pub(crate) fn conversation_suggestions_enabled(&self) -> bool {
        self.conversation_suggestions.enabled
    }

    pub(crate) fn assistant_model_pool(
        &self,
    ) -> impl Iterator<Item = &DesktopAssistantAiModelPoolItem> {
        self.assistant_model_pool_drafts.iter()
    }

    pub(crate) fn add_assistant_model(&mut self, id: &str, label: &str) -> bool {
        let Some(id) = normalized(id) else {
            return false;
        };
        let label = normalized(label).unwrap_or(id);
        let item = DesktopAssistantAiModelPoolItem {
            id: id.to_owned(),
            label: label.to_owned(),
            source: "remote".to_owned(),
            backend: "native-agentkit".to_owned(),
        };
        if let Some(existing) = self
            .assistant_model_pool_drafts
            .iter_mut()
            .find(|candidate| candidate.id == id)
        {
            *existing = item;
        } else {
            self.assistant_model_pool_drafts.push(item);
        }
        true
    }

    pub(crate) fn rename_assistant_model(&mut self, id: &str, label: String) {
        if let Some(item) = self
            .assistant_model_pool_drafts
            .iter_mut()
            .find(|item| item.id == id)
        {
            item.label = label;
        }
    }

    pub(crate) fn merge_fetched_assistant_models(
        &mut self,
        models: Vec<DesktopAssistantAiModelPoolItem>,
    ) -> usize {
        let mut added = 0;
        for item in normalize_model_pool(models) {
            if let Some(existing) = self
                .assistant_model_pool_drafts
                .iter_mut()
                .find(|candidate| candidate.id == item.id)
            {
                let label = existing.label.trim();
                if label.is_empty() {
                    existing.label = item.label;
                }
                existing.source = "remote".to_owned();
                existing.backend = "native-agentkit".to_owned();
            } else {
                self.assistant_model_pool_drafts
                    .push(DesktopAssistantAiModelPoolItem {
                        source: "remote".to_owned(),
                        backend: "native-agentkit".to_owned(),
                        ..item
                    });
                added += 1;
            }
        }
        added
    }

    pub(crate) fn builtin_presets(&self) -> impl Iterator<Item = &DesktopModelPresetGroup> {
        self.preset_drafts
            .iter()
            .filter(|preset| preset.kind == "builtin")
    }

    pub(crate) fn custom_presets(&self) -> impl Iterator<Item = &DesktopModelPresetGroup> {
        self.preset_drafts
            .iter()
            .filter(|preset| preset.kind == "custom")
    }

    pub(crate) fn has_preset(&self, preset_id: &str) -> bool {
        self.preset_drafts
            .iter()
            .any(|preset| preset.id == preset_id)
    }

    pub(crate) fn model_features(&self) -> &DesktopModelFeatureSettings {
        &self.model_features
    }

    pub(crate) fn composer_model_options(
        &self,
        runtime_default: Option<&str>,
    ) -> Vec<(String, String)> {
        let mut seen = BTreeSet::new();
        let mut options = Vec::new();
        let mut push = |model: Option<&str>, label: Option<&str>| {
            let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) else {
                return;
            };
            if seen.insert(model.to_owned()) {
                options.push((
                    model.to_owned(),
                    label
                        .map(str::trim)
                        .filter(|label| !label.is_empty())
                        .map_or_else(|| model.to_owned(), |label| format!("{label} · {model}")),
                ));
            }
        };

        push(runtime_default, Some("Provider 默认"));
        for preset in self
            .model_features
            .presets
            .iter()
            .filter(|preset| preset.enabled)
        {
            push(preset.model.as_deref(), Some(&preset.label));
        }
        push(self.model_features.chat.light.as_deref(), Some("轻量"));
        push(self.model_features.chat.normal.as_deref(), Some("默认"));
        push(self.model_features.chat.deep.as_deref(), Some("深度"));
        for item in &self.assistant.model_pool {
            push(Some(&item.id), Some(&item.label));
        }
        for family in ["openai", "anthropic"] {
            for tier in ["light", "normal", "deep"] {
                push(auto_model_for_provider_family_tier(family, tier), None);
            }
        }
        options
    }

    pub(crate) fn set_preset_model(&mut self, preset_id: &str, model: String) {
        if let Some(preset) = self
            .preset_drafts
            .iter_mut()
            .find(|preset| preset.id == preset_id)
        {
            preset.model = normalized_owned(model);
        }
    }

    pub(crate) fn cycle_preset_effort(&mut self, preset_id: &str) {
        const EFFORTS: [Option<&str>; 6] = [
            None,
            Some("low"),
            Some("medium"),
            Some("high"),
            Some("xhigh"),
            Some("max"),
        ];
        let Some(preset) = self
            .preset_drafts
            .iter_mut()
            .find(|preset| preset.id == preset_id)
        else {
            return;
        };
        let current = preset.reasoning_effort.as_deref();
        let next = EFFORTS
            .iter()
            .position(|effort| *effort == current)
            .map_or(0, |index| (index + 1) % EFFORTS.len());
        preset.reasoning_effort = EFFORTS[next].map(str::to_owned);
    }

    pub(crate) fn add_custom_preset(&mut self, label: &str) -> String {
        let label = normalized(label).unwrap_or("自定义预设").to_owned();
        let slug = label
            .chars()
            .map(|character| {
                if character.is_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-")
            .chars()
            .take(32)
            .collect::<String>();
        let base = if slug.is_empty() { "preset" } else { &slug };
        let mut id = format!("custom-{base}");
        let mut suffix = 2_u32;
        while self.has_preset(&id) {
            id = format!("custom-{base}-{suffix}");
            suffix = suffix.saturating_add(1);
        }
        self.preset_drafts.push(DesktopModelPresetGroup {
            id: id.clone(),
            label,
            kind: "custom".to_owned(),
            model: None,
            reasoning_effort: None,
            enabled: true,
        });
        id
    }

    pub(crate) fn rename_custom_preset(&mut self, preset_id: &str, label: String) {
        let Some(label) = normalized_owned(label) else {
            return;
        };
        if let Some(preset) = self
            .preset_drafts
            .iter_mut()
            .find(|preset| preset.id == preset_id && preset.kind == "custom")
        {
            preset.label = label;
        }
    }

    pub(crate) fn remove_custom_preset(&mut self, preset_id: &str) {
        self.preset_drafts
            .retain(|preset| preset.id != preset_id || preset.kind != "custom");
    }

    pub(crate) fn conversation_suggestion_update(
        &self,
        enabled: bool,
    ) -> DesktopConversationSuggestionSettings {
        DesktopConversationSuggestionSettings {
            enabled,
            source: self.conversation_suggestions.source,
        }
    }

    pub(crate) fn assistant_update(&self) -> DesktopAssistantAiConfigurationUpdate {
        DesktopAssistantAiConfigurationUpdate {
            settings: DesktopAssistantAiSettingsUpdate {
                expected_revision: self.assistant.revision,
                base_url: Some(self.assistant_base_url.clone()),
                model: Some(self.assistant_model.clone()),
                model_pool: self.assistant_model_pool_drafts.clone(),
                codex_account_spark_enabled: self.assistant.codex_account_spark_enabled,
            },
            secret: if self.assistant_secret.trim().is_empty() {
                DesktopAssistantAiSecretUpdate::Keep
            } else {
                DesktopAssistantAiSecretUpdate::Set(DesktopSecret::new(
                    self.assistant_secret.trim().as_bytes().to_vec(),
                ))
            },
        }
    }

    pub(crate) fn clear_secret_update(&self) -> DesktopAssistantAiConfigurationUpdate {
        let mut update = self.assistant_update();
        update.secret = DesktopAssistantAiSecretUpdate::Clear;
        update
    }

    pub(crate) fn model_features_update(&self) -> DesktopModelFeatureSettingsUpdate {
        DesktopModelFeatureSettingsUpdate {
            expected_revision: self.model_features.revision,
            chat: self.model_features.chat.clone(),
            presets: self.preset_drafts.clone(),
            title: Some(self.title_model.clone()),
            suggestion: Some(self.suggestion_model.clone()),
            prompt_router: Some(self.prompt_router_model.clone()),
            prompt_optimize: Some(self.prompt_optimize_model.clone()),
            auto_turn_decision: Some(self.auto_turn_decision_model.clone()),
        }
    }

    pub(crate) fn sync_assistant(
        &mut self,
        assistant: DesktopAssistantAiSettings,
        secret_configured: bool,
    ) {
        self.assistant_base_url = assistant.base_url.clone().unwrap_or_default();
        self.assistant_model = assistant.model.clone().unwrap_or_default();
        self.assistant_secret.clear();
        self.assistant_secret_configured = secret_configured;
        self.assistant_model_pool_drafts = assistant.model_pool.clone();
        self.assistant = assistant;
    }

    pub(crate) fn sync_model_features(&mut self, model_features: DesktopModelFeatureSettings) {
        self.title_model = model_features.title.clone().unwrap_or_default();
        self.suggestion_model = model_features.suggestion.clone().unwrap_or_default();
        self.prompt_router_model = model_features.prompt_router.clone().unwrap_or_default();
        self.prompt_optimize_model = model_features.prompt_optimize.clone().unwrap_or_default();
        self.auto_turn_decision_model = model_features
            .auto_turn_decision
            .clone()
            .unwrap_or_default();
        self.preset_drafts = model_features.presets.clone();
        self.model_features = model_features;
    }

    pub(crate) fn sync_conversation_suggestions(
        &mut self,
        settings: DesktopConversationSuggestionSettings,
    ) {
        self.conversation_suggestions = settings;
    }
}

fn normalized(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn normalized_owned(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use lilia_desktop_application::{DesktopModelFeatureChatSettings, DesktopModelPresetGroup};

    use super::*;

    #[test]
    fn feature_model_edit_preserves_hidden_settings_and_updates_runtime_presets() {
        let assistant = DesktopAssistantAiSettings::default();
        let features = DesktopModelFeatureSettings {
            revision: 7,
            chat: DesktopModelFeatureChatSettings {
                light: Some("light-model".to_owned()),
                normal: Some("normal-model".to_owned()),
                deep: Some("deep-model".to_owned()),
            },
            presets: vec![DesktopModelPresetGroup {
                id: "custom".to_owned(),
                label: "Custom".to_owned(),
                kind: "custom".to_owned(),
                model: Some("preset-model".to_owned()),
                reasoning_effort: Some("high".to_owned()),
                enabled: true,
            }],
            title: Some("old-title".to_owned()),
            suggestion: Some("old-suggestion".to_owned()),
            prompt_router: Some("router-model".to_owned()),
            prompt_optimize: Some("optimize-model".to_owned()),
            auto_turn_decision: Some("decision-model".to_owned()),
        };
        let mut state = ProviderAiSettingsState::new(
            assistant,
            features.clone(),
            DesktopConversationSuggestionSettings::default(),
            false,
        );
        state.title_model = "new-title".to_owned();
        state.suggestion_model = "new-suggestion".to_owned();
        state.prompt_router_model = "new-router".to_owned();
        state.prompt_optimize_model = "new-optimize".to_owned();
        state.auto_turn_decision_model = "new-decision".to_owned();
        state.set_preset_model("custom", "updated-custom".to_owned());
        state.set_preset_model("fast", "fast-model".to_owned());

        let update = state.model_features_update();

        assert_eq!(update.expected_revision, 7);
        assert_eq!(update.title.as_deref(), Some("new-title"));
        assert_eq!(update.suggestion.as_deref(), Some("new-suggestion"));
        assert_eq!(update.chat, features.chat);
        assert_eq!(update.presets[0].model.as_deref(), Some("updated-custom"));
        assert_eq!(update.prompt_router.as_deref(), Some("new-router"));
        assert_eq!(update.prompt_optimize.as_deref(), Some("new-optimize"));
        assert_eq!(update.auto_turn_decision.as_deref(), Some("new-decision"));
    }

    #[test]
    fn builtin_preset_drafts_cycle_effort_and_persist_model() {
        let mut state = ProviderAiSettingsState::new(
            DesktopAssistantAiSettings::default(),
            DesktopModelFeatureSettings::default(),
            DesktopConversationSuggestionSettings::default(),
            false,
        );

        state.set_preset_model("fast", "  fast-model  ".to_owned());
        state.cycle_preset_effort("fast");
        let fast = state
            .model_features_update()
            .presets
            .into_iter()
            .find(|preset| preset.id == "fast")
            .unwrap();

        assert_eq!(fast.model.as_deref(), Some("fast-model"));
        assert_eq!(fast.reasoning_effort.as_deref(), Some("low"));
        assert!(state.model_features_dirty());
    }

    #[test]
    fn assistant_secret_is_only_replaced_when_a_new_value_was_entered() {
        let mut state = ProviderAiSettingsState::new(
            DesktopAssistantAiSettings::default(),
            DesktopModelFeatureSettings::default(),
            DesktopConversationSuggestionSettings::default(),
            true,
        );
        assert!(matches!(
            state.assistant_update().secret,
            DesktopAssistantAiSecretUpdate::Keep
        ));

        state.assistant_secret = " replacement ".to_owned();
        let update = state.assistant_update();
        let DesktopAssistantAiSecretUpdate::Set(secret) = update.secret else {
            panic!("expected replacement secret");
        };
        assert_eq!(secret.expose(), b"replacement");
    }

    #[test]
    fn suggestion_toggle_preserves_the_selected_generation_source() {
        let state = ProviderAiSettingsState::new(
            DesktopAssistantAiSettings::default(),
            DesktopModelFeatureSettings::default(),
            DesktopConversationSuggestionSettings {
                enabled: true,
                source: lilia_desktop_application::DesktopConversationSuggestionSource::Provider,
            },
            false,
        );

        let update = state.conversation_suggestion_update(false);

        assert!(!update.enabled);
        assert_eq!(
            update.source,
            lilia_desktop_application::DesktopConversationSuggestionSource::Provider
        );
    }

    #[test]
    fn composer_model_options_merge_configured_and_contract_defaults_without_duplicates() {
        let mut assistant = DesktopAssistantAiSettings::default();
        assistant.model_pool = vec![
            lilia_desktop_application::DesktopAssistantAiModelPoolItem {
                id: "pool-model".to_owned(),
                label: "Pool".to_owned(),
                source: "remote".to_owned(),
                backend: "native-agentkit".to_owned(),
            },
            lilia_desktop_application::DesktopAssistantAiModelPoolItem {
                id: "runtime-model".to_owned(),
                label: "Duplicate".to_owned(),
                source: "remote".to_owned(),
                backend: "native-agentkit".to_owned(),
            },
        ];
        let state = ProviderAiSettingsState::new(
            assistant,
            DesktopModelFeatureSettings::default(),
            DesktopConversationSuggestionSettings::default(),
            false,
        );

        let options = state.composer_model_options(Some("runtime-model"));

        assert_eq!(
            options
                .iter()
                .filter(|(model, _)| model == "runtime-model")
                .count(),
            1
        );
        assert!(options.iter().any(|(model, _)| model == "pool-model"));
        assert!(options.len() >= 3);
    }

    #[test]
    fn custom_preset_crud_keeps_stable_unique_ids_and_editable_fields() {
        let mut state = ProviderAiSettingsState::new(
            DesktopAssistantAiSettings::default(),
            DesktopModelFeatureSettings::default(),
            DesktopConversationSuggestionSettings::default(),
            false,
        );

        let first = state.add_custom_preset("  中文 Review  ");
        let second = state.add_custom_preset("中文 Review");
        state.rename_custom_preset(&first, "  审查专用  ".to_owned());
        state.set_preset_model(&first, "review-model".to_owned());
        state.cycle_preset_effort(&first);

        assert_ne!(first, second);
        let preset = state
            .custom_presets()
            .find(|preset| preset.id == first)
            .unwrap();
        assert_eq!(preset.label, "审查专用");
        assert_eq!(preset.model.as_deref(), Some("review-model"));
        assert_eq!(preset.reasoning_effort.as_deref(), Some("low"));

        state.remove_custom_preset(&first);
        assert!(!state.has_preset(&first));
        assert!(state.has_preset(&second));
        assert!(state.model_features_dirty());
    }

    #[test]
    fn assistant_model_pool_crud_and_remote_merge_preserve_user_labels() {
        let mut state = ProviderAiSettingsState::new(
            DesktopAssistantAiSettings::default(),
            DesktopModelFeatureSettings::default(),
            DesktopConversationSuggestionSettings::default(),
            false,
        );

        assert!(state.add_assistant_model("  model-a  ", "  My Model  "));
        assert!(state.add_assistant_model("model-b", ""));
        state.rename_assistant_model("model-b", "Local B".to_owned());
        let added = state.merge_fetched_assistant_models(vec![
            DesktopAssistantAiModelPoolItem {
                id: "model-a".to_owned(),
                label: "Remote A".to_owned(),
                source: "remote".to_owned(),
                backend: "native-agentkit".to_owned(),
            },
            DesktopAssistantAiModelPoolItem {
                id: "model-c".to_owned(),
                label: "Remote C".to_owned(),
                source: "remote".to_owned(),
                backend: "native-agentkit".to_owned(),
            },
        ]);

        assert_eq!(added, 1);
        let items = state.assistant_model_pool().collect::<Vec<_>>();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].id, "model-a");
        assert_eq!(items[0].label, "My Model");
        assert_eq!(items[1].label, "Local B");
        assert_eq!(items[2].id, "model-c");
        assert!(state.assistant_dirty());
        assert_eq!(
            state.assistant_update().settings.model_pool,
            items.into_iter().cloned().collect::<Vec<_>>()
        );
    }
}

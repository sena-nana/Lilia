mod cache;
mod github_activity;
mod model;
mod scope;
mod settings;

pub use lilia_feature_suggestions::types::{
    DesktopSuggestionGitHubActivityRef, DesktopSuggestionItem, DesktopSuggestionItemSource,
    DesktopSuggestionLocalGitContextRef, DesktopSuggestionLocalGitProbe,
    DesktopSuggestionModelRequest, DesktopSuggestionSessionThreadRef, DesktopSuggestionSourceProbe,
};
pub use model::{
    request_model_completion, ConversationSuggestionModelPort,
    DesktopApplicationSuggestionModelPort,
};
pub use settings::{
    DesktopConversationSuggestionError, DesktopConversationSuggestionSettings,
    DesktopConversationSuggestionSource, CONVERSATION_SUGGESTION_SETTINGS_KEY,
};

use cache::{build_cache_key, cache_scope_key};
use lilia_feature_suggestions::generation::{
    build_generation_prompt, materialize_items, parse_model_suggestions,
};
use scope::summarize_scope_sources;

use crate::application::{DesktopApplication, DesktopApplicationError, DesktopEventKind};

impl DesktopApplication {
    pub fn conversation_suggestion_sources(
        &self,
        project_id: Option<&str>,
    ) -> Result<DesktopSuggestionSourceProbe, DesktopApplicationError> {
        let settings = self.conversation_suggestion_settings()?;
        if !settings.enabled {
            return Ok(DesktopSuggestionSourceProbe {
                sources: Vec::new(),
                local_git: None,
            });
        }
        let Some(scope) = self.build_suggestion_scope(project_id)? else {
            return Ok(DesktopSuggestionSourceProbe {
                sources: Vec::new(),
                local_git: None,
            });
        };
        Ok(summarize_scope_sources(&scope))
    }

    pub fn conversation_suggestions(
        &self,
        project_id: Option<&str>,
        force_refresh: bool,
        models: &dyn ConversationSuggestionModelPort,
    ) -> Result<Vec<DesktopSuggestionItem>, DesktopApplicationError> {
        let _generation = self
            .inner
            .conversation_suggestion_generation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let settings = self.conversation_suggestion_settings()?;
        if !settings.enabled {
            return Ok(Vec::new());
        }
        let Some(scope) = self.build_suggestion_scope(project_id)? else {
            return Ok(Vec::new());
        };
        let requests = models.resolve_requests();
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let prompt = build_generation_prompt(&scope);
        let cache_scope = cache_scope_key(project_id, &settings.source);
        for model in requests {
            let cache_key = build_cache_key(&scope, &model);
            if !force_refresh {
                if let Some(hit) = self.load_suggestion_cache_hit(&cache_scope, &cache_key)? {
                    return Ok(hit.items);
                }
            }
            match models
                .request_completion(&model, &prompt)
                .and_then(parse_model_suggestions)
            {
                Ok(raw) => {
                    let generated = materialize_items(raw, &scope);
                    if let Err(error) = self.save_suggestion_cache(
                        cache_scope.clone(),
                        cache_key,
                        generated.clone(),
                    ) {
                        eprintln!("[conversation-suggestions] save cache failed: {error}");
                    }
                    self.emit_event(DesktopEventKind::ConversationSuggestionsChanged {
                        project_id: project_id.map(str::to_owned),
                    });
                    return Ok(generated);
                }
                Err(error) => {
                    eprintln!("[conversation-suggestions] generate failed: {error}");
                }
            }
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use tempfile::TempDir;

    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopConversationSuggestionSettings,
        DesktopConversationSuggestionSource, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, DesktopTaskCreate, DesktopTodoCreate,
        DesktopTodoPriority,
    };

    struct TestHost;

    impl DesktopHost for TestHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    struct RecordingModel {
        calls: AtomicUsize,
        prompts: Mutex<Vec<String>>,
        task_id: String,
    }

    impl RecordingModel {
        fn new(task_id: String) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                prompts: Mutex::new(Vec::new()),
                task_id,
            }
        }
    }

    impl ConversationSuggestionModelPort for RecordingModel {
        fn resolve_requests(&self) -> Vec<DesktopSuggestionModelRequest> {
            vec![DesktopSuggestionModelRequest {
                source: DesktopConversationSuggestionSource::AssistantAi,
                backend: None,
                model: "test-model".to_owned(),
                base_url: "http://localhost".to_owned(),
                api_key: "test-key".to_owned(),
            }]
        }

        fn request_completion(
            &self,
            _model: &DesktopSuggestionModelRequest,
            prompt: &str,
        ) -> Result<String, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.prompts
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(prompt.to_owned());
            Ok(serde_json::json!([{
                "taskIds": [self.task_id],
                "summary": "继续权限回退",
                "reason": "任务仍有明确的未完成事项",
                "prompt": "请继续补齐权限失败后的回退路径。"
            }])
            .to_string())
        }
    }

    fn suggestion_app() -> (TempDir, DesktopApplication, String) {
        let home = TempDir::new().unwrap();
        let config = DesktopApplicationConfig::new(
            home.path(),
            format!("suggestion-test-{}", uuid::Uuid::new_v4()),
        )
        .unwrap();
        let application = DesktopApplication::bootstrap(config, Arc::new(TestHost)).unwrap();
        let task = application
            .create_task(DesktopTaskCreate::new(None, "补齐权限处理"))
            .unwrap();
        application
            .create_task_todo(DesktopTodoCreate {
                task_id: task.id.clone(),
                text: "补齐权限失败回退".to_owned(),
                priority: DesktopTodoPriority::High,
                attachments: Vec::new(),
                conversation_references: Vec::new(),
                workflow: None,
            })
            .unwrap();
        (home, application, task.id.as_str().to_owned())
    }

    #[test]
    fn generation_uses_unfinished_product_facts_and_reuses_the_durable_cache() {
        let (_home, application, task_id) = suggestion_app();
        let model = RecordingModel::new(task_id.clone());

        let first = application
            .conversation_suggestions(None, false, &model)
            .unwrap();
        let cached = application
            .conversation_suggestions(None, false, &model)
            .unwrap();

        assert_eq!(model.calls.load(Ordering::SeqCst), 1);
        assert_eq!(first, cached);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].task_ids, vec![task_id.clone()]);
        assert_eq!(first[0].source, DesktopSuggestionItemSource::Task);
        let prompts = model
            .prompts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(prompts[0].contains(&format!("任务 {task_id}")));
        assert!(prompts[0].contains("未完成信号: todo: 补齐权限失败回退"));
    }

    #[test]
    fn force_refresh_bypasses_cache_and_disabled_settings_skip_the_model() {
        let (_home, application, task_id) = suggestion_app();
        let model = RecordingModel::new(task_id);

        application
            .conversation_suggestions(None, false, &model)
            .unwrap();
        application
            .conversation_suggestions(None, true, &model)
            .unwrap();
        assert_eq!(model.calls.load(Ordering::SeqCst), 2);

        application
            .save_conversation_suggestion_settings(DesktopConversationSuggestionSettings {
                enabled: false,
                source: DesktopConversationSuggestionSource::AssistantAi,
            })
            .unwrap();
        assert!(application
            .conversation_suggestions(None, true, &model)
            .unwrap()
            .is_empty());
        assert_eq!(model.calls.load(Ordering::SeqCst), 2);
    }
}

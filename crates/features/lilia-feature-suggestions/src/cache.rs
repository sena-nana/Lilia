use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::{
    DesktopSuggestionItem, DesktopSuggestionModelRequest, SuggestionScope, SuggestionSource,
    CACHE_TTL_MS,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionCacheEntry {
    pub cache_key: String,
    pub generated_at: i64,
    pub items: Vec<DesktopSuggestionItem>,
}

pub type SuggestionCache = HashMap<String, SuggestionCacheEntry>;

pub fn cache_scope_key(project_id: Option<&str>, source: &SuggestionSource) -> String {
    format!("{}:{}", source.as_str(), project_id.unwrap_or("__recent__"))
}

pub fn cache_entry_is_valid(
    entry: &SuggestionCacheEntry,
    cache_key: &str,
    now: i64,
) -> bool {
    entry.cache_key == cache_key && now.saturating_sub(entry.generated_at) <= CACHE_TTL_MS
}

pub fn build_cache_key(
    scope: &SuggestionScope,
    model: &DesktopSuggestionModelRequest,
) -> String {
    let signal_fingerprint = scope
        .tasks
        .iter()
        .map(|task| {
            format!(
                "{}@{}:{}",
                task.id,
                task.latest_updated_at,
                task.unfinished_signals.join(" / ")
            )
        })
        .collect::<Vec<_>>()
        .join("||");
    let github_fingerprint = scope
        .github_activities
        .iter()
        .map(|activity| activity.fingerprint.as_str())
        .collect::<Vec<_>>()
        .join("||");
    let local_git_fingerprint = scope
        .local_git_contexts
        .iter()
        .map(|context| context.fingerprint.as_str())
        .collect::<Vec<_>>()
        .join("||");
    let session_thread_fingerprint = scope
        .codex_threads
        .iter()
        .map(|thread| thread.fingerprint.as_str())
        .collect::<Vec<_>>()
        .join("||");
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        scope.project_id.as_deref().unwrap_or("__recent__"),
        model.source.as_str(),
        model.backend.as_deref().unwrap_or("assistant-ai"),
        model.model,
        scope.latest_updated_at,
        signal_fingerprint,
        scope
            .github_repo
            .as_ref()
            .map(|repo| repo.full_name.as_str())
            .unwrap_or("__no_github_repo__"),
        github_fingerprint,
        local_git_fingerprint,
        session_thread_fingerprint
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use crate::types::{
        DesktopSuggestionLocalGitContextRef, DesktopSuggestionSessionThreadRef,
        GitHubActivitySample, LocalGitContextSample, SessionThreadSample, TaskSample,
    };
    use crate::settings::DesktopConversationSuggestionSource;

    #[test]
    fn cache_key_includes_source_and_scope() {
        assert_eq!(
            cache_scope_key(
                Some("p1"),
                &DesktopConversationSuggestionSource::AssistantAi
            ),
            "assistant-ai:p1"
        );
    }

    #[test]
    fn cache_ttl_validation() {
        let entry = SuggestionCacheEntry {
            cache_key: "k".into(),
            generated_at: 100,
            items: Vec::new(),
        };
        assert!(cache_entry_is_valid(&entry, "k", 100));
        assert!(!cache_entry_is_valid(&entry, "other", 100));
        assert!(!cache_entry_is_valid(&entry, "k", 100 + CACHE_TTL_MS + 1));
    }

    #[test]
    fn cache_key_tracks_every_generation_input_fingerprint() {
        let model = DesktopSuggestionModelRequest {
            source: DesktopConversationSuggestionSource::AssistantAi,
            backend: None,
            model: "mini".into(),
            base_url: "http://localhost".into(),
            api_key: "key".into(),
        };
        let mut scope = SuggestionScope {
            project_id: Some("p1".into()),
            project_name: None,
            tasks: vec![TaskSample {
                id: "task-1".into(),
                title: "Task".into(),
                status: "running".into(),
                project_id: Some("p1".into()),
                user_messages: Vec::new(),
                assistant_message: None,
                unfinished_signals: vec!["todo: first".into()],
                latest_updated_at: 10,
            }],
            github_repo: None,
            github_activities: vec![GitHubActivitySample {
                id: "gh-1".into(),
                repo_full_name: "owner/repo".into(),
                kind: "issue".into(),
                action: "opened".into(),
                title: "Issue".into(),
                url: None,
                details: Vec::new(),
                fingerprint: "github-first".into(),
            }],
            local_git_contexts: vec![LocalGitContextSample {
                context: DesktopSuggestionLocalGitContextRef {
                    id: "local-git-current".into(),
                    branch: "main".into(),
                    status: "clean".into(),
                    changed_files: Vec::new(),
                    recent_commits: vec!["abc first".into()],
                },
                fingerprint: "local-first".into(),
            }],
            codex_threads: vec![SessionThreadSample {
                thread: DesktopSuggestionSessionThreadRef {
                    id: "thread-1".into(),
                    title: "First".into(),
                    updated_at: Some(10),
                    preview: None,
                },
                fingerprint: "thread-first".into(),
            }],
            latest_updated_at: 10,
        };

        let initial = build_cache_key(&scope, &model);
        scope.tasks[0].unfinished_signals = vec!["todo: second".into()];
        let task_changed = build_cache_key(&scope, &model);
        scope.github_activities[0].fingerprint = "github-second".into();
        let github_changed = build_cache_key(&scope, &model);
        scope.local_git_contexts[0].fingerprint = "local-second".into();
        let local_changed = build_cache_key(&scope, &model);
        scope.codex_threads[0].fingerprint = "thread-second".into();
        let thread_changed = build_cache_key(&scope, &model);

        assert_ne!(initial, task_changed);
        assert_ne!(task_changed, github_changed);
        assert_ne!(github_changed, local_changed);
        assert_ne!(local_changed, thread_changed);
    }
}

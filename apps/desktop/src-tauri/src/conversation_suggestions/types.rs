use serde::{Deserialize, Serialize};

use super::contract;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SuggestionSource {
    Provider,
    AssistantAi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionSettings {
    pub(crate) enabled: bool,
    pub(crate) source: SuggestionSource,
}

impl Default for SuggestionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            source: contract::default_suggestion_source(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionItem {
    pub(crate) id: String,
    pub(crate) project_id: Option<String>,
    pub(crate) task_ids: Vec<String>,
    pub(crate) source: SuggestionItemSource,
    pub(crate) github_activities: Vec<SuggestionGitHubActivityRef>,
    #[serde(default)]
    pub(crate) local_git_contexts: Vec<SuggestionLocalGitContextRef>,
    #[serde(default)]
    pub(crate) codex_threads: Vec<SuggestionCodexThreadRef>,
    pub(crate) summary: String,
    pub(crate) reason: String,
    pub(crate) prompt: String,
    pub(crate) generated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SuggestionItemSource {
    Task,
    Github,
    LocalGit,
    #[serde(alias = "codex-thread")]
    SessionThread,
    #[serde(alias = "claude")]
    Provider,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionGitHubActivityRef {
    pub(crate) id: String,
    pub(crate) repo_full_name: String,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionLocalGitContextRef {
    pub(crate) id: String,
    pub(crate) branch: String,
    pub(crate) status: String,
    pub(crate) changed_files: Vec<String>,
    pub(crate) recent_commits: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionCodexThreadRef {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) updated_at: Option<i64>,
    pub(crate) preview: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionSourceProbe {
    pub(crate) sources: Vec<SuggestionItemSource>,
    pub(crate) local_git: Option<SuggestionLocalGitProbe>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionLocalGitProbe {
    pub(crate) has_recent_commits: bool,
    pub(crate) has_changed_files: bool,
}

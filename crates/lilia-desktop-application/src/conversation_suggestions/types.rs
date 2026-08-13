use serde::{Deserialize, Serialize};

pub const SUGGESTION_CACHE_KEY: &str = "desktop.conversation-suggestions.cache.v1";
pub const CACHE_TTL_MS: i64 = 24 * 60 * 60 * 1000;
pub const MAX_TASKS_PER_SCOPE: usize = 3;
pub const TASK_CANDIDATE_LIMIT: usize = 12;
pub const MAX_SUGGESTIONS: usize = 3;
pub const SAMPLE_TEXT_LIMIT: usize = 280;
pub const SUMMARY_LIMIT: usize = 40;
pub const REASON_LIMIT: usize = 120;
pub const PROMPT_LIMIT: usize = 600;
pub const UNFINISHED_SIGNAL_LIMIT: usize = 5;
pub const GITHUB_EVENT_FETCH_LIMIT: usize = 30;
pub const GITHUB_ACTIVITY_LIMIT: usize = 6;
pub const LOCAL_GIT_COMMIT_LIMIT: usize = 3;
pub const LOCAL_GIT_FILE_LIMIT: usize = 12;

pub use super::settings::DesktopConversationSuggestionSource as SuggestionSource;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSuggestionItem {
    pub id: String,
    pub project_id: Option<String>,
    pub task_ids: Vec<String>,
    pub source: DesktopSuggestionItemSource,
    pub github_activities: Vec<DesktopSuggestionGitHubActivityRef>,
    #[serde(default)]
    pub local_git_contexts: Vec<DesktopSuggestionLocalGitContextRef>,
    #[serde(default)]
    pub codex_threads: Vec<DesktopSuggestionSessionThreadRef>,
    pub summary: String,
    pub reason: String,
    pub prompt: String,
    pub generated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DesktopSuggestionItemSource {
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
pub struct DesktopSuggestionGitHubActivityRef {
    pub id: String,
    pub repo_full_name: String,
    pub kind: String,
    pub title: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSuggestionLocalGitContextRef {
    pub id: String,
    pub branch: String,
    pub status: String,
    pub changed_files: Vec<String>,
    pub recent_commits: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSuggestionSessionThreadRef {
    pub id: String,
    pub title: String,
    pub updated_at: Option<i64>,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSuggestionSourceProbe {
    pub sources: Vec<DesktopSuggestionItemSource>,
    pub local_git: Option<DesktopSuggestionLocalGitProbe>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSuggestionLocalGitProbe {
    pub has_recent_commits: bool,
    pub has_changed_files: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TaskSample {
    pub id: String,
    pub title: String,
    pub status: String,
    pub project_id: Option<String>,
    pub user_messages: Vec<String>,
    pub assistant_message: Option<String>,
    pub unfinished_signals: Vec<String>,
    pub latest_updated_at: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectContext {
    pub name: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitHubRepoRef {
    pub owner: String,
    pub name: String,
    pub full_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct GitHubActivitySample {
    pub id: String,
    pub repo_full_name: String,
    pub kind: String,
    pub action: String,
    pub title: String,
    pub url: Option<String>,
    pub details: Vec<String>,
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
pub(crate) struct LocalGitContextSample {
    pub context: DesktopSuggestionLocalGitContextRef,
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionThreadSample {
    pub thread: DesktopSuggestionSessionThreadRef,
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SuggestionScope {
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub tasks: Vec<TaskSample>,
    pub github_repo: Option<GitHubRepoRef>,
    pub github_activities: Vec<GitHubActivitySample>,
    pub local_git_contexts: Vec<LocalGitContextSample>,
    pub codex_threads: Vec<SessionThreadSample>,
    pub latest_updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct DesktopSuggestionModelRequest {
    pub source: SuggestionSource,
    pub backend: Option<String>,
    pub model: String,
    pub base_url: String,
    pub api_key: String,
}

pub(crate) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

use std::collections::HashSet;

use lilia_contracts::{ProductTask, ProductTaskStatus, ProjectId, TimelineProjectionEvent};
use serde_json::Value as JsonValue;

use super::generation::{compact_line, truncate_chars};
use super::github_activity::load_github_activity_context;
use super::local_git::load_local_git_context;
use super::types::{
    DesktopSuggestionItemSource, DesktopSuggestionLocalGitProbe, DesktopSuggestionSourceProbe,
    GitHubActivitySample, GitHubRepoRef, LocalGitContextSample, ProjectContext,
    SessionThreadSample, SuggestionScope, TaskSample, MAX_TASKS_PER_SCOPE, SAMPLE_TEXT_LIMIT,
    TASK_CANDIDATE_LIMIT, UNFINISHED_SIGNAL_LIMIT,
};
use crate::{
    DesktopApplication, DesktopApplicationError, DesktopTaskTodo, ProjectQuery, TaskQuery,
};

impl DesktopApplication {
    pub(crate) fn build_suggestion_scope(
        &self,
        requested_project_id: Option<&str>,
    ) -> Result<Option<SuggestionScope>, DesktopApplicationError> {
        let project = self.load_suggestion_project_context(requested_project_id)?;
        let github_context =
            project
                .cwd
                .as_deref()
                .and_then(|cwd| match load_github_activity_context(self, cwd) {
                    Ok(context) => context,
                    Err(error) => {
                        eprintln!("[conversation-suggestions] github context skipped: {error}");
                        None
                    }
                });
        let local_git_contexts = if github_context.is_none() {
            project
                .cwd
                .as_deref()
                .and_then(|cwd| match load_local_git_context(cwd) {
                    Ok(context) => context,
                    Err(error) => {
                        eprintln!("[conversation-suggestions] local git context skipped: {error}");
                        None
                    }
                })
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        self.build_suggestion_scope_from_parts(
            requested_project_id,
            project,
            github_context,
            local_git_contexts,
            Vec::new(),
        )
    }

    pub(crate) fn build_suggestion_scope_from_parts(
        &self,
        requested_project_id: Option<&str>,
        project: ProjectContext,
        github_context: Option<(GitHubRepoRef, Vec<GitHubActivitySample>)>,
        local_git_contexts: Vec<LocalGitContextSample>,
        codex_threads: Vec<SessionThreadSample>,
    ) -> Result<Option<SuggestionScope>, DesktopApplicationError> {
        let tasks =
            self.load_suggestion_task_samples(requested_project_id, TASK_CANDIDATE_LIMIT)?;
        let tasks = tasks
            .into_iter()
            .filter(|task| !task.unfinished_signals.is_empty())
            .take(MAX_TASKS_PER_SCOPE)
            .collect::<Vec<_>>();
        let (github_repo, github_activities) = match github_context {
            Some((repo, activities)) => (Some(repo), activities),
            None => (None, Vec::new()),
        };
        let local_git_contexts = if github_activities.is_empty() {
            local_git_contexts
        } else {
            Vec::new()
        };
        if tasks.is_empty()
            && github_activities.is_empty()
            && local_git_contexts.is_empty()
            && codex_threads.is_empty()
        {
            return Ok(None);
        }
        let latest_updated_at = tasks
            .iter()
            .map(|task| task.latest_updated_at)
            .chain(
                codex_threads
                    .iter()
                    .filter_map(|thread| thread.thread.updated_at),
            )
            .max()
            .unwrap_or(0);
        let project_id = requested_project_id
            .map(str::to_string)
            .or_else(|| tasks.iter().find_map(|task| task.project_id.clone()));
        Ok(Some(SuggestionScope {
            project_id,
            project_name: project.name,
            tasks,
            github_repo,
            github_activities,
            local_git_contexts,
            codex_threads,
            latest_updated_at,
        }))
    }
}

pub(crate) fn summarize_scope_sources(scope: &SuggestionScope) -> DesktopSuggestionSourceProbe {
    let mut sources = Vec::new();
    if !scope.tasks.is_empty() {
        sources.push(DesktopSuggestionItemSource::Task);
    }
    if !scope.github_activities.is_empty() {
        sources.push(DesktopSuggestionItemSource::Github);
    }
    if !scope.codex_threads.is_empty() {
        sources.push(DesktopSuggestionItemSource::SessionThread);
    }
    let local_git = if scope.local_git_contexts.is_empty() {
        None
    } else {
        sources.push(DesktopSuggestionItemSource::LocalGit);
        Some(DesktopSuggestionLocalGitProbe {
            has_recent_commits: scope
                .local_git_contexts
                .iter()
                .any(|context| !context.context.recent_commits.is_empty()),
            has_changed_files: scope
                .local_git_contexts
                .iter()
                .any(|context| !context.context.changed_files.is_empty()),
        })
    };
    DesktopSuggestionSourceProbe { sources, local_git }
}

impl DesktopApplication {
    fn load_suggestion_project_context(
        &self,
        requested_project_id: Option<&str>,
    ) -> Result<ProjectContext, DesktopApplicationError> {
        let Some(project_id) = requested_project_id else {
            return Ok(ProjectContext {
                name: None,
                cwd: None,
            });
        };
        let project_id =
            ProjectId::new(project_id).map_err(|error| DesktopApplicationError::InvalidInput {
                field: "project_id",
                message: error.to_string(),
            })?;
        let project = self
            .query_projects(ProjectQuery::default())?
            .into_iter()
            .find(|project| project.id == project_id);
        Ok(ProjectContext {
            name: project.as_ref().map(|project| project.name.clone()),
            cwd: project.and_then(|project| project.workspace_path),
        })
    }

    fn load_suggestion_task_samples(
        &self,
        requested_project_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TaskSample>, DesktopApplicationError> {
        let query = match requested_project_id {
            Some(project_id) => {
                let project_id = ProjectId::new(project_id).map_err(|error| {
                    DesktopApplicationError::InvalidInput {
                        field: "project_id",
                        message: error.to_string(),
                    }
                })?;
                TaskQuery::for_project(project_id)
            }
            None => TaskQuery::default(),
        };
        let mut tasks = self.query_tasks(query)?;
        tasks.sort_by_key(|task| std::cmp::Reverse(task.updated_at));
        tasks.truncate(limit);
        let mut samples = Vec::new();
        for task in tasks {
            if let Some(sample) = self.load_suggestion_task_sample(&task)? {
                samples.push(sample);
            }
        }
        Ok(samples)
    }

    fn load_suggestion_task_sample(
        &self,
        task: &ProductTask,
    ) -> Result<Option<TaskSample>, DesktopApplicationError> {
        let timeline = self.task_timeline_page(&task.id, None, 40)?.events;
        let todos = self.list_task_todos(&task.id)?;
        let unfinished_signals =
            unfinished_signals_from_task(&todos, &timeline, UNFINISHED_SIGNAL_LIMIT);
        if unfinished_signals.is_empty() {
            return Ok(None);
        }
        Ok(Some(TaskSample {
            user_messages: event_texts(&timeline, "message", Some("user"), None, 2),
            assistant_message: event_texts(
                &timeline,
                "message",
                Some("assistant"),
                Some("success"),
                1,
            )
            .into_iter()
            .next(),
            unfinished_signals,
            id: task.id.as_str().to_owned(),
            title: task.title.clone(),
            status: product_status_label(task.status).to_owned(),
            project_id: task.project_id.as_ref().map(|id| id.as_str().to_owned()),
            latest_updated_at: task.updated_at,
        }))
    }
}

fn product_status_label(status: ProductTaskStatus) -> &'static str {
    match status {
        ProductTaskStatus::Draft => "draft",
        ProductTaskStatus::Waiting => "waiting",
        ProductTaskStatus::Running => "running",
        ProductTaskStatus::Blocked => "blocked",
        ProductTaskStatus::Done => "done",
        ProductTaskStatus::Cancelled => "cancelled",
    }
}

fn event_texts(
    events: &[TimelineProjectionEvent],
    kind: &str,
    role: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    for event in events.iter().rev() {
        if event.kind != kind {
            continue;
        }
        if status.is_some_and(|expected| event.status != expected) {
            continue;
        }
        if role.is_some_and(|expected| {
            event.payload.get("role").and_then(|value| value.as_str()) != Some(expected)
        }) {
            continue;
        }
        let text = event
            .payload
            .get("content")
            .and_then(|value| value.as_str())
            .or(event.summary.as_deref())
            .map(compact_line)
            .unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        out.push(truncate_chars(&text, SAMPLE_TEXT_LIMIT));
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn unfinished_signals_from_task(
    todos: &[DesktopTaskTodo],
    events: &[TimelineProjectionEvent],
    limit: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for todo in todos.iter().filter(|todo| !todo.done) {
        push_unfinished_signal(&mut out, &mut seen, "todo", &todo.text, limit);
        if out.len() >= limit {
            return out;
        }
    }
    for event in events.iter().rev() {
        match event.kind.as_str() {
            "todo_list" => {
                for text in unfinished_todo_payload_items(&event.payload) {
                    push_unfinished_signal(&mut out, &mut seen, "todo", &text, limit);
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
            "error" => {
                push_unfinished_signal(
                    &mut out,
                    &mut seen,
                    "error",
                    event.summary.as_deref().unwrap_or(event.title.as_str()),
                    limit,
                );
            }
            _ => {}
        }
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn push_unfinished_signal(
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
    kind: &str,
    text: &str,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }
    let text = compact_line(text);
    if text.is_empty() || !seen.insert(format!("{kind}:{text}")) {
        return;
    }
    out.push(truncate_chars(
        &format!("{kind}: {text}"),
        SAMPLE_TEXT_LIMIT,
    ));
}

fn unfinished_todo_payload_items(payload: &JsonValue) -> Vec<String> {
    let Some(items) = payload
        .get("items")
        .or_else(|| payload.get("todos"))
        .and_then(|value| value.as_array())
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            if let Some(text) = item.as_str().map(str::trim).filter(|text| !text.is_empty()) {
                return Some(text.to_string());
            }
            if todo_item_is_done(item) {
                return None;
            }
            item.get("content")
                .or_else(|| item.get("text"))
                .or_else(|| item.get("title"))
                .or_else(|| item.get("description"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        })
        .collect()
}

fn todo_item_is_done(item: &JsonValue) -> bool {
    item.get("completed")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || item
            .get("done")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        || item
            .get("status")
            .and_then(|value| value.as_str())
            .map(|status| status.eq_ignore_ascii_case("completed"))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation_suggestions::types::DesktopSuggestionLocalGitContextRef;

    #[test]
    fn summarize_includes_local_git_probe() {
        let scope = SuggestionScope {
            project_id: None,
            project_name: None,
            tasks: Vec::new(),
            github_repo: None,
            github_activities: Vec::new(),
            local_git_contexts: vec![LocalGitContextSample {
                context: DesktopSuggestionLocalGitContextRef {
                    id: "local-git-current".into(),
                    branch: "main".into(),
                    status: "dirty: 1 changed files".into(),
                    changed_files: vec!["M a.rs".into()],
                    recent_commits: vec!["abc".into()],
                },
                fingerprint: "fp".into(),
            }],
            codex_threads: Vec::new(),
            latest_updated_at: 0,
        };
        let probe = summarize_scope_sources(&scope);
        assert_eq!(probe.sources, vec![DesktopSuggestionItemSource::LocalGit]);
        assert_eq!(
            probe.local_git,
            Some(DesktopSuggestionLocalGitProbe {
                has_recent_commits: true,
                has_changed_files: true,
            })
        );
    }

    #[test]
    fn unfinished_todo_payload_filters_completed() {
        let items = unfinished_todo_payload_items(&serde_json::json!({
            "items": [
                {"text": "done", "done": true},
                {"text": "open", "done": false}
            ]
        }));
        assert_eq!(items, vec!["open".to_string()]);
    }
}

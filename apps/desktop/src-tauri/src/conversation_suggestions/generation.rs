use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use uuid::Uuid;

use crate::prompt_contract;

use super::types::{
    now_millis, SuggestionGitHubActivityRef, SuggestionItem, SuggestionItemSource, SuggestionScope,
    MAX_SUGGESTIONS, PROMPT_LIMIT, REASON_LIMIT, SAMPLE_TEXT_LIMIT, SUMMARY_LIMIT,
};

pub(super) fn build_generation_prompt(scope: &SuggestionScope) -> String {
    let mut lines = prompt_contract::suggestion_generation_rules().to_vec();
    lines.push(format!(
        "scopeProjectId: {}",
        scope.project_id.as_deref().unwrap_or("recent-projects")
    ));
    if let Some(name) = &scope.project_name {
        lines.push(format!(
            "projectName: {}",
            truncate_chars(&compact_line(name), 80)
        ));
    }
    for task in &scope.tasks {
        lines.push(format!(
            "\n任务 {} | 标题: {} | 状态: {}",
            task.id,
            truncate_chars(&compact_line(&task.title), 80),
            task.status
        ));
        for text in &task.user_messages {
            lines.push(format!("用户: {}", truncate_chars(text, SAMPLE_TEXT_LIMIT)));
        }
        if let Some(text) = &task.assistant_message {
            lines.push(format!(
                "最近回复: {}",
                truncate_chars(text, SAMPLE_TEXT_LIMIT)
            ));
        }
        for signal in &task.unfinished_signals {
            lines.push(format!(
                "未完成信号: {}",
                truncate_chars(signal, SAMPLE_TEXT_LIMIT)
            ));
        }
    }
    if let Some(repo) = &scope.github_repo {
        lines.push(format!("\nGitHub 仓库: {}", repo.full_name));
    }
    for activity in &scope.github_activities {
        lines.push(format!(
            "GitHub 活动 {} | 类型: {} | action: {} | 标题: {}",
            activity.id,
            activity.kind,
            activity.action,
            truncate_chars(&compact_line(&activity.title), SAMPLE_TEXT_LIMIT)
        ));
        if let Some(url) = &activity.url {
            lines.push(format!("链接: {url}"));
        }
        for detail in &activity.details {
            lines.push(format!(
                "活动细节: {}",
                truncate_chars(&compact_line(detail), SAMPLE_TEXT_LIMIT)
            ));
        }
    }
    for context in &scope.local_git_contexts {
        lines.push(format!(
            "\n本地 Git 上下文 {} | branch: {} | status: {}",
            context.context.id,
            truncate_chars(&compact_line(&context.context.branch), SAMPLE_TEXT_LIMIT),
            truncate_chars(&compact_line(&context.context.status), SAMPLE_TEXT_LIMIT)
        ));
        for commit in &context.context.recent_commits {
            lines.push(format!(
                "最近提交: {}",
                truncate_chars(&compact_line(commit), SAMPLE_TEXT_LIMIT)
            ));
        }
        for file in &context.context.changed_files {
            lines.push(format!(
                "变更文件: {}",
                truncate_chars(&compact_line(file), SAMPLE_TEXT_LIMIT)
            ));
        }
    }
    for thread in &scope.codex_threads {
        lines.push(format!(
            "\nCodex thread {} | 标题: {} | updatedAt: {}",
            thread.thread.id,
            truncate_chars(&compact_line(&thread.thread.title), SAMPLE_TEXT_LIMIT),
            thread.thread.updated_at.unwrap_or(0)
        ));
        if let Some(preview) = &thread.thread.preview {
            lines.push(format!(
                "thread 预览: {}",
                truncate_chars(&compact_line(preview), SAMPLE_TEXT_LIMIT)
            ));
        }
    }
    lines.join("\n")
}

#[derive(Debug, Deserialize)]
pub(super) struct RawSuggestion {
    #[serde(default, rename = "taskIds")]
    pub(super) task_ids: Vec<String>,
    #[serde(default, rename = "githubActivityIds")]
    pub(super) github_activity_ids: Vec<String>,
    #[serde(default, rename = "localGitContextIds")]
    pub(super) local_git_context_ids: Vec<String>,
    #[serde(default, rename = "codexThreadIds")]
    pub(super) codex_thread_ids: Vec<String>,
    pub(super) summary: Option<String>,
    pub(super) reason: Option<String>,
    pub(super) prompt: Option<String>,
}

pub(super) fn parse_model_suggestions(text: String) -> Result<Vec<RawSuggestion>, String> {
    let trimmed = text.trim();
    let json_text = if trimmed.starts_with("```") {
        trimmed
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    } else {
        trimmed
    };
    serde_json::from_str::<Vec<RawSuggestion>>(json_text)
        .map_err(|e| format!("建议 JSON 解析失败：{e}"))
}

pub(super) fn materialize_items(
    raw: Vec<RawSuggestion>,
    scope: &SuggestionScope,
) -> Vec<SuggestionItem> {
    let generated_at = now_millis();
    let valid_task_ids: HashSet<String> = scope.tasks.iter().map(|task| task.id.clone()).collect();
    let activity_by_id = scope
        .github_activities
        .iter()
        .map(|activity| (activity.id.clone(), activity))
        .collect::<HashMap<_, _>>();
    let local_git_context_by_id = scope
        .local_git_contexts
        .iter()
        .map(|context| (context.context.id.clone(), context))
        .collect::<HashMap<_, _>>();
    let codex_thread_by_id = scope
        .codex_threads
        .iter()
        .map(|thread| (thread.thread.id.clone(), thread))
        .collect::<HashMap<_, _>>();
    raw.into_iter()
        .filter_map(|item| {
            let task_ids = item
                .task_ids
                .into_iter()
                .filter(|task_id| valid_task_ids.contains(task_id))
                .collect::<Vec<_>>();
            let github_activities = item
                .github_activity_ids
                .into_iter()
                .filter_map(|activity_id| activity_by_id.get(&activity_id).copied())
                .map(|activity| SuggestionGitHubActivityRef {
                    id: activity.id.clone(),
                    repo_full_name: activity.repo_full_name.clone(),
                    kind: activity.kind.clone(),
                    title: activity.title.clone(),
                    url: activity.url.clone(),
                })
                .collect::<Vec<_>>();
            let local_git_contexts = item
                .local_git_context_ids
                .into_iter()
                .filter_map(|context_id| local_git_context_by_id.get(&context_id).copied())
                .map(|context| context.context.clone())
                .collect::<Vec<_>>();
            let codex_threads = item
                .codex_thread_ids
                .into_iter()
                .filter_map(|thread_id| codex_thread_by_id.get(&thread_id).copied())
                .map(|thread| thread.thread.clone())
                .collect::<Vec<_>>();
            if task_ids.is_empty()
                && github_activities.is_empty()
                && local_git_contexts.is_empty()
                && codex_threads.is_empty()
            {
                return None;
            }
            let summary = truncate_chars(&compact_line(&item.summary?), SUMMARY_LIMIT);
            let reason = truncate_chars(&compact_line(&item.reason?), REASON_LIMIT);
            let prompt = truncate_chars(item.prompt?.trim(), PROMPT_LIMIT);
            if summary.is_empty() || reason.is_empty() || prompt.is_empty() {
                return None;
            }
            Some(SuggestionItem {
                id: format!("sg-{}", Uuid::new_v4()),
                project_id: scope.project_id.clone(),
                source: if task_ids.is_empty() {
                    if github_activities.is_empty() {
                        if local_git_contexts.is_empty() {
                            SuggestionItemSource::SessionThread
                        } else {
                            SuggestionItemSource::LocalGit
                        }
                    } else {
                        SuggestionItemSource::Github
                    }
                } else {
                    SuggestionItemSource::Task
                },
                task_ids,
                github_activities,
                local_git_contexts,
                codex_threads,
                summary,
                reason,
                prompt,
                generated_at,
            })
        })
        .take(MAX_SUGGESTIONS)
        .collect()
}

pub(super) fn compact_line(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn truncate_chars(input: &str, max: usize) -> String {
    let mut out = String::new();
    for (index, ch) in input.chars().enumerate() {
        if index >= max {
            out.push('…');
            return out;
        }
        out.push(ch);
    }
    out
}

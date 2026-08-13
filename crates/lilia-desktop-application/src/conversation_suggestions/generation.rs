use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::Deserialize;
use uuid::Uuid;

use super::types::{
    now_millis, DesktopSuggestionGitHubActivityRef, DesktopSuggestionItem,
    DesktopSuggestionItemSource, SuggestionScope, MAX_SUGGESTIONS, PROMPT_LIMIT, REASON_LIMIT,
    SAMPLE_TEXT_LIMIT, SUMMARY_LIMIT,
};

#[derive(Debug, Deserialize)]
struct PromptSuggestionSection {
    #[serde(rename = "systemInstruction")]
    system_instruction: String,
    #[serde(rename = "generationRules")]
    generation_rules: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PromptTextContract {
    suggestion: PromptSuggestionSection,
}

fn prompt_suggestion() -> &'static PromptSuggestionSection {
    static CONTRACT: OnceLock<PromptSuggestionSection> = OnceLock::new();
    CONTRACT.get_or_init(|| {
        let contract: PromptTextContract = serde_json::from_str(include_str!(
            "../../../lilia-contracts/contracts/prompt-text.json"
        ))
        .expect("prompt-text.json must deserialize");
        contract.suggestion
    })
}

pub fn suggestion_system_instruction() -> &'static str {
    &prompt_suggestion().system_instruction
}

pub fn suggestion_generation_rules() -> &'static [String] {
    &prompt_suggestion().generation_rules
}

pub(crate) fn build_generation_prompt(scope: &SuggestionScope) -> String {
    let mut lines = suggestion_generation_rules().to_vec();
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
pub(crate) struct RawSuggestion {
    #[serde(default, rename = "taskIds")]
    pub task_ids: Vec<String>,
    #[serde(default, rename = "githubActivityIds")]
    pub github_activity_ids: Vec<String>,
    #[serde(default, rename = "localGitContextIds")]
    pub local_git_context_ids: Vec<String>,
    #[serde(default, rename = "codexThreadIds")]
    pub codex_thread_ids: Vec<String>,
    pub summary: Option<String>,
    pub reason: Option<String>,
    pub prompt: Option<String>,
}

pub(crate) fn parse_model_suggestions(text: String) -> Result<Vec<RawSuggestion>, String> {
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
        .map_err(|error| format!("建议 JSON 解析失败：{error}"))
}

pub(crate) fn materialize_items(
    raw: Vec<RawSuggestion>,
    scope: &SuggestionScope,
) -> Vec<DesktopSuggestionItem> {
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
                .map(|activity| DesktopSuggestionGitHubActivityRef {
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
            Some(DesktopSuggestionItem {
                id: format!("sg-{}", Uuid::new_v4()),
                project_id: scope.project_id.clone(),
                source: if task_ids.is_empty() {
                    if github_activities.is_empty() {
                        if local_git_contexts.is_empty() {
                            DesktopSuggestionItemSource::SessionThread
                        } else {
                            DesktopSuggestionItemSource::LocalGit
                        }
                    } else {
                        DesktopSuggestionItemSource::Github
                    }
                } else {
                    DesktopSuggestionItemSource::Task
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

pub fn compact_line(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn truncate_chars(input: &str, max: usize) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation_suggestions::types::{
        DesktopSuggestionLocalGitContextRef, DesktopSuggestionSessionThreadRef,
        GitHubActivitySample, GitHubRepoRef, LocalGitContextSample, SessionThreadSample,
        SuggestionScope, TaskSample,
    };

    fn source_scope() -> SuggestionScope {
        SuggestionScope {
            project_id: Some("project-1".into()),
            project_name: Some("Demo".into()),
            tasks: vec![TaskSample {
                id: "task-1".into(),
                title: "Wire suggestions".into(),
                status: "running".into(),
                project_id: Some("project-1".into()),
                user_messages: vec!["继续".into()],
                assistant_message: None,
                unfinished_signals: vec!["todo: finish adapter".into()],
                latest_updated_at: 10,
            }],
            github_repo: Some(GitHubRepoRef {
                owner: "sena-nana".into(),
                name: "LiliaCode".into(),
                full_name: "sena-nana/LiliaCode".into(),
            }),
            github_activities: vec![GitHubActivitySample {
                id: "gh-1".into(),
                repo_full_name: "sena-nana/LiliaCode".into(),
                kind: "pull_request".into(),
                action: "opened".into(),
                title: "PR #1".into(),
                url: None,
                details: Vec::new(),
                fingerprint: "github-fp".into(),
            }],
            local_git_contexts: vec![LocalGitContextSample {
                context: DesktopSuggestionLocalGitContextRef {
                    id: "local-git-current".into(),
                    branch: "main".into(),
                    status: "dirty: 1 changed files".into(),
                    changed_files: vec!["M src/lib.rs".into()],
                    recent_commits: vec!["abc add".into()],
                },
                fingerprint: "local-fp".into(),
            }],
            codex_threads: vec![SessionThreadSample {
                thread: DesktopSuggestionSessionThreadRef {
                    id: "thread-1".into(),
                    title: "Continue migration".into(),
                    updated_at: Some(20),
                    preview: Some("Finish Native suggestion parity".into()),
                },
                fingerprint: "thread-fp".into(),
            }],
            latest_updated_at: 20,
        }
    }

    #[test]
    fn materialize_keeps_valid_local_git_refs() {
        let scope = source_scope();
        let items = materialize_items(
            vec![RawSuggestion {
                task_ids: vec!["task-1".into()],
                github_activity_ids: Vec::new(),
                local_git_context_ids: vec!["local-git-current".into()],
                codex_thread_ids: Vec::new(),
                summary: Some("继续本地改动".into()),
                reason: Some("有未提交变更".into()),
                prompt: Some("请继续完成本地 Git 改动".into()),
            }],
            &scope,
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].source, DesktopSuggestionItemSource::Task);
        assert_eq!(items[0].local_git_contexts.len(), 1);
    }

    #[test]
    fn prompt_bounds_history_and_exposes_every_reference_id() {
        let mut scope = source_scope();
        scope.tasks[0].title = "题".repeat(200);
        scope.tasks[0].user_messages = vec!["用".repeat(1_000)];
        scope.tasks[0].unfinished_signals = vec![format!("todo: {}", "信".repeat(1_000))];

        let prompt = build_generation_prompt(&scope);

        assert!(!prompt.contains(&"题".repeat(120)));
        assert!(!prompt.contains(&"用".repeat(400)));
        assert!(prompt.contains('…'));
        assert!(prompt.contains("GitHub 活动 gh-1"));
        assert!(prompt.contains("本地 Git 上下文 local-git-current"));
        assert!(prompt.contains("Codex thread thread-1"));
        assert!(prompt.contains("githubActivityIds"));
        assert!(prompt.contains("localGitContextIds"));
        assert!(prompt.contains("codexThreadIds"));
    }

    #[test]
    fn materialize_filters_unknown_anchors_accepts_each_source_and_caps_results() {
        let scope = source_scope();
        let raw = vec![
            RawSuggestion {
                task_ids: vec!["missing".into()],
                github_activity_ids: Vec::new(),
                local_git_context_ids: Vec::new(),
                codex_thread_ids: Vec::new(),
                summary: Some("无效".into()),
                reason: Some("没有有效锚点".into()),
                prompt: Some("不应出现".into()),
            },
            RawSuggestion {
                task_ids: Vec::new(),
                github_activity_ids: vec!["gh-1".into()],
                local_git_context_ids: Vec::new(),
                codex_thread_ids: Vec::new(),
                summary: Some("跟进 PR".into()),
                reason: Some("有新的 PR 活动".into()),
                prompt: Some("请继续跟进 PR。".into()),
            },
            RawSuggestion {
                task_ids: Vec::new(),
                github_activity_ids: Vec::new(),
                local_git_context_ids: vec!["local-git-current".into()],
                codex_thread_ids: Vec::new(),
                summary: Some("继续本地改动".into()),
                reason: Some("工作树有未提交内容".into()),
                prompt: Some("请继续处理本地改动。".into()),
            },
            RawSuggestion {
                task_ids: Vec::new(),
                github_activity_ids: Vec::new(),
                local_git_context_ids: Vec::new(),
                codex_thread_ids: vec!["thread-1".into()],
                summary: Some("继续任务".into()),
                reason: Some("关联任务尚未结束".into()),
                prompt: Some("请继续关联任务。".into()),
            },
            RawSuggestion {
                task_ids: vec!["task-1".into()],
                github_activity_ids: Vec::new(),
                local_git_context_ids: Vec::new(),
                codex_thread_ids: Vec::new(),
                summary: Some("超过上限".into()),
                reason: Some("第四条有效建议".into()),
                prompt: Some("不应超过三条。".into()),
            },
        ];

        let items = materialize_items(raw, &scope);

        assert_eq!(items.len(), MAX_SUGGESTIONS);
        assert_eq!(items[0].source, DesktopSuggestionItemSource::Github);
        assert_eq!(items[1].source, DesktopSuggestionItemSource::LocalGit);
        assert_eq!(items[2].source, DesktopSuggestionItemSource::SessionThread);
    }

    #[test]
    fn invalid_model_payload_is_rejected() {
        assert!(parse_model_suggestions("not json".to_owned()).is_err());
    }

    #[test]
    fn generation_rules_are_non_empty() {
        assert!(!suggestion_generation_rules().is_empty());
        assert!(!suggestion_system_instruction().trim().is_empty());
    }
}

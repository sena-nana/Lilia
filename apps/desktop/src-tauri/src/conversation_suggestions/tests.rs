use super::cache::{build_cache_key, cache_entry_is_valid, SuggestionCacheEntry};
use super::generation::{
    build_generation_prompt, materialize_items, parse_model_suggestions, RawSuggestion,
};
use super::github_context::{normalize_github_events, parse_github_repo_url};
use super::scope::{build_scope_from_parts, load_task_samples, summarize_scope_sources};
use super::types::*;
use crate::agent_timeline;
use rusqlite::{params, Connection};
use serde_json::{json, Value as JsonValue};

fn create_schema(conn: &Connection) {
    conn.execute_batch(
            r#"
            CREATE TABLE tasks (
              id TEXT PRIMARY KEY,
              project_id TEXT,
              session_id TEXT NOT NULL,
              title TEXT NOT NULL,
              status TEXT NOT NULL,
              created_at INTEGER NOT NULL,
              parent_id TEXT,
              archived INTEGER NOT NULL DEFAULT 0,
              sort_order INTEGER NOT NULL DEFAULT 0,
              pinned INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE projects (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              cwd TEXT,
              created_at INTEGER NOT NULL,
              sort_order INTEGER NOT NULL DEFAULT 0,
              pinned INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE task_todos (
              id           TEXT PRIMARY KEY,
              task_id      TEXT NOT NULL,
              text         TEXT NOT NULL,
              done         INTEGER NOT NULL DEFAULT 0,
              "order"      INTEGER NOT NULL,
              source       TEXT NOT NULL CHECK (source IN ('lilia','agent')),
              priority     TEXT NOT NULL DEFAULT 'normal' CHECK (priority IN ('high','normal','low')),
              guide_status TEXT CHECK (guide_status IS NULL OR guide_status IN ('pending','queued','sent')),
              attachments_json TEXT NOT NULL DEFAULT '[]',
              created_at   INTEGER NOT NULL,
              updated_at   INTEGER NOT NULL
            );
            "#,
        )
        .unwrap();
    agent_timeline::create_timeline_schema(conn).unwrap();
}

fn conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    create_schema(&conn);
    conn
}

fn empty_project_context() -> ProjectContext {
    ProjectContext {
        name: None,
        cwd: None,
    }
}

fn sample_github_repo() -> GitHubRepoRef {
    GitHubRepoRef {
        owner: "sena-nana".to_string(),
        name: "LiliaCode".to_string(),
        full_name: "sena-nana/LiliaCode".to_string(),
    }
}

fn sample_github_activity(id: &str, title: &str) -> GitHubActivitySample {
    GitHubActivitySample {
        id: id.to_string(),
        repo_full_name: "sena-nana/LiliaCode".to_string(),
        kind: "pull_request".to_string(),
        action: "opened".to_string(),
        title: title.to_string(),
        url: Some(format!("https://github.com/sena-nana/LiliaCode/pull/{id}")),
        details: vec!["state: open".to_string()],
        fingerprint: format!("{id}|pull_request|opened|{title}|open"),
    }
}

fn sample_local_git_context(branch: &str) -> LocalGitContextSample {
    LocalGitContextSample {
        context: SuggestionLocalGitContextRef {
            id: "local-git-current".to_string(),
            branch: branch.to_string(),
            status: "dirty: 2 changed files".to_string(),
            changed_files: vec![
                "M apps/desktop/src-tauri/src/conversation_suggestions.rs".to_string(),
                "?? packages/contracts/src/suggestions.ts".to_string(),
            ],
            recent_commits: vec![
                "abc1234 add suggestions".to_string(),
                "def5678 wire composer".to_string(),
            ],
        },
        fingerprint: format!("{branch}|abc1234 add suggestions|M conversation_suggestions.rs"),
    }
}

fn sample_codex_thread(id: &str, title: &str) -> CodexThreadSample {
    CodexThreadSample {
        thread: SuggestionCodexThreadRef {
            id: id.to_string(),
            title: title.to_string(),
            updated_at: Some(2_000),
            preview: Some("用户要求继续补齐 Codex thread 建议来源。".to_string()),
        },
        fingerprint: format!("{id}@2000:{title}:preview"),
    }
}

fn insert_task(conn: &Connection, id: &str, project_id: &str, archived: bool) {
    conn.execute(
            "INSERT INTO tasks (id, project_id, session_id, title, status, created_at, archived) VALUES (?1, ?2, ?1, ?3, 'running', 1, ?4)",
            params![id, project_id, format!("任务 {id}"), if archived { 1 } else { 0 }],
        )
        .unwrap();
}

fn insert_todo(conn: &Connection, task_id: &str, text: &str, done: bool, order: i64) {
    conn.execute(
            r#"INSERT INTO task_todos
               (id, task_id, text, done, "order", source, priority, guide_status, attachments_json, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, 'agent', 'normal', NULL, '[]', ?6, ?6)"#,
            params![
                format!("{task_id}-todo-{order}"),
                task_id,
                text,
                if done { 1 } else { 0 },
                order,
                order + 100
            ],
        )
        .unwrap();
}

fn insert_todo_list_event(conn: &Connection, task_id: &str, updated_at: i64, items: JsonValue) {
    conn.execute(
            r#"INSERT INTO agent_timeline_events
               (id, task_id, turn_id, backend, kind, status, title, summary, payload, created_at, updated_at, turn_seq, intra_turn_order)
               VALUES (?1, ?2, 'turn', 'claude', 'todo_list', 'success', 'Todo', NULL, ?3, ?4, ?4, 0, 1)"#,
            params![
                format!("{task_id}-todo-list-{updated_at}"),
                task_id,
                json!({ "items": items }).to_string(),
                updated_at
            ],
        )
        .unwrap();
}

fn insert_event(conn: &Connection, task_id: &str, updated_at: i64, content: &str) {
    conn.execute(
            r#"INSERT INTO agent_timeline_events
               (id, task_id, turn_id, backend, kind, status, title, summary, payload, created_at, updated_at, turn_seq, intra_turn_order)
               VALUES (?1, ?2, 'turn', 'claude', 'message', 'success', '用户输入', ?3, ?4, ?5, ?5, 0, 0)"#,
            params![
                format!("{task_id}-{updated_at}"),
                task_id,
                content,
                json!({ "role": "user", "content": content }).to_string(),
                updated_at
            ],
        )
        .unwrap();
}

#[test]
fn task_sampling_uses_recent_unarchived_project_tasks() {
    let conn = conn();
    insert_task(&conn, "old", "p1", false);
    insert_task(&conn, "new", "p1", false);
    insert_task(&conn, "archived", "p1", true);
    insert_event(&conn, "old", 10, "旧对话");
    insert_event(&conn, "new", 30, "新对话");
    insert_event(&conn, "archived", 50, "归档对话");

    let samples = load_task_samples(&conn, Some("p1"), 3).unwrap();

    assert_eq!(
        samples.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["new", "old"]
    );
}

#[test]
fn prompt_builder_truncates_long_history() {
    let scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: None,
        latest_updated_at: 1,
        github_repo: None,
        github_activities: Vec::new(),
        local_git_contexts: Vec::new(),
        codex_threads: Vec::new(),
        tasks: vec![TaskSample {
            id: "t1".to_string(),
            title: "x".repeat(200),
            status: "running".to_string(),
            project_id: Some("p1".to_string()),
            user_messages: vec!["a".repeat(1000)],
            assistant_message: None,
            unfinished_signals: vec!["todo: ".to_string() + &"b".repeat(1000)],
            latest_updated_at: 1,
        }],
    };

    let prompt = build_generation_prompt(&scope);

    assert!(!prompt.contains(&"x".repeat(120)));
    assert!(!prompt.contains(&"a".repeat(400)));
    assert!(!prompt.contains(&"b".repeat(400)));
    assert!(prompt.contains('…'));
}

#[test]
fn scope_ignores_recent_tasks_without_unfinished_signals() {
    let conn = conn();
    insert_task(&conn, "done", "p1", false);
    insert_event(&conn, "done", 20, "最近对话");
    insert_todo(&conn, "done", "已完成事项", true, 0);

    let scope = build_scope_from_parts(
        &conn,
        Some("p1"),
        empty_project_context(),
        None,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();

    assert!(scope.is_none());
}

#[test]
fn scope_uses_unfinished_task_todos_in_prompt() {
    let conn = conn();
    insert_task(&conn, "todo-task", "p1", false);
    insert_event(&conn, "todo-task", 20, "继续做权限检查");
    insert_todo(&conn, "todo-task", "补齐权限失败回退", false, 0);

    let scope = build_scope_from_parts(
        &conn,
        Some("p1"),
        empty_project_context(),
        None,
        Vec::new(),
        Vec::new(),
    )
    .unwrap()
    .unwrap();
    let prompt = build_generation_prompt(&scope);

    assert_eq!(scope.tasks.len(), 1);
    assert!(prompt.contains("未完成信号: todo: 补齐权限失败回退"));
    assert!(prompt.contains("可返回 []"));
    assert!(prompt.contains("不要提出泛化建议"));
}

#[test]
fn todo_list_payload_samples_only_unfinished_items() {
    let conn = conn();
    insert_task(&conn, "timeline-todo", "p1", false);
    insert_event(&conn, "timeline-todo", 20, "处理同步");
    insert_todo_list_event(
        &conn,
        "timeline-todo",
        30,
        json!([
            { "content": "已经完成", "status": "completed" },
            { "content": "继续同步 pending 状态", "status": "pending" },
            { "text": "布尔完成项", "done": true }
        ]),
    );

    let scope = build_scope_from_parts(
        &conn,
        Some("p1"),
        empty_project_context(),
        None,
        Vec::new(),
        Vec::new(),
    )
    .unwrap()
    .unwrap();
    let signals = &scope.tasks[0].unfinished_signals;

    assert_eq!(signals, &vec!["todo: 继续同步 pending 状态".to_string()]);
}

#[test]
fn github_repo_url_parser_supports_common_remote_forms() {
    let cases = [
        "https://github.com/sena-nana/LiliaCode.git",
        "git@github.com:sena-nana/LiliaCode.git",
        "ssh://git@github.com/sena-nana/LiliaCode.git",
    ];
    for input in cases {
        let repo = parse_github_repo_url(input).unwrap();
        assert_eq!(repo.full_name, "sena-nana/LiliaCode");
    }

    assert!(parse_github_repo_url("https://example.com/sena-nana/LiliaCode.git").is_none());
}

#[test]
fn github_events_normalize_pr_issue_and_push_only() {
    let repo = sample_github_repo();
    let events = vec![
        json!({
            "id": "1",
            "type": "PullRequestEvent",
            "payload": {
                "action": "opened",
                "pull_request": {
                    "number": 12,
                    "title": "Add GitHub suggestions",
                    "state": "open",
                    "html_url": "https://github.com/sena-nana/LiliaCode/pull/12"
                }
            }
        }),
        json!({
            "id": "2",
            "type": "IssuesEvent",
            "payload": {
                "action": "closed",
                "issue": {
                    "number": 7,
                    "title": "Suggestion prompt is vague",
                    "state": "closed",
                    "html_url": "https://github.com/sena-nana/LiliaCode/issues/7"
                }
            }
        }),
        json!({
            "id": "3",
            "type": "PushEvent",
            "payload": {
                "ref": "refs/heads/main",
                "head": "abc123",
                "commits": [
                    { "message": "Wire github activities" },
                    { "message": "Add tests" }
                ]
            }
        }),
        json!({
            "id": "4",
            "type": "WatchEvent",
            "payload": { "action": "started" }
        }),
    ];

    let activities = normalize_github_events(&repo, &events);

    assert_eq!(activities.len(), 3);
    assert_eq!(activities[0].kind, "pull_request");
    assert!(activities[0].title.contains("PR #12"));
    assert_eq!(activities[1].kind, "issue");
    assert!(activities[2].title.contains("main"));
    assert!(activities[2].details[0].contains("Wire github activities"));
}

#[test]
fn scope_can_use_github_activity_without_unfinished_tasks() {
    let conn = conn();
    insert_task(&conn, "done", "p1", false);
    insert_event(&conn, "done", 20, "最近对话");
    insert_todo(&conn, "done", "已完成事项", true, 0);

    let scope = build_scope_from_parts(
        &conn,
        Some("p1"),
        empty_project_context(),
        Some((
            sample_github_repo(),
            vec![sample_github_activity("gh-pr-1", "PR #1: 接入 GitHub 活动")],
        )),
        vec![sample_local_git_context("feature/local-git")],
        Vec::new(),
    )
    .unwrap()
    .unwrap();

    assert!(scope.tasks.is_empty());
    assert_eq!(scope.github_activities.len(), 1);
    assert!(scope.local_git_contexts.is_empty());
}

#[test]
fn source_probe_reports_task_and_github_sources() {
    let conn = conn();
    insert_task(&conn, "task-1", "p1", false);
    insert_event(&conn, "task-1", 20, "最近对话");
    insert_todo(&conn, "task-1", "继续处理建议展示", false, 0);

    let scope = build_scope_from_parts(
        &conn,
        Some("p1"),
        empty_project_context(),
        Some((
            sample_github_repo(),
            vec![sample_github_activity("gh-pr-1", "PR #1: 接入 GitHub 活动")],
        )),
        vec![sample_local_git_context("feature/local-git")],
        Vec::new(),
    )
    .unwrap()
    .unwrap();

    let probe = summarize_scope_sources(&scope);

    assert_eq!(
        probe.sources,
        vec![SuggestionItemSource::Task, SuggestionItemSource::Github]
    );
    assert!(probe.local_git.is_none());
}

#[test]
fn source_probe_reports_local_git_details() {
    let scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: Some("Lilia".to_string()),
        latest_updated_at: 1,
        github_repo: None,
        github_activities: Vec::new(),
        local_git_contexts: vec![sample_local_git_context("feature/local-git")],
        codex_threads: Vec::new(),
        tasks: Vec::new(),
    };

    let probe = summarize_scope_sources(&scope);
    let local_git = probe.local_git.unwrap();

    assert_eq!(probe.sources, vec![SuggestionItemSource::LocalGit]);
    assert!(local_git.has_recent_commits);
    assert!(local_git.has_changed_files);
}

#[test]
fn source_probe_distinguishes_clean_local_git_context() {
    let mut context = sample_local_git_context("feature/local-git");
    context.context.changed_files.clear();
    let scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: Some("Lilia".to_string()),
        latest_updated_at: 1,
        github_repo: None,
        github_activities: Vec::new(),
        local_git_contexts: vec![context],
        codex_threads: Vec::new(),
        tasks: Vec::new(),
    };

    let local_git = summarize_scope_sources(&scope).local_git.unwrap();

    assert!(local_git.has_recent_commits);
    assert!(!local_git.has_changed_files);
}

#[test]
fn source_probe_reports_codex_thread_source() {
    let scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: Some("Lilia".to_string()),
        latest_updated_at: 2_000,
        github_repo: None,
        github_activities: Vec::new(),
        local_git_contexts: Vec::new(),
        codex_threads: vec![sample_codex_thread("thread-1", "补齐 Codex 建议")],
        tasks: Vec::new(),
    };

    let probe = summarize_scope_sources(&scope);

    assert_eq!(probe.sources, vec![SuggestionItemSource::SessionThread]);
    assert!(probe.local_git.is_none());
}

#[test]
fn prompt_includes_github_activity_ids() {
    let scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: Some("Lilia".to_string()),
        latest_updated_at: 1,
        github_repo: Some(sample_github_repo()),
        github_activities: vec![sample_github_activity("gh-pr-1", "PR #1: 接入 GitHub 活动")],
        local_git_contexts: Vec::new(),
        codex_threads: Vec::new(),
        tasks: Vec::new(),
    };

    let prompt = build_generation_prompt(&scope);

    assert!(prompt.contains("githubActivityIds"));
    assert!(prompt.contains("GitHub 活动 gh-pr-1"));
    assert!(prompt.contains("GitHub 仓库: sena-nana/LiliaCode"));
}

#[test]
fn scope_can_use_local_git_context_without_unfinished_tasks() {
    let conn = conn();
    insert_task(&conn, "done", "p1", false);
    insert_event(&conn, "done", 20, "最近对话");
    insert_todo(&conn, "done", "已完成事项", true, 0);

    let scope = build_scope_from_parts(
        &conn,
        Some("p1"),
        empty_project_context(),
        None,
        vec![sample_local_git_context("feature/local-git")],
        Vec::new(),
    )
    .unwrap()
    .unwrap();

    assert!(scope.tasks.is_empty());
    assert!(scope.github_activities.is_empty());
    assert_eq!(scope.local_git_contexts.len(), 1);
}

#[test]
fn prompt_includes_local_git_context_ids() {
    let scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: Some("Lilia".to_string()),
        latest_updated_at: 1,
        github_repo: None,
        github_activities: Vec::new(),
        local_git_contexts: vec![sample_local_git_context("feature/local-git")],
        codex_threads: Vec::new(),
        tasks: Vec::new(),
    };

    let prompt = build_generation_prompt(&scope);

    assert!(prompt.contains("localGitContextIds"));
    assert!(prompt.contains("本地 Git 上下文 local-git-current"));
    assert!(prompt.contains("branch: feature/local-git"));
    assert!(prompt.contains("最近提交: abc1234 add suggestions"));
    assert!(prompt.contains("变更文件: M apps/desktop/src-tauri/src/conversation_suggestions.rs"));
}

#[test]
fn prompt_includes_codex_thread_ids() {
    let scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: Some("Lilia".to_string()),
        latest_updated_at: 2_000,
        github_repo: None,
        github_activities: Vec::new(),
        local_git_contexts: Vec::new(),
        codex_threads: vec![sample_codex_thread("thread-1", "补齐 Codex 建议")],
        tasks: Vec::new(),
    };

    let prompt = build_generation_prompt(&scope);

    assert!(prompt.contains("codexThreadIds"));
    assert!(prompt.contains("Codex thread thread-1"));
    assert!(prompt.contains("标题: 补齐 Codex 建议"));
    assert!(prompt.contains("thread 预览: 用户要求继续补齐 Codex thread 建议来源。"));
}

#[test]
fn materialize_allows_github_only_suggestions() {
    let scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: None,
        latest_updated_at: 1,
        github_repo: Some(sample_github_repo()),
        github_activities: vec![sample_github_activity("gh-pr-1", "PR #1: 接入 GitHub 活动")],
        local_git_contexts: Vec::new(),
        codex_threads: Vec::new(),
        tasks: Vec::new(),
    };
    let raw = vec![
        RawSuggestion {
            task_ids: Vec::new(),
            github_activity_ids: vec!["missing".to_string()],
            local_git_context_ids: Vec::new(),
            codex_thread_ids: Vec::new(),
            summary: Some("无效活动".to_string()),
            reason: Some("引用不存在的 GitHub 活动".to_string()),
            prompt: Some("请处理不存在的活动。".to_string()),
        },
        RawSuggestion {
            task_ids: Vec::new(),
            github_activity_ids: vec!["gh-pr-1".to_string()],
            local_git_context_ids: Vec::new(),
            codex_thread_ids: Vec::new(),
            summary: Some("跟进 PR 活动".to_string()),
            reason: Some("近期 PR 打开后需要继续确认实现范围。".to_string()),
            prompt: Some(
                "请基于 sena-nana/LiliaCode 的 PR #1 继续梳理接入 GitHub 活动的新对话建议。"
                    .to_string(),
            ),
        },
    ];

    let items = materialize_items(raw, &scope);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source, SuggestionItemSource::Github);
    assert!(items[0].task_ids.is_empty());
    assert_eq!(items[0].github_activities[0].id, "gh-pr-1");
}

#[test]
fn materialize_allows_local_git_only_suggestions() {
    let scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: None,
        latest_updated_at: 1,
        github_repo: None,
        github_activities: Vec::new(),
        local_git_contexts: vec![sample_local_git_context("feature/local-git")],
        codex_threads: Vec::new(),
        tasks: Vec::new(),
    };
    let raw = vec![
        RawSuggestion {
            task_ids: Vec::new(),
            github_activity_ids: Vec::new(),
            local_git_context_ids: vec!["missing".to_string()],
            codex_thread_ids: Vec::new(),
            summary: Some("无效本地上下文".to_string()),
            reason: Some("引用不存在的本地 Git 上下文。".to_string()),
            prompt: Some("请处理不存在的上下文。".to_string()),
        },
        RawSuggestion {
            task_ids: Vec::new(),
            github_activity_ids: Vec::new(),
            local_git_context_ids: vec!["local-git-current".to_string()],
            codex_thread_ids: Vec::new(),
            summary: Some("跟进本地变更".to_string()),
            reason: Some("当前分支有未提交变更需要继续处理。".to_string()),
            prompt: Some(
                "请基于 feature/local-git 分支最近提交和未提交变更继续处理本地 Git 上下文。"
                    .to_string(),
            ),
        },
    ];

    let items = materialize_items(raw, &scope);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source, SuggestionItemSource::LocalGit);
    assert!(items[0].task_ids.is_empty());
    assert!(items[0].github_activities.is_empty());
    assert_eq!(items[0].local_git_contexts[0].id, "local-git-current");
}

#[test]
fn materialize_allows_codex_thread_only_suggestions() {
    let scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: None,
        latest_updated_at: 2_000,
        github_repo: None,
        github_activities: Vec::new(),
        local_git_contexts: Vec::new(),
        codex_threads: vec![sample_codex_thread("thread-1", "补齐 Codex 建议")],
        tasks: Vec::new(),
    };
    let raw = vec![
        RawSuggestion {
            task_ids: Vec::new(),
            github_activity_ids: Vec::new(),
            local_git_context_ids: Vec::new(),
            codex_thread_ids: vec!["missing".to_string()],
            summary: Some("无效 thread".to_string()),
            reason: Some("引用不存在的 Codex thread。".to_string()),
            prompt: Some("请处理不存在的 thread。".to_string()),
        },
        RawSuggestion {
            task_ids: Vec::new(),
            github_activity_ids: Vec::new(),
            local_git_context_ids: Vec::new(),
            codex_thread_ids: vec!["thread-1".to_string()],
            summary: Some("继续 Codex thread".to_string()),
            reason: Some("最近 Codex thread 的预览显示建议来源还没补完。".to_string()),
            prompt: Some("请基于“补齐 Codex 建议”这个 Codex thread 继续补齐建议来源。".to_string()),
        },
    ];

    let items = materialize_items(raw, &scope);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source, SuggestionItemSource::SessionThread);
    assert!(items[0].task_ids.is_empty());
    assert!(items[0].github_activities.is_empty());
    assert!(items[0].local_git_contexts.is_empty());
    assert_eq!(items[0].codex_threads[0].id, "thread-1");
}

#[test]
fn materialize_supports_zero_to_three_items_and_requires_task_anchor() {
    let scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: None,
        latest_updated_at: 1,
        github_repo: None,
        github_activities: Vec::new(),
        local_git_contexts: Vec::new(),
        codex_threads: Vec::new(),
        tasks: vec![TaskSample {
            id: "t1".to_string(),
            title: "任务 t1".to_string(),
            status: "running".to_string(),
            project_id: Some("p1".to_string()),
            user_messages: Vec::new(),
            assistant_message: None,
            unfinished_signals: vec!["todo: 补齐测试".to_string()],
            latest_updated_at: 1,
        }],
    };
    assert!(materialize_items(Vec::new(), &scope).is_empty());

    let raw = vec![
        RawSuggestion {
            task_ids: Vec::new(),
            github_activity_ids: Vec::new(),
            local_git_context_ids: Vec::new(),
            codex_thread_ids: Vec::new(),
            summary: Some("泛化建议".to_string()),
            reason: Some("没有任务锚点".to_string()),
            prompt: Some("请随便优化一下。".to_string()),
        },
        RawSuggestion {
            task_ids: vec!["missing".to_string()],
            github_activity_ids: Vec::new(),
            local_git_context_ids: Vec::new(),
            codex_thread_ids: Vec::new(),
            summary: Some("错误锚点".to_string()),
            reason: Some("引用不存在任务".to_string()),
            prompt: Some("请处理不存在任务。".to_string()),
        },
        RawSuggestion {
            task_ids: vec!["t1".to_string()],
            github_activity_ids: Vec::new(),
            local_git_context_ids: Vec::new(),
            codex_thread_ids: Vec::new(),
            summary: Some("继续一".to_string()),
            reason: Some("锚定未完成信号一".to_string()),
            prompt: Some("请继续处理一。".to_string()),
        },
        RawSuggestion {
            task_ids: vec!["t1".to_string()],
            github_activity_ids: Vec::new(),
            local_git_context_ids: Vec::new(),
            codex_thread_ids: Vec::new(),
            summary: Some("继续二".to_string()),
            reason: Some("锚定未完成信号二".to_string()),
            prompt: Some("请继续处理二。".to_string()),
        },
        RawSuggestion {
            task_ids: vec!["t1".to_string()],
            github_activity_ids: Vec::new(),
            local_git_context_ids: Vec::new(),
            codex_thread_ids: Vec::new(),
            summary: Some("继续三".to_string()),
            reason: Some("锚定未完成信号三".to_string()),
            prompt: Some("请继续处理三。".to_string()),
        },
        RawSuggestion {
            task_ids: vec!["t1".to_string()],
            github_activity_ids: Vec::new(),
            local_git_context_ids: Vec::new(),
            codex_thread_ids: Vec::new(),
            summary: Some("继续四".to_string()),
            reason: Some("超过数量上限".to_string()),
            prompt: Some("请继续处理四。".to_string()),
        },
    ];

    let items = materialize_items(raw, &scope);

    assert_eq!(items.len(), 3);
    assert!(items
        .iter()
        .all(|item| item.task_ids == vec!["t1".to_string()]));
    assert_eq!(items[0].summary, "继续一");
}

#[test]
fn cache_key_changes_when_unfinished_signals_change() {
    let model = ModelRequest {
        source: SuggestionSource::AssistantAi,
        backend: None,
        model: "mini".to_string(),
        base_url: "http://localhost".to_string(),
        api_key: "key".to_string(),
    };
    let mut scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: None,
        latest_updated_at: 20,
        github_repo: None,
        github_activities: Vec::new(),
        local_git_contexts: Vec::new(),
        codex_threads: Vec::new(),
        tasks: vec![TaskSample {
            id: "t1".to_string(),
            title: "任务 t1".to_string(),
            status: "running".to_string(),
            project_id: Some("p1".to_string()),
            user_messages: Vec::new(),
            assistant_message: None,
            unfinished_signals: vec!["todo: 第一项".to_string()],
            latest_updated_at: 20,
        }],
    };
    let first = build_cache_key(&scope, &model);
    scope.tasks[0].unfinished_signals = vec!["todo: 第二项".to_string()];
    let second = build_cache_key(&scope, &model);

    assert_ne!(first, second);
}

#[test]
fn cache_key_changes_when_github_activity_changes() {
    let model = ModelRequest {
        source: SuggestionSource::AssistantAi,
        backend: None,
        model: "mini".to_string(),
        base_url: "http://localhost".to_string(),
        api_key: "key".to_string(),
    };
    let mut scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: None,
        latest_updated_at: 0,
        github_repo: Some(sample_github_repo()),
        github_activities: vec![sample_github_activity("gh-pr-1", "PR #1: 第一版")],
        local_git_contexts: Vec::new(),
        codex_threads: Vec::new(),
        tasks: Vec::new(),
    };
    let first = build_cache_key(&scope, &model);
    scope.github_activities[0] = sample_github_activity("gh-pr-1", "PR #1: 第二版");
    let second = build_cache_key(&scope, &model);

    assert_ne!(first, second);
}

#[test]
fn cache_key_changes_when_local_git_context_changes() {
    let model = ModelRequest {
        source: SuggestionSource::AssistantAi,
        backend: None,
        model: "mini".to_string(),
        base_url: "http://localhost".to_string(),
        api_key: "key".to_string(),
    };
    let mut scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: None,
        latest_updated_at: 0,
        github_repo: None,
        github_activities: Vec::new(),
        local_git_contexts: vec![sample_local_git_context("feature/local-git")],
        codex_threads: Vec::new(),
        tasks: Vec::new(),
    };
    let first = build_cache_key(&scope, &model);
    scope.local_git_contexts[0] = sample_local_git_context("feature/next");
    let second = build_cache_key(&scope, &model);

    assert_ne!(first, second);
}

#[test]
fn cache_key_changes_when_codex_thread_changes() {
    let model = ModelRequest {
        source: SuggestionSource::AssistantAi,
        backend: None,
        model: "mini".to_string(),
        base_url: "http://localhost".to_string(),
        api_key: "key".to_string(),
    };
    let mut scope = SuggestionScope {
        project_id: Some("p1".to_string()),
        project_name: None,
        latest_updated_at: 2_000,
        github_repo: None,
        github_activities: Vec::new(),
        local_git_contexts: Vec::new(),
        codex_threads: vec![sample_codex_thread("thread-1", "第一版")],
        tasks: Vec::new(),
    };
    let first = build_cache_key(&scope, &model);
    scope.codex_threads[0] = sample_codex_thread("thread-1", "第二版");
    let second = build_cache_key(&scope, &model);

    assert_ne!(first, second);
}

#[test]
fn invalid_model_json_returns_error() {
    assert!(parse_model_suggestions("not json".to_string()).is_err());
}

#[test]
fn cache_hit_requires_same_key_and_fresh_entry() {
    let item = SuggestionItem {
        id: "sg-1".to_string(),
        project_id: Some("p1".to_string()),
        task_ids: vec!["t1".to_string()],
        source: SuggestionItemSource::Task,
        github_activities: Vec::new(),
        local_git_contexts: Vec::new(),
        codex_threads: Vec::new(),
        summary: "继续测试".to_string(),
        reason: "缓存命中需要稳定判断".to_string(),
        prompt: "请验证建议缓存命中。".to_string(),
        generated_at: 100,
    };
    let entry = SuggestionCacheEntry {
        cache_key: "p1|assistant-ai|assistant-ai|mini|20".to_string(),
        generated_at: 100,
        items: vec![item],
    };

    assert!(cache_entry_is_valid(
        &entry,
        "p1|assistant-ai|assistant-ai|mini|20",
        100 + CACHE_TTL_MS,
    ));
    assert!(!cache_entry_is_valid(
        &entry,
        "p1|assistant-ai|assistant-ai|mini|30",
        100 + CACHE_TTL_MS,
    ));
    assert!(!cache_entry_is_valid(
        &entry,
        "p1|assistant-ai|assistant-ai|mini|20",
        101 + CACHE_TTL_MS,
    ));
}

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;
use tauri::AppHandle;

use super::generation::{compact_line, truncate_chars};
use super::github_context::load_github_activity_context;
use super::local_git::load_local_git_context;
use super::types::{
    CodexThreadSample, GitHubActivitySample, GitHubRepoRef, LocalGitContextSample, ProjectContext,
    SuggestionItemSource, SuggestionLocalGitProbe, SuggestionScope, SuggestionSourceProbe,
    TaskSample, MAX_TASKS_PER_SCOPE, SAMPLE_TEXT_LIMIT, TASK_CANDIDATE_LIMIT,
    UNFINISHED_SIGNAL_LIMIT,
};

pub(super) fn build_scope(
    app: &AppHandle,
    conn: &Connection,
    requested_project_id: Option<&str>,
) -> Result<Option<SuggestionScope>, String> {
    let project = load_project_context(conn, requested_project_id)?;
    let github_context = project
        .cwd
        .as_deref()
        .and_then(|cwd| match load_github_activity_context(app, cwd) {
            Ok(context) => context,
            Err(err) => {
                eprintln!("[conversation-suggestions] github context skipped: {err}");
                None
            }
        });
    let local_git_contexts = if github_context.is_none() {
        project
            .cwd
            .as_deref()
            .and_then(|cwd| match load_local_git_context(cwd) {
                Ok(context) => context,
                Err(err) => {
                    eprintln!("[conversation-suggestions] local git context skipped: {err}");
                    None
                }
            })
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };
    // Official Codex thread context removed; keep empty for shared suggestion shapes.
    let codex_threads = Vec::new();
    build_scope_from_parts(
        conn,
        requested_project_id,
        project,
        github_context,
        local_git_contexts,
        codex_threads,
    )
}

pub(super) fn build_scope_from_parts(
    conn: &Connection,
    requested_project_id: Option<&str>,
    project: ProjectContext,
    github_context: Option<(GitHubRepoRef, Vec<GitHubActivitySample>)>,
    local_git_contexts: Vec<LocalGitContextSample>,
    codex_threads: Vec<CodexThreadSample>,
) -> Result<Option<SuggestionScope>, String> {
    let tasks = if let Some(project_id) = requested_project_id {
        load_task_samples(conn, Some(project_id), TASK_CANDIDATE_LIMIT)?
    } else {
        load_recent_task_samples(conn, TASK_CANDIDATE_LIMIT)?
    };
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

pub(super) fn summarize_scope_sources(scope: &SuggestionScope) -> SuggestionSourceProbe {
    let mut sources = Vec::new();
    if !scope.tasks.is_empty() {
        sources.push(SuggestionItemSource::Task);
    }
    if !scope.github_activities.is_empty() {
        sources.push(SuggestionItemSource::Github);
    }
    if !scope.codex_threads.is_empty() {
        sources.push(SuggestionItemSource::SessionThread);
    }
    let local_git = if scope.local_git_contexts.is_empty() {
        None
    } else {
        sources.push(SuggestionItemSource::LocalGit);
        Some(SuggestionLocalGitProbe {
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
    SuggestionSourceProbe { sources, local_git }
}

fn load_project_context(
    conn: &Connection,
    requested_project_id: Option<&str>,
) -> Result<ProjectContext, String> {
    let Some(project_id) = requested_project_id else {
        return Ok(ProjectContext {
            name: None,
            cwd: None,
        });
    };
    let row = conn
        .query_row(
            "SELECT name, cwd FROM projects WHERE id = ?1",
            params![project_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(|e| format!("conversation_suggestions: query project 失败：{e}"))?;
    Ok(ProjectContext {
        name: row.as_ref().map(|(name, _)| name.clone()),
        cwd: row.and_then(|(_, cwd)| cwd),
    })
}
pub(super) fn load_task_samples(
    conn: &Connection,
    project_id: Option<&str>,
    limit: usize,
) -> Result<Vec<TaskSample>, String> {
    let mut out = Vec::new();
    if let Some(project_id) = project_id {
        let mut stmt = conn
            .prepare(
                r#"SELECT t.id, t.title, t.status, t.project_id, MAX(e.updated_at) AS latest
                   FROM tasks t
                   INNER JOIN agent_timeline_events e ON e.task_id = t.id
                   WHERE t.project_id = ?1 AND t.archived = 0
                   GROUP BY t.id
                   ORDER BY latest DESC
                   LIMIT ?2"#,
            )
            .map_err(|e| format!("conversation_suggestions: prepare tasks 失败：{e}"))?;
        let rows = stmt
            .query_map(params![project_id, limit as i64], read_task_sample_row)
            .map_err(|e| format!("conversation_suggestions: query tasks 失败：{e}"))?;
        for row in rows {
            let (id, title, status, project_id, latest) =
                row.map_err(|e| format!("conversation_suggestions: task row 失败：{e}"))?;
            out.push(load_sample_details(
                conn, id, title, status, project_id, latest,
            )?);
        }
    } else {
        let mut stmt = conn
            .prepare(
                r#"SELECT t.id, t.title, t.status, t.project_id, MAX(e.updated_at) AS latest
                   FROM tasks t
                   INNER JOIN agent_timeline_events e ON e.task_id = t.id
                   WHERE t.project_id IS NULL AND t.archived = 0
                   GROUP BY t.id
                   ORDER BY latest DESC
                   LIMIT ?1"#,
            )
            .map_err(|e| format!("conversation_suggestions: prepare tasks 失败：{e}"))?;
        let rows = stmt
            .query_map(params![limit as i64], read_task_sample_row)
            .map_err(|e| format!("conversation_suggestions: query tasks 失败：{e}"))?;
        for row in rows {
            let (id, title, status, project_id, latest) =
                row.map_err(|e| format!("conversation_suggestions: task row 失败：{e}"))?;
            out.push(load_sample_details(
                conn, id, title, status, project_id, latest,
            )?);
        }
    }
    Ok(out)
}

fn read_task_sample_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(String, String, String, Option<String>, i64)> {
    Ok((
        row.get::<_, String>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, Option<String>>(3)?,
        row.get::<_, i64>(4)?,
    ))
}

fn load_recent_task_samples(conn: &Connection, limit: usize) -> Result<Vec<TaskSample>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT t.id, t.title, t.status, t.project_id, MAX(e.updated_at) AS latest
               FROM tasks t
               INNER JOIN agent_timeline_events e ON e.task_id = t.id
               WHERE t.archived = 0
               GROUP BY t.id
               ORDER BY latest DESC
               LIMIT ?1"#,
        )
        .map_err(|e| format!("conversation_suggestions: prepare recent tasks 失败：{e}"))?;
    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| format!("conversation_suggestions: query recent tasks 失败：{e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let (id, title, status, project_id, latest) =
            row.map_err(|e| format!("conversation_suggestions: recent task row 失败：{e}"))?;
        out.push(load_sample_details(
            conn, id, title, status, project_id, latest,
        )?);
    }
    Ok(out)
}

fn load_sample_details(
    conn: &Connection,
    id: String,
    title: String,
    status: String,
    project_id: Option<String>,
    latest_updated_at: i64,
) -> Result<TaskSample, String> {
    Ok(TaskSample {
        user_messages: load_event_texts(conn, &id, "message", Some("user"), None, 2)?,
        assistant_message: load_event_texts(
            conn,
            &id,
            "message",
            Some("assistant"),
            Some("success"),
            1,
        )?
        .into_iter()
        .next(),
        unfinished_signals: load_unfinished_signal_texts(conn, &id, UNFINISHED_SIGNAL_LIMIT)?,
        id,
        title,
        status,
        project_id,
        latest_updated_at,
    })
}

fn load_event_texts(
    conn: &Connection,
    task_id: &str,
    kind: &str,
    role: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT summary, payload, status
               FROM agent_timeline_events
               WHERE task_id = ?1 AND kind = ?2
               ORDER BY updated_at DESC
               LIMIT 20"#,
        )
        .map_err(|e| format!("conversation_suggestions: prepare events 失败：{e}"))?;
    let rows = stmt
        .query_map(params![task_id, kind], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("conversation_suggestions: query events 失败：{e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let (summary, payload_text, event_status) =
            row.map_err(|e| format!("conversation_suggestions: event row 失败：{e}"))?;
        if status.is_some() && Some(event_status.as_str()) != status {
            continue;
        }
        let payload = serde_json::from_str::<JsonValue>(&payload_text).unwrap_or(JsonValue::Null);
        if role.is_some() && payload.get("role").and_then(|v| v.as_str()) != role {
            continue;
        }
        let text = payload
            .get("content")
            .and_then(|v| v.as_str())
            .or(summary.as_deref())
            .map(compact_line)
            .unwrap_or_default();
        if !text.is_empty() {
            out.push(truncate_chars(&text, SAMPLE_TEXT_LIMIT));
        }
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

fn load_unfinished_signal_texts(
    conn: &Connection,
    task_id: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for text in load_unfinished_task_todos(conn, task_id, limit)? {
        push_unfinished_signal(&mut out, &mut seen, "todo", &text, limit);
        if out.len() >= limit {
            return Ok(out);
        }
    }
    load_unfinished_timeline_signals(conn, task_id, limit, &mut out, &mut seen)?;
    Ok(out)
}

fn load_unfinished_task_todos(
    conn: &Connection,
    task_id: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT text
               FROM task_todos
               WHERE task_id = ?1 AND done = 0
               ORDER BY "order" ASC, updated_at DESC
               LIMIT ?2"#,
        )
        .map_err(|e| format!("conversation_suggestions: prepare todos 失败：{e}"))?;
    let rows = stmt
        .query_map(params![task_id, limit as i64], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| format!("conversation_suggestions: query todos 失败：{e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let text = row.map_err(|e| format!("conversation_suggestions: todo row 失败：{e}"))?;
        out.push(text);
    }
    Ok(out)
}

fn load_unfinished_timeline_signals(
    conn: &Connection,
    task_id: &str,
    limit: usize,
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT kind, title, summary, payload
               FROM agent_timeline_events
               WHERE task_id = ?1
                 AND kind IN ('todo_list', 'error')
               ORDER BY updated_at DESC
               LIMIT 20"#,
        )
        .map_err(|e| format!("conversation_suggestions: prepare signals 失败：{e}"))?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| format!("conversation_suggestions: query signals 失败：{e}"))?;
    for row in rows {
        let (kind, title, summary, payload_text) =
            row.map_err(|e| format!("conversation_suggestions: signal row 失败：{e}"))?;
        let payload = serde_json::from_str::<JsonValue>(&payload_text).unwrap_or(JsonValue::Null);
        if kind == "todo_list" {
            for text in unfinished_todo_payload_items(&payload) {
                push_unfinished_signal(out, seen, "todo", &text, limit);
                if out.len() >= limit {
                    return Ok(());
                }
            }
        } else if kind == "error" {
            push_unfinished_signal(
                out,
                seen,
                "error",
                summary.as_deref().unwrap_or(title.as_str()),
                limit,
            );
        }
        if out.len() >= limit {
            break;
        }
    }
    Ok(())
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

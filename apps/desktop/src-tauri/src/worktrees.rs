use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::process_command::hide_console_window;
use crate::projects_tasks::events::emit_tasks_changed;
use crate::store::LiliaStore;
use crate::util::now_millis;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskWorktree {
    pub(crate) task_id: String,
    pub(crate) project_id: Option<String>,
    pub(crate) base_repo_path: String,
    pub(crate) worktree_path: String,
    pub(crate) branch_name: String,
    pub(crate) base_branch: String,
    pub(crate) status: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorktreeListItem {
    pub(crate) path: String,
    pub(crate) head: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) bare: bool,
    pub(crate) detached: bool,
    pub(crate) prunable: bool,
    pub(crate) locked: bool,
    pub(crate) is_main: bool,
    pub(crate) is_task_bound: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorktreeCreateInput {
    pub(crate) task_id: String,
    pub(crate) project_id: Option<String>,
    pub(crate) base_repo_path: String,
    pub(crate) parent_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorktreeAttachInput {
    pub(crate) task_id: String,
    pub(crate) project_id: Option<String>,
    pub(crate) base_repo_path: String,
    pub(crate) worktree_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorktreeMergeResult {
    pub(crate) merged: bool,
    pub(crate) removed: bool,
    pub(crate) archived: bool,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitWorktree {
    pub(crate) path: String,
    pub(crate) head: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) bare: bool,
    pub(crate) detached: bool,
    pub(crate) prunable: bool,
    pub(crate) locked: bool,
}

fn run_git(args: &[&str], cwd: &Path) -> Result<String, String> {
    let mut command = Command::new("git");
    hide_console_window(&mut command);
    let output = command
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("无法启动 git（请确认 git 在 PATH 中）：{e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Err(if detail.is_empty() {
            format!(
                "git {} 失败：exit {}",
                args.join(" "),
                output.status.code().unwrap_or(-1)
            )
        } else {
            format!("git {} 失败：{detail}", args.join(" "))
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn normalize_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn canonical_path_string(path: &Path) -> Result<String, String> {
    path.canonicalize()
        .map(|p| normalize_path_string(&p))
        .map_err(|e| format!("解析路径失败：{}：{e}", path.display()))
}

fn trim_ref_prefix(branch: &str) -> String {
    branch
        .strip_prefix("refs/heads/")
        .unwrap_or(branch)
        .to_string()
}

fn ensure_git_repo(path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        return Err(format!("目录不存在：{}", path.display()));
    }
    run_git(&["rev-parse", "--show-toplevel"], path).map(|_| ())
}

fn current_branch(path: &Path) -> Result<String, String> {
    let branch = run_git(&["branch", "--show-current"], path)?;
    let trimmed = branch.trim();
    if trimmed.is_empty() {
        return Err("当前仓库处于 detached HEAD，无法作为 worktree 基准".to_string());
    }
    Ok(trimmed.to_string())
}

fn parse_worktree_porcelain(input: &str) -> Vec<GitWorktree> {
    let mut out = Vec::new();
    let mut current: Option<GitWorktree> = None;
    for line in input.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(item) = current.take() {
                out.push(item);
            }
            current = Some(GitWorktree {
                path: path.to_string(),
                head: None,
                branch: None,
                bare: false,
                detached: false,
                prunable: false,
                locked: false,
            });
            continue;
        }
        let Some(item) = current.as_mut() else {
            continue;
        };
        if let Some(head) = line.strip_prefix("HEAD ") {
            item.head = Some(head.to_string());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            item.branch = Some(trim_ref_prefix(branch));
        } else if line == "bare" {
            item.bare = true;
        } else if line == "detached" {
            item.detached = true;
        } else if line.starts_with("prunable") {
            item.prunable = true;
        } else if line.starts_with("locked") {
            item.locked = true;
        }
    }
    if let Some(item) = current.take() {
        out.push(item);
    }
    out
}

pub(crate) fn list_git_worktrees(base_repo_path: &Path) -> Result<Vec<GitWorktree>, String> {
    ensure_git_repo(base_repo_path)?;
    let output = run_git(&["worktree", "list", "--porcelain"], base_repo_path)?;
    Ok(parse_worktree_porcelain(&output))
}

#[cfg(feature = "runtime-domain-reference")]
pub(crate) fn runtime_reference_inspect_worktrees(
    base_repo_path: &Path,
    iterations: usize,
) -> Result<serde_json::Value, String> {
    if iterations == 0 {
        return Err("reference workspace iterations 必须大于零".to_string());
    }
    let mut worktrees = Vec::new();
    for _ in 0..iterations {
        worktrees = list_git_worktrees(base_repo_path)?;
    }
    Ok(serde_json::json!({
        "iterations": iterations,
        "worktrees": worktrees.into_iter().map(|worktree| serde_json::json!({
            "path": worktree.path,
            "head": worktree.head,
            "branch": worktree.branch,
            "bare": worktree.bare,
            "detached": worktree.detached,
            "prunable": worktree.prunable,
            "locked": worktree.locked,
        })).collect::<Vec<_>>(),
    }))
}

fn active_bound_paths(conn: &rusqlite::Connection) -> Result<HashSet<String>, String> {
    let mut stmt = conn
        .prepare("SELECT worktree_path FROM task_worktrees WHERE status = 'active'")
        .map_err(|e| format!("worktree: prepare bound paths failed: {e}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("worktree: query bound paths failed: {e}"))?;
    let mut out = HashSet::new();
    for row in rows {
        out.insert(row.map_err(|e| format!("worktree: bound path row failed: {e}"))?);
    }
    Ok(out)
}

fn load_task_worktree(
    conn: &rusqlite::Connection,
    task_id: &str,
) -> Result<Option<TaskWorktree>, String> {
    conn.query_row(
        r#"SELECT task_id, project_id, base_repo_path, worktree_path, branch_name,
                  base_branch, status, created_at, updated_at
           FROM task_worktrees
           WHERE task_id = ?1 AND status = 'active'"#,
        params![task_id],
        |row| {
            Ok(TaskWorktree {
                task_id: row.get(0)?,
                project_id: row.get(1)?,
                base_repo_path: row.get(2)?,
                worktree_path: row.get(3)?,
                branch_name: row.get(4)?,
                base_branch: row.get(5)?,
                status: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("worktree: 查询任务工作树失败：{e}"))
}

fn upsert_task_worktree(
    conn: &rusqlite::Connection,
    task_id: &str,
    project_id: Option<&str>,
    base_repo_path: &str,
    worktree_path: &str,
    branch_name: &str,
    base_branch: &str,
) -> Result<TaskWorktree, String> {
    let now = now_millis();
    conn.execute(
        r#"INSERT INTO task_worktrees
           (task_id, project_id, base_repo_path, worktree_path, branch_name, base_branch, status, created_at, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?7)
           ON CONFLICT(task_id) DO UPDATE SET
             project_id = excluded.project_id,
             base_repo_path = excluded.base_repo_path,
             worktree_path = excluded.worktree_path,
             branch_name = excluded.branch_name,
             base_branch = excluded.base_branch,
             status = 'active',
             updated_at = excluded.updated_at"#,
        params![
            task_id,
            project_id,
            base_repo_path,
            worktree_path,
            branch_name,
            base_branch,
            now
        ],
    )
    .map_err(|e| format!("worktree: 保存任务工作树失败：{e}"))?;
    Ok(TaskWorktree {
        task_id: task_id.to_string(),
        project_id: project_id.map(ToString::to_string),
        base_repo_path: base_repo_path.to_string(),
        worktree_path: worktree_path.to_string(),
        branch_name: branch_name.to_string(),
        base_branch: base_branch.to_string(),
        status: "active".to_string(),
        created_at: now,
        updated_at: now,
    })
}

fn task_title_slug(conn: &rusqlite::Connection, task_id: &str) -> String {
    let title = conn
        .query_row(
            "SELECT title FROM tasks WHERE id = ?1",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| task_id.to_string());
    let slug = title
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        task_id.chars().take(8).collect()
    } else {
        slug
    }
}

fn unique_worktree_target(parent: &Path, slug: &str) -> PathBuf {
    let base = format!("lilia-{slug}");
    let candidate = parent.join(&base);
    if !candidate.exists() {
        return candidate;
    }
    for i in 2..1024 {
        let next = parent.join(format!("{base}-{i}"));
        if !next.exists() {
            return next;
        }
    }
    parent.join(format!("{base}-{}", Uuid::new_v4()))
}

fn unique_branch_name(base_repo_path: &Path, slug: &str) -> Result<String, String> {
    let short = Uuid::new_v4().to_string();
    let short = &short[..8];
    let candidate = format!("lilia/{slug}-{short}");
    let mut command = Command::new("git");
    hide_console_window(&mut command);
    let exists = command
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{candidate}"),
        ])
        .current_dir(base_repo_path)
        .status()
        .map_err(|e| format!("无法检查 git branch：{e}"))?
        .success();
    if exists {
        Ok(format!("lilia/{slug}-{}", Uuid::new_v4()))
    } else {
        Ok(candidate)
    }
}

fn ensure_clean(path: &Path, label: &str) -> Result<(), String> {
    let status = run_git(&["status", "--porcelain"], path)?;
    if status.trim().is_empty() {
        Ok(())
    } else {
        Err(format!("{label}存在未提交改动，请先提交或清理后再继续"))
    }
}

fn branch_unique_commit_count(worktree_path: &Path, base_branch: &str) -> Result<i64, String> {
    let count = run_git(
        &["rev-list", "--count", &format!("{base_branch}..HEAD")],
        worktree_path,
    )?;
    Ok(count.trim().parse::<i64>().unwrap_or(0))
}

fn ensure_branch_has_unique_commits(worktree_path: &Path, base_branch: &str) -> Result<(), String> {
    let parsed = branch_unique_commit_count(worktree_path, base_branch)?;
    if parsed <= 0 {
        Err("worktree 分支没有可合并的提交".to_string())
    } else {
        Ok(())
    }
}

fn task_archive_core(
    conn: &rusqlite::Connection,
    task_id: &str,
) -> Result<(bool, Option<String>), String> {
    let project_id = conn
        .query_row(
            "SELECT project_id FROM tasks WHERE id = ?1 AND archived = 0",
            params![task_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|e| format!("worktree archive: 查询任务失败：{e}"))?
        .flatten();
    let changed = conn
        .execute(
            "UPDATE tasks SET archived = 1 WHERE id = ?1 AND archived = 0",
            params![task_id],
        )
        .map_err(|e| format!("worktree archive: 归档任务失败：{e}"))?;
    Ok((changed > 0, project_id))
}

fn finish_worktree_archive(
    conn: &rusqlite::Connection,
    app: &AppHandle,
    task_id: &str,
    status: &str,
    merged: bool,
    message: &str,
) -> Result<WorktreeMergeResult, String> {
    let now = now_millis();
    conn.execute(
        "UPDATE task_worktrees SET status = ?1, updated_at = ?2 WHERE task_id = ?3",
        params![status, now, task_id],
    )
    .map_err(|e| format!("worktree: 更新状态失败：{e}"))?;
    let (archived, project_id) = task_archive_core(conn, task_id)?;
    if archived {
        emit_tasks_changed(app, project_id);
    }
    Ok(WorktreeMergeResult {
        merged,
        removed: true,
        archived,
        message: message.to_string(),
    })
}

#[tauri::command]
pub(crate) fn worktree_list(
    base_repo_path: String,
    store: State<'_, LiliaStore>,
) -> Result<Vec<WorktreeListItem>, String> {
    let base = PathBuf::from(base_repo_path.trim());
    let base_canon = canonical_path_string(&base)?;
    let conn = store.conn()?;
    let bound_paths = active_bound_paths(&conn)?;
    // Production path: workspace RuntimeDomain owns git worktree list authority.
    let worktrees = crate::production_workspace_domain::list_worktrees_via_domain(&base)?;
    Ok(worktrees
        .into_iter()
        .map(|item| {
            let item_path =
                canonical_path_string(Path::new(&item.path)).unwrap_or(item.path.clone());
            WorktreeListItem {
                is_main: item_path == base_canon,
                is_task_bound: bound_paths.contains(&item_path),
                path: item_path,
                head: item.head,
                branch: item.branch,
                bare: item.bare,
                detached: item.detached,
                prunable: item.prunable,
                locked: item.locked,
            }
        })
        .collect())
}

#[tauri::command]
pub(crate) fn worktree_get_for_task(
    task_id: String,
    store: State<'_, LiliaStore>,
) -> Result<Option<TaskWorktree>, String> {
    let conn = store.conn()?;
    load_task_worktree(&conn, &task_id)
}

#[tauri::command]
pub(crate) fn worktree_clear_task(
    task_id: String,
    store: State<'_, LiliaStore>,
) -> Result<(), String> {
    let conn = store.conn()?;
    let now = now_millis();
    conn.execute(
        "UPDATE task_worktrees SET status = 'removed', updated_at = ?1 WHERE task_id = ?2",
        params![now, task_id],
    )
    .map_err(|e| format!("worktree: 清除任务工作树绑定失败：{e}"))?;
    Ok(())
}

#[tauri::command]
pub(crate) fn worktree_attach_task(
    input: WorktreeAttachInput,
    store: State<'_, LiliaStore>,
) -> Result<TaskWorktree, String> {
    let base = PathBuf::from(input.base_repo_path.trim());
    let worktree = PathBuf::from(input.worktree_path.trim());
    ensure_git_repo(&base)?;
    ensure_git_repo(&worktree)?;
    let base_canon = canonical_path_string(&base)?;
    let worktree_canon = canonical_path_string(&worktree)?;
    if base_canon == worktree_canon {
        return Err("不能把主仓库作为已有 worktree 绑定".to_string());
    }
    let branch = current_branch(&worktree)?;
    let base_branch = current_branch(&base)?;
    let conn = store.conn()?;
    upsert_task_worktree(
        &conn,
        &input.task_id,
        input.project_id.as_deref(),
        &base_canon,
        &worktree_canon,
        &branch,
        &base_branch,
    )
}

#[tauri::command]
pub(crate) fn worktree_create_for_task(
    input: WorktreeCreateInput,
    store: State<'_, LiliaStore>,
) -> Result<TaskWorktree, String> {
    let base = PathBuf::from(input.base_repo_path.trim());
    ensure_git_repo(&base)?;
    let base_canon = canonical_path_string(&base)?;
    let base_branch = current_branch(&base)?;
    let conn = store.conn()?;
    let slug = task_title_slug(&conn, &input.task_id);
    let parent = input
        .parent_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            base.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        });
    if !parent.is_dir() {
        return Err(format!("worktree 父目录不存在：{}", parent.display()));
    }
    let target = unique_worktree_target(&parent, &slug);
    let branch = unique_branch_name(&base, &slug)?;
    let target_text = target.to_string_lossy().to_string();
    run_git(
        &["worktree", "add", "-b", &branch, &target_text, &base_branch],
        &base,
    )?;
    let worktree_canon = canonical_path_string(&target)?;
    upsert_task_worktree(
        &conn,
        &input.task_id,
        input.project_id.as_deref(),
        &base_canon,
        &worktree_canon,
        &branch,
        &base_branch,
    )
}

#[tauri::command]
pub(crate) fn worktree_cleanup_archive(
    task_id: String,
    store: State<'_, LiliaStore>,
    app: AppHandle,
) -> Result<WorktreeMergeResult, String> {
    let conn = store.conn()?;
    let Some(worktree) = load_task_worktree(&conn, &task_id)? else {
        return Err("当前对话没有绑定 worktree".to_string());
    };
    let base = PathBuf::from(&worktree.base_repo_path);
    let worktree_path = PathBuf::from(&worktree.worktree_path);
    ensure_git_repo(&base)?;
    ensure_git_repo(&worktree_path)?;
    ensure_clean(&base, "主仓库")?;
    ensure_clean(&worktree_path, "worktree")?;
    let unique_commits = branch_unique_commit_count(&worktree_path, &worktree.base_branch)?;
    if unique_commits > 0 {
        return Err(
            "worktree 分支存在未合并提交，请使用“合并并删除”或关闭自动清理后再归档".to_string(),
        );
    }
    run_git(&["worktree", "remove", &worktree.worktree_path], &base)?;
    run_git(&["branch", "-d", &worktree.branch_name], &base)?;
    finish_worktree_archive(
        &conn,
        &app,
        &task_id,
        "removed",
        false,
        "已删除无新增提交的 worktree 并归档对话",
    )
}

#[tauri::command]
pub(crate) fn worktree_merge_delete_archive(
    task_id: String,
    store: State<'_, LiliaStore>,
    app: AppHandle,
) -> Result<WorktreeMergeResult, String> {
    let conn = store.conn()?;
    let Some(worktree) = load_task_worktree(&conn, &task_id)? else {
        return Err("当前对话没有绑定 worktree".to_string());
    };
    let base = PathBuf::from(&worktree.base_repo_path);
    let worktree_path = PathBuf::from(&worktree.worktree_path);
    ensure_git_repo(&base)?;
    ensure_git_repo(&worktree_path)?;
    ensure_clean(&base, "主仓库")?;
    ensure_clean(&worktree_path, "worktree")?;
    ensure_branch_has_unique_commits(&worktree_path, &worktree.base_branch)?;
    run_git(&["checkout", &worktree.base_branch], &base)?;
    run_git(&["merge", "--no-ff", &worktree.branch_name], &base)?;
    run_git(&["worktree", "remove", &worktree.worktree_path], &base)?;
    run_git(&["branch", "-d", &worktree.branch_name], &base)?;
    finish_worktree_archive(
        &conn,
        &app,
        &task_id,
        "merged",
        true,
        "已合并 worktree 分支、删除 worktree 并归档对话",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_worktree_porcelain() {
        let items = parse_worktree_porcelain(
            "worktree D:/repo\nHEAD abc\nbranch refs/heads/main\n\nworktree D:/repo-wt\nHEAD def\nbranch refs/heads/lilia/task\nlocked\n",
        );
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].branch.as_deref(), Some("main"));
        assert_eq!(items[1].path, "D:/repo-wt");
        assert_eq!(items[1].branch.as_deref(), Some("lilia/task"));
        assert!(items[1].locked);
    }
}

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection, OptionalExtension};
use tauri::State;
use uuid::Uuid;

use crate::store::LiliaStore;
use crate::task_contract;
use crate::util::now_millis;

use super::queries::row_to_project;
use super::types::{ProjectDashboardSummaryRow, ProjectRow, ProjectTaskStatusCountsRow};

fn project_task_status_count(counts: &ProjectTaskStatusCountsRow, status: &str) -> i64 {
    match status {
        "draft" => counts.draft,
        "waiting" => counts.waiting,
        "running" => counts.running,
        "blocked" => counts.blocked,
        "done" => counts.done,
        "cancelled" => counts.cancelled,
        _ => 0,
    }
}

fn sum_project_task_statuses(counts: &ProjectTaskStatusCountsRow, statuses: &[String]) -> i64 {
    statuses
        .iter()
        .map(|status| project_task_status_count(counts, status))
        .sum()
}

fn row_to_project_dashboard_summary(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ProjectDashboardSummaryRow> {
    let pinned: i64 = row.get(3)?;
    let waiting: i64 = row.get(6)?;
    let running: i64 = row.get(7)?;
    let blocked: i64 = row.get(8)?;
    let status_counts = ProjectTaskStatusCountsRow {
        waiting,
        running,
        blocked,
        draft: row.get(9)?,
        done: row.get(10)?,
        cancelled: row.get(11)?,
    };
    let blocked_count = sum_project_task_statuses(
        &status_counts,
        task_contract::project_dashboard_blocked_statuses(),
    );
    let active_count = sum_project_task_statuses(
        &status_counts,
        task_contract::project_dashboard_active_statuses(),
    );
    Ok(ProjectDashboardSummaryRow {
        id: row.get(0)?,
        name: row.get(1)?,
        cwd: row.get(2)?,
        pinned: pinned != 0,
        task_count: row.get(4)?,
        session_count: row.get(5)?,
        status_counts,
        blocked_count,
        active_count,
        recent_activity_at: row.get(12)?,
        total_tokens: row.get(13)?,
        known_cost_usd: row.get(14)?,
        cost_record_count: row.get(15)?,
        usage_record_count: row.get(16)?,
    })
}

fn normalize_windows_extended_path(value: &str) -> String {
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    value.strip_prefix(r"\\?\").unwrap_or(value).to_string()
}

pub(crate) fn display_project_path(path: &Path) -> String {
    normalize_windows_extended_path(&path.to_string_lossy())
}

fn cwd_key(value: &str) -> String {
    let cleaned = normalize_windows_extended_path(value.trim())
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\");
    if cfg!(windows) {
        cleaned.to_ascii_lowercase()
    } else {
        cleaned
    }
}

fn project_name_from_cwd(cwd: &str) -> String {
    let cleaned = cwd.trim().trim_end_matches(['\\', '/']);
    let name = cleaned.rsplit(['\\', '/']).next().unwrap_or("").trim();
    if name.is_empty() {
        "未命名项目".to_string()
    } else {
        name.to_string()
    }
}

fn find_project_row_by_cwd(
    conn: &Connection,
    target_key: &str,
    context: &str,
) -> Result<Option<ProjectRow>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT p.id, p.name, p.cwd,
                      (SELECT COUNT(*) FROM tasks t WHERE t.project_id = p.id AND t.archived = 0)
                      AS session_count,
                      p.sort_order,
                      p.pinned
               FROM projects p
               WHERE p.cwd IS NOT NULL"#,
        )
        .map_err(|err| format!("{context}: prepare 失败：{err}"))?;
    let rows = stmt
        .query_map([], row_to_project)
        .map_err(|err| format!("{context}: query 失败：{err}"))?;

    for row in rows {
        let project = row.map_err(|err| format!("{context}: row 失败：{err}"))?;
        if project
            .cwd
            .as_deref()
            .is_some_and(|cwd| cwd_key(cwd) == target_key)
        {
            return Ok(Some(project));
        }
    }
    Ok(None)
}

pub(crate) fn ensure_project_row_for_cwd(
    conn: &Connection,
    cwd: &str,
    context: &str,
) -> Result<ProjectRow, String> {
    let target_key = cwd_key(cwd);
    if let Some(project) = find_project_row_by_cwd(conn, &target_key, context)? {
        return Ok(project);
    }

    let id = Uuid::new_v4().to_string();
    let now = now_millis();
    let name = project_name_from_cwd(cwd);
    let sort_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) FROM projects",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|err| format!("{context}: max sort_order 失败：{err}"))?
        + 1;
    conn.execute(
        "INSERT INTO projects (id, name, cwd, created_at, sort_order) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, name, cwd, now, sort_order],
    )
    .map_err(|err| format!("{context}: 创建项目失败：{err}"))?;

    Ok(ProjectRow {
        id,
        name,
        cwd: Some(cwd.to_string()),
        session_count: 0,
        sort_order,
        pinned: false,
    })
}

fn resolve_folder_project_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    if !path.exists() || !path.is_dir() {
        return None;
    }
    fs::canonicalize(&path)
        .ok()
        .map(|path| display_project_path(&path))
}

fn list_project_dashboard_summaries(
    conn: &Connection,
) -> Result<Vec<ProjectDashboardSummaryRow>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            WITH activity_points AS (
              SELECT task_id, updated_at AS activity_at FROM agent_usage_records
              UNION ALL
              SELECT task_id, updated_at AS activity_at FROM agent_timeline_events
              UNION ALL
              SELECT task_id, updated_at AS activity_at FROM task_agent_sessions
              UNION ALL
              SELECT task_id, updated_at AS activity_at FROM task_runtime_states
              UNION ALL
              SELECT id AS task_id, created_at AS activity_at FROM tasks
            ),
            task_activity AS (
              SELECT t.id AS task_id, MAX(a.activity_at) AS recent_activity_at
              FROM tasks t
              LEFT JOIN activity_points a ON a.task_id = t.id
              WHERE t.archived = 0
              GROUP BY t.id
            ),
            project_usage AS (
              SELECT t.project_id,
                     COALESCE(SUM(u.total_tokens), 0) AS total_tokens,
                     SUM(CASE WHEN u.known_cost_usd IS NOT NULL THEN u.known_cost_usd ELSE 0 END)
                       AS known_cost_usd,
                     SUM(CASE WHEN u.known_cost_usd IS NOT NULL THEN 1 ELSE 0 END)
                       AS cost_record_count,
                     COUNT(u.event_id) AS usage_record_count
              FROM tasks t
              LEFT JOIN agent_usage_records u ON u.task_id = t.id
              WHERE t.archived = 0
              GROUP BY t.project_id
            )
            SELECT p.id,
                   p.name,
                   p.cwd,
                   p.pinned,
                   COUNT(t.id) AS task_count,
                   COUNT(DISTINCT t.session_id) AS session_count,
                   SUM(CASE WHEN t.status = 'waiting' THEN 1 ELSE 0 END) AS waiting_count,
                   SUM(CASE WHEN t.status = 'running' THEN 1 ELSE 0 END) AS running_count,
                   SUM(CASE WHEN t.status = 'blocked' THEN 1 ELSE 0 END) AS blocked_count,
                   SUM(CASE WHEN t.status = 'draft' THEN 1 ELSE 0 END) AS draft_count,
                   SUM(CASE WHEN t.status = 'done' THEN 1 ELSE 0 END) AS done_count,
                   SUM(CASE WHEN t.status = 'cancelled' THEN 1 ELSE 0 END) AS cancelled_count,
                   MAX(ta.recent_activity_at) AS recent_activity_at,
                   COALESCE(u.total_tokens, 0) AS total_tokens,
                   CASE
                     WHEN COALESCE(u.cost_record_count, 0) > 0 THEN u.known_cost_usd
                     ELSE NULL
                   END AS known_cost_usd,
                   COALESCE(u.cost_record_count, 0) AS cost_record_count,
                   COALESCE(u.usage_record_count, 0) AS usage_record_count
            FROM projects p
            LEFT JOIN tasks t
              ON t.project_id = p.id AND t.archived = 0
            LEFT JOIN task_activity ta
              ON ta.task_id = t.id
            LEFT JOIN project_usage u
              ON u.project_id = p.id
            GROUP BY p.id
            ORDER BY p.pinned DESC, p.sort_order ASC
            "#,
        )
        .map_err(|e| format!("project_dashboard_list: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map([], row_to_project_dashboard_summary)
        .map_err(|e| format!("project_dashboard_list: query 失败：{e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("project_dashboard_list: row 失败：{e}"))?);
    }
    Ok(out)
}

#[tauri::command]
pub fn project_list(store: State<'_, LiliaStore>) -> Result<Vec<ProjectRow>, String> {
    let conn = store.conn()?;
    let mut stmt = conn
        .prepare(
            r#"SELECT p.id, p.name, p.cwd,
                      COUNT(t.id) AS session_count,
                      p.sort_order,
                      p.pinned
               FROM projects p
               LEFT JOIN tasks t
                 ON t.project_id = p.id AND t.archived = 0
               GROUP BY p.id
               ORDER BY p.pinned DESC, p.sort_order ASC"#,
        )
        .map_err(|e| format!("project_list: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map([], row_to_project)
        .map_err(|e| format!("project_list: query 失败：{e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("project_list: row 失败：{e}"))?);
    }
    Ok(out)
}

#[tauri::command]
pub fn project_dashboard_list(
    store: State<'_, LiliaStore>,
) -> Result<Vec<ProjectDashboardSummaryRow>, String> {
    let conn = store.conn()?;
    list_project_dashboard_summaries(&conn)
}

#[tauri::command]
pub fn project_get(id: String, store: State<'_, LiliaStore>) -> Result<Option<ProjectRow>, String> {
    let conn = store.conn()?;
    let result = conn
        .query_row(
            r#"SELECT p.id, p.name, p.cwd,
                      (SELECT COUNT(*) FROM tasks t WHERE t.project_id = p.id AND t.archived = 0)
                      AS session_count,
                      p.sort_order,
                      p.pinned
               FROM projects p WHERE p.id = ?1"#,
            params![id],
            row_to_project,
        )
        .optional()
        .map_err(|e| format!("project_get: {e}"))?;
    Ok(result)
}

#[tauri::command]
pub fn project_create(
    name: String,
    cwd: Option<String>,
    store: State<'_, LiliaStore>,
) -> Result<ProjectRow, String> {
    let conn = store.conn()?;
    let id = Uuid::new_v4().to_string();
    let now = now_millis();
    let trimmed = name.trim();
    let final_name = if trimmed.is_empty() {
        "未命名项目"
    } else {
        trimmed
    };
    let max_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) FROM projects",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("project_create: max sort_order 失败：{e}"))?;
    let sort_order = max_order + 1;
    conn.execute(
        "INSERT INTO projects (id, name, cwd, created_at, sort_order) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, final_name, cwd, now, sort_order],
    )
    .map_err(|e| format!("project_create: insert 失败：{e}"))?;
    Ok(ProjectRow {
        id,
        name: final_name.to_string(),
        cwd,
        session_count: 0,
        sort_order,
        pinned: false,
    })
}

#[tauri::command]
pub fn project_ensure_folders(
    paths: Vec<String>,
    store: State<'_, LiliaStore>,
) -> Result<Vec<ProjectRow>, String> {
    let conn = store.conn()?;
    let mut seen = HashSet::new();
    let mut projects = Vec::new();

    for path in paths {
        let Some(cwd) = resolve_folder_project_path(&path) else {
            continue;
        };
        if !seen.insert(cwd_key(&cwd)) {
            continue;
        }
        projects.push(ensure_project_row_for_cwd(
            &conn,
            &cwd,
            "project_ensure_folders",
        )?);
    }

    Ok(projects)
}

#[tauri::command]
pub fn project_rename(
    id: String,
    next_name: String,
    store: State<'_, LiliaStore>,
) -> Result<bool, String> {
    let trimmed = next_name.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }
    let conn = store.conn()?;
    let changed = conn
        .execute(
            "UPDATE projects SET name = ?1 WHERE id = ?2 AND name != ?1",
            params![trimmed, id],
        )
        .map_err(|e| format!("project_rename: {e}"))?;
    Ok(changed > 0)
}

#[tauri::command]
pub fn project_remove(id: String, store: State<'_, LiliaStore>) -> Result<bool, String> {
    let conn = store.conn()?;
    conn.execute(
        "UPDATE tasks SET project_id = NULL WHERE project_id = ?1",
        params![id],
    )
    .map_err(|e| format!("project_remove: orphan tasks 失败：{e}"))?;
    let deleted = conn
        .execute("DELETE FROM projects WHERE id = ?1", params![id])
        .map_err(|e| format!("project_remove: {e}"))?;
    Ok(deleted > 0)
}

#[tauri::command]
pub fn project_toggle_pin(id: String, store: State<'_, LiliaStore>) -> Result<bool, String> {
    let conn = store.conn()?;
    let current: i64 = conn
        .query_row(
            "SELECT pinned FROM projects WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|e| format!("project_toggle_pin: 查询失败：{e}"))?;
    let new_val = if current == 0 { 1i64 } else { 0i64 };
    conn.execute(
        "UPDATE projects SET pinned = ?1 WHERE id = ?2",
        params![new_val, id],
    )
    .map_err(|e| format!("project_toggle_pin: 更新失败：{e}"))?;
    Ok(new_val != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_dashboard_schema(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE projects (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              cwd TEXT,
              created_at INTEGER NOT NULL,
              sort_order INTEGER NOT NULL DEFAULT 0,
              pinned INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE tasks (
              id TEXT PRIMARY KEY,
              project_id TEXT,
              session_id TEXT NOT NULL,
              title TEXT NOT NULL,
              status TEXT NOT NULL,
              created_at INTEGER NOT NULL,
              archived INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE agent_usage_records (
              event_id TEXT PRIMARY KEY,
              task_id TEXT NOT NULL,
              total_tokens INTEGER NOT NULL DEFAULT 0,
              known_cost_usd REAL,
              updated_at INTEGER NOT NULL
            );
            CREATE TABLE agent_timeline_events (
              id TEXT PRIMARY KEY,
              task_id TEXT NOT NULL,
              updated_at INTEGER NOT NULL
            );
            CREATE TABLE task_agent_sessions (
              task_id TEXT NOT NULL,
              backend TEXT NOT NULL,
              session_id TEXT NOT NULL,
              updated_at INTEGER NOT NULL,
              PRIMARY KEY (task_id, backend)
            );
            CREATE TABLE task_runtime_states (
              task_id TEXT PRIMARY KEY,
              updated_at INTEGER NOT NULL
            );
            "#,
        )
        .unwrap();
    }

    #[test]
    fn project_dashboard_aggregates_status_usage_and_recent_activity() {
        let conn = Connection::open_in_memory().unwrap();
        setup_dashboard_schema(&conn);
        conn.execute(
            "INSERT INTO projects (id, name, cwd, created_at, sort_order, pinned) VALUES ('p1', 'Lilia', 'C:\\work\\Lilia', 1, 0, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, cwd, created_at, sort_order, pinned) VALUES ('p2', 'Empty', NULL, 1, 1, 0)",
            [],
        )
        .unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO tasks (id, project_id, session_id, title, status, created_at, archived)
              VALUES ('t1', 'p1', 's1', 'Waiting', 'waiting', 1000, 0);
            INSERT INTO tasks (id, project_id, session_id, title, status, created_at, archived)
              VALUES ('t2', 'p1', 's1', 'Running', 'running', 2000, 0);
            INSERT INTO tasks (id, project_id, session_id, title, status, created_at, archived)
              VALUES ('t3', 'p1', 's2', 'Blocked', 'blocked', 3000, 0);
            INSERT INTO tasks (id, project_id, session_id, title, status, created_at, archived)
              VALUES ('t4', 'p1', 's3', 'Done', 'done', 4000, 0);
            INSERT INTO tasks (id, project_id, session_id, title, status, created_at, archived)
              VALUES ('t-archived', 'p1', 's4', 'Archived', 'cancelled', 5000, 1);
            INSERT INTO agent_usage_records (event_id, task_id, total_tokens, known_cost_usd, updated_at)
              VALUES ('u1', 't1', 100, 0.25, 7000);
            INSERT INTO agent_usage_records (event_id, task_id, total_tokens, known_cost_usd, updated_at)
              VALUES ('u2', 't2', 50, NULL, 6000);
            INSERT INTO agent_timeline_events (id, task_id, updated_at)
              VALUES ('e1', 't2', 8000);
            INSERT INTO task_agent_sessions (task_id, backend, session_id, updated_at)
              VALUES ('t4', 'codex', 'agent-session', 9000);
            INSERT INTO task_runtime_states (task_id, updated_at)
              VALUES ('t3', 9900);
            "#,
        )
        .unwrap();

        let summaries = list_project_dashboard_summaries(&conn).unwrap();
        let lilia = summaries.iter().find(|summary| summary.id == "p1").unwrap();
        assert_eq!(lilia.task_count, 4);
        assert_eq!(lilia.session_count, 3);
        assert_eq!(lilia.status_counts.waiting, 1);
        assert_eq!(lilia.status_counts.running, 1);
        assert_eq!(lilia.status_counts.blocked, 1);
        assert_eq!(lilia.status_counts.done, 1);
        assert_eq!(lilia.status_counts.cancelled, 0);
        assert_eq!(lilia.active_count, 2);
        assert_eq!(lilia.blocked_count, 1);
        assert_eq!(lilia.total_tokens, 150);
        assert_eq!(lilia.known_cost_usd, Some(0.25));
        assert_eq!(lilia.cost_record_count, 1);
        assert_eq!(lilia.usage_record_count, 2);
        assert_eq!(lilia.recent_activity_at, Some(9900));

        let empty = summaries.iter().find(|summary| summary.id == "p2").unwrap();
        assert_eq!(empty.task_count, 0);
        assert_eq!(empty.session_count, 0);
        assert_eq!(empty.total_tokens, 0);
        assert_eq!(empty.known_cost_usd, None);
        assert_eq!(empty.recent_activity_at, None);
    }
}

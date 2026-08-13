use lilia_contracts::{ProductTask, TaskId};
use lilia_desktop_application::{DesktopApplication, DesktopSessionSearchResult};
use rusqlite::{params, OptionalExtension};
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::automation::{
    task_created_event_kind, task_status_changed_event_kind, task_updated_event_kind,
};
use crate::store::LiliaStore;
use crate::task_contract;
use crate::util::now_millis;

use super::events::emit_tasks_changed;
use super::ordering::next_task_sort_order;
use super::queries::{build_task, insert_task_with_deps, load_project_deps, load_task_deps};
use super::relations::{normalize_dependency_ids, validate_parent};
use super::types::{NewTask, SidebarConversationSummaryRow, TaskRow};

#[tauri::command]
pub fn task_list(
    project_id: Option<String>,
    store: State<'_, LiliaStore>,
) -> Result<Vec<TaskRow>, String> {
    let conn = store.conn()?;
    let deps = load_project_deps(&conn, project_id.as_deref())?;
    let mut out = Vec::new();
    match &project_id {
        Some(pid) => {
            let mut stmt = conn
                .prepare(
                    r#"SELECT id, project_id, session_id, title, title_source, status, created_at, parent_id, sort_order, pinned
                       FROM tasks
                       WHERE project_id = ?1 AND archived = 0
                       ORDER BY pinned DESC, sort_order ASC"#,
                )
                .map_err(|e| format!("task_list: prepare 失败：{e}"))?;
            let rows = stmt
                .query_map(params![pid], |row| build_task(row, &deps))
                .map_err(|e| format!("task_list: query 失败：{e}"))?;
            for r in rows {
                out.push(r.map_err(|e| format!("task_list: row 失败：{e}"))?);
            }
        }
        None => {
            let mut stmt = conn
                .prepare(
                    r#"SELECT id, project_id, session_id, title, title_source, status, created_at, parent_id, sort_order, pinned
                       FROM tasks
                       WHERE project_id IS NULL AND archived = 0
                       ORDER BY pinned DESC, sort_order ASC"#,
                )
                .map_err(|e| format!("task_list: prepare 失败：{e}"))?;
            let rows = stmt
                .query_map([], |row| build_task(row, &deps))
                .map_err(|e| format!("task_list: query 失败：{e}"))?;
            for r in rows {
                out.push(r.map_err(|e| format!("task_list: row 失败：{e}"))?);
            }
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn search_sessions(
    application: State<'_, DesktopApplication>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<DesktopSessionSearchResult>, String> {
    application
        .search_sessions(&query, limit.unwrap_or(100))
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn task_list_sidebar_conversations(
    store: State<'_, LiliaStore>,
) -> Result<Vec<SidebarConversationSummaryRow>, String> {
    let conn = store.conn()?;
    let deps = load_sidebar_task_deps(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"SELECT
                   t.id,
                   t.project_id,
                   p.name,
                   t.title,
                   t.status,
                   t.created_at,
                   t.pinned
               FROM tasks t
               LEFT JOIN projects p ON p.id = t.project_id
               WHERE t.archived = 0
               ORDER BY t.pinned DESC, t.created_at DESC, t.sort_order ASC"#,
        )
        .map_err(|e| format!("task_list_sidebar_conversations: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map([], |row| {
            let task_id = row.get::<_, String>(0)?;
            let project_id = row.get::<_, Option<String>>(1)?;
            let project_name = row.get::<_, Option<String>>(2)?;
            let title = row.get::<_, String>(3)?;
            let status = row.get::<_, String>(4)?;
            let created_at = row.get::<_, i64>(5)?;
            let pinned = row.get::<_, i64>(6)? != 0;
            let depends_on = deps.get(&task_id).cloned().unwrap_or_default();
            let route = match project_id.as_deref() {
                Some(project_id) => format!("/projects/{project_id}/tasks/{task_id}"),
                None => format!("/chats/{task_id}"),
            };
            Ok(SidebarConversationSummaryRow {
                task_id,
                project_id,
                project_name,
                title,
                status,
                depends_on,
                created_at,
                pinned,
                route,
            })
        })
        .map_err(|e| format!("task_list_sidebar_conversations: query 失败：{e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("task_list_sidebar_conversations: row 失败：{e}"))?);
    }
    Ok(out)
}

fn load_sidebar_task_deps(
    conn: &rusqlite::Connection,
) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT d.task_id, d.depends_on_id
               FROM task_dependencies d
               INNER JOIN tasks t ON t.id = d.task_id
               WHERE t.archived = 0"#,
        )
        .map_err(|e| format!("task_list_sidebar_conversations: prepare deps 失败：{e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("task_list_sidebar_conversations: query deps 失败：{e}"))?;
    let mut out: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for row in rows {
        let (task_id, depends_on_id) =
            row.map_err(|e| format!("task_list_sidebar_conversations: dep row 失败：{e}"))?;
        out.entry(task_id).or_default().push(depends_on_id);
    }
    Ok(out)
}

#[tauri::command]
pub fn task_get(id: String, store: State<'_, LiliaStore>) -> Result<Option<TaskRow>, String> {
    let conn = store.conn()?;
    let deps_map = load_task_deps(&conn, &id)?;
    let result = conn
        .query_row(
            r#"SELECT id, project_id, session_id, title, title_source, status, created_at, parent_id, sort_order, pinned
               FROM tasks WHERE id = ?1 AND archived = 0"#,
            params![id],
            |row| build_task(row, &deps_map),
        )
        .optional()
        .map_err(|e| format!("task_get: {e}"))?;
    Ok(result)
}

#[tauri::command]
pub fn task_create(
    project_id: Option<String>,
    title: String,
    status: String,
    parent_id: Option<String>,
    depends_on: Vec<String>,
    store: State<'_, LiliaStore>,
    app: AppHandle,
) -> Result<TaskRow, String> {
    let conn = store.conn()?;
    let id = Uuid::new_v4().to_string();
    validate_parent(
        &conn,
        &id,
        project_id.as_deref(),
        parent_id.as_deref(),
        "task_create",
    )?;
    let depends_on =
        normalize_dependency_ids(&id, project_id.as_deref(), depends_on, &conn, "task_create")?;
    let now = now_millis();
    let sort_order = next_task_sort_order(&conn, project_id.as_deref(), "task_create")?;
    insert_task_with_deps(
        &conn,
        NewTask {
            id: &id,
            project_id: project_id.as_deref(),
            session_id: &id,
            title: &title,
            status: &status,
            created_at: now,
            parent_id: parent_id.as_deref(),
            sort_order,
            depends_on: &depends_on,
        },
        "task_create",
    )?;
    let row = TaskRow {
        id: id.clone(),
        project_id: project_id.clone(),
        session_id: id,
        title,
        title_source: "auto".to_string(),
        status,
        created_at: now,
        parent_id,
        depends_on,
        sort_order,
        pinned: false,
    };
    emit_tasks_changed(&app, project_id);
    crate::automation::emit_task_changed_signal(
        &app,
        row.project_id.clone(),
        Some(row.id.clone()),
        Some(row.status.clone()),
        task_created_event_kind(),
        None,
    );
    Ok(row)
}

#[tauri::command]
pub fn task_update(
    id: String,
    title: Option<String>,
    status: Option<String>,
    store: State<'_, LiliaStore>,
    app: AppHandle,
) -> Result<(), String> {
    if title.is_none() && status.is_none() {
        return Ok(());
    }
    let conn = store.conn()?;
    let (project_id, previous_status) = conn
        .query_row(
            "SELECT project_id, status FROM tasks WHERE id = ?1 AND archived = 0",
            params![id.as_str()],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|e| format!("task_update: 查询任务失败：{e}"))?
        .unwrap_or((None, String::new()));
    let mut next_status = previous_status;
    let status_changed = status.is_some();
    if let Some(t) = title {
        conn.execute(
            "UPDATE tasks SET title = ?1, title_source = 'manual' WHERE id = ?2",
            params![t, id.as_str()],
        )
        .map_err(|e| format!("task_update(title): {e}"))?;
    }
    if let Some(s) = status {
        conn.execute(
            "UPDATE tasks SET status = ?1 WHERE id = ?2",
            params![s, id.as_str()],
        )
        .map_err(|e| format!("task_update(status): {e}"))?;
        next_status = s;
    }
    emit_tasks_changed(&app, project_id.clone());
    crate::automation::emit_task_changed_signal(
        &app,
        project_id,
        Some(id),
        Some(next_status),
        if status_changed {
            task_status_changed_event_kind()
        } else {
            task_updated_event_kind()
        },
        None,
    );
    Ok(())
}

#[tauri::command]
pub fn task_delete(id: String, store: State<'_, LiliaStore>) -> Result<(), String> {
    let conn = store.conn()?;
    conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])
        .map_err(|e| format!("task_delete: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn task_promote(
    id: String,
    project_id: Option<String>,
    title: String,
    parent_id: Option<String>,
    depends_on: Vec<String>,
    store: State<'_, LiliaStore>,
    app: AppHandle,
) -> Result<TaskRow, String> {
    let conn = store.conn()?;
    let now = now_millis();
    validate_parent(
        &conn,
        &id,
        project_id.as_deref(),
        parent_id.as_deref(),
        "task_promote",
    )?;
    let depends_on = normalize_dependency_ids(
        &id,
        project_id.as_deref(),
        depends_on,
        &conn,
        "task_promote",
    )?;
    let sort_order = next_task_sort_order(&conn, project_id.as_deref(), "task_promote")?;
    let status = task_contract::running_task_status();
    insert_task_with_deps(
        &conn,
        NewTask {
            id: &id,
            project_id: project_id.as_deref(),
            session_id: &id,
            title: &title,
            status,
            created_at: now,
            parent_id: parent_id.as_deref(),
            sort_order,
            depends_on: &depends_on,
        },
        "task_promote",
    )?;
    let row = TaskRow {
        id: id.clone(),
        project_id: project_id.clone(),
        session_id: id,
        title,
        title_source: "auto".to_string(),
        status: status.to_string(),
        created_at: now,
        parent_id,
        depends_on,
        sort_order,
        pinned: false,
    };
    emit_tasks_changed(&app, project_id);
    crate::automation::emit_task_changed_signal(
        &app,
        row.project_id.clone(),
        Some(row.id.clone()),
        Some(row.status.clone()),
        task_created_event_kind(),
        None,
    );
    Ok(row)
}

#[tauri::command]
pub fn task_update_dependencies(
    id: String,
    depends_on: Vec<String>,
    application: State<'_, DesktopApplication>,
) -> Result<ProductTask, String> {
    let task_id =
        TaskId::new(id).map_err(|error| format!("task_update_dependencies: id 无效：{error}"))?;
    let depends_on = depends_on
        .into_iter()
        .map(TaskId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("task_update_dependencies: depends_on 无效：{error}"))?;
    application
        .update_task_dependencies(&task_id, depends_on)
        .map_err(|error| format!("task_update_dependencies: {error}"))
}

#[tauri::command]
pub fn task_archive_project(
    project_id: String,
    store: State<'_, LiliaStore>,
    _app: AppHandle,
) -> Result<i64, String> {
    let conn = store.conn()?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE project_id = ?1 AND archived = 0",
            params![project_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("task_archive_project: count 失败：{e}"))?;
    conn.execute(
        "UPDATE tasks SET archived = 1 WHERE project_id = ?1 AND archived = 0",
        params![project_id],
    )
    .map_err(|e| format!("task_archive_project: update 失败：{e}"))?;
    Ok(count)
}

#[tauri::command]
pub fn task_archive(
    id: String,
    store: State<'_, LiliaStore>,
    _app: AppHandle,
) -> Result<bool, String> {
    let conn = store.conn()?;
    let changed = conn
        .execute(
            "UPDATE tasks SET archived = 1 WHERE id = ?1 AND archived = 0",
            params![id],
        )
        .map_err(|e| format!("task_archive: {e}"))?;
    Ok(changed > 0)
}

#[tauri::command]
pub fn task_toggle_pin(id: String, store: State<'_, LiliaStore>) -> Result<bool, String> {
    let conn = store.conn()?;
    let current: i64 = conn
        .query_row(
            "SELECT pinned FROM tasks WHERE id = ?1 AND archived = 0",
            params![id],
            |row| row.get(0),
        )
        .map_err(|e| format!("task_toggle_pin: 查询失败：{e}"))?;
    let new_val = if current == 0 { 1i64 } else { 0i64 };
    conn.execute(
        "UPDATE tasks SET pinned = ?1 WHERE id = ?2",
        params![new_val, id],
    )
    .map_err(|e| format!("task_toggle_pin: 更新失败：{e}"))?;
    Ok(new_val != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_archive_schema(conn: &rusqlite::Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE tasks (
              id           TEXT PRIMARY KEY,
              project_id   TEXT,
              session_id   TEXT NOT NULL,
              title        TEXT NOT NULL,
              status       TEXT NOT NULL,
              created_at   INTEGER NOT NULL,
              archived     INTEGER NOT NULL DEFAULT 0,
              sort_order   INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE task_agent_sessions (
              task_id         TEXT NOT NULL,
              backend         TEXT NOT NULL,
              session_id      TEXT NOT NULL,
              updated_at      INTEGER NOT NULL,
              PRIMARY KEY (task_id, backend)
            );
            "#,
        )
        .unwrap();
    }

    fn insert_task(conn: &rusqlite::Connection, id: &str, project_id: &str, archived: bool) {
        conn.execute(
            r#"INSERT INTO tasks
               (id, project_id, session_id, title, status, created_at, archived, sort_order)
               VALUES (?1, ?2, ?1, ?3, 'waiting', 1, ?4, ?5)"#,
            params![
                id,
                project_id,
                format!("任务 {id}"),
                if archived { 1 } else { 0 },
                match id {
                    "task-2" => 2,
                    "task-3" => 3,
                    _ => 1,
                },
            ],
        )
        .unwrap();
    }

    fn insert_session(conn: &rusqlite::Connection, task_id: &str, backend: &str, session_id: &str) {
        conn.execute(
            r#"INSERT INTO task_agent_sessions
               (task_id, backend, session_id, updated_at)
               VALUES (?1, ?2, ?3, 1)"#,
            params![task_id, backend, session_id],
        )
        .unwrap();
    }

    fn task_archived(conn: &rusqlite::Connection, id: &str) -> bool {
        conn.query_row(
            "SELECT archived FROM tasks WHERE id = ?1",
            params![id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            != 0
    }

    #[test]
    fn archive_task_skips_already_archived_rows() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_archive_schema(&conn);
        insert_task(&conn, "active-task", "project-1", false);
        insert_task(&conn, "archived-task", "project-1", true);
        insert_session(
            &conn,
            "active-task",
            crate::native_agent::BACKEND_NATIVE_AGENTKIT,
            "native-session",
        );
        insert_session(
            &conn,
            "archived-task",
            crate::native_agent::BACKEND_NATIVE_AGENTKIT,
            "thread-archived",
        );

        let unchanged = conn
            .execute(
                "UPDATE tasks SET archived = 1 WHERE id = ?1 AND archived = 0",
                params!["archived-task"],
            )
            .unwrap();
        assert_eq!(unchanged, 0);

        let changed = conn
            .execute(
                "UPDATE tasks SET archived = 1 WHERE id = ?1 AND archived = 0",
                params!["active-task"],
            )
            .unwrap();
        assert_eq!(changed, 1);
        assert!(task_archived(&conn, "active-task"));
    }

    #[test]
    fn archive_project_marks_unarchived_tasks_only() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_archive_schema(&conn);
        insert_task(&conn, "task-1", "project-1", false);
        insert_task(&conn, "task-2", "project-1", false);
        insert_task(&conn, "task-3", "project-2", false);
        insert_task(&conn, "task-4", "project-1", true);
        insert_session(
            &conn,
            "task-1",
            crate::native_agent::BACKEND_NATIVE_AGENTKIT,
            "thread-1",
        );
        insert_session(
            &conn,
            "task-2",
            crate::native_agent::BACKEND_NATIVE_AGENTKIT,
            "thread-2",
        );

        let count = conn
            .execute(
                "UPDATE tasks SET archived = 1 WHERE project_id = ?1 AND archived = 0",
                params!["project-1"],
            )
            .unwrap();

        assert_eq!(count, 2);
        assert!(task_archived(&conn, "task-1"));
        assert!(task_archived(&conn, "task-2"));
        assert!(!task_archived(&conn, "task-3"));
        assert!(task_archived(&conn, "task-4"));
    }
}

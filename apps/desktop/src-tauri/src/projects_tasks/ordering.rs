use lilia_contracts::{ProductTask, ProjectId, TaskId};
use lilia_desktop_application::{DesktopApplication, DesktopTaskMove};
use rusqlite::{params, Connection};
use tauri::State;

use crate::store::LiliaStore;

#[cfg(test)]
use super::relations::validate_parent;

pub(super) fn next_task_sort_order(
    conn: &Connection,
    project_id: Option<&str>,
    context: &str,
) -> Result<i64, String> {
    let max_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) FROM tasks WHERE (project_id = ?1 OR (project_id IS NULL AND ?1 IS NULL)) AND archived = 0",
            params![project_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("{context}: max sort_order 失败：{e}"))?;
    Ok(max_order + 1)
}

#[tauri::command]
pub fn project_reorder(
    ordered_ids: Vec<String>,
    store: State<'_, LiliaStore>,
) -> Result<(), String> {
    let conn = store.conn()?;
    for (i, id) in ordered_ids.iter().enumerate() {
        conn.execute(
            "UPDATE projects SET sort_order = ?1 WHERE id = ?2",
            params![i as i64, id],
        )
        .map_err(|e| format!("project_reorder: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn task_reorder(
    project_id: Option<String>,
    ordered_ids: Vec<String>,
    application: State<'_, DesktopApplication>,
) -> Result<Vec<ProductTask>, String> {
    let project_id = project_id
        .map(|value| {
            ProjectId::new(value).map_err(|error| format!("task_reorder: project_id 无效：{error}"))
        })
        .transpose()?;
    let ordered_ids = ordered_ids
        .into_iter()
        .map(|value| {
            TaskId::new(value).map_err(|error| format!("task_reorder: task id 无效：{error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    application
        .reorder_tasks(project_id, &ordered_ids)
        .map_err(|error| format!("task_reorder: {error}"))
}

#[tauri::command]
pub fn task_reparent(
    task_id: String,
    new_project_id: Option<String>,
    new_parent_id: Option<String>,
    application: State<'_, DesktopApplication>,
) -> Result<ProductTask, String> {
    let task_id =
        TaskId::new(task_id).map_err(|error| format!("task_reparent: task_id 无效：{error}"))?;
    let target_project_id = new_project_id
        .map(|value| {
            ProjectId::new(value)
                .map_err(|error| format!("task_reparent: new_project_id 无效：{error}"))
        })
        .transpose()?;
    let target_parent_id = new_parent_id
        .map(|value| {
            TaskId::new(value)
                .map_err(|error| format!("task_reparent: new_parent_id 无效：{error}"))
        })
        .transpose()?;
    application
        .move_task(
            &task_id,
            DesktopTaskMove {
                target_project_id,
                target_parent_id,
            },
        )
        .map_err(|error| format!("task_reparent: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_tasks_schema(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE tasks (
              id          TEXT PRIMARY KEY,
              project_id  TEXT,
              session_id  TEXT NOT NULL,
              title       TEXT NOT NULL,
              status      TEXT NOT NULL DEFAULT 'waiting',
              created_at  INTEGER NOT NULL,
              parent_id   TEXT,
              archived    INTEGER NOT NULL DEFAULT 0,
              sort_order  INTEGER NOT NULL DEFAULT 0,
              pinned      INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE task_dependencies (
              task_id       TEXT NOT NULL,
              depends_on_id TEXT NOT NULL,
              PRIMARY KEY (task_id, depends_on_id)
            );
            "#,
        )
        .unwrap();
    }

    fn insert_task(conn: &Connection, id: &str, project_id: Option<&str>, parent_id: Option<&str>) {
        conn.execute(
            r#"INSERT INTO tasks
               (id, project_id, session_id, title, status, created_at, parent_id, sort_order)
               VALUES (?1, ?2, ?1, ?1, 'waiting', 1, ?3, 0)"#,
            params![id, project_id, parent_id],
        )
        .unwrap();
    }

    #[test]
    fn next_task_sort_order_is_scoped_by_project() {
        let conn = Connection::open_in_memory().unwrap();
        create_tasks_schema(&conn);
        conn.execute(
            "INSERT INTO tasks (id, project_id, session_id, title, status, created_at, sort_order) VALUES ('a', 'p1', 'a', 'A', 'waiting', 1, 4)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tasks (id, project_id, session_id, title, status, created_at, sort_order) VALUES ('b', NULL, 'b', 'B', 'waiting', 1, 2)",
            [],
        )
        .unwrap();

        assert_eq!(next_task_sort_order(&conn, Some("p1"), "test").unwrap(), 5);
        assert_eq!(next_task_sort_order(&conn, None, "test").unwrap(), 3);
        assert_eq!(next_task_sort_order(&conn, Some("p2"), "test").unwrap(), 0);
    }

    #[test]
    fn validate_parent_rejects_self_cross_project_and_cycles() {
        let conn = Connection::open_in_memory().unwrap();
        create_tasks_schema(&conn);
        insert_task(&conn, "parent", Some("p1"), None);
        insert_task(&conn, "child", Some("p1"), Some("parent"));
        insert_task(&conn, "other", Some("p2"), None);

        assert!(
            validate_parent(&conn, "child", Some("p1"), Some("child"), "test")
                .unwrap_err()
                .contains("自己的父任务")
        );
        assert!(
            validate_parent(&conn, "child", Some("p1"), Some("other"), "test")
                .unwrap_err()
                .contains("同一项目")
        );
        assert!(
            validate_parent(&conn, "parent", Some("p1"), Some("child"), "test")
                .unwrap_err()
                .contains("循环")
        );
        assert!(validate_parent(&conn, "child", Some("p1"), Some("parent"), "test").is_ok());
    }
}

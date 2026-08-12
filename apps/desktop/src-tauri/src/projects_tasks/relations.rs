use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};

pub(super) fn normalize_dependency_ids(
    task_id: &str,
    project_id: Option<&str>,
    depends_on: Vec<String>,
    conn: &Connection,
    context: &str,
) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dep in depends_on {
        let dep = dep.trim().to_string();
        if dep.is_empty() || !seen.insert(dep.clone()) {
            continue;
        }
        if dep == task_id {
            return Err(format!("{context}: 任务不能依赖自身"));
        }
        let dep_project_id = task_project_id(conn, &dep, context)?
            .ok_or_else(|| format!("{context}: 依赖任务不存在"))?;
        if dep_project_id.as_deref() != project_id {
            return Err(format!("{context}: 依赖任务必须属于同一项目"));
        }
        ensure_dependency_does_not_create_cycle(conn, task_id, &dep, context)?;
        out.push(dep);
    }
    Ok(out)
}

pub(super) fn validate_parent(
    conn: &Connection,
    task_id: &str,
    project_id: Option<&str>,
    parent_id: Option<&str>,
    context: &str,
) -> Result<(), String> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };
    if parent_id == task_id {
        return Err(format!("{context}: 任务不能成为自己的父任务"));
    }
    let parent_project_id = task_project_id(conn, parent_id, context)?
        .ok_or_else(|| format!("{context}: 父任务不存在"))?;
    if parent_project_id.as_deref() != project_id {
        return Err(format!("{context}: 父任务必须属于同一项目"));
    }
    ensure_parent_does_not_create_cycle(conn, task_id, parent_id, context)
}

pub(super) fn replace_task_dependencies(
    conn: &Connection,
    task_id: &str,
    depends_on: &[String],
    context: &str,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM task_dependencies WHERE task_id = ?1",
        params![task_id],
    )
    .map_err(|e| format!("{context}: 清理依赖失败：{e}"))?;
    for dep in depends_on {
        conn.execute(
            "INSERT INTO task_dependencies (task_id, depends_on_id) VALUES (?1, ?2)",
            params![task_id, dep],
        )
        .map_err(|e| format!("{context}: 写入依赖失败：{e}"))?;
    }
    Ok(())
}

pub(super) fn task_project_id(
    conn: &Connection,
    task_id: &str,
    context: &str,
) -> Result<Option<Option<String>>, String> {
    conn.query_row(
        "SELECT project_id FROM tasks WHERE id = ?1 AND archived = 0",
        params![task_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map_err(|e| format!("{context}: 查询任务失败：{e}"))
}

fn ensure_parent_does_not_create_cycle(
    conn: &Connection,
    task_id: &str,
    parent_id: &str,
    context: &str,
) -> Result<(), String> {
    let mut current = Some(parent_id.to_string());
    let mut seen = HashSet::new();
    while let Some(id) = current {
        if id == task_id {
            return Err(format!("{context}: 父子关系不能形成循环"));
        }
        if !seen.insert(id.clone()) {
            return Err(format!("{context}: 已存在循环父子关系"));
        }
        current = conn
            .query_row(
                "SELECT parent_id FROM tasks WHERE id = ?1 AND archived = 0",
                params![id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|e| format!("{context}: 查询父链失败：{e}"))?
            .flatten();
    }
    Ok(())
}

fn ensure_dependency_does_not_create_cycle(
    conn: &Connection,
    task_id: &str,
    dependency_id: &str,
    context: &str,
) -> Result<(), String> {
    let mut stack = vec![dependency_id.to_string()];
    let mut seen = HashSet::new();
    while let Some(current_id) = stack.pop() {
        if current_id == task_id {
            return Err(format!("{context}: 依赖关系不能形成循环"));
        }
        if !seen.insert(current_id.clone()) {
            continue;
        }
        let mut stmt = conn
            .prepare("SELECT depends_on_id FROM task_dependencies WHERE task_id = ?1")
            .map_err(|e| format!("{context}: 查询依赖链失败：{e}"))?;
        let rows = stmt
            .query_map(params![current_id], |row| row.get::<_, String>(0))
            .map_err(|e| format!("{context}: 查询依赖链失败：{e}"))?;
        for row in rows {
            stack.push(row.map_err(|e| format!("{context}: 读取依赖链失败：{e}"))?);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_schema(conn: &Connection) {
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

    fn insert_task(conn: &Connection, id: &str, project_id: Option<&str>) {
        conn.execute(
            r#"INSERT INTO tasks
               (id, project_id, session_id, title, status, created_at)
               VALUES (?1, ?2, ?1, ?1, 'waiting', 1)"#,
            params![id, project_id],
        )
        .unwrap();
    }

    #[test]
    fn normalize_dependency_ids_dedupes_and_keeps_order() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn);
        insert_task(&conn, "task", Some("p1"));
        insert_task(&conn, "dep-a", Some("p1"));
        insert_task(&conn, "dep-b", Some("p1"));

        let deps = normalize_dependency_ids(
            "task",
            Some("p1"),
            vec!["dep-a".into(), "dep-b".into(), "dep-a".into(), " ".into()],
            &conn,
            "test",
        )
        .unwrap();

        assert_eq!(deps, vec!["dep-a", "dep-b"]);
    }

    #[test]
    fn normalize_dependency_ids_rejects_self_and_cross_project() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn);
        insert_task(&conn, "task", Some("p1"));
        insert_task(&conn, "other", Some("p2"));

        assert!(
            normalize_dependency_ids("task", Some("p1"), vec!["task".into()], &conn, "test")
                .unwrap_err()
                .contains("依赖自身")
        );
        assert!(
            normalize_dependency_ids("task", Some("p1"), vec!["other".into()], &conn, "test")
                .unwrap_err()
                .contains("同一项目")
        );
    }

    #[test]
    fn normalize_dependency_ids_rejects_dependency_cycles() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn);
        insert_task(&conn, "task", Some("p1"));
        insert_task(&conn, "dep-a", Some("p1"));
        insert_task(&conn, "dep-b", Some("p1"));
        conn.execute(
            "INSERT INTO task_dependencies (task_id, depends_on_id) VALUES (?1, ?2)",
            params!["dep-a", "dep-b"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO task_dependencies (task_id, depends_on_id) VALUES (?1, ?2)",
            params!["dep-b", "task"],
        )
        .unwrap();

        assert!(
            normalize_dependency_ids("task", Some("p1"), vec!["dep-a".into()], &conn, "test")
                .unwrap_err()
                .contains("形成循环")
        );
    }
}

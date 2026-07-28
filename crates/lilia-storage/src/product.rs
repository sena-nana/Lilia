//! Product domain SQLite repositories (#56) — Project / Task / Binding.
//!
//! Separate from Agent Runtime and from Desktop Tauri UI cache (`lilia.db`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lilia_contracts::{
    AgentSessionBinding, AgentSessionRef, BindingId, ConversationId, ProductError, ProductResult,
    ProductRevision, ProductTask, ProductTaskStatus, Project, ProjectArchiveState, ProjectId,
    TaskId,
};
use rusqlite::{params, Connection, OptionalExtension};

const SCHEMA_VERSION: i64 = 1;

const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY NOT NULL,
  applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  workspace_path TEXT,
  pinned INTEGER NOT NULL DEFAULT 0,
  sort_order INTEGER NOT NULL DEFAULT 0,
  archive TEXT NOT NULL DEFAULT 'active',
  revision INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT,
  title TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft',
  parent_id TEXT,
  pinned INTEGER NOT NULL DEFAULT 0,
  sort_order INTEGER NOT NULL DEFAULT 0,
  revision INTEGER NOT NULL DEFAULT 1,
  legacy_source TEXT,
  FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE TABLE IF NOT EXISTS task_dependencies (
  task_id TEXT NOT NULL,
  depends_on_id TEXT NOT NULL,
  PRIMARY KEY (task_id, depends_on_id),
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (depends_on_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS agent_session_bindings (
  binding_id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  conversation_id TEXT,
  agent_session TEXT NOT NULL,
  profile_id TEXT,
  revision INTEGER NOT NULL DEFAULT 1,
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS migration_runs (
  id TEXT PRIMARY KEY NOT NULL,
  mode TEXT NOT NULL,
  legacy_db TEXT NOT NULL,
  product_db TEXT NOT NULL,
  status TEXT NOT NULL,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  backup_path TEXT,
  report_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS legacy_session_provenance (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  legacy_backend TEXT NOT NULL,
  legacy_session_id TEXT NOT NULL,
  disposition TEXT NOT NULL,
  compat_until TEXT,
  notes TEXT,
  UNIQUE (task_id, legacy_backend)
);
"#;

/// Durable product domain repository (SQLite).
pub struct SqliteProductStore {
    path: Option<PathBuf>,
    conn: Mutex<Connection>,
}

impl SqliteProductStore {
    pub fn open(path: impl AsRef<Path>) -> ProductResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| ProductError::Unavailable {
                message: format!("create product db dir: {err}"),
            })?;
        }
        let conn = Connection::open(&path).map_err(db_err)?;
        Self::configure(&conn)?;
        Self::migrate(&conn)?;
        Ok(Self {
            path: Some(path),
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> ProductResult<Self> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        Self::configure(&conn)?;
        Self::migrate(&conn)?;
        Ok(Self {
            path: None,
            conn: Mutex::new(conn),
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn configure(conn: &Connection) -> ProductResult<()> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;\
             PRAGMA journal_mode = WAL;\
             PRAGMA synchronous = NORMAL;\
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(db_err)
    }

    fn migrate(conn: &Connection) -> ProductResult<()> {
        conn.execute_batch(MIGRATION_V1).map_err(db_err)?;
        let current: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        if current < SCHEMA_VERSION {
            conn.execute(
                "INSERT INTO schema_migrations(version) VALUES (?1)",
                params![SCHEMA_VERSION],
            )
            .map_err(db_err)?;
        }
        Ok(())
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> ProductResult<T>) -> ProductResult<T> {
        let conn = self.conn.lock().map_err(|_| ProductError::Unavailable {
            message: "sqlite product store lock poisoned".into(),
        })?;
        f(&conn)
    }

    pub fn upsert_project(&self, project: &Project) -> ProductResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"INSERT INTO projects(id, name, workspace_path, pinned, sort_order, archive, revision)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                   ON CONFLICT(id) DO UPDATE SET
                     name=excluded.name,
                     workspace_path=excluded.workspace_path,
                     pinned=excluded.pinned,
                     sort_order=excluded.sort_order,
                     archive=excluded.archive,
                     revision=excluded.revision"#,
                params![
                    project.id.as_str(),
                    project.name,
                    project.workspace_path,
                    if project.pinned { 1 } else { 0 },
                    project.sort_order,
                    archive_to_str(project.archive),
                    project.revision.get() as i64,
                ],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    pub fn get_project(&self, id: &ProjectId) -> ProductResult<Project> {
        self.with_conn(|conn| {
            conn.query_row(
                r#"SELECT id, name, workspace_path, pinned, sort_order, archive, revision
                   FROM projects WHERE id = ?1"#,
                params![id.as_str()],
                map_project_row,
            )
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| ProductError::NotFound {
                entity: "project".into(),
                id: id.as_str().to_string(),
            })
        })
    }

    pub fn list_projects(&self) -> ProductResult<Vec<Project>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    r#"SELECT id, name, workspace_path, pinned, sort_order, archive, revision
                       FROM projects
                       ORDER BY pinned DESC, sort_order ASC, id ASC"#,
                )
                .map_err(db_err)?;
            let rows = stmt.query_map([], map_project_row).map_err(db_err)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db_err)?);
            }
            Ok(out)
        })
    }

    pub fn upsert_task(&self, task: &ProductTask) -> ProductResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"INSERT INTO tasks(id, project_id, title, status, parent_id, pinned, sort_order, revision, legacy_source)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                   ON CONFLICT(id) DO UPDATE SET
                     project_id=excluded.project_id,
                     title=excluded.title,
                     status=excluded.status,
                     parent_id=excluded.parent_id,
                     pinned=excluded.pinned,
                     sort_order=excluded.sort_order,
                     revision=excluded.revision,
                     legacy_source=excluded.legacy_source"#,
                params![
                    task.id.as_str(),
                    task.project_id.as_ref().map(|id| id.as_str().to_string()),
                    task.title,
                    status_to_str(task.status),
                    task.parent_id.as_ref().map(|id| id.as_str().to_string()),
                    if task.pinned { 1 } else { 0 },
                    task.sort_order,
                    task.revision.get() as i64,
                    task.legacy_source,
                ],
            )
            .map_err(db_err)?;
            conn.execute(
                "DELETE FROM task_dependencies WHERE task_id = ?1",
                params![task.id.as_str()],
            )
            .map_err(db_err)?;
            for dep in &task.depends_on {
                conn.execute(
                    "INSERT INTO task_dependencies(task_id, depends_on_id) VALUES (?1, ?2)",
                    params![task.id.as_str(), dep.as_str()],
                )
                .map_err(db_err)?;
            }
            Ok(())
        })
    }

    pub fn get_task(&self, id: &TaskId) -> ProductResult<ProductTask> {
        self.with_conn(|conn| {
            let mut task = conn
                .query_row(
                    r#"SELECT id, project_id, title, status, parent_id, pinned, sort_order, revision, legacy_source
                       FROM tasks WHERE id = ?1"#,
                    params![id.as_str()],
                    map_task_row,
                )
                .optional()
                .map_err(db_err)?
                .ok_or_else(|| ProductError::NotFound {
                    entity: "task".into(),
                    id: id.as_str().to_string(),
                })?;
            task.depends_on = load_deps(conn, id.as_str())?;
            Ok(task)
        })
    }

    pub fn list_tasks(&self) -> ProductResult<Vec<ProductTask>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    r#"SELECT id, project_id, title, status, parent_id, pinned, sort_order, revision, legacy_source
                       FROM tasks
                       ORDER BY pinned DESC, sort_order ASC, id ASC"#,
                )
                .map_err(db_err)?;
            let rows = stmt.query_map([], map_task_row).map_err(db_err)?;
            let mut out = Vec::new();
            for row in rows {
                let mut task = row.map_err(db_err)?;
                task.depends_on = load_deps(conn, task.id.as_str())?;
                out.push(task);
            }
            Ok(out)
        })
    }

    pub fn upsert_binding(&self, binding: &AgentSessionBinding) -> ProductResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"INSERT INTO agent_session_bindings
                   (binding_id, task_id, conversation_id, agent_session, profile_id, revision)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                   ON CONFLICT(binding_id) DO UPDATE SET
                     task_id=excluded.task_id,
                     conversation_id=excluded.conversation_id,
                     agent_session=excluded.agent_session,
                     profile_id=excluded.profile_id,
                     revision=excluded.revision"#,
                params![
                    binding.binding_id.as_str(),
                    binding.task_id.as_str(),
                    binding
                        .conversation_id
                        .as_ref()
                        .map(|id| id.as_str().to_string()),
                    binding.agent_session.as_str(),
                    binding.profile_id,
                    binding.revision.get() as i64,
                ],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    pub fn list_bindings_for_task(&self, task_id: &TaskId) -> ProductResult<Vec<AgentSessionBinding>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    r#"SELECT binding_id, task_id, conversation_id, agent_session, profile_id, revision
                       FROM agent_session_bindings WHERE task_id = ?1"#,
                )
                .map_err(db_err)?;
            let rows = stmt
                .query_map(params![task_id.as_str()], map_binding_row)
                .map_err(db_err)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db_err)?);
            }
            Ok(out)
        })
    }

    pub fn list_all_bindings(&self) -> ProductResult<Vec<AgentSessionBinding>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    r#"SELECT binding_id, task_id, conversation_id, agent_session, profile_id, revision
                       FROM agent_session_bindings
                       ORDER BY task_id ASC, binding_id ASC"#,
                )
                .map_err(db_err)?;
            let rows = stmt.query_map([], map_binding_row).map_err(db_err)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db_err)?);
            }
            Ok(out)
        })
    }

    pub fn record_legacy_session_provenance(
        &self,
        id: &str,
        task_id: &TaskId,
        legacy_backend: &str,
        legacy_session_id: &str,
        disposition: &str,
        compat_until: Option<&str>,
        notes: Option<&str>,
    ) -> ProductResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"INSERT INTO legacy_session_provenance
                   (id, task_id, legacy_backend, legacy_session_id, disposition, compat_until, notes)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                   ON CONFLICT(task_id, legacy_backend) DO UPDATE SET
                     legacy_session_id=excluded.legacy_session_id,
                     disposition=excluded.disposition,
                     compat_until=excluded.compat_until,
                     notes=excluded.notes"#,
                params![
                    id,
                    task_id.as_str(),
                    legacy_backend,
                    legacy_session_id,
                    disposition,
                    compat_until,
                    notes,
                ],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    pub fn list_legacy_session_provenance(&self) -> ProductResult<Vec<LegacySessionProvenance>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    r#"SELECT id, task_id, legacy_backend, legacy_session_id, disposition, compat_until, notes
                       FROM legacy_session_provenance
                       ORDER BY task_id, legacy_backend"#,
                )
                .map_err(db_err)?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(LegacySessionProvenance {
                        id: row.get(0)?,
                        task_id: row.get(1)?,
                        legacy_backend: row.get(2)?,
                        legacy_session_id: row.get(3)?,
                        disposition: row.get(4)?,
                        compat_until: row.get(5)?,
                        notes: row.get(6)?,
                    })
                })
                .map_err(db_err)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db_err)?);
            }
            Ok(out)
        })
    }

    pub fn record_migration_run(
        &self,
        id: &str,
        mode: &str,
        legacy_db: &str,
        product_db: &str,
        status: &str,
        started_at: &str,
        finished_at: Option<&str>,
        backup_path: Option<&str>,
        report_json: &str,
    ) -> ProductResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"INSERT INTO migration_runs
                   (id, mode, legacy_db, product_db, status, started_at, finished_at, backup_path, report_json)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                   ON CONFLICT(id) DO UPDATE SET
                     status=excluded.status,
                     finished_at=excluded.finished_at,
                     backup_path=excluded.backup_path,
                     report_json=excluded.report_json"#,
                params![
                    id,
                    mode,
                    legacy_db,
                    product_db,
                    status,
                    started_at,
                    finished_at,
                    backup_path,
                    report_json,
                ],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    pub fn latest_migration_run(&self) -> ProductResult<Option<MigrationRunRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                r#"SELECT id, mode, legacy_db, product_db, status, started_at, finished_at, backup_path, report_json
                   FROM migration_runs
                   ORDER BY started_at DESC
                   LIMIT 1"#,
                [],
                |row| {
                    Ok(MigrationRunRecord {
                        id: row.get(0)?,
                        mode: row.get(1)?,
                        legacy_db: row.get(2)?,
                        product_db: row.get(3)?,
                        status: row.get(4)?,
                        started_at: row.get(5)?,
                        finished_at: row.get(6)?,
                        backup_path: row.get(7)?,
                        report_json: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(db_err)
        })
    }

    /// Wipe product domain tables (used by rollback after restore from backup file).
    pub fn clear_all_product_rows(&self) -> ProductResult<()> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "DELETE FROM agent_session_bindings;\
                 DELETE FROM task_dependencies;\
                 DELETE FROM legacy_session_provenance;\
                 DELETE FROM tasks;\
                 DELETE FROM projects;",
            )
            .map_err(db_err)?;
            Ok(())
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacySessionProvenance {
    pub id: String,
    pub task_id: String,
    pub legacy_backend: String,
    pub legacy_session_id: String,
    pub disposition: String,
    pub compat_until: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationRunRecord {
    pub id: String,
    pub mode: String,
    pub legacy_db: String,
    pub product_db: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub backup_path: Option<String>,
    pub report_json: String,
}

fn db_err(err: rusqlite::Error) -> ProductError {
    ProductError::Unavailable {
        message: format!("sqlite product: {err}"),
    }
}

fn invalid_id(err: impl std::fmt::Display) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            err.to_string(),
        )),
    )
}

fn archive_to_str(state: ProjectArchiveState) -> &'static str {
    match state {
        ProjectArchiveState::Active => "active",
        ProjectArchiveState::Archived => "archived",
    }
}

fn archive_from_str(value: &str) -> ProjectArchiveState {
    match value {
        "archived" => ProjectArchiveState::Archived,
        _ => ProjectArchiveState::Active,
    }
}

fn status_to_str(status: ProductTaskStatus) -> &'static str {
    match status {
        ProductTaskStatus::Draft => "draft",
        ProductTaskStatus::Waiting => "waiting",
        ProductTaskStatus::Running => "running",
        ProductTaskStatus::Blocked => "blocked",
        ProductTaskStatus::Done => "done",
        ProductTaskStatus::Cancelled => "cancelled",
    }
}

fn status_from_str(value: &str) -> ProductTaskStatus {
    match value {
        "waiting" => ProductTaskStatus::Waiting,
        "running" => ProductTaskStatus::Running,
        "blocked" => ProductTaskStatus::Blocked,
        "done" => ProductTaskStatus::Done,
        "cancelled" => ProductTaskStatus::Cancelled,
        _ => ProductTaskStatus::Draft,
    }
}

fn map_project_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    let archive: String = row.get(5)?;
    Ok(Project {
        id: ProjectId::new(row.get::<_, String>(0)?).map_err(invalid_id)?,
        name: row.get(1)?,
        workspace_path: row.get(2)?,
        pinned: row.get::<_, i64>(3)? != 0,
        sort_order: row.get(4)?,
        archive: archive_from_str(&archive),
        revision: ProductRevision::new(row.get::<_, i64>(6)? as u64).map_err(invalid_id)?,
    })
}

fn map_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProductTask> {
    let project_id: Option<String> = row.get(1)?;
    let parent_id: Option<String> = row.get(4)?;
    let status: String = row.get(3)?;
    Ok(ProductTask {
        id: TaskId::new(row.get::<_, String>(0)?).map_err(invalid_id)?,
        project_id: project_id
            .map(ProjectId::new)
            .transpose()
            .map_err(invalid_id)?,
        title: row.get(2)?,
        status: status_from_str(&status),
        depends_on: Vec::new(),
        parent_id: parent_id.map(TaskId::new).transpose().map_err(invalid_id)?,
        pinned: row.get::<_, i64>(5)? != 0,
        sort_order: row.get(6)?,
        revision: ProductRevision::new(row.get::<_, i64>(7)? as u64).map_err(invalid_id)?,
        legacy_source: row.get(8)?,
    })
}

fn map_binding_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentSessionBinding> {
    let conversation_id: Option<String> = row.get(2)?;
    Ok(AgentSessionBinding {
        binding_id: BindingId::new(row.get::<_, String>(0)?).map_err(invalid_id)?,
        task_id: TaskId::new(row.get::<_, String>(1)?).map_err(invalid_id)?,
        conversation_id: conversation_id
            .map(ConversationId::new)
            .transpose()
            .map_err(invalid_id)?,
        agent_session: AgentSessionRef::new(row.get::<_, String>(3)?).map_err(invalid_id)?,
        profile_id: row.get(4)?,
        revision: ProductRevision::new(row.get::<_, i64>(5)? as u64).map_err(invalid_id)?,
    })
}

fn load_deps(conn: &Connection, task_id: &str) -> ProductResult<Vec<TaskId>> {
    let mut stmt = conn
        .prepare("SELECT depends_on_id FROM task_dependencies WHERE task_id = ?1 ORDER BY depends_on_id")
        .map_err(db_err)?;
    let rows = stmt
        .query_map(params![task_id], |row| row.get::<_, String>(0))
        .map_err(db_err)?;
    let mut out = Vec::new();
    for row in rows {
        let id = row.map_err(db_err)?;
        out.push(TaskId::new(id).map_err(|err| ProductError::InvalidInput {
            field: "depends_on".into(),
            message: err.to_string(),
        })?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_task_roundtrip() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let project = Project::new(ProjectId::new("p1").unwrap(), "Demo").unwrap();
        store.upsert_project(&project).unwrap();
        let mut task = ProductTask::new(
            TaskId::new("t1").unwrap(),
            Some(project.id.clone()),
            "hello",
        )
        .unwrap();
        task.legacy_source = Some("codex".into());
        store.upsert_task(&task).unwrap();
        assert_eq!(store.list_projects().unwrap().len(), 1);
        let loaded = store.get_task(&task.id).unwrap();
        assert_eq!(loaded.legacy_source.as_deref(), Some("codex"));
    }
}

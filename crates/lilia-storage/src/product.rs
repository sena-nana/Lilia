//! Product domain SQLite repositories (#56) — Project / Task / Binding.
//!
//! Separate from Agent Runtime and from Desktop Tauri UI cache (`lilia.db`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lilia_contracts::{
    AgentSessionBinding, AgentSessionRef, AssignmentId, BindingId, ConflictKind, ConversationId,
    ExpectedRevision, LiliaCodeTaskHandoff, MilestoneId, Page, PageRequest, ProductCommandMeta,
    ProductCommandResult, ProductEntity, ProductEntityKind, ProductError, ProductEvent,
    ProductEventSequence, ProductProjectRemovalOutcome, ProductProjectReorderEntry,
    ProductProjectReorderOutcome, ProductResult, ProductRevision, ProductTask,
    ProductTaskHandoffImport, ProductTaskHandoffRecord, ProductTaskMoveInput,
    ProductTaskMoveOutcome, ProductTaskPriority, ProductTaskReorderEntry,
    ProductTaskReorderOutcome, ProductTaskStatus, Project, ProjectArchiveState, ProjectId,
    TaskDependencyGraph, TaskId, WorkflowId,
};
use lilia_core::{ensure_expected_revision, ProductRepository};
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;

pub const PRODUCT_SCHEMA_VERSION: i64 = 6;

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

const MIGRATION_V2: &str = r#"
ALTER TABLE projects ADD COLUMN git_workspace_json TEXT;
ALTER TABLE projects ADD COLUMN settings_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE projects ADD COLUMN asset_ids_json TEXT NOT NULL DEFAULT '[]';

ALTER TABLE tasks ADD COLUMN description TEXT;
ALTER TABLE tasks ADD COLUMN priority TEXT NOT NULL DEFAULT 'normal';
ALTER TABLE tasks ADD COLUMN assignment_id TEXT;
ALTER TABLE tasks ADD COLUMN completion_criteria_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE tasks ADD COLUMN milestone_id TEXT;
ALTER TABLE tasks ADD COLUMN workflow_id TEXT;
ALTER TABLE tasks ADD COLUMN agent_profile_id TEXT;
ALTER TABLE tasks ADD COLUMN blocked_reason TEXT;
ALTER TABLE tasks ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]';
"#;

const MIGRATION_V3: &str = r#"
CREATE TABLE IF NOT EXISTS product_entities (
  kind TEXT NOT NULL,
  id TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  revision INTEGER NOT NULL,
  PRIMARY KEY (kind, id)
);
"#;

const MIGRATION_V4: &str = r#"
CREATE TABLE IF NOT EXISTS product_events (
  sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  command_id TEXT NOT NULL,
  entity TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  action TEXT NOT NULL,
  revision INTEGER
);

CREATE TABLE IF NOT EXISTS product_command_results (
  idempotency_key TEXT PRIMARY KEY NOT NULL,
  command_id TEXT NOT NULL,
  event_sequence INTEGER NOT NULL,
  result_json TEXT NOT NULL,
  FOREIGN KEY (event_sequence) REFERENCES product_events(sequence)
);
"#;

const MIGRATION_V5: &str = r#"
ALTER TABLE tasks ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
"#;

const MIGRATION_V6: &str = r#"
CREATE TABLE IF NOT EXISTS product_task_handoffs (
  handoff_id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL UNIQUE,
  payload_json TEXT NOT NULL,
  accepted_at INTEGER NOT NULL,
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);
"#;

/// Durable product domain repository (SQLite).
pub struct SqliteProductStore {
    path: Option<PathBuf>,
    conn: Mutex<Connection>,
    mutation_lock: Mutex<()>,
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
            mutation_lock: Mutex::new(()),
        })
    }

    pub fn open_in_memory() -> ProductResult<Self> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        Self::configure(&conn)?;
        Self::migrate(&conn)?;
        Ok(Self {
            path: None,
            conn: Mutex::new(conn),
            mutation_lock: Mutex::new(()),
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
        if current < 1 {
            conn.execute(
                "INSERT INTO schema_migrations(version) VALUES (?1)",
                params![1],
            )
            .map_err(db_err)?;
        }
        if current < 2 {
            conn.execute_batch(MIGRATION_V2).map_err(db_err)?;
            conn.execute(
                "INSERT INTO schema_migrations(version) VALUES (?1)",
                params![2],
            )
            .map_err(db_err)?;
        }
        if current < 3 {
            conn.execute_batch(MIGRATION_V3).map_err(db_err)?;
            conn.execute(
                "INSERT INTO schema_migrations(version) VALUES (?1)",
                params![3],
            )
            .map_err(db_err)?;
        }
        if current < 4 {
            conn.execute_batch(MIGRATION_V4).map_err(db_err)?;
            conn.execute(
                "INSERT INTO schema_migrations(version) VALUES (?1)",
                params![4],
            )
            .map_err(db_err)?;
        }
        if current < 5 {
            conn.execute_batch(MIGRATION_V5).map_err(db_err)?;
            conn.execute(
                "INSERT INTO schema_migrations(version) VALUES (?1)",
                params![5],
            )
            .map_err(db_err)?;
        }
        if current < 6 {
            conn.execute_batch(MIGRATION_V6).map_err(db_err)?;
            conn.execute(
                "INSERT INTO schema_migrations(version) VALUES (?1)",
                params![PRODUCT_SCHEMA_VERSION],
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

    fn lock_mutation(&self) -> ProductResult<std::sync::MutexGuard<'_, ()>> {
        self.mutation_lock
            .lock()
            .map_err(|_| ProductError::Unavailable {
                message: "sqlite product mutation lock poisoned".into(),
            })
    }

    pub fn upsert_project(&self, project: &Project) -> ProductResult<()> {
        self.with_conn(|conn| {
            let git_workspace_json = encode_json(&project.git_workspace)?;
            let settings_json = encode_json(&project.settings)?;
            let asset_ids_json = encode_json(&project.asset_ids)?;
            conn.execute(
                r#"INSERT INTO projects(
                     id, name, workspace_path, pinned, sort_order, archive,
                     git_workspace_json, settings_json, asset_ids_json, revision
                   )
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                   ON CONFLICT(id) DO UPDATE SET
                     name=excluded.name,
                     workspace_path=excluded.workspace_path,
                     pinned=excluded.pinned,
                     sort_order=excluded.sort_order,
                     archive=excluded.archive,
                     git_workspace_json=excluded.git_workspace_json,
                     settings_json=excluded.settings_json,
                     asset_ids_json=excluded.asset_ids_json,
                     revision=excluded.revision"#,
                params![
                    project.id.as_str(),
                    project.name,
                    project.workspace_path,
                    if project.pinned { 1 } else { 0 },
                    project.sort_order,
                    archive_to_str(project.archive),
                    git_workspace_json,
                    settings_json,
                    asset_ids_json,
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
                r#"SELECT id, name, workspace_path, pinned, sort_order, archive,
                          git_workspace_json, settings_json, asset_ids_json, revision
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
                    r#"SELECT id, name, workspace_path, pinned, sort_order, archive,
                              git_workspace_json, settings_json, asset_ids_json, revision
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
            let completion_criteria_json = encode_json(&task.completion_criteria)?;
            let tags_json = encode_json(&task.tags)?;
            conn.execute(
                r#"INSERT INTO tasks(
                     id, project_id, title, description, status, priority, assignment_id,
                     completion_criteria_json, milestone_id, workflow_id, agent_profile_id,
                     blocked_reason, parent_id, pinned, sort_order, archived, tags_json,
                     created_at, updated_at, revision, legacy_source
                   )
                   VALUES (
                     ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                     ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
                   )
                   ON CONFLICT(id) DO UPDATE SET
                     project_id=excluded.project_id,
                     title=excluded.title,
                     description=excluded.description,
                     status=excluded.status,
                     priority=excluded.priority,
                     assignment_id=excluded.assignment_id,
                     completion_criteria_json=excluded.completion_criteria_json,
                     milestone_id=excluded.milestone_id,
                     workflow_id=excluded.workflow_id,
                     agent_profile_id=excluded.agent_profile_id,
                     blocked_reason=excluded.blocked_reason,
                     parent_id=excluded.parent_id,
                     pinned=excluded.pinned,
                     sort_order=excluded.sort_order,
                     archived=excluded.archived,
                     tags_json=excluded.tags_json,
                     created_at=excluded.created_at,
                     updated_at=excluded.updated_at,
                     revision=excluded.revision,
                     legacy_source=excluded.legacy_source"#,
                params![
                    task.id.as_str(),
                    task.project_id.as_ref().map(|id| id.as_str()),
                    task.title,
                    task.description,
                    status_to_str(task.status),
                    priority_to_str(task.priority),
                    task.assignment_id.as_ref().map(|id| id.as_str()),
                    completion_criteria_json,
                    task.milestone_id.as_ref().map(|id| id.as_str()),
                    task.workflow_id.as_ref().map(|id| id.as_str()),
                    task.agent_profile_id,
                    task.blocked_reason,
                    task.parent_id.as_ref().map(|id| id.as_str()),
                    if task.pinned { 1 } else { 0 },
                    task.sort_order,
                    if task.archived { 1 } else { 0 },
                    tags_json,
                    task.created_at,
                    task.updated_at,
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
                    r#"SELECT id, project_id, title, description, status, priority, assignment_id,
                              completion_criteria_json, milestone_id, workflow_id, agent_profile_id,
                              blocked_reason, parent_id, pinned, sort_order, archived, tags_json,
                              created_at, updated_at, revision, legacy_source
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
                    r#"SELECT id, project_id, title, description, status, priority, assignment_id,
                              completion_criteria_json, milestone_id, workflow_id, agent_profile_id,
                              blocked_reason, parent_id, pinned, sort_order, archived, tags_json,
                              created_at, updated_at, revision, legacy_source
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

    pub fn list_bindings_for_task(
        &self,
        task_id: &TaskId,
    ) -> ProductResult<Vec<AgentSessionBinding>> {
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

    pub fn clear_bindings_for_task(&self, task_id: &TaskId) -> ProductResult<usize> {
        self.with_conn(|conn| {
            let removed = conn
                .execute(
                    "DELETE FROM agent_session_bindings WHERE task_id = ?1",
                    params![task_id.as_str()],
                )
                .map_err(db_err)?;
            Ok(removed)
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

    /// Read legacy session provenance rows (schema retained for existing product.db).
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
}

impl ProductRepository for SqliteProductStore {
    fn create_entity(&self, entity: ProductEntity) -> ProductResult<ProductEntity> {
        match entity {
            ProductEntity::Project(project) => {
                self.create_project(project).map(ProductEntity::Project)
            }
            ProductEntity::Task(task) => self.create_task(task).map(ProductEntity::Task),
            ProductEntity::Binding(binding) => {
                self.record_binding(binding).map(ProductEntity::Binding)
            }
            entity => {
                let payload_json = encode_json(&entity)?;
                let kind = entity.kind();
                let id = entity.id().to_string();
                let revision = entity.revision();
                self.with_conn(|conn| {
                    let result = conn.execute(
                        r#"INSERT INTO product_entities(kind, id, payload_json, revision)
                           VALUES (?1, ?2, ?3, ?4)"#,
                        params![kind.as_str(), id, payload_json, revision.get() as i64],
                    );
                    match result {
                        Ok(_) => Ok(()),
                        Err(err) if is_constraint_violation(&err) => Err(ProductError::Conflict {
                            conflict: ConflictKind::DuplicateIdempotency,
                            message: format!("{} `{}` already exists", kind.as_str(), id),
                        }),
                        Err(err) => Err(db_err(err)),
                    }
                })?;
                Ok(entity)
            }
        }
    }

    fn update_entity(
        &self,
        mut entity: ProductEntity,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductEntity> {
        let _mutation = self.lock_mutation()?;
        let current = self.get_entity(entity.kind(), entity.id())?;
        ensure_expected_revision(expected, current.revision())?;
        entity.set_revision(current.revision().next());
        match &entity {
            ProductEntity::Project(project) => self.upsert_project(project)?,
            ProductEntity::Task(task) => self.upsert_task(task)?,
            ProductEntity::Binding(binding) => self.upsert_binding(binding)?,
            _ => {
                let payload_json = encode_json(&entity)?;
                let changed = self.with_conn(|conn| {
                    conn.execute(
                        r#"UPDATE product_entities
                           SET payload_json = ?1, revision = ?2
                           WHERE kind = ?3 AND id = ?4 AND revision = ?5"#,
                        params![
                            payload_json,
                            entity.revision().get() as i64,
                            entity.kind().as_str(),
                            entity.id(),
                            expected.get() as i64,
                        ],
                    )
                    .map_err(db_err)
                })?;
                if changed != 1 {
                    let latest = self.get_entity(entity.kind(), entity.id())?;
                    ensure_expected_revision(expected, latest.revision())?;
                    return Err(ProductError::Unavailable {
                        message: "product entity update did not affect a row".into(),
                    });
                }
            }
        }
        Ok(entity)
    }

    fn get_entity(&self, kind: ProductEntityKind, id: &str) -> ProductResult<ProductEntity> {
        match kind {
            ProductEntityKind::Project => ProjectId::new(id)
                .and_then(|id| self.get_project(&id))
                .map(ProductEntity::Project),
            ProductEntityKind::Task => TaskId::new(id)
                .and_then(|id| self.get_task(&id))
                .map(ProductEntity::Task),
            ProductEntityKind::Binding => {
                let binding_id = BindingId::new(id)?;
                self.list_all_bindings()?
                    .into_iter()
                    .find(|binding| binding.binding_id == binding_id)
                    .map(ProductEntity::Binding)
                    .ok_or_else(|| ProductError::NotFound {
                        entity: kind.as_str().into(),
                        id: id.into(),
                    })
            }
            _ => self.with_conn(|conn| {
                conn.query_row(
                    r#"SELECT payload_json FROM product_entities
                       WHERE kind = ?1 AND id = ?2"#,
                    params![kind.as_str(), id],
                    |row| {
                        let payload_json: String = row.get(0)?;
                        decode_json(&payload_json)
                    },
                )
                .optional()
                .map_err(db_err)?
                .ok_or_else(|| ProductError::NotFound {
                    entity: kind.as_str().into(),
                    id: id.into(),
                })
            }),
        }
    }

    fn list_entities(&self, kind: ProductEntityKind) -> ProductResult<Vec<ProductEntity>> {
        match kind {
            ProductEntityKind::Project => self
                .list_projects()
                .map(|values| values.into_iter().map(ProductEntity::Project).collect()),
            ProductEntityKind::Task => self
                .list_tasks()
                .map(|values| values.into_iter().map(ProductEntity::Task).collect()),
            ProductEntityKind::Binding => self
                .list_all_bindings()
                .map(|values| values.into_iter().map(ProductEntity::Binding).collect()),
            _ => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        r#"SELECT payload_json FROM product_entities
                           WHERE kind = ?1 ORDER BY id ASC"#,
                    )
                    .map_err(db_err)?;
                let rows = stmt
                    .query_map(params![kind.as_str()], |row| {
                        let payload_json: String = row.get(0)?;
                        decode_json(&payload_json)
                    })
                    .map_err(db_err)?;
                let mut entities = Vec::new();
                for row in rows {
                    entities.push(row.map_err(db_err)?);
                }
                Ok(entities)
            }),
        }
    }

    fn create_entity_command(
        &self,
        meta: &ProductCommandMeta,
        entity: ProductEntity,
        action: &str,
    ) -> ProductResult<ProductCommandResult<ProductEntity>> {
        let _mutation = self.lock_mutation()?;
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction().map_err(db_err)?;
            if let Some(result) =
                load_command_result_on(&transaction, meta.idempotency_key.as_str())?
            {
                return duplicate_sqlite_command_result(meta, result);
            }
            insert_entity_on(&transaction, &entity)?;
            let result = record_command_result_on(&transaction, meta, entity, action)?;
            transaction.commit().map_err(db_err)?;
            Ok(result)
        })
    }

    fn update_entity_command(
        &self,
        meta: &ProductCommandMeta,
        mut entity: ProductEntity,
        action: &str,
    ) -> ProductResult<ProductCommandResult<ProductEntity>> {
        let expected = meta
            .expected_revision
            .ok_or_else(|| ProductError::InvalidInput {
                field: "expected_revision".into(),
                message: "update command requires expected_revision".into(),
            })?;
        let _mutation = self.lock_mutation()?;
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction().map_err(db_err)?;
            if let Some(result) =
                load_command_result_on(&transaction, meta.idempotency_key.as_str())?
            {
                return duplicate_sqlite_command_result(meta, result);
            }
            let current_revision = entity_revision_on(&transaction, entity.kind(), entity.id())?;
            ensure_expected_revision(expected, current_revision)?;
            entity.set_revision(current_revision.next());
            update_entity_on(&transaction, &entity, expected)?;
            let result = record_command_result_on(&transaction, meta, entity, action)?;
            transaction.commit().map_err(db_err)?;
            Ok(result)
        })
    }

    fn remove_project_command(
        &self,
        meta: &ProductCommandMeta,
        project_id: &ProjectId,
        removed_at: i64,
    ) -> ProductResult<ProductCommandResult<ProductProjectRemovalOutcome>> {
        let expected = meta
            .expected_revision
            .ok_or_else(|| ProductError::InvalidInput {
                field: "expected_revision".into(),
                message: "remove project command requires expected_revision".into(),
            })?;
        let _mutation = self.lock_mutation()?;
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction().map_err(db_err)?;
            if let Some(result) = load_command_result_on::<ProductProjectRemovalOutcome>(
                &transaction,
                meta.idempotency_key.as_str(),
            )? {
                return duplicate_sqlite_command_result(meta, result);
            }
            let mut project = load_project_on(&transaction, project_id)?.ok_or_else(|| {
                ProductError::NotFound {
                    entity: "project".into(),
                    id: project_id.as_str().into(),
                }
            })?;
            ensure_expected_revision(expected, project.revision)?;
            if project.archive == ProjectArchiveState::Archived {
                return Err(ProductError::InvalidState {
                    message: format!("project `{project_id}` is already archived"),
                });
            }

            let conversation_payloads = {
                let mut statement = transaction
                    .prepare(
                        "SELECT payload_json FROM product_entities WHERE kind = 'conversation' ORDER BY id ASC",
                    )
                    .map_err(db_err)?;
                let rows = statement
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(db_err)?;
                let mut payloads = Vec::new();
                for row in rows {
                    payloads.push(row.map_err(db_err)?);
                }
                payloads
            };
            let mut moved_conversation_ids = Vec::new();
            for payload in conversation_payloads {
                let ProductEntity::Conversation(mut conversation) =
                    decode_json::<ProductEntity>(&payload).map_err(db_err)?
                else {
                    continue;
                };
                if conversation.project_id.as_ref() != Some(project_id) || conversation.archived {
                    continue;
                }
                let current_revision = conversation.revision;
                conversation.project_id = None;
                conversation.updated_at = conversation.updated_at.max(removed_at);
                conversation.revision = current_revision.next();
                let entity = ProductEntity::Conversation(conversation.clone());
                update_entity_on(
                    &transaction,
                    &entity,
                    ExpectedRevision::new(current_revision.get())?,
                )?;
                record_product_event_on(
                    &transaction,
                    meta,
                    &entity,
                    "detached_from_project",
                )?;
                moved_conversation_ids.push(conversation.id);
            }

            let task_rows = {
                let mut statement = transaction
                    .prepare(
                        "SELECT id, revision FROM tasks WHERE project_id = ?1 AND archived = 0 ORDER BY id ASC",
                    )
                    .map_err(db_err)?;
                let rows = statement
                    .query_map(params![project_id.as_str()], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })
                    .map_err(db_err)?;
                let mut tasks = Vec::new();
                for row in rows {
                    tasks.push(row.map_err(db_err)?);
                }
                tasks
            };
            transaction
                .execute(
                    r#"UPDATE tasks
                       SET project_id = NULL,
                           updated_at = MAX(updated_at, ?1),
                           revision = revision + 1
                       WHERE project_id = ?2 AND archived = 0"#,
                    params![removed_at, project_id.as_str()],
                )
                .map_err(db_err)?;
            let mut moved_task_ids = Vec::with_capacity(task_rows.len());
            for (task_id, revision) in task_rows {
                let task_id = TaskId::new(task_id)?;
                record_product_event_fields_on(
                    &transaction,
                    meta,
                    ProductEntityKind::Task,
                    task_id.as_str(),
                    "detached_from_project",
                    ProductRevision::new((revision as u64).saturating_add(1))?,
                )?;
                moved_task_ids.push(task_id);
            }

            let project_revision = project.revision;
            project.archive = ProjectArchiveState::Archived;
            project.revision = project_revision.next();
            update_entity_on(
                &transaction,
                &ProductEntity::Project(project.clone()),
                ExpectedRevision::new(project_revision.get())?,
            )?;
            let outcome = ProductProjectRemovalOutcome {
                project,
                moved_task_ids,
                moved_conversation_ids,
                already_removed: false,
            };
            let result = record_project_removal_result_on(
                &transaction,
                meta,
                outcome,
                "project_removed",
            )?;
            transaction.commit().map_err(db_err)?;
            Ok(result)
        })
    }

    fn reorder_projects_command(
        &self,
        meta: &ProductCommandMeta,
        entries: &[ProductProjectReorderEntry],
    ) -> ProductResult<ProductCommandResult<ProductProjectReorderOutcome>> {
        validate_project_reorder_entries(entries)?;
        let _mutation = self.lock_mutation()?;
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction().map_err(db_err)?;
            if let Some(result) = load_command_result_on::<ProductProjectReorderOutcome>(
                &transaction,
                meta.idempotency_key.as_str(),
            )? {
                return duplicate_sqlite_command_result(meta, result);
            }
            let active_projects = {
                let mut statement = transaction
                    .prepare(
                        r#"SELECT id, name, workspace_path, pinned, sort_order, archive,
                                  git_workspace_json, settings_json, asset_ids_json, revision
                           FROM projects WHERE archive = 'active'
                           ORDER BY pinned DESC, sort_order ASC, name ASC, id ASC"#,
                    )
                    .map_err(db_err)?;
                let rows = statement.query_map([], map_project_row).map_err(db_err)?;
                let mut projects = Vec::new();
                for row in rows {
                    projects.push(row.map_err(db_err)?);
                }
                projects
            };
            let mut projects = entries
                .iter()
                .map(|entry| {
                    active_projects
                        .iter()
                        .find(|project| project.id == entry.project_id)
                        .cloned()
                        .ok_or_else(|| ProductError::NotFound {
                            entity: "project".into(),
                            id: entry.project_id.as_str().into(),
                        })
                })
                .collect::<ProductResult<Vec<_>>>()?;
            let pinned = projects[0].pinned;
            if projects.iter().any(|project| project.pinned != pinned) {
                return Err(ProductError::InvalidInput {
                    field: "ordered_project_ids".into(),
                    message: "project order must contain active projects from one pinned group"
                        .into(),
                });
            }
            let complete_group = active_projects
                .iter()
                .filter(|project| project.pinned == pinned)
                .collect::<Vec<_>>();
            if complete_group.len() != entries.len()
                || complete_group
                    .iter()
                    .any(|project| !entries.iter().any(|entry| entry.project_id == project.id))
            {
                return Err(ProductError::InvalidInput {
                    field: "ordered_project_ids".into(),
                    message: "project order must contain one complete pinned group".into(),
                });
            }
            for (project, entry) in projects.iter().zip(entries) {
                ensure_expected_revision(entry.expected_revision, project.revision)?;
            }
            let mut event_sequence = None;
            for (sort_order, project) in projects.iter_mut().enumerate() {
                let current_revision = project.revision;
                project.sort_order = sort_order as i64;
                project.revision = current_revision.next();
                let entity = ProductEntity::Project(project.clone());
                update_entity_on(
                    &transaction,
                    &entity,
                    ExpectedRevision::new(current_revision.get())?,
                )?;
                event_sequence = Some(record_product_event_on(
                    &transaction,
                    meta,
                    &entity,
                    "projects_reordered",
                )?);
            }
            let sequence = event_sequence.ok_or_else(|| ProductError::InvalidState {
                message: "project reorder command did not publish an event".into(),
            })?;
            let result = record_project_reorder_result_on(
                &transaction,
                meta,
                ProductProjectReorderOutcome { projects },
                sequence,
            )?;
            transaction.commit().map_err(db_err)?;
            Ok(result)
        })
    }

    fn reorder_tasks_command(
        &self,
        meta: &ProductCommandMeta,
        entries: &[ProductTaskReorderEntry],
    ) -> ProductResult<ProductCommandResult<ProductTaskReorderOutcome>> {
        validate_task_reorder_entries(entries)?;
        let _mutation = self.lock_mutation()?;
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction().map_err(db_err)?;
            if let Some(result) = load_command_result_on::<ProductTaskReorderOutcome>(
                &transaction,
                meta.idempotency_key.as_str(),
            )? {
                return duplicate_sqlite_command_result(meta, result);
            }
            let active_tasks = {
                let mut statement = transaction
                    .prepare(
                        r#"SELECT id, project_id, title, description, status, priority, assignment_id,
                                  completion_criteria_json, milestone_id, workflow_id, agent_profile_id,
                                  blocked_reason, parent_id, pinned, sort_order, archived, tags_json,
                                  created_at, updated_at, revision, legacy_source
                           FROM tasks WHERE archived = 0
                           ORDER BY pinned DESC, sort_order ASC, id ASC"#,
                    )
                    .map_err(db_err)?;
                let rows = statement.query_map([], map_task_row).map_err(db_err)?;
                let mut tasks = Vec::new();
                for row in rows {
                    let mut task = row.map_err(db_err)?;
                    task.depends_on = load_deps(&transaction, task.id.as_str())?;
                    tasks.push(task);
                }
                tasks
            };
            let mut tasks = entries
                .iter()
                .map(|entry| {
                    active_tasks
                        .iter()
                        .find(|task| task.id == entry.task_id)
                        .cloned()
                        .ok_or_else(|| ProductError::NotFound {
                            entity: "task".into(),
                            id: entry.task_id.as_str().into(),
                        })
                })
                .collect::<ProductResult<Vec<_>>>()?;
            let project_id = tasks[0].project_id.clone();
            let pinned = tasks[0].pinned;
            if tasks
                .iter()
                .any(|task| task.project_id != project_id || task.pinned != pinned)
            {
                return Err(ProductError::InvalidInput {
                    field: "ordered_task_ids".into(),
                    message: "task order must contain active tasks from one pinned group".into(),
                });
            }
            let complete_group = active_tasks
                .iter()
                .filter(|task| task.project_id == project_id && task.pinned == pinned)
                .collect::<Vec<_>>();
            if complete_group.len() != entries.len()
                || complete_group
                    .iter()
                    .any(|task| !entries.iter().any(|entry| entry.task_id == task.id))
            {
                return Err(ProductError::InvalidInput {
                    field: "ordered_task_ids".into(),
                    message: "task order must contain one complete pinned group".into(),
                });
            }
            for (task, entry) in tasks.iter().zip(entries) {
                ensure_expected_revision(entry.expected_revision, task.revision)?;
            }
            let mut event_sequence = None;
            for (sort_order, task) in tasks.iter_mut().enumerate() {
                let current_revision = task.revision;
                task.sort_order = sort_order as i64;
                task.revision = current_revision.next();
                let entity = ProductEntity::Task(task.clone());
                update_entity_on(
                    &transaction,
                    &entity,
                    ExpectedRevision::new(current_revision.get())?,
                )?;
                event_sequence = Some(record_product_event_on(
                    &transaction,
                    meta,
                    &entity,
                    "tasks_reordered",
                )?);
            }
            let sequence = event_sequence.ok_or_else(|| ProductError::InvalidState {
                message: "task reorder command did not publish an event".into(),
            })?;
            let result = record_task_reorder_result_on(
                &transaction,
                meta,
                ProductTaskReorderOutcome { tasks },
                sequence,
            )?;
            transaction.commit().map_err(db_err)?;
            Ok(result)
        })
    }

    fn move_task_command(
        &self,
        meta: &ProductCommandMeta,
        input: &ProductTaskMoveInput,
    ) -> ProductResult<ProductCommandResult<ProductTaskMoveOutcome>> {
        let _mutation = self.lock_mutation()?;
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction().map_err(db_err)?;
            if let Some(result) = load_command_result_on::<ProductTaskMoveOutcome>(
                &transaction,
                meta.idempotency_key.as_str(),
            )? {
                return duplicate_sqlite_command_result(meta, result);
            }
            let mut task = transaction
                .query_row(
                    r#"SELECT id, project_id, title, description, status, priority, assignment_id,
                              completion_criteria_json, milestone_id, workflow_id, agent_profile_id,
                              blocked_reason, parent_id, pinned, sort_order, archived, tags_json,
                              created_at, updated_at, revision, legacy_source
                       FROM tasks WHERE id = ?1"#,
                    params![input.task_id.as_str()],
                    map_task_row,
                )
                .optional()
                .map_err(db_err)?
                .ok_or_else(|| ProductError::NotFound {
                    entity: "task".into(),
                    id: input.task_id.as_str().into(),
                })?;
            task.depends_on = load_deps(&transaction, task.id.as_str())?;
            ensure_expected_revision(input.expected_revision, task.revision)?;
            if task.archived {
                return Err(ProductError::InvalidInput {
                    field: "task_id".into(),
                    message: "archived tasks cannot be moved".into(),
                });
            }
            if let Some(project_id) = &input.target_project_id {
                let project = load_project_on(&transaction, project_id)?.ok_or_else(|| {
                    ProductError::NotFound {
                        entity: "project".into(),
                        id: project_id.as_str().into(),
                    }
                })?;
                if project.archive == ProjectArchiveState::Archived {
                    return Err(ProductError::InvalidInput {
                        field: "target_project_id".into(),
                        message: "target project must be active".into(),
                    });
                }
            }
            let tasks = {
                let mut statement = transaction
                    .prepare(
                        r#"SELECT id, project_id, title, description, status, priority, assignment_id,
                                  completion_criteria_json, milestone_id, workflow_id, agent_profile_id,
                                  blocked_reason, parent_id, pinned, sort_order, archived, tags_json,
                                  created_at, updated_at, revision, legacy_source
                           FROM tasks
                           ORDER BY id ASC"#,
                    )
                    .map_err(db_err)?;
                let rows = statement.query_map([], map_task_row).map_err(db_err)?;
                let mut tasks = Vec::new();
                for row in rows {
                    let mut task = row.map_err(db_err)?;
                    task.depends_on = load_deps(&transaction, task.id.as_str())?;
                    tasks.push(task);
                }
                tasks
            };
            validate_task_move_parent(
                &tasks,
                &input.task_id,
                input.target_project_id.as_ref(),
                input.target_parent_id.as_ref(),
            )?;
            if task.project_id == input.target_project_id
                && task.parent_id == input.target_parent_id
            {
                return Err(ProductError::InvalidInput {
                    field: "target_parent_id".into(),
                    message: "task is already at the target location".into(),
                });
            }
            let subtree_ids = task_subtree_ids(&tasks, &input.task_id);
            let location_changed = task.project_id != input.target_project_id;
            let moved_task_ids = if location_changed {
                subtree_ids.clone()
            } else {
                vec![input.task_id.clone()]
            };

            let conversation_payloads = {
                let mut statement = transaction
                    .prepare(
                        "SELECT payload_json FROM product_entities WHERE kind = 'conversation' ORDER BY id ASC",
                    )
                    .map_err(db_err)?;
                let rows = statement
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(db_err)?;
                let mut payloads = Vec::new();
                for row in rows {
                    payloads.push(row.map_err(db_err)?);
                }
                payloads
            };
            let mut moved_conversation_ids = Vec::new();
            for payload in conversation_payloads {
                let ProductEntity::Conversation(mut conversation) =
                    decode_json::<ProductEntity>(&payload).map_err(db_err)?
                else {
                    continue;
                };
                if !conversation
                    .task_id
                    .as_ref()
                    .is_some_and(|task_id| moved_task_ids.contains(task_id))
                    || conversation.project_id == input.target_project_id
                {
                    continue;
                }
                let current_revision = conversation.revision;
                conversation.project_id = input.target_project_id.clone();
                conversation.updated_at = conversation.updated_at.max(input.moved_at);
                conversation.revision = current_revision.next();
                moved_conversation_ids.push(conversation.id.clone());
                let entity = ProductEntity::Conversation(conversation);
                update_entity_on(
                    &transaction,
                    &entity,
                    ExpectedRevision::new(current_revision.get())?,
                )?;
                record_product_event_on(
                    &transaction,
                    meta,
                    &entity,
                    "conversation_moved_with_task",
                )?;
            }

            if location_changed {
                task.sort_order = tasks
                    .iter()
                    .filter(|candidate| {
                        !candidate.archived
                            && !subtree_ids.contains(&candidate.id)
                            && candidate.project_id == input.target_project_id
                    })
                    .map(|candidate| candidate.sort_order)
                    .max()
                    .unwrap_or(-1)
                    .saturating_add(1);
                for descendant_id in subtree_ids.iter().skip(1) {
                    let mut descendant = tasks
                        .iter()
                        .find(|candidate| &candidate.id == descendant_id)
                        .cloned()
                        .expect("task subtree ids come from the task snapshot");
                    let descendant_revision = descendant.revision;
                    descendant.project_id = input.target_project_id.clone();
                    descendant.updated_at = descendant.updated_at.max(input.moved_at);
                    descendant.revision = descendant_revision.next();
                    let entity = ProductEntity::Task(descendant);
                    update_entity_on(
                        &transaction,
                        &entity,
                        ExpectedRevision::new(descendant_revision.get())?,
                    )?;
                    record_product_event_on(
                        &transaction,
                        meta,
                        &entity,
                        "task_moved_with_parent",
                    )?;
                }
            }
            let current_revision = task.revision;
            task.project_id = input.target_project_id.clone();
            task.parent_id = input.target_parent_id.clone();
            task.updated_at = task.updated_at.max(input.moved_at);
            task.revision = current_revision.next();
            let entity = ProductEntity::Task(task.clone());
            update_entity_on(
                &transaction,
                &entity,
                ExpectedRevision::new(current_revision.get())?,
            )?;
            let sequence = record_product_event_on(&transaction, meta, &entity, "task_moved")?;
            let result = record_task_move_result_on(
                &transaction,
                meta,
                ProductTaskMoveOutcome {
                    task,
                    moved_task_ids,
                    moved_conversation_ids,
                },
                sequence,
            )?;
            transaction.commit().map_err(db_err)?;
            Ok(result)
        })
    }

    fn product_events(&self, request: &PageRequest) -> ProductResult<Page<ProductEvent>> {
        self.with_conn(|conn| {
            let after = request.after.unwrap_or(ProductEventSequence::ORIGIN).get() as i64;
            let mut stmt = conn
                .prepare(
                    r#"SELECT sequence, command_id, entity, entity_id, action, revision
                       FROM product_events
                       WHERE sequence > ?1
                       ORDER BY sequence ASC
                       LIMIT ?2"#,
                )
                .map_err(db_err)?;
            let rows = stmt
                .query_map(params![after, request.normalized_limit() as i64], |row| {
                    let sequence: i64 = row.get(0)?;
                    let revision: Option<i64> = row.get(5)?;
                    Ok(ProductEvent {
                        sequence: ProductEventSequence::new(sequence as u64),
                        command_id: row.get(1)?,
                        entity: row.get(2)?,
                        entity_id: row.get(3)?,
                        action: row.get(4)?,
                        revision: revision.map(|value| value as u64),
                    })
                })
                .map_err(db_err)?;
            let mut items = Vec::new();
            for row in rows {
                items.push(row.map_err(db_err)?);
            }
            let next = items.last().map(|event| event.sequence);
            Ok(Page { items, next })
        })
    }

    fn create_project(&self, project: Project) -> ProductResult<Project> {
        let _mutation = self.lock_mutation()?;
        match self.get_project(&project.id) {
            Ok(_) => {
                return Err(ProductError::Conflict {
                    conflict: ConflictKind::DuplicateIdempotency,
                    message: format!("project `{}` already exists", project.id),
                });
            }
            Err(ProductError::NotFound { .. }) => {}
            Err(err) => return Err(err),
        }
        self.upsert_project(&project)?;
        Ok(project)
    }

    fn create_task(&self, task: ProductTask) -> ProductResult<ProductTask> {
        let _mutation = self.lock_mutation()?;
        match self.get_task(&task.id) {
            Ok(_) => {
                return Err(ProductError::Conflict {
                    conflict: ConflictKind::DuplicateIdempotency,
                    message: format!("task `{}` already exists", task.id),
                });
            }
            Err(ProductError::NotFound { .. }) => {}
            Err(err) => return Err(err),
        }
        self.upsert_task(&task)?;
        Ok(task)
    }

    fn accept_task_handoff(
        &self,
        import: ProductTaskHandoffImport,
    ) -> ProductResult<ProductTaskHandoffRecord> {
        let _mutation = self.lock_mutation()?;
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction().map_err(db_err)?;
            if let Some(mut existing) =
                load_task_handoff_by_id_on(&transaction, &import.handoff.id)?
            {
                existing.duplicate = true;
                transaction.commit().map_err(db_err)?;
                return Ok(existing);
            }
            if import.task.project_id.as_ref() != Some(&import.project.id) {
                return Err(ProductError::InvalidInput {
                    field: "task.project_id".into(),
                    message: "handoff task must belong to the imported project".into(),
                });
            }
            let decoded: LiliaCodeTaskHandoff =
                decode_json(&import.payload_json).map_err(db_err)?;
            if decoded != import.handoff {
                return Err(ProductError::InvalidInput {
                    field: "payload_json".into(),
                    message: "handoff payload does not match the parsed contract".into(),
                });
            }
            let project = match load_project_on(&transaction, &import.project.id)? {
                Some(project) => project,
                None => {
                    insert_entity_on(
                        &transaction,
                        &ProductEntity::Project(import.project.clone()),
                    )?;
                    import.project.clone()
                }
            };
            if load_task_on(&transaction, &import.task.id)?.is_some() {
                return Err(ProductError::Conflict {
                    conflict: ConflictKind::DuplicateIdempotency,
                    message: format!("task `{}` already exists", import.task.id),
                });
            }
            insert_entity_on(&transaction, &ProductEntity::Task(import.task.clone()))?;
            transaction
                .execute(
                    "INSERT INTO product_task_handoffs(handoff_id, task_id, payload_json, accepted_at) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        &import.handoff.id,
                        import.task.id.as_str(),
                        &import.payload_json,
                        import.accepted_at,
                    ],
                )
                .map_err(db_err)?;
            let record = ProductTaskHandoffRecord {
                handoff: import.handoff,
                payload_json: import.payload_json,
                project,
                task: import.task,
                accepted_at: import.accepted_at,
                duplicate: false,
            };
            transaction.commit().map_err(db_err)?;
            Ok(record)
        })
    }

    fn task_handoff_for_task(
        &self,
        task_id: &TaskId,
    ) -> ProductResult<Option<ProductTaskHandoffRecord>> {
        self.with_conn(|conn| load_task_handoff_by_task_on(conn, task_id))
    }

    fn update_task_dependencies(
        &self,
        task_id: &TaskId,
        depends_on: Vec<TaskId>,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductTask> {
        let _mutation = self.lock_mutation()?;
        let mut task = self.get_task(task_id)?;
        ensure_expected_revision(expected, task.revision)?;
        let tasks = self.list_tasks()?;
        let mut graph = TaskDependencyGraph::new();
        for candidate in &tasks {
            graph.register_task(
                &candidate.id,
                candidate.project_id.as_ref(),
                &candidate.depends_on,
            );
        }
        task.depends_on =
            graph.validate_dependencies(&task.id, task.project_id.as_ref(), &depends_on)?;
        task.revision = task.revision.next();
        self.upsert_task(&task)?;
        Ok(task)
    }

    fn get_project(&self, project_id: &ProjectId) -> ProductResult<Project> {
        SqliteProductStore::get_project(self, project_id)
    }

    fn list_projects(&self) -> ProductResult<Vec<Project>> {
        SqliteProductStore::list_projects(self)
    }

    fn get_task(&self, task_id: &TaskId) -> ProductResult<ProductTask> {
        SqliteProductStore::get_task(self, task_id)
    }

    fn list_tasks(&self) -> ProductResult<Vec<ProductTask>> {
        SqliteProductStore::list_tasks(self)
    }

    fn record_binding(&self, binding: AgentSessionBinding) -> ProductResult<AgentSessionBinding> {
        let _mutation = self.lock_mutation()?;
        self.get_task(&binding.task_id)?;
        if self
            .list_all_bindings()?
            .iter()
            .any(|existing| existing.binding_id == binding.binding_id)
        {
            return Err(ProductError::Conflict {
                conflict: ConflictKind::DuplicateBinding,
                message: format!("binding `{}` already exists", binding.binding_id),
            });
        }
        self.upsert_binding(&binding)?;
        Ok(binding)
    }

    fn list_bindings_for_task(&self, task_id: &TaskId) -> ProductResult<Vec<AgentSessionBinding>> {
        SqliteProductStore::list_bindings_for_task(self, task_id)
    }

    fn replace_binding_for_task(
        &self,
        binding: AgentSessionBinding,
    ) -> ProductResult<AgentSessionBinding> {
        let _mutation = self.lock_mutation()?;
        self.get_task(&binding.task_id)?;
        self.with_conn(|connection| {
            let transaction = connection.unchecked_transaction().map_err(db_err)?;
            transaction
                .execute(
                    "DELETE FROM agent_session_bindings WHERE task_id = ?1",
                    params![binding.task_id.as_str()],
                )
                .map_err(db_err)?;
            transaction
                .execute(
                    r#"INSERT INTO agent_session_bindings
                       (binding_id, task_id, conversation_id, agent_session, profile_id, revision)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
                    params![
                        binding.binding_id.as_str(),
                        binding.task_id.as_str(),
                        binding
                            .conversation_id
                            .as_ref()
                            .map(|id| id.as_str().to_owned()),
                        binding.agent_session.as_str(),
                        binding.profile_id.as_deref(),
                        binding.revision.get() as i64,
                    ],
                )
                .map_err(db_err)?;
            transaction.commit().map_err(db_err)?;
            Ok(binding)
        })
    }

    fn clear_bindings_for_task(&self, task_id: &TaskId) -> ProductResult<usize> {
        let _mutation = self.lock_mutation()?;
        SqliteProductStore::clear_bindings_for_task(self, task_id)
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

fn load_command_result_on<T: DeserializeOwned>(
    conn: &Connection,
    idempotency_key: &str,
) -> ProductResult<Option<ProductCommandResult<T>>> {
    conn.query_row(
        r#"SELECT command_id, event_sequence, result_json
           FROM product_command_results
           WHERE idempotency_key = ?1"#,
        params![idempotency_key],
        |row| {
            let event_sequence: i64 = row.get(1)?;
            let result_json: String = row.get(2)?;
            Ok(ProductCommandResult {
                command_id: row.get(0)?,
                event_sequence: ProductEventSequence::new(event_sequence as u64),
                value: decode_json(&result_json)?,
                duplicate: false,
            })
        },
    )
    .optional()
    .map_err(db_err)
}

fn record_product_event_fields_on(
    conn: &Connection,
    meta: &ProductCommandMeta,
    kind: ProductEntityKind,
    entity_id: &str,
    action: &str,
    revision: ProductRevision,
) -> ProductResult<ProductEventSequence> {
    conn.execute(
        r#"INSERT INTO product_events(
             command_id, entity, entity_id, action, revision
           ) VALUES (?1, ?2, ?3, ?4, ?5)"#,
        params![
            meta.command_id,
            kind.as_str(),
            entity_id,
            action,
            revision.get() as i64,
        ],
    )
    .map_err(db_err)?;
    Ok(ProductEventSequence::new(conn.last_insert_rowid() as u64))
}

fn record_product_event_on(
    conn: &Connection,
    meta: &ProductCommandMeta,
    entity: &ProductEntity,
    action: &str,
) -> ProductResult<ProductEventSequence> {
    record_product_event_fields_on(
        conn,
        meta,
        entity.kind(),
        entity.id(),
        action,
        entity.revision(),
    )
}

fn record_project_removal_result_on(
    conn: &Connection,
    meta: &ProductCommandMeta,
    value: ProductProjectRemovalOutcome,
    action: &str,
) -> ProductResult<ProductCommandResult<ProductProjectRemovalOutcome>> {
    let sequence = record_product_event_fields_on(
        conn,
        meta,
        ProductEntityKind::Project,
        value.project.id.as_str(),
        action,
        value.project.revision,
    )?;
    conn.execute(
        r#"INSERT INTO product_command_results(
             idempotency_key, command_id, event_sequence, result_json
           ) VALUES (?1, ?2, ?3, ?4)"#,
        params![
            meta.idempotency_key.as_str(),
            meta.command_id,
            sequence.get() as i64,
            encode_json(&value)?,
        ],
    )
    .map_err(db_err)?;
    Ok(ProductCommandResult {
        command_id: meta.command_id.clone(),
        event_sequence: sequence,
        value,
        duplicate: false,
    })
}

fn record_project_reorder_result_on(
    conn: &Connection,
    meta: &ProductCommandMeta,
    value: ProductProjectReorderOutcome,
    sequence: ProductEventSequence,
) -> ProductResult<ProductCommandResult<ProductProjectReorderOutcome>> {
    conn.execute(
        r#"INSERT INTO product_command_results(
             idempotency_key, command_id, event_sequence, result_json
           ) VALUES (?1, ?2, ?3, ?4)"#,
        params![
            meta.idempotency_key.as_str(),
            meta.command_id,
            sequence.get() as i64,
            encode_json(&value)?,
        ],
    )
    .map_err(db_err)?;
    Ok(ProductCommandResult {
        command_id: meta.command_id.clone(),
        event_sequence: sequence,
        value,
        duplicate: false,
    })
}

fn validate_project_reorder_entries(entries: &[ProductProjectReorderEntry]) -> ProductResult<()> {
    if entries.is_empty() {
        return Err(ProductError::InvalidInput {
            field: "ordered_project_ids".into(),
            message: "project order must not be empty".into(),
        });
    }
    if entries.iter().enumerate().any(|(index, entry)| {
        entries[..index]
            .iter()
            .any(|candidate| candidate.project_id == entry.project_id)
    }) {
        return Err(ProductError::InvalidInput {
            field: "ordered_project_ids".into(),
            message: "project order must not contain duplicate ids".into(),
        });
    }
    Ok(())
}

fn record_task_reorder_result_on(
    conn: &Connection,
    meta: &ProductCommandMeta,
    value: ProductTaskReorderOutcome,
    sequence: ProductEventSequence,
) -> ProductResult<ProductCommandResult<ProductTaskReorderOutcome>> {
    conn.execute(
        r#"INSERT INTO product_command_results(
             idempotency_key, command_id, event_sequence, result_json
           ) VALUES (?1, ?2, ?3, ?4)"#,
        params![
            meta.idempotency_key.as_str(),
            meta.command_id,
            sequence.get() as i64,
            encode_json(&value)?,
        ],
    )
    .map_err(db_err)?;
    Ok(ProductCommandResult {
        command_id: meta.command_id.clone(),
        event_sequence: sequence,
        value,
        duplicate: false,
    })
}

fn validate_task_reorder_entries(entries: &[ProductTaskReorderEntry]) -> ProductResult<()> {
    if entries.is_empty() {
        return Err(ProductError::InvalidInput {
            field: "ordered_task_ids".into(),
            message: "task order must not be empty".into(),
        });
    }
    if entries.iter().enumerate().any(|(index, entry)| {
        entries[..index]
            .iter()
            .any(|candidate| candidate.task_id == entry.task_id)
    }) {
        return Err(ProductError::InvalidInput {
            field: "ordered_task_ids".into(),
            message: "task order must not contain duplicate ids".into(),
        });
    }
    Ok(())
}

fn record_task_move_result_on(
    conn: &Connection,
    meta: &ProductCommandMeta,
    value: ProductTaskMoveOutcome,
    sequence: ProductEventSequence,
) -> ProductResult<ProductCommandResult<ProductTaskMoveOutcome>> {
    conn.execute(
        r#"INSERT INTO product_command_results(
             idempotency_key, command_id, event_sequence, result_json
           ) VALUES (?1, ?2, ?3, ?4)"#,
        params![
            meta.idempotency_key.as_str(),
            meta.command_id,
            sequence.get() as i64,
            encode_json(&value)?,
        ],
    )
    .map_err(db_err)?;
    Ok(ProductCommandResult {
        command_id: meta.command_id.clone(),
        event_sequence: sequence,
        value,
        duplicate: false,
    })
}

fn validate_task_move_parent(
    tasks: &[ProductTask],
    task_id: &TaskId,
    target_project_id: Option<&ProjectId>,
    target_parent_id: Option<&TaskId>,
) -> ProductResult<()> {
    let mut cursor = target_parent_id.cloned();
    let mut visited = Vec::new();
    while let Some(parent_id) = cursor {
        if &parent_id == task_id || visited.contains(&parent_id) {
            return Err(ProductError::InvalidInput {
                field: "target_parent_id".into(),
                message: "task parent would create a cycle".into(),
            });
        }
        let parent = tasks
            .iter()
            .find(|task| task.id == parent_id)
            .ok_or_else(|| ProductError::NotFound {
                entity: "task".into(),
                id: parent_id.as_str().into(),
            })?;
        if parent.archived || parent.project_id.as_ref() != target_project_id {
            return Err(ProductError::InvalidInput {
                field: "target_parent_id".into(),
                message: "task parent must be active in the target project".into(),
            });
        }
        visited.push(parent_id);
        cursor = parent.parent_id.clone();
    }
    Ok(())
}

fn task_subtree_ids(tasks: &[ProductTask], root_id: &TaskId) -> Vec<TaskId> {
    let mut ids = vec![root_id.clone()];
    loop {
        let mut added = tasks
            .iter()
            .filter(|task| {
                !ids.contains(&task.id)
                    && task
                        .parent_id
                        .as_ref()
                        .is_some_and(|parent_id| ids.contains(parent_id))
            })
            .map(|task| task.id.clone())
            .collect::<Vec<_>>();
        if added.is_empty() {
            break;
        }
        added.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids.extend(added);
    }
    ids
}

fn record_command_result_on(
    conn: &Connection,
    meta: &ProductCommandMeta,
    value: ProductEntity,
    action: &str,
) -> ProductResult<ProductCommandResult<ProductEntity>> {
    let result_json = encode_json(&value)?;
    conn.execute(
        r#"INSERT INTO product_events(
             command_id, entity, entity_id, action, revision
           ) VALUES (?1, ?2, ?3, ?4, ?5)"#,
        params![
            meta.command_id,
            value.kind().as_str(),
            value.id(),
            action,
            value.revision().get() as i64,
        ],
    )
    .map_err(db_err)?;
    let sequence = conn.last_insert_rowid() as u64;
    conn.execute(
        r#"INSERT INTO product_command_results(
             idempotency_key, command_id, event_sequence, result_json
           ) VALUES (?1, ?2, ?3, ?4)"#,
        params![
            meta.idempotency_key.as_str(),
            meta.command_id,
            sequence as i64,
            result_json,
        ],
    )
    .map_err(db_err)?;
    Ok(ProductCommandResult {
        command_id: meta.command_id.clone(),
        event_sequence: ProductEventSequence::new(sequence),
        value,
        duplicate: false,
    })
}

fn insert_entity_on(conn: &Connection, entity: &ProductEntity) -> ProductResult<()> {
    let result = match entity {
        ProductEntity::Project(project) => {
            let git_workspace_json = encode_json(&project.git_workspace)?;
            let settings_json = encode_json(&project.settings)?;
            let asset_ids_json = encode_json(&project.asset_ids)?;
            conn.execute(
                r#"INSERT INTO projects(
                     id, name, workspace_path, pinned, sort_order, archive,
                     git_workspace_json, settings_json, asset_ids_json, revision
                   ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
                params![
                    project.id.as_str(),
                    project.name,
                    project.workspace_path,
                    if project.pinned { 1 } else { 0 },
                    project.sort_order,
                    archive_to_str(project.archive),
                    git_workspace_json,
                    settings_json,
                    asset_ids_json,
                    project.revision.get() as i64,
                ],
            )
        }
        ProductEntity::Task(task) => {
            let completion_criteria_json = encode_json(&task.completion_criteria)?;
            let tags_json = encode_json(&task.tags)?;
            let changed = conn
                .execute(
                    r#"INSERT INTO tasks(
                     id, project_id, title, description, status, priority, assignment_id,
                     completion_criteria_json, milestone_id, workflow_id, agent_profile_id,
                     blocked_reason, parent_id, pinned, sort_order, archived, tags_json,
                     created_at, updated_at, revision, legacy_source
                   ) VALUES (
                     ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                     ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
                   )"#,
                    params![
                        task.id.as_str(),
                        task.project_id.as_ref().map(|id| id.as_str()),
                        task.title,
                        task.description,
                        status_to_str(task.status),
                        priority_to_str(task.priority),
                        task.assignment_id.as_ref().map(|id| id.as_str()),
                        completion_criteria_json,
                        task.milestone_id.as_ref().map(|id| id.as_str()),
                        task.workflow_id.as_ref().map(|id| id.as_str()),
                        task.agent_profile_id,
                        task.blocked_reason,
                        task.parent_id.as_ref().map(|id| id.as_str()),
                        if task.pinned { 1 } else { 0 },
                        task.sort_order,
                        if task.archived { 1 } else { 0 },
                        tags_json,
                        task.created_at,
                        task.updated_at,
                        task.revision.get() as i64,
                        task.legacy_source,
                    ],
                )
                .map_err(db_err)?;
            insert_task_dependencies_on(conn, task)?;
            return if changed == 1 {
                Ok(())
            } else {
                Err(ProductError::Unavailable {
                    message: "product task insert did not affect a row".into(),
                })
            };
        }
        ProductEntity::Binding(binding) => conn.execute(
            r#"INSERT INTO agent_session_bindings(
                 binding_id, task_id, conversation_id, agent_session, profile_id, revision
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
            params![
                binding.binding_id.as_str(),
                binding.task_id.as_str(),
                binding.conversation_id.as_ref().map(|id| id.as_str()),
                binding.agent_session.as_str(),
                binding.profile_id,
                binding.revision.get() as i64,
            ],
        ),
        entity => {
            let payload_json = encode_json(entity)?;
            conn.execute(
                r#"INSERT INTO product_entities(kind, id, payload_json, revision)
                   VALUES (?1, ?2, ?3, ?4)"#,
                params![
                    entity.kind().as_str(),
                    entity.id(),
                    payload_json,
                    entity.revision().get() as i64,
                ],
            )
        }
    };
    match result {
        Ok(1) => Ok(()),
        Ok(_) => Err(ProductError::Unavailable {
            message: format!(
                "product {} insert did not affect a row",
                entity.kind().as_str()
            ),
        }),
        Err(err) if is_constraint_violation(&err) => Err(ProductError::Conflict {
            conflict: if entity.kind() == ProductEntityKind::Binding {
                ConflictKind::DuplicateBinding
            } else {
                ConflictKind::DuplicateIdempotency
            },
            message: format!(
                "{} `{}` already exists or references a missing entity",
                entity.kind().as_str(),
                entity.id()
            ),
        }),
        Err(err) => Err(db_err(err)),
    }
}

fn entity_revision_on(
    conn: &Connection,
    kind: ProductEntityKind,
    id: &str,
) -> ProductResult<ProductRevision> {
    let (table, id_column) = match kind {
        ProductEntityKind::Project => ("projects", "id"),
        ProductEntityKind::Task => ("tasks", "id"),
        ProductEntityKind::Binding => ("agent_session_bindings", "binding_id"),
        _ => ("product_entities", "id"),
    };
    let sql = if table == "product_entities" {
        format!("SELECT revision FROM {table} WHERE kind = ?1 AND {id_column} = ?2")
    } else {
        format!("SELECT revision FROM {table} WHERE {id_column} = ?1")
    };
    let revision: Option<i64> = if table == "product_entities" {
        conn.query_row(&sql, params![kind.as_str(), id], |row| row.get(0))
            .optional()
            .map_err(db_err)?
    } else {
        conn.query_row(&sql, params![id], |row| row.get(0))
            .optional()
            .map_err(db_err)?
    };
    let revision = revision.ok_or_else(|| ProductError::NotFound {
        entity: kind.as_str().into(),
        id: id.into(),
    })?;
    ProductRevision::new(revision as u64)
}

fn update_entity_on(
    conn: &Connection,
    entity: &ProductEntity,
    expected: ExpectedRevision,
) -> ProductResult<()> {
    let changed = match entity {
        ProductEntity::Project(project) => {
            let git_workspace_json = encode_json(&project.git_workspace)?;
            let settings_json = encode_json(&project.settings)?;
            let asset_ids_json = encode_json(&project.asset_ids)?;
            conn.execute(
                r#"UPDATE projects SET
                     name = ?1,
                     workspace_path = ?2,
                     pinned = ?3,
                     sort_order = ?4,
                     archive = ?5,
                     git_workspace_json = ?6,
                     settings_json = ?7,
                     asset_ids_json = ?8,
                     revision = ?9
                   WHERE id = ?10 AND revision = ?11"#,
                params![
                    project.name,
                    project.workspace_path,
                    if project.pinned { 1 } else { 0 },
                    project.sort_order,
                    archive_to_str(project.archive),
                    git_workspace_json,
                    settings_json,
                    asset_ids_json,
                    project.revision.get() as i64,
                    project.id.as_str(),
                    expected.get() as i64,
                ],
            )
        }
        ProductEntity::Task(task) => {
            let completion_criteria_json = encode_json(&task.completion_criteria)?;
            let tags_json = encode_json(&task.tags)?;
            let changed = conn
                .execute(
                    r#"UPDATE tasks SET
                     project_id = ?1,
                     title = ?2,
                     description = ?3,
                     status = ?4,
                     priority = ?5,
                     assignment_id = ?6,
                     completion_criteria_json = ?7,
                     milestone_id = ?8,
                     workflow_id = ?9,
                     agent_profile_id = ?10,
                     blocked_reason = ?11,
                     parent_id = ?12,
                     pinned = ?13,
                     sort_order = ?14,
                     archived = ?15,
                     tags_json = ?16,
                     created_at = ?17,
                     updated_at = ?18,
                     revision = ?19,
                     legacy_source = ?20
                   WHERE id = ?21 AND revision = ?22"#,
                    params![
                        task.project_id.as_ref().map(|id| id.as_str()),
                        task.title,
                        task.description,
                        status_to_str(task.status),
                        priority_to_str(task.priority),
                        task.assignment_id.as_ref().map(|id| id.as_str()),
                        completion_criteria_json,
                        task.milestone_id.as_ref().map(|id| id.as_str()),
                        task.workflow_id.as_ref().map(|id| id.as_str()),
                        task.agent_profile_id,
                        task.blocked_reason,
                        task.parent_id.as_ref().map(|id| id.as_str()),
                        if task.pinned { 1 } else { 0 },
                        task.sort_order,
                        if task.archived { 1 } else { 0 },
                        tags_json,
                        task.created_at,
                        task.updated_at,
                        task.revision.get() as i64,
                        task.legacy_source,
                        task.id.as_str(),
                        expected.get() as i64,
                    ],
                )
                .map_err(db_err)?;
            if changed == 1 {
                conn.execute(
                    "DELETE FROM task_dependencies WHERE task_id = ?1",
                    params![task.id.as_str()],
                )
                .map_err(db_err)?;
                insert_task_dependencies_on(conn, task)?;
            }
            return match changed {
                1 => Ok(()),
                _ => Err(ProductError::Unavailable {
                    message: "product task update did not affect a row".into(),
                }),
            };
        }
        ProductEntity::Binding(binding) => conn.execute(
            r#"UPDATE agent_session_bindings SET
                 task_id = ?1,
                 conversation_id = ?2,
                 agent_session = ?3,
                 profile_id = ?4,
                 revision = ?5
               WHERE binding_id = ?6 AND revision = ?7"#,
            params![
                binding.task_id.as_str(),
                binding.conversation_id.as_ref().map(|id| id.as_str()),
                binding.agent_session.as_str(),
                binding.profile_id,
                binding.revision.get() as i64,
                binding.binding_id.as_str(),
                expected.get() as i64,
            ],
        ),
        entity => {
            let payload_json = encode_json(entity)?;
            conn.execute(
                r#"UPDATE product_entities
                   SET payload_json = ?1, revision = ?2
                   WHERE kind = ?3 AND id = ?4 AND revision = ?5"#,
                params![
                    payload_json,
                    entity.revision().get() as i64,
                    entity.kind().as_str(),
                    entity.id(),
                    expected.get() as i64,
                ],
            )
        }
    }
    .map_err(db_err)?;
    if changed == 1 {
        Ok(())
    } else {
        Err(ProductError::Unavailable {
            message: format!(
                "product {} update did not affect a row",
                entity.kind().as_str()
            ),
        })
    }
}

fn insert_task_dependencies_on(conn: &Connection, task: &ProductTask) -> ProductResult<()> {
    for dependency in &task.depends_on {
        conn.execute(
            "INSERT INTO task_dependencies(task_id, depends_on_id) VALUES (?1, ?2)",
            params![task.id.as_str(), dependency.as_str()],
        )
        .map_err(db_err)?;
    }
    Ok(())
}

fn db_err(err: rusqlite::Error) -> ProductError {
    ProductError::Unavailable {
        message: format!("sqlite product: {err}"),
    }
}

fn is_constraint_violation(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn duplicate_sqlite_command_result<T>(
    meta: &ProductCommandMeta,
    mut result: ProductCommandResult<T>,
) -> ProductResult<ProductCommandResult<T>> {
    if result.command_id != meta.command_id {
        return Err(ProductError::Conflict {
            conflict: ConflictKind::DuplicateIdempotency,
            message: "idempotency key was already used by another command".into(),
        });
    }
    result.duplicate = true;
    Ok(result)
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

fn priority_to_str(priority: ProductTaskPriority) -> &'static str {
    match priority {
        ProductTaskPriority::Low => "low",
        ProductTaskPriority::Normal => "normal",
        ProductTaskPriority::High => "high",
        ProductTaskPriority::Urgent => "urgent",
    }
}

fn priority_from_str(value: &str) -> ProductTaskPriority {
    match value {
        "low" => ProductTaskPriority::Low,
        "high" => ProductTaskPriority::High,
        "urgent" => ProductTaskPriority::Urgent,
        _ => ProductTaskPriority::Normal,
    }
}

fn load_project_on(conn: &Connection, id: &ProjectId) -> ProductResult<Option<Project>> {
    conn.query_row(
        r#"SELECT id, name, workspace_path, pinned, sort_order, archive,
                  git_workspace_json, settings_json, asset_ids_json, revision
           FROM projects WHERE id = ?1"#,
        params![id.as_str()],
        map_project_row,
    )
    .optional()
    .map_err(db_err)
}

fn load_task_on(conn: &Connection, id: &TaskId) -> ProductResult<Option<ProductTask>> {
    let task = conn
        .query_row(
            r#"SELECT id, project_id, title, description, status, priority, assignment_id,
                      completion_criteria_json, milestone_id, workflow_id, agent_profile_id,
                      blocked_reason, parent_id, pinned, sort_order, archived, tags_json,
                      created_at, updated_at, revision, legacy_source
               FROM tasks WHERE id = ?1"#,
            params![id.as_str()],
            map_task_row,
        )
        .optional()
        .map_err(db_err)?;
    task.map(|mut task| {
        task.depends_on = load_deps(conn, id.as_str())?;
        Ok(task)
    })
    .transpose()
}

fn load_task_handoff_by_id_on(
    conn: &Connection,
    handoff_id: &str,
) -> ProductResult<Option<ProductTaskHandoffRecord>> {
    load_task_handoff_on(
        conn,
        "SELECT task_id, payload_json, accepted_at FROM product_task_handoffs WHERE handoff_id = ?1",
        handoff_id,
    )
}

fn load_task_handoff_by_task_on(
    conn: &Connection,
    task_id: &TaskId,
) -> ProductResult<Option<ProductTaskHandoffRecord>> {
    load_task_handoff_on(
        conn,
        "SELECT task_id, payload_json, accepted_at FROM product_task_handoffs WHERE task_id = ?1",
        task_id.as_str(),
    )
}

fn load_task_handoff_on(
    conn: &Connection,
    query: &str,
    value: &str,
) -> ProductResult<Option<ProductTaskHandoffRecord>> {
    let row = conn
        .query_row(query, params![value], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .optional()
        .map_err(db_err)?;
    let Some((task_id, payload_json, accepted_at)) = row else {
        return Ok(None);
    };
    let task_id = TaskId::new(task_id)?;
    let task = load_task_on(conn, &task_id)?.ok_or_else(|| ProductError::InvalidState {
        message: format!("handoff references missing task `{task_id}`"),
    })?;
    let project_id = task
        .project_id
        .as_ref()
        .ok_or_else(|| ProductError::InvalidState {
            message: format!("handoff task `{task_id}` has no project"),
        })?;
    let project = load_project_on(conn, project_id)?.ok_or_else(|| ProductError::InvalidState {
        message: format!("handoff task `{task_id}` references missing project `{project_id}`"),
    })?;
    let handoff: LiliaCodeTaskHandoff = decode_json(&payload_json).map_err(db_err)?;
    Ok(Some(ProductTaskHandoffRecord {
        handoff,
        payload_json,
        project,
        task,
        accepted_at,
        duplicate: false,
    }))
}

fn map_project_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    let archive: String = row.get(5)?;
    let git_workspace_json: Option<String> = row.get(6)?;
    let settings_json: String = row.get(7)?;
    let asset_ids_json: String = row.get(8)?;
    Ok(Project {
        id: ProjectId::new(row.get::<_, String>(0)?).map_err(invalid_id)?,
        name: row.get(1)?,
        workspace_path: row.get(2)?,
        pinned: row.get::<_, i64>(3)? != 0,
        sort_order: row.get(4)?,
        archive: archive_from_str(&archive),
        git_workspace: git_workspace_json
            .as_deref()
            .map(decode_json)
            .transpose()?
            .flatten(),
        settings: decode_json(&settings_json)?,
        asset_ids: decode_json(&asset_ids_json)?,
        revision: ProductRevision::new(row.get::<_, i64>(9)? as u64).map_err(invalid_id)?,
    })
}

fn map_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProductTask> {
    let project_id: Option<String> = row.get(1)?;
    let status: String = row.get(4)?;
    let priority: String = row.get(5)?;
    let assignment_id: Option<String> = row.get(6)?;
    let completion_criteria_json: String = row.get(7)?;
    let milestone_id: Option<String> = row.get(8)?;
    let workflow_id: Option<String> = row.get(9)?;
    let parent_id: Option<String> = row.get(12)?;
    let tags_json: String = row.get(16)?;
    Ok(ProductTask {
        id: TaskId::new(row.get::<_, String>(0)?).map_err(invalid_id)?,
        project_id: project_id
            .map(ProjectId::new)
            .transpose()
            .map_err(invalid_id)?,
        title: row.get(2)?,
        description: row.get(3)?,
        status: status_from_str(&status),
        priority: priority_from_str(&priority),
        assignment_id: assignment_id
            .map(AssignmentId::new)
            .transpose()
            .map_err(invalid_id)?,
        completion_criteria: decode_json(&completion_criteria_json)?,
        milestone_id: milestone_id
            .map(MilestoneId::new)
            .transpose()
            .map_err(invalid_id)?,
        workflow_id: workflow_id
            .map(WorkflowId::new)
            .transpose()
            .map_err(invalid_id)?,
        agent_profile_id: row.get(10)?,
        blocked_reason: row.get(11)?,
        depends_on: Vec::new(),
        parent_id: parent_id.map(TaskId::new).transpose().map_err(invalid_id)?,
        pinned: row.get::<_, i64>(13)? != 0,
        sort_order: row.get(14)?,
        archived: row.get::<_, i64>(15)? != 0,
        tags: decode_json(&tags_json)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
        revision: ProductRevision::new(row.get::<_, i64>(19)? as u64).map_err(invalid_id)?,
        legacy_source: row.get(20)?,
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
        .prepare(
            "SELECT depends_on_id FROM task_dependencies WHERE task_id = ?1 ORDER BY depends_on_id",
        )
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

fn encode_json(value: &impl Serialize) -> ProductResult<String> {
    serde_json::to_string(value).map_err(|err| ProductError::Unavailable {
        message: format!("encode product data: {err}"),
    })
}

fn decode_json<T: DeserializeOwned>(value: &str) -> rusqlite::Result<T> {
    serde_json::from_str(value).map_err(invalid_id)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use super::*;
    use lilia_contracts::{
        GitWorkspaceRef, IdempotencyKey, LiliaCodeTaskHandoff, ProductConversation, ProductEntity,
        ProjectAssetId, ProjectSettings,
    };

    fn handoff_import(
        handoff_id: &str,
        project_id: &str,
        task_id: &str,
    ) -> ProductTaskHandoffImport {
        let handoff: LiliaCodeTaskHandoff = serde_json::from_value(serde_json::json!({
            "protocol": "lilia-code-task-handoff",
            "version": 1,
            "id": handoff_id,
            "createdAt": "2026-08-10T00:00:00Z",
            "title": "Fix CI",
            "kind": "workflowFailure",
            "repository": {
                "fullName": "acme/widget",
                "worktreePath": "C:/work/widget",
                "branch": "fix/ci"
            },
            "source": {
                "application": "LiliaGithub",
                "route": "/workflow/77"
            },
            "problem": "CI failed",
            "relatedFiles": [],
            "logSummary": "typecheck failed",
            "acceptanceCriteria": ["CI passes"],
            "workflow": {
                "runId": 77,
                "runUrl": "https://github.com/acme/widget/actions/runs/77",
                "workflowName": "verify"
            }
        }))
        .unwrap();
        let payload_json = serde_json::to_string(&handoff).unwrap();
        let project = Project::new(ProjectId::new(project_id).unwrap(), "Widget").unwrap();
        let task = ProductTask::new(
            TaskId::new(task_id).unwrap(),
            Some(project.id.clone()),
            "Fix CI",
        )
        .unwrap();
        ProductTaskHandoffImport {
            handoff,
            payload_json,
            project,
            task,
            accepted_at: 42,
        }
    }

    #[test]
    fn task_handoff_acceptance_is_atomic_and_idempotent() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let first = store
            .accept_task_handoff(handoff_import("handoff-1", "project-1", "task-1"))
            .unwrap();
        let duplicate = store
            .accept_task_handoff(handoff_import("handoff-1", "project-2", "task-2"))
            .unwrap();

        assert!(!first.duplicate);
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.project.id, first.project.id);
        assert_eq!(duplicate.task.id, first.task.id);
        assert_eq!(
            store
                .task_handoff_for_task(&first.task.id)
                .unwrap()
                .unwrap()
                .handoff
                .id,
            "handoff-1"
        );
        assert_eq!(store.list_projects().unwrap().len(), 1);
        assert_eq!(store.list_tasks().unwrap().len(), 1);

        let conflicting = handoff_import("handoff-2", "project-rollback", "task-1");
        assert!(store.accept_task_handoff(conflicting).is_err());
        assert!(store
            .get_project(&ProjectId::new("project-rollback").unwrap())
            .is_err());
        assert_eq!(store.list_projects().unwrap().len(), 1);
        assert_eq!(store.list_tasks().unwrap().len(), 1);
    }

    fn remove_project_fixture(store: &SqliteProductStore, project_id: &str) -> Project {
        let mut project = Project::new(ProjectId::new(project_id).unwrap(), "Removal").unwrap();
        project.workspace_path = Some("C:/workspace/removal".to_owned());
        store
            .create_entity(ProductEntity::Project(project.clone()))
            .unwrap();
        let active = ProductTask::new(
            TaskId::new(format!("{project_id}-active")).unwrap(),
            Some(project.id.clone()),
            "Active",
        )
        .unwrap();
        store
            .create_entity(ProductEntity::Task(active.clone()))
            .unwrap();
        let mut archived = ProductTask::new(
            TaskId::new(format!("{project_id}-archived")).unwrap(),
            Some(project.id.clone()),
            "Archived",
        )
        .unwrap();
        archived.archived = true;
        store
            .create_entity(ProductEntity::Task(archived.clone()))
            .unwrap();
        let active_conversation = ProductConversation::new(
            ConversationId::new(format!("conversation:{project_id}:active")).unwrap(),
            Some(project.id.clone()),
            Some(active.id),
            "Active",
        )
        .unwrap();
        store
            .create_entity(ProductEntity::Conversation(active_conversation))
            .unwrap();
        let mut archived_conversation = ProductConversation::new(
            ConversationId::new(format!("conversation:{project_id}:archived")).unwrap(),
            Some(project.id.clone()),
            Some(archived.id),
            "Archived",
        )
        .unwrap();
        archived_conversation.archived = true;
        store
            .create_entity(ProductEntity::Conversation(archived_conversation))
            .unwrap();
        project
    }

    fn remove_meta(project: &Project, command: &str, key: &str) -> ProductCommandMeta {
        ProductCommandMeta::update(
            command,
            IdempotencyKey::new(key).unwrap(),
            ExpectedRevision::new(project.revision.get()).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn project_removal_is_one_sqlite_transaction_and_exactly_idempotent() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let project = remove_project_fixture(&store, "project-remove");
        let meta = remove_meta(&project, "remove-project", "remove-project-key");

        let first = store
            .remove_project_command(&meta, &project.id, 99)
            .unwrap();
        assert!(!first.duplicate);
        assert_eq!(first.value.moved_task_ids.len(), 1);
        assert_eq!(first.value.moved_conversation_ids.len(), 1);
        assert_eq!(first.value.project.workspace_path, project.workspace_path);
        assert_eq!(first.value.project.archive, ProjectArchiveState::Archived);
        let active = store
            .get_task(&TaskId::new("project-remove-active").unwrap())
            .unwrap();
        let archived = store
            .get_task(&TaskId::new("project-remove-archived").unwrap())
            .unwrap();
        assert_eq!(active.project_id, None);
        assert_eq!(active.updated_at, 99);
        assert_eq!(active.revision.get(), 2);
        assert_eq!(archived.project_id, Some(project.id.clone()));
        let conversations = store
            .list_entities(ProductEntityKind::Conversation)
            .unwrap();
        assert!(conversations.iter().all(|entity| match entity {
            ProductEntity::Conversation(conversation) if conversation.archived => {
                conversation.project_id.as_ref() == Some(&project.id)
            }
            ProductEntity::Conversation(conversation) => conversation.project_id.is_none(),
            _ => false,
        }));
        let event_count = store
            .product_events(&PageRequest {
                after: None,
                limit: 100,
            })
            .unwrap()
            .items
            .len();
        assert_eq!(event_count, 3);

        let duplicate = store
            .remove_project_command(&meta, &project.id, 100)
            .unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.value, first.value);
        assert_eq!(
            store
                .product_events(&PageRequest {
                    after: None,
                    limit: 100,
                })
                .unwrap()
                .items
                .len(),
            event_count
        );
        let conflict = remove_meta(&project, "other-command", "remove-project-key");
        assert!(matches!(
            store.remove_project_command(&conflict, &project.id, 100),
            Err(ProductError::Conflict {
                conflict: ConflictKind::DuplicateIdempotency,
                ..
            })
        ));
    }

    #[test]
    fn project_removal_rolls_back_conversations_when_a_task_write_fails() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let project = remove_project_fixture(&store, "project-rollback");
        store
            .with_conn(|conn| {
                conn.execute_batch(
                    r#"CREATE TRIGGER fail_project_removal_task
                       BEFORE UPDATE ON tasks
                       WHEN OLD.project_id = 'project-rollback'
                       BEGIN
                         SELECT RAISE(ABORT, 'injected task failure');
                       END;"#,
                )
                .map_err(db_err)
            })
            .unwrap();
        let meta = remove_meta(&project, "remove-rollback", "remove-rollback-key");

        assert!(store
            .remove_project_command(&meta, &project.id, 99)
            .is_err());
        assert_eq!(
            store.get_project(&project.id).unwrap().archive,
            ProjectArchiveState::Active
        );
        assert_eq!(
            store
                .get_task(&TaskId::new("project-rollback-active").unwrap())
                .unwrap()
                .project_id,
            Some(project.id.clone())
        );
        assert!(store
            .list_entities(ProductEntityKind::Conversation)
            .unwrap()
            .iter()
            .all(|entity| matches!(
                entity,
                ProductEntity::Conversation(conversation)
                    if conversation.project_id.as_ref() == Some(&project.id)
            )));
        assert!(store
            .product_events(&PageRequest {
                after: None,
                limit: 100,
            })
            .unwrap()
            .items
            .is_empty());
        let result_count = store
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM product_command_results", [], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(db_err)
            })
            .unwrap();
        assert_eq!(result_count, 0);
    }

    fn project_reorder_fixture(store: &SqliteProductStore) -> Vec<Project> {
        ["a", "b", "c"]
            .into_iter()
            .enumerate()
            .map(|(sort_order, suffix)| {
                let mut project = Project::new(
                    ProjectId::new(format!("project-order-{suffix}")).unwrap(),
                    suffix.to_uppercase(),
                )
                .unwrap();
                project.sort_order = sort_order as i64;
                store
                    .create_entity(ProductEntity::Project(project.clone()))
                    .unwrap();
                project
            })
            .collect()
    }

    fn project_reorder_entries(projects: &[Project]) -> Vec<ProductProjectReorderEntry> {
        projects
            .iter()
            .rev()
            .map(|project| ProductProjectReorderEntry {
                project_id: project.id.clone(),
                expected_revision: ExpectedRevision::new(project.revision.get()).unwrap(),
            })
            .collect()
    }

    fn project_reorder_meta(command: &str, key: &str) -> ProductCommandMeta {
        ProductCommandMeta::create(command, IdempotencyKey::new(key).unwrap()).unwrap()
    }

    #[test]
    fn project_reorder_is_one_sqlite_transaction_and_exactly_idempotent() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let projects = project_reorder_fixture(&store);
        let entries = project_reorder_entries(&projects);
        let meta = project_reorder_meta("reorder-projects", "reorder-projects-key");

        let first = store.reorder_projects_command(&meta, &entries).unwrap();
        assert!(!first.duplicate);
        assert_eq!(
            first
                .value
                .projects
                .iter()
                .map(|project| project.id.as_str())
                .collect::<Vec<_>>(),
            vec!["project-order-c", "project-order-b", "project-order-a"]
        );
        assert!(first
            .value
            .projects
            .iter()
            .enumerate()
            .all(|(index, project)| project.sort_order == index as i64
                && project.revision.get() == 2));
        assert_eq!(
            store
                .list_projects()
                .unwrap()
                .into_iter()
                .map(|project| project.id.as_str().to_owned())
                .collect::<Vec<_>>(),
            vec![
                "project-order-c".to_owned(),
                "project-order-b".to_owned(),
                "project-order-a".to_owned(),
            ]
        );
        let event_count = store
            .product_events(&PageRequest {
                after: None,
                limit: 100,
            })
            .unwrap()
            .items
            .len();
        assert_eq!(event_count, 3);

        let duplicate = store.reorder_projects_command(&meta, &entries).unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.value, first.value);
        assert_eq!(
            store
                .product_events(&PageRequest {
                    after: None,
                    limit: 100,
                })
                .unwrap()
                .items
                .len(),
            event_count
        );
        let conflict = project_reorder_meta("other-command", "reorder-projects-key");
        assert!(matches!(
            store.reorder_projects_command(&conflict, &entries),
            Err(ProductError::Conflict {
                conflict: ConflictKind::DuplicateIdempotency,
                ..
            })
        ));
    }

    #[test]
    fn project_reorder_rolls_back_earlier_projects_when_a_later_write_fails() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let projects = project_reorder_fixture(&store);
        store
            .with_conn(|conn| {
                conn.execute_batch(
                    r#"CREATE TRIGGER fail_project_reorder
                       BEFORE UPDATE ON projects
                       WHEN OLD.id = 'project-order-b'
                       BEGIN
                         SELECT RAISE(ABORT, 'injected project reorder failure');
                       END;"#,
                )
                .map_err(db_err)
            })
            .unwrap();
        let entries = project_reorder_entries(&projects);
        let meta = project_reorder_meta("reorder-rollback", "reorder-rollback-key");

        assert!(store.reorder_projects_command(&meta, &entries).is_err());
        assert!(store
            .list_projects()
            .unwrap()
            .iter()
            .enumerate()
            .all(|(index, project)| project.sort_order == index as i64
                && project.revision == ProductRevision::INITIAL));
        assert!(store
            .product_events(&PageRequest {
                after: None,
                limit: 100,
            })
            .unwrap()
            .items
            .is_empty());
        let result_count = store
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM product_command_results", [], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(db_err)
            })
            .unwrap();
        assert_eq!(result_count, 0);
    }

    fn task_reorder_fixture(store: &SqliteProductStore) -> Vec<ProductTask> {
        let project = Project::new(ProjectId::new("task-order-project").unwrap(), "Tasks").unwrap();
        store
            .create_entity(ProductEntity::Project(project.clone()))
            .unwrap();
        ["a", "b", "c"]
            .into_iter()
            .enumerate()
            .map(|(sort_order, suffix)| {
                let mut task = ProductTask::new(
                    TaskId::new(format!("task-order-{suffix}")).unwrap(),
                    Some(project.id.clone()),
                    suffix.to_uppercase(),
                )
                .unwrap();
                task.sort_order = sort_order as i64;
                store
                    .create_entity(ProductEntity::Task(task.clone()))
                    .unwrap();
                task
            })
            .collect()
    }

    fn task_reorder_entries(tasks: &[ProductTask]) -> Vec<ProductTaskReorderEntry> {
        tasks
            .iter()
            .rev()
            .map(|task| ProductTaskReorderEntry {
                task_id: task.id.clone(),
                expected_revision: ExpectedRevision::new(task.revision.get()).unwrap(),
            })
            .collect()
    }

    #[test]
    fn task_reorder_is_one_sqlite_transaction_and_exactly_idempotent() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let tasks = task_reorder_fixture(&store);
        let entries = task_reorder_entries(&tasks);
        let meta = project_reorder_meta("reorder-tasks", "reorder-tasks-key");

        let first = store.reorder_tasks_command(&meta, &entries).unwrap();
        assert!(!first.duplicate);
        assert_eq!(
            first
                .value
                .tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["task-order-c", "task-order-b", "task-order-a"]
        );
        assert!(first
            .value
            .tasks
            .iter()
            .enumerate()
            .all(|(index, task)| task.sort_order == index as i64 && task.revision.get() == 2));
        assert_eq!(
            store
                .list_tasks()
                .unwrap()
                .into_iter()
                .map(|task| task.id.as_str().to_owned())
                .collect::<Vec<_>>(),
            vec![
                "task-order-c".to_owned(),
                "task-order-b".to_owned(),
                "task-order-a".to_owned(),
            ]
        );
        let event_count = store
            .product_events(&PageRequest {
                after: None,
                limit: 100,
            })
            .unwrap()
            .items
            .len();
        assert_eq!(event_count, 3);

        let duplicate = store.reorder_tasks_command(&meta, &entries).unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.value, first.value);
        assert_eq!(
            store
                .product_events(&PageRequest {
                    after: None,
                    limit: 100,
                })
                .unwrap()
                .items
                .len(),
            event_count
        );
        let conflict = project_reorder_meta("other-task-command", "reorder-tasks-key");
        assert!(matches!(
            store.reorder_tasks_command(&conflict, &entries),
            Err(ProductError::Conflict {
                conflict: ConflictKind::DuplicateIdempotency,
                ..
            })
        ));
    }

    #[test]
    fn task_reorder_rolls_back_earlier_tasks_when_a_later_write_fails() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let tasks = task_reorder_fixture(&store);
        store
            .with_conn(|conn| {
                conn.execute_batch(
                    r#"CREATE TRIGGER fail_task_reorder
                       BEFORE UPDATE ON tasks
                       WHEN OLD.id = 'task-order-b'
                       BEGIN
                         SELECT RAISE(ABORT, 'injected task reorder failure');
                       END;"#,
                )
                .map_err(db_err)
            })
            .unwrap();
        let entries = task_reorder_entries(&tasks);
        let meta = project_reorder_meta("task-reorder-rollback", "task-reorder-rollback-key");

        assert!(store.reorder_tasks_command(&meta, &entries).is_err());
        assert!(store
            .list_tasks()
            .unwrap()
            .iter()
            .enumerate()
            .all(|(index, task)| task.sort_order == index as i64
                && task.revision == ProductRevision::INITIAL));
        assert!(store
            .product_events(&PageRequest {
                after: None,
                limit: 100,
            })
            .unwrap()
            .items
            .is_empty());
        let result_count = store
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM product_command_results", [], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(db_err)
            })
            .unwrap();
        assert_eq!(result_count, 0);
    }

    fn task_move_fixture(
        store: &SqliteProductStore,
    ) -> (Project, Project, ProductTask, Vec<ConversationId>) {
        let source = Project::new(ProjectId::new("task-move-source").unwrap(), "Source").unwrap();
        let target = Project::new(ProjectId::new("task-move-target").unwrap(), "Target").unwrap();
        store
            .create_entity(ProductEntity::Project(source.clone()))
            .unwrap();
        store
            .create_entity(ProductEntity::Project(target.clone()))
            .unwrap();
        let task = ProductTask::new(
            TaskId::new("task-move-item").unwrap(),
            Some(source.id.clone()),
            "Move",
        )
        .unwrap();
        store
            .create_entity(ProductEntity::Task(task.clone()))
            .unwrap();
        let mut child = ProductTask::new(
            TaskId::new("task-move-child").unwrap(),
            Some(source.id.clone()),
            "Child",
        )
        .unwrap();
        child.parent_id = Some(task.id.clone());
        store
            .create_entity(ProductEntity::Task(child.clone()))
            .unwrap();
        let mut target_task = ProductTask::new(
            TaskId::new("task-move-target-existing").unwrap(),
            Some(target.id.clone()),
            "Existing",
        )
        .unwrap();
        target_task.sort_order = 4;
        store
            .create_entity(ProductEntity::Task(target_task))
            .unwrap();
        let conversation_ids = [
            ("task-move-conversation-a", task.id.clone()),
            ("task-move-conversation-b", task.id.clone()),
            ("task-move-conversation-child", child.id.clone()),
        ]
        .into_iter()
        .map(|(id, conversation_task_id)| {
            let id = ConversationId::new(id).unwrap();
            let conversation = ProductConversation::new(
                id.clone(),
                Some(source.id.clone()),
                Some(conversation_task_id),
                id.as_str(),
            )
            .unwrap();
            store
                .create_entity(ProductEntity::Conversation(conversation))
                .unwrap();
            id
        })
        .collect();
        (source, target, task, conversation_ids)
    }

    fn task_move_meta(task: &ProductTask, command: &str, key: &str) -> ProductCommandMeta {
        ProductCommandMeta::update(
            command,
            IdempotencyKey::new(key).unwrap(),
            ExpectedRevision::new(task.revision.get()).unwrap(),
        )
        .unwrap()
    }

    fn task_move_input(task: &ProductTask, target: &Project) -> ProductTaskMoveInput {
        ProductTaskMoveInput {
            task_id: task.id.clone(),
            target_project_id: Some(target.id.clone()),
            target_parent_id: None,
            expected_revision: ExpectedRevision::new(task.revision.get()).unwrap(),
            moved_at: 99,
        }
    }

    #[test]
    fn task_move_is_one_sqlite_transaction_and_exactly_idempotent() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let (_source, target, task, conversation_ids) = task_move_fixture(&store);
        let input = task_move_input(&task, &target);
        let meta = task_move_meta(&task, "move-task", "move-task-key");

        let first = store.move_task_command(&meta, &input).unwrap();
        assert!(!first.duplicate);
        assert_eq!(first.value.task.project_id, Some(target.id.clone()));
        assert_eq!(first.value.task.sort_order, 5);
        assert_eq!(first.value.task.revision.get(), 2);
        assert_eq!(
            first.value.moved_task_ids,
            vec![task.id.clone(), TaskId::new("task-move-child").unwrap()]
        );
        assert_eq!(first.value.moved_conversation_ids, conversation_ids);
        let child = store
            .get_task(&TaskId::new("task-move-child").unwrap())
            .unwrap();
        assert_eq!(child.project_id, Some(target.id.clone()));
        assert_eq!(child.parent_id, Some(task.id.clone()));
        let conversations = store
            .list_entities(ProductEntityKind::Conversation)
            .unwrap();
        assert!(conversations.iter().all(|entity| matches!(
            entity,
            ProductEntity::Conversation(conversation)
                if conversation.project_id.as_ref() == Some(&target.id)
                    && conversation.revision.get() == 2
        )));
        let event_count = store
            .product_events(&PageRequest {
                after: None,
                limit: 100,
            })
            .unwrap()
            .items
            .len();
        assert_eq!(event_count, 5);

        let duplicate = store.move_task_command(&meta, &input).unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.value, first.value);
        assert_eq!(
            store
                .product_events(&PageRequest {
                    after: None,
                    limit: 100,
                })
                .unwrap()
                .items
                .len(),
            event_count
        );
        let conflict = task_move_meta(&task, "other-move-task", "move-task-key");
        assert!(matches!(
            store.move_task_command(&conflict, &input),
            Err(ProductError::Conflict {
                conflict: ConflictKind::DuplicateIdempotency,
                ..
            })
        ));
    }

    #[test]
    fn task_move_rolls_back_conversations_when_the_task_write_fails() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let (source, target, task, _) = task_move_fixture(&store);
        store
            .with_conn(|conn| {
                conn.execute_batch(
                    r#"CREATE TRIGGER fail_task_move
                       BEFORE UPDATE ON tasks
                       WHEN OLD.id = 'task-move-item'
                       BEGIN
                         SELECT RAISE(ABORT, 'injected task move failure');
                       END;"#,
                )
                .map_err(db_err)
            })
            .unwrap();
        let input = task_move_input(&task, &target);
        let meta = task_move_meta(&task, "move-task-rollback", "move-task-rollback-key");

        assert!(store.move_task_command(&meta, &input).is_err());
        assert_eq!(
            store.get_task(&task.id).unwrap().project_id,
            Some(source.id.clone())
        );
        assert_eq!(
            store
                .get_task(&TaskId::new("task-move-child").unwrap())
                .unwrap()
                .project_id,
            Some(source.id.clone())
        );
        assert!(store
            .list_entities(ProductEntityKind::Conversation)
            .unwrap()
            .iter()
            .all(|entity| matches!(
                entity,
                ProductEntity::Conversation(conversation)
                    if conversation.project_id.as_ref() == Some(&source.id)
                        && conversation.revision == ProductRevision::INITIAL
            )));
        assert!(store
            .product_events(&PageRequest {
                after: None,
                limit: 100,
            })
            .unwrap()
            .items
            .is_empty());
        let result_count = store
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM product_command_results", [], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(db_err)
            })
            .unwrap();
        assert_eq!(result_count, 0);
    }

    #[test]
    fn clear_bindings_for_task_removes_only_that_task() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let project = Project::new(ProjectId::new("p-bind").unwrap(), "Demo").unwrap();
        store.upsert_project(&project).unwrap();
        let task_a = ProductTask::new(
            TaskId::new("task-a").unwrap(),
            Some(project.id.clone()),
            "A",
        )
        .unwrap();
        let task_b = ProductTask::new(
            TaskId::new("task-b").unwrap(),
            Some(project.id.clone()),
            "B",
        )
        .unwrap();
        store.upsert_task(&task_a).unwrap();
        store.upsert_task(&task_b).unwrap();
        store
            .upsert_binding(&AgentSessionBinding {
                binding_id: BindingId::new("binding-a").unwrap(),
                task_id: task_a.id.clone(),
                conversation_id: None,
                agent_session: AgentSessionRef::new("session-a").unwrap(),
                profile_id: Some("native-coding".into()),
                revision: ProductRevision::INITIAL,
            })
            .unwrap();
        store
            .upsert_binding(&AgentSessionBinding {
                binding_id: BindingId::new("binding-b").unwrap(),
                task_id: task_b.id.clone(),
                conversation_id: None,
                agent_session: AgentSessionRef::new("session-b").unwrap(),
                profile_id: Some("native-coding".into()),
                revision: ProductRevision::INITIAL,
            })
            .unwrap();
        assert_eq!(store.clear_bindings_for_task(&task_a.id).unwrap(), 1);
        assert!(store.list_bindings_for_task(&task_a.id).unwrap().is_empty());
        assert_eq!(store.list_bindings_for_task(&task_b.id).unwrap().len(), 1);
    }

    #[test]
    fn replace_binding_for_task_is_atomic_and_keeps_other_tasks() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let project = Project::new(ProjectId::new("p-replace-bind").unwrap(), "Demo").unwrap();
        store.upsert_project(&project).unwrap();
        let task_a = ProductTask::new(
            TaskId::new("replace-a").unwrap(),
            Some(project.id.clone()),
            "A",
        )
        .unwrap();
        let task_b = ProductTask::new(
            TaskId::new("replace-b").unwrap(),
            Some(project.id.clone()),
            "B",
        )
        .unwrap();
        store.upsert_task(&task_a).unwrap();
        store.upsert_task(&task_b).unwrap();
        for (task, binding, session) in [
            (&task_a, "binding-parent", "session-parent"),
            (&task_b, "binding-other", "session-other"),
        ] {
            store
                .upsert_binding(&AgentSessionBinding {
                    binding_id: BindingId::new(binding).unwrap(),
                    task_id: task.id.clone(),
                    conversation_id: None,
                    agent_session: AgentSessionRef::new(session).unwrap(),
                    profile_id: Some("native-coding".into()),
                    revision: ProductRevision::INITIAL,
                })
                .unwrap();
        }
        let forked = AgentSessionBinding {
            binding_id: BindingId::new("binding-forked").unwrap(),
            task_id: task_a.id.clone(),
            conversation_id: None,
            agent_session: AgentSessionRef::new("session-forked").unwrap(),
            profile_id: Some("native-coding".into()),
            revision: ProductRevision::INITIAL,
        };
        <SqliteProductStore as ProductRepository>::replace_binding_for_task(&store, forked.clone())
            .unwrap();

        assert_eq!(
            store.list_bindings_for_task(&task_a.id).unwrap(),
            vec![forked]
        );
        assert_eq!(store.list_bindings_for_task(&task_b.id).unwrap().len(), 1);
    }

    #[test]
    fn project_task_roundtrip() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let mut project = Project::new(ProjectId::new("p1").unwrap(), "Demo").unwrap();
        project.git_workspace = Some(GitWorkspaceRef {
            repository: Some("openai/lilia".into()),
            branch: Some("main".into()),
            worktree_path: Some("/tmp/lilia".into()),
        });
        project.settings.default_agent_profile_id = Some("reviewer".into());
        project
            .settings
            .values
            .insert("theme".into(), "system".into());
        project.asset_ids = vec![ProjectAssetId::new("asset-1").unwrap()];
        store.upsert_project(&project).unwrap();
        let mut task = ProductTask::new(
            TaskId::new("t1").unwrap(),
            Some(project.id.clone()),
            "hello",
        )
        .unwrap();
        task.description = Some("Product Core persistence".into());
        task.priority = ProductTaskPriority::High;
        task.assignment_id = Some(AssignmentId::new("assignment-1").unwrap());
        task.completion_criteria = vec!["roundtrip".into()];
        task.milestone_id = Some(MilestoneId::new("milestone-1").unwrap());
        task.workflow_id = Some(WorkflowId::new("workflow-1").unwrap());
        task.agent_profile_id = Some("reviewer".into());
        task.blocked_reason = Some("waiting for review".into());
        task.archived = true;
        task.tags = vec!["product-core".into()];
        task.legacy_source = Some("codex".into());
        store.upsert_task(&task).unwrap();
        assert_eq!(store.get_project(&project.id).unwrap(), project);
        assert_eq!(store.get_task(&task.id).unwrap(), task);
    }

    #[test]
    fn migrates_v1_rows_with_v2_defaults() {
        let conn = Connection::open_in_memory().unwrap();
        SqliteProductStore::configure(&conn).unwrap();
        conn.execute_batch(MIGRATION_V1).unwrap();
        conn.execute("INSERT INTO schema_migrations(version) VALUES (1)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO projects(id, name, revision) VALUES ('p1', 'Legacy', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tasks(id, project_id, title, revision) VALUES ('t1', 'p1', 'Legacy task', 1)",
            [],
        )
        .unwrap();

        SqliteProductStore::migrate(&conn).unwrap();
        let store = SqliteProductStore {
            path: None,
            conn: Mutex::new(conn),
            mutation_lock: Mutex::new(()),
        };

        let project = store.get_project(&ProjectId::new("p1").unwrap()).unwrap();
        assert_eq!(project.settings, ProjectSettings::default());
        assert!(project.asset_ids.is_empty());
        let task = store.get_task(&TaskId::new("t1").unwrap()).unwrap();
        assert_eq!(task.priority, ProductTaskPriority::Normal);
        assert!(task.completion_criteria.is_empty());
        assert!(task.tags.is_empty());
    }

    #[test]
    fn generic_product_entities_roundtrip_with_revision_conflicts() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        let conversation = ProductConversation::new(
            ConversationId::new("conversation-1").unwrap(),
            None,
            None,
            "First",
        )
        .unwrap();
        let created = store
            .create_entity(ProductEntity::Conversation(conversation))
            .unwrap();
        let mut conversation = match created {
            ProductEntity::Conversation(value) => value,
            _ => unreachable!(),
        };
        let expected = ExpectedRevision::new(conversation.revision.get()).unwrap();
        conversation.title = "Renamed".into();
        let updated = store
            .update_entity(ProductEntity::Conversation(conversation), expected)
            .unwrap();
        assert_eq!(updated.revision().get(), 2);
        assert!(matches!(
            store.update_entity(updated, expected),
            Err(ProductError::Conflict {
                conflict: ConflictKind::StaleRevision,
                ..
            })
        ));
        assert_eq!(
            store
                .list_entities(ProductEntityKind::Conversation)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn concurrent_updates_have_one_winner_for_the_same_revision() {
        let store = Arc::new(SqliteProductStore::open_in_memory().unwrap());
        let conversation = ProductConversation::new(
            ConversationId::new("conversation-concurrent").unwrap(),
            None,
            None,
            "First",
        )
        .unwrap();
        store
            .create_entity(ProductEntity::Conversation(conversation.clone()))
            .unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for title in ["Left", "Right"] {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            let mut candidate = conversation.clone();
            candidate.title = title.into();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                store.update_entity(
                    ProductEntity::Conversation(candidate),
                    ExpectedRevision::new(1).unwrap(),
                )
            }));
        }
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(
                    result,
                    Err(ProductError::Conflict {
                        conflict: ConflictKind::StaleRevision,
                        ..
                    })
                ))
                .count(),
            1
        );
    }

    #[test]
    fn command_rolls_back_entity_when_event_append_fails() {
        let store = SqliteProductStore::open_in_memory().unwrap();
        store
            .with_conn(|conn| {
                conn.execute_batch(
                    r#"CREATE TRIGGER reject_product_event
                       BEFORE INSERT ON product_events
                       BEGIN
                         SELECT RAISE(ABORT, 'event append rejected');
                       END;"#,
                )
                .map_err(db_err)
            })
            .unwrap();
        let conversation = ProductConversation::new(
            ConversationId::new("conversation-atomic").unwrap(),
            None,
            None,
            "Atomic",
        )
        .unwrap();
        let meta = ProductCommandMeta::create(
            "command-atomic",
            IdempotencyKey::new("idempotency-atomic").unwrap(),
        )
        .unwrap();

        assert!(store
            .create_entity_command(&meta, ProductEntity::Conversation(conversation), "created",)
            .is_err());
        assert!(matches!(
            store.get_entity(ProductEntityKind::Conversation, "conversation-atomic"),
            Err(ProductError::NotFound { .. })
        ));
        assert!(store
            .product_events(&PageRequest {
                after: None,
                limit: 10,
            })
            .unwrap()
            .items
            .is_empty());
    }
}

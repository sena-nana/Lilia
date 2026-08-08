//! Product domain SQLite repositories (#56) — Project / Task / Binding.
//!
//! Separate from Agent Runtime and from Desktop Tauri UI cache (`lilia.db`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lilia_contracts::{
    AgentSessionBinding, AgentSessionRef, AssignmentId, BindingId, ConflictKind, ConversationId,
    ExpectedRevision, MilestoneId, Page, PageRequest, ProductCommandMeta, ProductCommandResult,
    ProductEntity, ProductEntityKind, ProductError, ProductEvent, ProductEventSequence,
    ProductResult, ProductRevision, ProductTask, ProductTaskPriority, ProductTaskStatus, Project,
    ProjectArchiveState, ProjectId, TaskDependencyGraph, TaskId, WorkflowId,
};
use lilia_core::{ensure_expected_revision, ProductRepository};
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;

const SCHEMA_VERSION: i64 = 5;

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

fn load_command_result_on(
    conn: &Connection,
    idempotency_key: &str,
) -> ProductResult<Option<ProductCommandResult<ProductEntity>>> {
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

fn duplicate_sqlite_command_result(
    meta: &ProductCommandMeta,
    mut result: ProductCommandResult<ProductEntity>,
) -> ProductResult<ProductCommandResult<ProductEntity>> {
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
        GitWorkspaceRef, IdempotencyKey, ProductConversation, ProductEntity, ProjectAssetId,
        ProjectSettings,
    };

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

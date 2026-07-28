//! SQLite-backed product projection store (#56).
//!
//! Schema migrations live here — not in Desktop Tauri. Desktop UI SQLite remains
//! a rebuildable cache only.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lilia_contracts::{
    AgentSessionRef, ArtifactProjection, PendingProjection, PendingProjectionStatus, ProductError,
    ProductResult, ProjectionEventId, TaskId, TimelineProjectionCommand, TimelineProjectionEvent,
    TodoProjection,
};
use rusqlite::{params, Connection, OptionalExtension};

use crate::timeline::{ProjectionApplyResult, TimelineProjectionRepository};

const SCHEMA_VERSION: i64 = 1;

const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY NOT NULL,
  applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS timeline_projection_events (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  agent_session TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  turn_id TEXT,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  title TEXT NOT NULL,
  summary TEXT,
  payload TEXT NOT NULL,
  projected INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_timeline_proj_task
  ON timeline_projection_events(task_id, sequence);
CREATE INDEX IF NOT EXISTS idx_timeline_proj_session
  ON timeline_projection_events(agent_session, sequence);

CREATE TABLE IF NOT EXISTS artifact_projections (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  agent_session TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  turn_id TEXT,
  artifact_id TEXT NOT NULL,
  media_type TEXT NOT NULL,
  summary TEXT NOT NULL,
  kind TEXT,
  size_bytes INTEGER,
  content_hash TEXT,
  content_ref TEXT,
  provenance TEXT,
  status TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_artifact_proj_task
  ON artifact_projections(task_id, sequence);

CREATE TABLE IF NOT EXISTS todo_projections (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  agent_session TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  turn_id TEXT,
  todo_id TEXT NOT NULL,
  revision INTEGER NOT NULL,
  items TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_todo_proj_task
  ON todo_projections(task_id, sequence);

CREATE TABLE IF NOT EXISTS pending_projections (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  agent_session TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  turn_id TEXT,
  request_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  prompt TEXT,
  action_revision INTEGER,
  payload TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pending_proj_task
  ON pending_projections(task_id, sequence);

CREATE TABLE IF NOT EXISTS projection_cursors (
  agent_session TEXT PRIMARY KEY NOT NULL,
  cursor INTEGER NOT NULL
);
"#;

/// Durable product projection repository (SQLite).
pub struct SqliteTimelineProjectionStore {
    path: Option<PathBuf>,
    conn: Mutex<Connection>,
}

impl SqliteTimelineProjectionStore {
    pub fn open(path: impl AsRef<Path>) -> ProductResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| ProductError::Unavailable {
                message: format!("create projection db dir: {err}"),
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
            message: "sqlite projection store lock poisoned".into(),
        })?;
        f(&conn)
    }

    fn bump_cursor(conn: &Connection, session_id: &str, sequence: u64) -> ProductResult<()> {
        conn.execute(
            r#"INSERT INTO projection_cursors(agent_session, cursor) VALUES (?1, ?2)
               ON CONFLICT(agent_session) DO UPDATE SET cursor = MAX(cursor, excluded.cursor)"#,
            params![session_id, sequence as i64],
        )
        .map_err(db_err)?;
        Ok(())
    }
}

fn db_err(err: rusqlite::Error) -> ProductError {
    ProductError::Unavailable {
        message: format!("sqlite projection: {err}"),
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

fn pending_status_to_str(status: &PendingProjectionStatus) -> &'static str {
    match status {
        PendingProjectionStatus::Open => "open",
        PendingProjectionStatus::Resolved => "resolved",
        PendingProjectionStatus::Expired => "expired",
        PendingProjectionStatus::Cancelled => "cancelled",
        PendingProjectionStatus::Stale => "stale",
    }
}

fn pending_status_from_str(value: &str) -> PendingProjectionStatus {
    match value {
        "resolved" => PendingProjectionStatus::Resolved,
        "expired" => PendingProjectionStatus::Expired,
        "cancelled" => PendingProjectionStatus::Cancelled,
        "stale" => PendingProjectionStatus::Stale,
        _ => PendingProjectionStatus::Open,
    }
}

fn map_timeline_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TimelineProjectionEvent> {
    let id: String = row.get(0)?;
    let task_id: String = row.get(1)?;
    let agent_session: String = row.get(2)?;
    let payload: String = row.get(9)?;
    Ok(TimelineProjectionEvent {
        id: ProjectionEventId::new(id),
        task_id: TaskId::new(task_id).map_err(invalid_id)?,
        agent_session: AgentSessionRef::new(agent_session).map_err(invalid_id)?,
        sequence: row.get::<_, i64>(3)? as u64,
        turn_id: row.get(4)?,
        kind: row.get(5)?,
        status: row.get(6)?,
        title: row.get(7)?,
        summary: row.get(8)?,
        payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
        projected: row.get::<_, i64>(10)? != 0,
    })
}

fn map_artifact_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactProjection> {
    let content_ref: Option<String> = row.get(11)?;
    Ok(ArtifactProjection {
        id: row.get(0)?,
        task_id: TaskId::new(row.get::<_, String>(1)?).map_err(invalid_id)?,
        agent_session: AgentSessionRef::new(row.get::<_, String>(2)?).map_err(invalid_id)?,
        sequence: row.get::<_, i64>(3)? as u64,
        turn_id: row.get(4)?,
        artifact_id: row.get(5)?,
        media_type: row.get(6)?,
        summary: row.get(7)?,
        kind: row.get(8)?,
        size_bytes: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
        content_hash: row.get(10)?,
        content_ref: content_ref.and_then(|text| serde_json::from_str(&text).ok()),
        provenance: row.get(12)?,
        status: row.get(13)?,
    })
}

fn map_todo_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TodoProjection> {
    let items: String = row.get(7)?;
    Ok(TodoProjection {
        id: row.get(0)?,
        task_id: TaskId::new(row.get::<_, String>(1)?).map_err(invalid_id)?,
        agent_session: AgentSessionRef::new(row.get::<_, String>(2)?).map_err(invalid_id)?,
        sequence: row.get::<_, i64>(3)? as u64,
        turn_id: row.get(4)?,
        todo_id: row.get(5)?,
        revision: row.get::<_, i64>(6)? as u64,
        items: serde_json::from_str(&items).unwrap_or(serde_json::Value::Null),
    })
}

fn map_pending_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PendingProjection> {
    let status: String = row.get(7)?;
    let payload: String = row.get(10)?;
    Ok(PendingProjection {
        id: row.get(0)?,
        task_id: TaskId::new(row.get::<_, String>(1)?).map_err(invalid_id)?,
        agent_session: AgentSessionRef::new(row.get::<_, String>(2)?).map_err(invalid_id)?,
        sequence: row.get::<_, i64>(3)? as u64,
        turn_id: row.get(4)?,
        request_id: row.get(5)?,
        kind: row.get(6)?,
        status: pending_status_from_str(&status),
        prompt: row.get(8)?,
        action_revision: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
        payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
    })
}

impl TimelineProjectionRepository for SqliteTimelineProjectionStore {
    fn apply(&self, command: TimelineProjectionCommand) -> ProductResult<ProjectionApplyResult> {
        self.with_conn(|conn| match command {
            TimelineProjectionCommand::UpsertTimelineEvent { event } => {
                let exists: bool = conn
                    .query_row(
                        "SELECT 1 FROM timeline_projection_events WHERE id = ?1",
                        params![event.id.as_str()],
                        |_| Ok(true),
                    )
                    .optional()
                    .map_err(db_err)?
                    .unwrap_or(false);
                if exists {
                    return Ok(ProjectionApplyResult::DuplicateIgnored);
                }
                let payload = serde_json::to_string(&event.payload).map_err(|err| {
                    ProductError::InvalidInput {
                        field: "payload".into(),
                        message: err.to_string(),
                    }
                })?;
                conn.execute(
                    r#"INSERT INTO timeline_projection_events
                       (id, task_id, agent_session, sequence, turn_id, kind, status, title, summary, payload, projected)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
                    params![
                        event.id.as_str(),
                        event.task_id.as_str(),
                        event.agent_session.as_str(),
                        event.sequence as i64,
                        event.turn_id,
                        event.kind,
                        event.status,
                        event.title,
                        event.summary,
                        payload,
                        if event.projected { 1 } else { 0 },
                    ],
                )
                .map_err(db_err)?;
                Self::bump_cursor(conn, event.agent_session.as_str(), event.sequence)?;
                Ok(ProjectionApplyResult::Inserted)
            }
            TimelineProjectionCommand::UpsertArtifact { artifact } => {
                let exists: bool = conn
                    .query_row(
                        "SELECT 1 FROM artifact_projections WHERE id = ?1",
                        params![artifact.id],
                        |_| Ok(true),
                    )
                    .optional()
                    .map_err(db_err)?
                    .unwrap_or(false);
                let content_ref = artifact
                    .content_ref
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|err| ProductError::InvalidInput {
                        field: "content_ref".into(),
                        message: err.to_string(),
                    })?;
                conn.execute(
                    r#"INSERT INTO artifact_projections
                       (id, task_id, agent_session, sequence, turn_id, artifact_id, media_type, summary,
                        kind, size_bytes, content_hash, content_ref, provenance, status)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                       ON CONFLICT(id) DO UPDATE SET
                         sequence=excluded.sequence,
                         turn_id=excluded.turn_id,
                         media_type=excluded.media_type,
                         summary=excluded.summary,
                         kind=excluded.kind,
                         size_bytes=excluded.size_bytes,
                         content_hash=excluded.content_hash,
                         content_ref=excluded.content_ref,
                         provenance=excluded.provenance,
                         status=excluded.status"#,
                    params![
                        artifact.id,
                        artifact.task_id.as_str(),
                        artifact.agent_session.as_str(),
                        artifact.sequence as i64,
                        artifact.turn_id,
                        artifact.artifact_id,
                        artifact.media_type,
                        artifact.summary,
                        artifact.kind,
                        artifact.size_bytes.map(|v| v as i64),
                        artifact.content_hash,
                        content_ref,
                        artifact.provenance,
                        artifact.status,
                    ],
                )
                .map_err(db_err)?;
                Self::bump_cursor(conn, artifact.agent_session.as_str(), artifact.sequence)?;
                Ok(if exists {
                    ProjectionApplyResult::Updated
                } else {
                    ProjectionApplyResult::Inserted
                })
            }
            TimelineProjectionCommand::UpsertTodo { todo } => {
                let exists: bool = conn
                    .query_row(
                        "SELECT 1 FROM todo_projections WHERE id = ?1",
                        params![todo.id],
                        |_| Ok(true),
                    )
                    .optional()
                    .map_err(db_err)?
                    .unwrap_or(false);
                let items = serde_json::to_string(&todo.items).map_err(|err| {
                    ProductError::InvalidInput {
                        field: "items".into(),
                        message: err.to_string(),
                    }
                })?;
                conn.execute(
                    r#"INSERT INTO todo_projections
                       (id, task_id, agent_session, sequence, turn_id, todo_id, revision, items)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                       ON CONFLICT(id) DO UPDATE SET
                         sequence=excluded.sequence,
                         turn_id=excluded.turn_id,
                         revision=excluded.revision,
                         items=excluded.items"#,
                    params![
                        todo.id,
                        todo.task_id.as_str(),
                        todo.agent_session.as_str(),
                        todo.sequence as i64,
                        todo.turn_id,
                        todo.todo_id,
                        todo.revision as i64,
                        items,
                    ],
                )
                .map_err(db_err)?;
                Self::bump_cursor(conn, todo.agent_session.as_str(), todo.sequence)?;
                Ok(if exists {
                    ProjectionApplyResult::Updated
                } else {
                    ProjectionApplyResult::Inserted
                })
            }
            TimelineProjectionCommand::UpsertPending { pending } => {
                let exists: bool = conn
                    .query_row(
                        "SELECT 1 FROM pending_projections WHERE id = ?1",
                        params![pending.id],
                        |_| Ok(true),
                    )
                    .optional()
                    .map_err(db_err)?
                    .unwrap_or(false);
                let payload = serde_json::to_string(&pending.payload).map_err(|err| {
                    ProductError::InvalidInput {
                        field: "payload".into(),
                        message: err.to_string(),
                    }
                })?;
                conn.execute(
                    r#"INSERT INTO pending_projections
                       (id, task_id, agent_session, sequence, turn_id, request_id, kind, status,
                        prompt, action_revision, payload)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                       ON CONFLICT(id) DO UPDATE SET
                         sequence=excluded.sequence,
                         turn_id=excluded.turn_id,
                         kind=excluded.kind,
                         status=excluded.status,
                         prompt=excluded.prompt,
                         action_revision=excluded.action_revision,
                         payload=excluded.payload"#,
                    params![
                        pending.id,
                        pending.task_id.as_str(),
                        pending.agent_session.as_str(),
                        pending.sequence as i64,
                        pending.turn_id,
                        pending.request_id,
                        pending.kind,
                        pending_status_to_str(&pending.status),
                        pending.prompt,
                        pending.action_revision.map(|v| v as i64),
                        payload,
                    ],
                )
                .map_err(db_err)?;
                Self::bump_cursor(conn, pending.agent_session.as_str(), pending.sequence)?;
                Ok(if exists {
                    ProjectionApplyResult::Updated
                } else {
                    ProjectionApplyResult::Inserted
                })
            }
            TimelineProjectionCommand::ResolvePending {
                session_id,
                request_id,
                status,
                sequence,
                response,
            } => {
                let key = format!("{session_id}:{request_id}");
                let existing: Option<String> = conn
                    .query_row(
                        "SELECT payload FROM pending_projections WHERE id = ?1",
                        params![key],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(db_err)?;
                Self::bump_cursor(conn, &session_id, sequence)?;
                let Some(payload_text) = existing else {
                    return Ok(ProjectionApplyResult::SkippedUnknown);
                };
                let mut payload: serde_json::Value =
                    serde_json::from_str(&payload_text).unwrap_or(serde_json::Value::Null);
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("resolution".into(), response);
                } else {
                    payload = serde_json::json!({ "resolution": response });
                }
                let payload_text = serde_json::to_string(&payload).map_err(|err| {
                    ProductError::InvalidInput {
                        field: "payload".into(),
                        message: err.to_string(),
                    }
                })?;
                conn.execute(
                    r#"UPDATE pending_projections
                       SET status = ?1,
                           sequence = CASE WHEN sequence > ?2 THEN sequence ELSE ?2 END,
                           payload = ?3
                       WHERE id = ?4"#,
                    params![
                        pending_status_to_str(&status),
                        sequence as i64,
                        payload_text,
                        key
                    ],
                )
                .map_err(db_err)?;
                Ok(ProjectionApplyResult::Updated)
            }
            TimelineProjectionCommand::SkipUnknown {
                session_id,
                sequence,
                ..
            } => {
                Self::bump_cursor(conn, &session_id, sequence)?;
                Ok(ProjectionApplyResult::SkippedUnknown)
            }
        })
    }

    fn list_for_task(&self, task_id: &TaskId) -> Vec<TimelineProjectionEvent> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    r#"SELECT id, task_id, agent_session, sequence, turn_id, kind, status, title,
                              summary, payload, projected
                       FROM timeline_projection_events
                       WHERE task_id = ?1
                       ORDER BY sequence ASC, id ASC"#,
                )
                .map_err(db_err)?;
            let rows = stmt
                .query_map(params![task_id.as_str()], map_timeline_row)
                .map_err(db_err)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db_err)?);
            }
            Ok(out)
        })
        .unwrap_or_default()
    }

    fn list_for_session(&self, session: &AgentSessionRef) -> Vec<TimelineProjectionEvent> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    r#"SELECT id, task_id, agent_session, sequence, turn_id, kind, status, title,
                              summary, payload, projected
                       FROM timeline_projection_events
                       WHERE agent_session = ?1
                       ORDER BY sequence ASC"#,
                )
                .map_err(db_err)?;
            let rows = stmt
                .query_map(params![session.as_str()], map_timeline_row)
                .map_err(db_err)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db_err)?);
            }
            Ok(out)
        })
        .unwrap_or_default()
    }

    fn list_artifacts_for_task(&self, task_id: &TaskId) -> Vec<ArtifactProjection> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    r#"SELECT id, task_id, agent_session, sequence, turn_id, artifact_id, media_type,
                              summary, kind, size_bytes, content_hash, content_ref, provenance, status
                       FROM artifact_projections
                       WHERE task_id = ?1
                       ORDER BY sequence ASC, id ASC"#,
                )
                .map_err(db_err)?;
            let rows = stmt
                .query_map(params![task_id.as_str()], map_artifact_row)
                .map_err(db_err)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db_err)?);
            }
            Ok(out)
        })
        .unwrap_or_default()
    }

    fn list_todos_for_task(&self, task_id: &TaskId) -> Vec<TodoProjection> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    r#"SELECT id, task_id, agent_session, sequence, turn_id, todo_id, revision, items
                       FROM todo_projections
                       WHERE task_id = ?1
                       ORDER BY sequence ASC, id ASC"#,
                )
                .map_err(db_err)?;
            let rows = stmt
                .query_map(params![task_id.as_str()], map_todo_row)
                .map_err(db_err)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db_err)?);
            }
            Ok(out)
        })
        .unwrap_or_default()
    }

    fn list_pending_for_task(&self, task_id: &TaskId) -> Vec<PendingProjection> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    r#"SELECT id, task_id, agent_session, sequence, turn_id, request_id, kind, status,
                              prompt, action_revision, payload
                       FROM pending_projections
                       WHERE task_id = ?1
                       ORDER BY sequence ASC, id ASC"#,
                )
                .map_err(db_err)?;
            let rows = stmt
                .query_map(params![task_id.as_str()], map_pending_row)
                .map_err(db_err)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(db_err)?);
            }
            Ok(out)
        })
        .unwrap_or_default()
    }

    fn clear_session(&self, session: &AgentSessionRef) -> ProductResult<()> {
        self.with_conn(|conn| {
            let session_id = session.as_str();
            conn.execute(
                "DELETE FROM timeline_projection_events WHERE agent_session = ?1",
                params![session_id],
            )
            .map_err(db_err)?;
            conn.execute(
                "DELETE FROM artifact_projections WHERE agent_session = ?1",
                params![session_id],
            )
            .map_err(db_err)?;
            conn.execute(
                "DELETE FROM todo_projections WHERE agent_session = ?1",
                params![session_id],
            )
            .map_err(db_err)?;
            conn.execute(
                "DELETE FROM pending_projections WHERE agent_session = ?1",
                params![session_id],
            )
            .map_err(db_err)?;
            conn.execute(
                "DELETE FROM projection_cursors WHERE agent_session = ?1",
                params![session_id],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    fn rebuild_from(&self, commands: Vec<TimelineProjectionCommand>) -> ProductResult<usize> {
        let mut inserted = 0;
        for command in commands {
            match self.apply(command)? {
                ProjectionApplyResult::Inserted => inserted += 1,
                ProjectionApplyResult::DuplicateIgnored
                | ProjectionApplyResult::Updated
                | ProjectionApplyResult::SkippedUnknown => {}
            }
        }
        Ok(inserted)
    }

    fn rebuild_session(
        &self,
        session: &AgentSessionRef,
        commands: Vec<TimelineProjectionCommand>,
    ) -> ProductResult<usize> {
        self.clear_session(session)?;
        self.rebuild_from(commands)
    }

    fn cursor_for_session(&self, session: &AgentSessionRef) -> Option<u64> {
        self.with_conn(|conn| {
            let cursor: Option<i64> = conn
                .query_row(
                    "SELECT cursor FROM projection_cursors WHERE agent_session = ?1",
                    params![session.as_str()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_err)?;
            Ok(cursor.map(|v| v as u64))
        })
        .ok()
        .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_event(session: &str, sequence: u64) -> TimelineProjectionEvent {
        TimelineProjectionEvent {
            id: ProjectionEventId::from_session_sequence(session, sequence),
            task_id: TaskId::new("task-sqlite").unwrap(),
            agent_session: AgentSessionRef::new(session).unwrap(),
            sequence,
            turn_id: Some("turn-1".into()),
            kind: "message".into(),
            status: "success".into(),
            title: "ok".into(),
            summary: Some("hi".into()),
            payload: json!({ "projected": true }),
            projected: true,
        }
    }

    #[test]
    fn sqlite_survives_reopen_and_rebuild() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("lilia-proj-{nanos}.db"));
        let _ = std::fs::remove_file(&path);

        let session = AgentSessionRef::new("sess-persist").unwrap();
        let task = TaskId::new("task-sqlite").unwrap();
        {
            let store = SqliteTimelineProjectionStore::open(&path).unwrap();
            store
                .apply(TimelineProjectionCommand::UpsertTimelineEvent {
                    event: sample_event(session.as_str(), 1),
                })
                .unwrap();
            store
                .apply(TimelineProjectionCommand::UpsertArtifact {
                    artifact: ArtifactProjection {
                        id: format!("{}:art", session.as_str()),
                        task_id: task.clone(),
                        agent_session: session.clone(),
                        sequence: 2,
                        turn_id: Some("turn-1".into()),
                        artifact_id: "art".into(),
                        media_type: "text/plain".into(),
                        summary: "note".into(),
                        kind: Some("file".into()),
                        size_bytes: Some(3),
                        content_hash: Some("h".into()),
                        content_ref: Some(json!({ "id": "r1" })),
                        provenance: Some("test".into()),
                        status: "available".into(),
                    },
                })
                .unwrap();
            store
                .apply(TimelineProjectionCommand::UpsertTodo {
                    todo: TodoProjection {
                        id: format!("{}:todo", session.as_str()),
                        task_id: task.clone(),
                        agent_session: session.clone(),
                        sequence: 3,
                        turn_id: Some("turn-1".into()),
                        todo_id: "todo".into(),
                        revision: 1,
                        items: json!([]),
                    },
                })
                .unwrap();
            store
                .apply(TimelineProjectionCommand::UpsertPending {
                    pending: PendingProjection {
                        id: format!("{}:req", session.as_str()),
                        task_id: task.clone(),
                        agent_session: session.clone(),
                        sequence: 4,
                        turn_id: Some("turn-1".into()),
                        request_id: "req".into(),
                        kind: "approval".into(),
                        status: PendingProjectionStatus::Open,
                        prompt: Some("?".into()),
                        action_revision: Some(1),
                        payload: json!({}),
                    },
                })
                .unwrap();
            assert_eq!(store.list_for_task(&task).len(), 1);
            assert_eq!(store.cursor_for_session(&session), Some(4));
        }

        // Restart: reopen same file.
        let reopened = SqliteTimelineProjectionStore::open(&path).unwrap();
        assert_eq!(reopened.list_for_task(&task).len(), 1);
        assert_eq!(reopened.list_artifacts_for_task(&task).len(), 1);
        assert_eq!(reopened.list_todos_for_task(&task).len(), 1);
        assert_eq!(reopened.list_pending_for_task(&task).len(), 1);
        assert_eq!(reopened.cursor_for_session(&session), Some(4));

        let rebuilt = reopened
            .rebuild_session(
                &session,
                vec![
                    TimelineProjectionCommand::UpsertTimelineEvent {
                        event: sample_event(session.as_str(), 1),
                    },
                    TimelineProjectionCommand::UpsertTimelineEvent {
                        event: sample_event(session.as_str(), 2),
                    },
                ],
            )
            .unwrap();
        assert_eq!(rebuilt, 2);
        assert_eq!(reopened.list_for_session(&session).len(), 2);
        assert!(reopened.list_artifacts_for_task(&task).is_empty());

        let _ = std::fs::remove_file(&path);
    }

    /// #46 / #56 — product projection DB opens and serves timeline without legacy Desktop DB.
    #[test]
    fn product_projection_opens_without_legacy_desktop_db() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lilia-proj-nolegacy-{nanos}"));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::LiliaDataPaths::from_home(&root);
        paths.ensure_layout().unwrap();
        assert!(!paths.legacy_desktop_db().exists());

        let store = SqliteTimelineProjectionStore::open(paths.product_projections_db()).unwrap();
        let session = AgentSessionRef::new("sess-nolegacy").unwrap();
        // sample_event uses task-sqlite — prove projection serves that task without legacy DB.
        let task = TaskId::new("task-sqlite").unwrap();
        store
            .apply(TimelineProjectionCommand::UpsertTimelineEvent {
                event: sample_event(session.as_str(), 1),
            })
            .unwrap();
        assert_eq!(store.list_for_task(&task).len(), 1);
        assert!(!paths.legacy_desktop_db().exists());

        let _ = std::fs::remove_dir_all(&root);
    }
}

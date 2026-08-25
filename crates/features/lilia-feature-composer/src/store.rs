use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::{ExecutionPermission, TaskId};
use lilia_storage::Db;
use rusqlite::{params, Connection, OptionalExtension};

use crate::state::{ComposerCommand, ComposerState};
use crate::ComposerError;

/// Durable home of every task's composer draft.
pub struct ComposerStore {
    connection: Db,
}

impl ComposerStore {
    pub fn in_memory() -> Result<Self, ComposerError> {
        let connection = Db::in_memory().map_err(|error| ComposerError::Storage {
            operation: "open in-memory database",
            message: error.to_string(),
        })?;
        Self::new(connection)
    }

    pub fn new(connection: Db) -> Result<Self, ComposerError> {
        let locked = connection.lock();
        locked
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS desktop_composer_drafts (
                  task_id          TEXT PRIMARY KEY,
                  revision         INTEGER NOT NULL,
                  content          TEXT NOT NULL,
                  attachments_json TEXT NOT NULL,
                  conversation_references_json TEXT NOT NULL DEFAULT '[]',
                  workflow_json    TEXT,
                  model            TEXT,
                  reasoning_effort TEXT,
                  permission       TEXT NOT NULL CHECK (permission IN ('full','ask','readonly')),
                  plan_mode        INTEGER NOT NULL CHECK (plan_mode IN (0, 1)),
                  goal_mode        INTEGER NOT NULL CHECK (goal_mode IN (0, 1)),
                  updated_at       INTEGER NOT NULL
                );
                "#,
            )
            .map_err(|error| ComposerError::Storage {
                operation: "initialize composer schema",
                message: error.to_string(),
            })?;
        ensure_column(
            &locked,
            "desktop_composer_drafts",
            "conversation_references_json",
            "ALTER TABLE desktop_composer_drafts ADD COLUMN conversation_references_json TEXT NOT NULL DEFAULT '[]'",
        )?;
        ensure_column(
            &locked,
            "desktop_composer_drafts",
            "workflow_json",
            "ALTER TABLE desktop_composer_drafts ADD COLUMN workflow_json TEXT",
        )?;
        drop(locked);
        Ok(Self { connection })
    }

    pub fn snapshot(&self, task_id: &TaskId) -> Result<ComposerState, ComposerError> {
        let connection = self.connection.lock();
        Self::snapshot_from(&connection, task_id)
    }

    pub fn snapshot_from(
        connection: &Connection,
        task_id: &TaskId,
    ) -> Result<ComposerState, ComposerError> {
        connection
            .query_row(
                r#"SELECT task_id, revision, content, attachments_json, model,
                          reasoning_effort, permission, plan_mode, goal_mode,
                          conversation_references_json, workflow_json
                   FROM desktop_composer_drafts WHERE task_id = ?1"#,
                params![task_id.as_str()],
                row_to_composer,
            )
            .optional()
            .map_err(|error| ComposerError::Storage {
                operation: "read composer draft",
                message: error.to_string(),
            })
            .map(|state| state.unwrap_or_else(|| ComposerState::new(task_id.clone())))
    }

    pub fn execute(
        &self,
        task_id: &TaskId,
        command: ComposerCommand,
    ) -> Result<(ComposerState, bool), ComposerError> {
        let mut state = self.snapshot(task_id)?;
        let changed = state.apply_transient_command(command)?;
        if changed {
            self.save(&state)?;
        }
        Ok((state, changed))
    }

    /// Clears the payload a turn just consumed, but only when the draft is
    /// still the revision that was dispatched.
    pub fn clear_dispatched_payload(
        &self,
        task_id: &TaskId,
        dispatched_revision: u64,
    ) -> Result<Option<ComposerState>, ComposerError> {
        let connection = self.connection.lock();
        Self::clear_dispatched_payload_in(&connection, task_id, dispatched_revision)
    }

    /// Same as [`Self::clear_dispatched_payload`], but joins a transaction the
    /// caller already opened.
    pub fn clear_dispatched_payload_in(
        connection: &Connection,
        task_id: &TaskId,
        dispatched_revision: u64,
    ) -> Result<Option<ComposerState>, ComposerError> {
        let mut state = Self::snapshot_from(connection, task_id)?;
        if state.revision != dispatched_revision
            || (state.content.is_empty()
                && state.attachments.is_empty()
                && state.conversation_references.is_empty())
        {
            return Ok(None);
        }
        state.content.clear();
        state.attachments.clear();
        state.conversation_references.clear();
        state.workflow = None;
        state.revision = state
            .revision
            .checked_add(1)
            .ok_or(ComposerError::RevisionOverflow)?;
        Self::save_to(connection, &state)?;
        Ok(Some(state))
    }

    pub fn save(&self, state: &ComposerState) -> Result<(), ComposerError> {
        let connection = self.connection.lock();
        Self::save_to(&connection, state)
    }

    pub fn remove(&self, task_id: &TaskId) -> Result<(), ComposerError> {
        self.connection
            .lock()
            .execute(
                "DELETE FROM desktop_composer_drafts WHERE task_id = ?1",
                params![task_id.as_str()],
            )
            .map_err(|error| ComposerError::Storage {
                operation: "remove composer draft",
                message: error.to_string(),
            })?;
        Ok(())
    }

    pub fn save_to(connection: &Connection, state: &ComposerState) -> Result<(), ComposerError> {
        let attachments =
            serde_json::to_string(&state.attachments).map_err(|error| {
                ComposerError::Serialization {
                    field: "attachments",
                    message: error.to_string(),
                }
            })?;
        let conversation_references = serde_json::to_string(&state.conversation_references)
            .map_err(|error| ComposerError::Serialization {
                field: "conversationReferences",
                message: error.to_string(),
            })?;
        let workflow = state
            .workflow
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| ComposerError::Serialization {
                field: "workflow",
                message: error.to_string(),
            })?;
        connection
            .execute(
                r#"INSERT INTO desktop_composer_drafts
                   (task_id, revision, content, attachments_json, model, reasoning_effort,
                    permission, plan_mode, goal_mode, updated_at, conversation_references_json,
                    workflow_json)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                   ON CONFLICT(task_id) DO UPDATE SET
                     revision = excluded.revision,
                     content = excluded.content,
                     attachments_json = excluded.attachments_json,
                     model = excluded.model,
                     reasoning_effort = excluded.reasoning_effort,
                     permission = excluded.permission,
                     plan_mode = excluded.plan_mode,
                     goal_mode = excluded.goal_mode,
                     updated_at = excluded.updated_at,
                     conversation_references_json = excluded.conversation_references_json,
                     workflow_json = excluded.workflow_json"#,
                params![
                    state.task_id.as_str(),
                    i64::try_from(state.revision).map_err(|_| ComposerError::RevisionOverflow)?,
                    state.content,
                    attachments,
                    state.model,
                    state.reasoning_effort,
                    state.permission.as_str(),
                    i64::from(state.plan_mode),
                    i64::from(state.goal_mode),
                    now_millis(),
                    conversation_references,
                    workflow,
                ],
            )
            .map(|_| ())
            .map_err(|error| ComposerError::Storage {
                operation: "save composer draft",
                message: error.to_string(),
            })
    }
}

fn row_to_composer(row: &rusqlite::Row<'_>) -> rusqlite::Result<ComposerState> {
    let task_id =
        TaskId::new(row.get::<_, String>(0)?).map_err(|error| invalid_data(error.to_string()))?;
    let revision =
        u64::try_from(row.get::<_, i64>(1)?).map_err(|error| invalid_data(error.to_string()))?;
    let attachments_json = row.get::<_, String>(3)?;
    let attachments =
        serde_json::from_str(&attachments_json).map_err(|error| invalid_data(error.to_string()))?;
    let conversation_references_json = row.get::<_, String>(9)?;
    let conversation_references = serde_json::from_str(&conversation_references_json)
        .map_err(|error| invalid_data(error.to_string()))?;
    let workflow = row
        .get::<_, Option<String>>(10)?
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|error| invalid_data(error.to_string()))?;
    let permission = ExecutionPermission::parse(&row.get::<_, String>(6)?)
        .ok_or_else(|| invalid_data("invalid composer permission".to_owned()))?;
    Ok(ComposerState {
        task_id,
        revision,
        content: row.get(2)?,
        attachments,
        conversation_references,
        workflow,
        model: row.get(4)?,
        reasoning_effort: row.get(5)?,
        permission,
        plan_mode: row.get::<_, i64>(7)? != 0,
        goal_mode: row.get::<_, i64>(8)? != 0,
    })
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    migration: &str,
) -> Result<(), ComposerError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| ComposerError::Storage {
            operation: "inspect composer schema",
            message: error.to_string(),
        })?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| ComposerError::Storage {
            operation: "inspect composer schema",
            message: error.to_string(),
        })?;
    for candidate in columns {
        if candidate.map_err(|error| ComposerError::Storage {
            operation: "inspect composer schema",
            message: error.to_string(),
        })? == column
        {
            return Ok(());
        }
    }
    connection
        .execute_batch(migration)
        .map_err(|error| ComposerError::Storage {
            operation: "migrate composer schema",
            message: error.to_string(),
        })
}

fn invalid_data(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn now_millis() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

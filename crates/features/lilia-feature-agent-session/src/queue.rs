#[cfg(test)]
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::TaskId;
use lilia_storage::Db;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::turn::DesktopTurnRequest;

const TURN_QUEUE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS desktop_pending_turns (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id      TEXT NOT NULL,
  turn_id      TEXT NOT NULL UNIQUE,
  request_json TEXT NOT NULL,
  created_at   INTEGER NOT NULL,
  state        TEXT NOT NULL DEFAULT 'queued',
  claimed_at   INTEGER,
  claim_token  TEXT,
  claim_epoch  TEXT,
  claim_attempts INTEGER NOT NULL DEFAULT 0,
  guide_id TEXT,
  automation_run_id TEXT,
  automation_node_id TEXT
);
CREATE INDEX IF NOT EXISTS idx_desktop_pending_turns_task_id
  ON desktop_pending_turns(task_id, id);
CREATE TABLE IF NOT EXISTS desktop_quarantined_turns (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  original_id    INTEGER NOT NULL,
  task_id        TEXT NOT NULL,
  turn_id        TEXT NOT NULL,
  request_json   TEXT NOT NULL,
  created_at     INTEGER NOT NULL,
  state          TEXT NOT NULL,
  claimed_at     INTEGER,
  claim_token    TEXT,
  claim_epoch    TEXT,
  claim_attempts INTEGER NOT NULL,
  guide_id       TEXT,
  automation_run_id TEXT,
  automation_node_id TEXT,
  reason_code    TEXT NOT NULL,
  reason_message TEXT NOT NULL,
  quarantined_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_desktop_quarantined_turns_task_id
  ON desktop_quarantined_turns(task_id, id);
"#;

const QUEUED_STATE: &str = "queued";
const CLAIMED_STATE: &str = "claimed";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersistedDesktopTurnState {
    Queued,
    Claimed,
}

impl PersistedDesktopTurnState {
    fn parse(value: &str) -> Result<Self, DesktopTurnQueueError> {
        match value {
            QUEUED_STATE => Ok(Self::Queued),
            CLAIMED_STATE => Ok(Self::Claimed),
            value => Err(DesktopTurnQueueError::InvalidStoredValue {
                field: "state",
                message: format!("unsupported turn queue state `{value}`"),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistedDesktopTurn {
    pub turn_id: String,
    pub request: DesktopTurnRequest,
    pub state: PersistedDesktopTurnState,
    pub claim_token: Option<String>,
    pub claim_epoch: Option<String>,
    pub claim_attempts: u64,
}

#[cfg(debug_assertions)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistedDesktopTurnDebugState {
    pub turn_id: String,
    pub state: String,
    pub claim_epoch: Option<String>,
    pub claim_attempts: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuarantinedDesktopTurn {
    pub task_id: String,
    pub turn_id: String,
    pub original_state: String,
    pub guide_id: Option<String>,
    pub automation_run_id: Option<String>,
    pub automation_node_id: Option<String>,
    pub reason_code: String,
    pub quarantined_at: i64,
}

#[derive(Debug)]
struct RawDesktopTurn {
    id: i64,
    task_id: String,
    turn_id: String,
    request_json: String,
    created_at: i64,
    state: String,
    claimed_at: Option<i64>,
    claim_token: Option<String>,
    claim_epoch: Option<String>,
    claim_attempts: i64,
    guide_id: Option<String>,
    automation_run_id: Option<String>,
    automation_node_id: Option<String>,
}

pub struct DesktopTurnQueueStore {
    connection: Db,
}

impl DesktopTurnQueueStore {
    #[cfg(test)]
    pub fn open(path: &Path) -> Result<Self, DesktopTurnQueueError> {
        let connection = Db::open(path).map_err(|error| DesktopTurnQueueError::Storage {
            operation: "open turn queue database",
            message: error.to_string(),
        })?;
        Self::from_shared(connection)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self, DesktopTurnQueueError> {
        let connection = Db::in_memory().map_err(|error| DesktopTurnQueueError::Storage {
            operation: "open in-memory database",
            message: error.to_string(),
        })?;
        Self::from_shared(connection)
    }

    pub fn from_shared(connection: Db) -> Result<Self, DesktopTurnQueueError> {
        let locked = connection.lock();
        locked.execute_batch(TURN_QUEUE_SCHEMA).map_err(|error| {
            DesktopTurnQueueError::Storage {
                operation: "initialize turn queue schema",
                message: error.to_string(),
            }
        })?;
        ensure_column(
            &locked,
            "state",
            "ALTER TABLE desktop_pending_turns ADD COLUMN state TEXT NOT NULL DEFAULT 'queued'",
        )?;
        ensure_column(
            &locked,
            "claimed_at",
            "ALTER TABLE desktop_pending_turns ADD COLUMN claimed_at INTEGER",
        )?;
        ensure_column(
            &locked,
            "claim_token",
            "ALTER TABLE desktop_pending_turns ADD COLUMN claim_token TEXT",
        )?;
        ensure_column(
            &locked,
            "claim_epoch",
            "ALTER TABLE desktop_pending_turns ADD COLUMN claim_epoch TEXT",
        )?;
        ensure_column(
            &locked,
            "claim_attempts",
            "ALTER TABLE desktop_pending_turns ADD COLUMN claim_attempts INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_column(
            &locked,
            "guide_id",
            "ALTER TABLE desktop_pending_turns ADD COLUMN guide_id TEXT",
        )?;
        ensure_column(
            &locked,
            "automation_run_id",
            "ALTER TABLE desktop_pending_turns ADD COLUMN automation_run_id TEXT",
        )?;
        ensure_column(
            &locked,
            "automation_node_id",
            "ALTER TABLE desktop_pending_turns ADD COLUMN automation_node_id TEXT",
        )?;
        ensure_quarantine_column(
            &locked,
            "guide_id",
            "ALTER TABLE desktop_quarantined_turns ADD COLUMN guide_id TEXT",
        )?;
        ensure_quarantine_column(
            &locked,
            "automation_run_id",
            "ALTER TABLE desktop_quarantined_turns ADD COLUMN automation_run_id TEXT",
        )?;
        ensure_quarantine_column(
            &locked,
            "automation_node_id",
            "ALTER TABLE desktop_quarantined_turns ADD COLUMN automation_node_id TEXT",
        )?;
        locked
            .execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_desktop_pending_turns_state
                 ON desktop_pending_turns(task_id, state, id);
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_desktop_pending_turns_one_claim
                 ON desktop_pending_turns(task_id) WHERE state = 'claimed';",
            )
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "initialize turn queue state index",
                message: error.to_string(),
            })?;
        drop(locked);
        Ok(Self { connection })
    }

    pub fn enqueue(
        &self,
        turn_id: &str,
        request: &DesktopTurnRequest,
    ) -> Result<(), DesktopTurnQueueError> {
        let connection = self.connection();
        Self::enqueue_in(&connection, turn_id, request)
    }

    pub fn enqueue_in(
        connection: &Connection,
        turn_id: &str,
        request: &DesktopTurnRequest,
    ) -> Result<(), DesktopTurnQueueError> {
        let request_json = serde_json::to_string(request).map_err(|error| {
            DesktopTurnQueueError::Serialization {
                field: "request",
                message: error.to_string(),
            }
        })?;
        connection
            .execute(
                r#"INSERT INTO desktop_pending_turns
                   (task_id, turn_id, request_json, created_at, state, claimed_at,
                    claim_token, claim_epoch, claim_attempts, guide_id,
                    automation_run_id, automation_node_id)
                   VALUES (?1, ?2, ?3, ?4, 'queued', NULL, NULL, NULL, 0, ?5, ?6, ?7)"#,
                params![
                    request.task_id.as_str(),
                    turn_id,
                    request_json,
                    now_millis(),
                    request.guide_id.as_deref(),
                    request
                        .automation
                        .as_ref()
                        .map(|value| value.run_id.as_str()),
                    request
                        .automation
                        .as_ref()
                        .map(|value| value.node_id.as_str()),
                ],
            )
            .map(|_| ())
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "enqueue pending turn",
                message: error.to_string(),
            })
    }

    pub fn enqueue_idempotent(
        &self,
        turn_id: &str,
        request: &DesktopTurnRequest,
    ) -> Result<bool, DesktopTurnQueueError> {
        let request_json = serde_json::to_string(request).map_err(|error| {
            DesktopTurnQueueError::Serialization {
                field: "request",
                message: error.to_string(),
            }
        })?;
        let connection = self.connection();
        let changed = connection
            .execute(
                r#"INSERT INTO desktop_pending_turns
                   (task_id, turn_id, request_json, created_at, state, claimed_at,
                    claim_token, claim_epoch, claim_attempts, guide_id,
                    automation_run_id, automation_node_id)
                   VALUES (?1, ?2, ?3, ?4, 'queued', NULL, NULL, NULL, 0, ?5, ?6, ?7)
                   ON CONFLICT(turn_id) DO NOTHING"#,
                params![
                    request.task_id.as_str(),
                    turn_id,
                    request_json,
                    now_millis(),
                    request.guide_id.as_deref(),
                    request
                        .automation
                        .as_ref()
                        .map(|value| value.run_id.as_str()),
                    request
                        .automation
                        .as_ref()
                        .map(|value| value.node_id.as_str()),
                ],
            )
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "idempotently enqueue pending turn",
                message: error.to_string(),
            })?;
        if changed > 0 {
            return Ok(true);
        }
        let existing = connection
            .query_row(
                "SELECT task_id, request_json FROM desktop_pending_turns WHERE turn_id = ?1",
                params![turn_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "verify idempotent pending turn",
                message: error.to_string(),
            })?;
        let Some((task_id, existing_request_json)) = existing else {
            return Err(DesktopTurnQueueError::InvalidStoredValue {
                field: "turn_id",
                message: format!("turn `{turn_id}` conflicted but could not be reloaded"),
            });
        };
        if task_id != request.task_id.as_str() || existing_request_json != request_json {
            return Err(DesktopTurnQueueError::IdempotencyConflict {
                turn_id: turn_id.to_owned(),
            });
        }
        Ok(false)
    }

    pub fn update_request(
        &self,
        turn_id: &str,
        request: &DesktopTurnRequest,
    ) -> Result<(), DesktopTurnQueueError> {
        let request_json = serde_json::to_string(request).map_err(|error| {
            DesktopTurnQueueError::Serialization {
                field: "request",
                message: error.to_string(),
            }
        })?;
        let changed = self
            .connection()
            .execute(
                r#"UPDATE desktop_pending_turns
                   SET request_json = ?3, guide_id = ?4,
                       automation_run_id = ?5, automation_node_id = ?6
                   WHERE task_id = ?1 AND turn_id = ?2
                     AND state IN ('queued', 'claimed')"#,
                params![
                    request.task_id.as_str(),
                    turn_id,
                    request_json,
                    request.guide_id.as_deref(),
                    request
                        .automation
                        .as_ref()
                        .map(|value| value.run_id.as_str()),
                    request
                        .automation
                        .as_ref()
                        .map(|value| value.node_id.as_str()),
                ],
            )
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "update pending turn request",
                message: error.to_string(),
            })?;
        if changed != 1 {
            return Err(DesktopTurnQueueError::InvalidTransition {
                turn_id: turn_id.to_owned(),
                state: "missing_or_terminal".to_owned(),
                operation: "update its durable request",
            });
        }
        Ok(())
    }

    pub fn claim(
        &mut self,
        turn_id: &str,
        claim_epoch: &str,
    ) -> Result<Option<PersistedDesktopTurn>, DesktopTurnQueueError> {
        let mut connection = self.connection();
        let transaction = connection
            .transaction()
            .map_err(|error| storage_error("begin pending turn claim", error))?;
        let row = transaction
            .query_row(
                r#"SELECT id, task_id, request_json, state, claim_attempts
                   FROM desktop_pending_turns WHERE turn_id = ?1"#,
                params![turn_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| storage_error("read pending turn claim candidate", error))?;
        let Some((id, task_id, request_json, state, claim_attempts)) = row else {
            transaction
                .commit()
                .map_err(|error| storage_error("commit missing pending turn claim", error))?;
            return Ok(None);
        };
        if PersistedDesktopTurnState::parse(&state)? != PersistedDesktopTurnState::Queued {
            transaction
                .commit()
                .map_err(|error| storage_error("commit repeated pending turn claim", error))?;
            return Ok(None);
        }
        let request = decode_request(&task_id, &request_json)?;
        let claim_attempts = decode_claim_attempts(claim_attempts)?;
        let first_id = transaction
            .query_row(
                "SELECT id FROM desktop_pending_turns WHERE task_id = ?1 ORDER BY id ASC LIMIT 1",
                params![task_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| storage_error("read first pending turn claim candidate", error))?;
        if first_id != id {
            transaction
                .commit()
                .map_err(|error| storage_error("commit deferred pending turn claim", error))?;
            return Ok(None);
        }
        let claim_token = format!("turn-claim-{}", Uuid::new_v4());
        let changed = transaction
            .execute(
                r#"UPDATE desktop_pending_turns
                   SET state = 'claimed', claimed_at = ?2, claim_token = ?3,
                       claim_epoch = ?4, claim_attempts = claim_attempts + 1
                   WHERE id = ?1 AND state = 'queued'"#,
                params![id, now_millis(), claim_token.as_str(), claim_epoch],
            )
            .map_err(|error| storage_error("claim pending turn", error))?;
        if changed != 1 {
            return Err(DesktopTurnQueueError::InvalidTransition {
                turn_id: turn_id.to_owned(),
                state: "changed_concurrently".to_owned(),
                operation: "claim pending turn",
            });
        }
        transaction
            .commit()
            .map_err(|error| storage_error("commit pending turn claim", error))?;
        Ok(Some(PersistedDesktopTurn {
            turn_id: turn_id.to_owned(),
            request,
            state: PersistedDesktopTurnState::Claimed,
            claim_token: Some(claim_token),
            claim_epoch: Some(claim_epoch.to_owned()),
            claim_attempts: claim_attempts.saturating_add(1),
        }))
    }

    pub fn quarantine_invalid_rows(
        &mut self,
    ) -> Result<Vec<QuarantinedDesktopTurn>, DesktopTurnQueueError> {
        let mut connection = self.connection();
        let transaction = connection
            .transaction()
            .map_err(|error| storage_error("begin invalid pending turn quarantine", error))?;
        let rows = read_all_raw_turns(&transaction)?;
        let mut quarantined = Vec::new();
        for row in rows {
            let validation = TaskId::new(row.task_id.clone())
                .map_err(|error| DesktopTurnQueueError::InvalidStoredValue {
                    field: "task_id",
                    message: error.to_string(),
                })
                .and_then(|task_id| decode_raw_turn(&row, &task_id));
            match validation {
                Ok(decoded) => {
                    let automation_run_id = decoded
                        .request
                        .automation
                        .as_ref()
                        .map(|value| value.run_id.as_str());
                    let automation_node_id = decoded
                        .request
                        .automation
                        .as_ref()
                        .map(|value| value.node_id.as_str());
                    if row.guide_id.as_deref() != decoded.request.guide_id.as_deref()
                        || row.automation_run_id.as_deref() != automation_run_id
                        || row.automation_node_id.as_deref() != automation_node_id
                    {
                        transaction
                            .execute(
                                r#"UPDATE desktop_pending_turns
                                   SET guide_id = ?2, automation_run_id = ?3,
                                       automation_node_id = ?4
                                   WHERE id = ?1"#,
                                params![
                                    row.id,
                                    decoded.request.guide_id.as_deref(),
                                    automation_run_id,
                                    automation_node_id,
                                ],
                            )
                            .map_err(|error| {
                                storage_error("backfill pending turn recovery metadata", error)
                            })?;
                    }
                }
                Err(error @ DesktopTurnQueueError::InvalidStoredValue { .. }) => {
                    quarantined.push(quarantine_raw_turn(&transaction, &row, &error)?);
                }
                Err(error) => return Err(error),
            }
        }
        transaction
            .commit()
            .map_err(|error| storage_error("commit invalid pending turn quarantine", error))?;
        Ok(quarantined)
    }

    pub fn list_quarantined(&self) -> Result<Vec<QuarantinedDesktopTurn>, DesktopTurnQueueError> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                r#"SELECT task_id, turn_id, state, guide_id, automation_run_id,
                          automation_node_id, reason_code, quarantined_at
                   FROM desktop_quarantined_turns
                   ORDER BY id ASC"#,
            )
            .map_err(|error| storage_error("prepare quarantined turn list", error))?;
        let rows = statement
            .query_map([], |row| {
                Ok(QuarantinedDesktopTurn {
                    task_id: row.get(0)?,
                    turn_id: row.get(1)?,
                    original_state: row.get(2)?,
                    guide_id: row.get(3)?,
                    automation_run_id: row.get(4)?,
                    automation_node_id: row.get(5)?,
                    reason_code: row.get(6)?,
                    quarantined_at: row.get(7)?,
                })
            })
            .map_err(|error| storage_error("query quarantined turn list", error))?;
        rows.map(|row| row.map_err(|error| storage_error("decode quarantined turn list", error)))
            .collect()
    }

    #[cfg(debug_assertions)]
    pub fn corrupt_request_for_debug(&self, turn_id: &str) -> Result<bool, DesktopTurnQueueError> {
        self.connection()
            .execute(
                r#"UPDATE desktop_pending_turns
                   SET request_json = '{"debugCorrupt":'
                   WHERE turn_id = ?1 AND state = 'queued'"#,
                params![turn_id],
            )
            .map(|changed| changed == 1)
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "inject invalid pending turn request",
                message: error.to_string(),
            })
    }

    pub fn ack_and_claim_next(
        &mut self,
        task_id: &TaskId,
        turn_id: &str,
        claim_token: &str,
        claim_epoch: &str,
    ) -> Result<Option<PersistedDesktopTurn>, DesktopTurnQueueError> {
        let mut connection = self.connection();
        let transaction = connection
            .transaction()
            .map_err(|error| storage_error("begin pending turn acknowledgement", error))?;
        let changed = transaction
            .execute(
                r#"DELETE FROM desktop_pending_turns
                   WHERE task_id = ?1 AND turn_id = ?2 AND state = 'claimed'
                     AND claim_token = ?3"#,
                params![task_id.as_str(), turn_id, claim_token],
            )
            .map_err(|error| storage_error("acknowledge pending turn", error))?;
        if changed != 1 {
            return Err(DesktopTurnQueueError::ClaimOwnership {
                turn_id: turn_id.to_owned(),
            });
        }
        let next = claim_first_in_transaction(&transaction, task_id, claim_epoch)?;
        transaction
            .commit()
            .map_err(|error| storage_error("commit pending turn acknowledgement", error))?;
        Ok(next)
    }

    pub fn claim_first(
        &mut self,
        task_id: &TaskId,
        claim_epoch: &str,
    ) -> Result<Option<PersistedDesktopTurn>, DesktopTurnQueueError> {
        let mut connection = self.connection();
        let transaction = connection
            .transaction()
            .map_err(|error| storage_error("begin first pending turn claim", error))?;
        let next = claim_first_in_transaction(&transaction, task_id, claim_epoch)?;
        transaction
            .commit()
            .map_err(|error| storage_error("commit first pending turn claim", error))?;
        Ok(next)
    }

    pub fn discard_queued_and_claim_next(
        &mut self,
        task_id: &TaskId,
        turn_id: &str,
        claim_epoch: &str,
    ) -> Result<Option<PersistedDesktopTurn>, DesktopTurnQueueError> {
        let mut connection = self.connection();
        let transaction = connection
            .transaction()
            .map_err(|error| storage_error("begin prepared turn discard", error))?;
        let changed = transaction
            .execute(
                r#"DELETE FROM desktop_pending_turns
                   WHERE task_id = ?1 AND turn_id = ?2 AND state = 'queued'"#,
                params![task_id.as_str(), turn_id],
            )
            .map_err(|error| storage_error("discard prepared turn", error))?;
        if changed != 1 {
            return Err(DesktopTurnQueueError::InvalidTransition {
                turn_id: turn_id.to_owned(),
                state: "not_queued".to_owned(),
                operation: "discard prepared turn",
            });
        }
        let next = claim_first_in_transaction(&transaction, task_id, claim_epoch)?;
        transaction
            .commit()
            .map_err(|error| storage_error("commit prepared turn discard", error))?;
        Ok(next)
    }

    pub fn prepare_recovery(
        &mut self,
        task_id: &TaskId,
        active_turn_id: Option<&str>,
        claim_epoch: &str,
    ) -> Result<Option<String>, DesktopTurnQueueError> {
        let mut connection = self.connection();
        let transaction = connection
            .transaction()
            .map_err(|error| storage_error("begin pending turn recovery", error))?;
        if let Some(active_turn_id) = active_turn_id {
            let position = transaction
                .query_row(
                    r#"SELECT id,
                              (SELECT MIN(candidate.id) FROM desktop_pending_turns candidate
                               WHERE candidate.task_id = desktop_pending_turns.task_id)
                       FROM desktop_pending_turns
                       WHERE task_id = ?1 AND turn_id = ?2"#,
                    params![task_id.as_str(), active_turn_id],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()
                .map_err(|error| storage_error("validate active pending turn order", error))?;
            if position.is_some_and(|(active_id, first_id)| active_id != first_id) {
                return Err(DesktopTurnQueueError::InvalidStoredValue {
                    field: "active_turn_id",
                    message: format!(
                        "projected active turn `{active_turn_id}` is not the first durable turn"
                    ),
                });
            }
        }
        transaction
            .execute(
                r#"UPDATE desktop_pending_turns
                   SET state = 'queued', claimed_at = NULL, claim_token = NULL, claim_epoch = NULL
                   WHERE task_id = ?1 AND state = 'claimed'"#,
                params![task_id.as_str()],
            )
            .map_err(|error| storage_error("release stale pending turn claims", error))?;
        let mut active_claim_token = None;
        if let Some(active_turn_id) = active_turn_id {
            let claim_token = format!("turn-claim-{}", Uuid::new_v4());
            transaction
                .execute(
                    r#"UPDATE desktop_pending_turns
                       SET state = 'claimed', claimed_at = ?3, claim_token = ?4,
                           claim_epoch = ?5, claim_attempts = claim_attempts + 1
                       WHERE task_id = ?1 AND turn_id = ?2"#,
                    params![
                        task_id.as_str(),
                        active_turn_id,
                        now_millis(),
                        claim_token.as_str(),
                        claim_epoch
                    ],
                )
                .map_err(|error| storage_error("retain active pending turn claim", error))?;
            let exists = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM desktop_pending_turns WHERE task_id = ?1 AND turn_id = ?2)",
                    params![task_id.as_str(), active_turn_id],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|error| storage_error("verify active pending turn claim", error))?;
            if exists {
                active_claim_token = Some(claim_token);
            }
        }
        transaction
            .commit()
            .map_err(|error| storage_error("commit pending turn recovery", error))?;
        Ok(active_claim_token)
    }

    pub fn remove(&self, turn_id: &str) -> Result<bool, DesktopTurnQueueError> {
        self.connection()
            .execute(
                "DELETE FROM desktop_pending_turns WHERE turn_id = ?1",
                params![turn_id],
            )
            .map(|changed| changed > 0)
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "remove pending turn",
                message: error.to_string(),
            })
    }

    pub fn contains(&self, turn_id: &str) -> Result<bool, DesktopTurnQueueError> {
        self.connection()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM desktop_pending_turns WHERE turn_id = ?1)",
                params![turn_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "inspect pending turn",
                message: error.to_string(),
            })
    }

    pub fn discard_queued(
        &self,
        task_id: &TaskId,
        turn_id: &str,
    ) -> Result<bool, DesktopTurnQueueError> {
        self.connection()
            .execute(
                r#"DELETE FROM desktop_pending_turns
                   WHERE task_id = ?1 AND turn_id = ?2 AND state = 'queued'"#,
                params![task_id.as_str(), turn_id],
            )
            .map(|changed| changed > 0)
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "discard queued turn",
                message: error.to_string(),
            })
    }

    pub fn cancel_claim_and_clear_task(
        &mut self,
        task_id: &TaskId,
        turn_id: &str,
        claim_token: Option<&str>,
    ) -> Result<usize, DesktopTurnQueueError> {
        let mut connection = self.connection();
        let transaction = connection
            .transaction()
            .map_err(|error| storage_error("begin task turn cancellation", error))?;
        let current = transaction
            .query_row(
                "SELECT state, claim_token FROM desktop_pending_turns WHERE task_id = ?1 AND turn_id = ?2",
                params![task_id.as_str(), turn_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(|error| storage_error("read task turn cancellation owner", error))?;
        if let Some((state, stored_token)) = current {
            let owns_claim = PersistedDesktopTurnState::parse(&state)?
                == PersistedDesktopTurnState::Claimed
                && stored_token.as_deref() == claim_token
                && claim_token.is_some();
            if !owns_claim {
                return Err(DesktopTurnQueueError::ClaimOwnership {
                    turn_id: turn_id.to_owned(),
                });
            }
        } else if claim_token.is_some() {
            return Err(DesktopTurnQueueError::ClaimOwnership {
                turn_id: turn_id.to_owned(),
            });
        }
        let changed = transaction
            .execute(
                "DELETE FROM desktop_pending_turns WHERE task_id = ?1",
                params![task_id.as_str()],
            )
            .map_err(|error| storage_error("clear cancelled task turns", error))?;
        transaction
            .commit()
            .map_err(|error| storage_error("commit task turn cancellation", error))?;
        Ok(changed)
    }

    pub fn list_task_ids(&self) -> Result<Vec<TaskId>, DesktopTurnQueueError> {
        let connection = self.connection();
        let mut statement = connection
            .prepare("SELECT DISTINCT task_id FROM desktop_pending_turns ORDER BY task_id ASC")
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "prepare pending turn task list",
                message: error.to_string(),
            })?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "query pending turn task list",
                message: error.to_string(),
            })?;
        rows.map(|row| {
            let value = row.map_err(|error| DesktopTurnQueueError::Storage {
                operation: "decode pending turn task list",
                message: error.to_string(),
            })?;
            TaskId::new(value).map_err(|error| DesktopTurnQueueError::InvalidStoredValue {
                field: "task_id",
                message: error.to_string(),
            })
        })
        .collect()
    }

    pub fn list(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<PersistedDesktopTurn>, DesktopTurnQueueError> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                r#"SELECT turn_id, request_json, state, claim_token, claim_epoch, claim_attempts
                   FROM desktop_pending_turns
                   WHERE task_id = ?1
                   ORDER BY id ASC"#,
            )
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "prepare pending turn list",
                message: error.to_string(),
            })?;
        let rows = statement
            .query_map(params![task_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "query pending turn list",
                message: error.to_string(),
            })?;
        rows.map(|row| {
            let (turn_id, request_json, state, claim_token, claim_epoch, claim_attempts) = row
                .map_err(|error| DesktopTurnQueueError::Storage {
                    operation: "decode pending turn list",
                    message: error.to_string(),
                })?;
            let request = decode_request(task_id.as_str(), &request_json)?;
            Ok(PersistedDesktopTurn {
                turn_id,
                request,
                state: PersistedDesktopTurnState::parse(&state)?,
                claim_token,
                claim_epoch,
                claim_attempts: decode_claim_attempts(claim_attempts)?,
            })
        })
        .collect()
    }

    #[cfg(debug_assertions)]
    pub fn list_debug(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<PersistedDesktopTurnDebugState>, DesktopTurnQueueError> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                r#"SELECT turn_id, state, claim_epoch, claim_attempts
                   FROM desktop_pending_turns
                   WHERE task_id = ?1
                   ORDER BY id ASC"#,
            )
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "prepare debug pending turn list",
                message: error.to_string(),
            })?;
        let rows = statement
            .query_map(params![task_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "query debug pending turn list",
                message: error.to_string(),
            })?;
        rows.map(|row| {
            let (turn_id, state, claim_epoch, claim_attempts) =
                row.map_err(|error| DesktopTurnQueueError::Storage {
                    operation: "decode debug pending turn list",
                    message: error.to_string(),
                })?;
            Ok(PersistedDesktopTurnDebugState {
                turn_id,
                state,
                claim_epoch,
                claim_attempts: decode_claim_attempts(claim_attempts)?,
            })
        })
        .collect()
    }

    pub fn clear_task(&self, task_id: &TaskId) -> Result<usize, DesktopTurnQueueError> {
        self.connection()
            .execute(
                "DELETE FROM desktop_pending_turns WHERE task_id = ?1",
                params![task_id.as_str()],
            )
            .map_err(|error| DesktopTurnQueueError::Storage {
                operation: "clear task pending turns",
                message: error.to_string(),
            })
    }

    fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.connection.lock()
    }
}

fn now_millis() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

fn ensure_column(
    connection: &Connection,
    column: &'static str,
    migration: &'static str,
) -> Result<(), DesktopTurnQueueError> {
    let exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('desktop_pending_turns') WHERE name = ?1)",
            params![column],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| DesktopTurnQueueError::Storage {
            operation: "inspect turn queue schema",
            message: error.to_string(),
        })?;
    if exists {
        return Ok(());
    }
    connection
        .execute_batch(migration)
        .map_err(|error| DesktopTurnQueueError::Storage {
            operation: "migrate turn queue schema",
            message: error.to_string(),
        })?;
    Ok(())
}

fn ensure_quarantine_column(
    connection: &Connection,
    column: &'static str,
    migration: &'static str,
) -> Result<(), DesktopTurnQueueError> {
    let exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('desktop_quarantined_turns') WHERE name = ?1)",
            params![column],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| DesktopTurnQueueError::Storage {
            operation: "inspect quarantined turn schema",
            message: error.to_string(),
        })?;
    if exists {
        return Ok(());
    }
    connection
        .execute_batch(migration)
        .map_err(|error| DesktopTurnQueueError::Storage {
            operation: "migrate quarantined turn schema",
            message: error.to_string(),
        })?;
    Ok(())
}

fn read_all_raw_turns(
    transaction: &Transaction<'_>,
) -> Result<Vec<RawDesktopTurn>, DesktopTurnQueueError> {
    let mut statement = transaction
        .prepare(
            r#"SELECT id, task_id, turn_id, request_json, created_at, state,
                      claimed_at, claim_token, claim_epoch, claim_attempts,
                      guide_id, automation_run_id, automation_node_id
               FROM desktop_pending_turns
               ORDER BY id ASC"#,
        )
        .map_err(|error| storage_error("prepare invalid pending turn scan", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok(RawDesktopTurn {
                id: row.get(0)?,
                task_id: row.get(1)?,
                turn_id: row.get(2)?,
                request_json: row.get(3)?,
                created_at: row.get(4)?,
                state: row.get(5)?,
                claimed_at: row.get(6)?,
                claim_token: row.get(7)?,
                claim_epoch: row.get(8)?,
                claim_attempts: row.get(9)?,
                guide_id: row.get(10)?,
                automation_run_id: row.get(11)?,
                automation_node_id: row.get(12)?,
            })
        })
        .map_err(|error| storage_error("query invalid pending turn scan", error))?;
    rows.map(|row| row.map_err(|error| storage_error("decode invalid pending turn scan", error)))
        .collect()
}

fn decode_raw_turn(
    row: &RawDesktopTurn,
    task_id: &TaskId,
) -> Result<PersistedDesktopTurn, DesktopTurnQueueError> {
    if row.turn_id.trim().is_empty() {
        return Err(DesktopTurnQueueError::InvalidStoredValue {
            field: "turn_id",
            message: "turn id must not be empty".to_owned(),
        });
    }
    Ok(PersistedDesktopTurn {
        turn_id: row.turn_id.clone(),
        request: decode_request(task_id.as_str(), &row.request_json)?,
        state: PersistedDesktopTurnState::parse(&row.state)?,
        claim_token: row.claim_token.clone(),
        claim_epoch: row.claim_epoch.clone(),
        claim_attempts: decode_claim_attempts(row.claim_attempts)?,
    })
}

fn quarantine_raw_turn(
    transaction: &Transaction<'_>,
    row: &RawDesktopTurn,
    error: &DesktopTurnQueueError,
) -> Result<QuarantinedDesktopTurn, DesktopTurnQueueError> {
    let DesktopTurnQueueError::InvalidStoredValue { field, message } = error else {
        return Err(DesktopTurnQueueError::InvalidStoredValue {
            field: "quarantine_reason",
            message: "only invalid stored values may be quarantined".to_owned(),
        });
    };
    let quarantined_at = now_millis();
    let reason_code = format!("invalid_{field}").replace('.', "_");
    transaction
        .execute(
            r#"INSERT INTO desktop_quarantined_turns (
                 original_id, task_id, turn_id, request_json, created_at, state,
                 claimed_at, claim_token, claim_epoch, claim_attempts,
                 guide_id, automation_run_id, automation_node_id,
                 reason_code, reason_message, quarantined_at
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                         ?14, ?15, ?16)"#,
            params![
                row.id,
                row.task_id.as_str(),
                row.turn_id.as_str(),
                row.request_json.as_str(),
                row.created_at,
                row.state.as_str(),
                row.claimed_at,
                row.claim_token.as_deref(),
                row.claim_epoch.as_deref(),
                row.claim_attempts,
                row.guide_id.as_deref(),
                row.automation_run_id.as_deref(),
                row.automation_node_id.as_deref(),
                reason_code.as_str(),
                message.as_str(),
                quarantined_at,
            ],
        )
        .map_err(|error| storage_error("preserve invalid pending turn", error))?;
    let changed = transaction
        .execute(
            "DELETE FROM desktop_pending_turns WHERE id = ?1",
            params![row.id],
        )
        .map_err(|error| storage_error("remove quarantined pending turn", error))?;
    if changed != 1 {
        return Err(DesktopTurnQueueError::InvalidTransition {
            turn_id: row.turn_id.clone(),
            state: "missing".to_owned(),
            operation: "quarantine invalid pending turn",
        });
    }
    Ok(QuarantinedDesktopTurn {
        task_id: row.task_id.clone(),
        turn_id: row.turn_id.clone(),
        original_state: row.state.clone(),
        guide_id: row.guide_id.clone(),
        automation_run_id: row.automation_run_id.clone(),
        automation_node_id: row.automation_node_id.clone(),
        reason_code,
        quarantined_at,
    })
}

fn claim_first_in_transaction(
    transaction: &Transaction<'_>,
    task_id: &TaskId,
    claim_epoch: &str,
) -> Result<Option<PersistedDesktopTurn>, DesktopTurnQueueError> {
    let Some(mut next) = read_first_turn(transaction, task_id)? else {
        return Ok(None);
    };
    if next.state != PersistedDesktopTurnState::Queued {
        return Err(DesktopTurnQueueError::InvalidTransition {
            turn_id: next.turn_id,
            state: format!("{:?}", next.state).to_lowercase(),
            operation: "claim next pending turn",
        });
    }
    let claim_token = format!("turn-claim-{}", Uuid::new_v4());
    let changed = transaction
        .execute(
            r#"UPDATE desktop_pending_turns
               SET state = 'claimed', claimed_at = ?2, claim_token = ?3,
                   claim_epoch = ?4, claim_attempts = claim_attempts + 1
               WHERE turn_id = ?1 AND state = 'queued'"#,
            params![
                next.turn_id.as_str(),
                now_millis(),
                claim_token.as_str(),
                claim_epoch
            ],
        )
        .map_err(|error| storage_error("claim next pending turn", error))?;
    if changed != 1 {
        return Err(DesktopTurnQueueError::InvalidTransition {
            turn_id: next.turn_id,
            state: "changed_concurrently".to_owned(),
            operation: "claim next pending turn",
        });
    }
    next.state = PersistedDesktopTurnState::Claimed;
    next.claim_token = Some(claim_token);
    next.claim_epoch = Some(claim_epoch.to_owned());
    next.claim_attempts = next.claim_attempts.saturating_add(1);
    Ok(Some(next))
}

fn read_first_turn(
    transaction: &Transaction<'_>,
    task_id: &TaskId,
) -> Result<Option<PersistedDesktopTurn>, DesktopTurnQueueError> {
    let row = transaction
        .query_row(
            r#"SELECT turn_id, request_json, state, claim_token, claim_epoch, claim_attempts
               FROM desktop_pending_turns
               WHERE task_id = ?1
               ORDER BY id ASC
               LIMIT 1"#,
            params![task_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| storage_error("read next pending turn", error))?;
    row.map(
        |(turn_id, request_json, state, claim_token, claim_epoch, claim_attempts)| {
            let request = decode_request(task_id.as_str(), &request_json)?;
            Ok(PersistedDesktopTurn {
                turn_id,
                request,
                state: PersistedDesktopTurnState::parse(&state)?,
                claim_token,
                claim_epoch,
                claim_attempts: decode_claim_attempts(claim_attempts)?,
            })
        },
    )
    .transpose()
}

fn decode_request(
    expected_task_id: &str,
    request_json: &str,
) -> Result<DesktopTurnRequest, DesktopTurnQueueError> {
    let request: DesktopTurnRequest = serde_json::from_str(request_json).map_err(|error| {
        DesktopTurnQueueError::InvalidStoredValue {
            field: "request_json",
            message: error.to_string(),
        }
    })?;
    if request.task_id.as_str() != expected_task_id {
        return Err(DesktopTurnQueueError::InvalidStoredValue {
            field: "request.task_id",
            message: format!(
                "queued request belongs to `{}`, expected `{expected_task_id}`",
                request.task_id.as_str()
            ),
        });
    }
    Ok(request)
}

fn decode_claim_attempts(value: i64) -> Result<u64, DesktopTurnQueueError> {
    u64::try_from(value).map_err(|_| DesktopTurnQueueError::InvalidStoredValue {
        field: "claim_attempts",
        message: format!("claim attempts must be non-negative, got {value}"),
    })
}

fn storage_error(operation: &'static str, error: rusqlite::Error) -> DesktopTurnQueueError {
    DesktopTurnQueueError::Storage {
        operation,
        message: error.to_string(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopTurnQueueError {
    #[error("turn queue serialization failed for {field}: {message}")]
    Serialization {
        field: &'static str,
        message: String,
    },
    #[error("turn queue contains invalid {field}: {message}")]
    InvalidStoredValue {
        field: &'static str,
        message: String,
    },
    #[error("turn queue storage failed during {operation}: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },
    #[error("turn queue idempotency key `{turn_id}` belongs to a different request")]
    IdempotencyConflict { turn_id: String },
    #[error("turn `{turn_id}` cannot {operation} while it is `{state}`")]
    InvalidTransition {
        turn_id: String,
        state: String,
        operation: &'static str,
    },
    #[error("turn `{turn_id}` is not owned by the current durable claim")]
    ClaimOwnership { turn_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_turns_roundtrip_in_fifo_order_and_remove_individually() {
        let store = DesktopTurnQueueStore::in_memory().unwrap();
        let task_id = TaskId::new("queue-task").unwrap();
        let first = DesktopTurnRequest::new(task_id.clone(), "first");
        let second = DesktopTurnRequest::new(task_id.clone(), "second");

        store.enqueue("turn-1", &first).unwrap();
        store.enqueue("turn-2", &second).unwrap();

        assert_eq!(store.list_task_ids().unwrap(), vec![task_id.clone()]);
        assert_eq!(
            store
                .list(&task_id)
                .unwrap()
                .into_iter()
                .map(|turn| (turn.turn_id, turn.request.content))
                .collect::<Vec<_>>(),
            vec![
                ("turn-1".to_owned(), "first".to_owned()),
                ("turn-2".to_owned(), "second".to_owned())
            ]
        );
        assert!(store.remove("turn-1").unwrap());
        assert!(!store.remove("turn-1").unwrap());
        assert_eq!(store.clear_task(&task_id).unwrap(), 1);
        assert!(store.list_task_ids().unwrap().is_empty());
    }

    #[test]
    fn corrupt_row_is_quarantined_without_blocking_remaining_fifo_rows() {
        let mut store = DesktopTurnQueueStore::in_memory().unwrap();
        let task_id = TaskId::new("quarantine-task").unwrap();
        let mut corrupt_request = DesktopTurnRequest::new(task_id.clone(), "corrupt me");
        corrupt_request.guide_id = Some("guide-corrupt".to_owned());
        corrupt_request.automation = Some(crate::DesktopAutomationTurnCorrelation {
            run_id: "run-corrupt".to_owned(),
            node_id: "node-corrupt".to_owned(),
        });
        store.enqueue("turn-corrupt", &corrupt_request).unwrap();
        for (turn_id, content) in [
            ("turn-valid-1", "first valid"),
            ("turn-valid-2", "second valid"),
        ] {
            store
                .enqueue(turn_id, &DesktopTurnRequest::new(task_id.clone(), content))
                .unwrap();
        }
        store
            .connection()
            .execute(
                "UPDATE desktop_pending_turns SET request_json = ?2 WHERE turn_id = ?1",
                params!["turn-corrupt", "{not-json"],
            )
            .unwrap();

        let quarantined = store.quarantine_invalid_rows().unwrap();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(quarantined[0].task_id, task_id.as_str());
        assert_eq!(quarantined[0].turn_id, "turn-corrupt");
        assert_eq!(quarantined[0].original_state, QUEUED_STATE);
        assert_eq!(quarantined[0].guide_id.as_deref(), Some("guide-corrupt"));
        assert_eq!(
            quarantined[0].automation_run_id.as_deref(),
            Some("run-corrupt")
        );
        assert_eq!(
            quarantined[0].automation_node_id.as_deref(),
            Some("node-corrupt")
        );
        assert_eq!(quarantined[0].reason_code, "invalid_request_json");
        assert_eq!(
            store
                .list(&task_id)
                .unwrap()
                .into_iter()
                .map(|turn| turn.turn_id)
                .collect::<Vec<_>>(),
            vec!["turn-valid-1".to_owned(), "turn-valid-2".to_owned()]
        );

        let first = store
            .claim_first(&task_id, "quarantine-epoch")
            .unwrap()
            .expect("first remaining turn");
        assert_eq!(first.turn_id, "turn-valid-1");
        let second = store
            .ack_and_claim_next(
                &task_id,
                first.turn_id.as_str(),
                first.claim_token.as_deref().unwrap(),
                "quarantine-epoch",
            )
            .unwrap()
            .expect("second remaining turn");
        assert_eq!(second.turn_id, "turn-valid-2");

        let diagnostics = store.list_quarantined().unwrap();
        assert_eq!(diagnostics, quarantined);
        let raw_request: String = store
            .connection()
            .query_row(
                "SELECT request_json FROM desktop_quarantined_turns WHERE turn_id = ?1",
                params!["turn-corrupt"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(raw_request, "{not-json");
    }

    #[test]
    fn direct_claim_validates_before_committing_claim_state() {
        let mut store = DesktopTurnQueueStore::in_memory().unwrap();
        let task_id = TaskId::new("claim-validation-task").unwrap();
        store
            .enqueue(
                "turn-corrupt",
                &DesktopTurnRequest::new(task_id.clone(), "corrupt me"),
            )
            .unwrap();
        store
            .connection()
            .execute(
                "UPDATE desktop_pending_turns SET request_json = ?2 WHERE turn_id = ?1",
                params!["turn-corrupt", "[]"],
            )
            .unwrap();

        assert!(matches!(
            store.claim("turn-corrupt", "claim-validation-epoch"),
            Err(DesktopTurnQueueError::InvalidStoredValue {
                field: "request_json",
                ..
            })
        ));
        let state: String = store
            .connection()
            .query_row(
                "SELECT state FROM desktop_pending_turns WHERE turn_id = ?1",
                params!["turn-corrupt"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(state, QUEUED_STATE);
    }

    #[test]
    fn idempotent_enqueue_reuses_only_the_same_request() {
        let store = DesktopTurnQueueStore::in_memory().unwrap();
        let task_id = TaskId::new("idempotent-queue-task").unwrap();
        let request = DesktopTurnRequest::new(task_id.clone(), "same request");

        assert!(store.enqueue_idempotent("stable-turn", &request).unwrap());
        assert!(!store.enqueue_idempotent("stable-turn", &request).unwrap());
        assert!(matches!(
            store.enqueue_idempotent(
                "stable-turn",
                &DesktopTurnRequest::new(task_id, "different request")
            ),
            Err(DesktopTurnQueueError::IdempotencyConflict { .. })
        ));
    }

    #[test]
    fn claimed_turn_request_can_be_durably_refined_before_execution() {
        let mut store = DesktopTurnQueueStore::in_memory().unwrap();
        let task_id = TaskId::new("refined-queue-task").unwrap();
        let request = DesktopTurnRequest::new(task_id.clone(), "original");
        store.enqueue("turn-refined", &request).unwrap();
        store.claim("turn-refined", "epoch-1").unwrap().unwrap();

        let mut refined = request;
        refined.model = Some("gpt-5.5".into());
        refined.auto_turn_decision_applied = true;
        store.update_request("turn-refined", &refined).unwrap();

        let restored = store.list(&task_id).unwrap().pop().unwrap();
        assert_eq!(restored.request.model.as_deref(), Some("gpt-5.5"));
        assert!(restored.request.auto_turn_decision_applied);
        assert_eq!(restored.state, PersistedDesktopTurnState::Claimed);
    }

    #[test]
    fn claim_is_fifo_and_ack_claims_the_next_turn_with_a_new_owner() {
        let mut store = DesktopTurnQueueStore::in_memory().unwrap();
        let task_id = TaskId::new("claimed-queue-task").unwrap();
        store
            .enqueue("turn-1", &DesktopTurnRequest::new(task_id.clone(), "first"))
            .unwrap();
        store
            .enqueue(
                "turn-2",
                &DesktopTurnRequest::new(task_id.clone(), "second"),
            )
            .unwrap();

        assert!(store.claim("turn-2", "epoch-1").unwrap().is_none());
        let first = store
            .claim("turn-1", "epoch-1")
            .unwrap()
            .expect("first claim");
        assert_eq!(first.state, PersistedDesktopTurnState::Claimed);
        assert_eq!(first.claim_epoch.as_deref(), Some("epoch-1"));
        assert_eq!(first.claim_attempts, 1);
        assert!(store.claim("turn-1", "epoch-1").unwrap().is_none());
        assert!(matches!(
            store.ack_and_claim_next(&task_id, "turn-1", "wrong-token", "epoch-1"),
            Err(DesktopTurnQueueError::ClaimOwnership { .. })
        ));

        let second = store
            .ack_and_claim_next(
                &task_id,
                "turn-1",
                first.claim_token.as_deref().unwrap(),
                "epoch-1",
            )
            .unwrap()
            .expect("second claim");
        assert_eq!(second.turn_id, "turn-2");
        assert_eq!(second.state, PersistedDesktopTurnState::Claimed);
        assert_ne!(second.claim_token, first.claim_token);
        assert_eq!(store.list(&task_id).unwrap(), vec![second.clone()]);
        assert!(store
            .ack_and_claim_next(
                &task_id,
                "turn-2",
                second.claim_token.as_deref().unwrap(),
                "epoch-1",
            )
            .unwrap()
            .is_none());
        assert!(store.list(&task_id).unwrap().is_empty());
    }

    #[test]
    fn stale_claim_recovery_preserves_turn_id_and_reassigns_ownership() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("desktop.db");
        let task_id = TaskId::new("claim-recovery-task").unwrap();
        let first_claim_token = {
            let mut store = DesktopTurnQueueStore::open(&path).unwrap();
            store
                .enqueue(
                    "turn-before-crash",
                    &DesktopTurnRequest::new(task_id.clone(), "resume me"),
                )
                .unwrap();
            store
                .claim("turn-before-crash", "epoch-before-crash")
                .unwrap()
                .unwrap()
                .claim_token
                .unwrap()
        };

        let mut restarted = DesktopTurnQueueStore::open(&path).unwrap();
        restarted
            .prepare_recovery(&task_id, None, "epoch-after-crash")
            .unwrap();
        let released = restarted.list(&task_id).unwrap();
        assert_eq!(released[0].state, PersistedDesktopTurnState::Queued);
        assert!(released[0].claim_token.is_none());
        let replay = restarted
            .claim("turn-before-crash", "epoch-after-crash")
            .unwrap()
            .unwrap();
        assert_eq!(replay.turn_id, "turn-before-crash");
        assert_eq!(replay.claim_attempts, 2);
        assert_ne!(
            replay.claim_token.as_deref(),
            Some(first_claim_token.as_str())
        );
        assert!(matches!(
            restarted.ack_and_claim_next(
                &task_id,
                "turn-before-crash",
                first_claim_token.as_str(),
                "epoch-after-crash",
            ),
            Err(DesktopTurnQueueError::ClaimOwnership { .. })
        ));
        let still_claimed = restarted.list(&task_id).unwrap();
        assert_eq!(still_claimed[0].state, PersistedDesktopTurnState::Claimed);
        assert_eq!(still_claimed[0].claim_token, replay.claim_token);

        let rebound = restarted
            .prepare_recovery(&task_id, Some("turn-before-crash"), "epoch-projected-wait")
            .unwrap()
            .expect("projected wait claim");
        let projected = restarted.list(&task_id).unwrap();
        assert_eq!(projected[0].state, PersistedDesktopTurnState::Claimed);
        assert_eq!(projected[0].claim_token.as_deref(), Some(rebound.as_str()));
        assert_eq!(
            projected[0].claim_epoch.as_deref(),
            Some("epoch-projected-wait")
        );
        assert_eq!(projected[0].claim_attempts, 3);
    }

    #[test]
    fn recovery_rejects_a_projected_active_turn_that_is_not_fifo_head() {
        let mut store = DesktopTurnQueueStore::in_memory().unwrap();
        let task_id = TaskId::new("invalid-active-order-task").unwrap();
        store
            .enqueue(
                "turn-before-active",
                &DesktopTurnRequest::new(task_id.clone(), "first"),
            )
            .unwrap();
        store
            .enqueue(
                "turn-projected-active",
                &DesktopTurnRequest::new(task_id.clone(), "second"),
            )
            .unwrap();

        assert!(matches!(
            store.prepare_recovery(
                &task_id,
                Some("turn-projected-active"),
                "invalid-order-epoch"
            ),
            Err(DesktopTurnQueueError::InvalidStoredValue {
                field: "active_turn_id",
                ..
            })
        ));
        assert!(store
            .list(&task_id)
            .unwrap()
            .iter()
            .all(|turn| turn.state == PersistedDesktopTurnState::Queued));
    }

    #[test]
    fn cancellation_requires_the_current_claim_and_never_promotes_queue() {
        let mut store = DesktopTurnQueueStore::in_memory().unwrap();
        let task_id = TaskId::new("cancel-claimed-task").unwrap();
        for (turn_id, content) in [("turn-active", "active"), ("turn-queued", "queued")] {
            store
                .enqueue(turn_id, &DesktopTurnRequest::new(task_id.clone(), content))
                .unwrap();
        }
        let active = store.claim("turn-active", "cancel-epoch").unwrap().unwrap();
        assert!(matches!(
            store.cancel_claim_and_clear_task(&task_id, "turn-active", Some("wrong-claim-token")),
            Err(DesktopTurnQueueError::ClaimOwnership { .. })
        ));
        assert_eq!(store.list(&task_id).unwrap().len(), 2);

        assert_eq!(
            store
                .cancel_claim_and_clear_task(&task_id, "turn-active", active.claim_token.as_deref())
                .unwrap(),
            2
        );
        assert!(store.list(&task_id).unwrap().is_empty());
    }

    #[test]
    fn legacy_queue_schema_migrates_existing_rows_to_queued() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.db");
        let task_id = TaskId::new("legacy-queue-task").unwrap();
        let request = DesktopTurnRequest::new(task_id.clone(), "legacy request");
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    r#"CREATE TABLE desktop_pending_turns (
                         id INTEGER PRIMARY KEY AUTOINCREMENT,
                         task_id TEXT NOT NULL,
                         turn_id TEXT NOT NULL UNIQUE,
                         request_json TEXT NOT NULL,
                         created_at INTEGER NOT NULL
                       );"#,
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO desktop_pending_turns (task_id, turn_id, request_json, created_at) VALUES (?1, ?2, ?3, 1)",
                    params![task_id.as_str(), "legacy-turn", serde_json::to_string(&request).unwrap()],
                )
                .unwrap();
        }

        let store = DesktopTurnQueueStore::open(&path).unwrap();
        let rows = store.list(&task_id).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, PersistedDesktopTurnState::Queued);
        assert!(rows[0].claim_token.is_none());
        assert_eq!(rows[0].claim_attempts, 0);
    }

    #[test]
    fn pending_turns_survive_store_reopen_with_stable_turn_ids() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("desktop.db");
        let task_id = TaskId::new("restart-queue-task").unwrap();
        {
            let store = DesktopTurnQueueStore::open(&path).unwrap();
            let mut first = DesktopTurnRequest::new(task_id.clone(), "first");
            first.guide_id = Some("guide-before-restart".to_owned());
            store.enqueue("turn-before-restart-1", &first).unwrap();
            store
                .enqueue(
                    "turn-before-restart-2",
                    &DesktopTurnRequest::new(task_id.clone(), "second"),
                )
                .unwrap();
        }

        let restored = DesktopTurnQueueStore::open(&path)
            .unwrap()
            .list(&task_id)
            .unwrap();
        assert_eq!(
            restored
                .iter()
                .map(|turn| turn.turn_id.clone())
                .collect::<Vec<_>>(),
            vec![
                "turn-before-restart-1".to_owned(),
                "turn-before-restart-2".to_owned()
            ]
        );
        assert_eq!(
            restored[0].request.guide_id.as_deref(),
            Some("guide-before-restart")
        );
    }
}

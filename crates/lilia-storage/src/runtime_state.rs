use std::path::Path;
use std::sync::Mutex;

use lilia_contracts::{ProductError, ProductResult};
use rusqlite::{params, Connection};
use serde_json::Value;

const MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS agent_runtime_sessions (
  session_id TEXT PRIMARY KEY NOT NULL,
  payload TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

/// Opaque durable storage for Native AgentKit execution state.
///
/// Session/Wire semantics remain in AgentKit; this Host repository only supplies
/// atomic opaque persistence isolated from product projections and the legacy
/// Desktop cache.
pub struct SqliteAgentRuntimeStateStore {
    conn: Mutex<Connection>,
}

impl SqliteAgentRuntimeStateStore {
    pub fn open(path: impl AsRef<Path>) -> ProductResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| ProductError::Unavailable {
                message: format!("create agent runtime db dir: {error}"),
            })?;
        }
        let conn = Connection::open(path).map_err(db_error)?;
        Self::configure(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> ProductResult<Self> {
        let conn = Connection::open_in_memory().map_err(db_error)?;
        Self::configure(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn configure(conn: &Connection) -> ProductResult<()> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;\
             PRAGMA synchronous = NORMAL;\
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(db_error)?;
        conn.execute_batch(MIGRATION).map_err(db_error)
    }

    pub fn put_session(&self, session_id: &str, payload: &Value) -> ProductResult<()> {
        let payload =
            serde_json::to_string(payload).map_err(|error| ProductError::InvalidInput {
                field: "agent_runtime_session".into(),
                message: error.to_string(),
            })?;
        let conn = self.conn.lock().map_err(|_| ProductError::Unavailable {
            message: "agent runtime state store lock poisoned".into(),
        })?;
        conn.execute(
            r#"INSERT INTO agent_runtime_sessions(session_id, payload)
               VALUES (?1, ?2)
               ON CONFLICT(session_id) DO UPDATE SET
                 payload = excluded.payload,
                 updated_at = datetime('now')"#,
            params![session_id, payload],
        )
        .map_err(db_error)?;
        Ok(())
    }

    pub fn list_sessions(&self) -> ProductResult<Vec<(String, Value)>> {
        let conn = self.conn.lock().map_err(|_| ProductError::Unavailable {
            message: "agent runtime state store lock poisoned".into(),
        })?;
        let mut statement = conn
            .prepare("SELECT session_id, payload FROM agent_runtime_sessions ORDER BY session_id")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                let session_id: String = row.get(0)?;
                let payload: String = row.get(1)?;
                Ok((session_id, payload))
            })
            .map_err(db_error)?;
        rows.map(|row| {
            let (session_id, payload) = row.map_err(db_error)?;
            let payload =
                serde_json::from_str(&payload).map_err(|error| ProductError::Unavailable {
                    message: format!(
                        "decode agent runtime session `{session_id}` from sqlite: {error}"
                    ),
                })?;
            Ok((session_id, payload))
        })
        .collect()
    }

    pub fn delete_session(&self, session_id: &str) -> ProductResult<()> {
        let conn = self.conn.lock().map_err(|_| ProductError::Unavailable {
            message: "agent runtime state store lock poisoned".into(),
        })?;
        conn.execute(
            "DELETE FROM agent_runtime_sessions WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(db_error)?;
        Ok(())
    }
}

fn db_error(error: rusqlite::Error) -> ProductError {
    ProductError::Unavailable {
        message: format!("agent runtime sqlite: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn runtime_session_state_survives_reopen_and_updates_atomically() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lilia-agent-runtime-{nonce}"));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("agent_runtime.db");
        {
            let store = SqliteAgentRuntimeStateStore::open(&path).unwrap();
            store
                .put_session("session-1", &serde_json::json!({ "version": 1 }))
                .unwrap();
            store
                .put_session("session-1", &serde_json::json!({ "version": 2 }))
                .unwrap();
        }
        let reopened = SqliteAgentRuntimeStateStore::open(&path).unwrap();
        assert_eq!(
            reopened.list_sessions().unwrap(),
            vec![("session-1".to_string(), serde_json::json!({ "version": 2 }))]
        );
        reopened.delete_session("session-1").unwrap();
        assert!(reopened.list_sessions().unwrap().is_empty());
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }
}

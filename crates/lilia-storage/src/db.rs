//! One shared SQLite handle per database file, plus an idempotent migration
//! runner.
//!
//! Domains used to open the same `product.db` independently, which multiplied
//! WAL writers, made `PRAGMA foreign_keys` per-connection state and left schema
//! creation implicit in each store's constructor. A single [`Db`] removes all
//! three: one connection, one place that configures it, and one ordered ledger
//! of applied migrations.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::Connection;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const LEDGER: &str = r#"
CREATE TABLE IF NOT EXISTS lilia_migrations (
  id         TEXT PRIMARY KEY,
  applied_at INTEGER NOT NULL
);
"#;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database failed during {operation}: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },

    #[error("database integrity check failed: {message}")]
    Integrity { message: String },

    #[error("migration {id} failed: {message}")]
    Migration { id: &'static str, message: String },
}

impl DbError {
    fn storage(operation: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Storage {
            operation,
            message: error.to_string(),
        }
    }
}

/// One ordered, idempotent schema step. `id` is a stable name recorded in
/// `lilia_migrations`; `sql` is applied inside a single transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Migration {
    pub id: &'static str,
    pub sql: &'static str,
}

impl Migration {
    pub const fn new(id: &'static str, sql: &'static str) -> Self {
        Self { id, sql }
    }
}

/// Shared owner of one SQLite connection.
#[derive(Clone)]
pub struct Db {
    inner: Arc<DbInner>,
}

struct DbInner {
    path: Option<PathBuf>,
    connection: Mutex<Connection>,
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| DbError::storage("create database directory", error))?;
        }
        let connection =
            Connection::open(path).map_err(|error| DbError::storage("open database", error))?;
        configure(&connection)?;
        verify_integrity(&connection)?;
        connection
            .execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
            .map_err(|error| DbError::storage("configure database journal", error))?;
        Ok(Self::from_parts(Some(path.to_path_buf()), connection))
    }

    pub fn in_memory() -> Result<Self, DbError> {
        let connection = Connection::open_in_memory()
            .map_err(|error| DbError::storage("open in-memory database", error))?;
        configure(&connection)?;
        Ok(Self::from_parts(None, connection))
    }

    fn from_parts(path: Option<PathBuf>, connection: Connection) -> Self {
        Self {
            inner: Arc::new(DbInner {
                path,
                connection: Mutex::new(connection),
            }),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.inner.path.as_deref()
    }

    pub fn is_persistent(&self) -> bool {
        self.inner.path.is_some()
    }

    /// Applies every migration not yet recorded, in the given order, and returns
    /// the ids newly applied.
    pub fn migrate(&self, migrations: &[Migration]) -> Result<Vec<&'static str>, DbError> {
        let mut guard = self.lock();
        guard
            .execute_batch(LEDGER)
            .map_err(|error| DbError::storage("create migration ledger", error))?;
        let applied = read_applied(&guard)?;

        let mut newly = Vec::new();
        for migration in migrations {
            if applied.contains(migration.id) {
                continue;
            }
            let transaction = guard.transaction().map_err(|error| DbError::Migration {
                id: migration.id,
                message: error.to_string(),
            })?;
            transaction
                .execute_batch(migration.sql)
                .map_err(|error| DbError::Migration {
                    id: migration.id,
                    message: error.to_string(),
                })?;
            transaction
                .execute(
                    "INSERT INTO lilia_migrations (id, applied_at) VALUES (?1, ?2)",
                    rusqlite::params![migration.id, now_ms()],
                )
                .map_err(|error| DbError::Migration {
                    id: migration.id,
                    message: error.to_string(),
                })?;
            transaction.commit().map_err(|error| DbError::Migration {
                id: migration.id,
                message: error.to_string(),
            })?;
            newly.push(migration.id);
        }
        Ok(newly)
    }

    pub fn applied_migrations(&self) -> Result<Vec<String>, DbError> {
        let guard = self.lock();
        let exists = guard
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'lilia_migrations')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| DbError::storage("inspect migration ledger", error))?;
        if !exists {
            return Ok(Vec::new());
        }
        Ok(read_applied(&guard)?.into_iter().collect())
    }

    pub fn has_table(&self, name: &str) -> Result<bool, DbError> {
        self.lock()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                rusqlite::params![name],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| DbError::storage("inspect table", error))
    }

    /// Exclusive access to the one connection behind this handle.
    ///
    /// Poisoning is recovered rather than reported: a panic in one domain must
    /// not take the shared database down for every other domain.
    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        self.inner
            .connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl std::fmt::Debug for Db {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Db")
            .field("path", &self.inner.path)
            .finish_non_exhaustive()
    }
}

fn configure(connection: &Connection) -> Result<(), DbError> {
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| DbError::storage("configure database busy timeout", error))?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| DbError::storage("configure database foreign keys", error))
}

fn verify_integrity(connection: &Connection) -> Result<(), DbError> {
    let result = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get::<_, String>(0))
        .map_err(|error| DbError::storage("check database integrity", error))?;
    if result == "ok" {
        return Ok(());
    }
    Err(DbError::Integrity { message: result })
}

fn read_applied(connection: &Connection) -> Result<BTreeSet<String>, DbError> {
    let mut statement = connection
        .prepare("SELECT id FROM lilia_migrations")
        .map_err(|error| DbError::storage("read migration ledger", error))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| DbError::storage("read migration ledger", error))?;
    let mut applied = BTreeSet::new();
    for row in rows {
        applied.insert(row.map_err(|error| DbError::storage("read migration ledger", error))?);
    }
    Ok(applied)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const FIRST: Migration = Migration::new(
        "test.first",
        "CREATE TABLE widgets (id TEXT PRIMARY KEY, label TEXT NOT NULL);",
    );
    const SECOND: Migration = Migration::new(
        "test.second",
        "ALTER TABLE widgets ADD COLUMN weight INTEGER NOT NULL DEFAULT 0;",
    );

    #[test]
    fn migrations_apply_once_and_are_skipped_on_reopen() {
        let db = Db::in_memory().unwrap();

        assert_eq!(
            db.migrate(&[FIRST, SECOND]).unwrap(),
            ["test.first", "test.second"]
        );
        assert!(db.migrate(&[FIRST, SECOND]).unwrap().is_empty());
        assert_eq!(
            db.applied_migrations().unwrap(),
            ["test.first", "test.second"]
        );
        assert!(db.has_table("widgets").unwrap());
    }

    #[test]
    fn a_later_migration_is_applied_to_an_already_migrated_database() {
        let db = Db::in_memory().unwrap();
        db.migrate(&[FIRST]).unwrap();

        assert_eq!(db.migrate(&[FIRST, SECOND]).unwrap(), ["test.second"]);
        db.lock()
            .execute(
                "INSERT INTO widgets (id, label, weight) VALUES ('a', 'A', 3)",
                [],
            )
            .unwrap();
        let weight: i64 = db
            .lock()
            .query_row("SELECT weight FROM widgets WHERE id = 'a'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(weight, 3);
    }

    #[test]
    fn a_failing_migration_is_not_recorded_and_leaves_no_partial_schema() {
        let db = Db::in_memory().unwrap();
        let broken = Migration::new(
            "test.broken",
            "CREATE TABLE good (id TEXT PRIMARY KEY); CREATE TABLE good (id TEXT PRIMARY KEY);",
        );

        assert!(matches!(
            db.migrate(&[broken]),
            Err(DbError::Migration {
                id: "test.broken",
                ..
            })
        ));
        assert!(db.applied_migrations().unwrap().is_empty());
        assert!(!db.has_table("good").unwrap());
    }

    #[test]
    fn clones_share_one_connection_and_therefore_one_transaction_scope() {
        let db = Db::in_memory().unwrap();
        db.migrate(&[FIRST]).unwrap();
        let mirror = db.clone();

        db.lock()
            .execute("INSERT INTO widgets (id, label) VALUES ('x', 'X')", [])
            .unwrap();
        let count: i64 = mirror
            .lock()
            .query_row("SELECT COUNT(*) FROM widgets", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count, 1);
    }

    #[test]
    fn opening_a_corrupt_file_is_refused_without_rewriting_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("corrupt.db");
        let bytes = b"this is not a sqlite database";
        std::fs::File::create(&path)
            .unwrap()
            .write_all(bytes)
            .unwrap();

        assert!(Db::open(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn foreign_keys_are_enforced_on_the_shared_connection() {
        let db = Db::in_memory().unwrap();
        db.migrate(&[Migration::new(
            "test.fk",
            "CREATE TABLE parents (id TEXT PRIMARY KEY);
             CREATE TABLE children (
               id TEXT PRIMARY KEY,
               parent_id TEXT NOT NULL REFERENCES parents(id) ON DELETE CASCADE
             );",
        )])
        .unwrap();

        let rejected = db.lock().execute(
            "INSERT INTO children (id, parent_id) VALUES ('c', 'missing')",
            [],
        );

        assert!(rejected.is_err());
    }
}

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;

pub(crate) type SharedLegacyConnection = Arc<Mutex<Connection>>;

pub(crate) fn open_shared_legacy_connection(
    path: &Path,
) -> Result<SharedLegacyConnection, DesktopLegacyDatabaseError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| DesktopLegacyDatabaseError::Storage {
            operation: "create legacy desktop database directory",
            message: error.to_string(),
        })?;
    }
    let connection =
        Connection::open(path).map_err(|error| DesktopLegacyDatabaseError::Storage {
            operation: "open legacy desktop database",
            message: error.to_string(),
        })?;
    configure_connection(&connection, true)?;
    Ok(Arc::new(Mutex::new(connection)))
}

pub(crate) fn in_memory_shared_legacy_connection(
) -> Result<SharedLegacyConnection, DesktopLegacyDatabaseError> {
    let connection =
        Connection::open_in_memory().map_err(|error| DesktopLegacyDatabaseError::Storage {
            operation: "open in-memory legacy desktop database",
            message: error.to_string(),
        })?;
    configure_connection(&connection, false)?;
    Ok(Arc::new(Mutex::new(connection)))
}

fn configure_connection(
    connection: &Connection,
    persistent: bool,
) -> Result<(), DesktopLegacyDatabaseError> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| DesktopLegacyDatabaseError::Storage {
            operation: "configure legacy desktop database busy timeout",
            message: error.to_string(),
        })?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| DesktopLegacyDatabaseError::Storage {
            operation: "configure legacy desktop database foreign keys",
            message: error.to_string(),
        })?;
    if persistent {
        let quick_check = connection
            .query_row("PRAGMA quick_check(1)", [], |row| row.get::<_, String>(0))
            .map_err(|error| DesktopLegacyDatabaseError::Storage {
                operation: "check legacy desktop database integrity",
                message: error.to_string(),
            })?;
        if quick_check != "ok" {
            return Err(DesktopLegacyDatabaseError::Integrity {
                message: quick_check,
            });
        }
        connection
            .execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
            .map_err(|error| DesktopLegacyDatabaseError::Storage {
                operation: "configure legacy desktop database journal",
                message: error.to_string(),
            })?;
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopLegacyDatabaseError {
    #[error("legacy desktop database failed during {operation}: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },
    #[error("legacy desktop database integrity check failed: {message}")]
    Integrity { message: String },
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn corrupt_database_is_rejected_without_modifying_the_source_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("corrupt.db");
        let bytes = b"this is not a sqlite database";
        std::fs::File::create(&path)
            .unwrap()
            .write_all(bytes)
            .unwrap();

        assert!(matches!(
            open_shared_legacy_connection(&path),
            Err(DesktopLegacyDatabaseError::Storage {
                operation: "configure legacy desktop database journal"
                    | "check legacy desktop database integrity",
                ..
            }) | Err(DesktopLegacyDatabaseError::Integrity { .. })
        ));
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

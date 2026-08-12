use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

use super::{
    DesktopMemory, MemoryInjectionState, MemoryScope, MemoryStore, MemoryStoreError,
    MemoryUpsertInput,
};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS memories (
  id             TEXT PRIMARY KEY,
  scope          TEXT NOT NULL CHECK (scope IN ('user','project')),
  project_id     TEXT,
  title          TEXT NOT NULL,
  body           TEXT NOT NULL,
  tags_json      TEXT NOT NULL DEFAULT '[]',
  enabled        INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  source_task_id TEXT,
  created_at     INTEGER NOT NULL,
  updated_at     INTEGER NOT NULL,
  CHECK (
    (scope = 'user' AND project_id IS NULL) OR
    (scope = 'project' AND project_id IS NOT NULL)
  ),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_memories_scope_project
  ON memories(scope, project_id, enabled, updated_at DESC);

CREATE TABLE IF NOT EXISTS memory_injection_states (
  task_id                TEXT PRIMARY KEY,
  enabled                INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  last_injected_turn_seq INTEGER,
  updated_at             INTEGER NOT NULL,
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);
"#;

pub struct SqliteMemoryStore {
    connection: Connection,
}

impl SqliteMemoryStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MemoryStoreError> {
        let connection = Connection::open(path)
            .map_err(|error| MemoryStoreError::storage("open database", error))?;
        Self::from_connection(connection)
    }

    pub fn in_memory() -> Result<Self, MemoryStoreError> {
        let connection = Connection::open_in_memory()
            .map_err(|error| MemoryStoreError::storage("open in-memory database", error))?;
        connection
            .execute_batch(
                r#"CREATE TABLE projects (
                     id TEXT PRIMARY KEY,
                     name TEXT NOT NULL,
                     created_at INTEGER NOT NULL
                   );
                   CREATE TABLE tasks (
                     id TEXT PRIMARY KEY
                   );"#,
            )
            .map_err(|error| MemoryStoreError::storage("create in-memory project schema", error))?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self, MemoryStoreError> {
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|error| MemoryStoreError::storage("configure busy timeout", error))?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|error| MemoryStoreError::storage("enable foreign keys", error))?;
        let has_projects = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'projects')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| MemoryStoreError::storage("inspect project schema", error))?;
        if !has_projects {
            return Err(MemoryStoreError::ProjectsSchemaRequired);
        }
        let has_tasks = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'tasks')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| MemoryStoreError::storage("inspect task schema", error))?;
        if !has_tasks {
            return Err(MemoryStoreError::TasksSchemaRequired);
        }
        connection
            .execute_batch(SCHEMA)
            .map_err(|error| MemoryStoreError::storage("create schema", error))?;
        Ok(Self { connection })
    }

    #[cfg(test)]
    fn insert_project(&self, project_id: &str) {
        self.connection
            .execute(
                "INSERT INTO projects (id, name, created_at) VALUES (?1, ?1, 1)",
                params![project_id],
            )
            .unwrap();
    }

    #[cfg(test)]
    fn insert_task(&self, task_id: &str) {
        self.connection
            .execute("INSERT INTO tasks (id) VALUES (?1)", params![task_id])
            .unwrap();
    }
}

impl MemoryStore for SqliteMemoryStore {
    fn list(&self, project_id: Option<&str>) -> Result<Vec<DesktopMemory>, MemoryStoreError> {
        let mut memories = list_scope(&self.connection, MemoryScope::User, None)?;
        if let Some(project_id) = normalized_optional(project_id) {
            memories.extend(list_scope(
                &self.connection,
                MemoryScope::Project,
                Some(project_id),
            )?);
        }
        Ok(memories)
    }

    fn memory(&self, memory_id: &str) -> Result<Option<DesktopMemory>, MemoryStoreError> {
        memory_on(&self.connection, memory_id)
    }

    fn save(&mut self, input: MemoryUpsertInput) -> Result<DesktopMemory, MemoryStoreError> {
        let normalized = NormalizedMemoryInput::new(input)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| MemoryStoreError::storage("begin save transaction", error))?;

        if let Some(project_id) = normalized.project_id.as_deref() {
            ensure_project_exists(&transaction, project_id)?;
        }

        let previous = raw_memory_on(&transaction, &normalized.id)?;
        validate_expected_update(
            &normalized.id,
            normalized.expected_updated_at,
            previous.as_ref().map(|memory| memory.updated_at),
        )?;
        let created_at = previous
            .as_ref()
            .map_or_else(now_millis, |memory| memory.created_at);
        let updated_at = next_timestamp(previous.as_ref().map(|memory| memory.updated_at));
        let tags_json = serde_json::to_string(&normalized.tags).map_err(|error| {
            MemoryStoreError::Serialization {
                field: "tags_json",
                message: error.to_string(),
            }
        })?;

        transaction
            .execute(
                r#"INSERT INTO memories
                   (id, scope, project_id, title, body, tags_json, enabled, source_task_id,
                    created_at, updated_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                   ON CONFLICT(id) DO UPDATE SET
                     scope          = excluded.scope,
                     project_id     = excluded.project_id,
                     title          = excluded.title,
                     body           = excluded.body,
                     tags_json      = excluded.tags_json,
                     enabled        = excluded.enabled,
                     source_task_id = excluded.source_task_id,
                     updated_at     = excluded.updated_at"#,
                params![
                    normalized.id,
                    normalized.scope.as_storage(),
                    normalized.project_id,
                    normalized.title,
                    normalized.body,
                    tags_json,
                    i64::from(normalized.enabled),
                    normalized.source_task_id,
                    created_at,
                    updated_at,
                ],
            )
            .map_err(|error| MemoryStoreError::storage("save memory", error))?;
        let memory = memory_on(&transaction, &normalized.id)?.ok_or_else(|| {
            MemoryStoreError::MemoryNotFound {
                memory_id: normalized.id.clone(),
            }
        })?;
        transaction
            .commit()
            .map_err(|error| MemoryStoreError::storage("commit save transaction", error))?;
        Ok(memory)
    }

    fn set_enabled(
        &mut self,
        memory_id: &str,
        enabled: bool,
        expected_updated_at: Option<i64>,
    ) -> Result<DesktopMemory, MemoryStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| MemoryStoreError::storage("begin enable transaction", error))?;
        let existing = raw_memory_on(&transaction, memory_id)?.ok_or_else(|| {
            MemoryStoreError::MemoryNotFound {
                memory_id: memory_id.to_owned(),
            }
        })?;
        decode_memory(existing.clone())?;
        validate_expected_update(memory_id, expected_updated_at, Some(existing.updated_at))?;
        let updated_at = next_timestamp(Some(existing.updated_at));
        transaction
            .execute(
                "UPDATE memories SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
                params![i64::from(enabled), updated_at, memory_id],
            )
            .map_err(|error| MemoryStoreError::storage("set memory enabled", error))?;
        let memory = memory_on(&transaction, memory_id)?.ok_or_else(|| {
            MemoryStoreError::MemoryNotFound {
                memory_id: memory_id.to_owned(),
            }
        })?;
        transaction
            .commit()
            .map_err(|error| MemoryStoreError::storage("commit enable transaction", error))?;
        Ok(memory)
    }

    fn delete(
        &mut self,
        memory_id: &str,
        expected_updated_at: Option<i64>,
    ) -> Result<bool, MemoryStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| MemoryStoreError::storage("begin delete transaction", error))?;
        let existing = raw_memory_on(&transaction, memory_id)?;
        let Some(existing) = existing else {
            if expected_updated_at.is_some() {
                return Err(MemoryStoreError::MemoryNotFound {
                    memory_id: memory_id.to_owned(),
                });
            }
            transaction
                .commit()
                .map_err(|error| MemoryStoreError::storage("commit delete transaction", error))?;
            return Ok(false);
        };
        validate_expected_update(memory_id, expected_updated_at, Some(existing.updated_at))?;
        transaction
            .execute("DELETE FROM memories WHERE id = ?1", params![memory_id])
            .map_err(|error| MemoryStoreError::storage("delete memory", error))?;
        transaction
            .commit()
            .map_err(|error| MemoryStoreError::storage("commit delete transaction", error))?;
        Ok(true)
    }

    fn injection_state(&self, task_id: &str) -> Result<MemoryInjectionState, MemoryStoreError> {
        injection_state_on(&self.connection, task_id)
    }

    fn set_task_enabled(
        &mut self,
        task_id: &str,
        enabled: bool,
        expected_updated_at: Option<i64>,
    ) -> Result<MemoryInjectionState, MemoryStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                MemoryStoreError::storage("begin task memory enable transaction", error)
            })?;
        ensure_task_exists(&transaction, task_id)?;
        let previous = raw_injection_state_on(&transaction, task_id)?;
        validate_expected_injection_update(
            task_id,
            expected_updated_at,
            previous.as_ref().map(|state| state.updated_at),
        )?;
        let updated_at = next_timestamp(previous.as_ref().map(|state| state.updated_at));
        transaction
            .execute(
                r#"INSERT INTO memory_injection_states
                   (task_id, enabled, last_injected_turn_seq, updated_at)
                   VALUES (?1, ?2, NULL, ?3)
                   ON CONFLICT(task_id) DO UPDATE SET
                     enabled = excluded.enabled,
                     updated_at = excluded.updated_at"#,
                params![task_id, i64::from(enabled), updated_at],
            )
            .map_err(|error| MemoryStoreError::storage("set task memory enabled", error))?;
        let state = injection_state_on(&transaction, task_id)?;
        transaction.commit().map_err(|error| {
            MemoryStoreError::storage("commit task memory enable transaction", error)
        })?;
        Ok(state)
    }

    fn reset_task_cooldown(
        &mut self,
        task_id: &str,
        expected_updated_at: Option<i64>,
    ) -> Result<MemoryInjectionState, MemoryStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                MemoryStoreError::storage("begin task memory cooldown transaction", error)
            })?;
        ensure_task_exists(&transaction, task_id)?;
        let previous = raw_injection_state_on(&transaction, task_id)?;
        validate_expected_injection_update(
            task_id,
            expected_updated_at,
            previous.as_ref().map(|state| state.updated_at),
        )?;
        let updated_at = next_timestamp(previous.as_ref().map(|state| state.updated_at));
        transaction
            .execute(
                r#"INSERT INTO memory_injection_states
                   (task_id, enabled, last_injected_turn_seq, updated_at)
                   VALUES (?1, 1, NULL, ?2)
                   ON CONFLICT(task_id) DO UPDATE SET
                     last_injected_turn_seq = NULL,
                     updated_at = excluded.updated_at"#,
                params![task_id, updated_at],
            )
            .map_err(|error| MemoryStoreError::storage("reset task memory cooldown", error))?;
        let state = injection_state_on(&transaction, task_id)?;
        transaction.commit().map_err(|error| {
            MemoryStoreError::storage("commit task memory cooldown transaction", error)
        })?;
        Ok(state)
    }
}

#[derive(Clone, Debug)]
struct RawMemoryInjectionState {
    task_id: String,
    enabled: i64,
    last_injected_turn_seq: Option<i64>,
    updated_at: i64,
}

fn raw_injection_state_on(
    connection: &Connection,
    task_id: &str,
) -> Result<Option<RawMemoryInjectionState>, MemoryStoreError> {
    connection
        .query_row(
            r#"SELECT task_id, enabled, last_injected_turn_seq, updated_at
               FROM memory_injection_states WHERE task_id = ?1"#,
            params![task_id],
            |row| {
                Ok(RawMemoryInjectionState {
                    task_id: row.get(0)?,
                    enabled: row.get(1)?,
                    last_injected_turn_seq: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| MemoryStoreError::storage("read memory injection state", error))
}

fn injection_state_on(
    connection: &Connection,
    task_id: &str,
) -> Result<MemoryInjectionState, MemoryStoreError> {
    raw_injection_state_on(connection, task_id)?
        .map(decode_injection_state)
        .transpose()
        .map(|state| {
            state.unwrap_or_else(|| MemoryInjectionState {
                task_id: task_id.to_owned(),
                enabled: true,
                last_injected_turn_seq: None,
                updated_at: 0,
            })
        })
}

fn decode_injection_state(
    raw: RawMemoryInjectionState,
) -> Result<MemoryInjectionState, MemoryStoreError> {
    let enabled = match raw.enabled {
        0 => false,
        1 => true,
        value => {
            return Err(MemoryStoreError::InvalidStoredInjectionEnabled {
                task_id: raw.task_id,
                value,
            });
        }
    };
    Ok(MemoryInjectionState {
        task_id: raw.task_id,
        enabled,
        last_injected_turn_seq: raw.last_injected_turn_seq,
        updated_at: raw.updated_at,
    })
}

#[derive(Clone, Debug)]
struct RawMemory {
    id: String,
    scope: String,
    project_id: Option<String>,
    title: String,
    body: String,
    tags_json: String,
    enabled: i64,
    source_task_id: Option<String>,
    created_at: i64,
    updated_at: i64,
}

fn raw_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawMemory> {
    Ok(RawMemory {
        id: row.get(0)?,
        scope: row.get(1)?,
        project_id: row.get(2)?,
        title: row.get(3)?,
        body: row.get(4)?,
        tags_json: row.get(5)?,
        enabled: row.get(6)?,
        source_task_id: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn raw_memory_on(
    connection: &Connection,
    memory_id: &str,
) -> Result<Option<RawMemory>, MemoryStoreError> {
    connection
        .query_row(
            r#"SELECT id, scope, project_id, title, body, tags_json, enabled,
                      source_task_id, created_at, updated_at
               FROM memories WHERE id = ?1"#,
            params![memory_id],
            raw_memory,
        )
        .optional()
        .map_err(|error| MemoryStoreError::storage("read memory", error))
}

fn memory_on(
    connection: &Connection,
    memory_id: &str,
) -> Result<Option<DesktopMemory>, MemoryStoreError> {
    raw_memory_on(connection, memory_id)?
        .map(decode_memory)
        .transpose()
}

fn list_scope(
    connection: &Connection,
    scope: MemoryScope,
    project_id: Option<&str>,
) -> Result<Vec<DesktopMemory>, MemoryStoreError> {
    let (sql, argument) = match scope {
        MemoryScope::User => (
            r#"SELECT id, scope, project_id, title, body, tags_json, enabled,
                      source_task_id, created_at, updated_at
               FROM memories WHERE scope = 'user'
               ORDER BY updated_at DESC, created_at DESC"#,
            None,
        ),
        MemoryScope::Project => (
            r#"SELECT id, scope, project_id, title, body, tags_json, enabled,
                      source_task_id, created_at, updated_at
               FROM memories WHERE scope = 'project' AND project_id = ?1
               ORDER BY updated_at DESC, created_at DESC"#,
            project_id,
        ),
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| MemoryStoreError::storage("prepare memory list", error))?;
    let rows = match argument {
        Some(project_id) => statement.query_map(params![project_id], raw_memory),
        None => statement.query_map([], raw_memory),
    }
    .map_err(|error| MemoryStoreError::storage("query memory list", error))?;
    let raw = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| MemoryStoreError::storage("read memory list", error))?;
    raw.into_iter().map(decode_memory).collect()
}

fn decode_memory(raw: RawMemory) -> Result<DesktopMemory, MemoryStoreError> {
    let scope = MemoryScope::from_storage(&raw.scope).ok_or_else(|| {
        MemoryStoreError::InvalidStoredScope {
            memory_id: raw.id.clone(),
            scope: raw.scope.clone(),
        }
    })?;
    let project_invariant_valid = match scope {
        MemoryScope::User => raw.project_id.is_none(),
        MemoryScope::Project => raw
            .project_id
            .as_deref()
            .is_some_and(|project_id| !project_id.trim().is_empty()),
    };
    if !project_invariant_valid {
        return Err(MemoryStoreError::InvalidStoredProjectScope {
            memory_id: raw.id,
            scope: raw.scope,
        });
    }
    let enabled = match raw.enabled {
        0 => false,
        1 => true,
        value => {
            return Err(MemoryStoreError::InvalidStoredEnabled {
                memory_id: raw.id,
                value,
            });
        }
    };
    let tags = serde_json::from_str::<Vec<String>>(&raw.tags_json).map_err(|error| {
        MemoryStoreError::CorruptJson {
            memory_id: raw.id.clone(),
            field: "tags_json",
            message: error.to_string(),
        }
    })?;
    Ok(DesktopMemory {
        id: raw.id,
        scope,
        project_id: raw.project_id,
        title: raw.title,
        body: raw.body,
        tags,
        enabled,
        source_task_id: raw.source_task_id,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
    })
}

struct NormalizedMemoryInput {
    id: String,
    scope: MemoryScope,
    project_id: Option<String>,
    title: String,
    body: String,
    tags: Vec<String>,
    enabled: bool,
    source_task_id: Option<String>,
    expected_updated_at: Option<i64>,
}

impl NormalizedMemoryInput {
    fn new(input: MemoryUpsertInput) -> Result<Self, MemoryStoreError> {
        let supplied_id = normalized_owned(input.id);
        if input.expected_updated_at.is_some() && supplied_id.is_none() {
            return Err(MemoryStoreError::ExpectedUpdateRequiresId);
        }
        let title = input.title.trim().to_owned();
        if title.is_empty() {
            return Err(MemoryStoreError::EmptyTitle);
        }
        let body = input.body.trim().to_owned();
        if body.is_empty() {
            return Err(MemoryStoreError::EmptyBody);
        }
        let project_id = match input.scope {
            MemoryScope::User => None,
            MemoryScope::Project => normalized_owned(input.project_id)
                .ok_or(MemoryStoreError::ProjectIdRequired)
                .map(Some)?,
        };
        Ok(Self {
            id: supplied_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            scope: input.scope,
            project_id,
            title,
            body,
            tags: normalize_tags(input.tags),
            enabled: input.enabled,
            source_task_id: normalized_owned(input.source_task_id),
            expected_updated_at: input.expected_updated_at,
        })
    }
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.into_iter()
        .filter_map(|tag| {
            let tag = tag.trim();
            (!tag.is_empty() && seen.insert(tag.to_owned())).then(|| tag.to_owned())
        })
        .collect()
}

fn normalized_owned(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn normalized_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn ensure_project_exists(
    transaction: &Transaction<'_>,
    project_id: &str,
) -> Result<(), MemoryStoreError> {
    let exists = transaction
        .query_row(
            "SELECT 1 FROM projects WHERE id = ?1",
            params![project_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| MemoryStoreError::storage("find memory project", error))?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(MemoryStoreError::ProjectNotFound {
            project_id: project_id.to_owned(),
        })
    }
}

fn ensure_task_exists(
    transaction: &Transaction<'_>,
    task_id: &str,
) -> Result<(), MemoryStoreError> {
    let exists = transaction
        .query_row(
            "SELECT 1 FROM tasks WHERE id = ?1",
            params![task_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| MemoryStoreError::storage("find memory task", error))?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(MemoryStoreError::TaskNotFound {
            task_id: task_id.to_owned(),
        })
    }
}

fn validate_expected_update(
    memory_id: &str,
    expected_updated_at: Option<i64>,
    actual_updated_at: Option<i64>,
) -> Result<(), MemoryStoreError> {
    let Some(expected_updated_at) = expected_updated_at else {
        return Ok(());
    };
    let Some(actual_updated_at) = actual_updated_at else {
        return Err(MemoryStoreError::MemoryNotFound {
            memory_id: memory_id.to_owned(),
        });
    };
    if expected_updated_at == actual_updated_at {
        Ok(())
    } else {
        Err(MemoryStoreError::Conflict {
            memory_id: memory_id.to_owned(),
            expected_updated_at,
            actual_updated_at,
        })
    }
}

fn validate_expected_injection_update(
    task_id: &str,
    expected_updated_at: Option<i64>,
    actual_updated_at: Option<i64>,
) -> Result<(), MemoryStoreError> {
    let Some(expected_updated_at) = expected_updated_at else {
        return Ok(());
    };
    let actual_updated_at = actual_updated_at.unwrap_or(0);
    if expected_updated_at == actual_updated_at {
        Ok(())
    } else {
        Err(MemoryStoreError::InjectionStateConflict {
            task_id: task_id.to_owned(),
            expected_updated_at,
            actual_updated_at,
        })
    }
}

fn next_timestamp(previous: Option<i64>) -> i64 {
    previous.map_or_else(now_millis, |previous| {
        now_millis().max(previous.saturating_add(1))
    })
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use super::*;

    fn store() -> SqliteMemoryStore {
        let store = SqliteMemoryStore::in_memory().unwrap();
        store.insert_project("project-1");
        store.insert_project("project-2");
        store.insert_task("task-1");
        store
    }

    fn input(id: &str, scope: MemoryScope, project_id: Option<&str>) -> MemoryUpsertInput {
        MemoryUpsertInput {
            id: Some(id.to_owned()),
            scope,
            project_id: project_id.map(str::to_owned),
            title: " Release rule ".to_owned(),
            body: " Run tests first ".to_owned(),
            tags: vec![" rule ".to_owned(), "rule".to_owned(), String::new()],
            enabled: true,
            source_task_id: Some(" task-1 ".to_owned()),
            expected_updated_at: None,
        }
    }

    #[test]
    fn legacy_scope_and_record_shape_round_trip_without_cross_project_leakage() {
        let mut store = store();
        let user = store
            .save(input("user-1", MemoryScope::User, Some("project-2")))
            .unwrap();
        let project = store
            .save(input(
                "project-memory-1",
                MemoryScope::Project,
                Some("project-1"),
            ))
            .unwrap();
        store
            .save(input(
                "project-memory-2",
                MemoryScope::Project,
                Some("project-2"),
            ))
            .unwrap();

        assert_eq!(user.project_id, None);
        assert_eq!(user.title, "Release rule");
        assert_eq!(user.body, "Run tests first");
        assert_eq!(user.tags, vec!["rule"]);
        assert_eq!(user.source_task_id.as_deref(), Some("task-1"));
        let listed = store.list(Some("project-1")).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, user.id);
        assert_eq!(listed[1].id, project.id);
    }

    #[test]
    fn project_scope_validation_is_typed_and_does_not_partially_overwrite() {
        let mut store = store();
        let original = store
            .save(input("memory-1", MemoryScope::User, None))
            .unwrap();
        let mut missing = input("memory-1", MemoryScope::Project, Some("missing"));
        missing.title = "Overwritten".to_owned();
        assert!(matches!(
            store.save(missing),
            Err(MemoryStoreError::ProjectNotFound { project_id }) if project_id == "missing"
        ));
        assert_eq!(store.memory("memory-1").unwrap(), Some(original));

        assert!(matches!(
            store.save(input("memory-2", MemoryScope::Project, None)),
            Err(MemoryStoreError::ProjectIdRequired)
        ));
    }

    #[test]
    fn optimistic_conflict_keeps_newer_record_for_save_enable_and_delete() {
        let mut store = store();
        let original = store
            .save(input("memory-1", MemoryScope::User, None))
            .unwrap();
        let mut update = input("memory-1", MemoryScope::User, None);
        update.title = "Newer".to_owned();
        update.expected_updated_at = Some(original.updated_at);
        let newer = store.save(update).unwrap();
        assert!(newer.updated_at > original.updated_at);

        let mut stale = input("memory-1", MemoryScope::User, None);
        stale.title = "Stale".to_owned();
        stale.expected_updated_at = Some(original.updated_at);
        for result in [
            store.save(stale).map(|_| ()),
            store
                .set_enabled("memory-1", false, Some(original.updated_at))
                .map(|_| ()),
            store
                .delete("memory-1", Some(original.updated_at))
                .map(|_| ()),
        ] {
            assert!(matches!(result, Err(MemoryStoreError::Conflict { .. })));
        }
        assert_eq!(store.memory("memory-1").unwrap(), Some(newer));
    }

    #[test]
    fn corrupt_tags_are_reported_instead_of_becoming_an_empty_list() {
        let mut store = store();
        store
            .save(input("memory-1", MemoryScope::User, None))
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE memories SET tags_json = '{' WHERE id = 'memory-1'",
                [],
            )
            .unwrap();

        assert!(matches!(
            store.memory("memory-1"),
            Err(MemoryStoreError::CorruptJson {
                memory_id,
                field: "tags_json",
                ..
            }) if memory_id == "memory-1"
        ));
        assert!(matches!(
            store.set_enabled("memory-1", false, None),
            Err(MemoryStoreError::CorruptJson { .. })
        ));
        assert!(store.delete("memory-1", None).unwrap());
    }

    #[test]
    fn concurrent_conditional_saves_allow_exactly_one_writer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("memory.sqlite3");
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    r#"PRAGMA foreign_keys = ON;
                       CREATE TABLE projects (
                         id TEXT PRIMARY KEY,
                         name TEXT NOT NULL,
                         created_at INTEGER NOT NULL
                       );
                       CREATE TABLE tasks (
                         id TEXT PRIMARY KEY
                       );"#,
                )
                .unwrap();
        }
        let mut initial_store = SqliteMemoryStore::open(&path).unwrap();
        let original = initial_store
            .save(input("memory-1", MemoryScope::User, None))
            .unwrap();
        drop(initial_store);

        let barrier = Arc::new(Barrier::new(2));
        let handles = (0..2)
            .map(|index| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let mut store = SqliteMemoryStore::open(path).unwrap();
                    let mut input = input("memory-1", MemoryScope::User, None);
                    input.title = format!("Writer {index}");
                    input.expected_updated_at = Some(original.updated_at);
                    barrier.wait();
                    store.save(input)
                })
            })
            .collect::<Vec<_>>();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(MemoryStoreError::Conflict { .. })))
                .count(),
            1
        );
    }

    #[test]
    fn delete_is_idempotent_without_a_precondition() {
        let mut store = store();
        assert!(!store.delete("missing", None).unwrap());
        store
            .save(input("memory-1", MemoryScope::User, None))
            .unwrap();
        assert!(store.delete("memory-1", None).unwrap());
        assert!(!store.delete("memory-1", None).unwrap());
    }

    #[test]
    fn persistent_store_rejects_a_database_without_project_authority() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("empty.sqlite3");
        assert!(matches!(
            SqliteMemoryStore::open(path),
            Err(MemoryStoreError::ProjectsSchemaRequired)
        ));
    }

    #[test]
    fn persistent_store_rejects_a_database_without_task_authority() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("projects-only.sqlite3");
        Connection::open(&path)
            .unwrap()
            .execute_batch(
                r#"CREATE TABLE projects (
                     id TEXT PRIMARY KEY,
                     name TEXT NOT NULL,
                     created_at INTEGER NOT NULL
                   );"#,
            )
            .unwrap();
        assert!(matches!(
            SqliteMemoryStore::open(path),
            Err(MemoryStoreError::TasksSchemaRequired)
        ));
    }

    #[test]
    fn injection_state_matches_legacy_defaults_and_update_semantics() {
        let mut store = store();
        assert_eq!(
            store.injection_state("unknown-task").unwrap(),
            MemoryInjectionState {
                task_id: "unknown-task".to_owned(),
                enabled: true,
                last_injected_turn_seq: None,
                updated_at: 0,
            }
        );

        let disabled = store.set_task_enabled("task-1", false, Some(0)).unwrap();
        assert!(!disabled.enabled);
        store
            .connection
            .execute(
                "UPDATE memory_injection_states SET last_injected_turn_seq = 9 WHERE task_id = 'task-1'",
                [],
            )
            .unwrap();
        let enabled = store
            .set_task_enabled("task-1", true, Some(disabled.updated_at))
            .unwrap();
        assert!(enabled.enabled);
        assert_eq!(enabled.last_injected_turn_seq, Some(9));

        store
            .connection
            .execute(
                "UPDATE memory_injection_states SET enabled = 0, last_injected_turn_seq = 10 WHERE task_id = 'task-1'",
                [],
            )
            .unwrap();
        let reset = store
            .reset_task_cooldown("task-1", Some(enabled.updated_at))
            .unwrap();
        assert!(!reset.enabled);
        assert_eq!(reset.last_injected_turn_seq, None);
        assert!(reset.updated_at > enabled.updated_at);
    }

    #[test]
    fn injection_mutations_validate_task_and_conflicts_without_partial_writes() {
        let mut store = store();
        assert!(matches!(
            store.set_task_enabled("missing", false, None),
            Err(MemoryStoreError::TaskNotFound { task_id }) if task_id == "missing"
        ));
        assert!(!store
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM memory_injection_states WHERE task_id = 'missing')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .unwrap());

        let current = store.set_task_enabled("task-1", false, Some(0)).unwrap();
        assert!(matches!(
            store.reset_task_cooldown("task-1", Some(0)),
            Err(MemoryStoreError::InjectionStateConflict {
                task_id,
                actual_updated_at,
                ..
            }) if task_id == "task-1" && actual_updated_at == current.updated_at
        ));
        assert_eq!(store.injection_state("task-1").unwrap(), current);
    }

    #[test]
    fn corrupt_injection_enabled_value_is_typed() {
        let mut store = store();
        store.set_task_enabled("task-1", true, None).unwrap();
        store
            .connection
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE memory_injection_states SET enabled = 2 WHERE task_id = 'task-1'",
                [],
            )
            .unwrap();
        store
            .connection
            .execute_batch("PRAGMA ignore_check_constraints = OFF;")
            .unwrap();
        assert!(matches!(
            store.injection_state("task-1"),
            Err(MemoryStoreError::InvalidStoredInjectionEnabled { task_id, value: 2 })
                if task_id == "task-1"
        ));
    }

    #[test]
    fn injection_storage_failure_rolls_back_the_previous_state() {
        let mut store = store();
        let previous = store.set_task_enabled("task-1", false, Some(0)).unwrap();
        store
            .connection
            .execute_batch(
                r#"CREATE TRIGGER reject_memory_injection_update
                   BEFORE UPDATE ON memory_injection_states
                   BEGIN
                     SELECT RAISE(ABORT, 'injected failure');
                   END;"#,
            )
            .unwrap();

        assert!(matches!(
            store.set_task_enabled("task-1", true, Some(previous.updated_at)),
            Err(MemoryStoreError::Storage { .. })
        ));
        assert_eq!(store.injection_state("task-1").unwrap(), previous);
    }
}

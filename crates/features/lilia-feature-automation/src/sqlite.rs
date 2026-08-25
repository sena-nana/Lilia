use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_storage::Db;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::de::DeserializeOwned;
use serde::Serialize;
use uuid::Uuid;

use super::contract::{scope_event_kinds, scope_task_statuses};
use super::{
    validate_automation_graph, AutomationActiveRunConflict, AutomationBeginRunInput,
    AutomationDraft, AutomationExecutionTransition, AutomationRecordKind, AutomationRun,
    AutomationRunDetail, AutomationRunNodeState, AutomationRunStatus, AutomationRunSummary,
    AutomationSaveDraftInput, AutomationScopeFilter, AutomationSignalEnvelope, AutomationStore,
    AutomationStoreError, AutomationWorkflow, AutomationWorkflowVersion,
};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS automation_workflows (
  id                   TEXT PRIMARY KEY,
  name                 TEXT NOT NULL,
  enabled              INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
  scope_json           TEXT NOT NULL DEFAULT '{}',
  draft_json           TEXT NOT NULL DEFAULT '{"nodes":[],"edges":[],"scope":{}}',
  published_version_id TEXT,
  created_at           INTEGER NOT NULL,
  updated_at           INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS automation_workflow_versions (
  id            TEXT PRIMARY KEY,
  workflow_id   TEXT NOT NULL,
  version       INTEGER NOT NULL,
  snapshot_json TEXT NOT NULL,
  created_at    INTEGER NOT NULL,
  FOREIGN KEY (workflow_id) REFERENCES automation_workflows(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_automation_workflow_versions_workflow_version
  ON automation_workflow_versions(workflow_id, version);

CREATE TABLE IF NOT EXISTS automation_runs (
  id                  TEXT PRIMARY KEY,
  workflow_id         TEXT NOT NULL,
  workflow_version_id TEXT NOT NULL,
  status              TEXT NOT NULL CHECK (status IN
                        ('pending','running','succeeded','failed','skipped','cancelled','waiting_user')),
  trigger_json        TEXT NOT NULL,
  scope_json          TEXT NOT NULL,
  started_at          INTEGER NOT NULL,
  finished_at         INTEGER,
  error               TEXT,
  FOREIGN KEY (workflow_id) REFERENCES automation_workflows(id) ON DELETE CASCADE,
  FOREIGN KEY (workflow_version_id) REFERENCES automation_workflow_versions(id)
);

CREATE INDEX IF NOT EXISTS idx_automation_runs_workflow_started
  ON automation_runs(workflow_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_automation_runs_status
  ON automation_runs(status);

CREATE TABLE IF NOT EXISTS automation_run_nodes (
  id          TEXT PRIMARY KEY,
  run_id      TEXT NOT NULL,
  node_id     TEXT NOT NULL,
  status      TEXT NOT NULL CHECK (status IN
                ('pending','running','succeeded','failed','skipped','cancelled','waiting_user')),
  input_json  TEXT NOT NULL DEFAULT '{}',
  output_json TEXT,
  error       TEXT,
  started_at  INTEGER,
  finished_at INTEGER,
  FOREIGN KEY (run_id) REFERENCES automation_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_automation_run_nodes_run
  ON automation_run_nodes(run_id);
"#;

const ACTIVE_RUN_INDEX: &str = r#"
CREATE UNIQUE INDEX IF NOT EXISTS idx_automation_runs_one_active_workflow
  ON automation_runs(workflow_id)
  WHERE status IN ('pending', 'running', 'waiting_user');
"#;

pub struct SqliteAutomationStore {
    connection: Db,
}

impl SqliteAutomationStore {
    #[cfg(test)]
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, AutomationStoreError> {
        let connection = Db::open(path)
            .map_err(|error| AutomationStoreError::storage("open database", error))?;
        Self::from_db(connection)
    }

    pub fn in_memory() -> Result<Self, AutomationStoreError> {
        let connection = Db::in_memory()
            .map_err(|error| AutomationStoreError::storage("open in-memory database", error))?;
        Self::from_db(connection)
    }

    pub fn from_db(connection: Db) -> Result<Self, AutomationStoreError> {
        {
            let mut locked = connection.lock();
            migrate_cancelled_status(&mut locked)?;
            locked
                .execute_batch(SCHEMA)
                .map_err(|error| AutomationStoreError::storage("initialize schema", error))?;

            let conflicts = existing_active_run_conflicts(&locked)?;
            if !conflicts.is_empty() {
                return Err(AutomationStoreError::ExistingActiveRunConflict { conflicts });
            }
            locked.execute_batch(ACTIVE_RUN_INDEX).map_err(|error| {
                AutomationStoreError::storage("enforce active-run invariant", error)
            })?;
        }
        Ok(Self { connection })
    }

    pub fn db(&self) -> Db {
        self.connection.clone()
    }
}

fn migrate_cancelled_status(connection: &mut Connection) -> Result<(), AutomationStoreError> {
    let runs_sql = automation_table_sql(connection, "automation_runs")?;
    let nodes_sql = automation_table_sql(connection, "automation_run_nodes")?;
    match (&runs_sql, &nodes_sql) {
        (None, None) => return Ok(()),
        (Some(runs), Some(nodes))
            if runs.contains("'cancelled'") && nodes.contains("'cancelled'") =>
        {
            return Ok(());
        }
        (Some(_), Some(_)) => {}
        _ => {
            return Err(AutomationStoreError::SchemaInvariant {
                message: "automation_runs and automation_run_nodes must exist together".to_owned(),
            });
        }
    }

    connection
        .pragma_update(None, "foreign_keys", "OFF")
        .map_err(|error| {
            AutomationStoreError::storage("disable foreign keys for migration", error)
        })?;
    let migration = (|| {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AutomationStoreError::storage("begin status migration", error))?;
        transaction
            .execute_batch(
                r#"
DROP INDEX IF EXISTS idx_automation_runs_one_active_workflow;
DROP INDEX IF EXISTS idx_automation_runs_workflow_started;
DROP INDEX IF EXISTS idx_automation_runs_status;
DROP INDEX IF EXISTS idx_automation_run_nodes_run;
ALTER TABLE automation_run_nodes RENAME TO automation_run_nodes_legacy_status;
ALTER TABLE automation_runs RENAME TO automation_runs_legacy_status;
"#,
            )
            .map_err(|error| AutomationStoreError::storage("prepare status migration", error))?;
        transaction.execute_batch(SCHEMA).map_err(|error| {
            AutomationStoreError::storage("create migrated automation tables", error)
        })?;
        transaction
            .execute_batch(
                r#"
INSERT INTO automation_runs (
  id, workflow_id, workflow_version_id, status, trigger_json, scope_json,
  started_at, finished_at, error
)
SELECT id, workflow_id, workflow_version_id, status, trigger_json, scope_json,
       started_at, finished_at, error
FROM automation_runs_legacy_status;

INSERT INTO automation_run_nodes (
  id, run_id, node_id, status, input_json, output_json, error, started_at, finished_at
)
SELECT id, run_id, node_id, status, input_json, output_json, error, started_at, finished_at
FROM automation_run_nodes_legacy_status;

DROP TABLE automation_run_nodes_legacy_status;
DROP TABLE automation_runs_legacy_status;
"#,
            )
            .map_err(|error| {
                AutomationStoreError::storage("copy migrated automation data", error)
            })?;
        transaction
            .commit()
            .map_err(|error| AutomationStoreError::storage("commit status migration", error))
    })();
    let foreign_keys = connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|error| AutomationStoreError::storage("restore foreign keys", error));
    migration?;
    foreign_keys?;

    let has_foreign_key_error = connection
        .prepare("PRAGMA foreign_key_check")
        .and_then(|mut statement| statement.exists([]))
        .map_err(|error| AutomationStoreError::storage("verify migrated foreign keys", error))?;
    if has_foreign_key_error {
        return Err(AutomationStoreError::SchemaInvariant {
            message: "automation status migration produced invalid foreign keys".to_owned(),
        });
    }
    Ok(())
}

fn automation_table_sql(
    connection: &Connection,
    table: &str,
) -> Result<Option<String>, AutomationStoreError> {
    connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| AutomationStoreError::storage("inspect automation schema", error))
}

impl AutomationStore for SqliteAutomationStore {
    fn list_workflows(&self) -> Result<Vec<AutomationWorkflow>, AutomationStoreError> {
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(
                r#"SELECT id, name, enabled, scope_json, draft_json, published_version_id,
                          created_at, updated_at
                   FROM automation_workflows
                   ORDER BY updated_at DESC, id ASC"#,
            )
            .map_err(|error| AutomationStoreError::storage("prepare workflow list", error))?;
        let rows = statement
            .query_map([], raw_workflow)
            .map_err(|error| AutomationStoreError::storage("query workflow list", error))?;
        let raw = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AutomationStoreError::storage("read workflow list", error))?;
        raw.into_iter().map(decode_workflow).collect()
    }

    fn workflow(
        &self,
        workflow_id: &str,
    ) -> Result<Option<AutomationWorkflow>, AutomationStoreError> {
        workflow_on(&self.connection.lock(), workflow_id)
    }

    fn save_draft(
        &mut self,
        input: AutomationSaveDraftInput,
    ) -> Result<AutomationWorkflow, AutomationStoreError> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(AutomationStoreError::InvalidWorkflowName);
        }
        validate_automation_graph(&input.nodes, &input.edges)?;

        let workflow_id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let scope = normalize_scope(input.scope);
        let draft = AutomationDraft {
            nodes: input.nodes,
            edges: input.edges,
            scope: scope.clone(),
        };
        let scope_json = json_text(&scope, "scope_json")?;
        let draft_json = json_text(&draft, "draft_json")?;
        let now = now_millis();
        self.connection.lock()
            .execute(
                r#"INSERT INTO automation_workflows
                   (id, name, enabled, scope_json, draft_json, published_version_id,
                    created_at, updated_at)
                   VALUES (?1, ?2, 0, ?3, ?4, NULL, ?5, ?5)
                   ON CONFLICT(id) DO UPDATE SET
                     name = excluded.name,
                     scope_json = excluded.scope_json,
                     draft_json = excluded.draft_json,
                     updated_at = excluded.updated_at"#,
                params![workflow_id, name, scope_json, draft_json, now],
            )
            .map_err(|error| AutomationStoreError::storage("save workflow draft", error))?;
        workflow_on(&self.connection.lock(), &workflow_id)?.ok_or_else(|| {
            AutomationStoreError::SchemaInvariant {
                message: format!("workflow {workflow_id} disappeared after save"),
            }
        })
    }

    fn publish(
        &mut self,
        workflow_id: &str,
    ) -> Result<AutomationWorkflowVersion, AutomationStoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AutomationStoreError::storage("begin workflow publish", error))?;
        let workflow = required_workflow(&transaction, workflow_id)?;
        validate_automation_graph(&workflow.draft.nodes, &workflow.draft.edges)?;
        let next_version = transaction
            .query_row(
                r#"SELECT COALESCE(MAX(version), 0) + 1
                   FROM automation_workflow_versions WHERE workflow_id = ?1"#,
                params![workflow_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| AutomationStoreError::storage("read next workflow version", error))?;
        let version = AutomationWorkflowVersion {
            id: Uuid::new_v4().to_string(),
            workflow_id: workflow_id.to_owned(),
            version: next_version,
            snapshot: workflow.draft,
            created_at: now_millis(),
        };
        let snapshot_json = json_text(&version.snapshot, "snapshot_json")?;
        transaction
            .execute(
                r#"INSERT INTO automation_workflow_versions
                   (id, workflow_id, version, snapshot_json, created_at)
                   VALUES (?1, ?2, ?3, ?4, ?5)"#,
                params![
                    version.id,
                    version.workflow_id,
                    version.version,
                    snapshot_json,
                    version.created_at
                ],
            )
            .map_err(|error| AutomationStoreError::storage("insert workflow version", error))?;
        let updated = transaction
            .execute(
                r#"UPDATE automation_workflows
                   SET published_version_id = ?1, updated_at = ?2 WHERE id = ?3"#,
                params![version.id, version.created_at, workflow_id],
            )
            .map_err(|error| AutomationStoreError::storage("publish workflow version", error))?;
        if updated != 1 {
            return Err(AutomationStoreError::SchemaInvariant {
                message: format!("workflow {workflow_id} disappeared during publish"),
            });
        }
        transaction
            .commit()
            .map_err(|error| AutomationStoreError::storage("commit workflow publish", error))?;
        Ok(version)
    }

    fn set_enabled(
        &mut self,
        workflow_id: &str,
        enabled: bool,
    ) -> Result<AutomationWorkflow, AutomationStoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AutomationStoreError::storage("begin workflow state update", error))?;
        let workflow = required_workflow(&transaction, workflow_id)?;
        if enabled && workflow.published_version_id.is_none() {
            return Err(AutomationStoreError::PublishedVersionRequired {
                workflow_id: workflow_id.to_owned(),
            });
        }
        transaction
            .execute(
                "UPDATE automation_workflows SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
                params![i64::from(enabled), now_millis(), workflow_id],
            )
            .map_err(|error| AutomationStoreError::storage("update workflow state", error))?;
        let workflow = required_workflow(&transaction, workflow_id)?;
        transaction.commit().map_err(|error| {
            AutomationStoreError::storage("commit workflow state update", error)
        })?;
        Ok(workflow)
    }

    fn delete_workflow(&mut self, workflow_id: &str) -> Result<(), AutomationStoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AutomationStoreError::storage("begin workflow deletion", error))?;
        required_workflow(&transaction, workflow_id)?;
        if let Some(run_id) = active_run_id(&transaction, workflow_id)? {
            return Err(AutomationStoreError::ActiveRunExists {
                workflow_id: workflow_id.to_owned(),
                run_id,
            });
        }

        transaction
            .execute(
                r#"DELETE FROM automation_run_nodes
                   WHERE run_id IN (SELECT id FROM automation_runs WHERE workflow_id = ?1)"#,
                params![workflow_id],
            )
            .map_err(|error| AutomationStoreError::storage("delete workflow run nodes", error))?;
        transaction
            .execute(
                "DELETE FROM automation_runs WHERE workflow_id = ?1",
                params![workflow_id],
            )
            .map_err(|error| AutomationStoreError::storage("delete workflow runs", error))?;
        transaction
            .execute(
                "DELETE FROM automation_workflow_versions WHERE workflow_id = ?1",
                params![workflow_id],
            )
            .map_err(|error| AutomationStoreError::storage("delete workflow versions", error))?;
        let deleted = transaction
            .execute(
                "DELETE FROM automation_workflows WHERE id = ?1",
                params![workflow_id],
            )
            .map_err(|error| AutomationStoreError::storage("delete workflow", error))?;
        if deleted != 1 {
            return Err(AutomationStoreError::SchemaInvariant {
                message: format!("workflow {workflow_id} disappeared during deletion"),
            });
        }
        transaction
            .commit()
            .map_err(|error| AutomationStoreError::storage("commit workflow deletion", error))
    }

    fn version(
        &self,
        version_id: &str,
    ) -> Result<Option<AutomationWorkflowVersion>, AutomationStoreError> {
        version_on(&self.connection.lock(), version_id)
    }

    fn try_begin_run(
        &mut self,
        input: AutomationBeginRunInput,
    ) -> Result<AutomationRunDetail, AutomationStoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AutomationStoreError::storage("begin automation run", error))?;
        let workflow = required_workflow(&transaction, &input.workflow_id)?;
        let version_id = workflow.published_version_id.clone().ok_or_else(|| {
            AutomationStoreError::PublishedVersionRequired {
                workflow_id: input.workflow_id.clone(),
            }
        })?;
        let version = version_on(&transaction, &version_id)?.ok_or_else(|| {
            AutomationStoreError::VersionNotFound {
                version_id: version_id.clone(),
            }
        })?;
        if version.workflow_id != input.workflow_id {
            return Err(AutomationStoreError::VersionWorkflowMismatch {
                version_id,
                expected_workflow_id: input.workflow_id,
                actual_workflow_id: version.workflow_id,
            });
        }
        validate_automation_graph(&version.snapshot.nodes, &version.snapshot.edges)?;
        if let Some(run_id) = active_run_id(&transaction, &workflow.id)? {
            return Err(AutomationStoreError::ActiveRunExists {
                workflow_id: workflow.id,
                run_id,
            });
        }

        let now = now_millis();
        let run = AutomationRun {
            id: Uuid::new_v4().to_string(),
            workflow_id: workflow.id,
            workflow_version_id: version.id,
            status: AutomationRunStatus::Running,
            trigger: input.trigger,
            scope: version.snapshot.scope,
            started_at: now,
            finished_at: None,
            error: None,
        };
        let trigger_json = json_text(&run.trigger, "trigger_json")?;
        let scope_json = json_text(&run.scope, "scope_json")?;
        transaction
            .execute(
                r#"INSERT INTO automation_runs
                   (id, workflow_id, workflow_version_id, status, trigger_json, scope_json,
                    started_at, finished_at, error)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL)"#,
                params![
                    run.id,
                    run.workflow_id,
                    run.workflow_version_id,
                    run.status.as_str(),
                    trigger_json,
                    scope_json,
                    run.started_at
                ],
            )
            .map_err(|error| AutomationStoreError::storage("insert automation run", error))?;

        let input_json = json_text(&serde_json::json!({}), "input_json")?;
        let mut nodes = Vec::with_capacity(version.snapshot.nodes.len());
        for node in version.snapshot.nodes {
            let state = AutomationRunNodeState {
                id: format!("{}:{}", run.id, node.id),
                run_id: run.id.clone(),
                node_id: node.id,
                status: AutomationRunStatus::Pending,
                input: serde_json::json!({}),
                output: None,
                error: None,
                started_at: None,
                finished_at: None,
            };
            transaction
                .execute(
                    r#"INSERT INTO automation_run_nodes
                       (id, run_id, node_id, status, input_json, output_json, error,
                        started_at, finished_at)
                       VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6, NULL)"#,
                    params![
                        state.id,
                        state.run_id,
                        state.node_id,
                        state.status.as_str(),
                        input_json,
                        state.started_at
                    ],
                )
                .map_err(|error| {
                    AutomationStoreError::storage("initialize automation run node", error)
                })?;
            nodes.push(state);
        }
        transaction
            .commit()
            .map_err(|error| AutomationStoreError::storage("commit automation run", error))?;
        Ok(AutomationRunDetail { run, nodes })
    }

    fn list_runs(
        &self,
        workflow_id: Option<&str>,
    ) -> Result<Vec<AutomationRunSummary>, AutomationStoreError> {
        let sql = if workflow_id.is_some() {
            r#"SELECT id, workflow_id, workflow_version_id, status, trigger_json,
                      started_at, finished_at, error
               FROM automation_runs WHERE workflow_id = ?1
               ORDER BY started_at DESC, id ASC LIMIT 100"#
        } else {
            r#"SELECT id, workflow_id, workflow_version_id, status, trigger_json,
                      started_at, finished_at, error
               FROM automation_runs ORDER BY started_at DESC, id ASC LIMIT 100"#
        };
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(sql)
            .map_err(|error| AutomationStoreError::storage("prepare run list", error))?;
        let raw = if let Some(workflow_id) = workflow_id {
            statement
                .query_map(params![workflow_id], raw_run_summary)
                .map_err(|error| AutomationStoreError::storage("query run list", error))?
                .collect::<Result<Vec<_>, _>>()
        } else {
            statement
                .query_map([], raw_run_summary)
                .map_err(|error| AutomationStoreError::storage("query run list", error))?
                .collect::<Result<Vec<_>, _>>()
        }
        .map_err(|error| AutomationStoreError::storage("read run list", error))?;
        raw.into_iter().map(decode_run_summary).collect()
    }

    fn run_detail(
        &self,
        run_id: &str,
    ) -> Result<Option<AutomationRunDetail>, AutomationStoreError> {
        let Some(run) = run_on(&self.connection.lock(), run_id)? else {
            return Ok(None);
        };
        let nodes = run_nodes_on(&self.connection.lock(), run_id)?;
        Ok(Some(AutomationRunDetail { run, nodes }))
    }

    fn apply_execution_transition(
        &mut self,
        transition: AutomationExecutionTransition,
    ) -> Result<AutomationRunDetail, AutomationStoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AutomationStoreError::storage("begin execution transition", error))?;
        let detail_id = transition.run_id.clone();
        let run = run_on(&transaction, &transition.run_id)?.ok_or_else(|| {
            AutomationStoreError::RunNotFound {
                run_id: transition.run_id.clone(),
            }
        })?;
        require_expected_status(
            AutomationRecordKind::Run,
            &run.id,
            run.status,
            &transition.run.expected_statuses,
        )?;

        let current_nodes = run_nodes_on(&transaction, &run.id)?
            .into_iter()
            .map(|state| (state.node_id.clone(), state))
            .collect::<BTreeMap<_, _>>();
        let mut changed_nodes = BTreeSet::new();
        let now = now_millis();
        for update in &transition.nodes {
            if !changed_nodes.insert(update.node_id.clone()) {
                return Err(AutomationStoreError::DuplicateNodeTransition {
                    node_id: update.node_id.clone(),
                });
            }
            let current = current_nodes.get(&update.node_id).ok_or_else(|| {
                AutomationStoreError::RunNodeNotFound {
                    run_id: run.id.clone(),
                    node_id: update.node_id.clone(),
                }
            })?;
            require_expected_status(
                AutomationRecordKind::RunNode,
                &current.id,
                current.status,
                &update.expected_statuses,
            )?;
            let input_json = json_text(&update.input, "input_json")?;
            let output_json = update
                .output
                .as_ref()
                .map(|output| json_text(output, "output_json"))
                .transpose()?;
            let changed = transaction
                .execute(
                    r#"UPDATE automation_run_nodes
                       SET status = ?1,
                           input_json = ?2,
                           output_json = ?3,
                           error = ?4,
                           started_at = CASE
                             WHEN ?5 = 1 THEN COALESCE(started_at, ?6)
                             ELSE started_at
                           END,
                           finished_at = CASE WHEN ?7 = 1 THEN ?6 ELSE NULL END
                       WHERE run_id = ?8 AND node_id = ?9"#,
                    params![
                        update.status.as_str(),
                        input_json,
                        output_json,
                        update.error,
                        i64::from(update.mark_started),
                        now,
                        i64::from(update.finished),
                        run.id,
                        update.node_id,
                    ],
                )
                .map_err(|error| AutomationStoreError::storage("update execution node", error))?;
            if changed != 1 {
                return Err(AutomationStoreError::SchemaInvariant {
                    message: format!(
                        "run node {}/{} disappeared during execution transition",
                        run.id, update.node_id
                    ),
                });
            }
        }

        let changed = transaction
            .execute(
                r#"UPDATE automation_runs
                   SET status = ?1,
                       error = ?2,
                       finished_at = CASE WHEN ?3 = 1 THEN ?4 ELSE NULL END
                   WHERE id = ?5"#,
                params![
                    transition.run.status.as_str(),
                    transition.run.error,
                    i64::from(transition.run.finished),
                    now,
                    run.id,
                ],
            )
            .map_err(|error| AutomationStoreError::storage("update execution run", error))?;
        if changed != 1 {
            return Err(AutomationStoreError::SchemaInvariant {
                message: format!("run {} disappeared during execution transition", run.id),
            });
        }
        transaction
            .commit()
            .map_err(|error| AutomationStoreError::storage("commit execution transition", error))?;
        drop(connection);
        self.run_detail(&detail_id)?
            .ok_or_else(|| AutomationStoreError::SchemaInvariant {
                message: format!("run {detail_id} disappeared after execution transition"),
            })
    }
}

fn require_expected_status(
    record_kind: AutomationRecordKind,
    record_id: &str,
    actual: AutomationRunStatus,
    expected: &[AutomationRunStatus],
) -> Result<(), AutomationStoreError> {
    if expected.contains(&actual) {
        return Ok(());
    }
    Err(AutomationStoreError::InvalidStateTransition {
        record_kind,
        record_id: record_id.to_owned(),
        expected: expected.to_vec(),
        actual,
    })
}

#[derive(Debug)]
struct RawWorkflow {
    id: String,
    name: String,
    enabled: bool,
    scope_json: String,
    draft_json: String,
    published_version_id: Option<String>,
    created_at: i64,
    updated_at: i64,
}

fn raw_workflow(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawWorkflow> {
    Ok(RawWorkflow {
        id: row.get(0)?,
        name: row.get(1)?,
        enabled: row.get::<_, i64>(2)? != 0,
        scope_json: row.get(3)?,
        draft_json: row.get(4)?,
        published_version_id: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn decode_workflow(raw: RawWorkflow) -> Result<AutomationWorkflow, AutomationStoreError> {
    let scope = parse_json(
        AutomationRecordKind::Workflow,
        &raw.id,
        "scope_json",
        &raw.scope_json,
    )?;
    let draft = parse_json(
        AutomationRecordKind::Workflow,
        &raw.id,
        "draft_json",
        &raw.draft_json,
    )?;
    Ok(AutomationWorkflow {
        id: raw.id,
        name: raw.name,
        enabled: raw.enabled,
        scope,
        draft,
        published_version_id: raw.published_version_id,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
    })
}

fn workflow_on(
    connection: &Connection,
    workflow_id: &str,
) -> Result<Option<AutomationWorkflow>, AutomationStoreError> {
    let raw = connection
        .query_row(
            r#"SELECT id, name, enabled, scope_json, draft_json, published_version_id,
                      created_at, updated_at
               FROM automation_workflows WHERE id = ?1"#,
            params![workflow_id],
            raw_workflow,
        )
        .optional()
        .map_err(|error| AutomationStoreError::storage("read workflow", error))?;
    raw.map(decode_workflow).transpose()
}

fn required_workflow(
    connection: &Connection,
    workflow_id: &str,
) -> Result<AutomationWorkflow, AutomationStoreError> {
    workflow_on(connection, workflow_id)?.ok_or_else(|| AutomationStoreError::WorkflowNotFound {
        workflow_id: workflow_id.to_owned(),
    })
}

#[derive(Debug)]
struct RawVersion {
    id: String,
    workflow_id: String,
    version: i64,
    snapshot_json: String,
    created_at: i64,
}

fn raw_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawVersion> {
    Ok(RawVersion {
        id: row.get(0)?,
        workflow_id: row.get(1)?,
        version: row.get(2)?,
        snapshot_json: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn decode_version(raw: RawVersion) -> Result<AutomationWorkflowVersion, AutomationStoreError> {
    let snapshot = parse_json(
        AutomationRecordKind::WorkflowVersion,
        &raw.id,
        "snapshot_json",
        &raw.snapshot_json,
    )?;
    Ok(AutomationWorkflowVersion {
        id: raw.id,
        workflow_id: raw.workflow_id,
        version: raw.version,
        snapshot,
        created_at: raw.created_at,
    })
}

fn version_on(
    connection: &Connection,
    version_id: &str,
) -> Result<Option<AutomationWorkflowVersion>, AutomationStoreError> {
    let raw = connection
        .query_row(
            r#"SELECT id, workflow_id, version, snapshot_json, created_at
               FROM automation_workflow_versions WHERE id = ?1"#,
            params![version_id],
            raw_version,
        )
        .optional()
        .map_err(|error| AutomationStoreError::storage("read workflow version", error))?;
    raw.map(decode_version).transpose()
}

#[derive(Debug)]
struct RawRun {
    id: String,
    workflow_id: String,
    workflow_version_id: String,
    status: String,
    trigger_json: String,
    scope_json: String,
    started_at: i64,
    finished_at: Option<i64>,
    error: Option<String>,
}

fn raw_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRun> {
    Ok(RawRun {
        id: row.get(0)?,
        workflow_id: row.get(1)?,
        workflow_version_id: row.get(2)?,
        status: row.get(3)?,
        trigger_json: row.get(4)?,
        scope_json: row.get(5)?,
        started_at: row.get(6)?,
        finished_at: row.get(7)?,
        error: row.get(8)?,
    })
}

fn decode_run(raw: RawRun) -> Result<AutomationRun, AutomationStoreError> {
    let status = parse_status(AutomationRecordKind::Run, &raw.id, &raw.status)?;
    let trigger = parse_json(
        AutomationRecordKind::Run,
        &raw.id,
        "trigger_json",
        &raw.trigger_json,
    )?;
    let scope = parse_json(
        AutomationRecordKind::Run,
        &raw.id,
        "scope_json",
        &raw.scope_json,
    )?;
    Ok(AutomationRun {
        id: raw.id,
        workflow_id: raw.workflow_id,
        workflow_version_id: raw.workflow_version_id,
        status,
        trigger,
        scope,
        started_at: raw.started_at,
        finished_at: raw.finished_at,
        error: raw.error,
    })
}

fn run_on(
    connection: &Connection,
    run_id: &str,
) -> Result<Option<AutomationRun>, AutomationStoreError> {
    let raw = connection
        .query_row(
            r#"SELECT id, workflow_id, workflow_version_id, status, trigger_json, scope_json,
                      started_at, finished_at, error
               FROM automation_runs WHERE id = ?1"#,
            params![run_id],
            raw_run,
        )
        .optional()
        .map_err(|error| AutomationStoreError::storage("read automation run", error))?;
    raw.map(decode_run).transpose()
}

#[derive(Debug)]
struct RawRunSummary {
    id: String,
    workflow_id: String,
    workflow_version_id: String,
    status: String,
    trigger_json: String,
    started_at: i64,
    finished_at: Option<i64>,
    error: Option<String>,
}

fn raw_run_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRunSummary> {
    Ok(RawRunSummary {
        id: row.get(0)?,
        workflow_id: row.get(1)?,
        workflow_version_id: row.get(2)?,
        status: row.get(3)?,
        trigger_json: row.get(4)?,
        started_at: row.get(5)?,
        finished_at: row.get(6)?,
        error: row.get(7)?,
    })
}

fn decode_run_summary(raw: RawRunSummary) -> Result<AutomationRunSummary, AutomationStoreError> {
    let status = parse_status(AutomationRecordKind::Run, &raw.id, &raw.status)?;
    let trigger: AutomationSignalEnvelope = parse_json(
        AutomationRecordKind::Run,
        &raw.id,
        "trigger_json",
        &raw.trigger_json,
    )?;
    Ok(AutomationRunSummary {
        id: raw.id,
        workflow_id: raw.workflow_id,
        workflow_version_id: raw.workflow_version_id,
        status,
        trigger_kind: trigger.kind,
        project_id: trigger.project_id,
        task_id: trigger.task_id,
        backend: trigger.backend,
        event_kind: trigger.event_kind,
        started_at: raw.started_at,
        finished_at: raw.finished_at,
        error: raw.error,
    })
}

#[derive(Debug)]
struct RawRunNode {
    id: String,
    run_id: String,
    node_id: String,
    status: String,
    input_json: String,
    output_json: Option<String>,
    error: Option<String>,
    started_at: Option<i64>,
    finished_at: Option<i64>,
}

fn raw_run_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRunNode> {
    Ok(RawRunNode {
        id: row.get(0)?,
        run_id: row.get(1)?,
        node_id: row.get(2)?,
        status: row.get(3)?,
        input_json: row.get(4)?,
        output_json: row.get(5)?,
        error: row.get(6)?,
        started_at: row.get(7)?,
        finished_at: row.get(8)?,
    })
}

fn decode_run_node(raw: RawRunNode) -> Result<AutomationRunNodeState, AutomationStoreError> {
    let status = parse_status(AutomationRecordKind::RunNode, &raw.id, &raw.status)?;
    let input = parse_json(
        AutomationRecordKind::RunNode,
        &raw.id,
        "input_json",
        &raw.input_json,
    )?;
    let output = raw
        .output_json
        .as_deref()
        .map(|json| parse_json(AutomationRecordKind::RunNode, &raw.id, "output_json", json))
        .transpose()?;
    Ok(AutomationRunNodeState {
        id: raw.id,
        run_id: raw.run_id,
        node_id: raw.node_id,
        status,
        input,
        output,
        error: raw.error,
        started_at: raw.started_at,
        finished_at: raw.finished_at,
    })
}

fn run_nodes_on(
    connection: &Connection,
    run_id: &str,
) -> Result<Vec<AutomationRunNodeState>, AutomationStoreError> {
    let mut statement = connection
        .prepare(
            r#"SELECT id, run_id, node_id, status, input_json, output_json, error,
                      started_at, finished_at
               FROM automation_run_nodes WHERE run_id = ?1 ORDER BY id ASC"#,
        )
        .map_err(|error| AutomationStoreError::storage("prepare run nodes", error))?;
    let rows = statement
        .query_map(params![run_id], raw_run_node)
        .map_err(|error| AutomationStoreError::storage("query run nodes", error))?;
    let raw = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AutomationStoreError::storage("read run nodes", error))?;
    raw.into_iter().map(decode_run_node).collect()
}

fn active_run_id(
    connection: &Connection,
    workflow_id: &str,
) -> Result<Option<String>, AutomationStoreError> {
    connection
        .query_row(
            r#"SELECT id FROM automation_runs
               WHERE workflow_id = ?1 AND status IN ('pending', 'running', 'waiting_user')
               ORDER BY started_at ASC, id ASC LIMIT 1"#,
            params![workflow_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| AutomationStoreError::storage("read active automation run", error))
}

fn existing_active_run_conflicts(
    connection: &Connection,
) -> Result<Vec<AutomationActiveRunConflict>, AutomationStoreError> {
    let mut statement = connection
        .prepare(
            r#"SELECT workflow_id, id FROM automation_runs
               WHERE status IN ('pending', 'running', 'waiting_user')
               ORDER BY workflow_id ASC, started_at ASC, id ASC"#,
        )
        .map_err(|error| AutomationStoreError::storage("prepare active-run audit", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| AutomationStoreError::storage("query active-run audit", error))?;
    let mut by_workflow = BTreeMap::<String, Vec<String>>::new();
    for row in rows {
        let (workflow_id, run_id) =
            row.map_err(|error| AutomationStoreError::storage("read active-run audit", error))?;
        by_workflow.entry(workflow_id).or_default().push(run_id);
    }
    Ok(by_workflow
        .into_iter()
        .filter_map(|(workflow_id, run_ids)| {
            (run_ids.len() > 1).then_some(AutomationActiveRunConflict {
                workflow_id,
                run_ids,
            })
        })
        .collect())
}

fn parse_json<T: DeserializeOwned>(
    record_kind: AutomationRecordKind,
    record_id: &str,
    field: &'static str,
    text: &str,
) -> Result<T, AutomationStoreError> {
    serde_json::from_str(text).map_err(|error| AutomationStoreError::CorruptJson {
        record_kind,
        record_id: record_id.to_owned(),
        field,
        message: error.to_string(),
    })
}

fn parse_status(
    record_kind: AutomationRecordKind,
    record_id: &str,
    status: &str,
) -> Result<AutomationRunStatus, AutomationStoreError> {
    AutomationRunStatus::from_storage(status).ok_or_else(|| {
        AutomationStoreError::InvalidStoredStatus {
            record_kind,
            record_id: record_id.to_owned(),
            status: status.to_owned(),
        }
    })
}

fn json_text<T: Serialize>(value: &T, field: &'static str) -> Result<String, AutomationStoreError> {
    serde_json::to_string(value).map_err(|error| AutomationStoreError::Serialization {
        field,
        message: error.to_string(),
    })
}

fn normalize_scope(mut scope: AutomationScopeFilter) -> AutomationScopeFilter {
    scope.task_statuses = normalize_string_list(scope.task_statuses)
        .into_iter()
        .filter(|status| {
            scope_task_statuses()
                .iter()
                .any(|allowed| allowed == status)
        })
        .collect();
    scope.event_kinds = normalize_string_list(scope.event_kinds)
        .into_iter()
        .filter(|kind| scope_event_kinds().iter().any(|allowed| allowed == kind))
        .collect();
    scope
}

fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim();
        if !value.is_empty() && !normalized.iter().any(|existing| existing == value) {
            normalized.push(value.to_owned());
        }
    }
    normalized
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
    use std::path::Path;
    use std::sync::{Arc, Barrier};

    use super::*;
    use crate::{AutomationEdge, AutomationNode, AutomationNodePosition};
    use crate::{AutomationNodeStateUpdate, AutomationRunStateUpdate};

    fn node(id: &str, kind: &str) -> AutomationNode {
        AutomationNode {
            id: id.to_owned(),
            kind: kind.to_owned(),
            title: id.to_owned(),
            position: AutomationNodePosition { x: 0.0, y: 0.0 },
            config: serde_json::json!({}),
        }
    }

    fn edge(source: &str, target: &str) -> AutomationEdge {
        AutomationEdge {
            id: format!("{source}-{target}"),
            source: source.to_owned(),
            target: target.to_owned(),
            source_handle: None,
            target_handle: None,
        }
    }

    fn workflow_input(id: &str) -> AutomationSaveDraftInput {
        AutomationSaveDraftInput {
            id: Some(id.to_owned()),
            name: "Build release".to_owned(),
            scope: AutomationScopeFilter {
                task_statuses: vec![
                    " running ".to_owned(),
                    "unknown".to_owned(),
                    "running".to_owned(),
                ],
                event_kinds: vec!["task_created".to_owned(), "unknown".to_owned()],
                ..AutomationScopeFilter::default()
            },
            nodes: vec![node("trigger", "trigger"), node("tool", "tool")],
            edges: vec![edge("trigger", "tool")],
        }
    }

    fn signal(id: &str) -> AutomationSignalEnvelope {
        AutomationSignalEnvelope {
            id: id.to_owned(),
            kind: "manual".to_owned(),
            project_id: None,
            task_id: None,
            backend: None,
            event_kind: None,
            automation_run_id: None,
            payload: serde_json::json!({"source": "test"}),
            created_at: now_millis(),
        }
    }

    fn published_store(path: &Path) -> SqliteAutomationStore {
        let mut store = SqliteAutomationStore::open(path).unwrap();
        store.save_draft(workflow_input("workflow-1")).unwrap();
        store.publish("workflow-1").unwrap();
        store
    }

    #[test]
    fn existing_status_schema_is_migrated_without_losing_run_data() {
        let connection = Db::in_memory().unwrap();
        connection
            .lock()
            .execute_batch(&SCHEMA.replace(",'cancelled'", ""))
            .unwrap();
        let draft = AutomationDraft {
            nodes: vec![node("trigger", "trigger")],
            edges: Vec::new(),
            scope: AutomationScopeFilter::default(),
        };
        let draft_json = serde_json::to_string(&draft).unwrap();
        let scope_json = serde_json::to_string(&AutomationScopeFilter::default()).unwrap();
        let trigger_json = serde_json::to_string(&signal("legacy-signal")).unwrap();
        connection
            .lock()
            .execute(
                "INSERT INTO automation_workflows (id, name, enabled, scope_json, draft_json, published_version_id, created_at, updated_at) VALUES ('workflow-1', 'Legacy', 1, ?1, ?2, 'version-1', 1, 1)",
                params![scope_json, draft_json],
            )
            .unwrap();
        connection
            .lock()
            .execute(
                "INSERT INTO automation_workflow_versions (id, workflow_id, version, snapshot_json, created_at) VALUES ('version-1', 'workflow-1', 1, ?1, 1)",
                [serde_json::to_string(&draft).unwrap()],
            )
            .unwrap();
        connection
            .lock()
            .execute(
                "INSERT INTO automation_runs (id, workflow_id, workflow_version_id, status, trigger_json, scope_json, started_at) VALUES ('run-1', 'workflow-1', 'version-1', 'running', ?1, ?2, 1)",
                params![trigger_json, serde_json::to_string(&AutomationScopeFilter::default()).unwrap()],
            )
            .unwrap();
        connection
            .lock()
            .execute(
                "INSERT INTO automation_run_nodes (id, run_id, node_id, status, input_json, started_at) VALUES ('run-node-1', 'run-1', 'trigger', 'running', '{}', 1)",
                [],
            )
            .unwrap();

        let mut store = SqliteAutomationStore::from_db(connection).unwrap();
        let migrated = store.run_detail("run-1").unwrap().unwrap();
        assert_eq!(migrated.run.status, AutomationRunStatus::Running);
        assert_eq!(migrated.nodes.len(), 1);
        let cancelled = store
            .apply_execution_transition(AutomationExecutionTransition {
                run_id: "run-1".to_owned(),
                run: AutomationRunStateUpdate {
                    expected_statuses: vec![AutomationRunStatus::Running],
                    status: AutomationRunStatus::Cancelled,
                    error: None,
                    finished: true,
                },
                nodes: vec![AutomationNodeStateUpdate {
                    node_id: "trigger".to_owned(),
                    expected_statuses: vec![AutomationRunStatus::Running],
                    status: AutomationRunStatus::Cancelled,
                    input: serde_json::json!({}),
                    output: None,
                    error: None,
                    mark_started: false,
                    finished: true,
                }],
            })
            .unwrap();
        assert_eq!(cancelled.run.status, AutomationRunStatus::Cancelled);
        assert_eq!(cancelled.nodes[0].status, AutomationRunStatus::Cancelled);
    }

    #[test]
    fn draft_and_publish_preserve_contract_shape_and_snapshot_history() {
        let mut store = SqliteAutomationStore::in_memory().unwrap();
        let first = store.save_draft(workflow_input("workflow-1")).unwrap();
        assert_eq!(first.scope.task_statuses, vec!["running"]);
        assert_eq!(first.scope.event_kinds, vec!["task_created"]);
        assert_eq!(first.draft.scope, first.scope);

        let version_one = store.publish("workflow-1").unwrap();
        assert_eq!(version_one.version, 1);
        let mut changed = workflow_input("workflow-1");
        changed.name = "Build and ship".to_owned();
        changed.nodes[1].title = "Ship".to_owned();
        store.save_draft(changed).unwrap();
        let version_two = store.publish("workflow-1").unwrap();

        assert_eq!(version_two.version, 2);
        assert_eq!(
            store
                .version(&version_one.id)
                .unwrap()
                .unwrap()
                .snapshot
                .nodes[1]
                .title,
            "tool"
        );
        assert_eq!(
            store
                .version(&version_two.id)
                .unwrap()
                .unwrap()
                .snapshot
                .nodes[1]
                .title,
            "Ship"
        );
    }

    #[test]
    fn concurrent_begin_allows_exactly_one_active_run() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("automation.sqlite3");
        drop(published_store(&path));
        let barrier = Arc::new(Barrier::new(2));
        let handles = (0..2)
            .map(|index| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let mut store = SqliteAutomationStore::open(path).unwrap();
                    barrier.wait();
                    store.try_begin_run(AutomationBeginRunInput {
                        workflow_id: "workflow-1".to_owned(),
                        trigger: signal(&format!("signal-{index}")),
                    })
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
                .filter(|result| matches!(
                    result,
                    Err(AutomationStoreError::ActiveRunExists { .. })
                ))
                .count(),
            1
        );
        let store = SqliteAutomationStore::open(path).unwrap();
        let runs = store.list_runs(Some("workflow-1")).unwrap();
        assert_eq!(runs.len(), 1);
        let detail = store.run_detail(&runs[0].id).unwrap().unwrap();
        assert_eq!(detail.nodes.len(), 2);
        assert!(detail
            .nodes
            .iter()
            .all(|node| node.status == AutomationRunStatus::Pending));
    }

    #[test]
    fn begin_run_rolls_back_run_when_node_initialization_fails() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("automation.sqlite3");
        let mut store = published_store(&path);
        store
            .connection
            .lock()
            .execute_batch(
                r#"CREATE TRIGGER fail_tool_node
                   BEFORE INSERT ON automation_run_nodes
                   WHEN NEW.node_id = 'tool'
                   BEGIN
                     SELECT RAISE(ABORT, 'injected node failure');
                   END;"#,
            )
            .unwrap();

        let result = store.try_begin_run(AutomationBeginRunInput {
            workflow_id: "workflow-1".to_owned(),
            trigger: signal("rollback"),
        });
        assert!(matches!(result, Err(AutomationStoreError::Storage { .. })));
        assert!(store.list_runs(Some("workflow-1")).unwrap().is_empty());
        let node_count = store
            .connection
            .lock()
            .query_row("SELECT COUNT(*) FROM automation_run_nodes", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(node_count, 0);
    }

    #[test]
    fn delete_rejects_active_run_without_changing_workflow() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("automation.sqlite3");
        let mut store = published_store(&path);
        let run = store
            .try_begin_run(AutomationBeginRunInput {
                workflow_id: "workflow-1".to_owned(),
                trigger: signal("active-delete"),
            })
            .unwrap();

        assert!(matches!(
            store.delete_workflow("workflow-1"),
            Err(AutomationStoreError::ActiveRunExists { run_id, .. }) if run_id == run.run.id
        ));
        assert!(store.workflow("workflow-1").unwrap().is_some());
        assert!(store.run_detail(&run.run.id).unwrap().is_some());
    }

    #[test]
    fn corrupt_json_is_reported_with_record_and_field() {
        let mut store = SqliteAutomationStore::in_memory().unwrap();
        store.save_draft(workflow_input("workflow-1")).unwrap();
        store
            .connection
            .lock()
            .execute(
                "UPDATE automation_workflows SET draft_json = '{' WHERE id = 'workflow-1'",
                [],
            )
            .unwrap();

        assert!(matches!(
            store.workflow("workflow-1"),
            Err(AutomationStoreError::CorruptJson {
                record_kind: AutomationRecordKind::Workflow,
                record_id,
                field: "draft_json",
                ..
            }) if record_id == "workflow-1"
        ));
    }

    #[test]
    fn legacy_active_run_conflict_is_typed_and_history_is_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.sqlite3");
        {
            let connection = Db::open(&path).unwrap();
            connection.lock().execute_batch(SCHEMA).unwrap();
            connection
                .lock()
                .execute(
                    r#"INSERT INTO automation_workflows
                       (id, name, enabled, scope_json, draft_json, created_at, updated_at)
                       VALUES ('workflow-1', 'Legacy', 0, '{}', '{"nodes":[],"edges":[],"scope":{}}', 1, 1)"#,
                    [],
                )
                .unwrap();
            connection
                .lock()
                .execute(
                    r#"INSERT INTO automation_workflow_versions
                       (id, workflow_id, version, snapshot_json, created_at)
                       VALUES ('version-1', 'workflow-1', 1, '{"nodes":[],"edges":[],"scope":{}}', 1)"#,
                    [],
                )
                .unwrap();
            for (id, status) in [("run-1", "running"), ("run-2", "waiting_user")] {
                connection
                    .lock()
                    .execute(
                        r#"INSERT INTO automation_runs
                           (id, workflow_id, workflow_version_id, status, trigger_json, scope_json,
                            started_at)
                           VALUES (?1, 'workflow-1', 'version-1', ?2,
                                   '{"id":"signal","kind":"manual","payload":{},"createdAt":1}',
                                   '{}', 1)"#,
                        params![id, status],
                    )
                    .unwrap();
            }
        }

        let error = match SqliteAutomationStore::open(&path) {
            Ok(_) => panic!("legacy conflict must prevent adapter initialization"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            AutomationStoreError::ExistingActiveRunConflict { conflicts }
                if conflicts == vec![AutomationActiveRunConflict {
                    workflow_id: "workflow-1".to_owned(),
                    run_ids: vec!["run-1".to_owned(), "run-2".to_owned()],
                }]
        ));
        let connection = Db::open(path).unwrap();
        let locked = connection.lock();
        let statuses = locked
            .prepare("SELECT status FROM automation_runs ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(statuses, vec!["running", "waiting_user"]);
    }
}

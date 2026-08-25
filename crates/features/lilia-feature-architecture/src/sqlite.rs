use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_storage::Db;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use uuid::Uuid;

use super::{
    ArchitectureBackend, ArchitectureChangeStatus, ArchitecturePermission, ArchitectureStore,
    DesktopArchitectureError, ProjectArchitectureApplyInput, ProjectArchitectureApplyResult,
    ProjectArchitectureChange, ProjectArchitectureChangeEvent, ProjectArchitectureChangeRecord,
    ProjectArchitectureEdge, ProjectArchitectureGraph, ProjectArchitectureNode,
    ProjectArchitectureQuarantineRecord, ProjectArchitectureRejectInput,
    ProjectArchitectureRollbackResult,
};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS project_architecture_graphs (
  project_id TEXT PRIMARY KEY,
  version INTEGER NOT NULL,
  graph_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS project_architecture_changes (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  task_id TEXT NOT NULL,
  turn_id TEXT,
  backend TEXT NOT NULL,
  status TEXT NOT NULL,
  permission_mode TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  changes_json TEXT NOT NULL,
  before_graph_json TEXT,
  after_graph_json TEXT,
  created_at INTEGER NOT NULL,
  resolved_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_project_architecture_changes_project_created
  ON project_architecture_changes(project_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_project_architecture_changes_task
  ON project_architecture_changes(task_id, created_at DESC);

CREATE TABLE IF NOT EXISTS project_architecture_quarantine (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  record_kind TEXT NOT NULL,
  record_id TEXT NOT NULL,
  reason_code TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  quarantined_at INTEGER NOT NULL,
  UNIQUE(record_kind, record_id, reason_code, payload_json)
);

CREATE INDEX IF NOT EXISTS idx_project_architecture_quarantine_project_time
  ON project_architecture_quarantine(project_id, quarantined_at DESC);
"#;

pub struct SqliteArchitectureStore {
    connection: Db,
}

impl SqliteArchitectureStore {
    pub fn in_memory() -> Result<Self, DesktopArchitectureError> {
        let connection = Db::in_memory()
            .map_err(|error| DesktopArchitectureError::storage("open in-memory database", error))?;
        Self::from_db(connection)
    }

    pub fn from_db(connection: Db) -> Result<Self, DesktopArchitectureError> {
        connection
            .lock()
            .execute_batch(SCHEMA)
            .map_err(|error| DesktopArchitectureError::storage("initialize schema", error))?;
        Ok(Self { connection })
    }
}

impl ArchitectureStore for SqliteArchitectureStore {
    fn graph(
        &mut self,
        project_id: &str,
    ) -> Result<ProjectArchitectureGraph, DesktopArchitectureError> {
        validate_identity(project_id, "project")?;
        let mut connection = self.connection.lock();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| DesktopArchitectureError::storage("begin graph read", error))?;
        let graph = load_graph_recovering(&transaction, project_id)?;
        transaction
            .commit()
            .map_err(|error| DesktopArchitectureError::storage("commit graph recovery", error))?;
        Ok(graph)
    }

    fn list_changes(
        &mut self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<ProjectArchitectureChangeRecord>, DesktopArchitectureError> {
        validate_identity(project_id, "project")?;
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut connection = self.connection.lock();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| DesktopArchitectureError::storage("begin change history", error))?;
        let raw_records = {
            let mut statement = transaction
                .prepare(
                    r#"SELECT id, project_id, task_id, turn_id, backend, status, permission_mode,
                          summary, changes_json, before_graph_json, after_graph_json,
                          created_at, resolved_at
                   FROM project_architecture_changes
                   WHERE project_id = ?1
                   ORDER BY created_at DESC, rowid DESC"#,
                )
                .map_err(|error| {
                    DesktopArchitectureError::storage("prepare change history", error)
                })?;
            let rows = statement
                .query_map(params![project_id], raw_record)
                .map_err(|error| {
                    DesktopArchitectureError::storage("query change history", error)
                })?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| DesktopArchitectureError::storage("read change history", error))?
        };
        let mut records = Vec::with_capacity(limit.min(raw_records.len()));
        for raw in raw_records {
            if let Some(record) = decode_record_or_quarantine(&transaction, raw)? {
                records.push(record);
                if records.len() == limit {
                    break;
                }
            }
        }
        transaction
            .commit()
            .map_err(|error| DesktopArchitectureError::storage("commit change history", error))?;
        Ok(records)
    }

    fn list_quarantine(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectArchitectureQuarantineRecord>, DesktopArchitectureError> {
        validate_identity(project_id, "project")?;
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(
                r#"SELECT id, project_id, record_kind, record_id, reason_code, quarantined_at
                   FROM project_architecture_quarantine
                   WHERE project_id = ?1
                   ORDER BY quarantined_at DESC, rowid DESC"#,
            )
            .map_err(|error| DesktopArchitectureError::storage("prepare quarantine", error))?;
        let rows = statement
            .query_map(params![project_id], |row| {
                Ok(ProjectArchitectureQuarantineRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    record_kind: row.get(2)?,
                    record_id: row.get(3)?,
                    reason_code: row.get(4)?,
                    quarantined_at: row.get(5)?,
                })
            })
            .map_err(|error| DesktopArchitectureError::storage("query quarantine", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| DesktopArchitectureError::storage("read quarantine", error))
    }

    fn apply(
        &mut self,
        mut input: ProjectArchitectureApplyInput,
    ) -> Result<ProjectArchitectureApplyResult, DesktopArchitectureError> {
        validate_identity(&input.project_id, "project")?;
        validate_identity(&input.task_id, "task")?;
        input.reason = input.reason.trim().to_owned();
        input.changes = normalize_changes(input.changes)?;
        if input.changes.is_empty() {
            return Err(DesktopArchitectureError::EmptyChanges);
        }
        let mut connection = self.connection.lock();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| DesktopArchitectureError::storage("begin apply", error))?;
        let request_id = normalized_request_id(input.request_id.as_deref());
        if let Some(existing) = record_by_id(&transaction, &request_id)? {
            ensure_idempotent_apply(&existing, &input)?;
            let graph = existing.after_graph.clone().ok_or_else(|| {
                DesktopArchitectureError::storage(
                    "replay apply",
                    "applied history has no after graph",
                )
            })?;
            return Ok(ProjectArchitectureApplyResult {
                graph,
                event: existing.event,
            });
        }
        let before = load_graph_recovering(&transaction, &input.project_id)?;
        validate_expected_version(input.expected_version, before.version)?;
        let now = now_millis();
        let after = apply_changes(&before, &input.changes, now)?;
        write_graph(&transaction, &after)?;
        let event = insert_history(
            &transaction,
            &request_id,
            &input.project_id,
            &input.task_id,
            input.turn_id.as_deref(),
            input.backend,
            ArchitectureChangeStatus::Applied,
            input.permission,
            &input.reason,
            &input.changes,
            Some(&before),
            Some(&after),
            now,
            Some(now),
        )?;
        transaction
            .commit()
            .map_err(|error| DesktopArchitectureError::storage("commit apply", error))?;
        Ok(ProjectArchitectureApplyResult {
            graph: after,
            event,
        })
    }

    fn reject(
        &mut self,
        mut input: ProjectArchitectureRejectInput,
    ) -> Result<ProjectArchitectureChangeEvent, DesktopArchitectureError> {
        validate_identity(&input.project_id, "project")?;
        validate_identity(&input.task_id, "task")?;
        input.reason = input.reason.trim().to_owned();
        input.changes = normalize_changes(input.changes)?;
        let mut connection = self.connection.lock();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| DesktopArchitectureError::storage("begin reject", error))?;
        let request_id = normalized_request_id(input.request_id.as_deref());
        if let Some(existing) = record_by_id(&transaction, &request_id)? {
            ensure_idempotent_reject(&existing, &input)?;
            return Ok(existing.event);
        }
        let before = load_graph_recovering(&transaction, &input.project_id)?;
        validate_expected_version(input.expected_version, before.version)?;
        let now = now_millis();
        let event = insert_history(
            &transaction,
            &request_id,
            &input.project_id,
            &input.task_id,
            input.turn_id.as_deref(),
            input.backend,
            ArchitectureChangeStatus::Rejected,
            input.permission,
            &input.reason,
            &input.changes,
            Some(&before),
            None,
            now,
            Some(now),
        )?;
        transaction
            .commit()
            .map_err(|error| DesktopArchitectureError::storage("commit reject", error))?;
        Ok(event)
    }

    fn rollback(
        &mut self,
        project_id: &str,
        task_id: &str,
        backend: ArchitectureBackend,
    ) -> Result<ProjectArchitectureRollbackResult, DesktopArchitectureError> {
        validate_identity(project_id, "project")?;
        validate_identity(task_id, "task")?;
        let mut connection = self.connection.lock();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| DesktopArchitectureError::storage("begin rollback", error))?;
        let current = load_graph_recovering(&transaction, project_id)?;
        let latest = latest_applied_record(&transaction, project_id)?;
        let Some(latest) = latest else {
            return Ok(ProjectArchitectureRollbackResult {
                graph: current,
                event: None,
            });
        };
        if latest.after_graph.as_ref() != Some(&current) {
            return Ok(ProjectArchitectureRollbackResult {
                graph: current,
                event: None,
            });
        }
        let Some(mut restored) = latest.before_graph else {
            return Ok(ProjectArchitectureRollbackResult {
                graph: current,
                event: None,
            });
        };
        let now = now_millis();
        restored.version = current.version.saturating_add(1);
        restored.updated_at = now;
        write_graph(&transaction, &restored)?;
        let event = insert_history(
            &transaction,
            &Uuid::new_v4().to_string(),
            project_id,
            task_id,
            None,
            backend,
            ArchitectureChangeStatus::RolledBack,
            ArchitecturePermission::Full,
            "回滚到上一版本",
            &[],
            Some(&current),
            Some(&restored),
            now,
            Some(now),
        )?;
        transaction
            .commit()
            .map_err(|error| DesktopArchitectureError::storage("commit rollback", error))?;
        Ok(ProjectArchitectureRollbackResult {
            graph: restored,
            event: Some(event),
        })
    }
}

fn validate_identity(value: &str, kind: &'static str) -> Result<(), DesktopArchitectureError> {
    if !value.trim().is_empty() {
        return Ok(());
    }
    match kind {
        "project" => Err(DesktopArchitectureError::EmptyProjectId),
        _ => Err(DesktopArchitectureError::EmptyTaskId),
    }
}

fn validate_expected_version(
    expected: Option<i64>,
    current: i64,
) -> Result<(), DesktopArchitectureError> {
    if let Some(expected) = expected {
        if expected != current {
            return Err(DesktopArchitectureError::VersionConflict { expected, current });
        }
    }
    Ok(())
}

fn normalized_request_id(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn normalize_changes(
    changes: Vec<ProjectArchitectureChange>,
) -> Result<Vec<ProjectArchitectureChange>, DesktopArchitectureError> {
    changes.into_iter().map(normalize_change).collect()
}

fn normalize_change(
    mut change: ProjectArchitectureChange,
) -> Result<ProjectArchitectureChange, DesktopArchitectureError> {
    match &mut change {
        ProjectArchitectureChange::UpsertNode { node } => normalize_node(node)?,
        ProjectArchitectureChange::RemoveNode { node_id } => {
            *node_id = node_id.trim().to_owned();
            if node_id.is_empty() {
                return Err(DesktopArchitectureError::EmptyNodeId);
            }
        }
        ProjectArchitectureChange::UpsertEdge { edge } => normalize_edge(edge)?,
        ProjectArchitectureChange::RemoveEdge { edge_id } => {
            *edge_id = edge_id.trim().to_owned();
            if edge_id.is_empty() {
                return Err(DesktopArchitectureError::InvalidEdge);
            }
        }
        ProjectArchitectureChange::SetSummary { summary } => {
            *summary = summary.trim().to_owned();
        }
    }
    Ok(change)
}

fn normalize_node(node: &mut ProjectArchitectureNode) -> Result<(), DesktopArchitectureError> {
    node.id = node.id.trim().to_owned();
    if node.id.is_empty() {
        return Err(DesktopArchitectureError::EmptyNodeId);
    }
    node.label = normalized_or(&node.label, &node.id);
    node.node_type = normalized_or(&node.node_type, "module");
    node.summary = node.summary.trim().to_owned();
    node.paths = deduplicated(std::mem::take(&mut node.paths));
    node.tags = deduplicated(std::mem::take(&mut node.tags));
    Ok(())
}

fn normalize_edge(edge: &mut ProjectArchitectureEdge) -> Result<(), DesktopArchitectureError> {
    edge.id = edge.id.trim().to_owned();
    edge.from = edge.from.trim().to_owned();
    edge.to = edge.to.trim().to_owned();
    if edge.id.is_empty() || edge.from.is_empty() || edge.to.is_empty() {
        return Err(DesktopArchitectureError::InvalidEdge);
    }
    edge.edge_type = normalized_or(&edge.edge_type, "depends_on");
    edge.label = edge.label.trim().to_owned();
    edge.summary = edge.summary.trim().to_owned();
    Ok(())
}

fn normalized_or(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}

fn deduplicated(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .collect()
}

fn apply_changes(
    graph: &ProjectArchitectureGraph,
    changes: &[ProjectArchitectureChange],
    now: i64,
) -> Result<ProjectArchitectureGraph, DesktopArchitectureError> {
    let mut next = graph.clone();
    for change in changes {
        match change {
            ProjectArchitectureChange::UpsertNode { node } => {
                if let Some(existing) = next.nodes.iter_mut().find(|item| item.id == node.id) {
                    *existing = node.clone();
                } else {
                    next.nodes.push(node.clone());
                }
            }
            ProjectArchitectureChange::RemoveNode { node_id } => {
                next.nodes.retain(|node| node.id != *node_id);
                next.edges
                    .retain(|edge| edge.from != *node_id && edge.to != *node_id);
            }
            ProjectArchitectureChange::UpsertEdge { edge } => {
                for node_id in [&edge.from, &edge.to] {
                    if !next.nodes.iter().any(|node| &node.id == node_id) {
                        return Err(DesktopArchitectureError::MissingEdgeNode {
                            edge_id: edge.id.clone(),
                            node_id: node_id.clone(),
                        });
                    }
                }
                if let Some(existing) = next.edges.iter_mut().find(|item| item.id == edge.id) {
                    *existing = edge.clone();
                } else {
                    next.edges.push(edge.clone());
                }
            }
            ProjectArchitectureChange::RemoveEdge { edge_id } => {
                next.edges.retain(|edge| edge.id != *edge_id);
            }
            ProjectArchitectureChange::SetSummary { summary } => next.summary = summary.clone(),
        }
    }
    next.version = graph.version.saturating_add(1);
    next.updated_at = now;
    Ok(next)
}

#[derive(Serialize)]
struct RawGraphRow {
    project_id: String,
    version: i64,
    graph_json: String,
    updated_at: i64,
}

fn load_graph_recovering(
    transaction: &Transaction<'_>,
    project_id: &str,
) -> Result<ProjectArchitectureGraph, DesktopArchitectureError> {
    let row = transaction
        .query_row(
            r#"SELECT project_id, version, graph_json, updated_at
               FROM project_architecture_graphs WHERE project_id = ?1"#,
            params![project_id],
            |row| {
                Ok(RawGraphRow {
                    project_id: row.get(0)?,
                    version: row.get(1)?,
                    graph_json: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| DesktopArchitectureError::storage("read graph", error))?;
    let Some(row) = row else {
        return Ok(ProjectArchitectureGraph::empty(project_id));
    };
    let decoded = serde_json::from_str::<ProjectArchitectureGraph>(&row.graph_json)
        .map_err(|_| "invalid_graph_json")
        .and_then(|graph| {
            validate_graph(&graph, project_id, Some((row.version, row.updated_at)))?;
            Ok(graph)
        });
    match decoded {
        Ok(graph) => Ok(graph),
        Err(reason_code) => {
            quarantine_payload(
                transaction,
                project_id,
                "graph",
                project_id,
                reason_code,
                &row,
            )?;
            let recovered = recover_graph_from_history(transaction, project_id)?
                .unwrap_or_else(|| ProjectArchitectureGraph::empty(project_id));
            write_graph(transaction, &recovered)?;
            Ok(recovered)
        }
    }
}

fn recover_graph_from_history(
    transaction: &Transaction<'_>,
    project_id: &str,
) -> Result<Option<ProjectArchitectureGraph>, DesktopArchitectureError> {
    let raw_records = {
        let mut statement = transaction
            .prepare(
                r#"SELECT id, project_id, task_id, turn_id, backend, status, permission_mode,
                          summary, changes_json, before_graph_json, after_graph_json,
                          created_at, resolved_at
                   FROM project_architecture_changes
                   WHERE project_id = ?1 AND after_graph_json IS NOT NULL
                   ORDER BY created_at DESC, rowid DESC"#,
            )
            .map_err(|error| {
                DesktopArchitectureError::storage("prepare graph recovery history", error)
            })?;
        let rows = statement
            .query_map(params![project_id], raw_record)
            .map_err(|error| {
                DesktopArchitectureError::storage("query graph recovery history", error)
            })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
            DesktopArchitectureError::storage("read graph recovery history", error)
        })?
    };
    for raw in raw_records {
        if let Some(record) = decode_record_or_quarantine(transaction, raw)? {
            if let Some(graph) = record.after_graph {
                return Ok(Some(graph));
            }
        }
    }
    Ok(None)
}

fn validate_graph(
    graph: &ProjectArchitectureGraph,
    project_id: &str,
    stored_metadata: Option<(i64, i64)>,
) -> Result<(), &'static str> {
    if graph.project_id != project_id {
        return Err("graph_project_mismatch");
    }
    if graph.version < 0 || graph.updated_at < 0 {
        return Err("invalid_graph_metadata");
    }
    if let Some((version, updated_at)) = stored_metadata {
        if graph.version != version || graph.updated_at != updated_at {
            return Err("graph_metadata_mismatch");
        }
    }
    let mut node_ids = BTreeSet::new();
    for node in &graph.nodes {
        if node.id.is_empty() || node.id.trim() != node.id || !node_ids.insert(&node.id) {
            return Err("invalid_graph_nodes");
        }
    }
    let mut edge_ids = BTreeSet::new();
    for edge in &graph.edges {
        if edge.id.is_empty()
            || edge.id.trim() != edge.id
            || edge.from.is_empty()
            || edge.to.is_empty()
            || !edge_ids.insert(&edge.id)
            || !node_ids.contains(&edge.from)
            || !node_ids.contains(&edge.to)
        {
            return Err("invalid_graph_edges");
        }
    }
    Ok(())
}

fn write_graph(
    transaction: &Transaction<'_>,
    graph: &ProjectArchitectureGraph,
) -> Result<(), DesktopArchitectureError> {
    let graph_json = serde_json::to_string(graph)
        .map_err(|error| DesktopArchitectureError::storage("encode graph", error))?;
    transaction
        .execute(
            r#"INSERT INTO project_architecture_graphs
               (project_id, version, graph_json, updated_at)
               VALUES (?1, ?2, ?3, ?4)
               ON CONFLICT(project_id) DO UPDATE SET
                 version = excluded.version,
                 graph_json = excluded.graph_json,
                 updated_at = excluded.updated_at"#,
            params![
                graph.project_id,
                graph.version,
                graph_json,
                graph.updated_at
            ],
        )
        .map(|_| ())
        .map_err(|error| DesktopArchitectureError::storage("write graph", error))
}

#[allow(clippy::too_many_arguments)]
fn insert_history(
    transaction: &Transaction<'_>,
    id: &str,
    project_id: &str,
    task_id: &str,
    turn_id: Option<&str>,
    backend: ArchitectureBackend,
    status: ArchitectureChangeStatus,
    permission: ArchitecturePermission,
    reason: &str,
    changes: &[ProjectArchitectureChange],
    before_graph: Option<&ProjectArchitectureGraph>,
    after_graph: Option<&ProjectArchitectureGraph>,
    created_at: i64,
    resolved_at: Option<i64>,
) -> Result<ProjectArchitectureChangeEvent, DesktopArchitectureError> {
    let changes_json = serde_json::to_string(changes)
        .map_err(|error| DesktopArchitectureError::storage("encode changes", error))?;
    let before_graph_json = before_graph
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| DesktopArchitectureError::storage("encode before graph", error))?;
    let after_graph_json = after_graph
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| DesktopArchitectureError::storage("encode after graph", error))?;
    transaction
        .execute(
            r#"INSERT INTO project_architecture_changes
               (id, project_id, task_id, turn_id, backend, status, permission_mode,
                summary, changes_json, before_graph_json, after_graph_json, created_at, resolved_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"#,
            params![
                id,
                project_id,
                task_id,
                turn_id,
                backend.as_str(),
                status.as_str(),
                permission.as_str(),
                reason,
                changes_json,
                before_graph_json,
                after_graph_json,
                created_at,
                resolved_at,
            ],
        )
        .map_err(|error| DesktopArchitectureError::storage("insert history", error))?;
    Ok(ProjectArchitectureChangeEvent {
        id: Some(id.to_owned()),
        project_id: project_id.to_owned(),
        task_id: task_id.to_owned(),
        turn_id: turn_id.map(str::to_owned),
        backend,
        permission,
        status,
        reason: reason.to_owned(),
        changes: changes.to_vec(),
        before_version: before_graph.map(|graph| graph.version).unwrap_or_default(),
        after_version: after_graph.map(|graph| graph.version),
        created_at: Some(created_at),
        resolved_at,
    })
}

#[derive(Clone, Serialize)]
struct RawRecord {
    id: String,
    project_id: String,
    task_id: String,
    turn_id: Option<String>,
    backend: String,
    status: String,
    permission: String,
    reason: String,
    changes_json: String,
    before_graph_json: Option<String>,
    after_graph_json: Option<String>,
    created_at: i64,
    resolved_at: Option<i64>,
}

fn raw_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRecord> {
    Ok(RawRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        task_id: row.get(2)?,
        turn_id: row.get(3)?,
        backend: row.get(4)?,
        status: row.get(5)?,
        permission: row.get(6)?,
        reason: row.get(7)?,
        changes_json: row.get(8)?,
        before_graph_json: row.get(9)?,
        after_graph_json: row.get(10)?,
        created_at: row.get(11)?,
        resolved_at: row.get(12)?,
    })
}

#[derive(Debug)]
struct StoredRecordIssue {
    reason_code: &'static str,
}

impl StoredRecordIssue {
    const fn new(reason_code: &'static str) -> Self {
        Self { reason_code }
    }
}

fn decode_record(raw: RawRecord) -> Result<ProjectArchitectureChangeRecord, StoredRecordIssue> {
    let backend = match raw.backend.as_str() {
        "native-agentkit" => ArchitectureBackend::NativeAgentkit,
        _ => return Err(StoredRecordIssue::new("invalid_backend")),
    };
    let permission = match raw.permission.as_str() {
        "ask" => ArchitecturePermission::Ask,
        "full" => ArchitecturePermission::Full,
        "readonly" => ArchitecturePermission::Readonly,
        _ => return Err(StoredRecordIssue::new("invalid_permission")),
    };
    let status = ArchitectureChangeStatus::from_storage(&raw.status)
        .ok_or_else(|| StoredRecordIssue::new("invalid_status"))?;
    let changes = serde_json::from_str(&raw.changes_json)
        .map_err(|_| StoredRecordIssue::new("invalid_changes_json"))?;
    let before_graph = raw
        .before_graph_json
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|_| StoredRecordIssue::new("invalid_before_graph_json"))?;
    let after_graph = raw
        .after_graph_json
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|_| StoredRecordIssue::new("invalid_after_graph_json"))?;
    if let Some(graph) = before_graph.as_ref() {
        validate_graph(graph, &raw.project_id, None)
            .map_err(|_| StoredRecordIssue::new("invalid_before_graph_shape"))?;
    }
    if let Some(graph) = after_graph.as_ref() {
        validate_graph(graph, &raw.project_id, None)
            .map_err(|_| StoredRecordIssue::new("invalid_after_graph_shape"))?;
    }
    Ok(ProjectArchitectureChangeRecord {
        event: ProjectArchitectureChangeEvent {
            id: Some(raw.id),
            project_id: raw.project_id,
            task_id: raw.task_id,
            turn_id: raw.turn_id,
            backend,
            permission,
            status,
            reason: raw.reason,
            changes,
            before_version: before_graph
                .as_ref()
                .map(|graph: &ProjectArchitectureGraph| graph.version)
                .unwrap_or_default(),
            after_version: after_graph
                .as_ref()
                .map(|graph: &ProjectArchitectureGraph| graph.version),
            created_at: Some(raw.created_at),
            resolved_at: raw.resolved_at,
        },
        before_graph,
        after_graph,
    })
}

fn decode_record_or_quarantine(
    transaction: &Transaction<'_>,
    raw: RawRecord,
) -> Result<Option<ProjectArchitectureChangeRecord>, DesktopArchitectureError> {
    let project_id = raw.project_id.clone();
    let record_id = raw.id.clone();
    let payload_json = serde_json::to_string(&raw)
        .map_err(|error| DesktopArchitectureError::storage("encode corrupt history", error))?;
    match decode_record(raw) {
        Ok(record) => Ok(Some(record)),
        Err(issue) => {
            quarantine_json(
                transaction,
                &project_id,
                "history",
                &record_id,
                issue.reason_code,
                &payload_json,
            )?;
            transaction
                .execute(
                    "DELETE FROM project_architecture_changes WHERE id = ?1",
                    params![record_id],
                )
                .map_err(|error| {
                    DesktopArchitectureError::storage("remove corrupt history", error)
                })?;
            Ok(None)
        }
    }
}

fn quarantine_payload(
    transaction: &Transaction<'_>,
    project_id: &str,
    record_kind: &str,
    record_id: &str,
    reason_code: &str,
    payload: &impl Serialize,
) -> Result<(), DesktopArchitectureError> {
    let payload_json = serde_json::to_string(payload)
        .map_err(|error| DesktopArchitectureError::storage("encode quarantine", error))?;
    quarantine_json(
        transaction,
        project_id,
        record_kind,
        record_id,
        reason_code,
        &payload_json,
    )
}

fn quarantine_json(
    transaction: &Transaction<'_>,
    project_id: &str,
    record_kind: &str,
    record_id: &str,
    reason_code: &str,
    payload_json: &str,
) -> Result<(), DesktopArchitectureError> {
    transaction
        .execute(
            r#"INSERT OR IGNORE INTO project_architecture_quarantine
               (id, project_id, record_kind, record_id, reason_code, payload_json, quarantined_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            params![
                Uuid::new_v4().to_string(),
                project_id,
                record_kind,
                record_id,
                reason_code,
                payload_json,
                now_millis(),
            ],
        )
        .map(|_| ())
        .map_err(|error| DesktopArchitectureError::storage("quarantine record", error))
}

fn record_by_id(
    transaction: &Transaction<'_>,
    request_id: &str,
) -> Result<Option<ProjectArchitectureChangeRecord>, DesktopArchitectureError> {
    let raw = transaction
        .query_row(
            r#"SELECT id, project_id, task_id, turn_id, backend, status, permission_mode,
                      summary, changes_json, before_graph_json, after_graph_json,
                      created_at, resolved_at
               FROM project_architecture_changes WHERE id = ?1"#,
            params![request_id],
            raw_record,
        )
        .optional()
        .map_err(|error| DesktopArchitectureError::storage("read idempotency record", error))?;
    raw.map(|raw| decode_record_or_quarantine(transaction, raw))
        .transpose()
        .map(Option::flatten)
}

fn latest_applied_record(
    transaction: &Transaction<'_>,
    project_id: &str,
) -> Result<Option<ProjectArchitectureChangeRecord>, DesktopArchitectureError> {
    let raw_records = {
        let mut statement = transaction
            .prepare(
                r#"SELECT id, project_id, task_id, turn_id, backend, status, permission_mode,
                      summary, changes_json, before_graph_json, after_graph_json,
                      created_at, resolved_at
               FROM project_architecture_changes
               WHERE project_id = ?1 AND status = 'applied' AND before_graph_json IS NOT NULL
               ORDER BY created_at DESC, rowid DESC"#,
            )
            .map_err(|error| {
                DesktopArchitectureError::storage("prepare rollback history", error)
            })?;
        let rows = statement
            .query_map(params![project_id], raw_record)
            .map_err(|error| DesktopArchitectureError::storage("query rollback history", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| DesktopArchitectureError::storage("read rollback history", error))?
    };
    for raw in raw_records {
        if let Some(record) = decode_record_or_quarantine(transaction, raw)? {
            return Ok(Some(record));
        }
    }
    Ok(None)
}

fn ensure_idempotent_apply(
    record: &ProjectArchitectureChangeRecord,
    input: &ProjectArchitectureApplyInput,
) -> Result<(), DesktopArchitectureError> {
    let event = &record.event;
    if event.project_id == input.project_id
        && event.task_id == input.task_id
        && event.turn_id == input.turn_id
        && event.backend == input.backend
        && event.permission == input.permission
        && event.status == ArchitectureChangeStatus::Applied
        && event.reason == input.reason
        && event.changes == input.changes
    {
        Ok(())
    } else {
        Err(DesktopArchitectureError::IdempotencyConflict {
            request_id: event.id.clone().unwrap_or_default(),
        })
    }
}

fn ensure_idempotent_reject(
    record: &ProjectArchitectureChangeRecord,
    input: &ProjectArchitectureRejectInput,
) -> Result<(), DesktopArchitectureError> {
    let event = &record.event;
    if event.project_id == input.project_id
        && event.task_id == input.task_id
        && event.turn_id == input.turn_id
        && event.backend == input.backend
        && event.permission == input.permission
        && event.status == ArchitectureChangeStatus::Rejected
        && event.reason == input.reason
        && event.changes == input.changes
    {
        Ok(())
    } else {
        Err(DesktopArchitectureError::IdempotencyConflict {
            request_id: event.id.clone().unwrap_or_default(),
        })
    }
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
    use super::*;

    fn node(id: &str) -> ProjectArchitectureNode {
        ProjectArchitectureNode {
            id: id.to_owned(),
            label: id.to_owned(),
            node_type: "module".to_owned(),
            summary: String::new(),
            paths: Vec::new(),
            tags: Vec::new(),
        }
    }

    fn input(request_id: &str, expected_version: Option<i64>) -> ProjectArchitectureApplyInput {
        ProjectArchitectureApplyInput {
            project_id: "p1".to_owned(),
            task_id: "t1".to_owned(),
            turn_id: Some("turn-1".to_owned()),
            backend: ArchitectureBackend::NativeAgentkit,
            permission: ArchitecturePermission::Ask,
            reason: "测试".to_owned(),
            changes: vec![ProjectArchitectureChange::UpsertNode { node: node("ui") }],
            request_id: Some(request_id.to_owned()),
            expected_version,
        }
    }

    #[test]
    fn apply_replays_idempotently_and_conflicts_on_stale_version() {
        let mut store = SqliteArchitectureStore::in_memory().unwrap();
        let first = store.apply(input("request-1", Some(0))).unwrap();
        let replay = store.apply(input("request-1", Some(0))).unwrap();
        assert_eq!(first, replay);
        assert!(matches!(
            store.apply(input("request-2", Some(0))),
            Err(DesktopArchitectureError::VersionConflict {
                expected: 0,
                current: 1
            })
        ));
    }

    #[test]
    fn invalid_edge_is_atomic_and_rollback_creates_a_new_version() {
        let mut store = SqliteArchitectureStore::in_memory().unwrap();
        store.apply(input("request-1", Some(0))).unwrap();
        let invalid = ProjectArchitectureApplyInput {
            changes: vec![ProjectArchitectureChange::UpsertEdge {
                edge: ProjectArchitectureEdge {
                    id: "ui-store".to_owned(),
                    from: "ui".to_owned(),
                    to: "store".to_owned(),
                    edge_type: "depends_on".to_owned(),
                    label: String::new(),
                    summary: String::new(),
                },
            }],
            request_id: Some("request-2".to_owned()),
            expected_version: Some(1),
            ..input("ignored", None)
        };
        assert!(matches!(
            store.apply(invalid),
            Err(DesktopArchitectureError::MissingEdgeNode { .. })
        ));
        assert_eq!(store.graph("p1").unwrap().version, 1);
        let rolled_back = store
            .rollback("p1", "t1", ArchitectureBackend::NativeAgentkit)
            .unwrap();
        assert_eq!(rolled_back.graph.version, 2);
        assert!(rolled_back.graph.nodes.is_empty());
        assert_eq!(store.list_changes("p1", 20).unwrap().len(), 2);
        assert!(store
            .rollback("p1", "t1", ArchitectureBackend::NativeAgentkit)
            .unwrap()
            .event
            .is_none());
    }

    #[test]
    fn request_id_reuse_with_different_changes_is_rejected() {
        let mut store = SqliteArchitectureStore::in_memory().unwrap();
        store.apply(input("request-1", None)).unwrap();
        let mut conflict = input("request-1", None);
        conflict.reason = "different".to_owned();
        assert!(matches!(
            store.apply(conflict),
            Err(DesktopArchitectureError::IdempotencyConflict { .. })
        ));
    }

    #[test]
    fn corrupt_active_graph_recovers_latest_valid_history_and_accepts_next_change() {
        let mut store = SqliteArchitectureStore::in_memory().unwrap();
        store.apply(input("request-1", Some(0))).unwrap();
        store
            .connection
            .lock()
            .execute(
                r#"UPDATE project_architecture_graphs
                   SET version = 99, graph_json = '{broken', updated_at = 99
                   WHERE project_id = 'p1'"#,
                [],
            )
            .unwrap();

        let recovered = store.graph("p1").unwrap();
        assert_eq!(recovered.version, 1);
        assert_eq!(recovered.nodes, vec![node("ui")]);
        let quarantine = store.list_quarantine("p1").unwrap();
        assert_eq!(quarantine.len(), 1);
        assert_eq!(quarantine[0].record_kind, "graph");
        assert_eq!(quarantine[0].reason_code, "invalid_graph_json");

        let mut next = input("request-2", Some(1));
        next.changes = vec![ProjectArchitectureChange::UpsertNode {
            node: node("runtime"),
        }];
        let applied = store.apply(next).unwrap();
        assert_eq!(applied.graph.version, 2);
        assert_eq!(applied.graph.nodes.len(), 2);
    }

    #[test]
    fn corrupt_active_graph_without_history_recovers_empty_graph() {
        let mut store = SqliteArchitectureStore::in_memory().unwrap();
        store
            .connection
            .lock()
            .execute(
                r#"INSERT INTO project_architecture_graphs
                   (project_id, version, graph_json, updated_at)
                   VALUES ('p1', 4, '{broken', 4)"#,
                [],
            )
            .unwrap();

        assert_eq!(
            store.graph("p1").unwrap(),
            ProjectArchitectureGraph::empty("p1")
        );
        assert_eq!(store.list_quarantine("p1").unwrap().len(), 1);
        assert_eq!(
            store.graph("p1").unwrap(),
            ProjectArchitectureGraph::empty("p1")
        );
        assert_eq!(store.list_quarantine("p1").unwrap().len(), 1);
    }

    #[test]
    fn corrupt_history_is_quarantined_and_does_not_block_rollback() {
        let mut store = SqliteArchitectureStore::in_memory().unwrap();
        let applied = store.apply(input("request-1", Some(0))).unwrap();
        let after_graph_json = serde_json::to_string(&applied.graph).unwrap();
        store
            .connection
            .lock()
            .execute(
                r#"INSERT INTO project_architecture_changes
                   (id, project_id, task_id, turn_id, backend, status, permission_mode,
                    summary, changes_json, before_graph_json, after_graph_json,
                    created_at, resolved_at)
                   VALUES ('corrupt-history', 'p1', 't1', 'turn-x', 'native-agentkit',
                           'applied', 'ask', 'corrupt', '[]', '{broken', ?1, ?2, ?2)"#,
                params![after_graph_json, i64::MAX - 1],
            )
            .unwrap();

        let rolled_back = store
            .rollback("p1", "t1", ArchitectureBackend::NativeAgentkit)
            .unwrap();
        assert_eq!(rolled_back.graph.version, 2);
        assert!(rolled_back.graph.nodes.is_empty());
        assert!(rolled_back.event.is_some());
        let quarantine = store.list_quarantine("p1").unwrap();
        assert_eq!(quarantine.len(), 1);
        assert_eq!(quarantine[0].record_kind, "history");
        assert_eq!(quarantine[0].record_id, "corrupt-history");
        assert_eq!(quarantine[0].reason_code, "invalid_before_graph_json");
        let history = store.list_changes("p1", 20).unwrap();
        assert_eq!(history.len(), 2);
        assert!(history
            .iter()
            .all(|record| record.event.id.as_deref() != Some("corrupt-history")));
    }
}

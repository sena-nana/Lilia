use rusqlite::{params, types::Type, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::chat_backends_contract::chat_backends_contract;
use crate::project_architecture_contract::{
    project_architecture_change_statuses, project_architecture_permissions,
    project_architecture_rollback_permission,
};
use crate::store::LiliaStore;
use crate::util::now_millis;

fn contains_contract_value(values: &[String], value: &str) -> bool {
    values.iter().any(|item| item == value)
}

fn required_change_status(status: &str) -> &'static str {
    project_architecture_change_statuses()
        .iter()
        .find(|value| value.as_str() == status)
        .map(String::as_str)
        .unwrap_or_else(|| panic!("architecture-contract.json missing change status {status}"))
}

fn applied_change_status() -> &'static str {
    required_change_status("applied")
}

fn rejected_change_status() -> &'static str {
    required_change_status("rejected")
}

fn rolled_back_change_status() -> &'static str {
    required_change_status("rolled_back")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureNode {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub summary: String,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub edge_type: String,
    pub label: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureGraph {
    pub project_id: String,
    pub version: i64,
    pub summary: String,
    pub nodes: Vec<ProjectArchitectureNode>,
    pub edges: Vec<ProjectArchitectureEdge>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProjectArchitectureChange {
    UpsertNode { node: ProjectArchitectureNode },
    RemoveNode { node_id: String },
    UpsertEdge { edge: ProjectArchitectureEdge },
    RemoveEdge { edge_id: String },
    SetSummary { summary: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureChangeEvent {
    pub id: Option<String>,
    pub project_id: String,
    pub task_id: String,
    pub turn_id: Option<String>,
    pub backend: String,
    pub permission: String,
    pub status: String,
    pub reason: String,
    pub changes: Vec<ProjectArchitectureChange>,
    pub before_version: i64,
    pub after_version: Option<i64>,
    pub created_at: Option<i64>,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureApplyInput {
    pub project_id: String,
    pub task_id: String,
    pub turn_id: Option<String>,
    pub backend: String,
    pub permission: String,
    pub reason: String,
    #[serde(default)]
    pub changes: Vec<ProjectArchitectureChange>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureRejectInput {
    pub project_id: String,
    pub task_id: String,
    pub turn_id: Option<String>,
    pub backend: String,
    pub permission: String,
    pub reason: String,
    #[serde(default)]
    pub changes: Vec<ProjectArchitectureChange>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureApplyResult {
    pub graph: ProjectArchitectureGraph,
    pub event: ProjectArchitectureChangeEvent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureRollbackResult {
    pub graph: ProjectArchitectureGraph,
    pub event: Option<ProjectArchitectureChangeEvent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureChangeRecord {
    #[serde(flatten)]
    pub event: ProjectArchitectureChangeEvent,
    pub before_graph: Option<ProjectArchitectureGraph>,
    pub after_graph: Option<ProjectArchitectureGraph>,
}

fn empty_graph(project_id: &str) -> ProjectArchitectureGraph {
    ProjectArchitectureGraph {
        project_id: project_id.to_string(),
        version: 0,
        summary: String::new(),
        nodes: Vec::new(),
        edges: Vec::new(),
        updated_at: 0,
    }
}

fn validate_backend(value: &str) -> Result<(), String> {
    if contains_contract_value(&chat_backends_contract().chat_backends, value) {
        Ok(())
    } else {
        Err(format!("project_architecture: 无效 backend：{value}"))
    }
}

fn validate_permission(value: &str) -> Result<(), String> {
    if contains_contract_value(project_architecture_permissions(), value) {
        Ok(())
    } else {
        Err(format!("project_architecture: 无效 permission：{value}"))
    }
}

fn validate_status(value: &str) -> Result<(), String> {
    if contains_contract_value(project_architecture_change_statuses(), value) {
        Ok(())
    } else {
        Err(format!("project_architecture: 无效 status：{value}"))
    }
}

fn ensure_project_exists(conn: &Connection, project_id: &str) -> Result<(), String> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM projects WHERE id = ?1",
            params![project_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|e| format!("project_architecture: 查询项目失败：{e}"))?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err("project_architecture: 项目不存在".to_string())
    }
}

fn graph_from_json(text: &str) -> rusqlite::Result<ProjectArchitectureGraph> {
    serde_json::from_str(text)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(e)))
}

fn load_graph(conn: &Connection, project_id: &str) -> Result<ProjectArchitectureGraph, String> {
    conn.query_row(
        r#"SELECT graph_json FROM project_architecture_graphs WHERE project_id = ?1"#,
        params![project_id],
        |row| {
            let text: String = row.get(0)?;
            graph_from_json(&text)
        },
    )
    .optional()
    .map_err(|e| format!("project_architecture_get: 读取图失败：{e}"))
    .map(|graph| graph.unwrap_or_else(|| empty_graph(project_id)))
}

fn serialize_json<T: Serialize>(value: &T, label: &str) -> Result<String, String> {
    serde_json::to_string(value)
        .map_err(|e| format!("project_architecture: {label} 序列化失败：{e}"))
}

fn trim_text(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_node(node: &mut ProjectArchitectureNode) -> Result<(), String> {
    node.id = trim_text(&node.id, "");
    if node.id.is_empty() {
        return Err("project_architecture_apply: node.id 不能为空".to_string());
    }
    node.label = trim_text(&node.label, &node.id);
    node.node_type = trim_text(&node.node_type, "module");
    node.summary = node.summary.trim().to_string();
    node.paths = dedupe_trimmed(std::mem::take(&mut node.paths));
    node.tags = dedupe_trimmed(std::mem::take(&mut node.tags));
    Ok(())
}

fn normalize_edge(edge: &mut ProjectArchitectureEdge) -> Result<(), String> {
    edge.id = trim_text(&edge.id, "");
    edge.from = trim_text(&edge.from, "");
    edge.to = trim_text(&edge.to, "");
    if edge.id.is_empty() || edge.from.is_empty() || edge.to.is_empty() {
        return Err("project_architecture_apply: edge.id/from/to 不能为空".to_string());
    }
    edge.edge_type = trim_text(&edge.edge_type, "depends_on");
    edge.label = edge.label.trim().to_string();
    edge.summary = edge.summary.trim().to_string();
    Ok(())
}

fn dedupe_trimmed(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let text = item.trim();
        if text.is_empty() || !seen.insert(text.to_string()) {
            continue;
        }
        out.push(text.to_string());
    }
    out
}

fn normalize_change(
    mut change: ProjectArchitectureChange,
) -> Result<ProjectArchitectureChange, String> {
    match &mut change {
        ProjectArchitectureChange::UpsertNode { node } => normalize_node(node)?,
        ProjectArchitectureChange::RemoveNode { node_id } => {
            *node_id = trim_text(node_id, "");
            if node_id.is_empty() {
                return Err("project_architecture_apply: nodeId 不能为空".to_string());
            }
        }
        ProjectArchitectureChange::UpsertEdge { edge } => normalize_edge(edge)?,
        ProjectArchitectureChange::RemoveEdge { edge_id } => {
            *edge_id = trim_text(edge_id, "");
            if edge_id.is_empty() {
                return Err("project_architecture_apply: edgeId 不能为空".to_string());
            }
        }
        ProjectArchitectureChange::SetSummary { summary } => {
            *summary = summary.trim().to_string();
        }
    }
    Ok(change)
}

fn normalize_changes(
    changes: Vec<ProjectArchitectureChange>,
) -> Result<Vec<ProjectArchitectureChange>, String> {
    changes.into_iter().map(normalize_change).collect()
}

fn apply_changes(
    graph: &ProjectArchitectureGraph,
    changes: &[ProjectArchitectureChange],
    now: i64,
) -> Result<ProjectArchitectureGraph, String> {
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
                let has_from = next.nodes.iter().any(|node| node.id == edge.from);
                let has_to = next.nodes.iter().any(|node| node.id == edge.to);
                if !has_from || !has_to {
                    return Err(format!(
                        "project_architecture_apply: edge {} 引用了不存在的节点",
                        edge.id
                    ));
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
            ProjectArchitectureChange::SetSummary { summary } => {
                next.summary = summary.clone();
            }
        }
    }
    next.version = graph.version + 1;
    next.updated_at = now;
    Ok(next)
}

fn write_graph(conn: &Connection, graph: &ProjectArchitectureGraph) -> Result<(), String> {
    let graph_json = serialize_json(graph, "graph")?;
    conn.execute(
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
    .map_err(|e| format!("project_architecture: 写入图失败：{e}"))
}

fn insert_history(
    conn: &Connection,
    id: &str,
    project_id: &str,
    task_id: &str,
    turn_id: Option<&str>,
    backend: &str,
    status: &str,
    permission: &str,
    reason: &str,
    changes: &[ProjectArchitectureChange],
    before_graph: Option<&ProjectArchitectureGraph>,
    after_graph: Option<&ProjectArchitectureGraph>,
    created_at: i64,
    resolved_at: Option<i64>,
) -> Result<ProjectArchitectureChangeEvent, String> {
    validate_status(status)?;
    let changes_json = serialize_json(&changes, "changes")?;
    let before_graph_json = before_graph
        .map(|graph| serialize_json(graph, "before_graph"))
        .transpose()?;
    let after_graph_json = after_graph
        .map(|graph| serialize_json(graph, "after_graph"))
        .transpose()?;
    let before_version = before_graph.map(|graph| graph.version).unwrap_or(0);
    let after_version = after_graph.map(|graph| graph.version);
    conn.execute(
        r#"INSERT INTO project_architecture_changes
           (id, project_id, task_id, turn_id, backend, status, permission_mode,
            summary, changes_json, before_graph_json, after_graph_json, created_at, resolved_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"#,
        params![
            id,
            project_id,
            task_id,
            turn_id,
            backend,
            status,
            permission,
            reason,
            changes_json,
            before_graph_json,
            after_graph_json,
            created_at,
            resolved_at,
        ],
    )
    .map_err(|e| format!("project_architecture: 写入变更历史失败：{e}"))?;

    Ok(ProjectArchitectureChangeEvent {
        id: Some(id.to_string()),
        project_id: project_id.to_string(),
        task_id: task_id.to_string(),
        turn_id: turn_id.map(ToOwned::to_owned),
        backend: backend.to_string(),
        permission: permission.to_string(),
        status: status.to_string(),
        reason: reason.to_string(),
        changes: changes.to_vec(),
        before_version,
        after_version,
        created_at: Some(created_at),
        resolved_at,
    })
}

pub(crate) fn apply_project_architecture_changes_core(
    conn: &mut Connection,
    input: ProjectArchitectureApplyInput,
) -> Result<ProjectArchitectureApplyResult, String> {
    validate_backend(&input.backend)?;
    validate_permission(&input.permission)?;
    ensure_project_exists(conn, &input.project_id)?;
    let changes = normalize_changes(input.changes)?;
    if changes.is_empty() {
        return Err("project_architecture_apply: changes 不能为空".to_string());
    }
    let reason = input.reason.trim().to_string();
    let now = now_millis();
    let id = input
        .request_id
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let tx = conn
        .transaction()
        .map_err(|e| format!("project_architecture_apply: 开启事务失败：{e}"))?;
    let before = load_graph(&tx, &input.project_id)?;
    let after = apply_changes(&before, &changes, now)?;
    write_graph(&tx, &after)?;
    let event = insert_history(
        &tx,
        &id,
        &input.project_id,
        &input.task_id,
        input.turn_id.as_deref(),
        &input.backend,
        applied_change_status(),
        &input.permission,
        &reason,
        &changes,
        Some(&before),
        Some(&after),
        now,
        Some(now),
    )?;
    tx.commit()
        .map_err(|e| format!("project_architecture_apply: 提交事务失败：{e}"))?;
    Ok(ProjectArchitectureApplyResult {
        graph: after,
        event,
    })
}

pub(crate) fn reject_project_architecture_changes_core(
    conn: &mut Connection,
    input: ProjectArchitectureRejectInput,
) -> Result<ProjectArchitectureChangeEvent, String> {
    validate_backend(&input.backend)?;
    validate_permission(&input.permission)?;
    ensure_project_exists(conn, &input.project_id)?;
    let changes = normalize_changes(input.changes)?;
    let reason = input.reason.trim().to_string();
    let now = now_millis();
    let id = input
        .request_id
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let tx = conn
        .transaction()
        .map_err(|e| format!("project_architecture_reject: 开启事务失败：{e}"))?;
    let before = load_graph(&tx, &input.project_id)?;
    let event = insert_history(
        &tx,
        &id,
        &input.project_id,
        &input.task_id,
        input.turn_id.as_deref(),
        &input.backend,
        rejected_change_status(),
        &input.permission,
        &reason,
        &changes,
        Some(&before),
        None,
        now,
        Some(now),
    )?;
    tx.commit()
        .map_err(|e| format!("project_architecture_reject: 提交事务失败：{e}"))?;
    Ok(event)
}

fn row_to_change_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ProjectArchitectureChangeRecord> {
    let id: String = row.get(0)?;
    let project_id: String = row.get(1)?;
    let task_id: String = row.get(2)?;
    let turn_id: Option<String> = row.get(3)?;
    let backend: String = row.get(4)?;
    let status: String = row.get(5)?;
    let permission: String = row.get(6)?;
    let reason: String = row.get(7)?;
    let changes_json: String = row.get(8)?;
    let before_graph_json: Option<String> = row.get(9)?;
    let after_graph_json: Option<String> = row.get(10)?;
    let created_at: i64 = row.get(11)?;
    let resolved_at: Option<i64> = row.get(12)?;
    let changes: Vec<ProjectArchitectureChange> = serde_json::from_str(&changes_json)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(8, Type::Text, Box::new(e)))?;
    let before_graph = before_graph_json
        .as_deref()
        .map(graph_from_json)
        .transpose()?;
    let after_graph = after_graph_json
        .as_deref()
        .map(graph_from_json)
        .transpose()?;
    let before_version = before_graph
        .as_ref()
        .map(|graph| graph.version)
        .unwrap_or(0);
    let after_version = after_graph.as_ref().map(|graph| graph.version);
    Ok(ProjectArchitectureChangeRecord {
        event: ProjectArchitectureChangeEvent {
            id: Some(id),
            project_id,
            task_id,
            turn_id,
            backend,
            permission,
            status,
            reason,
            changes,
            before_version,
            after_version,
            created_at: Some(created_at),
            resolved_at,
        },
        before_graph,
        after_graph,
    })
}

#[tauri::command]
pub fn project_architecture_get(
    project_id: String,
    store: State<'_, LiliaStore>,
) -> Result<ProjectArchitectureGraph, String> {
    let conn = store.conn()?;
    ensure_project_exists(&conn, &project_id)?;
    load_graph(&conn, &project_id)
}

#[tauri::command]
pub fn project_architecture_list_changes(
    project_id: String,
    limit: Option<i64>,
    store: State<'_, LiliaStore>,
) -> Result<Vec<ProjectArchitectureChangeRecord>, String> {
    let conn = store.conn()?;
    ensure_project_exists(&conn, &project_id)?;
    let limit = limit.unwrap_or(20).clamp(1, 200);
    let mut stmt = conn
        .prepare(
            r#"SELECT id, project_id, task_id, turn_id, backend, status, permission_mode,
                      summary, changes_json, before_graph_json, after_graph_json,
                      created_at, resolved_at
               FROM project_architecture_changes
               WHERE project_id = ?1
               ORDER BY created_at DESC, rowid DESC
               LIMIT ?2"#,
        )
        .map_err(|e| format!("project_architecture_list_changes: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map(params![project_id.as_str(), limit], row_to_change_record)
        .map_err(|e| format!("project_architecture_list_changes: query 失败：{e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("project_architecture_list_changes: 行解析失败：{e}"))?);
    }
    Ok(out)
}

#[tauri::command]
pub fn project_architecture_apply(
    input: ProjectArchitectureApplyInput,
    store: State<'_, LiliaStore>,
) -> Result<ProjectArchitectureApplyResult, String> {
    let mut conn = store.conn()?;
    apply_project_architecture_changes_core(&mut conn, input)
}

#[tauri::command]
pub fn project_architecture_reject(
    input: ProjectArchitectureRejectInput,
    store: State<'_, LiliaStore>,
) -> Result<ProjectArchitectureChangeEvent, String> {
    let mut conn = store.conn()?;
    reject_project_architecture_changes_core(&mut conn, input)
}

#[tauri::command]
pub fn project_architecture_rollback(
    project_id: String,
    task_id: String,
    backend: String,
    store: State<'_, LiliaStore>,
) -> Result<ProjectArchitectureRollbackResult, String> {
    let mut conn = store.conn()?;
    rollback_project_architecture_core(&mut conn, project_id, task_id, backend)
}

pub(crate) fn rollback_project_architecture_core(
    conn: &mut Connection,
    project_id: String,
    task_id: String,
    backend: String,
) -> Result<ProjectArchitectureRollbackResult, String> {
    validate_backend(&backend)?;
    ensure_project_exists(&*conn, &project_id)?;
    let now = now_millis();
    let tx = conn
        .transaction()
        .map_err(|e| format!("project_architecture_rollback: 开启事务失败：{e}"))?;
    let current = load_graph(&tx, &project_id)?;
    let latest: Option<ProjectArchitectureChangeRecord> = tx
        .query_row(
            r#"SELECT id, project_id, task_id, turn_id, backend, status, permission_mode,
                      summary, changes_json, before_graph_json, after_graph_json,
                      created_at, resolved_at
               FROM project_architecture_changes
               WHERE project_id = ?1 AND status = ?2 AND before_graph_json IS NOT NULL
               ORDER BY created_at DESC, rowid DESC
               LIMIT 1"#,
            params![project_id.as_str(), applied_change_status()],
            row_to_change_record,
        )
        .optional()
        .map_err(|e| format!("project_architecture_rollback: 查询历史失败：{e}"))?;
    let Some(latest) = latest else {
        return Ok(ProjectArchitectureRollbackResult {
            graph: current,
            event: None,
        });
    };
    let Some(mut restored) = latest.before_graph else {
        return Ok(ProjectArchitectureRollbackResult {
            graph: current,
            event: None,
        });
    };
    restored.version = current.version + 1;
    restored.updated_at = now;
    write_graph(&tx, &restored)?;
    let event = insert_history(
        &tx,
        &Uuid::new_v4().to_string(),
        &project_id,
        &task_id,
        None,
        &backend,
        rolled_back_change_status(),
        project_architecture_rollback_permission(),
        "回滚到上一版本",
        &[],
        Some(&current),
        Some(&restored),
        now,
        Some(now),
    )?;
    tx.commit()
        .map_err(|e| format!("project_architecture_rollback: 提交事务失败：{e}"))?;
    Ok(ProjectArchitectureRollbackResult {
        graph: restored,
        event: Some(event),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_schema(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE projects (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              cwd TEXT,
              created_at INTEGER NOT NULL,
              sort_order INTEGER NOT NULL DEFAULT 0,
              pinned INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE project_architecture_graphs (
              project_id TEXT PRIMARY KEY,
              version INTEGER NOT NULL,
              graph_json TEXT NOT NULL,
              updated_at INTEGER NOT NULL
            );
            CREATE TABLE project_architecture_changes (
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
            INSERT INTO projects (id, name, created_at) VALUES ('p1', 'P1', 1);
            "#,
        )
        .unwrap();
    }

    fn node(id: &str) -> ProjectArchitectureNode {
        ProjectArchitectureNode {
            id: id.to_string(),
            label: id.to_string(),
            node_type: "module".to_string(),
            summary: String::new(),
            paths: Vec::new(),
            tags: Vec::new(),
        }
    }

    fn edge(id: &str, from: &str, to: &str) -> ProjectArchitectureEdge {
        ProjectArchitectureEdge {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            edge_type: "depends_on".to_string(),
            label: String::new(),
            summary: String::new(),
        }
    }

    fn input(changes: Vec<ProjectArchitectureChange>) -> ProjectArchitectureApplyInput {
        ProjectArchitectureApplyInput {
            project_id: "p1".to_string(),
            task_id: "t1".to_string(),
            turn_id: Some("turn-1".to_string()),
            backend: "native-agentkit".to_string(),
            permission: "ask".to_string(),
            reason: "测试".to_string(),
            changes,
            request_id: Some(Uuid::new_v4().to_string()),
        }
    }

    #[test]
    fn validators_accept_contract_values() {
        let backend_contract = chat_backends_contract();
        assert_eq!(
            backend_contract.chat_backends,
            vec!["native-agentkit".to_string()]
        );
        assert_eq!(
            project_architecture_permissions(),
            &["ask", "full", "readonly"]
        );
        assert_eq!(project_architecture_rollback_permission(), "full");
        assert_eq!(
            project_architecture_change_statuses(),
            &["proposed", "pending", "applied", "rejected", "rolled_back"]
        );
        assert_eq!(applied_change_status(), "applied");
        assert_eq!(rejected_change_status(), "rejected");
        assert_eq!(rolled_back_change_status(), "rolled_back");
        for backend in &backend_contract.chat_backends {
            validate_backend(backend).unwrap();
        }
        for permission in project_architecture_permissions() {
            validate_permission(permission).unwrap();
        }
        for status in project_architecture_change_statuses() {
            validate_status(status).unwrap();
        }
        assert!(validate_backend("openai").is_err());
        assert!(validate_permission("free").is_err());
        assert!(validate_status("queued").is_err());
    }

    #[test]
    fn apply_upsert_remove_and_summary_increments_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        create_schema(&conn);

        let result = apply_project_architecture_changes_core(
            &mut conn,
            input(vec![
                ProjectArchitectureChange::UpsertNode { node: node("ui") },
                ProjectArchitectureChange::UpsertNode {
                    node: node("store"),
                },
                ProjectArchitectureChange::UpsertEdge {
                    edge: edge("ui-store", "ui", "store"),
                },
                ProjectArchitectureChange::SetSummary {
                    summary: "前端依赖状态层".to_string(),
                },
            ]),
        )
        .unwrap();

        assert_eq!(result.graph.version, 1);
        assert_eq!(result.graph.nodes.len(), 2);
        assert_eq!(result.graph.edges.len(), 1);
        assert_eq!(result.graph.summary, "前端依赖状态层");

        let result = apply_project_architecture_changes_core(
            &mut conn,
            input(vec![ProjectArchitectureChange::RemoveNode {
                node_id: "store".to_string(),
            }]),
        )
        .unwrap();
        assert_eq!(result.graph.version, 2);
        assert_eq!(result.graph.nodes.len(), 1);
        assert!(result.graph.edges.is_empty());
    }

    #[test]
    fn history_contains_before_and_after_snapshots() {
        let mut conn = Connection::open_in_memory().unwrap();
        create_schema(&conn);
        apply_project_architecture_changes_core(
            &mut conn,
            input(vec![ProjectArchitectureChange::UpsertNode {
                node: node("api"),
            }]),
        )
        .unwrap();

        let records = {
            let mut stmt = conn
                .prepare(
                    "SELECT before_graph_json, after_graph_json FROM project_architecture_changes",
                )
                .unwrap();
            stmt.query_row([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
        };
        assert!(records.0.contains("\"version\":0"));
        assert!(records.1.contains("\"version\":1"));
    }

    #[test]
    fn rollback_restores_previous_graph_as_new_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        create_schema(&conn);
        apply_project_architecture_changes_core(
            &mut conn,
            input(vec![ProjectArchitectureChange::UpsertNode {
                node: node("api"),
            }]),
        )
        .unwrap();
        apply_project_architecture_changes_core(
            &mut conn,
            input(vec![ProjectArchitectureChange::UpsertNode {
                node: node("ui"),
            }]),
        )
        .unwrap();

        let now = now_millis();
        let tx = conn.transaction().unwrap();
        let current = load_graph(&tx, "p1").unwrap();
        let latest = tx
            .query_row(
                r#"SELECT id, project_id, task_id, turn_id, backend, status, permission_mode,
                          summary, changes_json, before_graph_json, after_graph_json,
                          created_at, resolved_at
                   FROM project_architecture_changes
                   WHERE project_id = 'p1' AND status = ?1
                   ORDER BY created_at DESC, rowid DESC
                   LIMIT 1"#,
                params![applied_change_status()],
                row_to_change_record,
            )
            .unwrap();
        let mut restored = latest.before_graph.unwrap();
        restored.version = current.version + 1;
        restored.updated_at = now;
        write_graph(&tx, &restored).unwrap();
        tx.commit().unwrap();

        let graph = load_graph(&conn, "p1").unwrap();
        assert_eq!(graph.version, 3);
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].id, "api");
    }

    #[test]
    fn rollback_history_uses_requested_backend_for_native_agentkit() {
        let backend = "native-agentkit";
        let mut conn = Connection::open_in_memory().unwrap();
        create_schema(&conn);
        apply_project_architecture_changes_core(
            &mut conn,
            input(vec![ProjectArchitectureChange::UpsertNode {
                node: node("api"),
            }]),
        )
        .unwrap();

        let result = rollback_project_architecture_core(
            &mut conn,
            "p1".to_string(),
            "t1".to_string(),
            backend.to_string(),
        )
        .unwrap();

        assert_eq!(result.event.as_ref().unwrap().backend, backend);
        let history_backend: String = conn
            .query_row(
                "SELECT backend FROM project_architecture_changes WHERE status = ?1",
                params![rolled_back_change_status()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(history_backend, backend);
    }
}

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{Map as JsonMap, Value as JsonValue};

use super::signals::manual_signal;
use crate::automation::types::{
    AutomationDraft, AutomationRun, AutomationRunNodeState, AutomationRunStatus,
    AutomationScopeFilter, AutomationWorkflow, AutomationWorkflowVersion,
};

pub(crate) fn json_text<T: Serialize>(value: &T, label: &str) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("automation: 序列化 {label} 失败：{e}"))
}

pub(crate) fn row_to_workflow(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationWorkflow> {
    let scope_json: String = row.get(3)?;
    let draft_json: String = row.get(4)?;
    let scope: AutomationScopeFilter = serde_json::from_str(&scope_json).unwrap_or_default();
    let draft = serde_json::from_str(&draft_json).unwrap_or(AutomationDraft {
        nodes: Vec::new(),
        edges: Vec::new(),
        scope: scope.clone(),
    });
    Ok(AutomationWorkflow {
        id: row.get(0)?,
        name: row.get(1)?,
        enabled: row.get::<_, i64>(2)? != 0,
        scope,
        draft,
        published_version_id: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn row_to_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationWorkflowVersion> {
    let snapshot_json: String = row.get(3)?;
    Ok(AutomationWorkflowVersion {
        id: row.get(0)?,
        workflow_id: row.get(1)?,
        version: row.get(2)?,
        snapshot: serde_json::from_str(&snapshot_json).unwrap_or_default(),
        created_at: row.get(4)?,
    })
}

fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationRun> {
    let trigger_json: String = row.get(4)?;
    let scope_json: String = row.get(5)?;
    let status: String = row.get(3)?;
    Ok(AutomationRun {
        id: row.get(0)?,
        workflow_id: row.get(1)?,
        workflow_version_id: row.get(2)?,
        status: AutomationRunStatus::from_contract(&status),
        trigger: serde_json::from_str(&trigger_json).unwrap_or_else(|_| manual_signal(None)),
        scope: serde_json::from_str(&scope_json).unwrap_or_default(),
        started_at: row.get(6)?,
        finished_at: row.get(7)?,
        error: row.get(8)?,
    })
}

fn row_to_node_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationRunNodeState> {
    let status: String = row.get(3)?;
    let input_json: String = row.get(4)?;
    let output_json: Option<String> = row.get(5)?;
    Ok(AutomationRunNodeState {
        id: row.get(0)?,
        run_id: row.get(1)?,
        node_id: row.get(2)?,
        status: AutomationRunStatus::from_contract(&status),
        input: serde_json::from_str(&input_json).unwrap_or(JsonValue::Object(JsonMap::new())),
        output: output_json.and_then(|text| serde_json::from_str(&text).ok()),
        error: row.get(6)?,
        started_at: row.get(7)?,
        finished_at: row.get(8)?,
    })
}

pub(crate) fn version_by_id(
    conn: &Connection,
    id: &str,
) -> Result<Option<AutomationWorkflowVersion>, String> {
    conn.query_row(
        r#"SELECT id, workflow_id, version, snapshot_json, created_at
           FROM automation_workflow_versions WHERE id = ?1"#,
        params![id],
        row_to_version,
    )
    .optional()
    .map_err(|e| format!("automation_version_by_id: {e}"))
}

pub(crate) fn run_by_id(conn: &Connection, run_id: &str) -> Result<Option<AutomationRun>, String> {
    conn.query_row(
        r#"SELECT id, workflow_id, workflow_version_id, status, trigger_json, scope_json, started_at, finished_at, error
           FROM automation_runs WHERE id = ?1"#,
        params![run_id],
        row_to_run,
    )
    .optional()
    .map_err(|e| format!("automation_get_run: {e}"))
}

pub(crate) fn node_states_for_run(
    conn: &Connection,
    run_id: &str,
) -> Result<Vec<AutomationRunNodeState>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT id, run_id, node_id, status, input_json, output_json, error, started_at, finished_at
               FROM automation_run_nodes
               WHERE run_id = ?1
               ORDER BY id ASC"#,
        )
        .map_err(|e| format!("automation_get_run_nodes: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map(params![run_id], row_to_node_state)
        .map_err(|e| format!("automation_get_run_nodes: query 失败：{e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("automation_get_run_nodes: row 失败：{e}"))?);
    }
    Ok(out)
}

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use uuid::Uuid;

use crate::chat::state::ChatStore;
use crate::store::LiliaStore;
use crate::util::now_millis;

pub(crate) mod commands;
mod contract;
mod node_handlers;
mod repository;
mod signals;
mod types;

pub(crate) use contract::{
    task_created_event_kind, task_status_changed_event_kind, task_updated_event_kind,
};
use node_handlers::execute_node;
use repository::{json_text, node_states_for_run, run_by_id, version_by_id, workflow_by_id};
pub use signals::{
    emit_interaction_signal, emit_task_changed_signal, emit_timeline_signal, emit_todo_signal,
};
use types::{AutomationChangedEvent, AutomationRunEvent, GraphExecution};
pub use types::{
    AutomationDraft, AutomationEdge, AutomationNode, AutomationRun, AutomationRunNodeState,
    AutomationRunStatus, AutomationSignalEnvelope, AutomationWorkflow, AutomationWorkflowVersion,
};

fn merge_json_objects(base: JsonValue, patch: JsonValue) -> JsonValue {
    let mut out = base.as_object().cloned().unwrap_or_default();
    if let Some(patch) = patch.as_object() {
        for (key, value) in patch {
            out.insert(key.clone(), value.clone());
        }
    }
    JsonValue::Object(out)
}

fn emit_changed<R: Runtime>(app: &AppHandle<R>, workflow_id: Option<String>) {
    let _ = app.emit(
        contract::changed_event_name(),
        AutomationChangedEvent { workflow_id },
    );
}

fn emit_run<R: Runtime>(app: &AppHandle<R>, event: &str, run: AutomationRun) {
    let _ = app.emit(event, AutomationRunEvent { run });
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AbandonedAgentTurn {
    run_id: String,
    node_id: String,
    task_id: String,
    turn_id: String,
}

pub(crate) fn recover_abandoned_agent_runs<R: Runtime>(
    app: &AppHandle<R>,
    chat_store: &ChatStore,
) -> Result<Vec<AutomationRun>, String> {
    let conn = app.state::<LiliaStore>().conn()?;
    let abandoned = abandoned_agent_turns(&conn, chat_store)?;
    let mut recovered = Vec::new();
    for turn in abandoned {
        let Some(run) = run_by_id(&conn, &turn.run_id)? else {
            continue;
        };
        if run.status != AutomationRunStatus::Running {
            continue;
        }
        let node_states = node_states_for_run(&conn, &run.id)?;
        let Some(agent_state) = node_states.into_iter().find(|state| {
            state.node_id == turn.node_id && state.status == AutomationRunStatus::Running
        }) else {
            continue;
        };
        let error = format!(
            "Agent 节点运行恢复失败：运行进程已丢失（task={}, turn={}）",
            turn.task_id, turn.turn_id
        );
        let prior_output = agent_state
            .output
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let node_output = merge_json_objects(
            prior_output,
            serde_json::json!({
                "waitingAgent": false,
                "completed": false,
                "selectedHandle": "error",
                "recovered": false,
            }),
        );
        update_node_state(
            &conn,
            &run.id,
            &agent_state.node_id,
            AutomationRunStatus::Failed,
            agent_state.input,
            Some(node_output),
            Some(error.clone()),
        )?;
        recovered.push(finish_run(
            &conn,
            app,
            &run.id,
            AutomationRunStatus::Failed,
            Some(error),
        )?);
        if let Err(err) = crate::chat::state::clear_runtime_state(&conn, &turn.task_id) {
            eprintln!(
                "[automation] clear abandoned agent runtime state failed for task {}: {err}",
                turn.task_id
            );
        }
    }
    Ok(recovered)
}

fn abandoned_agent_turns(
    conn: &Connection,
    chat_store: &ChatStore,
) -> Result<Vec<AbandonedAgentTurn>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT r.id, n.node_id, n.output_json
               FROM automation_runs r
               JOIN automation_run_nodes n ON n.run_id = r.id
               WHERE r.status = 'running'
                 AND n.status = 'running'
                 AND n.output_json IS NOT NULL
               ORDER BY r.started_at ASC"#,
        )
        .map_err(|e| format!("automation_recover_agent_runs: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("automation_recover_agent_runs: query 失败：{e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let Ok((run_id, node_id, output_json)) = row else {
            continue;
        };
        let Ok(output) = serde_json::from_str::<JsonValue>(&output_json) else {
            continue;
        };
        if !output
            .get("waitingAgent")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let Some(task_id) = output
            .get("taskId")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        let Some(turn_id) = output
            .get("turnId")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        if agent_turn_is_still_recoverable(conn, chat_store, task_id)? {
            continue;
        }
        out.push(AbandonedAgentTurn {
            run_id,
            node_id,
            task_id: task_id.to_string(),
            turn_id: turn_id.to_string(),
        });
    }
    Ok(out)
}

fn agent_turn_is_still_recoverable(
    conn: &Connection,
    chat_store: &ChatStore,
    task_id: &str,
) -> Result<bool, String> {
    if chat_store
        .running_turns
        .lock()
        .unwrap()
        .contains_key(task_id)
    {
        return Ok(true);
    }
    if chat_store
        .pending_turns
        .lock()
        .unwrap()
        .contains_key(task_id)
    {
        return Ok(true);
    }
    let pending_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM task_pending_turns WHERE task_id = ?1",
            params![task_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("automation_recover_agent_runs: pending turn 查询失败：{e}"))?;
    Ok(pending_count > 0)
}

pub fn automation_complete_agent_turn<R: Runtime>(
    app: &AppHandle<R>,
    chat_store: &ChatStore,
    automation_run_id: Option<String>,
    turn_id: &str,
    success: bool,
) {
    let Some(run_id) = automation_run_id else {
        return;
    };
    if let Err(err) = complete_agent_turn(app, chat_store, &run_id, turn_id, success) {
        eprintln!("[automation] complete agent turn failed for run {run_id}: {err}");
    }
}

pub(crate) fn automation_run_id_for_waiting_turn<R: Runtime>(
    app: &AppHandle<R>,
    turn_id: &str,
) -> Result<Option<String>, String> {
    let conn = app.state::<LiliaStore>().conn()?;
    automation_run_id_for_waiting_turn_with_conn(&conn, turn_id)
}

fn automation_run_id_for_waiting_turn_with_conn(
    conn: &Connection,
    turn_id: &str,
) -> Result<Option<String>, String> {
    conn.query_row(
        r#"SELECT run_id
           FROM automation_run_nodes
           WHERE status = 'running'
             AND json_extract(output_json, '$.waitingAgent') = 1
             AND json_extract(output_json, '$.turnId') = ?1
           ORDER BY started_at DESC
           LIMIT 1"#,
        params![turn_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| format!("查询等待 Agent Turn 的 Automation 失败：{error}"))
}

pub(crate) fn waiting_node_state(
    states: &[AutomationRunNodeState],
    node_id: Option<&str>,
) -> Result<AutomationRunNodeState, String> {
    states
        .iter()
        .find(|state| {
            state.status == AutomationRunStatus::WaitingUser
                && node_id.map(|id| id == state.node_id).unwrap_or(true)
        })
        .cloned()
        .ok_or_else(|| "automation_resume_run: 未找到等待中的人工节点".to_string())
}

pub(crate) fn outputs_from_node_states(
    states: &[AutomationRunNodeState],
) -> BTreeMap<String, JsonValue> {
    states
        .iter()
        .filter_map(|state| match state.status {
            AutomationRunStatus::Succeeded | AutomationRunStatus::Skipped => {
                state.output.clone().map(|output| {
                    (
                        state.node_id.clone(),
                        serde_json::json!({
                            "status": state.status.as_str(),
                            "output": output,
                        }),
                    )
                })
            }
            _ => None,
        })
        .collect()
}

fn running_agent_node_state(
    states: &[AutomationRunNodeState],
    turn_id: &str,
) -> Result<AutomationRunNodeState, String> {
    states
        .iter()
        .find(|state| {
            state.status == AutomationRunStatus::Running
                && state
                    .output
                    .as_ref()
                    .and_then(|output| output.get("waitingAgent"))
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
                && state
                    .output
                    .as_ref()
                    .and_then(|output| output.get("turnId"))
                    .and_then(|value| value.as_str())
                    == Some(turn_id)
        })
        .cloned()
        .ok_or_else(|| "automation_complete_agent_turn: 未找到等待中的 Agent 节点".to_string())
}

fn complete_agent_turn<R: Runtime>(
    app: &AppHandle<R>,
    chat_store: &ChatStore,
    run_id: &str,
    turn_id: &str,
    success: bool,
) -> Result<AutomationRun, String> {
    let conn = app.state::<LiliaStore>().conn()?;
    let run = run_by_id(&conn, run_id)?
        .ok_or_else(|| "automation_complete_agent_turn: run 不存在".to_string())?;
    if run.status != AutomationRunStatus::Running {
        return Ok(run);
    }
    let version = version_by_id(&conn, &run.workflow_version_id)?
        .ok_or_else(|| "automation_complete_agent_turn: 发布版本不存在".to_string())?;
    validate_workflow_graph(&version.snapshot.nodes, &version.snapshot.edges)?;
    let node_states = node_states_for_run(&conn, run_id)?;
    let agent_state = running_agent_node_state(&node_states, turn_id)?;
    let mut outputs = outputs_from_node_states(&node_states);
    let prior_output = agent_state
        .output
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    let node_output = merge_json_objects(
        prior_output,
        serde_json::json!({
            "waitingAgent": false,
            "completed": success,
            "selectedHandle": if success { "success" } else { "error" },
        }),
    );
    let node_status = if success {
        AutomationRunStatus::Succeeded
    } else {
        AutomationRunStatus::Failed
    };
    update_node_state(
        &conn,
        &run.id,
        &agent_state.node_id,
        node_status.clone(),
        agent_state.input.clone(),
        Some(node_output.clone()),
        None,
    )?;
    if !success {
        return finish_run(
            &conn,
            app,
            &run.id,
            AutomationRunStatus::Failed,
            Some("Agent 节点运行失败或被中断".to_string()),
        );
    }
    outputs.insert(
        agent_state.node_id.clone(),
        serde_json::json!({
            "status": node_status.as_str(),
            "output": node_output,
        }),
    );
    let result = execute_graph(
        app,
        chat_store,
        &conn,
        &run,
        &version.snapshot,
        Some(agent_state.node_id),
        outputs,
    );
    match result {
        Ok(GraphExecution::Finished) => {
            finish_run(&conn, app, &run.id, AutomationRunStatus::Succeeded, None)
        }
        Ok(GraphExecution::WaitingUser) => update_run_status(
            &conn,
            app,
            &run.id,
            AutomationRunStatus::WaitingUser,
            None,
            false,
        ),
        Ok(GraphExecution::WaitingAgent) => update_run_status(
            &conn,
            app,
            &run.id,
            AutomationRunStatus::Running,
            None,
            false,
        ),
        Err(err) => finish_run(&conn, app, &run.id, AutomationRunStatus::Failed, Some(err)),
    }
}

fn run_workflow<R: Runtime>(
    app: &AppHandle<R>,
    chat_store: &ChatStore,
    workflow_id: &str,
    signal: AutomationSignalEnvelope,
) -> Result<AutomationRun, String> {
    let conn = app.state::<LiliaStore>().conn()?;
    let workflow = workflow_by_id(&conn, workflow_id)?
        .ok_or_else(|| "automation_run_once: 自动化不存在".to_string())?;
    let version_id = workflow
        .published_version_id
        .clone()
        .ok_or_else(|| "automation_run_once: 运行前需要先发布".to_string())?;
    let version = version_by_id(&conn, &version_id)?
        .ok_or_else(|| "automation_run_once: 发布版本不存在".to_string())?;
    validate_workflow_graph(&version.snapshot.nodes, &version.snapshot.edges)?;
    if workflow_has_active_run(&conn, workflow_id)? {
        return create_skipped_run(&conn, app, &workflow, &version, signal);
    }
    let run = create_run(&conn, app, &workflow, &version, signal)?;
    emit_run(app, contract::run_started_event_name(), run.clone());
    let result = execute_graph(
        app,
        chat_store,
        &conn,
        &run,
        &version.snapshot,
        None,
        BTreeMap::new(),
    );
    match result {
        Ok(GraphExecution::Finished) => {
            finish_run(&conn, app, &run.id, AutomationRunStatus::Succeeded, None)
        }
        Ok(GraphExecution::WaitingUser) => update_run_status(
            &conn,
            app,
            &run.id,
            AutomationRunStatus::WaitingUser,
            None,
            false,
        ),
        Ok(GraphExecution::WaitingAgent) => update_run_status(
            &conn,
            app,
            &run.id,
            AutomationRunStatus::Running,
            None,
            false,
        ),
        Err(err) => finish_run(&conn, app, &run.id, AutomationRunStatus::Failed, Some(err)),
    }
}

fn workflow_has_active_run(conn: &Connection, workflow_id: &str) -> Result<bool, String> {
    let count: i64 = conn
        .query_row(
            r#"SELECT COUNT(*) FROM automation_runs
               WHERE workflow_id = ?1 AND status IN ('pending', 'running', 'waiting_user')"#,
            params![workflow_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("automation_active_run: {e}"))?;
    Ok(count > 0)
}

fn create_skipped_run<R: Runtime>(
    conn: &Connection,
    app: &AppHandle<R>,
    workflow: &AutomationWorkflow,
    version: &AutomationWorkflowVersion,
    signal: AutomationSignalEnvelope,
) -> Result<AutomationRun, String> {
    let mut run = insert_run(
        conn,
        workflow,
        version,
        signal,
        AutomationRunStatus::Skipped,
        Some("同一自动化已有运行中实例".to_string()),
    )?;
    run.finished_at = Some(now_millis());
    conn.execute(
        "UPDATE automation_runs SET finished_at = ?1 WHERE id = ?2",
        params![run.finished_at, run.id],
    )
    .map_err(|e| format!("automation_skipped_run: update 失败：{e}"))?;
    emit_run(app, contract::run_finished_event_name(), run.clone());
    Ok(run)
}

fn create_run<R: Runtime>(
    conn: &Connection,
    app: &AppHandle<R>,
    workflow: &AutomationWorkflow,
    version: &AutomationWorkflowVersion,
    signal: AutomationSignalEnvelope,
) -> Result<AutomationRun, String> {
    let run = insert_run(
        conn,
        workflow,
        version,
        signal,
        AutomationRunStatus::Running,
        None,
    )?;
    for node in &version.snapshot.nodes {
        insert_node_state(
            conn,
            &run.id,
            &node.id,
            AutomationRunStatus::Pending,
            serde_json::json!({}),
            None,
            None,
            None,
        )?;
    }
    emit_run(app, contract::run_updated_event_name(), run.clone());
    Ok(run)
}

fn insert_run(
    conn: &Connection,
    workflow: &AutomationWorkflow,
    version: &AutomationWorkflowVersion,
    signal: AutomationSignalEnvelope,
    status: AutomationRunStatus,
    error: Option<String>,
) -> Result<AutomationRun, String> {
    let now = now_millis();
    let run = AutomationRun {
        id: Uuid::new_v4().to_string(),
        workflow_id: workflow.id.clone(),
        workflow_version_id: version.id.clone(),
        status,
        trigger: signal,
        scope: version.snapshot.scope.clone(),
        started_at: now,
        finished_at: None,
        error,
    };
    conn.execute(
        r#"INSERT INTO automation_runs
           (id, workflow_id, workflow_version_id, status, trigger_json, scope_json, started_at, finished_at, error)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)"#,
        params![
            run.id,
            run.workflow_id,
            run.workflow_version_id,
            run.status.as_str(),
            json_text(&run.trigger, "trigger")?,
            json_text(&run.scope, "scope")?,
            run.started_at,
            run.error,
        ],
    )
    .map_err(|e| format!("automation_insert_run: {e}"))?;
    Ok(run)
}

fn finish_run<R: Runtime>(
    conn: &Connection,
    app: &AppHandle<R>,
    run_id: &str,
    status: AutomationRunStatus,
    error: Option<String>,
) -> Result<AutomationRun, String> {
    update_run_status(conn, app, run_id, status, error, true)
}

fn update_run_status<R: Runtime>(
    conn: &Connection,
    app: &AppHandle<R>,
    run_id: &str,
    status: AutomationRunStatus,
    error: Option<String>,
    finished: bool,
) -> Result<AutomationRun, String> {
    let now = now_millis();
    let finished_at = finished.then_some(now);
    conn.execute(
        r#"UPDATE automation_runs
           SET status = ?1, finished_at = ?2, error = ?3
           WHERE id = ?4"#,
        params![status.as_str(), finished_at, error, run_id],
    )
    .map_err(|e| format!("automation_update_run_status: {e}"))?;
    let run = run_by_id(conn, run_id)?
        .ok_or_else(|| "automation_update_run_status: run 不存在".to_string())?;
    emit_run(
        app,
        if finished {
            contract::run_finished_event_name()
        } else {
            contract::run_updated_event_name()
        },
        run.clone(),
    );
    Ok(run)
}

fn insert_node_state(
    conn: &Connection,
    run_id: &str,
    node_id: &str,
    status: AutomationRunStatus,
    input: JsonValue,
    output: Option<JsonValue>,
    error: Option<String>,
    finished_at: Option<i64>,
) -> Result<(), String> {
    let now = now_millis();
    conn.execute(
        r#"INSERT OR REPLACE INTO automation_run_nodes
           (id, run_id, node_id, status, input_json, output_json, error, started_at, finished_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
        params![
            format!("{run_id}:{node_id}"),
            run_id,
            node_id,
            status.as_str(),
            json_text(&input, "node input")?,
            output
                .map(|value| json_text(&value, "node output"))
                .transpose()?,
            error,
            now,
            finished_at,
        ],
    )
    .map_err(|e| format!("automation_node_state: {e}"))?;
    Ok(())
}

fn update_node_state(
    conn: &Connection,
    run_id: &str,
    node_id: &str,
    status: AutomationRunStatus,
    input: JsonValue,
    output: Option<JsonValue>,
    error: Option<String>,
) -> Result<(), String> {
    insert_node_state(
        conn,
        run_id,
        node_id,
        status,
        input,
        output,
        error,
        Some(now_millis()),
    )
}

fn execute_graph<R: Runtime>(
    app: &AppHandle<R>,
    chat_store: &ChatStore,
    conn: &Connection,
    run: &AutomationRun,
    draft: &AutomationDraft,
    resume_from_node: Option<String>,
    mut outputs: BTreeMap<String, JsonValue>,
) -> Result<GraphExecution, String> {
    let ordered = topological_order(&draft.nodes, &draft.edges)?;
    let node_map: BTreeMap<String, AutomationNode> = draft
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect();
    let mut active_nodes = resume_from_node
        .as_deref()
        .and_then(|node_id| node_map.get(node_id))
        .map(|node| {
            active_outgoing_edges(
                &draft.edges,
                &node.id,
                outputs
                    .get(&node.id)
                    .and_then(|value| value.get("output"))
                    .unwrap_or(&JsonValue::Null),
            )
            .into_iter()
            .map(|edge| edge.target.clone())
            .collect()
        })
        .unwrap_or_else(|| initial_active_nodes(&draft.nodes, &draft.edges));
    let mut reached_nodes: BTreeSet<String> = outputs.keys().cloned().collect();
    for node_id in ordered {
        let Some(node) = node_map.get(&node_id) else {
            continue;
        };
        if outputs.contains_key(&node.id) {
            continue;
        }
        if !active_nodes.contains(&node.id) {
            continue;
        }
        let input = serde_json::json!({
            "trigger": run.trigger,
            "nodes": outputs,
            "config": node.config,
        });
        update_node_state(
            conn,
            &run.id,
            &node.id,
            AutomationRunStatus::Running,
            input.clone(),
            None,
            None,
        )?;
        let output = match execute_node(app, chat_store, conn, run, node, &input) {
            Ok(output) => output,
            Err(err) => {
                update_node_state(
                    conn,
                    &run.id,
                    &node.id,
                    AutomationRunStatus::Failed,
                    input,
                    None,
                    Some(err.clone()),
                )?;
                return Err(err);
            }
        };
        if output
            .get("waitingUser")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            update_node_state(
                conn,
                &run.id,
                &node.id,
                AutomationRunStatus::WaitingUser,
                input,
                Some(output),
                None,
            )?;
            return Ok(GraphExecution::WaitingUser);
        }
        if output
            .get("waitingAgent")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            update_node_state(
                conn,
                &run.id,
                &node.id,
                AutomationRunStatus::Running,
                input,
                Some(output),
                None,
            )?;
            return Ok(GraphExecution::WaitingAgent);
        }
        let next_status = if output
            .get("skipped")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            AutomationRunStatus::Skipped
        } else {
            AutomationRunStatus::Succeeded
        };
        reached_nodes.insert(node.id.clone());
        outputs.insert(
            node.id.clone(),
            serde_json::json!({
                "status": next_status.as_str(),
                "output": output.clone(),
            }),
        );
        update_node_state(
            conn,
            &run.id,
            &node.id,
            next_status,
            input,
            Some(output.clone()),
            None,
        )?;
        if output
            .get("stopped")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            break;
        }
        for edge in active_outgoing_edges(&draft.edges, &node.id, &output) {
            active_nodes.insert(edge.target.clone());
        }
    }
    let skipped_input = serde_json::json!({
        "trigger": run.trigger,
        "nodes": outputs,
    });
    for node in &draft.nodes {
        if !reached_nodes.contains(&node.id) {
            update_node_state(
                conn,
                &run.id,
                &node.id,
                AutomationRunStatus::Skipped,
                skipped_input.clone(),
                Some(serde_json::json!({ "skipped": true, "reason": "分支未命中" })),
                None,
            )?;
        }
    }
    Ok(GraphExecution::Finished)
}

fn initial_active_nodes(nodes: &[AutomationNode], edges: &[AutomationEdge]) -> BTreeSet<String> {
    let trigger_nodes: BTreeSet<String> = nodes
        .iter()
        .filter(|node| node.kind == "trigger")
        .map(|node| node.id.clone())
        .collect();
    if !trigger_nodes.is_empty() {
        return trigger_nodes;
    }
    let targets: BTreeSet<String> = edges.iter().map(|edge| edge.target.clone()).collect();
    nodes
        .iter()
        .filter(|node| !targets.contains(&node.id))
        .map(|node| node.id.clone())
        .collect()
}

fn active_outgoing_edges<'a>(
    edges: &'a [AutomationEdge],
    node_id: &str,
    output: &JsonValue,
) -> Vec<&'a AutomationEdge> {
    let outgoing: Vec<&AutomationEdge> =
        edges.iter().filter(|edge| edge.source == node_id).collect();
    if outgoing.is_empty() {
        return Vec::new();
    }
    let selected_handles = selected_output_handles(output);
    let has_exact_selected_edge = outgoing.iter().any(|edge| {
        edge.source_handle
            .as_deref()
            .map(normalize_handle)
            .filter(|handle| handle != "default")
            .map(|handle| selected_handles.contains(&handle))
            .unwrap_or(false)
    });
    outgoing
        .into_iter()
        .filter(|edge| {
            let handle = edge.source_handle.as_deref().map(normalize_handle);
            match handle.as_deref() {
                Some("default") => {
                    output.get("routeKind").and_then(|value| value.as_str()) == Some("switch")
                        && !has_exact_selected_edge
                }
                Some(handle) => selected_handles.contains(handle),
                None => {
                    selected_handles.is_empty()
                        || selected_handles.contains("success")
                        || selected_handles.contains("true")
                }
            }
        })
        .collect()
}

fn selected_output_handles(output: &JsonValue) -> BTreeSet<String> {
    let mut handles = BTreeSet::new();
    if let Some(handle) = output
        .get("selectedHandle")
        .and_then(|value| value.as_str())
    {
        let normalized = normalize_handle(handle);
        if !normalized.is_empty() {
            handles.insert(normalized);
        }
    }
    if let Some(items) = output
        .get("selectedHandles")
        .and_then(|value| value.as_array())
    {
        for item in items {
            if let Some(handle) = item.as_str() {
                let normalized = normalize_handle(handle);
                if !normalized.is_empty() {
                    handles.insert(normalized);
                }
            }
        }
    }
    handles
}

fn normalize_handle(handle: &str) -> String {
    handle.trim().to_ascii_lowercase()
}

fn validate_workflow_graph(
    nodes: &[AutomationNode],
    edges: &[AutomationEdge],
) -> Result<(), String> {
    let ids: BTreeSet<String> = nodes.iter().map(|node| node.id.clone()).collect();
    if ids.len() != nodes.len() {
        return Err("自动化节点 id 不能重复".to_string());
    }
    let trigger_count = nodes.iter().filter(|node| node.kind == "trigger").count();
    if !nodes.is_empty() && trigger_count == 0 {
        return Err("MVP 自动化需要一个触发节点".to_string());
    }
    if trigger_count > 1 {
        return Err("MVP 自动化只允许一个触发节点".to_string());
    }
    for edge in edges {
        if !ids.contains(&edge.source) || !ids.contains(&edge.target) {
            return Err("自动化连线引用了不存在的节点".to_string());
        }
    }
    topological_order(nodes, edges).map(|_| ())
}

fn topological_order(
    nodes: &[AutomationNode],
    edges: &[AutomationEdge],
) -> Result<Vec<String>, String> {
    let mut incoming: BTreeMap<String, usize> =
        nodes.iter().map(|node| (node.id.clone(), 0)).collect();
    let mut outgoing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for edge in edges {
        *incoming.entry(edge.target.clone()).or_default() += 1;
        outgoing
            .entry(edge.source.clone())
            .or_default()
            .push(edge.target.clone());
    }
    let mut ready: VecDeque<String> = incoming
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(id.clone()))
        .collect();
    let mut ordered = Vec::new();
    while let Some(id) = ready.pop_front() {
        ordered.push(id.clone());
        for target in outgoing.get(&id).cloned().unwrap_or_default() {
            if let Some(count) = incoming.get_mut(&target) {
                *count -= 1;
                if *count == 0 {
                    ready.push_back(target);
                }
            }
        }
    }
    if ordered.len() != nodes.len() {
        return Err("自动化工作流不能包含循环".to_string());
    }
    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_interaction_contract;
    use crate::agent_timeline::AgentTimelineEvent;
    use crate::automation::types::{AutomationNodePosition, AutomationScopeFilter};
    use crate::chat::state::{ChatStore, RunningTurn};
    use crate::BACKEND_CODEX;
    use rusqlite::Connection;

    fn node(id: &str, kind: &str) -> AutomationNode {
        AutomationNode {
            id: id.to_string(),
            kind: kind.to_string(),
            title: id.to_string(),
            position: AutomationNodePosition { x: 0.0, y: 0.0 },
            config: serde_json::json!({}),
        }
    }

    fn edge(source: &str, target: &str) -> AutomationEdge {
        AutomationEdge {
            id: format!("{source}-{target}"),
            source: source.to_string(),
            target: target.to_string(),
            source_handle: None,
            target_handle: None,
        }
    }

    fn edge_with_handle(source: &str, target: &str, source_handle: &str) -> AutomationEdge {
        AutomationEdge {
            id: format!("{source}-{source_handle}-{target}"),
            source: source.to_string(),
            target: target.to_string(),
            source_handle: Some(source_handle.to_string()),
            target_handle: None,
        }
    }

    fn create_workflow_tables(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE automation_workflows (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                scope_json TEXT NOT NULL,
                draft_json TEXT NOT NULL,
                published_version_id TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE automation_workflow_versions (
                id TEXT PRIMARY KEY NOT NULL,
                workflow_id TEXT NOT NULL,
                version INTEGER NOT NULL,
                snapshot_json TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE automation_runs (
                id TEXT PRIMARY KEY NOT NULL,
                workflow_id TEXT NOT NULL,
                workflow_version_id TEXT NOT NULL,
                status TEXT NOT NULL,
                trigger_json TEXT NOT NULL,
                scope_json TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                error TEXT
            );
            "#,
        )
        .unwrap();
    }

    fn create_run_recovery_tables(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE automation_runs (
                id TEXT PRIMARY KEY NOT NULL,
                workflow_id TEXT NOT NULL,
                workflow_version_id TEXT NOT NULL,
                status TEXT NOT NULL,
                trigger_json TEXT NOT NULL,
                scope_json TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                error TEXT
            );
            CREATE TABLE automation_run_nodes (
                id TEXT PRIMARY KEY NOT NULL,
                run_id TEXT NOT NULL,
                node_id TEXT NOT NULL,
                status TEXT NOT NULL,
                input_json TEXT NOT NULL,
                output_json TEXT,
                error TEXT,
                started_at INTEGER,
                finished_at INTEGER
            );
            CREATE TABLE task_pending_turns (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL,
                turn_id TEXT NOT NULL
            );
            "#,
        )
        .unwrap();
    }

    fn insert_waiting_agent_run(conn: &Connection) {
        conn.execute(
            r#"INSERT INTO automation_runs
               (id, workflow_id, workflow_version_id, status, trigger_json, scope_json, started_at, finished_at, error)
               VALUES ('run-1', 'wf-1', 'ver-1', 'running', ?1, ?2, 1, NULL, NULL)"#,
            params![
                json_text(&signals::manual_signal(None), "trigger").unwrap(),
                json_text(&AutomationScopeFilter::default(), "scope").unwrap(),
            ],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO automation_run_nodes
               (id, run_id, node_id, status, input_json, output_json, error, started_at, finished_at)
               VALUES ('run-1:agent-1', 'run-1', 'agent-1', 'running', '{}', ?1, NULL, 1, NULL)"#,
            params![json_text(
                &serde_json::json!({
                    "waitingAgent": true,
                    "taskId": "task-1",
                    "turnId": "turn-1",
                }),
                "output",
            )
            .unwrap()],
        )
        .unwrap();
    }

    #[test]
    fn waiting_automation_run_is_recovered_by_agent_turn_id() {
        let conn = Connection::open_in_memory().unwrap();
        create_run_recovery_tables(&conn);
        insert_waiting_agent_run(&conn);

        assert_eq!(
            automation_run_id_for_waiting_turn_with_conn(&conn, "turn-1").unwrap(),
            Some("run-1".into())
        );
        assert_eq!(
            automation_run_id_for_waiting_turn_with_conn(&conn, "turn-missing").unwrap(),
            None
        );
    }

    #[test]
    fn graph_validation_rejects_cycles() {
        let nodes = vec![node("a", "trigger"), node("b", "tool")];
        let edges = vec![edge("a", "b"), edge("b", "a")];

        assert!(validate_workflow_graph(&nodes, &edges)
            .unwrap_err()
            .contains("循环"));
    }

    #[test]
    fn graph_validation_requires_single_trigger_for_non_empty_workflow() {
        assert!(validate_workflow_graph(&[], &[]).is_ok());

        let nodes = vec![node("a", "tool"), node("b", "logic")];
        assert!(validate_workflow_graph(&nodes, &[])
            .unwrap_err()
            .contains("触发节点"));

        let nodes = vec![
            node("first", "trigger"),
            node("second", "trigger"),
            node("tool", "tool"),
        ];
        assert!(validate_workflow_graph(&nodes, &[])
            .unwrap_err()
            .contains("只允许一个触发节点"));
    }

    #[test]
    fn automation_tool_contract_matches_rust_dispatch_defaults() {
        let rust_scope_event_kinds = vec![
            "task_created",
            "task_status_changed",
            "task_updated",
            "timeline_event",
            "todo_changed",
            "interaction_request",
        ];
        let rust_run_statuses = vec![
            "pending",
            "running",
            "succeeded",
            "failed",
            "skipped",
            "waiting_user",
        ];
        assert_eq!(contract::default_trigger_kind(), "manual");
        assert_eq!(contract::task_changed_trigger_kind(), "task_changed");
        assert_eq!(contract::timeline_trigger_kind(), "timeline_event");
        assert_eq!(contract::todo_trigger_kind(), "todo_changed");
        assert_eq!(contract::interaction_trigger_kind(), "interaction_request");
        assert_eq!(
            contract::scope_event_kinds()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            rust_scope_event_kinds
        );
        assert_eq!(contract::task_created_event_kind(), "task_created");
        assert_eq!(
            contract::task_status_changed_event_kind(),
            "task_status_changed"
        );
        assert_eq!(contract::task_updated_event_kind(), "task_updated");
        assert_eq!(
            contract::run_statuses()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            rust_run_statuses
        );
        assert_eq!(contract::default_run_status(), "pending");
        assert_eq!(contract::changed_event_name(), "automation:changed");
        assert_eq!(contract::run_started_event_name(), "automation:run-started");
        assert_eq!(contract::run_updated_event_name(), "automation:run-updated");
        assert_eq!(
            contract::run_finished_event_name(),
            "automation:run-finished"
        );
        assert_eq!(
            AutomationRunStatus::from_str("waiting_user"),
            AutomationRunStatus::WaitingUser
        );
        assert_eq!(
            AutomationRunStatus::from_str("unknown"),
            AutomationRunStatus::Pending
        );
        assert_eq!(node_handlers::normalize_task_priority("high"), "high");
        assert_eq!(node_handlers::normalize_task_priority("low"), "low");
        assert_eq!(node_handlers::normalize_task_priority("urgent"), "normal");
    }

    #[test]
    fn list_runs_returns_lightweight_summaries_sorted_and_filtered() {
        let conn = Connection::open_in_memory().unwrap();
        create_workflow_tables(&conn);
        let scope = AutomationScopeFilter::default();
        let trigger = AutomationSignalEnvelope {
            id: "signal-1".to_string(),
            kind: "timeline_event".to_string(),
            project_id: Some("project-1".to_string()),
            task_id: Some("task-1".to_string()),
            backend: Some("claude".to_string()),
            event_kind: Some("timeline_event".to_string()),
            automation_run_id: None,
            payload: serde_json::json!({ "largePayload": "x".repeat(1024) }),
            created_at: 1,
        };
        for (run_id, workflow_id, started_at) in [
            ("run-old", "wf-1", 10_i64),
            ("run-new", "wf-1", 20_i64),
            ("run-other", "wf-2", 30_i64),
        ] {
            conn.execute(
                r#"INSERT INTO automation_runs
                   (id, workflow_id, workflow_version_id, status, trigger_json, scope_json, started_at, finished_at, error)
                   VALUES (?1, ?2, 'ver-1', 'succeeded', ?3, ?4, ?5, ?6, NULL)"#,
                params![
                    run_id,
                    workflow_id,
                    json_text(&trigger, "trigger").unwrap(),
                    json_text(&scope, "scope").unwrap(),
                    started_at,
                    started_at + 1,
                ],
            )
            .unwrap();
        }

        let summaries = repository::list_runs(&conn, Some("wf-1")).unwrap();

        assert_eq!(
            summaries
                .iter()
                .map(|run| run.id.as_str())
                .collect::<Vec<_>>(),
            vec!["run-new", "run-old"],
        );
        assert_eq!(summaries[0].trigger_kind, "timeline_event");
        assert_eq!(summaries[0].project_id.as_deref(), Some("project-1"));
        assert_eq!(summaries[0].task_id.as_deref(), Some("task-1"));
        assert_eq!(summaries[0].backend.as_deref(), Some("claude"));
        assert_eq!(summaries[0].event_kind.as_deref(), Some("timeline_event"));
        let summary_json = serde_json::to_value(&summaries[0]).unwrap();
        assert!(summary_json.get("trigger").is_none());
        assert!(summary_json.get("scope").is_none());
    }

    #[test]
    fn scope_matches_backend_and_event_kind() {
        let scope = AutomationScopeFilter {
            project_ids: Vec::new(),
            include_inbox: true,
            task_statuses: Vec::new(),
            backends: vec!["codex".to_string()],
            event_kinds: vec!["timeline_event".to_string()],
        };
        let signal = AutomationSignalEnvelope {
            id: "s1".to_string(),
            kind: "timeline_event".to_string(),
            project_id: None,
            task_id: Some("task-1".to_string()),
            backend: Some("codex".to_string()),
            event_kind: Some("timeline_event".to_string()),
            automation_run_id: None,
            payload: serde_json::json!({}),
            created_at: 1,
        };

        assert!(signals::scope_matches(&scope, &signal));
    }

    #[test]
    fn task_changed_signal_keeps_trigger_kind_and_splits_event_kind() {
        let signal = signals::task_changed_signal(
            Some("project-1".to_string()),
            Some("task-1".to_string()),
            Some("done".to_string()),
            "task_status_changed",
            None,
        );

        assert_eq!(signal.kind, "task_changed");
        assert_eq!(signal.event_kind.as_deref(), Some("task_status_changed"));
        assert_eq!(signal.payload["eventKind"], "task_status_changed");
        assert_eq!(signal.payload["taskStatus"], "done");

        let fallback = signals::task_changed_signal(None, None, None, " ", None);
        assert_eq!(fallback.event_kind.as_deref(), Some("task_changed"));
    }

    #[test]
    fn timeline_signal_uses_scope_filter_event_kind_and_keeps_specific_kind_in_payload() {
        let event = AgentTimelineEvent {
            id: "event-1".to_string(),
            task_id: "task-1".to_string(),
            turn_id: Some("turn-1".to_string()),
            backend: BACKEND_CODEX.to_string(),
            kind: "tool".to_string(),
            status: "completed".to_string(),
            title: "Tool".to_string(),
            summary: None,
            payload: serde_json::json!({ "automationRunId": "run-1" }),
            created_at: 1,
            updated_at: 2,
            turn_seq: 1,
            intra_turn_order: 1,
        };

        let signal = signals::timeline_signal(&event);

        assert_eq!(signal.kind, "timeline_event");
        assert_eq!(signal.event_kind.as_deref(), Some("timeline_event"));
        assert_eq!(signal.automation_run_id.as_deref(), Some("run-1"));
        assert_eq!(signal.payload["timelineEventKind"], "tool");
        assert_eq!(signal.payload["timelineEvent"]["kind"], "tool");
    }

    #[test]
    fn interaction_signal_uses_scope_filter_event_kind_and_keeps_interaction_kind_in_payload() {
        let signal = signals::interaction_signal(
            "task-1".to_string(),
            "turn-1".to_string(),
            BACKEND_CODEX.to_string(),
            "request-1".to_string(),
            agent_interaction_contract::ask_user_interaction_kind().to_string(),
            serde_json::json!({ "prompt": "确认？" }),
            None,
        );

        assert_eq!(signal.kind, "interaction_request");
        assert_eq!(signal.event_kind.as_deref(), Some("interaction_request"));
        assert_eq!(
            signal.payload["interactionKind"],
            agent_interaction_contract::ask_user_interaction_kind()
        );
    }

    #[test]
    fn abandoned_agent_turns_reports_lost_running_agent_node() {
        let conn = Connection::open_in_memory().unwrap();
        create_run_recovery_tables(&conn);
        insert_waiting_agent_run(&conn);
        let chat_store = ChatStore::default();

        let abandoned = abandoned_agent_turns(&conn, &chat_store).unwrap();

        assert_eq!(
            abandoned,
            vec![AbandonedAgentTurn {
                run_id: "run-1".to_string(),
                node_id: "agent-1".to_string(),
                task_id: "task-1".to_string(),
                turn_id: "turn-1".to_string(),
            }]
        );
    }

    #[test]
    fn abandoned_agent_turns_keeps_live_or_pending_agent_node_recoverable() {
        let conn = Connection::open_in_memory().unwrap();
        create_run_recovery_tables(&conn);
        insert_waiting_agent_run(&conn);
        let chat_store = ChatStore::default();
        chat_store.running_turns.lock().unwrap().insert(
            "task-1".to_string(),
            RunningTurn {
                turn_id: "turn-1".to_string(),
                backend: BACKEND_CODEX.to_string(),
                native_approval_pause: None,
            },
        );

        assert!(abandoned_agent_turns(&conn, &chat_store)
            .unwrap()
            .is_empty());

        chat_store.running_turns.lock().unwrap().clear();
        conn.execute(
            "INSERT INTO task_pending_turns (task_id, turn_id) VALUES ('task-1', 'turn-1')",
            [],
        )
        .unwrap();

        assert!(abandoned_agent_turns(&conn, &chat_store)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn enabled_workflows_match_published_snapshot_not_draft() {
        let conn = Connection::open_in_memory().unwrap();
        create_workflow_tables(&conn);
        let draft = AutomationDraft {
            nodes: vec![AutomationNode {
                id: "draft-trigger".to_string(),
                kind: "trigger".to_string(),
                title: "Draft".to_string(),
                position: AutomationNodePosition { x: 0.0, y: 0.0 },
                config: serde_json::json!({ "triggerKind": "manual" }),
            }],
            edges: Vec::new(),
            scope: AutomationScopeFilter {
                include_inbox: true,
                backends: vec!["codex".to_string()],
                ..Default::default()
            },
        };
        let snapshot = AutomationDraft {
            nodes: vec![AutomationNode {
                id: "published-trigger".to_string(),
                kind: "trigger".to_string(),
                title: "Published".to_string(),
                position: AutomationNodePosition { x: 0.0, y: 0.0 },
                config: serde_json::json!({ "triggerKind": "task_changed" }),
            }],
            edges: Vec::new(),
            scope: AutomationScopeFilter {
                include_inbox: true,
                ..Default::default()
            },
        };
        conn.execute(
            r#"INSERT INTO automation_workflows
               (id, name, enabled, scope_json, draft_json, published_version_id, created_at, updated_at)
               VALUES ('wf-1', 'Workflow', 1, ?1, ?2, 'ver-1', 1, 1)"#,
            params![
                json_text(&draft.scope, "scope").unwrap(),
                json_text(&draft, "draft").unwrap()
            ],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO automation_workflow_versions
               (id, workflow_id, version, snapshot_json, created_at)
               VALUES ('ver-1', 'wf-1', 1, ?1, 1)"#,
            params![json_text(&snapshot, "snapshot").unwrap()],
        )
        .unwrap();
        let signal = AutomationSignalEnvelope {
            id: "signal-1".to_string(),
            kind: "task_changed".to_string(),
            project_id: None,
            task_id: Some("task-1".to_string()),
            backend: None,
            event_kind: Some("task_changed".to_string()),
            automation_run_id: None,
            payload: serde_json::json!({ "taskStatus": "waiting" }),
            created_at: 1,
        };

        let workflows = signals::enabled_workflows_for_signal(&conn, &signal).unwrap();

        assert_eq!(workflows.len(), 1);
        assert_eq!(workflows[0].id, "wf-1");
    }

    #[test]
    fn task_status_signal_enriches_context_and_matches_global_scope() {
        let conn = Connection::open_in_memory().unwrap();
        create_workflow_tables(&conn);
        conn.execute_batch(
            r#"
            CREATE TABLE tasks (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT,
                status TEXT NOT NULL,
                archived INTEGER NOT NULL DEFAULT 0
            );
            INSERT INTO tasks (id, project_id, status, archived)
            VALUES ('task-1', 'project-1', 'done', 0);
            "#,
        )
        .unwrap();
        let draft = AutomationDraft {
            nodes: vec![AutomationNode {
                id: "trigger-1".to_string(),
                kind: "trigger".to_string(),
                title: "任务变化".to_string(),
                position: AutomationNodePosition { x: 0.0, y: 0.0 },
                config: serde_json::json!({ "triggerKind": "task_changed" }),
            }],
            edges: Vec::new(),
            scope: AutomationScopeFilter {
                include_inbox: false,
                project_ids: vec!["project-1".to_string()],
                task_statuses: vec!["done".to_string()],
                event_kinds: vec!["task_status_changed".to_string()],
                ..Default::default()
            },
        };
        conn.execute(
            r#"INSERT INTO automation_workflows
               (id, name, enabled, scope_json, draft_json, published_version_id, created_at, updated_at)
               VALUES ('wf-1', 'Done workflow', 1, ?1, ?2, 'ver-1', 1, 1)"#,
            params![
                json_text(&draft.scope, "scope").unwrap(),
                json_text(&draft, "draft").unwrap()
            ],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO automation_workflow_versions
               (id, workflow_id, version, snapshot_json, created_at)
               VALUES ('ver-1', 'wf-1', 1, ?1, 1)"#,
            params![json_text(&draft, "snapshot").unwrap()],
        )
        .unwrap();
        let mut signal = AutomationSignalEnvelope {
            id: "signal-1".to_string(),
            kind: "task_changed".to_string(),
            project_id: None,
            task_id: Some("task-1".to_string()),
            backend: None,
            event_kind: Some("task_status_changed".to_string()),
            automation_run_id: None,
            payload: serde_json::json!({}),
            created_at: 1,
        };

        signals::enrich_signal_context(&conn, &mut signal).unwrap();
        let workflows = signals::enabled_workflows_for_signal(&conn, &signal).unwrap();

        assert_eq!(signal.project_id.as_deref(), Some("project-1"));
        assert_eq!(signal.payload["taskStatus"], "done");
        assert_eq!(workflows.len(), 1);
        assert_eq!(workflows[0].id, "wf-1");
    }

    #[test]
    fn inserted_run_scope_uses_published_snapshot() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE automation_runs (
                id TEXT PRIMARY KEY NOT NULL,
                workflow_id TEXT NOT NULL,
                workflow_version_id TEXT NOT NULL,
                status TEXT NOT NULL,
                trigger_json TEXT NOT NULL,
                scope_json TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                error TEXT
            );
            "#,
        )
        .unwrap();
        let workflow = AutomationWorkflow {
            id: "wf-1".to_string(),
            name: "Workflow".to_string(),
            enabled: true,
            scope: AutomationScopeFilter {
                include_inbox: true,
                backends: vec!["codex".to_string()],
                ..Default::default()
            },
            draft: AutomationDraft::default(),
            published_version_id: Some("ver-1".to_string()),
            created_at: 1,
            updated_at: 1,
        };
        let version = AutomationWorkflowVersion {
            id: "ver-1".to_string(),
            workflow_id: "wf-1".to_string(),
            version: 1,
            snapshot: AutomationDraft {
                nodes: Vec::new(),
                edges: Vec::new(),
                scope: AutomationScopeFilter {
                    include_inbox: false,
                    project_ids: vec!["project-1".to_string()],
                    ..Default::default()
                },
            },
            created_at: 1,
        };

        let run = insert_run(
            &conn,
            &workflow,
            &version,
            signals::manual_signal(None),
            AutomationRunStatus::Running,
            None,
        )
        .unwrap();

        assert_eq!(run.scope, version.snapshot.scope);
        assert_ne!(run.scope, workflow.scope);
    }

    #[test]
    fn tool_todo_rows_distinguish_agent_todos_from_guides() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE task_todos (
                id TEXT PRIMARY KEY NOT NULL,
                task_id TEXT NOT NULL,
                text TEXT NOT NULL,
                done INTEGER NOT NULL,
                "order" INTEGER NOT NULL,
                source TEXT NOT NULL CHECK (source IN ('lilia','agent')),
                priority TEXT NOT NULL,
                guide_status TEXT,
                attachments_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            "#,
        )
        .unwrap();

        node_handlers::insert_task_todo_row(
            &conn,
            "todo-1",
            "task-1",
            "普通 Todo",
            "agent",
            "normal",
            None,
            1,
        )
        .unwrap();
        node_handlers::insert_task_todo_row(
            &conn,
            "guide-1",
            "task-1",
            "引导消息",
            "lilia",
            "high",
            Some("pending"),
            2,
        )
        .unwrap();

        let todo = conn
            .query_row(
                "SELECT source, priority, guide_status FROM task_todos WHERE id = 'todo-1'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .unwrap();
        let guide = conn
            .query_row(
                "SELECT source, priority, guide_status FROM task_todos WHERE id = 'guide-1'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .unwrap();

        assert_eq!(todo, ("agent".to_string(), "normal".to_string(), None));
        assert_eq!(
            guide,
            (
                "lilia".to_string(),
                "high".to_string(),
                Some("pending".to_string())
            )
        );
    }

    #[test]
    fn record_timeline_tool_targets_task_and_marks_automation_origin() {
        let run = AutomationRun {
            id: "run-1".to_string(),
            workflow_id: "wf-1".to_string(),
            workflow_version_id: "ver-1".to_string(),
            status: AutomationRunStatus::Running,
            trigger: AutomationSignalEnvelope {
                id: "signal-1".to_string(),
                kind: "manual".to_string(),
                project_id: None,
                task_id: Some("task-1".to_string()),
                backend: Some("codex".to_string()),
                event_kind: Some("manual".to_string()),
                automation_run_id: None,
                payload: serde_json::json!({}),
                created_at: 1,
            },
            scope: AutomationScopeFilter::default(),
            started_at: 1,
            finished_at: None,
            error: None,
        };
        let node = AutomationNode {
            id: "tool-1".to_string(),
            kind: "tool".to_string(),
            title: "记录".to_string(),
            position: AutomationNodePosition { x: 0.0, y: 0.0 },
            config: serde_json::json!({
                "title": "记录 ${trigger.kind}",
                "summary": "来自 ${trigger.taskId}",
                "status": "success"
            }),
        };
        let input = serde_json::json!({
            "trigger": run.trigger,
            "nodes": {},
            "config": node.config,
        });

        let event = node_handlers::timeline_input_from_tool(&run, &node, &input).unwrap();

        assert_eq!(event.id.as_deref(), Some("automation:run-1:tool-1"));
        assert_eq!(event.task_id, "task-1");
        assert_eq!(event.backend, "native-agentkit");
        assert_eq!(event.kind, "automation");
        assert_eq!(event.status, "success");
        assert_eq!(event.title, "记录 manual");
        assert_eq!(event.summary.as_deref(), Some("来自 task-1"));
        assert_eq!(event.payload["automationRunId"], "run-1");
    }

    #[test]
    fn template_replaces_json_paths() {
        let value = serde_json::json!({
            "trigger": { "taskId": "task-1" },
            "nodes": { "a": { "output": { "summary": "done" } } }
        });

        assert_eq!(
            node_handlers::render_template(
                "任务 ${trigger.taskId}: ${nodes.a.output.summary}",
                &value
            ),
            "任务 task-1: done"
        );
    }

    #[test]
    fn condition_routes_only_selected_handle() {
        let edges = vec![
            edge_with_handle("logic", "yes", "true"),
            edge_with_handle("logic", "no", "false"),
            edge("logic", "implicit"),
        ];
        let output = serde_json::json!({ "selectedHandle": "false" });
        let targets: BTreeSet<String> = active_outgoing_edges(&edges, "logic", &output)
            .into_iter()
            .map(|edge| edge.target.clone())
            .collect();

        assert!(targets.contains("no"));
        assert!(!targets.contains("yes"));
        assert!(!targets.contains("implicit"));
    }

    #[test]
    fn switch_routes_default_when_no_case_matches() {
        let edges = vec![
            edge_with_handle("switch", "matched", "done"),
            edge_with_handle("switch", "fallback", "default"),
        ];
        let output = serde_json::json!({
            "routeKind": "switch",
            "selectedHandle": "blocked",
        });
        let targets: BTreeSet<String> = active_outgoing_edges(&edges, "switch", &output)
            .into_iter()
            .map(|edge| edge.target.clone())
            .collect();

        assert!(targets.contains("fallback"));
        assert!(!targets.contains("matched"));
    }
}

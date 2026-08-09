use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use uuid::Uuid;

use crate::agent_timeline::{default_timeline_status, AgentTimelineEventInput};
use crate::automation::contract;
use crate::automation::signals::{emit_task_changed_signal, emit_todo_signal};
use crate::automation::types::{AutomationNode, AutomationRun};
use crate::chat::runner::spawn_agent_turn;
use crate::chat::state::{
    default_backend, new_chat_message_id, normalize_backend, queue_pending_turn_for_app,
    should_persist_user_message, ChatStore,
};
use crate::chat::timeline_sink::{persist_and_emit_input, persist_and_emit_message_timeline_event};
use crate::chat::types::{ChatComposerState, ChatMessage, ChatSendResult, ChatWorkflow};
use crate::provider::normalize_permission_mode;
use crate::task_contract;
use crate::todos::contract as todo_contract;
use crate::util::now_millis;

fn default_automation_tool_action() -> &'static str {
    contract::default_tool_action()
}

pub(crate) fn execute_node<R: Runtime>(
    app: &AppHandle<R>,
    chat_store: &ChatStore,
    conn: &Connection,
    run: &AutomationRun,
    node: &AutomationNode,
    input: &JsonValue,
) -> Result<JsonValue, String> {
    match node.kind.as_str() {
        "trigger" => Ok(serde_json::json!({ "triggered": true })),
        "logic" => execute_logic_node(node, input),
        "human" => execute_human_node(node, input),
        "tool" => execute_tool_node(app, conn, run, node, input),
        "agent" => execute_agent_node(app, chat_store, run, node, input),
        other => Err(format!("未知自动化节点类型：{other}")),
    }
}

fn execute_human_node(node: &AutomationNode, input: &JsonValue) -> Result<JsonValue, String> {
    let prompt = node
        .config
        .get("prompt")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| contract::default_human_prompt().to_string());
    Ok(serde_json::json!({
        "waitingUser": true,
        "prompt": render_template(&prompt, input),
    }))
}

fn execute_logic_node(node: &AutomationNode, input: &JsonValue) -> Result<JsonValue, String> {
    let logic = node
        .config
        .get("logic")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| contract::default_logic_kind().to_string());
    match logic.as_str() {
        "stop" => Ok(serde_json::json!({ "stopped": true })),
        "condition" => {
            let path = node
                .config
                .get("path")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| contract::default_logic_path().to_string());
            let expected = node.config.get("equals").and_then(|value| value.as_str());
            let actual = json_path(input, &path);
            let passed = match expected {
                Some(expected) => actual
                    .map(|value| json_value_to_string(value) == expected)
                    .unwrap_or(false),
                None => actual.map(json_value_truthy).unwrap_or(false),
            };
            Ok(serde_json::json!({
                "passed": passed,
                "selectedHandle": if passed { "true" } else { "false" },
            }))
        }
        "switch" => {
            let path = node
                .config
                .get("path")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| contract::default_logic_path().to_string());
            let value = json_path(input, &path).cloned().unwrap_or(JsonValue::Null);
            let selected_handle = json_value_to_port(&value);
            Ok(serde_json::json!({
                "value": value,
                "selectedHandle": selected_handle,
                "routeKind": "switch",
            }))
        }
        other => Err(format!("未知逻辑节点类型：{other}")),
    }
}

fn execute_tool_node<R: Runtime>(
    app: &AppHandle<R>,
    conn: &Connection,
    run: &AutomationRun,
    node: &AutomationNode,
    input: &JsonValue,
) -> Result<JsonValue, String> {
    let action = node
        .config
        .get("action")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| default_automation_tool_action().to_string());
    match action.as_str() {
        "create_task" => create_task_from_tool(app, conn, run, node, input),
        "update_task_status" => update_task_status_from_tool(app, conn, run, node, input),
        "add_todo" => add_todo_from_tool(app, conn, run, node, input),
        "send_guide" => send_guide_from_tool(app, conn, run, node, input),
        "record_timeline" => record_timeline_from_tool(app, run, node, input),
        other => Err(format!("未知工具节点动作：{other}")),
    }
}

fn create_task_from_tool<R: Runtime>(
    app: &AppHandle<R>,
    conn: &Connection,
    run: &AutomationRun,
    node: &AutomationNode,
    input: &JsonValue,
) -> Result<JsonValue, String> {
    let id = Uuid::new_v4().to_string();
    let title = render_template(
        node.config
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("自动化任务"),
        input,
    );
    let project_id = optional_rendered_string(node.config.get("projectId"), input)
        .or_else(|| run.trigger.project_id.clone());
    let status = optional_rendered_string(node.config.get("status"), input)
        .unwrap_or_else(|| contract::waiting_task_status().to_string());
    let now = now_millis();
    let sort_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM tasks WHERE project_id IS ?1",
            params![project_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("automation create_task: sort_order 失败：{e}"))?;
    conn.execute(
        r#"INSERT INTO tasks
           (id, project_id, session_id, title, title_source, status, created_at, parent_id, archived, sort_order, pinned)
           VALUES (?1, ?2, ?3, ?4, 'auto', ?5, ?6, NULL, 0, ?7, 0)"#,
        params![id, project_id, id, title, status, now, sort_order],
    )
    .map_err(|e| format!("automation create_task: insert 失败：{e}"))?;
    let _ = app.emit(
        task_contract::tasks_changed_event_name(),
        task_contract::tasks_changed_event_payload(project_id.as_deref()),
    );
    emit_task_changed_signal(
        app,
        project_id.clone(),
        Some(id.clone()),
        Some(status.clone()),
        contract::task_created_event_kind(),
        Some(run.id.clone()),
    );
    Ok(serde_json::json!({ "taskId": id, "projectId": project_id, "status": status }))
}

fn update_task_status_from_tool<R: Runtime>(
    app: &AppHandle<R>,
    conn: &Connection,
    run: &AutomationRun,
    node: &AutomationNode,
    input: &JsonValue,
) -> Result<JsonValue, String> {
    let task_id = optional_rendered_string(node.config.get("taskId"), input)
        .or_else(|| run.trigger.task_id.clone())
        .ok_or_else(|| "update_task_status 需要 taskId".to_string())?;
    let status = render_template(
        node.config
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| contract::waiting_task_status()),
        input,
    );
    conn.execute(
        "UPDATE tasks SET status = ?1 WHERE id = ?2",
        params![status, task_id],
    )
    .map_err(|e| format!("automation update_task_status: {e}"))?;
    let project_id = conn
        .query_row(
            "SELECT project_id FROM tasks WHERE id = ?1 AND archived = 0",
            params![task_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|e| format!("automation update_task_status: 查询项目失败：{e}"))?
        .flatten();
    let _ = app.emit(
        task_contract::tasks_changed_event_name(),
        task_contract::tasks_changed_event_payload(project_id.as_deref()),
    );
    emit_task_changed_signal(
        app,
        project_id.clone(),
        Some(task_id.clone()),
        Some(status.clone()),
        contract::task_status_changed_event_kind(),
        Some(run.id.clone()),
    );
    Ok(serde_json::json!({ "taskId": task_id, "projectId": project_id, "status": status }))
}

fn add_todo_from_tool<R: Runtime>(
    app: &AppHandle<R>,
    conn: &Connection,
    run: &AutomationRun,
    node: &AutomationNode,
    input: &JsonValue,
) -> Result<JsonValue, String> {
    let task_id = optional_rendered_string(node.config.get("taskId"), input)
        .or_else(|| run.trigger.task_id.clone())
        .ok_or_else(|| "add_todo 需要 taskId".to_string())?;
    let text = render_template(
        node.config
            .get("text")
            .and_then(|value| value.as_str())
            .unwrap_or("自动化 Todo"),
        input,
    );
    let id = Uuid::new_v4().to_string();
    let now = now_millis();
    insert_task_todo_row(
        conn,
        &id,
        &task_id,
        &text,
        "agent",
        todo_contract::default_priority(),
        None,
        now,
    )?;
    let _ = app.emit(
        todo_contract::changed_event_name(),
        todo_contract::changed_event_payload(&task_id),
    );
    emit_todo_signal(app, task_id.clone(), Some(run.id.clone()));
    Ok(serde_json::json!({ "todoId": id, "taskId": task_id }))
}

fn send_guide_from_tool<R: Runtime>(
    app: &AppHandle<R>,
    conn: &Connection,
    run: &AutomationRun,
    node: &AutomationNode,
    input: &JsonValue,
) -> Result<JsonValue, String> {
    let task_id = optional_rendered_string(node.config.get("taskId"), input)
        .or_else(|| run.trigger.task_id.clone())
        .ok_or_else(|| "send_guide 需要 taskId".to_string())?;
    let text = render_template(
        node.config
            .get("text")
            .or_else(|| node.config.get("title"))
            .and_then(|value| value.as_str())
            .unwrap_or("自动化引导"),
        input,
    );
    let priority_value = node
        .config
        .get("priority")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| todo_contract::default_priority().to_string());
    let priority = normalize_task_priority(&priority_value);
    let id = Uuid::new_v4().to_string();
    let now = now_millis();
    insert_task_todo_row(
        conn,
        &id,
        &task_id,
        &text,
        "lilia",
        &priority,
        Some(todo_contract::pending_guide_status()),
        now,
    )?;
    let _ = app.emit(
        todo_contract::changed_event_name(),
        todo_contract::changed_event_payload(&task_id),
    );
    emit_todo_signal(app, task_id.clone(), Some(run.id.clone()));
    Ok(serde_json::json!({
        "guideId": id,
        "taskId": task_id,
        "priority": priority,
        "guideStatus": todo_contract::pending_guide_status(),
    }))
}

pub(crate) fn insert_task_todo_row(
    conn: &Connection,
    id: &str,
    task_id: &str,
    text: &str,
    source: &str,
    priority: &str,
    guide_status: Option<&str>,
    now: i64,
) -> Result<(), String> {
    let order: i64 = conn
        .query_row(
            r#"SELECT COALESCE(MAX("order"), -1) + 1 FROM task_todos WHERE task_id = ?1"#,
            params![task_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("automation todo: order 失败：{e}"))?;
    conn.execute(
        r#"INSERT INTO task_todos
           (id, task_id, text, done, "order", source, priority, guide_status, attachments_json, created_at, updated_at)
           VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7, '[]', ?8, ?9)"#,
        params![id, task_id, text, order, source, priority, guide_status, now, now],
    )
    .map_err(|e| format!("automation todo: insert 失败：{e}"))?;
    Ok(())
}

pub(crate) fn normalize_task_priority(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    if todo_contract::priorities()
        .iter()
        .any(|priority| priority == &normalized)
    {
        return normalized;
    }
    todo_contract::default_priority().to_string()
}

fn record_timeline_from_tool<R: Runtime>(
    app: &AppHandle<R>,
    run: &AutomationRun,
    node: &AutomationNode,
    input: &JsonValue,
) -> Result<JsonValue, String> {
    let timeline_input = timeline_input_from_tool(run, node, input)?;
    let event_id = timeline_input.id.clone();
    persist_and_emit_input(app, timeline_input);
    Ok(serde_json::json!({
        "recorded": true,
        "timelineEventId": event_id,
        "automationRunId": run.id,
    }))
}

pub(crate) fn timeline_input_from_tool(
    run: &AutomationRun,
    node: &AutomationNode,
    input: &JsonValue,
) -> Result<AgentTimelineEventInput, String> {
    let task_id = optional_rendered_string(node.config.get("taskId"), input)
        .or_else(|| run.trigger.task_id.clone())
        .ok_or_else(|| "record_timeline 需要 taskId".to_string())?;
    let title = render_template(
        node.config
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("自动化记录"),
        input,
    );
    let summary =
        optional_rendered_string(node.config.get("summary"), input).or(Some(title.clone()));
    let status = optional_rendered_string(node.config.get("status"), input)
        .unwrap_or_else(|| default_timeline_status().to_string());
    let backend = optional_rendered_string(node.config.get("backend"), input)
        .or_else(|| run.trigger.backend.clone());
    let backend = normalize_optional_backend(backend);
    let now = now_millis();
    Ok(AgentTimelineEventInput {
        id: Some(format!("automation:{}:{}", run.id, node.id)),
        task_id,
        turn_id: None,
        backend,
        kind: "automation".to_string(),
        status,
        title,
        summary,
        payload: serde_json::json!({
            "automationRunId": run.id,
            "workflowId": run.workflow_id,
            "workflowVersionId": run.workflow_version_id,
            "nodeId": node.id,
        }),
        created_at: Some(now),
        updated_at: Some(now),
    })
}

fn execute_agent_node<R: Runtime>(
    app: &AppHandle<R>,
    chat_store: &ChatStore,
    run: &AutomationRun,
    node: &AutomationNode,
    input: &JsonValue,
) -> Result<JsonValue, String> {
    // #49 / DoD: Automation Agent nodes must use AgentKit/Native — never Claude/Codex
    // official Server or Node agent-runner for new tasks.
    crate::native_agent::require_native_for_automation_or_multi_agent("Automation Agent 节点")?;

    let create_task = node
        .config
        .get("createTask")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let task_id = if create_task {
        create_agent_target_task(app, run, node, input)?
    } else {
        optional_rendered_string(node.config.get("taskId"), input)
            .or_else(|| run.trigger.task_id.clone())
            .ok_or_else(|| "Agent 节点需要 taskId".to_string())?
    };
    let content = render_template(
        &node
            .config
            .get("prompt")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| contract::default_agent_prompt().to_string()),
        input,
    );
    // Product backend labels (claude/codex) remain scope filters only; execution is Native.
    let configured = node
        .config
        .get("backend")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(backend) = configured {
        if matches!(
            backend,
            "claude" | "codex" | "node" | "node-agent-runner" | "agent-runner"
        ) {
            return Err(format!(
                "Automation Agent 节点禁止直调品牌后端 `{backend}`；须走 AgentKit/Native（backend=native-agentkit）"
            ));
        }
    }
    let backend = crate::native_agent::BACKEND_NATIVE_AGENTKIT.to_string();
    let model = node
        .config
        .get("model")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("native-default")
        .to_string();
    let permission = normalize_permission_mode(
        node.config
            .get("permission")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
    );
    let project_cwd = node
        .config
        .get("projectCwd")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let composer = ChatComposerState {
        task_id: task_id.clone(),
        backend,
        model,
        model_selection_mode: "auto".to_string(),
        reasoning_effort: None,
        plan_mode: false,
        goal_mode: false,
        permission,
        codex_settings: Default::default(),
    };
    let result = dispatch_agent_turn(
        app,
        chat_store,
        run.id.clone(),
        task_id.clone(),
        content,
        composer,
        project_cwd,
    )?;
    Ok(serde_json::json!({
        "waitingAgent": true,
        "taskId": task_id,
        "turnId": result.turn_id,
        "dispatch": result.dispatch,
        "queuedCount": result.queued_count,
        "messageId": result.message.id,
        "executionBackend": crate::native_agent::BACKEND_NATIVE_AGENTKIT,
    }))
}

fn create_agent_target_task<R: Runtime>(
    app: &AppHandle<R>,
    run: &AutomationRun,
    node: &AutomationNode,
    input: &JsonValue,
) -> Result<String, String> {
    let id = Uuid::new_v4().to_string();
    let title = render_template(
        node.config
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("自动化 Agent 任务"),
        input,
    );
    let project_id = optional_rendered_string(node.config.get("projectId"), input)
        .or_else(|| run.trigger.project_id.clone());
    let task_id = lilia_contracts::TaskId::new(id.clone()).map_err(|error| error.to_string())?;
    let product_project_id = project_id
        .as_deref()
        .map(lilia_contracts::ProjectId::new)
        .transpose()
        .map_err(|error| error.to_string())?;
    app.state::<crate::product_core::EmbeddedProductCore>()
        .create_task_with_conversation(task_id, product_project_id, &title)
        .map_err(|error| format!("automation agent create product task: {error}"))?;
    let _ = app.emit(
        task_contract::tasks_changed_event_name(),
        task_contract::tasks_changed_event_payload(project_id.as_deref()),
    );
    emit_task_changed_signal(
        app,
        project_id,
        Some(id.clone()),
        Some(contract::running_task_status().to_string()),
        contract::task_created_event_kind(),
        Some(run.id.clone()),
    );
    Ok(id)
}

fn dispatch_agent_turn<R: Runtime>(
    app: &AppHandle<R>,
    store: &ChatStore,
    automation_run_id: String,
    task_id: String,
    content: String,
    mut composer: ChatComposerState,
    project_cwd: String,
) -> Result<ChatSendResult, String> {
    crate::native_agent::require_native_for_automation_or_multi_agent("Automation 多 Agent 路径")?;
    // Keep Native backend identity — do not run through chat-backends.json normalize
    // (that manifest still lists only claude/codex product labels).
    composer.task_id = task_id.clone();
    composer.backend = crate::native_agent::BACKEND_NATIVE_AGENTKIT.to_string();
    composer.permission = normalize_permission_mode(&composer.permission);
    let workflow = Some(ChatWorkflow::Automation {
        automation_run_id: automation_run_id.clone(),
    });
    let message = ChatMessage {
        id: new_chat_message_id(),
        task_id: task_id.clone(),
        role: "user".to_string(),
        content: content.clone(),
        attachments: Vec::new(),
        conversation_references: Vec::new(),
        created_at: now_millis() as u64,
    };
    let turn_id = format!("turn-{}", now_millis());
    store
        .composers
        .lock()
        .unwrap()
        .insert(task_id.clone(), composer.clone());

    {
        let mut running = store.running_tasks.lock().unwrap();
        if running.contains_key(&task_id) {
            drop(running);
            let queued_count = queue_pending_turn_for_app(
                app,
                store,
                &task_id,
                content,
                composer.clone(),
                project_cwd,
                Vec::new(),
                Vec::new(),
                workflow.clone(),
                None,
                None,
                message.clone(),
                turn_id.clone(),
                None,
            );
            if should_persist_user_message(&message.content, &workflow, &None) {
                persist_and_emit_message_timeline_event(
                    app,
                    &message,
                    &composer.backend,
                    &turn_id,
                    true,
                    Some(&automation_run_id),
                );
            }
            return Ok(ChatSendResult {
                message,
                dispatch: "queued".to_string(),
                queued_count,
                turn_id,
            });
        }
        running.insert(task_id.clone(), true);
    }

    if should_persist_user_message(&message.content, &workflow, &None) {
        persist_and_emit_message_timeline_event(
            app,
            &message,
            &composer.backend,
            &turn_id,
            false,
            Some(&automation_run_id),
        );
    }
    spawn_agent_turn(
        app.clone(),
        task_id,
        content,
        composer,
        project_cwd,
        Vec::new(),
        Vec::new(),
        workflow,
        None,
        None,
        turn_id.clone(),
    );
    Ok(ChatSendResult {
        message,
        dispatch: "started".to_string(),
        queued_count: 0,
        turn_id,
    })
}

fn optional_rendered_string(value: Option<&JsonValue>, input: &JsonValue) -> Option<String> {
    value
        .and_then(|value| value.as_str())
        .map(|value| render_template(value, input))
        .filter(|value| !value.trim().is_empty())
}

fn normalize_optional_backend(value: Option<String>) -> String {
    value
        .as_deref()
        .map(normalize_backend)
        .unwrap_or_else(default_backend)
        .to_string()
}

pub(crate) fn render_template(template: &str, input: &JsonValue) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        let (prefix, tail) = rest.split_at(start);
        out.push_str(prefix);
        let Some(end) = tail.find('}') else {
            out.push_str(tail);
            return out;
        };
        let path = &tail[2..end];
        if let Some(value) = json_path(input, path) {
            out.push_str(&json_value_to_string(value));
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

fn json_path<'a>(value: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    let mut current = value;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

fn json_value_to_string(value: &JsonValue) -> String {
    value
        .as_str()
        .map(|value| value.to_string())
        .unwrap_or_else(|| value.to_string())
}

fn json_value_to_port(value: &JsonValue) -> String {
    json_value_to_string(value)
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn json_value_truthy(value: &JsonValue) -> bool {
    match value {
        JsonValue::Bool(value) => *value,
        JsonValue::Number(value) => value.as_i64().unwrap_or(0) != 0,
        JsonValue::String(value) => !value.trim().is_empty() && value != "false",
        JsonValue::Array(value) => !value.is_empty(),
        JsonValue::Object(value) => !value.is_empty(),
        JsonValue::Null => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_backend_defaults_from_contract_manifest() {
        assert_eq!(normalize_optional_backend(None), default_backend());
        assert_eq!(
            normalize_optional_backend(Some("unknown".to_string())),
            default_backend()
        );
        assert_eq!(
            normalize_optional_backend(Some("native-agentkit".to_string())),
            "native-agentkit"
        );
        // Brand backends are not product chat backends; fall back to default.
        assert_eq!(
            normalize_optional_backend(Some("codex".to_string())),
            default_backend()
        );
    }

    #[test]
    fn automation_agent_refuses_brand_backends_for_new_tasks() {
        let previous = std::env::var("LILIA_AGENT_EXECUTION_BACKEND").ok();
        std::env::remove_var("LILIA_AGENT_EXECUTION_BACKEND");
        assert!(
            crate::native_agent::require_native_for_automation_or_multi_agent(
                "Automation Agent 节点"
            )
            .is_ok()
        );

        for brand in ["claude", "codex", "node", "node-agent-runner"] {
            let err = format!(
                "Automation Agent 节点禁止直调品牌后端 `{brand}`；须走 AgentKit/Native（backend=native-agentkit）"
            );
            assert!(err.contains(brand));
            assert!(err.contains("AgentKit/Native"));
        }

        std::env::set_var("LILIA_AGENT_EXECUTION_BACKEND", "node");
        assert!(
            crate::native_agent::require_native_for_automation_or_multi_agent(
                "Automation 多 Agent 路径"
            )
            .is_ok(),
            "default builds ignore the unavailable legacy backend and remain Native"
        );
        match previous {
            Some(value) => std::env::set_var("LILIA_AGENT_EXECUTION_BACKEND", value),
            None => std::env::remove_var("LILIA_AGENT_EXECUTION_BACKEND"),
        }
    }

    #[test]
    fn automation_tool_defaults_come_from_contract_manifests() {
        assert_eq!(contract::waiting_task_status(), "waiting");
        assert_eq!(contract::running_task_status(), "running");
        assert_eq!(default_timeline_status(), "info");
        assert_eq!(todo_contract::default_priority(), "normal");
        assert_eq!(todo_contract::pending_guide_status(), "pending");
        assert_eq!(normalize_task_priority(" HIGH "), "high");
        assert_eq!(
            normalize_task_priority("urgent"),
            todo_contract::default_priority()
        );
    }
}

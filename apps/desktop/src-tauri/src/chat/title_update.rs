//! Auto-title after Native turn complete (Assistant AI + timeline UI cache samples).

use std::thread;
use std::time::Duration;

use reqwest::blocking::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Manager, Runtime};
use uuid::Uuid;

use crate::agent_timeline::{self, AgentTimelineEventInput};
use crate::agent_timeline_contract;
use crate::chat::timeline_sink::persist_and_emit_input;
use crate::native_agent::BACKEND_NATIVE_AGENTKIT;
use crate::projects_tasks::events::emit_tasks_changed;
use crate::prompt_contract;
use crate::provider::{
    assistant_ai_secret, load_assistant_ai_config, load_model_feature_settings, AssistantAIConfig,
};
use crate::store::LiliaStore;

const TITLE_LABEL: &str = "标题已更新";
const TITLE_MAX_CHARS: usize = 18;
const TITLE_MIN_CHARS: usize = 2;
const SAMPLE_TEXT_LIMIT: usize = 260;

#[derive(Debug, Clone)]
struct TaskTitleState {
    id: String,
    project_id: Option<String>,
    title: String,
    title_source: String,
}

/// Spawn background auto-title generation for a finished Native turn.
pub(crate) fn spawn_title_update<R: Runtime>(
    app: AppHandle<R>,
    task_id: String,
    turn_id: Option<String>,
) {
    thread::spawn(move || {
        if let Err(err) = run_title_update(&app, &task_id, turn_id.as_deref()) {
            eprintln!("[title-update] skipped: {err}");
        }
    });
}

fn run_title_update<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
    turn_id: Option<&str>,
) -> Result<(), String> {
    let store = app
        .try_state::<LiliaStore>()
        .ok_or_else(|| "store unavailable".to_string())?;
    let conn = store.conn()?;
    let task =
        load_task_title_state(&conn, task_id)?.ok_or_else(|| "task not found".to_string())?;
    let prompt = build_title_prompt(&conn, task_id, &task.title)?;
    let Some(prompt) = prompt else {
        return Ok(());
    };
    let model = assistant_ai_model_request(app)
        .ok_or_else(|| "assistant AI model unavailable for title".to_string())?;
    let proposed = request_title(&model, &prompt).and_then(normalize_title)?;
    if proposed == compact_line(&task.title) {
        return Ok(());
    }

    if task.title_source == "manual" {
        persist_title_event(
            app,
            &task,
            turn_id,
            "requires_action",
            &proposed,
            true,
        )?;
    } else {
        conn.execute(
            "UPDATE tasks SET title = ?1, title_source = 'auto' WHERE id = ?2 AND archived = 0",
            params![proposed.as_str(), task.id.as_str()],
        )
        .map_err(|e| format!("update auto title failed: {e}"))?;
        emit_tasks_changed(app, task.project_id.clone());
        persist_title_event(app, &task, turn_id, "success", &proposed, false)?;
    }
    Ok(())
}

/// Accept or decline a previously proposed title-update interaction.
#[tauri::command]
pub fn chat_respond_title_update(
    app: AppHandle,
    task_id: String,
    request_id: String,
    decision: String,
) -> Result<(), String> {
    let store = app.state::<LiliaStore>();
    let conn = store.conn()?;
    let event_id = title_event_id(&task_id, &request_id);
    let event = agent_timeline::list(&conn, &task_id)?
        .into_iter()
        .find(|event| event.id == event_id)
        .ok_or_else(|| "标题更新请求已失效".to_string())?;
    if event.kind != agent_timeline_contract::title_update_action_kind()
        || event.status != "requires_action"
    {
        return Ok(());
    }
    let payload = read_payload_record(&event.payload);
    let proposed = payload
        .get("proposedTitle")
        .and_then(|value| value.as_str())
        .and_then(|title| normalize_title(title.to_string()).ok())
        .ok_or_else(|| "标题更新请求缺少候选标题".to_string())?;
    let task = load_task_title_state(&conn, &task_id)?.ok_or_else(|| "任务不存在".to_string())?;
    let accepted = decision == "accept";
    if accepted {
        conn.execute(
            "UPDATE tasks SET title = ?1, title_source = 'manual' WHERE id = ?2 AND archived = 0",
            params![proposed.as_str(), task_id.as_str()],
        )
        .map_err(|e| format!("accept title update failed: {e}"))?;
        emit_tasks_changed(&app, task.project_id.clone());
    }

    let status = if accepted { "success" } else { "skipped" };
    let mut next_payload = payload;
    next_payload.insert("accepted".to_string(), JsonValue::Bool(accepted));
    next_payload.insert("decision".to_string(), JsonValue::String(decision));
    persist_and_emit_input(
        &app,
        AgentTimelineEventInput {
            id: Some(event_id),
            task_id,
            turn_id: event.turn_id,
            backend: event.backend,
            kind: agent_timeline_contract::title_update_action_kind().to_string(),
            status: status.to_string(),
            title: TITLE_LABEL.to_string(),
            summary: Some(proposed),
            payload: JsonValue::Object(next_payload.into_iter().collect()),
            created_at: Some(event.created_at),
            updated_at: None,
        },
    );
    Ok(())
}

fn load_task_title_state(
    conn: &Connection,
    task_id: &str,
) -> Result<Option<TaskTitleState>, String> {
    conn.query_row(
        "SELECT id, project_id, title, title_source FROM tasks WHERE id = ?1 AND archived = 0",
        params![task_id],
        |row| {
            Ok(TaskTitleState {
                id: row.get(0)?,
                project_id: row.get(1)?,
                title: row.get(2)?,
                title_source: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("load task title failed: {e}"))
}

fn build_title_prompt(
    conn: &Connection,
    task_id: &str,
    current_title: &str,
) -> Result<Option<String>, String> {
    let samples = load_timeline_samples(conn, task_id)?;
    if samples.is_empty() {
        return Ok(None);
    }
    let mut lines = vec![
        "你是 LiliaCode 的对话标题助手。基于下方最近对话内容生成一个新的中文短标题。".to_string(),
        "只输出标题本身，不要引号、解释、Markdown 或标点包装。".to_string(),
        "标题应概括当前真实任务方向或根因，6 到 18 个中文字，避免“帮我”“请你”等泛词。".to_string(),
        format!(
            "当前标题: {}",
            truncate_chars(&compact_line(current_title), 80)
        ),
    ];
    lines.extend(samples);
    Ok(Some(lines.join("\n")))
}

fn load_timeline_samples(conn: &Connection, task_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT kind, title, summary, payload
               FROM agent_timeline_events
               WHERE task_id = ?1
                 AND kind IN ('message','todo_list','error')
               ORDER BY turn_seq DESC, intra_turn_order DESC
               LIMIT 16"#,
        )
        .map_err(|e| format!("prepare title samples failed: {e}"))?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| format!("query title samples failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let (kind, title, summary, payload_text) =
            row.map_err(|e| format!("read title sample failed: {e}"))?;
        let payload = serde_json::from_str::<JsonValue>(&payload_text).unwrap_or(JsonValue::Null);
        let label = if kind == "message" {
            match payload.get("role").and_then(|value| value.as_str()) {
                Some("assistant") => "助手",
                Some("user") => "用户",
                Some("system") => "系统",
                _ => "消息",
            }
        } else if kind == "todo_list" {
            "待办"
        } else {
            "错误"
        };
        let text = payload
            .get("content")
            .and_then(|value| value.as_str())
            .or(summary.as_deref())
            .unwrap_or(title.as_str());
        let text = truncate_chars(&compact_line(text), SAMPLE_TEXT_LIMIT);
        if !text.is_empty() {
            out.push(format!("{label}: {text}"));
        }
    }
    out.reverse();
    Ok(out)
}

fn assistant_ai_model_request<R: Runtime>(app: &AppHandle<R>) -> Option<AssistantAIConfig> {
    let mut cfg = load_assistant_ai_config(app);
    cfg.base_url = cfg
        .base_url
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty());
    cfg.model = load_model_feature_settings(app)
        .title
        .or(cfg.model)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    cfg.api_key = assistant_ai_secret().ok().flatten();
    if cfg.base_url.is_none() || cfg.model.is_none() || cfg.api_key.is_none() {
        return None;
    }
    Some(cfg)
}

fn request_title(model: &AssistantAIConfig, prompt: &str) -> Result<String, String> {
    let base_url = model
        .base_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let url = format!("{base_url}/chat/completions");
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client failed: {e}"))?;
    let resp = client
        .post(url)
        .bearer_auth(model.api_key.as_deref().unwrap_or(""))
        .json(&json!({
            "model": model.model,
            "messages": [
                { "role": "system", "content": prompt_contract::title_system_instruction() },
                { "role": "user", "content": prompt }
            ],
            "temperature": 0.2,
            "max_tokens": 80
        }))
        .send()
        .map_err(|e| format!("title request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("title request HTTP {}", resp.status()));
    }
    let value = resp
        .json::<JsonValue>()
        .map_err(|e| format!("title response parse failed: {e}"))?;
    value
        .get("choices")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| "title response missing content".to_string())
}

fn persist_title_event<R: Runtime>(
    app: &AppHandle<R>,
    task: &TaskTitleState,
    turn_id: Option<&str>,
    status: &str,
    proposed: &str,
    requires_action: bool,
) -> Result<(), String> {
    let request_id = requires_action.then(|| Uuid::new_v4().to_string());
    let id = request_id
        .as_ref()
        .map(|request_id| title_event_id(&task.id, request_id));
    persist_and_emit_input(
        app,
        AgentTimelineEventInput {
            id,
            task_id: task.id.clone(),
            turn_id: turn_id.map(str::to_string),
            backend: BACKEND_NATIVE_AGENTKIT.to_string(),
            kind: agent_timeline_contract::title_update_action_kind().to_string(),
            status: status.to_string(),
            title: TITLE_LABEL.to_string(),
            summary: Some(proposed.to_string()),
            payload: json!({
                "proposedTitle": proposed,
                "previousTitle": task.title,
                "source": if requires_action { "manual-blocked" } else { "auto" },
                "requestId": request_id,
                "accepted": if requires_action { JsonValue::Null } else { JsonValue::Bool(true) },
            }),
            created_at: None,
            updated_at: None,
        },
    );
    Ok(())
}

fn title_event_id(task_id: &str, request_id: &str) -> String {
    format!("title-update:{task_id}:{request_id}")
}

fn read_payload_record(value: &JsonValue) -> serde_json::Map<String, JsonValue> {
    value
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new)
}

fn normalize_title(raw: String) -> Result<String, String> {
    let mut title = raw
        .trim()
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('“')
        .trim_matches('”')
        .trim_matches('《')
        .trim_matches('》')
        .trim()
        .to_string();
    if let Some(stripped) = title
        .strip_prefix("标题：")
        .or_else(|| title.strip_prefix("标题:"))
    {
        title = stripped.trim().to_string();
    }
    title = title
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('“')
        .trim_matches('”')
        .trim_matches('《')
        .trim_matches('》')
        .trim()
        .to_string();
    title = compact_line(&title)
        .trim_end_matches(['。', '.', '，', ',', '；', ';', '：', ':'])
        .to_string();
    title = truncate_chars(&title, TITLE_MAX_CHARS);
    let len = title.chars().count();
    if len < TITLE_MIN_CHARS {
        return Err("title too short".to_string());
    }
    Ok(title)
}

fn compact_line(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(input: &str, max: usize) -> String {
    let mut out = String::new();
    for (index, ch) in input.chars().enumerate() {
        if index >= max {
            return out;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_title_strips_wrappers_and_limits_length() {
        assert_eq!(
            normalize_title("标题：`对话标题事件化实现进度需要继续确认更多内容`".to_string())
                .unwrap(),
            "对话标题事件化实现进度需要继续确认更"
        );
        assert!(normalize_title(" ".to_string()).is_err());
    }
}

//! Auto-title after Native turn complete (Assistant AI + timeline UI cache samples).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use reqwest::blocking::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::Semaphore;
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
const TITLE_REVIEW_LABEL: &str = "建议更新标题";
const TITLE_SKIPPED_LABEL: &str = "标题更新已跳过";
const TITLE_MAX_CHARS: usize = 18;
const TITLE_MIN_CHARS: usize = 2;
const SAMPLE_TEXT_LIMIT: usize = 260;
const TITLE_UPDATE_CONCURRENCY: usize = 2;

#[derive(Debug, Clone)]
struct TaskTitleState {
    id: String,
    project_id: Option<String>,
    title: String,
    title_source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TimelineUpperBound {
    turn_seq: i64,
    intra_turn_order: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TitleJobVersion {
    upper_bound: TimelineUpperBound,
    generation: u64,
}

#[derive(Debug, Clone)]
struct TitleUpdateJob {
    task: TaskTitleState,
    turn_id: Option<String>,
    version: TitleJobVersion,
}

#[derive(Debug, Clone)]
enum TitleUpdateDecision {
    Success(TaskTitleState),
    RequiresAction(TaskTitleState),
    Stale(TaskTitleState),
    Unchanged,
    Stopped,
}

#[derive(Default)]
struct TitleGenerationState {
    next_generation: u64,
    latest_by_task: HashMap<String, TitleJobVersion>,
}

struct TitleUpdateCoordinatorInner {
    generations: Mutex<TitleGenerationState>,
    emissions: Mutex<()>,
    stopped: AtomicBool,
    lanes: Arc<Semaphore>,
    client: OnceLock<Result<Client, String>>,
}

#[derive(Clone)]
pub(crate) struct TitleUpdateCoordinator {
    inner: Arc<TitleUpdateCoordinatorInner>,
}

impl Default for TitleUpdateCoordinator {
    fn default() -> Self {
        Self {
            inner: Arc::new(TitleUpdateCoordinatorInner {
                generations: Mutex::new(TitleGenerationState::default()),
                emissions: Mutex::new(()),
                stopped: AtomicBool::new(false),
                lanes: Arc::new(Semaphore::new(TITLE_UPDATE_CONCURRENCY)),
                client: OnceLock::new(),
            }),
        }
    }
}

impl TitleUpdateCoordinator {
    fn schedule(
        &self,
        conn: &Connection,
        task_id: &str,
        turn_id: Option<String>,
        upper_bound: TimelineUpperBound,
    ) -> Result<Option<TitleUpdateJob>, String> {
        let mut state = self
            .inner
            .generations
            .lock()
            .map_err(|_| "title generation state lock poisoned".to_string())?;
        if self.inner.stopped.load(Ordering::Acquire) {
            return Ok(None);
        }
        let Some(task) = load_task_title_state(conn, task_id)? else {
            return Ok(None);
        };
        state.next_generation = state
            .next_generation
            .checked_add(1)
            .ok_or_else(|| "title generation exhausted".to_string())?;
        let version = TitleJobVersion {
            upper_bound,
            generation: state.next_generation,
        };
        if state
            .latest_by_task
            .get(&task.id)
            .is_some_and(|current| current.upper_bound > upper_bound)
        {
            return Ok(Some(TitleUpdateJob {
                task,
                turn_id,
                version,
            }));
        }
        state.latest_by_task.insert(task.id.clone(), version);
        Ok(Some(TitleUpdateJob {
            task,
            turn_id,
            version,
        }))
    }

    fn is_latest(&self, job: &TitleUpdateJob) -> bool {
        if self.inner.stopped.load(Ordering::Acquire) {
            return false;
        }
        self.inner
            .generations
            .lock()
            .ok()
            .is_some_and(|state| state.latest_by_task.get(&job.task.id) == Some(&job.version))
    }

    fn lanes(&self) -> Arc<Semaphore> {
        Arc::clone(&self.inner.lanes)
    }

    fn http_client(&self) -> Result<Client, String> {
        self.inner
            .client
            .get_or_init(|| {
                Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                    .map_err(|e| format!("HTTP client failed: {e}"))
            })
            .clone()
    }

    fn decide_proposal(
        &self,
        conn: &Connection,
        job: &TitleUpdateJob,
        proposed: &str,
    ) -> Result<TitleUpdateDecision, String> {
        let state = self
            .inner
            .generations
            .lock()
            .map_err(|_| "title generation state lock poisoned".to_string())?;
        if self.inner.stopped.load(Ordering::Acquire) {
            return Ok(TitleUpdateDecision::Stopped);
        }
        if state.latest_by_task.get(&job.task.id) != Some(&job.version) {
            return Ok(TitleUpdateDecision::Stale(job.task.clone()));
        }

        let Some(current) = load_task_title_state(conn, &job.task.id)? else {
            return Ok(TitleUpdateDecision::Stale(job.task.clone()));
        };
        if current.title_source == "manual" {
            return Ok(TitleUpdateDecision::RequiresAction(current));
        }
        if current.title != job.task.title || current.title_source != job.task.title_source {
            return Ok(TitleUpdateDecision::Stale(current));
        }
        if proposed == compact_line(&current.title) {
            return Ok(TitleUpdateDecision::Unchanged);
        }

        let updated = conn
            .execute(
                r#"UPDATE tasks
                   SET title = ?1, title_source = 'auto'
                   WHERE id = ?2
                     AND archived = 0
                     AND title_source = 'auto'
                     AND title = ?3"#,
                params![proposed, job.task.id.as_str(), job.task.title.as_str()],
            )
            .map_err(|e| format!("update auto title failed: {e}"))?;
        if updated == 1 {
            return Ok(TitleUpdateDecision::Success(current));
        }
        let current = load_task_title_state(conn, &job.task.id)?.unwrap_or(current);
        Ok(if current.title_source == "manual" {
            TitleUpdateDecision::RequiresAction(current)
        } else {
            TitleUpdateDecision::Stale(current)
        })
    }

    fn while_running(&self, action: impl FnOnce()) {
        let Ok(_emission) = self.inner.emissions.lock() else {
            return;
        };
        if self.inner.stopped.load(Ordering::Acquire) {
            return;
        }
        action();
    }

    pub(crate) fn shutdown(&self) {
        self.inner.stopped.store(true, Ordering::Release);
        self.inner.lanes.close();
        if let Ok(mut state) = self.inner.generations.lock() {
            state.latest_by_task.clear();
        }
        if let Ok(emission) = self.inner.emissions.lock() {
            drop(emission);
        }
    }
}

/// Spawn background auto-title generation for a finished Native turn.
pub(crate) fn spawn_title_update<R: Runtime>(
    app: AppHandle<R>,
    task_id: String,
    turn_id: Option<String>,
) {
    let Some(coordinator) = app
        .try_state::<TitleUpdateCoordinator>()
        .map(|state| state.inner().clone())
    else {
        eprintln!("[title-update] skipped: coordinator unavailable");
        return;
    };
    let job = (|| {
        let store = app
            .try_state::<LiliaStore>()
            .ok_or_else(|| "store unavailable".to_string())?;
        let conn = store.conn()?;
        prepare_title_job(&conn, &coordinator, &task_id, turn_id)
    })();
    let job = match job {
        Ok(Some(job)) => job,
        Ok(None) => return,
        Err(err) => {
            eprintln!("[title-update] skipped: {err}");
            return;
        }
    };
    let lanes = coordinator.lanes();
    tauri::async_runtime::spawn(async move {
        let Ok(_permit) = lanes.acquire_owned().await else {
            return;
        };
        let result = tauri::async_runtime::spawn_blocking(move || {
            run_title_update(&app, &coordinator, &job)
        })
        .await;
        match result {
            Ok(Err(err)) => eprintln!("[title-update] skipped: {err}"),
            Err(err) => eprintln!("[title-update] worker failed: {err}"),
            Ok(Ok(())) => {}
        }
    });
}

fn prepare_title_job(
    conn: &Connection,
    coordinator: &TitleUpdateCoordinator,
    task_id: &str,
    turn_id: Option<String>,
) -> Result<Option<TitleUpdateJob>, String> {
    let upper_bound = load_timeline_upper_bound(conn, task_id, turn_id.as_deref())?;
    match upper_bound {
        Some(upper_bound) => coordinator.schedule(conn, task_id, turn_id, upper_bound),
        None => Ok(None),
    }
}

fn run_title_update<R: Runtime>(
    app: &AppHandle<R>,
    coordinator: &TitleUpdateCoordinator,
    job: &TitleUpdateJob,
) -> Result<(), String> {
    if !coordinator.is_latest(job) {
        record_title_decision(
            app,
            coordinator,
            job,
            TitleUpdateDecision::Stale(job.task.clone()),
            None,
        );
        return Ok(());
    }
    let store = app
        .try_state::<LiliaStore>()
        .ok_or_else(|| "store unavailable".to_string())?;
    let prompt = {
        let conn = store.conn()?;
        build_title_prompt(
            &conn,
            &job.task.id,
            &job.task.title,
            job.version.upper_bound,
        )?
    };
    let Some(prompt) = prompt else {
        return Ok(());
    };
    let model = assistant_ai_model_request(app)
        .ok_or_else(|| "assistant AI model unavailable for title".to_string())?;
    let client = coordinator.http_client()?;
    let proposed = request_title(&client, &model, &prompt).and_then(normalize_title)?;
    let decision = {
        let conn = store.conn()?;
        coordinator.decide_proposal(&conn, job, &proposed)?
    };
    record_title_decision(app, coordinator, job, decision, Some(&proposed));
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
            title: title_event_label(status).to_string(),
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

fn load_timeline_upper_bound(
    conn: &Connection,
    task_id: &str,
    turn_id: Option<&str>,
) -> Result<Option<TimelineUpperBound>, String> {
    conn.query_row(
        r#"SELECT turn_seq, intra_turn_order
           FROM agent_timeline_events
           WHERE task_id = ?1 AND (?2 IS NULL OR turn_id = ?2)
           ORDER BY turn_seq DESC, intra_turn_order DESC
           LIMIT 1"#,
        params![task_id, turn_id],
        |row| {
            Ok(TimelineUpperBound {
                turn_seq: row.get(0)?,
                intra_turn_order: row.get(1)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("load title timeline upper bound failed: {e}"))
}

fn build_title_prompt(
    conn: &Connection,
    task_id: &str,
    current_title: &str,
    upper_bound: TimelineUpperBound,
) -> Result<Option<String>, String> {
    let samples = load_timeline_samples(conn, task_id, upper_bound)?;
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

fn load_timeline_samples(
    conn: &Connection,
    task_id: &str,
    upper_bound: TimelineUpperBound,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT kind, title, summary, payload
               FROM agent_timeline_events
               WHERE task_id = ?1
                 AND kind IN ('message','todo_list','error')
                 AND (
                   turn_seq < ?2
                   OR (turn_seq = ?2 AND intra_turn_order <= ?3)
                 )
               ORDER BY turn_seq DESC, intra_turn_order DESC
               LIMIT 16"#,
        )
        .map_err(|e| format!("prepare title samples failed: {e}"))?;
    let rows = stmt
        .query_map(
            params![task_id, upper_bound.turn_seq, upper_bound.intra_turn_order],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
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

fn request_title(
    client: &Client,
    model: &AssistantAIConfig,
    prompt: &str,
) -> Result<String, String> {
    let base_url = model
        .base_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let url = format!("{base_url}/chat/completions");
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
    proposed: Option<&str>,
) {
    let requires_action = status == "requires_action";
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
            title: title_event_label(status).to_string(),
            summary: proposed.map(str::to_string),
            payload: json!({
                "proposedTitle": proposed,
                "previousTitle": task.title,
                "source": match status {
                    "requires_action" => "manual-blocked",
                    "skipped" => "stale",
                    _ => "auto",
                },
                "requestId": request_id,
                "accepted": if status == "success" { JsonValue::Bool(true) } else { JsonValue::Null },
            }),
            created_at: None,
            updated_at: None,
        },
    );
}

fn title_event_label(status: &str) -> &'static str {
    match status {
        "requires_action" => TITLE_REVIEW_LABEL,
        "skipped" => TITLE_SKIPPED_LABEL,
        _ => TITLE_LABEL,
    }
}

fn record_title_decision<R: Runtime>(
    app: &AppHandle<R>,
    coordinator: &TitleUpdateCoordinator,
    job: &TitleUpdateJob,
    decision: TitleUpdateDecision,
    proposed: Option<&str>,
) {
    let (task, status, emit_change) = match decision {
        TitleUpdateDecision::Success(task) => (task, "success", true),
        TitleUpdateDecision::RequiresAction(task) => (task, "requires_action", false),
        TitleUpdateDecision::Stale(task) => (task, "skipped", false),
        TitleUpdateDecision::Unchanged | TitleUpdateDecision::Stopped => return,
    };
    coordinator.while_running(|| {
        if emit_change {
            emit_tasks_changed(app, task.project_id.clone());
        }
        persist_title_event(app, &task, job.turn_id.as_deref(), status, proposed);
    });
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::thread::{self, JoinHandle};

    use super::*;

    fn title_test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"CREATE TABLE tasks (
                 id TEXT PRIMARY KEY,
                 project_id TEXT,
                 title TEXT NOT NULL,
                 title_source TEXT NOT NULL,
                 archived INTEGER NOT NULL DEFAULT 0
               );"#,
        )
        .unwrap();
        agent_timeline::create_timeline_schema(&conn).unwrap();
        conn
    }

    fn insert_task(conn: &Connection, task_id: &str, title: &str) {
        conn.execute(
            "INSERT INTO tasks (id, project_id, title, title_source) VALUES (?1, 'project-1', ?2, 'auto')",
            params![task_id, title],
        )
        .unwrap();
    }

    fn insert_message(conn: &Connection, id: &str, task_id: &str, turn_id: &str, content: &str) {
        agent_timeline::insert(
            conn,
            AgentTimelineEventInput {
                id: Some(id.to_string()),
                task_id: task_id.to_string(),
                turn_id: Some(turn_id.to_string()),
                backend: BACKEND_NATIVE_AGENTKIT.to_string(),
                kind: "message".to_string(),
                status: "success".to_string(),
                title: "message".to_string(),
                summary: None,
                payload: json!({ "role": "user", "content": content }),
                created_at: None,
                updated_at: None,
            },
        )
        .unwrap();
    }

    fn controlled_title_server(
        title: &'static str,
    ) -> (String, Receiver<()>, Sender<()>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 16_384];
            let _ = stream.read(&mut bytes).unwrap();
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            let body = json!({
                "choices": [{ "message": { "content": title } }]
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        (format!("http://{address}"), started_rx, release_tx, server)
    }

    fn test_model(base_url: String) -> AssistantAIConfig {
        AssistantAIConfig {
            base_url: Some(base_url),
            api_key: Some("test-key".to_string()),
            model: Some("test-model".to_string()),
            ..AssistantAIConfig::default()
        }
    }

    #[test]
    fn normalize_title_strips_wrappers_and_limits_length() {
        assert_eq!(
            normalize_title("标题：`对话标题事件化实现进度需要继续确认更多内容`".to_string())
                .unwrap(),
            "对话标题事件化实现进度需要继续确认更"
        );
        assert!(normalize_title(" ".to_string()).is_err());
    }

    #[test]
    fn timeline_samples_stop_at_captured_upper_bound() {
        let conn = title_test_conn();
        insert_task(&conn, "task-1", "初始标题");
        insert_message(&conn, "a-1", "task-1", "turn-a", "A-BOUND");
        let upper_bound = load_timeline_upper_bound(&conn, "task-1", Some("turn-a"))
            .unwrap()
            .unwrap();

        insert_message(&conn, "a-2", "task-1", "turn-a", "A-LATE");
        insert_message(&conn, "b-1", "task-1", "turn-b", "B-EVENT");

        let samples = load_timeline_samples(&conn, "task-1", upper_bound).unwrap();
        let joined = samples.join("\n");
        assert!(joined.contains("A-BOUND"));
        assert!(!joined.contains("A-LATE"));
        assert!(!joined.contains("B-EVENT"));
    }

    #[test]
    fn newest_generation_wins_when_http_completes_out_of_order() {
        let conn = title_test_conn();
        insert_task(&conn, "task-1", "初始标题");
        let coordinator = TitleUpdateCoordinator::default();
        insert_message(&conn, "a-1", "task-1", "turn-a", "第一轮任务");
        let job_a = prepare_title_job(&conn, &coordinator, "task-1", Some("turn-a".to_string()))
            .unwrap()
            .unwrap();
        insert_message(&conn, "b-1", "task-1", "turn-b", "第二轮任务");
        let job_b = prepare_title_job(&conn, &coordinator, "task-1", Some("turn-b".to_string()))
            .unwrap()
            .unwrap();
        let prompt_a = build_title_prompt(
            &conn,
            "task-1",
            &job_a.task.title,
            job_a.version.upper_bound,
        )
        .unwrap()
        .unwrap();
        let prompt_b = build_title_prompt(
            &conn,
            "task-1",
            &job_b.task.title,
            job_b.version.upper_bound,
        )
        .unwrap()
        .unwrap();
        assert!(!prompt_a.contains("第二轮任务"));
        assert!(prompt_b.contains("第二轮任务"));

        let client = coordinator.http_client().unwrap();
        let (base_a, started_a, release_a, server_a) = controlled_title_server("较旧自动标题");
        let client_a = client.clone();
        let request_a = thread::spawn(move || {
            request_title(&client_a, &test_model(base_a), &prompt_a)
                .and_then(normalize_title)
                .unwrap()
        });
        started_a.recv_timeout(Duration::from_secs(5)).unwrap();

        let (base_b, started_b, release_b, server_b) = controlled_title_server("最新自动标题");
        let client_b = client.clone();
        let request_b = thread::spawn(move || {
            request_title(&client_b, &test_model(base_b), &prompt_b)
                .and_then(normalize_title)
                .unwrap()
        });
        started_b.recv_timeout(Duration::from_secs(5)).unwrap();

        release_b.send(()).unwrap();
        let proposed_b = request_b.join().unwrap();
        server_b.join().unwrap();
        let decision_b = coordinator
            .decide_proposal(&conn, &job_b, &proposed_b)
            .unwrap();
        assert!(matches!(decision_b, TitleUpdateDecision::Success(_)));

        release_a.send(()).unwrap();
        let proposed_a = request_a.join().unwrap();
        server_a.join().unwrap();
        let decision_a = coordinator
            .decide_proposal(&conn, &job_a, &proposed_a)
            .unwrap();
        assert!(matches!(decision_a, TitleUpdateDecision::Stale(_)));

        let final_task = load_task_title_state(&conn, "task-1").unwrap().unwrap();
        assert_eq!(final_task.title, "最新自动标题");
        assert_eq!(final_task.title_source, "auto");
    }

    #[test]
    fn manual_title_wins_when_changed_during_request() {
        let conn = title_test_conn();
        insert_task(&conn, "task-1", "初始标题");
        insert_message(&conn, "a-1", "task-1", "turn-a", "需要生成标题");
        let coordinator = TitleUpdateCoordinator::default();
        let job = prepare_title_job(&conn, &coordinator, "task-1", Some("turn-a".to_string()))
            .unwrap()
            .unwrap();
        let prompt = build_title_prompt(&conn, "task-1", &job.task.title, job.version.upper_bound)
            .unwrap()
            .unwrap();
        let (base_url, started, release, server) = controlled_title_server("后台候选标题");
        let client = coordinator.http_client().unwrap();
        let request = thread::spawn(move || {
            request_title(&client, &test_model(base_url), &prompt)
                .and_then(normalize_title)
                .unwrap()
        });
        started.recv_timeout(Duration::from_secs(5)).unwrap();

        conn.execute(
            "UPDATE tasks SET title = '用户手动标题', title_source = 'manual' WHERE id = 'task-1'",
            [],
        )
        .unwrap();
        release.send(()).unwrap();
        let proposed = request.join().unwrap();
        server.join().unwrap();

        let decision = coordinator.decide_proposal(&conn, &job, &proposed).unwrap();
        let TitleUpdateDecision::RequiresAction(task) = decision else {
            panic!("manual title should require action");
        };
        assert_eq!(task.title, "用户手动标题");
        let final_task = load_task_title_state(&conn, "task-1").unwrap().unwrap();
        assert_eq!(final_task.title, "用户手动标题");
        assert_eq!(final_task.title_source, "manual");
    }
}

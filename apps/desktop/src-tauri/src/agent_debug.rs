use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Manager, State};

use lilia_desktop_application::DesktopApplication;

const MAX_LOGS: usize = 500;

static AGENT_DEBUG_STATE: OnceLock<Mutex<AgentDebugBuffer>> = OnceLock::new();

#[derive(Debug, Default)]
struct AgentDebugBuffer {
    started_at: Option<u64>,
    logs: VecDeque<AgentDebugLogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentDebugLogEntry {
    pub(crate) id: String,
    pub(crate) action_id: Option<String>,
    pub(crate) kind: String,
    pub(crate) level: String,
    pub(crate) message: String,
    pub(crate) data: JsonValue,
    pub(crate) created_at: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentDebugStatus {
    enabled: bool,
    reason: Option<String>,
    started_at: Option<u64>,
    log_count: usize,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn buffer() -> &'static Mutex<AgentDebugBuffer> {
    AGENT_DEBUG_STATE.get_or_init(|| Mutex::new(AgentDebugBuffer::default()))
}

pub(crate) fn agent_debug_enabled() -> bool {
    cfg!(debug_assertions) && std::env::var("LILIA_AGENT_DEBUG").ok().as_deref() == Some("1")
}

fn disabled_reason() -> Option<String> {
    if cfg!(debug_assertions) {
        Some("LILIA_AGENT_DEBUG is not 1".to_string())
    } else {
        Some("agent debug is disabled in release builds".to_string())
    }
}

fn status_for(buffer: &AgentDebugBuffer) -> AgentDebugStatus {
    let enabled = agent_debug_enabled();
    AgentDebugStatus {
        enabled,
        reason: if enabled { None } else { disabled_reason() },
        started_at: buffer.started_at,
        log_count: buffer.logs.len(),
    }
}

fn push_log(buffer: &mut AgentDebugBuffer, entry: AgentDebugLogEntry) {
    if buffer.started_at.is_none() {
        buffer.started_at = Some(now_millis());
    }
    buffer.logs.push_back(entry);
    while buffer.logs.len() > MAX_LOGS {
        buffer.logs.pop_front();
    }
}

#[tauri::command]
pub(crate) fn agent_debug_status() -> AgentDebugStatus {
    let guard = buffer().lock().unwrap();
    status_for(&guard)
}

#[tauri::command]
pub(crate) fn agent_debug_logs(limit: Option<usize>) -> Result<Vec<AgentDebugLogEntry>, String> {
    if !agent_debug_enabled() {
        return Err(disabled_reason().unwrap_or_else(|| "agent debug disabled".to_string()));
    }
    let guard = buffer().lock().unwrap();
    let limit = limit.unwrap_or(200).min(MAX_LOGS);
    let skip = guard.logs.len().saturating_sub(limit);
    Ok(guard.logs.iter().skip(skip).cloned().collect())
}

#[tauri::command]
pub(crate) fn agent_debug_record_action(entry: AgentDebugLogEntry) -> Result<(), String> {
    if !agent_debug_enabled() {
        return Err(disabled_reason().unwrap_or_else(|| "agent debug disabled".to_string()));
    }
    let mut guard = buffer().lock().unwrap();
    push_log(&mut guard, entry);
    Ok(())
}

#[tauri::command]
pub(crate) fn agent_debug_reset_state() -> Result<AgentDebugStatus, String> {
    if !agent_debug_enabled() {
        return Err(disabled_reason().unwrap_or_else(|| "agent debug disabled".to_string()));
    }
    let mut guard = buffer().lock().unwrap();
    guard.logs.clear();
    guard.started_at = Some(now_millis());
    Ok(status_for(&guard))
}

#[tauri::command]
pub(crate) fn agent_debug_runtime_snapshot(
    app: AppHandle,
    chat_store: State<'_, crate::chat::state::ChatStore>,
) -> Result<JsonValue, String> {
    if !agent_debug_enabled() {
        return Err(disabled_reason().unwrap_or_else(|| "agent debug disabled".to_string()));
    }
    let main_window_present = app.get_webview_window(crate::MAIN_WINDOW_LABEL).is_some();
    let running_task_count = chat_store.running_tasks.lock().unwrap().len();
    let queued_task_count = chat_store.pending_turns.lock().unwrap().len();
    Ok(json!({
        "enabled": true,
        "capturedAt": now_millis(),
        "mainWindowPresent": main_window_present,
        "liliaHome": crate::store::resolve_lilia_home().to_string_lossy().to_string(),
        "runningTaskCount": running_task_count,
        "queuedTaskCount": queued_task_count,
    }))
}

#[tauri::command]
pub(crate) fn agent_debug_equivalence_snapshot(
    fixture_id: String,
    application: State<'_, DesktopApplication>,
) -> Result<JsonValue, String> {
    if !agent_debug_enabled() {
        return Err(disabled_reason().unwrap_or_else(|| "agent debug disabled".to_string()));
    }
    let configured = std::env::var("LILIA_EQUIVALENCE_FIXTURE_ID")
        .map_err(|_| "LILIA_EQUIVALENCE_FIXTURE_ID is required".to_owned())?;
    if configured != fixture_id {
        return Err(
            "requested equivalence fixture does not match the configured fixture".to_owned(),
        );
    }
    #[cfg(debug_assertions)]
    {
        let snapshot = application
            .debug_equivalence_snapshot(&fixture_id)
            .map_err(|error| error.to_string())?;
        serde_json::to_value(snapshot).map_err(|error| error.to_string())
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = application;
        Err("agent debug is disabled in release builds".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_entry(index: usize) -> AgentDebugLogEntry {
        AgentDebugLogEntry {
            id: format!("log-{index}"),
            action_id: Some(format!("act-{index}")),
            kind: "frontend".to_string(),
            level: "info".to_string(),
            message: format!("message-{index}"),
            data: JsonValue::Null,
            created_at: index as u64,
        }
    }

    #[test]
    fn ring_buffer_drops_old_entries() {
        let mut buffer = AgentDebugBuffer::default();
        for index in 0..(MAX_LOGS + 3) {
            push_log(&mut buffer, test_entry(index));
        }
        assert_eq!(buffer.logs.len(), MAX_LOGS);
        assert_eq!(buffer.logs.front().unwrap().id, "log-3");
    }

    #[test]
    fn enabled_requires_debug_env() {
        std::env::remove_var("LILIA_AGENT_DEBUG");
        assert!(!agent_debug_enabled());
        std::env::set_var("LILIA_AGENT_DEBUG", "1");
        assert_eq!(agent_debug_enabled(), cfg!(debug_assertions));
        std::env::remove_var("LILIA_AGENT_DEBUG");
    }

    #[test]
    fn status_reports_log_count() {
        let mut buffer = AgentDebugBuffer::default();
        push_log(&mut buffer, test_entry(1));
        let status = status_for(&buffer);
        assert_eq!(status.log_count, 1);
        assert!(status.started_at.is_some());
    }
}

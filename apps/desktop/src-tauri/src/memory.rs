use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::{Map as JsonMap, Value as JsonValue};
use tauri::{AppHandle, Runtime, State};
use uuid::Uuid;

use crate::settings_store::{load_store_value, save_store_value};
use crate::store::LiliaStore;
use crate::task_contract::{self, MemorySettingsDefaults};
use crate::util::now_millis;
#[cfg(test)]
use crate::{BACKEND_CLAUDE, BACKEND_CODEX};

const MEMORY_SETTINGS_KEY: &str = "memory.settings";

fn default_memory_settings() -> MemorySettingsDefaults {
    task_contract::default_memory_settings()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecord {
    pub id: String,
    pub scope: String,
    pub project_id: Option<String>,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub source_task_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryUpsertInput {
    #[serde(default)]
    pub id: Option<String>,
    pub scope: String,
    #[serde(default)]
    pub project_id: Option<String>,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub source_task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemorySettings {
    #[serde(default = "default_memory_enabled")]
    pub enabled: bool,
    #[serde(default = "default_memory_baseline_injection_enabled")]
    pub baseline_injection_enabled: bool,
    #[serde(default = "default_memory_cooldown_turns")]
    pub cooldown_turns: u64,
}

impl Default for MemorySettings {
    fn default() -> Self {
        let defaults = default_memory_settings();
        Self {
            enabled: defaults.enabled,
            baseline_injection_enabled: defaults.baseline_injection_enabled,
            cooldown_turns: defaults.cooldown_turns,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInjectionState {
    pub task_id: String,
    pub enabled: bool,
    pub last_injected_turn_seq: Option<i64>,
    pub updated_at: i64,
}

fn default_true() -> bool {
    true
}

fn default_memory_enabled() -> bool {
    default_memory_settings().enabled
}

fn default_memory_baseline_injection_enabled() -> bool {
    default_memory_settings().baseline_injection_enabled
}

fn default_memory_cooldown_turns() -> u64 {
    default_memory_settings().cooldown_turns
}

fn normalize_memory_settings(settings: Option<MemorySettings>) -> MemorySettings {
    let mut settings = settings.unwrap_or_default();
    if settings.cooldown_turns == 0 {
        settings.cooldown_turns = default_memory_settings().cooldown_turns;
    }
    settings
}

pub(crate) fn load_memory_settings<R: Runtime>(app: &AppHandle<R>) -> MemorySettings {
    normalize_memory_settings(load_store_value(app, MEMORY_SETTINGS_KEY))
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() || !seen.insert(tag.to_string()) {
            continue;
        }
        out.push(tag.to_string());
    }
    out
}

fn normalize_scope_project(
    scope: &str,
    project_id: Option<String>,
) -> Result<Option<String>, String> {
    match scope {
        "user" => Ok(None),
        "project" => project_id
            .and_then(|id| {
                let id = id.trim().to_string();
                (!id.is_empty()).then_some(id)
            })
            .ok_or_else(|| "memory_upsert: 项目级记忆必须关联项目".to_string())
            .map(Some),
        _ => Err(format!("memory_upsert: 无效作用域：{scope}")),
    }
}

fn tags_json(tags: &[String]) -> Result<String, String> {
    serde_json::to_string(tags).map_err(|e| format!("memory_upsert: tags 序列化失败：{e}"))
}

fn parse_tags(text: String) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(&text).unwrap_or_default()
}

fn row_to_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRecord> {
    let tags_text: String = row.get(5)?;
    Ok(MemoryRecord {
        id: row.get(0)?,
        scope: row.get(1)?,
        project_id: row.get(2)?,
        title: row.get(3)?,
        body: row.get(4)?,
        tags: parse_tags(tags_text),
        enabled: row.get::<_, i64>(6)? != 0,
        source_task_id: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn load_memory_by_id(conn: &Connection, id: &str) -> Result<MemoryRecord, String> {
    conn.query_row(
        r#"SELECT id, scope, project_id, title, body, tags_json, enabled,
                  source_task_id, created_at, updated_at
           FROM memories WHERE id = ?1"#,
        params![id],
        row_to_memory,
    )
    .optional()
    .map_err(|e| format!("memory_get: 查询记忆失败：{e}"))?
    .ok_or_else(|| "memory_get: 记忆不存在".to_string())
}

pub(crate) fn list_memories_core(
    conn: &Connection,
    project_id: Option<&str>,
) -> Result<Vec<MemoryRecord>, String> {
    let mut out = Vec::new();
    let mut user_stmt = conn
        .prepare(
            r#"SELECT id, scope, project_id, title, body, tags_json, enabled,
                      source_task_id, created_at, updated_at
               FROM memories
               WHERE scope = 'user'
               ORDER BY updated_at DESC, created_at DESC"#,
        )
        .map_err(|e| format!("memory_list: prepare user 失败：{e}"))?;
    let user_rows = user_stmt
        .query_map([], row_to_memory)
        .map_err(|e| format!("memory_list: query user 失败：{e}"))?;
    for row in user_rows {
        out.push(row.map_err(|e| format!("memory_list: user row 失败：{e}"))?);
    }

    if let Some(project_id) = project_id.filter(|id| !id.trim().is_empty()) {
        let mut project_stmt = conn
            .prepare(
                r#"SELECT id, scope, project_id, title, body, tags_json, enabled,
                          source_task_id, created_at, updated_at
                   FROM memories
                   WHERE scope = 'project' AND project_id = ?1
                   ORDER BY updated_at DESC, created_at DESC"#,
            )
            .map_err(|e| format!("memory_list: prepare project 失败：{e}"))?;
        let project_rows = project_stmt
            .query_map(params![project_id], row_to_memory)
            .map_err(|e| format!("memory_list: query project 失败：{e}"))?;
        for row in project_rows {
            out.push(row.map_err(|e| format!("memory_list: project row 失败：{e}"))?);
        }
    }
    Ok(out)
}

pub(crate) fn upsert_memory_core(
    conn: &Connection,
    input: MemoryUpsertInput,
) -> Result<MemoryRecord, String> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err("memory_upsert: 标题不能为空".to_string());
    }
    let body = input.body.trim();
    if body.is_empty() {
        return Err("memory_upsert: 正文不能为空".to_string());
    }
    let project_id = normalize_scope_project(&input.scope, input.project_id)?;
    if let Some(project_id) = project_id.as_deref() {
        let exists = conn
            .query_row(
                "SELECT 1 FROM projects WHERE id = ?1",
                params![project_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(|e| format!("memory_upsert: 查询项目失败：{e}"))?
            .is_some();
        if !exists {
            return Err("memory_upsert: 项目不存在".to_string());
        }
    }

    let id = input
        .id
        .and_then(|id| {
            let id = id.trim().to_string();
            (!id.is_empty()).then_some(id)
        })
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let now = now_millis();
    let created_at = conn
        .query_row(
            "SELECT created_at FROM memories WHERE id = ?1",
            params![id.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|e| format!("memory_upsert: 查询已有记忆失败：{e}"))?
        .unwrap_or(now);
    let tags = normalize_tags(input.tags);
    let tags_json = tags_json(&tags)?;
    let source_task_id = input.source_task_id.and_then(|task_id| {
        let task_id = task_id.trim().to_string();
        (!task_id.is_empty()).then_some(task_id)
    });

    conn.execute(
        r#"INSERT INTO memories
           (id, scope, project_id, title, body, tags_json, enabled, source_task_id, created_at, updated_at)
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
            id.as_str(),
            input.scope.as_str(),
            project_id.as_deref(),
            title,
            body,
            tags_json,
            if input.enabled { 1 } else { 0 },
            source_task_id.as_deref(),
            created_at,
            now,
        ],
    )
    .map_err(|e| format!("memory_upsert: 写入失败：{e}"))?;

    load_memory_by_id(conn, &id)
}

pub(crate) fn set_memory_enabled_core(
    conn: &Connection,
    id: &str,
    enabled: bool,
) -> Result<MemoryRecord, String> {
    let changed = conn
        .execute(
            "UPDATE memories SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            params![if enabled { 1 } else { 0 }, now_millis(), id],
        )
        .map_err(|e| format!("memory_set_enabled: 更新失败：{e}"))?;
    if changed == 0 {
        return Err("memory_set_enabled: 记忆不存在".to_string());
    }
    load_memory_by_id(conn, id)
}

pub(crate) fn delete_memory_core(conn: &Connection, id: &str) -> Result<bool, String> {
    conn.execute("DELETE FROM memories WHERE id = ?1", params![id])
        .map(|count| count > 0)
        .map_err(|e| format!("memory_delete: 删除失败：{e}"))
}

pub(crate) fn get_injection_state_core(
    conn: &Connection,
    task_id: &str,
) -> Result<MemoryInjectionState, String> {
    let row = conn
        .query_row(
            r#"SELECT enabled, last_injected_turn_seq, updated_at
               FROM memory_injection_states WHERE task_id = ?1"#,
            params![task_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)? != 0,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|e| format!("memory_injection_state: 查询失败：{e}"))?;
    let (enabled, last_injected_turn_seq, updated_at) = row.unwrap_or((true, None, 0));
    Ok(MemoryInjectionState {
        task_id: task_id.to_string(),
        enabled,
        last_injected_turn_seq,
        updated_at,
    })
}

pub(crate) fn set_task_memory_enabled_core(
    conn: &Connection,
    task_id: &str,
    enabled: bool,
) -> Result<MemoryInjectionState, String> {
    let now = now_millis();
    conn.execute(
        r#"INSERT INTO memory_injection_states
           (task_id, enabled, last_injected_turn_seq, updated_at)
           VALUES (?1, ?2, NULL, ?3)
           ON CONFLICT(task_id) DO UPDATE SET
             enabled = excluded.enabled,
             updated_at = excluded.updated_at"#,
        params![task_id, if enabled { 1 } else { 0 }, now],
    )
    .map_err(|e| format!("memory_set_task_enabled: 写入失败：{e}"))?;
    get_injection_state_core(conn, task_id)
}

pub(crate) fn reset_task_memory_cooldown_core(
    conn: &Connection,
    task_id: &str,
) -> Result<MemoryInjectionState, String> {
    let now = now_millis();
    conn.execute(
        r#"INSERT INTO memory_injection_states
           (task_id, enabled, last_injected_turn_seq, updated_at)
           VALUES (?1, 1, NULL, ?2)
           ON CONFLICT(task_id) DO UPDATE SET
             last_injected_turn_seq = NULL,
             updated_at = excluded.updated_at"#,
        params![task_id, now],
    )
    .map_err(|e| format!("memory_reset_task_cooldown: 写入失败：{e}"))?;
    get_injection_state_core(conn, task_id)
}

#[cfg(test)]
fn current_turn_seq(conn: &Connection, task_id: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT MAX(turn_seq) FROM agent_timeline_events WHERE task_id = ?1",
        params![task_id],
        |row| row.get::<_, Option<i64>>(0),
    )
    .map(|value| value.unwrap_or(0))
    .map_err(|e| format!("memory_baseline: 查询当前 turn 失败：{e}"))
}

#[cfg(test)]
fn resolve_project_id(
    conn: &Connection,
    task_id: &str,
    project_cwd: &str,
) -> Result<Option<String>, String> {
    let by_task = conn
        .query_row(
            "SELECT project_id FROM tasks WHERE id = ?1",
            params![task_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|e| format!("memory_baseline: 查询 task 项目失败：{e}"))?
        .flatten();
    if by_task.is_some() {
        return Ok(by_task);
    }

    let cwd = project_cwd.trim();
    if cwd.is_empty() {
        return Ok(None);
    }
    conn.query_row(
        "SELECT id FROM projects WHERE cwd = ?1 ORDER BY created_at DESC LIMIT 1",
        params![cwd],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|e| format!("memory_baseline: 按 cwd 查询项目失败：{e}"))
}

#[cfg(test)]
fn enabled_memories_for_baseline(
    conn: &Connection,
    project_id: Option<&str>,
) -> Result<(Vec<MemoryRecord>, Vec<MemoryRecord>), String> {
    let user = list_enabled_memories(conn, "user", None)?;
    let project = if let Some(project_id) = project_id {
        list_enabled_memories(conn, "project", Some(project_id))?
    } else {
        Vec::new()
    };
    Ok((user, project))
}

#[cfg(test)]
fn list_enabled_memories(
    conn: &Connection,
    scope: &str,
    project_id: Option<&str>,
) -> Result<Vec<MemoryRecord>, String> {
    let mut out = Vec::new();
    if scope == "project" {
        let mut stmt = conn
            .prepare(
                r#"SELECT id, scope, project_id, title, body, tags_json, enabled,
                          source_task_id, created_at, updated_at
                   FROM memories
                   WHERE scope = 'project' AND project_id = ?1 AND enabled = 1
                   ORDER BY updated_at DESC, created_at DESC"#,
            )
            .map_err(|e| format!("memory_baseline: prepare project memories 失败：{e}"))?;
        let rows = stmt
            .query_map(params![project_id.unwrap_or_default()], row_to_memory)
            .map_err(|e| format!("memory_baseline: query project memories 失败：{e}"))?;
        for row in rows {
            out.push(row.map_err(|e| format!("memory_baseline: memory row 失败：{e}"))?);
        }
    } else {
        let mut stmt = conn
            .prepare(
                r#"SELECT id, scope, project_id, title, body, tags_json, enabled,
                          source_task_id, created_at, updated_at
                   FROM memories
                   WHERE scope = 'user' AND enabled = 1
                   ORDER BY updated_at DESC, created_at DESC"#,
            )
            .map_err(|e| format!("memory_baseline: prepare user memories 失败：{e}"))?;
        let rows = stmt
            .query_map([], row_to_memory)
            .map_err(|e| format!("memory_baseline: query user memories 失败：{e}"))?;
        for row in rows {
            out.push(row.map_err(|e| format!("memory_baseline: memory row 失败：{e}"))?);
        }
    }
    Ok(out)
}

#[cfg(test)]
fn compact_body(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
pub(crate) fn format_memory_baseline(
    user: &[MemoryRecord],
    project: &[MemoryRecord],
) -> Option<String> {
    if user.is_empty() && project.is_empty() {
        return None;
    }
    let mut lines = vec!["[Lilia Memory Baseline]".to_string()];
    if !user.is_empty() {
        lines.push(String::new());
        lines.push("User constraints:".to_string());
        for item in user {
            lines.push(format!(
                "- {}: {}",
                item.title.trim(),
                compact_body(&item.body)
            ));
        }
    }
    if !project.is_empty() {
        lines.push(String::new());
        lines.push("Project constraints:".to_string());
        for item in project {
            lines.push(format!(
                "- {}: {}",
                item.title.trim(),
                compact_body(&item.body)
            ));
        }
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
pub(crate) fn build_memory_baseline_core(
    conn: &Connection,
    task_id: &str,
    project_cwd: &str,
    settings: &MemorySettings,
) -> Result<Option<String>, String> {
    if !settings.enabled || !settings.baseline_injection_enabled {
        return Ok(None);
    }
    let state = get_injection_state_core(conn, task_id)?;
    if !state.enabled {
        return Ok(None);
    }
    let turn_seq = current_turn_seq(conn, task_id)?;
    if let Some(last) = state.last_injected_turn_seq {
        let cooldown = settings.cooldown_turns as i64;
        if turn_seq.saturating_sub(last) < cooldown {
            return Ok(None);
        }
    }
    let project_id = resolve_project_id(conn, task_id, project_cwd)?;
    let (user, project) = enabled_memories_for_baseline(conn, project_id.as_deref())?;
    let baseline = format_memory_baseline(&user, &project);
    if baseline.is_some() {
        record_memory_injection_core(conn, task_id, turn_seq)?;
    }
    Ok(baseline)
}

#[cfg(test)]
fn record_memory_injection_core(
    conn: &Connection,
    task_id: &str,
    turn_seq: i64,
) -> Result<(), String> {
    let now = now_millis();
    conn.execute(
        r#"INSERT INTO memory_injection_states
           (task_id, enabled, last_injected_turn_seq, updated_at)
           VALUES (?1, 1, ?2, ?3)
           ON CONFLICT(task_id) DO UPDATE SET
             last_injected_turn_seq = excluded.last_injected_turn_seq,
             updated_at = excluded.updated_at"#,
        params![task_id, turn_seq, now],
    )
    .map(|_| ())
    .map_err(|e| format!("memory_baseline: 记录注入状态失败：{e}"))
}

#[cfg(test)]
fn ensure_runtime_options_object(runtime_options: Option<JsonValue>) -> JsonValue {
    match runtime_options {
        Some(value @ JsonValue::Object(_)) => value,
        _ => JsonValue::Object(JsonMap::new()),
    }
}

#[cfg(test)]
fn append_context(existing: Option<&JsonValue>, baseline: &str) -> String {
    let existing = existing
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty());
    match existing {
        Some(existing) => format!("{existing}\n\n{baseline}"),
        None => baseline.to_string(),
    }
}

#[cfg(test)]
pub(crate) fn append_context_to_runtime_options(
    backend: &str,
    runtime_options: Option<JsonValue>,
    context: &str,
) -> Option<JsonValue> {
    if context.trim().is_empty() {
        return runtime_options;
    }
    let provider_key = match backend {
        BACKEND_CODEX => "codex",
        BACKEND_CLAUDE => "claude",
        _ => return runtime_options,
    };
    let mut value = ensure_runtime_options_object(runtime_options);
    if !value
        .get("provider")
        .is_some_and(|provider| provider.is_object())
    {
        value["provider"] = JsonValue::Object(JsonMap::new());
    }
    if !value["provider"]
        .get(provider_key)
        .is_some_and(|provider| provider.is_object())
    {
        value["provider"][provider_key] = JsonValue::Object(JsonMap::new());
    }
    let next = append_context(
        value["provider"][provider_key].get("additionalContext"),
        context,
    );
    value["provider"][provider_key]["additionalContext"] = JsonValue::String(next);
    Some(value)
}


#[tauri::command]
pub fn memory_list(
    project_id: Option<String>,
    store: State<'_, LiliaStore>,
) -> Result<Vec<MemoryRecord>, String> {
    let conn = store.conn()?;
    list_memories_core(&conn, project_id.as_deref())
}

#[tauri::command]
pub fn memory_upsert(
    input: MemoryUpsertInput,
    store: State<'_, LiliaStore>,
) -> Result<MemoryRecord, String> {
    let conn = store.conn()?;
    upsert_memory_core(&conn, input)
}

#[tauri::command]
pub fn memory_set_enabled(
    id: String,
    enabled: bool,
    store: State<'_, LiliaStore>,
) -> Result<MemoryRecord, String> {
    let conn = store.conn()?;
    set_memory_enabled_core(&conn, &id, enabled)
}

#[tauri::command]
pub fn memory_delete(id: String, store: State<'_, LiliaStore>) -> Result<bool, String> {
    let conn = store.conn()?;
    delete_memory_core(&conn, &id)
}

#[tauri::command]
pub fn memory_get_settings<R: Runtime>(app: AppHandle<R>) -> MemorySettings {
    load_memory_settings(&app)
}

#[tauri::command]
pub fn memory_set_settings<R: Runtime>(
    app: AppHandle<R>,
    settings: MemorySettings,
) -> Result<(), String> {
    save_store_value(
        &app,
        MEMORY_SETTINGS_KEY,
        &normalize_memory_settings(Some(settings)),
    )
}

#[tauri::command]
pub fn memory_get_injection_state(
    task_id: String,
    store: State<'_, LiliaStore>,
) -> Result<MemoryInjectionState, String> {
    let conn = store.conn()?;
    get_injection_state_core(&conn, &task_id)
}

#[tauri::command]
pub fn memory_set_task_enabled(
    task_id: String,
    enabled: bool,
    store: State<'_, LiliaStore>,
) -> Result<MemoryInjectionState, String> {
    let conn = store.conn()?;
    set_task_memory_enabled_core(&conn, &task_id, enabled)
}

#[tauri::command]
pub fn memory_reset_task_cooldown(
    task_id: String,
    store: State<'_, LiliaStore>,
) -> Result<MemoryInjectionState, String> {
    let conn = store.conn()?;
    reset_task_memory_cooldown_core(&conn, &task_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_timeline;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE projects (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              cwd TEXT,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE tasks (
              id TEXT PRIMARY KEY,
              project_id TEXT,
              session_id TEXT NOT NULL,
              title TEXT NOT NULL,
              status TEXT NOT NULL,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE memories (
              id TEXT PRIMARY KEY,
              scope TEXT NOT NULL CHECK (scope IN ('user','project')),
              project_id TEXT,
              title TEXT NOT NULL,
              body TEXT NOT NULL,
              tags_json TEXT NOT NULL DEFAULT '[]',
              enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
              source_task_id TEXT,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              CHECK (
                (scope = 'user' AND project_id IS NULL) OR
                (scope = 'project' AND project_id IS NOT NULL)
              )
            );
            CREATE TABLE memory_injection_states (
              task_id TEXT PRIMARY KEY,
              enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
              last_injected_turn_seq INTEGER,
              updated_at INTEGER NOT NULL
            );
            "#,
        )
        .unwrap();
        agent_timeline::create_timeline_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, cwd, created_at) VALUES ('project-1', 'Lilia', 'C:/repo', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tasks (id, project_id, session_id, title, status, created_at) VALUES ('task-1', 'project-1', 's1', 'Task', 'waiting', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO agent_timeline_events
               (id, task_id, turn_id, backend, kind, status, title, summary, payload, created_at, updated_at, turn_seq, intra_turn_order)
               VALUES ('event-1', 'task-1', 'turn-1', 'codex', 'message', 'success', '用户输入', NULL, '{}', 1, 1, 0, 0)"#,
            [],
        )
        .unwrap();
        conn
    }

    fn input(scope: &str, project_id: Option<&str>, title: &str, body: &str) -> MemoryUpsertInput {
        MemoryUpsertInput {
            id: None,
            scope: scope.to_string(),
            project_id: project_id.map(|value| value.to_string()),
            title: title.to_string(),
            body: body.to_string(),
            tags: vec![" rule ".to_string(), "rule".to_string(), "".to_string()],
            enabled: true,
            source_task_id: None,
        }
    }

    #[test]
    fn memory_crud_normalizes_scope_tags_and_enabled() {
        let conn = conn();
        let user = upsert_memory_core(
            &conn,
            input("user", Some("project-1"), " No emoji ", "正文"),
        )
        .unwrap();
        assert_eq!(user.scope, "user");
        assert_eq!(user.project_id, None);
        assert_eq!(user.tags, vec!["rule"]);

        let project = upsert_memory_core(
            &conn,
            input("project", Some("project-1"), "迁移", "先 dry-run"),
        )
        .unwrap();
        let listed = list_memories_core(&conn, Some("project-1")).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|item| item.id == project.id));

        let disabled = set_memory_enabled_core(&conn, &project.id, false).unwrap();
        assert!(!disabled.enabled);
        assert!(delete_memory_core(&conn, &project.id).unwrap());
    }

    #[test]
    fn project_memory_requires_project_id() {
        let conn = conn();
        let err = upsert_memory_core(&conn, input("project", None, "标题", "正文")).unwrap_err();
        assert!(err.contains("项目级记忆必须关联项目"));
    }

    #[test]
    fn memory_settings_defaults_follow_contract_manifest() {
        let defaults = default_memory_settings();
        let settings = MemorySettings::default();
        assert_eq!(settings.enabled, defaults.enabled);
        assert_eq!(
            settings.baseline_injection_enabled,
            defaults.baseline_injection_enabled
        );
        assert_eq!(settings.cooldown_turns, defaults.cooldown_turns);
        let partial: MemorySettings = serde_json::from_value(serde_json::json!({
            "enabled": false
        }))
        .unwrap();
        assert_eq!(
            partial,
            MemorySettings {
                enabled: false,
                baseline_injection_enabled: defaults.baseline_injection_enabled,
                cooldown_turns: defaults.cooldown_turns,
            }
        );
        assert_eq!(
            normalize_memory_settings(Some(MemorySettings {
                cooldown_turns: 0,
                ..settings
            }))
            .cooldown_turns,
            defaults.cooldown_turns
        );
    }

    #[test]
    fn baseline_includes_user_and_project_then_respects_cooldown() {
        let conn = conn();
        upsert_memory_core(&conn, input("user", None, "PR", "描述不要出现 emoji")).unwrap();
        upsert_memory_core(
            &conn,
            input("project", Some("project-1"), "DB", "迁移必须先 dry-run"),
        )
        .unwrap();
        let settings = MemorySettings::default();

        let baseline = build_memory_baseline_core(&conn, "task-1", "C:/repo", &settings)
            .unwrap()
            .unwrap();
        assert!(baseline.contains("[Lilia Memory Baseline]"));
        assert!(baseline.contains("User constraints:"));
        assert!(baseline.contains("Project constraints:"));
        assert!(baseline.contains("PR: 描述不要出现 emoji"));
        assert!(baseline.contains("DB: 迁移必须先 dry-run"));

        let skipped = build_memory_baseline_core(&conn, "task-1", "C:/repo", &settings).unwrap();
        assert_eq!(skipped, None);
    }

    #[test]
    fn baseline_respects_global_and_task_switches() {
        let conn = conn();
        upsert_memory_core(&conn, input("user", None, "PR", "不要 emoji")).unwrap();
        let mut settings = MemorySettings::default();
        settings.enabled = false;
        assert_eq!(
            build_memory_baseline_core(&conn, "task-1", "C:/repo", &settings).unwrap(),
            None
        );

        settings.enabled = true;
        set_task_memory_enabled_core(&conn, "task-1", false).unwrap();
        assert_eq!(
            build_memory_baseline_core(&conn, "task-1", "C:/repo", &settings).unwrap(),
            None
        );
    }

    #[test]
    fn runtime_options_append_existing_additional_context() {
        let value = append_context_to_runtime_options(
            BACKEND_CODEX,
            Some(serde_json::json!({
                "provider": {
                    "codex": {
                        "additionalContext": "existing"
                    }
                }
            })),
            "[Lilia Memory Baseline]\nUser constraints:\n- PR: no emoji",
        )
        .unwrap();
        assert_eq!(
            value["provider"]["codex"]["additionalContext"],
            serde_json::json!(
                "existing\n\n[Lilia Memory Baseline]\nUser constraints:\n- PR: no emoji"
            )
        );
    }
}

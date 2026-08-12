#[cfg(test)]
use rusqlite::{params, Connection, OptionalExtension};
#[cfg(test)]
use serde_json::{Map as JsonMap, Value as JsonValue};
use tauri::{AppHandle, State};

pub use lilia_desktop_application::{
    DesktopMemory as MemoryRecord, MemoryInjectionState, MemorySettings, MemoryUpsertInput,
};
use lilia_desktop_application::{DesktopMemoryService, MEMORY_SETTINGS_KEY};
use lilia_desktop_application::{MemorySettingsStore, MemoryStoreError};

use crate::settings_store::{load_store_value, save_store_value};
#[cfg(test)]
use crate::{BACKEND_CLAUDE, BACKEND_CODEX};

#[cfg(test)]
fn normalize_memory_settings(settings: Option<MemorySettings>) -> MemorySettings {
    settings.unwrap_or_default().normalized()
}

pub(crate) struct TauriMemorySettingsStore {
    app: AppHandle,
}

impl TauriMemorySettingsStore {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl MemorySettingsStore for TauriMemorySettingsStore {
    fn load(&self) -> Result<Option<MemorySettings>, MemoryStoreError> {
        Ok(load_store_value(&self.app, MEMORY_SETTINGS_KEY))
    }

    fn save(&mut self, settings: &MemorySettings) -> Result<(), MemoryStoreError> {
        save_store_value(&self.app, MEMORY_SETTINGS_KEY, settings).map_err(|message| {
            MemoryStoreError::SettingsStorage {
                operation: "save",
                message,
            }
        })
    }
}

fn list_memories_core(
    memory: &DesktopMemoryService,
    project_id: Option<&str>,
) -> Result<Vec<MemoryRecord>, String> {
    memory.list(project_id).map_err(|error| error.to_string())
}

fn upsert_memory_core(
    memory: &DesktopMemoryService,
    input: MemoryUpsertInput,
) -> Result<MemoryRecord, String> {
    memory.save(input).map_err(|error| error.to_string())
}

fn set_memory_enabled_core(
    memory: &DesktopMemoryService,
    id: &str,
    enabled: bool,
) -> Result<MemoryRecord, String> {
    memory
        .set_enabled(id, enabled)
        .map_err(|error| error.to_string())
}

fn delete_memory_core(memory: &DesktopMemoryService, id: &str) -> Result<bool, String> {
    memory.delete(id).map_err(|error| error.to_string())
}

fn get_injection_state_core(
    memory: &DesktopMemoryService,
    task_id: &str,
) -> Result<MemoryInjectionState, String> {
    memory
        .injection_state(task_id)
        .map_err(|error| error.to_string())
}

fn set_task_memory_enabled_core(
    memory: &DesktopMemoryService,
    task_id: &str,
    enabled: bool,
) -> Result<MemoryInjectionState, String> {
    memory
        .set_task_enabled(task_id, enabled)
        .map_err(|error| error.to_string())
}

fn reset_task_memory_cooldown_core(
    memory: &DesktopMemoryService,
    task_id: &str,
) -> Result<MemoryInjectionState, String> {
    memory
        .reset_task_cooldown(task_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn memory_list(
    project_id: Option<String>,
    memory: State<'_, DesktopMemoryService>,
) -> Result<Vec<MemoryRecord>, String> {
    list_memories_core(&memory, project_id.as_deref())
}

#[tauri::command]
pub fn memory_upsert(
    input: MemoryUpsertInput,
    memory: State<'_, DesktopMemoryService>,
) -> Result<MemoryRecord, String> {
    upsert_memory_core(&memory, input)
}

#[tauri::command]
pub fn memory_set_enabled(
    id: String,
    enabled: bool,
    memory: State<'_, DesktopMemoryService>,
) -> Result<MemoryRecord, String> {
    set_memory_enabled_core(&memory, &id, enabled)
}

#[tauri::command]
pub fn memory_delete(id: String, memory: State<'_, DesktopMemoryService>) -> Result<bool, String> {
    delete_memory_core(&memory, &id)
}

#[tauri::command]
pub fn memory_get_settings(
    memory: State<'_, DesktopMemoryService>,
) -> Result<MemorySettings, String> {
    memory.settings().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn memory_set_settings(
    memory: State<'_, DesktopMemoryService>,
    settings: MemorySettings,
) -> Result<(), String> {
    memory
        .save_settings(settings)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn memory_get_injection_state(
    task_id: String,
    memory: State<'_, DesktopMemoryService>,
) -> Result<MemoryInjectionState, String> {
    get_injection_state_core(&memory, &task_id)
}

#[tauri::command]
pub fn memory_set_task_enabled(
    task_id: String,
    enabled: bool,
    memory: State<'_, DesktopMemoryService>,
) -> Result<MemoryInjectionState, String> {
    set_task_memory_enabled_core(&memory, &task_id, enabled)
}

#[tauri::command]
pub fn memory_reset_task_cooldown(
    task_id: String,
    memory: State<'_, DesktopMemoryService>,
) -> Result<MemoryInjectionState, String> {
    reset_task_memory_cooldown_core(&memory, &task_id)
}

#[cfg(test)]
fn current_turn_seq(conn: &Connection, task_id: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT MAX(turn_seq) FROM agent_timeline_events WHERE task_id = ?1",
        params![task_id],
        |row| row.get::<_, Option<i64>>(0),
    )
    .map(|value| value.unwrap_or(0))
    .map_err(|error| format!("memory_baseline: 查询当前 turn 失败：{error}"))
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
        .map_err(|error| format!("memory_baseline: 查询 task 项目失败：{error}"))?
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
    .map_err(|error| format!("memory_baseline: 按 cwd 查询项目失败：{error}"))
}

#[cfg(test)]
fn test_row_to_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRecord> {
    let scope = match row.get::<_, String>(1)?.as_str() {
        "user" => lilia_desktop_application::MemoryScope::User,
        _ => lilia_desktop_application::MemoryScope::Project,
    };
    let tags_json = row.get::<_, String>(5)?;
    Ok(MemoryRecord {
        id: row.get(0)?,
        scope,
        project_id: row.get(2)?,
        title: row.get(3)?,
        body: row.get(4)?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        enabled: row.get::<_, i64>(6)? != 0,
        source_task_id: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

#[cfg(test)]
fn list_enabled_memories(
    conn: &Connection,
    scope: lilia_desktop_application::MemoryScope,
    project_id: Option<&str>,
) -> Result<Vec<MemoryRecord>, String> {
    let (sql, argument) = match scope {
        lilia_desktop_application::MemoryScope::User => (
            r#"SELECT id, scope, project_id, title, body, tags_json, enabled,
                      source_task_id, created_at, updated_at
               FROM memories
               WHERE scope = 'user' AND enabled = 1
               ORDER BY updated_at DESC, created_at DESC"#,
            None,
        ),
        lilia_desktop_application::MemoryScope::Project => (
            r#"SELECT id, scope, project_id, title, body, tags_json, enabled,
                      source_task_id, created_at, updated_at
               FROM memories
               WHERE scope = 'project' AND project_id = ?1 AND enabled = 1
               ORDER BY updated_at DESC, created_at DESC"#,
            project_id,
        ),
    };
    let mut statement = conn
        .prepare(sql)
        .map_err(|error| format!("memory_baseline: prepare memories 失败：{error}"))?;
    let rows = match argument {
        Some(project_id) => statement.query_map(params![project_id], test_row_to_memory),
        None => statement.query_map([], test_row_to_memory),
    }
    .map_err(|error| format!("memory_baseline: query memories 失败：{error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("memory_baseline: memory row 失败：{error}"))
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
    let mut lines = vec!["[Lilia Memory Baseline]".to_owned()];
    if !user.is_empty() {
        lines.push(String::new());
        lines.push("User constraints:".to_owned());
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
        lines.push("Project constraints:".to_owned());
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
fn test_injection_state(conn: &Connection, task_id: &str) -> Result<MemoryInjectionState, String> {
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
        .map_err(|error| format!("memory_injection_state: 查询失败：{error}"))?;
    let (enabled, last_injected_turn_seq, updated_at) = row.unwrap_or((true, None, 0));
    Ok(MemoryInjectionState {
        task_id: task_id.to_owned(),
        enabled,
        last_injected_turn_seq,
        updated_at,
    })
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
    let state = test_injection_state(conn, task_id)?;
    if !state.enabled {
        return Ok(None);
    }
    let turn_seq = current_turn_seq(conn, task_id)?;
    if let Some(last) = state.last_injected_turn_seq {
        if turn_seq.saturating_sub(last) < settings.cooldown_turns as i64 {
            return Ok(None);
        }
    }
    let project_id = resolve_project_id(conn, task_id, project_cwd)?;
    let user = list_enabled_memories(conn, lilia_desktop_application::MemoryScope::User, None)?;
    let project = match project_id.as_deref() {
        Some(project_id) => list_enabled_memories(
            conn,
            lilia_desktop_application::MemoryScope::Project,
            Some(project_id),
        )?,
        None => Vec::new(),
    };
    let baseline = format_memory_baseline(&user, &project);
    if baseline.is_some() {
        let now = crate::util::now_millis();
        conn.execute(
            r#"INSERT INTO memory_injection_states
               (task_id, enabled, last_injected_turn_seq, updated_at)
               VALUES (?1, 1, ?2, ?3)
               ON CONFLICT(task_id) DO UPDATE SET
                 last_injected_turn_seq = excluded.last_injected_turn_seq,
                 updated_at = excluded.updated_at"#,
            params![task_id, turn_seq, now],
        )
        .map_err(|error| format!("memory_baseline: 记录注入状态失败：{error}"))?;
    }
    Ok(baseline)
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
        None => baseline.to_owned(),
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
        .is_some_and(serde_json::Value::is_object)
    {
        value["provider"] = JsonValue::Object(JsonMap::new());
    }
    if !value["provider"]
        .get(provider_key)
        .is_some_and(serde_json::Value::is_object)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_timeline;
    use lilia_desktop_application::MemoryScope;

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

    fn input(scope: MemoryScope, project_id: Option<&str>) -> MemoryUpsertInput {
        MemoryUpsertInput {
            id: None,
            scope,
            project_id: project_id.map(str::to_owned),
            title: " No emoji ".to_owned(),
            body: "正文".to_owned(),
            tags: vec![" rule ".to_owned(), "rule".to_owned(), String::new()],
            enabled: true,
            source_task_id: None,
            expected_updated_at: None,
        }
    }

    #[test]
    fn memory_commands_delegate_crud_and_injection_reads_to_application_service() {
        let service = DesktopMemoryService::in_memory().unwrap();
        let user = upsert_memory_core(&service, input(MemoryScope::User, None)).unwrap();
        assert_eq!(user.scope, MemoryScope::User);
        assert_eq!(user.tags, vec!["rule"]);
        assert_eq!(
            list_memories_core(&service, None).unwrap(),
            vec![user.clone()]
        );
        assert!(
            !set_memory_enabled_core(&service, &user.id, false)
                .unwrap()
                .enabled
        );
        assert_eq!(
            get_injection_state_core(&service, "task-unknown").unwrap(),
            MemoryInjectionState {
                task_id: "task-unknown".to_owned(),
                enabled: true,
                last_injected_turn_seq: None,
                updated_at: 0,
            }
        );
        assert!(set_task_memory_enabled_core(&service, "task-unknown", false).is_err());
        assert!(delete_memory_core(&service, &user.id).unwrap());
    }

    #[test]
    fn memory_settings_defaults_follow_shared_application_contract() {
        let defaults = MemorySettings::default();
        let partial: MemorySettings = serde_json::from_value(serde_json::json!({
            "enabled": false
        }))
        .unwrap();
        assert_eq!(partial.cooldown_turns, defaults.cooldown_turns);
        assert_eq!(
            normalize_memory_settings(Some(MemorySettings {
                cooldown_turns: 0,
                ..defaults.clone()
            })),
            defaults
        );
        assert_eq!(MEMORY_SETTINGS_KEY, "memory.settings");
    }

    #[test]
    fn baseline_includes_user_and_project_then_respects_cooldown() {
        let conn = conn();
        conn.execute(
            r#"INSERT INTO memories
               (id, scope, project_id, title, body, tags_json, enabled, created_at, updated_at)
               VALUES
               ('user-memory', 'user', NULL, 'PR', '描述不要出现 emoji', '[]', 1, 1, 1),
               ('project-memory', 'project', 'project-1', 'DB', '迁移必须先 dry-run', '[]', 1, 1, 1)"#,
            [],
        )
        .unwrap();
        let settings = MemorySettings::default();
        let baseline = build_memory_baseline_core(&conn, "task-1", "C:/repo", &settings)
            .unwrap()
            .unwrap();
        assert!(baseline.contains("PR: 描述不要出现 emoji"));
        assert!(baseline.contains("DB: 迁移必须先 dry-run"));
        assert_eq!(
            build_memory_baseline_core(&conn, "task-1", "C:/repo", &settings).unwrap(),
            None
        );
    }

    #[test]
    fn baseline_respects_global_and_task_switches() {
        let conn = conn();
        conn.execute(
            r#"INSERT INTO memories
               (id, scope, project_id, title, body, tags_json, enabled, created_at, updated_at)
               VALUES ('user-memory', 'user', NULL, 'PR', '不要 emoji', '[]', 1, 1, 1)"#,
            [],
        )
        .unwrap();
        let mut settings = MemorySettings::default();
        settings.enabled = false;
        assert_eq!(
            build_memory_baseline_core(&conn, "task-1", "C:/repo", &settings).unwrap(),
            None
        );
        settings.enabled = true;
        conn.execute(
            "INSERT INTO memory_injection_states (task_id, enabled, updated_at) VALUES ('task-1', 0, 1)",
            [],
        )
        .unwrap();
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
                "provider": { "codex": { "additionalContext": "existing" } }
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

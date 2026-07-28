/*!
 * Agent 工作过程时间线：把事件持久化到 Desktop SQLite。
 *
 * **事实源边界（#46 / #56）**
 * - Native AgentKit 路径：产品事实在 `lilia-storage` 投影；本模块 SQLite 仅为
 *   rebuildable UI cache（payload 含 `uiCache=true` / `notExecutionFactSource=true`）。
 * - Legacy Node `agent-runner` 路径：仍可能直接写入本表；不得作为 Native 恢复事实源。
 *
 * 本模块只提供契约与存取命令，不负责 NDJSON 映射，也不触碰 chat runner
 * 的 stdout 事件读取循环。
 *
 * 表结构故意只存「事实」字段：kind / status / title / summary / payload。display
 * （图标、中文动词、详情面板）是渲染时的视图缓存，由前端
 * `deriveTimelineDisplay()` 现算 —— 历史事件能跟着 display 规则的迭代自动更新。
 *
 * 排序键：`(turn_seq, intra_turn_order)`。turn_seq 在 task 内单调，按 `turn_id`
 * 首次出现分配；intra_turn_order 在 turn 内单调，按事件落库顺序分配。彻底按
 * turn 隔离意味着「同 sourceId 撞 id」最坏只能让同 turn 内事件互覆盖位置，
 * 不可能再让事件跨 turn 跳到时间线最前 —— 这是从全局单调 `order` 升级过来的
 * 主要动机。
 */

use rusqlite::{params, types::Type, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::State;
use uuid::Uuid;

use crate::agent_timeline_contract;
use crate::store::LiliaStore;
use crate::util::now_millis;

pub(crate) const AGENT_TIMELINE_SCHEMA_SQL: &str = r#"
CREATE TABLE agent_timeline_events (
  id                TEXT PRIMARY KEY,
  task_id           TEXT NOT NULL,
  turn_id           TEXT,
  backend           TEXT NOT NULL,
  kind              TEXT NOT NULL,
  status            TEXT NOT NULL,
  title             TEXT NOT NULL,
  summary           TEXT,
  payload           TEXT NOT NULL,
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL,
  turn_seq          INTEGER NOT NULL,
  intra_turn_order  INTEGER NOT NULL
);
CREATE INDEX idx_agent_timeline_events_task_id_turn
  ON agent_timeline_events(task_id, turn_seq, intra_turn_order);
"#;

pub(crate) fn create_timeline_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(AGENT_TIMELINE_SCHEMA_SQL)
}

pub(crate) fn default_timeline_status() -> &'static str {
    agent_timeline_contract::default_timeline_status()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTimelineEvent {
    pub id: String,
    pub task_id: String,
    pub turn_id: Option<String>,
    /// backend：`"claude" | "codex" | "native-agentkit"`（Native 行为 UI cache）
    pub backend: String,
    /// "reasoning" | "plan" | "todo_list" | "tool" | ...
    pub kind: String,
    pub status: String,
    pub title: String,
    pub summary: Option<String>,
    pub payload: JsonValue,
    pub created_at: i64,
    pub updated_at: i64,
    /// task 内 turn 的单调序号，按 turn_id 首次出现分配。turn_id=NULL 走单独的
    /// "无 turn" 哨兵分组，所有 NULL 事件共享同一个 turn_seq。
    pub turn_seq: i64,
    /// turn 内事件的落库序号，按 (task_id, turn_seq) 单调递增。
    pub intra_turn_order: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelinePageCursor {
    pub turn_seq: i64,
    pub intra_turn_order: i64,
    pub created_at: i64,
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct TimelinePage {
    pub events: Vec<AgentTimelineEvent>,
    pub before_cursor: Option<String>,
    pub after_cursor: Option<String>,
    pub has_more_before: bool,
    pub has_more_after: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTimelineEventInput {
    pub id: Option<String>,
    pub task_id: String,
    pub turn_id: Option<String>,
    pub backend: String,
    pub kind: String,
    pub status: String,
    pub title: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub payload: JsonValue,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentTimelineEvent> {
    let payload_text: String = row.get(8)?;
    let payload = serde_json::from_str(&payload_text)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(8, Type::Text, Box::new(e)))?;
    Ok(AgentTimelineEvent {
        id: row.get(0)?,
        task_id: row.get(1)?,
        turn_id: row.get(2)?,
        backend: row.get(3)?,
        kind: row.get(4)?,
        status: row.get(5)?,
        title: row.get(6)?,
        summary: row.get(7)?,
        payload,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        turn_seq: row.get(11)?,
        intra_turn_order: row.get(12)?,
    })
}

/// 给某个 (task_id, turn_id) 分配 turn_seq：
/// - 已经有同 turn_id 的事件 → 复用既有 turn_seq，turn 内部继续按 intra_turn_order
///   递增；
/// - 没有 → 取 task 内 `MAX(turn_seq) + 1`，第一个 turn 起始为 0。
///
/// `turn_id IS ?2` 用 SQLite 的 NULL-safe 比较，让 NULL 也能稳定分组——所有
/// turn_id=NULL 的事件共享同一个哨兵 turn_seq（实际场景已极少：user message
/// 现在跟 agent turn 共享 turn_id，剩下只有手动构造的兜底事件）。
fn resolve_turn_seq(
    conn: &Connection,
    task_id: &str,
    turn_id: Option<&str>,
    insert_before_created_at: Option<i64>,
) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            r#"SELECT turn_seq FROM agent_timeline_events
               WHERE task_id = ?1 AND turn_id IS ?2
               LIMIT 1"#,
            params![task_id, turn_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("agent_timeline: 查询 turn_seq 失败：{e}"))?;
    if let Some(seq) = existing {
        return Ok(seq);
    }
    if let Some(created_at) = insert_before_created_at {
        let insert_at: Option<i64> = conn
            .query_row(
                r#"SELECT turn_seq
                   FROM (
                     SELECT turn_seq, MIN(created_at) AS first_created_at
                     FROM agent_timeline_events
                     WHERE task_id = ?1
                     GROUP BY turn_seq
                   )
                   WHERE first_created_at > ?2
                   ORDER BY first_created_at ASC, turn_seq ASC
                   LIMIT 1"#,
                params![task_id, created_at],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("agent_timeline: 查询 history 插入位置失败：{e}"))?;
        if let Some(seq) = insert_at {
            conn.execute(
                r#"UPDATE agent_timeline_events
                   SET turn_seq = turn_seq + 1
                   WHERE task_id = ?1 AND turn_seq >= ?2"#,
                params![task_id, seq],
            )
            .map_err(|e| format!("agent_timeline: 移动 history 后续 turn_seq 失败：{e}"))?;
            return Ok(seq);
        }
    }
    let max: Option<i64> = conn
        .query_row(
            r#"SELECT MAX(turn_seq) FROM agent_timeline_events WHERE task_id = ?1"#,
            params![task_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("agent_timeline: 查询 max(turn_seq) 失败：{e}"))?
        .flatten();
    Ok(max.map_or(0, |m| m + 1))
}

fn should_insert_by_created_at(input: &AgentTimelineEventInput) -> bool {
    input
        .payload
        .get("history")
        .and_then(|value| value.as_bool())
        == Some(true)
}

fn next_intra_turn_order(conn: &Connection, task_id: &str, turn_seq: i64) -> Result<i64, String> {
    let max: Option<i64> = conn
        .query_row(
            r#"SELECT MAX(intra_turn_order) FROM agent_timeline_events
               WHERE task_id = ?1 AND turn_seq = ?2"#,
            params![task_id, turn_seq],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("agent_timeline: 查询 max(intra_turn_order) 失败：{e}"))?
        .flatten();
    Ok(max.map_or(0, |m| m + 1))
}

pub fn insert(
    conn: &Connection,
    input: AgentTimelineEventInput,
) -> Result<AgentTimelineEvent, String> {
    let now = now_millis();
    let insert_by_created_at = should_insert_by_created_at(&input);
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let existing: Option<(i64, i64, i64)> = conn
        .query_row(
            r#"SELECT created_at, turn_seq, intra_turn_order
               FROM agent_timeline_events WHERE id = ?1"#,
            params![id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|e| format!("agent_timeline_insert: 查询已有事件位置失败：{e}"))?;

    let requested_created_at = input.created_at.unwrap_or(now);
    let (existing_created_at, turn_seq, intra_turn_order) = match existing {
        Some((created_at, turn_seq, intra)) => (Some(created_at), turn_seq, intra),
        None => {
            let insert_before_created_at = insert_by_created_at.then_some(requested_created_at);
            let turn_seq = resolve_turn_seq(
                conn,
                &input.task_id,
                input.turn_id.as_deref(),
                insert_before_created_at,
            )?;
            let intra = next_intra_turn_order(conn, &input.task_id, turn_seq)?;
            (None, turn_seq, intra)
        }
    };

    let created_at = input.created_at.or(existing_created_at).unwrap_or(now);
    let updated_at = input
        .updated_at
        .unwrap_or(if existing.is_some() { now } else { created_at });
    let payload_text = serde_json::to_string(&input.payload)
        .map_err(|e| format!("agent_timeline_insert: payload 序列化失败：{e}"))?;

    let event = AgentTimelineEvent {
        id,
        task_id: input.task_id,
        turn_id: input.turn_id,
        backend: input.backend,
        kind: input.kind,
        status: input.status,
        title: input.title,
        summary: input.summary,
        payload: input.payload,
        created_at,
        updated_at,
        turn_seq,
        intra_turn_order,
    };

    conn.execute(
        r#"INSERT INTO agent_timeline_events
           (id, task_id, turn_id, backend, kind, status, title, summary, payload,
            created_at, updated_at, turn_seq, intra_turn_order)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
           ON CONFLICT(id) DO UPDATE SET
             task_id          = excluded.task_id,
             turn_id          = excluded.turn_id,
             backend          = excluded.backend,
             kind             = excluded.kind,
             status           = excluded.status,
             title            = excluded.title,
             summary          = excluded.summary,
             payload          = excluded.payload,
             created_at       = excluded.created_at,
             updated_at       = excluded.updated_at,
             turn_seq         = excluded.turn_seq,
             intra_turn_order = excluded.intra_turn_order"#,
        params![
            event.id,
            event.task_id,
            event.turn_id,
            event.backend,
            event.kind,
            event.status,
            event.title,
            event.summary,
            payload_text,
            event.created_at,
            event.updated_at,
            event.turn_seq,
            event.intra_turn_order,
        ],
    )
    .map_err(|e| format!("agent_timeline_insert: 写入失败：{e}"))?;

    Ok(event)
}

pub fn list(conn: &Connection, task_id: &str) -> Result<Vec<AgentTimelineEvent>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT id, task_id, turn_id, backend, kind, status, title, summary,
                      payload, created_at, updated_at, turn_seq, intra_turn_order
               FROM agent_timeline_events
               WHERE task_id = ?1
               ORDER BY turn_seq ASC, intra_turn_order ASC, created_at ASC"#,
        )
        .map_err(|e| format!("agent_timeline_list: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map(params![task_id], row_to_event)
        .map_err(|e| format!("agent_timeline_list: query 失败：{e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("agent_timeline_list: 行解析失败：{e}"))?);
    }
    Ok(out)
}

pub fn list_latest_page(
    conn: &Connection,
    task_id: &str,
    limit: usize,
) -> Result<TimelinePage, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT id, task_id, turn_id, backend, kind, status, title, summary,
                      payload, created_at, updated_at, turn_seq, intra_turn_order
               FROM agent_timeline_events
               WHERE task_id = ?1
               ORDER BY turn_seq DESC, intra_turn_order DESC, created_at DESC, id DESC
               LIMIT ?2"#,
        )
        .map_err(|e| format!("agent_timeline_latest_page: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map(params![task_id, limit as i64], row_to_event)
        .map_err(|e| format!("agent_timeline_latest_page: query 失败：{e}"))?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row.map_err(|e| format!("agent_timeline_latest_page: 行解析失败：{e}"))?);
    }
    events.reverse();
    page_for_events(conn, task_id, events)
}

pub fn list_before_page(
    conn: &Connection,
    task_id: &str,
    cursor: &str,
    limit: usize,
) -> Result<TimelinePage, String> {
    let cursor = cursor_for_event(conn, task_id, cursor)?
        .ok_or_else(|| "agent_timeline: timeline cursor 事件不存在".to_string())?;
    let mut stmt = conn
        .prepare(
            r#"SELECT id, task_id, turn_id, backend, kind, status, title, summary,
                      payload, created_at, updated_at, turn_seq, intra_turn_order
               FROM agent_timeline_events
               WHERE task_id = ?1
                 AND (
                   turn_seq < ?2
                   OR (turn_seq = ?2 AND intra_turn_order < ?3)
                   OR (turn_seq = ?2 AND intra_turn_order = ?3 AND created_at < ?4)
                   OR (turn_seq = ?2 AND intra_turn_order = ?3 AND created_at = ?4 AND id < ?5)
                 )
               ORDER BY turn_seq DESC, intra_turn_order DESC, created_at DESC, id DESC
               LIMIT ?6"#,
        )
        .map_err(|e| format!("agent_timeline_before_page: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map(
            params![
                task_id,
                cursor.turn_seq,
                cursor.intra_turn_order,
                cursor.created_at,
                cursor.id,
                limit as i64
            ],
            row_to_event,
        )
        .map_err(|e| format!("agent_timeline_before_page: query 失败：{e}"))?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row.map_err(|e| format!("agent_timeline_before_page: 行解析失败：{e}"))?);
    }
    events.reverse();
    page_for_events(conn, task_id, events)
}

pub fn list_after_event(
    conn: &Connection,
    task_id: &str,
    event_id: &str,
    limit: Option<usize>,
) -> Result<Vec<AgentTimelineEvent>, String> {
    let Some(cursor) = cursor_for_event(conn, task_id, event_id)? else {
        return list(conn, task_id);
    };
    list_after_cursor_key(conn, task_id, &cursor, limit)
}

pub fn list_after_cursor(
    conn: &Connection,
    task_id: &str,
    cursor: &str,
    limit: Option<usize>,
) -> Result<Vec<AgentTimelineEvent>, String> {
    let Some(cursor) = cursor_for_event(conn, task_id, cursor)? else {
        return list(conn, task_id);
    };
    list_after_cursor_key(conn, task_id, &cursor, limit)
}

fn list_after_cursor_key(
    conn: &Connection,
    task_id: &str,
    cursor: &TimelinePageCursor,
    limit: Option<usize>,
) -> Result<Vec<AgentTimelineEvent>, String> {
    let limit = limit.map(|value| value as i64).unwrap_or(i64::MAX);
    let mut stmt = conn
        .prepare(
            r#"SELECT id, task_id, turn_id, backend, kind, status, title, summary,
                      payload, created_at, updated_at, turn_seq, intra_turn_order
               FROM agent_timeline_events
               WHERE task_id = ?1
                 AND (
                   turn_seq > ?2
                   OR (turn_seq = ?2 AND intra_turn_order > ?3)
                   OR (turn_seq = ?2 AND intra_turn_order = ?3 AND created_at > ?4)
                   OR (turn_seq = ?2 AND intra_turn_order = ?3 AND created_at = ?4 AND id > ?5)
                 )
               ORDER BY turn_seq ASC, intra_turn_order ASC, created_at ASC, id ASC
               LIMIT ?6"#,
        )
        .map_err(|e| format!("agent_timeline_after_event: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map(
            params![
                task_id,
                cursor.turn_seq,
                cursor.intra_turn_order,
                cursor.created_at,
                &cursor.id,
                limit
            ],
            row_to_event,
        )
        .map_err(|e| format!("agent_timeline_after_event: query 失败：{e}"))?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row.map_err(|e| format!("agent_timeline_after_event: 行解析失败：{e}"))?);
    }
    Ok(events)
}

pub fn encode_page_cursor(event: &AgentTimelineEvent) -> String {
    event.id.clone()
}

fn cursor_for_event(
    conn: &Connection,
    task_id: &str,
    event_id: &str,
) -> Result<Option<TimelinePageCursor>, String> {
    conn.query_row(
        r#"SELECT turn_seq, intra_turn_order, created_at, id
           FROM agent_timeline_events
           WHERE task_id = ?1 AND id = ?2"#,
        params![task_id, event_id],
        |row| {
            Ok(TimelinePageCursor {
                turn_seq: row.get(0)?,
                intra_turn_order: row.get(1)?,
                created_at: row.get(2)?,
                id: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("agent_timeline_cursor_for_event: query 失败：{e}"))
}

fn page_for_events(
    conn: &Connection,
    task_id: &str,
    events: Vec<AgentTimelineEvent>,
) -> Result<TimelinePage, String> {
    let before_cursor = events.first().map(encode_page_cursor);
    let after_cursor = events.last().map(encode_page_cursor);
    let has_more_before = events
        .first()
        .map(|event| has_event_before(conn, task_id, event))
        .transpose()?
        .unwrap_or(false);
    let has_more_after = events
        .last()
        .map(|event| has_event_after(conn, task_id, event))
        .transpose()?
        .unwrap_or(false);
    Ok(TimelinePage {
        events,
        before_cursor,
        after_cursor,
        has_more_before,
        has_more_after,
    })
}

fn has_event_before(
    conn: &Connection,
    task_id: &str,
    event: &AgentTimelineEvent,
) -> Result<bool, String> {
    has_event_on_side(conn, task_id, event, "<")
}

fn has_event_after(
    conn: &Connection,
    task_id: &str,
    event: &AgentTimelineEvent,
) -> Result<bool, String> {
    has_event_on_side(conn, task_id, event, ">")
}

fn has_event_on_side(
    conn: &Connection,
    task_id: &str,
    event: &AgentTimelineEvent,
    direction: &str,
) -> Result<bool, String> {
    let comparator = match direction {
        "<" => {
            r#"turn_seq < ?2
               OR (turn_seq = ?2 AND intra_turn_order < ?3)
               OR (turn_seq = ?2 AND intra_turn_order = ?3 AND created_at < ?4)
               OR (turn_seq = ?2 AND intra_turn_order = ?3 AND created_at = ?4 AND id < ?5)"#
        }
        ">" => {
            r#"turn_seq > ?2
               OR (turn_seq = ?2 AND intra_turn_order > ?3)
               OR (turn_seq = ?2 AND intra_turn_order = ?3 AND created_at > ?4)
               OR (turn_seq = ?2 AND intra_turn_order = ?3 AND created_at = ?4 AND id > ?5)"#
        }
        _ => unreachable!(),
    };
    let sql = format!(
        r#"SELECT 1
           FROM agent_timeline_events
           WHERE task_id = ?1 AND ({comparator})
           LIMIT 1"#
    );
    conn.query_row(
        &sql,
        params![
            task_id,
            event.turn_seq,
            event.intra_turn_order,
            event.created_at,
            event.id
        ],
        |_| Ok(()),
    )
    .optional()
    .map(|value| value.is_some())
    .map_err(|e| format!("agent_timeline_has_event: query 失败：{e}"))
}

pub fn clear(conn: &Connection, task_id: &str) -> Result<usize, String> {
    let _ = crate::quota_usage::clear_task_usage(conn, task_id);
    conn.execute(
        "DELETE FROM agent_timeline_events WHERE task_id = ?1",
        params![task_id],
    )
    .map_err(|e| format!("agent_timeline_clear_task: 删除失败：{e}"))
}

pub fn latest_session_id(
    conn: &Connection,
    task_id: &str,
    backend: &str,
) -> Result<Option<String>, String> {
    let mut stmt = conn
        .prepare(
            r#"SELECT payload
               FROM agent_timeline_events
               WHERE task_id = ?1
                 AND backend = ?2
                 AND kind = 'turn'
                 AND payload LIKE '%"sessionId"%'
               ORDER BY updated_at DESC
               LIMIT 100"#,
        )
        .map_err(|e| format!("agent_timeline_latest_session_id: prepare 失败：{e}"))?;
    let rows = stmt
        .query_map(params![task_id, backend], |row| row.get::<_, String>(0))
        .map_err(|e| format!("agent_timeline_latest_session_id: query 失败：{e}"))?;

    for row in rows {
        let payload_text =
            row.map_err(|e| format!("agent_timeline_latest_session_id: row 失败：{e}"))?;
        let Ok(payload) = serde_json::from_str::<JsonValue>(&payload_text) else {
            continue;
        };
        let Some(session_id) = payload.get("sessionId").and_then(|v| v.as_str()) else {
            continue;
        };
        let trimmed = session_id.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
    }
    Ok(None)
}

#[tauri::command]
pub fn agent_timeline_list(
    task_id: String,
    store: State<'_, LiliaStore>,
) -> Result<Vec<AgentTimelineEvent>, String> {
    // #46 / #56 — Desktop default timeline reads product projection first.
    if let Some(events) = crate::native_agent::list_default_timeline_for_task(&task_id)? {
        return Ok(events);
    }
    let conn = store.conn()?;
    list(&conn, &task_id)
}

#[tauri::command]
pub fn agent_timeline_clear_task(
    task_id: String,
    store: State<'_, LiliaStore>,
) -> Result<usize, String> {
    let conn = store.conn()?;
    clear(&conn, &task_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input(
        id: &str,
        task_id: &str,
        turn_id: Option<&str>,
        kind: &str,
        created_at: i64,
    ) -> AgentTimelineEventInput {
        AgentTimelineEventInput {
            id: Some(id.to_string()),
            task_id: task_id.to_string(),
            turn_id: turn_id.map(|s| s.to_string()),
            backend: "claude".to_string(),
            kind: kind.to_string(),
            status: "running".to_string(),
            title: kind.to_string(),
            summary: None,
            payload: json!({}),
            created_at: Some(created_at),
            updated_at: Some(created_at),
        }
    }

    #[test]
    fn first_event_in_task_gets_turn_seq_0_intra_order_0() {
        let conn = Connection::open_in_memory().unwrap();
        create_timeline_schema(&conn).unwrap();

        let saved = insert(
            &conn,
            input("e1", "task-1", Some("turn-a"), "reasoning", 100),
        )
        .unwrap();

        assert_eq!(saved.turn_seq, 0);
        assert_eq!(saved.intra_turn_order, 0);
    }

    #[test]
    fn events_within_same_turn_share_turn_seq_and_increment_intra_order() {
        let conn = Connection::open_in_memory().unwrap();
        create_timeline_schema(&conn).unwrap();

        let a = insert(
            &conn,
            input("e1", "task-1", Some("turn-a"), "reasoning", 100),
        )
        .unwrap();
        let b = insert(&conn, input("e2", "task-1", Some("turn-a"), "command", 110)).unwrap();
        let c = insert(&conn, input("e3", "task-1", Some("turn-a"), "message", 120)).unwrap();

        assert_eq!(a.turn_seq, 0);
        assert_eq!(b.turn_seq, 0);
        assert_eq!(c.turn_seq, 0);
        assert_eq!(a.intra_turn_order, 0);
        assert_eq!(b.intra_turn_order, 1);
        assert_eq!(c.intra_turn_order, 2);
    }

    #[test]
    fn new_turn_id_allocates_next_turn_seq() {
        let conn = Connection::open_in_memory().unwrap();
        create_timeline_schema(&conn).unwrap();

        insert(
            &conn,
            input("e1", "task-1", Some("turn-a"), "reasoning", 100),
        )
        .unwrap();
        let b = insert(
            &conn,
            input("e2", "task-1", Some("turn-b"), "reasoning", 200),
        )
        .unwrap();
        let c = insert(&conn, input("e3", "task-1", Some("turn-b"), "command", 210)).unwrap();

        assert_eq!(b.turn_seq, 1);
        assert_eq!(b.intra_turn_order, 0);
        assert_eq!(c.turn_seq, 1);
        assert_eq!(c.intra_turn_order, 1);
    }

    #[test]
    fn upsert_same_id_keeps_position_even_across_status_changes() {
        let conn = Connection::open_in_memory().unwrap();
        create_timeline_schema(&conn).unwrap();

        // 流式 reasoning：先 running 一条，再用同 id 推一条 success。位置必须稳定，
        // 否则 turn 末尾 finalize 时事件会被推到 turn 最后。
        let first = insert(
            &conn,
            input("e1", "task-1", Some("turn-a"), "reasoning", 100),
        )
        .unwrap();
        insert(&conn, input("e2", "task-1", Some("turn-a"), "command", 110)).unwrap();
        let upserted = {
            let mut next = input("e1", "task-1", Some("turn-a"), "reasoning", 100);
            next.status = "success".to_string();
            insert(&conn, next).unwrap()
        };

        assert_eq!(upserted.turn_seq, first.turn_seq);
        assert_eq!(upserted.intra_turn_order, first.intra_turn_order);
        assert_eq!(upserted.created_at, first.created_at);
    }

    #[test]
    fn turn_seq_isolation_prevents_cross_turn_sourceid_collision() {
        let conn = Connection::open_in_memory().unwrap();
        create_timeline_schema(&conn).unwrap();

        // 模拟 bug 场景：两个不同 turn 误用同一 sourceId（=> 同一 DB id）。
        // 老 schema 下第二 turn 会"继承"第一 turn 的最小 order 被排到时间线最前；
        // 新 schema 下 upsert 保留原 (turn_seq, intra_turn_order)，所以最坏只是
        // 第二 turn 那条更新被无声"覆盖"掉，而不会污染时间线整体次序。
        insert(
            &conn,
            input("dup-source-id", "task-1", Some("turn-a"), "reasoning", 100),
        )
        .unwrap();
        insert(
            &conn,
            input("turn-a-end", "task-1", Some("turn-a"), "message", 110),
        )
        .unwrap();
        insert(
            &conn,
            input("turn-b-start", "task-1", Some("turn-b"), "reasoning", 200),
        )
        .unwrap();
        let collided = insert(&conn, {
            let mut next = input("dup-source-id", "task-1", Some("turn-b"), "reasoning", 210);
            next.status = "success".to_string();
            next
        })
        .unwrap();

        // 撞 id 的事件仍然挂在 turn-a 上（这是 upsert 语义的诚实代价），
        // 但绝不会跳到时间线最前。
        assert_eq!(collided.turn_seq, 0);

        let listed: Vec<_> = list(&conn, "task-1")
            .unwrap()
            .into_iter()
            .map(|e| (e.id, e.turn_seq, e.intra_turn_order))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("dup-source-id".to_string(), 0, 0),
                ("turn-a-end".to_string(), 0, 1),
                ("turn-b-start".to_string(), 1, 0),
            ]
        );
    }

    #[test]
    fn list_orders_by_turn_seq_then_intra_order() {
        let conn = Connection::open_in_memory().unwrap();
        create_timeline_schema(&conn).unwrap();

        // turn_seq 是按 turn_id 首次出现的顺序分配的。下面 turn-2 先入库 → turn_seq=0；
        // turn-1 后入库 → turn_seq=1。最终列表完全按 (turn_seq, intra_turn_order) 排，
        // 不再按 created_at —— 这正是"按 turn 隔离"的核心：迟到的事件只在自己 turn 内
        // 排位，不会因为 created_at 早就插到别的 turn 里。
        insert(
            &conn,
            input("t2-late", "task-1", Some("turn-2"), "message", 300),
        )
        .unwrap();
        insert(
            &conn,
            input("t1-first", "task-1", Some("turn-1"), "reasoning", 100),
        )
        .unwrap();
        insert(
            &conn,
            input("t1-second", "task-1", Some("turn-1"), "command", 200),
        )
        .unwrap();
        insert(
            &conn,
            input("t2-early", "task-1", Some("turn-2"), "reasoning", 400),
        )
        .unwrap();

        let listed: Vec<_> = list(&conn, "task-1")
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(listed, vec!["t2-late", "t2-early", "t1-first", "t1-second"],);
    }

    #[test]
    fn latest_page_returns_tail_in_forward_order() {
        let conn = Connection::open_in_memory().unwrap();
        create_timeline_schema(&conn).unwrap();
        for index in 0..5 {
            insert(
                &conn,
                input(
                    &format!("event-{index}"),
                    "task-1",
                    Some(&format!("turn-{index}")),
                    "message",
                    100 + index,
                ),
            )
            .unwrap();
        }

        let page = list_latest_page(&conn, "task-1", 3).unwrap();

        assert_eq!(
            page.events
                .iter()
                .map(|event| event.id.as_str())
                .collect::<Vec<_>>(),
            vec!["event-2", "event-3", "event-4"],
        );
        assert!(page.has_more_before);
        assert!(!page.has_more_after);
        assert!(page.before_cursor.is_some());
        assert!(page.after_cursor.is_some());
    }

    #[test]
    fn before_page_returns_older_events_and_has_more_before() {
        let conn = Connection::open_in_memory().unwrap();
        create_timeline_schema(&conn).unwrap();
        for index in 0..5 {
            insert(
                &conn,
                input(
                    &format!("event-{index}"),
                    "task-1",
                    Some(&format!("turn-{index}")),
                    "message",
                    100 + index,
                ),
            )
            .unwrap();
        }
        let latest = list_latest_page(&conn, "task-1", 2).unwrap();
        let cursor = latest.before_cursor.unwrap();

        let older = list_before_page(&conn, "task-1", &cursor, 2).unwrap();

        assert_eq!(
            older
                .events
                .iter()
                .map(|event| event.id.as_str())
                .collect::<Vec<_>>(),
            vec!["event-1", "event-2"],
        );
        assert!(older.has_more_before);
        assert!(older.has_more_after);
    }

    #[test]
    fn after_event_returns_only_newer_events() {
        let conn = Connection::open_in_memory().unwrap();
        create_timeline_schema(&conn).unwrap();
        for index in 0..4 {
            insert(
                &conn,
                input(
                    &format!("event-{index}"),
                    "task-1",
                    Some(&format!("turn-{index}")),
                    "message",
                    100 + index,
                ),
            )
            .unwrap();
        }

        let events = list_after_event(&conn, "task-1", "event-1", None).unwrap();

        assert_eq!(
            events
                .iter()
                .map(|event| event.id.as_str())
                .collect::<Vec<_>>(),
            vec!["event-2", "event-3"],
        );
    }

    #[test]
    fn after_cursor_returns_only_newer_events() {
        let conn = Connection::open_in_memory().unwrap();
        create_timeline_schema(&conn).unwrap();
        for index in 0..4 {
            insert(
                &conn,
                input(
                    &format!("event-{index}"),
                    "task-1",
                    Some(&format!("turn-{index}")),
                    "message",
                    100 + index,
                ),
            )
            .unwrap();
        }
        let latest = list_latest_page(&conn, "task-1", 3).unwrap();
        let cursor = encode_page_cursor(&latest.events[0]);

        let events = list_after_cursor(&conn, "task-1", &cursor, Some(2)).unwrap();

        assert_eq!(
            events
                .iter()
                .map(|event| event.id.as_str())
                .collect::<Vec<_>>(),
            vec!["event-2", "event-3"],
        );
    }

    #[test]
    fn history_event_with_older_created_at_is_inserted_before_current_turn() {
        let conn = Connection::open_in_memory().unwrap();
        create_timeline_schema(&conn).unwrap();

        insert(
            &conn,
            input(
                "current-message",
                "task-1",
                Some("turn-current"),
                "message",
                10_000,
            ),
        )
        .unwrap();

        let mut history = input(
            "history-message",
            "task-1",
            Some("codex-turn-old"),
            "message",
            1_000,
        );
        history.backend = "codex".to_string();
        history.payload = json!({
            "history": true,
            "threadId": "thread-1",
            "turnId": "codex-turn-old",
            "itemId": "msg-1"
        });
        let saved = insert(&conn, history).unwrap();

        assert_eq!(saved.turn_seq, 0);
        let listed: Vec<_> = list(&conn, "task-1")
            .unwrap()
            .into_iter()
            .map(|event| (event.id, event.turn_seq, event.intra_turn_order))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("history-message".to_string(), 0, 0),
                ("current-message".to_string(), 1, 0),
            ]
        );
    }

    #[test]
    fn latest_session_id_reads_recent_turn_payload() {
        let conn = Connection::open_in_memory().unwrap();
        create_timeline_schema(&conn).unwrap();

        let mut a = input("turn-old", "task-1", Some("turn-1"), "turn", 100);
        a.payload = json!({ "sessionId": "old-session" });
        insert(&conn, a).unwrap();

        let mut b = input("turn-new", "task-1", Some("turn-2"), "turn", 200);
        b.payload = json!({ "sessionId": "new-session" });
        insert(&conn, b).unwrap();

        assert_eq!(
            latest_session_id(&conn, "task-1", "claude").unwrap(),
            Some("new-session".to_string())
        );
    }
}

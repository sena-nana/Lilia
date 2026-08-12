#[cfg(test)]
use std::collections::BTreeMap;

use rusqlite::{params, Connection};
#[cfg(test)]
use rusqlite::{OptionalExtension, Row, ToSql};
use serde::Deserialize;
#[cfg(test)]
use serde::Serialize;
use serde_json::Value as JsonValue;
use tauri::State;

#[cfg(test)]
use crate::agent_interaction_contract;
use crate::agent_timeline::AgentTimelineEvent;
#[cfg(test)]
use crate::agent_timeline_contract;
use crate::chat::state::try_normalize_backend;
#[cfg(test)]
use crate::quota_usage_contract::{
    default_usage_stats_days, usage_stats_backend_filters, usage_stats_days,
};
use lilia_desktop_application::DesktopApplication;

#[cfg(test)]
const DAY_MS: i64 = 86_400_000;
#[cfg(test)]
const RECENT_LIMIT: i64 = 20;

#[derive(Debug, Clone)]
struct UsageRecord {
    event_id: String,
    task_id: String,
    turn_id: Option<String>,
    backend: String,
    session_id: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_tokens: i64,
    total_tokens: i64,
    known_cost_usd: Option<f64>,
    raw_usage_json: String,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageStatsInput {
    pub days: Option<i64>,
    pub backend: Option<String>,
}

#[cfg(test)]
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageTokenTotals {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageCostCoverage {
    pub known_cost_usd: Option<f64>,
    pub cost_record_count: i64,
    pub total_record_count: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageDailyBucket {
    pub day_start: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
    pub known_cost_usd: Option<f64>,
    pub cost_record_count: i64,
    pub record_count: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageBackendSummary {
    pub backend: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
    pub known_cost_usd: Option<f64>,
    pub cost_record_count: i64,
    pub record_count: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageRecentRecord {
    pub event_id: String,
    pub task_id: String,
    pub turn_id: Option<String>,
    pub backend: String,
    pub session_id: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
    pub known_cost_usd: Option<f64>,
    pub created_at: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageProjectSummary {
    pub project_id: Option<String>,
    pub project_name: String,
    pub project_cwd: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
    pub known_cost_usd: Option<f64>,
    pub cost_record_count: i64,
    pub record_count: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageConversationSummary {
    pub task_id: String,
    pub task_title: String,
    pub task_status: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
    pub known_cost_usd: Option<f64>,
    pub cost_record_count: i64,
    pub record_count: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageToolSummary {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub subkind: Option<String>,
    pub tool_name: Option<String>,
    pub call_count: i64,
    pub share_percent: f64,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageStats {
    pub days: i64,
    pub backend: String,
    pub range_start: i64,
    pub range_end: i64,
    pub totals: QuotaUsageTokenTotals,
    pub cost: QuotaUsageCostCoverage,
    pub daily: Vec<QuotaUsageDailyBucket>,
    pub backends: Vec<QuotaUsageBackendSummary>,
    pub recent: Vec<QuotaUsageRecentRecord>,
    pub projects: Vec<QuotaUsageProjectSummary>,
    pub conversations: Vec<QuotaUsageConversationSummary>,
    pub tools: Vec<QuotaUsageToolSummary>,
}

#[cfg(test)]
#[derive(Default)]
struct AggregatedUsage {
    totals: QuotaUsageTokenTotals,
    known_cost_usd: f64,
    cost_record_count: i64,
    record_count: i64,
}

#[cfg(test)]
impl AggregatedUsage {
    fn add_record(&mut self, record: &UsageRecord) {
        self.totals.input_tokens += record.input_tokens;
        self.totals.output_tokens += record.output_tokens;
        self.totals.cache_read_tokens += record.cache_read_tokens;
        self.totals.cache_creation_tokens += record.cache_creation_tokens;
        self.totals.total_tokens += record.total_tokens;
        self.record_count += 1;
        if let Some(cost) = record.known_cost_usd {
            self.known_cost_usd += cost;
            self.cost_record_count += 1;
        }
    }

    fn known_cost(&self) -> Option<f64> {
        (self.cost_record_count > 0).then_some(self.known_cost_usd)
    }

    fn token_totals(&self) -> QuotaUsageTokenTotals {
        self.totals.clone()
    }
}

#[cfg(test)]
fn usage_daily_bucket(day_start: i64, bucket: AggregatedUsage) -> QuotaUsageDailyBucket {
    let totals = bucket.token_totals();
    QuotaUsageDailyBucket {
        day_start,
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        cache_read_tokens: totals.cache_read_tokens,
        cache_creation_tokens: totals.cache_creation_tokens,
        total_tokens: totals.total_tokens,
        known_cost_usd: bucket.known_cost(),
        cost_record_count: bucket.cost_record_count,
        record_count: bucket.record_count,
    }
}

#[cfg(test)]
fn usage_backend_summary(backend: String, summary: AggregatedUsage) -> QuotaUsageBackendSummary {
    let totals = summary.token_totals();
    QuotaUsageBackendSummary {
        backend,
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        cache_read_tokens: totals.cache_read_tokens,
        cache_creation_tokens: totals.cache_creation_tokens,
        total_tokens: totals.total_tokens,
        known_cost_usd: summary.known_cost(),
        cost_record_count: summary.cost_record_count,
        record_count: summary.record_count,
    }
}

#[cfg(test)]
fn usage_project_summary(
    project_id: Option<String>,
    project_name: String,
    project_cwd: Option<String>,
    summary: AggregatedUsage,
) -> QuotaUsageProjectSummary {
    let totals = summary.token_totals();
    QuotaUsageProjectSummary {
        project_id,
        project_name,
        project_cwd,
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        cache_read_tokens: totals.cache_read_tokens,
        cache_creation_tokens: totals.cache_creation_tokens,
        total_tokens: totals.total_tokens,
        known_cost_usd: summary.known_cost(),
        cost_record_count: summary.cost_record_count,
        record_count: summary.record_count,
    }
}

#[cfg(test)]
fn usage_conversation_summary(
    task_id: String,
    task_title: String,
    task_status: String,
    project_id: Option<String>,
    project_name: Option<String>,
    summary: AggregatedUsage,
) -> QuotaUsageConversationSummary {
    let totals = summary.token_totals();
    QuotaUsageConversationSummary {
        task_id,
        task_title,
        task_status,
        project_id,
        project_name,
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        cache_read_tokens: totals.cache_read_tokens,
        cache_creation_tokens: totals.cache_creation_tokens,
        total_tokens: totals.total_tokens,
        known_cost_usd: summary.known_cost(),
        cost_record_count: summary.cost_record_count,
        record_count: summary.record_count,
    }
}

pub(crate) fn record_from_timeline_event(
    conn: &Connection,
    event: &AgentTimelineEvent,
) -> Result<(), String> {
    let Some(record) = extract_usage_record(event)? else {
        return Ok(());
    };
    conn.execute(
        r#"INSERT INTO agent_usage_records
           (event_id, task_id, turn_id, backend, session_id,
            input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
            total_tokens, known_cost_usd, raw_usage_json, created_at, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
           ON CONFLICT(event_id) DO UPDATE SET
             task_id               = excluded.task_id,
             turn_id               = excluded.turn_id,
             backend               = excluded.backend,
             session_id            = excluded.session_id,
             input_tokens          = excluded.input_tokens,
             output_tokens         = excluded.output_tokens,
             cache_read_tokens     = excluded.cache_read_tokens,
             cache_creation_tokens = excluded.cache_creation_tokens,
             total_tokens          = excluded.total_tokens,
             known_cost_usd        = excluded.known_cost_usd,
             raw_usage_json        = excluded.raw_usage_json,
             created_at            = excluded.created_at,
             updated_at            = excluded.updated_at"#,
        params![
            record.event_id,
            record.task_id,
            record.turn_id,
            record.backend,
            record.session_id,
            record.input_tokens,
            record.output_tokens,
            record.cache_read_tokens,
            record.cache_creation_tokens,
            record.total_tokens,
            record.known_cost_usd,
            record.raw_usage_json,
            record.created_at,
            record.updated_at,
        ],
    )
    .map(|_| ())
    .map_err(|e| format!("quota_usage: 写入 usage 记录失败：{e}"))
}

pub(crate) fn clear_task_usage(conn: &Connection, task_id: &str) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM agent_usage_records WHERE task_id = ?1",
        params![task_id],
    )
    .map_err(|e| format!("quota_usage: 清理任务 usage 记录失败：{e}"))
}

fn extract_usage_record(event: &AgentTimelineEvent) -> Result<Option<UsageRecord>, String> {
    let Some(payload) = event.payload.as_object() else {
        return Ok(None);
    };
    let Some(usage) = payload.get("usage") else {
        return Ok(None);
    };
    if !usage.is_object() {
        return Ok(None);
    }

    let input_tokens = int_field(
        usage,
        &[
            "inputTokens",
            "input_tokens",
            "promptTokens",
            "prompt_tokens",
        ],
    );
    let output_tokens = int_field(
        usage,
        &[
            "outputTokens",
            "output_tokens",
            "completionTokens",
            "completion_tokens",
        ],
    );
    let cache_read_tokens = int_field(
        usage,
        &[
            "cacheReadTokens",
            "cache_read_tokens",
            "cacheReadInputTokens",
            "cache_read_input_tokens",
            "cachedInputTokens",
            "cached_input_tokens",
        ],
    );
    let cache_creation_tokens = int_field(
        usage,
        &[
            "cacheCreationTokens",
            "cache_creation_tokens",
            "cacheCreationInputTokens",
            "cache_creation_input_tokens",
        ],
    );
    let explicit_total = int_field(usage, &["totalTokens", "total_tokens"]);
    let token_sum = input_tokens + output_tokens + cache_read_tokens + cache_creation_tokens;
    let total_tokens = if explicit_total > 0 {
        explicit_total.max(token_sum)
    } else {
        token_sum
    };
    let known_cost_usd = number_field(&event.payload, &["totalCostUsd", "total_cost_usd"])
        .or_else(|| number_field(usage, &["totalCostUsd", "total_cost_usd"]))
        .or_else(|| number_field(&event.payload, &["costUsd", "cost_usd"]))
        .or_else(|| number_field(usage, &["costUsd", "cost_usd"]))
        .or_else(|| number_field(&event.payload, &["estimatedCostUsd", "estimated_cost_usd"]))
        .or_else(|| number_field(usage, &["estimatedCostUsd", "estimated_cost_usd"]));

    if total_tokens == 0 && known_cost_usd.is_none() {
        return Ok(None);
    }

    let raw_usage_json =
        serde_json::to_string(usage).map_err(|e| format!("quota_usage: usage 序列化失败：{e}"))?;
    Ok(Some(UsageRecord {
        event_id: event.id.clone(),
        task_id: event.task_id.clone(),
        turn_id: event.turn_id.clone(),
        backend: normalize_backend(&event.backend).unwrap_or_else(|| event.backend.clone()),
        session_id: string_field(&event.payload, &["sessionId", "session_id"]),
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        total_tokens,
        known_cost_usd,
        raw_usage_json,
        created_at: event.created_at,
        updated_at: event.updated_at,
    }))
}

fn int_field(value: &JsonValue, keys: &[&str]) -> i64 {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(json_int)
        .unwrap_or(0)
}

fn json_int(value: &JsonValue) -> Option<i64> {
    if let Some(n) = value.as_i64() {
        return (n > 0).then_some(n);
    }
    value
        .as_u64()
        .and_then(|n| i64::try_from(n).ok())
        .filter(|n| *n > 0)
}

fn number_field(value: &JsonValue, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(|value| value.as_f64())
        .filter(|n| n.is_finite() && *n >= 0.0)
}

fn string_field(value: &JsonValue, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

fn normalize_backend(value: &str) -> Option<String> {
    try_normalize_backend(value).map(str::to_string)
}

#[cfg(test)]
fn normalize_days(value: Option<i64>) -> i64 {
    if let Some(days) = value {
        if usage_stats_days().contains(&days) {
            return days;
        }
    }
    default_usage_stats_days()
}

#[cfg(test)]
fn normalize_backend_filter(value: Option<String>) -> String {
    let value = value.as_deref().map(str::trim).unwrap_or_default();
    let backend_filters = usage_stats_backend_filters();
    if backend_filters.iter().any(|filter| filter == value) {
        value.to_string()
    } else if let Some(backend) = try_normalize_backend(value) {
        backend.to_string()
    } else {
        backend_filters
            .first()
            .cloned()
            .unwrap_or_else(|| "all".to_string())
    }
}

#[cfg(test)]
fn day_start(timestamp: i64) -> i64 {
    timestamp.div_euclid(DAY_MS) * DAY_MS
}

#[cfg(test)]
fn load_usage_records(
    conn: &Connection,
    range_start: i64,
    range_end: i64,
    backend: &str,
) -> Result<Vec<UsageRecord>, String> {
    let mut sql = String::from(
        r#"SELECT event_id, task_id, turn_id, backend, session_id,
                  input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                  total_tokens, known_cost_usd, raw_usage_json, created_at, updated_at
           FROM agent_usage_records
           WHERE created_at >= ?1 AND created_at < ?2"#,
    );
    if backend != "all" {
        sql.push_str(" AND backend = ?3");
    }
    sql.push_str(" ORDER BY created_at ASC");

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("quota_usage_get_stats: prepare 失败：{e}"))?;
    let mut rows = if backend == "all" {
        stmt.query(params![range_start, range_end])
    } else {
        stmt.query(params![range_start, range_end, backend])
    }
    .map_err(|e| format!("quota_usage_get_stats: query 失败：{e}"))?;

    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| format!("quota_usage_get_stats: 读取行失败：{e}"))?
    {
        out.push(row_to_usage_record(row)?);
    }
    Ok(out)
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct TaskUsageContext {
    task_title: String,
    task_status: String,
    project_id: Option<String>,
    project_name: Option<String>,
    project_cwd: Option<String>,
}

#[cfg(test)]
fn load_task_contexts(
    conn: &Connection,
    records: &[UsageRecord],
) -> Result<BTreeMap<String, TaskUsageContext>, String> {
    let mut contexts = BTreeMap::new();
    if records.is_empty() {
        return Ok(contexts);
    }

    let mut stmt = conn
        .prepare(
            r#"SELECT t.title, t.status, t.project_id, p.name, p.cwd
               FROM tasks t
               LEFT JOIN projects p ON p.id = t.project_id
               WHERE t.id = ?1"#,
        )
        .map_err(|e| format!("quota_usage_get_stats: prepare task context 失败：{e}"))?;

    for record in records {
        if contexts.contains_key(&record.task_id) {
            continue;
        }
        let row: Option<(
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = stmt
            .query_row(params![&record.task_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .optional()
            .map_err(|e| format!("quota_usage_get_stats: task context 失败：{e}"))?;
        let (title, status, project_id, project_name, project_cwd) = row.unwrap_or((
            "未知对话".to_string(),
            "waiting".to_string(),
            None,
            None,
            None,
        ));
        contexts.insert(
            record.task_id.clone(),
            TaskUsageContext {
                task_title: if title.trim().is_empty() {
                    "未命名对话".to_string()
                } else {
                    title
                },
                task_status: status,
                project_id,
                project_name,
                project_cwd,
            },
        );
    }
    Ok(contexts)
}

#[cfg(test)]
fn load_project_summaries(
    records: &[UsageRecord],
    contexts: &BTreeMap<String, TaskUsageContext>,
) -> Vec<QuotaUsageProjectSummary> {
    #[derive(Default)]
    struct ProjectBucket {
        project_id: Option<String>,
        project_name: String,
        project_cwd: Option<String>,
        usage: AggregatedUsage,
    }

    let mut buckets: BTreeMap<String, ProjectBucket> = BTreeMap::new();
    for record in records {
        let context = contexts.get(&record.task_id);
        let project_id = context.and_then(|value| value.project_id.clone());
        let key = project_id
            .clone()
            .unwrap_or_else(|| "__unassigned__".to_string());
        let bucket = buckets.entry(key).or_insert_with(|| ProjectBucket {
            project_id: project_id.clone(),
            project_name: if project_id.is_some() {
                let project_name = context.and_then(|value| value.project_name.clone());
                if project_name.as_deref().unwrap_or("").trim().is_empty() {
                    "未命名项目".to_string()
                } else {
                    project_name.unwrap()
                }
            } else {
                "未归属项目".to_string()
            },
            project_cwd: context.and_then(|value| value.project_cwd.clone()),
            usage: AggregatedUsage::default(),
        });
        bucket.usage.add_record(record);
    }

    let mut out: Vec<_> = buckets
        .into_values()
        .map(|bucket| {
            usage_project_summary(
                bucket.project_id,
                bucket.project_name,
                bucket.project_cwd,
                bucket.usage,
            )
        })
        .collect();
    out.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| b.record_count.cmp(&a.record_count))
            .then_with(|| a.project_name.cmp(&b.project_name))
    });
    out
}

#[cfg(test)]
fn load_conversation_summaries(
    records: &[UsageRecord],
    contexts: &BTreeMap<String, TaskUsageContext>,
) -> Vec<QuotaUsageConversationSummary> {
    #[derive(Default)]
    struct ConversationBucket {
        task_id: String,
        task_title: String,
        task_status: String,
        project_id: Option<String>,
        project_name: Option<String>,
        usage: AggregatedUsage,
    }

    let mut buckets: BTreeMap<String, ConversationBucket> = BTreeMap::new();
    for record in records {
        let context = contexts.get(&record.task_id);
        let bucket = buckets
            .entry(record.task_id.clone())
            .or_insert_with(|| ConversationBucket {
                task_id: record.task_id.clone(),
                task_title: context
                    .map(|value| value.task_title.clone())
                    .unwrap_or_else(|| "未知对话".to_string()),
                task_status: context
                    .map(|value| value.task_status.clone())
                    .unwrap_or_else(|| "waiting".to_string()),
                project_id: context.and_then(|value| value.project_id.clone()),
                project_name: context.and_then(|value| value.project_name.clone()),
                usage: AggregatedUsage::default(),
            });
        bucket.usage.add_record(record);
    }

    let mut out: Vec<_> = buckets
        .into_values()
        .map(|bucket| {
            usage_conversation_summary(
                bucket.task_id,
                bucket.task_title,
                bucket.task_status,
                bucket.project_id,
                bucket.project_name,
                bucket.usage,
            )
        })
        .collect();
    out.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| b.record_count.cmp(&a.record_count))
            .then_with(|| a.task_title.cmp(&b.task_title))
    });
    out
}

#[cfg(test)]
fn load_tool_summaries(
    conn: &Connection,
    range_start: i64,
    range_end: i64,
    backend: &str,
) -> Result<Vec<QuotaUsageToolSummary>, String> {
    let timeline_kinds = agent_timeline_contract::agent_timeline_tool_window_kinds();
    let kind_placeholders = vec!["?"; timeline_kinds.len()].join(",");
    let mut sql = format!(
        r#"SELECT kind, payload, COUNT(*)
           FROM agent_timeline_events
           WHERE created_at >= ?1 AND created_at < ?2
             AND kind IN ({kind_placeholders})"#,
    );
    if backend != "all" {
        sql.push_str(&format!(" AND backend = ?{}", timeline_kinds.len() + 3));
    }
    sql.push_str(" GROUP BY kind, payload");

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("quota_usage_get_stats: prepare tool stats 失败：{e}"))?;
    let mut query_params: Vec<&dyn ToSql> = Vec::with_capacity(timeline_kinds.len() + 3);
    query_params.push(&range_start);
    query_params.push(&range_end);
    for kind in timeline_kinds {
        query_params.push(kind);
    }
    if backend != "all" {
        query_params.push(&backend);
    }
    let mut rows = stmt
        .query(query_params.as_slice())
        .map_err(|e| format!("quota_usage_get_stats: query tool stats 失败：{e}"))?;

    let mut counts: BTreeMap<String, (String, Option<String>, Option<String>, i64)> =
        BTreeMap::new();
    let mut total = 0_i64;
    while let Some(row) = rows
        .next()
        .map_err(|e| format!("quota_usage_get_stats: 读取 tool stats 失败：{e}"))?
    {
        let kind: String = row
            .get(0)
            .map_err(|e| format!("quota_usage_get_stats: tool kind 失败：{e}"))?;
        let payload_text: String = row
            .get(1)
            .map_err(|e| format!("quota_usage_get_stats: tool payload 失败：{e}"))?;
        let count: i64 = row
            .get(2)
            .map_err(|e| format!("quota_usage_get_stats: tool count 失败：{e}"))?;
        let payload = serde_json::from_str::<JsonValue>(&payload_text).unwrap_or(JsonValue::Null);
        let subkind = string_field(&payload, &["subkind"]);
        let tool_name = string_field(&payload, &["toolName", "tool", "hookName", "name"]);
        let key = [
            kind.as_str(),
            subkind.as_deref().unwrap_or(""),
            tool_name.as_deref().unwrap_or(""),
        ]
        .join(":");
        let entry = counts
            .entry(key)
            .or_insert_with(|| (kind.clone(), subkind.clone(), tool_name.clone(), 0));
        entry.3 += count;
        total += count;
    }

    let mut out: Vec<_> = counts
        .into_iter()
        .map(|(key, (kind, subkind, tool_name, call_count))| {
            let label = quota_tool_label(&kind, subkind.as_deref(), tool_name.as_deref());
            QuotaUsageToolSummary {
                key,
                label,
                kind,
                subkind,
                tool_name,
                call_count,
                share_percent: if total > 0 {
                    (call_count as f64 / total as f64) * 100.0
                } else {
                    0.0
                },
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.call_count
            .cmp(&a.call_count)
            .then_with(|| a.label.cmp(&b.label))
    });
    Ok(out)
}

#[cfg(test)]
fn quota_tool_label(kind: &str, subkind: Option<&str>, tool_name: Option<&str>) -> String {
    if let Some(tool) = tool_name.filter(|value| !value.trim().is_empty()) {
        return tool.to_string();
    }
    if kind == agent_interaction_contract::ask_user_interaction_kind() {
        return "询问用户".to_string();
    }
    if kind == agent_interaction_contract::architecture_interaction_kind() {
        return "架构变更".to_string();
    }
    match (kind, subkind) {
        ("command", Some("lilia_edit_exec")) => "执行已编辑命令".to_string(),
        ("command", _) => "命令".to_string(),
        ("file_read", _) => "读取文件".to_string(),
        ("file_change", Some("write")) => "写入文件".to_string(),
        ("file_change", Some("multi_edit")) => "批量修改".to_string(),
        ("file_change", _) => "修改文件".to_string(),
        ("search", Some("web")) => "网络搜索".to_string(),
        ("search", Some("grep")) => "内容搜索".to_string(),
        ("search", Some("glob")) => "文件查找".to_string(),
        ("search", _) => "搜索".to_string(),
        ("web_fetch", _) => "抓取网页".to_string(),
        ("subagent", _) => "子代理".to_string(),
        ("plan", _) => "计划".to_string(),
        ("todo_list", _) => "待办".to_string(),
        ("mcp", _) => "MCP 工具".to_string(),
        ("tool", Some("hook")) => "Hook".to_string(),
        ("tool", _) => "工具".to_string(),
        _ => kind.to_string(),
    }
}

#[cfg(test)]
fn row_to_usage_record(row: &Row<'_>) -> Result<UsageRecord, String> {
    Ok(UsageRecord {
        event_id: row
            .get(0)
            .map_err(|e| format!("quota_usage_get_stats: event_id 失败：{e}"))?,
        task_id: row
            .get(1)
            .map_err(|e| format!("quota_usage_get_stats: task_id 失败：{e}"))?,
        turn_id: row
            .get(2)
            .map_err(|e| format!("quota_usage_get_stats: turn_id 失败：{e}"))?,
        backend: row
            .get(3)
            .map_err(|e| format!("quota_usage_get_stats: backend 失败：{e}"))?,
        session_id: row
            .get(4)
            .map_err(|e| format!("quota_usage_get_stats: session_id 失败：{e}"))?,
        input_tokens: row
            .get(5)
            .map_err(|e| format!("quota_usage_get_stats: input_tokens 失败：{e}"))?,
        output_tokens: row
            .get(6)
            .map_err(|e| format!("quota_usage_get_stats: output_tokens 失败：{e}"))?,
        cache_read_tokens: row
            .get(7)
            .map_err(|e| format!("quota_usage_get_stats: cache_read_tokens 失败：{e}"))?,
        cache_creation_tokens: row
            .get(8)
            .map_err(|e| format!("quota_usage_get_stats: cache_creation_tokens 失败：{e}"))?,
        total_tokens: row
            .get(9)
            .map_err(|e| format!("quota_usage_get_stats: total_tokens 失败：{e}"))?,
        known_cost_usd: row
            .get(10)
            .map_err(|e| format!("quota_usage_get_stats: known_cost_usd 失败：{e}"))?,
        raw_usage_json: row
            .get(11)
            .map_err(|e| format!("quota_usage_get_stats: raw_usage_json 失败：{e}"))?,
        created_at: row
            .get(12)
            .map_err(|e| format!("quota_usage_get_stats: created_at 失败：{e}"))?,
        updated_at: row
            .get(13)
            .map_err(|e| format!("quota_usage_get_stats: updated_at 失败：{e}"))?,
    })
}

#[cfg(test)]
pub(crate) fn stats(
    conn: &Connection,
    input: QuotaUsageStatsInput,
    now: i64,
) -> Result<QuotaUsageStats, String> {
    let days = normalize_days(input.days);
    let backend = normalize_backend_filter(input.backend);
    let range_end = day_start(now) + DAY_MS;
    let range_start = range_end - days * DAY_MS;
    let records = load_usage_records(conn, range_start, range_end, &backend)?;
    let task_contexts = load_task_contexts(conn, &records)?;
    let projects = load_project_summaries(&records, &task_contexts);
    let conversations = load_conversation_summaries(&records, &task_contexts);
    let tools = load_tool_summaries(conn, range_start, range_end, &backend)?;

    let mut total = AggregatedUsage::default();
    let mut daily: BTreeMap<i64, AggregatedUsage> = (0..days)
        .map(|offset| (range_start + offset * DAY_MS, AggregatedUsage::default()))
        .collect();
    let mut by_backend: BTreeMap<String, AggregatedUsage> = BTreeMap::new();

    for record in &records {
        total.add_record(record);
        daily
            .entry(day_start(record.created_at))
            .or_default()
            .add_record(record);
        by_backend
            .entry(record.backend.clone())
            .or_default()
            .add_record(record);
    }

    let daily = daily
        .into_iter()
        .map(|(day_start, bucket)| usage_daily_bucket(day_start, bucket))
        .collect();

    let backends = by_backend
        .into_iter()
        .map(|(backend, summary)| usage_backend_summary(backend, summary))
        .collect();

    let mut recent: Vec<_> = records
        .iter()
        .rev()
        .take(RECENT_LIMIT as usize)
        .map(|record| QuotaUsageRecentRecord {
            event_id: record.event_id.clone(),
            task_id: record.task_id.clone(),
            turn_id: record.turn_id.clone(),
            backend: record.backend.clone(),
            session_id: record.session_id.clone(),
            input_tokens: record.input_tokens,
            output_tokens: record.output_tokens,
            cache_read_tokens: record.cache_read_tokens,
            cache_creation_tokens: record.cache_creation_tokens,
            total_tokens: record.total_tokens,
            known_cost_usd: record.known_cost_usd,
            created_at: record.created_at,
        })
        .collect();
    recent.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(QuotaUsageStats {
        days,
        backend,
        range_start,
        range_end,
        cost: QuotaUsageCostCoverage {
            known_cost_usd: total.known_cost(),
            cost_record_count: total.cost_record_count,
            total_record_count: total.record_count,
        },
        totals: total.totals,
        daily,
        backends,
        recent,
        projects,
        conversations,
        tools,
    })
}

#[tauri::command]
pub fn quota_usage_get_stats(
    input: Option<QuotaUsageStatsInput>,
    application: State<'_, DesktopApplication>,
) -> Result<lilia_desktop_application::QuotaUsageStats, String> {
    let input = input.unwrap_or(QuotaUsageStatsInput {
        days: None,
        backend: None,
    });
    application
        .quota_usage_stats(lilia_desktop_application::QuotaUsageStatsInput {
            days: input.days,
            backend: input.backend,
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_timeline;
    use crate::native_agent::BACKEND_NATIVE_AGENTKIT;
    use crate::BACKEND_CLAUDE;
    use serde_json::json;

    fn create_usage_schema(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE projects (
              id         TEXT PRIMARY KEY,
              name       TEXT NOT NULL,
              cwd        TEXT,
              created_at INTEGER NOT NULL,
              sort_order INTEGER NOT NULL DEFAULT 0,
              pinned     INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE tasks (
              id          TEXT PRIMARY KEY,
              project_id  TEXT,
              session_id  TEXT NOT NULL,
              title       TEXT NOT NULL,
              title_source TEXT NOT NULL DEFAULT 'auto',
              status      TEXT NOT NULL DEFAULT 'waiting',
              created_at  INTEGER NOT NULL,
              parent_id   TEXT,
              archived    INTEGER NOT NULL DEFAULT 0,
              sort_order  INTEGER NOT NULL DEFAULT 0,
              pinned      INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE agent_usage_records (
              event_id              TEXT PRIMARY KEY,
              task_id               TEXT NOT NULL,
              turn_id               TEXT,
              backend               TEXT NOT NULL CHECK (
                backend IN ('native-agentkit','claude','codex')
              ),
              session_id            TEXT,
              input_tokens          INTEGER NOT NULL DEFAULT 0,
              output_tokens         INTEGER NOT NULL DEFAULT 0,
              cache_read_tokens     INTEGER NOT NULL DEFAULT 0,
              cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
              total_tokens          INTEGER NOT NULL DEFAULT 0,
              known_cost_usd        REAL,
              raw_usage_json        TEXT NOT NULL,
              created_at            INTEGER NOT NULL,
              updated_at            INTEGER NOT NULL
            );
            CREATE INDEX idx_agent_usage_records_created_at
              ON agent_usage_records(created_at);
            CREATE INDEX idx_agent_usage_records_backend_created
              ON agent_usage_records(backend, created_at);
            INSERT INTO projects (id, name, cwd, created_at)
              VALUES ('project-1', 'Lilia', 'C:/repo', 1);
            INSERT INTO tasks (id, project_id, session_id, title, status, created_at)
              VALUES ('task-1', 'project-1', 'task-1', '额度统计', 'running', 1);
            "#,
        )
        .unwrap();
        agent_timeline::create_timeline_schema(conn).unwrap();
    }

    fn usage_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_usage_schema(&conn);
        conn
    }

    fn timeline_event(
        id: &str,
        backend: &str,
        payload: JsonValue,
        created_at: i64,
    ) -> AgentTimelineEvent {
        AgentTimelineEvent {
            id: id.to_string(),
            task_id: "task-1".to_string(),
            turn_id: Some("turn-1".to_string()),
            backend: backend.to_string(),
            kind: "turn".to_string(),
            status: "success".to_string(),
            title: "turn".to_string(),
            summary: None,
            payload,
            created_at,
            updated_at: created_at,
            turn_seq: 0,
            intra_turn_order: 0,
        }
    }

    fn insert_tool_event(
        conn: &Connection,
        id: &str,
        kind: &str,
        title: &str,
        payload: JsonValue,
        created_at: i64,
        order: i64,
    ) {
        conn.execute(
            r#"INSERT INTO agent_timeline_events
               (id, task_id, turn_id, backend, kind, status, title, payload, created_at, updated_at, turn_seq, intra_turn_order)
               VALUES (?1, 'task-1', 'turn-1', 'native-agentkit', ?2, 'success', ?3, ?4, ?5, ?5, 0, ?6)"#,
            params![id, kind, title, payload.to_string(), created_at, order],
        )
        .unwrap();
    }

    #[test]
    fn extracts_claude_snake_case_usage_and_cost() {
        let event = timeline_event(
            "event-1",
            BACKEND_CLAUDE,
            json!({
                "sessionId": "claude-session",
                "totalCostUsd": 0.12,
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 25,
                    "cache_read_input_tokens": 10,
                    "cache_creation_input_tokens": 5
                }
            }),
            1_000,
        );

        let record = extract_usage_record(&event).unwrap().unwrap();

        assert_eq!(record.input_tokens, 100);
        assert_eq!(record.output_tokens, 25);
        assert_eq!(record.cache_read_tokens, 10);
        assert_eq!(record.cache_creation_tokens, 5);
        assert_eq!(record.total_tokens, 140);
        assert_eq!(record.known_cost_usd, Some(0.12));
        assert_eq!(record.session_id.as_deref(), Some("claude-session"));
    }

    #[test]
    fn extracts_native_camel_case_usage_without_cost() {
        let event = timeline_event(
            "event-2",
            BACKEND_NATIVE_AGENTKIT,
            json!({
                "sessionId": "thread-1",
                "usage": {
                    "inputTokens": 200,
                    "outputTokens": 50,
                    "cacheReadTokens": 20,
                    "totalTokens": 250
                }
            }),
            1_000,
        );

        let record = extract_usage_record(&event).unwrap().unwrap();

        assert_eq!(record.input_tokens, 200);
        assert_eq!(record.output_tokens, 50);
        assert_eq!(record.cache_read_tokens, 20);
        assert_eq!(record.total_tokens, 270);
        assert_eq!(record.known_cost_usd, None);
    }

    #[test]
    fn backend_normalization_uses_chat_backend_contract() {
        assert_eq!(usage_stats_days(), &[7, 30]);
        assert_eq!(normalize_days(None), default_usage_stats_days());
        assert_eq!(normalize_days(Some(30)), 30);
        assert_eq!(normalize_days(Some(90)), default_usage_stats_days());
        assert_eq!(
            normalize_backend(&format!(" {BACKEND_NATIVE_AGENTKIT} ")).as_deref(),
            Some(BACKEND_NATIVE_AGENTKIT)
        );
        assert_eq!(normalize_backend("unknown"), None);
        // Removed brand backends are not product filters; fall back to "all".
        assert_eq!(
            normalize_backend_filter(Some(format!(" {BACKEND_CLAUDE} "))),
            "all"
        );
        assert_eq!(
            normalize_backend_filter(Some(format!(" {BACKEND_NATIVE_AGENTKIT} "))),
            BACKEND_NATIVE_AGENTKIT
        );
        assert_eq!(normalize_backend_filter(Some(" all ".to_string())), "all");
        assert_eq!(normalize_backend_filter(Some("unknown".to_string())), "all");
    }

    #[test]
    fn upserts_usage_record_for_same_event() {
        let conn = usage_conn();

        let first = timeline_event(
            "event-1",
            BACKEND_NATIVE_AGENTKIT,
            json!({ "usage": { "inputTokens": 10, "outputTokens": 5 } }),
            1_000,
        );
        record_from_timeline_event(&conn, &first).unwrap();
        let updated = timeline_event(
            "event-1",
            BACKEND_NATIVE_AGENTKIT,
            json!({ "usage": { "inputTokens": 20, "outputTokens": 7, "costUsd": 0.03 } }),
            2_000,
        );
        record_from_timeline_event(&conn, &updated).unwrap();

        let row: (i64, i64, Option<f64>, i64) = conn
            .query_row(
                "SELECT input_tokens, output_tokens, known_cost_usd, created_at FROM agent_usage_records WHERE event_id = 'event-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(row, (20, 7, Some(0.03), 2_000));
    }

    #[test]
    fn stats_aggregate_range_backend_filter_and_cost_coverage() {
        let conn = usage_conn();
        let day = DAY_MS;
        for event in [
            timeline_event(
                "native-cost",
                BACKEND_NATIVE_AGENTKIT,
                json!({ "totalCostUsd": 0.1, "usage": { "input_tokens": 100, "output_tokens": 10 } }),
                day * 10 + 1,
            ),
            timeline_event(
                "native-1",
                BACKEND_NATIVE_AGENTKIT,
                json!({ "usage": { "inputTokens": 50, "outputTokens": 20 } }),
                day * 11 + 1,
            ),
            timeline_event(
                "legacy-brand",
                BACKEND_CLAUDE,
                json!({ "totalCostUsd": 0.2, "usage": { "input_tokens": 999 } }),
                day,
            ),
        ] {
            record_from_timeline_event(&conn, &event).unwrap();
        }

        let all = stats(
            &conn,
            QuotaUsageStatsInput {
                days: Some(7),
                backend: None,
            },
            day * 12 + 5,
        )
        .unwrap();

        // Recent 7-day window: native-cost (110) + native-1 (70) = 180; legacy-brand is outside window.
        assert_eq!(all.totals.total_tokens, 180);
        assert_eq!(all.cost.known_cost_usd, Some(0.1));
        assert_eq!(all.cost.cost_record_count, 1);
        assert_eq!(all.cost.total_record_count, 2);
        assert_eq!(all.daily.len(), 7);
        assert_eq!(all.backends.len(), 1);
        assert_eq!(all.recent.len(), 2);

        let native = stats(
            &conn,
            QuotaUsageStatsInput {
                days: Some(7),
                backend: Some(BACKEND_NATIVE_AGENTKIT.to_string()),
            },
            day * 12 + 5,
        )
        .unwrap();
        assert_eq!(native.totals.total_tokens, 180);
        assert_eq!(native.cost.known_cost_usd, Some(0.1));
        assert_eq!(native.backends.len(), 1);
        assert_eq!(native.backends[0].backend, BACKEND_NATIVE_AGENTKIT);
        assert_eq!(all.projects.len(), 1);
        assert_eq!(all.projects[0].project_name, "Lilia");
        assert_eq!(all.conversations.len(), 1);
        assert_eq!(all.conversations[0].task_title, "额度统计");
    }

    #[test]
    fn stats_counts_tool_activity_without_token_attribution() {
        let conn = usage_conn();
        let day = DAY_MS;
        insert_tool_event(
            &conn,
            "tool-1",
            "command",
            "Bash",
            json!({ "toolName": "Bash" }),
            day * 10 + 2,
            0,
        );
        insert_tool_event(
            &conn,
            "tool-2",
            "search",
            "Grep",
            json!({ "subkind": "grep" }),
            day * 10 + 3,
            1,
        );

        let all = stats(
            &conn,
            QuotaUsageStatsInput {
                days: Some(7),
                backend: Some(BACKEND_NATIVE_AGENTKIT.to_string()),
            },
            day * 12 + 5,
        )
        .unwrap();

        assert_eq!(all.tools.len(), 2);
        assert!(all
            .tools
            .iter()
            .any(|tool| tool.label == "Bash" && tool.call_count == 1));
        assert!(all
            .tools
            .iter()
            .any(|tool| tool.label == "内容搜索" && tool.call_count == 1));
        assert!(all.tools.iter().all(|tool| tool.share_percent == 50.0));
    }

    #[test]
    fn stats_empty_range_returns_full_daily_buckets() {
        let conn = usage_conn();

        let result = stats(
            &conn,
            QuotaUsageStatsInput {
                days: Some(30),
                backend: Some("all".to_string()),
            },
            DAY_MS * 30,
        )
        .unwrap();

        assert_eq!(result.days, 30);
        assert_eq!(result.totals.total_tokens, 0);
        assert_eq!(result.cost.total_record_count, 0);
        assert_eq!(result.daily.len(), 30);
        assert!(result.backends.is_empty());
        assert!(result.recent.is_empty());
    }
}

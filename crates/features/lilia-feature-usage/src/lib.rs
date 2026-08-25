//! Usage domain feature.
//!
//! Aggregates timeline facts into the project dashboard and the quota usage
//! report. Everything here is a pure projection over facts the caller supplies.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::{ProductTask, ProductTaskStatus, Project};
use lilia_kernel::{
    Feature, FeatureContext, FeatureId, JobContext, JobProtocol, JobSlot, KernelError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const QUOTA_PROTOCOL: &str = "lilia.usage/quota@1";

/// Aggregates the quota report. It scans the whole timeline, so it never runs
/// on the UI thread.
pub trait UsagePort: Send + Sync + 'static {
    fn quota(&self, input: QuotaUsageStatsInput) -> Result<QuotaUsageStats, String>;
}

/// Single-flight lane: a second report request replaces the first, because the
/// surface only ever shows the newest one.
pub fn quota_slot() -> JobSlot {
    JobSlot::new("lilia.usage.quota").expect("the quota slot name is not blank")
}

pub struct UsageFeature {
    port: Arc<dyn UsagePort>,
}

impl UsageFeature {
    pub fn new(port: Arc<dyn UsagePort>) -> Self {
        Self { port }
    }
}

impl Feature for UsageFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.usage").expect("the usage feature id is not blank")
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        let port = Arc::clone(&self.port);
        vec![JobProtocol::new(
            QUOTA_PROTOCOL,
            Arc::new(move |payload, _context: &JobContext| {
                run_quota_job(payload, port.as_ref())
            }),
        )]
    }

    fn mount(&self, _cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        Ok(())
    }
}

fn run_quota_job(payload: Value, port: &dyn UsagePort) -> Result<Value, String> {
    let input: QuotaUsageStatsInput = serde_json::from_value(payload)
        .map_err(|error| format!("invalid quota request: {error}"))?;
    let stats = port.quota(input)?;
    serde_json::to_value(stats).map_err(|error| format!("invalid quota report: {error}"))
}


pub const DAY_MS: i64 = 86_400_000;
pub const RECENT_LIMIT: usize = 20;
pub const DEFAULT_DAYS: i64 = 30;
pub const ALLOWED_DAYS: [i64; 3] = [7, 30, 90];
pub const ALL_BACKENDS: &str = "all";
pub const NATIVE_BACKEND: &str = "native-agentkit";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageStatsInput {
    pub days: Option<i64>,
    pub backend: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageTokenTotals {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsageCostCoverage {
    pub known_cost_usd: Option<f64>,
    pub cost_record_count: i64,
    pub total_record_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProjectTaskStatusCounts {
    pub draft: i64,
    pub waiting: i64,
    pub running: i64,
    pub blocked: i64,
    pub done: i64,
    pub cancelled: i64,
}

impl DesktopProjectTaskStatusCounts {
    pub fn increment(&mut self, status: ProductTaskStatus) {
        let value = match status {
            ProductTaskStatus::Draft => &mut self.draft,
            ProductTaskStatus::Waiting => &mut self.waiting,
            ProductTaskStatus::Running => &mut self.running,
            ProductTaskStatus::Blocked => &mut self.blocked,
            ProductTaskStatus::Done => &mut self.done,
            ProductTaskStatus::Cancelled => &mut self.cancelled,
        };
        *value = value.saturating_add(1);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProjectDashboardSummary {
    pub project_id: String,
    pub name: String,
    pub workspace_path: Option<String>,
    pub pinned: bool,
    pub task_count: i64,
    pub session_count: i64,
    pub status_counts: DesktopProjectTaskStatusCounts,
    pub blocked_count: i64,
    pub active_count: i64,
    pub recent_activity_at: Option<i64>,
    pub total_tokens: i64,
    pub known_cost_usd: Option<f64>,
    pub cost_record_count: i64,
    pub usage_record_count: i64,
}

#[derive(Clone)]
pub struct UsageRecord {
    pub event_id: String,
    pub task_id: String,
    pub turn_id: Option<String>,
    pub session_id: Option<String>,
    pub backend: String,
    pub totals: QuotaUsageTokenTotals,
    pub created_at: i64,
}

#[derive(Default)]
pub struct Aggregate {
    totals: QuotaUsageTokenTotals,
    record_count: i64,
}

impl Aggregate {
    pub fn add(&mut self, record: &UsageRecord) {
        self.totals.input_tokens = self
            .totals
            .input_tokens
            .saturating_add(record.totals.input_tokens);
        self.totals.output_tokens = self
            .totals
            .output_tokens
            .saturating_add(record.totals.output_tokens);
        self.totals.cache_read_tokens = self
            .totals
            .cache_read_tokens
            .saturating_add(record.totals.cache_read_tokens);
        self.totals.cache_creation_tokens = self
            .totals
            .cache_creation_tokens
            .saturating_add(record.totals.cache_creation_tokens);
        self.totals.total_tokens = self
            .totals
            .total_tokens
            .saturating_add(record.totals.total_tokens);
        self.record_count = self.record_count.saturating_add(1);
    }
}


pub fn max_activity(current: Option<i64>, candidate: i64) -> Option<i64> {
    if candidate <= 0 {
        return current;
    }
    Some(current.map_or(candidate, |value| value.max(candidate)))
}

pub fn usage_record(
    event: &lilia_contracts::TimelineProjectionEvent,
    backend: &str,
    created_at: i64,
) -> Option<UsageRecord> {
    let input_tokens = positive_i64(first_value(
        &event.payload,
        &[
            "inputTokens",
            "input_tokens",
            "promptTokens",
            "prompt_tokens",
        ],
    ));
    let output_tokens = positive_i64(first_value(
        &event.payload,
        &[
            "outputTokens",
            "output_tokens",
            "completionTokens",
            "completion_tokens",
        ],
    ));
    let cache_read_tokens = positive_i64(first_value(
        &event.payload,
        &["cacheReadTokens", "cache_read_tokens"],
    ));
    let cache_creation_tokens = positive_i64(first_value(
        &event.payload,
        &["cacheCreationTokens", "cache_creation_tokens"],
    ));
    let component_total = input_tokens
        .saturating_add(output_tokens)
        .saturating_add(cache_read_tokens)
        .saturating_add(cache_creation_tokens);
    let total_tokens = positive_i64(first_value(
        &event.payload,
        &["totalTokens", "total_tokens"],
    ))
    .max(component_total);
    if total_tokens == 0 {
        return None;
    }
    Some(UsageRecord {
        event_id: event.id.as_str().to_owned(),
        task_id: event.task_id.as_str().to_owned(),
        turn_id: event.turn_id.clone(),
        session_id: Some(event.agent_session.as_str().to_owned()),
        backend: backend.to_owned(),
        totals: QuotaUsageTokenTotals {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            total_tokens,
        },
        created_at,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn build_stats(
    days: i64,
    backend: String,
    range_start: i64,
    range_end: i64,
    records: Vec<UsageRecord>,
    tool_counts: BTreeMap<String, (String, Option<String>, Option<String>, i64)>,
    tasks: &BTreeMap<String, &ProductTask>,
    projects: &BTreeMap<String, &Project>,
) -> QuotaUsageStats {
    let mut total = Aggregate::default();
    let mut daily = (0..days)
        .map(|offset| (range_start + offset * DAY_MS, Aggregate::default()))
        .collect::<BTreeMap<_, _>>();
    let mut by_backend = BTreeMap::<String, Aggregate>::new();
    let mut by_project = BTreeMap::<Option<String>, Aggregate>::new();
    let mut by_conversation = BTreeMap::<String, Aggregate>::new();
    for record in &records {
        total.add(record);
        daily
            .entry(day_start(record.created_at))
            .or_default()
            .add(record);
        by_backend
            .entry(record.backend.clone())
            .or_default()
            .add(record);
        let project_id = tasks
            .get(&record.task_id)
            .and_then(|task| task.project_id.as_ref())
            .map(|project_id| project_id.as_str().to_owned());
        by_project.entry(project_id).or_default().add(record);
        by_conversation
            .entry(record.task_id.clone())
            .or_default()
            .add(record);
    }
    let totals = total.totals.clone();
    let mut project_rows = by_project
        .into_iter()
        .map(|(project_id, aggregate)| {
            let project = project_id
                .as_deref()
                .and_then(|id| projects.get(id).copied());
            project_summary(project_id, project, aggregate)
        })
        .collect::<Vec<_>>();
    project_rows.sort_by_key(|row| std::cmp::Reverse(row.total_tokens));
    let mut conversation_rows = by_conversation
        .into_iter()
        .map(|(task_id, aggregate)| {
            conversation_summary(
                task_id.clone(),
                tasks.get(&task_id).copied(),
                projects,
                aggregate,
            )
        })
        .collect::<Vec<_>>();
    conversation_rows.sort_by_key(|row| std::cmp::Reverse(row.total_tokens));
    let total_tools = tool_counts.values().map(|entry| entry.3).sum::<i64>();
    let mut tools = tool_counts
        .into_iter()
        .map(
            |(key, (kind, subkind, tool_name, call_count))| QuotaUsageToolSummary {
                key,
                label: tool_name
                    .clone()
                    .unwrap_or_else(|| tool_kind_label(&kind).to_owned()),
                kind,
                subkind,
                tool_name,
                call_count,
                share_percent: if total_tools > 0 {
                    (call_count as f64 / total_tools as f64) * 100.0
                } else {
                    0.0
                },
            },
        )
        .collect::<Vec<_>>();
    tools.sort_by_key(|tool| std::cmp::Reverse(tool.call_count));
    let recent = records
        .iter()
        .rev()
        .take(RECENT_LIMIT)
        .map(|record| QuotaUsageRecentRecord {
            event_id: record.event_id.clone(),
            task_id: record.task_id.clone(),
            turn_id: record.turn_id.clone(),
            backend: record.backend.clone(),
            session_id: record.session_id.clone(),
            input_tokens: record.totals.input_tokens,
            output_tokens: record.totals.output_tokens,
            cache_read_tokens: record.totals.cache_read_tokens,
            cache_creation_tokens: record.totals.cache_creation_tokens,
            total_tokens: record.totals.total_tokens,
            known_cost_usd: None,
            created_at: record.created_at,
        })
        .collect();
    QuotaUsageStats {
        days,
        backend,
        range_start,
        range_end,
        totals,
        cost: QuotaUsageCostCoverage {
            known_cost_usd: None,
            cost_record_count: 0,
            total_record_count: total.record_count,
        },
        daily: daily
            .into_iter()
            .map(|(day_start, aggregate)| QuotaUsageDailyBucket {
                day_start,
                input_tokens: aggregate.totals.input_tokens,
                output_tokens: aggregate.totals.output_tokens,
                cache_read_tokens: aggregate.totals.cache_read_tokens,
                cache_creation_tokens: aggregate.totals.cache_creation_tokens,
                total_tokens: aggregate.totals.total_tokens,
                known_cost_usd: None,
                cost_record_count: 0,
                record_count: aggregate.record_count,
            })
            .collect(),
        backends: by_backend
            .into_iter()
            .map(|(backend, aggregate)| QuotaUsageBackendSummary {
                backend,
                input_tokens: aggregate.totals.input_tokens,
                output_tokens: aggregate.totals.output_tokens,
                cache_read_tokens: aggregate.totals.cache_read_tokens,
                cache_creation_tokens: aggregate.totals.cache_creation_tokens,
                total_tokens: aggregate.totals.total_tokens,
                known_cost_usd: None,
                cost_record_count: 0,
                record_count: aggregate.record_count,
            })
            .collect(),
        recent,
        projects: project_rows,
        conversations: conversation_rows,
        tools,
    }
}

pub fn project_summary(
    project_id: Option<String>,
    project: Option<&Project>,
    aggregate: Aggregate,
) -> QuotaUsageProjectSummary {
    QuotaUsageProjectSummary {
        project_name: project
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "未归属项目".to_owned()),
        project_cwd: project.and_then(|project| project.workspace_path.clone()),
        project_id,
        input_tokens: aggregate.totals.input_tokens,
        output_tokens: aggregate.totals.output_tokens,
        cache_read_tokens: aggregate.totals.cache_read_tokens,
        cache_creation_tokens: aggregate.totals.cache_creation_tokens,
        total_tokens: aggregate.totals.total_tokens,
        known_cost_usd: None,
        cost_record_count: 0,
        record_count: aggregate.record_count,
    }
}

pub fn conversation_summary(
    task_id: String,
    task: Option<&ProductTask>,
    projects: &BTreeMap<String, &Project>,
    aggregate: Aggregate,
) -> QuotaUsageConversationSummary {
    let project_id = task
        .and_then(|task| task.project_id.as_ref())
        .map(|project_id| project_id.as_str().to_owned());
    QuotaUsageConversationSummary {
        task_title: task
            .map(|task| task.title.clone())
            .unwrap_or_else(|| "未知对话".to_owned()),
        task_status: task
            .map(|task| task_status_key(task.status).to_owned())
            .unwrap_or_else(|| "waiting".to_owned()),
        project_name: project_id
            .as_deref()
            .and_then(|project_id| projects.get(project_id))
            .map(|project| project.name.clone()),
        task_id,
        project_id,
        input_tokens: aggregate.totals.input_tokens,
        output_tokens: aggregate.totals.output_tokens,
        cache_read_tokens: aggregate.totals.cache_read_tokens,
        cache_creation_tokens: aggregate.totals.cache_creation_tokens,
        total_tokens: aggregate.totals.total_tokens,
        known_cost_usd: None,
        cost_record_count: 0,
        record_count: aggregate.record_count,
    }
}

pub fn normalized_backend(value: Option<&str>) -> String {
    match value.map(str::trim) {
        Some(NATIVE_BACKEND) => NATIVE_BACKEND.to_owned(),
        _ => ALL_BACKENDS.to_owned(),
    }
}

pub fn first_value<'a>(payload: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| payload.get(*key))
}

pub fn positive_i64(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|value| value.try_into().ok()))
        })
        .filter(|value| *value > 0)
        .unwrap_or_default()
}

pub fn string_value(payload: &Value, keys: &[&str]) -> Option<String> {
    first_value(payload, keys)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub fn is_tool_kind(kind: &str) -> bool {
    matches!(
        kind,
        "tool"
            | "command"
            | "file_read"
            | "file_change"
            | "search"
            | "web_fetch"
            | "subagent"
            | "mcp"
    )
}

pub fn tool_kind_label(kind: &str) -> &str {
    match kind {
        "command" => "命令",
        "file_read" => "读取文件",
        "file_change" => "修改文件",
        "search" => "搜索",
        "web_fetch" => "抓取网页",
        "subagent" => "子代理",
        "mcp" => "MCP 工具",
        _ => "工具",
    }
}

pub fn task_status_key(status: ProductTaskStatus) -> &'static str {
    match status {
        ProductTaskStatus::Draft => "draft",
        ProductTaskStatus::Waiting => "waiting",
        ProductTaskStatus::Running => "running",
        ProductTaskStatus::Blocked => "blocked",
        ProductTaskStatus::Done => "done",
        ProductTaskStatus::Cancelled => "cancelled",
    }
}

pub fn day_start(timestamp: i64) -> i64 {
    timestamp.div_euclid(DAY_MS) * DAY_MS
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}


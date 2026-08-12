use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::{ProductTask, ProductTaskStatus, Project};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{DesktopApplication, DesktopApplicationError, ProjectQuery, TaskQuery};

const DAY_MS: i64 = 86_400_000;
const RECENT_LIMIT: usize = 20;
const DEFAULT_DAYS: i64 = 30;
const ALLOWED_DAYS: [i64; 3] = [7, 30, 90];
const ALL_BACKENDS: &str = "all";
const NATIVE_BACKEND: &str = "native-agentkit";

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

#[derive(Clone)]
struct UsageRecord {
    event_id: String,
    task_id: String,
    turn_id: Option<String>,
    session_id: Option<String>,
    backend: String,
    totals: QuotaUsageTokenTotals,
    created_at: i64,
}

#[derive(Default)]
struct Aggregate {
    totals: QuotaUsageTokenTotals,
    record_count: i64,
}

impl Aggregate {
    fn add(&mut self, record: &UsageRecord) {
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

impl DesktopApplication {
    pub fn quota_usage_stats(
        &self,
        input: QuotaUsageStatsInput,
    ) -> Result<QuotaUsageStats, DesktopApplicationError> {
        self.quota_usage_stats_at(input, now_millis())
    }

    fn quota_usage_stats_at(
        &self,
        input: QuotaUsageStatsInput,
        now: i64,
    ) -> Result<QuotaUsageStats, DesktopApplicationError> {
        let days = input
            .days
            .filter(|days| ALLOWED_DAYS.contains(days))
            .unwrap_or(DEFAULT_DAYS);
        let backend = normalized_backend(input.backend.as_deref());
        let range_end = day_start(now).saturating_add(DAY_MS);
        let range_start = range_end.saturating_sub(days.saturating_mul(DAY_MS));
        let tasks = self.query_tasks(TaskQuery {
            include_archived: true,
            ..TaskQuery::default()
        })?;
        let projects = self.query_projects(ProjectQuery {
            include_archived: true,
        })?;
        let project_by_id = projects
            .iter()
            .map(|project| (project.id.as_str().to_owned(), project))
            .collect::<BTreeMap<_, _>>();
        let task_by_id = tasks
            .iter()
            .map(|task| (task.id.as_str().to_owned(), task))
            .collect::<BTreeMap<_, _>>();
        let mut records = Vec::new();
        let mut tool_counts =
            BTreeMap::<String, (String, Option<String>, Option<String>, i64)>::new();
        for task in &tasks {
            for event in self.authority().projection_timeline_for_task(&task.id) {
                let created_at = positive_i64(event.payload.get("createdAt"));
                if created_at < range_start || created_at >= range_end {
                    continue;
                }
                let event_backend = event
                    .payload
                    .get("backend")
                    .and_then(Value::as_str)
                    .unwrap_or(NATIVE_BACKEND);
                if backend != ALL_BACKENDS && backend != event_backend {
                    continue;
                }
                if event.kind == "usage" {
                    if let Some(record) = usage_record(&event, event_backend, created_at) {
                        records.push(record);
                    }
                } else if is_tool_kind(&event.kind) {
                    let subkind = string_value(&event.payload, &["subkind"]);
                    let tool_name =
                        string_value(&event.payload, &["toolName", "tool", "hookName", "name"]);
                    let key = format!(
                        "{}:{}:{}",
                        event.kind,
                        subkind.as_deref().unwrap_or_default(),
                        tool_name.as_deref().unwrap_or_default()
                    );
                    let entry = tool_counts.entry(key).or_insert_with(|| {
                        (event.kind.clone(), subkind.clone(), tool_name.clone(), 0)
                    });
                    entry.3 = entry.3.saturating_add(1);
                }
            }
        }
        records.sort_by_key(|record| record.created_at);
        Ok(build_stats(
            days,
            backend,
            range_start,
            range_end,
            records,
            tool_counts,
            &task_by_id,
            &project_by_id,
        ))
    }
}

fn usage_record(
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
fn build_stats(
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

fn project_summary(
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

fn conversation_summary(
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

fn normalized_backend(value: Option<&str>) -> String {
    match value.map(str::trim) {
        Some(NATIVE_BACKEND) => NATIVE_BACKEND.to_owned(),
        _ => ALL_BACKENDS.to_owned(),
    }
}

fn first_value<'a>(payload: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| payload.get(*key))
}

fn positive_i64(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|value| value.try_into().ok()))
        })
        .filter(|value| *value > 0)
        .unwrap_or_default()
}

fn string_value(payload: &Value, keys: &[&str]) -> Option<String> {
    first_value(payload, keys)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn is_tool_kind(kind: &str) -> bool {
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

fn tool_kind_label(kind: &str) -> &str {
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

fn task_status_key(status: ProductTaskStatus) -> &'static str {
    match status {
        ProductTaskStatus::Draft => "draft",
        ProductTaskStatus::Waiting => "waiting",
        ProductTaskStatus::Running => "running",
        ProductTaskStatus::Blocked => "blocked",
        ProductTaskStatus::Done => "done",
        ProductTaskStatus::Cancelled => "cancelled",
    }
}

fn day_start(timestamp: i64) -> i64 {
    timestamp.div_euclid(DAY_MS) * DAY_MS
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use lilia_contracts::{
        AgentSessionRef, ProjectionEventId, TimelineProjectionCommand, TimelineProjectionEvent,
    };
    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, DesktopProjectCreate, DesktopTaskCreate,
    };

    static NEXT_APPLICATION_ID: AtomicU64 = AtomicU64::new(1);

    struct NoopHost;

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    fn application() -> DesktopApplication {
        let id = NEXT_APPLICATION_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:quota-usage:{id}"),
            format!("quota-usage-test:{id}"),
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                "C:/lilia/quota-usage",
                format!("liliacode.quota-usage-test.{id}"),
            )
            .unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap()
    }

    #[test]
    fn projected_usage_reads_only_positive_authoritative_values() {
        let event = lilia_contracts::TimelineProjectionEvent {
            id: lilia_contracts::ProjectionEventId::new("usage-1"),
            task_id: lilia_contracts::TaskId::new("task-1").unwrap(),
            agent_session: lilia_contracts::AgentSessionRef::new("session-1").unwrap(),
            sequence: 1,
            turn_id: Some("turn-1".to_owned()),
            kind: "usage".to_owned(),
            status: "success".to_owned(),
            title: "Usage".to_owned(),
            summary: None,
            payload: serde_json::json!({
                "inputTokens": 10,
                "outputTokens": 5,
                "totalTokens": 15,
                "createdAt": 1_725_000_000_000_i64,
            }),
            projected: true,
        };
        let record = usage_record(&event, NATIVE_BACKEND, 1_725_000_000_000).unwrap();
        assert_eq!(record.totals.input_tokens, 10);
        assert_eq!(record.totals.output_tokens, 5);
        assert_eq!(record.totals.total_tokens, 15);
    }

    #[test]
    fn daily_stats_fill_empty_days_and_keep_unknown_cost_explicit() {
        let record = UsageRecord {
            event_id: "usage-1".to_owned(),
            task_id: "task-1".to_owned(),
            turn_id: None,
            session_id: None,
            backend: NATIVE_BACKEND.to_owned(),
            totals: QuotaUsageTokenTotals {
                input_tokens: 8,
                output_tokens: 2,
                total_tokens: 10,
                ..QuotaUsageTokenTotals::default()
            },
            created_at: DAY_MS,
        };
        let stats = build_stats(
            7,
            ALL_BACKENDS.to_owned(),
            0,
            7 * DAY_MS,
            vec![record],
            BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert_eq!(stats.daily.len(), 7);
        assert_eq!(stats.totals.total_tokens, 10);
        assert_eq!(stats.cost.known_cost_usd, None);
        assert_eq!(stats.cost.cost_record_count, 0);
        assert_eq!(stats.cost.total_record_count, 1);
    }

    #[test]
    fn application_stats_read_projected_usage_with_product_context() {
        let application = application();
        let project = application
            .create_project(DesktopProjectCreate::new("Native IDE"))
            .unwrap();
        let task = application
            .create_task(DesktopTaskCreate::new(
                Some(project.id.clone()),
                "Track native usage",
            ))
            .unwrap();
        let session = AgentSessionRef::new("usage-session").unwrap();
        let created_at = now_millis();
        for event in [
            TimelineProjectionEvent {
                id: ProjectionEventId::new("usage-event"),
                task_id: task.id.clone(),
                agent_session: session.clone(),
                sequence: 1,
                turn_id: Some("turn-1".to_owned()),
                kind: "usage".to_owned(),
                status: "success".to_owned(),
                title: "Native usage".to_owned(),
                summary: None,
                payload: serde_json::json!({
                    "inputTokens": 21,
                    "outputTokens": 13,
                    "totalTokens": 34,
                    "createdAt": created_at,
                    "backend": NATIVE_BACKEND,
                }),
                projected: true,
            },
            TimelineProjectionEvent {
                id: ProjectionEventId::new("tool-event"),
                task_id: task.id.clone(),
                agent_session: session,
                sequence: 2,
                turn_id: Some("turn-1".to_owned()),
                kind: "tool".to_owned(),
                status: "success".to_owned(),
                title: "Search".to_owned(),
                summary: None,
                payload: serde_json::json!({
                    "toolName": "Search",
                    "createdAt": created_at,
                    "backend": NATIVE_BACKEND,
                }),
                projected: true,
            },
        ] {
            application
                .authority()
                .apply_projection(TimelineProjectionCommand::UpsertTimelineEvent { event })
                .unwrap();
        }

        let stats = application
            .quota_usage_stats(QuotaUsageStatsInput {
                days: Some(7),
                backend: Some(NATIVE_BACKEND.to_owned()),
            })
            .unwrap();
        assert_eq!(stats.totals.total_tokens, 34);
        assert_eq!(stats.projects[0].project_name, "Native IDE");
        assert_eq!(stats.conversations[0].task_title, "Track native usage");
        assert_eq!(stats.tools[0].label, "Search");
        assert_eq!(stats.tools[0].call_count, 1);
        assert_eq!(stats.cost.known_cost_usd, None);
    }
}

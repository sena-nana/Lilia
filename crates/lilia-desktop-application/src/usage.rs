use std::collections::{BTreeMap, BTreeSet};

use lilia_contracts::ProductTask;
use serde_json::Value;

use crate::{DesktopApplication, DesktopApplicationError, ProjectQuery, TaskQuery};

pub use lilia_feature_usage::*;

impl DesktopApplication {
    pub fn project_dashboard_summaries(
        &self,
    ) -> Result<Vec<DesktopProjectDashboardSummary>, DesktopApplicationError> {
        let projects = self.query_projects(ProjectQuery::default())?;
        let tasks = self.query_tasks(TaskQuery::default())?;
        let conversations = self.authority().client()?.products().list_conversations()?;
        let mut tasks_by_project = BTreeMap::<String, Vec<&ProductTask>>::new();
        for task in &tasks {
            if let Some(project_id) = &task.project_id {
                tasks_by_project
                    .entry(project_id.as_str().to_owned())
                    .or_default()
                    .push(task);
            }
        }

        let mut summaries = Vec::with_capacity(projects.len());
        for project in projects {
            let project_id = project.id.as_str().to_owned();
            let project_tasks = tasks_by_project
                .get(&project_id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let task_ids = project_tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<BTreeSet<_>>();
            let mut status_counts = DesktopProjectTaskStatusCounts::default();
            let mut recent_activity_at = None;
            let mut total_tokens = 0_i64;
            let mut known_cost_usd = 0.0_f64;
            let mut cost_record_count = 0_i64;
            let mut usage_record_count = 0_i64;
            for task in project_tasks {
                status_counts.increment(task.status);
                recent_activity_at = max_activity(recent_activity_at, task.updated_at);
                for event in self.authority().projection_timeline_for_task(&task.id) {
                    recent_activity_at = max_activity(
                        recent_activity_at,
                        positive_i64(event.payload.get("createdAt")),
                    );
                    if event.kind != "usage" {
                        continue;
                    }
                    let Some(record) = usage_record(
                        &event,
                        event
                            .payload
                            .get("backend")
                            .and_then(Value::as_str)
                            .unwrap_or(NATIVE_BACKEND),
                        positive_i64(event.payload.get("createdAt")),
                    ) else {
                        continue;
                    };
                    total_tokens = total_tokens.saturating_add(record.totals.total_tokens);
                    usage_record_count = usage_record_count.saturating_add(1);
                    if let Some(cost) = first_value(
                        &event.payload,
                        &["knownCostUsd", "known_cost_usd", "costUsd", "cost_usd"],
                    )
                    .and_then(Value::as_f64)
                    .filter(|cost| cost.is_finite() && *cost >= 0.0)
                    {
                        known_cost_usd += cost;
                        cost_record_count = cost_record_count.saturating_add(1);
                    }
                }
            }
            let session_ids = conversations
                .iter()
                .filter(|conversation| {
                    !conversation.archived
                        && conversation
                            .task_id
                            .as_ref()
                            .is_some_and(|task_id| task_ids.contains(task_id.as_str()))
                })
                .map(|conversation| {
                    recent_activity_at = max_activity(recent_activity_at, conversation.updated_at);
                    conversation.id.as_str()
                })
                .collect::<BTreeSet<_>>();
            let active_count = status_counts.waiting.saturating_add(status_counts.running);
            let blocked_count = status_counts.blocked;
            summaries.push(DesktopProjectDashboardSummary {
                project_id,
                name: project.name,
                workspace_path: project.workspace_path,
                pinned: project.pinned,
                task_count: project_tasks.len() as i64,
                session_count: session_ids.len() as i64,
                status_counts,
                blocked_count,
                active_count,
                recent_activity_at,
                total_tokens,
                known_cost_usd: (cost_record_count > 0).then_some(known_cost_usd),
                cost_record_count,
                usage_record_count,
            });
        }
        Ok(summaries)
    }

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

    #[test]
    fn project_dashboard_joins_product_status_sessions_activity_and_usage() {
        let application = application();
        let project = application
            .create_project(DesktopProjectCreate::new("Dashboard"))
            .unwrap();
        let task = application
            .create_task(DesktopTaskCreate::new(
                Some(project.id.clone()),
                "Dashboard task",
            ))
            .unwrap();
        application
            .authority()
            .apply_projection(TimelineProjectionCommand::UpsertTimelineEvent {
                event: TimelineProjectionEvent {
                    id: ProjectionEventId::new("dashboard-usage"),
                    task_id: task.id,
                    agent_session: AgentSessionRef::new("dashboard-session").unwrap(),
                    sequence: 1,
                    turn_id: Some("dashboard-turn".to_owned()),
                    kind: "usage".to_owned(),
                    status: "success".to_owned(),
                    title: "Usage".to_owned(),
                    summary: None,
                    payload: serde_json::json!({
                        "inputTokens": 12,
                        "outputTokens": 8,
                        "totalTokens": 20,
                        "knownCostUsd": 0.25,
                        "createdAt": 1_800_000_000_000_i64,
                        "backend": NATIVE_BACKEND,
                    }),
                    projected: true,
                },
            })
            .unwrap();

        let dashboard = application.project_dashboard_summaries().unwrap();
        assert_eq!(dashboard.len(), 1);
        let summary = &dashboard[0];
        assert_eq!(summary.project_id, project.id.as_str());
        assert_eq!(summary.task_count, 1);
        assert_eq!(summary.session_count, 1);
        assert_eq!(summary.status_counts.draft, 1);
        assert_eq!(summary.total_tokens, 20);
        assert_eq!(summary.known_cost_usd, Some(0.25));
        assert_eq!(summary.cost_record_count, 1);
        assert_eq!(summary.usage_record_count, 1);
        assert_eq!(summary.recent_activity_at, Some(1_800_000_000_000));
    }
}

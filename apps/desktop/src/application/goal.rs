use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::{
    AgentSessionRef, ProjectionEventId, TaskId, TimelineProjectionCommand, TimelineProjectionEvent,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::application::{DesktopApplication, DesktopApplicationError};
use crate::application::{GoalChanged, TimelineChanged};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DesktopGoalStatus {
    Active,
    Paused,
    Blocked,
    UsageLimited,
    BudgetLimited,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopGoalSnapshot {
    pub thread_id: String,
    pub objective: String,
    pub status: DesktopGoalStatus,
    pub token_budget: Option<u64>,
    pub tokens_used: u64,
    pub time_used_seconds: u64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl DesktopApplication {
    pub fn task_goal(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopGoalSnapshot>, DesktopApplicationError> {
        self.get_task(task_id)?;
        Ok(latest_goal(
            &self
                .authority()
                .shared_runtime()
                .inner()
                .product_timeline_for_task(task_id),
        ))
    }

    pub fn set_task_goal(
        &self,
        task_id: &TaskId,
        objective: impl Into<String>,
        token_budget: Option<u64>,
    ) -> Result<DesktopGoalSnapshot, DesktopApplicationError> {
        self.get_task(task_id)?;
        let objective = objective.into();
        let objective = objective.trim();
        if objective.is_empty() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "objective",
                message: "goal objective must not be empty".to_owned(),
            });
        }
        let now = now_millis();
        let previous = self.task_goal(task_id)?;
        let goal = DesktopGoalSnapshot {
            thread_id: task_id.as_str().to_owned(),
            objective: objective.to_owned(),
            status: DesktopGoalStatus::Active,
            token_budget,
            tokens_used: previous
                .as_ref()
                .map(|goal| goal.tokens_used)
                .unwrap_or_default(),
            time_used_seconds: previous
                .as_ref()
                .map(|goal| goal.time_used_seconds)
                .unwrap_or_default(),
            created_at: previous.as_ref().map(|goal| goal.created_at).unwrap_or(now),
            updated_at: now,
        };
        self.record_goal_event(task_id, Some(&goal), "Goal 已设置")?;
        Ok(goal)
    }

    pub fn refresh_task_goal(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopGoalSnapshot, DesktopApplicationError> {
        let mut goal = self
            .task_goal(task_id)?
            .ok_or_else(|| DesktopApplicationError::GoalNotFound(task_id.clone()))?;
        let timeline = self
            .authority()
            .shared_runtime()
            .inner()
            .product_timeline_for_task(task_id);
        goal.tokens_used = timeline.iter().map(timeline_token_usage).sum();
        goal.time_used_seconds =
            now_millis().saturating_sub(goal.created_at).unsigned_abs() / 1_000;
        let todos = self.list_task_todos(task_id)?;
        let incomplete = todos.iter().filter(|todo| !todo.done).count();
        goal.status = if !todos.is_empty() && incomplete == 0 {
            DesktopGoalStatus::Complete
        } else if goal
            .token_budget
            .is_some_and(|budget| goal.tokens_used >= budget)
        {
            DesktopGoalStatus::BudgetLimited
        } else {
            DesktopGoalStatus::Active
        };
        goal.updated_at = now_millis();
        self.record_goal_event(task_id, Some(&goal), "Goal 已刷新")?;
        Ok(goal)
    }

    pub fn clear_task_goal(&self, task_id: &TaskId) -> Result<bool, DesktopApplicationError> {
        if self.task_goal(task_id)?.is_none() {
            return Ok(false);
        }
        self.record_goal_event(task_id, None, "Goal 已清除")?;
        Ok(true)
    }

    fn record_goal_event(
        &self,
        task_id: &TaskId,
        goal: Option<&DesktopGoalSnapshot>,
        title: &str,
    ) -> Result<(), DesktopApplicationError> {
        let timeline = self
            .authority()
            .shared_runtime()
            .inner()
            .product_timeline_for_task(task_id);
        let sequence = timeline
            .iter()
            .map(|event| event.sequence)
            .max()
            .unwrap_or_default()
            .saturating_add(1);
        let session = AgentSessionRef::new(format!("desktop-goal:{}", task_id.as_str()))?;
        let event = TimelineProjectionEvent {
            id: ProjectionEventId::from_session_sequence(session.as_str(), sequence),
            task_id: task_id.clone(),
            agent_session: session,
            sequence,
            turn_id: None,
            kind: "goal".to_owned(),
            status: "success".to_owned(),
            title: title.to_owned(),
            summary: goal.map(|goal| goal.objective.clone()),
            payload: match goal {
                Some(goal) => json!({ "goal": goal }),
                None => json!({ "cleared": true }),
            },
            projected: true,
        };
        self.authority()
            .apply_projection(TimelineProjectionCommand::UpsertTimelineEvent { event })?;
        self.emit_event(GoalChanged {
            task_id: task_id.clone(),
        });
        self.emit_event(TimelineChanged {
            task_id: task_id.clone(),
            cursor: Some(sequence),
        });
        Ok(())
    }
}

fn latest_goal(timeline: &[TimelineProjectionEvent]) -> Option<DesktopGoalSnapshot> {
    let latest = timeline
        .iter()
        .filter(|event| event.kind == "goal")
        .max_by_key(|event| event.sequence)?;
    if latest.payload.get("cleared").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    serde_json::from_value(latest.payload.get("goal")?.clone()).ok()
}

fn timeline_token_usage(event: &TimelineProjectionEvent) -> u64 {
    [
        event.payload.pointer("/usage/total_tokens"),
        event.payload.pointer("/usage/totalTokens"),
        event.payload.get("totalTokens"),
        event.payload.get("tokensUsed"),
    ]
    .into_iter()
    .flatten()
    .find_map(Value::as_u64)
    .unwrap_or_default()
}

fn now_millis() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use lilia_contracts::{ProductEntity, ProductTask};
    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, DesktopTodoCreate, DesktopTodoPriority,
        DesktopTodoUpdate, GoalChanged, TimelineChanged,
    };

    static NEXT_GOAL_ID: AtomicU64 = AtomicU64::new(1);

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

    fn application() -> (DesktopApplication, TaskId) {
        let id = NEXT_GOAL_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:desktop-goal:{id}"),
            format!("desktop-goal-test:{id}"),
        )
        .unwrap();
        let task_id = TaskId::new(format!("goal-task-{id}")).unwrap();
        authority
            .client()
            .unwrap()
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task_id.clone(), None, "Goal task").unwrap(),
            ))
            .unwrap();
        let application = DesktopApplication::from_authority(
            DesktopApplicationConfig::new("C:/lilia/goal", "liliacode.test").unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();
        (application, task_id)
    }

    #[test]
    fn goal_set_refresh_and_clear_are_typed_timeline_facts() {
        let (application, task_id) = application();
        let events = application.subscribe_events();
        let goal = application
            .set_task_goal(&task_id, "  ship Native  ", Some(100))
            .unwrap();
        assert_eq!(goal.objective, "ship Native");
        assert_eq!(application.task_goal(&task_id).unwrap(), Some(goal));
        assert!(matches!(
            events.recv().unwrap().downcast::<GoalChanged>(),
            Some(GoalChanged { task_id: ref changed }) if changed == &task_id
        ));
        assert!(matches!(
            events.recv().unwrap().downcast::<TimelineChanged>(),
            Some(TimelineChanged { task_id: ref changed, .. }) if changed == &task_id
        ));

        let todo = application
            .create_task_todo(DesktopTodoCreate {
                task_id: task_id.clone(),
                text: "finish gate".to_owned(),
                priority: DesktopTodoPriority::Normal,
                attachments: Vec::new(),
                conversation_references: Vec::new(),
                workflow: None,
            })
            .unwrap();
        application
            .update_task_todo(
                &todo.id,
                DesktopTodoUpdate {
                    done: Some(true),
                    ..DesktopTodoUpdate::default()
                },
            )
            .unwrap();
        let refreshed = application.refresh_task_goal(&task_id).unwrap();
        assert_eq!(refreshed.status, DesktopGoalStatus::Complete);

        assert!(application.clear_task_goal(&task_id).unwrap());
        assert_eq!(application.task_goal(&task_id).unwrap(), None);
        assert!(!application.clear_task_goal(&task_id).unwrap());
    }

    #[test]
    fn goal_refresh_requires_an_existing_goal() {
        let (application, task_id) = application();

        assert!(matches!(
            application.refresh_task_goal(&task_id),
            Err(DesktopApplicationError::GoalNotFound(ref missing)) if missing == &task_id
        ));
    }
}

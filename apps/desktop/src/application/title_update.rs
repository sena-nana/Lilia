//! Host adapter for task title updates.
//!
//! Prompt assembly, HTTP, persist and projection live in
//! `lilia-feature-agent-session`. This file only implements [`TitleHost`].

use std::path::PathBuf;
use std::sync::Arc;

use lilia_contracts::{
    PendingProjection, ProjectId, TaskId, TimelineProjectionCommand, TimelineProjectionEvent,
};
use lilia_feature_agent_session::{
    apply_title_proposal, build_title_prompt_for_job, respond_title_update,
    respond_title_update_review, run_title_update_after_turn, task_title_state, TitleError,
    TitleHost, TitleModelRequest,
};
use lilia_storage::ProjectionApplyResult;

use crate::application::{
    DesktopApplication, DesktopApplicationError, DesktopTaskPatch, TasksChanged, TimelineChanged,
    ASSISTANT_AI_CREDENTIAL_KEY,
};

pub use lilia_feature_agent_session::{
    normalize_title, title_event_id, title_system_instruction, DesktopTaskTitleSource,
    DesktopTaskTitleState, DesktopTimelineUpperBound, DesktopTitleUpdateCoordinator,
    DesktopTitleUpdateDecision, DesktopTitleUpdateJob, DesktopTitleUpdateReview,
    DesktopTitleUpdateScheduler, TITLE_MAX_CHARS, TITLE_MIN_CHARS, TITLE_UPDATE_ACTION_KIND,
};

impl From<TitleError> for DesktopApplicationError {
    fn from(error: TitleError) -> Self {
        match error {
            TitleError::InvalidInput { field, message } => Self::InvalidInput { field, message },
            TitleError::TaskNotFound => Self::InvalidInput {
                field: "task_id",
                message: "任务不存在".to_owned(),
            },
            TitleError::Product(error) => Self::Product(error),
            TitleError::PendingInteractionNotFound {
                task_id,
                request_id,
            } => Self::PendingInteractionNotFound {
                task_id,
                request_id,
            },
            TitleError::InvalidPendingInteraction {
                request_id,
                message,
            } => Self::InvalidPendingInteraction {
                request_id,
                message,
            },
        }
    }
}

impl TitleHost for DesktopApplication {
    fn load_task(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<(String, Option<ProjectId>)>, TitleError> {
        match self.get_task(task_id) {
            Ok(task) => Ok(Some((task.title, task.project_id))),
            Err(DesktopApplicationError::Product(lilia_contracts::ProductError::NotFound {
                ..
            })) => Ok(None),
            Err(DesktopApplicationError::Product(error)) => Err(TitleError::Product(error)),
            Err(error) => Err(TitleError::InvalidInput {
                field: "task_id",
                message: error.to_string(),
            }),
        }
    }

    fn write_task_title(&self, task_id: &TaskId, title: &str) -> Result<(), TitleError> {
        self.update_task(
            task_id,
            DesktopTaskPatch {
                title: Some(title.to_owned()),
                ..DesktopTaskPatch::default()
            },
        )
        .map(|_| ())
        .map_err(title_host_error)
    }

    fn timeline_events(
        &self,
        task_id: &TaskId,
        limit: u32,
    ) -> Result<Vec<TimelineProjectionEvent>, TitleError> {
        Ok(self
            .task_timeline_page(task_id, None, limit as usize)
            .map_err(title_host_error)?
            .events)
    }

    fn apply_projection(
        &self,
        command: TimelineProjectionCommand,
    ) -> Result<ProjectionApplyResult, TitleError> {
        self.authority()
            .apply_projection(command)
            .map_err(|error| title_host_error(error.into()))
    }

    fn pending_projections(&self, task_id: &TaskId) -> Result<Vec<PendingProjection>, TitleError> {
        Ok(self
            .task_session_snapshot(task_id)
            .map_err(title_host_error)?
            .pending)
    }

    fn prepare_title_source_store(&self) -> Result<PathBuf, TitleError> {
        self.config()
            .data_paths()
            .ensure_layout()
            .map_err(|error| TitleError::InvalidInput {
                field: "title_source",
                message: error.to_string(),
            })?;
        Ok(self.config().data_paths().agent_runtime_db())
    }

    fn emit_tasks_changed(&self, project_id: Option<ProjectId>, task_id: TaskId) {
        self.emit_event(TasksChanged {
            project_id,
            task_id: Some(task_id),
        });
    }

    fn emit_timeline_changed(&self, task_id: TaskId, cursor: Option<u64>) {
        self.emit_event(TimelineChanged { task_id, cursor });
    }

    fn title_model(&self) -> Option<TitleModelRequest> {
        let assistant = self.assistant_ai_settings().ok()?;
        let features = self.model_feature_settings().ok()?;
        let base_url = assistant.base_url?.trim().trim_end_matches('/').to_string();
        let model = features.title.or(assistant.model)?.trim().to_string();
        let api_key = self
            .read_host_credential_text(ASSISTANT_AI_CREDENTIAL_KEY)?
            .trim()
            .to_string();
        if base_url.is_empty() || model.is_empty() || api_key.is_empty() {
            return None;
        }
        Some(TitleModelRequest {
            base_url,
            model,
            api_key,
        })
    }
}

fn title_host_error(error: DesktopApplicationError) -> TitleError {
    match error {
        DesktopApplicationError::Product(error) => TitleError::Product(error),
        DesktopApplicationError::InvalidInput { field, message } => {
            TitleError::InvalidInput { field, message }
        }
        DesktopApplicationError::PendingInteractionNotFound {
            task_id,
            request_id,
        } => TitleError::PendingInteractionNotFound {
            task_id,
            request_id,
        },
        DesktopApplicationError::InvalidPendingInteraction {
            request_id,
            message,
        } => TitleError::InvalidPendingInteraction {
            request_id,
            message,
        },
        other => TitleError::InvalidInput {
            field: "title_update",
            message: other.to_string(),
        },
    }
}

impl DesktopApplication {
    pub fn title_update_coordinator(&self) -> Arc<DesktopTitleUpdateCoordinator> {
        Arc::clone(&self.inner.title_update)
    }

    pub fn install_title_update_scheduler(
        &self,
        scheduler: Arc<dyn DesktopTitleUpdateScheduler>,
    ) -> Result<(), DesktopApplicationError> {
        self.inner
            .title_update_scheduler
            .set(scheduler)
            .map_err(|_| DesktopApplicationError::InvalidInput {
                field: "titleUpdateScheduler",
                message: "title update scheduler is already installed".to_owned(),
            })
    }

    pub fn task_title_state(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopTaskTitleState>, DesktopApplicationError> {
        Ok(task_title_state(self, task_id)?)
    }

    pub fn build_title_prompt_for_job(
        &self,
        job: &DesktopTitleUpdateJob,
    ) -> Result<Option<String>, DesktopApplicationError> {
        Ok(build_title_prompt_for_job(self, job)?)
    }

    pub fn apply_title_proposal(
        &self,
        job: &DesktopTitleUpdateJob,
        proposed: &str,
    ) -> Result<DesktopTitleUpdateDecision, DesktopApplicationError> {
        Ok(apply_title_proposal(
            self,
            &self.title_update_coordinator(),
            job,
            proposed,
        )?)
    }

    pub fn respond_title_update(
        &self,
        task_id: &TaskId,
        proposed: &str,
        accept: bool,
    ) -> Result<DesktopTaskTitleState, DesktopApplicationError> {
        Ok(respond_title_update(self, task_id, proposed, accept)?)
    }

    pub fn respond_title_update_review(
        &self,
        task_id: &TaskId,
        request_id: &str,
        accept: bool,
    ) -> Result<DesktopTaskTitleState, DesktopApplicationError> {
        Ok(respond_title_update_review(
            self, task_id, request_id, accept,
        )?)
    }

    pub fn normalize_generated_title(input: String) -> Result<String, String> {
        normalize_title(input)
    }

    pub fn request_title_update_after_turn(&self, task_id: TaskId, turn_id: Option<String>) {
        let Some(scheduler) = self.inner.title_update_scheduler.get() else {
            return;
        };
        scheduler.request(task_id, turn_id);
    }

    pub fn run_title_update_after_turn(
        &self,
        task_id: TaskId,
        turn_id: Option<String>,
    ) -> Result<(), String> {
        run_title_update_after_turn(self, &self.title_update_coordinator(), task_id, turn_id)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use lilia_contracts::{PendingProjectionStatus, TaskId};
    use lilia_feature_agent_session::persist_task_title;
    use serde_json::Value as JsonValue;
    use tempfile::TempDir;

    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, DesktopTaskCreate,
    };

    struct TestHost;

    impl DesktopHost for TestHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    fn temp_app() -> (TempDir, DesktopApplication) {
        let home = TempDir::new().unwrap();
        let config = DesktopApplicationConfig::new(
            home.path(),
            format!("title-update-test-{}", uuid::Uuid::new_v4()),
        )
        .unwrap();
        let application = DesktopApplication::bootstrap(config, Arc::new(TestHost)).unwrap();
        (home, application)
    }

    #[derive(Default)]
    struct RecordingScheduler {
        requests: Mutex<Vec<(TaskId, Option<String>)>>,
    }

    impl DesktopTitleUpdateScheduler for RecordingScheduler {
        fn request(&self, task_id: TaskId, turn_id: Option<String>) {
            self.requests.lock().unwrap().push((task_id, turn_id));
        }
    }

    #[test]
    fn a_finished_turn_hands_its_title_to_the_scheduler_instead_of_titling_inline() {
        let (_home, application) = temp_app();
        let scheduler = Arc::new(RecordingScheduler::default());
        application
            .install_title_update_scheduler(scheduler.clone())
            .unwrap();
        let task_id = TaskId::new("task-1").unwrap();

        application.request_title_update_after_turn(task_id.clone(), Some("turn-7".to_owned()));

        assert_eq!(
            scheduler.requests.lock().unwrap().as_slice(),
            [(task_id, Some("turn-7".to_owned()))]
        );
    }

    #[test]
    fn a_host_without_a_scheduler_leaves_the_task_untitled_rather_than_blocking_the_turn() {
        let (_home, application) = temp_app();

        application
            .request_title_update_after_turn(TaskId::new("task-1").unwrap(), Some("t".to_owned()));
    }

    #[test]
    fn the_scheduler_is_installed_once_so_two_hosts_cannot_both_title() {
        let (_home, application) = temp_app();
        application
            .install_title_update_scheduler(Arc::new(RecordingScheduler::default()))
            .unwrap();

        application
            .install_title_update_scheduler(Arc::new(RecordingScheduler::default()))
            .expect_err("a second scheduler is refused");
    }

    #[test]
    fn automatic_title_update_persists_one_success_event() {
        let (_home, application) = temp_app();
        let task = application
            .create_task(DesktopTaskCreate::new(None, "初始标题"))
            .unwrap();
        let job = application
            .title_update_coordinator()
            .schedule(
                application.task_title_state(&task.id).unwrap().unwrap(),
                Some("turn-auto-title".to_owned()),
                DesktopTimelineUpperBound {
                    turn_seq: 1,
                    intra_turn_order: 0,
                },
            )
            .unwrap()
            .unwrap();

        let decision = application
            .apply_title_proposal(&job, "新的自动标题")
            .unwrap();
        assert!(matches!(decision, DesktopTitleUpdateDecision::Success(_)));
        let current = application.task_title_state(&task.id).unwrap().unwrap();
        assert_eq!(current.title, "新的自动标题");
        assert_eq!(current.title_source, DesktopTaskTitleSource::Auto);

        let title_events = application
            .authority()
            .projection_timeline_for_task(&task.id)
            .into_iter()
            .filter(|event| event.kind == TITLE_UPDATE_ACTION_KIND)
            .collect::<Vec<_>>();
        assert_eq!(title_events.len(), 1);
        assert_eq!(title_events[0].status, "success");
        assert_eq!(title_events[0].summary.as_deref(), Some("新的自动标题"));
        assert_eq!(
            title_events[0]
                .payload
                .get("previousTitle")
                .and_then(JsonValue::as_str),
            Some("初始标题")
        );

        assert!(matches!(
            application
                .apply_title_proposal(&job, "新的自动标题")
                .unwrap(),
            DesktopTitleUpdateDecision::Stale(_)
        ));
        assert_eq!(
            application
                .authority()
                .projection_timeline_for_task(&task.id)
                .into_iter()
                .filter(|event| event.kind == TITLE_UPDATE_ACTION_KIND)
                .count(),
            1
        );
    }

    #[test]
    fn manual_title_reviews_are_durable_superseded_and_resolvable() {
        let (_home, application) = temp_app();
        let task = application
            .create_task(DesktopTaskCreate::new(None, "初始标题"))
            .unwrap();
        persist_task_title(
            &application,
            &task.id,
            "手动标题",
            DesktopTaskTitleSource::Manual,
            "初始标题",
        )
        .unwrap();

        let first_job = application
            .title_update_coordinator()
            .schedule(
                application.task_title_state(&task.id).unwrap().unwrap(),
                Some("turn-1".to_owned()),
                DesktopTimelineUpperBound {
                    turn_seq: 1,
                    intra_turn_order: 0,
                },
            )
            .unwrap()
            .unwrap();
        let DesktopTitleUpdateDecision::RequiresAction(first_review) = application
            .apply_title_proposal(&first_job, "第一版建议")
            .unwrap()
        else {
            panic!("manual title should require a review");
        };

        let second_job = application
            .title_update_coordinator()
            .schedule(
                application.task_title_state(&task.id).unwrap().unwrap(),
                Some("turn-2".to_owned()),
                DesktopTimelineUpperBound {
                    turn_seq: 2,
                    intra_turn_order: 0,
                },
            )
            .unwrap()
            .unwrap();
        let DesktopTitleUpdateDecision::RequiresAction(second_review) = application
            .apply_title_proposal(&second_job, "第二版建议")
            .unwrap()
        else {
            panic!("newer manual title proposal should require a review");
        };

        let pending = application.task_session_snapshot(&task.id).unwrap().pending;
        assert!(pending.iter().any(|item| {
            item.request_id == first_review.request_id
                && item.status == PendingProjectionStatus::Stale
        }));
        assert!(pending.iter().any(|item| {
            item.request_id == second_review.request_id
                && item.status == PendingProjectionStatus::Open
        }));
        assert_eq!(
            pending
                .iter()
                .filter(|item| {
                    item.kind == TITLE_UPDATE_ACTION_KIND
                        && item.status == PendingProjectionStatus::Open
                })
                .count(),
            1
        );

        let declined = application
            .respond_title_update_review(&task.id, &second_review.request_id, false)
            .unwrap();
        assert_eq!(declined.title, "手动标题");
        assert_eq!(declined.title_source, DesktopTaskTitleSource::Manual);

        let third_job = application
            .title_update_coordinator()
            .schedule(
                application.task_title_state(&task.id).unwrap().unwrap(),
                Some("turn-3".to_owned()),
                DesktopTimelineUpperBound {
                    turn_seq: 3,
                    intra_turn_order: 0,
                },
            )
            .unwrap()
            .unwrap();
        let DesktopTitleUpdateDecision::RequiresAction(third_review) = application
            .apply_title_proposal(&third_job, "最终建议标题")
            .unwrap()
        else {
            panic!("manual title should keep review semantics after a decline");
        };
        let accepted = application
            .respond_title_update_review(&task.id, &third_review.request_id, true)
            .unwrap();
        assert_eq!(accepted.title, "最终建议标题");
        assert_eq!(accepted.title_source, DesktopTaskTitleSource::Manual);
        assert!(application
            .task_session_snapshot(&task.id)
            .unwrap()
            .pending
            .iter()
            .any(|item| {
                item.request_id == third_review.request_id
                    && item.status == PendingProjectionStatus::Resolved
            }));
    }
}

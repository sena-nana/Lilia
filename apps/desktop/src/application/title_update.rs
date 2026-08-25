//! Shared title-update coordinator for desktop hosts.
//!
//! Application owns scheduling / freshness / apply decisions. Hosts only
//! resolve models, run the update off the turn worker, and invalidate UI via
//! events.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use lilia_contracts::{
    AgentSessionRef, PendingProjection, PendingProjectionStatus, ProjectionEventId, TaskId,
    TimelineProjectionCommand, TimelineProjectionEvent,
};
use lilia_storage::ProjectionApplyResult;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::application::{DesktopApplication, DesktopApplicationError, DesktopEventKind, DesktopTaskPatch};

const TITLE_REQUEST_TIMEOUT_SECS: u64 = 10;

pub const TITLE_UPDATE_ACTION_KIND: &str = "title_update";
pub const TITLE_MAX_CHARS: usize = 18;
pub const TITLE_MIN_CHARS: usize = 2;
const SAMPLE_TEXT_LIMIT: usize = 260;
const TITLE_SOURCE_SETTINGS_KEY: &str = "desktop.task-title-source.v1";
const TITLE_SOURCE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopTaskTitleSource {
    Auto,
    Manual,
}

impl DesktopTaskTitleSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim() {
            "manual" => Self::Manual,
            _ => Self::Auto,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopTaskTitleState {
    pub id: TaskId,
    pub project_id: Option<String>,
    pub title: String,
    pub title_source: DesktopTaskTitleSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DesktopTimelineUpperBound {
    pub turn_seq: i64,
    pub intra_turn_order: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TitleJobVersion {
    upper_bound: DesktopTimelineUpperBound,
    generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopTitleUpdateJob {
    pub task: DesktopTaskTitleState,
    pub turn_id: Option<String>,
    version: TitleJobVersion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopTitleUpdateReview {
    pub task: DesktopTaskTitleState,
    pub request_id: String,
    pub proposed_title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopTitleUpdateDecision {
    Success(DesktopTaskTitleState),
    RequiresAction(DesktopTitleUpdateReview),
    Stale(DesktopTaskTitleState),
    Unchanged,
    Stopped,
}

#[derive(Default)]
struct TitleGenerationState {
    next_generation: u64,
    latest_by_task: HashMap<String, TitleJobVersion>,
}

struct DesktopTitleUpdateCoordinatorInner {
    generations: Mutex<TitleGenerationState>,
    emissions: Mutex<()>,
    stopped: AtomicBool,
}

/// Carries a finished turn's title update away from the turn worker that
/// produced it. The desktop host answers this by submitting `lilia.agent/title@1`;
/// a host that installs nothing simply goes untitled, because naming a task is
/// never worth holding up the turn that just ended.
pub trait DesktopTitleUpdateScheduler: Send + Sync + 'static {
    fn request(&self, task_id: TaskId, turn_id: Option<String>);
}

#[derive(Clone)]
pub struct DesktopTitleUpdateCoordinator {
    inner: Arc<DesktopTitleUpdateCoordinatorInner>,
}

impl Default for DesktopTitleUpdateCoordinator {
    fn default() -> Self {
        Self {
            inner: Arc::new(DesktopTitleUpdateCoordinatorInner {
                generations: Mutex::new(TitleGenerationState::default()),
                emissions: Mutex::new(()),
                stopped: AtomicBool::new(false),
            }),
        }
    }
}

impl DesktopTitleUpdateCoordinator {
    pub fn schedule(
        &self,
        task: DesktopTaskTitleState,
        turn_id: Option<String>,
        upper_bound: DesktopTimelineUpperBound,
    ) -> Result<Option<DesktopTitleUpdateJob>, String> {
        let mut state = self
            .inner
            .generations
            .lock()
            .map_err(|_| "title generation state lock poisoned".to_string())?;
        if self.inner.stopped.load(Ordering::Acquire) {
            return Ok(None);
        }
        state.next_generation = state
            .next_generation
            .checked_add(1)
            .ok_or_else(|| "title generation exhausted".to_string())?;
        let version = TitleJobVersion {
            upper_bound,
            generation: state.next_generation,
        };
        if state
            .latest_by_task
            .get(task.id.as_str())
            .is_some_and(|current| current.upper_bound > upper_bound)
        {
            return Ok(Some(DesktopTitleUpdateJob {
                task,
                turn_id,
                version,
            }));
        }
        state
            .latest_by_task
            .insert(task.id.as_str().to_owned(), version);
        Ok(Some(DesktopTitleUpdateJob {
            task,
            turn_id,
            version,
        }))
    }

    pub fn is_latest(&self, job: &DesktopTitleUpdateJob) -> bool {
        if self.inner.stopped.load(Ordering::Acquire) {
            return false;
        }
        self.inner.generations.lock().ok().is_some_and(|state| {
            state.latest_by_task.get(job.task.id.as_str()) == Some(&job.version)
        })
    }

    pub fn decide_proposal(
        &self,
        job: &DesktopTitleUpdateJob,
        proposed: &str,
        current: DesktopTaskTitleState,
    ) -> DesktopTitleUpdateDecision {
        let Ok(state) = self.inner.generations.lock() else {
            return DesktopTitleUpdateDecision::Stopped;
        };
        if self.inner.stopped.load(Ordering::Acquire) {
            return DesktopTitleUpdateDecision::Stopped;
        }
        if state.latest_by_task.get(job.task.id.as_str()) != Some(&job.version) {
            return DesktopTitleUpdateDecision::Stale(job.task.clone());
        }
        if current.title_source == DesktopTaskTitleSource::Manual {
            return DesktopTitleUpdateDecision::RequiresAction(DesktopTitleUpdateReview {
                task: current,
                request_id: uuid::Uuid::new_v4().to_string(),
                proposed_title: proposed.to_owned(),
            });
        }
        if current.title != job.task.title || current.title_source != job.task.title_source {
            return DesktopTitleUpdateDecision::Stale(current);
        }
        if proposed == compact_line(&current.title) {
            return DesktopTitleUpdateDecision::Unchanged;
        }
        DesktopTitleUpdateDecision::Success(current)
    }

    pub fn while_running(&self, action: impl FnOnce()) {
        let Ok(_emission) = self.inner.emissions.lock() else {
            return;
        };
        if self.inner.stopped.load(Ordering::Acquire) {
            return;
        }
        action();
    }

    pub fn shutdown(&self) {
        self.inner.stopped.store(true, Ordering::Release);
        if let Ok(mut state) = self.inner.generations.lock() {
            state.latest_by_task.clear();
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct StoredTitleSources {
    schema_version: u32,
    #[serde(default)]
    sources: HashMap<String, String>,
}

impl DesktopApplication {
    pub fn title_update_coordinator(&self) -> Arc<DesktopTitleUpdateCoordinator> {
        Arc::clone(&self.inner.title_update)
    }

    /// Installed once by the host, after the kernel it submits to exists.
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
        let task = match self.get_task(task_id) {
            Ok(task) => task,
            Err(DesktopApplicationError::Product(lilia_contracts::ProductError::NotFound {
                ..
            })) => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(Some(DesktopTaskTitleState {
            id: task.id.clone(),
            project_id: task.project_id.as_ref().map(|id| id.as_str().to_owned()),
            title: task.title,
            title_source: self.task_title_source(task_id)?,
        }))
    }

    fn schedule_title_update(
        &self,
        task_id: &TaskId,
        turn_id: Option<String>,
    ) -> Result<Option<DesktopTitleUpdateJob>, DesktopApplicationError> {
        let Some(task) = self.task_title_state(task_id)? else {
            return Ok(None);
        };
        let Some(upper_bound) = self.timeline_upper_bound(task_id, turn_id.as_deref())? else {
            return Ok(None);
        };
        self.title_update_coordinator()
            .schedule(task, turn_id, upper_bound)
            .map_err(|message| DesktopApplicationError::InvalidInput {
                field: "title_update",
                message,
            })
    }

    pub fn build_title_prompt_for_job(
        &self,
        job: &DesktopTitleUpdateJob,
    ) -> Result<Option<String>, DesktopApplicationError> {
        let samples = self.timeline_title_samples(&job.task.id, job.version.upper_bound)?;
        if samples.is_empty() {
            return Ok(None);
        }
        let mut lines = vec![
            "你是 LiliaCode 的对话标题助手。基于下方最近对话内容生成一个新的中文短标题。"
                .to_string(),
            "只输出标题本身，不要引号、解释、Markdown 或标点包装。".to_string(),
            "标题应概括当前真实任务方向或根因，6 到 18 个中文字，避免“帮我”“请你”等泛词。"
                .to_string(),
            format!(
                "当前标题: {}",
                truncate_chars(&compact_line(&job.task.title), 80)
            ),
        ];
        lines.extend(samples);
        Ok(Some(lines.join("\n")))
    }

    pub fn apply_title_proposal(
        &self,
        job: &DesktopTitleUpdateJob,
        proposed: &str,
    ) -> Result<DesktopTitleUpdateDecision, DesktopApplicationError> {
        let coordinator = self.title_update_coordinator();
        if !coordinator.is_latest(job) {
            return Ok(DesktopTitleUpdateDecision::Stale(job.task.clone()));
        }
        let Some(current) = self.task_title_state(&job.task.id)? else {
            return Ok(DesktopTitleUpdateDecision::Stale(job.task.clone()));
        };
        let decision = coordinator.decide_proposal(job, proposed, current);
        match &decision {
            DesktopTitleUpdateDecision::Success(current) => {
                if !self.persist_task_title(
                    &job.task.id,
                    proposed,
                    DesktopTaskTitleSource::Auto,
                    &current.title,
                )? {
                    return Ok(DesktopTitleUpdateDecision::Stale(
                        self.task_title_state(&job.task.id)?
                            .unwrap_or_else(|| job.task.clone()),
                    ));
                }
                self.persist_title_update_success(job, current, proposed)?;
            }
            DesktopTitleUpdateDecision::RequiresAction(review) => {
                self.persist_title_update_review(job, review)?;
            }
            DesktopTitleUpdateDecision::Stale(_)
            | DesktopTitleUpdateDecision::Unchanged
            | DesktopTitleUpdateDecision::Stopped => {}
        }
        Ok(decision)
    }

    pub fn respond_title_update(
        &self,
        task_id: &TaskId,
        proposed: &str,
        accept: bool,
    ) -> Result<DesktopTaskTitleState, DesktopApplicationError> {
        let Some(current) = self.task_title_state(task_id)? else {
            return Err(DesktopApplicationError::InvalidInput {
                field: "task_id",
                message: "任务不存在".to_owned(),
            });
        };
        if accept {
            self.persist_task_title(
                task_id,
                proposed,
                DesktopTaskTitleSource::Manual,
                &current.title,
            )?;
        }
        self.task_title_state(task_id)?
            .ok_or_else(|| DesktopApplicationError::InvalidInput {
                field: "task_id",
                message: "任务不存在".to_owned(),
            })
    }

    pub fn respond_title_update_review(
        &self,
        task_id: &TaskId,
        request_id: &str,
        accept: bool,
    ) -> Result<DesktopTaskTitleState, DesktopApplicationError> {
        let pending = self
            .task_session_snapshot(task_id)?
            .pending
            .into_iter()
            .find(|pending| {
                pending.request_id == request_id && pending.kind == TITLE_UPDATE_ACTION_KIND
            })
            .ok_or_else(|| DesktopApplicationError::PendingInteractionNotFound {
                task_id: task_id.clone(),
                request_id: request_id.to_owned(),
            })?;
        if pending.status != PendingProjectionStatus::Open {
            return self.task_title_state(task_id)?.ok_or_else(|| {
                DesktopApplicationError::InvalidInput {
                    field: "task_id",
                    message: "任务不存在".to_owned(),
                }
            });
        }
        let proposed = pending
            .payload
            .get("proposedTitle")
            .and_then(JsonValue::as_str)
            .map(str::to_owned)
            .and_then(|value| normalize_title(value).ok())
            .ok_or_else(|| DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "标题更新请求缺少候选标题".to_owned(),
            })?;
        let current = self.respond_title_update(task_id, &proposed, accept)?;
        let sequence = pending.sequence.checked_add(1).ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "标题更新请求版本已耗尽".to_owned(),
            }
        })?;
        self.authority()
            .apply_projection(TimelineProjectionCommand::ResolvePending {
                session_id: pending.agent_session.as_str().to_owned(),
                request_id: request_id.to_owned(),
                status: if accept {
                    PendingProjectionStatus::Resolved
                } else {
                    PendingProjectionStatus::Cancelled
                },
                sequence,
                response: serde_json::json!({
                    "accepted": accept,
                    "decision": if accept { "accept" } else { "decline" },
                    "proposedTitle": proposed,
                }),
            })?;
        self.emit_event(DesktopEventKind::TimelineChanged {
            task_id: task_id.clone(),
            cursor: None,
        });
        Ok(current)
    }

    pub fn normalize_generated_title(input: String) -> Result<String, String> {
        normalize_title(input)
    }

    /// Hands auto-titling to the host after a completed turn. Runs nothing here:
    /// the caller is the turn worker, and titling calls a model.
    pub fn request_title_update_after_turn(&self, task_id: TaskId, turn_id: Option<String>) {
        let Some(scheduler) = self.inner.title_update_scheduler.get() else {
            return;
        };
        scheduler.request(task_id, turn_id);
    }

    /// Runs one requested title update to completion. Silent skip when the
    /// model or prompt is unavailable, or when a later turn already superseded
    /// this one.
    pub fn run_title_update_after_turn(
        &self,
        task_id: TaskId,
        turn_id: Option<String>,
    ) -> Result<(), String> {
        let job = match self.schedule_title_update(&task_id, turn_id) {
            Ok(Some(job)) => job,
            Ok(None) => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        if !self.title_update_coordinator().is_latest(&job) {
            return Ok(());
        }
        let Some(prompt) = self
            .build_title_prompt_for_job(&job)
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        // An unconfigured auxiliary model is a setting, not a failure: failing
        // here would record one job failure per completed turn for every user
        // who never set one up.
        let Some(model) = self.resolve_title_model_request() else {
            return Ok(());
        };
        let proposed = request_title(&model, &prompt).and_then(normalize_title)?;
        self.apply_title_proposal(&job, &proposed)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn resolve_title_model_request(&self) -> Option<TitleModelRequest> {
        let assistant = self.assistant_ai_settings().ok()?;
        let features = self.model_feature_settings().ok()?;
        let base_url = assistant.base_url?.trim().trim_end_matches('/').to_string();
        let model = features.title.or(assistant.model)?.trim().to_string();
        let api_key = self.read_assistant_ai_secret()?.trim().to_string();
        if base_url.is_empty() || model.is_empty() || api_key.is_empty() {
            return None;
        }
        Some(TitleModelRequest {
            base_url,
            model,
            api_key,
        })
    }

    fn read_assistant_ai_secret(&self) -> Option<String> {
        self.read_host_credential_text(crate::application::ASSISTANT_AI_CREDENTIAL_KEY)
    }

    fn timeline_upper_bound(
        &self,
        task_id: &TaskId,
        turn_id: Option<&str>,
    ) -> Result<Option<DesktopTimelineUpperBound>, DesktopApplicationError> {
        let events = self.task_timeline_page(task_id, None, 200)?.events;
        let mut best: Option<DesktopTimelineUpperBound> = None;
        for event in events {
            if turn_id.is_some_and(|expected| event.turn_id.as_deref() != Some(expected)) {
                continue;
            }
            let bound = DesktopTimelineUpperBound {
                turn_seq: event.sequence as i64,
                intra_turn_order: 0,
            };
            best = Some(match best {
                Some(current) if current >= bound => current,
                _ => bound,
            });
        }
        Ok(best)
    }

    fn timeline_title_samples(
        &self,
        task_id: &TaskId,
        upper_bound: DesktopTimelineUpperBound,
    ) -> Result<Vec<String>, DesktopApplicationError> {
        let events = self.task_timeline_page(task_id, None, 200)?.events;
        let mut samples = Vec::new();
        for event in events
            .into_iter()
            .filter(|event| {
                matches!(event.kind.as_str(), "message" | "todo_list" | "error")
                    && (event.sequence as i64) <= upper_bound.turn_seq
            })
            .rev()
            .take(16)
        {
            if let Some(sample) = title_sample_from_event(&event) {
                samples.push(sample);
            }
        }
        Ok(samples)
    }

    fn task_title_source(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopTaskTitleSource, DesktopApplicationError> {
        Ok(self
            .load_title_sources()?
            .sources
            .get(task_id.as_str())
            .map(|value| DesktopTaskTitleSource::parse(value))
            .unwrap_or(DesktopTaskTitleSource::Auto))
    }

    fn persist_task_title(
        &self,
        task_id: &TaskId,
        title: &str,
        source: DesktopTaskTitleSource,
        expected_title: &str,
    ) -> Result<bool, DesktopApplicationError> {
        let current = match self.get_task(task_id) {
            Ok(task) => task,
            Err(DesktopApplicationError::Product(lilia_contracts::ProductError::NotFound {
                ..
            })) => return Ok(false),
            Err(error) => return Err(error),
        };
        if current.title != expected_title && source == DesktopTaskTitleSource::Auto {
            return Ok(false);
        }
        self.update_task(
            task_id,
            DesktopTaskPatch {
                title: Some(title.to_owned()),
                ..DesktopTaskPatch::default()
            },
        )?;
        let mut stored = self.load_title_sources()?;
        stored.schema_version = TITLE_SOURCE_SCHEMA_VERSION;
        stored
            .sources
            .insert(task_id.as_str().to_owned(), source.as_str().to_owned());
        self.save_title_sources(&stored)?;
        self.emit_event(DesktopEventKind::TasksChanged {
            project_id: current.project_id,
            task_id: Some(task_id.clone()),
        });
        Ok(true)
    }

    fn persist_title_update_success(
        &self,
        job: &DesktopTitleUpdateJob,
        previous: &DesktopTaskTitleState,
        proposed: &str,
    ) -> Result<(), DesktopApplicationError> {
        let sequence = self
            .authority()
            .projection_timeline_for_task(&job.task.id)
            .into_iter()
            .map(|event| event.sequence)
            .max()
            .unwrap_or_default()
            .saturating_add(1);
        let request_id = job.turn_id.clone().unwrap_or_else(|| {
            format!(
                "{}-{}-{}",
                job.version.upper_bound.turn_seq,
                job.version.upper_bound.intra_turn_order,
                job.version.generation
            )
        });
        let session =
            AgentSessionRef::new(format!("desktop-title-update:{}", job.task.id.as_str()))?;
        let event = TimelineProjectionEvent {
            id: ProjectionEventId::new(title_event_id(job.task.id.as_str(), &request_id)),
            task_id: job.task.id.clone(),
            agent_session: session,
            sequence,
            turn_id: job.turn_id.clone(),
            kind: TITLE_UPDATE_ACTION_KIND.to_owned(),
            status: "success".to_owned(),
            title: "标题已更新".to_owned(),
            summary: Some(proposed.to_owned()),
            payload: serde_json::json!({
                "proposedTitle": proposed,
                "previousTitle": previous.title,
                "requestId": request_id,
            }),
            projected: true,
        };
        let applied = self
            .authority()
            .apply_projection(TimelineProjectionCommand::UpsertTimelineEvent { event })?;
        if applied != ProjectionApplyResult::DuplicateIgnored {
            self.emit_event(DesktopEventKind::TimelineChanged {
                task_id: job.task.id.clone(),
                cursor: Some(sequence),
            });
        }
        Ok(())
    }

    fn load_title_sources(&self) -> Result<StoredTitleSources, DesktopApplicationError> {
        let value = self
            .title_source_store()?
            .setting(TITLE_SOURCE_SETTINGS_KEY)
            .map_err(|error| DesktopApplicationError::InvalidInput {
                field: "title_source",
                message: error.to_string(),
            })?;
        let Some(value) = value else {
            return Ok(StoredTitleSources {
                schema_version: TITLE_SOURCE_SCHEMA_VERSION,
                sources: HashMap::new(),
            });
        };
        serde_json::from_value(value).map_err(|error| DesktopApplicationError::InvalidInput {
            field: "title_source",
            message: error.to_string(),
        })
    }

    fn save_title_sources(
        &self,
        stored: &StoredTitleSources,
    ) -> Result<(), DesktopApplicationError> {
        let value = serde_json::to_value(stored).map_err(|error| {
            DesktopApplicationError::InvalidInput {
                field: "title_source",
                message: error.to_string(),
            }
        })?;
        self.title_source_store()?
            .put_setting(TITLE_SOURCE_SETTINGS_KEY, &value)
            .map_err(|error| DesktopApplicationError::InvalidInput {
                field: "title_source",
                message: error.to_string(),
            })
    }

    fn title_source_store(
        &self,
    ) -> Result<lilia_storage::SqliteAgentRuntimeStateStore, DesktopApplicationError> {
        self.config()
            .data_paths()
            .ensure_layout()
            .map_err(|error| DesktopApplicationError::InvalidInput {
                field: "title_source",
                message: error.to_string(),
            })?;
        lilia_storage::SqliteAgentRuntimeStateStore::open(
            self.config().data_paths().agent_runtime_db(),
        )
        .map_err(|error| DesktopApplicationError::InvalidInput {
            field: "title_source",
            message: error.to_string(),
        })
    }

    fn persist_title_update_review(
        &self,
        job: &DesktopTitleUpdateJob,
        review: &DesktopTitleUpdateReview,
    ) -> Result<(), DesktopApplicationError> {
        let session = AgentSessionRef::new(format!("title-review-{}", review.request_id)).map_err(
            |error| DesktopApplicationError::InvalidPendingInteraction {
                request_id: review.request_id.clone(),
                message: error.to_string(),
            },
        )?;
        self.authority()
            .apply_projection(TimelineProjectionCommand::UpsertPending {
                pending: PendingProjection {
                    id: format!("{}:{}", session.as_str(), review.request_id),
                    task_id: review.task.id.clone(),
                    agent_session: session,
                    sequence: 1,
                    turn_id: job.turn_id.clone(),
                    request_id: review.request_id.clone(),
                    kind: TITLE_UPDATE_ACTION_KIND.to_owned(),
                    status: PendingProjectionStatus::Open,
                    prompt: Some(format!("建议将标题更新为「{}」", review.proposed_title)),
                    action_revision: Some(1),
                    payload: serde_json::json!({
                        "proposedTitle": review.proposed_title,
                        "previousTitle": review.task.title,
                        "requestId": review.request_id,
                    }),
                },
            })?;

        let stale_reviews = self
            .task_session_snapshot(&review.task.id)?
            .pending
            .into_iter()
            .filter(|pending| {
                pending.kind == TITLE_UPDATE_ACTION_KIND
                    && pending.status == PendingProjectionStatus::Open
                    && pending.request_id != review.request_id
            })
            .collect::<Vec<_>>();
        for pending in stale_reviews {
            let Some(sequence) = pending.sequence.checked_add(1) else {
                continue;
            };
            self.authority()
                .apply_projection(TimelineProjectionCommand::ResolvePending {
                    session_id: pending.agent_session.as_str().to_owned(),
                    request_id: pending.request_id,
                    status: PendingProjectionStatus::Stale,
                    sequence,
                    response: serde_json::json!({ "supersededBy": review.request_id }),
                })?;
        }
        self.emit_event(DesktopEventKind::TimelineChanged {
            task_id: review.task.id.clone(),
            cursor: None,
        });
        Ok(())
    }
}

fn title_sample_from_event(event: &TimelineProjectionEvent) -> Option<String> {
    let text = match event.kind.as_str() {
        "message" => event
            .payload
            .get("content")
            .and_then(|value| value.as_str())
            .or(event.summary.as_deref())
            .map(compact_line)
            .filter(|value| !value.is_empty())?,
        "todo_list" => {
            let items = unfinished_todo_texts(&event.payload);
            if items.is_empty() {
                return None;
            }
            format!("待办: {}", items.join(" / "))
        }
        "error" => compact_line(event.summary.as_deref().unwrap_or(event.title.as_str())),
        _ => return None,
    };
    if text.is_empty() {
        return None;
    }
    Some(format!(
        "{}: {}",
        event.kind,
        truncate_chars(&text, SAMPLE_TEXT_LIMIT)
    ))
}

fn unfinished_todo_texts(payload: &JsonValue) -> Vec<String> {
    let Some(items) = payload
        .get("items")
        .or_else(|| payload.get("todos"))
        .and_then(|value| value.as_array())
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            if item
                .get("done")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
                || item
                    .get("completed")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
            {
                return None;
            }
            item.as_str()
                .map(str::to_string)
                .or_else(|| {
                    item.get("text")
                        .or_else(|| item.get("content"))
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                })
                .map(|value| compact_line(&value))
                .filter(|value| !value.is_empty())
        })
        .take(4)
        .collect()
}

pub fn normalize_title(input: String) -> Result<String, String> {
    let mut title = compact_line(&input);
    for prefix in ["标题：", "标题:", "Title:", "title:"] {
        if let Some(rest) = title.strip_prefix(prefix) {
            title = compact_line(rest);
        }
    }
    title = title
        .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '`' | '“' | '”' | '‘' | '’'))
        .to_string();
    title = compact_line(&title);
    if title.chars().count() < TITLE_MIN_CHARS {
        return Err("generated title too short".to_string());
    }
    Ok(truncate_chars(&title, TITLE_MAX_CHARS)
        .trim_end_matches('…')
        .to_string())
}

pub fn title_event_id(task_id: &str, request_id: &str) -> String {
    format!("title-update:{task_id}:{request_id}")
}

struct TitleModelRequest {
    base_url: String,
    model: String,
    api_key: String,
}

fn request_title(model: &TitleModelRequest, prompt: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(TITLE_REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|error| format!("HTTP client failed: {error}"))?;
    let url = format!("{}/chat/completions", model.base_url.trim_end_matches('/'));
    let response = client
        .post(url)
        .bearer_auth(&model.api_key)
        .json(&serde_json::json!({
            "model": model.model,
            "messages": [
                { "role": "system", "content": title_system_instruction() },
                { "role": "user", "content": prompt }
            ],
            "temperature": 0.2,
            "max_tokens": 40
        }))
        .send()
        .map_err(|error| format!("title request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("title request HTTP {}", response.status()));
    }
    let value = response
        .json::<JsonValue>()
        .map_err(|error| format!("title response parse failed: {error}"))?;
    value
        .get("choices")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "title response missing content".to_string())
}

fn compact_line(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(input: &str, max: usize) -> String {
    let mut out = String::new();
    for (index, ch) in input.chars().enumerate() {
        if index >= max {
            out.push('…');
            return out;
        }
        out.push(ch);
    }
    out
}

static TITLE_SYSTEM_INSTRUCTION: OnceLock<String> = OnceLock::new();

pub fn title_system_instruction() -> &'static str {
    TITLE_SYSTEM_INSTRUCTION.get_or_init(|| {
        #[derive(Deserialize)]
        struct TitleSection {
            #[serde(rename = "systemInstruction")]
            system_instruction: String,
        }
        #[derive(Deserialize)]
        struct PromptText {
            title: TitleSection,
        }
        let contract: PromptText = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/lilia-contracts/contracts/prompt-text.json"
        )))
        .expect("prompt-text.json must deserialize");
        contract.title.system_instruction
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use lilia_contracts::TaskId;
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

    #[test]
    fn newest_generation_wins_for_decide() {
        let coordinator = DesktopTitleUpdateCoordinator::default();
        let task_id = TaskId::new("task-1").unwrap();
        let task = DesktopTaskTitleState {
            id: task_id.clone(),
            project_id: None,
            title: "初始标题".into(),
            title_source: DesktopTaskTitleSource::Auto,
        };
        let job_a = coordinator
            .schedule(
                task.clone(),
                Some("turn-a".into()),
                DesktopTimelineUpperBound {
                    turn_seq: 1,
                    intra_turn_order: 0,
                },
            )
            .unwrap()
            .unwrap();
        let job_b = coordinator
            .schedule(
                task.clone(),
                Some("turn-b".into()),
                DesktopTimelineUpperBound {
                    turn_seq: 2,
                    intra_turn_order: 0,
                },
            )
            .unwrap()
            .unwrap();
        assert!(coordinator.is_latest(&job_b));
        assert!(!coordinator.is_latest(&job_a));
        let decision = coordinator.decide_proposal(&job_a, "旧标题", task.clone());
        assert!(matches!(decision, DesktopTitleUpdateDecision::Stale(_)));
        let decision = coordinator.decide_proposal(&job_b, "新标题", task);
        assert!(matches!(decision, DesktopTitleUpdateDecision::Success(_)));
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
    fn manual_title_requires_action() {
        let coordinator = DesktopTitleUpdateCoordinator::default();
        let task_id = TaskId::new("task-2").unwrap();
        let task = DesktopTaskTitleState {
            id: task_id,
            project_id: None,
            title: "手动标题".into(),
            title_source: DesktopTaskTitleSource::Manual,
        };
        let job = coordinator
            .schedule(
                task.clone(),
                None,
                DesktopTimelineUpperBound {
                    turn_seq: 1,
                    intra_turn_order: 0,
                },
            )
            .unwrap()
            .unwrap();
        let decision = coordinator.decide_proposal(&job, "建议标题", task);
        let DesktopTitleUpdateDecision::RequiresAction(review) = decision else {
            panic!("manual title should create a review");
        };
        assert_eq!(review.task.title, "手动标题");
        assert_eq!(review.proposed_title, "建议标题");
        assert!(!review.request_id.is_empty());
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
        application
            .persist_task_title(
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

    #[test]
    fn normalize_title_strips_wrappers() {
        assert_eq!(
            normalize_title("标题：`对话标题事件化实现进度需要继续确认更多内容`".into()).unwrap(),
            "对话标题事件化实现进度需要继续确认更"
        );
        assert!(normalize_title(" ".into()).is_err());
    }
}

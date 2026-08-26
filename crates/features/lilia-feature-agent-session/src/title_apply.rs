//! Title HTTP, persistence and timeline projection.
//!
//! The host supplies task / projection I/O and the auxiliary-model credential.
//! This module owns prompt assembly, the model request, apply/review writes
//! and the title-source map.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use lilia_contracts::{
    AgentSessionRef, PendingProjection, PendingProjectionStatus, ProjectId, ProjectionEventId,
    TaskId, TimelineProjectionCommand, TimelineProjectionEvent,
};
use lilia_storage::ProjectionApplyResult;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::title_coordinator::{
    compact_line, normalize_title, title_event_id, title_system_instruction, truncate_chars,
    DesktopTaskTitleSource, DesktopTaskTitleState, DesktopTimelineUpperBound,
    DesktopTitleUpdateCoordinator, DesktopTitleUpdateDecision, DesktopTitleUpdateJob,
    DesktopTitleUpdateReview, TITLE_SOURCE_SCHEMA_VERSION, TITLE_SOURCE_SETTINGS_KEY,
    TITLE_UPDATE_ACTION_KIND,
};

const TITLE_REQUEST_TIMEOUT_SECS: u64 = 10;
const SAMPLE_TEXT_LIMIT: usize = 260;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum TitleError {
    #[error("invalid desktop input `{field}`: {message}")]
    InvalidInput {
        field: &'static str,
        message: String,
    },
    #[error("task not found")]
    TaskNotFound,
    #[error(transparent)]
    Product(#[from] lilia_contracts::ProductError),
    #[error("pending interaction `{request_id}` was not found for task `{task_id}`")]
    PendingInteractionNotFound { task_id: TaskId, request_id: String },
    #[error("invalid pending interaction `{request_id}`: {message}")]
    InvalidPendingInteraction { request_id: String, message: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StoredTitleSources {
    pub schema_version: u32,
    #[serde(default)]
    pub sources: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleModelRequest {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

/// Host I/O for titling: tasks, projections, settings file and model config.
pub trait TitleHost: Send + Sync {
    fn load_task(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<(String, Option<ProjectId>)>, TitleError>;
    fn write_task_title(&self, task_id: &TaskId, title: &str) -> Result<(), TitleError>;
    fn timeline_events(
        &self,
        task_id: &TaskId,
        limit: u32,
    ) -> Result<Vec<TimelineProjectionEvent>, TitleError>;
    fn apply_projection(
        &self,
        command: TimelineProjectionCommand,
    ) -> Result<ProjectionApplyResult, TitleError>;
    fn pending_projections(&self, task_id: &TaskId) -> Result<Vec<PendingProjection>, TitleError>;
    fn prepare_title_source_store(&self) -> Result<PathBuf, TitleError>;
    fn emit_tasks_changed(&self, project_id: Option<ProjectId>, task_id: TaskId);
    fn emit_timeline_changed(&self, task_id: TaskId, cursor: Option<u64>);
    fn title_model(&self) -> Option<TitleModelRequest>;
}

pub fn task_title_state(
    host: &dyn TitleHost,
    task_id: &TaskId,
) -> Result<Option<DesktopTaskTitleState>, TitleError> {
    let Some((title, project_id)) = host.load_task(task_id)? else {
        return Ok(None);
    };
    Ok(Some(DesktopTaskTitleState {
        id: task_id.clone(),
        project_id: project_id.as_ref().map(|id| id.as_str().to_owned()),
        title,
        title_source: task_title_source(host, task_id)?,
    }))
}

pub fn schedule_title_update(
    host: &dyn TitleHost,
    coordinator: &DesktopTitleUpdateCoordinator,
    task_id: &TaskId,
    turn_id: Option<String>,
) -> Result<Option<DesktopTitleUpdateJob>, TitleError> {
    let Some(task) = task_title_state(host, task_id)? else {
        return Ok(None);
    };
    let Some(upper_bound) = timeline_upper_bound(host, task_id, turn_id.as_deref())? else {
        return Ok(None);
    };
    coordinator
        .schedule(task, turn_id, upper_bound)
        .map_err(|message| TitleError::InvalidInput {
            field: "title_update",
            message,
        })
}

pub fn build_title_prompt_for_job(
    host: &dyn TitleHost,
    job: &DesktopTitleUpdateJob,
) -> Result<Option<String>, TitleError> {
    let samples = timeline_title_samples(host, &job.task.id, job.upper_bound())?;
    if samples.is_empty() {
        return Ok(None);
    }
    let mut lines = vec![
        "你是 LiliaCode 的对话标题助手。基于下方最近对话内容生成一个新的中文短标题。".to_string(),
        "只输出标题本身，不要引号、解释、Markdown 或标点包装。".to_string(),
        "标题应概括当前真实任务方向或根因，6 到 18 个中文字，避免“帮我”“请你”等泛词。".to_string(),
        format!(
            "当前标题: {}",
            truncate_chars(&compact_line(&job.task.title), 80)
        ),
    ];
    lines.extend(samples);
    Ok(Some(lines.join("\n")))
}

pub fn apply_title_proposal(
    host: &dyn TitleHost,
    coordinator: &DesktopTitleUpdateCoordinator,
    job: &DesktopTitleUpdateJob,
    proposed: &str,
) -> Result<DesktopTitleUpdateDecision, TitleError> {
    if !coordinator.is_latest(job) {
        return Ok(DesktopTitleUpdateDecision::Stale(job.task.clone()));
    }
    let Some(current) = task_title_state(host, &job.task.id)? else {
        return Ok(DesktopTitleUpdateDecision::Stale(job.task.clone()));
    };
    let decision = coordinator.decide_proposal(job, proposed, current);
    match &decision {
        DesktopTitleUpdateDecision::Success(current) => {
            if !persist_task_title(
                host,
                &job.task.id,
                proposed,
                DesktopTaskTitleSource::Auto,
                &current.title,
            )? {
                return Ok(DesktopTitleUpdateDecision::Stale(
                    task_title_state(host, &job.task.id)?.unwrap_or_else(|| job.task.clone()),
                ));
            }
            persist_title_update_success(host, job, current, proposed)?;
        }
        DesktopTitleUpdateDecision::RequiresAction(review) => {
            persist_title_update_review(host, job, review)?;
        }
        DesktopTitleUpdateDecision::Stale(_)
        | DesktopTitleUpdateDecision::Unchanged
        | DesktopTitleUpdateDecision::Stopped => {}
    }
    Ok(decision)
}

pub fn respond_title_update(
    host: &dyn TitleHost,
    task_id: &TaskId,
    proposed: &str,
    accept: bool,
) -> Result<DesktopTaskTitleState, TitleError> {
    let Some(current) = task_title_state(host, task_id)? else {
        return Err(TitleError::InvalidInput {
            field: "task_id",
            message: "任务不存在".to_owned(),
        });
    };
    if accept {
        persist_task_title(
            host,
            task_id,
            proposed,
            DesktopTaskTitleSource::Manual,
            &current.title,
        )?;
    }
    task_title_state(host, task_id)?.ok_or_else(|| TitleError::InvalidInput {
        field: "task_id",
        message: "任务不存在".to_owned(),
    })
}

pub fn respond_title_update_review(
    host: &dyn TitleHost,
    task_id: &TaskId,
    request_id: &str,
    accept: bool,
) -> Result<DesktopTaskTitleState, TitleError> {
    let pending = host
        .pending_projections(task_id)?
        .into_iter()
        .find(|pending| {
            pending.request_id == request_id && pending.kind == TITLE_UPDATE_ACTION_KIND
        })
        .ok_or_else(|| TitleError::PendingInteractionNotFound {
            task_id: task_id.clone(),
            request_id: request_id.to_owned(),
        })?;
    if pending.status != PendingProjectionStatus::Open {
        return task_title_state(host, task_id)?.ok_or_else(|| TitleError::InvalidInput {
            field: "task_id",
            message: "任务不存在".to_owned(),
        });
    }
    let proposed = pending
        .payload
        .get("proposedTitle")
        .and_then(JsonValue::as_str)
        .map(str::to_owned)
        .and_then(|value| normalize_title(value).ok())
        .ok_or_else(|| TitleError::InvalidPendingInteraction {
            request_id: request_id.to_owned(),
            message: "标题更新请求缺少候选标题".to_owned(),
        })?;
    let current = respond_title_update(host, task_id, &proposed, accept)?;
    let sequence =
        pending
            .sequence
            .checked_add(1)
            .ok_or_else(|| TitleError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "标题更新请求版本已耗尽".to_owned(),
            })?;
    host.apply_projection(TimelineProjectionCommand::ResolvePending {
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
    host.emit_timeline_changed(task_id.clone(), None);
    Ok(current)
}

pub fn run_title_update_after_turn(
    host: &dyn TitleHost,
    coordinator: &DesktopTitleUpdateCoordinator,
    task_id: TaskId,
    turn_id: Option<String>,
) -> Result<(), String> {
    let job = match schedule_title_update(host, coordinator, &task_id, turn_id) {
        Ok(Some(job)) => job,
        Ok(None) => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if !coordinator.is_latest(&job) {
        return Ok(());
    }
    let Some(prompt) = build_title_prompt_for_job(host, &job).map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let Some(model) = host.title_model() else {
        return Ok(());
    };
    let proposed = request_title(&model, &prompt).and_then(normalize_title)?;
    apply_title_proposal(host, coordinator, &job, &proposed).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn persist_task_title(
    host: &dyn TitleHost,
    task_id: &TaskId,
    title: &str,
    source: DesktopTaskTitleSource,
    expected_title: &str,
) -> Result<bool, TitleError> {
    let Some((current_title, project_id)) = host.load_task(task_id)? else {
        return Ok(false);
    };
    if current_title != expected_title && source == DesktopTaskTitleSource::Auto {
        return Ok(false);
    }
    host.write_task_title(task_id, title)?;
    let mut stored = load_title_sources(host)?;
    stored.schema_version = TITLE_SOURCE_SCHEMA_VERSION;
    stored
        .sources
        .insert(task_id.as_str().to_owned(), source.as_str().to_owned());
    save_title_sources(host, &stored)?;
    host.emit_tasks_changed(project_id, task_id.clone());
    Ok(true)
}

pub fn request_title(model: &TitleModelRequest, prompt: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(TITLE_REQUEST_TIMEOUT_SECS))
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

fn timeline_upper_bound(
    host: &dyn TitleHost,
    task_id: &TaskId,
    turn_id: Option<&str>,
) -> Result<Option<DesktopTimelineUpperBound>, TitleError> {
    let events = host.timeline_events(task_id, 200)?;
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
    host: &dyn TitleHost,
    task_id: &TaskId,
    upper_bound: DesktopTimelineUpperBound,
) -> Result<Vec<String>, TitleError> {
    let events = host.timeline_events(task_id, 200)?;
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
    host: &dyn TitleHost,
    task_id: &TaskId,
) -> Result<DesktopTaskTitleSource, TitleError> {
    Ok(load_title_sources(host)?
        .sources
        .get(task_id.as_str())
        .map(|value| DesktopTaskTitleSource::parse(value))
        .unwrap_or(DesktopTaskTitleSource::Auto))
}

fn persist_title_update_success(
    host: &dyn TitleHost,
    job: &DesktopTitleUpdateJob,
    previous: &DesktopTaskTitleState,
    proposed: &str,
) -> Result<(), TitleError> {
    let sequence = host
        .timeline_events(&job.task.id, 200)?
        .into_iter()
        .map(|event| event.sequence)
        .max()
        .unwrap_or_default()
        .saturating_add(1);
    let request_id = job.turn_id.clone().unwrap_or_else(|| {
        format!(
            "{}-{}-{}",
            job.upper_bound().turn_seq,
            job.upper_bound().intra_turn_order,
            job.generation()
        )
    });
    let session = AgentSessionRef::new(format!("desktop-title-update:{}", job.task.id.as_str()))?;
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
    let applied =
        host.apply_projection(TimelineProjectionCommand::UpsertTimelineEvent { event })?;
    if applied != ProjectionApplyResult::DuplicateIgnored {
        host.emit_timeline_changed(job.task.id.clone(), Some(sequence));
    }
    Ok(())
}

fn persist_title_update_review(
    host: &dyn TitleHost,
    job: &DesktopTitleUpdateJob,
    review: &DesktopTitleUpdateReview,
) -> Result<(), TitleError> {
    let session =
        AgentSessionRef::new(format!("title-review-{}", review.request_id)).map_err(|error| {
            TitleError::InvalidPendingInteraction {
                request_id: review.request_id.clone(),
                message: error.to_string(),
            }
        })?;
    host.apply_projection(TimelineProjectionCommand::UpsertPending {
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

    let stale_reviews = host
        .pending_projections(&review.task.id)?
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
        host.apply_projection(TimelineProjectionCommand::ResolvePending {
            session_id: pending.agent_session.as_str().to_owned(),
            request_id: pending.request_id,
            status: PendingProjectionStatus::Stale,
            sequence,
            response: serde_json::json!({ "supersededBy": review.request_id }),
        })?;
    }
    host.emit_timeline_changed(review.task.id.clone(), None);
    Ok(())
}

fn load_title_sources(host: &dyn TitleHost) -> Result<StoredTitleSources, TitleError> {
    let path = host.prepare_title_source_store()?;
    let store = lilia_storage::SqliteAgentRuntimeStateStore::open(path).map_err(|error| {
        TitleError::InvalidInput {
            field: "title_source",
            message: error.to_string(),
        }
    })?;
    let value =
        store
            .setting(TITLE_SOURCE_SETTINGS_KEY)
            .map_err(|error| TitleError::InvalidInput {
                field: "title_source",
                message: error.to_string(),
            })?;
    let Some(value) = value else {
        return Ok(StoredTitleSources {
            schema_version: TITLE_SOURCE_SCHEMA_VERSION,
            sources: HashMap::new(),
        });
    };
    serde_json::from_value(value).map_err(|error| TitleError::InvalidInput {
        field: "title_source",
        message: error.to_string(),
    })
}

fn save_title_sources(host: &dyn TitleHost, stored: &StoredTitleSources) -> Result<(), TitleError> {
    let path = host.prepare_title_source_store()?;
    let store = lilia_storage::SqliteAgentRuntimeStateStore::open(path).map_err(|error| {
        TitleError::InvalidInput {
            field: "title_source",
            message: error.to_string(),
        }
    })?;
    let value = serde_json::to_value(stored).map_err(|error| TitleError::InvalidInput {
        field: "title_source",
        message: error.to_string(),
    })?;
    store
        .put_setting(TITLE_SOURCE_SETTINGS_KEY, &value)
        .map_err(|error| TitleError::InvalidInput {
            field: "title_source",
            message: error.to_string(),
        })
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

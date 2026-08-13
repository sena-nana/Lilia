//! Thin Tauri adapter for shared title-update coordinator.
//! Application owns scheduling/apply; Tauri resolves Assistant AI credentials and
//! optionally mirrors review decisions into the legacy timeline UI surface.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use lilia_contracts::TaskId;
use lilia_desktop_application::{
    normalize_title, title_event_id, title_system_instruction, DesktopApplication,
    DesktopApplicationError, DesktopTitleUpdateDecision, DesktopTitleUpdateJob,
    TITLE_UPDATE_ACTION_KIND,
};
use reqwest::blocking::Client;
use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Manager, Runtime};
use uuid::Uuid;

use crate::agent_timeline::{self, AgentTimelineEventInput};
use crate::chat::timeline_sink::persist_and_emit_input;
use crate::projects_tasks::events::emit_tasks_changed;
use crate::provider::{
    assistant_ai_secret, load_assistant_ai_config, load_model_feature_settings, AssistantAIConfig,
};
use crate::store::LiliaStore;

const TITLE_LABEL: &str = "标题已更新";
const TITLE_REVIEW_LABEL: &str = "建议更新标题";
const TITLE_SKIPPED_LABEL: &str = "标题更新已跳过";
const TITLE_UPDATE_CONCURRENCY: usize = 2;

fn title_update_lanes() -> Arc<tokio::sync::Semaphore> {
    static LANES: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    LANES
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(TITLE_UPDATE_CONCURRENCY)))
        .clone()
}

/// Spawn background auto-title generation for a finished turn.
pub(crate) fn spawn_title_update<R: Runtime>(
    app: AppHandle<R>,
    task_id: String,
    turn_id: Option<String>,
) {
    let Some(application) = app
        .try_state::<DesktopApplication>()
        .map(|state| state.inner().clone())
    else {
        eprintln!("[title-update] skipped: DesktopApplication unavailable");
        return;
    };
    let Ok(parsed_task_id) = TaskId::new(task_id.clone()) else {
        eprintln!("[title-update] skipped: invalid task id");
        return;
    };
    let job = match application.schedule_title_update(&parsed_task_id, turn_id.clone()) {
        Ok(Some(job)) => job,
        Ok(None) => return,
        Err(error) => {
            eprintln!("[title-update] skipped: {error}");
            return;
        }
    };
    let lanes = title_update_lanes();
    tauri::async_runtime::spawn(async move {
        let Ok(_permit) = lanes.acquire_owned().await else {
            return;
        };
        let result = tauri::async_runtime::spawn_blocking(move || {
            run_title_update(&app, &application, &job)
        })
        .await;
        match result {
            Ok(Err(error)) => eprintln!("[title-update] skipped: {error}"),
            Err(error) => eprintln!("[title-update] worker failed: {error}"),
            Ok(Ok(())) => {}
        }
    });
}

fn run_title_update<R: Runtime>(
    app: &AppHandle<R>,
    application: &DesktopApplication,
    job: &DesktopTitleUpdateJob,
) -> Result<(), String> {
    let coordinator = application.title_update_coordinator();
    if !coordinator.is_latest(job) {
        return Ok(());
    }
    let Some(prompt) = application
        .build_title_prompt_for_job(job)
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let model = assistant_ai_model_request(app)
        .ok_or_else(|| "assistant AI model unavailable for title".to_string())?;
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| format!("HTTP client failed: {error}"))?;
    let proposed = request_title(&client, &model, &prompt).and_then(normalize_title)?;
    let decision = application
        .apply_title_proposal(job, &proposed)
        .map_err(|error| error.to_string())?;
    record_title_decision(app, job, decision, &proposed);
    Ok(())
}

/// Accept or decline a previously proposed title-update interaction.
#[tauri::command]
pub fn chat_respond_title_update(
    app: AppHandle,
    task_id: String,
    request_id: String,
    decision: String,
) -> Result<(), String> {
    let application = app
        .try_state::<DesktopApplication>()
        .map(|state| state.inner().clone())
        .ok_or_else(|| "DesktopApplication unavailable".to_string())?;
    let parsed_task_id =
        TaskId::new(task_id.clone()).map_err(|error| format!("invalid task id: {error}"))?;
    let store = app.state::<LiliaStore>();
    let conn = store.conn()?;
    let event_id = title_event_id(&task_id, &request_id);
    let event = agent_timeline::list(&conn, &task_id)?
        .into_iter()
        .find(|event| event.id == event_id)
        .ok_or_else(|| "标题更新请求已失效".to_string())?;
    if event.kind != TITLE_UPDATE_ACTION_KIND || event.status != "requires_action" {
        return Ok(());
    }
    let payload = read_payload_record(&event.payload);
    let proposed = payload
        .get("proposedTitle")
        .and_then(|value| value.as_str())
        .and_then(|title| normalize_title(title.to_string()).ok())
        .ok_or_else(|| "标题更新请求缺少候选标题".to_string())?;
    let accepted = decision == "accept";
    match application.respond_title_update_review(&parsed_task_id, &request_id, accepted) {
        Ok(_) => {}
        Err(DesktopApplicationError::PendingInteractionNotFound { .. }) => {
            let _ = application
                .respond_title_update(&parsed_task_id, &proposed, accepted)
                .map_err(|error| error.to_string())?;
        }
        Err(error) => return Err(error.to_string()),
    }
    if accepted {
        emit_tasks_changed(&app, None);
    }

    let status = if accepted { "success" } else { "skipped" };
    let mut next_payload = payload;
    next_payload.insert("accepted".to_string(), JsonValue::Bool(accepted));
    next_payload.insert("decision".to_string(), JsonValue::String(decision));
    persist_and_emit_input(
        &app,
        AgentTimelineEventInput {
            id: Some(event_id),
            task_id,
            turn_id: event.turn_id,
            backend: event.backend,
            kind: TITLE_UPDATE_ACTION_KIND.to_string(),
            status: status.to_string(),
            title: title_event_label(status).to_string(),
            summary: Some(proposed),
            payload: JsonValue::Object(next_payload.into_iter().collect()),
            created_at: Some(event.created_at),
            updated_at: None,
        },
    );
    Ok(())
}

pub(crate) fn shutdown_title_update(app: &AppHandle) {
    if let Some(application) = app.try_state::<DesktopApplication>() {
        application.title_update_coordinator().shutdown();
    }
}

fn assistant_ai_model_request<R: Runtime>(app: &AppHandle<R>) -> Option<AssistantAIConfig> {
    let cfg = load_assistant_ai_config(app);
    let base_url = cfg
        .base_url
        .as_ref()?
        .trim()
        .trim_end_matches('/')
        .to_string();
    let api_key = assistant_ai_secret().ok().flatten()?;
    let model = load_model_feature_settings(app)
        .title
        .or(cfg.model.clone())?
        .trim()
        .to_string();
    if base_url.is_empty() || api_key.is_empty() || model.is_empty() {
        return None;
    }
    Some(AssistantAIConfig {
        base_url: Some(base_url),
        api_key: Some(api_key),
        model: Some(model),
        ..AssistantAIConfig::default()
    })
}

fn request_title(
    client: &Client,
    model: &AssistantAIConfig,
    prompt: &str,
) -> Result<String, String> {
    let base_url = model
        .base_url
        .as_deref()
        .ok_or_else(|| "assistant AI base_url missing".to_string())?;
    let api_key = model
        .api_key
        .as_deref()
        .ok_or_else(|| "assistant AI api_key missing".to_string())?;
    let model_name = model
        .model
        .as_deref()
        .ok_or_else(|| "assistant AI model missing".to_string())?;
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let response = client
        .post(url)
        .bearer_auth(api_key)
        .json(&json!({
            "model": model_name,
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

fn record_title_decision<R: Runtime>(
    app: &AppHandle<R>,
    job: &DesktopTitleUpdateJob,
    decision: DesktopTitleUpdateDecision,
    proposed: &str,
) {
    let (status, emit_review, request_id, proposed, previous_title) = match &decision {
        DesktopTitleUpdateDecision::Success(_) => (
            "success",
            false,
            Uuid::new_v4().to_string(),
            proposed.to_owned(),
            job.task.title.clone(),
        ),
        DesktopTitleUpdateDecision::RequiresAction(review) => (
            "requires_action",
            true,
            review.request_id.clone(),
            review.proposed_title.clone(),
            review.task.title.clone(),
        ),
        DesktopTitleUpdateDecision::Stale(_) => (
            "skipped",
            false,
            Uuid::new_v4().to_string(),
            proposed.to_owned(),
            job.task.title.clone(),
        ),
        DesktopTitleUpdateDecision::Unchanged | DesktopTitleUpdateDecision::Stopped => return,
    };
    if matches!(
        decision,
        DesktopTitleUpdateDecision::Success(_) | DesktopTitleUpdateDecision::RequiresAction(_)
    ) {
        emit_tasks_changed(app, job.task.project_id.clone());
    }
    if !emit_review && status != "success" {
        return;
    }
    let event_id = title_event_id(job.task.id.as_str(), &request_id);
    persist_and_emit_input(
        app,
        AgentTimelineEventInput {
            id: Some(event_id),
            task_id: job.task.id.as_str().to_owned(),
            turn_id: job.turn_id.clone(),
            backend: String::new(),
            kind: TITLE_UPDATE_ACTION_KIND.to_string(),
            status: status.to_string(),
            title: title_event_label(status).to_string(),
            summary: Some(proposed.clone()),
            payload: json!({
                "proposedTitle": proposed,
                "previousTitle": previous_title,
                "requestId": request_id,
            }),
            created_at: None,
            updated_at: None,
        },
    );
}

fn title_event_label(status: &str) -> &'static str {
    match status {
        "success" => TITLE_LABEL,
        "requires_action" => TITLE_REVIEW_LABEL,
        _ => TITLE_SKIPPED_LABEL,
    }
}

fn read_payload_record(payload: &JsonValue) -> serde_json::Map<String, JsonValue> {
    payload
        .as_object()
        .cloned()
        .unwrap_or_else(|| json!({}).as_object().cloned().unwrap_or_default())
}

//! Desktop Embedded Native AgentKit host wiring (#44 / #50 / #46 / #47).
//!
//! Execution backend is Native AgentKit only.
//!
//! Task timeline product facts come from AgentKit event projections in
//! `lilia-storage`. Desktop SQLite is a rebuildable UI cache, not an execution
//! fact source.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

use lilia_agent_integration::{
    NativeAgentKitRuntime, NativeAgentWireService, NativeRuntimeBootstrap, NativeTurnStreamPage,
    ProductCredentialImportInput, ProductCredentialLoginInput, SharedNativeAgentKitRuntime,
    TurnCancellationDisposition, PRODUCT_NATIVE_CODING_PROFILE_HINT,
};
use lilia_contracts::{
    AgentSessionRef, ProductApprovalDecision, TaskId, TimelineProjectionEvent,
    PRODUCT_TIMELINE_STORE_ID, TIMELINE_UI_CACHE_KIND,
};
use lilia_core::{AgentKitClientPort, NativeAgentCapabilitySnapshot};
use mutsuki_agent_contracts::{
    AgentEventEnvelope, AgentMessage, AgentSession, AgentWireError, AgentWireRequestEnvelope,
    AgentWireResponseEnvelope, CredentialRef, EditorContextSnapshot, EditorWorkspaceRef,
};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::agent_timeline::AgentTimelineEventInput;
use crate::chat::runner::{finish_agent_turn, RunnerInvocation, RunnerOutput};
use crate::chat::state::{
    finish_running_turn_handles, pause_native_running_turn, register_running_turn,
    remember_agent_session, session_key, ChatStore, NativeApprovalPause,
};
use crate::chat::timeline_sink::persist_and_emit_input;
use crate::chat::workflow::automation_run_id;
use crate::store::LiliaStore;
use crate::util::now_millis;

pub const BACKEND_NATIVE_AGENTKIT: &str = "native-agentkit";
const ENV_EXECUTION_BACKEND: &str = "LILIA_AGENT_EXECUTION_BACKEND";

/// Historical product cut-off label retained for host/status API honesty fields.
pub const LEGACY_NODE_RUNNER_COMPAT_UNTIL: &str = "1.0.0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionBackend {
    NativeAgentkit,
}

impl ExecutionBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeAgentkit => BACKEND_NATIVE_AGENTKIT,
        }
    }
}

/// Resolve Desktop execution backend. Always Native AgentKit.
///
/// `LILIA_AGENT_EXECUTION_BACKEND=node` is ignored (Node agent-runner removed).
pub fn resolve_execution_backend() -> ExecutionBackend {
    match std::env::var(ENV_EXECUTION_BACKEND)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "node" | "node-agent-runner" | "agent-runner" | "legacy" => {
            eprintln!(
                "[native-agent] {ENV_EXECUTION_BACKEND}=node ignored: Native AgentKit only \
                 (Node agent-runner removed)"
            );
            ExecutionBackend::NativeAgentkit
        }
        "" | "native" | "native-agentkit" | "agentkit" => ExecutionBackend::NativeAgentkit,
        other => {
            eprintln!(
                "[native-agent] unknown {ENV_EXECUTION_BACKEND}={other:?}; defaulting to native-agentkit"
            );
            ExecutionBackend::NativeAgentkit
        }
    }
}

/// Automation / multi-Agent new tasks must use AgentKit/Native only.
pub fn require_native_for_automation_or_multi_agent(context: &str) -> Result<(), String> {
    let _ = context;
    match resolve_execution_backend() {
        ExecutionBackend::NativeAgentkit => Ok(()),
    }
}

/// Honesty field: Node legacy-runner feature is permanently removed (always false).
pub const LEGACY_RUNNER_FEATURE_COMPILED: bool = false;

fn shared_runtime() -> Result<&'static SharedNativeAgentKitRuntime, String> {
    static RUNTIME: OnceLock<Result<SharedNativeAgentKitRuntime, String>> = OnceLock::new();
    match RUNTIME.get_or_init(|| {
        NativeRuntimeBootstrap::embedded_reference()
            .map_err(|err| err.to_string())
            .and_then(|bootstrap| {
                // Same path assembly as Service (#56): `$LILIA_HOME/db/product_projections.db`.
                let paths =
                    lilia_storage::LiliaDataPaths::from_home(crate::store::resolve_lilia_home());
                let _ = paths.ensure_layout();
                let store = lilia_storage::SqliteTimelineProjectionStore::open(
                    paths.product_projections_db(),
                )
                .map_err(|err| err.to_string())?;
                let runtime_state =
                    lilia_storage::SqliteAgentRuntimeStateStore::open(paths.agent_runtime_db())
                        .map_err(|err| err.to_string())?;
                let runtime = SharedNativeAgentKitRuntime::new(
                    bootstrap.into_runtime_with_stores(store, runtime_state),
                );
                runtime
                    .inner()
                    .apply_migrated_skill_roots(&paths)
                    .map_err(|err| err.to_string())?;
                Ok(runtime)
            })
    }) {
        Ok(runtime) => Ok(runtime),
        Err(err) => Err(err.clone()),
    }
}

pub fn native_runtime() -> Result<&'static NativeAgentKitRuntime, String> {
    Ok(shared_runtime()?.inner())
}

pub fn cancel_product_agent_turn<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
    turn_id: &str,
) -> Result<TurnCancellationDisposition, String> {
    let task = TaskId::new(task_id.to_string()).map_err(|error| error.to_string())?;
    let bound_session = app
        .try_state::<crate::product_core::EmbeddedProductCore>()
        .and_then(|core| core.binding_for_task(&task).ok().flatten())
        .map(|binding| binding.agent_session.as_str().to_string());
    let session_id = bound_session.or_else(|| {
        let store = app.try_state::<ChatStore>()?;
        let session = store
            .sdk_sessions
            .lock()
            .ok()?
            .get(&session_key(BACKEND_NATIVE_AGENTKIT, task_id))
            .cloned();
        session
    });
    let session_id = session_id.ok_or_else(|| {
        format!("Native AgentKit session binding missing for running task `{task_id}`")
    })?;
    let runtime = native_runtime()?;
    let disposition = runtime
        .cancel_session_turn(&session_id, turn_id)
        .map_err(|error| error.to_string())?;
    if let Some(cancelled) = runtime
        .product_timeline_for_task(&task)
        .into_iter()
        .filter(|event| {
            event.turn_id.as_deref() == Some(turn_id)
                && event
                    .payload
                    .get("turnStatus")
                    .and_then(serde_json::Value::as_str)
                    == Some("cancelled")
        })
        .max_by_key(|event| event.sequence)
    {
        mirror_product_timeline_to_ui_cache(app, std::slice::from_ref(&cancelled))?;
        emit_native_stream_event(
            app,
            task_id,
            turn_id,
            Some(cancelled.sequence),
            1,
            true,
            None,
        );
    }
    Ok(disposition)
}

pub(crate) fn finish_cancelled_native_approval_turn<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: &str,
    turn: crate::chat::state::RunningTurn,
) {
    let Some(pause) = turn.native_approval_pause else {
        return;
    };
    let store = app_handle.state::<ChatStore>();
    let _ = finish_running_turn_handles(&store, task_id, &turn.turn_id, BACKEND_NATIVE_AGENTKIT);
    crate::automation::automation_complete_agent_turn(
        app_handle,
        &store,
        pause.automation_run_id,
        &turn.turn_id,
        false,
    );
    finish_agent_turn(
        app_handle.clone(),
        task_id.to_string(),
        BACKEND_NATIVE_AGENTKIT.to_string(),
        pause.last_session_id,
        false,
        None,
    );
}

fn agent_wire_service() -> Result<&'static Mutex<NativeAgentWireService>, AgentWireError> {
    static SERVICE: OnceLock<Result<Mutex<NativeAgentWireService>, AgentWireError>> =
        OnceLock::new();
    let runtime = shared_runtime().map_err(|message| AgentWireError {
        code: "agent.runtime.unavailable".into(),
        message,
        retryable: true,
    })?;
    match SERVICE.get_or_init(|| NativeAgentWireService::try_new(runtime.clone()).map(Mutex::new)) {
        Ok(service) => Ok(service),
        Err(error) => Err(error.clone()),
    }
}

/// Dispatch Agent Wire through the same process-wide runtime used by Desktop turns.
///
/// Remote Control calls this only after its pairing/trust envelope has been
/// authorized; this function intentionally does not expose a separate network
/// listener that could bypass that boundary.
pub fn dispatch_agent_wire(
    request: AgentWireRequestEnvelope,
) -> Result<AgentWireResponseEnvelope, AgentWireError> {
    let mut service = agent_wire_service()?.lock().map_err(|_| AgentWireError {
        code: "agent.runtime.lock_poisoned".into(),
        message: "Desktop Agent Wire service lock is poisoned".into(),
        retryable: true,
    })?;
    mutsuki_agent_client::dispatch_agent_request(&mut *service, request)
}

fn open_product_wire_session<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: &TaskId,
    profile_id: &str,
    fallback_session_id: Option<&str>,
) -> Result<AgentSession, String> {
    let product_binding = app_handle
        .try_state::<crate::product_core::EmbeddedProductCore>()
        .and_then(|core| core.binding_for_task(task_id).ok().flatten());
    let requested_session_id = product_binding
        .as_ref()
        .map(|binding| binding.agent_session.as_str())
        .or(fallback_session_id);
    let session = agent_wire_service()
        .map_err(|error| format!("{}: {}", error.code, error.message))?
        .lock()
        .map_err(|_| "Desktop Agent Wire service lock is poisoned".to_string())?
        .open_task_session(task_id, requested_session_id, profile_id, None)
        .map_err(|error| format!("{}: {}", error.code, error.message))?;
    let session_ref =
        AgentSessionRef::new(session.session_id.clone()).map_err(|error| error.to_string())?;
    if let Some(core) = app_handle.try_state::<crate::product_core::EmbeddedProductCore>() {
        core.persist_agent_session_binding(task_id, &session_ref, Some(profile_id.to_string()))
            .map_err(|error| error.to_string())?;
    }
    Ok(session)
}

/// Product projection used by authenticated Remote Control before it submits
/// canonical Agent Wire requests for a task.
pub fn open_product_agent_wire_session<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: &str,
) -> Result<AgentSession, String> {
    let task_id = TaskId::new(task_id.to_string()).map_err(|error| error.to_string())?;
    let profile = native_runtime()?
        .refresh_product_profile(None)
        .map_err(|error| error.to_string())?;
    open_product_wire_session(app_handle, &task_id, &profile.profile_id, None)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAgentHostStatus {
    pub wired: bool,
    pub default_backend: &'static str,
    pub active_backend: &'static str,
    pub profile_hint: &'static str,
    pub capabilities: NativeAgentCapabilitySnapshot,
    pub env_override: Option<String>,
    /// Credential health is independent from Runtime readiness (#50 / #121).
    pub diagnostics: Option<lilia_agent_integration::IndependentDiagnostics>,
    /// Honest: timeline rows from Native path are AgentKit event projections.
    pub timeline_is_agentkit_projection: bool,
    /// Product timeline fact surface id (`lilia-storage`), not Desktop SQLite.
    pub product_timeline_store: &'static str,
    /// Desktop SQLite rows are UI cache only and rebuildable from product projection.
    pub desktop_sqlite_is_ui_cache_only: bool,
    /// #47 — Node runner is opt-in legacy compatibility, never default.
    pub node_runner_legacy_compatibility: bool,
    pub node_runner_compat_until: &'static str,
    /// #47 honesty: default NSIS/resources must not ship Codex app-server.
    pub default_bundle_includes_official_agent_server: bool,
    /// #47 honesty: default NSIS/resources must not ship Node `agent-runner.mjs`.
    pub default_bundle_includes_node_agent_runner: bool,
    /// Always false: Node legacy-runner feature permanently removed.
    pub legacy_runner_feature_compiled: bool,
    /// Honest: true when product profile binds openai-compatible or anthropic-messages
    /// CredentialRef and turns are driven by protocol HTTP Model Adapter (not reference-only).
    pub live_model_adapter_drives_turn: bool,
}

pub fn host_status() -> NativeAgentHostStatus {
    let active = resolve_execution_backend();
    let runtime = native_runtime().ok();
    let capabilities = runtime
        .map(|rt| rt.capabilities())
        .transpose()
        .ok()
        .flatten()
        .unwrap_or(NativeAgentCapabilitySnapshot {
            backend: "unavailable".into(),
            bundle_id: String::new(),
            official_agent_server: false,
            node_runner_default: false,
            supports_session: false,
            supports_stream: false,
            supports_approval: false,
            supports_cancel: false,
            supports_resume: false,
            profile_id: None,
        });
    let diagnostics = runtime.map(|rt| rt.independent_diagnostics());
    let live_model_adapter_drives_turn = diagnostics
        .as_ref()
        .map(|d| d.live_model_adapter_drives_turn)
        .unwrap_or(false);
    let wired = runtime.is_some();
    NativeAgentHostStatus {
        wired,
        default_backend: BACKEND_NATIVE_AGENTKIT,
        active_backend: active.as_str(),
        profile_hint: PRODUCT_NATIVE_CODING_PROFILE_HINT,
        capabilities,
        env_override: std::env::var(ENV_EXECUTION_BACKEND).ok(),
        diagnostics,
        timeline_is_agentkit_projection: true,
        product_timeline_store: PRODUCT_TIMELINE_STORE_ID,
        desktop_sqlite_is_ui_cache_only: true,
        // Node runner feature removed; flag kept false for honesty API stability.
        node_runner_legacy_compatibility: false,
        node_runner_compat_until: LEGACY_NODE_RUNNER_COMPAT_UNTIL,
        default_bundle_includes_official_agent_server: false,
        default_bundle_includes_node_agent_runner: false,
        legacy_runner_feature_compiled: LEGACY_RUNNER_FEATURE_COMPILED,
        live_model_adapter_drives_turn,
    }
}

#[tauri::command]
pub fn native_agent_host_status() -> NativeAgentHostStatus {
    host_status()
}

#[tauri::command]
pub fn native_credential_providers() -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    serde_json::to_value(runtime.credentials().providers()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn native_credential_login(
    input: ProductCredentialLoginInput,
) -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    let view = runtime
        .credentials()
        .login(input)
        .map_err(|e| e.to_string())?;
    let _ = runtime.refresh_product_profile(None);
    serde_json::to_value(view).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn native_credential_import(
    input: ProductCredentialImportInput,
) -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    let view = runtime
        .credentials()
        .import_generated_api_key(input)
        .map_err(|e| e.to_string())?;
    let _ = runtime.refresh_product_profile(None);
    serde_json::to_value(view).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn native_credential_revoke(
    credential_id: String,
    revision: u64,
    reason: Option<String>,
) -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    let view = runtime
        .credentials()
        .revoke(
            CredentialRef {
                credential_id,
                revision,
            },
            reason,
        )
        .map_err(|e| e.to_string())?;
    let _ = runtime.refresh_product_profile(None);
    serde_json::to_value(view).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn native_credential_diagnostics() -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    serde_json::to_value(runtime.independent_diagnostics()).map_err(|e| e.to_string())
}

/// Credential Broker quota / limits honesty surface (#50).
/// Remote Provider quota API is unavailable — response never invents remaining-quota numbers.
#[tauri::command]
pub fn native_quota_surface() -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    serde_json::to_value(runtime.native_quota_surface()).map_err(|e| e.to_string())
}

/// Feed product approval back into Native runtime and continue/deny the tool.
#[tauri::command]
pub fn native_respond_approval<R: Runtime>(
    app_handle: AppHandle<R>,
    task_id: String,
    decision: ProductApprovalDecision,
) -> Result<serde_json::Value, String> {
    let expected_task = native_runtime()?
        .task_for_session(&decision.session_id)
        .map_err(|error| error.to_string())?;
    if expected_task.as_str() != task_id {
        return Err(format!(
            "Native approval session `{}` belongs to task `{}`, not `{task_id}`",
            decision.session_id,
            expected_task.as_str(),
        ));
    }
    let streamed_sequences = Arc::new(Mutex::new(HashSet::new()));
    let observed_sequences = streamed_sequences.clone();
    let stream_app = app_handle.clone();
    let stream_task_id = task_id.clone();
    let stream_turn_id = decision.turn_id.clone();
    let page = respond_product_agent_approval_observed(decision, move |events| {
        if mirror_agent_events_to_ui_cache(&stream_app, &stream_task_id, events).is_ok() {
            if let Ok(mut sequences) = observed_sequences.lock() {
                sequences.extend(events.iter().map(|event| event.sequence));
            }
        }
        emit_native_stream_event(
            &stream_app,
            &stream_task_id,
            &stream_turn_id,
            events.last().map(|event| event.sequence),
            events.len(),
            false,
            None,
        );
    })?;
    mirror_stream_page_to_ui_cache(&app_handle, &task_id, &page, streamed_sequences.as_ref())?;
    emit_native_stream_event(
        &app_handle,
        &task_id,
        &page.turn_id,
        Some(page.next_sequence),
        page.events.len(),
        !page.waiting_approval,
        Some(&page),
    );
    if !page.waiting_approval {
        finish_native_approval_turn(&app_handle, &task_id, &page)?;
    }
    serde_json::to_value(page).map_err(|e| e.to_string())
}

pub fn respond_product_agent_approval(
    decision: ProductApprovalDecision,
) -> Result<NativeTurnStreamPage, String> {
    Ok(agent_wire_service()
        .map_err(|error| format!("{}: {}", error.code, error.message))?
        .lock()
        .map_err(|_| "Desktop Agent Wire service lock is poisoned".to_string())?
        .respond_task_approval(decision)
        .map_err(|error| format!("{}: {}", error.code, error.message))?
        .page)
}

fn respond_product_agent_approval_observed<O>(
    decision: ProductApprovalDecision,
    observer: O,
) -> Result<NativeTurnStreamPage, String>
where
    O: Fn(&[AgentEventEnvelope]) + Send + Sync + 'static,
{
    Ok(agent_wire_service()
        .map_err(|error| format!("{}: {}", error.code, error.message))?
        .lock()
        .map_err(|_| "Desktop Agent Wire service lock is poisoned".to_string())?
        .respond_task_approval_observed(decision, observer)
        .map_err(|error| format!("{}: {}", error.code, error.message))?
        .page)
}

pub fn list_product_pending_for_task(
    task_id: &str,
) -> Result<Vec<lilia_contracts::PendingProjection>, String> {
    let task_id = TaskId::new(task_id.to_string()).map_err(|error| error.to_string())?;
    Ok(native_runtime()?.product_pending_for_task(&task_id))
}

/// Default product timeline read surface from `lilia-storage` (not Desktop SQLite).
#[tauri::command]
pub fn native_product_timeline(task_id: String) -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    let task = TaskId::new(task_id).map_err(|err| err.to_string())?;
    serde_json::to_value(runtime.product_timeline_for_task(&task)).map_err(|e| e.to_string())
}

/// Product Artifact / Todo / Pending projection queries (#46 / #56).
#[tauri::command]
pub fn native_product_artifacts(task_id: String) -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    let task = TaskId::new(task_id).map_err(|err| err.to_string())?;
    serde_json::to_value(runtime.product_artifacts_for_task(&task)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn native_product_todos(task_id: String) -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    let task = TaskId::new(task_id).map_err(|err| err.to_string())?;
    serde_json::to_value(runtime.product_todos_for_task(&task)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn native_product_pending(task_id: String) -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    let task = TaskId::new(task_id).map_err(|err| err.to_string())?;
    serde_json::to_value(runtime.product_pending_for_task(&task)).map_err(|e| e.to_string())
}

/// Convert product timeline rows into Desktop UI timeline events.
pub fn product_timeline_as_ui_events(
    events: &[TimelineProjectionEvent],
) -> Vec<crate::agent_timeline::AgentTimelineEvent> {
    let now = now_millis() as i64;
    events
        .iter()
        .enumerate()
        .map(|(index, event)| crate::agent_timeline::AgentTimelineEvent {
            id: event.id.as_str().to_string(),
            task_id: event.task_id.as_str().to_string(),
            turn_id: event.turn_id.clone(),
            backend: BACKEND_NATIVE_AGENTKIT.to_string(),
            kind: event.kind.clone(),
            status: event.status.clone(),
            title: event.title.clone(),
            summary: event.summary.clone(),
            payload: annotate_projection_payload(event, ProjectionPayloadKind::ProductRead),
            created_at: now,
            updated_at: now,
            turn_seq: event.sequence as i64,
            intra_turn_order: index as i64,
        })
        .collect()
}

/// Default Desktop timeline list: prefer product projection over UI SQLite cache.
pub fn list_default_timeline_for_task(
    task_id: &str,
) -> Result<Option<Vec<crate::agent_timeline::AgentTimelineEvent>>, String> {
    let Ok(runtime) = native_runtime() else {
        return Ok(None);
    };
    let Ok(task) = TaskId::new(task_id) else {
        return Ok(None);
    };
    let projected = runtime.product_timeline_for_task(&task);
    if projected.is_empty() {
        return Ok(None);
    }
    Ok(Some(product_timeline_as_ui_events(&projected)))
}

pub fn latest_product_timeline_page(
    task_id: &str,
    limit: usize,
) -> Result<Option<crate::agent_timeline::TimelinePage>, String> {
    let Some(events) = list_default_timeline_for_task(task_id)? else {
        return Ok(None);
    };
    let start = events.len().saturating_sub(limit);
    let page_events = events[start..].to_vec();
    Ok(Some(product_timeline_page(page_events, start > 0, false)))
}

pub fn product_timeline_page_before(
    task_id: &str,
    cursor: &str,
    limit: usize,
) -> Result<Option<crate::agent_timeline::TimelinePage>, String> {
    let Some(events) = list_default_timeline_for_task(task_id)? else {
        return Ok(None);
    };
    let cursor_index = events
        .iter()
        .position(|event| event.id == cursor)
        .ok_or_else(|| "product timeline cursor event was not found".to_string())?;
    let start = cursor_index.saturating_sub(limit);
    Ok(Some(product_timeline_page(
        events[start..cursor_index].to_vec(),
        start > 0,
        cursor_index < events.len(),
    )))
}

pub fn product_timeline_after(
    task_id: &str,
    cursor: &str,
    limit: Option<usize>,
) -> Result<Option<Vec<crate::agent_timeline::AgentTimelineEvent>>, String> {
    let Some(events) = list_default_timeline_for_task(task_id)? else {
        return Ok(None);
    };
    let start = events
        .iter()
        .position(|event| event.id == cursor)
        .map(|index| index.saturating_add(1))
        .unwrap_or(0);
    let iter = events.into_iter().skip(start);
    Ok(Some(match limit {
        Some(limit) => iter.take(limit).collect(),
        None => iter.collect(),
    }))
}

fn product_timeline_page(
    events: Vec<crate::agent_timeline::AgentTimelineEvent>,
    has_more_before: bool,
    has_more_after: bool,
) -> crate::agent_timeline::TimelinePage {
    crate::agent_timeline::TimelinePage {
        before_cursor: events.first().map(|event| event.id.clone()),
        after_cursor: events.last().map(|event| event.id.clone()),
        events,
        has_more_before,
        has_more_after,
    }
}

/// Rebuild product timeline projection for one Agent session from stored envelopes.
#[tauri::command]
pub fn native_rebuild_product_timeline(session_id: String) -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    let session =
        lilia_contracts::AgentSessionRef::new(session_id).map_err(|err| err.to_string())?;
    let inserted = runtime
        .rebuild_product_timeline_for_session(&session)
        .map_err(|err| err.to_string())?;
    Ok(json!({
        "sessionId": session.as_str(),
        "inserted": inserted,
        "store": PRODUCT_TIMELINE_STORE_ID,
        "source": "agentkit-event-replay",
    }))
}

/// Clear Desktop SQLite UI cache for a task and rebuild it from product projection.
#[tauri::command]
pub fn native_rebuild_ui_timeline_cache<R: Runtime>(
    app_handle: AppHandle<R>,
    task_id: String,
) -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    let task = TaskId::new(task_id.clone()).map_err(|err| err.to_string())?;
    let projected = runtime.product_timeline_for_task(&task);

    if let Some(lilia_store) = app_handle.try_state::<LiliaStore>() {
        let conn = lilia_store.conn()?;
        crate::agent_timeline::clear(&conn, &task_id)?;
    }

    let mirrored = mirror_product_timeline_to_ui_cache(&app_handle, &projected)?;
    // Rebuild product todo checklist into Desktop task_todos when present.
    if let Err(err) =
        crate::native_projection_hooks::mirror_product_todos_for_task(&app_handle, &task_id)
    {
        eprintln!("[native-agent] rebuild todo mirror skipped: {err}");
    }
    Ok(json!({
        "taskId": task_id,
        "productRows": projected.len(),
        "uiCacheMirrored": mirrored,
        "productStore": PRODUCT_TIMELINE_STORE_ID,
        "uiCacheKind": TIMELINE_UI_CACHE_KIND,
        "desktopSqliteIsFactSource": false,
    }))
}

/// Desktop default turn path through Native AgentKit Client (no Node runner).
pub fn run_native_agent_turn<R: Runtime>(
    app_handle: &AppHandle<R>,
    invocation: RunnerInvocation,
) -> Result<RunnerOutput, String> {
    let runtime = native_runtime()?;
    let task_id = TaskId::new(invocation.task_id.clone()).map_err(|err| err.to_string())?;
    let workflow_kind = invocation
        .workflow
        .as_ref()
        .and_then(|workflow| serde_json::to_value(workflow).ok())
        .and_then(|value| {
            value
                .get("type")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
    let profile = runtime
        .refresh_product_profile(workflow_kind.as_deref())
        .map_err(|err| err.to_string())?;
    let session = open_product_wire_session(
        app_handle,
        &task_id,
        &profile.profile_id,
        invocation.resume_session_id.as_deref(),
    )?;

    {
        let store = app_handle.state::<ChatStore>();
        if let Some(lilia_store) = app_handle.try_state::<LiliaStore>() {
            if let Ok(conn) = lilia_store.conn() {
                remember_agent_session(
                    &conn,
                    &store,
                    &invocation.task_id,
                    BACKEND_NATIVE_AGENTKIT,
                    &session.session_id,
                    "native-agentkit",
                );
            }
        } else {
            store.sdk_sessions.lock().unwrap().insert(
                crate::chat::state::session_key(BACKEND_NATIVE_AGENTKIT, &invocation.task_id),
                session.session_id.clone(),
            );
        }
        register_running_turn(
            &store,
            invocation.task_id.clone(),
            invocation.turn_id.clone(),
            BACKEND_NATIVE_AGENTKIT,
        );
    }

    let context = native_turn_context(app_handle, &invocation);
    let mut message = AgentMessage::user(&invocation.content);
    message.metadata = Some(context);
    let stream_app = app_handle.clone();
    let stream_task_id = invocation.task_id.clone();
    let stream_turn_id = invocation.turn_id.clone();
    let streamed_sequences = Arc::new(Mutex::new(HashSet::new()));
    let observed_sequences = streamed_sequences.clone();
    let mut service = agent_wire_service()
        .map_err(|error| format!("{}: {}", error.code, error.message))?
        .lock()
        .map_err(|_| "Desktop Agent Wire service lock is poisoned".to_string())?;
    let page = service
        .submit_task_turn_observed(
            &session.session_id,
            &invocation.turn_id,
            vec![message],
            &format!("desktop:{}:{}", invocation.task_id, invocation.turn_id),
            move |events| {
                if mirror_agent_events_to_ui_cache(&stream_app, &stream_task_id, events).is_ok() {
                    if let Ok(mut sequences) = observed_sequences.lock() {
                        sequences.extend(events.iter().map(|event| event.sequence));
                    }
                }
                let _ = stream_app.emit(
                    crate::native_agent_contract::stream_event_name(),
                    serde_json::json!({
                        "taskId": stream_task_id.clone(),
                        "turnId": stream_turn_id.clone(),
                        "eventCount": events.len(),
                        "nextSequence": events.last().map(|event| event.sequence),
                        "timelineIsProjection": true,
                        "productTimelineStore": PRODUCT_TIMELINE_STORE_ID,
                        "desktopSqliteIsUiCacheOnly": true,
                        "terminal": false,
                    }),
                );
            },
        )
        .map_err(|error| format!("{}: {}", error.code, error.message))?
        .page;

    let waiting_approval = page.waiting_approval
        && pause_native_running_turn(
            &app_handle.state::<ChatStore>(),
            &invocation.task_id,
            &invocation.turn_id,
            automation_run_id(invocation.workflow.as_ref()),
            Some(page.session_id.clone()),
        );
    drop(service);

    // Runtime already applied projection commands to `lilia-storage`.
    // Desktop SQLite only mirrors the product surface as UI cache.
    mirror_stream_page_to_ui_cache(
        app_handle,
        &invocation.task_id,
        &page,
        streamed_sequences.as_ref(),
    )?;

    emit_native_stream_event(
        app_handle,
        &invocation.task_id,
        &invocation.turn_id,
        Some(page.next_sequence),
        page.events.len(),
        !waiting_approval,
        Some(&page),
    );

    if !waiting_approval {
        crate::chat::title_update::spawn_title_update(
            app_handle.clone(),
            invocation.task_id.clone(),
            Some(invocation.turn_id.clone()),
        );
    }

    Ok(RunnerOutput {
        last_session_id: Some(page.session_id.clone()),
        interrupted: false,
        reset: false,
        waiting_approval,
        terminal_failed: !waiting_approval && !page.completed,
    })
}

fn emit_native_stream_event<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: &str,
    turn_id: &str,
    next_sequence: Option<u64>,
    event_count: usize,
    terminal: bool,
    page: Option<&NativeTurnStreamPage>,
) {
    let _ = app_handle.emit(
        crate::native_agent_contract::stream_event_name(),
        serde_json::json!({
            "taskId": task_id,
            "turnId": turn_id,
            "sessionId": page.map(|page| page.session_id.as_str()),
            "eventCount": event_count,
            "nextSequence": next_sequence,
            "officialAgentServer": page.map(|page| page.official_agent_server),
            "credentialBound": page.map(|page| page.credential_bound),
            "liveModelAdapterDrivesTurn": page.map(|page| page.live_model_adapter_drives_turn),
            "profileId": page.map(|page| page.profile_id.as_str()),
            "toolSummary": page.and_then(|page| page.tool_summary.as_ref()),
            "waitingApproval": page.map(|page| page.waiting_approval),
            "completed": page.map(|page| page.completed),
            "timelineIsProjection": true,
            "productTimelineStore": PRODUCT_TIMELINE_STORE_ID,
            "desktopSqliteIsUiCacheOnly": true,
            "terminal": terminal,
        }),
    );
}

fn finish_native_approval_turn<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: &str,
    page: &NativeTurnStreamPage,
) -> Result<(), String> {
    let store = app_handle.state::<ChatStore>();
    let running_turn = store
        .running_turns
        .lock()
        .unwrap()
        .get(task_id)
        .filter(|turn| turn.turn_id == page.turn_id && turn.backend == BACKEND_NATIVE_AGENTKIT)
        .cloned();
    if running_turn
        .as_ref()
        .is_some_and(|turn| turn.native_approval_pause.is_none())
    {
        return Err(format!(
            "Native turn `{}` is still running and is not paused for approval",
            page.turn_id
        ));
    }
    let pause = running_turn.and_then(|turn| turn.native_approval_pause);
    let (automation_run_id, last_session_id, interrupted, reset) = match pause {
        Some(NativeApprovalPause {
            automation_run_id,
            last_session_id,
        }) => {
            let finished = finish_running_turn_handles(
                &store,
                task_id,
                &page.turn_id,
                BACKEND_NATIVE_AGENTKIT,
            );
            (
                automation_run_id,
                last_session_id,
                finished.interrupted,
                finished.reset,
            )
        }
        None => (
            crate::automation::automation_run_id_for_waiting_turn(app_handle, &page.turn_id)?,
            None,
            false,
            false,
        ),
    };
    let success = page.completed && !interrupted && !reset;
    crate::automation::automation_complete_agent_turn(
        app_handle,
        &store,
        automation_run_id,
        &page.turn_id,
        success,
    );
    finish_agent_turn(
        app_handle.clone(),
        task_id.to_string(),
        BACKEND_NATIVE_AGENTKIT.to_string(),
        last_session_id.or_else(|| Some(page.session_id.clone())),
        success,
        None,
    );
    if !interrupted && !reset {
        crate::chat::title_update::spawn_title_update(
            app_handle.clone(),
            task_id.to_string(),
            Some(page.turn_id.clone()),
        );
    }
    Ok(())
}

fn native_turn_context<R: Runtime>(
    app_handle: &AppHandle<R>,
    invocation: &RunnerInvocation,
) -> serde_json::Value {
    let generation = u64::try_from(now_millis()).unwrap_or_default();
    let workspace = EditorWorkspaceRef {
        workspace_id: format!("lilia.task:{}", invocation.task_id),
        folders: (!invocation.project_cwd.trim().is_empty())
            .then(|| invocation.project_cwd.clone())
            .into_iter()
            .collect(),
        metadata: json!({
            "productTaskId": invocation.task_id,
            "source": "lilia-desktop",
        }),
    };
    let editor_context = EditorContextSnapshot {
        snapshot_id: format!("lilia:{}:{}", invocation.task_id, invocation.turn_id),
        workspace: workspace.clone(),
        generation,
        active_document: None,
        documents: Vec::new(),
        supports_workspace_edit_preview: false,
        supports_workspace_edit_apply: false,
    };
    json!({
        "workspace": workspace,
        "editorContext": editor_context,
        "model": invocation.composer.model,
        "modelSelectionMode": invocation.composer.model_selection_mode,
        "reasoningEffort": invocation.composer.reasoning_effort,
        "permission": invocation.composer.permission,
        "planMode": invocation.composer.plan_mode,
        "goalMode": invocation.composer.goal_mode,
        "attachments": invocation.attachments,
        "conversationReferences": invocation.conversation_references,
        "conversationContext": crate::chat::runner::build_runner_conversation_context(
            app_handle,
            &invocation.task_id,
        ),
        "workflow": invocation.workflow,
        "runtimeCommand": invocation.runtime_command,
        "runtimeOptions": invocation.runtime_options,
        "resumeSessionId": invocation.resume_session_id,
        "queuedCount": invocation.queued_count,
    })
}

fn mirror_stream_page_to_ui_cache<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: &str,
    page: &NativeTurnStreamPage,
    streamed_sequences: &Mutex<HashSet<u64>>,
) -> Result<(), String> {
    let streamed = streamed_sequences
        .lock()
        .map_err(|_| "Native AgentKit streamed sequence lock is poisoned".to_string())?;
    let remaining = page
        .events
        .iter()
        .filter(|event| !streamed.contains(&event.sequence))
        .cloned()
        .collect::<Vec<_>>();
    drop(streamed);
    mirror_agent_events_to_ui_cache(app_handle, task_id, &remaining)
}

fn mirror_agent_events_to_ui_cache<R: Runtime>(
    app_handle: &AppHandle<R>,
    task_id: &str,
    events: &[AgentEventEnvelope],
) -> Result<(), String> {
    // Product-side effects (todos / context ring) consume raw AgentKit envelopes.
    crate::native_projection_hooks::apply_projection_side_effects(app_handle, task_id, events);

    let runtime = native_runtime()?;
    let projected_task_id = TaskId::new(task_id.to_string()).map_err(|e| e.to_string())?;
    let sequences: HashSet<u64> = events.iter().map(|event| event.sequence).collect();
    let projected = runtime
        .product_timeline_for_task(&projected_task_id)
        .into_iter()
        .filter(|event| sequences.contains(&event.sequence))
        .collect::<Vec<_>>();
    let _ = mirror_product_timeline_to_ui_cache(app_handle, &projected)?;
    Ok(())
}

fn mirror_product_timeline_to_ui_cache<R: Runtime>(
    app_handle: &AppHandle<R>,
    events: &[TimelineProjectionEvent],
) -> Result<usize, String> {
    let now = now_millis() as i64;
    let mut mirrored = 0usize;
    for event in events {
        let input = AgentTimelineEventInput {
            id: Some(event.id.as_str().to_string()),
            task_id: event.task_id.as_str().to_string(),
            turn_id: event.turn_id.clone(),
            backend: BACKEND_NATIVE_AGENTKIT.to_string(),
            kind: event.kind.clone(),
            status: event.status.clone(),
            title: event.title.clone(),
            summary: event.summary.clone(),
            payload: annotate_projection_payload(event, ProjectionPayloadKind::UiCache),
            created_at: Some(now),
            updated_at: Some(now),
        };
        persist_and_emit_input(app_handle, input);
        mirrored += 1;
    }
    Ok(mirrored)
}

#[derive(Clone, Copy)]
enum ProjectionPayloadKind {
    ProductRead,
    UiCache,
}

fn annotate_projection_payload(
    event: &TimelineProjectionEvent,
    kind: ProjectionPayloadKind,
) -> serde_json::Value {
    let mut payload = event.payload.clone();
    if let Some(obj) = payload.as_object_mut() {
        match kind {
            ProjectionPayloadKind::ProductRead => {
                obj.insert("uiCache".into(), json!(false));
                obj.insert("readFromProductProjection".into(), json!(true));
            }
            ProjectionPayloadKind::UiCache => {
                obj.insert("uiCache".into(), json!(true));
                obj.insert("uiCacheKind".into(), json!(TIMELINE_UI_CACHE_KIND));
            }
        }
        obj.insert(
            "productProjectionStore".into(),
            json!(PRODUCT_TIMELINE_STORE_ID),
        );
        obj.insert("notExecutionFactSource".into(), json!(true));
        obj.insert("projected".into(), json!(true));
        obj.insert("sequence".into(), json!(event.sequence));
        obj.insert("agentSessionId".into(), json!(event.agent_session.as_str()));
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use lilia_contracts::{AgentSessionRef, ProjectionEventId};

    #[test]
    fn product_timeline_ui_events_are_not_desktop_sqlite_fact_source() {
        let event = TimelineProjectionEvent {
            id: ProjectionEventId::from_session_sequence("sess-ui", 1),
            task_id: TaskId::new("task-ui").unwrap(),
            agent_session: AgentSessionRef::new("sess-ui").unwrap(),
            sequence: 1,
            turn_id: Some("turn-1".into()),
            kind: "message".into(),
            status: "success".into(),
            title: "hi".into(),
            summary: Some("body".into()),
            payload: json!({ "role": "assistant" }),
            projected: true,
        };
        let ui = product_timeline_as_ui_events(&[event]);
        assert_eq!(ui.len(), 1);
        let payload = ui[0].payload.as_object().expect("payload object");
        assert_eq!(payload.get("uiCache"), Some(&json!(false)));
        assert_eq!(payload.get("notExecutionFactSource"), Some(&json!(true)));
        assert_eq!(payload.get("readFromProductProjection"), Some(&json!(true)));
        assert_eq!(
            payload.get("productProjectionStore"),
            Some(&json!(PRODUCT_TIMELINE_STORE_ID))
        );
        assert_eq!(payload.get("projected"), Some(&json!(true)));
        assert_eq!(ui[0].backend, BACKEND_NATIVE_AGENTKIT);
    }

    #[test]
    fn default_execution_backend_is_native_and_node_env_requires_feature() {
        let previous = std::env::var(ENV_EXECUTION_BACKEND).ok();
        std::env::remove_var(ENV_EXECUTION_BACKEND);
        assert_eq!(
            resolve_execution_backend(),
            ExecutionBackend::NativeAgentkit
        );
        assert!(require_native_for_automation_or_multi_agent("test").is_ok());
        std::env::set_var(ENV_EXECUTION_BACKEND, "node");
        // Default build: Node env is ignored; binary does not link Node path.
        assert_eq!(
            resolve_execution_backend(),
            ExecutionBackend::NativeAgentkit
        );
        assert!(!LEGACY_RUNNER_FEATURE_COMPILED);
        assert!(require_native_for_automation_or_multi_agent("Automation Agent 节点").is_ok());
        std::env::set_var(ENV_EXECUTION_BACKEND, "native");
        assert_eq!(
            resolve_execution_backend(),
            ExecutionBackend::NativeAgentkit
        );
        match previous {
            Some(value) => std::env::set_var(ENV_EXECUTION_BACKEND, value),
            None => std::env::remove_var(ENV_EXECUTION_BACKEND),
        }
    }

    #[test]
    fn host_status_reports_native_wired_projection_and_legacy_compat() {
        let previous = std::env::var(ENV_EXECUTION_BACKEND).ok();
        std::env::remove_var(ENV_EXECUTION_BACKEND);
        let status = host_status();
        assert!(status.wired);
        assert_eq!(status.default_backend, BACKEND_NATIVE_AGENTKIT);
        assert_eq!(status.active_backend, BACKEND_NATIVE_AGENTKIT);
        assert!(!status.capabilities.node_runner_default);
        assert!(!status.capabilities.official_agent_server);
        assert!(status.timeline_is_agentkit_projection);
        assert_eq!(status.product_timeline_store, PRODUCT_TIMELINE_STORE_ID);
        assert!(status.desktop_sqlite_is_ui_cache_only);
        assert!(!status.node_runner_legacy_compatibility);
        assert_eq!(
            status.node_runner_compat_until,
            LEGACY_NODE_RUNNER_COMPAT_UNTIL
        );
        assert!(!status.default_bundle_includes_official_agent_server);
        assert!(!status.default_bundle_includes_node_agent_runner);
        assert!(!status.legacy_runner_feature_compiled);
        // Fresh process has no product credentials → reference path; live flag stays false.
        assert!(!status.live_model_adapter_drives_turn);
        let diagnostics = status.diagnostics.expect("diagnostics");
        assert!(diagnostics.credential_and_runtime_independent);
        assert!(diagnostics.runtime_ready);
        assert!(!diagnostics.live_model_adapter_drives_turn);
        match previous {
            Some(value) => std::env::set_var(ENV_EXECUTION_BACKEND, value),
            None => std::env::remove_var(ENV_EXECUTION_BACKEND),
        }
    }
}

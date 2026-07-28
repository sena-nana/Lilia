//! Desktop Embedded Native AgentKit host wiring (#44 / #50 / #46 / #47).
//!
//! Default execution backend is Native AgentKit. Node `agent-runner` is a
//! limited-time compatibility escape hatch only
//! (`LILIA_AGENT_EXECUTION_BACKEND=node`, until [`LEGACY_NODE_RUNNER_COMPAT_UNTIL`]).
//!
//! Task timeline product facts come from AgentKit event projections in
//! `lilia-storage`. Desktop SQLite is a rebuildable UI cache, not an execution
//! fact source.

use std::collections::HashSet;
use std::sync::OnceLock;

use lilia_agent_integration::{
    NativeAgentKitRuntime, NativeRuntimeBootstrap, NativeTurnStreamPage,
    ProductCredentialImportInput, ProductCredentialLoginInput, SharedNativeAgentKitRuntime,
    PRODUCT_NATIVE_CODING_PROFILE_HINT,
};
use lilia_contracts::{
    ProductApprovalDecision, TaskId, TimelineProjectionEvent, PRODUCT_TIMELINE_STORE_ID,
    TIMELINE_UI_CACHE_KIND,
};
use lilia_core::{AgentKitClientPort, NativeAgentCapabilitySnapshot};
use mutsuki_agent_contracts::CredentialRef;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::agent_timeline::AgentTimelineEventInput;
use crate::chat::runner::{RunnerInvocation, RunnerOutput};
use crate::chat::state::{remember_agent_session, ChatStore};
use crate::chat::timeline_sink::persist_and_emit_input;
use crate::store::LiliaStore;
use crate::util::now_millis;

pub const BACKEND_NATIVE_AGENTKIT: &str = "native-agentkit";
const ENV_EXECUTION_BACKEND: &str = "LILIA_AGENT_EXECUTION_BACKEND";

/// #47 — Node `agent-runner` limited-time compatibility identity.
pub const LEGACY_NODE_AGENT_RUNNER_ID: &str = "node-agent-runner";
/// Product version after which Node runner must not return as a default path.
pub const LEGACY_NODE_RUNNER_COMPAT_UNTIL: &str = "1.0.0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionBackend {
    NativeAgentkit,
    /// Limited-time compatibility only (#47). Requires `legacy-runner` Cargo feature + env.
    #[cfg(feature = "legacy-runner")]
    NodeAgentRunner,
}

impl ExecutionBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeAgentkit => BACKEND_NATIVE_AGENTKIT,
            #[cfg(feature = "legacy-runner")]
            Self::NodeAgentRunner => LEGACY_NODE_AGENT_RUNNER_ID,
        }
    }
}

/// Resolve default Desktop execution backend.
///
/// Default: Native AgentKit. Escape hatch (only when built with `legacy-runner`):
/// `LILIA_AGENT_EXECUTION_BACKEND=node`. Never silently falls back to Node when Native fails.
pub fn resolve_execution_backend() -> ExecutionBackend {
    match std::env::var(ENV_EXECUTION_BACKEND)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "node" | "node-agent-runner" | "agent-runner" | "legacy" => {
            #[cfg(feature = "legacy-runner")]
            {
                ExecutionBackend::NodeAgentRunner
            }
            #[cfg(not(feature = "legacy-runner"))]
            {
                eprintln!(
                    "[native-agent] {ENV_EXECUTION_BACKEND}=node ignored: this binary was built \
                     without the `legacy-runner` feature (sources under apps/desktop/legacy/; \
                     compat until {LEGACY_NODE_RUNNER_COMPAT_UNTIL})"
                );
                ExecutionBackend::NativeAgentkit
            }
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

/// Automation / multi-Agent new tasks must not call Claude/Codex official Server
/// or Node `agent-runner` directly — only AgentKit/Native (or an explicit refuse).
pub fn require_native_for_automation_or_multi_agent(context: &str) -> Result<(), String> {
    match resolve_execution_backend() {
        ExecutionBackend::NativeAgentkit => {
            let _ = context;
            Ok(())
        }
        #[cfg(feature = "legacy-runner")]
        ExecutionBackend::NodeAgentRunner => Err(format!(
            "{context}: 新任务不得直调 Claude/Codex 官方 Server 或 Node runner，须走 AgentKit/Native（请取消 {ENV_EXECUTION_BACKEND}=node）"
        )),
    }
}

/// #47 honesty: whether this binary compiled the Node legacy-runner feature gate.
pub const LEGACY_RUNNER_FEATURE_COMPILED: bool = cfg!(feature = "legacy-runner");

fn shared_runtime() -> Result<&'static SharedNativeAgentKitRuntime, String> {
    static RUNTIME: OnceLock<Result<SharedNativeAgentKitRuntime, String>> = OnceLock::new();
    match RUNTIME.get_or_init(|| {
        NativeRuntimeBootstrap::embedded_reference()
            .map_err(|err| err.to_string())
            .and_then(|bootstrap| {
                // Same path assembly as Service (#56): `$LILIA_HOME/db/product_projections.db`.
                let paths = lilia_storage::LiliaDataPaths::from_home(crate::store::resolve_lilia_home());
                let _ = paths.ensure_layout();
                let store =
                    lilia_storage::SqliteTimelineProjectionStore::open(paths.product_projections_db())
                        .map_err(|err| err.to_string())?;
                Ok(SharedNativeAgentKitRuntime::new(
                    bootstrap.into_runtime_with_projection_store(store),
                ))
            })
    }) {
        Ok(runtime) => Ok(runtime),
        Err(err) => Err(err.clone()),
    }
}

pub fn native_runtime() -> Result<&'static NativeAgentKitRuntime, String> {
    Ok(shared_runtime()?.inner())
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
    /// #47 honesty: `legacy-runner` Cargo feature compiled into this binary.
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
            node_runner_default: {
                #[cfg(feature = "legacy-runner")]
                {
                    active == ExecutionBackend::NodeAgentRunner
                }
                #[cfg(not(feature = "legacy-runner"))]
                {
                    let _ = active;
                    false
                }
            },
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
    NativeAgentHostStatus {
        wired: true,
        default_backend: BACKEND_NATIVE_AGENTKIT,
        active_backend: active.as_str(),
        profile_hint: PRODUCT_NATIVE_CODING_PROFILE_HINT,
        capabilities,
        env_override: std::env::var(ENV_EXECUTION_BACKEND).ok(),
        diagnostics,
        timeline_is_agentkit_projection: true,
        product_timeline_store: PRODUCT_TIMELINE_STORE_ID,
        desktop_sqlite_is_ui_cache_only: true,
        node_runner_legacy_compatibility: true,
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
    let view = runtime.credentials().login(input).map_err(|e| e.to_string())?;
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
pub fn native_respond_approval(
    decision: ProductApprovalDecision,
) -> Result<serde_json::Value, String> {
    let runtime = native_runtime()?;
    let session = lilia_contracts::AgentSessionRef::new(decision.session_id.clone())
        .map_err(|err| err.to_string())?;
    let page = runtime
        .respond_approval_streaming(&session, &decision)
        .map_err(|err| err.to_string())?;
    serde_json::to_value(page).map_err(|e| e.to_string())
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
        .map(|(index, event)| {
            crate::agent_timeline::AgentTimelineEvent {
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
            }
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
    let session = runtime
        .start_session_for_task(&task_id, Some(PRODUCT_NATIVE_CODING_PROFILE_HINT))
        .map_err(|err| err.to_string())?;

    {
        let store = app_handle.state::<ChatStore>();
        if let Some(lilia_store) = app_handle.try_state::<LiliaStore>() {
            if let Ok(conn) = lilia_store.conn() {
                remember_agent_session(
                    &conn,
                    &store,
                    &invocation.task_id,
                    BACKEND_NATIVE_AGENTKIT,
                    session.as_str(),
                    "native-agentkit",
                );
            }
        } else {
            store
                .sdk_sessions
                .lock()
                .unwrap()
                .insert(
                    crate::chat::state::session_key(BACKEND_NATIVE_AGENTKIT, &invocation.task_id),
                    session.as_str().to_string(),
                );
        }
    }

    let page = runtime
        .submit_turn_streaming(&session, &invocation.content, &invocation.turn_id)
        .map_err(|err| err.to_string())?;

    // Runtime already applied projection commands to `lilia-storage`.
    // Desktop SQLite only mirrors the product surface as UI cache.
    mirror_stream_page_to_ui_cache(app_handle, &invocation, &page)?;

    let _ = app_handle.emit(
        "native-agent-stream",
        serde_json::json!({
            "taskId": invocation.task_id,
            "turnId": invocation.turn_id,
            "sessionId": page.session_id,
            "eventCount": page.events.len(),
            "nextSequence": page.next_sequence,
            "officialAgentServer": page.official_agent_server,
            "credentialBound": page.credential_bound,
            "liveModelAdapterDrivesTurn": page.live_model_adapter_drives_turn,
            "profileId": page.profile_id,
            "toolSummary": page.tool_summary,
            "timelineIsProjection": true,
            "productTimelineStore": PRODUCT_TIMELINE_STORE_ID,
            "desktopSqliteIsUiCacheOnly": true,
        }),
    );

    Ok(RunnerOutput {
        last_session_id: Some(page.session_id),
        interrupted: false,
        reset: false,
    })
}

fn mirror_stream_page_to_ui_cache<R: Runtime>(
    app_handle: &AppHandle<R>,
    invocation: &RunnerInvocation,
    page: &NativeTurnStreamPage,
) -> Result<(), String> {
    let runtime = native_runtime()?;
    let task_id = TaskId::new(invocation.task_id.clone()).map_err(|e| e.to_string())?;
    let sequences: HashSet<u64> = page.events.iter().map(|event| event.sequence).collect();
    let events = runtime
        .product_timeline_for_task(&task_id)
        .into_iter()
        .filter(|event| sequences.contains(&event.sequence))
        .collect::<Vec<_>>();
    let _ = mirror_product_timeline_to_ui_cache(app_handle, &events)?;
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
        obj.insert(
            "agentSessionId".into(),
            json!(event.agent_session.as_str()),
        );
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
        #[cfg(feature = "legacy-runner")]
        {
            assert_eq!(
                resolve_execution_backend(),
                ExecutionBackend::NodeAgentRunner
            );
            let err =
                require_native_for_automation_or_multi_agent("Automation Agent 节点").unwrap_err();
            assert!(err.contains("不得直调"));
            assert!(err.contains("AgentKit/Native"));
        }
        #[cfg(not(feature = "legacy-runner"))]
        {
            // Default build: Node env is ignored; binary does not link Node path.
            assert_eq!(
                resolve_execution_backend(),
                ExecutionBackend::NativeAgentkit
            );
            assert!(!LEGACY_RUNNER_FEATURE_COMPILED);
            assert!(require_native_for_automation_or_multi_agent("Automation Agent 节点").is_ok());
        }
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
        assert!(status.node_runner_legacy_compatibility);
        assert_eq!(status.node_runner_compat_until, LEGACY_NODE_RUNNER_COMPAT_UNTIL);
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

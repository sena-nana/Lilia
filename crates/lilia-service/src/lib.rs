//! Service-mode authority (#60).
//!
//! Host-neutral long-running authority for LiliaCore + AgentKit:
//! - one shared Runtime Arc
//! - one shared product store + timeline projection
//! - process-local single-writer lease + optional single-machine file lock
//! - health / restart surfaces for `apps/service`
//! - Desktop client disconnect does **not** stop Service-side projection turns
//! - crash restart reloads SQLite projections + session bindings under `LiliaDataPaths`
//!
//! Full ServiceHost + MutsukiLink multiplexing + cross-process **epoch fencing**
//! of late commands remain follow-ups; file lock covers single-host dual-writer
//! exclusion only (see `writer_lease`).

mod health;
mod observe;
mod writer_lease;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use lilia_agent_integration::{
    IndependentDiagnostics, NativeAgentWireService, NativeQuotaSurface, NativeRuntimeBootstrap,
    NativeRuntimeError, NativeRuntimeMode, NativeTurnStreamPage, ProductCredentialBridge,
    SharedNativeAgentKitRuntime,
};
use lilia_client::LiliaClient;
use lilia_contracts::{
    AgentSessionBinding, AgentSessionRef, ProductApprovalDecision, ProductResult, TaskId,
    TimelineProjectionCommand, TimelineProjectionEvent,
};
use lilia_core::{
    AgentKitClientPort, AgentKitPortError, InMemoryProductStore, NativeAgentCapabilitySnapshot,
    ProductRepository,
};
use lilia_storage::{
    InMemoryTimelineProjectionStore, LiliaDataPaths, ProjectionApplyResult,
    SqliteAgentRuntimeStateStore, SqliteProductStore, SqliteTimelineProjectionStore,
    TimelineProjectionRepository,
};
use mutsuki_agent_client::dispatch_agent_request;
use mutsuki_agent_contracts::{
    AgentEventEnvelope, AgentMessage, AgentSession, AgentWireError, AgentWireRequestEnvelope,
    AgentWireResponseEnvelope, InteractionResolution,
};
use serde::Serialize;

pub use health::{ComponentHealth, ServiceHealthReport, ServiceHealthStatus};
pub use observe::{
    read_http_request, serve_readonly_http, RemoteDiagnosticsObserve, RemoteObserveStatus,
    RemoteTimelineObserve,
};
pub use writer_lease::{
    writer_lease_health, StorageWriterGuard, StorageWriterLease, WriterLeaseError,
    WriterLeaseHealth, WriterMode,
};

#[derive(Debug, thiserror::Error)]
pub enum ServiceAuthorityError {
    #[error(transparent)]
    Native(#[from] NativeRuntimeError),
    #[error(transparent)]
    Writer(#[from] WriterLeaseError),
    #[error("service authority is stopped")]
    Stopped,
    #[error("{0}")]
    Product(String),
}

impl From<lilia_contracts::ProductError> for ServiceAuthorityError {
    fn from(value: lilia_contracts::ProductError) -> Self {
        Self::Product(value.to_string())
    }
}

struct ServiceAuthorityInner {
    runtime: SharedNativeAgentKitRuntime,
    wire: Mutex<NativeAgentWireService>,
    product_repository: Arc<dyn ProductRepository>,
    timeline: Arc<InMemoryTimelineProjectionStore>,
    writer: StorageWriterGuard,
    storage_key: String,
    generation: AtomicU64,
    stopped: AtomicBool,
    /// Shared on-disk layout with Desktop when bootstrapped via `bootstrap_with_home`.
    data_paths: Option<LiliaDataPaths>,
    product_store: Option<Arc<SqliteProductStore>>,
    projection_db: Option<PathBuf>,
    product_db: Option<PathBuf>,
}

/// Host-neutral service authority holding one shared AgentKit Runtime.
#[derive(Clone)]
pub struct ServiceAuthority {
    inner: Arc<ServiceAuthorityInner>,
}

impl std::fmt::Debug for ServiceAuthority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceAuthority")
            .field("storage_key", &self.inner.storage_key)
            .field("generation", &self.generation())
            .field("stopped", &self.inner.stopped.load(Ordering::SeqCst))
            .field("writer", self.writer_lease())
            .finish()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAuthorityStatus {
    pub mode: &'static str,
    pub desktop_exclusive_runtime: bool,
    pub shared_runtime_clients: bool,
    pub shared_projection_clients: bool,
    pub shared_projection_db_layout: bool,
    pub credential_broker_ready: bool,
    pub supports_approval: bool,
    pub remote_quota_api: &'static str,
    pub writer_epoch: u64,
    pub generation: u64,
    pub capabilities: NativeAgentCapabilitySnapshot,
    pub projection_db_path: Option<String>,
    pub product_db_path: Option<String>,
    pub writer_file_lock_held: bool,
    pub cross_process_epoch_fencing: bool,
}

impl ServiceAuthority {
    /// Bootstrap an in-memory Service-mode authority (no Desktop UI).
    pub fn bootstrap_in_memory() -> Result<Self, ServiceAuthorityError> {
        Self::bootstrap_in_memory_named("in-memory:default", "lilia-service")
    }

    pub fn bootstrap_in_memory_named(
        storage_key: impl Into<String>,
        owner_id: impl Into<String>,
    ) -> Result<Self, ServiceAuthorityError> {
        Self::bootstrap_in_memory_named_with_credentials(
            storage_key,
            owner_id,
            ProductCredentialBridge::new(),
        )
    }

    pub fn bootstrap_in_memory_named_with_credentials(
        storage_key: impl Into<String>,
        owner_id: impl Into<String>,
        credentials: ProductCredentialBridge,
    ) -> Result<Self, ServiceAuthorityError> {
        let storage_key = storage_key.into();
        let owner_id = owner_id.into();
        let writer =
            StorageWriterGuard::try_acquire(storage_key.clone(), owner_id, WriterMode::Service)?;
        let bootstrap = NativeRuntimeBootstrap::service_reference_with_credentials(credentials)?;
        debug_assert_eq!(bootstrap.mode(), NativeRuntimeMode::Service);
        let product_repository: Arc<dyn ProductRepository> =
            Arc::new(Mutex::new(InMemoryProductStore::new()));
        let runtime = SharedNativeAgentKitRuntime::new(bootstrap.into_runtime());
        Ok(Self {
            inner: Arc::new(ServiceAuthorityInner {
                wire: Mutex::new(NativeAgentWireService::new(runtime.clone())),
                runtime,
                product_repository,
                timeline: Arc::new(InMemoryTimelineProjectionStore::new()),
                writer,
                storage_key,
                generation: AtomicU64::new(1),
                stopped: AtomicBool::new(false),
                data_paths: None,
                product_store: None,
                projection_db: None,
                product_db: None,
            }),
        })
    }

    /// Bootstrap against the shared on-disk layout used by Desktop Embedded (#56).
    ///
    /// Opens `$home/db/product_projections.db` for AgentKit product projections and
    /// `$home/db/product.db` for Project/Task/Binding — same `LiliaDataPaths`
    /// assembly as Desktop `native_agent`. Acquires `$home/db/writer.lock` for
    /// single-machine dual-writer exclusion (not distributed epoch fencing).
    pub fn bootstrap_with_home(home: impl Into<PathBuf>) -> Result<Self, ServiceAuthorityError> {
        Self::bootstrap_with_home_and_credentials(home, ProductCredentialBridge::new())
    }

    pub fn bootstrap_with_home_and_credentials(
        home: impl Into<PathBuf>,
        credentials: ProductCredentialBridge,
    ) -> Result<Self, ServiceAuthorityError> {
        let paths = LiliaDataPaths::from_home(home.into());
        paths.ensure_layout().map_err(|err| {
            ServiceAuthorityError::Product(format!("ensure lilia data layout: {err}"))
        })?;
        let projection_path = paths.product_projections_db();
        let product_path = paths.product_db();
        let lock_path = paths.db_dir().join("writer.lock");
        let storage_key = projection_path.display().to_string();
        let writer = StorageWriterGuard::try_acquire_with_file_lock(
            storage_key.clone(),
            "lilia-service",
            WriterMode::Service,
            &lock_path,
        )?;
        let projections = SqliteTimelineProjectionStore::open(&projection_path)
            .map_err(|err| ServiceAuthorityError::Product(err.to_string()))?;
        let runtime_state = SqliteAgentRuntimeStateStore::open(paths.agent_runtime_db())
            .map_err(|err| ServiceAuthorityError::Product(err.to_string()))?;
        let product = Arc::new(
            SqliteProductStore::open(&product_path)
                .map_err(|err| ServiceAuthorityError::Product(err.to_string()))?,
        );
        let timeline = Arc::new(InMemoryTimelineProjectionStore::new());
        hydrate_timeline_from_sqlite(&timeline, product.as_ref(), &projections)?;
        let bootstrap = NativeRuntimeBootstrap::service_reference_with_credentials(credentials)?;
        debug_assert_eq!(bootstrap.mode(), NativeRuntimeMode::Service);
        let runtime = SharedNativeAgentKitRuntime::new(
            bootstrap.into_runtime_with_stores(projections, runtime_state),
        );
        runtime.inner().apply_migrated_skill_roots(&paths)?;
        for binding in product
            .list_all_bindings()
            .map_err(ServiceAuthorityError::from)?
        {
            match runtime.inner().restore_product_session_binding(
                &binding.task_id,
                binding.agent_session.as_str(),
                binding.profile_id.as_deref(),
            ) {
                Ok(_) | Err(AgentKitPortError::NotFound(_)) => {}
                Err(error) => {
                    return Err(ServiceAuthorityError::Product(format!(
                        "restore AgentKit product session binding: {error}"
                    )))
                }
            }
        }
        let wire = NativeAgentWireService::try_new(runtime.clone()).map_err(|error| {
            ServiceAuthorityError::Product(format!("{}: {}", error.code, error.message))
        })?;
        Ok(Self {
            inner: Arc::new(ServiceAuthorityInner {
                wire: Mutex::new(wire),
                runtime,
                product_repository: product.clone(),
                timeline,
                writer,
                storage_key,
                generation: AtomicU64::new(1),
                stopped: AtomicBool::new(false),
                data_paths: Some(paths),
                product_store: Some(product),
                projection_db: Some(projection_path),
                product_db: Some(product_path),
            }),
        })
    }

    pub fn storage_key(&self) -> &str {
        &self.inner.storage_key
    }

    pub fn data_paths(&self) -> Option<&LiliaDataPaths> {
        self.inner.data_paths.as_ref()
    }

    pub fn projection_db_path(&self) -> Option<&PathBuf> {
        self.inner.projection_db.as_ref()
    }

    pub fn product_store(&self) -> Option<&Arc<SqliteProductStore>> {
        self.inner.product_store.as_ref()
    }

    pub fn shared_runtime(&self) -> SharedNativeAgentKitRuntime {
        self.inner.runtime.clone()
    }

    /// Dispatch the canonical Mutsuki Agent Wire envelope against the same
    /// Service-owned runtime used by all product clients.
    pub fn dispatch_agent_wire(
        &self,
        request: AgentWireRequestEnvelope,
    ) -> Result<AgentWireResponseEnvelope, AgentWireError> {
        let mut wire = self.agent_wire_lock()?;
        dispatch_agent_request(&mut *wire, request)
    }

    pub fn open_agent_task_session(
        &self,
        task_id: &TaskId,
        requested_session_id: Option<&str>,
        profile_id: &str,
        title: Option<String>,
    ) -> Result<AgentSession, AgentWireError> {
        self.agent_wire_lock()?
            .open_task_session(task_id, requested_session_id, profile_id, title)
    }

    pub fn fork_agent_task_session_through_turn(
        &self,
        source_session_id: &str,
        target_session_id: &str,
        through_turn_id: &str,
    ) -> Result<AgentSession, AgentWireError> {
        self.agent_wire_lock()?.fork_task_session_through_turn(
            source_session_id,
            target_session_id,
            through_turn_id,
        )
    }

    pub fn fork_agent_task_session(
        &self,
        source_session_id: &str,
        target_session_id: &str,
    ) -> Result<AgentSession, AgentWireError> {
        self.agent_wire_lock()?
            .fork_task_session(source_session_id, target_session_id)
    }

    pub fn submit_agent_task_turn_observed<O>(
        &self,
        session_id: &str,
        turn_id: &str,
        messages: Vec<AgentMessage>,
        idempotency_key: &str,
        observer: O,
    ) -> Result<NativeTurnStreamPage, AgentWireError>
    where
        O: Fn(&[AgentEventEnvelope]) + Send + Sync + 'static,
    {
        self.agent_wire_lock()?
            .submit_task_turn_observed(session_id, turn_id, messages, idempotency_key, observer)
            .map(|result| result.page)
    }

    pub fn respond_agent_task_approval_observed<O>(
        &self,
        decision: ProductApprovalDecision,
        observer: O,
    ) -> Result<NativeTurnStreamPage, AgentWireError>
    where
        O: Fn(&[AgentEventEnvelope]) + Send + Sync + 'static,
    {
        self.agent_wire_lock()?
            .respond_task_approval_observed(decision, observer)
            .map(|result| result.page)
    }

    pub fn respond_agent_task_interaction_observed<O>(
        &self,
        resolution: InteractionResolution,
        observer: O,
    ) -> Result<NativeTurnStreamPage, AgentWireError>
    where
        O: Fn(&[AgentEventEnvelope]) + Send + Sync + 'static,
    {
        self.agent_wire_lock()?
            .respond_task_interaction_observed(resolution, observer)
            .map(|result| result.page)
    }

    pub fn shared_timeline(&self) -> Arc<InMemoryTimelineProjectionStore> {
        Arc::clone(&self.inner.timeline)
    }

    pub fn writer_lease(&self) -> &StorageWriterLease {
        self.inner.writer.lease()
    }

    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::SeqCst)
    }

    /// Each call returns a LiliaClient bound to the **same** Runtime + product + timeline Arcs.
    ///
    /// Dropping a client (Desktop disconnect) does not stop the authority or other clients.
    pub fn client(
        &self,
    ) -> Result<LiliaClient<SharedNativeAgentKitRuntime>, ServiceAuthorityError> {
        self.ensure_running()?;
        Ok(LiliaClient::with_repository_and_timeline(
            Arc::clone(&self.inner.product_repository),
            self.inner.runtime.clone(),
            Arc::clone(&self.inner.timeline),
        ))
    }

    /// Apply a product timeline projection to the authority fact store.
    ///
    /// Always writes the Runtime-owned SQLite projection store (durable under
    /// `bootstrap_with_home`). Also mirrors into the shared in-memory client timeline
    /// so live `LiliaClient` observers stay consistent within the process.
    pub fn apply_projection(
        &self,
        command: TimelineProjectionCommand,
    ) -> Result<ProjectionApplyResult, ServiceAuthorityError> {
        self.ensure_running()?;
        let result = self
            .inner
            .runtime
            .inner()
            .projections()
            .apply(command.clone())
            .map_err(ServiceAuthorityError::from)?;
        let _ = self.inner.timeline.apply(command);
        Ok(result)
    }

    /// Read product timeline from the Runtime projection store (SQLite when home-backed).
    pub fn projection_timeline_for_task(&self, task_id: &TaskId) -> Vec<TimelineProjectionEvent> {
        self.inner
            .runtime
            .inner()
            .projections()
            .list_for_task(task_id)
    }

    pub fn projection_cursor_for_session(&self, session: &AgentSessionRef) -> Option<u64> {
        self.inner
            .runtime
            .inner()
            .projections()
            .cursor_for_session(session)
    }

    /// Session bindings from durable product DB when available; else process memory.
    pub fn list_session_bindings(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<AgentSessionBinding>, ServiceAuthorityError> {
        if let Some(store) = &self.inner.product_store {
            return store
                .list_bindings_for_task(task_id)
                .map_err(ServiceAuthorityError::from);
        }
        self.client()?
            .list_bindings(task_id)
            .map_err(ServiceAuthorityError::from)
    }

    /// Product commands are written through the authoritative repository immediately.
    ///
    /// Retained as a compatibility checkpoint; no delayed in-memory flush exists.
    pub fn checkpoint_durable_product(&self) -> Result<(), ServiceAuthorityError> {
        Ok(())
    }

    pub fn credential_diagnostics(&self) -> IndependentDiagnostics {
        self.inner.runtime.inner().independent_diagnostics()
    }

    pub fn quota_surface(&self) -> NativeQuotaSurface {
        self.inner.runtime.inner().native_quota_surface()
    }

    pub fn status(&self) -> ServiceAuthorityStatus {
        let diagnostics = self.credential_diagnostics();
        let caps = self
            .inner
            .runtime
            .capabilities()
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
        let writer = self.writer_lease();
        ServiceAuthorityStatus {
            mode: "service",
            desktop_exclusive_runtime: false,
            shared_runtime_clients: true,
            shared_projection_clients: true,
            shared_projection_db_layout: self.inner.data_paths.is_some(),
            credential_broker_ready: diagnostics.credential.broker_ready,
            supports_approval: caps.supports_approval,
            remote_quota_api: "unavailable",
            writer_epoch: writer.epoch,
            generation: self.generation(),
            capabilities: caps,
            projection_db_path: self
                .inner
                .projection_db
                .as_ref()
                .map(|p| p.display().to_string()),
            product_db_path: self
                .inner
                .product_db
                .as_ref()
                .map(|p| p.display().to_string()),
            writer_file_lock_held: writer.file_lock_path.is_some(),
            cross_process_epoch_fencing: false,
        }
    }

    pub fn health(&self) -> ServiceHealthReport {
        if self.inner.stopped.load(Ordering::SeqCst) {
            return ServiceHealthReport {
                status: ServiceHealthStatus::Stopped,
                generation: self.generation(),
                mode: "service",
                desktop_exclusive_runtime: false,
                shared_runtime_clients: true,
                shared_projection_clients: true,
                core: ComponentHealth {
                    ok: false,
                    detail: "stopped",
                },
                agentkit: ComponentHealth {
                    ok: false,
                    detail: "stopped",
                },
                credential: ComponentHealth {
                    ok: false,
                    detail: "stopped",
                },
                projection: ComponentHealth {
                    ok: false,
                    detail: "stopped",
                },
                writer: writer_lease_health(&self.inner.storage_key),
            };
        }

        let diagnostics = self.credential_diagnostics();
        let caps = self.inner.runtime.capabilities().ok();
        let agent_ok = caps
            .as_ref()
            .map(|c| c.supports_session && !c.official_agent_server && !c.node_runner_default)
            .unwrap_or(false);
        let credential_ok =
            diagnostics.credential.broker_ready && !diagnostics.credential.broker_degraded;
        let projection_ok = true;
        let writer = writer_lease_health(&self.inner.storage_key);
        let writer_ok = writer.held && writer.single_writer;
        let core_ok = agent_ok && credential_ok && projection_ok && writer_ok;
        let status = if core_ok {
            ServiceHealthStatus::Ready
        } else {
            ServiceHealthStatus::Degraded
        };

        ServiceHealthReport {
            status,
            generation: self.generation(),
            mode: "service",
            desktop_exclusive_runtime: false,
            shared_runtime_clients: true,
            shared_projection_clients: true,
            core: ComponentHealth {
                ok: core_ok,
                detail: if core_ok { "ready" } else { "degraded" },
            },
            agentkit: ComponentHealth {
                ok: agent_ok,
                detail: if agent_ok {
                    "native-agentkit"
                } else {
                    "unavailable"
                },
            },
            credential: ComponentHealth {
                ok: credential_ok,
                detail: if diagnostics.credential.broker_degraded {
                    "broker_degraded"
                } else if credential_ok {
                    "broker_ready"
                } else {
                    "broker_not_ready"
                },
            },
            projection: ComponentHealth {
                ok: projection_ok,
                detail: if self.inner.projection_db.is_some() {
                    "shared_sqlite_path"
                } else {
                    "shared_in_memory"
                },
            },
            writer,
        }
    }

    /// Mark stopped and release the writer lease when the last Arc drops.
    pub fn shutdown(self) {
        self.inner.stopped.store(true, Ordering::SeqCst);
        // Dropping the last Arc releases StorageWriterGuard via Drop.
        drop(self);
    }

    /// Stop this authority and bootstrap a fresh generation.
    ///
    /// Home-backed authorities reopen the same SQLite layout (projection + bindings).
    pub fn restart(self) -> Result<Self, ServiceAuthorityError> {
        let home = self
            .inner
            .data_paths
            .as_ref()
            .map(|paths| paths.home().to_path_buf());
        let storage_key = self.storage_key().to_string();
        let owner = self.writer_lease().owner_id.clone();
        let previous_generation = self.generation();
        let credentials = self.inner.runtime.inner().credentials().clone();
        self.shutdown();
        let next = if let Some(home) = home {
            Self::bootstrap_with_home_and_credentials(home, credentials)?
        } else {
            Self::bootstrap_in_memory_named_with_credentials(storage_key, owner, credentials)?
        };
        next.inner
            .generation
            .store(previous_generation.saturating_add(1), Ordering::SeqCst);
        Ok(next)
    }

    /// Minimal multi-client path: bind session + approval without Desktop.
    pub fn respond_approval(
        &self,
        session: &AgentSessionRef,
        decision: &ProductApprovalDecision,
    ) -> ProductResult<()> {
        if session.as_str() != decision.session_id {
            return Err(lilia_contracts::ProductError::InvalidInput {
                field: "session_id".into(),
                message: "approval decision belongs to a different session".into(),
            });
        }
        self.inner
            .wire
            .lock()
            .map_err(|_| lilia_contracts::ProductError::Unavailable {
                message: "Agent Wire service lock is poisoned".into(),
            })?
            .respond_task_approval(decision.clone())
            .map(|_| ())
            .map_err(|error| lilia_contracts::ProductError::Unavailable {
                message: format!("{}: {}", error.code, error.message),
            })
    }

    fn ensure_running(&self) -> Result<(), ServiceAuthorityError> {
        if self.inner.stopped.load(Ordering::SeqCst) {
            Err(ServiceAuthorityError::Stopped)
        } else {
            Ok(())
        }
    }

    fn agent_wire_lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, NativeAgentWireService>, AgentWireError> {
        self.ensure_running().map_err(|error| AgentWireError {
            code: "agent.service.stopped".into(),
            message: error.to_string(),
            retryable: true,
        })?;
        self.inner.wire.lock().map_err(|_| AgentWireError {
            code: "agent.service.lock_poisoned".into(),
            message: "agent wire service lock is poisoned".into(),
            retryable: true,
        })
    }
}

fn hydrate_timeline_from_sqlite(
    timeline: &Arc<InMemoryTimelineProjectionStore>,
    product: &SqliteProductStore,
    projections: &SqliteTimelineProjectionStore,
) -> Result<(), ServiceAuthorityError> {
    let tasks = product.list_tasks()?;
    for task in tasks {
        for event in projections.list_for_task(&task.id) {
            let _ = timeline.apply(TimelineProjectionCommand::UpsertTimelineEvent { event });
        }
    }
    Ok(())
}

/// Pointer equality proof for two clients sharing one authority Runtime.
pub fn shared_runtime_ptr_eq(left: &ServiceAuthority, right: &ServiceAuthority) -> bool {
    Arc::ptr_eq(&left.inner.runtime.0, &right.inner.runtime.0)
}

pub fn shared_timeline_ptr_eq(left: &ServiceAuthority, right: &ServiceAuthority) -> bool {
    Arc::ptr_eq(&left.inner.timeline, &right.inner.timeline)
}

/// Minimal HTTP/1.1 health responder for `apps/service` (no framework).
pub fn health_http_response(report: &ServiceHealthReport) -> String {
    let body = serde_json::to_string(report)
        .unwrap_or_else(|_| r#"{"status":"degraded","error":"serialize_failed"}"#.to_string());
    let code = match report.status {
        ServiceHealthStatus::Ready => "200 OK",
        ServiceHealthStatus::Degraded => "503 Service Unavailable",
        ServiceHealthStatus::Stopped => "503 Service Unavailable",
    };
    format!(
        "HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lilia_agent_integration::{ProductCredentialLoginInput, QuotaApiAvailability};
    use lilia_contracts::{
        AgentSessionRef, BindingId, ProjectId, ProjectionEventId, TaskId,
        TimelineProjectionCommand, TimelineProjectionEvent, PRODUCT_TIMELINE_STORE_ID,
    };
    use mutsuki_agent_contracts::{CredentialKind, OPENAI_CREDENTIAL_PROVIDER_ID};
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    fn sample_event(
        task_id: &TaskId,
        session: &AgentSessionRef,
        sequence: u64,
        title: &str,
    ) -> TimelineProjectionEvent {
        TimelineProjectionEvent {
            id: ProjectionEventId::from_session_sequence(session.as_str(), sequence),
            task_id: task_id.clone(),
            agent_session: session.clone(),
            sequence,
            turn_id: Some(format!("turn-{sequence}")),
            kind: "message".into(),
            status: "success".into(),
            title: title.into(),
            summary: Some("projection".into()),
            payload: json!({
                "projected": true,
                "productProjectionStore": PRODUCT_TIMELINE_STORE_ID,
            }),
            projected: true,
        }
    }

    #[test]
    fn service_authority_is_not_desktop_exclusive_and_health_ready() {
        let authority =
            ServiceAuthority::bootstrap_in_memory_named("test:health-ready", "owner-health")
                .unwrap();
        let status = authority.status();
        assert_eq!(status.mode, "service");
        assert!(!status.desktop_exclusive_runtime);
        assert!(status.shared_runtime_clients);
        assert!(status.shared_projection_clients);
        assert!(!status.shared_projection_db_layout);
        assert!(status.credential_broker_ready);
        assert!(status.supports_approval);
        assert_eq!(status.remote_quota_api, "unavailable");
        assert!(!status.capabilities.official_agent_server);
        assert!(!status.capabilities.node_runner_default);
        assert!(!status.writer_file_lock_held);
        assert!(!status.cross_process_epoch_fencing);

        let health = authority.health();
        assert_eq!(health.status, ServiceHealthStatus::Ready);
        assert!(health.core.ok);
        assert!(health.agentkit.ok);
        assert!(health.credential.ok);
        assert!(health.projection.ok);
        assert!(health.writer.held);
        assert!(health.writer.single_writer);
        assert!(!health.writer.cross_process_epoch_fencing);
        assert_eq!(health.writer.mode, Some(WriterMode::Service));
    }

    #[test]
    fn service_authority_uses_injected_persistent_credential_bridge() {
        let credentials = ProductCredentialBridge::new();
        credentials
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: Some("service".into()),
                source: Some("injected".into()),
            })
            .unwrap();

        let authority = ServiceAuthority::bootstrap_in_memory_named_with_credentials(
            "test:credential-injection",
            "owner-credential-injection",
            credentials,
        )
        .unwrap();
        let authority = authority.restart().unwrap();
        let diagnostics = authority.credential_diagnostics();
        assert_eq!(diagnostics.credential.active_count, 1);
        assert!(diagnostics.profile_has_credential_refs);
        assert!(diagnostics.live_model_adapter_drives_turn);
    }

    #[test]
    fn two_clients_reuse_same_runtime_store_and_timeline_projection() {
        let authority =
            ServiceAuthority::bootstrap_in_memory_named("test:shared-clients", "owner-shared")
                .unwrap();
        let client_a = authority.client().unwrap();
        let client_b = authority.client().unwrap();

        let project = client_a
            .create_project(ProjectId::new("p-svc").unwrap(), "Service")
            .unwrap();
        let task = client_a
            .create_task(
                TaskId::new("t-svc").unwrap(),
                Some(project.id),
                "shared runtime",
            )
            .unwrap();
        let binding = client_a
            .bind_agent_session(
                &task.id,
                None,
                Some("mutsuki.reference.coding-agent"),
                BindingId::new("bind-svc").unwrap(),
            )
            .unwrap();

        assert_eq!(client_b.list_bindings(&task.id).unwrap().len(), 1);
        assert_eq!(
            client_b.list_bindings(&task.id).unwrap()[0].binding_id,
            binding.binding_id
        );

        let session = AgentSessionRef::new("sess-shared").unwrap();
        let event = sample_event(&task.id, &session, 1, "shared");
        client_a
            .apply_timeline_projection(TimelineProjectionCommand::UpsertTimelineEvent {
                event: event.clone(),
            })
            .unwrap();
        let listed = client_b.product_timeline_for_task(&task.id);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "shared");
        assert!(listed[0].projected);

        let clone = authority.clone();
        assert!(shared_runtime_ptr_eq(&authority, &clone));
        assert!(shared_timeline_ptr_eq(&authority, &clone));
        assert!(std::sync::Arc::ptr_eq(
            &authority.shared_runtime().0,
            &clone.shared_runtime().0
        ));
        assert!(std::sync::Arc::ptr_eq(
            &authority.shared_timeline(),
            &clone.shared_timeline()
        ));

        let diag_a = authority.credential_diagnostics();
        let diag_b = clone.credential_diagnostics();
        assert_eq!(diag_a.runtime_backend, diag_b.runtime_backend);
        assert!(diag_a.credential_and_runtime_independent);
        assert_eq!(
            authority.quota_surface().remote_quota_api,
            QuotaApiAvailability::Unavailable
        );

        let session = binding.agent_session.clone();
        let err = authority
            .respond_approval(
                &session,
                &ProductApprovalDecision {
                    session_id: session.as_str().to_string(),
                    turn_id: "turn-missing".into(),
                    action_id: "action-missing".into(),
                    version: 1,
                    approved: false,
                },
            )
            .unwrap_err();
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn desktop_client_disconnect_does_not_stop_service_turn_projection() {
        let authority =
            ServiceAuthority::bootstrap_in_memory_named("test:desktop-disconnect", "owner-disc")
                .unwrap();

        // Desktop client attaches, starts a turn projection, then disconnects.
        let desktop = authority.client().unwrap();
        let project = desktop
            .create_project(ProjectId::new("p-disc").unwrap(), "Disconnect")
            .unwrap();
        let task = desktop
            .create_task(
                TaskId::new("t-disc").unwrap(),
                Some(project.id),
                "continue after close",
            )
            .unwrap();
        let _binding = desktop
            .bind_agent_session(
                &task.id,
                None,
                Some("mutsuki.reference.coding-agent"),
                BindingId::new("bind-disc").unwrap(),
            )
            .unwrap();
        let session = AgentSessionRef::new("sess-disc").unwrap();
        authority
            .apply_projection(TimelineProjectionCommand::UpsertTimelineEvent {
                event: sample_event(&task.id, &session, 1, "before-desktop-close"),
            })
            .unwrap();
        drop(desktop);

        // Service-side authority continues the turn / projection without Desktop.
        assert_eq!(authority.health().status, ServiceHealthStatus::Ready);
        authority
            .apply_projection(TimelineProjectionCommand::UpsertTimelineEvent {
                event: sample_event(&task.id, &session, 2, "after-desktop-close"),
            })
            .unwrap();

        let service = authority.client().unwrap();
        let listed = service.product_timeline_for_task(&task.id);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].title, "before-desktop-close");
        assert_eq!(listed[1].title, "after-desktop-close");
        assert_eq!(authority.projection_timeline_for_task(&task.id).len(), 2);
        assert_eq!(authority.projection_cursor_for_session(&session), Some(2));
        assert!(!authority.status().desktop_exclusive_runtime);
    }

    #[test]
    fn single_writer_lease_blocks_second_service_authority() {
        let key = "test:single-writer";
        let _first = ServiceAuthority::bootstrap_in_memory_named(key, "owner-a").unwrap();
        let err = ServiceAuthority::bootstrap_in_memory_named(key, "owner-b").unwrap_err();
        assert!(matches!(
            err,
            ServiceAuthorityError::Writer(WriterLeaseError::AlreadyHeld { .. })
        ));
    }

    #[test]
    fn restart_releases_writer_and_returns_ready_health() {
        let authority =
            ServiceAuthority::bootstrap_in_memory_named("test:restart", "restart-owner").unwrap();
        let before = authority.health();
        assert_eq!(before.status, ServiceHealthStatus::Ready);
        let epoch_before = authority.writer_lease().epoch;

        let restarted = authority.restart().unwrap();
        let after = restarted.health();
        assert_eq!(after.status, ServiceHealthStatus::Ready);
        assert!(after.writer.held);
        assert_ne!(restarted.writer_lease().epoch, epoch_before);
        assert_eq!(restarted.writer_lease().owner_id, "restart-owner");
        assert!(restarted.generation() >= 2);
    }

    #[test]
    fn health_http_endpoint_reports_ready() {
        let authority =
            ServiceAuthority::bootstrap_in_memory_named("test:health-http", "health-http").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let report = authority.health();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = health_http_response(&report);
            stream.write_all(response.as_bytes()).unwrap();
        });

        thread::sleep(Duration::from_millis(20));
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut body = String::new();
        stream.read_to_string(&mut body).unwrap();
        assert!(body.contains("200 OK"));
        assert!(body.contains("\"status\":\"ready\""));
        assert!(body.contains("\"singleWriter\":true"));
        assert!(body.contains("\"crossProcessEpochFencing\":false"));
    }

    #[test]
    fn service_and_desktop_share_projection_db_path_under_same_home() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("lilia-svc-shared-{nanos}"));
        let _ = std::fs::remove_dir_all(&home);

        let expected = LiliaDataPaths::from_home(&home).product_projections_db();
        let authority = ServiceAuthority::bootstrap_with_home(&home).unwrap();
        let status = authority.status();
        assert!(status.shared_projection_db_layout);
        assert!(status.writer_file_lock_held);
        assert!(!status.cross_process_epoch_fencing);
        assert_eq!(
            status.projection_db_path.as_deref(),
            Some(expected.to_string_lossy().as_ref())
        );
        assert_eq!(
            authority.projection_db_path().map(|p| p.as_path()),
            Some(expected.as_path())
        );
        assert!(home.join("db").join("product.db").is_file());
        assert!(home.join("db").join("writer.lock").is_file());
        assert!(expected.is_file());
        assert_eq!(authority.health().projection.detail, "shared_sqlite_path");
        assert!(authority.health().writer.file_lock_held);

        // Second authority on same home is rejected by file lock / process lease.
        let err = ServiceAuthority::bootstrap_with_home(&home).unwrap_err();
        assert!(matches!(
            err,
            ServiceAuthorityError::Writer(
                WriterLeaseError::AlreadyHeld { .. } | WriterLeaseError::FileLockBusy { .. }
            )
        ));

        drop(authority);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn home_bootstrap_loads_migrated_skills_into_agentkit_registry() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("lilia-svc-skills-{nanos}"));
        let paths = LiliaDataPaths::from_home(&home);
        paths.ensure_layout().unwrap();
        let skill_root = home.join("legacy-skills");
        let skill_dir = skill_root.join("review");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nid: migrated-review\nversion: 1.0.0\ntitle: Migrated Review\nsummary: migrated skill\n---\n\nReview the workspace.\n",
        )
        .unwrap();
        let registry = lilia_storage::AgentkitSkillsRegistry {
            version: 1,
            revision: 0,
            secret_free: true,
            user_skill_roots: vec![skill_root.to_string_lossy().into_owned()],
            packages: Vec::new(),
        };
        std::fs::write(
            lilia_storage::skills_registry_path(&paths),
            serde_json::to_vec_pretty(&registry).unwrap(),
        )
        .unwrap();

        let authority = ServiceAuthority::bootstrap_with_home(&home).unwrap();
        let discovered = authority
            .shared_runtime()
            .inner()
            .bootstrap()
            .bundle()
            .core
            .skills
            .discover(Default::default())
            .unwrap();
        assert!(discovered
            .catalog
            .iter()
            .any(|skill| skill.skill_id == "migrated-review"));

        drop(authority);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn crash_restart_restores_sqlite_projection_and_session_binding() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("lilia-svc-crash-{nanos}"));
        let _ = std::fs::remove_dir_all(&home);

        let task_id;
        let session;
        let binding_id;
        {
            let authority = ServiceAuthority::bootstrap_with_home(&home).unwrap();
            let client = authority.client().unwrap();
            let project = client
                .create_project(ProjectId::new("p-crash").unwrap(), "Crash")
                .unwrap();
            let task = client
                .create_task(
                    TaskId::new("t-crash").unwrap(),
                    Some(project.id),
                    "recover me",
                )
                .unwrap();
            let binding = client
                .bind_agent_session(
                    &task.id,
                    None,
                    Some("mutsuki.reference.coding-agent"),
                    BindingId::new("bind-crash").unwrap(),
                )
                .unwrap();
            session = binding.agent_session.clone();
            authority
                .apply_projection(TimelineProjectionCommand::UpsertTimelineEvent {
                    event: sample_event(&task.id, &session, 1, "pre-crash"),
                })
                .unwrap();
            authority
                .apply_projection(TimelineProjectionCommand::UpsertTimelineEvent {
                    event: sample_event(&task.id, &session, 2, "still-running"),
                })
                .unwrap();
            task_id = task.id;
            binding_id = binding.binding_id;
            // Simulate crash: drop without graceful restart helper.
            drop(authority);
        }

        let recovered = ServiceAuthority::bootstrap_with_home(&home).unwrap();
        assert_eq!(recovered.health().status, ServiceHealthStatus::Ready);
        assert!(recovered.status().writer_file_lock_held);

        let timeline = recovered.projection_timeline_for_task(&task_id);
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline[0].title, "pre-crash");
        assert_eq!(timeline[1].title, "still-running");
        assert_eq!(recovered.projection_cursor_for_session(&session), Some(2));

        let bindings = recovered.list_session_bindings(&task_id).unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].binding_id, binding_id);
        assert_eq!(bindings[0].agent_session, session);

        // The same client contract reads the durable repository directly.
        let client = recovered.client().unwrap();
        assert_eq!(
            client.products().get_task(&task_id).unwrap().title,
            "recover me"
        );
        assert_eq!(
            client
                .products()
                .get_project(&ProjectId::new("p-crash").unwrap())
                .unwrap()
                .name,
            "Crash"
        );
        assert_eq!(client.list_bindings(&task_id).unwrap().len(), 1);
        assert_eq!(client.product_timeline_for_task(&task_id).len(), 2);

        drop(recovered);
        let _ = std::fs::remove_dir_all(&home);
    }
}

//! Product-core facade status exposed to Desktop.
//!
//! Native AgentKit is the default Desktop execution backend after Host pin alignment
//! (`Mutsuki@d475f1b`). Legacy Node `agent-runner` is limited-time compatibility
//! only via `LILIA_AGENT_EXECUTION_BACKEND=node` until
//! [`crate::native_agent::LEGACY_NODE_RUNNER_COMPAT_UNTIL`] (#47).

use std::path::Path;
use std::sync::Arc;

use lilia_client::LiliaClient;
use lilia_contracts::{
    AgentSessionBinding, AgentSessionRef, BindingId, ConversationId, ExpectedRevision,
    IdempotencyKey, Page, PageRequest, ProductCommandMeta, ProductCommandResult, ProductEntity,
    ProductEntityKind, ProductError, ProductEvent, ProductRevision, ProductTaskStatus, ProjectId,
    TaskId,
};
use lilia_core::UnavailableAgentKitPort;
use lilia_storage::SqliteProductStore;
use serde::Serialize;
use tauri::{Emitter, State};

use crate::native_agent::{self, BACKEND_NATIVE_AGENTKIT, LEGACY_NODE_RUNNER_COMPAT_UNTIL};

pub struct EmbeddedProductCore {
    client: LiliaClient,
}

impl EmbeddedProductCore {
    pub fn open(home: &Path) -> Result<Self, ProductError> {
        let paths = lilia_storage::LiliaDataPaths::from_home(home.to_path_buf());
        paths
            .ensure_layout()
            .map_err(|err| ProductError::Unavailable {
                message: format!("prepare product data layout: {err}"),
            })?;
        let repository = Arc::new(SqliteProductStore::open(paths.product_db())?);
        Ok(Self {
            client: LiliaClient::with_repository(repository, UnavailableAgentKitPort),
        })
    }

    pub(crate) fn binding_for_task(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<AgentSessionBinding>, ProductError> {
        Ok(self.client.list_bindings(task_id)?.into_iter().next())
    }

    /// Clear product Agent session bindings so a user reset does not resume the old session.
    pub(crate) fn clear_bindings_for_task(
        &self,
        task_id: &TaskId,
    ) -> Result<usize, ProductError> {
        self.client.clear_bindings(task_id)
    }

    pub(crate) fn create_task_with_conversation(
        &self,
        task_id: TaskId,
        project_id: Option<ProjectId>,
        title: &str,
    ) -> Result<(), ProductError> {
        let mut task = self
            .client
            .create_task(task_id.clone(), project_id.clone(), title)?;
        task.status = ProductTaskStatus::Running;
        let task = match self.client.products().update_entity(
            ProductEntity::Task(task),
            ExpectedRevision::new(ProductRevision::INITIAL.get())?,
        )? {
            ProductEntity::Task(task) => task,
            _ => {
                return Err(ProductError::InvalidState {
                    message: "task update returned a non-task entity".into(),
                });
            }
        };
        self.client.products().create_conversation(
            ConversationId::new(format!("conversation:{}", task.id.as_str()))?,
            project_id,
            Some(task.id),
            title,
        )?;
        Ok(())
    }

    pub(crate) fn persist_agent_session_binding(
        &self,
        task_id: &TaskId,
        session: &AgentSessionRef,
        profile_id: Option<String>,
    ) -> Result<AgentSessionBinding, ProductError> {
        if let Some(binding) = self.binding_for_task(task_id)? {
            if binding.agent_session == *session {
                return Ok(binding);
            }
        }
        let conversation_id = self
            .client
            .products()
            .list_entities(ProductEntityKind::Conversation)?
            .into_iter()
            .find_map(|entity| match entity {
                ProductEntity::Conversation(conversation)
                    if conversation.task_id.as_ref() == Some(task_id) =>
                {
                    Some(conversation.id)
                }
                _ => None,
            });
        let stable = format!("{}:{}", task_id.as_str(), session.as_str());
        let binding = AgentSessionBinding {
            binding_id: BindingId::new(format!("binding:{stable}"))?,
            task_id: task_id.clone(),
            conversation_id: conversation_id
                .map(|id| ConversationId::new(id.as_str()))
                .transpose()?,
            agent_session: session.clone(),
            profile_id,
            revision: ProductRevision::INITIAL,
        };
        let meta = ProductCommandMeta::create(
            format!("bind-agent-session:{stable}"),
            IdempotencyKey::new(format!("bind-agent-session:{stable}"))?,
        )?;
        let result = self.client.create_product_entity(
            &meta,
            ProductEntity::Binding(binding),
            "agent_session_bound",
        )?;
        match result.value {
            ProductEntity::Binding(binding) => Ok(binding),
            _ => Err(ProductError::InvalidState {
                message: "binding command returned a non-binding entity".into(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCoreStatus {
    pub cargo_workspace: bool,
    pub lilia_contracts: bool,
    pub lilia_core: bool,
    pub lilia_storage: bool,
    pub default_execution_backend: &'static str,
    pub active_execution_backend: &'static str,
    pub native_agentkit_crate: &'static str,
    pub native_agentkit_wired_in_desktop: bool,
    pub node_runner_is_default: bool,
    /// #47 — Node runner remains compile/runtime compatible only behind explicit env.
    pub node_runner_legacy_compatibility: bool,
    pub node_runner_compat_until: &'static str,
    /// #47 — default install resources exclude Codex app-server.
    pub default_bundle_includes_official_agent_server: bool,
    /// #47 — default install resources exclude Node agent-runner.
    pub default_bundle_includes_node_agent_runner: bool,
    /// #47 honesty: `legacy-runner` Cargo feature compiled into this binary.
    pub legacy_runner_feature_compiled: bool,
    /// Raw `LILIA_AGENT_EXECUTION_BACKEND` when set (debug / escape hatch).
    pub execution_backend_env_override: Option<String>,
    pub agent_capabilities: lilia_core::NativeAgentCapabilitySnapshot,
    pub mutsuki_core_pin: &'static str,
    pub credential_broker_wired: bool,
    pub timeline_is_agentkit_projection: bool,
    pub product_timeline_store: &'static str,
    pub desktop_sqlite_is_ui_cache_only: bool,
    pub live_model_adapter_drives_turn: bool,
}

#[tauri::command]
pub fn product_core_status() -> ProductCoreStatus {
    let host = native_agent::host_status();
    ProductCoreStatus {
        cargo_workspace: true,
        lilia_contracts: true,
        lilia_core: true,
        lilia_storage: true,
        default_execution_backend: BACKEND_NATIVE_AGENTKIT,
        active_execution_backend: host.active_backend,
        native_agentkit_crate: "crates/lilia-agent-integration",
        native_agentkit_wired_in_desktop: host.wired,
        node_runner_is_default: false,
        node_runner_legacy_compatibility: host.node_runner_legacy_compatibility,
        node_runner_compat_until: LEGACY_NODE_RUNNER_COMPAT_UNTIL,
        default_bundle_includes_official_agent_server: host
            .default_bundle_includes_official_agent_server,
        default_bundle_includes_node_agent_runner: host.default_bundle_includes_node_agent_runner,
        legacy_runner_feature_compiled: host.legacy_runner_feature_compiled,
        execution_backend_env_override: host.env_override.clone(),
        agent_capabilities: host.capabilities,
        mutsuki_core_pin: "d475f1ba24942b50e42ed2588e8fd208f1381a12",
        credential_broker_wired: host
            .diagnostics
            .as_ref()
            .map(|d| d.credential.broker_ready)
            .unwrap_or(false),
        timeline_is_agentkit_projection: host.timeline_is_agentkit_projection,
        product_timeline_store: host.product_timeline_store,
        desktop_sqlite_is_ui_cache_only: host.desktop_sqlite_is_ui_cache_only,
        live_model_adapter_drives_turn: host.live_model_adapter_drives_turn,
    }
}

#[tauri::command]
pub fn product_create_entity(
    app: tauri::AppHandle,
    state: State<'_, EmbeddedProductCore>,
    meta: ProductCommandMeta,
    entity: ProductEntity,
    action: String,
) -> Result<ProductCommandResult<ProductEntity>, ProductError> {
    let result = state.client.create_product_entity(&meta, entity, &action)?;
    emit_product_event(&app, &state, result.event_sequence.get());
    Ok(result)
}

#[tauri::command]
pub fn product_update_entity(
    app: tauri::AppHandle,
    state: State<'_, EmbeddedProductCore>,
    meta: ProductCommandMeta,
    entity: ProductEntity,
    action: String,
) -> Result<ProductCommandResult<ProductEntity>, ProductError> {
    let result = state.client.update_product_entity(&meta, entity, &action)?;
    emit_product_event(&app, &state, result.event_sequence.get());
    Ok(result)
}

#[tauri::command]
pub fn product_get_entity(
    state: State<'_, EmbeddedProductCore>,
    kind: ProductEntityKind,
    id: String,
) -> Result<Option<ProductEntity>, ProductError> {
    match state.client.products().get_entity(kind, &id) {
        Ok(entity) => Ok(Some(entity)),
        Err(ProductError::NotFound { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

#[tauri::command]
pub fn product_list_entities(
    state: State<'_, EmbeddedProductCore>,
    kind: ProductEntityKind,
) -> Result<Vec<ProductEntity>, ProductError> {
    state.client.products().list_entities(kind)
}

#[tauri::command]
pub fn product_list_events(
    state: State<'_, EmbeddedProductCore>,
    request: PageRequest,
) -> Result<Page<ProductEvent>, ProductError> {
    state.client.product_events(&request)
}

fn emit_product_event(app: &tauri::AppHandle, state: &EmbeddedProductCore, sequence: u64) {
    let request = PageRequest {
        after: sequence
            .checked_sub(1)
            .map(lilia_contracts::ProductEventSequence::new),
        limit: 1,
    };
    if let Ok(page) = state.client.product_events(&request) {
        if let Some(event) = page.items.into_iter().next() {
            let _ = app.emit(lilia_contracts::product_event_name(), event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lilia_contracts::ProjectId;

    #[test]
    fn status_defaults_to_native_agentkit_after_cutover() {
        let previous = std::env::var("LILIA_AGENT_EXECUTION_BACKEND").ok();
        std::env::remove_var("LILIA_AGENT_EXECUTION_BACKEND");
        let status = product_core_status();
        assert_eq!(status.default_execution_backend, BACKEND_NATIVE_AGENTKIT);
        assert_eq!(status.active_execution_backend, BACKEND_NATIVE_AGENTKIT);
        assert!(status.native_agentkit_wired_in_desktop);
        assert!(!status.node_runner_is_default);
        assert!(!status.agent_capabilities.node_runner_default);
        assert!(status.node_runner_legacy_compatibility);
        assert_eq!(
            status.node_runner_compat_until,
            LEGACY_NODE_RUNNER_COMPAT_UNTIL
        );
        assert!(!status.default_bundle_includes_official_agent_server);
        assert!(!status.default_bundle_includes_node_agent_runner);
        assert!(!status.legacy_runner_feature_compiled);
        assert!(status.execution_backend_env_override.is_none());
        assert!(status.timeline_is_agentkit_projection);
        assert!(status.desktop_sqlite_is_ui_cache_only);
        assert_eq!(
            status.product_timeline_store,
            lilia_contracts::PRODUCT_TIMELINE_STORE_ID
        );
        assert_eq!(
            native_agent::resolve_execution_backend(),
            native_agent::ExecutionBackend::NativeAgentkit
        );
        match previous {
            Some(value) => std::env::set_var("LILIA_AGENT_EXECUTION_BACKEND", value),
            None => std::env::remove_var("LILIA_AGENT_EXECUTION_BACKEND"),
        }
    }

    #[test]
    fn agent_session_binding_is_durable_and_idempotent_for_a_task() {
        let home = std::env::temp_dir().join(format!(
            "lilia-product-binding-{}-{}",
            std::process::id(),
            crate::util::now_millis()
        ));
        {
            let core = EmbeddedProductCore::open(&home).unwrap();
            let project = core
                .client
                .create_project(ProjectId::new("project-binding").unwrap(), "Binding")
                .unwrap();
            let task = core
                .client
                .create_task(
                    TaskId::new("task-binding").unwrap(),
                    Some(project.id),
                    "Persist binding",
                )
                .unwrap();
            let session = AgentSessionRef::new("agent-session-binding").unwrap();

            let first = core
                .persist_agent_session_binding(
                    &task.id,
                    &session,
                    Some("mutsuki.reference.coding-agent".into()),
                )
                .unwrap();
            let duplicate = core
                .persist_agent_session_binding(
                    &task.id,
                    &session,
                    Some("mutsuki.reference.coding-agent".into()),
                )
                .unwrap();

            assert_eq!(duplicate.binding_id, first.binding_id);
            assert_eq!(core.client.list_bindings(&task.id).unwrap().len(), 1);
        }
        {
            let reopened = EmbeddedProductCore::open(&home).unwrap();
            let binding = reopened
                .binding_for_task(&TaskId::new("task-binding").unwrap())
                .unwrap()
                .unwrap();
            assert_eq!(binding.agent_session.as_str(), "agent-session-binding");
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn automation_task_creation_uses_product_task_and_conversation() {
        let home = std::env::temp_dir().join(format!(
            "lilia-product-automation-{}-{}",
            std::process::id(),
            crate::util::now_millis()
        ));
        {
            let core = EmbeddedProductCore::open(&home).unwrap();
            let project = core
                .client
                .create_project(ProjectId::new("project-automation").unwrap(), "Automation")
                .unwrap();
            core.create_task_with_conversation(
                TaskId::new("task-automation").unwrap(),
                Some(project.id),
                "Automation task",
            )
            .unwrap();

            let task = core
                .client
                .products()
                .get_task(&TaskId::new("task-automation").unwrap())
                .unwrap();
            assert_eq!(task.status, ProductTaskStatus::Running);
            let conversations = core.client.products().list_conversations().unwrap();
            assert_eq!(conversations.len(), 1);
            assert_eq!(
                conversations[0].task_id.as_ref().map(TaskId::as_str),
                Some("task-automation")
            );
        }
        let _ = std::fs::remove_dir_all(home);
    }
}

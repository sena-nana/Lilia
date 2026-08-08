//! Unified `LiliaClient` facade. Desktop / CLI / Remote / Service share this contract.
//!
//! Product timeline reads go through `lilia-storage` projection store (#46 / #56),
//! not Desktop SQLite.

mod remote_agent;
mod remote_observe;

pub use remote_agent::AgentWireHttpBackend;
pub use remote_observe::{RemoteObserveError, RemoteObserveHttpClient};

use std::sync::{Arc, Mutex};

use lilia_contracts::{
    AgentSessionBinding, BindingId, ConversationId, ExpectedRevision, Page, PageRequest,
    ProductApprovalDecision, ProductCommandMeta, ProductCommandResult, ProductEntity, ProductEvent,
    ProductResult, ProductTask, Project, ProjectId, TaskId, TimelineProjectionCommand,
    TimelineProjectionEvent,
};
use lilia_core::{
    AgentKitClientPort, InMemoryProductStore, NativeAgentCapabilitySnapshot, ProductRepository,
    ProductServices, SessionBindingService, UnavailableAgentKitPort,
};
use lilia_storage::{
    InMemoryTimelineProjectionStore, ProjectionApplyResult, TimelineProjectionRepository,
};

pub struct LiliaClient<P: AgentKitClientPort = UnavailableAgentKitPort> {
    products: ProductServices,
    agent: P,
    /// Product timeline projection fact surface (not Host UI cache).
    timeline: Arc<InMemoryTimelineProjectionStore>,
}

impl LiliaClient<UnavailableAgentKitPort> {
    pub fn in_memory() -> Self {
        Self::with_agent(UnavailableAgentKitPort)
    }
}

impl<P: AgentKitClientPort> LiliaClient<P> {
    pub fn with_agent(agent: P) -> Self {
        Self {
            products: ProductServices::new(Arc::new(Mutex::new(InMemoryProductStore::new()))),
            agent,
            timeline: Arc::new(InMemoryTimelineProjectionStore::new()),
        }
    }

    pub fn with_store(store: Arc<Mutex<InMemoryProductStore>>, agent: P) -> Self {
        Self::with_repository(store, agent)
    }

    pub fn with_repository(repository: Arc<dyn ProductRepository>, agent: P) -> Self {
        Self {
            products: ProductServices::with_repository(repository),
            agent,
            timeline: Arc::new(InMemoryTimelineProjectionStore::new()),
        }
    }

    pub fn with_timeline_store(
        store: Arc<Mutex<InMemoryProductStore>>,
        agent: P,
        timeline: Arc<InMemoryTimelineProjectionStore>,
    ) -> Self {
        Self::with_repository_and_timeline(store, agent, timeline)
    }

    pub fn with_repository_and_timeline(
        repository: Arc<dyn ProductRepository>,
        agent: P,
        timeline: Arc<InMemoryTimelineProjectionStore>,
    ) -> Self {
        Self {
            products: ProductServices::with_repository(repository),
            agent,
            timeline,
        }
    }

    pub fn products(&self) -> &ProductServices {
        &self.products
    }

    /// Default product timeline read surface (#46 / #56).
    pub fn timeline(&self) -> &InMemoryTimelineProjectionStore {
        self.timeline.as_ref()
    }

    pub fn product_timeline_for_task(&self, task_id: &TaskId) -> Vec<TimelineProjectionEvent> {
        self.timeline.list_for_task(task_id)
    }

    pub fn apply_timeline_projection(
        &self,
        command: TimelineProjectionCommand,
    ) -> ProductResult<ProjectionApplyResult> {
        self.timeline.apply(command)
    }

    pub fn agent_capabilities(&self) -> ProductResult<NativeAgentCapabilitySnapshot> {
        self.agent.capabilities().map_err(Into::into)
    }

    pub fn create_project(&self, id: ProjectId, name: impl Into<String>) -> ProductResult<Project> {
        self.products.create_project(id, name)
    }

    pub fn create_task(
        &self,
        id: TaskId,
        project_id: Option<ProjectId>,
        title: impl Into<String>,
    ) -> ProductResult<ProductTask> {
        self.products.create_task(id, project_id, title)
    }

    pub fn update_task_dependencies(
        &self,
        task_id: &TaskId,
        depends_on: Vec<TaskId>,
        expected: ExpectedRevision,
    ) -> ProductResult<ProductTask> {
        self.products
            .update_task_dependencies(task_id, depends_on, expected)
    }

    pub fn create_product_entity(
        &self,
        meta: &ProductCommandMeta,
        entity: ProductEntity,
        action: &str,
    ) -> ProductResult<ProductCommandResult<ProductEntity>> {
        self.products.create_entity_command(meta, entity, action)
    }

    pub fn update_product_entity(
        &self,
        meta: &ProductCommandMeta,
        entity: ProductEntity,
        action: &str,
    ) -> ProductResult<ProductCommandResult<ProductEntity>> {
        self.products.update_entity_command(meta, entity, action)
    }

    pub fn product_events(&self, request: &PageRequest) -> ProductResult<Page<ProductEvent>> {
        self.products.product_events(request)
    }

    pub fn bind_agent_session(
        &self,
        task_id: &TaskId,
        conversation_id: Option<ConversationId>,
        profile_id: Option<&str>,
        binding_id: BindingId,
    ) -> ProductResult<AgentSessionBinding> {
        SessionBindingService::new(&self.products, &self.agent).bind_new_session(
            task_id,
            conversation_id,
            profile_id,
            binding_id,
        )
    }

    pub fn list_bindings(&self, task_id: &TaskId) -> ProductResult<Vec<AgentSessionBinding>> {
        self.products.list_bindings_for_task(task_id)
    }

    pub fn clear_bindings(&self, task_id: &TaskId) -> ProductResult<usize> {
        self.products.clear_bindings_for_task(task_id)
    }

    /// Submit a turn through the shared AgentKit client port (Desktop / CLI / Service).
    pub fn submit_turn(
        &self,
        session: &lilia_contracts::AgentSessionRef,
        prompt: impl AsRef<str>,
    ) -> ProductResult<()> {
        SessionBindingService::new(&self.products, &self.agent)
            .submit_prompt(session, prompt.as_ref())
    }

    pub fn respond_approval(
        &self,
        session: &lilia_contracts::AgentSessionRef,
        decision: &ProductApprovalDecision,
    ) -> ProductResult<()> {
        self.agent
            .respond_approval(session, decision)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lilia_contracts::{
        AgentSessionRef, IdempotencyKey, ProductEntity, ProductEventSequence, ProductRevision,
        ProductWorkflow, ProjectionEventId, WorkflowId, PRODUCT_TIMELINE_STORE_ID,
    };
    use lilia_storage::SqliteProductStore;
    use serde_json::json;

    #[test]
    fn in_memory_client_creates_project_and_task() {
        let client = LiliaClient::in_memory();
        let project = client
            .create_project(ProjectId::new("p1").unwrap(), "Demo")
            .unwrap();
        let task = client
            .create_task(
                TaskId::new("t1").unwrap(),
                Some(project.id.clone()),
                "First task",
            )
            .unwrap();
        assert_eq!(task.revision, ProductRevision::INITIAL);
        let caps = client.agent_capabilities().unwrap();
        assert!(!caps.node_runner_default);
        assert!(!caps.supports_session);
    }

    fn assert_product_command_conformance(client: &LiliaClient) {
        let workflow =
            ProductWorkflow::new(WorkflowId::new("workflow-conformance").unwrap(), "Review")
                .unwrap();
        let create_meta = ProductCommandMeta::create(
            "command-create-workflow",
            IdempotencyKey::new("idempotency-create-workflow").unwrap(),
        )
        .unwrap();
        let first = client
            .create_product_entity(
                &create_meta,
                ProductEntity::Workflow(workflow.clone()),
                "created",
            )
            .unwrap();
        assert!(!first.duplicate);
        let duplicate = client
            .create_product_entity(&create_meta, ProductEntity::Workflow(workflow), "created")
            .unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.event_sequence, first.event_sequence);

        let events = client.product_events(&PageRequest::default()).unwrap();
        assert_eq!(events.items.len(), 1);
        assert_eq!(events.items[0].action, "created");
        let after = PageRequest {
            after: events.next,
            limit: 10,
        };
        assert!(client.product_events(&after).unwrap().items.is_empty());
        assert_eq!(events.next, Some(ProductEventSequence::new(1)));
    }

    #[test]
    fn in_memory_and_sqlite_clients_share_product_command_contract() {
        assert_product_command_conformance(&LiliaClient::in_memory());

        let repository = Arc::new(SqliteProductStore::open_in_memory().unwrap());
        let client = LiliaClient::with_repository(repository, UnavailableAgentKitPort);
        assert_product_command_conformance(&client);
    }

    #[test]
    fn product_timeline_is_default_read_surface_and_idempotent() {
        let client = LiliaClient::in_memory();
        let task = TaskId::new("task-tl").unwrap();
        let session = AgentSessionRef::new("sess-tl").unwrap();
        let event = TimelineProjectionEvent {
            id: ProjectionEventId::from_session_sequence(session.as_str(), 1),
            task_id: task.clone(),
            agent_session: session.clone(),
            sequence: 1,
            turn_id: Some("turn-1".into()),
            kind: "message".into(),
            status: "success".into(),
            title: "ok".into(),
            summary: Some("hi".into()),
            payload: json!({
                "projected": true,
                "productProjectionStore": PRODUCT_TIMELINE_STORE_ID,
            }),
            projected: true,
        };
        let cmd = TimelineProjectionCommand::UpsertTimelineEvent {
            event: event.clone(),
        };
        assert_eq!(
            client.apply_timeline_projection(cmd.clone()).unwrap(),
            ProjectionApplyResult::Inserted
        );
        assert_eq!(
            client.apply_timeline_projection(cmd).unwrap(),
            ProjectionApplyResult::DuplicateIgnored
        );
        let listed = client.product_timeline_for_task(&task);
        assert_eq!(listed.len(), 1);
        assert!(listed[0].projected);
        assert_eq!(
            listed[0].payload.get("productProjectionStore"),
            Some(&json!(PRODUCT_TIMELINE_STORE_ID))
        );
        let rebuilt = client
            .timeline()
            .rebuild_session(
                &session,
                vec![TimelineProjectionCommand::UpsertTimelineEvent { event }],
            )
            .unwrap();
        assert_eq!(rebuilt, 1);
        assert_eq!(client.product_timeline_for_task(&task).len(), 1);
    }
}

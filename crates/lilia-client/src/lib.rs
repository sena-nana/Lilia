//! Unified `LiliaClient` facade. Desktop / CLI / Remote / Service share this contract.
//!
//! Product timeline reads go through `lilia-storage` projection store (#46 / #56),
//! not Desktop SQLite.

mod remote_observe;

pub use remote_observe::{RemoteObserveError, RemoteObserveHttpClient};

use std::sync::{Arc, Mutex};

use lilia_contracts::{
    AgentSessionBinding, BindingId, ConversationId, ExpectedRevision, ProductApprovalDecision,
    ProductResult, ProductTask, Project, ProjectId, TaskId, TimelineProjectionCommand,
    TimelineProjectionEvent,
};
use lilia_core::{
    AgentKitClientPort, InMemoryProductStore, NativeAgentCapabilitySnapshot, ProductServices,
    SessionBindingService, UnavailableAgentKitPort,
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
        Self {
            products: ProductServices::new(store),
            agent,
            timeline: Arc::new(InMemoryTimelineProjectionStore::new()),
        }
    }

    pub fn with_timeline_store(
        store: Arc<Mutex<InMemoryProductStore>>,
        agent: P,
        timeline: Arc<InMemoryTimelineProjectionStore>,
    ) -> Self {
        Self {
            products: ProductServices::new(store),
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

    pub fn create_project(
        &self,
        id: ProjectId,
        name: impl Into<String>,
    ) -> ProductResult<Project> {
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

    pub fn list_bindings(&self, task_id: &TaskId) -> Vec<AgentSessionBinding> {
        self.products.list_bindings_for_task(task_id)
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
        AgentSessionRef, ProductRevision, ProjectionEventId, PRODUCT_TIMELINE_STORE_ID,
    };
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

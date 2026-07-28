use lilia_contracts::{
    AgentSessionBinding, AgentSessionRef, BindingId, ConversationId, ProductError, ProductResult,
    ProductRevision, TaskId,
};

use crate::application::{AgentKitClientPort, ProductServices};

pub struct SessionBindingService<'a, P: AgentKitClientPort + ?Sized> {
    products: &'a ProductServices,
    agent: &'a P,
}

impl<'a, P: AgentKitClientPort + ?Sized> SessionBindingService<'a, P> {
    pub fn new(products: &'a ProductServices, agent: &'a P) -> Self {
        Self { products, agent }
    }

    /// Starts a Native AgentKit session and records a product binding. Does not
    /// complete the Product Task when the Agent session finishes.
    pub fn bind_new_session(
        &self,
        task_id: &TaskId,
        conversation_id: Option<ConversationId>,
        profile_id: Option<&str>,
        binding_id: BindingId,
    ) -> ProductResult<AgentSessionBinding> {
        let session = self
            .agent
            .start_session_for_task(task_id, profile_id)
            .map_err(ProductError::from)?;
        let binding = AgentSessionBinding {
            binding_id,
            task_id: task_id.clone(),
            conversation_id,
            agent_session: session,
            profile_id: profile_id.map(str::to_string),
            revision: ProductRevision::INITIAL,
        };
        self.products.record_binding(binding)
    }

    pub fn submit_prompt(&self, session: &AgentSessionRef, prompt: &str) -> ProductResult<()> {
        self.agent
            .submit_turn(session, prompt)
            .map_err(ProductError::from)
    }
}

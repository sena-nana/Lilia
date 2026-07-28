use serde::{Deserialize, Serialize};

use crate::{BindingId, ConversationId, ProductRevision, TaskId};

/// Opaque AgentKit session id reference. Product storage never owns Agent session state.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentSessionRef(String);

impl AgentSessionRef {
    pub fn new(value: impl Into<String>) -> Result<Self, crate::ProductError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(crate::ProductError::InvalidInput {
                field: "agent_session_id".into(),
                message: "agent_session_id must not be empty".into(),
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Product ↔ AgentKit session binding. Does not embed Agent turn/tool state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionBinding {
    pub binding_id: BindingId,
    pub task_id: TaskId,
    pub conversation_id: Option<ConversationId>,
    pub agent_session: AgentSessionRef,
    pub profile_id: Option<String>,
    pub revision: ProductRevision,
}

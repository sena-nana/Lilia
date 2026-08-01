use lilia_contracts::{AgentSessionRef, ProductApprovalDecision, TaskId};
use serde::{Deserialize, Serialize};

/// Host-neutral capability snapshot for Native AgentKit backends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAgentCapabilitySnapshot {
    pub backend: String,
    pub bundle_id: String,
    pub official_agent_server: bool,
    pub node_runner_default: bool,
    pub supports_session: bool,
    pub supports_stream: bool,
    pub supports_approval: bool,
    pub supports_cancel: bool,
    pub supports_resume: bool,
    pub profile_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AgentKitPortError {
    #[error("agentkit unavailable: {0}")]
    Unavailable(String),
    #[error("agentkit invalid input: {0}")]
    InvalidInput(String),
    #[error("agentkit not found: {0}")]
    NotFound(String),
}

/// Application port. Core never depends on concrete AgentKit crates.
pub trait AgentKitClientPort: Send + Sync {
    fn capabilities(&self) -> Result<NativeAgentCapabilitySnapshot, AgentKitPortError>;

    fn start_session_for_task(
        &self,
        task_id: &TaskId,
        profile_id: Option<&str>,
    ) -> Result<AgentSessionRef, AgentKitPortError>;

    fn submit_turn(&self, session: &AgentSessionRef, prompt: &str)
        -> Result<(), AgentKitPortError>;

    fn cancel_turn(&self, session: &AgentSessionRef) -> Result<(), AgentKitPortError>;

    /// Feed an approval decision back into the Native runtime and continue/deny the tool.
    fn respond_approval(
        &self,
        _session: &AgentSessionRef,
        _decision: &ProductApprovalDecision,
    ) -> Result<(), AgentKitPortError> {
        Err(AgentKitPortError::Unavailable(
            "approval response is not supported by this AgentKit backend".into(),
        ))
    }
}

impl From<AgentKitPortError> for lilia_contracts::ProductError {
    fn from(value: AgentKitPortError) -> Self {
        match value {
            AgentKitPortError::Unavailable(message) => {
                lilia_contracts::ProductError::Unavailable { message }
            }
            AgentKitPortError::InvalidInput(message) => {
                lilia_contracts::ProductError::InvalidInput {
                    field: "agentkit".into(),
                    message,
                }
            }
            AgentKitPortError::NotFound(message) => lilia_contracts::ProductError::NotFound {
                entity: "agent_session".into(),
                id: message,
            },
        }
    }
}

/// Unavailable placeholder until a Host injects a real Native backend.
#[derive(Debug, Default)]
pub struct UnavailableAgentKitPort;

impl AgentKitClientPort for UnavailableAgentKitPort {
    fn capabilities(&self) -> Result<NativeAgentCapabilitySnapshot, AgentKitPortError> {
        Ok(NativeAgentCapabilitySnapshot {
            backend: "unavailable".into(),
            bundle_id: String::new(),
            official_agent_server: false,
            // Native AgentKit is the product default; Node runner is never implied (#47).
            node_runner_default: false,
            supports_session: false,
            supports_stream: false,
            supports_approval: false,
            supports_cancel: false,
            supports_resume: false,
            profile_id: None,
        })
    }

    fn start_session_for_task(
        &self,
        _task_id: &TaskId,
        _profile_id: Option<&str>,
    ) -> Result<AgentSessionRef, AgentKitPortError> {
        Err(AgentKitPortError::Unavailable(
            "Native AgentKit backend is not wired in this host".into(),
        ))
    }

    fn submit_turn(
        &self,
        _session: &AgentSessionRef,
        _prompt: &str,
    ) -> Result<(), AgentKitPortError> {
        Err(AgentKitPortError::Unavailable(
            "Native AgentKit backend is not wired in this host".into(),
        ))
    }

    fn cancel_turn(&self, _session: &AgentSessionRef) -> Result<(), AgentKitPortError> {
        Err(AgentKitPortError::Unavailable(
            "Native AgentKit backend is not wired in this host".into(),
        ))
    }
}

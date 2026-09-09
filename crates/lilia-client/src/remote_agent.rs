use mutsuki_agent_client::{AgentClient, AgentClientBackend};
use mutsuki_agent_contracts::{
    AgentWireError, AgentWireRequestEnvelope, AgentWireResponseEnvelope,
};

use crate::remote_observe::{RemoteObserveError, ServiceHttpEndpoint};

/// Canonical Agent Client backend over the Lilia Service HTTP carrier.
///
/// The payload is the unmodified Mutsuki Agent Wire envelope; HTTP is only the
/// carrier, so Desktop, Service and remote clients share negotiation, versions,
/// approval binding, idempotency and event cursor behavior.
#[derive(Clone, Debug)]
pub struct AgentWireHttpBackend {
    endpoint: ServiceHttpEndpoint,
}

impl AgentWireHttpBackend {
    pub fn new(base_url: impl AsRef<str>) -> Result<Self, RemoteObserveError> {
        Ok(Self {
            endpoint: ServiceHttpEndpoint::new(base_url)?,
        })
    }

    /// Attach `Authorization: Bearer` required for `POST /agent/wire`.
    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.endpoint.bearer_token = Some(token.into());
        self
    }

    pub fn into_client(self) -> AgentClient<Self> {
        AgentClient::new(self)
    }
}

impl AgentClientBackend for AgentWireHttpBackend {
    fn request(
        &mut self,
        request: AgentWireRequestEnvelope,
    ) -> Result<AgentWireResponseEnvelope, AgentWireError> {
        let body = serde_json::to_vec(&request).map_err(|error| AgentWireError {
            code: "agent.http.encode".into(),
            message: error.to_string(),
            retryable: false,
        })?;
        let value = self
            .endpoint
            .request_json("POST", "/agent/wire", Some(&body))
            .map_err(http_error)?;
        serde_json::from_value(value).map_err(|error| AgentWireError {
            code: "agent.http.decode".into(),
            message: error.to_string(),
            retryable: false,
        })
    }
}

fn http_error(error: RemoteObserveError) -> AgentWireError {
    let retryable = matches!(
        error,
        RemoteObserveError::Io(_)
            | RemoteObserveError::Http {
                status: 500..=599,
                ..
            }
    );
    AgentWireError {
        code: "agent.http.transport".into(),
        message: error.to_string(),
        retryable,
    }
}

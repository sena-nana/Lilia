//! LiliaCore — product domain and application services.
//!
//! Not an Agent Runtime. AgentKit is reached only through the `AgentKitClientPort`.

pub mod application;
pub mod domain;

pub use application::{
    AgentKitClientPort, AgentKitPortError, InMemoryProductStore, NativeAgentCapabilitySnapshot,
    ProductRepository, ProductServices, SessionBindingService, UnavailableAgentKitPort,
};
pub use domain::{ensure_expected_revision, promote_agent_todo_title};

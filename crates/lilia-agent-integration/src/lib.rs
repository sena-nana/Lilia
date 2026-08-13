//! Lilia ↔ Mutsuki AgentKit anticorruption layer.
//!
//! Implements the Lilia product protocol on Mutsuki: Native Coding Agent bootstrap,
//! Credential Broker bridge, product profile assembly, Agent Wire service, and
//! AgentKit → product timeline projection.
//! Does not store product SQLite rows or Agent Runtime private coordinator state.
//! Does not talk to Claude Code / Codex official products.

mod agentkit_host;
mod anthropic_adapter;
mod credential;
mod host_backends;
mod model_turn;
mod native_runtime;
mod profile;
mod projection;
mod quota;
mod shared_services;
mod subagent;
mod wire_service;

pub use anthropic_adapter::{
    resolve_anthropic_endpoint, AnthropicMessagesAdapter, ANTHROPIC_MESSAGES_ADAPTER_ID,
    DEFAULT_ANTHROPIC_ENDPOINT, DEFAULT_ANTHROPIC_MODEL, ENV_ANTHROPIC_ENDPOINT,
};
pub use credential::{
    CredentialDescriptorView, CredentialHealthSnapshot, InMemoryProductCredentialRegistry,
    IndependentDiagnostics, ProductCredentialBridge, ProductCredentialImportInput,
    ProductCredentialLoginInput, ProductCredentialRecord, ProductCredentialRecoveryIssue,
    ProductCredentialRegistry, ProductCredentialRegistryLoad, ProductCredentialRevocationIntent,
    SqliteProductCredentialRegistry,
};
pub use model_turn::{
    live_model_adapter_eligible, resolve_model_endpoint, LiveModelDriver,
    DEFAULT_OPENAI_COMPATIBLE_ENDPOINT, ENV_MODEL_ENDPOINT,
};
pub use mutsuki_agent_runtime::SecretStore;
pub use native_runtime::{
    NativeAgentKitRuntime, NativeContextCompactionSource, NativeControlModelRequest,
    NativeControlModelResult, NativeModelRuntimeConfiguration, NativeRuntimeBootstrap,
    NativeRuntimeError, NativeRuntimeMode, NativeTurnStreamPage, SharedNativeAgentKitRuntime,
    TurnCancellationDisposition,
};
pub use profile::{
    build_product_coding_profile, build_product_coding_profile_id, profile_has_credential_refs,
    ANTHROPIC_MESSAGES_PROTOCOL_FAMILY, PRODUCT_NATIVE_CODING_PROFILE_HINT,
};
pub use projection::{project_agent_event, project_agent_events};
pub use quota::{
    KnownCapabilityLimit, NativeProviderQuotaRow, NativeQuotaSurface, QuotaApiAvailability,
};
pub use shared_services::{
    RegisteredMcpActivation, SharedCodingServicesStatus, SharedMcpResourceRead,
};
pub use subagent::NativeSubagentDefinition;
pub use wire_service::{NativeAgentWireService, NativeWireTurnResult};

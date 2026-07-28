//! Lilia ↔ AgentKit anticorruption layer.
//!
//! Owns Native Coding Agent bootstrap, Credential Broker bridge, product profile
//! assembly, and AgentKit → product timeline projection commands.
//! Does not store product SQLite rows or Agent Runtime private coordinator state.

mod anthropic_adapter;
mod credential;
mod model_turn;
mod native_runtime;
mod profile;
mod projection;
mod quota;
mod shared_services;

pub use anthropic_adapter::{
    resolve_anthropic_endpoint, AnthropicMessagesAdapter, ANTHROPIC_MESSAGES_ADAPTER_ID,
    DEFAULT_ANTHROPIC_ENDPOINT, DEFAULT_ANTHROPIC_MODEL, ENV_ANTHROPIC_ENDPOINT,
};
pub use credential::{
    CredentialDescriptorView, CredentialHealthSnapshot, IndependentDiagnostics,
    ProductCredentialBridge, ProductCredentialImportInput, ProductCredentialLoginInput,
};
pub use model_turn::{
    drive_live_model_turn_streaming, live_model_adapter_eligible, resolve_model_endpoint,
    LiveModelDriver, DEFAULT_OPENAI_COMPATIBLE_ENDPOINT, ENV_MODEL_ENDPOINT,
};
pub use native_runtime::{
    NativeAgentKitRuntime, NativeRuntimeBootstrap, NativeRuntimeError, NativeRuntimeMode,
    NativeTurnStreamPage, SharedNativeAgentKitRuntime,
};
pub use profile::{
    build_product_coding_profile, build_product_coding_profile_id, profile_has_credential_refs,
    ANTHROPIC_MESSAGES_PROTOCOL_FAMILY, PRODUCT_NATIVE_CODING_PROFILE_HINT,
};
pub use projection::{project_agent_event, project_agent_events};
pub use quota::{
    KnownCapabilityLimit, NativeProviderQuotaRow, NativeQuotaSurface, QuotaApiAvailability,
};
pub use shared_services::SharedCodingServicesStatus;

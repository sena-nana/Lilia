//! Lilia Storage — product persistence and Agent event projection store (#56).
//!
//! This crate does **not** define Agent Runtime session/tool semantics. Projection
//! repositories persist timeline / todo / artifact / pending product surfaces,
//! while an isolated opaque repository persists resumable runtime bytes.
//! Product domain repositories persist Project / Task / Binding.
//! Durable AgentKit MCP/Skills registry files under `$LILIA_HOME/config/` are
//! loaded here for Host / Shared Services.

mod agentkit_registry;
mod artifact_policy;
mod paths;
mod product;
mod runtime_state;
mod sqlite;
mod timeline;

pub use agentkit_registry::{
    load_mcp_registry, load_skills_registry, mcp_registry_path, registry_status_json,
    skills_registry_path, AgentkitMcpRegistry, AgentkitMcpRegistryEntry, AgentkitSkillPackageRef,
    AgentkitSkillsRegistry, AGENTKIT_MCP_REGISTRY_FILE, AGENTKIT_SKILLS_REGISTRY_FILE,
};
pub use artifact_policy::{
    apply_retention_for_task, evaluate_artifact, pin_artifact_row, ArtifactPolicyDecision,
    ArtifactRetentionPolicy, ARTIFACT_DEFAULT_COMPAT_UNTIL, ARTIFACT_STATUS_AVAILABLE,
    ARTIFACT_STATUS_EXPIRED, ARTIFACT_STATUS_INACCESSIBLE, ARTIFACT_STATUS_PINNED,
};
pub use paths::{
    LiliaDataPaths, AGENT_RUNTIME_DB_FILE, LEGACY_DESKTOP_DB_FILE, LILIA_HOME_ENV, PRODUCT_DB_FILE,
    PRODUCT_PROJECTIONS_DB_FILE,
};
pub use product::{LegacySessionProvenance, SqliteProductStore};
pub use runtime_state::SqliteAgentRuntimeStateStore;
pub use sqlite::SqliteTimelineProjectionStore;
pub use timeline::{
    InMemoryTimelineProjectionStore, ProjectionApplyResult, TimelineProjectionRepository,
};

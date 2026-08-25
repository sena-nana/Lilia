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
mod db;
mod paths;
mod product;
mod runtime_state;
mod sqlite;
mod timeline;

pub use agentkit_registry::{
    load_hooks_document, load_mcp_registry, load_mcp_registry_file, load_plugin_manifest,
    load_plugins_registry, load_skills_registry, mcp_registry_path, plugin_manifest_path,
    plugins_registry_path, plugins_root_path, project_hooks_document_path, registry_status_json,
    save_hooks_document, save_mcp_registry, save_plugins_registry, save_skills_registry,
    skills_registry_path, user_hooks_document_path, AgentkitHookHandler, AgentkitHooksDocument,
    AgentkitMcpRegistry, AgentkitMcpRegistryEntry, AgentkitPluginPackageRef,
    AgentkitPluginsRegistry, AgentkitSkillPackageRef, AgentkitSkillsRegistry,
    LiliaPluginContributions, LiliaPluginManifest, AGENTKIT_HOOKS_DOCUMENT_FILE,
    AGENTKIT_MCP_REGISTRY_FILE, AGENTKIT_PLUGINS_REGISTRY_FILE, AGENTKIT_SKILLS_REGISTRY_FILE,
    LILIA_PLUGIN_MANIFEST_FILE,
};
pub use artifact_policy::{
    apply_retention_for_task, evaluate_artifact, pin_artifact_row, ArtifactPolicyDecision,
    ArtifactRetentionPolicy, ARTIFACT_DEFAULT_COMPAT_UNTIL, ARTIFACT_STATUS_AVAILABLE,
    ARTIFACT_STATUS_EXPIRED, ARTIFACT_STATUS_INACCESSIBLE, ARTIFACT_STATUS_PINNED,
};
pub use db::{Db, DbError, Migration};
pub use paths::{
    LiliaDataPaths, AGENT_RUNTIME_DB_FILE, LEGACY_DESKTOP_DB_FILE, LILIA_HOME_ENV, PRODUCT_DB_FILE,
    PRODUCT_PROJECTIONS_DB_FILE,
};
pub use product::{LegacySessionProvenance, SqliteProductStore, PRODUCT_SCHEMA_VERSION};
pub use runtime_state::SqliteAgentRuntimeStateStore;
pub use sqlite::SqliteTimelineProjectionStore;
pub use timeline::{
    InMemoryTimelineProjectionStore, ProjectionApplyResult, TimelineProjectionRepository,
};

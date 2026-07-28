//! Lilia Storage — product persistence and Agent event projection store (#56).
//!
//! This crate does **not** own Agent Runtime session/tool state. Projection
//! repositories persist timeline / todo / artifact / pending product surfaces.
//! Product domain repositories persist Project / Task / Binding.
//! Legacy migration tools live here so Desktop / CLI / Service share one library.

mod artifact_policy;
mod migration;
mod paths;
mod product;
mod sqlite;
mod timeline;

pub use artifact_policy::{
    apply_retention_for_task, evaluate_artifact, pin_artifact_row, ArtifactPolicyDecision,
    ArtifactRetentionPolicy, ARTIFACT_DEFAULT_COMPAT_UNTIL, ARTIFACT_STATUS_AVAILABLE,
    ARTIFACT_STATUS_EXPIRED, ARTIFACT_STATUS_INACCESSIBLE, ARTIFACT_STATUS_PINNED,
};
pub use migration::{
    apply_compat_assets_to_agentkit_registry, load_mcp_registry, load_skills_registry,
    mcp_registry_path, planned_agentkit_session_id, preview_compat_assets, registry_status_json,
    skills_registry_path, AgentkitMcpRegistry, AgentkitMcpRegistryEntry, AgentkitSkillPackageRef,
    AgentkitSkillsRegistry, CompatApplyResult, CompatAssetPreview, LegacyMigrationTool,
    LegacySessionPlan, MigrationMode, MigrationObjectResult, MigrationReport, ObjectKind,
    AGENTKIT_MCP_REGISTRY_FILE, AGENTKIT_SKILLS_REGISTRY_FILE, LEGACY_SESSION_COMPAT_UNTIL,
};
pub use paths::{
    LiliaDataPaths, LEGACY_DESKTOP_DB_FILE, LILIA_HOME_ENV, PRODUCT_DB_FILE,
    PRODUCT_PROJECTIONS_DB_FILE,
};
pub use product::{LegacySessionProvenance, MigrationRunRecord, SqliteProductStore};
pub use sqlite::SqliteTimelineProjectionStore;
pub use timeline::{
    InMemoryTimelineProjectionStore, ProjectionApplyResult, TimelineProjectionRepository,
};

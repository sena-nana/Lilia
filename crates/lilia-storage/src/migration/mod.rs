//! Legacy Desktop SQLite → Lilia Storage migration (#47 / #56).
//!
//! Separates Product Data migration from Agent Session provenance:
//! - Project / Task / dependencies → product.db
//! - Claude / Codex `task_agent_sessions` → AgentKit bindings + legacy provenance
//!   (never forged as completed AgentKit tool state)
//! - Timeline → product_projections (readonly legacy import; pending skipped)
//! - MCP / Skills / Provider / Credential → secret-free preview + apply into
//!   AgentKit registry config under `$LILIA_HOME/config/`

mod compat_apply;
mod compat_preview;
mod report;
mod tool;

pub use compat_apply::{
    apply_compat_assets_to_agentkit_registry, load_mcp_registry, load_skills_registry,
    mcp_registry_path, registry_status_json, skills_registry_path, AgentkitMcpRegistry,
    AgentkitMcpRegistryEntry, AgentkitSkillPackageRef, AgentkitSkillsRegistry, CompatApplyResult,
    AGENTKIT_MCP_REGISTRY_FILE, AGENTKIT_SKILLS_REGISTRY_FILE,
};
pub use compat_preview::preview_compat_assets;
pub use report::{
    CompatAssetPreview, LegacySessionPlan, MigrationMode, MigrationObjectResult, MigrationReport,
    ObjectKind,
};
pub use tool::{
    planned_agentkit_session_id, LegacyMigrationTool, DESKTOP_PRODUCT_CORE_CUTOVER,
    LEGACY_SESSION_COMPAT_UNTIL,
};

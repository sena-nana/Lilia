use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationMode {
    Inspect,
    DryRun,
    Apply,
    Status,
    Rollback,
    /// Combined inspect + durable status + compat asset preview (no writes).
    Report,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Project,
    Task,
    TaskDependency,
    LegacySession,
    AgentKitBinding,
    TimelineEvent,
    CompatAsset,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationObjectResult {
    pub kind: ObjectKind,
    pub id: String,
    pub action: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacySessionPlan {
    pub task_id: String,
    pub legacy_backend: String,
    pub legacy_session_id: String,
    /// `migrated_to_agentkit` | `migrated_readonly` | `legacy_continue_until` | `skipped`
    pub disposition: String,
    pub compat_until: Option<String>,
    /// Deterministic new AgentKit session id for subsequent Native turns (not forged tool state).
    pub new_agent_session_id: Option<String>,
    pub notes: String,
}

/// Preview of MCP / Skills / Provider / Credential migration without secret material.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatAssetPreview {
    /// `mcp` | `skill` | `provider` | `credential` | `hook` | `plugin`
    pub kind: String,
    pub id: String,
    /// `map_to_agentkit` | `report_only` | `skip`
    pub disposition: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub mode: MigrationMode,
    pub legacy_db: String,
    pub product_db: String,
    pub ok: bool,
    pub projects_seen: usize,
    pub tasks_seen: usize,
    pub claude_sessions_seen: usize,
    pub codex_sessions_seen: usize,
    pub timeline_events_seen: usize,
    pub agentkit_bindings_planned: usize,
    pub objects: Vec<MigrationObjectResult>,
    pub legacy_sessions: Vec<LegacySessionPlan>,
    pub compat_assets: Vec<CompatAssetPreview>,
    pub backup_path: Option<String>,
    pub notes: Vec<String>,
    pub errors: Vec<String>,
}

impl MigrationReport {
    pub fn summary_line(&self) -> String {
        format!(
            "mode={:?} ok={} projects={} tasks={} claude={} codex={} timeline={} bindings={} assets={} errors={}",
            self.mode,
            self.ok,
            self.projects_seen,
            self.tasks_seen,
            self.claude_sessions_seen,
            self.codex_sessions_seen,
            self.timeline_events_seen,
            self.agentkit_bindings_planned,
            self.compat_assets.len(),
            self.errors.len()
        )
    }
}

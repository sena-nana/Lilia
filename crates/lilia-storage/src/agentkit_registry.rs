//! Durable AgentKit MCP / Skills registry files under `$LILIA_HOME/config/`.
//!
//! Secret-free manifests consumed by Host / Shared Services at runtime.
//! Does not import legacy Claude/Codex product data.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::LiliaDataPaths;
use lilia_contracts::{ProductError, ProductResult};

pub const AGENTKIT_MCP_REGISTRY_FILE: &str = "agentkit-mcp-registry.json";
pub const AGENTKIT_SKILLS_REGISTRY_FILE: &str = "agentkit-skills-registry.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentkitMcpRegistryEntry {
    pub server_id: String,
    pub source: String,
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Always empty on import — secrets must be re-bound as CredentialRef.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_allowlist: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub registered_from: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentkitMcpRegistry {
    pub version: u32,
    pub secret_free: bool,
    pub servers: Vec<AgentkitMcpRegistryEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentkitSkillsRegistry {
    pub version: u32,
    pub secret_free: bool,
    /// Absolute skill package directories for AgentKit SkillRoots.user.
    pub user_skill_roots: Vec<String>,
    pub packages: Vec<AgentkitSkillPackageRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentkitSkillPackageRef {
    pub skill_id: String,
    pub path: String,
    pub registered_from: String,
}

pub fn mcp_registry_path(paths: &LiliaDataPaths) -> PathBuf {
    paths.home().join("config").join(AGENTKIT_MCP_REGISTRY_FILE)
}

pub fn skills_registry_path(paths: &LiliaDataPaths) -> PathBuf {
    paths
        .home()
        .join("config")
        .join(AGENTKIT_SKILLS_REGISTRY_FILE)
}

pub fn load_mcp_registry(paths: &LiliaDataPaths) -> ProductResult<Option<AgentkitMcpRegistry>> {
    let path = mcp_registry_path(paths);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|err| ProductError::Unavailable {
        message: format!("read mcp registry: {err}"),
    })?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|err| ProductError::Unavailable {
            message: format!("parse mcp registry: {err}"),
        })
}

pub fn load_skills_registry(
    paths: &LiliaDataPaths,
) -> ProductResult<Option<AgentkitSkillsRegistry>> {
    let path = skills_registry_path(paths);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|err| ProductError::Unavailable {
        message: format!("read skills registry: {err}"),
    })?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|err| ProductError::Unavailable {
            message: format!("parse skills registry: {err}"),
        })
}

/// JSON snapshot for Host / Shared Services UI (no secrets).
pub fn registry_status_json(paths: &LiliaDataPaths) -> Value {
    let mcp = load_mcp_registry(paths).ok().flatten();
    let skills = load_skills_registry(paths).ok().flatten();
    json!({
        "mcpRegistryPath": mcp_registry_path(paths).display().to_string(),
        "skillsRegistryPath": skills_registry_path(paths).display().to_string(),
        "mcpServerCount": mcp.as_ref().map(|r| r.servers.len()).unwrap_or(0),
        "skillPackageCount": skills.as_ref().map(|r| r.packages.len()).unwrap_or(0),
        "userSkillRoots": skills.as_ref().map(|r| r.user_skill_roots.clone()).unwrap_or_default(),
        "mcpServers": mcp.map(|r| r.servers).unwrap_or_default(),
        "secretFree": true,
        "dataSource": "lilia.config.agentkit_registry",
    })
}

//! Apply MCP / Skills preview into durable AgentKit registry config (#47).
//!
//! Writes secret-free manifests under `$LILIA_HOME/config/`. Never copies env
//! tokens / cookies. Live MCP connect remains Host/runtime responsibility.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::migration::report::{CompatAssetPreview, MigrationObjectResult, ObjectKind};
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
    /// Always empty on migrate apply — secrets must be re-bound as CredentialRef.
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
    /// Absolute skill package directories imported for AgentKit SkillRoots.user.
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

#[derive(Clone, Debug, Default)]
pub struct CompatApplyResult {
    pub objects: Vec<MigrationObjectResult>,
    pub mcp_registry_path: Option<PathBuf>,
    pub skills_registry_path: Option<PathBuf>,
    pub mcp_count: usize,
    pub skill_count: usize,
}

impl CompatApplyResult {
    pub fn into_preview_assets(self) -> Vec<CompatAssetPreview> {
        let mut out = Vec::new();
        for obj in self.objects {
            out.push(CompatAssetPreview {
                kind: match obj.kind {
                    ObjectKind::CompatAsset => "mcp".into(),
                    _ => "compat".into(),
                },
                id: obj.id,
                disposition: obj.action,
                reason: obj.detail.unwrap_or_default(),
            });
        }
        out
    }
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
    serde_json::from_str(&text).map(Some).map_err(|err| ProductError::Unavailable {
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
    serde_json::from_str(&text).map(Some).map_err(|err| ProductError::Unavailable {
        message: format!("parse skills registry: {err}"),
    })
}

/// Scan legacy Claude/Codex MCP + Claude skills and write AgentKit registry files.
pub fn apply_compat_assets_to_agentkit_registry(
    paths: &LiliaDataPaths,
) -> ProductResult<CompatApplyResult> {
    paths
        .ensure_layout()
        .map_err(|err| ProductError::Unavailable {
            message: format!("ensure layout: {err}"),
        })?;
    let home = user_home();
    let mut result = CompatApplyResult::default();

    let mut mcp = AgentkitMcpRegistry {
        version: 1,
        secret_free: true,
        servers: Vec::new(),
    };
    collect_claude_mcp(paths, &mut mcp, &mut result);
    collect_codex_mcp(home.as_deref(), &mut mcp, &mut result);

    let mut skills = AgentkitSkillsRegistry {
        version: 1,
        secret_free: true,
        user_skill_roots: Vec::new(),
        packages: Vec::new(),
    };
    collect_claude_skills(home.as_deref(), &mut skills, &mut result);

    // Merge with existing registries (idempotent upsert by id).
    if let Some(existing) = load_mcp_registry(paths)? {
        for entry in existing.servers {
            if !mcp.servers.iter().any(|s| s.server_id == entry.server_id) {
                mcp.servers.push(entry);
            }
        }
    }
    if let Some(existing) = load_skills_registry(paths)? {
        for pkg in existing.packages {
            if !skills.packages.iter().any(|p| p.skill_id == pkg.skill_id) {
                skills.packages.push(pkg);
            }
        }
        for root in existing.user_skill_roots {
            if !skills.user_skill_roots.iter().any(|r| r == &root) {
                skills.user_skill_roots.push(root);
            }
        }
    }

    let mcp_path = mcp_registry_path(paths);
    write_json(&mcp_path, &mcp)?;
    result.mcp_registry_path = Some(mcp_path);
    result.mcp_count = mcp.servers.len();
    result.objects.push(MigrationObjectResult {
        kind: ObjectKind::CompatAsset,
        id: "registry:mcp".into(),
        action: "registered".into(),
        detail: Some(format!("{} server(s) → {}", mcp.servers.len(), AGENTKIT_MCP_REGISTRY_FILE)),
    });

    let skills_path = skills_registry_path(paths);
    write_json(&skills_path, &skills)?;
    result.skills_registry_path = Some(skills_path);
    result.skill_count = skills.packages.len();
    result.objects.push(MigrationObjectResult {
        kind: ObjectKind::CompatAsset,
        id: "registry:skills".into(),
        action: "registered".into(),
        detail: Some(format!(
            "{} package(s) → {}",
            skills.packages.len(),
            AGENTKIT_SKILLS_REGISTRY_FILE
        )),
    });

    Ok(result)
}

fn write_json(path: &Path, value: &impl Serialize) -> ProductResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| ProductError::Unavailable {
            message: format!("create config dir: {err}"),
        })?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|err| ProductError::Unavailable {
        message: format!("serialize registry: {err}"),
    })?;
    // Defense: never persist obvious secret-looking substrings from source configs.
    if text.contains("sk-") && text.to_ascii_lowercase().contains("api") {
        // Values should never be present; fail loud if something leaked.
        return Err(ProductError::InvalidState {
            message: "refusing to write registry that appears to embed API key material".into(),
        });
    }
    fs::write(path, text).map_err(|err| ProductError::Unavailable {
        message: format!("write registry {}: {err}", path.display()),
    })?;
    Ok(())
}

fn user_home() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn collect_claude_mcp(
    paths: &LiliaDataPaths,
    mcp: &mut AgentkitMcpRegistry,
    result: &mut CompatApplyResult,
) {
    let file = paths.home().join("config").join("claude-mcp-servers.json");
    let Ok(text) = fs::read_to_string(&file) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        result.objects.push(MigrationObjectResult {
            kind: ObjectKind::CompatAsset,
            id: "claude:config".into(),
            action: "skip".into(),
            detail: Some("Claude MCP config is not valid JSON".into()),
        });
        return;
    };
    let Some(map) = value
        .get("mcpServers")
        .or_else(|| value.get("servers"))
        .and_then(|v| v.as_object())
    else {
        return;
    };
    for (name, cfg) in map {
        let command = cfg.get("command").and_then(|v| v.as_str()).map(str::to_string);
        let args = cfg
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let url = cfg.get("url").and_then(|v| v.as_str()).map(str::to_string);
        let transport = if url.is_some() {
            "streamable_http"
        } else {
            "stdio"
        };
        let server_id = format!("claude-{name}");
        mcp.servers.push(AgentkitMcpRegistryEntry {
            server_id: server_id.clone(),
            source: "legacy.claude.mcp".into(),
            transport: transport.into(),
            command,
            args,
            env_allowlist: Vec::new(),
            url,
            registered_from: file.display().to_string(),
        });
        result.objects.push(MigrationObjectResult {
            kind: ObjectKind::CompatAsset,
            id: format!("claude:{name}"),
            action: "registered".into(),
            detail: Some("mapped to AgentKit MCP registry without env/token values".into()),
        });
    }
}

fn collect_codex_mcp(
    home: Option<&Path>,
    mcp: &mut AgentkitMcpRegistry,
    result: &mut CompatApplyResult,
) {
    let Some(home) = home else {
        return;
    };
    let file = home.join(".codex").join("config.toml");
    let Ok(text) = fs::read_to_string(&file) else {
        return;
    };
    let mut current: Option<String> = None;
    let mut command: Option<String> = None;
    let mut args: Vec<String> = Vec::new();
    let mut url: Option<String> = None;

    let flush = |mcp: &mut AgentkitMcpRegistry,
                 result: &mut CompatApplyResult,
                 name: &str,
                 command: &Option<String>,
                 args: &[String],
                 url: &Option<String>,
                 file: &Path| {
        let transport = if url.is_some() {
            "streamable_http"
        } else {
            "stdio"
        };
        let server_id = format!("codex-{name}");
        mcp.servers.push(AgentkitMcpRegistryEntry {
            server_id: server_id.clone(),
            source: "legacy.codex.mcp".into(),
            transport: transport.into(),
            command: command.clone(),
            args: args.to_vec(),
            env_allowlist: Vec::new(),
            url: url.clone(),
            registered_from: file.display().to_string(),
        });
        result.objects.push(MigrationObjectResult {
            kind: ObjectKind::CompatAsset,
            id: format!("codex:{name}"),
            action: "registered".into(),
            detail: Some("mapped to AgentKit MCP registry; OAuth/cookies not imported".into()),
        });
    };

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("[mcp_servers.") {
            if let Some(name) = rest.strip_suffix(']') {
                if let Some(prev) = current.take() {
                    flush(mcp, result, &prev, &command, &args, &url, &file);
                }
                current = Some(name.to_string());
                command = None;
                args = Vec::new();
                url = None;
            }
            continue;
        }
        if current.is_none() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("command") {
            if let Some(val) = parse_toml_string_assign(rest) {
                command = Some(val);
            }
        } else if let Some(rest) = trimmed.strip_prefix("url") {
            if let Some(val) = parse_toml_string_assign(rest) {
                url = Some(val);
            }
        } else if let Some(rest) = trimmed.strip_prefix("args") {
            if let Some(list) = rest.split_once('=').map(|(_, r)| r.trim()) {
                args = parse_toml_string_array(list);
            }
        }
    }
    if let Some(prev) = current.take() {
        flush(mcp, result, &prev, &command, &args, &url, &file);
    }
}

fn parse_toml_string_assign(rest: &str) -> Option<String> {
    let (_, rhs) = rest.split_once('=')?;
    let rhs = rhs.trim();
    if let Some(inner) = rhs.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Some(inner.to_string());
    }
    if let Some(inner) = rhs.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Some(inner.to_string());
    }
    None
}

fn parse_toml_string_array(raw: &str) -> Vec<String> {
    let trimmed = raw.trim().trim_start_matches('[').trim_end_matches(']');
    trimmed
        .split(',')
        .filter_map(|part| {
            let p = part.trim();
            p.strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| p.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .map(str::to_string)
        })
        .collect()
}

fn collect_claude_skills(
    home: Option<&Path>,
    skills: &mut AgentkitSkillsRegistry,
    result: &mut CompatApplyResult,
) {
    let Some(home) = home else {
        return;
    };
    let skills_root = home.join(".claude").join("skills");
    if !skills_root.is_dir() {
        return;
    }
    let root_str = skills_root.display().to_string();
    if !skills.user_skill_roots.iter().any(|r| r == &root_str) {
        skills.user_skill_roots.push(root_str);
    }
    let Ok(rd) = fs::read_dir(&skills_root) else {
        return;
    };
    for ent in rd.flatten() {
        let Ok(ft) = ent.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = ent.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = ent.path();
        skills.packages.push(AgentkitSkillPackageRef {
            skill_id: format!("claude-{name}"),
            path: path.display().to_string(),
            registered_from: "legacy.claude.skills".into(),
        });
        result.objects.push(MigrationObjectResult {
            kind: ObjectKind::CompatAsset,
            id: format!("claude:{name}"),
            action: "registered".into(),
            detail: Some("skill directory registered for AgentKit SkillRoots.user".into()),
        });
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_registers_claude_mcp_without_env_secrets() {
        let root = std::env::temp_dir().join("lilia-compat-apply-mcp");
        let _ = fs::remove_dir_all(&root);
        let config = root.join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("claude-mcp-servers.json"),
            r#"{"mcpServers":{"weather":{"command":"npx","args":["-y","weather"],"env":{"TOKEN":"super-secret-token-xyz"}}}}"#,
        )
        .unwrap();
        let paths = LiliaDataPaths::from_home(&root);
        let applied = apply_compat_assets_to_agentkit_registry(&paths).unwrap();
        assert!(applied.mcp_count >= 1);
        let registry = load_mcp_registry(&paths).unwrap().expect("mcp registry");
        assert!(registry.secret_free);
        let weather = registry
            .servers
            .iter()
            .find(|s| s.server_id == "claude-weather")
            .expect("weather");
        assert_eq!(weather.command.as_deref(), Some("npx"));
        assert!(weather.env_allowlist.is_empty());
        let blob = fs::read_to_string(mcp_registry_path(&paths)).unwrap();
        assert!(!blob.contains("super-secret-token-xyz"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_registers_claude_skills_root() {
        let root = std::env::temp_dir().join("lilia-compat-apply-skills");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("config")).unwrap();
        // Point HOME at fixture so ~/.claude/skills resolves under temp.
        let previous = std::env::var_os("HOME");
        std::env::set_var("HOME", &root);
        let skill_dir = root.join(".claude").join("skills").join("demo-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# demo\n").unwrap();

        let paths = LiliaDataPaths::from_home(root.join("lilia-home"));
        let applied = apply_compat_assets_to_agentkit_registry(&paths).unwrap();
        assert!(applied.skill_count >= 1);
        let registry = load_skills_registry(&paths).unwrap().expect("skills");
        assert!(registry
            .packages
            .iter()
            .any(|p| p.skill_id == "claude-demo-skill"));

        match previous {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        let _ = fs::remove_dir_all(&root);
    }
}

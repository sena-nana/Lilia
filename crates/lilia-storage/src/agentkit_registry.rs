//! Durable AgentKit MCP / Skills registry files under `$LILIA_HOME/config/`.
//!
//! Secret-free manifests consumed by Host / Shared Services at runtime.
//! Does not import legacy Claude/Codex product data.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::LiliaDataPaths;
use lilia_contracts::{ProductError, ProductResult};

pub const AGENTKIT_MCP_REGISTRY_FILE: &str = "agentkit-mcp-registry.json";
pub const AGENTKIT_SKILLS_REGISTRY_FILE: &str = "agentkit-skills-registry.json";
pub const AGENTKIT_PLUGINS_REGISTRY_FILE: &str = "agentkit-plugins-registry.json";
pub const AGENTKIT_HOOKS_DOCUMENT_FILE: &str = "agentkit-hooks.json";
pub const LILIA_PLUGIN_MANIFEST_FILE: &str = "lilia-plugin.json";
const AGENTKIT_REGISTRY_VERSION: u32 = 1;

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
    /// Environment variable names whose values are resolved from the OS Keyring.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_secret_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// HTTP header names whose values are resolved from the OS Keyring.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub header_secret_names: Vec<String>,
    pub registered_from: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentkitMcpRegistry {
    pub version: u32,
    #[serde(default)]
    pub revision: u64,
    pub secret_free: bool,
    pub servers: Vec<AgentkitMcpRegistryEntry>,
}

impl Default for AgentkitMcpRegistry {
    fn default() -> Self {
        Self {
            version: AGENTKIT_REGISTRY_VERSION,
            revision: 0,
            secret_free: true,
            servers: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentkitSkillsRegistry {
    pub version: u32,
    #[serde(default)]
    pub revision: u64,
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
    #[serde(default = "default_skill_scope")]
    pub scope: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl Default for AgentkitSkillsRegistry {
    fn default() -> Self {
        Self {
            version: AGENTKIT_REGISTRY_VERSION,
            revision: 0,
            secret_free: true,
            user_skill_roots: Vec::new(),
            packages: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentkitPluginsRegistry {
    pub version: u32,
    #[serde(default)]
    pub revision: u64,
    pub secret_free: bool,
    #[serde(default)]
    pub packages: Vec<AgentkitPluginPackageRef>,
}

impl Default for AgentkitPluginsRegistry {
    fn default() -> Self {
        Self {
            version: AGENTKIT_REGISTRY_VERSION,
            revision: 0,
            secret_free: true,
            packages: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentkitPluginPackageRef {
    pub plugin_id: String,
    pub name: String,
    pub plugin_version: String,
    #[serde(default)]
    pub description: String,
    pub path: String,
    pub package_sha256: String,
    pub registered_from: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiliaPluginManifest {
    pub schema_version: u32,
    pub plugin_id: String,
    pub name: String,
    pub plugin_version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub contributions: LiliaPluginContributions,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiliaPluginContributions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentkitHooksDocument {
    pub version: u32,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub handlers: Vec<AgentkitHookHandler>,
}

impl Default for AgentkitHooksDocument {
    fn default() -> Self {
        Self {
            version: AGENTKIT_REGISTRY_VERSION,
            revision: 0,
            enabled: false,
            handlers: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentkitHookHandler {
    pub id: String,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    #[serde(rename = "type")]
    pub handler_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_windows: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
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

pub fn plugins_registry_path(paths: &LiliaDataPaths) -> PathBuf {
    paths
        .home()
        .join("config")
        .join(AGENTKIT_PLUGINS_REGISTRY_FILE)
}

pub fn plugins_root_path(paths: &LiliaDataPaths) -> PathBuf {
    paths.home().join("plugins")
}

pub fn plugin_manifest_path(package_root: &Path) -> PathBuf {
    package_root.join(LILIA_PLUGIN_MANIFEST_FILE)
}

pub fn user_hooks_document_path(paths: &LiliaDataPaths) -> PathBuf {
    paths
        .home()
        .join("config")
        .join(AGENTKIT_HOOKS_DOCUMENT_FILE)
}

pub fn project_hooks_document_path(project_cwd: &Path) -> PathBuf {
    project_cwd.join(".lilia").join("hooks.json")
}

pub fn load_mcp_registry(paths: &LiliaDataPaths) -> ProductResult<Option<AgentkitMcpRegistry>> {
    let path = mcp_registry_path(paths);
    if !path.is_file() {
        return Ok(None);
    }
    load_mcp_registry_file(&path).map(Some)
}

pub fn load_mcp_registry_file(path: &Path) -> ProductResult<AgentkitMcpRegistry> {
    let text = fs::read_to_string(path).map_err(|err| ProductError::Unavailable {
        message: format!("read mcp registry: {err}"),
    })?;
    let registry = serde_json::from_str(&text).map_err(|err| ProductError::Unavailable {
        message: format!("parse mcp registry: {err}"),
    })?;
    validate_mcp_registry(&registry)?;
    Ok(registry)
}

pub fn save_mcp_registry(
    paths: &LiliaDataPaths,
    registry: &AgentkitMcpRegistry,
) -> ProductResult<()> {
    validate_mcp_registry(registry)?;
    let _guard = registry_write_lock()
        .lock()
        .map_err(|_| unavailable("MCP registry write lock is unavailable"))?;
    atomic_write_json(mcp_registry_path(paths), registry, "MCP registry")
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
    let registry = serde_json::from_str(&text).map_err(|err| ProductError::Unavailable {
        message: format!("parse skills registry: {err}"),
    })?;
    validate_skills_registry(&registry)?;
    Ok(Some(registry))
}

pub fn save_skills_registry(
    paths: &LiliaDataPaths,
    registry: &AgentkitSkillsRegistry,
) -> ProductResult<()> {
    validate_skills_registry(registry)?;
    let _guard = registry_write_lock()
        .lock()
        .map_err(|_| unavailable("Skills registry write lock is unavailable"))?;
    atomic_write_json(skills_registry_path(paths), registry, "Skills registry")
}

pub fn load_plugins_registry(
    paths: &LiliaDataPaths,
) -> ProductResult<Option<AgentkitPluginsRegistry>> {
    let path = plugins_registry_path(paths);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| unavailable(format!("read Plugins registry: {error}")))?;
    let registry = serde_json::from_str(&text)
        .map_err(|error| unavailable(format!("parse Plugins registry: {error}")))?;
    validate_plugins_registry(&registry)?;
    Ok(Some(registry))
}

pub fn save_plugins_registry(
    paths: &LiliaDataPaths,
    registry: &AgentkitPluginsRegistry,
) -> ProductResult<()> {
    validate_plugins_registry(registry)?;
    let _guard = registry_write_lock()
        .lock()
        .map_err(|_| unavailable("Plugins registry write lock is unavailable"))?;
    atomic_write_json(plugins_registry_path(paths), registry, "Plugins registry")
}

pub fn load_plugin_manifest(package_root: &Path) -> ProductResult<LiliaPluginManifest> {
    let path = plugin_manifest_path(package_root);
    let text = fs::read_to_string(&path)
        .map_err(|error| unavailable(format!("read Lilia Plugin manifest: {error}")))?;
    let manifest = serde_json::from_str(&text)
        .map_err(|error| unavailable(format!("parse Lilia Plugin manifest: {error}")))?;
    validate_plugin_manifest(&manifest)?;
    Ok(manifest)
}

pub fn load_hooks_document(path: &Path) -> ProductResult<Option<AgentkitHooksDocument>> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)
        .map_err(|error| unavailable(format!("read AgentKit Hooks document: {error}")))?;
    let document = serde_json::from_str(&text)
        .map_err(|error| unavailable(format!("parse AgentKit Hooks document: {error}")))?;
    validate_hooks_document(&document)?;
    Ok(Some(document))
}

pub fn save_hooks_document(path: &Path, document: &AgentkitHooksDocument) -> ProductResult<()> {
    validate_hooks_document(document)?;
    let _guard = registry_write_lock()
        .lock()
        .map_err(|_| unavailable("Hooks document write lock is unavailable"))?;
    atomic_write_json(path.to_path_buf(), document, "AgentKit Hooks document")
}

/// JSON snapshot for Host / Shared Services UI (no secrets).
pub fn registry_status_json(paths: &LiliaDataPaths) -> Value {
    let mcp = load_mcp_registry(paths).ok().flatten();
    let skills = load_skills_registry(paths).ok().flatten();
    let plugins = load_plugins_registry(paths).ok().flatten();
    json!({
        "mcpRegistryPath": mcp_registry_path(paths).display().to_string(),
        "skillsRegistryPath": skills_registry_path(paths).display().to_string(),
        "hooksDocumentPath": user_hooks_document_path(paths).display().to_string(),
        "pluginsRegistryPath": plugins_registry_path(paths).display().to_string(),
        "mcpServerCount": mcp.as_ref().map(|r| r.servers.len()).unwrap_or(0),
        "mcpRegistryRevision": mcp.as_ref().map(|r| r.revision).unwrap_or(0),
        "skillPackageCount": skills.as_ref().map(|r| r.packages.len()).unwrap_or(0),
        "pluginPackageCount": plugins.as_ref().map(|r| r.packages.len()).unwrap_or(0),
        "pluginsRegistryRevision": plugins.as_ref().map(|r| r.revision).unwrap_or(0),
        "userSkillRoots": skills.as_ref().map(|r| r.user_skill_roots.clone()).unwrap_or_default(),
        "mcpServers": mcp.map(|r| r.servers).unwrap_or_default(),
        "secretFree": true,
        "dataSource": "lilia.config.agentkit_registry",
    })
}

fn validate_plugins_registry(registry: &AgentkitPluginsRegistry) -> ProductResult<()> {
    if registry.version != AGENTKIT_REGISTRY_VERSION {
        return Err(unavailable(format!(
            "unsupported Plugins registry version {}",
            registry.version
        )));
    }
    if !registry.secret_free {
        return Err(unavailable("Plugins registry must remain secret-free"));
    }
    let mut ids = BTreeSet::new();
    for package in &registry.packages {
        validate_extension_id(&package.plugin_id, "Plugin registry package id")?;
        if !ids.insert(package.plugin_id.as_str()) {
            return Err(unavailable(format!(
                "Plugins registry contains duplicate package id `{}`",
                package.plugin_id
            )));
        }
        validate_bounded_text(&package.name, "Plugin registry name", 128, false)?;
        validate_bounded_text(
            &package.plugin_version,
            "Plugin registry version",
            64,
            false,
        )?;
        validate_bounded_text(
            &package.description,
            "Plugin registry description",
            2_048,
            true,
        )?;
        validate_bounded_text(
            &package.registered_from,
            "Plugin registry provenance",
            128,
            false,
        )?;
        let path = Path::new(&package.path);
        if package.path.trim() != package.path || !path.is_absolute() {
            return Err(unavailable(format!(
                "Plugin registry contains an invalid path for `{}`",
                package.plugin_id
            )));
        }
        if package.package_sha256.len() != 64
            || !package
                .package_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(unavailable(format!(
                "Plugin registry contains an invalid manifest digest for `{}`",
                package.plugin_id
            )));
        }
    }
    Ok(())
}

fn validate_plugin_manifest(manifest: &LiliaPluginManifest) -> ProductResult<()> {
    if manifest.schema_version != AGENTKIT_REGISTRY_VERSION {
        return Err(unavailable(format!(
            "unsupported Lilia Plugin manifest version {}",
            manifest.schema_version
        )));
    }
    validate_extension_id(&manifest.plugin_id, "Plugin id")?;
    validate_bounded_text(&manifest.name, "Plugin name", 128, false)?;
    validate_bounded_text(&manifest.plugin_version, "Plugin version", 64, false)?;
    validate_bounded_text(&manifest.description, "Plugin description", 2_048, true)?;
    validate_contribution_paths(&manifest.contributions.skills, "Skill")?;
    validate_contribution_paths(&manifest.contributions.hooks, "Hook")?;
    validate_contribution_paths(&manifest.contributions.mcp, "MCP")?;
    if manifest.contributions.skills.is_empty()
        && manifest.contributions.hooks.is_empty()
        && manifest.contributions.mcp.is_empty()
    {
        return Err(unavailable(
            "Lilia Plugin manifest must declare at least one contribution",
        ));
    }
    Ok(())
}

fn validate_contribution_paths(paths: &[String], label: &str) -> ProductResult<()> {
    let mut unique = BTreeSet::new();
    for value in paths {
        if value.is_empty()
            || value.trim() != value
            || value.len() > 512
            || value.chars().any(char::is_control)
        {
            return Err(unavailable(format!(
                "Lilia Plugin contains an invalid {label} contribution path"
            )));
        }
        let path = Path::new(value);
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(unavailable(format!(
                "Lilia Plugin {label} contribution must stay inside its package"
            )));
        }
        if !unique.insert(value) {
            return Err(unavailable(format!(
                "Lilia Plugin contains duplicate {label} contribution `{value}`"
            )));
        }
    }
    Ok(())
}

fn validate_extension_id(value: &str, label: &str) -> ProductResult<()> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 96
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(unavailable(format!("{label} is invalid")));
    }
    Ok(())
}

fn validate_bounded_text(
    value: &str,
    label: &str,
    max_len: usize,
    allow_empty: bool,
) -> ProductResult<()> {
    if (!allow_empty && value.is_empty())
        || value.trim() != value
        || value.len() > max_len
        || value.chars().any(char::is_control)
    {
        return Err(unavailable(format!("{label} is invalid")));
    }
    Ok(())
}

fn validate_mcp_registry(registry: &AgentkitMcpRegistry) -> ProductResult<()> {
    if registry.version != AGENTKIT_REGISTRY_VERSION {
        return Err(unavailable(format!(
            "unsupported MCP registry version {}",
            registry.version
        )));
    }
    if !registry.secret_free {
        return Err(unavailable("MCP registry must remain secret-free"));
    }
    let mut server_ids = BTreeSet::new();
    for server in &registry.servers {
        let server_id = server.server_id.trim();
        if server_id.is_empty() || server_id != server.server_id {
            return Err(unavailable("MCP registry contains an invalid server id"));
        }
        if !server_ids.insert(server_id) {
            return Err(unavailable(format!(
                "MCP registry contains duplicate server id `{server_id}`"
            )));
        }
        validate_unique_names(&server.env_secret_names, "environment secret")?;
        validate_unique_names(&server.header_secret_names, "header secret")?;
    }
    Ok(())
}

fn validate_skills_registry(registry: &AgentkitSkillsRegistry) -> ProductResult<()> {
    if registry.version != AGENTKIT_REGISTRY_VERSION {
        return Err(unavailable(format!(
            "unsupported Skills registry version {}",
            registry.version
        )));
    }
    if !registry.secret_free {
        return Err(unavailable("Skills registry must remain secret-free"));
    }
    let mut roots = BTreeSet::new();
    for root in &registry.user_skill_roots {
        let path = std::path::Path::new(root);
        if root.trim() != root || root.is_empty() || !path.is_absolute() {
            return Err(unavailable("Skills registry contains an invalid user root"));
        }
        if !roots.insert(root) {
            return Err(unavailable(format!(
                "Skills registry contains duplicate user root `{root}`"
            )));
        }
    }
    let mut skill_ids = BTreeSet::new();
    for package in &registry.packages {
        let skill_id = package.skill_id.trim();
        if skill_id.is_empty()
            || skill_id != package.skill_id
            || skill_id.chars().any(char::is_control)
        {
            return Err(unavailable("Skills registry contains an invalid skill id"));
        }
        if !skill_ids.insert(skill_id) {
            return Err(unavailable(format!(
                "Skills registry contains duplicate skill id `{skill_id}`"
            )));
        }
        if !matches!(package.scope.as_str(), "user" | "project") {
            return Err(unavailable(format!(
                "Skills registry contains unsupported scope `{}`",
                package.scope
            )));
        }
        let path = std::path::Path::new(&package.path);
        if package.path.trim() != package.path || !path.is_absolute() {
            return Err(unavailable(format!(
                "Skills registry contains an invalid path for `{skill_id}`"
            )));
        }
        if package.registered_from.trim().is_empty()
            || package.registered_from.chars().any(char::is_control)
        {
            return Err(unavailable(format!(
                "Skills registry contains invalid provenance for `{skill_id}`"
            )));
        }
    }
    Ok(())
}

fn validate_hooks_document(document: &AgentkitHooksDocument) -> ProductResult<()> {
    if document.version != AGENTKIT_REGISTRY_VERSION {
        return Err(unavailable(format!(
            "unsupported Hooks document version {}",
            document.version
        )));
    }
    let mut handler_ids = BTreeSet::new();
    for handler in &document.handlers {
        let id = handler.id.trim();
        if id.is_empty()
            || id != handler.id
            || id.len() > 64
            || !id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            return Err(unavailable("Hooks document contains an invalid handler id"));
        }
        if !handler_ids.insert(id) {
            return Err(unavailable(format!(
                "Hooks document contains duplicate handler id `{id}`"
            )));
        }
        if !matches!(handler.event.as_str(), "UserPromptSubmit" | "Stop") {
            return Err(unavailable(format!(
                "Hooks document contains unsupported event `{}`",
                handler.event
            )));
        }
        if handler.handler_type != "command" {
            return Err(unavailable(format!(
                "Hooks document contains unsupported handler type `{}`",
                handler.handler_type
            )));
        }
        validate_optional_hook_text(handler.matcher.as_deref(), "matcher", 256)?;
        validate_optional_hook_text(handler.command.as_deref(), "command", 8_192)?;
        validate_optional_hook_text(handler.command_windows.as_deref(), "Windows command", 8_192)?;
        validate_optional_hook_text(handler.status_message.as_deref(), "status message", 512)?;
        if handler.command.as_deref().is_none_or(str::is_empty)
            && handler.command_windows.as_deref().is_none_or(str::is_empty)
        {
            return Err(unavailable(format!(
                "Hook handler `{id}` must define a command"
            )));
        }
        if let Some(timeout) = handler.timeout_seconds {
            if !(1..=300).contains(&timeout) {
                return Err(unavailable(format!(
                    "Hook handler `{id}` timeout must be between 1 and 300 seconds"
                )));
            }
        }
    }
    Ok(())
}

fn validate_optional_hook_text(
    value: Option<&str>,
    field: &str,
    max_len: usize,
) -> ProductResult<()> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.trim() != value || value.len() > max_len || value.chars().any(char::is_control) {
        return Err(unavailable(format!(
            "Hooks document contains an invalid {field}"
        )));
    }
    Ok(())
}

fn validate_unique_names(names: &[String], label: &str) -> ProductResult<()> {
    let mut unique = BTreeSet::new();
    for name in names {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed != name || name.chars().any(char::is_control) {
            return Err(unavailable(format!(
                "MCP registry contains an invalid {label} name"
            )));
        }
        if !unique.insert(name) {
            return Err(unavailable(format!(
                "MCP registry contains duplicate {label} name `{name}`"
            )));
        }
    }
    Ok(())
}

fn atomic_write_json(
    path: PathBuf,
    value: &impl Serialize,
    label: &'static str,
) -> ProductResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| unavailable(format!("{label} path has no parent")))?;
    fs::create_dir_all(parent)
        .map_err(|error| unavailable(format!("create {label} directory: {error}")))?;
    let staging = path.with_extension("json.tmp");
    let backup = path.with_extension("json.bak");
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| unavailable(format!("serialize {label}: {error}")))?;
    bytes.push(b'\n');
    if staging.exists() {
        fs::remove_file(&staging)
            .map_err(|error| unavailable(format!("remove stale {label} staging file: {error}")))?;
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&staging)
        .map_err(|error| unavailable(format!("create {label} staging file: {error}")))?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&staging);
        return Err(unavailable(format!("write {label} staging file: {error}")));
    }
    drop(file);
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| unavailable(format!("remove stale {label} backup: {error}")))?;
    }
    if path.exists() {
        fs::rename(&path, &backup)
            .map_err(|error| unavailable(format!("back up {label}: {error}")))?;
    }
    if let Err(error) = fs::rename(&staging, &path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, &path);
        }
        let _ = fs::remove_file(&staging);
        return Err(unavailable(format!("publish {label}: {error}")));
    }
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| unavailable(format!("remove {label} backup: {error}")))?;
    }
    Ok(())
}

fn registry_write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn unavailable(message: impl Into<String>) -> ProductError {
    ProductError::Unavailable {
        message: message.into(),
    }
}

const fn default_enabled() -> bool {
    true
}

fn default_skill_scope() -> String {
    "user".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths(label: &str) -> LiliaDataPaths {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        LiliaDataPaths::from_home(std::env::temp_dir().join(format!(
            "lilia-agentkit-registry-{label}-{}-{nanos}",
            std::process::id()
        )))
    }

    fn entry(server_id: &str) -> AgentkitMcpRegistryEntry {
        AgentkitMcpRegistryEntry {
            server_id: server_id.to_owned(),
            source: "test".to_owned(),
            transport: "stdio".to_owned(),
            command: Some("fixture".to_owned()),
            args: Vec::new(),
            env_allowlist: Vec::new(),
            env_secret_names: Vec::new(),
            url: None,
            header_secret_names: Vec::new(),
            registered_from: "test".to_owned(),
            enabled: true,
        }
    }

    #[test]
    fn legacy_registry_defaults_revision_and_enabled_state() {
        let registry: AgentkitMcpRegistry = serde_json::from_str(
            r#"{"version":1,"secretFree":true,"servers":[{"serverId":"alpha","source":"test","transport":"stdio","command":"fixture","registeredFrom":"test"}]}"#,
        )
        .unwrap();
        assert_eq!(registry.revision, 0);
        assert!(registry.servers[0].enabled);
    }

    #[test]
    fn registry_save_round_trips_revision_and_replaces_previous_content() {
        let paths = temp_paths("roundtrip");
        let mut registry = AgentkitMcpRegistry {
            revision: 1,
            servers: vec![entry("alpha")],
            ..AgentkitMcpRegistry::default()
        };
        save_mcp_registry(&paths, &registry).unwrap();
        registry.revision = 2;
        registry.servers[0].enabled = false;
        save_mcp_registry(&paths, &registry).unwrap();
        assert_eq!(load_mcp_registry(&paths).unwrap(), Some(registry));
        assert!(!mcp_registry_path(&paths)
            .with_extension("json.tmp")
            .exists());
        assert!(!mcp_registry_path(&paths)
            .with_extension("json.bak")
            .exists());
        let _ = fs::remove_dir_all(paths.home());
    }

    #[test]
    fn registry_rejects_duplicate_ids_and_non_secret_free_manifests() {
        let paths = temp_paths("invalid");
        let duplicate = AgentkitMcpRegistry {
            servers: vec![entry("alpha"), entry("alpha")],
            ..AgentkitMcpRegistry::default()
        };
        assert!(save_mcp_registry(&paths, &duplicate).is_err());
        let non_secret_free = AgentkitMcpRegistry {
            secret_free: false,
            ..AgentkitMcpRegistry::default()
        };
        assert!(save_mcp_registry(&paths, &non_secret_free).is_err());
        assert!(!mcp_registry_path(&paths).exists());
        let _ = fs::remove_dir_all(paths.home());
    }

    #[test]
    fn legacy_skills_registry_defaults_revision_scope_and_enabled_state() {
        let registry: AgentkitSkillsRegistry = serde_json::from_str(
            r#"{"version":1,"secretFree":true,"userSkillRoots":["C:\\\\skills"],"packages":[{"skillId":"review","path":"C:\\\\skills\\\\review","registeredFrom":"migration"}]}"#,
        )
        .unwrap();
        assert_eq!(registry.revision, 0);
        assert_eq!(registry.packages[0].scope, "user");
        assert!(registry.packages[0].description.is_empty());
        assert!(registry.packages[0].enabled);
    }

    #[test]
    fn skills_registry_save_is_revisioned_and_rejects_duplicate_ids() {
        let paths = temp_paths("skills-roundtrip");
        let root = paths.home().join("skills");
        let package = root.join("review");
        let mut registry = AgentkitSkillsRegistry {
            revision: 1,
            user_skill_roots: vec![root.to_string_lossy().into_owned()],
            packages: vec![AgentkitSkillPackageRef {
                skill_id: "review".to_owned(),
                path: package.to_string_lossy().into_owned(),
                registered_from: "test".to_owned(),
                scope: "user".to_owned(),
                description: "Review changes".to_owned(),
                enabled: true,
            }],
            ..AgentkitSkillsRegistry::default()
        };
        save_skills_registry(&paths, &registry).unwrap();
        registry.revision = 2;
        registry.packages[0].enabled = false;
        save_skills_registry(&paths, &registry).unwrap();
        assert_eq!(
            load_skills_registry(&paths).unwrap(),
            Some(registry.clone())
        );

        registry.packages.push(registry.packages[0].clone());
        assert!(save_skills_registry(&paths, &registry).is_err());
        let _ = fs::remove_dir_all(paths.home());
    }

    #[test]
    fn hooks_document_is_revisioned_atomic_and_defaults_disabled() {
        let paths = temp_paths("hooks-roundtrip");
        let path = user_hooks_document_path(&paths);
        let mut document = AgentkitHooksDocument {
            revision: 1,
            handlers: vec![AgentkitHookHandler {
                id: "prompt-check".to_owned(),
                event: "UserPromptSubmit".to_owned(),
                matcher: None,
                handler_type: "command".to_owned(),
                command: Some("check-prompt".to_owned()),
                command_windows: None,
                timeout_seconds: Some(10),
                status_message: Some("Checking prompt".to_owned()),
            }],
            ..AgentkitHooksDocument::default()
        };
        save_hooks_document(&path, &document).unwrap();
        assert!(!load_hooks_document(&path).unwrap().unwrap().enabled);

        document.revision = 2;
        document.enabled = true;
        save_hooks_document(&path, &document).unwrap();
        assert_eq!(load_hooks_document(&path).unwrap(), Some(document));
        assert!(!path.with_extension("json.tmp").exists());
        assert!(!path.with_extension("json.bak").exists());
        let _ = fs::remove_dir_all(paths.home());
    }

    #[test]
    fn hooks_document_rejects_unknown_events_and_duplicate_handlers() {
        let paths = temp_paths("hooks-invalid");
        let path = user_hooks_document_path(&paths);
        let handler = AgentkitHookHandler {
            id: "duplicate".to_owned(),
            event: "UserPromptSubmit".to_owned(),
            matcher: None,
            handler_type: "command".to_owned(),
            command: Some("check".to_owned()),
            command_windows: None,
            timeout_seconds: None,
            status_message: None,
        };
        let duplicate = AgentkitHooksDocument {
            handlers: vec![handler.clone(), handler.clone()],
            ..AgentkitHooksDocument::default()
        };
        assert!(save_hooks_document(&path, &duplicate).is_err());
        let unknown = AgentkitHooksDocument {
            handlers: vec![AgentkitHookHandler {
                event: "PreToolUse".to_owned(),
                ..handler
            }],
            ..AgentkitHooksDocument::default()
        };
        assert!(save_hooks_document(&path, &unknown).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn plugin_registry_and_manifest_round_trip_secret_free_contributions() {
        let paths = temp_paths("plugins-roundtrip");
        let package_root = plugins_root_path(&paths).join("review-tools");
        fs::create_dir_all(package_root.join("skills/review")).unwrap();
        fs::write(
            plugin_manifest_path(&package_root),
            serde_json::to_vec_pretty(&LiliaPluginManifest {
                schema_version: 1,
                plugin_id: "review-tools".to_owned(),
                name: "Review Tools".to_owned(),
                plugin_version: "1.2.3".to_owned(),
                description: "Review extension bundle".to_owned(),
                contributions: LiliaPluginContributions {
                    skills: vec!["skills/review".to_owned()],
                    hooks: vec!["hooks.json".to_owned()],
                    mcp: vec!["mcp.json".to_owned()],
                },
            })
            .unwrap(),
        )
        .unwrap();
        let manifest = load_plugin_manifest(&package_root).unwrap();
        assert_eq!(manifest.plugin_id, "review-tools");
        assert_eq!(manifest.contributions.skills, vec!["skills/review"]);

        let registry = AgentkitPluginsRegistry {
            revision: 1,
            packages: vec![AgentkitPluginPackageRef {
                plugin_id: manifest.plugin_id,
                name: manifest.name,
                plugin_version: manifest.plugin_version,
                description: manifest.description,
                path: package_root.to_string_lossy().into_owned(),
                package_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_owned(),
                registered_from: "test".to_owned(),
                enabled: true,
            }],
            ..AgentkitPluginsRegistry::default()
        };
        save_plugins_registry(&paths, &registry).unwrap();
        assert_eq!(load_plugins_registry(&paths).unwrap(), Some(registry));
        let _ = fs::remove_dir_all(paths.home());
    }

    #[test]
    fn plugin_manifest_rejects_escape_paths_and_empty_contributions() {
        let paths = temp_paths("plugins-invalid");
        let package_root = plugins_root_path(&paths).join("unsafe");
        fs::create_dir_all(&package_root).unwrap();
        let write_manifest = |manifest: &LiliaPluginManifest| {
            fs::write(
                plugin_manifest_path(&package_root),
                serde_json::to_vec_pretty(manifest).unwrap(),
            )
            .unwrap();
        };
        let mut manifest = LiliaPluginManifest {
            schema_version: 1,
            plugin_id: "unsafe".to_owned(),
            name: "Unsafe".to_owned(),
            plugin_version: "1.0.0".to_owned(),
            description: String::new(),
            contributions: LiliaPluginContributions::default(),
        };
        write_manifest(&manifest);
        assert!(load_plugin_manifest(&package_root).is_err());
        manifest.contributions.skills = vec!["../outside".to_owned()];
        write_manifest(&manifest);
        assert!(load_plugin_manifest(&package_root).is_err());
        assert!(!plugins_registry_path(&paths).exists());
        let _ = fs::remove_dir_all(paths.home());
    }
}

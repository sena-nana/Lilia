use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::paths::{
    ensure_dir, lilia_config_dir, sanitize_extension_name, BUILTIN_CLAUDE_MCP_SERVER,
    CLAUDE_MCP_CONFIG_FILE,
};
#[cfg(any(feature = "legacy-runner", test))]
use super::types::ClaudeRuntimeMcpServer;
use super::types::{PluginMcpServer, PluginMcpServerInput};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ClaudeMcpConfigFile {
    #[serde(default)]
    servers: Vec<ClaudeMcpConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ClaudeMcpConfigEntry {
    name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    disabled: bool,
}

pub fn claude_mcp_config_path() -> PathBuf {
    lilia_config_dir().join(CLAUDE_MCP_CONFIG_FILE)
}

fn sanitize_claude_mcp_name(raw: &str) -> Result<String, String> {
    let name = sanitize_extension_name(raw, "Claude MCP server")?;
    if name.eq_ignore_ascii_case(BUILTIN_CLAUDE_MCP_SERVER) {
        return Err("Claude MCP server 名称不能使用内置名称：lilia".to_string());
    }
    Ok(name)
}

fn normalize_command(raw: &str) -> Result<String, String> {
    let command = raw.trim();
    if command.is_empty() {
        return Err("command 不能为空".to_string());
    }
    Ok(command.to_string())
}

fn normalize_args(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .map(|arg| arg.trim().to_string())
        .filter(|arg| !arg.is_empty())
        .collect()
}

fn normalize_env(env: BTreeMap<String, String>) -> BTreeMap<String, String> {
    env.into_iter()
        .filter_map(|(key, value)| {
            let key = key.trim().to_string();
            if key.is_empty() || value.is_empty() {
                None
            } else {
                Some((key, value))
            }
        })
        .collect()
}

fn apply_claude_mcp_env_update(
    current: &mut BTreeMap<String, String>,
    env: Option<BTreeMap<String, String>>,
    remove_env_keys: Vec<String>,
) {
    for key in remove_env_keys {
        current.remove(key.trim());
    }
    if let Some(env) = env {
        current.extend(normalize_env(env));
    }
}

fn sort_claude_mcp_entries(entries: &mut [ClaudeMcpConfigEntry]) {
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
}

fn read_claude_mcp_config() -> Result<ClaudeMcpConfigFile, String> {
    let path = claude_mcp_config_path();
    read_claude_mcp_config_from_path(&path)
}

fn read_claude_mcp_config_from_path(path: &Path) -> Result<ClaudeMcpConfigFile, String> {
    if !path.exists() {
        return Ok(ClaudeMcpConfigFile::default());
    }
    let text =
        fs::read_to_string(&path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(ClaudeMcpConfigFile::default());
    }
    serde_json::from_str::<ClaudeMcpConfigFile>(&text)
        .map_err(|e| format!("{} 不是合法 Claude MCP 配置：{e}", path.display()))
}

fn write_claude_mcp_config(config: &ClaudeMcpConfigFile) -> Result<(), String> {
    let path = claude_mcp_config_path();
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let mut text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    text.push('\n');
    fs::write(&path, text).map_err(|e| format!("写入 {} 失败：{e}", path.display()))
}

fn public_claude_mcp_server(entry: &ClaudeMcpConfigEntry, include_env: bool) -> PluginMcpServer {
    let mut env_keys: Vec<String> = entry.env.keys().cloned().collect();
    env_keys.sort_by_key(|key| key.to_lowercase());
    PluginMcpServer {
        backend: "claude".to_string(),
        name: entry.name.clone(),
        command: entry.command.clone(),
        args: entry.args.clone(),
        env: if include_env && !entry.env.is_empty() {
            Some(entry.env.clone())
        } else {
            None
        },
        env_keys,
        enabled: !entry.disabled,
        editable: true,
        transport: Some("stdio".to_string()),
    }
}

fn validate_claude_mcp_entries(
    mut entries: Vec<ClaudeMcpConfigEntry>,
) -> (Vec<ClaudeMcpConfigEntry>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut out = Vec::new();
    for mut entry in entries.drain(..) {
        let name = match sanitize_claude_mcp_name(&entry.name) {
            Ok(name) => name,
            Err(e) => {
                warnings.push(format!("跳过 Claude MCP server：{e}"));
                continue;
            }
        };
        let lower = name.to_lowercase();
        if let Some(existing) = seen.get(&lower) {
            warnings.push(format!(
                "跳过重复 Claude MCP server：{}（已存在 {}）",
                name, existing
            ));
            continue;
        }
        let command = match normalize_command(&entry.command) {
            Ok(command) => command,
            Err(e) => {
                warnings.push(format!("跳过 Claude MCP server {name}：{e}"));
                continue;
            }
        };
        entry.name = name.clone();
        entry.command = command;
        entry.args = normalize_args(entry.args);
        entry.env = normalize_env(entry.env);
        seen.insert(lower, name);
        out.push(entry);
    }
    sort_claude_mcp_entries(&mut out);
    (out, warnings)
}

pub fn list_claude_mcp_servers() -> (Vec<PluginMcpServer>, Vec<String>) {
    let config = match read_claude_mcp_config() {
        Ok(config) => config,
        Err(e) => return (Vec::new(), vec![e]),
    };
    let (entries, warnings) = validate_claude_mcp_entries(config.servers);
    let servers = entries
        .iter()
        .map(|entry| public_claude_mcp_server(entry, false))
        .collect();
    (servers, warnings)
}

pub fn create_claude_mcp_server(input: PluginMcpServerInput) -> Result<PluginMcpServer, String> {
    let name = sanitize_claude_mcp_name(&input.name)?;
    let command = normalize_command(&input.command)?;
    let mut config = read_claude_mcp_config()?;
    if config
        .servers
        .iter()
        .any(|server| server.name.eq_ignore_ascii_case(&name))
    {
        return Err(format!("同名 Claude MCP server 已存在：{name}"));
    }
    let entry = ClaudeMcpConfigEntry {
        name,
        command,
        args: normalize_args(input.args),
        env: normalize_env(input.env.unwrap_or_default()),
        disabled: false,
    };
    let public = public_claude_mcp_server(&entry, false);
    config.servers.push(entry);
    sort_claude_mcp_entries(&mut config.servers);
    write_claude_mcp_config(&config)?;
    Ok(public)
}

pub fn update_claude_mcp_server(
    name: &str,
    input: PluginMcpServerInput,
) -> Result<PluginMcpServer, String> {
    let current_name = sanitize_claude_mcp_name(name)?;
    let next_name = sanitize_claude_mcp_name(&input.name)?;
    let command = normalize_command(&input.command)?;
    let mut config = read_claude_mcp_config()?;
    if config.servers.iter().any(|server| {
        server.name.eq_ignore_ascii_case(&next_name)
            && !server.name.eq_ignore_ascii_case(&current_name)
    }) {
        return Err(format!("同名 Claude MCP server 已存在：{next_name}"));
    }
    let Some(server) = config
        .servers
        .iter_mut()
        .find(|server| server.name.eq_ignore_ascii_case(&current_name))
    else {
        return Err(format!("未找到 Claude MCP server：{current_name}"));
    };
    server.name = next_name;
    server.command = command;
    server.args = normalize_args(input.args);
    apply_claude_mcp_env_update(&mut server.env, input.env, input.remove_env_keys);
    let public = public_claude_mcp_server(server, false);
    sort_claude_mcp_entries(&mut config.servers);
    write_claude_mcp_config(&config)?;
    Ok(public)
}

pub fn delete_claude_mcp_server(name: &str) -> Result<(), String> {
    let name = sanitize_claude_mcp_name(name)?;
    let mut config = read_claude_mcp_config()?;
    let Some(index) = config
        .servers
        .iter()
        .position(|server| server.name.eq_ignore_ascii_case(&name))
    else {
        return Err(format!("未找到 Claude MCP server：{name}"));
    };
    config.servers.remove(index);
    write_claude_mcp_config(&config)
}

pub fn set_claude_mcp_server_enabled(name: &str, enabled: bool) -> Result<(), String> {
    let name = sanitize_claude_mcp_name(name)?;
    let mut config = read_claude_mcp_config()?;
    let Some(server) = config
        .servers
        .iter_mut()
        .find(|server| server.name.eq_ignore_ascii_case(&name))
    else {
        return Err(format!("未找到 Claude MCP server：{name}"));
    };
    server.disabled = !enabled;
    write_claude_mcp_config(&config)
}

#[cfg(feature = "legacy-runner")]
pub fn runtime_claude_mcp_servers() -> (BTreeMap<String, ClaudeRuntimeMcpServer>, Vec<String>) {
    let config = match read_claude_mcp_config() {
        Ok(config) => config,
        Err(e) => return (BTreeMap::new(), vec![e]),
    };
    runtime_claude_mcp_servers_from_config(config)
}

#[cfg(any(feature = "legacy-runner", test))]
fn runtime_claude_mcp_servers_from_config(
    config: ClaudeMcpConfigFile,
) -> (BTreeMap<String, ClaudeRuntimeMcpServer>, Vec<String>) {
    let (entries, warnings) = validate_claude_mcp_entries(config.servers);
    let mut out = BTreeMap::new();
    for entry in entries {
        if entry.disabled {
            continue;
        }
        out.insert(
            entry.name,
            ClaudeRuntimeMcpServer {
                r#type: "stdio".to_string(),
                command: entry.command,
                args: entry.args,
                env: entry.env,
            },
        );
    }
    (out, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_mcp_config_empty_when_missing() {
        let missing = std::env::temp_dir().join(format!(
            "lilia-missing-claude-mcp-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&missing);
        let config = read_claude_mcp_config_from_path(&missing).unwrap();
        assert!(config.servers.is_empty());
    }

    #[test]
    fn claude_mcp_validates_and_sorts_servers() {
        let entries = vec![
            ClaudeMcpConfigEntry {
                name: "weather".to_string(),
                command: "node".to_string(),
                args: vec!["weather.js".to_string(), "".to_string()],
                env: BTreeMap::from([
                    ("TOKEN".to_string(), "secret".to_string()),
                    ("EMPTY".to_string(), "".to_string()),
                ]),
                disabled: false,
            },
            ClaudeMcpConfigEntry {
                name: "alpha".to_string(),
                command: "uvx".to_string(),
                args: vec!["alpha".to_string()],
                env: BTreeMap::new(),
                disabled: true,
            },
        ];
        let (entries, warnings) = validate_claude_mcp_entries(entries);
        assert!(warnings.is_empty(), "warnings: {warnings:?}");
        assert_eq!(entries[0].name, "alpha");
        assert_eq!(entries[1].name, "weather");
        assert_eq!(entries[1].args, vec!["weather.js"]);
        assert_eq!(entries[1].env.len(), 1);
    }

    #[test]
    fn claude_mcp_rejects_builtin_name() {
        assert!(sanitize_claude_mcp_name("lilia").is_err());
        assert!(sanitize_claude_mcp_name("LILIA").is_err());
        assert!(sanitize_claude_mcp_name("ok_server").is_ok());
    }

    #[test]
    fn claude_mcp_public_view_hides_env_values() {
        let entry = ClaudeMcpConfigEntry {
            name: "weather".to_string(),
            command: "node".to_string(),
            args: vec![],
            env: BTreeMap::from([("TOKEN".to_string(), "secret".to_string())]),
            disabled: false,
        };
        let public = public_claude_mcp_server(&entry, false);
        assert_eq!(public.env_keys, vec!["TOKEN"]);
        assert!(public.env.is_none());

        let runtime = public_claude_mcp_server(&entry, true);
        assert_eq!(
            runtime.env.and_then(|env| env.get("TOKEN").cloned()),
            Some("secret".to_string())
        );
    }

    #[test]
    fn claude_mcp_env_patch_preserves_removes_and_overwrites_keys() {
        let mut env = BTreeMap::from([
            ("KEEP".to_string(), "old".to_string()),
            ("REMOVE".to_string(), "gone".to_string()),
            ("TOKEN".to_string(), "old-token".to_string()),
        ]);

        apply_claude_mcp_env_update(
            &mut env,
            Some(BTreeMap::from([
                ("TOKEN".to_string(), "new-token".to_string()),
                ("EMPTY".to_string(), "".to_string()),
                (" ADD ".to_string(), "fresh".to_string()),
            ])),
            vec!["REMOVE".to_string(), " ".to_string()],
        );

        assert_eq!(env.get("KEEP").map(String::as_str), Some("old"));
        assert_eq!(env.get("TOKEN").map(String::as_str), Some("new-token"));
        assert_eq!(env.get("ADD").map(String::as_str), Some("fresh"));
        assert!(!env.contains_key("REMOVE"));
        assert!(!env.contains_key("EMPTY"));
    }

    #[test]
    fn claude_mcp_runtime_returns_only_enabled_servers_with_env() {
        let config = ClaudeMcpConfigFile {
            servers: vec![
                ClaudeMcpConfigEntry {
                    name: "enabled".to_string(),
                    command: "node".to_string(),
                    args: vec!["server.js".to_string()],
                    env: BTreeMap::from([("TOKEN".to_string(), "secret".to_string())]),
                    disabled: false,
                },
                ClaudeMcpConfigEntry {
                    name: "disabled".to_string(),
                    command: "node".to_string(),
                    args: vec!["disabled.js".to_string()],
                    env: BTreeMap::new(),
                    disabled: true,
                },
            ],
        };

        let (servers, warnings) = runtime_claude_mcp_servers_from_config(config);
        assert!(warnings.is_empty(), "warnings: {warnings:?}");
        assert!(servers.contains_key("enabled"));
        assert!(!servers.contains_key("disabled"));
        let enabled = servers.get("enabled").unwrap();
        assert_eq!(enabled.r#type, "stdio");
        assert_eq!(enabled.command, "node");
        assert_eq!(enabled.args, vec!["server.js"]);
        assert_eq!(enabled.env.get("TOKEN").map(String::as_str), Some("secret"));
    }

    #[test]
    fn claude_mcp_validation_skips_lilia_with_warning() {
        let entries = vec![ClaudeMcpConfigEntry {
            name: "lilia".to_string(),
            command: "node".to_string(),
            args: vec![],
            env: BTreeMap::new(),
            disabled: false,
        }];
        let (entries, warnings) = validate_claude_mcp_entries(entries);
        assert!(entries.is_empty());
        assert!(warnings.iter().any(|warning| warning.contains("lilia")));
    }
}

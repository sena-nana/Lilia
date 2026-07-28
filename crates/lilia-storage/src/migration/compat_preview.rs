//! MCP / Skills / Provider / Credential migration preview (#47).
//!
//! Never reads or emits secret material — only names, paths, and dispositions.

use std::fs;
use std::path::{Path, PathBuf};

use crate::migration::report::CompatAssetPreview;
use crate::LiliaDataPaths;

/// Scan well-known legacy config locations and produce a secret-free preview.
pub fn preview_compat_assets(paths: &LiliaDataPaths) -> Vec<CompatAssetPreview> {
    let mut out = Vec::new();
    let home = user_home();

    preview_claude_mcp(paths, &mut out);
    preview_codex_mcp(home.as_deref(), &mut out);
    preview_claude_skills(home.as_deref(), &mut out);
    preview_provider_endpoints(paths, &mut out);
    preview_credential_env_presence(&mut out);
    preview_unsupported_hooks(home.as_deref(), &mut out);

    out.sort_by(|a, b| (&a.kind, &a.id).cmp(&(&b.kind, &b.id)));
    out
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

fn preview_claude_mcp(paths: &LiliaDataPaths, out: &mut Vec<CompatAssetPreview>) {
    let file = paths.home().join("config").join("claude-mcp-servers.json");
    if !file.is_file() {
        out.push(CompatAssetPreview {
            kind: "mcp".into(),
            id: "claude:config".into(),
            disposition: "report_only".into(),
            reason: format!(
                "no Claude MCP config at {}; nothing to map",
                file.display()
            ),
        });
        return;
    }
    let Ok(text) = fs::read_to_string(&file) else {
        out.push(CompatAssetPreview {
            kind: "mcp".into(),
            id: "claude:config".into(),
            disposition: "skip".into(),
            reason: "Claude MCP config exists but could not be read".into(),
        });
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        out.push(CompatAssetPreview {
            kind: "mcp".into(),
            id: "claude:config".into(),
            disposition: "skip".into(),
            reason: "Claude MCP config is not valid JSON".into(),
        });
        return;
    };
    let servers = value
        .get("mcpServers")
        .or_else(|| value.get("servers"))
        .and_then(|v| v.as_object());
    match servers {
        Some(map) if !map.is_empty() => {
            for name in map.keys() {
                out.push(CompatAssetPreview {
                    kind: "mcp".into(),
                    id: format!("claude:{name}"),
                    disposition: "map_to_agentkit".into(),
                    reason: "Claude MCP server name can map to AgentKit registry / Profile binding; env/token values are not imported".into(),
                });
            }
        }
        _ => out.push(CompatAssetPreview {
            kind: "mcp".into(),
            id: "claude:config".into(),
            disposition: "report_only".into(),
            reason: "Claude MCP config present but no named servers found".into(),
        }),
    }
}

fn preview_codex_mcp(home: Option<&Path>, out: &mut Vec<CompatAssetPreview>) {
    let Some(home) = home else {
        out.push(CompatAssetPreview {
            kind: "mcp".into(),
            id: "codex:config".into(),
            disposition: "report_only".into(),
            reason: "HOME unavailable; cannot inspect Codex MCP".into(),
        });
        return;
    };
    let file = home.join(".codex").join("config.toml");
    if !file.is_file() {
        out.push(CompatAssetPreview {
            kind: "mcp".into(),
            id: "codex:config".into(),
            disposition: "report_only".into(),
            reason: format!("no Codex config at {}; nothing to map", file.display()),
        });
        return;
    }
    let Ok(text) = fs::read_to_string(&file) else {
        out.push(CompatAssetPreview {
            kind: "mcp".into(),
            id: "codex:config".into(),
            disposition: "skip".into(),
            reason: "Codex config exists but could not be read".into(),
        });
        return;
    };
    let mut names = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("[mcp_servers.") {
            if let Some(name) = rest.strip_suffix(']') {
                names.push(name.to_string());
            }
        }
    }
    if names.is_empty() {
        out.push(CompatAssetPreview {
            kind: "mcp".into(),
            id: "codex:config".into(),
            disposition: "report_only".into(),
            reason: "Codex config present but no [mcp_servers.*] tables found".into(),
        });
        return;
    }
    for name in names {
        out.push(CompatAssetPreview {
            kind: "mcp".into(),
            id: format!("codex:{name}"),
            disposition: "map_to_agentkit".into(),
            reason: "Codex MCP server name can map to AgentKit registry; OAuth/cookie credentials are not imported".into(),
        });
    }
}

fn preview_claude_skills(home: Option<&Path>, out: &mut Vec<CompatAssetPreview>) {
    let Some(home) = home else {
        return;
    };
    let skills_root = home.join(".claude").join("skills");
    if !skills_root.is_dir() {
        out.push(CompatAssetPreview {
            kind: "skill".into(),
            id: "claude:user-skills".into(),
            disposition: "report_only".into(),
            reason: format!("no Claude skills dir at {}", skills_root.display()),
        });
        return;
    }
    let Ok(rd) = fs::read_dir(&skills_root) else {
        return;
    };
    let mut found = 0usize;
    for ent in rd.flatten() {
        let Ok(ft) = ent.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = ent.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        found += 1;
        out.push(CompatAssetPreview {
            kind: "skill".into(),
            id: format!("claude:{name}"),
            disposition: "map_to_agentkit".into(),
            reason: "Claude skill directory can import into AgentKit Skills registry / Profile binding".into(),
        });
    }
    if found == 0 {
        out.push(CompatAssetPreview {
            kind: "skill".into(),
            id: "claude:user-skills".into(),
            disposition: "report_only".into(),
            reason: "Claude skills directory empty".into(),
        });
    }
}

fn preview_provider_endpoints(paths: &LiliaDataPaths, out: &mut Vec<CompatAssetPreview>) {
    let config_dir = paths.home().join("config");
    // Desktop settings store is Host-owned; we only report presence of known metadata files.
    let candidates = [
        ("provider:claude-metadata", "provider.claude.json"),
        ("provider:codex-metadata", "provider.codex.json"),
        ("provider:assistant-ai-metadata", "assistant-ai.config.json"),
    ];
    let mut any = false;
    for (id, name) in candidates {
        let path = config_dir.join(name);
        if path.is_file() {
            any = true;
            out.push(CompatAssetPreview {
                kind: "provider".into(),
                id: id.into(),
                disposition: "map_to_agentkit".into(),
                reason: "Provider endpoint metadata can become a Provider Instance; API keys must be re-bound as CredentialRef".into(),
            });
        }
    }
    if !any {
        out.push(CompatAssetPreview {
            kind: "provider".into(),
            id: "provider:endpoints".into(),
            disposition: "report_only".into(),
            reason: format!(
                "no exported provider metadata under {}; Desktop store keys remain Host-local until reconfigured in Native Credential UI",
                config_dir.display()
            ),
        });
    }
}

fn preview_credential_env_presence(out: &mut Vec<CompatAssetPreview>) {
    // Only report whether well-known env *names* are set — never values.
    const ENV_CANDIDATES: &[(&str, &str)] = &[
        ("ANTHROPIC_API_KEY", "anthropic-api-key"),
        ("OPENAI_API_KEY", "openai-api-key"),
        ("CODEX_API_KEY", "codex-api-key"),
    ];
    for (env_name, id) in ENV_CANDIDATES {
        match std::env::var_os(env_name) {
            Some(val) if !val.is_empty() => out.push(CompatAssetPreview {
                kind: "credential".into(),
                id: (*id).into(),
                disposition: "map_to_agentkit".into(),
                reason: format!(
                    "{env_name} is present in process env (value not logged); import as CredentialRef / Host secret — do not copy into migration log"
                ),
            }),
            _ => out.push(CompatAssetPreview {
                kind: "credential".into(),
                id: (*id).into(),
                disposition: "report_only".into(),
                reason: format!("{env_name} not set in this process; Native Credential Broker login required for live turns"),
            }),
        }
    }
    out.push(CompatAssetPreview {
        kind: "credential".into(),
        id: "subscription:cli-refresh-cookie".into(),
        disposition: "skip".into(),
        reason: "CLI private refresh tokens / cookies / subscription credentials are not imported".into(),
    });
}

fn preview_unsupported_hooks(home: Option<&Path>, out: &mut Vec<CompatAssetPreview>) {
    let Some(home) = home else {
        return;
    };
    let settings = home.join(".claude").join("settings.json");
    if settings.is_file() {
        out.push(CompatAssetPreview {
            kind: "hook".into(),
            id: "claude:settings-hooks".into(),
            disposition: "skip".into(),
            reason: "Claude hooks / managed settings are not migrated into AgentKit public contract".into(),
        });
    }
    let plugins = home.join(".claude").join("plugins");
    if plugins.is_dir() {
        out.push(CompatAssetPreview {
            kind: "plugin".into(),
            id: "claude:marketplace-plugins".into(),
            disposition: "skip".into(),
            reason: "Claude marketplace plugins / proprietary transport are not migrated".into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_never_embeds_secret_values() {
        let previous = std::env::var_os("OPENAI_API_KEY");
        std::env::set_var("OPENAI_API_KEY", "sk-should-never-appear-in-report");
        let root = std::env::temp_dir().join("lilia-compat-preview-secret");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("config")).unwrap();
        let paths = LiliaDataPaths::from_home(&root);
        let assets = preview_compat_assets(&paths);
        let blob = serde_json::to_string(&assets).unwrap();
        assert!(!blob.contains("sk-should-never-appear-in-report"));
        assert!(assets.iter().any(|a| a.kind == "credential" && a.id == "openai-api-key"));
        match previous {
            Some(v) => std::env::set_var("OPENAI_API_KEY", v),
            None => std::env::remove_var("OPENAI_API_KEY"),
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn preview_maps_claude_mcp_names_without_env() {
        let root = std::env::temp_dir().join("lilia-compat-preview-mcp");
        let _ = fs::remove_dir_all(&root);
        let config = root.join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("claude-mcp-servers.json"),
            r#"{"mcpServers":{"weather":{"command":"npx","env":{"TOKEN":"super-secret-token-xyz"}}}}"#,
        )
        .unwrap();
        let paths = LiliaDataPaths::from_home(&root);
        let assets = preview_compat_assets(&paths);
        let weather = assets
            .iter()
            .find(|a| a.id == "claude:weather")
            .expect("weather mcp");
        assert_eq!(weather.disposition, "map_to_agentkit");
        let blob = serde_json::to_string(&assets).unwrap();
        assert!(!blob.contains("super-secret-token-xyz"));
        let _ = fs::remove_dir_all(&root);
    }
}

use std::collections::BTreeMap;

use tauri::{AppHandle, Runtime};

use super::claude_mcp::{claude_mcp_config_path, list_claude_mcp_servers};
#[cfg(feature = "legacy-runner")]
use super::claude_mcp::runtime_claude_mcp_servers;
use super::claude_plugins::list_claude_plugins;
use super::claude_skills::list_claude_skills;
use super::codex_mcp::{codex_config_path, list_codex_mcp_servers};
#[cfg(feature = "legacy-runner")]
use super::hooks::runtime_claude_hooks;
use super::paths::{SCOPE_PROJECT, SCOPE_USER};
#[cfg(feature = "legacy-runner")]
use super::types::{
    AgentRuntimeExtensions, ClaudeRuntimeExtensions, ClaudeRuntimePlugin, CodexRuntimeExtensions,
};
use super::types::{PluginSkill, PluginsOverview};

fn list_scoped_claude_skills<R: Runtime>(
    app: &AppHandle<R>,
    project_cwd: Option<&str>,
) -> (Vec<PluginSkill>, Vec<PluginSkill>, Vec<String>) {
    let mut warnings = Vec::new();
    let (user_skills, w1) = list_claude_skills(app, SCOPE_USER, None);
    warnings.extend(w1);
    let (project_skills, w2) = if project_cwd.is_some() {
        list_claude_skills(app, SCOPE_PROJECT, project_cwd)
    } else {
        (Vec::new(), Vec::new())
    };
    warnings.extend(w2);
    (user_skills, project_skills, warnings)
}

pub fn overview<R: Runtime>(app: &AppHandle<R>, project_cwd: Option<&str>) -> PluginsOverview {
    let (user_skills, project_skills, mut warnings) = list_scoped_claude_skills(app, project_cwd);
    let (user_plugins, w3) = list_claude_plugins(app, SCOPE_USER);
    warnings.extend(w3);
    let (claude_mcp_servers, w4) = list_claude_mcp_servers();
    warnings.extend(w4);
    let claude_mcp_config_path = Some(claude_mcp_config_path().to_string_lossy().to_string());
    let (codex_mcp_servers, w5) = list_codex_mcp_servers(app);
    warnings.extend(w5);
    let codex_config_path = codex_config_path(app)
        .ok()
        .map(|path| path.to_string_lossy().to_string());
    let mut skills = user_skills;
    skills.extend(project_skills);
    let mut mcp_servers = claude_mcp_servers;
    mcp_servers.extend(codex_mcp_servers);
    let mut config_paths = BTreeMap::new();
    config_paths.insert("claude".to_string(), claude_mcp_config_path);
    config_paths.insert("codex".to_string(), codex_config_path);
    PluginsOverview {
        skills,
        packages: user_plugins,
        mcp_servers,
        config_paths,
        warnings,
    }
}

#[cfg(feature = "legacy-runner")]
pub fn runtime_extensions<R: Runtime>(
    app: &AppHandle<R>,
    project_cwd: Option<&str>,
) -> AgentRuntimeExtensions {
    let (user_skills, project_skills, mut claude_warnings) =
        list_scoped_claude_skills(app, project_cwd);
    let skills = user_skills
        .into_iter()
        .chain(project_skills)
        .filter(|skill| skill.enabled)
        .map(|skill| skill.name)
        .collect();

    let (plugins, w3) = list_claude_plugins(app, SCOPE_USER);
    claude_warnings.extend(w3);
    let runtime_plugins = plugins
        .into_iter()
        .filter(|plugin| plugin.enabled)
        .map(|plugin| ClaudeRuntimePlugin {
            r#type: "local".to_string(),
            path: plugin.path,
        })
        .collect();
    let (claude_mcp_servers, mcp_warnings) = runtime_claude_mcp_servers();
    claude_warnings.extend(mcp_warnings);
    let (hooks, hook_warnings) = runtime_claude_hooks(app, project_cwd);
    claude_warnings.extend(hook_warnings);

    let config_path = codex_config_path(app).ok();
    let (mcp_servers, codex_warnings) = list_codex_mcp_servers(app);

    AgentRuntimeExtensions {
        claude: ClaudeRuntimeExtensions {
            skills,
            plugins: runtime_plugins,
            mcp_servers: claude_mcp_servers,
            hooks,
            warnings: claude_warnings,
        },
        codex: CodexRuntimeExtensions {
            mcp_servers,
            config_path: config_path.map(|path| path.to_string_lossy().to_string()),
            warnings: codex_warnings,
        },
    }
}

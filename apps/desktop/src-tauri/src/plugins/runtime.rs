use std::collections::BTreeMap;

use tauri::{AppHandle, Runtime};

use super::types::PluginsOverview;

pub fn overview<R: Runtime>(_app: &AppHandle<R>, _project_cwd: Option<&str>) -> PluginsOverview {
    PluginsOverview {
        skills: Vec::new(),
        packages: Vec::new(),
        mcp_servers: Vec::new(),
        config_paths: BTreeMap::new(),
        warnings: vec![
            "官方 Claude / Codex 插件管理已移除；扩展请通过 LiliaCore / Native AgentKit 配置。"
                .to_string(),
        ],
    }
}

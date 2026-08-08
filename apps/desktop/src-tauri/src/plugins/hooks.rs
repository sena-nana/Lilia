use tauri::{AppHandle, Runtime};

use super::types::HooksOverview;

pub fn hooks_overview<R: Runtime>(_app: &AppHandle<R>, _project_cwd: Option<&str>) -> HooksOverview {
    HooksOverview {
        sources: Vec::new(),
        warnings: vec![
            "官方 Claude / Codex Hooks 管理已移除。".to_string(),
        ],
    }
}

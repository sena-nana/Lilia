use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use lilia_desktop_application::{
    DesktopApplication, DesktopProjectCloneRequest, DesktopProjectSettings,
    DesktopWorktreeSelectionMode, DesktopWorktreeSettings,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_opener::OpenerExt;

use crate::process_command::hide_console_window;
use crate::provider::CodexProfileSettings;
use crate::settings_store::{load_store_value, save_store_value};

const PROJECT_SETTINGS_KEY: &str = "project.cloneParentDir";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitHubBindingMetadata {
    pub(crate) login: String,
    pub(crate) avatar_url: Option<String>,
    pub(crate) bound_at: i64,
    #[serde(default)]
    pub(crate) scopes: Vec<String>,
    pub(crate) client_id_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorktreeSettings {
    pub(crate) default_mode: String,
    pub(crate) parent_dir: Option<String>,
    pub(crate) auto_instructions: String,
    pub(crate) cleanup_on_archive: bool,
}

impl Default for WorktreeSettings {
    fn default() -> Self {
        Self {
            default_mode: "current".to_string(),
            parent_dir: None,
            auto_instructions: default_worktree_auto_instructions(),
            cleanup_on_archive: true,
        }
    }
}

fn default_worktree_auto_instructions() -> String {
    [
        "This task is running inside a dedicated git worktree managed by Lilia.",
        "Keep changes scoped to this task and create commits in the worktree before requesting merge/archive.",
    ]
    .join("\n")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectSettings {
    pub(crate) clone_parent_dir: Option<String>,
    #[serde(default)]
    pub(crate) codex_defaults: Option<CodexProfileSettings>,
    #[serde(default)]
    pub(crate) github_binding: Option<GitHubBindingMetadata>,
    #[serde(default)]
    pub(crate) worktree: WorktreeSettings,
}

// ---------- Project / Git ----------

/// GitHub binding / Codex defaults still live in the Tauri host store until those adapters move.
fn load_tauri_project_bindings<R: Runtime>(app: &AppHandle<R>) -> ProjectSettings {
    let settings =
        load_store_value::<ProjectSettings, _>(app, PROJECT_SETTINGS_KEY).unwrap_or_default();
    ProjectSettings {
        clone_parent_dir: None,
        worktree: WorktreeSettings::default(),
        codex_defaults: settings.codex_defaults,
        github_binding: settings.github_binding,
    }
}

fn shared_from_project_settings(settings: &ProjectSettings) -> DesktopProjectSettings {
    DesktopProjectSettings {
        clone_parent_dir: settings.clone_parent_dir.clone(),
        worktree: DesktopWorktreeSettings {
            default_mode: DesktopWorktreeSelectionMode::parse(&settings.worktree.default_mode),
            parent_dir: settings.worktree.parent_dir.clone(),
            auto_instructions: settings.worktree.auto_instructions.clone(),
            cleanup_on_archive: settings.worktree.cleanup_on_archive,
        },
    }
}

fn project_settings_from_shared(
    shared: DesktopProjectSettings,
    bindings: &ProjectSettings,
) -> ProjectSettings {
    ProjectSettings {
        clone_parent_dir: shared.clone_parent_dir,
        codex_defaults: bindings.codex_defaults.clone(),
        github_binding: bindings.github_binding.clone(),
        worktree: WorktreeSettings {
            default_mode: shared.worktree.default_mode.as_str().to_owned(),
            parent_dir: shared.worktree.parent_dir,
            auto_instructions: shared.worktree.auto_instructions,
            cleanup_on_archive: shared.worktree.cleanup_on_archive,
        },
    }
}

pub(crate) fn load_project_settings<R: Runtime>(app: &AppHandle<R>) -> ProjectSettings {
    let bindings = load_tauri_project_bindings(app);
    let Some(application) = app.try_state::<DesktopApplication>() else {
        return bindings;
    };
    match application.project_settings() {
        Ok(shared) => project_settings_from_shared(shared, &bindings),
        Err(err) => {
            eprintln!("[project-settings] load shared prefs failed: {err}");
            project_settings_from_shared(DesktopProjectSettings::default(), &bindings)
        }
    }
}

pub(crate) fn save_project_settings<R: Runtime>(
    app: &AppHandle<R>,
    settings: &ProjectSettings,
) -> Result<(), String> {
    if let Some(application) = app.try_state::<DesktopApplication>() {
        application
            .save_project_settings(shared_from_project_settings(settings))
            .map_err(|err| err.to_string())?;
    }
    // Single-write shared clone/worktree prefs via DesktopApplication.
    // Host store only keeps unfinished GitHub/Codex binding fields.
    save_store_value(
        app,
        PROJECT_SETTINGS_KEY,
        &ProjectSettings {
            clone_parent_dir: None,
            worktree: WorktreeSettings::default(),
            codex_defaults: settings.codex_defaults.clone(),
            github_binding: settings.github_binding.clone(),
        },
    )
}

#[tauri::command]
pub fn project_get_settings(app: AppHandle) -> ProjectSettings {
    load_project_settings(&app)
}

#[tauri::command]
pub fn project_set_settings(app: AppHandle, settings: ProjectSettings) -> Result<(), String> {
    save_project_settings(&app, &settings)
}

/// 从 git URL 推断仓库目录名。`https://github.com/foo/bar.git` → `bar`。
pub(crate) fn derive_repo_dir_name(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let stripped = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let last = stripped
        .rsplit(|c| c == '/' || c == ':')
        .next()
        .unwrap_or("");
    let cleaned = last.trim().trim_end_matches('/');
    if cleaned.is_empty() {
        "repo".to_string()
    } else {
        cleaned.to_string()
    }
}

/// 在已有的同级目录里挑一个不冲突的名字：`bar`、`bar-2`、`bar-3`…
pub(crate) fn unique_target_path(parent: &Path, base_name: &str) -> PathBuf {
    let candidate = parent.join(base_name);
    if !candidate.exists() {
        return candidate;
    }
    for i in 2..1024 {
        let p = parent.join(format!("{base_name}-{i}"));
        if !p.exists() {
            return p;
        }
    }
    parent.join(base_name)
}

pub(crate) fn run_git_clone(
    url: &str,
    target: &Path,
    github_auth_header: Option<&str>,
) -> Result<(), String> {
    let mut command = Command::new("git");
    hide_console_window(&mut command);
    command
        .arg("clone")
        .arg("--progress")
        .arg(url)
        .arg(target)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(header) = github_auth_header {
        command
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
            .env("GIT_CONFIG_VALUE_0", header);
    }

    let output = command
        .output()
        .map_err(|e| format!("无法启动 git（请确认 git 在 PATH 中）：{e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!(
            "git clone 失败：{}",
            if stderr.trim().is_empty() {
                format!("exit {}", output.status.code().unwrap_or(-1))
            } else {
                stderr.trim().to_string()
            }
        ));
    }

    Ok(())
}

/// 用系统默认文件管理器打开 `path` 指向的目录/文件。
/// Windows: 资源管理器；macOS: Finder；Linux: xdg-open。
#[tauri::command]
pub fn system_open_path(app: AppHandle, path: String) -> Result<(), String> {
    let p = path.trim();
    if p.is_empty() {
        return Err("路径为空".to_string());
    }
    if !Path::new(p).exists() {
        return Err(format!("路径不存在：{p}"));
    }
    app.opener()
        .open_path(p.to_string(), None::<&str>)
        .map_err(|e| format!("打开路径失败：{e}"))
}

#[tauri::command]
pub fn system_open_url(app: AppHandle, url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("URL 为空".to_string());
    }
    app.opener()
        .open_url(trimmed.to_string(), None::<&str>)
        .map_err(|e| format!("打开链接失败：{e}"))
}

/// 尝试用 VSCode 打开 `path`。
/// PATH 里依次找 `code` / `code.cmd` / `code.exe`；都找不到时返回友好错误。
#[tauri::command]
pub fn system_open_in_vscode(path: String) -> Result<(), String> {
    let p = path.trim();
    if p.is_empty() {
        return Err("路径为空".to_string());
    }
    if !Path::new(p).exists() {
        return Err(format!("路径不存在：{p}"));
    }
    let candidates: &[&str] = if cfg!(windows) {
        &["code.cmd", "code.exe", "code"]
    } else {
        &["code"]
    };
    let mut last_err: Option<String> = None;
    for cmd_name in candidates {
        let mut command = Command::new(cmd_name);
        hide_console_window(&mut command);
        match command
            .arg(p)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(_) => return Ok(()),
            Err(e) => {
                last_err = Some(e.to_string());
                continue;
            }
        }
    }
    Err(format!(
        "未能启动 VSCode（请确认 `code` 命令在 PATH 中；可在 VSCode 内执行 Shell Command: Install 'code' command in PATH）：{}",
        last_err.unwrap_or_else(|| "unknown".to_string())
    ))
}

/// 同步调用 `git clone <url> <target>`；成功后返回 target 绝对路径。
#[tauri::command]
pub fn git_clone_repo(
    application: State<'_, DesktopApplication>,
    url: String,
    parent_dir: String,
) -> Result<String, String> {
    application
        .clone_project_repository(DesktopProjectCloneRequest {
            repository: url,
            parent_directory: PathBuf::from(parent_dir.trim()),
        })
        .map(|result| result.workspace_path.to_string_lossy().into_owned())
        .map_err(|error| error.to_string())
}

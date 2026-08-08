use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use crate::process_command::hide_console_window;
use crate::store::resolve_lilia_home;
use crate::{BACKEND_CODEX, MIN_CODEX_APP_SERVER_VERSION};

use super::types::{CodexAppServerProbeStatus, CodexAppServerStatus};

static CODEX_APP_SERVER_PROBE_CACHE: OnceLock<Mutex<Option<CodexAppServerProbeStatus>>> =
    OnceLock::new();

const CODEX_FAILURE_MISSING_CLI: &str = "missingCli";
const CODEX_FAILURE_APP_SERVER_UNAVAILABLE: &str = "appServerUnavailable";
const CODEX_FAILURE_EXPERIMENTAL_API_UNSUPPORTED: &str = "experimentalApiUnsupported";
const CODEX_FAILURE_PROVIDER_INCOMPATIBLE: &str = "providerIncompatible";

pub(crate) fn cli_available(name: &str) -> bool {
    let candidates: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    for ext in candidates {
        let candidate = format!("{name}{ext}");
        let mut command = Command::new(&candidate);
        hide_console_window(&mut command);
        let ok = command
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return true;
        }
    }
    false
}

pub(crate) fn codex_candidate_filenames() -> &'static [&'static str] {
    if cfg!(windows) {
        &["codex.cmd", "codex.exe", "codex.bat", "codex"]
    } else {
        &["codex"]
    }
}

pub(crate) fn managed_codex_install_dir() -> PathBuf {
    resolve_lilia_home()
        .join("runtime")
        .join("codex")
        .join("bin")
}

pub(crate) fn managed_codex_home_dir() -> PathBuf {
    resolve_lilia_home()
        .join("runtime")
        .join("codex")
        .join("home")
}

pub(crate) fn managed_codex_paths() -> Vec<String> {
    let bin = managed_codex_install_dir();
    codex_candidate_filenames()
        .iter()
        .map(|filename| bin.join(filename).to_string_lossy().to_string())
        .collect()
}

pub(crate) fn is_managed_codex_path(path: &str) -> bool {
    let candidate = PathBuf::from(path.trim());
    candidate
        .parent()
        .is_some_and(|parent| parent == managed_codex_install_dir())
}

fn public_status(
    version: Option<String>,
    install_path: Option<String>,
    available: bool,
    supports_required_protocol: bool,
    failure_kind: Option<&str>,
    issues: Vec<String>,
) -> CodexAppServerStatus {
    let managed = install_path.as_deref().is_some_and(is_managed_codex_path);
    CodexAppServerStatus {
        version,
        install_path,
        managed,
        available,
        supports_required_protocol,
        failure_kind: failure_kind.map(ToString::to_string),
        issues,
        latest_version: None,
        update_available: false,
        release_notes: Vec::new(),
        update_error: None,
        update_state: "idle".to_string(),
        prepared_version: None,
        update_progress_percent: None,
    }
}

pub(crate) fn command_output_result(program: &str, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new(program);
    hide_console_window(&mut command);
    let output = command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !err.is_empty() { err } else { out };
        return Err(if detail.is_empty() {
            format!("command exited with {}", output.status)
        } else {
            detail
        });
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !text.is_empty() {
        return Ok(text);
    }
    let text = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if text.is_empty() {
        Err("command produced no output".to_string())
    } else {
        Ok(text)
    }
}

fn codex_app_server_help_available(help: &str) -> bool {
    help.contains("codex app-server") || help.contains("Usage:")
}

pub(crate) fn parse_codex_cli_version(output: &str) -> Option<(u32, u32, u32)> {
    let version = output
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))?;
    let mut parts = version.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u32>().ok()?;
    let patch_text = parts.next().unwrap_or("0");
    let patch_digits: String = patch_text
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    let patch = patch_digits.parse::<u32>().ok()?;
    Some((major, minor, patch))
}

pub(crate) fn codex_version_at_least(version: (u32, u32, u32), minimum: (u32, u32, u32)) -> bool {
    version >= minimum
}

pub(crate) fn build_codex_app_server_probe_status_with<F>(
    candidates: &[String],
    mut command_output: F,
) -> CodexAppServerProbeStatus
where
    F: FnMut(&str, &[&str]) -> Result<String, String>,
{
    let mut app_server_unavailable: Option<CodexAppServerProbeStatus> = None;
    let mut experimental_unsupported: Option<CodexAppServerProbeStatus> = None;

    for candidate in candidates {
        let version = match command_output(candidate, &["--version"]) {
            Ok(output) => output,
            Err(_) => continue,
        };
        let path = Some(candidate.clone());
        let parsed_version = parse_codex_cli_version(&version);
        let help = match command_output(candidate, &["app-server", "--help"]) {
            Ok(output) => output,
            Err(_) => {
                if app_server_unavailable.is_none() {
                    app_server_unavailable = Some(CodexAppServerProbeStatus {
                        public: public_status(
                            Some(version),
                            path.clone(),
                            false,
                            false,
                            Some(CODEX_FAILURE_APP_SERVER_UNAVAILABLE),
                            vec!["当前 codex CLI 不支持 app-server 子命令。".to_string()],
                        ),
                        path,
                    });
                }
                continue;
            }
        };
        let available = codex_app_server_help_available(&help);
        if !available {
            if app_server_unavailable.is_none() {
                app_server_unavailable = Some(CodexAppServerProbeStatus {
                    public: public_status(
                        Some(version),
                        path.clone(),
                        false,
                        false,
                        Some(CODEX_FAILURE_APP_SERVER_UNAVAILABLE),
                        vec!["当前 codex CLI 不支持 app-server 子命令。".to_string()],
                    ),
                    path,
                });
            }
            continue;
        }

        let version_supported = parsed_version
            .map(|version| codex_version_at_least(version, MIN_CODEX_APP_SERVER_VERSION))
            .unwrap_or(false);
        if version_supported {
            return CodexAppServerProbeStatus {
                public: public_status(Some(version), path.clone(), true, true, None, Vec::new()),
                path,
            };
        }

        if experimental_unsupported.is_none() {
            let issue = if parsed_version.is_none() {
                "无法读取 codex CLI 版本，无法确认 experimental API 协议能力。".to_string()
            } else {
                "当前 codex CLI 版本过低，需要 0.136.0 或更新版本以支持 Lilia 的流式事件、工具审批、AskUser 和官方额度重置次数查询。".to_string()
            };
            experimental_unsupported = Some(CodexAppServerProbeStatus {
                public: public_status(
                    Some(version),
                    path.clone(),
                    true,
                    false,
                    Some(CODEX_FAILURE_EXPERIMENTAL_API_UNSUPPORTED),
                    vec![issue],
                ),
                path,
            });
        }
    }

    if let Some(status) = experimental_unsupported {
        return status;
    }
    if let Some(status) = app_server_unavailable {
        return status;
    }

    CodexAppServerProbeStatus {
        public: public_status(
            None,
            None,
            false,
            false,
            Some(CODEX_FAILURE_MISSING_CLI),
            vec![
                "未找到 Lilia 内置 Codex app-server。请在 Provider 设置中安装或更新。".to_string(),
            ],
        ),
        path: None,
    }
}

pub(crate) fn build_codex_app_server_probe_status() -> CodexAppServerProbeStatus {
    build_codex_app_server_probe_status_with(&managed_codex_paths(), command_output_result)
}

pub(crate) fn build_codex_app_server_probe_status_cached(
    force_refresh: bool,
) -> CodexAppServerProbeStatus {
    let cache = CODEX_APP_SERVER_PROBE_CACHE.get_or_init(|| Mutex::new(None));
    build_codex_app_server_probe_status_cached_with(cache, force_refresh, || {
        build_codex_app_server_probe_status()
    })
}

fn build_codex_app_server_probe_status_cached_with<F>(
    cache: &Mutex<Option<CodexAppServerProbeStatus>>,
    force_refresh: bool,
    mut probe: F,
) -> CodexAppServerProbeStatus
where
    F: FnMut() -> CodexAppServerProbeStatus,
{
    if !force_refresh {
        if let Ok(guard) = cache.lock() {
            if let Some(status) = guard.clone() {
                return status;
            }
        }
    }
    let status = probe();
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(status.clone());
    }
    status
}

pub(crate) fn codex_send_block_reason(status: &CodexAppServerStatus) -> Option<String> {
    if status.supports_required_protocol {
        return None;
    }

    let detail = if status.issues.is_empty() {
        "Codex app-server 环境不满足。".to_string()
    } else {
        status.issues.join(" ")
    };
    if status.failure_kind.as_deref() == Some(CODEX_FAILURE_PROVIDER_INCOMPATIBLE) {
        return Some(format!(
            "{detail} 请确认当前 API 来源支持 OpenAI Responses API 与 Codex 模型白名单。"
        ));
    }

    Some(format!(
        "{detail} 请在 Provider 设置中安装或更新 Lilia 内置 Codex app-server 后重新检测。"
    ))
}

pub(crate) fn validate_backend_ready_for_send(active_backend: &str) -> Result<(), String> {
    // Native AgentKit execution does not depend on official Codex app-server.
    // Brand labels (claude/codex) remain product provider scope only.
    if crate::native_agent::resolve_execution_backend()
        == crate::native_agent::ExecutionBackend::NativeAgentkit
    {
        return Ok(());
    }
    if active_backend != BACKEND_CODEX {
        return Ok(());
    }
    let status = build_codex_app_server_probe_status_cached(false);
    if let Some(reason) = codex_send_block_reason(&status.public) {
        return Err(reason);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn supported_status(path: &str) -> CodexAppServerProbeStatus {
        CodexAppServerProbeStatus {
            public: public_status(
                Some("codex 0.136.0".to_string()),
                Some(path.to_string()),
                true,
                true,
                None,
                Vec::new(),
            ),
            path: Some(path.to_string()),
        }
    }

    #[test]
    fn native_execution_skips_codex_app_server_send_gate() {
        let previous = std::env::var("LILIA_AGENT_EXECUTION_BACKEND").ok();
        std::env::remove_var("LILIA_AGENT_EXECUTION_BACKEND");
        // Brand label is codex; execution is still Native — must not require app-server.
        assert!(validate_backend_ready_for_send(BACKEND_CODEX).is_ok());
        match previous {
            Some(value) => std::env::set_var("LILIA_AGENT_EXECUTION_BACKEND", value),
            None => std::env::remove_var("LILIA_AGENT_EXECUTION_BACKEND"),
        }
    }

    #[test]
    fn parse_codex_cli_version_accepts_missing_patch_and_suffix() {
        assert_eq!(parse_codex_cli_version("codex 0.128.1"), Some((0, 128, 1)));
        assert_eq!(parse_codex_cli_version("codex 0.129"), Some((0, 129, 0)));
        assert_eq!(
            parse_codex_cli_version("codex 0.128.0-beta.1"),
            Some((0, 128, 0))
        );
        assert_eq!(parse_codex_cli_version("codex unknown"), None);
    }

    #[test]
    fn codex_probe_reports_empty_candidates_as_missing_cli() {
        let status =
            build_codex_app_server_probe_status_with(&[], |_, _| Err("unexpected".to_string()));

        assert!(!status.public.available);
        assert!(!status.public.supports_required_protocol);
        assert!(status.path.is_none());
        assert_eq!(
            status.public.failure_kind.as_deref(),
            Some(CODEX_FAILURE_MISSING_CLI)
        );
        assert!(status.public.issues[0].contains("Lilia 内置 Codex app-server"));
    }

    #[test]
    fn codex_probe_picks_supported_candidate() {
        let candidates = vec!["old".to_string(), "new".to_string()];
        let status =
            build_codex_app_server_probe_status_with(&candidates, |program, args| {
                match (program, args) {
                    ("old", ["--version"]) => Ok("codex 0.127.0".to_string()),
                    ("old", ["app-server", "--help"]) => Ok("Usage: codex app-server".to_string()),
                    ("new", ["--version"]) => Ok("codex 0.136.0".to_string()),
                    ("new", ["app-server", "--help"]) => Ok("Usage: codex app-server".to_string()),
                    _ => Err("unknown command".to_string()),
                }
            });

        assert_eq!(status.path.as_deref(), Some("new"));
        assert!(status.public.supports_required_protocol);
        assert!(status.public.failure_kind.is_none());
    }

    #[test]
    fn codex_probe_reports_old_cli_as_experimental_api_unsupported() {
        let candidates = vec!["old".to_string()];
        let status = build_codex_app_server_probe_status_with(&candidates, |_, args| match args {
            ["--version"] => Ok("codex 0.127.0".to_string()),
            ["app-server", "--help"] => Ok("Usage: codex app-server".to_string()),
            _ => Err("unknown command".to_string()),
        });

        assert_eq!(status.path.as_deref(), Some("old"));
        assert!(status.public.available);
        assert!(!status.public.supports_required_protocol);
        assert_eq!(
            status.public.failure_kind.as_deref(),
            Some(CODEX_FAILURE_EXPERIMENTAL_API_UNSUPPORTED)
        );
        assert!(status.public.issues.join(" ").contains("版本过低"));
    }

    #[test]
    fn codex_probe_reports_missing_app_server_subcommand() {
        let candidates = vec!["codex".to_string()];
        let status = build_codex_app_server_probe_status_with(&candidates, |_, args| match args {
            ["--version"] => Ok("codex 0.136.0".to_string()),
            ["app-server", "--help"] => Err("unknown subcommand".to_string()),
            _ => Err("unknown command".to_string()),
        });

        assert_eq!(status.path.as_deref(), Some("codex"));
        assert!(!status.public.available);
        assert!(!status.public.supports_required_protocol);
        assert_eq!(
            status.public.failure_kind.as_deref(),
            Some(CODEX_FAILURE_APP_SERVER_UNAVAILABLE)
        );
        assert!(status.public.issues.join(" ").contains("app-server"));
    }

    #[test]
    fn codex_probe_cache_reuses_status_until_force_refresh() {
        let cache = Mutex::new(None);
        let calls = Cell::new(0);

        let first = build_codex_app_server_probe_status_cached_with(&cache, false, || {
            calls.set(calls.get() + 1);
            supported_status("first")
        });
        let second = build_codex_app_server_probe_status_cached_with(&cache, false, || {
            calls.set(calls.get() + 1);
            supported_status("second")
        });

        assert_eq!(calls.get(), 1);
        assert_eq!(first.path.as_deref(), Some("first"));
        assert_eq!(second.path.as_deref(), Some("first"));

        let refreshed = build_codex_app_server_probe_status_cached_with(&cache, true, || {
            calls.set(calls.get() + 1);
            supported_status("second")
        });

        assert_eq!(calls.get(), 2);
        assert_eq!(refreshed.path.as_deref(), Some("second"));
        assert_eq!(
            cache
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|status| status.path.as_deref()),
            Some("second")
        );
    }
}

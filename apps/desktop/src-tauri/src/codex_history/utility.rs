use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::Value as JsonValue;
use tauri::{AppHandle, Manager, Runtime};

use crate::process_command::hide_console_window;
use crate::provider::{resolve_connection_for, validate_backend_ready_for_send, ConnectionMode};
use crate::BACKEND_CODEX;

use super::types::CodexHistoryUtilityOutput;

/// #47 LEGACY — locate `codex-history.mjs` under `apps/desktop/legacy/`.
fn locate_codex_history_utility<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("codex-history.mjs"));
            candidates.push(dir.join("legacy").join("codex-history.mjs"));
            candidates.push(dir.join("../../../legacy/codex-history.mjs"));
        }
    }
    if let Ok(res) = app.path().resource_dir() {
        candidates.push(res.join("codex-history.mjs"));
        candidates.push(res.join("legacy").join("codex-history.mjs"));
    }
    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    candidates
        .into_iter()
        .last()
        .unwrap_or_else(|| PathBuf::from("legacy/codex-history.mjs"))
}

pub(super) fn run_codex_history_utility(
    app: &AppHandle<impl Runtime>,
    payload: JsonValue,
) -> Result<CodexHistoryUtilityOutput, String> {
    eprintln!(
        "[legacy-history] Codex history utility (compat until {})",
        crate::native_agent::LEGACY_NODE_RUNNER_COMPAT_UNTIL
    );
    validate_backend_ready_for_send(BACKEND_CODEX)?;
    let script = locate_codex_history_utility(app);
    let connection = resolve_connection_for(app, BACKEND_CODEX);

    let mut cmd = Command::new("node");
    hide_console_window(&mut cmd);
    cmd.arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if connection.mode == ConnectionMode::CodexAccount {
        cmd.env_remove("OPENAI_BASE_URL");
        cmd.env_remove("OPENAI_API_KEY");
        cmd.env_remove("CODEX_API_KEY");
    }
    if let Some(url) = connection.base_url {
        cmd.env("OPENAI_BASE_URL", url);
    }
    if let Some(key) = connection.api_key {
        cmd.env("OPENAI_API_KEY", key);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("无法启动 Codex history utility：{e}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        let mut bytes = serde_json::to_vec(&payload)
            .map_err(|e| format!("Codex history payload 序列化失败：{e}"))?;
        bytes.push(b'\n');
        stdin
            .write_all(&bytes)
            .map_err(|e| format!("写入 Codex history utility 失败：{e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("等待 Codex history utility 失败：{e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| {
            let detail = stderr.trim();
            if detail.is_empty() {
                "Codex history utility 没有返回数据".to_string()
            } else {
                format!("Codex history utility 没有返回数据：{detail}")
            }
        })?;
    let result: CodexHistoryUtilityOutput = serde_json::from_str(line)
        .map_err(|e| format!("解析 Codex history utility 输出失败：{e}"))?;
    if let Some(error) = result.error.as_ref().filter(|s| !s.trim().is_empty()) {
        return Err(error.clone());
    }
    if !output.status.success() {
        let detail = stderr.trim();
        return Err(if detail.is_empty() {
            "Codex history utility 异常退出".to_string()
        } else {
            format!("Codex history utility 异常退出：{detail}")
        });
    }
    Ok(result)
}

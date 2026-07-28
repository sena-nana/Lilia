use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use tauri::{AppHandle, Manager, Runtime};

use super::types::ClaudeHistoryUtilityOutput;
use crate::process_command::hide_console_window;

/// #47 LEGACY — locate `claude-history.mjs` under `apps/desktop/legacy/`.
fn locate_claude_history_utility<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("claude-history.mjs"));
            candidates.push(dir.join("legacy").join("claude-history.mjs"));
            candidates.push(dir.join("../../../legacy/claude-history.mjs"));
        }
    }
    if let Ok(res) = app.path().resource_dir() {
        candidates.push(res.join("claude-history.mjs"));
        candidates.push(res.join("legacy").join("claude-history.mjs"));
    }
    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    candidates
        .into_iter()
        .last()
        .unwrap_or_else(|| PathBuf::from("legacy/claude-history.mjs"))
}

pub(super) fn run_claude_history_utility(
    app: &AppHandle,
    payload: serde_json::Value,
) -> Result<ClaudeHistoryUtilityOutput, String> {
    eprintln!(
        "[legacy-history] Claude history utility (compat until {})",
        crate::native_agent::LEGACY_NODE_RUNNER_COMPAT_UNTIL
    );
    let script = locate_claude_history_utility(app);
    let node = std::env::var("LILIA_NODE_BIN").unwrap_or_else(|_| "node".to_string());
    let mut command = Command::new(node);
    hide_console_window(&mut command);
    let mut child = command
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("无法启动 Claude history utility：{e}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        let line = serde_json::to_string(&payload)
            .map_err(|e| format!("Claude history payload 序列化失败：{e}"))?;
        stdin
            .write_all(format!("{line}\n").as_bytes())
            .map_err(|e| format!("写入 Claude history utility 失败：{e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("等待 Claude history utility 失败：{e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| {
            let detail = stderr.trim();
            if detail.is_empty() {
                "Claude history utility 没有返回数据".to_string()
            } else {
                format!("Claude history utility 没有返回数据：{detail}")
            }
        })?;
    let result: ClaudeHistoryUtilityOutput = serde_json::from_str(line)
        .map_err(|e| format!("解析 Claude history utility 输出失败：{e}"))?;
    if let Some(error) = result.error.clone() {
        return Err(error);
    }
    if !output.status.success() {
        let detail = stderr.trim();
        return Err(if detail.is_empty() {
            "Claude history utility 异常退出".to_string()
        } else {
            format!("Claude history utility 异常退出：{detail}")
        });
    }
    Ok(result)
}

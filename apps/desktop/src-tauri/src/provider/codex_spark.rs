use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};

#[cfg(feature = "legacy-runner")]
use crate::process_command::hide_console_window;
use crate::BACKEND_CODEX;

use super::config::{
    load_active_backend, load_assistant_ai_config, load_router_mode, ROUTER_CODEX_ACCOUNT,
};

pub(crate) const CODEX_SPARK_MODEL: &str = "gpt-5.3-codex-spark";
pub(crate) const CODEX_SPARK_BASE_URL: &str = "codex-account://spark";

const DEFAULT_SPARK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexSparkPromptCommand<'a> {
    kind: &'static str,
    prompt: &'a str,
    instruction: &'a str,
    timeout_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexSparkPromptOutput {
    ok: bool,
    text: Option<String>,
    error: Option<String>,
}

pub(crate) fn codex_account_spark_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    let config = load_assistant_ai_config(app);
    config.codex_account_spark_enabled
        && load_active_backend(app) == BACKEND_CODEX
        && load_router_mode(app, BACKEND_CODEX) == ROUTER_CODEX_ACCOUNT
}

pub(crate) fn is_codex_account_spark_request(
    backend: Option<&str>,
    model: &str,
    base_url: &str,
) -> bool {
    backend == Some(BACKEND_CODEX) && model == CODEX_SPARK_MODEL && base_url == CODEX_SPARK_BASE_URL
}

pub(crate) fn request_codex_account_spark<R: Runtime>(
    app: &AppHandle<R>,
    prompt: &str,
    instruction: &str,
) -> Result<String, String> {
    request_codex_account_spark_with_timeout(app, prompt, instruction, DEFAULT_SPARK_TIMEOUT)
}

fn request_codex_account_spark_with_timeout<R: Runtime>(
    app: &AppHandle<R>,
    prompt: &str,
    instruction: &str,
    timeout: Duration,
) -> Result<String, String> {
    // #47 — default Native path never locates/starts Node agent-runner.
    // Node escape hatch requires Cargo feature `legacy-runner` + explicit env.
    match crate::native_agent::resolve_execution_backend() {
        crate::native_agent::ExecutionBackend::NativeAgentkit => {
            let _ = (app, prompt, instruction, timeout);
            Err(format!(
            "Codex Spark via Node agent-runner is retired on the default Native path \
             (compat until {}; rebuild with `--features legacy-runner` and set \
             LILIA_AGENT_EXECUTION_BACKEND=node for explicit legacy)",
            crate::native_agent::LEGACY_NODE_RUNNER_COMPAT_UNTIL
            ))
        }
        #[cfg(feature = "legacy-runner")]
        crate::native_agent::ExecutionBackend::NodeAgentRunner => {
            request_codex_account_spark_via_node_runner(app, prompt, instruction, timeout)
        }
    }
}

/// #47 LEGACY — Node agent-runner Codex Spark (explicit env + feature only, until 1.0.0).
#[cfg(feature = "legacy-runner")]
fn request_codex_account_spark_via_node_runner<R: Runtime>(
    app: &AppHandle<R>,
    prompt: &str,
    instruction: &str,
    timeout: Duration,
) -> Result<String, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Instant;

    use crate::chat::runner::locate_agent_runner;

    eprintln!(
        "[legacy-agent-runner] Codex Spark using Node agent-runner (compat until {})",
        crate::native_agent::LEGACY_NODE_RUNNER_COMPAT_UNTIL
    );
    let runner = locate_agent_runner(app);
    let payload = CodexSparkPromptCommand {
        kind: "codex_spark_prompt",
        prompt,
        instruction,
        timeout_ms: timeout.as_millis().min(u64::MAX as u128) as u64,
    };
    let mut command = Command::new("node");
    hide_console_window(&mut command);
    let mut child = command
        .arg(&runner)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("无法启动 Codex Spark runner：{err}"))?;

    let input =
        serde_json::to_vec(&payload).map_err(|err| format!("Codex Spark 请求序列化失败：{err}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&input)
            .and_then(|_| stdin.write_all(b"\n"))
            .map_err(|err| format!("写入 Codex Spark runner stdin 失败：{err}"))?;
    }

    let started = Instant::now();
    loop {
        if started.elapsed() > timeout {
            let _ = child.kill();
            return Err("Codex Spark runner 超时".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                if let Some(mut out) = child.stdout.take() {
                    use std::io::Read;
                    let _ = out.read_to_string(&mut stdout);
                }
                let mut stderr = String::new();
                if let Some(mut err) = child.stderr.take() {
                    use std::io::Read;
                    let _ = err.read_to_string(&mut stderr);
                }
                if !status.success() {
                    return Err(format!(
                        "Codex Spark runner 失败：{status}; stderr={stderr}"
                    ));
                }
                let parsed: CodexSparkPromptOutput = serde_json::from_str(stdout.trim())
                    .map_err(|err| format!("解析 Codex Spark 输出失败：{err}; stdout={stdout}"))?;
                if !parsed.ok {
                    return Err(parsed
                        .error
                        .unwrap_or_else(|| "Codex Spark 返回失败".into()));
                }
                return parsed
                    .text
                    .filter(|t| !t.trim().is_empty())
                    .ok_or_else(|| "Codex Spark 返回空文本".into());
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(err) => return Err(format!("等待 Codex Spark runner 失败：{err}")),
        }
    }
}

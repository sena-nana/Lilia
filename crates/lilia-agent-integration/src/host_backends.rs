use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use mutsuki_agent_bundle::NativeCodingBackends;
use mutsuki_agent_contracts::{
    AgentError, BrowserNavigateRequest, ProcessExecRequest, ProcessExecResult,
};
use mutsuki_agent_plugin_computer_use::{
    BrowserBackend, ProcessBackend, WorkspaceFilesystemBackend,
};
use mutsuki_agent_plugin_git::CliGitBackend;
use mutsuki_agent_plugin_lsp::StdioLspProcessFactory;
use mutsuki_agent_plugin_mcp::{CompositeMcpTransportFactory, McpHttpClient};
use reqwest::blocking::Client;
use serde_json::Value;

struct ActiveProcess {
    child: Mutex<Child>,
    cancelled: AtomicBool,
}

/// Local Host process capability used by the shared Computer Use service.
///
/// AgentKit owns the approval plan and handle identity; this backend owns the
/// OS process and makes that handle cancellable.
#[derive(Default)]
pub(crate) struct HostProcessBackend {
    active: Mutex<BTreeMap<String, Arc<ActiveProcess>>>,
}

impl ProcessBackend for HostProcessBackend {
    fn exec(
        &self,
        handle_id: &str,
        request: &ProcessExecRequest,
    ) -> Result<ProcessExecResult, AgentError> {
        let root = std::fs::canonicalize(&request.workspace.root)
            .map_err(|error| AgentError::new("lilia.host.workspace", error.to_string()))?;
        if !root.is_dir() {
            return Err(AgentError::invalid_input(
                "process workspace root must be a directory",
            ));
        }

        let mut command = Command::new(&request.command);
        command
            .args(&request.args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| AgentError::new("lilia.host.process.spawn", error.to_string()))?;

        if let Some(input) = request.stdin.as_deref() {
            let mut stdin = child.stdin.take().ok_or_else(|| {
                AgentError::new("lilia.host.process.stdin", "process stdin is unavailable")
            })?;
            stdin
                .write_all(input.as_bytes())
                .map_err(|error| AgentError::new("lilia.host.process.stdin", error.to_string()))?;
        }
        drop(child.stdin.take());

        let stdout = child.stdout.take().ok_or_else(|| {
            AgentError::new("lilia.host.process.stdout", "process stdout is unavailable")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            AgentError::new("lilia.host.process.stderr", "process stderr is unavailable")
        })?;
        let stdout_reader = thread::spawn(move || read_stream(stdout));
        let stderr_reader = thread::spawn(move || read_stream(stderr));

        let active = Arc::new(ActiveProcess {
            child: Mutex::new(child),
            cancelled: AtomicBool::new(false),
        });
        self.active
            .lock()
            .expect("host process registry mutex")
            .insert(handle_id.to_string(), Arc::clone(&active));

        let timeout = Duration::from_millis(request.limits.timeout_ms.max(1));
        let started = Instant::now();
        let status = loop {
            if let Some(status) = active
                .child
                .lock()
                .expect("host child mutex")
                .try_wait()
                .map_err(|error| AgentError::new("lilia.host.process.wait", error.to_string()))?
            {
                break status;
            }
            if started.elapsed() >= timeout {
                active.cancelled.store(true, Ordering::Release);
                let mut child = active.child.lock().expect("host child mutex");
                let _ = child.kill();
                break child.wait().map_err(|error| {
                    AgentError::new("lilia.host.process.wait", error.to_string())
                })?;
            }
            thread::sleep(Duration::from_millis(5));
        };

        self.active
            .lock()
            .expect("host process registry mutex")
            .remove(handle_id);
        let stdout = join_reader(stdout_reader)?;
        let stderr = join_reader(stderr_reader)?;
        let limit = usize::try_from(request.limits.max_output_bytes)
            .unwrap_or(usize::MAX)
            .max(1);
        let mut combined = stdout;
        if !stderr.is_empty() {
            if !combined.is_empty() {
                combined.push(b'\n');
            }
            combined.extend_from_slice(&stderr);
        }
        let truncated = combined.len() > limit;
        combined.truncate(limit);

        Ok(ProcessExecResult {
            exit_code: status.code().unwrap_or(-1),
            summary: String::from_utf8_lossy(&combined).into_owned(),
            stdout_ref: None,
            stderr_ref: None,
            truncated,
            cancelled: active.cancelled.load(Ordering::Acquire),
        })
    }

    fn cancel(&self, handle_id: &str) -> Result<(), AgentError> {
        let active = self
            .active
            .lock()
            .expect("host process registry mutex")
            .get(handle_id)
            .cloned();
        if let Some(active) = active {
            active.cancelled.store(true, Ordering::Release);
            active
                .child
                .lock()
                .expect("host child mutex")
                .kill()
                .map_err(|error| AgentError::new("lilia.host.process.cancel", error.to_string()))?;
        }
        Ok(())
    }
}

fn read_stream(mut stream: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn join_reader(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
) -> Result<Vec<u8>, AgentError> {
    reader
        .join()
        .map_err(|_| AgentError::new("lilia.host.process.read", "process reader panicked"))?
        .map_err(|error| AgentError::new("lilia.host.process.read", error.to_string()))
}

#[derive(Default)]
pub(crate) struct HostHttpBackend;

impl BrowserBackend for HostHttpBackend {
    fn snapshot(
        &self,
        request: &BrowserNavigateRequest,
    ) -> Result<(String, String, Vec<u8>), AgentError> {
        let client = Client::builder()
            .timeout(Duration::from_millis(request.limits.timeout_ms.max(1)))
            .build()
            .map_err(|error| AgentError::new("lilia.host.http.client", error.to_string()))?;
        let response = client
            .get(&request.url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| AgentError::new("lilia.host.http.request", error.to_string()))?;
        let final_url = response.url().to_string();
        let bytes = response
            .bytes()
            .map_err(|error| AgentError::new("lilia.host.http.body", error.to_string()))?;
        let limit = usize::try_from(request.limits.max_output_bytes)
            .unwrap_or(usize::MAX)
            .max(1);
        let body = bytes[..bytes.len().min(limit)].to_vec();
        let title = html_title(&body).unwrap_or_else(|| final_url.clone());
        Ok((final_url, title, body))
    }

    fn cancel(&self, _handle_id: &str) -> Result<(), AgentError> {
        Ok(())
    }
}

fn html_title(body: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(body);
    let lower = text.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let content_start = lower[start..].find('>')? + start + 1;
    let end = lower[content_start..].find("</title>")? + content_start;
    let title = text[content_start..end].trim();
    (!title.is_empty()).then(|| title.to_string())
}

#[derive(Default)]
struct ReqwestMcpHttpClient;

impl McpHttpClient for ReqwestMcpHttpClient {
    fn post_json(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &Value,
        timeout: Duration,
    ) -> Result<Value, AgentError> {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| AgentError::new("lilia.host.mcp.client", error.to_string()))?;
        let mut request = client.post(url).json(body);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        request
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .and_then(reqwest::blocking::Response::json)
            .map_err(|error| AgentError::new("lilia.host.mcp.http", error.to_string()))
    }
}

pub(crate) fn native_coding_backends() -> NativeCodingBackends {
    NativeCodingBackends {
        git: Arc::new(CliGitBackend::default()),
        filesystem: Arc::new(WorkspaceFilesystemBackend),
        process: Some(Arc::new(HostProcessBackend::default())),
        browser: Some(Arc::new(HostHttpBackend)),
        lsp: Arc::new(StdioLspProcessFactory),
        mcp: Arc::new(CompositeMcpTransportFactory::new(Arc::new(
            ReqwestMcpHttpClient,
        ))),
        code_index_lsp: Arc::new(mutsuki_agent_plugin_code_index::UnavailableLspSignals),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mutsuki_agent_contracts::{AgentWorkspaceRef, ExecutionLimits};

    #[test]
    fn process_backend_executes_in_workspace_and_honors_output_limit() {
        let root = std::env::temp_dir().join("lilia-host-process-backend");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let backend = HostProcessBackend::default();
        let result = backend
            .exec(
                "process-test",
                &ProcessExecRequest {
                    workspace: AgentWorkspaceRef {
                        workspace_id: "test".into(),
                        root: root.display().to_string(),
                    },
                    command: "sh".into(),
                    args: vec!["-c".into(), "printf 123456".into()],
                    stdin: None,
                    limits: ExecutionLimits {
                        timeout_ms: 1_000,
                        max_output_bytes: 4,
                        max_concurrency: 1,
                    },
                    allow_network: false,
                },
            )
            .unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.summary, "1234");
        assert!(result.truncated);
        assert!(!result.cancelled);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn title_is_extracted_from_html() {
        assert_eq!(
            html_title(b"<html><TITLE>Workspace</TITLE></html>").as_deref(),
            Some("Workspace")
        );
    }
}

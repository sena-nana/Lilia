use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::path::Path;
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
    BrowserGateway, ProcessGateway, WorkspaceFilesystemBackend,
};
use mutsuki_agent_plugin_git::CliGitBackend;
use mutsuki_agent_plugin_lsp::StdioLspProcessFactory;
use mutsuki_agent_plugin_mcp::{CompositeMcpTransportFactory, ReqwestMcpHttpClient};
use reqwest::blocking::Client;
use reqwest::redirect::Policy;

/// Maximum redirects followed by [`HostHttpBackend`]. Each hop is re-validated.
const HOST_HTTP_MAX_REDIRECTS: usize = 3;

/// Known network-enabling program names blocked when `allow_network=false`.
///
/// Residual risk: unknown binaries can still open sockets. Without an OS-level
/// network sandbox, this is a fail-closed best-effort deny list — not a proof of
/// offline execution.
const NETWORK_PROGRAM_DENYLIST: &[&str] = &[
    "curl", "wget", "aria2c", "http", "httpie", "nc", "ncat", "netcat", "socat",
    "ssh", "scp", "sftp", "ftp", "telnet", "nmap", "dig", "nslookup", "host",
    "ping", "traceroute", "tracepath", "tcpdump", "tshark",
];

/// Local Host process capability used by the shared Computer Use service.
///
/// AgentKit owns the approval plan and handle identity; this backend owns the
/// OS process and makes that handle cancellable.
///
/// # Network policy
///
/// `ProcessExecRequest::allow_network` is enforced fail-closed: when false, the
/// backend refuses commands that match the network-program denylist or that
/// embed obvious network URLs/shell invocations of those programs. There is no
/// OS sandbox here; callers that need unrestricted process execution must set
/// `allow_network=true` after explicit approval.
#[derive(Default)]
pub(crate) struct HostProcessBackend {
    active: Mutex<BTreeMap<String, Arc<ActiveProcess>>>,
}

struct ActiveProcess {
    child: Mutex<Child>,
    cancelled: AtomicBool,
}

impl ProcessGateway for HostProcessBackend {
    fn exec(
        &self,
        handle_id: &str,
        request: &ProcessExecRequest,
    ) -> Result<ProcessExecResult, AgentError> {
        enforce_process_network_policy(request)?;

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

fn enforce_process_network_policy(request: &ProcessExecRequest) -> Result<(), AgentError> {
    if request.allow_network {
        return Ok(());
    }
    if let Some(hit) = network_policy_violation(&request.command, &request.args) {
        return Err(AgentError::new(
            "lilia.host.process.network_denied",
            format!(
                "network is denied for this process exec (allow_network=false); blocked {hit}. \
                 HostProcessBackend has no OS network sandbox — set allow_network=true only after \
                 explicit approval, or remove the network-enabling command/args."
            ),
        ));
    }
    Ok(())
}

fn network_policy_violation(command: &str, args: &[String]) -> Option<&'static str> {
    if program_name_is_denied(command) {
        return Some("command program on the network denylist");
    }
    for arg in args {
        if looks_like_network_url(arg) {
            return Some("argument looks like a network URL");
        }
        if program_name_is_denied(arg) {
            return Some("argument names a network program");
        }
    }
    // Shell wrappers can hide network tools inside `-c` / `/C` payloads.
    let command_base = program_basename(command);
    let shell = matches!(
        command_base.as_str(),
        "sh" | "bash" | "zsh" | "dash" | "cmd" | "cmd.exe" | "powershell" | "powershell.exe" | "pwsh"
    );
    if shell {
        for (idx, arg) in args.iter().enumerate() {
            let is_script_flag = matches!(arg.as_str(), "-c" | "/C" | "/c" | "-Command" | "-command");
            let script = if is_script_flag {
                args.get(idx + 1).map(String::as_str)
            } else if idx == args.len() - 1 && args.iter().any(|a| matches!(a.as_str(), "-c" | "/C" | "/c" | "-Command" | "-command")) {
                None
            } else {
                None
            };
            if let Some(script) = script {
                if let Some(hit) = shell_script_network_violation(script) {
                    return Some(hit);
                }
            }
        }
        // Also scan all args as a joined script for denylist tokens / URLs.
        let joined = args.join(" ");
        if let Some(hit) = shell_script_network_violation(&joined) {
            return Some(hit);
        }
    }
    None
}

fn shell_script_network_violation(script: &str) -> Option<&'static str> {
    if looks_like_network_url(script) {
        return Some("shell script contains a network URL");
    }
    for token in script.split(|c: char| c.is_whitespace() || matches!(c, '|' | '&' | ';' | '`' | '$' | '(' | ')' | '<' | '>' | '"' | '\'')) {
        let trimmed = token.trim_start_matches("./").trim_start_matches('\\');
        if program_name_is_denied(trimmed) {
            return Some("shell script invokes a network program");
        }
    }
    None
}

fn program_name_is_denied(program: &str) -> bool {
    let base = program_basename(program);
    NETWORK_PROGRAM_DENYLIST
        .iter()
        .any(|denied| base.eq_ignore_ascii_case(denied))
}

fn program_basename(program: &str) -> String {
    Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase()
}

fn looks_like_network_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("https://")
        || lower.contains("http://")
        || lower.contains("ftp://")
        || lower.contains("ws://")
        || lower.contains("wss://")
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

/// Local Host HTTP snapshot backend used by Computer Use browser navigation.
///
/// SSRF controls: only `http`/`https` (HTTPS preferred), block private /
/// link-local / metadata targets, resolve hostnames and re-check IPs, and cap
/// redirects with per-hop validation. Residual risk: DNS rebinding between
/// check and connect.
#[derive(Default)]
pub(crate) struct HostHttpBackend;

impl BrowserGateway for HostHttpBackend {
    fn snapshot(
        &self,
        request: &BrowserNavigateRequest,
    ) -> Result<(String, String, Vec<u8>), AgentError> {
        validate_public_http_url(&request.url)?;

        let client = Client::builder()
            .timeout(Duration::from_millis(request.limits.timeout_ms.max(1)))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= HOST_HTTP_MAX_REDIRECTS {
                    return attempt.error(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!(
                            "refusing more than {HOST_HTTP_MAX_REDIRECTS} redirects for host HTTP snapshot"
                        ),
                    ));
                }
                match validate_public_http_url(attempt.url().as_str()) {
                    Ok(()) => attempt.follow(),
                    Err(error) => attempt.error(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        error.message,
                    )),
                }
            }))
            .build()
            .map_err(|error| AgentError::new("lilia.host.http.client", error.to_string()))?;
        let response = client
            .get(&request.url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| AgentError::new("lilia.host.http.request", error.to_string()))?;
        let final_url = response.url().to_string();
        validate_public_http_url(&final_url)?;
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

fn validate_public_http_url(raw: &str) -> Result<(), AgentError> {
    let url = reqwest::Url::parse(raw).map_err(|error| {
        AgentError::new(
            "lilia.host.http.ssrf",
            format!("invalid URL: {error}"),
        )
    })?;
    match url.scheme() {
        "https" | "http" => {}
        other => {
            return Err(AgentError::new(
                "lilia.host.http.ssrf",
                format!("refusing non-http(s) scheme `{other}` (https preferred)"),
            ));
        }
    }
    if url.username() != "" || url.password().is_some() {
        return Err(AgentError::new(
            "lilia.host.http.ssrf",
            "refusing URLs that embed credentials",
        ));
    }
    let host = url.host_str().ok_or_else(|| {
        AgentError::new("lilia.host.http.ssrf", "URL is missing a host")
    })?;
    if host_is_blocked(host) {
        return Err(AgentError::new(
            "lilia.host.http.ssrf",
            format!("refusing private/link-local/metadata host `{host}`"),
        ));
    }
    let port = url.port_or_known_default().unwrap_or(0);
    match (host, port).to_socket_addrs() {
        Ok(addrs) => {
            let mut saw_addr = false;
            for addr in addrs {
                saw_addr = true;
                if ip_is_blocked(addr.ip()) {
                    return Err(AgentError::new(
                        "lilia.host.http.ssrf",
                        format!(
                            "refusing URL whose resolved address is private/link-local/metadata ({})",
                            addr.ip()
                        ),
                    ));
                }
            }
            if !saw_addr {
                return Err(AgentError::new(
                    "lilia.host.http.ssrf",
                    format!("failed to resolve host `{host}`"),
                ));
            }
        }
        Err(error) => {
            // Literal IPs already checked via host_is_blocked; unresolved names fail closed.
            if host.parse::<IpAddr>().is_err() {
                return Err(AgentError::new(
                    "lilia.host.http.ssrf",
                    format!("failed to resolve host `{host}`: {error}"),
                ));
            }
        }
    }
    Ok(())
}

fn host_is_blocked(host: &str) -> bool {
    let lower = host.trim().trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
    if lower == "localhost"
        || lower.ends_with(".localhost")
        || lower == "metadata.google.internal"
        || lower == "metadata"
        || lower.ends_with(".internal")
    {
        return true;
    }
    if let Ok(ip) = lower.parse::<IpAddr>() {
        return ip_is_blocked(ip);
    }
    // Reject path-like or weird hosts that could confuse parsers.
    if lower.contains('/') || lower.contains('\\') {
        return true;
    }
    // Block obvious dotted forms that parse as IPv4 after normalization failures.
    if looks_like_ipv4_literal(&lower) {
        if let Ok(ip) = parse_loose_ipv4(&lower) {
            return ip_is_blocked(IpAddr::V4(ip));
        }
    }
    false
}

fn looks_like_ipv4_literal(host: &str) -> bool {
    !host.is_empty() && host.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

fn parse_loose_ipv4(host: &str) -> Result<Ipv4Addr, ()> {
    host.parse().map_err(|_| ())
}

fn ip_is_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => ipv4_is_blocked(v4),
        IpAddr::V6(v6) => ipv6_is_blocked(v6),
    }
}

fn ipv4_is_blocked(ip: Ipv4Addr) -> bool {
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || ip.octets()[0] == 0
        // CGNAT / shared transition space often treated as non-public for SSRF.
        || (ip.octets()[0] == 100 && (ip.octets()[1] & 0b1100_0000) == 0b0100_0000)
        // IETF protocol assignments 192.0.0.0/24 (excl. .9/.10) — block whole /24 for simplicity.
        || (ip.octets()[0] == 192 && ip.octets()[1] == 0 && ip.octets()[2] == 0)
        || ip.octets() == [169, 254, 169, 254]
        || (ip.octets()[0] == 169 && ip.octets()[1] == 254)
}

fn ipv6_is_blocked(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return true;
    }
    // Unique local fc00::/7, link-local fe80::/10.
    let segments = ip.segments();
    if (segments[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    if (segments[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    // IPv4-mapped IPv6.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return ipv4_is_blocked(v4);
    }
    if let Some(v4) = ip.to_ipv4() {
        return ipv4_is_blocked(v4);
    }
    false
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

    fn sample_process_request(
        root: &Path,
        command: String,
        args: Vec<String>,
        allow_network: bool,
    ) -> ProcessExecRequest {
        ProcessExecRequest {
            workspace: AgentWorkspaceRef {
                workspace_id: "test".into(),
                root: root.display().to_string(),
            },
            command,
            args,
            stdin: None,
            limits: ExecutionLimits {
                timeout_ms: 1_000,
                max_output_bytes: 4,
                max_concurrency: 1,
            },
            allow_network,
        }
    }

    #[test]
    fn process_backend_executes_in_workspace_and_honors_output_limit() {
        let root = std::env::temp_dir().join("lilia-host-process-backend");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let backend = HostProcessBackend::default();
        #[cfg(windows)]
        let (command, args) = ("cmd".to_string(), vec!["/C".into(), "echo 123456".into()]);
        #[cfg(not(windows))]
        let (command, args) = ("sh".to_string(), vec!["-c".into(), "printf 123456".into()]);
        let result = backend
            .exec(
                "process-test",
                &sample_process_request(&root, command, args, false),
            )
            .unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(&result.summary[..4.min(result.summary.len())], "1234");
        assert!(result.truncated);
        assert!(!result.cancelled);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn process_backend_denies_network_programs_when_allow_network_false() {
        let root = std::env::temp_dir().join("lilia-host-process-network-deny");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let backend = HostProcessBackend::default();
        let error = backend
            .exec(
                "process-net-deny",
                &sample_process_request(
                    &root,
                    "curl".into(),
                    vec!["https://example.com".into()],
                    false,
                ),
            )
            .expect_err("curl must be denied when allow_network=false");
        assert_eq!(error.code, "lilia.host.process.network_denied");
        assert!(error.message.contains("network is denied"));

        let shell_error = backend
            .exec(
                "process-net-deny-shell",
                &sample_process_request(
                    &root,
                    "sh".into(),
                    vec!["-c".into(), "curl https://example.com".into()],
                    false,
                ),
            )
            .expect_err("shell-wrapped curl must be denied");
        assert_eq!(shell_error.code, "lilia.host.process.network_denied");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn process_backend_allows_network_programs_when_allow_network_true_policy_only() {
        // Policy check only — spawn may fail if curl is absent; we assert the
        // network denylist is skipped before spawn by using a missing binary name
        // that is not on the denylist when allow_network=true would still try spawn.
        assert!(network_policy_violation("curl", &["https://example.com".into()]).is_some());
        // When allow_network is true, enforce_process_network_policy short-circuits.
        let request = ProcessExecRequest {
            workspace: AgentWorkspaceRef {
                workspace_id: "test".into(),
                root: "/tmp".into(),
            },
            command: "curl".into(),
            args: vec!["https://example.com".into()],
            stdin: None,
            limits: ExecutionLimits {
                timeout_ms: 1_000,
                max_output_bytes: 4,
                max_concurrency: 1,
            },
            allow_network: true,
        };
        assert!(enforce_process_network_policy(&request).is_ok());
    }

    #[test]
    fn http_backend_blocks_ssrf_targets() {
        for target in [
            "http://127.0.0.1/latest/meta-data",
            "http://localhost/",
            "http://[::1]/",
            "http://169.254.169.254/latest/meta-data",
            "http://10.0.0.1/",
            "http://192.168.1.1/",
            "http://172.16.0.5/",
            "file:///etc/passwd",
            "ftp://example.com/",
        ] {
            let error = validate_public_http_url(target)
                .expect_err(&format!("expected SSRF block for {target}"));
            assert_eq!(error.code, "lilia.host.http.ssrf", "target={target}");
        }
    }

    #[test]
    fn http_backend_accepts_public_https_shape_without_resolution_guarantee() {
        // example.com should resolve publicly in CI; if DNS fails the helper fails closed.
        match validate_public_http_url("https://example.com/") {
            Ok(()) => {}
            Err(error) => {
                assert_eq!(error.code, "lilia.host.http.ssrf");
                assert!(
                    error.message.contains("failed to resolve"),
                    "unexpected error: {}",
                    error.message
                );
            }
        }
    }

    #[test]
    fn title_is_extracted_from_html() {
        assert_eq!(
            html_title(b"<html><TITLE>Workspace</TITLE></html>").as_deref(),
            Some("Workspace")
        );
    }

    #[test]
    fn ipv4_private_ranges_are_blocked() {
        assert!(ip_is_blocked(IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3))));
        assert!(ip_is_blocked(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(ip_is_blocked(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1))));
        assert!(ip_is_blocked(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(ip_is_blocked(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))));
        assert!(!ip_is_blocked(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(ip_is_blocked(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }
}

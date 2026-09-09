//! Minimal remote-observe API (#60 / #59 DoD): read-only status / timeline / diagnostics.
//!
//! Exposed over HTTP by `apps/service` and reusable by Remote clients. Mutation and
//! AgentKit session control are intentionally out of scope here.

use lilia_agent::IndependentDiagnostics;
use lilia_contracts::{TaskId, TimelineProjectionEvent};
use mutsuki_agent_contracts::AgentWireRequestEnvelope;
use serde::Serialize;
use std::io::{self, Read};

use crate::{health_http_response, ServiceAuthority, ServiceAuthorityStatus, ServiceHealthReport};

/// Read-only product/Agent observation surface for remote clients.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteObserveStatus {
    pub read_only: bool,
    pub authority: ServiceAuthorityStatus,
    pub health: ServiceHealthReport,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTimelineObserve {
    pub read_only: bool,
    pub task_id: String,
    pub events: Vec<TimelineProjectionEvent>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDiagnosticsObserve {
    pub read_only: bool,
    pub diagnostics: IndependentDiagnostics,
    pub health: ServiceHealthReport,
}

impl ServiceAuthority {
    pub fn observe_status(&self) -> RemoteObserveStatus {
        RemoteObserveStatus {
            read_only: true,
            authority: self.status(),
            health: self.health(),
        }
    }

    pub fn observe_timeline(
        &self,
        task_id: &str,
    ) -> Result<RemoteTimelineObserve, crate::ServiceAuthorityError> {
        let task = TaskId::new(task_id.trim())
            .map_err(|err| crate::ServiceAuthorityError::Product(err.to_string()))?;
        Ok(RemoteTimelineObserve {
            read_only: true,
            task_id: task.as_str().to_string(),
            // Prefer Runtime SQLite projection store (survives Desktop disconnect / restart).
            events: self.projection_timeline_for_task(&task),
        })
    }

    pub fn observe_diagnostics(&self) -> RemoteDiagnosticsObserve {
        RemoteDiagnosticsObserve {
            read_only: true,
            diagnostics: self.credential_diagnostics(),
            health: self.health(),
        }
    }
}

fn json_ok(body: impl Serialize) -> String {
    let body = serde_json::to_string(&body)
        .unwrap_or_else(|_| r#"{"error":"serialize_failed"}"#.to_string());
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn json_err(code: &str, message: &str) -> String {
    let body = serde_json::json!({ "error": message }).to_string();
    format!(
        "HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn parse_request_target(request: &str) -> (&str, &str, &str, &str) {
    let line = request.lines().next().unwrap_or("");
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let body = request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("");
    (method, path, query, body)
}

fn query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if k == key {
            Some(v)
        } else {
            None
        }
    })
}

/// Read one bounded HTTP/1 request, including the complete Content-Length body.
pub fn read_http_request(reader: &mut impl Read) -> io::Result<String> {
    const MAX_HEADER_BYTES: usize = 64 * 1024;
    const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let count = reader.read(&mut chunk)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before HTTP headers completed",
            ));
        }
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
        if bytes.len() > MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HTTP headers exceed limit",
            ));
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]);
    let content_length = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find_map(|(name, value)| {
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "HTTP body exceeds limit",
        ));
    }
    let expected = header_end.saturating_add(content_length);
    while bytes.len() < expected {
        let count = reader.read(&mut chunk)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before HTTP body completed",
            ));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    bytes.truncate(expected);
    String::from_utf8(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
}

/// Environment variable for the observe/wire bearer token.
pub const SERVICE_OBSERVE_TOKEN_ENV: &str = "LILIA_SERVICE_OBSERVE_TOKEN";

fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

fn bearer_token<'a>(request: &'a str) -> Option<&'a str> {
    let value = header_value(request, "Authorization")?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?;
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

fn observe_path_requires_auth(path: &str) -> bool {
    matches!(
        path,
        "/status"
            | "/observe/status"
            | "/timeline"
            | "/observe/timeline"
            | "/diagnostics"
            | "/observe/diagnostics"
            | "/agent/wire"
    )
}

fn ensure_observe_auth(request: &str, path: &str, observe_token: Option<&str>) -> Option<String> {
    let Some(expected) = observe_token.filter(|token| !token.is_empty()) else {
        return None;
    };
    if !observe_path_requires_auth(path) {
        return None;
    }
    match bearer_token(request) {
        Some(provided) if constant_time_eq(provided.as_bytes(), expected.as_bytes()) => None,
        _ => Some(json_err(
            "401 Unauthorized",
            "Authorization Bearer token required for observe endpoints",
        )),
    }
}

/// Serve the Service HTTP surface used by remote clients.
///
/// Routes:
/// - `GET /health`
/// - `GET /status` | `GET /observe/status`
/// - `GET /timeline?taskId=` | `GET /observe/timeline?taskId=`
/// - `GET /diagnostics` | `GET /observe/diagnostics`
/// - `POST /agent/wire` (canonical Mutsuki Agent Wire envelope)
///
/// When `observe_token` is `Some`, observe + wire routes require
/// `Authorization: Bearer <token>` (fail-closed). `/health` stays unauthenticated
/// for local liveness probes.
pub fn serve_readonly_http(authority: &ServiceAuthority, request: &str) -> String {
    serve_readonly_http_with_auth(authority, request, None)
}

pub fn serve_readonly_http_with_auth(
    authority: &ServiceAuthority,
    request: &str,
    observe_token: Option<&str>,
) -> String {
    let (method, path, query, body) = parse_request_target(request);
    if let Some(response) = ensure_observe_auth(request, path, observe_token) {
        return response;
    }
    match (method, path) {
        ("GET", "/health") => health_http_response(&authority.health()),
        ("GET", "/status" | "/observe/status") => json_ok(authority.observe_status()),
        ("GET", "/timeline" | "/observe/timeline") => {
            let Some(task_id) = query_param(query, "taskId").filter(|v| !v.is_empty()) else {
                return json_err("400 Bad Request", "taskId query parameter is required");
            };
            match authority.observe_timeline(task_id) {
                Ok(body) => json_ok(body),
                Err(err) => json_err("400 Bad Request", &err.to_string()),
            }
        }
        ("GET", "/diagnostics" | "/observe/diagnostics") => {
            json_ok(authority.observe_diagnostics())
        }
        ("POST", "/agent/wire") => {
            let request = match serde_json::from_str::<AgentWireRequestEnvelope>(body) {
                Ok(request) => request,
                Err(error) => {
                    return json_err("400 Bad Request", &format!("invalid agent wire: {error}"));
                }
            };
            match authority.dispatch_agent_wire(request) {
                Ok(response) => json_ok(response),
                Err(error) => json_err(
                    "400 Bad Request",
                    &format!("{}: {}", error.code, error.message),
                ),
            }
        }
        _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lilia_agent::ProductCredentialLoginInput;
    use lilia_contracts::{
        AgentSessionRef, ProjectionEventId, TimelineProjectionCommand, TimelineProjectionEvent,
        PRODUCT_TIMELINE_STORE_ID,
    };
    use mutsuki_agent_client::AgentEventCursor;
    use mutsuki_agent_contracts::{
        AgentMessage, AgentSessionCreateRequest, CredentialKind, SessionVersion,
        OPENAI_CREDENTIAL_PROVIDER_ID,
    };
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    fn authority(key: &str) -> ServiceAuthority {
        ServiceAuthority::bootstrap_in_memory_named(key, "observe-owner").unwrap()
    }

    #[test]
    fn observe_status_and_diagnostics_are_read_only() {
        let authority = authority("test:observe-status");
        let status = authority.observe_status();
        assert!(status.read_only);
        assert_eq!(status.authority.mode, "service");
        assert!(!status.authority.capabilities.official_agent_server);
        assert!(!status.authority.capabilities.node_runner_default);

        let diagnostics = authority.observe_diagnostics();
        assert!(diagnostics.read_only);
        assert!(diagnostics.diagnostics.credential_and_runtime_independent);
        assert!(!diagnostics.diagnostics.official_agent_server);
        assert!(!diagnostics.diagnostics.node_runner_default);
    }

    #[test]
    fn observe_timeline_returns_product_projection_events() {
        let authority = authority("test:observe-timeline");
        let client = authority.client().unwrap();
        let task = client
            .create_task(
                TaskId::new("task-observe").unwrap(),
                None,
                "observe timeline",
            )
            .unwrap();
        let session = AgentSessionRef::new("sess-observe").unwrap();
        authority
            .apply_projection(TimelineProjectionCommand::UpsertTimelineEvent {
                event: TimelineProjectionEvent {
                    id: ProjectionEventId::from_session_sequence(session.as_str(), 1),
                    task_id: task.id.clone(),
                    agent_session: session,
                    sequence: 1,
                    turn_id: Some("turn-1".into()),
                    kind: "message".into(),
                    status: "success".into(),
                    title: "hello".into(),
                    summary: Some("remote".into()),
                    payload: json!({
                        "projected": true,
                        "productProjectionStore": PRODUCT_TIMELINE_STORE_ID,
                    }),
                    projected: true,
                },
            })
            .unwrap();

        let observed = authority.observe_timeline(task.id.as_str()).unwrap();
        assert!(observed.read_only);
        assert_eq!(observed.events.len(), 1);
        assert_eq!(observed.events[0].title, "hello");
    }

    #[test]
    fn readonly_http_covers_status_timeline_and_diagnostics() {
        let authority = authority("test:observe-http");
        let runtime = authority.shared_runtime();
        runtime
            .inner()
            .credentials()
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: None,
                source: Some("user_api_key".into()),
            })
            .unwrap();
        runtime.inner().refresh_product_profile(None).unwrap();
        let model_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let model_address = model_listener.local_addr().unwrap();
        let model_server = thread::spawn(move || {
            let (mut stream, _) = model_listener.accept().unwrap();
            let mut bytes = [0_u8; 16_384];
            let _ = stream.read(&mut bytes).unwrap();
            let payload = json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {"role": "assistant", "content": "service wire complete"}
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        runtime.inner().set_model_endpoint_override(Some(format!(
            "http://{model_address}/v1/chat/completions"
        )));
        let client = authority.client().unwrap();
        let task = client
            .create_task(TaskId::new("task-http").unwrap(), None, "http")
            .unwrap();
        let session = AgentSessionRef::new("sess-http").unwrap();
        authority
            .apply_projection(TimelineProjectionCommand::UpsertTimelineEvent {
                event: TimelineProjectionEvent {
                    id: ProjectionEventId::from_session_sequence(session.as_str(), 1),
                    task_id: task.id.clone(),
                    agent_session: session,
                    sequence: 1,
                    turn_id: None,
                    kind: "message".into(),
                    status: "success".into(),
                    title: "http-event".into(),
                    summary: None,
                    payload: json!({ "projected": true }),
                    projected: true,
                },
            })
            .unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let authority_for_server = authority.clone();
        thread::spawn(move || {
            for _ in 0..12 {
                let (mut stream, _) = listener.accept().unwrap();
                let req = read_http_request(&mut stream).unwrap();
                let response = serve_readonly_http(&authority_for_server, &req);
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        thread::sleep(Duration::from_millis(20));

        let get = |path: &str| {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream
                .write_all(
                    format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                )
                .unwrap();
            let mut body = String::new();
            stream.read_to_string(&mut body).unwrap();
            body
        };

        let status = get("/observe/status");
        assert!(status.contains("200 OK"));
        assert!(status.contains("\"readOnly\":true"));
        assert!(status.contains("\"mode\":\"service\""));

        let timeline = get(&format!("/observe/timeline?taskId={}", task.id));
        assert!(timeline.contains("200 OK"));
        assert!(timeline.contains("http-event"));

        let diagnostics = get("/observe/diagnostics");
        assert!(diagnostics.contains("200 OK"));
        assert!(diagnostics.contains("\"credentialAndRuntimeIndependent\":true"));

        let missing = get("/observe/timeline");
        assert!(missing.contains("400 Bad Request"));
        assert!(missing.contains("taskId"));

        let remote = lilia_client::RemoteObserveHttpClient::new(format!(
            "http://{}:{}",
            addr.ip(),
            addr.port()
        ))
        .unwrap();
        let remote_status = remote.get_status().unwrap();
        assert_eq!(remote_status["readOnly"], json!(true));
        assert_eq!(remote_status["authority"]["mode"], json!("service"));
        let remote_timeline = remote.get_timeline(task.id.as_str()).unwrap();
        assert_eq!(remote_timeline["events"][0]["title"], json!("http-event"));
        let remote_diag = remote.get_diagnostics().unwrap();
        assert_eq!(remote_diag["readOnly"], json!(true));

        let mut agent = lilia_client::AgentWireHttpBackend::new(format!(
            "http://{}:{}",
            addr.ip(),
            addr.port()
        ))
        .unwrap()
        .into_client();
        let agent_session = agent
            .start_session(AgentSessionCreateRequest {
                session_id: None,
                profile_id: "mutsuki.reference.coding-agent".into(),
                title: Some("remote wire task".into()),
            })
            .unwrap();
        let version = agent
            .submit_turn(
                &agent_session.session_id,
                SessionVersion(1),
                "remote-wire-turn",
                vec![AgentMessage::user("run through service wire")],
                "remote-wire-idempotency",
            )
            .unwrap();
        model_server.join().unwrap();
        assert_eq!(version, SessionVersion(2));
        let mut cursor = AgentEventCursor::new(&agent_session.session_id, 0, 100).unwrap();
        let agent_events = cursor.poll(&mut agent).unwrap();
        assert!(!agent_events.is_empty());
        assert!(cursor.last_seen() > 0);
    }

    #[test]
    fn observe_endpoints_require_bearer_when_token_configured() {
        let authority = authority("test:observe-auth");
        let denied = serve_readonly_http_with_auth(
            &authority,
            "GET /observe/status HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            Some("secret-token"),
        );
        assert!(denied.contains("401 Unauthorized"), "{denied}");

        let allowed = serve_readonly_http_with_auth(
            &authority,
            "GET /observe/status HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer secret-token\r\nConnection: close\r\n\r\n",
            Some("secret-token"),
        );
        assert!(allowed.contains("200 OK"), "{allowed}");
        assert!(allowed.contains("\"readOnly\":true"));

        let health = serve_readonly_http_with_auth(
            &authority,
            "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            Some("secret-token"),
        );
        assert!(health.contains("200 OK"), "{health}");
    }
}


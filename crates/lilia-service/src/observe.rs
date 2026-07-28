//! Minimal remote-observe API (#60 / #59 DoD): read-only status / timeline / diagnostics.
//!
//! Exposed over HTTP by `apps/service` and reusable by Remote clients. Mutation and
//! AgentKit session control are intentionally out of scope here.

use lilia_agent_integration::IndependentDiagnostics;
use lilia_contracts::{TaskId, TimelineProjectionEvent};
use serde::Serialize;

use crate::{
    health_http_response, ServiceAuthority, ServiceAuthorityStatus, ServiceHealthReport,
};

/// Read-only product/Agent observation surface for Remote / CLI.
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
        let task = TaskId::new(task_id.trim()).map_err(|err| {
            crate::ServiceAuthorityError::Product(err.to_string())
        })?;
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
    let body = serde_json::to_string(&body).unwrap_or_else(|_| {
        r#"{"error":"serialize_failed"}"#.to_string()
    });
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

fn parse_request_target(request: &str) -> (&str, &str) {
    let line = request.lines().next().unwrap_or("");
    let mut parts = line.split_whitespace();
    let _method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    }
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

/// Serve the minimal read-only HTTP surface used by `apps/service`.
///
/// Routes:
/// - `GET /health`
/// - `GET /status` | `GET /observe/status`
/// - `GET /timeline?taskId=` | `GET /observe/timeline?taskId=`
/// - `GET /diagnostics` | `GET /observe/diagnostics`
pub fn serve_readonly_http(authority: &ServiceAuthority, request: &str) -> String {
    let (path, query) = parse_request_target(request);
    match path {
        "/health" => health_http_response(&authority.health()),
        "/status" | "/observe/status" => json_ok(authority.observe_status()),
        "/timeline" | "/observe/timeline" => {
            let Some(task_id) = query_param(query, "taskId").filter(|v| !v.is_empty()) else {
                return json_err("400 Bad Request", "taskId query parameter is required");
            };
            match authority.observe_timeline(task_id) {
                Ok(body) => json_ok(body),
                Err(err) => json_err("400 Bad Request", &err.to_string()),
            }
        }
        "/diagnostics" | "/observe/diagnostics" => json_ok(authority.observe_diagnostics()),
        _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lilia_contracts::{
        AgentSessionRef, ProjectionEventId, TimelineProjectionCommand, TimelineProjectionEvent,
        PRODUCT_TIMELINE_STORE_ID,
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
            for _ in 0..8 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
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
    }
}

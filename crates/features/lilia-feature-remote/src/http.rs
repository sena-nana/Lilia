//! HTTP bridge parse / route / respond. The host only binds the listener.
//!
//! Security notes:
//! - Unauthenticated `GET /status` returns [`crate::types::RemotePublicStatus`] only
//!   (no pairing tickets, challenges, or trusted endpoint ids).
//! - `POST /dispatch` and other sensitive routes require
//!   `Authorization: Bearer <sessionToken>` minted at pair time.
//! - CORS is not wildcarded; native clients do not need `Access-Control-Allow-Origin: *`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use crate::dispatch::{dispatch_remote_request, RemoteHost};
use crate::types::{RemotePairDeviceInput, RemoteRequestEnvelope};

const MAX_HTTP_REQUEST_BYTES: usize = 2 * 1024 * 1024;

pub fn serve_http_bridge<F, H>(listener: TcpListener, resolve: F)
where
    F: Fn() -> Option<H> + Send,
    H: RemoteHost + 'static,
{
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let Some(host) = resolve() else {
                    return;
                };
                let _ = thread::Builder::new()
                    .name("lilia-remote-request".to_owned())
                    .spawn(move || handle_http_stream(host, stream));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if resolve().is_none() {
                    return;
                }
                thread::sleep(Duration::from_millis(40));
            }
            Err(_) => return,
        }
    }
}

fn handle_http_stream<H: RemoteHost + 'static>(host: H, mut stream: TcpStream) {
    let response = match read_http_request(&mut stream) {
        Ok(request) => handle_http_request(&host, request),
        Err(error) => http_json_response(400, http_error_payload("invalidRequest", error, false)),
    };
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

struct HttpRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    origin: Option<String>,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("connection closed before request completed".to_owned());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break index;
        }
        if buffer.len() > 64 * 1024 {
            return Err("request headers are too large".to_owned());
        }
    };
    let header = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header.lines();
    let mut request_line = lines
        .next()
        .ok_or_else(|| "request line is missing".to_owned())?
        .split_whitespace();
    let method = request_line.next().unwrap_or_default().to_owned();
    let path = request_line.next().unwrap_or_default().to_owned();
    if method.is_empty() || path.is_empty() {
        return Err("request line is invalid".to_owned());
    }
    let mut content_length = 0usize;
    let mut authorization = None;
    let mut origin = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value
                .parse::<usize>()
                .map_err(|_| "content-length is invalid".to_owned())?;
        } else if name.eq_ignore_ascii_case("authorization") {
            authorization = Some(value.to_owned());
        } else if name.eq_ignore_ascii_case("origin") {
            origin = Some(value.to_owned());
        }
    }
    if content_length > MAX_HTTP_REQUEST_BYTES {
        return Err("request body is too large".to_owned());
    }
    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    if buffer.len() < body_start + content_length {
        return Err("request body is incomplete".to_owned());
    }
    Ok(HttpRequest {
        method,
        path,
        authorization,
        origin,
        body: buffer[body_start..body_start + content_length].to_vec(),
    })
}

fn bearer_token(authorization: Option<&str>) -> Option<&str> {
    let value = authorization?.trim();
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?
        .trim();
    (!token.is_empty()).then_some(token)
}

fn require_session_device<H: RemoteHost>(
    host: &H,
    authorization: Option<&str>,
) -> Result<String, (u16, Value)> {
    let Some(token) = bearer_token(authorization) else {
        return Err((
            401,
            http_error_payload("unauthorized", "session token is required", false),
        ));
    };
    host.authenticate_session_token(token).map_err(|error| {
        (
            401,
            http_error_payload(&error.code, error.message, error.retryable),
        )
    })
}

fn handle_http_request<H: RemoteHost>(host: &H, request: HttpRequest) -> String {
    let cors_origin = cors_allow_origin(request.origin.as_deref());
    match (request.method.as_str(), request.path.as_str()) {
        ("OPTIONS", _) => http_empty_response(204, cors_origin.as_deref()),
        ("GET", "/status") => {
            // Always return the public health view over HTTP — never pairing secrets.
            match host.public_status() {
                Ok(status) => {
                    http_json_response(200, json!({ "ok": true, "status": status }), cors_origin.as_deref())
                }
                Err(error) => http_json_response(
                    500,
                    http_error_payload(&error.code, error.message, error.retryable),
                    cors_origin.as_deref(),
                ),
            }
        }
        ("POST", "/pair") => {
            let input = match serde_json::from_slice::<RemotePairDeviceInput>(&request.body) {
                Ok(input) => input,
                Err(error) => {
                    return http_json_response(
                        400,
                        http_error_payload("invalidRequest", error.to_string(), false),
                        cors_origin.as_deref(),
                    );
                }
            };
            match host.pair_device(input) {
                Ok(peer) => match host.issue_session_token(&peer.endpoint_id) {
                    Ok(session) => http_json_response(
                        200,
                        json!({
                            "ok": true,
                            "peer": peer,
                            "sessionToken": session.session_token,
                            "sessionExpiresAt": session.expires_at,
                        }),
                        cors_origin.as_deref(),
                    ),
                    Err(error) => http_json_response(
                        500,
                        http_error_payload(&error.code, error.message, error.retryable),
                        cors_origin.as_deref(),
                    ),
                },
                Err(error) => http_json_response(
                    403,
                    http_error_payload(&error.code, error.message, error.retryable),
                    cors_origin.as_deref(),
                ),
            }
        }
        ("POST", "/dispatch") => {
            let session_device = match require_session_device(host, request.authorization.as_deref())
            {
                Ok(device) => device,
                Err((status, payload)) => {
                    return http_json_response(status, payload, cors_origin.as_deref());
                }
            };
            let envelope = match serde_json::from_slice::<RemoteRequestEnvelope>(&request.body) {
                Ok(envelope) => envelope,
                Err(error) => {
                    return http_json_response(
                        400,
                        http_error_payload("invalidRequest", error.to_string(), false),
                        cors_origin.as_deref(),
                    );
                }
            };
            if !crate::service::constant_time_eq(
                session_device.as_bytes(),
                envelope.device_id.as_bytes(),
            ) {
                return http_json_response(
                    401,
                    http_error_payload(
                        "unauthorized",
                        "session token does not match device id",
                        false,
                    ),
                    cors_origin.as_deref(),
                );
            }
            http_json_response(
                200,
                dispatch_remote_request(host, envelope),
                cors_origin.as_deref(),
            )
        }
        _ => http_json_response(
            404,
            http_error_payload("unsupported", "unknown remote route", false),
            cors_origin.as_deref(),
        ),
    }
}

/// Allow browser localhost origins only when an Origin header is present; never `*`.
fn cors_allow_origin(origin: Option<&str>) -> Option<String> {
    let origin = origin?.trim();
    const ALLOWED: &[&str] = &[
        "http://127.0.0.1",
        "http://localhost",
        "https://127.0.0.1",
        "https://localhost",
    ];
    for allowed in ALLOWED {
        if origin == *allowed
            || origin
                .strip_prefix(allowed)
                .is_some_and(|rest| rest.starts_with(':') || rest.is_empty())
        {
            // Reject credentialed wildcard; echo only explicit localhost origins.
            if origin.contains('*') {
                return None;
            }
            return Some(origin.to_owned());
        }
    }
    None
}

fn http_json_response(status: u16, payload: Value, cors_origin: Option<&str>) -> String {
    let reason = status_reason(status);
    let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned());
    let mut headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(origin) = cors_origin {
        headers.push_str(&format!("Access-Control-Allow-Origin: {origin}\r\n"));
        headers.push_str("Vary: Origin\r\n");
        headers.push_str("Access-Control-Allow-Headers: Authorization, Content-Type\r\n");
        headers.push_str("Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n");
    }
    format!("{headers}\r\n{body}")
}

fn http_empty_response(status: u16, cors_origin: Option<&str>) -> String {
    let reason = status_reason(status);
    let mut headers = format!("HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n");
    if let Some(origin) = cors_origin {
        headers.push_str(&format!("Access-Control-Allow-Origin: {origin}\r\n"));
        headers.push_str("Vary: Origin\r\n");
        headers.push_str("Access-Control-Allow-Headers: Authorization, Content-Type\r\n");
        headers.push_str("Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n");
    }
    format!("{headers}\r\n")
}

fn status_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

fn http_error_payload(code: &str, message: impl Into<String>, retryable: bool) -> Value {
    json!({
        "ok": false,
        "error": { "code": code, "message": message.into(), "retryable": retryable },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_never_allows_star_or_foreign_origins() {
        assert_eq!(cors_allow_origin(Some("*")), None);
        assert_eq!(cors_allow_origin(Some("https://evil.example")), None);
        assert_eq!(
            cors_allow_origin(Some("http://127.0.0.1:5173")).as_deref(),
            Some("http://127.0.0.1:5173")
        );
        assert_eq!(
            cors_allow_origin(Some("http://localhost")).as_deref(),
            Some("http://localhost")
        );
    }

    #[test]
    fn bearer_token_parser_accepts_standard_header() {
        assert_eq!(bearer_token(Some("Bearer abc.def")), Some("abc.def"));
        assert_eq!(bearer_token(Some("Token abc")), None);
        assert_eq!(bearer_token(None), None);
    }

    #[test]
    fn json_response_omits_cors_star() {
        let response = http_json_response(200, json!({"ok": true}), None);
        assert!(!response.contains("Access-Control-Allow-Origin: *"));
        assert!(response.contains("200 OK"));
    }
}

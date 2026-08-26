//! HTTP bridge parse / route / respond. The host only binds the listener.

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
    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find_map(|(name, value)| {
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
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
        body: buffer[body_start..body_start + content_length].to_vec(),
    })
}

fn handle_http_request<H: RemoteHost>(host: &H, request: HttpRequest) -> String {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/status") => match host.status() {
            Ok(status) => http_json_response(200, json!({ "ok": true, "status": status })),
            Err(error) => http_json_response(
                500,
                http_error_payload(&error.code, error.message, error.retryable),
            ),
        },
        ("POST", "/pair") => {
            let input = match serde_json::from_slice::<RemotePairDeviceInput>(&request.body) {
                Ok(input) => input,
                Err(error) => {
                    return http_json_response(
                        400,
                        http_error_payload("invalidRequest", error.to_string(), false),
                    );
                }
            };
            match host.pair_device(input) {
                Ok(peer) => http_json_response(200, json!({ "ok": true, "peer": peer })),
                Err(error) => http_json_response(
                    403,
                    http_error_payload(&error.code, error.message, error.retryable),
                ),
            }
        }
        ("POST", "/dispatch") => {
            let envelope = match serde_json::from_slice::<RemoteRequestEnvelope>(&request.body) {
                Ok(envelope) => envelope,
                Err(error) => {
                    return http_json_response(
                        400,
                        http_error_payload("invalidRequest", error.to_string(), false),
                    );
                }
            };
            http_json_response(200, dispatch_remote_request(host, envelope))
        }
        _ => http_json_response(
            404,
            http_error_payload("unsupported", "unknown remote route", false),
        ),
    }
}

fn http_json_response(status: u16, payload: Value) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned());
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n{body}",
        body.len()
    )
}

fn http_error_payload(code: &str, message: impl Into<String>, retryable: bool) -> Value {
    json!({
        "ok": false,
        "error": { "code": code, "message": message.into(), "retryable": retryable },
    })
}

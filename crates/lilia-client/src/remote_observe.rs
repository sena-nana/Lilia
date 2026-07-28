//! Minimal HTTP Remote observe client skeleton (#59 / #60).
//!
//! Talks to `apps/service` read-only endpoints (`/observe/status|timeline|diagnostics`).
//! Product mutations and AgentKit session control remain out of scope.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use serde_json::Value as JsonValue;

#[derive(Debug, thiserror::Error)]
pub enum RemoteObserveError {
    #[error("invalid base url: {0}")]
    InvalidBaseUrl(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("http {status}: {body}")]
    Http { status: u16, body: String },
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Thin std-only HTTP GET client for Service remote-observe.
#[derive(Clone, Debug)]
pub struct RemoteObserveHttpClient {
    host: String,
    port: u16,
}

impl RemoteObserveHttpClient {
    /// Accepts `http://127.0.0.1:8787` style base URLs (HTTP only).
    pub fn new(base_url: impl AsRef<str>) -> Result<Self, RemoteObserveError> {
        let raw = base_url.as_ref().trim().trim_end_matches('/');
        let without_scheme = raw
            .strip_prefix("http://")
            .ok_or_else(|| RemoteObserveError::InvalidBaseUrl(raw.to_string()))?;
        let (host, port) = match without_scheme.split_once(':') {
            Some((host, port)) => {
                let port: u16 = port.parse().map_err(|_| {
                    RemoteObserveError::InvalidBaseUrl(raw.to_string())
                })?;
                (host.to_string(), port)
            }
            None => (without_scheme.to_string(), 80),
        };
        if host.is_empty() {
            return Err(RemoteObserveError::InvalidBaseUrl(raw.to_string()));
        }
        Ok(Self { host, port })
    }

    pub fn get_status(&self) -> Result<JsonValue, RemoteObserveError> {
        self.get_json("/observe/status")
    }

    pub fn get_timeline(&self, task_id: &str) -> Result<JsonValue, RemoteObserveError> {
        self.get_json(&format!("/observe/timeline?taskId={task_id}"))
    }

    pub fn get_diagnostics(&self) -> Result<JsonValue, RemoteObserveError> {
        self.get_json("/observe/diagnostics")
    }

    fn get_json(&self, path: &str) -> Result<JsonValue, RemoteObserveError> {
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\nAccept: application/json\r\n\r\n",
            host = self.host,
            port = self.port,
        );
        stream.write_all(request.as_bytes())?;
        let mut raw = String::new();
        stream.read_to_string(&mut raw)?;
        let (header, body) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
        let status = header
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .unwrap_or(0);
        if !(200..300).contains(&status) {
            return Err(RemoteObserveError::Http {
                status,
                body: body.to_string(),
            });
        }
        Ok(serde_json::from_str(body)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http_base_url() {
        let client = RemoteObserveHttpClient::new("http://127.0.0.1:8787/").unwrap();
        assert_eq!(client.host, "127.0.0.1");
        assert_eq!(client.port, 8787);
    }
}

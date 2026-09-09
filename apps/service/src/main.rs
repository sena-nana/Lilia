//! Minimal Service-mode process entry (#60).
//!
//! Boots a Host-neutral `ServiceAuthority` (no Desktop UI) and serves the
//! read-only remote-observe HTTP surface:
//! - `GET /health`
//! - `GET /status` | `GET /observe/status`
//! - `GET /timeline?taskId=` | `GET /observe/timeline?taskId=`
//! - `GET /diagnostics` | `GET /observe/diagnostics`
//!
//! Bind address: `LILIA_SERVICE_BIND` (default `127.0.0.1:8787`).
//! Non-loopback binds are fail-closed: require `LILIA_SERVICE_OBSERVE_TOKEN`
//! and enforce Bearer auth on observe/wire routes.
//! This is not a full ServiceHost/Link multiplexor.

use std::env;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use lilia_service::{
    read_http_request, serve_readonly_http_with_auth, ServiceAuthority, ServiceHealthStatus,
    SERVICE_OBSERVE_TOKEN_ENV,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("lilia-service failed: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let bind = env::var("LILIA_SERVICE_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let observe_token = env::var(SERVICE_OBSERVE_TOKEN_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    let authority = if let Ok(home) = env::var("LILIA_SERVICE_HOME") {
        ServiceAuthority::bootstrap_with_home(home)?
    } else {
        let storage_key =
            env::var("LILIA_SERVICE_STORAGE_KEY").unwrap_or_else(|_| "in-memory:default".into());
        let owner = env::var("LILIA_SERVICE_OWNER").unwrap_or_else(|_| "lilia-service".into());
        ServiceAuthority::bootstrap_in_memory_named(storage_key, owner)?
    };
    let health = authority.health();
    if health.status != ServiceHealthStatus::Ready {
        return Err(format!(
            "service health not ready: {}",
            serde_json::to_string(&health)?
        )
        .into());
    }

    let listener = TcpListener::bind(&bind)?;
    let local = listener.local_addr()?;
    if !local.ip().is_loopback() && observe_token.is_none() {
        return Err(format!(
            "refusing non-loopback bind `{local}` without {SERVICE_OBSERVE_TOKEN_ENV} (fail-closed)"
        )
        .into());
    }
    println!("lilia-service listening on http://{local}");
    if !local.ip().is_loopback() {
        println!("observe auth=required ({SERVICE_OBSERVE_TOKEN_ENV})");
    } else if observe_token.is_some() {
        println!("observe auth=enabled (loopback + token)");
    } else {
        println!("observe auth=disabled (loopback default)");
    }
    println!("health={}", serde_json::to_string(&authority.health())?);

    let running = Arc::new(AtomicBool::new(true));
    {
        let running = Arc::clone(&running);
        ctrlc_shim(running);
    }

    let observe_token = observe_token;
    while running.load(Ordering::SeqCst) {
        listener.set_nonblocking(true)?;
        match listener.accept() {
            Ok((mut stream, _)) => {
                let req = match read_http_request(&mut stream) {
                    Ok(request) => request,
                    Err(error) => {
                        let response = format!(
                            "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            error.to_string().len(),
                            error
                        );
                        let _ = stream.write_all(response.as_bytes());
                        continue;
                    }
                };
                let response = serve_readonly_http_with_auth(
                    &authority,
                    &req,
                    observe_token.as_deref(),
                );
                let _ = stream.write_all(response.as_bytes());
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(err) => return Err(err.into()),
        }
    }

    authority.shutdown();
    Ok(())
}

/// Run until the process is killed. Optional `LILIA_SERVICE_STDIN_STOP=1` stops on stdin EOF (piped smoke).
fn ctrlc_shim(running: Arc<AtomicBool>) {
    if env::var_os("LILIA_SERVICE_STDIN_STOP").is_none() {
        return;
    }
    std::thread::spawn(move || {
        let mut sink = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut sink);
        running.store(false, Ordering::SeqCst);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, SocketAddr};

    #[test]
    fn loopback_addrs_are_detected() {
        let loopback: SocketAddr = "127.0.0.1:8787".parse().unwrap();
        assert!(loopback.ip().is_loopback());
        let all: SocketAddr = "0.0.0.0:8787".parse().unwrap();
        assert!(!all.ip().is_loopback());
        assert!(matches!(all.ip(), IpAddr::V4(v4) if v4.is_unspecified()));
    }
}

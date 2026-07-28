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
//! This is not a full ServiceHost/Link multiplexor.

use std::env;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use lilia_service::{serve_readonly_http, ServiceAuthority, ServiceHealthStatus};

fn main() {
    if let Err(err) = run() {
        eprintln!("lilia-service failed: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let bind = env::var("LILIA_SERVICE_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());
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
        return Err(format!("service health not ready: {}", serde_json::to_string(&health)?).into());
    }

    let listener = TcpListener::bind(&bind)?;
    let local = listener.local_addr()?;
    println!("lilia-service listening on http://{local}");
    println!(
        "health={}",
        serde_json::to_string(&authority.health())?
    );

    let running = Arc::new(AtomicBool::new(true));
    {
        let running = Arc::clone(&running);
        ctrlc_shim(running);
    }

    while running.load(Ordering::SeqCst) {
        listener.set_nonblocking(true)?;
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let response = serve_readonly_http(&authority, &req);
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

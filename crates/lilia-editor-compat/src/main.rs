//! JSONL CLI for VS Code extension (#40).
//!
//! One JSON request per stdin line → one JSON response per stdout line.
//! Does not start LiliaCore, Desktop, Node agent-runner, or official Agent Server.

use std::io::{self, BufRead, Write};

use lilia_editor_compat::{handle_request, EditorCompatHost, HostRequest};

fn main() {
    let host = EditorCompatHost::new();
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::json!({"ok": false, "error": err.to_string()})
                );
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<HostRequest>(&line) {
            Ok(request) => handle_request(&host, request),
            Err(err) => lilia_editor_compat::HostResponse {
                ok: false,
                status: None,
                completion: None,
                next_edit: None,
                error: Some(format!("invalid request JSON: {err}")),
            },
        };
        match serde_json::to_string(&response) {
            Ok(payload) => {
                let _ = writeln!(stdout, "{payload}");
            }
            Err(err) => {
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::json!({"ok": false, "error": err.to_string()})
                );
            }
        }
        let _ = stdout.flush();
    }
}

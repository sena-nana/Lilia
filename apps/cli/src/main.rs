//! Minimal `lilia-cli` entry (#58).
//!
//! Commands:
//! - `smoke` — credential → submit turn (loopback) → product timeline → approval
//! - `status` — shared authority status JSON
//! - `timeline --task <id>` — read product projection timeline
//!
//! Bootstrap: in-memory by default, or `--home <path>` / `LILIA_HOME` for shared
//! `LiliaDataPaths` layout with Desktop / Service.

use lilia_cli::{print_json, resolve_home, run_remote_agent_wire_turn, CliSession};
use lilia_contracts::TaskId;

fn main() {
    if let Err(err) = run() {
        eprintln!("lilia-cli failed: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }

    let command = args[0].as_str();
    if command == "agent-run" {
        let service = arg_value(&args, "--service")
            .map(str::to_string)
            .or_else(|| std::env::var("LILIA_SERVICE_URL").ok())
            .ok_or("agent-run requires --service <url> or LILIA_SERVICE_URL")?;
        let prompt = arg_value(&args, "--prompt").ok_or("agent-run requires --prompt <text>")?;
        let profile = arg_value(&args, "--profile").unwrap_or("mutsuki.reference.coding-agent");
        let report = run_remote_agent_wire_turn(&service, profile, prompt)?;
        print_json(&report)?;
        return Ok(());
    }
    let session = match resolve_home(&args) {
        Some(home) => CliSession::bootstrap_with_home(home)?,
        None => {
            let key = std::env::var("LILIA_CLI_STORAGE_KEY")
                .unwrap_or_else(|_| "cli:in-memory:default".into());
            CliSession::bootstrap_in_memory(key)?
        }
    };

    match command {
        "smoke" => {
            let report = session.run_same_use_case_as_desktop()?;
            print_json(&report)?;
            if !report.proof.desktop_and_cli_share_runtime {
                return Err("shared runtime proof failed".into());
            }
        }
        "status" => {
            print_json(&session.authority().status())?;
        }
        "timeline" => {
            let task = arg_value(&args, "--task").ok_or("timeline requires --task <id>")?;
            let task_id = TaskId::new(task).map_err(|e| e.to_string())?;
            let events = session.product_timeline(&task_id)?;
            print_json(&events)?;
        }
        "products" => {
            let view = session.list_migrated_products()?;
            print_json(&view)?;
        }
        "credential-login" => {
            session.login_test_openai_credential()?;
            let diag = session.authority().credential_diagnostics();
            // Never print secret material — only broker health / counts.
            print_json(&diag)?;
        }
        other => {
            return Err(format!("unknown command: {other}").into());
        }
    }
    Ok(())
}

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
}

fn print_help() {
    println!(
        "\
lilia-cli — Host-neutral CLI via shared LiliaClient / ServiceAuthority

Usage:
  lilia-cli smoke [--home <path>]
  lilia-cli status [--home <path>]
  lilia-cli timeline --task <id> [--home <path>]
  lilia-cli products [--home <path>]
  lilia-cli credential-login [--home <path>]
  lilia-cli agent-run --service <url> --prompt <text> [--profile <id>]

Environment:
  LILIA_HOME              Shared data home (same LiliaDataPaths as Desktop)
  LILIA_CLI_STORAGE_KEY   In-memory authority key (default cli:in-memory:default)
  LILIA_CLI_TEST_API_KEY  Optional test credential secret (never echoed)
  LILIA_SERVICE_URL       Service URL for Agent Wire commands
"
    );
}

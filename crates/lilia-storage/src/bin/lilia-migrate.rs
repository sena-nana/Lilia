//! `lilia-migrate` — legacy Desktop DB → Lilia Storage (#47).
//!
//! Usage:
//!   lilia-migrate dry-run [--legacy PATH] [--product PATH] [--home PATH]
//!   lilia-migrate apply ...
//!   lilia-migrate status ...
//!   lilia-migrate report ...
//!   lilia-migrate rollback ...
//!   lilia-migrate inspect ...

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use lilia_storage::{LiliaDataPaths, LegacyMigrationTool, MigrationMode};

fn main() -> ExitCode {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "lilia-migrate <inspect|dry-run|apply|status|report|rollback> [--home DIR] [--legacy PATH] [--product PATH]"
        );
        return ExitCode::SUCCESS;
    }

    let mode = match args[0].as_str() {
        "inspect" => MigrationMode::Inspect,
        "dry-run" | "dry_run" => MigrationMode::DryRun,
        "apply" => MigrationMode::Apply,
        "status" => MigrationMode::Status,
        "report" => MigrationMode::Report,
        "rollback" => MigrationMode::Rollback,
        other => {
            eprintln!("unknown mode: {other}");
            return ExitCode::FAILURE;
        }
    };
    args.remove(0);

    let mut home: Option<PathBuf> = None;
    let mut legacy: Option<PathBuf> = None;
    let mut product: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--home" => {
                i += 1;
                home = args.get(i).map(PathBuf::from);
            }
            "--legacy" => {
                i += 1;
                legacy = args.get(i).map(PathBuf::from);
            }
            "--product" => {
                i += 1;
                product = args.get(i).map(PathBuf::from);
            }
            other => {
                eprintln!("unknown arg: {other}");
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }

    let paths = match home {
        Some(h) => LiliaDataPaths::from_home(h),
        None => LiliaDataPaths::resolve(),
    };
    let tool = LegacyMigrationTool {
        legacy_db: legacy.unwrap_or_else(|| paths.legacy_desktop_db()),
        product_db: product.unwrap_or_else(|| paths.product_db()),
        paths,
    };

    let result = match mode {
        MigrationMode::Inspect => tool.inspect(),
        MigrationMode::DryRun => tool.dry_run(),
        MigrationMode::Apply => tool.apply(),
        MigrationMode::Status => tool.status(),
        MigrationMode::Report => tool.report(),
        MigrationMode::Rollback => tool.rollback(),
    };

    match result {
        Ok(report) => {
            match serde_json::to_string_pretty(&report) {
                Ok(json) => println!("{json}"),
                Err(err) => {
                    eprintln!("serialize report failed: {err}");
                    eprintln!("{}", report.summary_line());
                }
            }
            if report.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(err) => {
            eprintln!("migration failed: {err}");
            ExitCode::FAILURE
        }
    }
}

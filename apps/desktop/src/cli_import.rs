use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_desktop_application::{
    CredentialImportDecision, DesktopApplicationConfig, DesktopDataImportService,
    DesktopImportExecutionOptions, DesktopImportPlan, DesktopImportReport,
    DesktopImportReportStatus,
};

use crate::data_import::legacy_instance_identity;
use crate::host::NativeDesktopHost;
use crate::storage::{lilia_home, LILIA_INSTANCE_IDENTITY};

static NEXT_STAGING_FILE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupAction {
    Launch { arguments: Vec<OsString> },
    Import(ImportCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportCommand {
    Plan {
        source: PathBuf,
        output: PathBuf,
    },
    Execute {
        plan: PathBuf,
        credentials: CredentialChoice,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CredentialChoice {
    #[default]
    Deny,
    Confirm,
}

impl From<CredentialChoice> for CredentialImportDecision {
    fn from(value: CredentialChoice) -> Self {
        match value {
            CredentialChoice::Deny => Self::Denied,
            CredentialChoice::Confirm => Self::Confirmed,
        }
    }
}

pub struct CliResult {
    pub exit_code: i32,
    pub stdout: Option<String>,
}

pub fn parse_startup(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<StartupAction, String> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return Ok(StartupAction::Launch {
            arguments: Vec::new(),
        });
    };
    if command != "import" {
        return Ok(StartupAction::Launch {
            arguments: std::iter::once(command).chain(arguments).collect(),
        });
    }
    let subcommand = arguments
        .next()
        .ok_or_else(|| "import requires `plan` or `execute`".to_owned())?;
    let remaining: Vec<_> = arguments.collect();
    match subcommand.to_str() {
        Some("plan") => parse_plan(remaining)
            .map(|(source, output)| StartupAction::Import(ImportCommand::Plan { source, output })),
        Some("execute") => parse_execute(remaining).map(|(plan, credentials)| {
            StartupAction::Import(ImportCommand::Execute { plan, credentials })
        }),
        _ => Err(format!(
            "unknown import command `{}`",
            subcommand.to_string_lossy()
        )),
    }
}

pub fn run(command: ImportCommand) -> Result<CliResult, String> {
    let target = desktop_config(lilia_home()?)?;
    let service = DesktopDataImportService::new(target, Arc::new(NativeDesktopHost));
    match command {
        ImportCommand::Plan { source, output } => {
            let source_identity = legacy_instance_identity(&source)?;
            let source = DesktopApplicationConfig::new(source, source_identity)
                .map_err(|error| error.to_string())?;
            let plan = service.plan(&source).map_err(|error| error.to_string())?;
            write_plan(&output, &plan)?;
            Ok(CliResult {
                exit_code: 0,
                stdout: Some(output.display().to_string()),
            })
        }
        ImportCommand::Execute { plan, credentials } => {
            let plan = read_plan(&plan)?;
            let report = service.execute(
                &plan,
                DesktopImportExecutionOptions {
                    credential_decision: credentials.into(),
                },
            );
            report_result(report)
        }
    }
}

fn desktop_config(home: impl Into<PathBuf>) -> Result<DesktopApplicationConfig, String> {
    DesktopApplicationConfig::new(home, LILIA_INSTANCE_IDENTITY).map_err(|error| error.to_string())
}

fn parse_plan(arguments: Vec<OsString>) -> Result<(PathBuf, PathBuf), String> {
    let options = parse_options(arguments, &["--source", "--output"])?;
    Ok((
        required_path(&options, "--source")?,
        required_path(&options, "--output")?,
    ))
}

fn parse_execute(arguments: Vec<OsString>) -> Result<(PathBuf, CredentialChoice), String> {
    let options = parse_options(arguments, &["--plan", "--credentials"])?;
    let plan = required_path(&options, "--plan")?;
    let credentials = match options
        .iter()
        .find(|(name, _)| name == "--credentials")
        .map(|(_, value)| value.to_string_lossy())
        .as_deref()
    {
        None | Some("deny") => CredentialChoice::Deny,
        Some("confirm") => CredentialChoice::Confirm,
        Some(other) => {
            return Err(format!(
                "invalid --credentials value `{other}`; expected deny or confirm"
            ));
        }
    };
    Ok((plan, credentials))
}

fn parse_options(
    arguments: Vec<OsString>,
    allowed: &[&str],
) -> Result<Vec<(String, OsString)>, String> {
    let mut parsed = Vec::new();
    let mut arguments = arguments.into_iter();
    while let Some(name) = arguments.next() {
        let Some(name) = name.to_str() else {
            return Err("option names must be valid UTF-8".to_owned());
        };
        if !allowed.contains(&name) {
            return Err(format!("unknown option `{name}`"));
        }
        if parsed.iter().any(|(existing, _)| existing == name) {
            return Err(format!("duplicate option `{name}`"));
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("{name} requires a value"))?;
        if value.is_empty() {
            return Err(format!("{name} must not be empty"));
        }
        parsed.push((name.to_owned(), value));
    }
    Ok(parsed)
}

fn required_path(options: &[(String, OsString)], name: &str) -> Result<PathBuf, String> {
    options
        .iter()
        .find(|(option, _)| option == name)
        .map(|(_, value)| PathBuf::from(value))
        .ok_or_else(|| format!("{name} is required"))
}

fn write_plan(path: &Path, plan: &DesktopImportPlan) -> Result<(), String> {
    let encoded = serde_json::to_vec_pretty(plan)
        .map_err(|error| format!("failed to encode import plan: {error}"))?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create plan directory: {error}"))?;
    }
    if path.exists() {
        return Err(format!(
            "import plan output already exists: {}",
            path.display()
        ));
    }
    let temporary = unique_staging_path(path)?;
    let mut staging = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("failed to create import plan staging file: {error}"))?;
    let write_result = staging
        .write_all(&encoded)
        .and_then(|_| staging.flush())
        .and_then(|_| staging.sync_all());
    drop(staging);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("failed to write import plan: {error}"));
    }
    if path.exists() {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "import plan output already exists: {}",
            path.display()
        ));
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("failed to publish import plan: {error}"));
    }
    Ok(())
}

fn unique_staging_path(path: &Path) -> Result<PathBuf, String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "import plan output must have a valid UTF-8 file name".to_owned())?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
        .as_nanos();
    let sequence = NEXT_STAGING_FILE.fetch_add(1, Ordering::Relaxed);
    Ok(parent.join(format!(
        ".{file_name}.{}.{}.{}.tmp",
        std::process::id(),
        timestamp,
        sequence
    )))
}

fn read_plan(path: &Path) -> Result<DesktopImportPlan, String> {
    let encoded = fs::read(path).map_err(|error| format!("failed to read import plan: {error}"))?;
    serde_json::from_slice(&encoded)
        .map_err(|error| format!("failed to decode import plan: {error}"))
}

fn report_result(report: DesktopImportReport) -> Result<CliResult, String> {
    let exit_code = match report.status {
        DesktopImportReportStatus::Completed | DesktopImportReportStatus::NothingToImport => 0,
        DesktopImportReportStatus::AwaitingCredentialConfirmation
        | DesktopImportReportStatus::PartialFailure
        | DesktopImportReportStatus::Failed => 1,
    };
    let stdout = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("failed to encode import report: {error}"))?;
    Ok(CliResult {
        exit_code,
        stdout: Some(stdout),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use lilia_desktop_application::{DesktopImportItemKind, DesktopImportReportItemStatus};

    use super::*;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn parses_plan_and_execute_with_credentials_denied_by_default() {
        assert_eq!(
            parse_startup(["C:/work/Lilia project with spaces".into()]).unwrap(),
            StartupAction::Launch {
                arguments: vec!["C:/work/Lilia project with spaces".into()]
            }
        );
        assert_eq!(
            parse_startup([
                "--task-handoff".into(),
                "C:/work/handoff with spaces.json".into(),
            ])
            .unwrap(),
            StartupAction::Launch {
                arguments: vec![
                    "--task-handoff".into(),
                    "C:/work/handoff with spaces.json".into(),
                ]
            }
        );
        assert_eq!(
            parse_startup([
                "import".into(),
                "plan".into(),
                "--source".into(),
                "C:/stable".into(),
                "--output".into(),
                "C:/plan.json".into(),
            ])
            .unwrap(),
            StartupAction::Import(ImportCommand::Plan {
                source: PathBuf::from("C:/stable"),
                output: PathBuf::from("C:/plan.json"),
            })
        );
        assert_eq!(
            parse_startup([
                "import".into(),
                "execute".into(),
                "--plan".into(),
                "C:/plan.json".into(),
            ])
            .unwrap(),
            StartupAction::Import(ImportCommand::Execute {
                plan: PathBuf::from("C:/plan.json"),
                credentials: CredentialChoice::Deny,
            })
        );
        assert!(parse_startup([
            "import".into(),
            "execute".into(),
            "--plan".into(),
            "plan.json".into(),
            "--credentials".into(),
            "yes".into(),
        ])
        .is_err());
        assert!(parse_startup(["import".into(), "unknown".into()]).is_err());
    }

    #[test]
    fn plan_file_roundtrips_and_execute_denies_credentials_without_host_access() {
        let root = temporary_root();
        let source_home = root.join("source");
        let target_home = root.join("target");
        let plan_file = root.join("plan.json");
        fs::create_dir_all(&source_home).unwrap();
        let source = DesktopApplicationConfig::new(
            &source_home,
            legacy_instance_identity(&source_home).unwrap(),
        )
        .unwrap();
        let target = desktop_config(&target_home).unwrap();
        let service = DesktopDataImportService::new(target, Arc::new(NativeDesktopHost));

        let plan = service.plan(&source).unwrap();
        write_plan(&plan_file, &plan).unwrap();
        let restored = read_plan(&plan_file).unwrap();
        assert_eq!(restored, plan);

        let report = service.execute(
            &restored,
            DesktopImportExecutionOptions {
                credential_decision: CredentialImportDecision::Denied,
            },
        );
        assert!(report.items.iter().any(|item| {
            item.kind == DesktopImportItemKind::Credentials
                && item.status == DesktopImportReportItemStatus::SkippedCredentialDenied
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn confirmed_empty_credential_manifest_completes_without_keyring_changes() {
        let root = temporary_root();
        let source_home = root.join("source");
        let source = DesktopApplicationConfig::new(
            &source_home,
            legacy_instance_identity(&source_home).unwrap(),
        )
        .unwrap();
        let target = desktop_config(root.join("target")).unwrap();
        let service = DesktopDataImportService::new(target, Arc::new(NativeDesktopHost));
        let plan = service.plan(&source).unwrap();
        let report = service.execute(
            &plan,
            DesktopImportExecutionOptions {
                credential_decision: CredentialImportDecision::Confirmed,
            },
        );
        let credentials = report
            .items
            .iter()
            .find(|item| item.kind == DesktopImportItemKind::Credentials)
            .unwrap();
        assert_eq!(
            credentials.status,
            DesktopImportReportItemStatus::CredentialsImported {
                imported: 0,
                skipped: 0,
                failed: 0,
            }
        );
        assert!(credentials.error.is_none());
        assert_eq!(report_result(report).unwrap().exit_code, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plan_generation_never_overwrites_an_existing_output() {
        let root = temporary_root();
        let output = root.join("plan.json");
        fs::write(&output, b"keep-this-plan").unwrap();
        let source_home = root.join("source");
        let source = DesktopApplicationConfig::new(
            &source_home,
            legacy_instance_identity(&source_home).unwrap(),
        )
        .unwrap();
        let target = desktop_config(root.join("target")).unwrap();
        let service = DesktopDataImportService::new(target, Arc::new(NativeDesktopHost));
        let plan = service.plan(&source).unwrap();

        assert!(write_plan(&output, &plan).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"keep-this-plan");
        fs::remove_dir_all(root).unwrap();
    }

    fn temporary_root() -> PathBuf {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "lilia-native-import-test-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
}

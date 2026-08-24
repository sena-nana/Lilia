use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

pub mod agent_debug;
pub mod android;
pub mod boundary;
pub mod icons;
pub mod installer_smoke;
pub mod performance;
pub mod pin;
pub mod release;
pub mod screenshot;
pub mod signing;

pub type Result<T = ()> = std::result::Result<T, XtaskError>;

#[derive(Debug)]
pub struct XtaskError {
    pub code: &'static str,
    pub message: String,
    pub blocker: bool,
}

impl XtaskError {
    pub fn failure(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            blocker: false,
        }
    }

    pub fn blocker(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            blocker: true,
        }
    }

    pub fn io(code: &'static str, action: &str, error: std::io::Error) -> Self {
        Self::failure(code, format!("{action}: {error}"))
    }
}

impl std::fmt::Display for XtaskError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for XtaskError {}

pub fn repo_root() -> Result<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().map(Path::to_path_buf).ok_or_else(|| {
        XtaskError::failure(
            "repo_root_missing",
            "xtask manifest has no parent directory",
        )
    })
}

pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command
}

pub fn run(command: &mut Command, label: &str) -> Result {
    let status = command.status().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            XtaskError::blocker(
                "external_tool_missing",
                format!("{label}: executable not found"),
            )
        } else {
            XtaskError::io("command_start_failed", label, error)
        }
    })?;
    require_success(status, label)
}

pub fn output(command: &mut Command, label: &str) -> Result<String> {
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                XtaskError::blocker(
                    "external_tool_missing",
                    format!("{label}: executable not found"),
                )
            } else {
                XtaskError::io("command_start_failed", label, error)
            }
        })?;
    if !output.status.success() {
        return Err(XtaskError::failure(
            "command_failed",
            format!(
                "{label} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| XtaskError::failure("command_output_invalid", format!("{label}: {error}")))
}

fn require_success(status: ExitStatus, label: &str) -> Result {
    if status.success() {
        Ok(())
    } else {
        Err(XtaskError::failure(
            "command_failed",
            format!("{label} exited with {status}"),
        ))
    }
}

pub fn executable(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

pub fn print_error(error: &XtaskError) {
    let payload = serde_json::json!({
        "ok": false,
        "kind": if error.blocker { "blocker" } else { "failure" },
        "code": error.code,
        "message": error.message,
    });
    eprintln!("{payload}");
}

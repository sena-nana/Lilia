use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use crate::agent_debug::{capture_window, require_ok, Session};
use crate::{repo_root, Result, XtaskError};

/// The window reports ready before the first frames settle, so the capture waits
/// out the opening layout instead of photographing a half-built shell.
const SETTLE: Duration = Duration::from_millis(1_200);

pub fn run(arguments: &[String]) -> Result {
    if !cfg!(target_os = "windows") {
        return Err(XtaskError::blocker(
            "windows_required",
            "desktop screenshots require Windows with a real WGPU desktop",
        ));
    }
    let output = parse_output(arguments)?;
    let session = Session::start("screenshot")?;
    require_ok(
        &session.request(&serde_json::json!({ "command": "observe" }))?,
        "observe",
    )?;
    thread::sleep(SETTLE);
    let output = match output {
        Some(path) if path.is_absolute() => path,
        Some(path) => repo_root()?.join(path),
        None => session.run_dir.join("desktop.png"),
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            XtaskError::io(
                "screenshot_directory_failed",
                "create screenshot directory",
                error,
            )
        })?;
    }
    capture_window(session.pid(), &output)?;
    println!("screenshot: ok ({})", output.display());
    Ok(())
}

fn parse_output(arguments: &[String]) -> Result<Option<PathBuf>> {
    match arguments {
        [] => Ok(None),
        [flag, value] if flag == "--out" => Ok(Some(PathBuf::from(value))),
        _ => Err(usage()),
    }
}

fn usage() -> XtaskError {
    XtaskError::failure("usage", "usage: cargo xtask screenshot [--out <png>]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_output_path_and_rejects_anything_else() {
        assert_eq!(parse_output(&[]).unwrap(), None);
        assert_eq!(
            parse_output(&["--out".to_owned(), "artifacts/shot.png".to_owned()]).unwrap(),
            Some(PathBuf::from("artifacts/shot.png"))
        );
        assert_eq!(
            parse_output(&["--out".to_owned()]).unwrap_err().code,
            "usage"
        );
        assert_eq!(
            parse_output(&["--nope".to_owned(), "1".to_owned()])
                .unwrap_err()
                .code,
            "usage"
        );
    }
}

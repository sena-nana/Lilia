use std::path::PathBuf;
use std::process::Command;

use crate::{PlatformError, PlatformResult};

/// Hands a path to the OS default handler. The path is canonicalised first, so
/// a missing target fails here instead of silently opening nothing.
pub fn open_path(path: PathBuf) -> PlatformResult<()> {
    let path = resolve(path, "path_open_invalid")?;
    default_opener()
        .arg(&path)
        .spawn()
        .map_err(|error| {
            PlatformError::new(
                "path_open_failed",
                format!("failed to open {}: {error}", path.display()),
                true,
            )
        })
        .map(drop)
}

/// Only HTTP and HTTPS are launched. Anything else — `file:`, `javascript:`, a
/// bare executable path — is rejected before reaching the OS.
pub fn open_external(uri: &str) -> PlatformResult<()> {
    let uri = validated_external_uri(uri)?;
    default_opener()
        .arg(&uri)
        .spawn()
        .map_err(|error| {
            PlatformError::new(
                "external_open_failed",
                format!("failed to open `{uri}`: {error}"),
                true,
            )
        })
        .map(drop)
}

pub fn open_terminal(path: PathBuf) -> PlatformResult<()> {
    let path = validated_directory(path)?;
    #[cfg(target_os = "windows")]
    {
        match Command::new("wt.exe").arg("-d").arg(&path).spawn() {
            Ok(_) => return Ok(()),
            Err(windows_terminal_error) => {
                Command::new("powershell.exe")
                    .args(["-NoExit", "-Command", "Set-Location", "-LiteralPath"])
                    .arg(&path)
                    .spawn()
                    .map_err(|powershell_error| {
                        PlatformError::new(
                            "terminal_open_failed",
                            format!(
                                "failed to open a terminal for {}: Windows Terminal: {windows_terminal_error}; PowerShell: {powershell_error}",
                                path.display()
                            ),
                            true,
                        )
                    })?;
            }
        }
    }
    #[cfg(target_os = "macos")]
    Command::new("open")
        .args(["-a", "Terminal"])
        .arg(&path)
        .spawn()
        .map_err(|error| terminal_failure(&path, error))?;
    #[cfg(all(unix, not(target_os = "macos")))]
    Command::new("x-terminal-emulator")
        .arg("--working-directory")
        .arg(&path)
        .spawn()
        .map_err(|error| terminal_failure(&path, error))?;
    Ok(())
}

pub fn open_code_editor(path: PathBuf) -> PlatformResult<()> {
    let path = validated_directory(path)?;
    let mut errors = Vec::new();
    #[cfg(target_os = "windows")]
    {
        let mut executable = Command::new("code.exe");
        executable.arg(&path);
        if spawn_ok(&mut executable, &mut errors) {
            return Ok(());
        }
        let mut command_script = Command::new("cmd.exe");
        command_script
            .args(["/D", "/S", "/C", "code.cmd"])
            .arg(&path);
        if spawn_ok(&mut command_script, &mut errors) {
            return Ok(());
        }
    }
    let mut path_command = Command::new("code");
    path_command.arg(&path);
    if spawn_ok(&mut path_command, &mut errors) {
        return Ok(());
    }
    Err(PlatformError::new(
        "code_editor_open_failed",
        format!(
            "failed to open {} in VS Code: {}",
            path.display(),
            errors.join("; ")
        ),
        true,
    ))
}

/// Canonicalises and requires an existing directory. Used for every launcher
/// that hands a working directory to another process.
pub fn validated_directory(path: PathBuf) -> PlatformResult<PathBuf> {
    let path = resolve(path, "workspace_directory_invalid")?;
    if !path.is_dir() {
        return Err(PlatformError::rejected(
            "workspace_directory_invalid",
            format!("{} is not a directory", path.display()),
        ));
    }
    Ok(path)
}

pub fn validated_external_uri(uri: &str) -> PlatformResult<String> {
    let uri = uri.trim();
    if uri.is_empty() || uri.chars().any(char::is_control) {
        return Err(PlatformError::rejected(
            "external_uri_invalid",
            "external URI must be a non-empty HTTP or HTTPS URL",
        ));
    }
    let scheme = uri
        .split_once(':')
        .map(|(scheme, _)| scheme.to_ascii_lowercase());
    if !matches!(scheme.as_deref(), Some("http" | "https")) {
        return Err(PlatformError::rejected(
            "external_uri_unsupported",
            "only HTTP and HTTPS links are opened",
        ));
    }
    Ok(uri.to_owned())
}

fn resolve(path: PathBuf, code: &'static str) -> PlatformResult<PathBuf> {
    path.canonicalize()
        .map(platform_compatible_path)
        .map_err(|error| {
            PlatformError::rejected(code, format!("failed to resolve {}: {error}", path.display()))
        })
}

/// Windows canonicalisation yields `\\?\` extended paths, which `explorer.exe`
/// and most shells refuse.
fn platform_compatible_path(path: PathBuf) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let value = path.to_string_lossy();
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}

fn default_opener() -> Command {
    #[cfg(target_os = "windows")]
    return Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    return Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    return Command::new("xdg-open");
}

#[cfg(unix)]
fn terminal_failure(path: &std::path::Path, error: std::io::Error) -> PlatformError {
    PlatformError::new(
        "terminal_open_failed",
        format!("failed to open a terminal for {}: {error}", path.display()),
        true,
    )
}

fn spawn_ok(command: &mut Command, errors: &mut Vec<String>) -> bool {
    match command.spawn() {
        Ok(_) => true,
        Err(error) => {
            errors.push(error.to_string());
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_links_accept_web_urls_and_reject_local_or_executable_schemes() {
        assert_eq!(
            validated_external_uri(" HTTPS://example.com/docs ").unwrap(),
            "HTTPS://example.com/docs"
        );
        assert_eq!(
            validated_external_uri("file:///C:/Windows/System32/cmd.exe")
                .unwrap_err()
                .code,
            "external_uri_unsupported"
        );
        assert_eq!(
            validated_external_uri("javascript:alert(1)").unwrap_err().code,
            "external_uri_unsupported"
        );
    }

    #[test]
    fn workspace_targets_require_an_existing_directory() {
        let directory = std::env::temp_dir().join(format!(
            "lilia-platform-directory-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        assert!(validated_directory(directory.clone()).unwrap().is_dir());

        let file = directory.join("file.txt");
        std::fs::write(&file, b"not a directory").unwrap();
        assert_eq!(
            validated_directory(file).unwrap_err().code,
            "workspace_directory_invalid"
        );
        assert_eq!(
            validated_directory(directory.join("missing")).unwrap_err().code,
            "workspace_directory_invalid"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}

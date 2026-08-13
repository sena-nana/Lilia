use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{command, run as run_command, Result, XtaskError};

pub fn run(arguments: &[String]) -> Result {
    if !cfg!(target_os = "windows") {
        return Err(XtaskError::blocker(
            "windows_required",
            "installer smoke requires Windows",
        ));
    }
    let root = crate::repo_root()?;
    let version = crate::release::parse_tag(required_option(arguments, "--tag")?)?;
    let installer = option(arguments, "--path")
        .map(PathBuf::from)
        .or_else(|| env::var_os("LILIA_INSTALLER_PATH").map(PathBuf::from))
        .unwrap_or_else(|| {
            root.join("artifacts/windows")
                .join(version.to_string())
                .join(format!("LiliaCode-{version}-setup.exe"))
        });
    if !installer.is_file() {
        return Err(XtaskError::failure(
            "installer_missing",
            format!("{} is not a file", installer.display()),
        ));
    }
    let sandbox = env::temp_dir().join(format!("lilia-installer-smoke-{}", timestamp()));
    let install = sandbox.join("app");
    let home = sandbox.join("home");
    fs::create_dir_all(&home).map_err(|error| {
        XtaskError::io(
            "smoke_directory_failed",
            "create installer smoke directory",
            error,
        )
    })?;
    run_command(
        command(&installer)
            .env("LILIA_HOME", &home)
            .arg("/S")
            .arg(format!("/D={}", install.display())),
        "install LiliaCode",
    )?;
    for artifact in ["liliacode.exe", "liliacode_host.dll", "liliacode.cmd"] {
        if !install.join(artifact).is_file() {
            return Err(XtaskError::failure(
                "installed_artifact_missing",
                format!("installed {artifact} is missing from {}", install.display()),
            ));
        }
    }
    require_user_path(&install, true)?;
    run_command(
        command(&installer)
            .env("LILIA_HOME", &home)
            .arg("/S")
            .arg(format!("/D={}", install.display())),
        "overwrite-update LiliaCode",
    )?;

    let binary = install.join("liliacode.exe");
    let mut first = std::process::Command::new(&binary)
        .env("LILIA_HOME", &home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            XtaskError::io(
                "installed_launch_failed",
                "launch installed LiliaCode",
                error,
            )
        })?;
    thread::sleep(Duration::from_secs(3));
    if first
        .try_wait()
        .map_err(|error| {
            XtaskError::io(
                "installed_launch_failed",
                "inspect installed LiliaCode",
                error,
            )
        })?
        .is_some()
    {
        return Err(XtaskError::failure(
            "installed_launch_failed",
            "installed LiliaCode exited before the smoke interaction",
        ));
    }
    run_command(
        command("cmd.exe")
            .env("LILIA_HOME", &home)
            .args(["/D", "/C"])
            .arg(install.join("liliacode.cmd")),
        "invoke installed liliacode CLI shim",
    )?;
    let _ = first.kill();
    let _ = first.wait();
    let uninstaller = install.join("uninstall.exe");
    run_command(command(&uninstaller).arg("/S"), "uninstall LiliaCode")?;
    if install.join("liliacode.exe").exists() {
        return Err(XtaskError::failure(
            "uninstall_incomplete",
            "uninstaller left liliacode.exe behind",
        ));
    }
    if !home.exists() {
        return Err(XtaskError::failure(
            "user_data_removed",
            "uninstaller removed LILIA_HOME",
        ));
    }
    require_user_path(&install, false)?;
    let _ = fs::remove_dir_all(&sandbox);
    println!("installer-smoke: ok");
    Ok(())
}

fn require_user_path(install: &std::path::Path, expected: bool) -> Result {
    let escaped = install.display().to_string().replace('\'', "''");
    let script = format!(
        "$target=[IO.Path]::GetFullPath('{escaped}').TrimEnd('\\'); $found=([Environment]::GetEnvironmentVariable('Path','User') -split ';' | Where-Object {{ $_ -and ([IO.Path]::GetFullPath($_).TrimEnd('\\') -ieq $target) }}).Count -gt 0; if ($found -ne ${}) {{ exit 3 }}",
        if expected { "true" } else { "false" }
    );
    run_command(
        command("powershell.exe").args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ]),
        if expected {
            "verify installed PATH"
        } else {
            "verify uninstalled PATH cleanup"
        },
    )
}

fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn option<'a>(arguments: &'a [String], name: &str) -> Option<&'a str> {
    arguments
        .iter()
        .position(|value| value == name)
        .and_then(|index| arguments.get(index + 1))
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
}

fn required_option<'a>(arguments: &'a [String], name: &str) -> Result<&'a str> {
    option(arguments, name).ok_or_else(|| {
        XtaskError::failure(
            "installer_smoke_option_missing",
            format!("{name} is required"),
        )
    })
}

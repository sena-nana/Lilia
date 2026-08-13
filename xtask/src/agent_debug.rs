use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::{executable, repo_root, run as run_command, Result, XtaskError};

const READY_TIMEOUT: Duration = Duration::from_secs(30);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Session {
    child: Child,
    address: String,
    pub run_dir: PathBuf,
    pub startup_ms: f64,
}

impl Session {
    pub fn start(profile: &str) -> Result<Self> {
        let root = repo_root()?;
        let run_dir = root
            .join("agent-debug-runs")
            .join(format!("lilia-{profile}-{}", timestamp()));
        fs::create_dir_all(&run_dir).map_err(|error| {
            XtaskError::io(
                "artifact_directory_failed",
                "create Agent Debug artifact directory",
                error,
            )
        })?;
        let ready = run_dir.join("ready.txt");
        let home = run_dir.join("home");
        fs::create_dir_all(&home).map_err(|error| {
            XtaskError::io("debug_home_failed", "create isolated LILIA_HOME", error)
        })?;

        let performance_fixture = (profile == "performance").then(|| {
            (
                root.join("tests/desktop/performance-v1.json"),
                "equivalence-performance-v1",
            )
        });
        if let Some((fixture, _)) = &performance_fixture {
            run_command(
                crate::command("cargo").current_dir(&root).args([
                    "run",
                    "--locked",
                    "--quiet",
                    "-p",
                    "lilia-desktop-application",
                    "--example",
                    "equivalence_fixture",
                    "--",
                    "--manifest",
                    fixture.to_string_lossy().as_ref(),
                    "--home",
                    home.to_string_lossy().as_ref(),
                    "--identity",
                    "liliacode",
                ]),
                "seed native performance corpus",
            )?;
        }

        run_command(
            crate::command("cargo").current_dir(&root).args([
                "build",
                "--locked",
                "-p",
                "lilia-desktop",
            ]),
            "build native desktop",
        )?;
        let binary = root.join("target/debug").join(executable("liliacode"));
        let stdout = fs::File::create(run_dir.join("desktop.stdout.log")).map_err(|error| {
            XtaskError::io("debug_log_failed", "create desktop stdout log", error)
        })?;
        let stderr = fs::File::create(run_dir.join("desktop.stderr.log")).map_err(|error| {
            XtaskError::io("debug_log_failed", "create desktop stderr log", error)
        })?;
        let started = Instant::now();
        let mut desktop = Command::new(&binary);
        desktop
            .current_dir(&root)
            .env("LILIA_HOME", &home)
            .env("LILIA_AGENT_DEBUG", "1")
            .env("LILIA_AGENT_DEBUG_ADDR", "127.0.0.1:0")
            .env("LILIA_AGENT_DEBUG_READY", &ready)
            .env("LILIA_AGENT_DEBUG_SEED", "1")
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        if let Some((_, fixture_id)) = &performance_fixture {
            desktop.env("LILIA_EQUIVALENCE_FIXTURE_ID", fixture_id);
        }
        let child = desktop.spawn().map_err(|error| {
            XtaskError::io(
                "desktop_launch_failed",
                &format!("launch {}", binary.display()),
                error,
            )
        })?;
        let address = wait_ready(&ready)?;
        let startup_ms = started.elapsed().as_secs_f64() * 1_000.0;
        Ok(Self {
            child,
            address,
            run_dir,
            startup_ms,
        })
    }

    pub fn request(&self, payload: &Value) -> Result<Value> {
        request(&self.address, payload)
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn run() -> Result {
    if !cfg!(target_os = "windows") {
        return Err(XtaskError::blocker(
            "windows_required",
            "Agent Debug acceptance requires Windows with a real WGPU desktop",
        ));
    }
    let session = Session::start("agent-debug")?;
    let observation = session.request(&serde_json::json!({ "command": "observe" }))?;
    require_ok(&observation, "observe")?;
    let action = session.request(&serde_json::json!({
        "command": "click",
        "targetId": "lilia.settings.open"
    }))?;
    require_ok(&action, "click settings")?;
    let replay = session.request(&serde_json::json!({ "command": "observe" }))?;
    require_ok(&replay, "observe after action")?;
    let errors = session.request(&serde_json::json!({ "command": "recent-errors" }))?;
    require_ok(&errors, "recent-errors")?;
    let secret_canary = "sk-native-agent-debug-fixture";
    for (label, artifact) in [
        ("observation", &observation),
        ("action", &action),
        ("replay", &replay),
        ("errors", &errors),
    ] {
        if artifact.to_string().contains(secret_canary) {
            return Err(XtaskError::failure(
                "agent_debug_secret_leak",
                format!("{label} artifact contains the secret canary"),
            ));
        }
    }
    write_json(&session.run_dir.join("observe.json"), &observation)?;
    write_json(&session.run_dir.join("action.json"), &action)?;
    write_json(&session.run_dir.join("replay.json"), &replay)?;
    write_json(&session.run_dir.join("errors.json"), &errors)?;
    capture_window(session.child.id(), &session.run_dir.join("desktop.png"))?;
    write_json(
        &session.run_dir.join("summary.json"),
        &serde_json::json!({
            "ok": true,
            "protocol": "lilia-agent-debug-v1",
            "artifacts": session.run_dir,
        }),
    )?;
    println!("agent-debug: ok ({})", session.run_dir.display());
    Ok(())
}

fn capture_window(pid: u32, output: &Path) -> Result {
    let escaped = output.display().to_string().replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName System.Drawing; $p=Get-Process -Id {pid}; if (-not $p.MainWindowHandle) {{ throw 'desktop window not ready' }}; Add-Type @'\nusing System; using System.Runtime.InteropServices; public static class LiliaRect {{ [DllImport(\"user32.dll\")] public static extern bool GetWindowRect(IntPtr h, out RECT r); public struct RECT {{ public int L,T,R,B; }} }}\n'@; $r=New-Object LiliaRect+RECT; [LiliaRect]::GetWindowRect($p.MainWindowHandle,[ref]$r)|Out-Null; $w=$r.R-$r.L; $h=$r.B-$r.T; if ($w -le 0 -or $h -le 0) {{ throw 'desktop window has invalid bounds' }}; $bmp=New-Object Drawing.Bitmap $w,$h; $g=[Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($r.L,$r.T,0,0,$bmp.Size); $bmp.Save('{escaped}',[Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()"
    );
    crate::run(
        crate::command("powershell.exe").args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ]),
        "capture native desktop screenshot",
    )?;
    if !output.is_file() || output.metadata().map(|value| value.len()).unwrap_or(0) == 0 {
        return Err(XtaskError::failure(
            "agent_debug_screenshot_missing",
            format!("screenshot was not created: {}", output.display()),
        ));
    }
    Ok(())
}

pub(crate) fn require_ok(response: &Value, command: &str) -> Result {
    if response.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(XtaskError::failure(
            "agent_debug_command_failed",
            format!("{command}: {response}"),
        ))
    }
}

fn wait_ready(path: &Path) -> Result<String> {
    let started = Instant::now();
    loop {
        if let Ok(value) = fs::read_to_string(path) {
            let address = value.trim();
            if !address.is_empty() {
                return Ok(address.to_owned());
            }
        }
        if started.elapsed() >= READY_TIMEOUT {
            return Err(XtaskError::blocker(
                "agent_debug_ready_timeout",
                format!(
                    "desktop did not create {} within {READY_TIMEOUT:?}",
                    path.display()
                ),
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn request(address: &str, request: &Value) -> Result<Value> {
    let mut stream = TcpStream::connect(address)
        .map_err(|error| XtaskError::io("agent_debug_connect_failed", address, error))?;
    stream
        .set_read_timeout(Some(RESPONSE_TIMEOUT))
        .map_err(|error| {
            XtaskError::io(
                "agent_debug_timeout_failed",
                "set debug socket timeout",
                error,
            )
        })?;
    writeln!(stream, "{request}").map_err(|error| {
        XtaskError::io("agent_debug_write_failed", "write debug request", error)
    })?;
    stream.flush().map_err(|error| {
        XtaskError::io("agent_debug_write_failed", "flush debug request", error)
    })?;
    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .map_err(|error| XtaskError::io("agent_debug_read_failed", "read debug response", error))?;
    serde_json::from_str(response.trim())
        .map_err(|error| XtaskError::failure("agent_debug_response_invalid", error.to_string()))
}

fn write_json(path: &Path, value: &Value) -> Result {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| XtaskError::failure("artifact_serialize_failed", error.to_string()))?;
    fs::write(path, bytes).map_err(|error| {
        XtaskError::io("artifact_write_failed", &path.display().to_string(), error)
    })
}

fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

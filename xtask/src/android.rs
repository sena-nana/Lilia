use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use url::Url;

use crate::{command, executable, output, repo_root, run as run_command, Result, XtaskError};

pub fn run(action: &str) -> Result {
    match action {
        "doctor" => doctor(),
        "test" => {
            doctor()?;
            gradle(&["test"])
        }
        "build" => {
            doctor()?;
            gradle(&["assembleDebug"])
        }
        "smoke" => smoke(),
        _ => Err(XtaskError::failure(
            "android_action_invalid",
            format!("unknown Android action `{action}`"),
        )),
    }
}

fn doctor() -> Result {
    let java = external_version(command("java").arg("-version"), "java", "Java runtime")?;
    let sdk = android_sdk()?;
    if !sdk.join("platforms").is_dir() || !sdk.join("build-tools").is_dir() {
        return Err(XtaskError::blocker(
            "android_sdk_incomplete",
            format!(
                "Android SDK lacks platforms or build-tools: {}",
                sdk.display()
            ),
        ));
    }
    let adb = adb_path()?;
    external_version(command(&adb).arg("version"), "adb", "Android Debug Bridge")?;
    let wrapper = gradle_wrapper()?;
    if !wrapper.is_file() {
        return Err(XtaskError::blocker(
            "gradle_wrapper_missing",
            format!("Gradle wrapper is missing: {}", wrapper.display()),
        ));
    }
    println!(
        "android doctor: ok (java output {} bytes, adb {})",
        java.len(),
        adb.display()
    );
    Ok(())
}

fn gradle(tasks: &[&str]) -> Result {
    let wrapper = gradle_wrapper()?;
    let directory = wrapper.parent().unwrap();
    let mut invocation = command(&wrapper);
    invocation
        .current_dir(directory)
        .args(tasks)
        .arg("--no-daemon");
    run_command(&mut invocation, "Android Gradle")
}

fn smoke() -> Result {
    doctor()?;
    gradle(&[":app:assembleDebug"])?;
    let adb = adb_path()?;
    let _emulator = start_avd_if_requested(&adb)?;
    let devices = output(command(&adb).args(["devices"]), "list Android devices")?;
    let authorized = authorized_devices(&devices);
    let selected = env::var("ANDROID_SERIAL").ok();
    if authorized.is_empty() {
        return Err(XtaskError::blocker(
            "android_device_missing",
            "Android smoke requires an authorized adb device",
        ));
    }
    if let Some(serial) = selected.as_deref() {
        if !authorized.iter().any(|device| device == serial) {
            return Err(XtaskError::blocker(
                "android_serial_unavailable",
                format!("ANDROID_SERIAL `{serial}` is not an authorized device"),
            ));
        }
    } else if authorized.len() > 1 {
        return Err(XtaskError::blocker(
            "android_device_ambiguous",
            format!(
                "{} devices are authorized; set ANDROID_SERIAL",
                authorized.len()
            ),
        ));
    }
    let apk = env::var_os("LILIA_ANDROID_APK")
        .map(PathBuf::from)
        .unwrap_or(repo_root()?.join("apps/android/app/build/outputs/apk/debug/app-debug.apk"));
    if !apk.is_file() {
        return Err(XtaskError::failure(
            "android_apk_missing",
            format!("built APK is missing: {}", apk.display()),
        ));
    }
    inspect_apk(&apk)?;
    let pairing = env::var("LILIA_REMOTE_PAIRING_URI").map_err(|_| {
        XtaskError::blocker(
            "android_pairing_uri_missing",
            "LILIA_REMOTE_PAIRING_URI is required for real device pairing smoke",
        )
    })?;
    let probe = PairingProbe::parse(&pairing)?;
    let before = probe.status()?;
    if before
        .pointer("/status/activeTicket/id")
        .and_then(Value::as_str)
        != Some(probe.ticket_id.as_str())
    {
        return Err(XtaskError::failure(
            "android_pairing_ticket_inactive",
            format!(
                "desktop bridge does not expose active ticket {}",
                probe.ticket_id
            ),
        ));
    }
    if probe.bridge.host_str() == Some("127.0.0.1") || probe.bridge.host_str() == Some("localhost")
    {
        let port = probe.bridge.port().ok_or_else(|| {
            XtaskError::failure(
                "android_pairing_uri_invalid",
                "pairing bridge must use an explicit port",
            )
        })?;
        run_command(
            command(&adb).args(["reverse", &format!("tcp:{port}"), &format!("tcp:{port}")]),
            "configure Android pairing bridge",
        )?;
    }
    run_command(
        command(&adb).args(["install", "-r"]).arg(&apk),
        "install Android APK",
    )?;
    let component = env::var("LILIA_ANDROID_COMPONENT")
        .unwrap_or_else(|_| "com.lilia.remote/.MainActivity".to_owned());
    if let Ok(port) = env::var("LILIA_ANDROID_REVERSE_PORT") {
        run_command(
            command(&adb).args(["reverse", &format!("tcp:{port}"), &format!("tcp:{port}")]),
            "configure Android reverse bridge",
        )?;
    }
    run_command(
        command(adb).args(["shell", "am", "start", "-W", "-n", &component]),
        "launch Android companion",
    )?;
    run_command(
        command(adb_path()?).args([
            "shell",
            "am",
            "start",
            "-W",
            "-a",
            "android.intent.action.VIEW",
            "-d",
            &pairing,
            "-n",
            &component,
        ]),
        "launch Android pairing deep link",
    )?;
    let endpoint = probe.wait_for_pairing(&before)?;
    probe.resume(&endpoint)?;
    let pid = output(
        command(adb_path()?).args(["shell", "pidof", "com.lilia.remote"]),
        "verify Android companion process",
    )?;
    if pid.trim().is_empty() {
        return Err(XtaskError::failure(
            "android_process_missing",
            "Lilia Remote is not running after pairing",
        ));
    }
    if env::var("LILIA_ANDROID_UI_TEST").as_deref() == Ok("1") {
        gradle(&[":app:connectedDebugAndroidTest"])?;
    }
    println!("android smoke: ok ({component})");
    Ok(())
}

fn start_avd_if_requested(adb: &Path) -> Result<Option<std::process::Child>> {
    let Ok(avd) = env::var("LILIA_ANDROID_AVD") else {
        return Ok(None);
    };
    let emulator = android_sdk()?.join("emulator").join(executable("emulator"));
    if !emulator.is_file() {
        return Err(XtaskError::blocker(
            "android_emulator_missing",
            format!("Android emulator is missing: {}", emulator.display()),
        ));
    }
    let available = output(
        command(&emulator).arg("-list-avds"),
        "list Android virtual devices",
    )?;
    if !available.lines().any(|candidate| candidate.trim() == avd) {
        return Err(XtaskError::blocker(
            "android_avd_missing",
            format!("Android virtual device `{avd}` is not installed"),
        ));
    }
    external_version(
        command(&emulator).arg("-accel-check"),
        "emulator",
        "Android emulator acceleration",
    )?;
    let child = std::process::Command::new(&emulator)
        .args(["-avd", &avd, "-no-snapshot-save"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| {
            XtaskError::io(
                "android_emulator_start_failed",
                "start Android emulator",
                error,
            )
        })?;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(180) {
        let devices = output(command(adb).arg("devices"), "wait for Android emulator")?;
        for serial in authorized_devices(&devices) {
            let booted = output(
                command(adb).args(["-s", &serial, "shell", "getprop", "sys.boot_completed"]),
                "check Android emulator boot",
            )?;
            if booted.trim() == "1" {
                if env::var_os("ANDROID_SERIAL").is_none() {
                    env::set_var("ANDROID_SERIAL", serial);
                }
                return Ok(Some(child));
            }
        }
        thread::sleep(Duration::from_secs(2));
    }
    Err(XtaskError::blocker(
        "android_emulator_boot_timeout",
        format!("Android virtual device `{avd}` did not boot within 180 seconds"),
    ))
}

struct PairingProbe {
    ticket_id: String,
    bridge: Url,
    client: reqwest::blocking::Client,
}

impl PairingProbe {
    fn parse(pairing: &str) -> Result<Self> {
        let uri = Url::parse(pairing.trim()).map_err(|error| {
            XtaskError::failure("android_pairing_uri_invalid", error.to_string())
        })?;
        if !matches!(uri.scheme(), "lilia-remote" | "lilia-voice") || uri.host_str() != Some("pair")
        {
            return Err(XtaskError::failure(
                "android_pairing_uri_invalid",
                "pairing URI must use lilia-remote://pair or lilia-voice://pair",
            ));
        }
        let query = uri
            .query_pairs()
            .collect::<std::collections::BTreeMap<_, _>>();
        let ticket_id = query
            .get("ticket")
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .ok_or_else(|| {
                XtaskError::failure("android_pairing_uri_invalid", "pairing ticket is missing")
            })?;
        let bridge = query
            .get("bridge")
            .map(|value| Url::parse(value))
            .transpose()
            .map_err(|error| XtaskError::failure("android_pairing_uri_invalid", error.to_string()))?
            .ok_or_else(|| {
                XtaskError::failure("android_pairing_uri_invalid", "pairing bridge is missing")
            })?;
        if !matches!(bridge.scheme(), "http" | "https")
            || bridge.host_str().is_none()
            || bridge.port().is_none()
        {
            return Err(XtaskError::failure(
                "android_pairing_uri_invalid",
                "pairing bridge must be HTTP(S) with an explicit port",
            ));
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|error| {
                XtaskError::failure("android_bridge_client_failed", error.to_string())
            })?;
        Ok(Self {
            ticket_id,
            bridge,
            client,
        })
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.bridge
            .join(path)
            .map_err(|error| XtaskError::failure("android_bridge_url_invalid", error.to_string()))
    }

    fn status(&self) -> Result<Value> {
        self.client
            .get(self.endpoint("/status")?)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .and_then(reqwest::blocking::Response::json)
            .map_err(|error| XtaskError::failure("android_bridge_status_failed", error.to_string()))
    }

    fn wait_for_pairing(&self, before: &Value) -> Result<String> {
        let previous = trusted_endpoints(before);
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(30) {
            let status = self.status()?;
            let active = status
                .pointer("/status/activeTicket/id")
                .and_then(Value::as_str);
            if active != Some(self.ticket_id.as_str()) {
                if let Some(endpoint) = trusted_endpoints(&status)
                    .into_iter()
                    .find(|endpoint| !previous.contains(endpoint))
                    .or_else(|| trusted_endpoints(&status).into_iter().next())
                {
                    return Ok(endpoint);
                }
            }
            thread::sleep(Duration::from_secs(1));
        }
        Err(XtaskError::failure(
            "android_pairing_timeout",
            "desktop bridge did not consume the pairing ticket and trust the Android endpoint",
        ))
    }

    fn resume(&self, endpoint: &str) -> Result {
        let sent_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let response: Value = self
            .client
            .post(self.endpoint("/dispatch")?)
            .json(&serde_json::json!({
                "id": "android-smoke-resume",
                "protocolVersion": 1,
                "sentAt": sent_at,
                "deviceId": endpoint,
                "request": { "type": "connection.resume", "androidEndpointId": endpoint }
            }))
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .and_then(reqwest::blocking::Response::json)
            .map_err(|error| XtaskError::failure("android_resume_failed", error.to_string()))?;
        if response.get("ok").and_then(Value::as_bool) != Some(true)
            || response
                .pointer("/payload/accepted")
                .and_then(Value::as_bool)
                != Some(true)
        {
            return Err(XtaskError::failure(
                "android_resume_rejected",
                "desktop bridge rejected connection.resume",
            ));
        }
        Ok(())
    }
}

fn trusted_endpoints(status: &Value) -> Vec<String> {
    status
        .pointer("/status/trustedDevices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|device| device.get("trusted").and_then(Value::as_bool) != Some(false))
        .filter_map(|device| device.get("endpointId").and_then(Value::as_str))
        .filter(|endpoint| !endpoint.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn external_version(
    command: &mut std::process::Command,
    tool: &str,
    label: &str,
) -> Result<String> {
    let result = command.output().map_err(|error| {
        XtaskError::blocker(
            "external_tool_missing",
            format!("{label}: `{tool}` could not be started: {error}"),
        )
    })?;
    let mut text = String::from_utf8_lossy(&result.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&result.stderr));
    if result.status.success() {
        Ok(text)
    } else {
        let detail = if text.trim().is_empty() {
            format!("`{tool}` exited with {}", result.status)
        } else {
            text.trim().to_owned()
        };
        Err(XtaskError::blocker(
            "external_tool_unavailable",
            format!("{label}: {detail}"),
        ))
    }
}

fn authorized_devices(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?;
            (fields.next() == Some("device")).then(|| serial.to_owned())
        })
        .collect()
}

fn inspect_apk(apk: &Path) -> Result {
    let sdk = env::var_os("ANDROID_SDK_ROOT")
        .or_else(|| env::var_os("ANDROID_HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| {
            XtaskError::blocker(
                "android_sdk_missing",
                "ANDROID_SDK_ROOT or ANDROID_HOME is required for APK inspection",
            )
        })?;
    let build_tools = sdk.join("build-tools");
    let mut versions = fs::read_dir(&build_tools)
        .map_err(|error| XtaskError::io("android_build_tools_missing", "read build-tools", error))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect::<Vec<_>>();
    versions.sort();
    let aapt2 = versions
        .into_iter()
        .rev()
        .map(|path| path.join(executable("aapt2")))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            XtaskError::blocker("aapt2_missing", "Android SDK build-tools lacks aapt2")
        })?;
    let manifest = output(
        command(aapt2)
            .args(["dump", "xmltree"])
            .arg(apk)
            .arg("AndroidManifest.xml"),
        "inspect Android APK manifest",
    )?;
    if !manifest.contains("com.lilia.remote") {
        return Err(XtaskError::failure(
            "android_manifest_invalid",
            "APK manifest does not contain the Lilia remote package",
        ));
    }
    Ok(())
}

fn gradle_wrapper() -> Result<PathBuf> {
    let root = repo_root()?;
    Ok(root.join("apps/android").join(if cfg!(windows) {
        "gradlew.bat"
    } else {
        "gradlew"
    }))
}

fn android_sdk() -> Result<PathBuf> {
    if let Some(path) = env::var_os("ANDROID_SDK_ROOT").or_else(|| env::var_os("ANDROID_HOME")) {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Ok(path);
        }
    }
    let properties = repo_root()?.join("apps/android/local.properties");
    if let Ok(source) = fs::read_to_string(&properties) {
        if let Some(value) = source
            .lines()
            .find_map(|line| line.strip_prefix("sdk.dir="))
        {
            let path = PathBuf::from(value.replace("\\\\", "\\"));
            if path.is_dir() {
                return Ok(path);
            }
        }
    }
    Err(XtaskError::blocker(
        "android_sdk_missing",
        "ANDROID_SDK_ROOT, ANDROID_HOME, or apps/android/local.properties must identify the Android SDK",
    ))
}

fn adb_path() -> Result<PathBuf> {
    if let Some(sdk) = env::var_os("ANDROID_SDK_ROOT").or_else(|| env::var_os("ANDROID_HOME")) {
        let path = Path::new(&sdk)
            .join("platform-tools")
            .join(executable("adb"));
        if path.is_file() {
            return Ok(path);
        }
    }
    Ok(PathBuf::from(executable("adb")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adb_device_parser_keeps_only_authorized_devices() {
        let output = "List of devices attached\nemulator-5554\tdevice product:sdk\nphone\tunauthorized\noffline\toffline\n";
        assert_eq!(authorized_devices(output), vec!["emulator-5554"]);
    }

    #[test]
    fn pairing_probe_requires_a_real_versioned_bridge_ticket() {
        let probe = PairingProbe::parse(
            "lilia-remote://pair?v=1&ticket=ticket-1&challenge=challenge-1&endpoint=pc-1&bridge=http%3A%2F%2F127.0.0.1%3A41478",
        )
        .unwrap();
        assert_eq!(probe.ticket_id, "ticket-1");
        assert_eq!(probe.bridge.as_str(), "http://127.0.0.1:41478/");
        assert!(PairingProbe::parse("https://example.test/pair").is_err());
    }

    #[test]
    fn trusted_endpoint_parser_excludes_revoked_devices() {
        let status = serde_json::json!({
            "status": { "trustedDevices": [
                { "endpointId": "phone-1", "trusted": true },
                { "endpointId": "phone-2", "trusted": false }
            ] }
        });
        assert_eq!(trusted_endpoints(&status), vec!["phone-1"]);
    }
}

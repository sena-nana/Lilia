use lilia_desktop_application::{
    DesktopApplication, RemoteControlStatus, RemotePairDeviceInput, RemotePairingTicket,
    RemotePeerSummary, RemoteRequestEnvelope,
};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub fn remote_control_status(
    application: State<'_, DesktopApplication>,
) -> Result<RemoteControlStatus, String> {
    application
        .remote_control_status()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remote_control_set_host_enabled(
    enabled: bool,
    application: State<'_, DesktopApplication>,
) -> Result<RemoteControlStatus, String> {
    application
        .set_remote_control_enabled(enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remote_control_set_pc_name(
    name: String,
    application: State<'_, DesktopApplication>,
) -> Result<RemoteControlStatus, String> {
    application
        .set_remote_control_pc_name(name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remote_control_set_keep_awake_enabled(
    enabled: bool,
    application: State<'_, DesktopApplication>,
) -> Result<RemoteControlStatus, String> {
    application
        .set_remote_control_keep_awake(enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remote_control_start_pairing(
    application: State<'_, DesktopApplication>,
) -> Result<RemotePairingTicket, String> {
    application
        .start_remote_pairing()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remote_control_cancel_pairing(
    application: State<'_, DesktopApplication>,
) -> Result<(), String> {
    application
        .cancel_remote_pairing()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remote_control_pair_device(
    input: RemotePairDeviceInput,
    application: State<'_, DesktopApplication>,
) -> Result<RemotePeerSummary, String> {
    application
        .pair_remote_device(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remote_control_revoke_device(
    device_id: String,
    application: State<'_, DesktopApplication>,
) -> Result<RemoteControlStatus, String> {
    application
        .revoke_remote_device(&device_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remote_control_dispatch_request(
    envelope: RemoteRequestEnvelope,
    application: State<'_, DesktopApplication>,
) -> Value {
    application.dispatch_remote_request(envelope)
}

pub(crate) fn restore_http_bridge_if_enabled(app: &AppHandle) {
    let Some(application) = app.try_state::<DesktopApplication>() else {
        return;
    };
    if let Err(error) = application.restore_remote_control() {
        eprintln!("[remote-control] restore failed: {error}");
    }
}

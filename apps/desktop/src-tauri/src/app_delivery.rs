use std::path::PathBuf;
use std::sync::Arc;

use mutsuki_runtime_contracts::{CapabilityDescriptor, DeliveryReceipt};
use mutsuki_tauri_host::{AppCapabilityEndpoint, AppId};
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

use crate::task_handoff;

const LILIA_CODE_APP_ID: &str = "lilia.code";
const LILIA_CODE_CAPABILITY: &str = "lilia.code.task.accept";

pub fn start_app_delivery_endpoint<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let lease_dir = std::env::temp_dir()
        .join("mutsuki-app-delivery")
        .join("lilia-code");
    std::fs::create_dir_all(&lease_dir)
        .map_err(|error| format!("创建 LiliaCode endpoint 目录失败：{error}"))?;
    let endpoint = AppCapabilityEndpoint::open(
        AppId::new(LILIA_CODE_APP_ID).map_err(|error| error.to_string())?,
        format!("code-{}", std::process::id()),
        &lease_dir,
    )
    .map_err(|error| error.to_string())?;
    let app_handle = app.clone();
    endpoint.register_handler(
        CapabilityDescriptor::new(LILIA_CODE_CAPABILITY, 1, 1),
        move |envelope| match accept_handoff_payload(&app_handle, envelope.payload) {
            Ok(task_id) => DeliveryReceipt::Accepted {
                request_id: envelope.request_id,
                remote_task_id: Some(task_id),
            },
            Err(message) => DeliveryReceipt::Failed {
                request_id: envelope.request_id,
                remote_task_id: None,
                code: "handoff_rejected".into(),
                message,
            },
        },
    );
    app.manage(Arc::new(endpoint));
    Ok(())
}

fn accept_handoff_payload<R: Runtime>(
    app: &AppHandle<R>,
    payload: Value,
) -> Result<String, String> {
    let payload_json = serde_json::to_string(&payload)
        .map_err(|error| format!("序列化交接 payload 失败：{error}"))?;
    let handoff_id = payload
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "交接 payload 缺少 id".to_string())?;
    let path = std::env::temp_dir()
        .join("lilia-code-task-handoffs")
        .join(format!("{handoff_id}.direct.json"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("创建临时交接目录失败：{error}"))?;
    }
    std::fs::write(&path, payload_json.as_bytes())
        .map_err(|error| format!("写入临时交接失败：{error}"))?;
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let open = task_handoff::resolve_task_handoff(app, &path, &cwd)?;
    Ok(open.task_id)
}

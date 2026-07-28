use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::{
    utils::config::Color, AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::settings_store::{load_store_value, normalize_optional_string, save_store_value};
use crate::store::LiliaStore;
use crate::{app_events_contract, BG, MAIN_WINDOW_LABEL};

const POPUP_WINDOW_SETTINGS_KEY: &str = "popup-window.config";
const POPUP_LAST_PROJECT_KEY: &str = "popup-window.lastProjectId";
const POPUP_WINDOW_PREFIX: &str = "popup-";
const POPUP_EXISTING_TASK_PREFIX: &str = "popup-task-";
pub(crate) const CONVERSATION_STATUS_WINDOW_LABEL: &str = "conversation-status";
const POPUP_WIDTH: f64 = 430.0;
const POPUP_HEIGHT: f64 = 760.0;
const POPUP_MIN_WIDTH: f64 = 320.0;
const POPUP_MIN_HEIGHT: f64 = 420.0;
const STATUS_WINDOW_WIDTH: f64 = 360.0;
const STATUS_WINDOW_HEIGHT: f64 = 520.0;
const STATUS_WINDOW_MIN_WIDTH: f64 = 260.0;
const STATUS_WINDOW_MIN_HEIGHT: f64 = 320.0;
const TRANSPARENT_BG: Color = Color(0x00, 0x00, 0x00, 0x00);
static POPUP_WINDOW_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PopupWindowSettings {
    pub(crate) shortcut: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MainNavigateEvent {
    pub(crate) route: String,
}

pub(crate) fn normalize_popup_window_settings(
    settings: PopupWindowSettings,
) -> PopupWindowSettings {
    PopupWindowSettings {
        shortcut: normalize_optional_string(settings.shortcut),
    }
}

pub(crate) fn load_popup_window_settings(app: &AppHandle) -> PopupWindowSettings {
    normalize_popup_window_settings(
        load_store_value(app, POPUP_WINDOW_SETTINGS_KEY).unwrap_or_default(),
    )
}

pub(crate) fn save_popup_window_settings(
    app: &AppHandle,
    settings: &PopupWindowSettings,
) -> Result<(), String> {
    save_store_value(app, POPUP_WINDOW_SETTINGS_KEY, settings)
}

pub(crate) fn load_popup_last_project_id(app: &AppHandle) -> Option<String> {
    normalize_optional_string(load_store_value(app, POPUP_LAST_PROJECT_KEY))
}

pub(crate) fn save_popup_last_project_id(app: &AppHandle, project_id: &str) -> Result<(), String> {
    save_store_value(app, POPUP_LAST_PROJECT_KEY, &project_id)
}

pub(crate) fn project_exists(app: &AppHandle, project_id: &str) -> bool {
    let Some(store) = app.try_state::<LiliaStore>() else {
        return false;
    };
    let Ok(conn) = store.conn() else {
        return false;
    };
    conn.query_row(
        "SELECT 1 FROM projects WHERE id = ?1 LIMIT 1",
        rusqlite::params![project_id],
        |_| Ok(()),
    )
    .is_ok()
}

pub(crate) fn sanitize_window_label_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

pub(crate) fn unique_popup_label(kind: &str) -> String {
    let seq = POPUP_WINDOW_SEQ.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    format!("{POPUP_WINDOW_PREFIX}{kind}-{now}-{seq}")
}

pub(crate) fn popup_task_label(task_id: &str) -> String {
    format!(
        "{POPUP_EXISTING_TASK_PREFIX}{}",
        sanitize_window_label_segment(task_id)
    )
}

pub(crate) fn normalize_popup_route(route: &str) -> String {
    let route = route.trim();
    if route.starts_with('/') {
        route.to_string()
    } else {
        format!("/{route}")
    }
}

pub(crate) fn popup_route_bootstrap_script(route: &str) -> String {
    let route = serde_json::to_string(&normalize_popup_route(route))
        .unwrap_or_else(|_| "\"/popup/chats/new\"".to_string());
    format!("window.location.hash = \"#\" + {route};")
}

pub(crate) fn focus_window<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

pub(crate) fn build_popup_window(
    app: &AppHandle,
    label: String,
    route: String,
) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(&label) {
        existing
            .emit(
                app_events_contract::popup_navigate_event_name(),
                MainNavigateEvent {
                    route: normalize_popup_route(&route),
                },
            )
            .map_err(|e| format!("通知弹出窗口导航失败：{e}"))?;
        focus_window(&existing);
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .initialization_script(popup_route_bootstrap_script(&route))
        .title("LiliaCode")
        .inner_size(POPUP_WIDTH, POPUP_HEIGHT)
        .min_inner_size(POPUP_MIN_WIDTH, POPUP_MIN_HEIGHT)
        .center()
        .decorations(false)
        .resizable(true)
        .background_color(BG)
        .build()
        .map_err(|e| format!("创建弹出窗口失败：{e}"))?;
    focus_window(&window);
    Ok(())
}

pub(crate) fn open_conversation_status_window(app: &AppHandle) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(CONVERSATION_STATUS_WINDOW_LABEL) {
        focus_window(&existing);
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(
        app,
        CONVERSATION_STATUS_WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .initialization_script(popup_route_bootstrap_script("/popup/status"))
    .title("对话状态")
    .inner_size(STATUS_WINDOW_WIDTH, STATUS_WINDOW_HEIGHT)
    .min_inner_size(STATUS_WINDOW_MIN_WIDTH, STATUS_WINDOW_MIN_HEIGHT)
    .center()
    .decorations(false)
    .resizable(true)
    .background_color(TRANSPARENT_BG);
    // macOS requires `macos-private-api` for transparent windows; Windows/Linux keep it.
    #[cfg(not(target_os = "macos"))]
    let builder = builder.transparent(true);
    let window = builder
        .build()
        .map_err(|e| format!("创建对话状态悬浮窗失败：{e}"))?;
    let _ = window.set_background_color(Some(TRANSPARENT_BG));
    focus_window(&window);
    Ok(())
}

pub(crate) fn toggle_conversation_status_window(app: &AppHandle) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(CONVERSATION_STATUS_WINDOW_LABEL) {
        existing
            .close()
            .map_err(|e| format!("关闭对话状态悬浮窗失败：{e}"))?;
        return Ok(());
    }
    open_conversation_status_window(app)
}

fn route_with_initial_draft(route: String, initial_draft_content: Option<String>) -> String {
    let Some(content) = initial_draft_content else {
        return route;
    };
    if content.trim().is_empty() {
        return route;
    }
    let encoded = URL_SAFE_NO_PAD.encode(content.as_bytes());
    format!("{route}?draft={encoded}")
}

pub(crate) fn open_new_chat_window(
    app: &AppHandle,
    project_id: Option<String>,
    initial_draft_content: Option<String>,
    allow_orphan_fallback: bool,
) -> Result<(), String> {
    let route = if let Some(project_id) = normalize_optional_string(project_id) {
        if project_exists(app, &project_id) {
            save_popup_last_project_id(app, &project_id)?;
            format!("popup/projects/{project_id}/new")
        } else if allow_orphan_fallback {
            "popup/chats/new".to_string()
        } else {
            return Err("项目不存在，无法在弹出窗口中创建对话".to_string());
        }
    } else {
        "popup/chats/new".to_string()
    };
    let route = route_with_initial_draft(route, initial_draft_content);
    build_popup_window(app, unique_popup_label("new"), route)
}

pub(crate) fn open_popup_for_shortcut(app: &AppHandle) -> Result<(), String> {
    let project_id = load_popup_last_project_id(app).filter(|id| project_exists(app, id));
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(err) = open_new_chat_window(&app, project_id, None, true) {
            eprintln!("{err}");
        }
    });
    Ok(())
}

pub(crate) fn open_task_window(
    app: &AppHandle,
    project_id: Option<String>,
    task_id: String,
) -> Result<(), String> {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        return Err("缺少对话 ID".to_string());
    }
    let route = if let Some(project_id) = normalize_optional_string(project_id) {
        save_popup_last_project_id(app, &project_id)?;
        format!("popup/projects/{project_id}/tasks/{task_id}")
    } else {
        format!("popup/chats/{task_id}")
    };
    build_popup_window(app, popup_task_label(task_id), route)
}

pub(crate) fn open_child_question_window(
    app: &AppHandle,
    project_id: Option<String>,
    parent_task_id: String,
) -> Result<(), String> {
    let parent_task_id = parent_task_id.trim();
    if parent_task_id.is_empty() {
        return Err("缺少父对话 ID".to_string());
    }
    let route = if let Some(project_id) = normalize_optional_string(project_id) {
        save_popup_last_project_id(app, &project_id)?;
        format!("popup/projects/{project_id}/new")
    } else {
        "popup/chats/new".to_string()
    };
    let encoded = URL_SAFE_NO_PAD.encode(parent_task_id.as_bytes());
    build_popup_window(
        app,
        unique_popup_label("ask"),
        format!("{route}?parentTaskId={encoded}"),
    )
}

pub(crate) fn register_popup_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| format!("注册弹出窗口快捷键失败：{e}"))
}

pub(crate) fn unregister_popup_shortcut(app: &AppHandle, shortcut: &str) {
    if let Err(err) = app.global_shortcut().unregister(shortcut) {
        eprintln!("[popup-window] unregister shortcut failed: {err}");
    }
}

pub(crate) fn register_initial_popup_shortcut(app: &AppHandle) {
    let settings = load_popup_window_settings(app);
    if let Some(shortcut) = settings.shortcut.as_deref() {
        if let Err(err) = register_popup_shortcut(app, shortcut) {
            eprintln!("[popup-window] register shortcut failed: {err}");
        }
    }
}

#[tauri::command]
pub fn popup_get_window_settings(app: AppHandle) -> PopupWindowSettings {
    load_popup_window_settings(&app)
}

#[tauri::command]
pub fn popup_set_window_settings(
    app: AppHandle,
    settings: PopupWindowSettings,
) -> Result<(), String> {
    let previous = load_popup_window_settings(&app);
    let next = normalize_popup_window_settings(settings);
    if previous.shortcut == next.shortcut {
        return save_popup_window_settings(&app, &next);
    }

    if let Some(shortcut) = previous.shortcut.as_deref() {
        unregister_popup_shortcut(&app, shortcut);
    }

    if let Some(shortcut) = next.shortcut.as_deref() {
        if let Err(err) = register_popup_shortcut(&app, shortcut) {
            if let Some(previous_shortcut) = previous.shortcut.as_deref() {
                let _ = register_popup_shortcut(&app, previous_shortcut);
            }
            return Err(err);
        }
    }

    save_popup_window_settings(&app, &next)
}

#[tauri::command]
pub fn popup_remember_last_project(app: AppHandle, project_id: String) -> Result<(), String> {
    let project_id = project_id.trim();
    if project_id.is_empty() || !project_exists(&app, project_id) {
        return Ok(());
    }
    save_popup_last_project_id(&app, project_id)
}

#[tauri::command]
pub async fn popup_open_new_chat(
    app: AppHandle,
    project_id: Option<String>,
    initial_draft_content: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        open_new_chat_window(&app, project_id, initial_draft_content, false)
    })
    .await
    .map_err(|err| format!("弹出窗口任务执行失败：{err}"))?
}

#[tauri::command]
pub async fn popup_open_task(
    app: AppHandle,
    project_id: Option<String>,
    task_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || open_task_window(&app, project_id, task_id))
        .await
        .map_err(|err| format!("弹出窗口任务执行失败：{err}"))?
}

#[tauri::command]
pub async fn popup_open_child_question(
    app: AppHandle,
    project_id: Option<String>,
    parent_task_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        open_child_question_window(&app, project_id, parent_task_id)
    })
    .await
    .map_err(|err| format!("弹出窗口任务执行失败：{err}"))?
}

#[tauri::command]
pub async fn popup_open_conversation_status(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || open_conversation_status_window(&app))
        .await
        .map_err(|err| format!("悬浮窗任务执行失败：{err}"))?
}

#[tauri::command]
pub async fn popup_toggle_conversation_status(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || toggle_conversation_status_window(&app))
        .await
        .map_err(|err| format!("悬浮窗任务执行失败：{err}"))?
}

#[tauri::command]
pub fn popup_focus_main(app: AppHandle, route: String) -> Result<(), String> {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return Err("主窗口不存在".to_string());
    };
    window
        .emit(
            app_events_contract::main_navigate_event_name(),
            MainNavigateEvent { route },
        )
        .map_err(|e| format!("通知主窗口导航失败：{e}"))?;
    focus_window(&window);
    Ok(())
}

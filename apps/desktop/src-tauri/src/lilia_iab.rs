#[cfg(windows)]
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use uuid::Uuid;

use crate::agent_events::runner_lilia_iab_result_control_type;
use crate::chat::attachments::describe::describe_attachment_path;
use crate::chat::runner::write_runner_stdin_for_task;
use crate::chat::state::{now_millis, ChatStore, RunningTurn};
use crate::chat::types::ChatAttachment;
use crate::popup_windows::focus_window;
use crate::store;
use crate::{BACKEND_CODEX, BG};

const IAB_WIDTH: f64 = 1180.0;
const IAB_HEIGHT: f64 = 760.0;
const IAB_MIN_WIDTH: f64 = 720.0;
const IAB_MIN_HEIGHT: f64 = 480.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LiliaIabSnapshot {
    pub(crate) task_id: String,
    pub(crate) url: String,
    pub(crate) title: Option<String>,
    pub(crate) note: Option<String>,
    pub(crate) captured_at: u64,
    pub(crate) screenshot_path: Option<String>,
    pub(crate) screenshot_attachment: Option<ChatAttachment>,
    pub(crate) status: String,
    pub(crate) warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LiliaIabSubmitResult {
    pub(crate) snapshot: LiliaIabSnapshot,
    pub(crate) delivery: String,
    pub(crate) stdin_forwarded: bool,
}

pub(crate) fn iab_window_label(task_id: &str) -> String {
    let mut out = String::from("lilia-iab-");
    for ch in task_id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    if out.len() > 80 {
        out.truncate(80);
    }
    out
}

fn iab_title(task_id: &str) -> String {
    format!("Lilia IAB · {task_id}")
}

fn normalize_note(note: Option<String>) -> Option<String> {
    note.map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_iab_url(url: Option<String>) -> Result<tauri::Url, String> {
    let raw = url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "about:blank".to_string());
    tauri::Url::parse(&raw).map_err(|e| format!("IAB URL 无效：{e}"))
}

fn iab_snapshots_cache_dir(home: &Path) -> PathBuf {
    home.join("cache").join("iab-snapshots")
}

fn snapshot_path(home: &Path, task_id: &str, now: u64) -> PathBuf {
    iab_snapshots_cache_dir(home).join(format!(
        "iab-{now}-{}-{}.png",
        safe_filename_segment(task_id),
        Uuid::new_v4()
    ))
}

fn safe_filename_segment(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars().take(40) {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    if out.is_empty() {
        "task".to_string()
    } else {
        out
    }
}

fn describe_iab_screenshot(path: &Path) -> ChatAttachment {
    let mut attachment = describe_attachment_path(path.to_string_lossy().to_string());
    attachment.name = "IAB 截图.png".to_string();
    attachment.mime = Some("image/png".to_string());
    attachment
}

fn current_iab_title<R: Runtime>(window: &WebviewWindow<R>) -> Option<String> {
    let (tx, rx) = mpsc::channel();
    let script = r#"(function () { return document.title || ""; })()"#;
    if window
        .eval_with_callback(script, move |value| {
            let _ = tx.send(value);
        })
        .is_err()
    {
        return None;
    }
    rx.recv_timeout(Duration::from_millis(500))
        .ok()
        .and_then(|value| serde_json::from_str::<String>(&value).ok().or(Some(value)))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn window_url<R: Runtime>(window: &WebviewWindow<R>) -> String {
    window
        .url()
        .map(|url| url.to_string())
        .unwrap_or_else(|_| "about:blank".to_string())
}

#[cfg(windows)]
fn capture_window_png<R: Runtime>(window: &WebviewWindow<R>, path: &Path) -> Result<(), String> {
    use image::{ImageBuffer, Rgba};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ptr::null_mut;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HBITMAP, HDC, HGDIOBJ, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

    struct DcGuard {
        hwnd: HWND,
        hdc: HDC,
    }
    impl Drop for DcGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = ReleaseDC(Some(self.hwnd), self.hdc);
            }
        }
    }

    struct MemDcGuard(HDC);
    impl Drop for MemDcGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteDC(self.0);
            }
        }
    }

    struct BitmapGuard(HBITMAP);
    impl Drop for BitmapGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(self.0 .0));
            }
        }
    }

    let handle = window
        .window_handle()
        .map_err(|e| format!("读取 IAB 窗口句柄失败：{e}"))?;
    let hwnd = match handle.as_raw() {
        RawWindowHandle::Win32(raw) => HWND(raw.hwnd.get() as *mut _),
        _ => return Err("当前平台不支持 Windows IAB 截图".to_string()),
    };

    unsafe {
        let mut rect = Default::default();
        GetClientRect(hwnd, &mut rect).map_err(|e| format!("读取 IAB 窗口尺寸失败：{e}"))?;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return Err("IAB 窗口尺寸无效，无法截图".to_string());
        }

        let hdc = GetDC(Some(hwnd));
        if hdc.0 == null_mut() {
            return Err("获取 IAB 窗口 DC 失败".to_string());
        }
        let hdc_guard = DcGuard { hwnd, hdc };
        let mem_dc = CreateCompatibleDC(Some(hdc_guard.hdc));
        if mem_dc.0 == null_mut() {
            return Err("创建 IAB 截图 DC 失败".to_string());
        }
        let mem_guard = MemDcGuard(mem_dc);
        let bitmap = CreateCompatibleBitmap(hdc_guard.hdc, width, height);
        if bitmap.0 == null_mut() {
            return Err("创建 IAB 截图位图失败".to_string());
        }
        let bitmap_guard = BitmapGuard(bitmap);
        let old = SelectObject(mem_guard.0, HGDIOBJ(bitmap_guard.0 .0));
        if old.0 == null_mut() {
            return Err("选择 IAB 截图位图失败".to_string());
        }
        BitBlt(
            mem_guard.0,
            0,
            0,
            width,
            height,
            Some(hdc_guard.hdc),
            0,
            0,
            SRCCOPY,
        )
        .map_err(|e| format!("复制 IAB 窗口像素失败：{e}"))?;

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bgra = vec![0_u8; (width as usize) * (height as usize) * 4];
        let rows = GetDIBits(
            mem_guard.0,
            bitmap_guard.0,
            0,
            height as u32,
            Some(bgra.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        if rows == 0 {
            return Err("读取 IAB 截图像素失败".to_string());
        }
        for chunk in bgra.chunks_exact_mut(4) {
            chunk.swap(0, 2);
            chunk[3] = 255;
        }
        let image = ImageBuffer::<Rgba<u8>, _>::from_raw(width as u32, height as u32, bgra)
            .ok_or_else(|| "组装 IAB 截图失败".to_string())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建 IAB 截图目录失败：{e}"))?;
        }
        image
            .save(path)
            .map_err(|e| format!("保存 IAB 截图失败：{e}"))?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn capture_window_png<R: Runtime>(_window: &WebviewWindow<R>, _path: &Path) -> Result<(), String> {
    Err("当前平台暂不支持 IAB 截图".to_string())
}

fn build_snapshot<R: Runtime>(
    task_id: &str,
    note: Option<String>,
    window: &WebviewWindow<R>,
) -> LiliaIabSnapshot {
    let captured_at = now_millis();
    let note = normalize_note(note);
    let url = window_url(window);
    let title = current_iab_title(window);
    let home = store::resolve_lilia_home();
    let path = snapshot_path(&home, task_id, captured_at);
    match capture_window_png(window, &path) {
        Ok(()) => {
            let attachment = describe_iab_screenshot(&path);
            LiliaIabSnapshot {
                task_id: task_id.to_string(),
                url,
                title,
                note,
                captured_at,
                screenshot_path: Some(path.to_string_lossy().to_string()),
                screenshot_attachment: Some(attachment),
                status: "captured".to_string(),
                warning: None,
            }
        }
        Err(err) => LiliaIabSnapshot {
            task_id: task_id.to_string(),
            url,
            title,
            note,
            captured_at,
            screenshot_path: None,
            screenshot_attachment: None,
            status: "metadata_only".to_string(),
            warning: Some(err),
        },
    }
}

fn lilia_iab_result_payload(snapshot: &LiliaIabSnapshot) -> JsonValue {
    serde_json::json!({
        "type": runner_lilia_iab_result_control_type(),
        "snapshot": snapshot,
    })
}

fn submit_result(snapshot: LiliaIabSnapshot, stdin_forwarded: bool) -> LiliaIabSubmitResult {
    LiliaIabSubmitResult {
        snapshot,
        delivery: if stdin_forwarded { "runner" } else { "message" }.to_string(),
        stdin_forwarded,
    }
}

fn can_forward_lilia_iab_to_codex_runner(running: Option<&RunningTurn>) -> bool {
    running.is_some_and(|turn| turn.backend == BACKEND_CODEX)
}

fn forward_lilia_iab_to_running_backend(
    store: &ChatStore,
    task_id: &str,
    snapshot: &LiliaIabSnapshot,
) -> Result<bool, String> {
    let can_forward_to_codex = {
        let turns = store.running_turns.lock().unwrap();
        can_forward_lilia_iab_to_codex_runner(turns.get(task_id))
    };
    if !can_forward_to_codex {
        return Ok(false);
    }
    write_runner_stdin_for_task(store, task_id, lilia_iab_result_payload(snapshot))
}

fn iab_window<R: Runtime>(app: &AppHandle<R>, task_id: &str) -> Result<WebviewWindow<R>, String> {
    app.get_webview_window(&iab_window_label(task_id))
        .ok_or_else(|| "尚未打开 Lilia IAB 窗口".to_string())
}

fn open_iab_window<R: Runtime>(
    app: &AppHandle<R>,
    task_id: String,
    url: Option<String>,
) -> Result<(), String> {
    let target = parse_iab_url(url)?;
    let label = iab_window_label(&task_id);
    if let Some(existing) = app.get_webview_window(&label) {
        existing
            .navigate(target)
            .map_err(|e| format!("IAB 导航失败：{e}"))?;
        focus_window(&existing);
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::External(target))
        .title(iab_title(&task_id))
        .inner_size(IAB_WIDTH, IAB_HEIGHT)
        .min_inner_size(IAB_MIN_WIDTH, IAB_MIN_HEIGHT)
        .center()
        .decorations(true)
        .resizable(true)
        .background_color(BG)
        .build()
        .map_err(|e| format!("创建 Lilia IAB 窗口失败：{e}"))?;
    focus_window(&window);
    Ok(())
}

#[tauri::command]
pub(crate) fn lilia_iab_open(
    app: AppHandle,
    task_id: String,
    url: Option<String>,
) -> Result<(), String> {
    open_iab_window(&app, task_id, url)
}

#[tauri::command]
pub(crate) fn lilia_iab_submit(
    app: AppHandle,
    task_id: String,
    note: Option<String>,
    store: tauri::State<'_, ChatStore>,
) -> Result<LiliaIabSubmitResult, String> {
    let window = iab_window(&app, &task_id)?;
    focus_window(&window);
    let snapshot = build_snapshot(&task_id, note, &window);
    let stdin_forwarded = forward_lilia_iab_to_running_backend(&store, &task_id, &snapshot)?;
    Ok(submit_result(snapshot, stdin_forwarded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iab_window_label_is_stable_and_safe() {
        assert_eq!(iab_window_label("task-1"), "lilia-iab-task-1");
        assert_eq!(iab_window_label("项目/任务 1"), "lilia-iab-------1");
    }

    #[test]
    fn iab_result_payload_wraps_snapshot() {
        let snapshot = LiliaIabSnapshot {
            task_id: "task-1".to_string(),
            url: "https://example.com/".to_string(),
            title: Some("Example".to_string()),
            note: Some("note".to_string()),
            captured_at: 1,
            screenshot_path: Some("C:/shot.png".to_string()),
            screenshot_attachment: None,
            status: "captured".to_string(),
            warning: None,
        };

        let payload = lilia_iab_result_payload(&snapshot);

        assert_eq!(payload["type"], runner_lilia_iab_result_control_type());
        assert_eq!(payload["snapshot"]["url"], "https://example.com/");
        assert_eq!(payload["snapshot"]["note"], "note");
    }

    #[test]
    fn parse_iab_url_defaults_to_blank() {
        assert_eq!(parse_iab_url(None).unwrap().as_str(), "about:blank");
        assert!(parse_iab_url(Some("not a url".to_string())).is_err());
    }

    #[test]
    fn iab_forwarding_requires_codex_running_turn() {
        let codex_turn = RunningTurn {
            turn_id: "turn-1".to_string(),
            backend: BACKEND_CODEX.to_string(),
            native_approval_pause: None,
        };
        let claude_turn = RunningTurn {
            backend: "claude".to_string(),
            ..codex_turn.clone()
        };
        assert!(can_forward_lilia_iab_to_codex_runner(Some(&codex_turn)));
        assert!(!can_forward_lilia_iab_to_codex_runner(Some(&claude_turn)));
        assert!(!can_forward_lilia_iab_to_codex_runner(None));
    }
}

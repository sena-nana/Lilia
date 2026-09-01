use std::sync::Arc;

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
use std::sync::atomic::{AtomicU64, Ordering};

use crate::application::DesktopApplication;
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
use crate::application::{DesktopPopupWindowSettings, TaskQuery};
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
use lilia_contracts::ProductTask;
use lilia_contracts::TaskId;

use crate::desktop::{ChromeMessage, Message};

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
const TRAY_ID: &str = "liliacode";
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
const MENU_OPEN_MAIN: &str = "native-tray:open-main";
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
const MENU_TOGGLE_CONVERSATION_STATUS: &str = "native-tray:toggle-conversation-status";
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
const MENU_RECENT: &str = "native-tray:recent";
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
const MENU_RECENT_EMPTY: &str = "native-tray:recent-empty";
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
const MENU_TASK_PREFIX: &str = "native-tray:task:";
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
const MENU_QUIT: &str = "native-tray:quit";
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
const RECENT_TASK_LIMIT: usize = 8;
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
const SINGLE_CLICK_DELAY: std::time::Duration = std::time::Duration::from_millis(250);
#[cfg(any(windows, target_os = "macos", target_os = "linux", test))]
const RECENT_TITLE_MAX_CHARS: usize = 32;

type MessageSender = Arc<dyn Fn(Message) -> Result<(), String> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCommand {
    FocusMainWindow,
    OpenNewConversation,
    OpenTask(TaskId),
    ToggleConversationStatus,
    Quit,
}

pub struct NativeShellIntegration {
    shortcut: Option<String>,
    startup_errors: Vec<String>,
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    tray: Option<tray_icon::TrayIcon>,
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    hotkey_manager: Option<global_hotkey::GlobalHotKeyManager>,
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    registered_hotkey: Option<global_hotkey::hotkey::HotKey>,
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    registered_hotkey_id: Arc<AtomicU64>,
}

impl NativeShellIntegration {
    pub fn initialize(application: &DesktopApplication, message_sender: MessageSender) -> Self {
        let (shortcut, startup_errors) = match application.popup_window_settings() {
            Ok(settings) => (settings.shortcut, Vec::new()),
            Err(error) => (None, vec![format!("读取弹出窗口设置失败：{error}")]),
        };

        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        {
            let mut startup_errors = startup_errors;
            let tasks = application
                .query_tasks(TaskQuery::default())
                .unwrap_or_else(|error| {
                    startup_errors.push(format!("读取最近任务失败：{error}"));
                    Vec::new()
                });
            let tray = prepare_tray_runtime()
                .and_then(|()| build_tray(tasks))
                .map(Some)
                .unwrap_or_else(|error| {
                    startup_errors.push(error);
                    None
                });
            let hotkey_manager = global_hotkey::GlobalHotKeyManager::new()
                .map(Some)
                .unwrap_or_else(|error| {
                    startup_errors.push(format!("全局快捷键服务不可用：{error}"));
                    None
                });
            let registered_hotkey = shortcut.as_deref().and_then(|shortcut| {
                let parsed = match shortcut.parse::<global_hotkey::hotkey::HotKey>() {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        startup_errors.push(format!("已保存的快捷键无效：{error}"));
                        return None;
                    }
                };
                let manager = hotkey_manager.as_ref()?;
                match manager.register(parsed) {
                    Ok(()) => Some(parsed),
                    Err(error) => {
                        startup_errors.push(format!("注册全局快捷键失败：{error}"));
                        None
                    }
                }
            });
            let registered_hotkey_id = Arc::new(AtomicU64::new(
                registered_hotkey
                    .map(registered_hotkey_identity)
                    .unwrap_or_default(),
            ));
            install_event_handlers(
                Arc::clone(&message_sender),
                Arc::clone(&registered_hotkey_id),
            );
            Self {
                shortcut,
                startup_errors,
                tray,
                hotkey_manager,
                registered_hotkey,
                registered_hotkey_id,
            }
        }

        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            let _ = (application, message_sender);
            Self {
                shortcut,
                startup_errors,
            }
        }
    }

    pub fn shortcut(&self) -> Option<&str> {
        self.shortcut.as_deref()
    }

    pub fn shortcut_active(&self) -> bool {
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        {
            self.registered_hotkey.is_some()
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            false
        }
    }

    pub fn tray_active(&self) -> bool {
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        {
            self.tray.is_some()
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            false
        }
    }

    pub fn startup_error(&self) -> Option<String> {
        (!self.startup_errors.is_empty()).then(|| self.startup_errors.join("；"))
    }

    pub fn next_wakeup(&self) -> Option<std::time::Instant> {
        #[cfg(target_os = "linux")]
        {
            self.tray
                .is_some()
                .then(|| std::time::Instant::now() + std::time::Duration::from_millis(50))
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    pub fn pump_platform_events(&self) {
        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }

    pub fn refresh_tray(&mut self, application: &DesktopApplication) -> Result<(), String> {
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        {
            if self.tray.is_none() {
                return Err("系统托盘当前不可用".to_owned());
            }
            let tasks = application
                .query_tasks(TaskQuery::default())
                .map_err(|error| format!("读取最近任务失败：{error}"))?;
            #[cfg(target_os = "linux")]
            {
                self.tray = Some(build_tray(tasks)?);
            }
            #[cfg(any(windows, target_os = "macos"))]
            {
                let menu = build_tray_menu(tasks)?;
                self.tray
                    .as_ref()
                    .expect("tray availability checked above")
                    .set_menu(Some(Box::new(menu)));
            }
            Ok(())
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            let _ = application;
            Err("当前平台不支持系统托盘".to_owned())
        }
    }

    pub fn set_shortcut(
        &mut self,
        application: &DesktopApplication,
        shortcut: Option<String>,
    ) -> Result<Option<String>, String> {
        let shortcut = normalize_shortcut(shortcut);
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        {
            let parsed = shortcut
                .as_deref()
                .map(str::parse::<global_hotkey::hotkey::HotKey>)
                .transpose()
                .map_err(|error| format!("快捷键格式无效：{error}"))?;
            let previous_shortcut = self.shortcut.clone();
            let previous_hotkey = self.registered_hotkey;
            if parsed == previous_hotkey && shortcut == previous_shortcut {
                application
                    .save_popup_window_settings(DesktopPopupWindowSettings {
                        shortcut: shortcut.clone(),
                    })
                    .map_err(|error| format!("保存弹出窗口设置失败：{error}"))?;
                return Ok(shortcut);
            }
            let manager = match (self.hotkey_manager.as_ref(), parsed) {
                (Some(manager), _) => Some(manager),
                (None, Some(_)) => return Err("全局快捷键服务当前不可用".to_owned()),
                (None, None) => None,
            };

            if let (Some(manager), Some(previous)) = (manager, previous_hotkey) {
                manager
                    .unregister(previous)
                    .map_err(|error| format!("更新快捷键前无法注销旧快捷键：{error}"))?;
            }
            if let (Some(manager), Some(next)) = (manager, parsed) {
                if let Err(error) = manager.register(next) {
                    if let Some(previous) = previous_hotkey {
                        let _ = manager.register(previous);
                    }
                    return Err(format!("注册全局快捷键失败：{error}"));
                }
            }

            if let Err(error) = application.save_popup_window_settings(DesktopPopupWindowSettings {
                shortcut: shortcut.clone(),
            }) {
                if let (Some(manager), Some(next)) = (manager, parsed) {
                    let _ = manager.unregister(next);
                }
                if let (Some(manager), Some(previous)) = (manager, previous_hotkey) {
                    let _ = manager.register(previous);
                }
                return Err(format!("保存弹出窗口设置失败：{error}"));
            }
            self.shortcut = shortcut.clone();
            self.registered_hotkey = parsed;
            self.registered_hotkey_id.store(
                parsed.map(registered_hotkey_identity).unwrap_or_default(),
                Ordering::Release,
            );
            self.startup_errors.clear();
            Ok(shortcut)
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            let _ = (application, shortcut);
            Err("当前平台不支持全局快捷键".to_owned())
        }
    }
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
impl Drop for NativeShellIntegration {
    fn drop(&mut self) {
        if let (Some(manager), Some(hotkey)) =
            (self.hotkey_manager.as_ref(), self.registered_hotkey)
        {
            self.registered_hotkey_id.store(0, Ordering::Release);
            let _ = manager.unregister(hotkey);
        }
    }
}

fn normalize_shortcut(shortcut: Option<String>) -> Option<String> {
    shortcut
        .map(|shortcut| shortcut.trim().to_owned())
        .filter(|shortcut| !shortcut.is_empty())
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn install_event_handlers(message_sender: MessageSender, registered_hotkey_id: Arc<AtomicU64>) {
    let click_sequence = Arc::new(AtomicU64::new(0));
    let menu_sender = Arc::clone(&message_sender);
    let menu_click_sequence = Arc::clone(&click_sequence);
    tray_icon::menu::MenuEvent::set_event_handler(Some(
        move |event: tray_icon::menu::MenuEvent| {
            menu_click_sequence.fetch_add(1, Ordering::SeqCst);
            let id = event.id.as_ref();
            let command = if id == MENU_OPEN_MAIN {
                Some(ShellCommand::FocusMainWindow)
            } else if id == MENU_TOGGLE_CONVERSATION_STATUS {
                Some(ShellCommand::ToggleConversationStatus)
            } else if id == MENU_QUIT {
                Some(ShellCommand::Quit)
            } else if let Some(task_id) = id.strip_prefix(MENU_TASK_PREFIX) {
                TaskId::new(task_id.to_owned())
                    .ok()
                    .map(ShellCommand::OpenTask)
            } else {
                None
            };
            if let Some(command) = command {
                let _ = menu_sender(Message::Chrome(ChromeMessage::Shell(command)));
            }
        },
    ));

    let tray_sender = Arc::clone(&message_sender);
    let tray_click_sequence = Arc::clone(&click_sequence);
    tray_icon::TrayIconEvent::set_event_handler(Some(move |event: tray_icon::TrayIconEvent| {
        match event {
            tray_icon::TrayIconEvent::Click {
                id,
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } if id.as_ref() == TRAY_ID => {
                let sequence = tray_click_sequence.fetch_add(1, Ordering::SeqCst) + 1;
                let sender = Arc::clone(&tray_sender);
                let click_sequence = Arc::clone(&tray_click_sequence);
                let _ = std::thread::Builder::new()
                    .name("lilia-native-tray-click".to_owned())
                    .spawn(move || {
                        std::thread::sleep(SINGLE_CLICK_DELAY);
                        if click_sequence.load(Ordering::SeqCst) == sequence {
                            let _ = sender(Message::Chrome(ChromeMessage::Shell(
                                ShellCommand::OpenNewConversation,
                            )));
                        }
                    });
            }
            tray_icon::TrayIconEvent::DoubleClick {
                id,
                button: tray_icon::MouseButton::Left,
                ..
            } if id.as_ref() == TRAY_ID => {
                tray_click_sequence.fetch_add(1, Ordering::SeqCst);
                let _ = tray_sender(Message::Chrome(ChromeMessage::Shell(
                    ShellCommand::FocusMainWindow,
                )));
            }
            _ => {}
        }
    }));

    global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(
        move |event: global_hotkey::GlobalHotKeyEvent| {
            if hotkey_event_matches(
                registered_hotkey_id.load(Ordering::Acquire),
                event.id,
                event.state == global_hotkey::HotKeyState::Pressed,
            ) {
                let _ = message_sender(Message::Chrome(ChromeMessage::Shell(
                    ShellCommand::OpenNewConversation,
                )));
            }
        },
    ));
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn registered_hotkey_identity(hotkey: global_hotkey::hotkey::HotKey) -> u64 {
    u64::from(hotkey.id()).saturating_add(1)
}

#[cfg(any(windows, target_os = "macos", target_os = "linux", test))]
fn hotkey_event_matches(registered_identity: u64, event_id: u32, pressed: bool) -> bool {
    pressed
        && registered_identity != 0
        && registered_identity == u64::from(event_id).saturating_add(1)
}

#[cfg(target_os = "linux")]
fn prepare_tray_runtime() -> Result<(), String> {
    gtk::init().map_err(|error| format!("系统托盘服务不可用：{error}"))
}

#[cfg(any(windows, target_os = "macos"))]
fn prepare_tray_runtime() -> Result<(), String> {
    Ok(())
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn build_tray(tasks: Vec<ProductTask>) -> Result<tray_icon::TrayIcon, String> {
    let icon = load_tray_icon()?;
    let menu = build_tray_menu(tasks)?;
    tray_icon::TrayIconBuilder::new()
        .with_id(TRAY_ID)
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .with_tooltip("LiliaCode")
        .with_menu_on_left_click(false)
        .with_menu_on_right_click(true)
        .build()
        .map_err(|error| format!("创建系统托盘失败：{error}"))
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn build_tray_menu(mut tasks: Vec<ProductTask>) -> Result<tray_icon::menu::Menu, String> {
    use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};

    tasks.sort_by(|left, right| {
        right
            .pinned
            .cmp(&left.pinned)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.title.cmp(&right.title))
    });
    tasks.truncate(RECENT_TASK_LIMIT);

    let root = Menu::new();
    let open = MenuItem::with_id(MENU_OPEN_MAIN, "打开主窗口", true, None);
    root.append(&open)
        .map_err(|error| format!("创建托盘菜单失败：{error}"))?;
    let conversation_status =
        MenuItem::with_id(MENU_TOGGLE_CONVERSATION_STATUS, "对话悬浮窗", true, None);
    root.append(&conversation_status)
        .map_err(|error| format!("创建托盘菜单失败：{error}"))?;

    let recent = Submenu::with_id(MENU_RECENT, "最近任务", true);
    if tasks.is_empty() {
        let empty = MenuItem::with_id(MENU_RECENT_EMPTY, "暂无任务", false, None);
        recent
            .append(&empty)
            .map_err(|error| format!("创建最近任务菜单失败：{error}"))?;
    } else {
        for task in tasks {
            let item = MenuItem::with_id(
                format!("{MENU_TASK_PREFIX}{}", task.id.as_str()),
                recent_task_label(&task.title),
                true,
                None,
            );
            recent
                .append(&item)
                .map_err(|error| format!("创建最近任务菜单失败：{error}"))?;
        }
    }
    root.append(&recent)
        .map_err(|error| format!("创建托盘菜单失败：{error}"))?;
    root.append(&PredefinedMenuItem::separator())
        .map_err(|error| format!("创建托盘菜单失败：{error}"))?;
    root.append(&MenuItem::with_id(MENU_QUIT, "退出应用", true, None))
        .map_err(|error| format!("创建托盘菜单失败：{error}"))?;
    Ok(root)
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn load_tray_icon() -> Result<tray_icon::Icon, String> {
    let image = image::load_from_memory_with_format(
        include_bytes!("../assets/icons/32x32.png"),
        image::ImageFormat::Png,
    )
    .map_err(|error| format!("读取托盘图标失败：{error}"))?
    .into_rgba8();
    let (width, height) = image.dimensions();
    tray_icon::Icon::from_rgba(image.into_raw(), width, height)
        .map_err(|error| format!("创建托盘图标失败：{error}"))
}

#[cfg(any(windows, target_os = "macos", target_os = "linux", test))]
fn recent_task_label(title: &str) -> String {
    let mut label = title
        .trim()
        .chars()
        .take(RECENT_TITLE_MAX_CHARS)
        .collect::<String>();
    if title.trim().chars().count() > RECENT_TITLE_MAX_CHARS {
        label.push('…');
    }
    if label.is_empty() {
        "未命名任务".to_owned()
    } else {
        label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_task_label_is_unicode_safe_and_bounded() {
        let title = "原生任务".repeat(10);
        let label = recent_task_label(&title);
        assert_eq!(label.chars().count(), RECENT_TITLE_MAX_CHARS + 1);
        assert!(label.ends_with('…'));
        assert_eq!(recent_task_label("   "), "未命名任务");
    }

    #[test]
    fn global_hotkey_dispatch_requires_the_registered_pressed_identity() {
        let registered = u64::from(42_u32) + 1;
        assert!(hotkey_event_matches(registered, 42, true));
        assert!(!hotkey_event_matches(registered, 41, true));
        assert!(!hotkey_event_matches(registered, 42, false));
        assert!(!hotkey_event_matches(0, 42, true));
    }
}

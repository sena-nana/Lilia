use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lilia_contracts::{ProductTask, TaskId};
use lilia_desktop_application::{DesktopApplication, TaskQuery};
use serde::{Deserialize, Serialize};

use crate::preview::Message;

const SETTINGS_FILE: &str = "native-shell.json";
const TRAY_ID: &str = "liliacode-native-preview";
const MENU_OPEN_MAIN: &str = "native-tray:open-main";
const MENU_RECENT: &str = "native-tray:recent";
const MENU_RECENT_EMPTY: &str = "native-tray:recent-empty";
const MENU_TASK_PREFIX: &str = "native-tray:task:";
const MENU_QUIT: &str = "native-tray:quit";
const RECENT_TASK_LIMIT: usize = 8;
const RECENT_TITLE_MAX_CHARS: usize = 32;

type MessageSender = Arc<dyn Fn(Message) -> Result<(), String> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCommand {
    FocusMainWindow,
    OpenTask(TaskId),
    Quit,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct NativeShellSettings {
    shortcut: Option<String>,
}

pub struct NativeShellIntegration {
    home: PathBuf,
    settings: NativeShellSettings,
    startup_errors: Vec<String>,
    #[cfg(windows)]
    tray: Option<tray_icon::TrayIcon>,
    #[cfg(windows)]
    hotkey_manager: Option<global_hotkey::GlobalHotKeyManager>,
    #[cfg(windows)]
    registered_hotkey: Option<global_hotkey::hotkey::HotKey>,
}

impl NativeShellIntegration {
    pub fn initialize(
        home: impl Into<PathBuf>,
        application: &DesktopApplication,
        message_sender: MessageSender,
    ) -> Self {
        let home = home.into();
        let (settings, mut startup_errors) = match load_settings(&home) {
            Ok(settings) => (settings, Vec::new()),
            Err(error) => (NativeShellSettings::default(), vec![error]),
        };

        #[cfg(windows)]
        {
            install_event_handlers(Arc::clone(&message_sender));
            let tasks = application
                .query_tasks(TaskQuery::default())
                .unwrap_or_else(|error| {
                    startup_errors.push(format!("读取最近任务失败：{error}"));
                    Vec::new()
                });
            let tray = build_tray(tasks).map(Some).unwrap_or_else(|error| {
                startup_errors.push(error);
                None
            });
            let hotkey_manager = global_hotkey::GlobalHotKeyManager::new()
                .map(Some)
                .unwrap_or_else(|error| {
                    startup_errors.push(format!("全局快捷键服务不可用：{error}"));
                    None
                });
            let registered_hotkey = settings.shortcut.as_deref().and_then(|shortcut| {
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
            Self {
                home,
                settings,
                startup_errors,
                tray,
                hotkey_manager,
                registered_hotkey,
            }
        }

        #[cfg(not(windows))]
        {
            let _ = (application, message_sender);
            startup_errors.push("桌面托盘与全局快捷键当前仅支持 Windows 11".to_owned());
            Self {
                home,
                settings,
                startup_errors,
            }
        }
    }

    pub fn shortcut(&self) -> Option<&str> {
        self.settings.shortcut.as_deref()
    }

    pub fn shortcut_active(&self) -> bool {
        #[cfg(windows)]
        {
            self.registered_hotkey.is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    pub fn tray_active(&self) -> bool {
        #[cfg(windows)]
        {
            self.tray.is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    pub fn startup_error(&self) -> Option<String> {
        (!self.startup_errors.is_empty()).then(|| self.startup_errors.join("；"))
    }

    pub fn refresh_tray(&mut self, application: &DesktopApplication) -> Result<(), String> {
        #[cfg(windows)]
        {
            let Some(tray) = self.tray.as_ref() else {
                return Err("系统托盘当前不可用".to_owned());
            };
            let tasks = application
                .query_tasks(TaskQuery::default())
                .map_err(|error| format!("读取最近任务失败：{error}"))?;
            let menu = build_tray_menu(tasks)?;
            tray.set_menu(Some(Box::new(menu)));
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = application;
            Err("系统托盘当前仅支持 Windows 11".to_owned())
        }
    }

    pub fn set_shortcut(&mut self, shortcut: Option<String>) -> Result<Option<String>, String> {
        let shortcut = normalize_shortcut(shortcut);
        #[cfg(windows)]
        {
            let parsed = shortcut
                .as_deref()
                .map(str::parse::<global_hotkey::hotkey::HotKey>)
                .transpose()
                .map_err(|error| format!("快捷键格式无效：{error}"))?;
            let previous_settings = self.settings.clone();
            let previous_hotkey = self.registered_hotkey;
            if parsed == previous_hotkey && shortcut == previous_settings.shortcut {
                save_settings(&self.home, &previous_settings)?;
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

            let next_settings = NativeShellSettings {
                shortcut: shortcut.clone(),
            };
            if let Err(error) = save_settings(&self.home, &next_settings) {
                if let (Some(manager), Some(next)) = (manager, parsed) {
                    let _ = manager.unregister(next);
                }
                if let (Some(manager), Some(previous)) = (manager, previous_hotkey) {
                    let _ = manager.register(previous);
                }
                return Err(error);
            }
            self.settings = next_settings;
            self.registered_hotkey = parsed;
            self.startup_errors.clear();
            Ok(shortcut)
        }
        #[cfg(not(windows))]
        {
            let _ = shortcut;
            Err("全局快捷键当前仅支持 Windows 11".to_owned())
        }
    }
}

#[cfg(windows)]
impl Drop for NativeShellIntegration {
    fn drop(&mut self) {
        if let (Some(manager), Some(hotkey)) =
            (self.hotkey_manager.as_ref(), self.registered_hotkey)
        {
            let _ = manager.unregister(hotkey);
        }
    }
}

fn normalize_shortcut(shortcut: Option<String>) -> Option<String> {
    shortcut
        .map(|shortcut| shortcut.trim().to_owned())
        .filter(|shortcut| !shortcut.is_empty())
}

fn settings_path(home: &Path) -> PathBuf {
    home.join(SETTINGS_FILE)
}

fn load_settings(home: &Path) -> Result<NativeShellSettings, String> {
    let path = settings_path(home);
    match fs::read(&path) {
        Ok(content) => {
            serde_json::from_slice(&content).map_err(|error| format!("读取桌面设置失败：{error}"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(NativeShellSettings::default())
        }
        Err(error) => Err(format!("读取桌面设置失败：{error}")),
    }
}

fn save_settings(home: &Path, settings: &NativeShellSettings) -> Result<(), String> {
    fs::create_dir_all(home).map_err(|error| format!("创建设置目录失败：{error}"))?;
    let path = settings_path(home);
    let staging = home.join(format!("{SETTINGS_FILE}.tmp"));
    let backup = home.join(format!("{SETTINGS_FILE}.bak"));
    let content = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("序列化桌面设置失败：{error}"))?;
    fs::write(&staging, content).map_err(|error| format!("暂存桌面设置失败：{error}"))?;
    if backup.exists() {
        fs::remove_file(&backup).map_err(|error| format!("清理设置备份失败：{error}"))?;
    }
    if path.exists() {
        fs::rename(&path, &backup).map_err(|error| format!("备份桌面设置失败：{error}"))?;
    }
    if let Err(error) = fs::rename(&staging, &path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, &path);
        }
        return Err(format!("保存桌面设置失败：{error}"));
    }
    if backup.exists() {
        fs::remove_file(&backup).map_err(|error| format!("清理设置备份失败：{error}"))?;
    }
    Ok(())
}

#[cfg(windows)]
fn install_event_handlers(message_sender: MessageSender) {
    let menu_sender = Arc::clone(&message_sender);
    tray_icon::menu::MenuEvent::set_event_handler(Some(
        move |event: tray_icon::menu::MenuEvent| {
            let id = event.id.as_ref();
            let command = if id == MENU_OPEN_MAIN {
                Some(ShellCommand::FocusMainWindow)
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
                let _ = menu_sender(Message::Shell(command));
            }
        },
    ));

    let tray_sender = Arc::clone(&message_sender);
    tray_icon::TrayIconEvent::set_event_handler(Some(move |event: tray_icon::TrayIconEvent| {
        if let tray_icon::TrayIconEvent::Click {
            id,
            button: tray_icon::MouseButton::Left,
            button_state: tray_icon::MouseButtonState::Up,
            ..
        } = event
        {
            if id.as_ref() == TRAY_ID {
                let _ = tray_sender(Message::Shell(ShellCommand::FocusMainWindow));
            }
        }
    }));

    global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(
        move |event: global_hotkey::GlobalHotKeyEvent| {
            if event.state == global_hotkey::HotKeyState::Pressed {
                let _ = message_sender(Message::Shell(ShellCommand::FocusMainWindow));
            }
        },
    ));
}

#[cfg(windows)]
fn build_tray(tasks: Vec<ProductTask>) -> Result<tray_icon::TrayIcon, String> {
    let icon = load_tray_icon()?;
    let menu = build_tray_menu(tasks)?;
    tray_icon::TrayIconBuilder::new()
        .with_id(TRAY_ID)
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .with_tooltip("LiliaCode Native Preview")
        .with_menu_on_left_click(false)
        .with_menu_on_right_click(true)
        .build()
        .map_err(|error| format!("创建系统托盘失败：{error}"))
}

#[cfg(windows)]
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

#[cfg(windows)]
fn load_tray_icon() -> Result<tray_icon::Icon, String> {
    let image = image::load_from_memory_with_format(
        include_bytes!("../../desktop/src-tauri/icons/32x32.png"),
        image::ImageFormat::Png,
    )
    .map_err(|error| format!("读取托盘图标失败：{error}"))?
    .into_rgba8();
    let (width, height) = image.dimensions();
    tray_icon::Icon::from_rgba(image.into_raw(), width, height)
        .map_err(|error| format!("创建托盘图标失败：{error}"))
}

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
    fn shell_settings_round_trip_and_clear() {
        let directory = std::env::temp_dir().join(format!(
            "lilia-native-shell-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let settings = NativeShellSettings {
            shortcut: Some("Ctrl+Shift+KeyL".to_owned()),
        };
        save_settings(&directory, &settings).unwrap();
        assert_eq!(load_settings(&directory).unwrap(), settings);
        save_settings(&directory, &NativeShellSettings::default()).unwrap();
        assert_eq!(
            load_settings(&directory).unwrap(),
            NativeShellSettings::default()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recent_task_label_is_unicode_safe_and_bounded() {
        let title = "原生任务".repeat(10);
        let label = recent_task_label(&title);
        assert_eq!(label.chars().count(), RECENT_TITLE_MAX_CHARS + 1);
        assert!(label.ends_with('…'));
        assert_eq!(recent_task_label("   "), "未命名任务");
    }
}

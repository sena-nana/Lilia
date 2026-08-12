#[cfg(not(windows))]
fn main() {
    eprintln!("LiliaCode Native Preview currently requires Windows 11");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    std::process::exit(windows_launcher::run());
}

#[cfg(windows)]
mod windows_launcher {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::thread::{self, JoinHandle};

    use libloading::{Library, Symbol};
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{
        CloseHandle, GetLastError, COLORREF, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT,
        RECT, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, SetBkMode,
        SetTextColor, UpdateWindow, DT_CENTER, DT_VCENTER, HGDIOBJ, PAINTSTRUCT, TRANSPARENT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
        GetMessageW, GetSystemMetrics, PostMessageW, PostQuitMessage, RegisterClassW, ShowWindow,
        TranslateMessage, CS_HREDRAW, CS_VREDRAW, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WM_CLOSE,
        WM_DESTROY, WM_PAINT, WNDCLASSW, WS_BORDER, WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
    };

    const PRODUCT_NAME: &str = "LiliaCode Native Preview";
    const HOST_LIBRARY: &str = "lilia_native_host.dll";
    const HOST_ENTRYPOINT: &[u8] = b"lilia_native_run\0";
    const STARTUP_WIDTH: i32 = 480;
    const STARTUP_HEIGHT: i32 = 196;

    type HostEntrypoint = unsafe extern "system" fn(isize) -> i32;

    pub fn run() -> i32 {
        let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
        let guard = match LauncherMutex::acquire() {
            Ok(guard) => guard,
            Err(error) => {
                eprintln!("{error}");
                return 2;
            }
        };
        let show_startup = guard.is_primary() && should_show_startup(&arguments);
        let startup = if show_startup {
            match StartupWindow::create() {
                Ok(window) => Some(window),
                Err(error) => {
                    eprintln!("failed to create Native startup window: {error}");
                    None
                }
            }
        } else {
            None
        };
        let library_path = match std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join(HOST_LIBRARY)))
        {
            Some(path) => path,
            None => {
                eprintln!("cannot locate the Native host library");
                return 2;
            }
        };
        let result = unsafe {
            run_host(
                &library_path,
                startup.as_ref().map_or(0, |window| window.raw()),
            )
        };
        drop(startup);
        drop(guard);
        result.unwrap_or_else(|error| {
            eprintln!("{error}");
            2
        })
    }

    unsafe fn run_host(path: &Path, startup_window: isize) -> Result<i32, String> {
        let library = unsafe { Library::new(path) }
            .map_err(|error| format!("cannot load Native host {}: {error}", path.display()))?;
        let entrypoint: Symbol<'_, HostEntrypoint> = unsafe { library.get(HOST_ENTRYPOINT) }
            .map_err(|error| format!("Native host entrypoint is unavailable: {error}"))?;
        Ok(unsafe { entrypoint(startup_window) })
    }

    fn should_show_startup(arguments: &[OsString]) -> bool {
        !matches!(
            arguments.first().and_then(|argument| argument.to_str()),
            Some("import" | "--complete-pending-data-import")
        )
    }

    struct LauncherMutex {
        handle: HANDLE,
        primary: bool,
    }

    impl LauncherMutex {
        fn acquire() -> Result<Self, String> {
            let name = HSTRING::from(format!(
                "Local\\LiliaCode.NativePreview.Launcher.{:016x}",
                home_hash(&preview_home()?)
            ));
            let handle = unsafe { CreateMutexW(None, true, &name) }
                .map_err(|error| format!("cannot create Native launcher mutex: {error}"))?;
            let primary = unsafe { GetLastError() } != ERROR_ALREADY_EXISTS;
            Ok(Self { handle, primary })
        }

        const fn is_primary(&self) -> bool {
            self.primary
        }
    }

    impl Drop for LauncherMutex {
        fn drop(&mut self) {
            unsafe {
                if self.primary {
                    let _ = ReleaseMutex(self.handle);
                }
                let _ = CloseHandle(self.handle);
            }
        }
    }

    fn preview_home() -> Result<PathBuf, String> {
        if let Some(home) =
            std::env::var_os("LILIA_NATIVE_PREVIEW_HOME").filter(|value| !value.is_empty())
        {
            return Ok(PathBuf::from(home));
        }
        std::env::var_os("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(|base| base.join("LiliaCode Native Preview"))
            .ok_or_else(|| "LILIA_NATIVE_PREVIEW_HOME or LOCALAPPDATA must be set".to_owned())
    }

    fn home_hash(home: &Path) -> u64 {
        let mut normalized = if home.is_absolute() {
            home.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(home)
        }
        .to_string_lossy()
        .replace('\\', "/");
        normalized.make_ascii_lowercase();
        normalized
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
            })
    }

    struct StartupWindow {
        window: isize,
        thread: Option<JoinHandle<()>>,
    }

    impl StartupWindow {
        fn create() -> Result<Self, String> {
            let (sender, receiver) = mpsc::sync_channel(1);
            let thread = thread::Builder::new()
                .name("lilia-native-startup-window".to_owned())
                .spawn(move || unsafe {
                    let result = create_startup_window().map(|window| window.0 as isize);
                    let window = result.as_ref().copied().unwrap_or_default();
                    if sender.send(result).is_err() || window == 0 {
                        return;
                    }
                    let mut message = MSG::default();
                    while GetMessageW(&mut message, None, 0, 0).as_bool() {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                })
                .map_err(|error| format!("cannot start startup-window thread: {error}"))?;
            match receiver.recv() {
                Ok(Ok(window)) => Ok(Self {
                    window,
                    thread: Some(thread),
                }),
                Ok(Err(error)) => {
                    let _ = thread.join();
                    Err(error)
                }
                Err(error) => {
                    let _ = thread.join();
                    Err(format!(
                        "startup-window thread stopped before readiness: {error}"
                    ))
                }
            }
        }

        const fn raw(&self) -> isize {
            self.window
        }
    }

    impl Drop for StartupWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(self.window as *mut core::ffi::c_void)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    unsafe fn create_startup_window() -> Result<HWND, String> {
        unsafe {
            let instance = GetModuleHandleW(None)
                .map_err(|error| format!("cannot read launcher module: {error}"))?;
            let class_name = HSTRING::from("LiliaCode.NativePreview.StartupWindow");
            let title = HSTRING::from(PRODUCT_NAME);
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(startup_window_proc),
                hInstance: instance.into(),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(format!(
                    "cannot register startup window: {}",
                    GetLastError().0
                ));
            }
            let x = (GetSystemMetrics(SM_CXSCREEN) - STARTUP_WIDTH) / 2;
            let y = (GetSystemMetrics(SM_CYSCREEN) - STARTUP_HEIGHT) / 2;
            let window = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                &class_name,
                &title,
                WS_POPUP | WS_BORDER | WS_VISIBLE,
                x,
                y,
                STARTUP_WIDTH,
                STARTUP_HEIGHT,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(|error| format!("cannot create startup window: {error}"))?;
            let _ = ShowWindow(window, SW_SHOW);
            let _ = UpdateWindow(window);
            Ok(window)
        }
    }

    unsafe extern "system" fn startup_window_proc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_PAINT => unsafe {
                let mut paint = PAINTSTRUCT::default();
                let device = BeginPaint(window, &mut paint);
                let mut bounds = RECT::default();
                let _ = GetClientRect(window, &mut bounds);
                let background = CreateSolidBrush(COLORREF(0x0018_1818));
                FillRect(device, &bounds, background);
                let _ = DeleteObject(HGDIOBJ(background.0));
                SetBkMode(device, TRANSPARENT);
                SetTextColor(device, COLORREF(0x00dd_dddd));
                let mut content = "LiliaCode Native Preview\nOpening workspace..."
                    .encode_utf16()
                    .collect::<Vec<_>>();
                DrawTextW(device, &mut content, &mut bounds, DT_CENTER | DT_VCENTER);
                let _ = EndPaint(window, &paint);
                LRESULT(0)
            },
            WM_CLOSE => unsafe {
                let _ = DestroyWindow(window);
                LRESULT(0)
            },
            WM_DESTROY => unsafe {
                PostQuitMessage(0);
                LRESULT(0)
            },
            _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{home_hash, should_show_startup};
        use std::ffi::OsString;
        use std::path::Path;

        #[test]
        fn noninteractive_entrypoints_never_open_the_startup_window() {
            assert!(!should_show_startup(&[OsString::from("import")]));
            assert!(!should_show_startup(&[OsString::from(
                "--complete-pending-data-import",
            )]));
            assert!(should_show_startup(&[]));
            assert!(should_show_startup(&[OsString::from("C:/workspace")]));
        }

        #[test]
        fn launcher_mutex_identity_matches_windows_path_semantics() {
            assert_eq!(
                home_hash(Path::new("C:\\Users\\Alice\\Native")),
                home_hash(Path::new("c:/users/alice/native")),
            );
            assert_ne!(
                home_hash(Path::new("C:/users/alice/native")),
                home_hash(Path::new("C:/users/alice/other")),
            );
        }
    }
}

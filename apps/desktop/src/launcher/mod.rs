use std::ffi::OsString;
use std::path::{Path, PathBuf};

use libloading::{Library, Symbol};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod win;

const ICON_128: &[u8] = include_bytes!("../../assets/icons/128x128.png");
const ICON_256: &[u8] = include_bytes!("../../assets/icons/128x128@2x.png");

#[cfg(windows)]
const HOST_LIBRARY: &str = "liliacode_host.dll";
#[cfg(target_os = "macos")]
const HOST_LIBRARY: &str = "libliliacode_host.dylib";
#[cfg(all(unix, not(target_os = "macos")))]
const HOST_LIBRARY: &str = "libliliacode_host.so";

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
    let mut splash = if guard.is_primary() && should_show_startup(&arguments) {
        match Splash::show() {
            Ok(window) => Some(window),
            Err(error) => {
                eprintln!("failed to create startup window: {error}");
                None
            }
        }
    } else {
        None
    };
    let library_path = match host_library_path() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let handle = splash.as_ref().map_or(0, Splash::handle);
    let result = unsafe { run_host(&library_path, handle) };
    if result.is_ok() {
        if let Some(window) = splash.as_mut() {
            window.disarm();
        }
    }
    result.unwrap_or_else(|error| {
        eprintln!("{error}");
        2
    })
}

fn should_show_startup(arguments: &[OsString]) -> bool {
    !matches!(
        arguments.first().and_then(|argument| argument.to_str()),
        Some("import" | "--complete-pending-data-import")
    )
}

fn host_library_path() -> Result<PathBuf, String> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join(HOST_LIBRARY)))
        .ok_or_else(|| "cannot locate the Native host library".to_owned())
}

unsafe fn run_host(path: &Path, startup_window: isize) -> Result<i32, String> {
    let library = unsafe { Library::new(path) }
        .map_err(|error| format!("cannot load Native host {}: {error}", path.display()))?;
    let entrypoint: Symbol<'_, HostEntrypoint> = unsafe { library.get(b"liliacode_run\0") }
        .map_err(|error| format!("Native host entrypoint is unavailable: {error}"))?;
    Ok(unsafe { entrypoint(startup_window) })
}

struct Splash {
    handle: isize,
    armed: bool,
}

impl Splash {
    fn show() -> Result<Self, String> {
        Ok(Self {
            handle: create_splash()?,
            armed: true,
        })
    }

    const fn handle(&self) -> isize {
        self.handle
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for Splash {
    fn drop(&mut self) {
        if self.armed {
            close_splash(self.handle);
        }
    }
}

fn create_splash() -> Result<isize, String> {
    #[cfg(windows)]
    {
        win::create()
    }
    #[cfg(target_os = "macos")]
    {
        macos::create()
    }
    #[cfg(target_os = "linux")]
    {
        linux::create()
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        Ok(0)
    }
}

fn close_splash(handle: isize) {
    if handle == 0 {
        return;
    }
    #[cfg(windows)]
    win::close(handle);
    #[cfg(target_os = "macos")]
    macos::close(handle);
    #[cfg(target_os = "linux")]
    linux::close(handle);
}

struct LauncherMutex {
    #[cfg(windows)]
    handle: ::windows::Win32::Foundation::HANDLE,
    primary: bool,
}

impl LauncherMutex {
    fn acquire() -> Result<Self, String> {
        #[cfg(windows)]
        {
            use windows::core::HSTRING;
            use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
            use windows::Win32::System::Threading::CreateMutexW;

            let home = lilia_storage::LiliaDataPaths::resolve()
                .home()
                .to_path_buf();
            let name = HSTRING::from(format!(
                "Local\\LiliaCode.Launcher.{:016x}",
                home_hash(&home)
            ));
            let handle = unsafe { CreateMutexW(None, true, &name) }
                .map_err(|error| format!("cannot create Native launcher mutex: {error}"))?;
            let primary = unsafe { GetLastError() } != ERROR_ALREADY_EXISTS;
            Ok(Self { handle, primary })
        }
        #[cfg(not(windows))]
        {
            Ok(Self { primary: true })
        }
    }

    const fn is_primary(&self) -> bool {
        self.primary
    }
}

#[cfg(windows)]
impl Drop for LauncherMutex {
    fn drop(&mut self) {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::ReleaseMutex;

        unsafe {
            if self.primary {
                let _ = ReleaseMutex(self.handle);
            }
            let _ = CloseHandle(self.handle);
        }
    }
}

fn home_hash(home: &Path) -> u64 {
    let mut normalized = home.to_string_lossy().replace('\\', "/");
    normalized.make_ascii_lowercase();
    normalized
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

#[cfg(test)]
mod tests {
    use super::{should_show_startup, Splash};
    use std::ffi::OsString;

    #[test]
    fn noninteractive_entrypoints_never_open_the_startup_window() {
        assert!(!should_show_startup(&[OsString::from("import")]));
        assert!(!should_show_startup(&[OsString::from(
            "--complete-pending-data-import",
        )]));
        assert!(should_show_startup(&[]));
        assert!(should_show_startup(&[OsString::from("C:/workspace")]));
    }

    #[cfg(windows)]
    #[test]
    fn launcher_mutex_identity_matches_windows_path_semantics() {
        use super::home_hash;
        use std::path::Path;

        assert_eq!(
            home_hash(Path::new("C:\\Users\\Alice\\Native")),
            home_hash(Path::new("c:/users/alice/native")),
        );
        assert_ne!(
            home_hash(Path::new("C:/users/alice/native")),
            home_hash(Path::new("C:/users/alice/other")),
        );
    }

    #[cfg(windows)]
    #[test]
    fn startup_icon_window_can_be_created_and_closed() {
        let splash = Splash::show().expect("startup icon window should open");
        assert_ne!(splash.handle(), 0);
        drop(splash);
    }
}

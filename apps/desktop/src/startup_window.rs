use std::sync::atomic::{AtomicIsize, Ordering};

static STARTUP_WINDOW: AtomicIsize = AtomicIsize::new(0);

pub fn register(window: isize) {
    STARTUP_WINDOW.store(window, Ordering::Release);
}

pub fn close() {
    let window = STARTUP_WINDOW.swap(0, Ordering::AcqRel);
    if window == 0 {
        return;
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, IsWindow};

        let handle = HWND(window as *mut core::ffi::c_void);
        if IsWindow(Some(handle)).as_bool() {
            let _ = DestroyWindow(handle);
        }
    }
    #[cfg(target_os = "macos")]
    {
        use objc2::rc::Retained;
        use objc2_app_kit::NSWindow;

        if let Some(handle) = unsafe { Retained::from_raw(window as *mut NSWindow) } {
            if handle.isVisible() {
                handle.close();
            }
        }
    }
    #[cfg(target_os = "linux")]
    unsafe {
        gtk::ffi::gtk_widget_hide(window as *mut gtk::ffi::GtkWidget);
    }
}

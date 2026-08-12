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
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

        let _ = PostMessageW(
            Some(HWND(window as *mut core::ffi::c_void)),
            WM_CLOSE,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

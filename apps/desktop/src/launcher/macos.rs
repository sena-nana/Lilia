use objc2::rc::Retained;
use objc2_app_kit::NSWindow;

pub fn close(handle: isize) {
    if let Some(window) = unsafe { Retained::from_raw(handle as *mut NSWindow) } {
        if window.isVisible() {
            window.close();
        }
    }
}

use objc2::{AnyThread, MainThreadMarker, rc::Retained};
use objc2_app_kit::{NSApplication, NSImage, NSWindow};
use objc2_foundation::NSData;

pub fn set_dock_icon(bytes: &[u8]) {
    let Some(marker) = MainThreadMarker::new() else {
        return;
    };
    let data = unsafe { NSData::dataWithBytes_length(bytes.as_ptr().cast(), bytes.len()) };
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    unsafe { NSApplication::sharedApplication(marker).setApplicationIconImage(Some(&image)) };
}

pub fn close(handle: isize) {
    if let Some(window) = unsafe { Retained::from_raw(handle as *mut NSWindow) } {
        if window.isVisible() {
            window.close();
        }
    }
}

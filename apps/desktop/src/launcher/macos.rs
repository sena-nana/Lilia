use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSColor, NSImage, NSImageView, NSScreen, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{NSData, NSPoint, NSRect, NSSize};

use super::{ICON_128, ICON_256};

const LOGICAL_SIZE: f64 = 128.0;

pub fn create() -> Result<isize, String> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| "startup window requires the main thread".to_owned())?;
    let _app = NSApplication::sharedApplication(mtm);
    let scale = NSScreen::mainScreen(mtm)
        .map(|screen| screen.backingScaleFactor())
        .unwrap_or(1.0);
    let data = NSData::with_bytes(if scale >= 1.5 { ICON_256 } else { ICON_128 });
    let image = NSImage::initWithData(NSImage::alloc(), &data)
        .ok_or_else(|| "cannot decode application icon".to_owned())?;
    image.setSize(NSSize::new(LOGICAL_SIZE, LOGICAL_SIZE));
    let frame = NSScreen::mainScreen(mtm)
        .map(|screen| {
            let screen = screen.frame();
            NSRect::new(
                NSPoint::new(
                    screen.origin.x + (screen.size.width - LOGICAL_SIZE) / 2.0,
                    screen.origin.y + (screen.size.height - LOGICAL_SIZE) / 2.0,
                ),
                NSSize::new(LOGICAL_SIZE, LOGICAL_SIZE),
            )
        })
        .unwrap_or(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(LOGICAL_SIZE, LOGICAL_SIZE),
        ));
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(),
            frame,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe {
        window.setReleasedWhenClosed(false);
    }
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setHasShadow(false);
    window.setContentView(Some(&NSImageView::imageViewWithImage(&image, mtm)));
    window.orderFrontRegardless();
    Ok(Retained::into_raw(window) as isize)
}

pub fn close(handle: isize) {
    if let Some(window) = unsafe { Retained::from_raw(handle as *mut NSWindow) } {
        if window.isVisible() {
            window.close();
        }
    }
}

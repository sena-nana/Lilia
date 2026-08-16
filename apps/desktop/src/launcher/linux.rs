use gtk::gdk_pixbuf::Pixbuf;
use gtk::gio::{Cancellable, MemoryInputStream};
use gtk::glib;
use gtk::prelude::*;

use super::{ICON_128, ICON_256};

pub fn create() -> Result<isize, String> {
    gtk::init().map_err(|error| format!("cannot initialize startup window: {error}"))?;
    let window = gtk::Window::new(gtk::WindowType::Popup);
    let png = if window.scale_factor() >= 2 {
        ICON_256
    } else {
        ICON_128
    };
    let pixbuf = Pixbuf::from_stream(
        &MemoryInputStream::from_bytes(&glib::Bytes::from_static(png)),
        None::<&Cancellable>,
    )
    .map_err(|error| format!("cannot decode application icon: {error}"))?;
    window.set_decorated(false);
    window.set_skip_taskbar_hint(true);
    window.set_position(gtk::WindowPosition::CenterAlways);
    window.add(&gtk::Image::from_pixbuf(Some(&pixbuf)));
    window.show_all();
    while gtk::events_pending() {
        gtk::main_iteration_do(false);
    }
    let handle = window.as_ptr() as isize;
    std::mem::forget(window);
    Ok(handle)
}

pub fn close(handle: isize) {
    unsafe {
        gtk::ffi::gtk_widget_hide(handle as *mut gtk::ffi::GtkWidget);
    }
}

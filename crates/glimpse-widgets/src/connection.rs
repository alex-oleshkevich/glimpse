use std::ffi::c_void;
use std::io;

use gtk4::gdk;
use gtk4::glib::translate::ToGlibPtr;
use gtk4::prelude::*;
use wayland_client::Connection;
use wayland_client::backend::Backend;

unsafe extern "C" {
    fn gdk_wayland_display_get_wl_display(display: *mut gdk::ffi::GdkDisplay) -> *mut c_void;
}

pub(crate) fn of(display: &gdk::Display) -> io::Result<Connection> {
    if display.type_().name() != "GdkWaylandDisplay" {
        return Err(io::Error::other("not a Wayland display"));
    }
    let pointer = unsafe { gdk_wayland_display_get_wl_display(display.to_glib_none().0) };
    if pointer.is_null() {
        return Err(io::Error::other("GTK holds no wl_display"));
    }
    let backend = unsafe { Backend::from_foreign_display(pointer.cast()) };
    Ok(Connection::from_backend(backend))
}

pub fn roundtrip(display: &gdk::Display) -> io::Result<()> {
    of(display)?.roundtrip().map(drop).map_err(io::Error::other)
}

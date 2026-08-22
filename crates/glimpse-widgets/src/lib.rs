use gio::*;

pub fn register_resources() -> Result<(), glib::Error> {
    gio::resources_register_include!("glimpse-panel.gresource")
}

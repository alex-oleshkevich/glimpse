mod panel;
mod theme;

pub use panel::Panel;
pub use theme::Styles;

pub fn register_resources() -> Result<(), glib::Error> {
    gio::resources_register_include!("glimpse-panel.gresource")
}

use gtk::{gio, glib};

mod app;
mod commands;
mod contrib;
mod extensions;
mod messages;
mod widgets;
mod windows;
mod search;
mod worker;

fn main() -> glib::ExitCode {
    adw::init().expect("Failed to initialize Adwaita");
    gio::resources_register_include!("resources.gresource").expect("Failed to register resources.");

    let app = app::App::new();
    app.run();

    glib::ExitCode::SUCCESS
}

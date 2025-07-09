use adw::prelude::*;
use gtk::{gio, glib};

mod widgets;
mod windows;

fn main() -> glib::ExitCode {
    adw::init().expect("Failed to initialize Adwaita");
    gio::resources_register_include!("resources.gresource")
        .expect("Failed to register resources.");

    let app = adw::Application::builder()
        .application_id("me.aresa.glimpse")
        .build();

    app.connect_activate(|app| {
        let main_window = windows::main::MainWindow::new(&app);
        main_window.show();
    });
    app.run();

    glib::ExitCode::SUCCESS
}

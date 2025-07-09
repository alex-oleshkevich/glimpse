use adw::{ffi::AdwStyleManager, prelude::*};
use gtk::{CssProvider, gdk::Display, gio, glib};

mod widgets;
mod windows;

fn main() -> glib::ExitCode {
    adw::init().expect("Failed to initialize Adwaita");
    gio::resources_register_include!("resources.gresource").expect("Failed to register resources.");

    let app = adw::Application::builder()
        .application_id("me.aresa.glimpse")
        .build();

    app.connect_startup(|_| load_css());
    app.connect_activate(|app| {
        let main_window = windows::main::MainWindow::new(&app);

        let debug_close_on_close = std::env::var("GLIMPSE_DEBUG_CLOSE_ON_CLOSE").is_ok();
        if debug_close_on_close {
            main_window.set_hide_on_close(false);
        }


        let action_close = gio::ActionEntry::builder("close")
            .activate(|window: &windows::main::MainWindow, _, _| {
                window.close();
            })
            .build();
        main_window.add_action_entries([action_close]);

        main_window.show();
    });
    app.set_accels_for_action("win.close", &["<Ctrl>W"]);
    app.run();

    glib::ExitCode::SUCCESS
}

fn load_css() {
    println! ("Loading CSS styles...");
    let provider = CssProvider::new();
    provider.load_from_resource("/me/aresa/glimpse/styles.css");
    gtk::style_context_add_provider_for_display(
        &Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

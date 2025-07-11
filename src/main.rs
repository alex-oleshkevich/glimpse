use adw::prelude::*;
use gtk::{CssProvider, gdk::Display, gio, glib};
use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
};

mod commands;
mod contrib;
mod executor;
mod extensions;
mod messages;
mod widgets;
mod windows;

fn main() -> glib::ExitCode {
    adw::init().expect("Failed to initialize Adwaita");
    gio::resources_register_include!("resources.gresource").expect("Failed to register resources.");

    let mut extensions = extensions::ExtensionManager::new();
    extensions.load_extensions();

    let (search_tx, search_rx) = mpsc::channel::<messages::Message>();
    let (ui_tx, ui_rx) = async_channel::unbounded::<messages::UIMessage>();

    let extension_for_thread = Arc::new(extensions);
    let search_handle = thread::spawn(move || {
        let extensions = extension_for_thread.all();
        let executor = executor::Executor::new(extensions);
        for message in search_rx {
            let mut cleared = false;
            for command in executor.query(&message) {
                if !cleared {
                    ui_tx
                        .send_blocking(messages::UIMessage::ClearResults)
                        .expect("Failed to clear results in UI thread");
                    cleared = true;
                }

                ui_tx
                    .send_blocking(messages::UIMessage::AddCommand(command.clone()))
                    .expect("Failed to send command to UI thread");
            }
        }
    });

    let app = adw::Application::builder()
        .application_id("me.aresa.glimpse")
        .build();

    app.connect_startup(|_| load_css());

    let search_tx_arc = Arc::new(Mutex::new(search_tx));
    app.connect_activate(move |app| {
        let main_window = windows::main::MainWindow::new(&app);

        if let Ok(_) = std::env::var("GLIMPSE_DEBUG_CLOSE_ON_CLOSE") {
            main_window.set_hide_on_close(false);
        }

        let tx_cloned = Arc::clone(&search_tx_arc);
        main_window.connect_closure(
            "glimpse-query",
            false,
            glib::closure_local!(move |_: &windows::main::MainWindow, query: String| {
                tx_cloned
                    .lock()
                    .expect("Failed to lock tx")
                    .send(messages::Message::Query(query.clone()))
                    .expect("Failed to send query message");
            }),
        );

        main_window.subscribe(ui_rx.clone());

        main_window.show();
    });
    app.run();

    search_handle.join().expect("Search thread panicked");
    glib::ExitCode::SUCCESS
}

fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_resource("/me/aresa/glimpse/styles.css");
    gtk::style_context_add_provider_for_display(
        &Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

use adw::prelude::*;
use glib::subclass::types::ObjectSubclassIsExt;
use gtk::{CssProvider, gdk::Display, glib};
use std::{
    rc::Rc,
    sync::{Arc, mpsc},
    thread,
};

use crate::{
    extensions, messages, search::Search, widgets::search_row_object::SearchRowObject, windows,
    worker::Worker,
};

pub struct App {}

impl App {
    pub fn new() -> Self {
        Self {}
    }

    pub fn run(&self) {
        let mut extensions = extensions::ExtensionManager::new();
        extensions.load_extensions();

        let (search_sender, search_receiver) = mpsc::channel::<messages::Message>();
        let (ui_sender, ui_receiver) = async_channel::unbounded::<messages::UIMessage>();
        let extensions_arc = Arc::new(extensions);
        let worker_handle = thread::spawn(move || {
            let search = Search::new(Arc::clone(&extensions_arc));
            let worker = Worker::new(search);
            worker.run(search_receiver, ui_sender);
        });
        let search_sender_clone = search_sender.clone();

        let app = adw::Application::builder()
            .application_id("me.aresa.glimpse")
            .build();

        app.connect_startup(|_| load_css());
        app.connect_activate(move |app| {
            let main_window = windows::main::MainWindow::new(&app);

            if let Ok(_) = std::env::var("GLIMPSE_DEBUG_CLOSE_ON_CLOSE") {
                main_window.set_hide_on_close(false);
            }

            Self::setup_command_activate(main_window.clone(), search_sender.clone());
            Self::setup_search_handler(main_window.clone(), search_sender.clone());
            Self::setup_ui_message_handler(main_window.clone(), ui_receiver.clone());

            search_sender.send(messages::Message::Query("".to_string())).expect("Failed to send initial query");
            main_window.show();
        });

        app.connect_shutdown(move |_| {
            search_sender_clone
                .send(messages::Message::Shutdown)
                .expect("Failed to send shutdown message");
        });
        app.run();

        worker_handle.join().expect("Search thread panicked");
    }

    fn setup_ui_message_handler(
        window: windows::main::MainWindow,
        ui_receiver: async_channel::Receiver<messages::UIMessage>,
    ) {
        glib::MainContext::default().spawn_local(async move {
            while let Ok(msg) = ui_receiver.recv().await {
                window.dispatch(msg);
            }
        });
    }

    fn setup_search_handler(
        window: windows::main::MainWindow,
        search_sender: mpsc::Sender<messages::Message>,
    ) {
        window.search_entry().connect_changed(move |entry| {
            let query = entry.text().to_string();
            search_sender
                .send(messages::Message::Query(query))
                .expect("Failed to send query message");
        });
    }

    fn setup_command_activate(
        window: windows::main::MainWindow,
        search_sender: mpsc::Sender<messages::Message>,
    ) {
        let window_rc = Rc::new(window);
        let window_activate = Rc::clone(&window_rc);
        let search_sender_activate = search_sender.clone();
        window_rc.search_entry().connect_activate(move |_| {
            if window_activate.selection_model().n_items() == 0 {
                println!("No items to activate");
                return;
            }

            window_activate.selection_model().select_item(0, true);
            dispatch_action(window_activate.clone(), search_sender_activate.clone());
        });

        let search_sender_copy = search_sender.clone();
        window_rc.result_view().connect_activate(move |_, _| {
            dispatch_action(Rc::clone(&window_rc), search_sender_copy.clone());
        });
    }
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

fn dispatch_action(
    window: Rc<windows::main::MainWindow>,
    search_sender: mpsc::Sender<messages::Message>,
) {
    let item = window.selection_model().selected_item();
    if item.is_none() {
        println!("No item selected");
        return;
    }
    let item = item.unwrap();
    let command_id = item
        .downcast_ref::<SearchRowObject>()
        .expect("Item should be SearchRowObject")
        .id()
        .to_string();
    let commands = window.imp().command_map.borrow_mut();
    let command = commands
        .get(&command_id)
        .expect("Command should exist in command map")
        .clone();

    let primary_action = command.primary_action();
    if primary_action.is_none() {
        println!("No primary action for command: {}", command_id);
        return;
    }
    search_sender
        .send(messages::Message::ExecAction(
            primary_action.unwrap().clone(),
        ))
        .expect("Failed to send activate command message");
    window.close();
}

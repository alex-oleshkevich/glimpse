mod imp;

use crate::{
    messages,
    widgets::{search_row::SearchRow, search_row_object::SearchRowObject},
};
use glib::{Object, subclass::prelude::*};
use gtk::{gio, glib, prelude::*};

glib::wrapper! {
    pub struct MainWindow(ObjectSubclass<imp::MainWindow>)
        @extends adw::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl MainWindow {
    pub fn new(app: &adw::Application) -> Self {
        Object::builder().property("application", app).build()
    }

    pub fn search_entry(&self) -> gtk::Entry {
        self.imp().search_entry.clone()
    }

    fn results(&self) -> gio::ListStore {
        self.imp()
            .results
            .borrow()
            .clone()
            .expect("Results should be initialized")
    }

    fn selection_model(&self) -> gtk::SingleSelection {
        self.imp()
            .result_view
            .model()
            .unwrap()
            .downcast::<gtk::SingleSelection>()
            .clone()
            .expect("Result view should have a SingleSelection model")
    }

    fn setup(&self) {
        self.setup_results();
        self.setup_factory();
        // self.setup_focus_detection();
        self.setup_window_keyhandler();
        self.setup_result_keyhandler();
        self.setup_query();
    }

    fn setup_results(&self) {
        let model = gio::ListStore::new::<SearchRowObject>();
        self.imp().results.replace(Some(model));

        let selection_model = gtk::SingleSelection::new(Some(self.results()));
        self.imp().result_view.set_model(Some(&selection_model));
    }

    fn setup_factory(&self) {
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(move |_, list_item| {
            let search_row = SearchRow::new();
            list_item
                .downcast_ref::<gtk::ListItem>()
                .expect("Needs to be ListItem")
                .set_child(Some(&search_row));
        });

        factory.connect_bind(move |_, list_item| {
            let search_row_object = list_item
                .downcast_ref::<gtk::ListItem>()
                .expect("Needs to be ListItem")
                .item()
                .and_downcast::<SearchRowObject>()
                .expect("The item has to be an `SearchRowObject`.");

            // Get `SearchRow` from `ListItem`
            let search_row = list_item
                .downcast_ref::<gtk::ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<SearchRow>()
                .expect("The child has to be a `SearchRow`.");

            search_row.bind(&search_row_object);
        });

        factory.connect_unbind(move |_, list_item| {
            let search_row = list_item
                .downcast_ref::<gtk::ListItem>()
                .expect("Needs to be ListItem")
                .child()
                .and_downcast::<SearchRow>()
                .expect("The child has to be a `SearchRow`.");

            search_row.unbind();
        });

        self.imp().result_view.set_factory(Some(&factory));
    }

    fn setup_query(&self) {
        let search_entry = self.search_entry();
        let window = self.clone();
        search_entry.connect_changed(glib::clone!(
            #[weak]
            window,
            move |entry| {
                window.emit_by_name::<()>("glimpse-query", &[&entry.text().to_string()]);
            }
        ));
    }

    // fn setup_focus_detection(&self) {
    //     let interacted = Rc::new(Cell::new(false));
    //     self.search_entry().connect_has_focus_notify(glib::clone!(
    //         #[weak]
    //         interacted,
    //         move |entry| {
    //             if !interacted.get() {
    //                 interacted.set(true);
    //                 println!("Search entry gained focus: {}", entry.text());
    //             }
    //         }
    //     ));

    //     let window = self.clone();
    //     let controller = gtk::EventControllerFocus::new();
    //     controller.connect_leave(glib::clone!(move |_| {
    //         if interacted.get() {
    //             window.close();
    //             println!("Search entry lost focus");
    //         }
    //     }));
    //     self.add_controller(controller);
    // }

    fn setup_window_keyhandler(&self) {
        let main_window = self;

        let controller = gtk::EventControllerKey::new();
        controller.connect_key_pressed(glib::clone!(
            #[weak]
            main_window,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| -> glib::Propagation {
                match key {
                    gtk::gdk::Key::Escape => {
                        if main_window.imp().search_entry.text().is_empty() {
                            main_window.close();
                        } else {
                            main_window.imp().search_entry.set_text("");
                        }
                        return glib::Propagation::Stop;
                    }
                    gtk::gdk::Key::Down | gtk::gdk::Key::Up => {
                        let view = &main_window.imp().result_view;
                        if !view.has_focus() {
                            let model = main_window.selection_model();
                            model.set_selected(0);
                            view.grab_focus();
                        }

                        glib::Propagation::Proceed
                    }
                    gtk::gdk::Key::BackSpace => {
                        let search_entry = &main_window.imp().search_entry;
                        let selection_model = main_window.selection_model();
                        let current_text = search_entry.text();
                        let cursor_pos = search_entry.position() as usize;

                        if cursor_pos > 0 && !current_text.is_empty() {
                            let mut new_text = current_text.to_string();

                            let char_indices: Vec<_> = new_text.char_indices().collect();
                            if let Some((byte_pos, _)) = char_indices.get(cursor_pos - 1) {
                                new_text.remove(*byte_pos);
                                selection_model.unselect_all();
                                search_entry.grab_focus();
                                search_entry.set_text(&new_text);
                                search_entry.set_position(cursor_pos as i32 - 1);
                                search_entry.emit_by_name::<()>("changed", &[]);
                            }
                        }
                        gtk::glib::Propagation::Stop
                    }
                    _ => {
                        let selection_model = main_window.selection_model();
                        selection_model.unselect_all();

                        if let Some(ch) = key.to_unicode() {
                            if ch.is_alphabetic() || ch.is_numeric() || ch.is_whitespace() {
                                let search_entry = &main_window.imp().search_entry;
                                let current_text = search_entry.text();
                                let cursor_pos = search_entry.position() as usize;
                                let mut new_text = current_text.to_string();
                                let byte_pos = new_text
                                    .char_indices()
                                    .nth(cursor_pos)
                                    .map(|(i, _)| i)
                                    .unwrap_or(new_text.len());

                                new_text.insert(byte_pos, ch);
                                selection_model.unselect_all();
                                search_entry.grab_focus();
                                search_entry.set_text(&new_text);
                                search_entry.set_position(cursor_pos as i32 + 1);
                                search_entry.emit_by_name::<()>("changed", &[]);
                            }
                        }
                        glib::Propagation::Proceed
                    }
                }
            }
        ));
        self.add_controller(controller);
    }

    fn setup_result_keyhandler(&self) {
        let controller = gtk::EventControllerKey::new();
        let selection_model = self.selection_model();
        let window = self.clone();
        self.imp().result_view.connect_activate(glib::clone!(
            #[weak]
            selection_model,
            #[weak]
            window,
            move |_, _| {
                let item = selection_model.selected_item();
                if item.is_none() {
                    return;
                }
                let item = item.unwrap();
                let command_id = item
                    .downcast_ref::<SearchRowObject>()
                    .expect("Item should be SearchRowObject")
                    .id()
                    .to_string();
                let command = window.imp().command_map.borrow_mut();
                println!("Item activated {:?}", command.get(&command_id));
            }
        ));
        self.imp().result_view.add_controller(controller);
    }

    pub fn dispatch(&self, message: messages::UIMessage) {
        match message {
            messages::UIMessage::AddCommand(command) => {
                let command_clone = command.clone();
                let row = SearchRowObject::new(
                    command.id(),
                    command.title,
                    command.subtitle,
                    command.icon,
                );
                self.imp()
                    .command_map
                    .borrow_mut()
                    .insert(command_clone.id(), command_clone);
                self.results().append(&row);
            }
            messages::UIMessage::ClearResults => {
                self.results().remove_all();
                self.imp().command_map.borrow_mut().clear();
            }
            _ => {
                eprintln!("Unhandled UIMessage: {:?}", message);
            }
        }
    }

    pub fn subscribe(&self, receiver: async_channel::Receiver<messages::UIMessage>) {
        let clone = self.clone();
        glib::MainContext::default().spawn_local(async move {
            while let Ok(msg) = receiver.recv().await {
                clone.dispatch(msg);
            }
        });
    }
}

mod imp;

use crate::widgets::{search_row::SearchRow, search_row_object::SearchRowObject};
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

    fn results(&self) -> gio::ListStore {
        self.imp()
            .results
            .borrow()
            .clone()
            .expect("Results should be initialized")
    }

    fn setup_results(&self) {
        let model = gio::ListStore::new::<SearchRowObject>();
        model.append(&SearchRowObject::new(
            "Example Item 1".to_string(),
            "Subtitle 1".to_string(),
            "application-x-executable".to_string(),
        ));
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
}

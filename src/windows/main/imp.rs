use adw::subclass::prelude::*;
use glib::subclass::{InitializingObject, Signal};
use gtk::glib;
use std::{cell::RefCell, collections::HashMap, hash::Hash, sync::OnceLock};

use gtk::prelude::*;

use crate::{commands, messages};

#[derive(gtk::CompositeTemplate, Default)]
#[template(resource = "/me/aresa/glimpse/ui/main_window.ui")]
pub struct MainWindow {
    #[template_child]
    pub search_entry: TemplateChild<gtk::Entry>,

    #[template_child]
    pub result_view: TemplateChild<gtk::ListView>,

    #[template_child]
    pub main_box: TemplateChild<gtk::Box>,

    pub results: RefCell<Option<gio::ListStore>>,
    pub command_map: RefCell<HashMap<String, commands::Command>>,
}

#[glib::object_subclass]
impl ObjectSubclass for MainWindow {
    const NAME: &'static str = "MainWindow";
    type Type = super::MainWindow;
    type ParentType = adw::ApplicationWindow;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for MainWindow {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj();
        obj.setup();
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("glimpse-query")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }
}

// Trait shared by all widgets
impl WidgetImpl for MainWindow {}

// Trait shared by all windows
impl WindowImpl for MainWindow {}

// Trait shared by all application windows
impl ApplicationWindowImpl for MainWindow {}

impl AdwApplicationWindowImpl for MainWindow {}

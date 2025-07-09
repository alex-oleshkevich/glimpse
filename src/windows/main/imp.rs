use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::InitializingObject;
use gtk::glib;
use std::cell::RefCell;

#[derive(gtk::CompositeTemplate, Default)]
#[template(resource = "/me/aresa/glimpse/ui/main_window.ui")]
pub struct MainWindow {
    #[template_child]
    pub search_entry: TemplateChild<gtk::Entry>,

    #[template_child]
    pub result_view: TemplateChild<gtk::ListView>,

    pub results: RefCell<Option<gio::ListStore>>,
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
        obj.setup_results();
        obj.setup_factory();
    }
}

// Trait shared by all widgets
impl WidgetImpl for MainWindow {}

// Trait shared by all windows
impl WindowImpl for MainWindow {}

// Trait shared by all application windows
impl ApplicationWindowImpl for MainWindow {}

impl AdwApplicationWindowImpl for MainWindow {}

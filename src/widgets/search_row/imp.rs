use adw::subclass::prelude::*;
use glib::Binding;
use gtk::glib;
use std::cell::RefCell;

#[derive(Default, gtk::CompositeTemplate)]
#[template(resource = "/me/aresa/glimpse/ui/search_row.ui")]
pub struct SearchRow {
    #[template_child]
    pub title: TemplateChild<gtk::Label>,
    #[template_child]
    pub subtitle: TemplateChild<gtk::Label>,
    #[template_child]
    pub icon: TemplateChild<gtk::Image>,
    pub bindings: RefCell<Vec<Binding>>,
}

#[glib::object_subclass]
impl ObjectSubclass for SearchRow {
    const NAME: &'static str = "GlimpseSearchRow";
    type Type = super::SearchRow;
    type ParentType = gtk::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for SearchRow {}

impl WidgetImpl for SearchRow {}

impl BoxImpl for SearchRow {}

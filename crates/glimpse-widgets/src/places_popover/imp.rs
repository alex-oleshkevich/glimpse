use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Expandable, Hero, PopoverShell, Row, Section};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub busy: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Trash {
    pub items: u32,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/places_popover.ui")]
pub struct PlacesPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub places: TemplateChild<Section>,
    #[template_child]
    pub places_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub bookmarks: TemplateChild<Section>,
    #[template_child]
    pub bookmarks_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub bookmarks_more: TemplateChild<Row>,
    #[template_child]
    pub network: TemplateChild<Section>,
    #[template_child]
    pub network_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub trash: TemplateChild<Section>,
    #[template_child]
    pub trash_item: TemplateChild<Expandable>,
    #[template_child]
    pub trash_row: TemplateChild<Row>,
    #[template_child]
    pub trash_open: TemplateChild<Row>,
    #[template_child]
    pub trash_empty: TemplateChild<Row>,
    #[template_child]
    pub footer: TemplateChild<Row>,

    pub places_data: RefCell<Vec<Entry>>,
    pub places_held: RefCell<Vec<(String, Row)>>,
    pub bookmarks_data: RefCell<Vec<Entry>>,
    pub bookmarks_held: RefCell<Vec<(String, Row)>>,
    pub network_data: RefCell<Vec<Entry>>,
    pub network_held: RefCell<Vec<(String, Row)>>,
    pub trash_data: RefCell<Option<Trash>>,
    pub trash_armed: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for PlacesPopover {
    const NAME: &'static str = "PlacesPopover";
    type Type = super::PlacesPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        Expandable::static_type();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for PlacesPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("more").build(),
                glib::subclass::Signal::builder("empty-trash").build(),
                glib::subclass::Signal::builder("footer-activated").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.trash_open.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("activated", &[&"trash".to_owned()])
        ));
        self.trash_empty.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.press_empty_trash()
        ));
        self.trash_item.connect_notify_local(
            Some("expanded"),
            glib::clone!(
                #[weak]
                popover,
                move |_, _| popover.disarm_empty_trash()
            ),
        );
        self.bookmarks_more.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("more", &[])
        ));
        self.footer.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("footer-activated", &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for PlacesPopover {}

use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Expandable, Hero, PopoverShell, Row, Section, SplitRow};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Action {
    pub key: String,
    pub label: String,
    pub destructive: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Device {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub value: String,
    pub actions: Vec<Action>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Nearby {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub busy: bool,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/kdeconnect_popover.ui")]
pub struct KdeconnectPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub devices: TemplateChild<Section>,
    #[template_child]
    pub devices_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub nearby: TemplateChild<Section>,
    #[template_child]
    pub nearby_toggle: TemplateChild<Row>,
    #[template_child]
    pub nearby_drawer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub nearby_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub nearby_more: TemplateChild<Row>,
    #[template_child]
    pub footer: TemplateChild<Row>,

    pub devices_held: RefCell<Vec<(String, Expandable)>>,
    pub devices_data: RefCell<Vec<Device>>,
    pub nearby_held: RefCell<Vec<(String, SplitRow)>>,
    pub nearby_open: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for KdeconnectPopover {
    const NAME: &'static str = "KdeconnectPopover";
    type Type = super::KdeconnectPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for KdeconnectPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("action")
                    .param_types([String::static_type(), String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("pair")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("footer-activated").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.nearby_toggle.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.set_nearby_open(!popover.imp().nearby_open.get())
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

impl WidgetImpl for KdeconnectPopover {}

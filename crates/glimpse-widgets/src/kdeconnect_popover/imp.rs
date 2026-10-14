use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{ActionRow, ExpanderRow, PopoverShell, Section};

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
    pub devices: TemplateChild<Section>,
    #[template_child]
    pub devices_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub nearby: TemplateChild<Section>,
    #[template_child]
    pub nearby_toggle: TemplateChild<ExpanderRow>,
    #[template_child]
    pub nearby_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub nearby_more: TemplateChild<ActionRow>,
    #[template_child]
    pub settings: TemplateChild<ActionRow>,

    pub devices_held: RefCell<Vec<(String, ExpanderRow)>>,
    pub devices_data: RefCell<Vec<Device>>,
    pub nearby_held: RefCell<Vec<(String, ActionRow)>>,
}

#[glib::object_subclass]
impl ObjectSubclass for KdeconnectPopover {
    const NAME: &'static str = "KdeconnectPopover";
    type Type = super::KdeconnectPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        ActionRow::static_type();
        ExpanderRow::static_type();
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
            ]
        })
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for KdeconnectPopover {}

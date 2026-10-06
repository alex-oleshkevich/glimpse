use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{ActionRow, ExpanderRow, Fact, PopoverShell, Section};

use super::RemovableDevice;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Volume {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub fraction: Option<f64>,
    pub space: String,
    pub busy: bool,
    pub mounted: bool,
    pub problem: String,
    pub facts: Vec<Fact>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Drive {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub ejectable: bool,
    pub busy: bool,
    pub dimmed: bool,
    pub facts: Vec<Fact>,
    pub volumes: Vec<Volume>,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/removable_popover.ui")]
pub struct RemovablePopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub devices: TemplateChild<Section>,
    #[template_child]
    pub devices_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub devices_more: TemplateChild<ExpanderRow>,
    #[template_child]
    pub more_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub footer: TemplateChild<ActionRow>,

    pub devices_data: RefCell<(Vec<Drive>, Vec<Drive>)>,
    pub more_held: RefCell<Vec<(String, RemovableDevice)>>,
    pub quiet: Cell<bool>,
    pub devices_held: RefCell<Vec<(String, RemovableDevice)>>,
}

#[glib::object_subclass]
impl ObjectSubclass for RemovablePopover {
    const NAME: &'static str = "RemovablePopover";
    type Type = super::RemovablePopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        ActionRow::static_type();
        ExpanderRow::static_type();
        RemovableDevice::static_type();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for RemovablePopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("more")
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("eject")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("unmount")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("footer-activated").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.devices_more.connect_toggled(glib::clone!(
            #[weak]
            popover,
            move |_, open| {
                if !popover.imp().quiet.get() {
                    popover.emit_by_name::<()>("more", &[&open]);
                }
            }
        ));
        self.footer.connect_activated(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("footer-activated", &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for RemovablePopover {}

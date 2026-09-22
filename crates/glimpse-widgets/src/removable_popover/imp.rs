use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Hero, PopoverShell, Row, Section};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Volume {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub value: String,
    pub fraction: Option<f64>,
    pub activatable: bool,
    pub busy: bool,
    pub read_only: bool,
    pub mounted: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Drive {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub value: String,
    pub ejectable: bool,
    pub busy: bool,
    pub activatable: bool,
    pub dimmed: bool,
    pub volumes: Vec<Volume>,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/removable_popover.ui")]
pub struct RemovablePopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub devices: TemplateChild<Section>,
    #[template_child]
    pub devices_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub devices_more: TemplateChild<Row>,
    #[template_child]
    pub footer: TemplateChild<Row>,

    pub devices_data: RefCell<Vec<Drive>>,
    pub devices_held: RefCell<Vec<(String, gtk4::Box)>>,
}

#[glib::object_subclass]
impl ObjectSubclass for RemovablePopover {
    const NAME: &'static str = "RemovablePopover";
    type Type = super::RemovablePopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
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
                glib::subclass::Signal::builder("more").build(),
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

        self.devices_more.connect_clicked(glib::clone!(
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

impl WidgetImpl for RemovablePopover {}

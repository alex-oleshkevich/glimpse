use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Hero, PopoverShell, Row, Section};

#[derive(Debug, Clone, PartialEq)]
pub struct UsageTile {
    pub id: String,
    pub title: String,
    pub value: String,
    pub fraction: Option<f64>,
    pub severity: Option<crate::Severity>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetailTile {
    pub id: String,
    pub title: String,
    pub value: String,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/system_monitor_popover.ui")]
pub struct SystemMonitorPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub usage: TemplateChild<Section>,
    #[template_child]
    pub usage_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub details: TemplateChild<Section>,
    #[template_child]
    pub details_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub footer: TemplateChild<Row>,

    pub usage_data: RefCell<Vec<UsageTile>>,
    pub usage_held: RefCell<Vec<(String, gtk4::Box)>>,
    pub details_data: RefCell<Vec<DetailTile>>,
    pub details_held: RefCell<Vec<(String, Row)>>,
}

#[glib::object_subclass]
impl ObjectSubclass for SystemMonitorPopover {
    const NAME: &'static str = "SystemMonitorPopover";
    type Type = super::SystemMonitorPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for SystemMonitorPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("footer-activated").build()])
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

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

impl WidgetImpl for SystemMonitorPopover {}

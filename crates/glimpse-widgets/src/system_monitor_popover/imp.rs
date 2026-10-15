use std::cell::RefCell;

use gtk4::{AccessibleRole, CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

use crate::{MeterRow, PopoverShell, Row, Section};

#[derive(Debug, Clone, PartialEq)]
pub struct Meter {
    pub level: f64,
    pub value: String,
    pub severity: Option<crate::Severity>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamedMeter {
    pub title: String,
    pub meter: Meter,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Readings {
    pub cpu: Option<Meter>,
    pub memory: Option<Meter>,
    pub swap: Option<Meter>,
    pub disks: Vec<NamedMeter>,
    pub gpu: Option<Meter>,
    pub gpu_memory: Option<NamedMeter>,
    pub load: Option<String>,
    pub uptime: Option<String>,
    pub network: Option<String>,
    pub cpu_temp: Option<String>,
    pub gpu_temp: Option<String>,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/system_monitor_popover.ui")]
pub struct SystemMonitorPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub usage: TemplateChild<Section>,
    #[template_child]
    pub cpu: TemplateChild<MeterRow>,
    #[template_child]
    pub memory: TemplateChild<MeterRow>,
    #[template_child]
    pub swap: TemplateChild<MeterRow>,
    #[template_child]
    pub disks: TemplateChild<gtk4::Box>,
    #[template_child]
    pub gpu: TemplateChild<MeterRow>,
    #[template_child]
    pub gpu_memory: TemplateChild<MeterRow>,
    #[template_child]
    pub details: TemplateChild<Section>,
    #[template_child]
    pub load: TemplateChild<Row>,
    #[template_child]
    pub uptime: TemplateChild<Row>,
    #[template_child]
    pub network: TemplateChild<Row>,
    #[template_child]
    pub cpu_temp: TemplateChild<Row>,
    #[template_child]
    pub gpu_temp: TemplateChild<Row>,

    pub readings: RefCell<Readings>,
    pub disks_held: RefCell<Vec<(String, MeterRow)>>,
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
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for SystemMonitorPopover {}

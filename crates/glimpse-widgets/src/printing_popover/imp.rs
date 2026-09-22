#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Hero, Placeholder, PopoverShell, Row, Section};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub printer: String,
    pub status: String,
    pub progress: Option<(u32, u32)>,
    pub busy: bool,
    pub cancellable: bool,
    pub pausable: bool,
    pub resumable: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Printer {
    pub id: String,
    pub name: String,
    pub status: String,
    pub network: bool,
    /// Rendered one `$Row` each in the printer's detail panel. The applet formats every label and
    /// value; the widget only draws them, so nothing here is translated in this crate.
    pub details: Vec<Detail>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Detail {
    pub icon: String,
    pub label: String,
    pub value: String,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/printing_popover.ui")]
pub struct PrintingPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub jobs: TemplateChild<Section>,
    #[template_child]
    pub empty_jobs: TemplateChild<Placeholder>,
    #[template_child]
    pub job_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub printers: TemplateChild<Section>,
    #[template_child]
    pub printer_rows: TemplateChild<gtk4::Box>,

    pub job_data: RefCell<Vec<Job>>,
    pub printer_data: RefCell<Vec<Printer>>,
    pub job_held: RefCell<Vec<(String, gtk4::Box)>>,
    pub printer_held: RefCell<Vec<(String, gtk4::Box)>>,
    pub printer_lines: RefCell<Vec<(String, Row)>>,
    #[cfg(test)]
    pub renders: Cell<u32>,
    #[cfg(test)]
    pub printer_renders: Cell<u32>,
}

#[glib::object_subclass]
impl ObjectSubclass for PrintingPopover {
    const NAME: &'static str = "PrintingPopover";
    type Type = super::PrintingPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for PrintingPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("cancelled")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("paused")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("resumed")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for PrintingPopover {}

#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Hero, PopoverShell, Row, Section};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct HistoryEntry {
    pub id: u64,
    pub title: String,
    pub value: String,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/ruler_popover.ui")]
pub struct RulerPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub measure: TemplateChild<Row>,
    #[template_child]
    pub history_section: TemplateChild<Section>,
    #[template_child]
    pub history_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub footer: TemplateChild<Row>,

    pub history_data: RefCell<Vec<HistoryEntry>>,
    pub history_held: RefCell<Vec<(u64, Row)>>,
    #[cfg(test)]
    pub renders: Cell<u32>,
}

#[glib::object_subclass]
impl ObjectSubclass for RulerPopover {
    const NAME: &'static str = "RulerPopover";
    type Type = super::RulerPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        PopoverShell::static_type();
        Hero::static_type();
        Section::static_type();
        Row::static_type();
        klass.set_layout_manager_type::<gtk4::BinLayout>();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for RulerPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([u64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("measure-requested").build(),
                glib::subclass::Signal::builder("footer-activated").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        self.measure.connect_clicked(glib::clone!(
            #[weak]
            obj,
            move |_| obj.emit_by_name::<()>("measure-requested", &[])
        ));
        self.footer.connect_clicked(glib::clone!(
            #[weak]
            obj,
            move |_| obj.emit_by_name::<()>("footer-activated", &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for RulerPopover {}

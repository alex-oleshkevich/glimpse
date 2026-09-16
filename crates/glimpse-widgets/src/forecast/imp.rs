use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::RefCell;
use std::sync::OnceLock;

use super::{Day, ForecastDay, ForecastHour, Hour};

#[derive(Debug, Default)]
pub struct ForecastStrip {
    pub hours: RefCell<Vec<Hour>>,
    pub columns: RefCell<Vec<ForecastHour>>,
    pub unit: RefCell<String>,
}

#[glib::object_subclass]
impl ObjectSubclass for ForecastStrip {
    const NAME: &'static str = "ForecastStrip";
    type Type = super::ForecastStrip;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::Group);
    }
}

impl ObjectImpl for ForecastStrip {
    fn constructed(&self) {
        self.parent_constructed();
        let strip = self.obj();
        self.unit.replace(super::DEFAULT_UNIT.to_owned());
        strip.add_css_class("forecast-strip");
        if let Some(layout) = strip.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_homogeneous(true);
        }
    }

    fn dispose(&self) {
        for column in self.columns.borrow_mut().drain(..) {
            column.unparent();
        }
    }
}

impl WidgetImpl for ForecastStrip {}

#[derive(Debug, Default)]
pub struct ForecastList {
    pub days: RefCell<Vec<Day>>,
    pub rows: RefCell<Vec<ForecastDay>>,
    pub holders: RefCell<Vec<gtk4::Box>>,
    pub unit: RefCell<String>,
}

#[glib::object_subclass]
impl ObjectSubclass for ForecastList {
    const NAME: &'static str = "ForecastList";
    type Type = super::ForecastList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::List);
    }
}

impl ObjectImpl for ForecastList {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([u32::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        self.unit.replace(super::DEFAULT_UNIT.to_owned());
        list.add_css_class("forecast-list");
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
    }

    fn dispose(&self) {
        self.rows.borrow_mut().clear();
        for holder in self.holders.borrow_mut().drain(..) {
            holder.unparent();
        }
    }
}

impl WidgetImpl for ForecastList {}

use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use crate::Row;

use super::{DETAILS_OPEN_CHANGED, Detail, Display, ENABLE_REQUESTED};

#[derive(Debug)]
pub struct DisplayList {
    pub displays: RefCell<Vec<Display>>,
    pub rows: RefCell<Vec<Row>>,
    pub holders: RefCell<Vec<gtk4::Box>>,
    pub details: RefCell<Vec<Detail>>,
    pub details_open: Cell<bool>,
    pub output_power: Cell<bool>,
    #[cfg(test)]
    pub renders: Cell<u32>,
}

impl Default for DisplayList {
    fn default() -> Self {
        Self {
            displays: RefCell::default(),
            rows: RefCell::default(),
            holders: RefCell::default(),
            details: RefCell::default(),
            details_open: Cell::default(),
            output_power: Cell::new(true),
            #[cfg(test)]
            renders: Cell::default(),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for DisplayList {
    const NAME: &'static str = "DisplayList";
    type Type = super::DisplayList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::List);
    }
}

impl ObjectImpl for DisplayList {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder(ENABLE_REQUESTED)
                    .param_types([String::static_type(), bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder(DETAILS_OPEN_CHANGED)
                    .param_types([bool::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        list.add_css_class("display-list");
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
    }

    fn dispose(&self) {
        self.rows.borrow_mut().clear();
        self.details.borrow_mut().clear();
        for holder in self.holders.borrow_mut().drain(..) {
            holder.unparent();
        }
    }
}

impl WidgetImpl for DisplayList {}

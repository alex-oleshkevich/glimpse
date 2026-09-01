use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use super::{Event, EventRow};
use crate::Row;

#[derive(Debug, Default)]
pub struct EventList {
    pub events: RefCell<Vec<Event>>,
    pub rows: RefCell<Vec<EventRow>>,
    pub overflow: RefCell<Option<Row>>,
    pub max_rows: Cell<u32>,
    pub activatable: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for EventList {
    const NAME: &'static str = "EventList";
    type Type = super::EventList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::List);
    }
}

impl ObjectImpl for EventList {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("overflow").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        list.add_css_class("event-list");
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
    }

    fn dispose(&self) {
        for row in self.rows.borrow_mut().drain(..) {
            row.unparent();
        }
        if let Some(row) = self.overflow.take() {
            row.unparent();
        }
    }
}

impl WidgetImpl for EventList {}

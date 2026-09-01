use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};

use super::{ClockRow, Zone};

#[derive(Debug, Default)]
pub struct WorldClock {
    pub zones: RefCell<Vec<Zone>>,
    pub rows: RefCell<Vec<ClockRow>>,
    pub now: RefCell<Option<glib::DateTime>>,
    pub twelve_hour: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for WorldClock {
    const NAME: &'static str = "WorldClock";
    type Type = super::WorldClock;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::List);
    }
}

impl ObjectImpl for WorldClock {
    fn constructed(&self) {
        self.parent_constructed();
        let clock = self.obj();
        clock.add_css_class("world-clock");
        if let Some(layout) = clock.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
        self.now.replace(
            glib::DateTime::now_local()
                .or_else(|_| glib::DateTime::now_utc())
                .ok(),
        );
    }

    fn dispose(&self) {
        for row in self.rows.borrow_mut().drain(..) {
            row.unparent();
        }
    }
}

impl WidgetImpl for WorldClock {}

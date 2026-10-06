use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::RefCell;

use super::Fact;
use crate::Row;

#[derive(Debug, Default)]
pub struct FactList {
    pub facts: RefCell<Vec<Fact>>,
    pub rows: RefCell<Vec<Row>>,
}

#[glib::object_subclass]
impl ObjectSubclass for FactList {
    const NAME: &'static str = "FactList";
    type Type = super::FactList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::List);
    }
}

impl ObjectImpl for FactList {
    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        list.add_css_class("fact-list");
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
    }

    fn dispose(&self) {
        for row in self.rows.borrow_mut().drain(..) {
            row.unparent();
        }
    }
}

impl WidgetImpl for FactList {}

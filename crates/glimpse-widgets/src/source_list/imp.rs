use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;
use std::sync::OnceLock;

use crate::{Fader, Row};

use super::{CHANGED, Source};

#[derive(Debug, Default)]
pub struct SourceList {
    pub sources: RefCell<Vec<Source>>,
    pub rows: RefCell<Vec<(Row, Fader)>>,
    #[cfg(test)]
    pub renders: Cell<u32>,
}

#[glib::object_subclass]
impl ObjectSubclass for SourceList {
    const NAME: &'static str = "SourceList";
    type Type = super::SourceList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::List);
    }
}

impl ObjectImpl for SourceList {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder(CHANGED)
                    .param_types([String::static_type(), f64::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        list.add_css_class("source-list");
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
    }

    fn dispose(&self) {
        for (row, fader) in self.rows.borrow_mut().drain(..) {
            row.unparent();
            fader.unparent();
        }
    }
}

impl WidgetImpl for SourceList {}

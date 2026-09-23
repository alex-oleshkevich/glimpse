use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};

use super::{Notation, Shade};

#[derive(Debug, Default)]
pub struct ColorList {
    pub shades: RefCell<Vec<Shade>>,
    pub holders: RefCell<Vec<gtk4::Box>>,
    pub open: Cell<Option<u64>>,
    pub built: RefCell<Option<(u64, Vec<Notation>)>>,
}

#[glib::object_subclass]
impl ObjectSubclass for ColorList {
    const NAME: &'static str = "ColorList";
    type Type = super::ColorList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::List);
    }
}

impl ObjectImpl for ColorList {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([u64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("detailed")
                    .param_types([u64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("copied")
                    .param_types([u64::static_type(), String::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        list.add_css_class("color-list");
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
    }

    fn dispose(&self) {
        for holder in self.holders.borrow_mut().drain(..) {
            holder.unparent();
        }
    }
}

impl WidgetImpl for ColorList {}

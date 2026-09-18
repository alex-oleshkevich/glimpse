use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};

use super::InhibitorEntry;
use crate::{FactList, Row};

#[derive(Debug)]
pub struct Item {
    pub holder: gtk4::Box,
    pub row: Row,
    pub panel: gtk4::Revealer,
    pub description: gtk4::Label,
    pub facts: FactList,
    pub cancel: gtk4::Button,
}

#[derive(Debug, Default)]
pub struct InhibitorList {
    pub entries: RefCell<Vec<InhibitorEntry>>,
    pub items: RefCell<Vec<Item>>,
    pub opened: Cell<Option<u64>>,
}

#[glib::object_subclass]
impl ObjectSubclass for InhibitorList {
    const NAME: &'static str = "InhibitorList";
    type Type = super::InhibitorList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::List);
    }
}

impl ObjectImpl for InhibitorList {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("detail-toggled")
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("release-requested")
                    .param_types([u64::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        list.add_css_class("inhibitor-list");
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
    }

    fn dispose(&self) {
        for item in self.items.borrow_mut().drain(..) {
            item.holder.unparent();
        }
    }
}

impl WidgetImpl for InhibitorList {}

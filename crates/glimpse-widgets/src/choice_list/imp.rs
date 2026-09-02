use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use super::Choice;
use crate::Row;

#[derive(Debug, Default)]
pub struct ChoiceList {
    pub choices: RefCell<Vec<Choice>>,
    pub rows: RefCell<Vec<Row>>,
    pub selected: Cell<Option<u32>>,
}

#[glib::object_subclass]
impl ObjectSubclass for ChoiceList {
    const NAME: &'static str = "ChoiceList";
    type Type = super::ChoiceList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::RadioGroup);
    }
}

impl ObjectImpl for ChoiceList {
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
        list.add_css_class("choice-list");
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

impl WidgetImpl for ChoiceList {}

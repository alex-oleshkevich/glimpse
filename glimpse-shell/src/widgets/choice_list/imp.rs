use glib::subclass::Signal;
use gtk4::{glib, prelude::*, subclass::prelude::*};
use std::{cell::RefCell, sync::OnceLock};

use crate::widgets::choice_tile::ChoiceTile;

pub struct ChoiceList {
    pub(super) rows: RefCell<Vec<(String, ChoiceTile)>>,
    pub(super) active: RefCell<Option<String>>,
}

impl Default for ChoiceList {
    fn default() -> Self {
        Self {
            rows: RefCell::new(Vec::new()),
            active: RefCell::new(None),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for ChoiceList {
    const NAME: &'static str = "GlimpseChoiceList";
    type Type = super::ChoiceList;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for ChoiceList {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.set_orientation(gtk4::Orientation::Vertical);
        obj.set_overflow(gtk4::Overflow::Hidden);
        obj.add_css_class("boxed-list");
        obj.add_css_class("choice-list");
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("changed")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }
}

impl WidgetImpl for ChoiceList {}
impl BoxImpl for ChoiceList {}

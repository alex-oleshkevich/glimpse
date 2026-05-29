use std::cell::RefCell;
use std::sync::OnceLock;

use glib::subclass::Signal;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::widgets::pager_item::PagerItem;

#[derive(Default)]
pub struct PagerStrip {
    pub(super) rows_box: gtk4::Box,
    pub(super) placeholder: gtk4::Box,
    pub(super) items: RefCell<Vec<(usize, PagerItem)>>,
}

#[glib::object_subclass]
impl ObjectSubclass for PagerStrip {
    const NAME: &'static str = "GlimpsePagerStrip";
    type Type = super::PagerStrip;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for PagerStrip {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("pager");
        obj.set_orientation(gtk4::Orientation::Horizontal);
        obj.set_valign(gtk4::Align::Center);

        self.rows_box.set_orientation(gtk4::Orientation::Horizontal);
        self.rows_box.set_valign(gtk4::Align::Center);
        obj.append(&self.rows_box);

        self.placeholder
            .set_orientation(gtk4::Orientation::Horizontal);
        self.placeholder.set_valign(gtk4::Align::Center);
        self.placeholder.add_css_class("pager-dot");
        self.placeholder.add_css_class("active");
        self.placeholder.set_visible(false);
        obj.append(&self.placeholder);
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("activated")
                    .param_types([u64::static_type()])
                    .build(),
            ]
        })
    }
}

impl WidgetImpl for PagerStrip {}
impl BoxImpl for PagerStrip {}

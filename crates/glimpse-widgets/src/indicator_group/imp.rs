use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::RefCell;
use std::sync::OnceLock;

use crate::indicator::Indicator;

const SPACING: u32 = 4;

#[derive(Debug, Default)]
pub struct IndicatorGroup {
    pub items: RefCell<Vec<(String, Indicator)>>,
}

#[glib::object_subclass]
impl ObjectSubclass for IndicatorGroup {
    const NAME: &'static str = "GlimpseIndicatorGroup";
    type Type = super::IndicatorGroup;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::Group);
    }
}

impl ObjectImpl for IndicatorGroup {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("pressed")
                    .param_types([String::static_type(), u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("scrolled")
                    .param_types([
                        String::static_type(),
                        f64::static_type(),
                        f64::static_type(),
                    ])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("indicator-group");
        obj.set_visible(false);
        if let Some(layout) = obj.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_spacing(SPACING);
        }
    }

    fn dispose(&self) {
        for (_, indicator) in self.items.borrow_mut().drain(..) {
            indicator.unparent();
        }
    }
}

impl WidgetImpl for IndicatorGroup {}

mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::indicator::{Indicator, IndicatorSpec};

glib::wrapper! {
    pub struct IndicatorGroup(ObjectSubclass<imp::IndicatorGroup>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for IndicatorGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl IndicatorGroup {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_orientation(&self, orientation: gtk4::Orientation) {
        if let Some(layout) = self.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(orientation);
        }
    }

    pub fn set_items(&self, items: &[IndicatorSpec]) {
        let imp = self.imp();
        {
            let mut current = imp.items.borrow_mut();

            for (index, spec) in items.iter().enumerate() {
                match current.get(index) {
                    Some(indicator) => indicator.apply(spec),
                    None => {
                        let indicator = Indicator::new();
                        indicator.apply(spec);
                        indicator.insert_after(self, current.last());
                        current.push(indicator);
                    }
                }
            }

            while current.len() > items.len() {
                if let Some(indicator) = current.pop() {
                    indicator.unparent();
                }
            }
        }
        self.set_visible(!items.is_empty());
    }

    pub fn connect_pressed<F: Fn(&Self, u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "pressed",
            false,
            glib::closure_local!(move |group: Self, button: u32| f(&group, button)),
        )
    }

    pub fn connect_scrolled<F: Fn(&Self, f64, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "scrolled",
            false,
            glib::closure_local!(move |group: Self, dx: f64, dy: f64| f(&group, dx, dy)),
        )
    }
}

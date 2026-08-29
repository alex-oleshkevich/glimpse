mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};
use std::collections::HashMap;

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
        let mut previous: HashMap<String, Indicator> = imp.items.borrow_mut().drain(..).collect();
        let mut next: Vec<(String, Indicator)> = Vec::with_capacity(items.len());
        let mut sibling: Option<gtk4::Widget> = None;

        for spec in items {
            if next.iter().any(|(id, _)| id == &spec.id) {
                tracing::warn!(id = %spec.id, "duplicate indicator id, skipping");
                continue;
            }

            let indicator = previous.remove(&spec.id).unwrap_or_else(|| {
                let indicator = Indicator::new();
                self.forward(&indicator, &spec.id);
                indicator
            });
            indicator.apply(spec);

            if indicator.parent().is_none() || indicator.prev_sibling() != sibling {
                indicator.insert_after(self, sibling.as_ref());
            }
            sibling = Some(indicator.clone().upcast());
            next.push((spec.id.clone(), indicator));
        }

        for (_, indicator) in previous {
            indicator.unparent();
        }

        let is_empty = next.is_empty();
        *imp.items.borrow_mut() = next;
        self.set_visible(!is_empty);
    }

    pub fn connect_pressed<F: Fn(&Self, &str, u32) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "pressed",
            false,
            glib::closure_local!(move |group: Self, id: String, button: u32| f(
                &group, &id, button
            )),
        )
    }

    pub fn connect_scrolled<F: Fn(&Self, &str, f64, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "scrolled",
            false,
            glib::closure_local!(move |group: Self, id: String, dx: f64, dy: f64| f(
                &group, &id, dx, dy
            )),
        )
    }

    fn forward(&self, indicator: &Indicator, id: &str) {
        indicator.connect_pressed(glib::clone!(
            #[weak(rename_to = group)]
            self,
            #[to_owned]
            id,
            move |_, button| group.emit_by_name::<()>("pressed", &[&id, &button])
        ));
        indicator.connect_scrolled(glib::clone!(
            #[weak(rename_to = group)]
            self,
            #[to_owned]
            id,
            move |_, dx, dy| group.emit_by_name::<()>("scrolled", &[&id, &dx, &dy])
        ));
    }
}

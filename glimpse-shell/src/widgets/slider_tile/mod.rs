mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct SliderTile(ObjectSubclass<imp::SliderTile>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl SliderTile {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_label(&self, text: Option<&str>) {
        let label = &self.imp().label;
        match text {
            Some(t) => {
                label.set_text(t);
                label.set_visible(true);
            }
            None => label.set_visible(false),
        }
    }

    pub fn set_left(&self, child: Option<impl IsA<gtk4::Widget>>) {
        let slot = &self.imp().left_slot;
        if let Some(w) = slot.first_child() {
            slot.remove(&w);
        }
        if let Some(w) = child {
            slot.append(&w);
            slot.set_visible(true);
        } else {
            slot.set_visible(false);
        }
    }

    pub fn set_value(&self, value: f64) {
        self.imp().slider.set_value(self.snapped_value(value));
    }

    pub fn value(&self) -> f64 {
        self.imp().slider.value()
    }

    pub fn set_range(&self, min: f64, max: f64) {
        self.imp().slider.set_range(min, max);
    }

    pub fn set_increments(&self, step: f64, page: f64) {
        self.imp().slider.set_increments(step, page);
    }

    pub fn set_digits(&self, digits: i32) {
        self.imp().slider.set_digits(digits);
    }

    pub fn set_snap_step(&self, step: Option<f64>) {
        let step = step.filter(|step| step.is_finite() && *step > 0.0);
        self.imp().snap_step.set(step);
        self.set_value(self.value());
    }

    pub fn connect_changed(&self, f: impl Fn(&Self, f64) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "changed",
            false,
            closure_local!(move |tile: &Self, value: f64| f(tile, value)),
        )
    }

    pub(super) fn snapped_value(&self, value: f64) -> f64 {
        let Some(step) = self.imp().snap_step.get() else {
            return value;
        };
        let adjustment = self.imp().slider.adjustment();
        snap_value(value, adjustment.lower(), adjustment.upper(), step)
    }
}

fn snap_value(value: f64, lower: f64, upper: f64, step: f64) -> f64 {
    let snapped = lower + ((value - lower) / step).round() * step;
    snapped.clamp(lower, upper)
}

impl Default for SliderTile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::gtk_available_on_this_thread;

    #[test]
    fn slider_tile_has_css_class() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = SliderTile::new();
        assert!(tile.has_css_class("slider-tile"));
    }

    #[test]
    fn default_value_is_zero() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = SliderTile::new();
        assert_eq!(tile.value(), 0.0);
    }

    #[test]
    fn set_value_updates_slider() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = SliderTile::new();
        tile.set_value(0.75);
        assert_eq!(tile.value(), 0.75);
    }

    #[test]
    fn snap_value_rounds_to_step_and_clamps_to_range() {
        assert_eq!(snap_value(1.4, 0.0, 3.0, 1.0), 1.0);
        assert_eq!(snap_value(1.6, 0.0, 3.0, 1.0), 2.0);
        assert_eq!(snap_value(3.6, 0.0, 3.0, 1.0), 3.0);
        assert_eq!(snap_value(-0.6, 0.0, 3.0, 1.0), 0.0);
    }
}

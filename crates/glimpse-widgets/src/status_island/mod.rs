mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Indicator, IndicatorSpec};

glib::wrapper! {
    pub struct StatusIsland(ObjectSubclass<imp::StatusIsland>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for StatusIsland {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusIsland {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_weather(&self, spec: Option<&IndicatorSpec>) {
        apply_slot(&self.imp().weather, spec);
    }

    pub fn set_bluetooth(&self, spec: Option<&IndicatorSpec>) {
        apply_slot(&self.imp().bluetooth, spec);
    }

    pub fn set_network(&self, spec: Option<&IndicatorSpec>) {
        apply_slot(&self.imp().network, spec);
    }

    pub fn set_layout(&self, spec: Option<&IndicatorSpec>) {
        apply_slot(&self.imp().layout, spec);
    }

    pub fn set_battery(&self, spec: Option<&IndicatorSpec>) {
        apply_slot(&self.imp().battery, spec);
    }

    pub fn set_session_available(&self, available: bool) {
        let button = &self.imp().power_button;
        if button.get_visible() == available {
            return;
        }
        button.set_visible(available);
    }

    pub fn connect_session_toggled<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "session-toggled",
            false,
            glib::closure_local!(move |island: Self| f(&island)),
        )
    }
}

fn apply_slot(indicator: &Indicator, spec: Option<&IndicatorSpec>) {
    match spec {
        Some(spec) => {
            indicator.apply(spec);
            indicator.set_visible(true);
        }
        None => {
            if indicator.get_visible() {
                indicator.set_visible(false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    fn spec(label: &str) -> IndicatorSpec {
        IndicatorSpec {
            label: Some(label.to_owned()),
            ..Default::default()
        }
    }

    fn child_named<T: IsA<gtk4::Widget>>(parent: &impl IsA<gtk4::Widget>, class: &str) -> T {
        fn find(widget: &gtk4::Widget, class: &str) -> Option<gtk4::Widget> {
            if widget.has_css_class(class) {
                return Some(widget.clone());
            }
            let mut child = widget.first_child();
            while let Some(candidate) = child {
                if let Some(found) = find(&candidate, class) {
                    return Some(found);
                }
                child = candidate.next_sibling();
            }
            None
        }

        find(parent.as_ref(), class)
            .and_downcast::<T>()
            .unwrap_or_else(|| panic!("no {class} below the widget"))
    }

    #[test]
    #[ignore = "needs a display"]
    fn status_island_states() {
        if gtk4::init().is_err() {
            return;
        }
        crate::register_resources().expect("resources");

        let island = StatusIsland::new();
        let window = gtk4::Window::new();
        window.set_child(Some(&island));
        let imp = island.imp();

        assert!(!island.is_focusable(), "the island itself takes no focus");
        for indicator in [
            &imp.weather,
            &imp.bluetooth,
            &imp.network,
            &imp.layout,
            &imp.battery,
        ] {
            assert!(
                !indicator.is_focusable(),
                "an indicator slot takes no focus"
            );
        }
        assert!(
            imp.power_button.is_focusable(),
            "the power button is the one focusable thing on the island"
        );

        assert!(!imp.weather.get_visible());
        island.set_weather(Some(&spec("14°")));
        assert!(imp.weather.get_visible());
        island.set_bluetooth(Some(&spec("WH-1000XM4")));
        island.set_network(Some(&spec("Skylink")));
        island.set_layout(Some(&spec("EN")));
        let battery = IndicatorSpec {
            label: Some("84%".to_owned()),
            icon: gio::Icon::for_string("battery-level-80-symbolic").ok(),
            ..Default::default()
        };
        island.set_battery(Some(&battery));
        let battery_label: gtk4::Label = child_named(&*imp.battery, "indicator__label");
        assert_eq!(battery_label.text().as_str(), "84%");
        let battery_icon: gtk4::Image = child_named(&*imp.battery, "indicator__icon");
        assert!(
            crate::icons_equal(battery_icon.gicon().as_ref(), battery.icon.as_ref()),
            "the battery indicator shows the spec's icon"
        );
        for indicator in [&imp.bluetooth, &imp.network, &imp.layout, &imp.battery] {
            assert!(indicator.get_visible());
        }

        island.set_weather(None);
        assert!(!imp.weather.get_visible(), "None hides the slot");

        let toggled = Rc::new(Cell::new(0));
        island.connect_session_toggled({
            let toggled = Rc::clone(&toggled);
            move |_| toggled.set(toggled.get() + 1)
        });
        assert!(imp.power_button.get_visible());
        imp.power_button.emit_clicked();
        assert_eq!(toggled.get(), 1);
        imp.power_button.emit_clicked();
        assert_eq!(toggled.get(), 2, "each click fires the signal once");

        island.set_session_available(false);
        assert!(!imp.power_button.get_visible());
        island.set_session_available(true);
        assert!(imp.power_button.get_visible());

        window.destroy();
    }
}

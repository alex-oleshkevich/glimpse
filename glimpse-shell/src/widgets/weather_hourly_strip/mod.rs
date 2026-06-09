mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct WeatherHourlyStrip(ObjectSubclass<imp::WeatherHourlyStrip>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget,
                    gtk4::Orientable;
}

pub struct WeatherHourlyItem {
    pub time: String,
    pub icon: String,
    pub temperature: String,
}

impl WeatherHourlyStrip {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_items(&self, items: &[WeatherHourlyItem]) {
        let inner = &self.imp().inner;
        let mut child_opt = inner.first_child();

        for item in items {
            if let Some(child) = child_opt.take() {
                if let Some(box_container) = child.downcast_ref::<gtk4::Box>() {
                    let mut elements = box_container.first_child();
                    if let Some(time_label) = elements.take().and_then(|w| w.downcast::<gtk4::Label>().ok()) {
                        time_label.set_label(&item.time);
                        elements = time_label.next_sibling();
                    }
                    if let Some(icon) = elements.take().and_then(|w| w.downcast::<gtk4::Image>().ok()) {
                        icon.set_icon_name(Some(&item.icon));
                        elements = icon.next_sibling();
                    }
                    if let Some(temp_label) = elements.take().and_then(|w| w.downcast::<gtk4::Label>().ok()) {
                        temp_label.set_label(&item.temperature);
                    }
                }
                child_opt = child.next_sibling();
            } else {
                inner.append(&make_slot(item));
            }
        }

        while let Some(child) = child_opt {
            let next = child.next_sibling();
            inner.remove(&child);
            child_opt = next;
        }
    }
}

impl Default for WeatherHourlyStrip {
    fn default() -> Self {
        Self::new()
    }
}

fn make_slot(item: &WeatherHourlyItem) -> gtk4::Box {
    let slot = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    slot.add_css_class("weather-hourly-col");
    slot.set_hexpand(true);

    let time = gtk4::Label::new(Some(&item.time));

    let icon = gtk4::Image::from_icon_name(&item.icon);
    icon.set_pixel_size(20);

    let temp = gtk4::Label::new(Some(&item.temperature));
    temp.add_css_class("weather-hourly-temp");

    slot.append(&time);
    slot.append(&icon);
    slot.append(&temp);
    slot
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::gtk_available_on_this_thread;

    fn item(time: &str) -> WeatherHourlyItem {
        WeatherHourlyItem {
            time: time.into(),
            icon: "weather-overcast-symbolic".into(),
            temperature: "10°".into(),
        }
    }

    fn child_count(strip: &WeatherHourlyStrip) -> usize {
        let mut n = 0;
        let mut child = strip.imp().inner.first_child();
        while child.is_some() {
            n += 1;
            child = child.unwrap().next_sibling();
        }
        n
    }

    #[test]
    fn set_items_creates_correct_slot_count() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let strip = WeatherHourlyStrip::new();
        strip.set_items(&[item("12:00"), item("13:00"), item("14:00")]);
        assert_eq!(child_count(&strip), 3);
    }

    #[test]
    fn set_items_replaces_previous_slots() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let strip = WeatherHourlyStrip::new();
        strip.set_items(&[item("12:00"), item("13:00")]);
        strip.set_items(&[item("15:00")]);
        assert_eq!(child_count(&strip), 1);
    }
}

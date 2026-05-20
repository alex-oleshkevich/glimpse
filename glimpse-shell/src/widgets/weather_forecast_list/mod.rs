mod imp;

use gtk4::{glib, prelude::*};

glib::wrapper! {
    pub struct WeatherForecastList(ObjectSubclass<imp::WeatherForecastList>)
        @extends gtk4::Grid, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

pub struct WeatherForecastItem {
    pub day_name: String,
    pub icon: String,
    pub condition: String,
    pub temperatures: String,
    pub is_today: bool,
}

impl WeatherForecastList {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_items(&self, items: &[WeatherForecastItem]) {
        while let Some(child) = self.first_child() {
            self.remove(&child);
        }
        for (i, item) in items.iter().enumerate() {
            let row = i as i32;

            let day = gtk4::Label::new(Some(&item.day_name));
            day.set_xalign(0.0);
            day.set_hexpand(true);
            day.add_css_class("weather-forecast-day");
            day.set_tooltip_text(Some(&item.condition));

            let icon = gtk4::Image::from_icon_name(&item.icon);
            icon.set_pixel_size(18);
            icon.set_halign(gtk4::Align::Center);
            icon.set_valign(gtk4::Align::Center);

            let temps = gtk4::Label::new(Some(&item.temperatures));
            temps.add_css_class("weather-forecast-temps");
            temps.set_xalign(1.0);

            if item.is_today {
                day.add_css_class("weather-forecast-today");
                icon.add_css_class("weather-forecast-today");
                temps.add_css_class("weather-forecast-today");
            }

            self.attach(&day, 0, row, 1, 1);
            self.attach(&icon, 1, row, 1, 1);
            self.attach(&temps, 2, row, 1, 1);
        }
    }
}

impl Default for WeatherForecastList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    fn item(day: &str, is_today: bool) -> WeatherForecastItem {
        WeatherForecastItem {
            day_name: day.into(),
            icon: "weather-clear-symbolic".into(),
            condition: "Clear".into(),
            temperatures: "8° / 14°".into(),
            is_today,
        }
    }

    fn row_count(list: &WeatherForecastList) -> usize {
        let mut n = 0;
        let mut child = list.first_child();
        while child.is_some() {
            n += 1;
            child = child.unwrap().next_sibling();
        }
        n / 3 // 3 cells per row
    }

    #[test]
    fn set_items_creates_correct_row_count() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let list = WeatherForecastList::new();
        list.set_items(&[item("Mon", false), item("Tue", false)]);
        assert_eq!(row_count(&list), 2);
    }

    #[test]
    fn today_row_gets_css_class() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let list = WeatherForecastList::new();
        list.set_items(&[item("Today", true)]);
        assert!(
            list.first_child()
                .unwrap()
                .has_css_class("weather-forecast-today")
        );
    }

    #[test]
    fn set_items_replaces_previous_rows() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let list = WeatherForecastList::new();
        list.set_items(&[item("Mon", false), item("Tue", false)]);
        list.set_items(&[item("Wed", false)]);
        assert_eq!(row_count(&list), 1);
    }
}

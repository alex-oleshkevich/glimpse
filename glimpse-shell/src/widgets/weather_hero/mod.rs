mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct WeatherHero(ObjectSubclass<imp::WeatherHero>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget,
                    gtk4::Orientable;
}

impl WeatherHero {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_icon(&self, name: &str) {
        self.imp().icon.set_icon_name(Some(name));
    }

    pub fn set_location(&self, text: &str) {
        self.imp().location.set_text(text);
    }

    pub fn set_condition(&self, text: &str) {
        self.imp().condition.set_text(text);
    }

    pub fn set_temperature(&self, text: &str) {
        self.imp().temperature.set_text(text);
    }
}

impl Default for WeatherHero {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn weather_hero_has_css_class() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let hero = WeatherHero::new();
        assert!(hero.has_css_class("weather-hero"));
    }

    #[test]
    fn set_condition_updates_label() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let hero = WeatherHero::new();
        hero.set_condition("Overcast · Feels like 12°");
        assert_eq!(hero.imp().condition.text(), "Overcast · Feels like 12°");
    }

    #[test]
    fn set_temperature_updates_label() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let hero = WeatherHero::new();
        hero.set_temperature("15°");
        assert_eq!(hero.imp().temperature.text(), "15°");
    }
}

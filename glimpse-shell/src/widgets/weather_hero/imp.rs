use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Default)]
pub struct WeatherHero {
    pub(super) icon: gtk4::Image,
    pub(super) location: gtk4::Label,
    pub(super) condition: gtk4::Label,
    pub(super) temperature: gtk4::Label,
}

#[glib::object_subclass]
impl ObjectSubclass for WeatherHero {
    const NAME: &'static str = "GlimpseWeatherHero";
    type Type = super::WeatherHero;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for WeatherHero {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("weather-hero");
        obj.set_orientation(gtk4::Orientation::Horizontal);
        obj.set_spacing(8);

        self.icon.set_pixel_size(32);
        self.icon.set_valign(gtk4::Align::Center);
        self.icon.set_icon_name(Some("weather-overcast-symbolic"));

        let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        text_box.set_valign(gtk4::Align::Center);
        self.location.set_halign(gtk4::Align::Start);
        self.location.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        self.location.set_max_width_chars(24);
        self.location.add_css_class("weather-hero-location");
        self.condition.set_halign(gtk4::Align::Start);
        self.condition.add_css_class("weather-hero-condition");
        self.condition.set_text("Weather unavailable");
        text_box.append(&self.location);
        text_box.append(&self.condition);

        self.temperature.set_halign(gtk4::Align::End);
        self.temperature.set_hexpand(true);
        self.temperature.set_valign(gtk4::Align::Center);
        self.temperature.set_text("—");
        self.temperature.add_css_class("weather-hero-temp");

        obj.append(&self.icon);
        obj.append(&text_box);
        obj.append(&self.temperature);
    }
}

impl WidgetImpl for WeatherHero {}
impl BoxImpl for WeatherHero {}

use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Default)]
pub struct WeatherHourlyStrip {
    pub(super) inner: gtk4::Box,
}

#[glib::object_subclass]
impl ObjectSubclass for WeatherHourlyStrip {
    const NAME: &'static str = "GlimpseWeatherHourlyStrip";
    type Type = super::WeatherHourlyStrip;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for WeatherHourlyStrip {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("weather-hourly");

        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Automatic, gtk4::PolicyType::Never);
        scroll.set_hexpand(true);

        self.inner.set_orientation(gtk4::Orientation::Horizontal);
        self.inner.set_homogeneous(true);
        self.inner.set_spacing(8);
        scroll.set_child(Some(&self.inner));

        obj.append(&scroll);
    }
}

impl WidgetImpl for WeatherHourlyStrip {}
impl BoxImpl for WeatherHourlyStrip {}

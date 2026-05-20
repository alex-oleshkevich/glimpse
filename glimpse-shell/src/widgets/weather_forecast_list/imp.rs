use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Default)]
pub struct WeatherForecastList;

#[glib::object_subclass]
impl ObjectSubclass for WeatherForecastList {
    const NAME: &'static str = "GlimpseWeatherForecastList";
    type Type = super::WeatherForecastList;
    type ParentType = gtk4::Grid;
}

impl ObjectImpl for WeatherForecastList {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("weather-forecast");
        obj.set_column_spacing(8);
        obj.set_row_spacing(2);
    }
}

impl WidgetImpl for WeatherForecastList {}
impl GridImpl for WeatherForecastList {}

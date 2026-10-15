mod day;
mod hour;
mod imp;

pub(crate) use day::ForecastDay;
pub(crate) use hour::ForecastHour;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{ExpanderRow, replace_children};

pub(crate) const DEFAULT_UNIT: &str = "°";

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Hour {
    pub label: String,
    pub icon_name: String,
    pub temperature: f64,
    pub now: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Day {
    pub label: String,
    pub icon_name: String,
    pub precipitation: Option<u32>,
    pub low: f64,
    pub high: f64,
    pub now: Option<f64>,
}

glib::wrapper! {
    pub struct ForecastStrip(ObjectSubclass<imp::ForecastStrip>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

glib::wrapper! {
    pub struct ForecastList(ObjectSubclass<imp::ForecastList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ForecastStrip {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ForecastList {
    fn default() -> Self {
        Self::new()
    }
}

impl ForecastStrip {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The symbol printed after every temperature. `weather.status` carries the unit system the
    /// numbers are in, so the caller passes what that payload declares rather than what a
    /// configuration says one round trip later.
    pub fn set_unit(&self, unit: &str) {
        let imp = self.imp();
        if imp.unit.borrow().as_str() == unit {
            return;
        }
        imp.unit.replace(unit.to_owned());
        self.render();
    }

    pub fn set_hours(&self, hours: &[Hour]) {
        let imp = self.imp();
        if imp.hours.borrow().as_slice() == hours {
            return;
        }
        imp.hours.replace(hours.to_vec());
        self.render();
    }

    fn render(&self) {
        let imp = self.imp();
        let hours = imp.hours.borrow();
        let unit = imp.unit.borrow();
        let mut columns = imp.columns.borrow_mut();

        for (index, hour) in hours.iter().enumerate() {
            if columns.len() == index {
                let column = ForecastHour::new();
                column.insert_after(self, columns.last());
                columns.push(column);
            }
            let column = &columns[index];
            column.set_label(Some(hour.label.as_str()));
            column.set_icon_name(Some(hour.icon_name.as_str()));
            column.set_temperature(Some(temperature(hour.temperature, &unit).as_str()));
            column.set_now(hour.now);
        }
        for column in columns.split_off(hours.len()) {
            column.unparent();
        }
    }
}

impl ForecastList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_unit(&self, unit: &str) {
        let imp = self.imp();
        if imp.unit.borrow().as_str() == unit {
            return;
        }
        imp.unit.replace(unit.to_owned());
        self.render();
    }

    pub fn set_days(&self, days: &[Day]) {
        let imp = self.imp();
        if imp.days.borrow().as_slice() == days {
            return;
        }
        imp.days.replace(days.to_vec());
        self.render();
    }

    pub fn scale(&self) -> (f64, f64) {
        let days = self.imp().days.borrow();
        let minimum = days.iter().map(|day| day.low).fold(f64::INFINITY, f64::min);
        let maximum = days
            .iter()
            .map(|day| day.high)
            .fold(f64::NEG_INFINITY, f64::max);
        match minimum.is_finite() && maximum > minimum {
            true => (minimum, maximum),
            false => (0.0, 1.0),
        }
    }

    pub fn set_details(&self, details: &[Option<gtk4::Widget>]) {
        for (index, (row, _)) in self.imp().holders.borrow().iter().enumerate() {
            let detail = details.get(index).and_then(Option::as_ref);
            row.set_openable(detail.is_some());
            replace_children(&row.details(), detail);
        }
    }

    fn render(&self) {
        let imp = self.imp();
        let (minimum, maximum) = self.scale();
        let days = imp.days.borrow();
        let unit = imp.unit.borrow();
        let mut holders = imp.holders.borrow_mut();

        for (index, day) in days.iter().enumerate() {
            if holders.len() == index {
                let row = ExpanderRow::new();
                let forecast = ForecastDay::new();
                row.set_end(&forecast);
                row.insert_after(self, holders.last().map(|(last, _)| last));
                holders.push((row, forecast));
            }
            let (item, row) = &holders[index];
            item.set_title(Some(day.label.as_str()));
            item.set_icon_name(Some(day.icon_name.as_str()));
            row.set_precipitation(
                day.precipitation
                    .filter(|chance| *chance > 0)
                    .map(|chance| format!("{chance}%"))
                    .as_deref(),
            );
            row.set_low(Some(temperature(day.low, &unit).as_str()));
            row.set_high(Some(temperature(day.high, &unit).as_str()));
            row.bar().set_scale(minimum, maximum);
            row.bar().set_range(day.low, day.high);
            row.bar().set_now(day.now);
        }
        for (row, _) in holders.split_off(days.len()) {
            row.unparent();
        }
    }
}

fn temperature(value: f64, unit: &str) -> String {
    format!("{}{unit}", value.round() as i64)
}

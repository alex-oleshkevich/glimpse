mod day;
mod hour;
mod imp;

pub use day::ForecastDay;
pub use hour::ForecastHour;

use gtk4::{glib, prelude::*, subclass::prelude::*};

const DEFAULT_UNIT: &str = "°";

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
            column.set_temperature(Some(temperature(hour.temperature).as_str()));
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

    pub fn connect_activated<F: Fn(&Self, u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |list: Self, index: u32| f(&list, index)),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        let (minimum, maximum) = self.scale();
        let days = imp.days.borrow();
        let mut rows = imp.rows.borrow_mut();

        for (index, day) in days.iter().enumerate() {
            if rows.len() == index {
                let row = self.build_row(index as u32);
                row.insert_after(self, rows.last());
                rows.push(row);
            }
            let row = &rows[index];
            let item: &crate::Row = row.upcast_ref();
            item.set_title(Some(day.label.as_str()));
            item.set_lead_icon(Some(day.icon_name.as_str()));
            row.set_precipitation(
                day.precipitation
                    .filter(|chance| *chance > 0)
                    .map(|chance| format!("{chance}%"))
                    .as_deref(),
            );
            row.set_low(Some(temperature(day.low).as_str()));
            row.set_high(Some(temperature(day.high).as_str()));
            row.bar().set_scale(minimum, maximum);
            row.bar().set_range(day.low, day.high);
        }
        for row in rows.split_off(days.len()) {
            row.unparent();
        }
    }

    fn build_row(&self, index: u32) -> ForecastDay {
        let row = ForecastDay::new();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("activated", &[&index])
        ));
        row
    }
}

fn temperature(value: f64) -> String {
    format!("{}{DEFAULT_UNIT}", value.round() as i64)
}

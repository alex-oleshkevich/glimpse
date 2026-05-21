use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::sync::OnceLock;

use chrono::{Datelike, Days, Local, NaiveDate};
use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};

use crate::services::calendar_events::MonthKey;

use super::super::geometry;

const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

pub struct DayCell {
    pub(super) button: gtk4::Button,
    pub(super) number: gtk4::Label,
}

#[derive(CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/calendar_month_view.ui")]
pub struct MonthView {
    #[template_child]
    grid: TemplateChild<gtk4::Grid>,
    day_cells: RefCell<Vec<DayCell>>,
    week_numbers: RefCell<Vec<gtk4::Label>>,
    pub(super) visible_month: Cell<NaiveDate>,
    pub(super) selected_date: Cell<NaiveDate>,
    event_days: RefCell<HashSet<NaiveDate>>,
}

impl Default for MonthView {
    fn default() -> Self {
        let today = Local::now().date_naive();
        Self {
            grid: TemplateChild::default(),
            day_cells: RefCell::new(Vec::new()),
            week_numbers: RefCell::new(Vec::new()),
            visible_month: Cell::new(geometry::first_of_month(today)),
            selected_date: Cell::new(today),
            event_days: RefCell::new(HashSet::new()),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for MonthView {
    const NAME: &'static str = "MonthView";
    type Type = super::MonthView;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for MonthView {
    fn constructed(&self) {
        self.parent_constructed();

        let wk_header = gtk4::Label::new(Some("Wk"));
        wk_header.add_css_class("calendar__week-number");
        wk_header.add_css_class("calendar__week-number--header");
        self.grid.attach(&wk_header, 0, 0, 1, 1);

        for (i, label) in WEEKDAYS.iter().enumerate() {
            let wd = gtk4::Label::new(Some(label));
            wd.add_css_class("calendar__weekday");
            wd.set_hexpand(true);
            self.grid.attach(&wd, (i + 1) as i32, 0, 1, 1);
        }

        let mut week_numbers = self.week_numbers.borrow_mut();
        let mut day_cells = self.day_cells.borrow_mut();
        for row in 0..6 {
            let wk = gtk4::Label::new(None);
            wk.add_css_class("calendar__week-number");
            self.grid.attach(&wk, 0, (row + 1) as i32, 1, 1);
            week_numbers.push(wk);

            for col in 0..7 {
                let button = gtk4::Button::new();
                button.add_css_class("flat");
                button.add_css_class("calendar__day");
                button.set_hexpand(true);

                let inner = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                inner.set_valign(gtk4::Align::Center);
                inner.set_halign(gtk4::Align::Center);

                let number = gtk4::Label::new(None);
                number.add_css_class("calendar__day-number");
                inner.append(&number);

                let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                dot.add_css_class("calendar__day-dot");
                dot.set_halign(gtk4::Align::Center);
                inner.append(&dot);

                button.set_child(Some(&inner));

                self.grid
                    .attach(&button, (col + 1) as i32, (row + 1) as i32, 1, 1);

                let obj = self.obj().downgrade();
                let index = row * 7 + col;
                button.connect_clicked(move |_| {
                    if let Some(o) = obj.upgrade() {
                        o.imp().on_day_clicked(index);
                    }
                });

                day_cells.push(DayCell {
                    button: button.clone(),
                    number,
                });
            }
        }
        drop(day_cells);
        drop(week_numbers);

        self.refresh();
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| vec![Signal::builder("day-selected").build()])
    }
}

impl WidgetImpl for MonthView {}
impl BoxImpl for MonthView {}

impl MonthView {
    fn refresh(&self) {
        let visible_month = self.visible_month.get();
        let selected = self.selected_date.get();
        let today = Local::now().date_naive();
        let event_days = self.event_days.borrow();

        let start = geometry::grid_start(visible_month);
        let cells = self.day_cells.borrow();
        let week_numbers = self.week_numbers.borrow();

        for row in 0..6 {
            let row_date = start
                .checked_add_days(Days::new((row * 7) as u64))
                .unwrap_or(start);
            week_numbers[row].set_label(&row_date.iso_week().week().to_string());

            for col in 0..7 {
                let i = row * 7 + col;
                let date = start
                    .checked_add_days(Days::new(i as u64))
                    .unwrap_or(start);
                let cell = &cells[i];
                cell.number.set_label(&date.day().to_string());

                set_class(&cell.button, "other-month", date.month() != visible_month.month());
                set_class(&cell.button, "today", date == today);
                set_class(&cell.button, "selected", date == selected);
                set_class(&cell.button, "has-events", event_days.contains(&date));
            }
        }
    }

    fn on_day_clicked(&self, index: usize) {
        let start = geometry::grid_start(self.visible_month.get());
        let date = start
            .checked_add_days(Days::new(index as u64))
            .unwrap_or(start);
        self.selected_date.set(date);
        self.obj().emit_by_name::<()>("day-selected", &[]);
    }

    pub(super) fn set_visible_month(&self, key: MonthKey) {
        if let Some(date) = key.to_naive_date() {
            self.visible_month.set(date);
            self.refresh();
        }
    }

    pub(super) fn set_selected_date(&self, date: NaiveDate) {
        self.selected_date.set(date);
        self.refresh();
    }

    pub(super) fn set_event_days(&self, dates: &HashSet<NaiveDate>) {
        *self.event_days.borrow_mut() = dates.clone();
        self.refresh();
    }
}

fn set_class(button: &gtk4::Button, class: &str, on: bool) {
    if on {
        button.add_css_class(class);
    } else {
        button.remove_css_class(class);
    }
}

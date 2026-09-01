use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, gdk, glib, prelude::*, subclass::prelude::*,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::OnceLock;

use super::dots::Dots;
use super::grid::{CELLS, COLUMNS, Ymd, month_grid, step_month};
use crate::set_css_class;

const TODAY: &str = "calendar__cell--today";
const SELECTED: &str = "calendar__cell--selected";
const MONTH_VIEW: &str = "month";
const YEAR_VIEW: &str = "year";

pub struct Day {
    pub button: gtk4::Button,
    pub label: gtk4::Label,
    pub dots: Dots,
    pub date: Cell<Ymd>,
}

#[derive(Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::Calendar)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/calendar.ui")]
pub struct Calendar {
    #[template_child]
    pub scope: TemplateChild<gtk4::Button>,
    #[template_child]
    pub scope_label: TemplateChild<gtk4::Label>,
    #[template_child(id = "today")]
    pub today_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub previous: TemplateChild<gtk4::Button>,
    #[template_child]
    pub next: TemplateChild<gtk4::Button>,
    #[template_child]
    pub views: TemplateChild<gtk4::Stack>,
    #[template_child]
    pub month: TemplateChild<gtk4::Grid>,
    #[template_child]
    pub year: TemplateChild<gtk4::Grid>,

    pub days: RefCell<Vec<Day>>,
    pub months: RefCell<Vec<gtk4::Button>>,
    pub weekdays: RefCell<Vec<gtk4::Label>>,
    pub events: RefCell<HashMap<Ymd, Vec<gdk::RGBA>>>,

    pub shown: Cell<(i32, u32)>,
    pub today: Cell<Ymd>,
    pub selected: Cell<Option<Ymd>>,

    #[property(name = "first-weekday", get = Self::first_weekday, set = Self::set_first_weekday)]
    weekday_start: Cell<u32>,
}

impl Calendar {
    fn first_weekday(&self) -> u32 {
        self.weekday_start.get().max(1)
    }

    fn set_first_weekday(&self, first: u32) {
        let first = first.clamp(1, COLUMNS as u32);
        if self.weekday_start.get() == first {
            return;
        }
        self.weekday_start.set(first);
        self.render();
    }

    pub fn step(&self, by: i32) {
        let (year, month) = self.shown.get();
        match self.in_year_view() {
            true => self.shown.set((year + by, month)),
            false => self.shown.set(step_month(year, month, by)),
        }
        self.render();
    }

    pub fn go_to_today(&self) {
        let today = self.today.get();
        self.shown.set((today.year, today.month));
        self.render();
        self.select(today);
    }

    pub fn select(&self, date: Ymd) {
        if self.selected.get() == Some(date) {
            return;
        }
        self.selected.set(Some(date));
        self.render();
        self.obj()
            .emit_by_name::<()>("day-selected", &[&date.year, &date.month, &date.day]);
    }

    fn in_year_view(&self) -> bool {
        self.views.visible_child_name().as_deref() == Some(YEAR_VIEW)
    }

    fn toggle_view(&self) {
        let name = match self.in_year_view() {
            true => MONTH_VIEW,
            false => YEAR_VIEW,
        };
        self.views.set_visible_child_name(name);
        self.render();
    }

    pub fn render(&self) {
        let (year, month) = self.shown.get();
        let today = self.today.get();
        let selected = self.selected.get();
        let year_view = self.in_year_view();

        self.scope_label.set_text(&match year_view {
            true => year.to_string(),
            false => format_month(year, month),
        });

        let on_today = match year_view {
            true => year == today.year,
            false => (year, month) == (today.year, today.month),
        };
        self.today_button.set_visible(!on_today);

        for (index, label) in self.weekdays.borrow().iter().enumerate() {
            let weekday = (self.first_weekday() - 1 + index as u32) % COLUMNS as u32 + 1;
            label.set_text(&format_weekday(weekday));
        }

        let cells = month_grid(year, month, self.first_weekday());
        let events = self.events.borrow();
        for (day, cell) in self.days.borrow().iter().zip(cells.iter()) {
            day.date.set(cell.date);
            day.label.set_text(&cell.date.day.to_string());
            let weekday = super::grid::weekday(cell.date.year, cell.date.month, cell.date.day);
            let is_today = cell.date == today;
            set_css_class(&day.button, "calendar__day--out", !cell.in_month);
            set_css_class(&day.button, "calendar__day--weekend", weekday >= 6);
            set_css_class(&day.button, TODAY, is_today);
            set_css_class(&day.button, SELECTED, selected == Some(cell.date));
            day.dots.set_uniform(is_today);
            match events.get(&cell.date) {
                Some(colors) => day.dots.set_colors(colors),
                None => day.dots.set_colors(&[]),
            }
        }

        for (index, button) in self.months.borrow().iter().enumerate() {
            let this = index as u32 + 1;
            set_css_class(button, TODAY, (year, this) == (today.year, today.month));
            set_css_class(button, SELECTED, this == month);
        }
    }
}

fn format_month(year: i32, month: u32) -> String {
    glib::DateTime::from_utc(year, month as i32, 1, 0, 0, 0.0)
        .and_then(|date| date.format("%OB %Y"))
        .map(|text| text.to_string())
        .unwrap_or_else(|_| format!("{month}/{year}"))
}

fn format_weekday(weekday: u32) -> String {
    glib::DateTime::from_utc(2024, 1, weekday as i32, 0, 0, 0.0)
        .and_then(|date| date.format("%a"))
        .map(|text| text.chars().take(2).collect())
        .unwrap_or_default()
}

fn format_month_short(month: u32) -> String {
    glib::DateTime::from_utc(2024, month as i32, 1, 0, 0, 0.0)
        .and_then(|date| date.format("%Ob"))
        .map(|text| text.to_string())
        .unwrap_or_else(|_| month.to_string())
}

#[glib::object_subclass]
impl ObjectSubclass for Calendar {
    const NAME: &'static str = "Calendar";
    type Type = super::Calendar;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for Calendar {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("day-selected")
                    .param_types([i32::static_type(), u32::static_type(), u32::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let today = glib::DateTime::now_local()
            .or_else(|_| glib::DateTime::now_utc())
            .map(|now| Ymd::new(now.year(), now.month() as u32, now.day_of_month() as u32))
            .unwrap_or_else(|_| Ymd::new(1970, 1, 1));
        self.today.set(today);
        self.shown.set((today.year, today.month));
        self.build_month();
        self.build_year();
        self.wire();
        self.render();
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Calendar {}

impl Calendar {
    fn build_month(&self) {
        let mut weekdays = Vec::with_capacity(COLUMNS);
        for column in 0..COLUMNS {
            let label = gtk4::Label::new(None);
            label.add_css_class("calendar__weekday");
            self.month.attach(&label, column as i32, 0, 1, 1);
            weekdays.push(label);
        }
        self.weekdays.replace(weekdays);

        let mut days = Vec::with_capacity(CELLS);
        for index in 0..CELLS {
            let label = gtk4::Label::new(None);
            label.add_css_class("calendar__number");
            let dots = Dots::new();
            dots.add_css_class("calendar__dots");

            let stack = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            stack.set_valign(gtk4::Align::Center);
            stack.append(&label);
            stack.append(&dots);

            let button = gtk4::Button::builder()
                .has_frame(false)
                .halign(gtk4::Align::Center)
                .valign(gtk4::Align::Center)
                .child(&stack)
                .build();
            button.add_css_class("calendar__day");
            self.month.attach(
                &button,
                (index % COLUMNS) as i32,
                (index / COLUMNS) as i32 + 1,
                1,
                1,
            );

            days.push(Day {
                button,
                label,
                dots,
                date: Cell::new(Ymd::new(1970, 1, 1)),
            });
        }

        for (index, day) in days.iter().enumerate() {
            let calendar = self.obj().clone();
            day.button.connect_clicked(move |_| {
                let imp = calendar.imp();
                let date = imp.days.borrow()[index].date.get();
                imp.shown.set((date.year, date.month));
                imp.select(date);
            });
        }
        self.days.replace(days);
    }

    fn build_year(&self) {
        let mut months = Vec::with_capacity(12);
        for index in 0..12u32 {
            let button = gtk4::Button::builder()
                .has_frame(false)
                .label(format_month_short(index + 1))
                .build();
            button.add_css_class("calendar__month");
            self.year
                .attach(&button, (index % 4) as i32, (index / 4) as i32, 1, 1);

            let calendar = self.obj().clone();
            button.connect_clicked(move |_| {
                let imp = calendar.imp();
                imp.shown.set((imp.shown.get().0, index + 1));
                imp.views.set_visible_child_name(MONTH_VIEW);
                imp.render();
            });
            months.push(button);
        }
        self.months.replace(months);
    }

    fn wire(&self) {
        let calendar = self.obj().clone();
        self.previous
            .connect_clicked(move |_| calendar.imp().step(-1));

        let calendar = self.obj().clone();
        self.next.connect_clicked(move |_| calendar.imp().step(1));

        let calendar = self.obj().clone();
        self.today_button
            .connect_clicked(move |_| calendar.imp().go_to_today());

        let calendar = self.obj().clone();
        self.scope
            .connect_clicked(move |_| calendar.imp().toggle_view());

        let scrolling = gtk4::EventControllerScroll::new(
            gtk4::EventControllerScrollFlags::VERTICAL | gtk4::EventControllerScrollFlags::DISCRETE,
        );
        let calendar = self.obj().clone();
        scrolling.connect_scroll(move |_, _, delta| {
            let steps = delta as i32;
            if steps != 0 {
                calendar.imp().step(steps);
            }
            glib::Propagation::Stop
        });
        self.views.add_controller(scrolling);
    }
}

use std::cell::{Cell, OnceCell};
use std::collections::HashSet;
use std::sync::OnceLock;

use chrono::{Datelike, Local, NaiveDate};
use glib::subclass::Signal;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::services::calendar_events::MonthKey;

use super::controls::CalendarControls;
use super::geometry::{self, first_of_month};
use super::month_view::MonthView;
use super::year_view::YearView;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum View {
    #[default]
    Days,
    Year,
}

pub struct Calendar {
    controls: OnceCell<CalendarControls>,
    stack: OnceCell<gtk4::Stack>,
    month_view: OnceCell<MonthView>,
    year_view: OnceCell<YearView>,
    pub(super) selected_date: Cell<NaiveDate>,
    pub(super) visible_month: Cell<NaiveDate>,
    view: Cell<View>,
}

impl Default for Calendar {
    fn default() -> Self {
        let today = Local::now().date_naive();
        Self {
            controls: OnceCell::new(),
            stack: OnceCell::new(),
            month_view: OnceCell::new(),
            year_view: OnceCell::new(),
            selected_date: Cell::new(today),
            visible_month: Cell::new(first_of_month(today)),
            view: Cell::new(View::Days),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Calendar {
    const NAME: &'static str = "GlimpseCalendar";
    type Type = super::Calendar;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for Calendar {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.set_orientation(gtk4::Orientation::Vertical);
        obj.set_spacing(8);
        obj.add_css_class("calendar");

        let controls = CalendarControls::new();
        let stack = gtk4::Stack::new();
        stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
        stack.set_transition_duration(120);
        let month_view = MonthView::new();
        let year_view = YearView::new();

        stack.add_named(&month_view, Some("month"));
        stack.add_named(&year_view, Some("year"));

        obj.append(&controls);
        obj.append(&stack);

        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        let weak = obj.downgrade();
        scroll.connect_scroll(move |_, _dx, dy| {
            if let Some(o) = weak.upgrade() {
                if dy < 0.0 {
                    o.imp().step(-1);
                } else if dy > 0.0 {
                    o.imp().step(1);
                }
            }
            glib::Propagation::Stop
        });
        obj.add_controller(scroll);

        wire(
            &obj,
            |imp| imp.step(-1),
            |f| {
                controls.connect_prev_clicked(f);
            },
        );
        wire(
            &obj,
            |imp| imp.step(1),
            |f| {
                controls.connect_next_clicked(f);
            },
        );
        wire(
            &obj,
            |imp| imp.go_to_today(),
            |f| {
                controls.connect_today_clicked(f);
            },
        );
        wire(
            &obj,
            |imp| imp.toggle_view(),
            |f| {
                controls.connect_title_clicked(f);
            },
        );

        {
            let weak = obj.downgrade();
            month_view.connect_day_selected(move |mv| {
                if let Some(o) = weak.upgrade() {
                    o.imp().on_month_view_day_selected(mv.selected_date());
                }
            });
        }
        {
            let weak = obj.downgrade();
            year_view.connect_month_picked(move |yv, month| {
                if let Some(o) = weak.upgrade() {
                    o.imp().on_year_view_month_picked(yv.visible_year(), month);
                }
            });
        }

        let _ = self.controls.set(controls);
        let _ = self.stack.set(stack);
        let _ = self.month_view.set(month_view);
        let _ = self.year_view.set(year_view);

        self.sync_children();
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("day-selected").build(),
                Signal::builder("month-changed").build(),
            ]
        })
    }
}

impl WidgetImpl for Calendar {}
impl BoxImpl for Calendar {}

impl Calendar {
    fn children(&self) -> Option<(&CalendarControls, &gtk4::Stack, &MonthView, &YearView)> {
        Some((
            self.controls.get()?,
            self.stack.get()?,
            self.month_view.get()?,
            self.year_view.get()?,
        ))
    }

    fn sync_children(&self) {
        let Some((controls, stack, month_view, year_view)) = self.children() else {
            return;
        };

        let visible_month = self.visible_month.get();
        match self.view.get() {
            View::Days => {
                stack.set_visible_child_name("month");
                controls.set_title(&format_month(visible_month));
                month_view.set_visible_month(MonthKey::from_date(visible_month));
                month_view.set_selected_date(self.selected_date.get());
            }
            View::Year => {
                stack.set_visible_child_name("year");
                controls.set_title(&visible_month.year().to_string());
                year_view.set_current_month(visible_month.year(), visible_month.month());
            }
        }
    }

    fn step(&self, delta: i32) {
        match self.view.get() {
            View::Days => self.step_month(delta),
            View::Year => self.step_year(delta),
        }
    }

    fn step_month(&self, delta: i32) {
        let new_month = geometry::shift_month(self.visible_month.get(), delta);
        self.visible_month.set(new_month);
        self.sync_children();
        self.obj().emit_by_name::<()>("month-changed", &[]);
    }

    fn step_year(&self, delta: i32) {
        let current = self.visible_month.get();
        let new_month =
            NaiveDate::from_ymd_opt(current.year() + delta, current.month(), 1).unwrap_or(current);
        self.visible_month.set(new_month);
        self.sync_children();
    }

    fn go_to_today(&self) {
        let today = Local::now().date_naive();
        let month = first_of_month(today);
        let month_changed = month != self.visible_month.get();
        self.selected_date.set(today);
        self.visible_month.set(month);
        self.view.set(View::Days);
        self.sync_children();
        if month_changed {
            self.obj().emit_by_name::<()>("month-changed", &[]);
        }
        self.obj().emit_by_name::<()>("day-selected", &[]);
    }

    fn toggle_view(&self) {
        if self.view.get() != View::Days {
            return;
        }
        self.view.set(View::Year);
        self.sync_children();
    }

    fn on_month_view_day_selected(&self, date: NaiveDate) {
        let new_month = first_of_month(date);
        let month_changed = new_month != self.visible_month.get();

        self.selected_date.set(date);
        self.visible_month.set(new_month);
        if month_changed {
            self.sync_children();
            self.obj().emit_by_name::<()>("month-changed", &[]);
        }
        self.obj().emit_by_name::<()>("day-selected", &[]);
    }

    fn on_year_view_month_picked(&self, year: i32, month: u32) {
        let new_month = NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(self.visible_month.get());
        let changed = new_month != self.visible_month.get();
        self.visible_month.set(new_month);
        self.view.set(View::Days);
        self.sync_children();
        if changed {
            self.obj().emit_by_name::<()>("month-changed", &[]);
        }
    }

    pub(super) fn set_selected_date(&self, date: NaiveDate) {
        self.selected_date.set(date);
        self.visible_month.set(first_of_month(date));
        self.sync_children();
    }

    pub(super) fn set_event_days(&self, dates: &HashSet<NaiveDate>) {
        if let Some(month_view) = self.month_view.get() {
            month_view.set_event_days(dates);
        }
    }
}

fn wire(
    obj: &super::Calendar,
    action: impl Fn(&Calendar) + 'static,
    connect: impl FnOnce(Box<dyn Fn(&CalendarControls)>),
) {
    let weak = obj.downgrade();
    connect(Box::new(move |_| {
        if let Some(o) = weak.upgrade() {
            action(o.imp());
        }
    }));
}

fn format_month(date: NaiveDate) -> String {
    date.format("%B %Y").to_string()
}

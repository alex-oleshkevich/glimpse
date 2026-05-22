use std::cell::{Cell, RefCell};

use chrono::{Local, NaiveDate};
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};

use crate::applets::clock::format;
use crate::services::calendar_events::CalendarEvent;

use super::row::EventRow;

#[derive(CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/events.ui")]
pub struct Events {
    #[template_child]
    list: TemplateChild<gtk4::Box>,
    #[template_child]
    empty: TemplateChild<gtk4::Label>,
    rows: RefCell<Vec<EventRow>>,
    cached_events: RefCell<Vec<CalendarEvent>>,
    selected_date: Cell<NaiveDate>,
}

impl Default for Events {
    fn default() -> Self {
        Self {
            list: TemplateChild::default(),
            empty: TemplateChild::default(),
            rows: RefCell::default(),
            cached_events: RefCell::default(),
            selected_date: Cell::new(Local::now().date_naive()),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Events {
    const NAME: &'static str = "Events";
    type Type = super::Events;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for Events {}
impl WidgetImpl for Events {}
impl BoxImpl for Events {}

impl Events {
    pub(super) fn set_data(&self, date: NaiveDate, events: &[CalendarEvent], loading: bool) {
        self.selected_date.set(date);
        *self.cached_events.borrow_mut() = events.to_vec();

        let mut rows = self.rows.borrow_mut();
        while rows.len() < events.len() {
            let row = EventRow::new();
            self.list.append(&row);
            rows.push(row);
        }
        while rows.len() > events.len() {
            if let Some(row) = rows.pop() {
                self.list.remove(&row);
            }
        }

        let now = Local::now();
        for (row, event) in rows.iter().zip(events) {
            row.set_title(&event.title);
            row.set_time(&format::event_time(event, date, now));
            row.set_tooltip_text(tooltip_for(event).as_deref());
        }

        let has_events = !events.is_empty();
        self.list.set_visible(has_events);
        self.empty.set_visible(!has_events);
        if !has_events {
            self.empty.set_label(empty_label(date, loading));
        }
    }

    pub(super) fn tick(&self) {
        let date = self.selected_date.get();
        let events = self.cached_events.borrow();
        let rows = self.rows.borrow();
        let now = Local::now();
        for (row, event) in rows.iter().zip(events.iter()) {
            row.set_time(&format::event_time(event, date, now));
        }
    }
}

fn empty_label(date: NaiveDate, loading: bool) -> &'static str {
    if loading {
        "Loading..."
    } else if date == Local::now().date_naive() {
        "No more events today"
    } else {
        "No events"
    }
}

fn tooltip_for(event: &CalendarEvent) -> Option<String> {
    let location = event
        .location
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let source = Some(event.source.display_name.trim()).filter(|s| !s.is_empty());
    match (location, source) {
        (Some(loc), Some(src)) => Some(format!("{loc} · {src}")),
        (Some(loc), None) => Some(loc.to_string()),
        (None, Some(src)) => Some(src.to_string()),
        (None, None) => None,
    }
}

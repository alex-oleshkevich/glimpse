mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::Row;
use imp::Entry;

const UNKNOWN: &str = "—";
const TWENTY_FOUR_HOUR: &str = "%R";
const TWELVE_HOUR: &str = "%l:%M %p";
const OFFSET: &str = "%Z (UTC%:z)";
const DAY_ICON: &str = "weather-clear-symbolic";
const NIGHT_ICON: &str = "weather-clear-night-symbolic";
const DAY: std::ops::Range<i32> = 7..19;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Zone {
    pub label: String,
    pub timezone: String,
    pub note: String,
    pub icon_name: String,
}

glib::wrapper! {
    pub struct WorldClock(ObjectSubclass<imp::WorldClock>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for WorldClock {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldClock {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_zones(&self, zones: &[Zone]) {
        let imp = self.imp();
        if imp.zones.borrow().as_slice() == zones {
            return;
        }
        imp.zones.replace(zones.to_vec());
        self.render();
    }

    pub fn set_now(&self, now: &glib::DateTime) {
        self.imp().now.replace(Some(now.clone()));
        self.render();
    }

    pub fn set_twelve_hour(&self, twelve_hour: bool) {
        if self.imp().twelve_hour.replace(twelve_hour) == twelve_hour {
            return;
        }
        self.render();
    }

    fn render(&self) {
        let imp = self.imp();
        let Some(now) = imp.now.borrow().clone() else {
            return;
        };
        let format = match imp.twelve_hour.get() {
            true => TWELVE_HOUR,
            false => TWENTY_FOUR_HOUR,
        };

        let zones = imp.zones.borrow();
        let mut rows = imp.rows.borrow_mut();
        for (index, zone) in zones.iter().enumerate() {
            if rows.len() == index {
                let entry = build_row();
                entry
                    .row
                    .insert_after(self, rows.last().map(|entry| &entry.row));
                rows.push(entry);
            }
            let entry = &rows[index];
            let there = glib::TimeZone::from_identifier(Some(&zone.timezone))
                .and_then(|timezone| now.to_timezone(&timezone).ok());

            entry.row.set_title(Some(zone.label.as_str()));
            match &there {
                Some(there) => {
                    let time = there.format(format).unwrap_or_default();
                    entry.time.set_text(time.trim());
                }
                None => entry.time.set_text(UNKNOWN),
            }
            let day = there.as_ref().and_then(|there| day_note(&now, there));
            entry.row.set_subtitle(subtitle(day, &zone.note).as_deref());
            set_phase(&entry.phase, there.as_ref(), &zone.icon_name);
            set_tooltip(&entry.row, &zone.timezone, there.as_ref());
        }

        for entry in rows.split_off(zones.len()) {
            entry.row.unparent();
        }
    }
}

fn build_row() -> Entry {
    let row = Row::new();
    row.set_can_focus(false);

    let phase = gtk4::Image::builder()
        .css_classes(["world-clock__phase"])
        .accessible_role(gtk4::AccessibleRole::Presentation)
        .build();
    row.set_lead(&phase);

    let time = gtk4::Label::builder()
        .xalign(1.0)
        .single_line_mode(true)
        .css_classes(["world-clock__time"])
        .build();
    row.set_trail(&time);

    Entry { row, time, phase }
}

fn set_phase(phase: &gtk4::Image, there: Option<&glib::DateTime>, icon_name: &str) {
    let icon = match (icon_name.is_empty(), there) {
        (false, _) => Some(icon_name),
        (true, Some(there)) if DAY.contains(&there.hour()) => Some(DAY_ICON),
        (true, Some(_)) => Some(NIGHT_ICON),
        (true, None) => None,
    };
    if phase.icon_name().as_deref() == icon {
        return;
    }
    phase.set_icon_name(icon);
    phase.set_visible(icon.is_some());
}

fn set_tooltip(row: &Row, timezone: &str, there: Option<&glib::DateTime>) {
    let identifier = crate::truncate(timezone, crate::TEXT_MAX_CHARS);
    let tooltip = match there.and_then(|there| there.format(OFFSET).ok()) {
        Some(offset) => format!("{identifier} · {offset}"),
        None => identifier,
    };
    if row.tooltip_text().as_deref() == Some(tooltip.as_str()) {
        return;
    }
    row.set_tooltip_text(Some(&tooltip));
}

fn subtitle(day: Option<&str>, note: &str) -> Option<String> {
    match (day, note.is_empty()) {
        (Some(day), true) => Some(day.to_owned()),
        (Some(day), false) => Some(format!("{day} · {note}")),
        (None, true) => None,
        (None, false) => Some(note.to_owned()),
    }
}

fn day_note(here: &glib::DateTime, there: &glib::DateTime) -> Option<&'static str> {
    let day = |moment: &glib::DateTime| (moment.year(), moment.day_of_year());
    match day(there).cmp(&day(here)) {
        std::cmp::Ordering::Greater => Some("Tomorrow"),
        std::cmp::Ordering::Less => Some("Yesterday"),
        std::cmp::Ordering::Equal => None,
    }
}

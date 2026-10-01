use std::collections::BTreeMap;

use chrono::{
    DateTime, Datelike, Local, Months, NaiveDate, NaiveTime, TimeDelta, TimeZone as _, Utc,
};
use gettextrs::gettext;
use glimpse_widgets::{Event, Ymd, Zone};
use gtk4::glib;

use crate::applets::agenda::{self, Occasion};

pub fn ymd(date: NaiveDate) -> Ymd {
    Ymd::new(date.year(), date.month(), date.day())
}

pub fn date(date: Ymd) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(date.year, date.month, date.day)
}

pub fn heading(today: NaiveDate, week_numbers: bool) -> (String, Option<String>) {
    let title = local(today, "%A, %-d %B");
    let week = week_numbers.then(|| {
        gettext("Week {number}").replace("{number}", &today.iso_week().week().to_string())
    });
    (title, week)
}

pub fn day_title(day: NaiveDate, today: NaiveDate) -> String {
    match day - today {
        difference if difference == TimeDelta::zero() => gettext("Today"),
        difference if difference == TimeDelta::days(1) => gettext("Tomorrow"),
        difference if difference == TimeDelta::days(-1) => gettext("Yesterday"),
        _ => local(day, "%A, %-d %b"),
    }
}

fn local(day: NaiveDate, format: &str) -> String {
    glib::DateTime::from_local(day.year(), day.month() as i32, day.day() as i32, 12, 0, 0.0)
        .and_then(|date| date.format(format))
        .map(|text| text.to_string())
        .unwrap_or_else(|_| day.to_string())
}

pub fn truncated(day: NaiveDate, truncated_from: Option<DateTime<Utc>>) -> bool {
    truncated_from.is_some_and(|from| day >= from.with_timezone(&Local).date_naive())
}

pub fn month_pair(year: i32, month: u32) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let after = first.checked_add_months(Months::new(2))?;
    Some((midnight(first), midnight(after)))
}

fn midnight(day: NaiveDate) -> DateTime<Utc> {
    let local = day.and_time(NaiveTime::MIN);
    Local
        .from_local_datetime(&local)
        .earliest()
        .map(|instant| instant.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&local))
}

pub fn rows(
    now: DateTime<Local>,
    day: NaiveDate,
    events: &[Occasion],
    clock: &str,
    hide_all_day: bool,
) -> Vec<Event> {
    events
        .iter()
        .filter(|event| covers(event, day) && !(hide_all_day && event.all_day))
        .map(|event| Event {
            past: day == now.date_naive() && !event.all_day && event.end <= now,
            links: agenda::links(event),
            facts: agenda::facts(event),
            ..agenda::row(now, day, event, clock)
        })
        .collect()
}

pub fn markers(events: &[Occasion], hide_all_day: bool) -> Vec<(Ymd, Vec<gtk4::gdk::RGBA>)> {
    let mut marked: BTreeMap<NaiveDate, Vec<gtk4::gdk::RGBA>> = BTreeMap::new();
    for event in events {
        if hide_all_day && event.all_day {
            continue;
        }
        let Some(color) = event.color else {
            continue;
        };
        for day in days(event) {
            let colors = marked.entry(day).or_default();
            if !colors.contains(&color) {
                colors.push(color);
            }
        }
    }
    marked
        .into_iter()
        .map(|(day, colors)| (ymd(day), colors))
        .collect()
}

fn covers(event: &Occasion, day: NaiveDate) -> bool {
    let first = event.start.date_naive();
    (first..=event.end.date_naive().max(first)).contains(&day)
}

fn days(event: &Occasion) -> impl Iterator<Item = NaiveDate> {
    let first = event.start.date_naive();
    let last = event.end.date_naive().max(first);
    first.iter_days().take_while(move |day| *day <= last)
}

pub fn zones(configured: &[glimpse_config::ClockTimezone]) -> Vec<Zone> {
    configured
        .iter()
        .map(|zone| Zone {
            label: zone.label.clone(),
            timezone: zone.timezone.clone(),
            note: zone.note.clone().unwrap_or_default(),
            icon_name: zone.icon.clone().unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests;

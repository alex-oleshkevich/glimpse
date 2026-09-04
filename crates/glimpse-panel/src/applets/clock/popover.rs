use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Local, NaiveDate, TimeDelta};
use glimpse_widgets::{CalendarPopover, Event, Ymd, Zone};
use gtk4::glib;

use super::agenda::{Occasion, when};

const TWENTY_FOUR: &str = "%H:%M";
const TWELVE: &str = "%l:%M %p";

pub fn locale_is_twelve_hour() -> bool {
    let Ok(afternoon) = glib::DateTime::from_local(2026, 1, 1, 15, 30, 0.0) else {
        return false;
    };
    let shown = afternoon.format("%X").unwrap_or_default();
    let marker = afternoon.format("%p").unwrap_or_default();
    reads_as_twelve_hour(&shown, &marker)
}

pub fn reads_as_twelve_hour(shown: &str, marker: &str) -> bool {
    !marker.is_empty() && shown.contains(marker)
}

pub fn ymd(date: NaiveDate) -> Ymd {
    Ymd::new(date.year(), date.month(), date.day())
}

pub fn date(date: Ymd) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(date.year, date.month, date.day)
}

pub fn heading(day: NaiveDate, week_numbers: bool) -> (String, Option<String>) {
    let title = day.format("%A, %-d %B").to_string();
    let week = week_numbers.then(|| format!("Week {}", day.iso_week().week()));
    (title, week)
}

pub fn day_title(day: NaiveDate, today: NaiveDate) -> String {
    match day - today {
        difference if difference == TimeDelta::zero() => "Today".to_owned(),
        difference if difference == TimeDelta::days(1) => "Tomorrow".to_owned(),
        difference if difference == TimeDelta::days(-1) => "Yesterday".to_owned(),
        _ => day.format("%A").to_string(),
    }
}

pub fn rows(now: DateTime<Local>, day: NaiveDate, events: &[Occasion], clock: &str) -> Vec<Event> {
    events
        .iter()
        .filter(|event| covers(event, day))
        .map(|event| Event {
            summary: event.summary.clone(),
            detail: event.detail.clone(),
            when: when(now, day, event, clock),
            color: event.color,
        })
        .collect()
}

pub fn markers(events: &[Occasion]) -> Vec<(Ymd, Vec<gtk4::gdk::RGBA>)> {
    let mut marked: BTreeMap<NaiveDate, Vec<gtk4::gdk::RGBA>> = BTreeMap::new();
    for event in events {
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

pub fn render(
    shown: &CalendarPopover,
    now: DateTime<Local>,
    day: NaiveDate,
    events: &[Occasion],
    twelve: bool,
    week_numbers: bool,
) {
    let clock = match twelve {
        true => TWELVE,
        false => TWENTY_FOUR,
    };
    let (title, week) = heading(day, week_numbers);
    shown.set_heading(&title, week.as_deref());
    shown.set_day(
        &day_title(day, now.date_naive()),
        &rows(now, day, events, clock),
    );
    if let Ok(instant) = glib::DateTime::from_unix_local(now.timestamp()) {
        shown.set_now(&instant);
    }
}

pub fn fixture(today: NaiveDate) -> Vec<Occasion> {
    let work = gtk4::gdk::RGBA::new(0.88, 0.11, 0.14, 1.0);
    let personal = gtk4::gdk::RGBA::new(0.26, 0.52, 0.96, 1.0);
    let tomorrow = today.succ_opt().unwrap_or(today);
    let at = |day: NaiveDate, hour: u32, minute: u32| {
        day.and_hms_opt(hour, minute, 0)
            .and_then(|naive| naive.and_local_timezone(Local).single())
    };

    [
        (
            "Standup",
            "Meeting room 2",
            (today, 9, 0),
            (today, 9, 15),
            work,
        ),
        ("Design review", "", (today, 11, 0), (today, 12, 0), work),
        ("Lunch", "", (today, 12, 30), (today, 13, 15), personal),
        ("One to one", "", (today, 14, 0), (today, 14, 30), work),
        ("Retrospective", "", (today, 16, 0), (today, 17, 0), work),
        ("Gym", "", (today, 18, 0), (today, 19, 0), personal),
        ("Conference", "", (today, 0, 0), (tomorrow, 0, 0), personal),
        ("Dentist", "", (tomorrow, 8, 30), (tomorrow, 9, 0), personal),
    ]
    .into_iter()
    .filter_map(|(summary, detail, start, end, color)| {
        Some(Occasion {
            summary: summary.to_owned(),
            detail: detail.to_owned(),
            start: at(start.0, start.1, start.2)?,
            end: at(end.0, end.1, end.2)?,
            all_day: summary == "Conference",
            color: Some(color),
        })
    })
    .collect()
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

pub fn run(command: &[String]) {
    let Some(program) = command.first() else {
        return;
    };
    let argv: Vec<&std::ffi::OsStr> = command.iter().map(|argument| argument.as_ref()).collect();

    if let Err(error) = gtk4::gio::Subprocess::newv(&argv, gtk4::gio::SubprocessFlags::NONE) {
        tracing::warn!(program, %error, "settings-command did not start");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(day: u32, hour: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 9, day, hour, 0, 0)
            .single()
            .expect("an unambiguous local time")
    }

    fn event(start: DateTime<Local>, end: DateTime<Local>) -> Occasion {
        Occasion {
            summary: "Standup".to_owned(),
            detail: String::new(),
            start,
            end,
            all_day: false,
            color: None,
        }
    }

    #[test]
    fn a_locale_that_writes_a_meridiem_into_its_own_time_reads_as_twelve_hour() {
        assert!(reads_as_twelve_hour("3:30:00 PM", "PM"));
        assert!(!reads_as_twelve_hour("15:30:00", "PM"));
        assert!(
            !reads_as_twelve_hour("15:30:00", ""),
            "a locale with no meridiem string makes `contains` trivially true"
        );
    }

    #[test]
    fn a_day_is_named_relatively_only_next_to_today() {
        let today = at(4, 12).date_naive();
        assert_eq!(day_title(today, today), "Today");
        assert_eq!(day_title(today + TimeDelta::days(1), today), "Tomorrow");
        assert_eq!(day_title(today - TimeDelta::days(1), today), "Yesterday");
        assert_eq!(day_title(today + TimeDelta::days(3), today), "Monday");
    }

    #[test]
    fn an_event_appears_under_every_day_it_covers() {
        let overnight = event(at(4, 22), at(5, 6));
        let rows = rows(at(4, 23), at(5, 6).date_naive(), &[overnight], TWENTY_FOUR);

        assert_eq!(rows.len(), 1, "a night-spanning event belongs to both days");
    }

    #[test]
    fn a_day_with_nothing_on_it_has_no_rows() {
        let meeting = event(at(4, 9), at(4, 10));
        assert!(rows(at(4, 12), at(6, 9).date_naive(), &[meeting], TWENTY_FOUR).is_empty());
    }

    #[test]
    fn a_marker_carries_one_dot_per_calendar_not_one_per_event() {
        let color = gtk4::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0);
        let mut first = event(at(4, 9), at(4, 10));
        first.color = Some(color);
        let mut second = event(at(4, 14), at(4, 15));
        second.color = Some(color);

        let markers = markers(&[first, second]);
        assert_eq!(markers.len(), 1);
        assert_eq!(
            markers[0].1.len(),
            1,
            "two events from one calendar are one dot"
        );
    }

    #[test]
    fn a_week_number_is_offered_only_when_it_was_asked_for() {
        let day = at(4, 12).date_naive();
        assert_eq!(heading(day, true).1.as_deref(), Some("Week 36"));
        assert_eq!(heading(day, false).1, None);
        assert_eq!(heading(day, true).0, "Friday, 4 September");
    }
}

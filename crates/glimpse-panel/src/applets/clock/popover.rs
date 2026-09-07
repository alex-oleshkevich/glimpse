use std::collections::BTreeMap;

use chrono::{
    DateTime, Datelike, Local, Months, NaiveDate, NaiveTime, TimeDelta, TimeZone as _, Utc,
};
use glimpse_contracts::CalendarEvent;
use glimpse_widgets::{Event, Ymd, Zone};
use gtk4::{gdk, glib};

use super::agenda::{Occasion, when};

pub const TWENTY_FOUR: &str = "%H:%M";
pub const TWELVE: &str = "%l:%M %p";

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

pub fn occasions(events: &[CalendarEvent]) -> Vec<Occasion> {
    events
        .iter()
        .map(|event| Occasion {
            summary: event.summary.clone(),
            detail: event.detail.clone(),
            start: event.start.with_timezone(&Local),
            end: event.end.with_timezone(&Local),
            all_day: event.all_day,
            color: event
                .color
                .as_deref()
                .and_then(|text| gdk::RGBA::parse(text).ok()),
        })
        .collect()
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

    /// The mark is the first instant the daemon's list stops being complete, so the day holding
    /// it is already incomplete and must not read as "nothing scheduled".
    #[test]
    fn a_day_at_or_after_the_truncation_mark_is_truncated() {
        let from = at(6, 22).with_timezone(&Utc);
        let day = at(6, 12).date_naive();

        assert!(!truncated(day - TimeDelta::days(1), Some(from)));
        assert!(
            truncated(day, Some(from)),
            "the day the mark falls on is already missing entries"
        );
        assert!(truncated(day + TimeDelta::days(1), Some(from)));
        assert!(
            !truncated(day + TimeDelta::days(400), None),
            "an untruncated list leaves every day loaded"
        );
    }

    /// The panel asks for the month it is showing and the one after it, so an event that starts
    /// on the last day of the shown month and runs into the next is still in the answer.
    #[test]
    fn a_month_pair_starts_at_its_own_first_day_and_ends_two_months_later() {
        let (from, to) = month_pair(2026, 12).expect("December is a month");

        assert_eq!(
            from.with_timezone(&Local).date_naive(),
            NaiveDate::from_ymd_opt(2026, 12, 1).expect("the first of December")
        );
        assert_eq!(
            to.with_timezone(&Local).date_naive(),
            NaiveDate::from_ymd_opt(2027, 2, 1).expect("the first of February"),
            "two months on from December is February, across the year boundary"
        );
        assert!(
            month_pair(2026, 13).is_none(),
            "there is no thirteenth month"
        );
    }

    /// A colour that will not parse must cost its event a dot, not the whole popover.
    #[test]
    fn an_event_converts_from_the_wire_and_survives_a_bad_color() {
        let wire = |color: Option<&str>| glimpse_contracts::CalendarEvent {
            source: "work".to_owned(),
            summary: "Standup".to_owned(),
            detail: "Room 2".to_owned(),
            start: at(4, 9).with_timezone(&Utc),
            end: at(4, 10).with_timezone(&Utc),
            all_day: false,
            color: color.map(str::to_owned),
        };

        let converted = occasions(&[wire(Some("#e0563f")), wire(Some("nonsense")), wire(None)]);

        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].summary, "Standup");
        assert_eq!(converted[0].start, at(4, 9));
        assert!(converted[0].color.is_some(), "a hex colour parses");
        assert!(
            converted[1].color.is_none(),
            "an unparseable colour is dropped"
        );
        assert!(converted[2].color.is_none());
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

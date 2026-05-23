use chrono::{DateTime, Datelike, Duration, Local, NaiveDate};

use glimpse_core::services::calendar_events::{
    CalendarEvent, State,
    model::{CalendarSource, MonthKey},
};

pub const DEFAULT_LABEL_FORMAT: &str = "{name} in {remaining}";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "{name} ({time}) — {duration}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextEvent {
    pub title: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub source: CalendarSource,
    pub location: Option<String>,
}

impl NextEvent {
    pub fn is_in_progress(&self, now: DateTime<Local>) -> bool {
        self.start <= now && now < self.end
    }
}

/// Find the soonest non-all-day event that is either currently in progress
/// or starts within `threshold` of `now`. Returns `None` otherwise.
pub fn next_event(
    state: &State,
    threshold: Duration,
    now: DateTime<Local>,
) -> Option<NextEvent> {
    let mut best: Option<NextEvent> = None;
    for snapshot in state.month_cache.values() {
        for day in snapshot.day_snapshots.values() {
            for event in &day.events {
                let Some(candidate) = parse_event(event) else {
                    continue;
                };
                if candidate.end <= now {
                    continue;
                }
                let is_visible =
                    candidate.is_in_progress(now) || candidate.start - now <= threshold;
                if !is_visible {
                    continue;
                }
                match &best {
                    Some(current) if current.start <= candidate.start => {}
                    _ => best = Some(candidate),
                }
            }
        }
    }
    best
}

pub fn label(format: &str, event: &NextEvent, now: DateTime<Local>) -> String {
    render(format, event, now)
}

pub fn tooltip(format: &str, event: &NextEvent, now: DateTime<Local>) -> String {
    render(format, event, now)
}

fn render(format: &str, event: &NextEvent, now: DateTime<Local>) -> String {
    format
        .replace("{name}", &event.title)
        .replace("{time}", &format_time(event.start, now))
        .replace("{duration}", &format_duration(event.end - event.start))
        .replace("{source}", &event.source.display_name)
        .replace("{remaining}", &format_remaining(event, now))
        .replace("{location}", event.location.as_deref().unwrap_or(""))
        .trim()
        .to_string()
}

fn parse_event(event: &CalendarEvent) -> Option<NextEvent> {
    if event.all_day {
        return None;
    }
    let start = DateTime::parse_from_rfc3339(&event.start)
        .ok()?
        .with_timezone(&Local);
    let end = DateTime::parse_from_rfc3339(&event.end)
        .ok()?
        .with_timezone(&Local);
    if event.title.trim().is_empty() {
        return None;
    }
    Some(NextEvent {
        title: event.title.clone(),
        start,
        end,
        source: event.source.clone(),
        location: event.location.clone(),
    })
}

fn format_time(start: DateTime<Local>, now: DateTime<Local>) -> String {
    let today = now.date_naive();
    let start_date = start.date_naive();
    if start_date == today {
        start.format("%H:%M").to_string()
    } else if start_date == today.succ_opt().unwrap_or(today) {
        format!("Tomorrow {}", start.format("%H:%M"))
    } else if days_between(today, start_date).abs() < 7 {
        start.format("%a %H:%M").to_string()
    } else {
        start.format("%Y-%m-%d %H:%M").to_string()
    }
}

fn format_duration(duration: Duration) -> String {
    let total_minutes = duration.num_minutes().max(0);
    if total_minutes == 0 {
        return "0m".to_string();
    }
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    match (hours, minutes) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h{m}m"),
    }
}

fn format_remaining(event: &NextEvent, now: DateTime<Local>) -> String {
    if event.is_in_progress(now) {
        let remaining = event.end - now;
        if remaining <= Duration::zero() {
            return "ending".to_string();
        }
        return format!("ends in {}", format_short_duration(remaining));
    }
    let until = event.start - now;
    if until <= Duration::zero() {
        "now".to_string()
    } else if until < Duration::minutes(1) {
        "in <1m".to_string()
    } else {
        format!("in {}", format_short_duration(until))
    }
}

fn format_short_duration(duration: Duration) -> String {
    let minutes = duration.num_minutes().max(0);
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    let rem = minutes % 60;
    if rem == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h{rem}m")
    }
}

fn days_between(a: NaiveDate, b: NaiveDate) -> i64 {
    (b - a).num_days()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use glimpse_core::services::calendar_events::model::{
        CalendarDate, CalendarDaySnapshot, CalendarMonthSnapshot, CalendarSource, MonthKey,
    };
    use std::collections::BTreeMap;

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(y, m, d, h, min, 0)
            .single()
            .expect("valid local time")
    }

    fn event(title: &str, start: DateTime<Local>, end: DateTime<Local>) -> CalendarEvent {
        CalendarEvent {
            event_id: format!("e-{title}"),
            title: title.into(),
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
            location: None,
            all_day: false,
            source: CalendarSource {
                source_id: "src".into(),
                display_name: "Personal".into(),
                color: None,
            },
        }
    }

    fn all_day_event(title: &str, start: DateTime<Local>, end: DateTime<Local>) -> CalendarEvent {
        CalendarEvent {
            all_day: true,
            ..event(title, start, end)
        }
    }

    fn state_with(events: Vec<CalendarEvent>) -> State {
        let mut day = CalendarDaySnapshot::default();
        day.date = CalendarDate {
            year: 2026,
            month: 5,
            day: 22,
        };
        day.events = events;
        let mut snapshot = CalendarMonthSnapshot::default();
        snapshot.key = MonthKey {
            year: 2026,
            month: 5,
        };
        let mut day_snapshots = BTreeMap::new();
        day_snapshots.insert(day.date, day);
        snapshot.day_snapshots = day_snapshots;
        let mut cache = BTreeMap::new();
        cache.insert(snapshot.key, snapshot);
        State {
            month_cache: cache,
            ..State::default()
        }
    }

    #[test]
    fn picks_event_within_threshold() {
        let now = local(2026, 5, 22, 10, 0);
        let state = state_with(vec![event(
            "Standup",
            local(2026, 5, 22, 10, 15),
            local(2026, 5, 22, 10, 30),
        )]);
        let picked = next_event(&state, Duration::minutes(30), now).expect("event picked");
        assert_eq!(picked.title, "Standup");
    }

    #[test]
    fn hides_event_outside_threshold() {
        let now = local(2026, 5, 22, 10, 0);
        let state = state_with(vec![event(
            "Lunch",
            local(2026, 5, 22, 13, 0),
            local(2026, 5, 22, 14, 0),
        )]);
        assert!(next_event(&state, Duration::minutes(30), now).is_none());
    }

    #[test]
    fn ignores_all_day_events() {
        let now = local(2026, 5, 22, 10, 0);
        let state = state_with(vec![all_day_event(
            "Holiday",
            local(2026, 5, 22, 0, 0),
            local(2026, 5, 23, 0, 0),
        )]);
        assert!(next_event(&state, Duration::minutes(30), now).is_none());
    }

    #[test]
    fn keeps_in_progress_event_visible() {
        let now = local(2026, 5, 22, 10, 20);
        let state = state_with(vec![event(
            "Standup",
            local(2026, 5, 22, 10, 0),
            local(2026, 5, 22, 10, 30),
        )]);
        let picked = next_event(&state, Duration::minutes(1), now).expect("in-progress picked");
        assert!(picked.is_in_progress(now));
    }

    #[test]
    fn drops_past_events() {
        let now = local(2026, 5, 22, 10, 30);
        let state = state_with(vec![event(
            "Standup",
            local(2026, 5, 22, 10, 0),
            local(2026, 5, 22, 10, 15),
        )]);
        assert!(next_event(&state, Duration::minutes(60), now).is_none());
    }

    #[test]
    fn renders_label_tokens() {
        let now = local(2026, 5, 22, 10, 0);
        let event = NextEvent {
            title: "Standup".into(),
            start: local(2026, 5, 22, 10, 15),
            end: local(2026, 5, 22, 10, 45),
            source: CalendarSource {
                source_id: "s".into(),
                display_name: "Work".into(),
                color: None,
            },
            location: Some("Room A".into()),
        };
        assert_eq!(label("{name} in {remaining}", &event, now), "Standup in 15m");
        assert_eq!(
            tooltip("{name} ({time}) — {duration}", &event, now),
            "Standup (10:15) — 30m"
        );
        assert_eq!(label("{source}: {location}", &event, now), "Work: Room A");
    }

    #[test]
    fn remaining_switches_to_ends_in_when_in_progress() {
        let now = local(2026, 5, 22, 10, 20);
        let event = NextEvent {
            title: "Standup".into(),
            start: local(2026, 5, 22, 10, 0),
            end: local(2026, 5, 22, 10, 30),
            source: CalendarSource::default(),
            location: None,
        };
        assert_eq!(format_remaining(&event, now), "ends in 10m");
    }

    #[test]
    fn duration_renders_short_form() {
        assert_eq!(format_duration(Duration::minutes(45)), "45m");
        assert_eq!(format_duration(Duration::minutes(60)), "1h");
        assert_eq!(format_duration(Duration::minutes(75)), "1h15m");
    }
}

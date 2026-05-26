use chrono::{DateTime, Datelike, Duration, Local, NaiveDate};
use gtk4::glib;

use glimpse_core::services::calendar_events::{
    CalendarEvent, State,
    model::{CalendarAttendee, CalendarPerson, CalendarSource, MonthKey},
};

pub const DEFAULT_LABEL_FORMAT: &str = "{name} {remaining}";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "{name} ({time}) — {duration}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextEvent {
    pub title: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub source: CalendarSource,
    pub location: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub meeting_url: Option<String>,
    pub status: Option<String>,
    pub organizer: Option<CalendarPerson>,
    pub attendees: Vec<CalendarAttendee>,
    pub transparency: Option<String>,
    pub last_modified: Option<String>,
    pub sequence: Option<u32>,
}

impl NextEvent {
    pub fn is_in_progress(&self, now: DateTime<Local>) -> bool {
        self.start <= now && now < self.end
    }

    pub fn status_label(&self) -> Option<String> {
        self.status.as_deref().and_then(status_label)
    }
}

/// Find the soonest non-all-day event that is either currently in progress
/// or starts within `threshold` of `now`. Returns `None` otherwise.
pub fn next_event(state: &State, threshold: Duration, now: DateTime<Local>) -> Option<NextEvent> {
    let current_month = MonthKey {
        year: now.year(),
        month: now.month(),
    };
    state
        .month_cache
        .iter()
        // Past months can't contain events whose `end > now`; skip the scan.
        .filter(|(key, _)| **key >= current_month)
        .flat_map(|(_, snapshot)| snapshot.day_snapshots.values())
        .flat_map(|day| &day.events)
        .filter_map(parse_event)
        .filter(|candidate| candidate.end > now)
        .filter(|candidate| candidate.is_in_progress(now) || candidate.start - now <= threshold)
        .min_by_key(|candidate| candidate.start)
}

pub fn label(format: &str, event: &NextEvent, now: DateTime<Local>) -> String {
    render(format, event, now)
}

pub fn label_markup(format: &str, event: &NextEvent, now: DateTime<Local>) -> String {
    render_markup(format, event, now)
}

pub fn tooltip(format: &str, event: &NextEvent, now: DateTime<Local>) -> String {
    render(format, event, now)
}

pub fn time_range(event: &NextEvent, now: DateTime<Local>) -> String {
    let start_label = format_time(event.start, now);
    if event.start.date_naive() == event.end.date_naive() {
        format!("{start_label}-{}", event.end.format("%H:%M"))
    } else {
        format!("{start_label}-{}", event.end.format("%Y-%m-%d %H:%M"))
    }
}

pub fn remaining_label(event: &NextEvent, now: DateTime<Local>) -> String {
    format_remaining(event, now)
}

pub fn duration_label(event: &NextEvent) -> String {
    format_duration(event.end - event.start)
}

pub fn organizer_label(event: &NextEvent) -> Option<String> {
    event.organizer.as_ref().and_then(person_label)
}

pub fn attendee_summary(event: &NextEvent) -> Option<String> {
    if event.attendees.is_empty() {
        return None;
    }

    let mut accepted = 0;
    let mut tentative = 0;
    let mut declined = 0;
    let mut pending = 0;
    for attendee in &event.attendees {
        match attendee.participation_status.as_deref() {
            Some("ACCEPTED") => accepted += 1,
            Some("TENTATIVE") => tentative += 1,
            Some("DECLINED") => declined += 1,
            Some("NEEDS-ACTION") | None => pending += 1,
            Some(_) => pending += 1,
        }
    }

    let mut parts = Vec::new();
    push_count(&mut parts, accepted, "accepted");
    push_count(&mut parts, tentative, "tentative");
    push_count(&mut parts, declined, "declined");
    push_count(&mut parts, pending, "pending");
    Some(parts.join(", "))
}

pub fn description_preview(event: &NextEvent) -> Option<String> {
    event.description.as_deref().and_then(|description| {
        let trimmed = description.trim();
        (!trimmed.is_empty()).then(|| {
            let mut preview = trimmed.lines().next().unwrap_or(trimmed).trim().to_string();
            if preview.chars().count() > 160 {
                preview = preview.chars().take(157).collect::<String>();
                preview.push_str("...");
            }
            preview
        })
    })
}

fn render(format: &str, event: &NextEvent, now: DateTime<Local>) -> String {
    render_tokens(
        format,
        RenderTokens {
            name: event.title.clone(),
            time: format_time(event.start, now),
            duration: format_duration(event.end - event.start),
            source: event.source.display_name.clone(),
            remaining: format_remaining(event, now),
            location: event.location.clone().unwrap_or_default(),
        },
    )
}

fn render_markup(format: &str, event: &NextEvent, now: DateTime<Local>) -> String {
    render_tokens(
        format,
        RenderTokens {
            name: markup_name(event),
            time: escape_markup(&format_time(event.start, now)),
            duration: escape_markup(&format_duration(event.end - event.start)),
            source: escape_markup(&event.source.display_name),
            remaining: escape_markup(&format_remaining(event, now)),
            location: escape_markup(event.location.as_deref().unwrap_or("")),
        },
    )
}

struct RenderTokens {
    name: String,
    time: String,
    duration: String,
    source: String,
    remaining: String,
    location: String,
}

fn render_tokens(format: &str, tokens: RenderTokens) -> String {
    let replacements = [
        ("{name}", tokens.name.as_str()),
        ("{time}", tokens.time.as_str()),
        ("{duration}", tokens.duration.as_str()),
        ("{source}", tokens.source.as_str()),
        ("{remaining}", tokens.remaining.as_str()),
        ("{location}", tokens.location.as_str()),
    ];

    replacements
        .into_iter()
        .fold(format.to_string(), |rendered, (token, value)| {
            rendered.replace(token, value)
        })
        .trim()
        .to_string()
}

fn markup_name(event: &NextEvent) -> String {
    let title = escape_markup(&event.title);
    match event
        .source
        .color
        .as_deref()
        .filter(|color| !color.is_empty())
    {
        Some(color) => format!(
            "<span foreground=\"{}\">●</span> {}",
            escape_markup(color),
            title
        ),
        None => title,
    }
}

fn escape_markup(value: &str) -> String {
    glib::markup_escape_text(value).to_string()
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
    if event.title.trim().is_empty() || end <= start {
        return None;
    }
    Some(NextEvent {
        title: event.title.clone(),
        start,
        end,
        source: event.source.clone(),
        location: event.location.clone(),
        description: event.description.clone(),
        url: event.url.clone(),
        meeting_url: event.meeting_url.clone(),
        status: event.status.clone(),
        organizer: event.organizer.clone(),
        attendees: event.attendees.clone(),
        transparency: event.transparency.clone(),
        last_modified: event.last_modified.clone(),
        sequence: event.sequence,
    })
}

fn status_label(value: &str) -> Option<String> {
    match value.trim() {
        "CONFIRMED" => Some("Confirmed".into()),
        "TENTATIVE" => Some("Tentative".into()),
        "CANCELLED" => Some("Cancelled".into()),
        "" => None,
        other => Some(title_case_token(other)),
    }
}

fn title_case_token(value: &str) -> String {
    value
        .split(['-', '_', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn person_label(person: &CalendarPerson) -> Option<String> {
    person
        .name
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| person.email.as_deref().filter(|value| !value.is_empty()))
        .map(ToOwned::to_owned)
}

fn push_count(parts: &mut Vec<String>, count: usize, label: &str) {
    if count > 0 {
        parts.push(format!("{count} {label}"));
    }
}

fn format_time(start: DateTime<Local>, now: DateTime<Local>) -> String {
    let today = now.date_naive();
    let start_date = start.date_naive();
    let diff = days_between(today, start_date);
    if diff == 0 {
        start.format("%H:%M").to_string()
    } else if diff == 1 {
        format!("Tomorrow {}", start.format("%H:%M"))
    } else if (2..7).contains(&diff) {
        // Within the next 6 days — show weekday so it doesn't collide with
        // the same weekday from last week.
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
        CalendarAttendee, CalendarDate, CalendarDaySnapshot, CalendarMonthSnapshot, CalendarPerson,
        CalendarSource, MonthKey,
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
            ..CalendarEvent::default()
        }
    }

    fn all_day_event(title: &str, start: DateTime<Local>, end: DateTime<Local>) -> CalendarEvent {
        CalendarEvent {
            all_day: true,
            ..event(title, start, end)
        }
    }

    fn state_with(events: Vec<CalendarEvent>) -> State {
        let date = CalendarDate {
            year: 2026,
            month: 5,
            day: 22,
        };
        let key = MonthKey {
            year: 2026,
            month: 5,
        };
        let day = CalendarDaySnapshot {
            date,
            events,
            ..CalendarDaySnapshot::default()
        };
        let snapshot = CalendarMonthSnapshot {
            key,
            day_snapshots: BTreeMap::from([(date, day)]),
            ..CalendarMonthSnapshot::default()
        };
        State {
            month_cache: BTreeMap::from([(key, snapshot)]),
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
            description: None,
            url: None,
            meeting_url: None,
            status: None,
            organizer: None,
            attendees: Vec::new(),
            transparency: None,
            last_modified: None,
            sequence: None,
        };
        assert_eq!(label("{name} {remaining}", &event, now), "Standup in 15m");
        assert_eq!(
            tooltip("{name} ({time}) — {duration}", &event, now),
            "Standup (10:15) — 30m"
        );
        assert_eq!(label("{source}: {location}", &event, now), "Work: Room A");
    }

    #[test]
    fn renders_label_markup_with_calendar_color_dot_next_to_name() {
        let now = local(2026, 5, 22, 10, 0);
        let event = NextEvent {
            title: "Standup".into(),
            start: local(2026, 5, 22, 10, 15),
            end: local(2026, 5, 22, 10, 45),
            source: CalendarSource {
                source_id: "s".into(),
                display_name: "Work".into(),
                color: Some("#4285f4".into()),
            },
            location: Some("Room A".into()),
            description: None,
            url: None,
            meeting_url: None,
            status: None,
            organizer: None,
            attendees: Vec::new(),
            transparency: None,
            last_modified: None,
            sequence: None,
        };

        assert_eq!(
            label_markup("{name} {remaining}", &event, now),
            "<span foreground=\"#4285f4\">●</span> Standup in 15m"
        );
    }

    #[test]
    fn label_markup_escapes_calendar_text() {
        let now = local(2026, 5, 22, 10, 0);
        let event = NextEvent {
            title: "Review <Q2> & plan".into(),
            start: local(2026, 5, 22, 10, 15),
            end: local(2026, 5, 22, 10, 45),
            source: CalendarSource {
                source_id: "s".into(),
                display_name: "R&D <Team>".into(),
                color: Some("#ea4335".into()),
            },
            location: Some("Room <A> & B".into()),
            description: None,
            url: None,
            meeting_url: None,
            status: None,
            organizer: None,
            attendees: Vec::new(),
            transparency: None,
            last_modified: None,
            sequence: None,
        };

        assert_eq!(
            label_markup("{source}: {name} @ {location}", &event, now),
            "R&amp;D &lt;Team&gt;: <span foreground=\"#ea4335\">●</span> Review &lt;Q2&gt; &amp; plan @ Room &lt;A&gt; &amp; B"
        );
    }

    #[test]
    fn label_markup_omits_dot_without_calendar_color() {
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
            location: None,
            description: None,
            url: None,
            meeting_url: None,
            status: None,
            organizer: None,
            attendees: Vec::new(),
            transparency: None,
            last_modified: None,
            sequence: None,
        };

        assert_eq!(
            label_markup("{name} {remaining}", &event, now),
            "Standup in 15m"
        );
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
            description: None,
            url: None,
            meeting_url: None,
            status: None,
            organizer: None,
            attendees: Vec::new(),
            transparency: None,
            last_modified: None,
            sequence: None,
        };
        assert_eq!(format_remaining(&event, now), "ends in 10m");
    }

    #[test]
    fn duration_renders_short_form() {
        assert_eq!(format_duration(Duration::minutes(45)), "45m");
        assert_eq!(format_duration(Duration::minutes(60)), "1h");
        assert_eq!(format_duration(Duration::minutes(75)), "1h15m");
    }

    #[test]
    fn next_event_preserves_rich_details() {
        let now = local(2026, 5, 22, 10, 0);
        let mut event = event(
            "Sprint Planning",
            local(2026, 5, 22, 10, 15),
            local(2026, 5, 22, 11, 0),
        );
        event.description = Some("Discuss Q2 scope".into());
        event.url = Some("https://calendar.example/event".into());
        event.meeting_url = Some("https://zoom.us/j/123".into());
        event.status = Some("TENTATIVE".into());
        event.organizer = Some(CalendarPerson {
            name: Some("Marta Nowak".into()),
            email: Some("marta@example.com".into()),
        });
        event.attendees = vec![CalendarAttendee {
            person: CalendarPerson {
                name: Some("Alex".into()),
                email: Some("alex@example.com".into()),
            },
            participation_status: Some("ACCEPTED".into()),
            role: Some("REQ-PARTICIPANT".into()),
            rsvp: Some(true),
        }];

        let state = state_with(vec![event]);
        let picked = next_event(&state, Duration::minutes(30), now).expect("event picked");

        assert_eq!(picked.description.as_deref(), Some("Discuss Q2 scope"));
        assert_eq!(picked.meeting_url.as_deref(), Some("https://zoom.us/j/123"));
        assert_eq!(picked.status_label().as_deref(), Some("Tentative"));
        assert_eq!(
            picked
                .organizer
                .as_ref()
                .and_then(|person| person.name.as_deref()),
            Some("Marta Nowak")
        );
        assert_eq!(picked.attendees.len(), 1);
    }

    #[test]
    fn attendee_summary_counts_participation_states() {
        let event = NextEvent {
            title: "Planning".into(),
            start: local(2026, 5, 22, 10, 0),
            end: local(2026, 5, 22, 11, 0),
            source: CalendarSource::default(),
            location: None,
            description: None,
            url: None,
            meeting_url: None,
            status: None,
            organizer: None,
            attendees: vec![
                attendee("ACCEPTED"),
                attendee("ACCEPTED"),
                attendee("TENTATIVE"),
                attendee("NEEDS-ACTION"),
            ],
            transparency: None,
            last_modified: None,
            sequence: None,
        };

        assert_eq!(
            attendee_summary(&event).as_deref(),
            Some("2 accepted, 1 tentative, 1 pending")
        );
    }

    #[test]
    fn event_exactly_at_threshold_is_visible() {
        let now = local(2026, 5, 22, 10, 0);
        let state = state_with(vec![event(
            "Edge",
            local(2026, 5, 22, 10, 30),
            local(2026, 5, 22, 11, 0),
        )]);
        let picked = next_event(&state, Duration::minutes(30), now).expect("boundary picked");
        assert_eq!(picked.title, "Edge");
    }

    #[test]
    fn rejects_event_with_end_before_start() {
        let now = local(2026, 5, 22, 10, 0);
        let state = state_with(vec![event(
            "Broken",
            local(2026, 5, 22, 12, 0),
            local(2026, 5, 22, 11, 0),
        )]);
        assert!(next_event(&state, Duration::hours(6), now).is_none());
    }

    #[test]
    fn format_time_uses_tomorrow_branch() {
        let now = local(2026, 5, 22, 10, 0);
        let start = local(2026, 5, 23, 9, 0);
        assert_eq!(format_time(start, now), "Tomorrow 09:00");
    }

    #[test]
    fn format_time_uses_weekday_within_week() {
        let now = local(2026, 5, 22, 10, 0); // Friday
        let start = local(2026, 5, 25, 14, 0); // Monday, 3 days out
        assert!(format_time(start, now).ends_with("14:00"));
        assert!(!format_time(start, now).contains("-"));
    }

    #[test]
    fn format_time_uses_absolute_after_one_week() {
        let now = local(2026, 5, 22, 10, 0);
        let start = local(2026, 5, 29, 14, 0); // exactly 7 days out
        assert_eq!(format_time(start, now), "2026-05-29 14:00");
    }

    fn attendee(status: &str) -> CalendarAttendee {
        CalendarAttendee {
            participation_status: Some(status.into()),
            ..CalendarAttendee::default()
        }
    }
}

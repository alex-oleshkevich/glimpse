use std::collections::HashMap;

use chrono::DateTime;

use super::model::CalendarEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventSourcePriority {
    Weak,
    Strong,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventCandidate {
    pub event: CalendarEvent,
    pub priority: EventSourcePriority,
}

impl EventCandidate {
    pub fn new(event: CalendarEvent, priority: EventSourcePriority) -> Self {
        Self { event, priority }
    }
}

pub fn dedupe_events(events: Vec<EventCandidate>) -> Vec<CalendarEvent> {
    let mut by_key: HashMap<DedupeKey, EventCandidate> = HashMap::new();

    for candidate in events {
        let key = DedupeKey::from_event(&candidate.event);
        match by_key.get(&key) {
            Some(existing) if existing.priority >= candidate.priority => {}
            _ => {
                by_key.insert(key, candidate);
            }
        }
    }

    let mut events: Vec<_> = by_key
        .into_values()
        .map(|candidate| candidate.event)
        .collect();
    events.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| a.end.cmp(&b.end))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.event_id.cmp(&b.event_id))
    });
    events
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DedupeKey {
    title: String,
    start: String,
    end: String,
    all_day: bool,
}

impl DedupeKey {
    fn from_event(event: &CalendarEvent) -> Self {
        Self {
            title: normalize_title(&event.title),
            start: normalize_timestamp(&event.start),
            end: normalize_timestamp(&event.end),
            all_day: event.all_day,
        }
    }
}

fn normalize_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn normalize_timestamp(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|ts| ts.to_utc().to_rfc3339())
        .unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::calendar_events::model::{CalendarEvent, CalendarSource};

    fn event(id: &str, title: &str, start: &str, end: &str, source_id: &str) -> CalendarEvent {
        CalendarEvent {
            event_id: id.into(),
            title: title.into(),
            start: start.into(),
            end: end.into(),
            location: None,
            all_day: false,
            source: CalendarSource {
                source_id: source_id.into(),
                display_name: source_id.into(),
                color: None,
            },
        }
    }

    #[test]
    fn dedupe_events_prefers_strong_source_for_same_name_and_time() {
        let events = vec![
            EventCandidate::new(
                event(
                    "gnome-1",
                    " Team Standup ",
                    "2026-05-26T09:00:00+02:00",
                    "2026-05-26T09:30:00+02:00",
                    "gnome",
                ),
                EventSourcePriority::Weak,
            ),
            EventCandidate::new(
                event(
                    "google-1",
                    "team  standup",
                    "2026-05-26T09:00:00+02:00",
                    "2026-05-26T09:30:00+02:00",
                    "google",
                ),
                EventSourcePriority::Strong,
            ),
        ];

        let deduped = dedupe_events(events);

        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].event_id, "google-1");
    }

    #[test]
    fn dedupe_events_keeps_same_name_at_different_times() {
        let events = vec![
            EventCandidate::new(
                event(
                    "work-1",
                    "Standup",
                    "2026-05-26T09:00:00+02:00",
                    "2026-05-26T09:30:00+02:00",
                    "work",
                ),
                EventSourcePriority::Strong,
            ),
            EventCandidate::new(
                event(
                    "client-1",
                    "standup",
                    "2026-05-26T10:00:00+02:00",
                    "2026-05-26T10:30:00+02:00",
                    "client",
                ),
                EventSourcePriority::Strong,
            ),
        ];

        let deduped = dedupe_events(events);

        assert_eq!(deduped.len(), 2);
    }
}

use super::*;

fn event() -> CalendarEvent {
    CalendarEvent {
        source: "work".into(),
        calendar: "Work".into(),
        summary: "Standup".into(),
        location: "Office".into(),
        description: "Daily meeting".into(),
        meeting_url: Some("https://meet.example.test/room".into()),
        event_url: Some("https://calendar.example.test/event".into()),
        organizer: Some("Alex".into()),
        guests: Some(GuestCounts {
            total: 3,
            accepted: 2,
        }),
        tentative: true,
        start: epoch(1_800_000_000_000_000).unwrap(),
        end: epoch(1_800_003_600_000_000).unwrap(),
        all_day: false,
        color: Some("purple".into()),
    }
}

#[test]
fn snapshot_and_range_preserve_the_calendar_types() {
    let events = CalendarEvents {
        events: vec![event()],
        truncated_from: Some(epoch(1_800_010_000_000_000).unwrap()),
        revision: 0,
    };
    assert_eq!(decode_range(encode_range(&events)).unwrap(), events);
    let state = decode_snapshot(encode_snapshot(&events, Some("offline"))).unwrap();
    assert_eq!(state.events, events.events);
    assert_eq!(state.truncated_from, events.truncated_from);
    assert!(state.available);
    assert_eq!(state.reason.as_deref(), Some("offline"));
    let empty = decode_snapshot(encode_snapshot(&CalendarEvents::default(), None)).unwrap();
    assert!(empty.available);
    assert!(empty.reason.is_none());
}

#[test]
fn both_decoders_cap_the_first_512_events_and_every_string() {
    let mut wire = encode_event(&event());
    wire.0 = "s".repeat(DETAIL + 20);
    wire.1 = "c".repeat(DETAIL + 20);
    wire.2 = "é".repeat(10_000);
    wire.3 = "l".repeat(DETAIL + 20);
    wire.4 = "d".repeat(DETAIL + 20);
    wire.5 = "m".repeat(MEETING_URL + 20);
    wire.6 = "e".repeat(MEETING_URL + 20);
    wire.7 = "o".repeat(ORGANIZER + 20);
    wire.13 = "p".repeat(DETAIL + 20);
    let mut wires = vec![wire; 600];
    wires[EVENT_CAP].10 = i64::MIN;
    let range = decode_range(((false, 0), wires.clone())).unwrap();
    let snapshot = decode_snapshot(("r".repeat(REASON + 20), (false, 0), wires, 0)).unwrap();
    assert_eq!(range.events, snapshot.events);
    assert_eq!(range.events.len(), EVENT_CAP);
    let reason = snapshot.reason.unwrap();
    assert_eq!(reason.chars().count(), REASON + 1);
    assert!(reason.ends_with('…'));
    let event = &range.events[0];
    for (text, cap) in [
        (&event.source, DETAIL),
        (&event.calendar, DETAIL),
        (&event.summary, SUMMARY),
        (&event.location, DETAIL),
        (&event.description, DETAIL),
        (event.meeting_url.as_ref().unwrap(), MEETING_URL),
        (event.event_url.as_ref().unwrap(), MEETING_URL),
        (event.organizer.as_ref().unwrap(), ORGANIZER),
        (event.color.as_ref().unwrap(), DETAIL),
    ] {
        assert_eq!(text.chars().count(), cap + 1);
        assert!(text.ends_with('…'));
    }
}

#[test]
fn invalid_event_and_truncation_timestamps_are_rejected_by_both_decoders() {
    for field in [10, 11] {
        let mut wire = encode_event(&event());
        if field == 10 {
            wire.10 = i64::MIN;
        } else {
            wire.11 = i64::MAX;
        }
        assert!(decode_range(((false, 0), vec![wire.clone()])).is_err());
        assert!(decode_snapshot((String::new(), (false, 0), vec![wire], 0)).is_err());
    }
    assert!(decode_range(((true, i64::MIN), Vec::new())).is_err());
    assert!(decode_snapshot((String::new(), (true, i64::MAX), Vec::new(), 0)).is_err());
}

#[test]
fn loss_retains_occurrences_and_the_truncation_marker() {
    let previous = CalendarProviderState {
        events: vec![event()],
        truncated_from: Some(epoch(1_800_010_000_000_000).unwrap()),
        available: true,
        reason: None,
        revision: 4,
    };
    let lost = CalendarProviderState::lost("gone".into(), Some(&previous));
    assert_eq!(lost.events, previous.events);
    assert_eq!(lost.truncated_from, previous.truncated_from);
    assert!(!lost.available);
    assert_eq!(lost.reason.as_deref(), Some("gone"));
    assert_eq!(lost.revision, previous.revision);
    let initial = CalendarProviderState::lost("absent".into(), None);
    assert!(initial.events.is_empty());
    assert_eq!(initial.truncated_from, None);
}

#[test]
fn named_errors_keep_the_calendar_error_variants() {
    assert!(matches!(
        CalendarProviderError::named("me.aresa.Glimpse.Calendar1.Error.InvalidRange", "bad".into()),
        Some(CalendarProviderError::InvalidRange(reason)) if reason == "bad"
    ));
    assert!(matches!(
        CalendarProviderError::named("me.aresa.Glimpse.Calendar1.Error.Unavailable", "gone".into()),
        Some(CalendarProviderError::Unavailable(reason)) if reason == "gone"
    ));
    assert!(CalendarProviderError::named("unknown", String::new()).is_none());
}

#[test]
fn an_event_that_runs_backwards_or_past_the_span_is_bounded() {
    let start = event().start;
    let reversed = ending(start - TimeDelta::days(1));
    let unbounded = ending(start + TimeDelta::days(SPAN + 3_000));
    let exact = ending(start + TimeDelta::days(SPAN));

    let reversed_events = decode_range(((false, 0), vec![reversed.clone()]))
        .unwrap()
        .events;
    assert_eq!(reversed_events[0].end, reversed_events[0].start);
    let unbounded_events = decode_range(((false, 0), vec![unbounded.clone()]))
        .unwrap()
        .events;
    assert_eq!(
        unbounded_events[0].end - unbounded_events[0].start,
        TimeDelta::days(SPAN)
    );
    let exact = decode_range(((false, 0), vec![exact])).unwrap().events;
    assert_eq!(exact[0].end - exact[0].start, TimeDelta::days(SPAN));

    let snapshot = decode_snapshot((String::new(), (false, 0), vec![unbounded], 0)).unwrap();
    assert_eq!(
        snapshot.events[0].end - snapshot.events[0].start,
        TimeDelta::days(SPAN)
    );
    let snapshot = decode_snapshot((String::new(), (false, 0), vec![reversed], 0)).unwrap();
    assert_eq!(snapshot.events[0].end, snapshot.events[0].start);
}

fn ending(end: chrono::DateTime<chrono::Utc>) -> CalendarEventWire {
    let mut wire = encode_event(&event());
    wire.11 = end.timestamp_micros();
    wire
}

#[test]
fn a_feed_revision_survives_the_snapshot_when_the_events_do_not_change() {
    let events = CalendarEvents {
        events: vec![event()],
        truncated_from: None,
        revision: 3,
    };
    let same = CalendarEvents {
        revision: 4,
        ..events.clone()
    };

    let left = encode_snapshot(&events, None);
    let right = encode_snapshot(&same, None);
    assert_eq!(left.3, 3);
    assert_eq!(right.3, 4);
    let state = decode_snapshot(right).unwrap();
    assert_eq!(state.revision, 4);
    assert_eq!(state.events, events.events);
}

#[test]
fn the_wire_signatures_are_frozen() {
    use zbus::zvariant::Type;
    assert_eq!(
        CalendarSnapshot::SIGNATURE.to_string(),
        "(s(bx)a(ssssssss(buu)bxxbs)t)"
    );
    assert_eq!(
        CalendarRange::SIGNATURE.to_string(),
        "((bx)a(ssssssss(buu)bxxbs))"
    );
}

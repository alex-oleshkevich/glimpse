use super::{
    Applet, Browsed, Clock, MINUTE, SECOND, accept, changed, failed, formatted, invalidate,
    painted, period, ticks_below_a_minute,
};
use crate::applets::agenda::Occasion;
use chrono::{DateTime, Local, TimeZone, Utc};
use glimpse_dbus::calendar::CalendarProvider;
use glimpse_services::{CalendarEvent, CalendarEvents};

#[test]
fn an_unavailable_provider_leaves_the_clock_rendering_with_no_events() {
    let provider = CalendarProvider::unavailable("no bus in tests");
    let applet = Clock::start(provider.handle());

    assert!(applet.events.is_empty());
    assert_eq!(applet.indicators().len(), 1, "the label still renders");
}

#[test]
fn a_format_that_shows_seconds_asks_for_a_faster_tick() {
    assert_eq!(period("%H:%M:%S", None), SECOND);
    assert_eq!(period("%T", None), SECOND);
    assert_eq!(period("%H:%M", None), MINUTE);
}

#[test]
fn a_padded_specifier_is_still_the_specifier_it_pads() {
    assert!(
        ticks_below_a_minute("%-S"),
        "a padding modifier hides the specifier from a plain substring search"
    );
    assert!(ticks_below_a_minute("%.3f"));
    assert!(!ticks_below_a_minute("%-d %b"));
}

#[test]
fn an_escaped_percent_does_not_name_a_specifier() {
    assert!(
        !ticks_below_a_minute("100%%S"),
        "`%%` is a literal percent, so the S after it is text"
    );
    assert!(ticks_below_a_minute("100%% %S"));
}

#[test]
fn the_tooltip_can_be_what_asks_for_the_faster_tick() {
    assert_eq!(period("%H:%M", Some("%H:%M:%S")), SECOND);
    assert_eq!(period("%H:%M", Some("%A")), MINUTE);
}

fn occasion(summary: &str) -> Occasion {
    let at = Local.with_ymd_and_hms(2026, 9, 4, 15, 30, 0).unwrap();
    Occasion {
        summary: summary.to_owned(),
        location: String::new(),
        description: String::new(),
        calendar: String::new(),
        meeting_url: None,
        event_url: None,
        organizer: None,
        guests: None,
        tentative: false,
        start: at,
        end: at,
        all_day: false,
        color: None,
    }
}

fn instant(day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, day, 0, 0, 0).unwrap()
}

#[test]
fn a_browsed_month_other_than_the_one_shown_keeps_the_snapshot() {
    let shown = Some((instant(1), instant(2)));
    let snapshot = vec![occasion("Near")];
    let elsewhere = Browsed {
        range: Some((instant(3), instant(4))),
        events: vec![occasion("Far")],
        truncated_from: None,
        asked: None,
        generation: 0,
    };

    let (events, _) = painted(&elsewhere, shown, &snapshot, None);

    assert_eq!(events[0].summary, "Near");

    let matching = Browsed {
        range: shown,
        events: vec![occasion("Far")],
        truncated_from: None,
        asked: None,
        generation: 0,
    };
    let (events, _) = painted(&matching, shown, &snapshot, None);
    assert_eq!(events[0].summary, "Far");
}

#[test]
fn an_availability_flip_with_identical_events_is_a_change() {
    let events = vec![occasion("Near")];
    let truncated = instant(9);

    assert!(changed(
        (&events, &Some(truncated), true),
        (&events, &Some(truncated), false),
    ));
    assert!(!changed(
        (&events, &Some(truncated), true),
        (&events, &Some(truncated), true),
    ));
}

#[test]
fn a_reset_then_reasked_range_keeps_the_browsed_events() {
    let range = (instant(3), instant(4));
    let browsed = Browsed {
        range: Some(range),
        events: vec![occasion("Far")],
        truncated_from: None,
        asked: Some(range),
        generation: 0,
    };
    let snapshot = vec![occasion("Near")];

    let (events, _) = painted(&browsed, Some(range), &snapshot, None);
    assert_eq!(events[0].summary, "Far");

    let (events, _) = painted(&browsed, None, &snapshot, None);
    assert_eq!(
        events[0].summary, "Near",
        "between the reset and the re-ask the snapshot would flash"
    );
}

fn wire(summary: &str, truncated_from: Option<DateTime<Utc>>) -> CalendarEvents {
    let at = instant(1);
    CalendarEvents {
        events: vec![CalendarEvent {
            source: String::new(),
            calendar: String::new(),
            summary: summary.to_owned(),
            location: String::new(),
            description: String::new(),
            meeting_url: None,
            event_url: None,
            organizer: None,
            guests: None,
            tentative: false,
            start: at,
            end: at,
            all_day: false,
            color: None,
        }],
        truncated_from,
        revision: 0,
    }
}

#[test]
fn accept_takes_a_reply_whose_range_was_asked() {
    let range = (instant(1), instant(2));
    let truncated = instant(9);
    let mut browsed = Browsed {
        asked: Some(range),
        ..Browsed::default()
    };

    assert!(accept(
        &mut browsed,
        range,
        0,
        wire("March", Some(truncated))
    ));

    assert_eq!(browsed.range, Some(range));
    assert_eq!(browsed.events[0].summary, "March");
    assert_eq!(browsed.truncated_from, Some(truncated));
    assert_eq!(browsed.asked, Some(range));
}

#[test]
fn accept_drops_a_reply_for_an_earlier_range_after_a_newer_one_was_asked() {
    let earlier = (instant(1), instant(2));
    let newer = (instant(3), instant(4));
    let mut browsed = Browsed {
        range: Some(newer),
        events: vec![occasion("April")],
        truncated_from: Some(instant(8)),
        asked: Some(newer),
        generation: 2,
    };
    let before = browsed.clone();

    assert!(!accept(&mut browsed, earlier, 1, wire("March", None)));
    assert_eq!(browsed, before);
}

#[test]
fn accept_drops_an_older_reply_for_the_same_range() {
    let range = (instant(1), instant(2));
    let mut browsed = Browsed {
        range: Some(range),
        events: vec![occasion("Fresh")],
        truncated_from: None,
        asked: Some(range),
        generation: 2,
    };
    let before = browsed.clone();

    assert!(!accept(&mut browsed, range, 1, wire("Stale", None)));
    assert_eq!(browsed, before);

    assert!(accept(&mut browsed, range, 2, wire("Fresh reply", None)));
    assert_eq!(browsed.events[0].summary, "Fresh reply");
}

#[test]
fn a_failed_query_can_retry_without_losing_the_browsed_month() {
    let range = (instant(1), instant(2));
    let mut browsed = Browsed {
        range: Some(range),
        events: vec![occasion("Kept")],
        truncated_from: Some(instant(2)),
        asked: Some(range),
        generation: 2,
    };
    let before = browsed.clone();
    assert!(!failed(&mut browsed, 1));
    assert_eq!(browsed, before);
    assert!(failed(&mut browsed, 2));
    assert!(browsed.asked.is_none());
    assert_eq!(
        painted(&browsed, Some(range), &[], None),
        (before.events.as_slice(), before.truncated_from)
    );
    assert!(!accept(&mut browsed, range, 2, wire("Stale", None)));
    browsed.asked = Some(range);
    browsed.generation = 3;
    assert!(accept(&mut browsed, range, 3, wire("Retried", None)));
    assert_eq!(browsed.events[0].summary, "Retried");
}

#[test]
fn a_feed_revision_with_the_same_events_asks_again_while_a_month_is_open() {
    let events = vec![occasion("Near")];
    let truncated = Some(instant(9));

    assert!(invalidate(
        (&events, &truncated, true, 1),
        (&events, &truncated, true, 2),
        true,
    ));
    assert!(!invalidate(
        (&events, &truncated, true, 2),
        (&events, &truncated, true, 2),
        true,
    ));
    assert!(!invalidate(
        (&events, &truncated, true, 1),
        (&events, &truncated, true, 2),
        false,
    ));
}

#[test]
fn a_format_that_cannot_render_yields_nothing_rather_than_panicking() {
    let now = Utc.with_ymd_and_hms(2026, 9, 4, 15, 30, 0).unwrap();

    assert_eq!(formatted(&now, "%H:%M").as_deref(), Some("15:30"));
    assert!(
        formatted(&now, "%Q").is_none(),
        "chrono's Display returns an error for an unknown specifier, and to_string() turns \
             that into a panic that would stop the applet for good"
    );
}

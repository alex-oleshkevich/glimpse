use super::*;

#[test]
fn a_recurring_entry_is_expanded_and_its_exceptions_removed() {
    let found = expanded("Weekly");

    let days: Vec<u32> = found
        .iter()
        .map(|occurrence| occurrence.start.day())
        .collect();
    assert_eq!(days, vec![7, 21], "COUNT=3 minus the EXDATE on the 14th");
}

#[test]
fn an_all_day_entry_is_marked_and_lasts_its_own_day() {
    let found = expanded("Holiday");

    assert_eq!(found.len(), 1);
    assert!(found[0].all_day);
    assert_eq!(
        found[0].end - found[0].start,
        TimeDelta::days(1) - TimeDelta::seconds(1)
    );
}

#[tokio::test]
async fn a_document_that_is_not_icalendar_is_a_failure_rather_than_an_empty_calendar() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let file = root.path().join("a.ics");
    tokio::fs::write(&file, "<html>not a calendar</html>")
        .await
        .expect("a written decoy");

    let read = read(
        None,
        CalendarSourceKind::Ical,
        file.to_string_lossy().into_owned(),
    )
    .await;

    assert!(
        read.is_err(),
        "a document is parsed where it is fetched, so a broken one is reported rather than \
             expanded into nothing every time the window moves"
    );
}

#[test]
fn widening_the_window_finds_an_entry_the_narrow_one_missed() {
    let calendar = parsed(&ics("far@example", "Far", now() + TimeDelta::days(200)));

    let near = payload(expanding(
        0,
        vec![loaded("far", calendar.clone())],
        window(),
    ));
    let wide = payload(expanding(
        0,
        vec![loaded("far", calendar)],
        Window::asked(now(), now() + TimeDelta::days(365)),
    ));

    assert!(
        near.events.is_empty(),
        "two hundred days out is past the window nobody asked to widen"
    );
    assert_eq!(wide.events.len(), 1);
    assert_eq!(wide.events[0].summary, "Far");
}

#[test]
fn a_range_at_the_far_end_of_time_is_clipped_rather_than_panicking() {
    let far: DateTime<Utc> = serde_json::from_str("\"+262142-06-01T00:00:00Z\"")
        .expect("an extended year is a value the wire accepts");

    let window = Window::asked(far, DateTime::<Utc>::MAX_UTC);

    assert_eq!(window.from, far);
    assert_eq!(
        window.to,
        DateTime::<Utc>::MAX_UTC,
        "the widest window it can have is everything left"
    );
}

#[test]
fn a_window_nobody_could_render_is_clipped_rather_than_refused() {
    let from = now();

    assert_eq!(
        Window::asked(from, from - TimeDelta::days(1)).to,
        from,
        "an end before its start is an empty window, not a negative one"
    );
    assert_eq!(
        Window::asked(from, from + TimeDelta::days(5000)).to,
        from + TimeDelta::days(SPAN)
    );
}

#[test]
fn a_past_window_stays_inside_the_range_it_was_given() {
    let today = Utc::now();
    let calendar = parsed(&ics("soon@example", "Soon", today + TimeDelta::days(10)));
    let past = Window::asked(today - TimeDelta::days(200), today - TimeDelta::days(170));

    let found = payload(expanding(0, vec![loaded("soon", calendar.clone())], past));
    assert!(
        found.events.is_empty(),
        "a range keeps only what it was asked for"
    );

    let near = payload(expanding(
        0,
        vec![loaded("soon", calendar)],
        Window::around(today),
    ));
    assert_eq!(near.events.len(), 1);
    assert_eq!(near.events[0].summary, "Soon");
}

#[test]
fn an_occurrence_on_the_upper_bound_belongs_to_the_next_window() {
    let february = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
    let calendar = parsed(&ics("edge@example", "Edge", february));
    let january = Window::asked(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(), february);
    let march = Window::asked(february, Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap());

    assert!(
        expand(&calendar, january).is_empty(),
        "the upper bound is exclusive, so midnight on the first is February's"
    );
    assert_eq!(expand(&calendar, march).len(), 1);
}

#[test]
fn an_entry_that_started_earlier_and_runs_into_the_window_is_kept() {
    let start = Utc.with_ymd_and_hms(2026, 10, 31, 18, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 11, 2, 12, 0, 0).unwrap();
    let document = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:trip@example\r\nDTSTAMP:{}\r\nDTSTART:{}\r\nDTEND:{}\r\n\
             SUMMARY:Trip\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        start.format("%Y%m%dT%H%M%SZ"),
        start.format("%Y%m%dT%H%M%SZ"),
        end.format("%Y%m%dT%H%M%SZ"),
    );
    let november = Window::asked(
        Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap(),
    );
    let december = Window::asked(
        Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap(),
    );

    let found = expand(&parsed(&document), november);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].summary, "Trip");
    assert!(expand(&parsed(&document), december).is_empty());
}

#[test]
fn a_later_generation_differs_when_the_near_window_does_not() {
    let first = payload(expanding(1, Vec::new(), window()));
    let second = payload(expanding(2, Vec::new(), window()));

    assert_eq!(first.events, second.events);
    assert_ne!(first.revision, second.revision);
}

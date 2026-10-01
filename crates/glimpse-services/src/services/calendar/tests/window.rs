use super::*;

/// The rule is the reason this service exists rather than a list of DTSTARTs: a weekly entry
/// shows once and never again without it, and an EXDATE nobody applies shows a meeting that
/// was cancelled.
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

/// The whole point of the range command: an entry outside the window is absent, and asking
/// for a wider one brings it back without fetching anything again.
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

/// `DateTime + TimeDelta` panics on overflow, so an extended year must be clipped before it
/// reaches the expansion code.
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

/// A client that asks for a thousand years would have the daemon expand every rule in every
/// calendar to answer, so the ask is clipped rather than refused.
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

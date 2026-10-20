use super::*;

#[test]
fn an_all_day_entry_ends_on_the_last_day_it_covers() {
    let start =
        DatePerhapsTime::Date(chrono::NaiveDate::from_ymd_opt(2026, 9, 5).expect("a real date"));
    let end =
        DatePerhapsTime::Date(chrono::NaiveDate::from_ymd_opt(2026, 9, 6).expect("a real date"));

    assert_eq!(
        length(&start, Some(&end), None, true),
        TimeDelta::days(1) - TimeDelta::seconds(1),
        "iCalendar writes an exclusive end; a surface asks which day the entry is on"
    );
    assert_eq!(
        length(&start, None, None, true),
        TimeDelta::days(1) - TimeDelta::seconds(1),
        "an all-day entry with no end still covers its own day"
    );
}

#[test]
fn a_timed_entry_keeps_its_own_length_and_survives_a_missing_end() {
    let start = DatePerhapsTime::DateTime(CalendarDateTime::Utc(now()));
    let end = DatePerhapsTime::DateTime(CalendarDateTime::Utc(now() + TimeDelta::minutes(30)));

    assert_eq!(
        length(&start, Some(&end), None, false),
        TimeDelta::minutes(30)
    );
    assert_eq!(length(&start, None, None, false), TimeDelta::zero());
    assert_eq!(
        length(&end, Some(&start), None, false),
        TimeDelta::zero(),
        "an end before its start is a broken entry, not one that lasts negative time"
    );
}

#[test]
fn an_entry_with_a_duration_and_no_end_keeps_its_length() {
    let start = DatePerhapsTime::DateTime(CalendarDateTime::Utc(now()));

    assert_eq!(
        length(&start, None, Some("PT1H30M"), false),
        TimeDelta::minutes(90)
    );
    assert_eq!(length(&start, None, Some("P1D"), false), TimeDelta::days(1));
    assert_eq!(
        length(&start, None, Some("P2W"), false),
        TimeDelta::weeks(2)
    );
    assert_eq!(
        length(&start, None, Some("  PT45M  "), false),
        TimeDelta::minutes(45),
        "a property value arrives with whatever whitespace the exporter folded in"
    );

    let end = DatePerhapsTime::DateTime(CalendarDateTime::Utc(now() + TimeDelta::minutes(30)));
    assert_eq!(
        length(&start, Some(&end), Some("PT9H"), false),
        TimeDelta::minutes(30),
        "RFC 5545 forbids both, and an entry that writes both is trusted on its end"
    );
    assert_eq!(
        length(&start, None, Some("not a duration"), false),
        TimeDelta::zero(),
        "an unparseable duration costs that entry its length, not the whole calendar"
    );
}

#[test]
fn an_entry_cannot_last_longer_than_the_widest_window() {
    let start = DatePerhapsTime::DateTime(CalendarDateTime::Utc(now()));
    let forever =
        DatePerhapsTime::DateTime(CalendarDateTime::Utc(now() + TimeDelta::days(SPAN * 1000)));

    assert_eq!(
        length(&start, None, Some("P9999Y"), false),
        TimeDelta::days(SPAN),
        "iso8601 parses a year, which RFC 5545 forbids, so the cap is what stops it"
    );
    assert_eq!(
        length(&start, Some(&forever), None, false),
        TimeDelta::days(SPAN)
    );
    assert_eq!(
        length(&start, Some(&forever), None, true),
        TimeDelta::days(SPAN),
        "an all-day entry is capped after its own day is added, not before"
    );
}

#[test]
fn a_single_entry_carries_its_summary_location_and_instants() {
    let found = expanded("Standup");

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].location, "Meeting room 2");
    assert_eq!(found[0].description, "");
    assert_eq!(
        found[0].start,
        Utc.with_ymd_and_hms(2026, 9, 4, 9, 0, 0).unwrap()
    );
    assert_eq!(
        found[0].end,
        Utc.with_ymd_and_hms(2026, 9, 4, 9, 30, 0).unwrap()
    );
    assert!(!found[0].all_day);
}

#[test]
fn a_moved_instance_replaces_the_one_the_rule_would_have_produced() {
    assert_eq!(
        recurring(
            "BEGIN:VEVENT\r\nUID:r@example\r\nDTSTAMP:20260901T000000Z\r\n\
                 RECURRENCE-ID:20260903T090000Z\r\nDTSTART:20260903T140000Z\r\n\
                 DTEND:20260903T143000Z\r\nSUMMARY:Daily moved\r\nEND:VEVENT\r\n"
        ),
        [
            "2026-09-02 09:00:00 UTC Daily",
            "2026-09-03 14:00:00 UTC Daily moved",
            "2026-09-04 09:00:00 UTC Daily",
            "2026-09-05 09:00:00 UTC Daily",
        ],
        "RECURRENCE-ID names the instant the rule produced, not the one the override sits at"
    );
}

#[test]
fn a_zoned_override_replaces_its_instance_the_way_a_utc_one_does() {
    let document = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:z@example\r\nDTSTAMP:20260901T000000Z\r\n\
             DTSTART;TZID=Europe/Warsaw:20260902T110000\r\n\
             DTEND;TZID=Europe/Warsaw:20260902T113000\r\n\
             RRULE:FREQ=DAILY;COUNT=4\r\nSUMMARY:Daily\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:z@example\r\nDTSTAMP:20260901T000000Z\r\n\
             RECURRENCE-ID;TZID=Europe/Warsaw:20260903T110000\r\n\
             DTSTART;TZID=Europe/Warsaw:20260903T160000\r\n\
             DTEND;TZID=Europe/Warsaw:20260903T163000\r\n\
             SUMMARY:Daily moved\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let mut shown: Vec<String> = expand(&parsed(document), window())
        .iter()
        .map(|occurrence| format!("{} {}", occurrence.start, occurrence.summary))
        .collect();
    shown.sort();

    assert_eq!(
        shown,
        [
            "2026-09-02 09:00:00 UTC Daily",
            "2026-09-03 14:00:00 UTC Daily moved",
            "2026-09-04 09:00:00 UTC Daily",
            "2026-09-05 09:00:00 UTC Daily",
        ],
        "a TZID RECURRENCE-ID names a local instant while the occurrence is published in UTC, \
             so matching only the UTC face leaves the replaced instance behind — and every Google \
             and Outlook feed is written this way"
    );
}

#[test]
fn a_cancelled_instance_leaves_a_gap_rather_than_a_duplicate() {
    assert_eq!(
        recurring(
            "BEGIN:VEVENT\r\nUID:r@example\r\nDTSTAMP:20260901T000000Z\r\n\
                 RECURRENCE-ID:20260903T090000Z\r\nDTSTART:20260903T090000Z\r\n\
                 DTEND:20260903T093000Z\r\nSTATUS:CANCELLED\r\nSUMMARY:Daily\r\nEND:VEVENT\r\n"
        ),
        [
            "2026-09-02 09:00:00 UTC Daily",
            "2026-09-04 09:00:00 UTC Daily",
            "2026-09-05 09:00:00 UTC Daily",
        ]
    );
}

#[test]
fn an_excluded_date_is_dropped_by_the_rule_itself() {
    let document = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:e@example\r\nDTSTAMP:20260901T000000Z\r\n\
             DTSTART:20260902T090000Z\r\nDTEND:20260902T093000Z\r\n\
             RRULE:FREQ=DAILY;COUNT=4\r\nEXDATE:20260903T090000Z\r\n\
             SUMMARY:Daily\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    assert_eq!(expand(&parsed(document), window()).len(), 3);
}

#[test]
fn a_zoned_hourly_series_keeps_the_instance_whose_utc_face_looks_like_the_override() {
    let document = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:h@example\r\nDTSTAMP:20260901T000000Z\r\n\
             DTSTART;TZID=America/New_York:20260903T160000\r\n\
             DTEND;TZID=America/New_York:20260903T161500\r\n\
             RRULE:FREQ=HOURLY;COUNT=5\r\nSUMMARY:Hourly\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:h@example\r\nDTSTAMP:20260901T000000Z\r\n\
             RECURRENCE-ID;TZID=America/New_York:20260903T200000\r\n\
             DTSTART;TZID=America/New_York:20260903T210000\r\n\
             DTEND;TZID=America/New_York:20260903T211500\r\n\
             SUMMARY:Hourly moved\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let found = expand(&parsed(document), window());

    assert_eq!(
        found.len(),
        5,
        "16:00 EDT is 20:00 UTC, the same wall time the override names in local terms — \
             matching whichever face happens to agree would drop it as well as the real 20:00"
    );
    assert_eq!(
        found.iter().filter(|o| o.summary == "Hourly moved").count(),
        1
    );
}

fn zoned(tzid: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
         BEGIN:VEVENT\r\nUID:zoned@example\r\nDTSTAMP:20260901T000000Z\r\n\
         DTSTART;TZID={tzid}:20260907T100000\r\nDTEND;TZID={tzid}:20260907T103000\r\n\
         SUMMARY:Zoned\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

async fn zoned_occurrences(tzid: &str) -> Vec<Occurrence> {
    let root = tempfile::tempdir().expect("a scratch directory");
    let file = root.path().join("zoned.ics");
    tokio::fs::write(&file, zoned(tzid))
        .await
        .expect("a written fixture");
    let fetch = read(
        None,
        CalendarSourceKind::Ical,
        file.to_string_lossy().into_owned(),
    )
    .await
    .expect("a readable document");
    expand(&fetch.calendars[0], window())
}

#[tokio::test]
async fn an_exchange_event_with_a_windows_timezone_name_is_expanded_in_that_zone() {
    let found = zoned_occurrences("W. Europe Standard Time").await;

    assert_eq!(found.len(), 1, "the Exchange event vanished");
    assert_eq!(
        found[0].start,
        Utc.with_ymd_and_hms(2026, 9, 7, 8, 0, 0)
            .single()
            .expect("one instant")
    );
}

#[tokio::test]
async fn an_event_in_an_unknown_zone_keeps_its_single_instance() {
    let found = zoned_occurrences("Mars/Base").await;

    assert_eq!(found.len(), 1, "an unknown zone must not drop the event");
    assert_eq!(found[0].summary, "Zoned");
}

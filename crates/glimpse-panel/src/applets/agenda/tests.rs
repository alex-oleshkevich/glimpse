use super::*;
use chrono::{TimeZone, Utc};

const CLOCK: &str = "%H:%M";

fn at(hour: u32, minute: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 9, 4, hour, minute, 0)
        .single()
        .expect("an unambiguous local time")
}

fn event(start: DateTime<Local>, end: DateTime<Local>) -> Occasion {
    Occasion {
        summary: "Standup".to_owned(),
        location: String::new(),
        description: String::new(),
        calendar: String::new(),
        meeting_url: None,
        event_url: None,
        organizer: None,
        guests: None,
        tentative: false,
        start,
        end,
        all_day: false,
        color: None,
    }
}

fn read(now: DateTime<Local>, event: &Occasion) -> String {
    when(now, now.date_naive(), event, CLOCK)
}

#[test]
fn an_event_converts_from_the_wire_and_survives_a_bad_color() {
    let wire = |color: Option<&str>| CalendarEvent {
        source: "work".to_owned(),
        calendar: "Work".to_owned(),
        summary: "Standup".to_owned(),
        location: "Room 2".to_owned(),
        description: "Bring slides".to_owned(),
        meeting_url: None,
        event_url: None,
        organizer: None,
        guests: None,
        tentative: false,
        start: at(9, 0).with_timezone(&Utc),
        end: at(10, 0).with_timezone(&Utc),
        all_day: false,
        color: color.map(str::to_owned),
    };

    let converted = occasions(&[wire(Some("#e0563f")), wire(Some("nonsense")), wire(None)]);

    assert_eq!(converted.len(), 3);
    assert_eq!(converted[0].summary, "Standup");
    assert_eq!(converted[0].calendar, "Work");
    assert_eq!(converted[0].location, "Room 2");
    assert_eq!(converted[0].description, "Bring slides");
    assert_eq!(converted[0].subtitle(), "Room 2");
    assert_eq!(converted[0].start, at(9, 0));
    assert!(converted[0].color.is_some(), "a hex colour parses");
    assert!(
        converted[1].color.is_none(),
        "an unparseable colour is dropped"
    );
    assert!(converted[2].color.is_none());
}

#[test]
fn an_all_day_event_says_so_and_a_multi_day_one_says_where_it_is() {
    let mut all_day = event(at(0, 0), at(23, 59));
    all_day.all_day = true;
    assert_eq!(read(at(12, 0), &all_day), "All day");

    let mut conference = event(at(0, 0), at(0, 0) + TimeDelta::days(2));
    conference.all_day = true;
    assert_eq!(
        when(at(12, 0), at(12, 0).date_naive(), &conference, CLOCK),
        "All day · day 1 of 3"
    );
    assert_eq!(
        when(
            at(12, 0),
            (at(12, 0) + TimeDelta::days(1)).date_naive(),
            &conference,
            CLOCK
        ),
        "All day · day 2 of 3"
    );
}

#[test]
fn a_timed_event_running_past_midnight_names_the_day_it_ends() {
    let overnight = event(at(22, 0), at(22, 0) + TimeDelta::hours(16));
    assert_eq!(read(at(23, 0), &overnight), "22:00 · until Sat 14:00");
}

#[test]
fn a_row_under_another_date_reads_as_a_time_and_a_length() {
    let meeting = event(at(9, 0), at(10, 30));
    let elsewhere = at(9, 0).date_naive() - TimeDelta::days(1);

    assert_eq!(
        when(at(12, 0), elsewhere, &meeting, CLOCK),
        "09:00 · 1 h 30 min"
    );
}

#[test]
fn an_event_that_ended_keeps_its_recency_for_an_hour() {
    let meeting = event(at(9, 0), at(10, 0));

    assert_eq!(read(at(10, 12), &meeting), "ended 12 min ago");
    assert_eq!(
        read(at(11, 30), &meeting),
        "09:00 · over",
        "an hour on, when it ended stops being the useful fact"
    );
}

#[test]
fn a_running_event_says_when_it_ends_and_counts_down_near_the_end() {
    let meeting = event(at(9, 0), at(10, 0));

    assert_eq!(read(at(9, 30), &meeting), "now · ends 10:00");
    assert_eq!(read(at(9, 52), &meeting), "now · ends in 8 min");
}

#[test]
fn an_event_about_to_start_counts_down_rather_than_saying_in_zero_minutes() {
    let meeting = event(at(9, 0), at(10, 0));

    assert_eq!(read(at(8, 48), &meeting), "in 12 min · 1 h");
    assert_eq!(
        read(at(9, 0) - TimeDelta::seconds(30), &meeting),
        "starting now · 1 h",
        "`in 0 min` is what this exists to avoid"
    );
    assert_eq!(read(at(7, 0), &meeting), "09:00 · 1 h");
}

#[test]
fn a_length_grows_through_minutes_hours_and_days() {
    assert_eq!(span(TimeDelta::minutes(45)), "45 min");
    assert_eq!(span(TimeDelta::hours(1)), "1 h");
    assert_eq!(span(TimeDelta::minutes(90)), "1 h 30 min");
    assert_eq!(span(TimeDelta::hours(48)), "2 d");
    assert_eq!(span(TimeDelta::hours(51)), "2 d 3 h");
    assert_eq!(
        span(TimeDelta::seconds(-10)),
        "0 min",
        "a negative span is not a huge one"
    );
}

#[test]
fn the_location_and_description_are_facts_of_the_card() {
    let mut meeting = event(at(9, 0), at(10, 0));
    meeting.location = "Room 2".to_owned();
    meeting.description = "Bring slides".to_owned();
    let labels = |meeting: &Occasion| -> Vec<String> {
        facts(meeting).into_iter().map(|fact| fact.label).collect()
    };
    assert_eq!(labels(&meeting), ["Location", "Description"]);

    meeting.location.clear();
    meeting.description.clear();
    assert!(labels(&meeting).is_empty());
}

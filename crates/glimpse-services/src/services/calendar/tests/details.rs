use super::*;

#[test]
fn location_and_description_stay_apart() {
    let event = one("LOCATION:Room 2\r\nDESCRIPTION:Bring slides\\nMore\r\n");

    assert_eq!(event.location, "Room 2");
    assert_eq!(event.description, "Bring slides");
}

#[test]
fn a_description_alone_does_not_become_a_location() {
    let event = one("DESCRIPTION:Bring slides\r\n");

    assert_eq!(event.location, "");
    assert_eq!(event.description, "Bring slides");
}

#[test]
fn a_google_conference_is_the_join_url() {
    let event = one("X-GOOGLE-CONFERENCE:https://meet.google.com/aaa-bbbb-ccc\r\n");

    assert_eq!(
        event.meeting_url.as_deref(),
        Some("https://meet.google.com/aaa-bbbb-ccc")
    );
}

#[test]
fn a_teams_url_is_the_join_url() {
    let event =
        one("X-MICROSOFT-SKYPETEAMSMEETINGURL:https://teams.microsoft.com/l/meetup-join/19\r\n");

    assert_eq!(
        event.meeting_url.as_deref(),
        Some("https://teams.microsoft.com/l/meetup-join/19")
    );
}

#[test]
fn a_plain_url_earns_a_join_row_only_when_it_names_a_conference_host() {
    let page = one("URL:https://calendar.example/event\r\n");
    assert_eq!(
        page.meeting_url, None,
        "an event page is not a meeting, and a Join row that opens one is a lie"
    );

    let conference = one("URL:https://zoom.us/j/123\r\n");
    assert_eq!(
        conference.meeting_url.as_deref(),
        Some("https://zoom.us/j/123")
    );

    let hostile = one("URL:javascript:alert(1)\r\n");
    assert_eq!(hostile.meeting_url, None);
}

#[test]
fn a_meet_in_the_location_or_description_is_still_a_join_url() {
    let at_location = one("LOCATION:https://meet.google.com/loc-only\r\n");
    assert_eq!(
        at_location.meeting_url.as_deref(),
        Some("https://meet.google.com/loc-only")
    );

    let in_body = one("DESCRIPTION:Dial in at https://zoom.us/j/123 then sit down\r\n");
    assert_eq!(
        in_body.meeting_url.as_deref(),
        Some("https://zoom.us/j/123")
    );

    let street = one("LOCATION:Antakalnio g. 18\r\n");
    assert_eq!(street.meeting_url, None);

    let spoof = one("LOCATION:https://evilzoom.us/j/1\r\n");
    assert_eq!(spoof.meeting_url, None);
}

#[test]
fn a_dedicated_conference_property_outranks_a_url_in_the_body() {
    let event = one(
        "X-GOOGLE-CONFERENCE:https://meet.google.com/aaa-bbbb-ccc\r\n\
             LOCATION:https://zoom.us/j/999\r\n",
    );

    assert_eq!(
        event.meeting_url.as_deref(),
        Some("https://meet.google.com/aaa-bbbb-ccc")
    );
}

#[test]
fn the_organizer_prefers_a_common_name_over_the_mailbox() {
    let named = one("ORGANIZER;CN=Marta Kazlauskiene:mailto:marta@example.com\r\n");
    assert_eq!(named.organizer.as_deref(), Some("Marta Kazlauskiene"));

    let mail = one("ORGANIZER:mailto:marta@example.com\r\n");
    assert_eq!(mail.organizer.as_deref(), Some("marta"));
}

#[test]
fn guests_are_a_count_and_only_when_there_are_two() {
    let pair = one(
        "ATTENDEE;CN=Alex;PARTSTAT=ACCEPTED:mailto:alex@example.com\r\n\
             ATTENDEE;CN=Marta;PARTSTAT=NEEDS-ACTION:mailto:marta@example.com\r\n",
    );
    assert_eq!(
        pair.guests,
        Some(GuestCounts {
            total: 2,
            accepted: 1
        })
    );

    let solo = one("ATTENDEE;CN=Alex;PARTSTAT=ACCEPTED:mailto:alex@example.com\r\n");
    assert_eq!(solo.guests, None);
}

#[test]
fn tentative_is_a_flag_and_cancelled_is_not_published() {
    let maybe = one("STATUS:TENTATIVE\r\n");
    assert!(maybe.tentative);

    let confirmed = one("STATUS:CONFIRMED\r\n");
    assert!(!confirmed.tentative);

    none("STATUS:CANCELLED\r\n");
}

#[test]
fn the_published_calendar_label_is_the_source_name() {
    let payload = payload(expanding(
        0,
        vec![Loaded {
            id: "work".to_owned(),
            name: Some("Work".to_owned()),
            color: None,
            calendars: vec![parsed(DOCUMENT)],
        }],
        window(),
    ));

    let standup = payload
        .events
        .iter()
        .find(|event| event.summary == "Standup")
        .expect("standup");
    assert_eq!(standup.source, "work");
    assert_eq!(standup.calendar, "Work");
    assert_eq!(standup.location, "Meeting room 2");
}

#[test]
fn an_unset_name_falls_back_to_the_source_id() {
    let published = payload(expanding(
        0,
        vec![loaded("work", parsed(DOCUMENT))],
        window(),
    ));
    assert_eq!(
        published
            .events
            .iter()
            .find(|event| event.summary == "Standup")
            .expect("standup")
            .calendar,
        "work",
        "`name` is documented as falling back to `id`, and an empty string hides the row"
    );
}

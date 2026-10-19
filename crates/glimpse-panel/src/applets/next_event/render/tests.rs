use super::*;
use chrono::TimeZone;
use glimpse_config::TWENTY_FOUR;
use glimpse_services::GuestCounts;

const HORIZON: TimeDelta = TimeDelta::hours(1);
const REACH: TimeDelta = TimeDelta::hours(12);

fn at(day: u32, hour: u32, minute: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 9, day, hour, minute, 0)
        .single()
        .expect("an unambiguous local time")
}

fn event(summary: &str, start: DateTime<Local>, end: DateTime<Local>) -> Occasion {
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
        start,
        end,
        all_day: false,
        color: None,
    }
}

fn all_day(summary: &str, day: u32) -> Occasion {
    spanning(summary, day, day)
}

fn spanning(summary: &str, first: u32, last: u32) -> Occasion {
    let mut event = event(summary, at(first, 0, 0), at(last, 23, 59));
    event.all_day = true;
    event
}

#[test]
fn a_multi_day_entry_counts_the_day_it_is_actually_on() {
    let trip = spanning("Vilnius trip", 8, 10);

    assert_eq!(
        heading(at(9, 12, 0), &trip, TWENTY_FOUR).1,
        "All day · day 2 of 3",
        "on the middle day it is day two, not day one"
    );
    assert_eq!(
        heading(at(8, 12, 0), &trip, TWENTY_FOUR).1,
        "All day · day 1 of 3"
    );
    assert_eq!(
        heading(at(10, 12, 0), &trip, TWENTY_FOUR).1,
        "All day · day 3 of 3"
    );
}

#[test]
fn the_bar_counts_down_only_inside_its_own_window() {
    let meeting = event("Design review", at(4, 14, 0), at(4, 15, 0));
    let counting = TimeDelta::hours(1);

    assert_eq!(
        label(at(4, 13, 48), &meeting, counting),
        "Design review in 12 min"
    );
    assert_eq!(
        label(at(4, 9, 0), &meeting, counting),
        "Design review",
        "five hours out is past the window, so the title stands alone"
    );
    assert_eq!(
        label(at(4, 14, 35), &meeting, counting),
        "Design review ends in 25 min",
        "a running event counts down to its end"
    );
    assert_eq!(
        label(at(4, 13, 48), &meeting, TimeDelta::zero()),
        "Design review",
        "a zero window never counts"
    );
    assert_eq!(
        label(at(4, 9, 0), &all_day("Conference", 4), counting),
        "Conference",
        "an all-day entry has no minute to count, whatever the window"
    );
}

#[test]
fn the_countdown_survives_a_title_long_enough_to_be_truncated() {
    let long = event(
        "Ünicöde tîtle that runs on well past the bar",
        at(4, 14, 0),
        at(4, 15, 0),
    );

    let shown = label(at(4, 13, 48), &long, TimeDelta::hours(1));
    assert!(shown.ends_with("in 12 min"), "{shown}");
    assert!(shown.contains(ELLIPSIS), "{shown}");
}

#[test]
fn the_tooltip_and_the_hero_agree_on_which_day_a_trip_is_on() {
    let trip = spanning("Vilnius trip", 8, 10);
    let now = at(9, 12, 0);

    assert_eq!(reading(now, &trip, TWENTY_FOUR), "All day · day 2 of 3");
    assert!(
        heading(now, &trip, TWENTY_FOUR).1.contains("day 2 of 3"),
        "the hero says the same day the tooltip does"
    );
}

#[test]
fn a_multi_day_entry_still_ahead_reads_as_its_first_day() {
    let trip = spanning("Vilnius trip", 8, 10);
    let reach = TimeDelta::days(7);
    let rows = upcoming(at(4, 9, 0), &[trip], None, reach, 5, TWENTY_FOUR);

    assert_eq!(rows[0].when, "Tue All day · day 1 of 3");
}

#[test]
fn a_single_day_entry_is_not_given_a_counter_it_does_not_need() {
    let holiday = all_day("Knabenschiessen", 12);

    assert_eq!(
        heading(at(12, 9, 0), &holiday, TWENTY_FOUR).1,
        "All day",
        "one day of one day is a number that says nothing"
    );
}

#[test]
fn an_all_day_entry_never_outranks_a_meeting_that_is_actually_next() {
    let events = vec![
        all_day("Conference", 4),
        event("Standup", at(4, 9, 30), at(4, 9, 45)),
    ];

    let chosen = next(at(4, 9, 0), &events, HORIZON, true).expect("something is next");
    assert_eq!(events[chosen].summary, "Standup");
}

#[test]
fn an_all_day_entry_takes_the_bar_only_when_it_was_allowed_to() {
    let events = vec![all_day("Conference", 4)];

    let chosen = next(at(4, 9, 0), &events, HORIZON, true).expect("something is next");
    assert_eq!(events[chosen].summary, "Conference");
    assert!(
        next(at(4, 9, 0), &events, HORIZON, false).is_none(),
        "a week of leave must not pin the applet open for the whole week"
    );
}

#[test]
fn a_running_meeting_outranks_one_that_has_not_started() {
    let events = vec![
        event("Later", at(4, 10, 15), at(4, 11, 0)),
        event("Running", at(4, 9, 0), at(4, 10, 0)),
    ];

    let chosen = next(at(4, 9, 30), &events, HORIZON, true).expect("something is next");
    assert_eq!(events[chosen].summary, "Running");
}

#[test]
fn an_event_that_has_ended_is_never_next() {
    let events = vec![event("Over", at(4, 8, 0), at(4, 9, 0))];
    assert!(next(at(4, 9, 30), &events, HORIZON, true).is_none());
}

#[test]
fn an_event_beyond_the_window_stays_off_the_bar_until_the_window_reaches_it() {
    let events = vec![event("Review", at(6, 14, 0), at(6, 15, 0))];

    assert!(next(at(4, 9, 0), &events, HORIZON, true).is_none());
    assert!(next(at(6, 13, 30), &events, HORIZON, true).is_some());
    assert!(
        next(at(6, 12, 59), &events, HORIZON, true).is_none(),
        "an hour and a minute out is still outside an hour"
    );
}

#[test]
fn a_running_event_stays_on_the_bar_however_far_back_it_started() {
    let marathon = vec![event("Offsite", at(1, 9, 0), at(6, 17, 0))];

    assert!(next(at(4, 12, 0), &marathon, TimeDelta::zero(), true).is_some());
}

#[test]
fn a_window_nobody_could_mean_is_clamped_rather_than_overflowing() {
    let events = vec![event("Review", at(6, 14, 0), at(6, 15, 0))];

    assert!(next(at(4, 9, 0), &events, window(u64::MAX), true).is_some());
    assert!(
        next(at(4, 9, 0), &events, window(0), true).is_none(),
        "a zero window shows only what is already running"
    );
}

#[test]
fn a_heading_names_the_day_only_when_it_is_not_today() {
    let now = at(4, 9, 0);
    let today = event("Standup", at(4, 14, 0), at(4, 15, 0));
    let other = event("Retro", at(5, 14, 0), at(5, 15, 0));

    assert_eq!(heading(now, &today, TWENTY_FOUR).1, "14:00–15:00");
    assert_eq!(heading(now, &other, TWENTY_FOUR).1, "Sat 14:00–15:00");
}

#[test]
fn a_heading_that_runs_past_midnight_names_the_day_it_ends() {
    let now = at(4, 9, 0);
    let overnight = event("Deploy", at(4, 22, 0), at(5, 2, 0));

    assert_eq!(heading(now, &overnight, TWENTY_FOUR).1, "22:00–Sat 02:00");
}

#[test]
fn a_location_joins_the_heading_and_an_absent_one_leaves_no_separator() {
    let now = at(4, 9, 0);
    let mut meeting = event("Standup", at(4, 14, 0), at(4, 15, 0));
    meeting.location = "Room 2".to_owned();

    assert_eq!(
        heading(now, &meeting, TWENTY_FOUR).1,
        "14:00–15:00 · Room 2"
    );
}

#[test]
fn a_countdown_grows_through_minutes_hours_and_days() {
    let meeting = event("Standup", at(4, 14, 0), at(4, 15, 0));
    let readout = |now| {
        countdown(now, &meeting).map(|c| (c.readout().0.to_owned(), c.readout().1.to_owned()))
    };

    assert_eq!(
        readout(at(4, 13, 48)),
        Some(("12".to_owned(), "min".to_owned()))
    );
    assert_eq!(readout(at(4, 9, 0)), Some(("5".to_owned(), "h".to_owned())));
    assert_eq!(readout(at(1, 9, 0)), Some(("3".to_owned(), "d".to_owned())));
    assert_eq!(
        readout(at(4, 14, 30)),
        Some(("30".to_owned(), "min left".to_owned())),
        "a running event counts down to its end rather than up from its start"
    );
    assert!(
        countdown(at(4, 9, 0), &all_day("Conference", 4)).is_none(),
        "an all-day entry has no minute to count"
    );
}

#[test]
fn the_list_below_the_hero_leaves_out_the_event_the_hero_is_showing() {
    let events = vec![
        event("Standup", at(4, 9, 30), at(4, 9, 45)),
        event("Review", at(4, 14, 0), at(4, 15, 0)),
    ];

    let rows = upcoming(at(4, 9, 0), &events, Some(0), REACH, 5, TWENTY_FOUR);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].summary, "Review");
}

#[test]
fn a_row_on_another_day_says_which_day() {
    let events = vec![event("Retro", at(5, 14, 0), at(5, 15, 0))];
    let reach = TimeDelta::days(2);
    let rows = upcoming(at(4, 9, 0), &events, None, reach, 5, TWENTY_FOUR);

    assert_eq!(rows[0].when, "Sat 14:00 · 1 h");
}

#[test]
fn the_list_reaches_past_the_window_the_bar_is_held_to() {
    let events = vec![event("Review", at(4, 18, 0), at(4, 19, 0))];
    let now = at(4, 9, 0);

    assert!(
        next(now, &events, HORIZON, true).is_none(),
        "nine hours out is well past the hour the bar is held to"
    );
    assert_eq!(
        upcoming(now, &events, None, REACH, 5, TWENTY_FOUR).len(),
        1,
        "but it is inside the twelve the list reaches"
    );
}

#[test]
fn the_horizon_crosses_midnight_rather_than_stopping_at_it() {
    let standup = vec![event("Standup", at(5, 8, 0), at(5, 8, 15))];
    let evening = at(4, 21, 0);

    assert_eq!(
        upcoming(evening, &standup, None, REACH, 5, TWENTY_FOUR).len(),
        1,
        "eleven hours away is tomorrow, and still inside a twelve-hour reach"
    );
    assert!(
        upcoming(evening, &standup, None, TimeDelta::hours(6), 5, TWENTY_FOUR).is_empty(),
        "a narrower reach stops short of it"
    );
}

#[test]
fn the_list_is_capped_so_a_crowded_calendar_cannot_grow_the_popover() {
    let events: Vec<Occasion> = (0..40)
        .map(|index| {
            let start = at(4, 10, 0) + TimeDelta::hours(index);
            event("Busy", start, start + TimeDelta::minutes(30))
        })
        .collect();
    let reach = TimeDelta::days(7);

    assert_eq!(
        upcoming(at(4, 9, 0), &events, None, reach, 5, TWENTY_FOUR).len(),
        5
    );
    assert_eq!(
        upcoming(at(4, 9, 0), &events, None, reach, usize::MAX, TWENTY_FOUR).len(),
        MOST_ROWS,
        "a configured length nobody could mean is still a popover that fits on screen"
    );
}

#[test]
fn a_tooltip_resolves_its_own_tokens_and_keeps_the_text_around_them() {
    let mut meeting = event("Standup", at(4, 14, 0), at(4, 15, 0));
    meeting.location = "Room 2".to_owned();

    assert_eq!(
        tooltip("{summary} in {detail} — {when}", &meeting, "in 12 min", &[]),
        "Standup in Room 2 — in 12 min"
    );
    assert_eq!(
        tooltip("{nonesuch}", &meeting, "in 12 min", &[]),
        "{nonesuch}",
        "an unknown token is left alone rather than silently emptied"
    );
    assert_eq!(tooltip("{unclosed", &meeting, "", &[]), "{unclosed");
}

#[test]
fn a_token_inside_an_events_own_text_is_not_substituted() {
    let hostile = event("{when}", at(4, 14, 0), at(4, 15, 0));

    assert_eq!(tooltip("{summary}", &hostile, "in 12 min", &[]), "{when}");
}

#[test]
fn a_capped_bar_label_says_that_it_was_cut() {
    let mut long = event(
        "Ünicöde tîtle that runs on well past the bar",
        at(4, 14, 0),
        at(4, 15, 0),
    );
    long.summary.push('é');

    let cut = label(at(4, 9, 0), &long, TimeDelta::zero());
    assert_eq!(cut.chars().count(), TITLE + 1);
    assert!(cut.ends_with(ELLIPSIS), "{cut}");

    let short = event("Standup", at(4, 14, 0), at(4, 15, 0));
    assert_eq!(
        label(at(4, 9, 0), &short, TimeDelta::zero()),
        "Standup",
        "a title that fits is not marked as cut"
    );
}

#[test]
fn join_is_labelled_from_the_host_and_absent_when_there_is_no_url() {
    let mut meeting = event("Standup", at(4, 14, 0), at(4, 15, 0));
    assert!(agenda::join(&meeting).is_none());

    meeting.meeting_url = Some("https://meet.google.com/aaa-bbbb-ccc".to_owned());
    let shown = agenda::join(&meeting).expect("a meet url");
    assert_eq!(shown.title, "Join Google Meet");
    assert_eq!(shown.address, "meet.google.com/aaa-bbbb-ccc");
    assert_eq!(shown.url, "https://meet.google.com/aaa-bbbb-ccc");

    meeting.meeting_url = Some("https://zoom.us/j/123".to_owned());
    assert_eq!(agenda::join(&meeting).expect("zoom").title, "Join Zoom");

    meeting.meeting_url = Some("https://teams.microsoft.com/l/meetup-join/19".to_owned());
    assert_eq!(
        agenda::join(&meeting).expect("teams").title,
        "Join Microsoft Teams"
    );

    meeting.meeting_url = Some("https://calendar.example/event".to_owned());
    assert_eq!(agenda::join(&meeting).expect("other").title, "Join meeting");

    meeting.meeting_url = Some("https://user:pass@zoom.us/j/123".to_owned());
    let shown = agenda::join(&meeting).expect("userinfo");
    assert_eq!(shown.title, "Join Zoom");
    assert_eq!(shown.address, "zoom.us/j/123");
    assert!(
        !shown.address.contains("pass"),
        "a password in the URL is not shown"
    );

    meeting.meeting_url = Some("https://acme.webex.com/meet/sam".to_owned());
    assert_eq!(
        agenda::join(&meeting).expect("webex").title,
        "Join Webex",
        "the service accepts a webex link as a meeting, so the row has to name it — the two \
             lists drifting is what sharing one classifier prevents"
    );

    meeting.meeting_url = Some("https://acme.zoom.us/j/123".to_owned());
    assert_eq!(
        agenda::join(&meeting).expect("subdomain").title,
        "Join Zoom"
    );

    meeting.meeting_url = Some("https://notzoom.us/j/123".to_owned());
    assert_eq!(
        agenda::join(&meeting).expect("lookalike").title,
        "Join meeting",
        "a suffix match must not treat notzoom.us as zoom.us"
    );
}

#[test]
fn open_event_reads_the_events_own_url_and_is_absent_without_one() {
    let mut meeting = event("Standup", at(4, 14, 0), at(4, 15, 0));
    assert!(agenda::open_event(&meeting).is_none());

    meeting.event_url = Some("https://calendar.example/event/abc123".to_owned());
    let shown = agenda::open_event(&meeting).expect("an event url");
    assert_eq!(shown.title, "Open event");
    assert_eq!(shown.url, "https://calendar.example/event/abc123");
    assert_eq!(shown.address, "https://calendar.example/event/abc123");
}

#[test]
fn facts_hold_only_what_the_heading_does_not_already_say() {
    let mut meeting = event("Standup", at(4, 14, 0), at(4, 15, 0));
    meeting.location = "Room 2".to_owned();
    assert!(
        facts(&meeting, &[]).is_empty(),
        "location and length are in the heading, and a confirmed event is the ordinary case"
    );

    meeting.calendar = "Work".to_owned();
    meeting.organizer = Some("Marta".to_owned());
    meeting.guests = Some(GuestCounts {
        total: 2,
        accepted: 1,
    });
    meeting.tentative = true;
    meeting.description = "Bring the roadmap slides".to_owned();

    let shown: Vec<(String, String)> = facts(&meeting, &[])
        .into_iter()
        .map(|fact| (fact.label, fact.value))
        .collect();
    assert_eq!(
        shown,
        vec![
            ("Calendar".to_owned(), "Work".to_owned()),
            ("Organizer".to_owned(), "Marta".to_owned()),
            ("Guests".to_owned(), "2 · 1 accepted".to_owned()),
            ("Status".to_owned(), "Tentative".to_owned()),
            (
                "Description".to_owned(),
                "Bring the roadmap slides".to_owned()
            ),
        ]
    );

    meeting.guests = Some(GuestCounts {
        total: 1,
        accepted: 1,
    });
    assert!(
        !facts(&meeting, &[])
            .iter()
            .any(|fact| fact.label == "Guests"),
        "a solo attendee is not a guest list"
    );

    meeting.meeting_url = Some("https://meet.google.com/aaa-bbbb-ccc".to_owned());
    meeting.event_url = Some("https://calendar.example/event/abc".to_owned());
    let links: Vec<(String, String)> = facts(&meeting, &[])
        .into_iter()
        .filter(|fact| fact.label == "Meeting" || fact.label == "Link")
        .map(|fact| (fact.label, fact.value))
        .collect();
    assert_eq!(
        links,
        vec![
            (
                "Meeting".to_owned(),
                "meet.google.com/aaa-bbbb-ccc".to_owned()
            ),
            (
                "Link".to_owned(),
                "https://calendar.example/event/abc".to_owned()
            ),
        ],
        "the addresses the action rows no longer carry stay in the details"
    );

    let clash = facts(&meeting, &["Retro".to_owned()]);
    let last = clash.last().expect("a conflict fact");
    assert_eq!(last.label, "Conflicts");
    assert!(last.warning, "a conflict is a problem, so it reads amber");
    assert!(clash[..clash.len() - 1].iter().all(|fact| !fact.warning));
}

#[test]
fn an_upcoming_row_carries_the_same_card_as_the_calendar() {
    let mut meeting = event("Standup", at(4, 14, 0), at(4, 15, 0));
    let now = at(4, 13, 0);
    let plain = upcoming(
        now,
        std::slice::from_ref(&meeting),
        None,
        TimeDelta::hours(4),
        5,
        "%H:%M",
    );
    assert!(plain[0].links.is_empty() && plain[0].facts.is_empty());

    meeting.calendar = "Work".to_owned();
    meeting.meeting_url = Some("https://meet.google.com/abc-defg-hij".to_owned());
    let carded = upcoming(
        now,
        std::slice::from_ref(&meeting),
        None,
        TimeDelta::hours(4),
        5,
        "%H:%M",
    );
    assert_eq!(carded[0].links, agenda::links(&meeting));
    assert_eq!(carded[0].facts, agenda::facts(&meeting));
    assert_eq!(carded[0].links[0].title, "Join Google Meet");
}

#[test]
fn an_overlapping_event_is_named_beside_the_one_the_bar_chose() {
    let events = vec![
        event("Design review", at(4, 14, 0), at(4, 15, 0)),
        event("1:1 with Sam", at(4, 14, 0), at(4, 14, 30)),
        event("Retro", at(4, 16, 0), at(4, 17, 0)),
    ];
    let now = at(4, 13, 0);
    let chosen = next(now, &events, TimeDelta::hours(2), false);

    assert_eq!(
        events[chosen.expect("one is chosen")].summary,
        "1:1 with Sam",
        "the same start is broken by the earlier end, so the shorter one is the bar's"
    );
    assert_eq!(
        conflicts(&events, chosen),
        ["Design review"],
        "the later-ending overlap is named; Retro does not touch it"
    );
}

#[test]
fn an_event_with_nothing_over_it_reports_no_conflict_and_adds_no_fact() {
    let events = vec![
        event("Standup", at(4, 14, 0), at(4, 15, 0)),
        event("Retro", at(4, 15, 0), at(4, 16, 0)),
    ];
    let now = at(4, 13, 0);
    let chosen = next(now, &events, TimeDelta::hours(2), false);

    assert!(
        conflicts(&events, chosen).is_empty(),
        "a back-to-back pair touches at one instant and does not overlap"
    );
    assert!(
        !facts(&events[chosen.expect("one is chosen")], &[])
            .iter()
            .any(|fact| fact.label == "Conflicts")
    );
}

#[test]
fn a_clash_reaches_the_tooltip_with_and_without_a_format() {
    let meeting = event("Design review", at(4, 14, 0), at(4, 15, 0));
    let clash = ["1:1 with Sam (14:00–14:30)".to_owned()];

    assert_eq!(
        tooltip("{summary} — {conflicts}", &meeting, "", &clash),
        "Design review — Clashes with 1:1 with Sam (14:00–14:30)"
    );
    assert_eq!(
        tooltip("{summary}{conflicts}", &meeting, "", &[]),
        "Design review",
        "a quiet calendar leaves the token empty rather than printing an empty clause"
    );
}

#[test]
fn an_event_outside_the_bar_window_leaves_no_applet_at_all() {
    let events = vec![event("Standup", at(4, 14, 0), at(4, 15, 0))];
    let now = at(4, 9, 0);

    assert!(
        next(now, &events, TimeDelta::hours(1), false).is_none(),
        "the applet shows one event's details; five hours out it has none to show"
    );
    assert!(next(now, &events, TimeDelta::hours(12), false).is_some());
    assert!(next(now, &[], TimeDelta::hours(12), false).is_none());
}

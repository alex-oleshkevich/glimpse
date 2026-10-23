use super::*;
use chrono::TimeZone;
use glimpse_config::TWENTY_FOUR;
use glimpse_widgets::Fact;

fn at(day: u32, hour: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 9, day, hour, 0, 0)
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

#[test]
fn a_day_at_or_after_the_truncation_mark_is_truncated() {
    let from = at(6, 22).with_timezone(&Utc);
    let day = at(6, 12).date_naive();

    assert!(!truncated(day - TimeDelta::days(1), Some(from)));
    assert!(
        truncated(day, Some(from)),
        "the day the mark falls on is already missing entries"
    );
    assert!(truncated(day + TimeDelta::days(1), Some(from)));
    assert!(
        !truncated(day + TimeDelta::days(400), None),
        "an untruncated list leaves every day loaded"
    );
}

#[test]
fn a_month_pair_keeps_the_lookback_and_ends_two_months_later() {
    let (from, to) = month_pair(2026, 12).expect("December is a month");

    assert_eq!(
        from.with_timezone(&Local).date_naive(),
        NaiveDate::from_ymd_opt(2026, 10, 31).expect("31 days before December")
    );
    assert_eq!(
        to.with_timezone(&Local).date_naive(),
        NaiveDate::from_ymd_opt(2027, 2, 1).expect("the first of February"),
        "two months on from December is February, across the year boundary"
    );
    assert!(
        month_pair(2026, 13).is_none(),
        "there is no thirteenth month"
    );
}

#[test]
fn a_day_is_named_relatively_only_next_to_today() {
    let today = at(4, 12).date_naive();
    assert_eq!(day_title(today, today), "Today");
    assert_eq!(day_title(today + TimeDelta::days(1), today), "Tomorrow");
    assert_eq!(day_title(today - TimeDelta::days(1), today), "Yesterday");
    assert_eq!(
        day_title(today + TimeDelta::days(3), today),
        "Monday, 7 Sep",
        "a bare weekday is ambiguous once the grid is a week away"
    );
}

#[test]
fn today_folds_what_is_over_and_an_event_opens_only_with_something_behind_it() {
    let mut over = event(at(4, 9), at(4, 10));
    over.calendar = "Work".to_owned();
    let mut ahead = event(at(4, 14), at(4, 15));
    ahead.meeting_url = Some("https://meet.google.com/abc-defg-hij".to_owned());
    let today = rows(
        at(4, 12),
        at(4, 12).date_naive(),
        &[over.clone(), ahead],
        TWENTY_FOUR,
        false,
    );
    assert!(today[0].past && !today[1].past);
    assert_eq!(today[0].facts, [Fact::new("Calendar", "Work")]);
    assert!(today[0].links.is_empty());
    assert_eq!(today[1].links[0].title, "Join Google Meet");
    assert!(
        today[1].facts.is_empty(),
        "no calendar, no organizer, nothing to list"
    );

    let yesterday = rows(
        at(5, 12),
        at(4, 12).date_naive(),
        &[over],
        TWENTY_FOUR,
        false,
    );
    assert!(
        !yesterday[0].past,
        "only today folds: every event on a past day is over, so folding it hides the day"
    );
}

#[test]
fn an_event_appears_under_every_day_it_covers() {
    let overnight = event(at(4, 22), at(5, 6));
    let rows = rows(
        at(4, 23),
        at(5, 6).date_naive(),
        &[overnight],
        TWENTY_FOUR,
        false,
    );

    assert_eq!(rows.len(), 1, "a night-spanning event belongs to both days");
}

#[test]
fn a_day_with_nothing_on_it_has_no_rows() {
    let meeting = event(at(4, 9), at(4, 10));
    assert!(
        rows(
            at(4, 12),
            at(6, 9).date_naive(),
            &[meeting],
            TWENTY_FOUR,
            false
        )
        .is_empty()
    );
}

#[test]
fn hide_all_day_leaves_a_timed_event_on_the_same_day_alone() {
    let mut holiday = event(at(4, 0), at(4, 23));
    holiday.all_day = true;
    let meeting = event(at(4, 9), at(4, 10));

    let shown = rows(
        at(4, 12),
        at(4, 9).date_naive(),
        &[holiday, meeting],
        TWENTY_FOUR,
        true,
    );
    assert_eq!(
        shown.len(),
        1,
        "the all-day entry is left out, the timed one stays"
    );
}

#[test]
fn a_marker_carries_one_dot_per_calendar_not_one_per_event() {
    let color = gtk4::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0);
    let mut first = event(at(4, 9), at(4, 10));
    first.color = Some(color);
    let mut second = event(at(4, 14), at(4, 15));
    second.color = Some(color);

    let markers = markers(&[first, second], false);
    assert_eq!(markers.len(), 1);
    assert_eq!(
        markers[0].1.len(),
        1,
        "two events from one calendar are one dot"
    );
}

#[test]
fn hide_all_day_drops_a_days_only_dot() {
    let color = gtk4::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0);
    let mut holiday = event(at(4, 0), at(4, 23));
    holiday.all_day = true;
    holiday.color = Some(color);

    assert!(
        markers(&[holiday], true).is_empty(),
        "a day marked only by a hidden all-day entry must not still show a dot"
    );
}

#[test]
fn a_week_number_is_offered_only_when_it_was_asked_for() {
    let day = at(4, 12).date_naive();
    assert_eq!(heading(day, true).1.as_deref(), Some("Week 36"));
    assert_eq!(heading(day, false).1, None);
    assert_eq!(heading(day, true).0, "Friday, 4 September");
}

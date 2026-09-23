use chrono::{DateTime, Local, NaiveDate, TimeDelta, Utc};
use gettextrs::gettext;
use glimpse_services::MeetingProvider;
use glimpse_widgets::{Event, Fact};

use crate::applets::agenda::{self, Occasion};
use crate::applets::tokens;

const MOST_ROWS: usize = 20;
const TITLE: usize = 24;
const JOIN: usize = 48;
const ELLIPSIS: char = '…';
const HOUR_MINUTES: i64 = 60;
const DAY_HOURS: i64 = 24;

pub fn window(minutes: u64) -> TimeDelta {
    i64::try_from(minutes)
        .ok()
        .and_then(TimeDelta::try_minutes)
        .unwrap_or(TimeDelta::MAX)
}

pub fn next(
    now: DateTime<Local>,
    events: &[Occasion],
    within: TimeDelta,
    all_day: bool,
) -> Option<usize> {
    soonest(now, events, within, false).or_else(|| soonest(now, events, within, all_day))
}

fn soonest(
    now: DateTime<Local>,
    events: &[Occasion],
    within: TimeDelta,
    with_all_day: bool,
) -> Option<usize> {
    let edge = edge(now, within);
    events
        .iter()
        .enumerate()
        .filter(|(_, event)| inside(now, event, edge) && (with_all_day || !event.all_day))
        .min_by_key(|(_, event)| (event.start.max(now), event.end))
        .map(|(index, _)| index)
}

fn edge(now: DateTime<Local>, window: TimeDelta) -> DateTime<Local> {
    now.checked_add_signed(window)
        .unwrap_or_else(|| DateTime::<Utc>::MAX_UTC.with_timezone(&Local))
}

fn inside(now: DateTime<Local>, event: &Occasion, edge: DateTime<Local>) -> bool {
    event.end > now && event.start <= edge
}

pub fn label(now: DateTime<Local>, event: &Occasion, counting: TimeDelta) -> String {
    let mut title: String = event.summary.chars().take(TITLE).collect();
    if event.summary.chars().nth(TITLE).is_some() {
        title.push(ELLIPSIS);
    }

    match inside(now, event, edge(now, counting)) {
        true => match countdown(now, event) {
            Some(countdown) => format!("{title} {}", countdown.beside()),
            None => title,
        },
        false => title,
    }
}

pub fn heading(now: DateTime<Local>, event: &Occasion, clock: &str) -> (String, String) {
    let extent = extent(now, event, clock);
    let subtitle = match event.location.is_empty() {
        true => extent,
        false => format!("{extent} · {}", event.location),
    };
    (event.summary.clone(), subtitle)
}

pub fn reading(now: DateTime<Local>, event: &Occasion, clock: &str) -> String {
    agenda::when(now, shown_day(now, event), event, clock)
}

fn shown_day(now: DateTime<Local>, event: &Occasion) -> NaiveDate {
    let first = event.start.date_naive();
    now.date_naive()
        .clamp(first, event.end.date_naive().max(first))
}

fn extent(now: DateTime<Local>, event: &Occasion, clock: &str) -> String {
    let started = event.start.date_naive();
    let body = match event.all_day {
        true => reading(now, event, clock),
        false => {
            let ends = match event.end.date_naive() == started {
                true => at(event.end, clock),
                false => format!("{} {}", event.end.format("%a"), at(event.end, clock)),
            };
            format!("{}–{ends}", at(event.start, clock))
        }
    };

    match started > now.date_naive() {
        true => format!("{} {body}", event.start.format("%a")),
        false => body,
    }
}

fn at(instant: DateTime<Local>, clock: &str) -> String {
    instant.format(clock).to_string()
}

pub struct Countdown {
    value: String,
    unit: String,
    readout_unit: String,
    running: bool,
}

impl Countdown {
    pub fn readout(&self) -> (&str, &str) {
        (&self.value, &self.readout_unit)
    }

    fn beside(&self) -> String {
        match self.running {
            true => gettext("ends in {value} {unit}"),
            false => gettext("in {value} {unit}"),
        }
        .replace("{value}", &self.value)
        .replace("{unit}", &self.unit)
    }
}

pub fn countdown(now: DateTime<Local>, event: &Occasion) -> Option<Countdown> {
    if event.all_day {
        return None;
    }

    let running = now >= event.start;
    if running {
        let left = (event.end - now).num_minutes().max(0);
        return Some(Countdown {
            value: left.to_string(),
            unit: gettext("min"),
            readout_unit: gettext("min left"),
            running,
        });
    }

    let until = event.start - now;
    let (value, unit) = match until {
        _ if until.num_minutes() < HOUR_MINUTES => (until.num_minutes().max(0), gettext("min")),
        _ if until.num_hours() < DAY_HOURS => (until.num_hours(), gettext("h")),
        _ => (until.num_days(), gettext("d")),
    };
    Some(Countdown {
        value: value.to_string(),
        readout_unit: unit.clone(),
        unit,
        running,
    })
}

pub fn upcoming(
    now: DateTime<Local>,
    events: &[Occasion],
    shown: Option<usize>,
    horizon: TimeDelta,
    limit: usize,
    clock: &str,
) -> Vec<Event> {
    let edge = edge(now, horizon);
    let mut rest: Vec<&Occasion> = events
        .iter()
        .enumerate()
        .filter(|(index, event)| Some(*index) != shown && inside(now, event, edge))
        .map(|(_, event)| event)
        .collect();
    rest.sort_by_key(|event| (event.start.max(now), event.end));
    rest.into_iter()
        .take(limit.min(MOST_ROWS))
        .map(|event| row(now, event, clock))
        .collect()
}

fn row(now: DateTime<Local>, event: &Occasion, clock: &str) -> Event {
    let day = shown_day(now, event);
    let mut row = agenda::row(now, day, event, clock);
    if day != now.date_naive() {
        row.when = format!("{} {}", event.start.format("%a"), row.when);
    }
    row
}

pub fn tooltip(format: &str, event: &Occasion, reading: &str, conflicts: &[String]) -> String {
    let clashes = clashes(conflicts);
    tokens::render(format, |token| match token {
        "summary" => Some(event.summary.as_str()),
        "detail" => Some(event.subtitle()),
        "when" => Some(reading),
        "conflicts" => Some(clashes.as_str()),
        _ => None,
    })
}

fn clashes(conflicts: &[String]) -> String {
    match conflicts.is_empty() {
        true => String::new(),
        false => gettext("Clashes with {events}").replace("{events}", &conflicts.join(", ")),
    }
}

pub struct Join {
    pub title: String,
    pub subtitle: String,
    pub url: String,
}

pub fn join(event: &Occasion) -> Option<Join> {
    let url = event.meeting_url.as_deref()?;
    let meeting = glimpse_services::meeting(url)?;
    Some(Join {
        title: join_title(meeting.provider),
        subtitle: glimpse_utils::clean(&meeting.location, JOIN),
        url: url.to_owned(),
    })
}

fn join_title(provider: MeetingProvider) -> String {
    match provider {
        MeetingProvider::GoogleMeet => gettext("Join Google Meet"),
        MeetingProvider::Zoom => gettext("Join Zoom"),
        MeetingProvider::Teams => gettext("Join Microsoft Teams"),
        MeetingProvider::Webex => gettext("Join Webex"),
        MeetingProvider::Other => gettext("Join meeting"),
    }
}

pub struct Open {
    pub title: String,
    pub subtitle: String,
    pub url: String,
}

pub fn open_event(event: &Occasion) -> Option<Open> {
    let url = event.event_url.as_deref()?;
    Some(Open {
        title: gettext("Open event"),
        subtitle: glimpse_utils::clean(url, JOIN),
        url: url.to_owned(),
    })
}

pub fn conflicts(events: &[Occasion], chosen: Option<usize>) -> Vec<String> {
    let Some(event) = chosen.and_then(|index| events.get(index)) else {
        return Vec::new();
    };
    events
        .iter()
        .enumerate()
        .filter(|(index, other)| Some(*index) != chosen && overlaps(event, other))
        .map(|(_, other)| other.summary.clone())
        .take(MOST_ROWS)
        .collect()
}

fn overlaps(event: &Occasion, other: &Occasion) -> bool {
    event.start < other.end && other.start < event.end
}

pub fn facts(event: &Occasion, conflicts: &[String]) -> Vec<Fact> {
    let mut facts = Vec::new();
    if !event.calendar.is_empty() {
        facts.push(Fact::new(gettext("Calendar"), event.calendar.clone()));
    }
    if !event.location.is_empty() {
        facts.push(Fact::new(gettext("Location"), event.location.clone()));
    }
    if !event.all_day {
        facts.push(Fact::new(
            gettext("Duration"),
            agenda::span(event.end - event.start),
        ));
    }
    if let Some(organizer) = &event.organizer {
        facts.push(Fact::new(gettext("Organizer"), organizer.clone()));
    }
    if let Some(guests) = &event.guests
        && guests.total >= 2
    {
        facts.push(Fact::new(
            gettext("Guests"),
            gettext("{total} · {accepted} accepted")
                .replace("{total}", &guests.total.to_string())
                .replace("{accepted}", &guests.accepted.to_string()),
        ));
    }
    facts.push(Fact::new(
        gettext("Status"),
        match event.tentative {
            true => gettext("Tentative"),
            false => gettext("Confirmed"),
        },
    ));
    if !event.description.is_empty() {
        facts.push(Fact::new(gettext("Description"), event.description.clone()));
    }
    if !conflicts.is_empty() {
        facts.push(Fact::new(gettext("Conflicts"), conflicts.join(", ")));
    }
    facts
}

#[cfg(test)]
mod tests {
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

    /// One entry covering three days is one entry, not three, so the row has to say which day of
    /// it the reader is on. Anchoring that to the event's own start made every row read "day 1".
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

    /// The bar spells out how long is left only once the event is close enough to be worth the
    /// width; outside that window the title stands alone.
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

    /// The suffix is added after the title is cut, so a long summary cannot eat the time.
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

    /// The bar's tooltip and the popover's hero describe the same event, so they have to pick the
    /// same day of it. Anchoring the tooltip to the event's start had them disagree mid-trip.
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

    /// An all-day entry covers the whole day, so ordering by start alone would let it outrank
    /// every meeting on it — which is exactly the one thing the bar has to get right.
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

    /// The bar has nothing useful to say about a meeting two days out, and saying it anyway is
    /// what the window exists to stop.
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

    /// A meeting you are already in is never too far away, however long ago it started.
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

    /// Two windows, and the list's is the wider one: the bar answers "is something about to
    /// happen", the popover answers "what does the rest of this look like".
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

    /// A late-afternoon glance has to reach tomorrow morning, which is the whole reason the list
    /// rolls forward by hours rather than stopping at midnight.
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

    /// A summary comes from a `.ics` file the user did not write, so a token inside one must
    /// stay text rather than become a second round of substitution.
    #[test]
    fn a_token_inside_an_events_own_text_is_not_substituted() {
        let hostile = event("{when}", at(4, 14, 0), at(4, 15, 0));

        assert_eq!(tooltip("{summary}", &hostile, "in 12 min", &[]), "{when}");
    }

    /// The bar's label ellipsizes at its rendered width, but only for a string that still
    /// overflows it — a hard cut with no mark reaches GTK already fitting, so it draws no
    /// ellipsis and the cut reads as the title itself.
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
        assert!(join(&meeting).is_none());

        meeting.meeting_url = Some("https://meet.google.com/aaa-bbbb-ccc".to_owned());
        let shown = join(&meeting).expect("a meet url");
        assert_eq!(shown.title, "Join Google Meet");
        assert_eq!(shown.subtitle, "meet.google.com/aaa-bbbb-ccc");
        assert_eq!(shown.url, "https://meet.google.com/aaa-bbbb-ccc");

        meeting.meeting_url = Some("https://zoom.us/j/123".to_owned());
        assert_eq!(join(&meeting).expect("zoom").title, "Join Zoom");

        meeting.meeting_url = Some("https://teams.microsoft.com/l/meetup-join/19".to_owned());
        assert_eq!(join(&meeting).expect("teams").title, "Join Microsoft Teams");

        meeting.meeting_url = Some("https://calendar.example/event".to_owned());
        assert_eq!(join(&meeting).expect("other").title, "Join meeting");

        meeting.meeting_url = Some("https://user:pass@zoom.us/j/123".to_owned());
        let shown = join(&meeting).expect("userinfo");
        assert_eq!(shown.title, "Join Zoom");
        assert_eq!(shown.subtitle, "zoom.us/j/123");
        assert!(
            !shown.subtitle.contains("pass"),
            "a password in the URL is not a subtitle"
        );

        meeting.meeting_url = Some("https://acme.webex.com/meet/sam".to_owned());
        assert_eq!(
            join(&meeting).expect("webex").title,
            "Join Webex",
            "the service accepts a webex link as a meeting, so the row has to name it — the two \
             lists drifting is what sharing one classifier prevents"
        );

        meeting.meeting_url = Some("https://acme.zoom.us/j/123".to_owned());
        assert_eq!(join(&meeting).expect("subdomain").title, "Join Zoom");

        meeting.meeting_url = Some("https://notzoom.us/j/123".to_owned());
        assert_eq!(
            join(&meeting).expect("lookalike").title,
            "Join meeting",
            "a suffix match must not treat notzoom.us as zoom.us"
        );
    }

    #[test]
    fn open_event_reads_the_events_own_url_and_is_absent_without_one() {
        let mut meeting = event("Standup", at(4, 14, 0), at(4, 15, 0));
        assert!(open_event(&meeting).is_none());

        meeting.event_url = Some("https://calendar.example/event/abc123".to_owned());
        let shown = open_event(&meeting).expect("an event url");
        assert_eq!(shown.title, "Open event");
        assert_eq!(shown.url, "https://calendar.example/event/abc123");
        assert_eq!(shown.subtitle, "https://calendar.example/event/abc123");
    }

    #[test]
    fn facts_omit_empty_rows_and_a_solo_attendee() {
        let mut meeting = event("Standup", at(4, 14, 0), at(4, 15, 0));
        assert_eq!(
            facts(&meeting, &[])
                .into_iter()
                .map(|fact| fact.label)
                .collect::<Vec<_>>(),
            vec!["Duration".to_owned(), "Status".to_owned()],
            "duration and status are read off every timed event, not only a decorated one"
        );

        meeting.calendar = "Work".to_owned();
        meeting.location = "Room 2".to_owned();
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
                ("Location".to_owned(), "Room 2".to_owned()),
                ("Duration".to_owned(), "1 h".to_owned()),
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
        meeting.tentative = false;
        meeting.organizer = None;
        meeting.description = String::new();
        let labels: Vec<String> = facts(&meeting, &[])
            .into_iter()
            .map(|fact| fact.label)
            .collect();
        assert_eq!(
            labels,
            vec![
                "Calendar".to_owned(),
                "Location".to_owned(),
                "Duration".to_owned(),
                "Status".to_owned(),
            ]
        );

        let mut holiday = event("Conference", at(4, 0, 0), at(4, 23, 59));
        holiday.all_day = true;
        assert!(
            !facts(&holiday, &[])
                .iter()
                .any(|fact| fact.label == "Duration"),
            "a day has no minute count worth showing"
        );
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
}

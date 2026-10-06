use chrono::{DateTime, Local, NaiveDate, TimeDelta, Utc};
use gettextrs::gettext;
use glimpse_widgets::{Event, Fact};

use crate::applets::agenda::{self, Occasion};
use crate::applets::tokens;

const MOST_ROWS: usize = 20;
const TITLE: usize = 24;
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
    let mut row = Event {
        links: agenda::links(event),
        facts: agenda::facts(event),
        ..agenda::row(now, day, event, clock)
    };
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
    if event.tentative {
        facts.push(Fact::new(gettext("Status"), gettext("Tentative")));
    }
    if !event.description.is_empty() {
        facts.push(Fact::new(gettext("Description"), event.description.clone()));
    }
    if let Some(join) = agenda::join(event) {
        facts.push(Fact::new(gettext("Meeting"), join.address));
    }
    if let Some(open) = agenda::open_event(event) {
        facts.push(Fact::new(gettext("Link"), open.address));
    }
    if !conflicts.is_empty() {
        facts.push(Fact::warning(gettext("Conflicts"), conflicts.join(", ")));
    }
    facts
}

#[cfg(test)]
mod tests;

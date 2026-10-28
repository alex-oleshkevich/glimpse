use chrono::{DateTime, Local, NaiveDate, TimeDelta};
use gettextrs::gettext;
use glimpse_services::{CalendarEvent, GuestCounts, MeetingProvider};
use glimpse_widgets::{Event, EventLink, Fact};
use gtk4::gdk;

const JOIN: usize = 48;
const SOON: i64 = 15;
const NEAR: i64 = 60;
const HOUR: i64 = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct Occasion {
    pub summary: String,
    pub location: String,
    pub description: String,
    pub calendar: String,
    pub meeting_url: Option<String>,
    pub event_url: Option<String>,
    pub organizer: Option<String>,
    pub guests: Option<GuestCounts>,
    pub tentative: bool,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub all_day: bool,
    pub color: Option<gdk::RGBA>,
}

impl Occasion {
    pub fn subtitle(&self) -> &str {
        if self.location.is_empty() {
            &self.description
        } else {
            &self.location
        }
    }
}

pub fn occasions(events: &[CalendarEvent]) -> Vec<Occasion> {
    events
        .iter()
        .map(|event| Occasion {
            summary: event.summary.clone(),
            location: event.location.clone(),
            description: event.description.clone(),
            calendar: event.calendar.clone(),
            meeting_url: event.meeting_url.clone(),
            event_url: event.event_url.clone(),
            organizer: event.organizer.clone(),
            guests: event.guests,
            tentative: event.tentative,
            start: event.start.with_timezone(&Local),
            end: event.end.with_timezone(&Local),
            all_day: event.all_day,
            color: event
                .color
                .as_deref()
                .and_then(|text| gdk::RGBA::parse(text).ok()),
        })
        .collect()
}

pub fn row(now: DateTime<Local>, day: NaiveDate, event: &Occasion, clock: &str) -> Event {
    Event {
        id: format!("{}|{}", event.start.timestamp(), event.summary),
        summary: event.summary.clone(),
        when: when(now, day, event, clock),
        color: event.color,
        ..Default::default()
    }
}

pub fn when(now: DateTime<Local>, day: NaiveDate, event: &Occasion, clock: &str) -> String {
    let first = event.start.date_naive();
    let last = event.end.date_naive();
    let days = (last - first).num_days() + 1;

    if event.all_day {
        return match days > 1 {
            true => gettext("All day · day {day} of {days}")
                .replace("{day}", &((day - first).num_days() + 1).to_string())
                .replace("{days}", &days.to_string()),
            false => gettext("All day"),
        };
    }

    let started = event.start.format(clock).to_string();
    let length = span(event.end - event.start);

    if days > 1 {
        return gettext("{start} · until {weekday} {end}")
            .replace("{start}", &started)
            .replace(
                "{weekday}",
                &crate::applets::strftime(&event.end, "%a")
                    .unwrap_or_else(|| event.end.format("%a").to_string()),
            )
            .replace("{end}", &event.end.format(clock).to_string());
    }

    if day != now.date_naive() {
        return gettext("{start} · {length}")
            .replace("{start}", &started)
            .replace("{length}", &length);
    }

    if now >= event.end {
        let ago = now - event.end;
        return match ago >= TimeDelta::minutes(NEAR) {
            true => gettext("{start} · over").replace("{start}", &started),
            false => gettext("ended {length} ago").replace("{length}", &span(ago)),
        };
    }

    if now >= event.start {
        let left = event.end - now;
        return match left <= TimeDelta::minutes(SOON) {
            true => gettext("now · ends in {length}").replace("{length}", &span(left)),
            false => {
                gettext("now · ends {end}").replace("{end}", &event.end.format(clock).to_string())
            }
        };
    }

    let until = event.start - now;
    if until < TimeDelta::minutes(1) {
        return gettext("starting now · {length}").replace("{length}", &length);
    }
    if until <= TimeDelta::minutes(NEAR) {
        return gettext("in {until} · {length}")
            .replace("{until}", &span(until))
            .replace("{length}", &length);
    }
    gettext("{start} · {length}")
        .replace("{start}", &started)
        .replace("{length}", &length)
}

pub struct Join {
    pub title: String,
    pub address: String,
    pub url: String,
}

pub fn join(event: &Occasion) -> Option<Join> {
    let url = event.meeting_url.as_deref()?;
    let meeting = glimpse_services::meeting(url)?;
    Some(Join {
        title: join_title(meeting.provider),
        address: glimpse_utils::clean(&meeting.location, JOIN),
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
    pub address: String,
    pub url: String,
}

pub fn open_event(event: &Occasion) -> Option<Open> {
    let url = event.event_url.as_deref()?;
    Some(Open {
        title: gettext("Open event"),
        address: glimpse_utils::clean(url, JOIN),
        url: url.to_owned(),
    })
}

pub fn links(event: &Occasion) -> Vec<EventLink> {
    let join = join(event).map(|join| EventLink {
        title: join.title,
        url: join.url,
    });
    let open = open_event(event).map(|open| EventLink {
        title: open.title,
        url: open.url,
    });
    join.into_iter().chain(open).collect()
}

pub fn facts(event: &Occasion) -> Vec<Fact> {
    let location = (!event.location.is_empty())
        .then(|| Fact::new(gettext("Location"), event.location.clone()));
    let calendar = (!event.calendar.is_empty())
        .then(|| Fact::new(gettext("Calendar"), event.calendar.clone()));
    let organizer = event
        .organizer
        .as_ref()
        .map(|organizer| Fact::new(gettext("Organizer"), organizer.clone()));
    let description = (!event.description.is_empty())
        .then(|| Fact::new(gettext("Description"), event.description.clone()));
    location
        .into_iter()
        .chain(calendar)
        .chain(organizer)
        .chain(description)
        .collect()
}

pub fn open_http(url: String) {
    if url.starts_with("https://") || url.starts_with("http://") {
        crate::applet::popover::run(&["xdg-open".to_owned(), url]);
    }
}

fn span(length: TimeDelta) -> String {
    let minutes = length.num_minutes().max(0);
    if minutes < HOUR {
        return gettext("{minutes} min").replace("{minutes}", &minutes.to_string());
    }

    let hours = minutes / HOUR;
    if hours < 24 {
        return match minutes % HOUR {
            0 => gettext("{hours} h").replace("{hours}", &hours.to_string()),
            rest => gettext("{hours} h {minutes} min")
                .replace("{hours}", &hours.to_string())
                .replace("{minutes}", &rest.to_string()),
        };
    }

    let days = hours / 24;
    match hours % 24 {
        0 => gettext("{days} d").replace("{days}", &days.to_string()),
        rest => gettext("{days} d {hours} h")
            .replace("{days}", &days.to_string())
            .replace("{hours}", &rest.to_string()),
    }
}

#[cfg(test)]
mod tests;

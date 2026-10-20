use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, NaiveTime, TimeDelta, TimeZone as _, Utc};
use futures_util::{StreamExt as _, stream, stream::BoxStream};
use glimpse_config::{CalendarSource, CalendarSourceKind, Update};
use glimpse_utils::clean;
use icalendar::{
    Calendar as ICalendar, CalendarDateTime, Component as _, DatePerhapsTime, Event as IEvent,
    EventLike as _, EventStatus, PartStat,
};
use reqwest::Url;
use tokio::{fs, sync::oneshot};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceEndpoint},
    subscription::Sub,
};

const BACK: i64 = 31;
const AHEAD: i64 = 62;
const SPAN: i64 = 400;
const OCCURRENCES: u16 = 512;
const EVENTS: usize = 512;
const DIRECTORY_BYTES: u64 = 16 * 1024 * 1024;
const SUMMARY: usize = 120;
const DETAIL: usize = 120;
const ORGANIZER: usize = 80;
const MEETING_URL: usize = 512;
const SIDECAR: usize = 2048;
const REASON: usize = 240;
const MIN_POLL: u64 = 60;
const TIMEOUT: Duration = Duration::from_secs(20);
use super::{AGENT, transport};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub source: String,
    pub calendar: String,
    pub summary: String,
    pub location: String,
    pub description: String,
    pub meeting_url: Option<String>,
    pub event_url: Option<String>,
    pub organizer: Option<String>,
    pub guests: Option<GuestCounts>,
    pub tentative: bool,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub all_day: bool,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestCounts {
    pub total: u32,
    pub accepted: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CalendarEvents {
    pub events: Vec<CalendarEvent>,
    pub truncated_from: Option<DateTime<Utc>>,
}

const WEBCAL: &str = "webcal://";

#[derive(Debug)]
pub enum Command {
    Refresh {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Range {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

pub type CalendarHandle = ServiceEndpoint<Calendar>;

impl ServiceEndpoint<Calendar> {
    pub async fn refresh(&self) -> Result<(), CommandError> {
        self.call("calendar stopped before refreshing", |reply| {
            Command::Refresh { reply }
        })
        .await?
    }

    pub async fn set_range(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<(), CommandError> {
        self.call("calendar stopped before changing its range", |reply| {
            Command::Range { from, to, reply }
        })
        .await?
    }
}

pub enum Event {
    Fetched {
        id: String,
        result: Result<Fetch, String>,
    },
    Expanded {
        generation: u64,
        payload: CalendarEvents,
    },
    Unwatched {
        id: String,
        reason: String,
    },
}

pub struct Fetch {
    calendars: Vec<Arc<ICalendar>>,
    remote: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Window {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
}

impl Window {
    fn around(now: DateTime<Utc>) -> Self {
        Self {
            from: now - TimeDelta::days(BACK),
            to: now + TimeDelta::days(AHEAD),
        }
    }

    fn asked(from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        let widest = from
            .checked_add_signed(TimeDelta::days(SPAN))
            .unwrap_or(DateTime::<Utc>::MAX_UTC);
        Self {
            from,
            to: to.clamp(from, widest),
        }
    }
}

pub struct Occurrence {
    summary: String,
    location: String,
    description: String,
    meeting_url: Option<String>,
    event_url: Option<String>,
    organizer: Option<String>,
    guests: Option<GuestCounts>,
    tentative: bool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    all_day: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    poll_interval: u64,
    sources: Vec<CalendarSource>,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            poll_interval: document.calendar.poll_interval.max(MIN_POLL),
            sources: distinct(&document.calendar.sources),
        }
    }
}

fn distinct(sources: &[CalendarSource]) -> Vec<CalendarSource> {
    let mut seen = BTreeSet::new();
    sources
        .iter()
        .filter(|source| seen.insert(source.id.clone()))
        .cloned()
        .collect()
}

struct Declares {
    watch: bool,
    timer: bool,
}

fn declares(source: &CalendarSource, remote: bool) -> Declares {
    let local = matches!(location(&source.uri), Ok(Location::Local(_)));
    Declares {
        watch: local || source.kind == CalendarSourceKind::Directory,
        timer: source.kind == CalendarSourceKind::Ical && (!local || remote),
    }
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Poll {
        id: String,
        uri: String,
        period: u64,
        attempt: u64,
    },
    Directory {
        id: String,
        uri: String,
        attempt: u64,
    },
    Expand {
        generation: u64,
    },
}

pub struct Calendar {
    events: Publisher<CalendarEvents>,
    client: Option<reqwest::Client>,
    sources: Vec<CalendarSource>,
    poll: u64,
    attempt: u64,
    generation: u64,
    window: Window,
    fetched: BTreeMap<String, Fetch>,
    failures: BTreeMap<String, String>,
}

impl Service for Calendar {
    const NAME: &'static str = "calendar";

    type Config = Config;
    type State = CalendarEvents;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut declared = Vec::new();

        for source in &self.sources {
            let remote = self
                .fetched
                .get(&source.id)
                .is_some_and(|fetch| fetch.remote);
            let declares = declares(source, remote);

            if declares.watch {
                let client = self.client.clone();
                let id = source.id.clone();
                let uri = source.uri.clone();
                let kind = source.kind;

                declared.push(Sub::stream(
                    Watch::Directory {
                        id: source.id.clone(),
                        uri: source.uri.clone(),
                        attempt: self.attempt,
                    },
                    move |_ctx| async move { watching(client, id, uri, kind) },
                ));
            }

            if declares.timer {
                let period = source.poll_interval.unwrap_or(self.poll).max(MIN_POLL);
                let client = self.client.clone();
                let id = source.id.clone();
                let uri = source.uri.clone();

                declared.push(Sub::interval(
                    Watch::Poll {
                        id: source.id.clone(),
                        uri: source.uri.clone(),
                        period,
                        attempt: self.attempt,
                    },
                    Duration::from_secs(period),
                    move |_ctx| {
                        let client = client.clone();
                        let id = id.clone();
                        let uri = uri.clone();
                        async move { reread(client, id, uri, CalendarSourceKind::Ical).await }
                    },
                ));
            }
        }

        let generation = self.generation;
        let window = self.window;
        let loaded = self.loaded();

        declared.push(Sub::stream(
            Watch::Expand { generation },
            move |_ctx| async move {
                let expansion =
                    tokio::task::spawn_blocking(move || expanding(generation, loaded, window));
                stream::once(expansion)
                    .filter_map(|joined| async move {
                        match joined {
                            Ok(event) => Some(event),
                            Err(error) => {
                                tracing::error!(%error, "expanding the calendar panicked");
                                None
                            }
                        }
                    })
                    .boxed()
            },
        ));

        declared
    }

    async fn start(ctx: &Ctx<Self>, config: Self::Config, (): Self::Dependencies) -> Self {
        let client = match reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(AGENT)
            .build()
        {
            Ok(client) => Some(client),
            Err(_) => {
                ctx.degraded("no http client; only local calendars can be read");
                None
            }
        };

        Self {
            events: ctx.publisher(),
            client,
            sources: config.sources,
            poll: config.poll_interval,
            attempt: 0,
            generation: 0,
            window: Window::around(Utc::now()),
            fetched: BTreeMap::new(),
            failures: BTreeMap::new(),
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Fetched { id, .. } | Event::Unwatched { id, .. })
                if !self.knows(&id) => {}
            Input::Event(Event::Expanded { generation, .. }) if generation != self.generation => {}
            Input::Event(Event::Fetched { id, result }) => {
                match result {
                    Ok(fetch) => {
                        self.failures.remove(&id);
                        self.fetched.insert(id, fetch);
                    }
                    Err(reason) => {
                        self.failures.insert(id, reason);
                    }
                }
                self.report(ctx);
                self.generation += 1;
            }
            Input::Event(Event::Expanded { payload, .. }) => {
                self.events.set(payload);
            }
            Input::Event(Event::Unwatched { id, reason }) => {
                self.failures.entry(id).or_insert(reason);
                self.report(ctx);
            }
            Input::Config(config) => {
                self.poll = config.poll_interval;
                self.sources = config.sources;
                let known: BTreeSet<String> = self
                    .sources
                    .iter()
                    .map(|source| source.id.clone())
                    .collect();
                self.fetched.retain(|id, _| known.contains(id));
                self.failures.retain(|id, _| known.contains(id));
                self.report(ctx);
                self.generation += 1;
            }
            Input::Command(Command::Refresh { reply }) => {
                self.attempt += 1;
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::Range { from, to, reply }) => {
                let window = Window::asked(from, to);
                if window != self.window {
                    self.window = window;
                    self.generation += 1;
                }
                let _ = reply.send(Ok(()));
            }
        }
    }
}

impl Calendar {
    fn knows(&self, id: &str) -> bool {
        self.sources.iter().any(|source| source.id == id)
    }

    fn report(&self, ctx: &Ctx<Self>) {
        if self.failures.is_empty() {
            ctx.running();
            return;
        }
        let reasons = self
            .failures
            .iter()
            .map(|(id, reason)| format!("{id}: {reason}"))
            .collect::<Vec<_>>()
            .join("; ");
        ctx.degraded(clean(&reasons, REASON));
    }

    fn loaded(&self) -> Vec<Loaded> {
        self.sources
            .iter()
            .filter_map(|source| {
                let fetch = self.fetched.get(&source.id)?;
                Some(Loaded {
                    id: source.id.clone(),
                    name: source.name.clone(),
                    color: source.color.clone(),
                    calendars: fetch.calendars.clone(),
                })
            })
            .collect()
    }
}

struct Loaded {
    id: String,
    name: Option<String>,
    color: Option<String>,
    calendars: Vec<Arc<ICalendar>>,
}

fn expanding(generation: u64, loaded: Vec<Loaded>, window: Window) -> Event {
    let mut events = Vec::new();
    for source in &loaded {
        for calendar in &source.calendars {
            for occurrence in expand(calendar, window) {
                events.push(CalendarEvent {
                    source: source.id.clone(),
                    calendar: source.name.clone().unwrap_or_else(|| source.id.clone()),
                    summary: occurrence.summary,
                    location: occurrence.location,
                    description: occurrence.description,
                    meeting_url: occurrence.meeting_url,
                    event_url: occurrence.event_url,
                    organizer: occurrence.organizer,
                    guests: occurrence.guests,
                    tentative: occurrence.tentative,
                    start: occurrence.start,
                    end: occurrence.end,
                    all_day: occurrence.all_day,
                    color: source.color.clone(),
                });
            }
        }
    }
    events.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.summary.cmp(&right.summary))
    });
    let truncated_from = events.get(EVENTS).map(|dropped| dropped.start);
    events.truncate(EVENTS);

    Event::Expanded {
        generation,
        payload: CalendarEvents {
            events,
            truncated_from,
        },
    }
}

enum Location {
    Remote(Url),
    Local(PathBuf),
}

fn subscribed(uri: &str) -> String {
    match uri
        .get(..WEBCAL.len())
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case(WEBCAL))
    {
        true => format!("https://{}", &uri[WEBCAL.len()..]),
        false => uri.to_owned(),
    }
}

fn location(uri: &str) -> Result<Location, String> {
    let uri = &subscribed(uri);
    match Url::parse(uri) {
        Ok(url) => match url.scheme() {
            "http" | "https" => Ok(Location::Remote(url)),
            "file" => Ok(Location::Local(
                url.to_file_path().unwrap_or_else(|()| url.path().into()),
            )),
            scheme => Err(format!("`{scheme}` is not a scheme this service reads")),
        },
        Err(_) => Ok(Location::Local(uri.into())),
    }
}

fn sidecar(text: &str) -> Option<Url> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > SIDECAR || trimmed.lines().count() > 1 {
        return None;
    }
    match Url::parse(&subscribed(trimmed)) {
        Ok(url) if matches!(url.scheme(), "http" | "https") => Some(url),
        _ => None,
    }
}

async fn fetch(client: Option<reqwest::Client>, url: Url) -> Result<String, String> {
    let client = client.ok_or("there is no http client, so a feed cannot be read")?;
    let response = client.get(url).send().await.map_err(transport)?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("the server answered {status}"));
    }
    response.text().await.map_err(transport)
}

enum Fetching {
    Remote(Url),
    Document(String),
}

impl Fetching {
    fn remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }
}

async fn resolve(uri: &str) -> Result<Fetching, String> {
    match location(uri)? {
        Location::Remote(url) => Ok(Fetching::Remote(url)),
        Location::Local(path) => {
            let text = fs::read_to_string(&path)
                .await
                .map_err(|error| format!("its file cannot be read: {}", error.kind()))?;
            Ok(match sidecar(&text) {
                Some(url) => Fetching::Remote(url),
                None => Fetching::Document(text),
            })
        }
    }
}

async fn feed(client: Option<reqwest::Client>, uri: &str) -> Result<(String, bool), String> {
    let resolved = resolve(uri).await?;
    let remote = resolved.remote();
    let document = match resolved {
        Fetching::Remote(url) => fetch(client, url).await?,
        Fetching::Document(text) => text,
    };
    Ok((document, remote))
}

async fn directory(uri: &str) -> Result<Vec<String>, String> {
    let Location::Local(root) = location(uri)? else {
        return Err("a directory source names a path, not a feed".to_owned());
    };
    let mut entries = fs::read_dir(&root)
        .await
        .map_err(|error| format!("its directory cannot be read: {}", error.kind()))?;

    let mut documents = Vec::new();
    while documents.len() < FILES {
        let Ok(Some(entry)) = entries.next_entry().await else {
            break;
        };
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ics"))
        {
            continue;
        }
        match fs::read_to_string(&path).await {
            Ok(text) => documents.push(text),
            Err(error) => tracing::warn!(kind = ?error.kind(), "an .ics file could not be read"),
        }
    }

    match documents.is_empty() {
        true => Err("its directory holds no readable `.ics` file".to_owned()),
        false => Ok(documents),
    }
}

async fn read(
    client: Option<reqwest::Client>,
    kind: CalendarSourceKind,
    uri: String,
) -> Result<Fetch, String> {
    let (documents, remote) = match kind {
        CalendarSourceKind::Ical => {
            let (document, remote) = feed(client, &uri).await?;
            (vec![document], remote)
        }
        CalendarSourceKind::Directory => (directory(&uri).await?, false),
    };

    let mut calendars = Vec::new();
    for document in &documents {
        let calendar: ICalendar = document
            .parse()
            .map_err(|_| "its document is not iCalendar".to_owned())?;
        calendars.push(Arc::new(calendar));
    }
    Ok(Fetch { calendars, remote })
}

fn expand(calendar: &ICalendar, window: Window) -> Vec<Occurrence> {
    let from = bound(window.from);
    let to = bound(window.to);
    let overridden = overridden_instances(calendar);

    let mut occurrences = Vec::new();
    for entry in calendar.calendar_events() {
        let event = entry.event();
        if event.get_status() == Some(EventStatus::Cancelled) {
            continue;
        }
        let Some(start) = event.get_start() else {
            continue;
        };
        let all_day = matches!(start, DatePerhapsTime::Date(_));
        let length = length(
            &start,
            event.get_end().as_ref(),
            event.property_value("DURATION"),
            all_day,
        );
        let Ok(set) = entry.get_recurrence() else {
            continue;
        };
        let summary = clean(event.get_summary().unwrap_or_default(), SUMMARY);
        let location = line(event.get_location());
        let description = line(event.get_description().and_then(first_line));
        let meeting_url = meeting_url(event);
        let event_url = http_url(event.get_url());
        let organizer = organizer(event);
        let guests = guests(event);
        let tentative = event.get_status() == Some(EventStatus::Tentative);
        let replaced = match event.property_value("RECURRENCE-ID") {
            Some(_) => None,
            None => overridden.get(event.get_uid().unwrap_or("")),
        };

        for date in set.after(from).before(to).all(OCCURRENCES).dates {
            let start = date.with_timezone(&Utc);
            if replaced.is_some_and(|replaced| replaced.holds(&date)) {
                continue;
            }
            occurrences.push(Occurrence {
                summary: summary.clone(),
                location: location.clone(),
                description: description.clone(),
                meeting_url: meeting_url.clone(),
                event_url: event_url.clone(),
                organizer: organizer.clone(),
                guests,
                tentative,
                start,
                end: start.checked_add_signed(length).unwrap_or(start),
                all_day,
            });
        }
    }
    occurrences
}

async fn reread(
    client: Option<reqwest::Client>,
    id: String,
    uri: String,
    kind: CalendarSourceKind,
) -> Event {
    let result = read(client, kind, uri).await;
    Event::Fetched { id, result }
}

fn failed(id: String, reason: String) -> BoxStream<'static, Event> {
    stream::once(async move {
        Event::Fetched {
            id,
            result: Err(reason),
        }
    })
    .boxed()
}

fn watching(
    client: Option<reqwest::Client>,
    id: String,
    uri: String,
    kind: CalendarSourceKind,
) -> BoxStream<'static, Event> {
    let root = match (location(&uri), kind) {
        (Ok(Location::Local(path)), CalendarSourceKind::Directory) => path,
        (Ok(Location::Local(path)), CalendarSourceKind::Ical) => match path.parent() {
            Some(parent) => parent.to_path_buf(),
            None => return failed(id, "its path has no directory to watch".to_owned()),
        },
        _ => return failed(id, "a watched source names a local path".to_owned()),
    };

    let leading = stream::once(reread(client.clone(), id.clone(), uri.clone(), kind));
    let changes = glimpse_config::watch(root).then(move |update| {
        let client = client.clone();
        let id = id.clone();
        let uri = uri.clone();
        async move {
            match update {
                Update::Changed(_) | Update::Rearmed => reread(client, id, uri, kind).await,
                Update::Unavailable(reason) => Event::Unwatched {
                    id,
                    reason: format!("its directory is not being watched: {reason}"),
                },
            }
        }
    });

    leading.chain(changes).boxed()
}

fn bound(instant: DateTime<Utc>) -> DateTime<rrule::Tz> {
    rrule::Tz::UTC.from_utc_datetime(&instant.naive_utc())
}

fn moment(value: &DatePerhapsTime) -> NaiveDateTime {
    match value {
        DatePerhapsTime::Date(date) => date.and_time(NaiveTime::MIN),
        DatePerhapsTime::DateTime(CalendarDateTime::Floating(naive)) => *naive,
        DatePerhapsTime::DateTime(CalendarDateTime::Utc(utc)) => utc.naive_utc(),
        DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone { date_time, .. }) => *date_time,
    }
}

fn length(
    start: &DatePerhapsTime,
    end: Option<&DatePerhapsTime>,
    duration: Option<&str>,
    all_day: bool,
) -> TimeDelta {
    let span = match end {
        Some(end) => moment(end) - moment(start),
        None => duration.and_then(spanning).unwrap_or_else(TimeDelta::zero),
    };
    let span = match all_day {
        true => span.max(TimeDelta::days(1)) - TimeDelta::seconds(1),
        false => span.max(TimeDelta::zero()),
    };
    span.min(TimeDelta::days(SPAN))
}

fn spanning(text: &str) -> Option<TimeDelta> {
    let parsed: iso8601::Duration = text.trim().parse().ok()?;
    TimeDelta::from_std(std::time::Duration::from(parsed)).ok()
}

#[derive(Default)]
struct Replaced {
    utc: BTreeSet<NaiveDateTime>,
    local: BTreeSet<NaiveDateTime>,
}

impl Replaced {
    fn holds(&self, date: &DateTime<rrule::Tz>) -> bool {
        self.utc.contains(&date.naive_utc()) || self.local.contains(&date.naive_local())
    }
}

fn overridden_instances(calendar: &ICalendar) -> BTreeMap<String, Replaced> {
    let mut overridden: BTreeMap<String, Replaced> = BTreeMap::new();
    for entry in calendar.calendar_events() {
        let event = entry.event();
        let Some(uid) = event.get_uid() else {
            continue;
        };
        let Some(recurrence) = event.get_recurrence_id() else {
            continue;
        };
        let replaced = overridden.entry(uid.to_owned()).or_default();
        match recurrence {
            DatePerhapsTime::DateTime(CalendarDateTime::Utc(_)) => {
                replaced.utc.insert(moment(&recurrence))
            }
            _ => replaced.local.insert(moment(&recurrence)),
        };
    }
    overridden
}

fn line(text: Option<&str>) -> String {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| clean(text, DETAIL))
        .unwrap_or_default()
}

fn first_line(description: &str) -> Option<&str> {
    description
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
}

fn organizer(event: &IEvent) -> Option<String> {
    let property = event.properties().get("ORGANIZER")?;
    let named = property
        .params()
        .get("CN")
        .map(|parameter| parameter.value().trim())
        .filter(|text| !text.is_empty());
    named
        .or_else(|| mailbox(property.value()))
        .map(|text| clean(text, ORGANIZER))
}

fn mailbox(value: &str) -> Option<&str> {
    let address = value
        .trim()
        .strip_prefix("mailto:")
        .or_else(|| value.trim().strip_prefix("MAILTO:"))
        .unwrap_or(value.trim());
    let local = address.split('@').next()?.trim();
    (!local.is_empty()).then_some(local)
}

fn guests(event: &IEvent) -> Option<GuestCounts> {
    let attendees = event.get_attendees();
    let total = u32::try_from(attendees.len()).unwrap_or(u32::MAX);
    if total < 2 {
        return None;
    }
    let accepted = attendees
        .iter()
        .filter(|attendee| attendee.part_stat == Some(PartStat::Accepted))
        .count();
    Some(GuestCounts {
        total,
        accepted: u32::try_from(accepted).unwrap_or(u32::MAX),
    })
}

fn meeting_url(event: &IEvent) -> Option<String> {
    http_url(event.property_value("X-GOOGLE-CONFERENCE"))
        .or_else(|| http_url(event.property_value("X-MICROSOFT-SKYPETEAMSMEETINGURL")))
        .or_else(|| event.get_url().and_then(conference_url))
        .or_else(|| event.get_location().and_then(conference_url))
        .or_else(|| event.get_description().and_then(first_conference))
}

fn http_url(value: Option<&str>) -> Option<String> {
    let text = value?.trim();
    let url = Url::parse(text).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let canon = url.as_str();
    if canon.chars().count() > MEETING_URL {
        return None;
    }
    Some(clean(canon, MEETING_URL))
}

fn conference_url(text: &str) -> Option<String> {
    let url = http_url(Some(text))?;
    match meeting(&url)?.provider {
        MeetingProvider::Other => None,
        _ => Some(url),
    }
}

fn first_conference(description: &str) -> Option<String> {
    description
        .split(|character: char| character.is_whitespace() || matches!(character, '<' | '>' | '"'))
        .map(|token| token.trim_end_matches([')', ']', '.', ',', ';', '>']))
        .find(|token| token.starts_with("http://") || token.starts_with("https://"))
        .and_then(conference_url)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingProvider {
    GoogleMeet,
    Zoom,
    Teams,
    Webex,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meeting {
    pub provider: MeetingProvider,
    pub location: String,
}

pub fn meeting(url: &str) -> Option<Meeting> {
    let parsed = Url::parse(url.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed
        .host_str()?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let path = parsed.path().trim_end_matches('/');
    Some(Meeting {
        provider: provider_of(&host),
        location: format!("{host}{path}"),
    })
}

fn provider_of(host: &str) -> MeetingProvider {
    if under(host, "meet.google.com") {
        MeetingProvider::GoogleMeet
    } else if under(host, "zoom.us") {
        MeetingProvider::Zoom
    } else if under(host, "teams.microsoft.com") {
        MeetingProvider::Teams
    } else if under(host, "webex.com") {
        MeetingProvider::Webex
    } else {
        MeetingProvider::Other
    }
}

fn under(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|rest| rest.ends_with('.'))
}

#[cfg(test)]
mod tests;

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
    service::{CommandError, Input, Service, ServiceEndpoint, ServiceError},
    subscription::Sub,
};

const BACK: i64 = 31;
const AHEAD: i64 = 62;
const SPAN: i64 = 400;
const OCCURRENCES: u16 = 512;
const EVENTS: usize = 512;
const FILES: usize = 256;
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

#[derive(Clone)]
pub struct CalendarHandle(ServiceEndpoint<Calendar>);

impl CalendarHandle {
    pub fn snapshot(&self) -> CalendarEvents {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<CalendarEvents> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn refresh(&self) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::Refresh { reply })?;
        result.await.map_err(|_| {
            CommandError::Unavailable("calendar stopped before refreshing".to_owned())
        })?
    }

    pub async fn set_range(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::Range { from, to, reply })?;
        result.await.map_err(|_| {
            CommandError::Unavailable("calendar stopped before changing its range".to_owned())
        })?
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
    type Handle = CalendarHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        CalendarHandle(endpoint)
    }

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

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        (): Self::Dependencies,
    ) -> Result<Self, ServiceError> {
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

        Ok(Self {
            events: ctx.publisher(),
            client,
            sources: config.sources,
            poll: config.poll_interval,
            attempt: 0,
            generation: 0,
            window: Window::around(Utc::now()),
            fetched: BTreeMap::new(),
            failures: BTreeMap::new(),
        })
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
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use chrono::{Datelike as _, TimeZone};
    use glimpse_dbus::Buses;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{ServiceState, service::ServiceRuntime};

    const DOCUMENT: &str = "\
BEGIN:VCALENDAR\r
VERSION:2.0\r
PRODID:-//glimpse//test//EN\r
BEGIN:VEVENT\r
UID:one@example\r
DTSTAMP:20260901T000000Z\r
DTSTART:20260904T090000Z\r
DTEND:20260904T093000Z\r
SUMMARY:Standup\r
LOCATION:Meeting room 2\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:two@example\r
DTSTAMP:20260901T000000Z\r
DTSTART:20260907T100000Z\r
DTEND:20260907T103000Z\r
RRULE:FREQ=WEEKLY;COUNT=3\r
EXDATE:20260914T100000Z\r
SUMMARY:Weekly\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:three@example\r
DTSTAMP:20260901T000000Z\r
DTSTART;VALUE=DATE:20260905\r
DTEND;VALUE=DATE:20260906\r
SUMMARY:Holiday\r
END:VEVENT\r
END:VCALENDAR\r
";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 4, 8, 0, 0)
            .single()
            .expect("one instant")
    }

    fn source(id: &str, kind: CalendarSourceKind, uri: &str) -> CalendarSource {
        CalendarSource {
            id: id.to_owned(),
            kind,
            uri: uri.to_owned(),
            name: None,
            poll_interval: None,
            color: None,
        }
    }

    fn document(poll_interval: u64, sources: Vec<CalendarSource>) -> glimpse_config::Config {
        glimpse_config::Config {
            calendar: glimpse_config::Calendar {
                poll_interval,
                sources,
            },
            ..Default::default()
        }
    }

    /// `Duration::from_secs(0)` makes `tokio::time::interval` panic, and every value between one
    /// and the floor is a request the provider would answer by blocking us.
    #[test]
    fn a_poll_interval_under_the_floor_is_raised_to_it() {
        assert_eq!(Config::from(&document(0, Vec::new())).poll_interval, 60);
        assert_eq!(Config::from(&document(59, Vec::new())).poll_interval, 60);
        assert_eq!(Config::from(&document(900, Vec::new())).poll_interval, 900);
    }

    /// Two sources under one id would share a subscription key, so the second would never run
    /// while silently overwriting the first's events in the published map.
    #[test]
    fn two_sources_sharing_an_id_are_reduced_to_the_first() {
        let sources = vec![
            source(
                "work",
                CalendarSourceKind::Ical,
                "https://one.example/a.ics",
            ),
            source(
                "work",
                CalendarSourceKind::Ical,
                "https://two.example/b.ics",
            ),
            source(
                "home",
                CalendarSourceKind::Ical,
                "https://one.example/c.ics",
            ),
        ];
        let kept = Config::from(&document(600, sources)).sources;

        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].uri, "https://one.example/a.ics");
        assert_eq!(kept[1].id, "home");
    }

    #[test]
    fn a_uri_is_read_as_a_feed_a_path_or_nothing_at_all() {
        assert!(matches!(
            location("https://example.test/a.ics"),
            Ok(Location::Remote(_))
        ));
        assert!(matches!(
            location("file:///home/someone/a.ics"),
            Ok(Location::Local(path)) if path == Path::new("/home/someone/a.ics")
        ));
        assert!(matches!(
            location("/home/someone/a.ics"),
            Ok(Location::Local(path)) if path == Path::new("/home/someone/a.ics")
        ));
        assert!(
            location("gopher://example.test/a.ics").is_err(),
            "a scheme nothing here fetches must say so rather than become a path"
        );
    }

    /// `webcal://` is what a provider's Subscribe button hands out, and it is https underneath.
    /// Refusing it made the most common way to get a feed URL the one shape that did not work.
    #[test]
    fn a_webcal_subscription_is_read_as_the_https_feed_it_is() {
        assert!(matches!(
            location("webcal://example.test/a.ics"),
            Ok(Location::Remote(url)) if url.as_str() == "https://example.test/a.ics"
        ));
        assert!(
            matches!(
                location("WEBCAL://example.test/a.ics"),
                Ok(Location::Remote(_))
            ),
            "a url scheme is case-insensitive"
        );
        assert_eq!(
            subscribed("https://example.test/a.ics"),
            "https://example.test/a.ics",
            "everything else is passed through untouched"
        );
        assert_eq!(
            subscribed("webc"),
            "webc",
            "a uri shorter than the scheme is not sliced"
        );
        assert_eq!(subscribed("wébcal://x"), "wébcal://x");
    }

    /// The whole point of the sidecar is that the secret URL stays out of `config.toml`, so a file
    /// holding one has to be told apart from a file holding a calendar.
    #[test]
    fn only_a_one_line_url_is_read_as_a_sidecar() {
        assert_eq!(
            sidecar("https://example.test/private/a.ics\n")
                .map(|url| url.to_string())
                .as_deref(),
            Some("https://example.test/private/a.ics")
        );
        assert!(sidecar(DOCUMENT).is_none(), "a calendar is not a sidecar");
        assert!(sidecar("").is_none());
        assert!(
            sidecar("https://example.test/a.ics\nhttps://example.test/b.ics").is_none(),
            "two lines is neither a sidecar nor something to guess at"
        );
        assert!(sidecar("file:///etc/passwd").is_none());
        assert_eq!(
            sidecar("webcal://example.test/private/a.ics")
                .map(|url| url.to_string())
                .as_deref(),
            Some("https://example.test/private/a.ics"),
            "the sidecar exists to hold the link a provider gave you, and that link is webcal"
        );
    }

    #[test]
    fn an_all_day_entry_ends_on_the_last_day_it_covers() {
        let start = DatePerhapsTime::Date(
            chrono::NaiveDate::from_ymd_opt(2026, 9, 5).expect("a real date"),
        );
        let end = DatePerhapsTime::Date(
            chrono::NaiveDate::from_ymd_opt(2026, 9, 6).expect("a real date"),
        );

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

    /// RFC 5545 lets an entry carry DURATION instead of DTEND, and Apple and some CalDAV exports
    /// do. `icalendar` does not surface it, so without this every one of them rendered as an
    /// instant with no length at all.
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

    /// A surface draws one dot per day an entry covers, walking the days one at a time — so an
    /// entry claiming to last ten thousand years is not a long entry, it is a hang. Neither route
    /// into a length is trustworthy: `DURATION` is text off a feed, and a `DTEND` can name any
    /// year at all.
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

    /// An iCalendar timestamp has no sub-second field, so an expectation built from `Utc::now()`
    /// carries precision the document cannot round-trip.
    fn whole_second() -> DateTime<Utc> {
        DateTime::from_timestamp(Utc::now().timestamp(), 0).expect("an instant in range")
    }

    fn crowded(base: DateTime<Utc>, count: usize) -> String {
        let mut document =
            String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n");
        for index in 0..count {
            let start = base + TimeDelta::minutes(30 * index as i64);
            let end = start + TimeDelta::minutes(30);
            document.push_str(&format!(
                "BEGIN:VEVENT\r\nUID:crowd-{index}\r\nDTSTAMP:20260901T000000Z\r\n\
                 DTSTART:{}\r\nDTEND:{}\r\nSUMMARY:Crowded {index}\r\nEND:VEVENT\r\n",
                start.format("%Y%m%dT%H%M%SZ"),
                end.format("%Y%m%dT%H%M%SZ"),
            ));
        }
        document.push_str("END:VCALENDAR\r\n");
        document
    }

    fn window() -> Window {
        Window::around(now())
    }

    fn parsed(document: &str) -> Arc<ICalendar> {
        Arc::new(document.parse().expect("a parseable document"))
    }

    fn loaded(id: &str, calendar: Arc<ICalendar>) -> Loaded {
        Loaded {
            id: id.to_owned(),
            name: None,
            color: None,
            calendars: vec![calendar],
        }
    }

    fn payload(expansion: Event) -> CalendarEvents {
        let Event::Expanded { payload, .. } = expansion else {
            panic!("expanding yields an expansion");
        };
        payload
    }

    async fn crowd(count: usize) -> CalendarEvents {
        let root = tempfile::tempdir().expect("a scratch directory");
        tokio::fs::write(root.path().join("crowd.ics"), crowded(now(), count))
            .await
            .expect("a written fixture");
        let fetch = read(
            None,
            CalendarSourceKind::Directory,
            root.path().to_string_lossy().into_owned(),
        )
        .await
        .expect("a readable directory");

        payload(expanding(
            0,
            vec![Loaded {
                id: "crowd".to_owned(),
                name: None,
                color: None,
                calendars: fetch.calendars,
            }],
            window(),
        ))
    }

    fn expanded(summary: &str) -> Vec<Occurrence> {
        expand(&parsed(DOCUMENT), window())
            .into_iter()
            .filter(|occurrence| occurrence.summary == summary)
            .collect()
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

    fn recurring(overrides: &str) -> Vec<String> {
        let document = format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:r@example\r\nDTSTAMP:20260901T000000Z\r\n\
             DTSTART:20260902T090000Z\r\nDTEND:20260902T093000Z\r\n\
             RRULE:FREQ=DAILY;COUNT=4\r\nSUMMARY:Daily\r\nEND:VEVENT\r\n\
             {overrides}END:VCALENDAR\r\n"
        );
        let mut shown: Vec<String> = expand(&parsed(&document), window())
            .iter()
            .map(|occurrence| format!("{} {}", occurrence.start, occurrence.summary))
            .collect();
        shown.sort();
        shown
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

    fn one(body: &str) -> Occurrence {
        let document = format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:x@example\r\nDTSTAMP:20260901T000000Z\r\n\
             DTSTART:20260904T090000Z\r\nDTEND:20260904T093000Z\r\n\
             SUMMARY:Entry\r\n{body}END:VEVENT\r\nEND:VCALENDAR\r\n"
        );
        let found = expand(&parsed(&document), window());
        assert_eq!(found.len(), 1, "{body}");
        found.into_iter().next().expect("one occurrence")
    }

    fn none(body: &str) {
        let document = format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:x@example\r\nDTSTAMP:20260901T000000Z\r\n\
             DTSTART:20260904T090000Z\r\nDTEND:20260904T093000Z\r\n\
             SUMMARY:Entry\r\n{body}END:VEVENT\r\nEND:VCALENDAR\r\n"
        );
        assert!(
            expand(&parsed(&document), window()).is_empty(),
            "{body} should not publish"
        );
    }

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
        let event = one(
            "X-MICROSOFT-SKYPETEAMSMEETINGURL:https://teams.microsoft.com/l/meetup-join/19\r\n",
        );

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

    /// A fetch reaches the filesystem through tokio's blocking pool, which yielding does not
    /// advance: without waiting on real time the inbox is still empty when the assertion runs.
    #[derive(Default)]
    struct Observation {
        states: std::sync::Mutex<Vec<CalendarEvents>>,
        health: std::sync::Mutex<Vec<ServiceState>>,
    }

    impl Observation {
        fn published(&self) -> Vec<CalendarEvents> {
            self.states
                .lock()
                .expect("state lock")
                .iter()
                .cloned()
                .collect()
        }

        fn health(&self) -> Vec<ServiceState> {
            self.health
                .lock()
                .expect("health lock")
                .iter()
                .cloned()
                .collect()
        }
    }

    struct Live {
        observation: Arc<Observation>,
        sender: crate::service::ServiceSender<Calendar>,
        cancel: CancellationToken,
        handle: tokio::task::JoinHandle<()>,
    }

    impl Live {
        fn start(sources: Vec<CalendarSource>) -> Self {
            let observation = Arc::new(Observation::default());
            let cancel = CancellationToken::new();
            let (mut runtime, handle) = ServiceRuntime::<Calendar>::new(
                Config {
                    poll_interval: MIN_POLL,
                    sources,
                },
                Buses::unavailable("no bus in tests"),
                cancel.clone(),
            );
            let mut state = handle.subscribe();
            let mut health = handle.health();
            let observed = observation.clone();
            let initial = handle.snapshot();
            tokio::spawn(async move {
                observed.states.lock().expect("state lock").push(initial);
                observed
                    .health
                    .lock()
                    .expect("health lock")
                    .push(health.borrow().clone());
                loop {
                    tokio::select! {
                        changed = state.changed() => {
                            if changed.is_err() { break; }
                            observed.states.lock().expect("state lock").push(state.borrow_and_update().clone());
                        }
                        changed = health.changed() => {
                            if changed.is_err() { break; }
                            observed.health.lock().expect("health lock").push(health.borrow_and_update().clone());
                        }
                    }
                }
            });
            let sender = runtime.sender();
            let handle = tokio::spawn(async move {
                let _ = runtime.run(()).await;
            });

            Self {
                observation,
                sender,
                cancel,
                handle,
            }
        }

        async fn until(&self, done: impl Fn(&Observation) -> bool) -> bool {
            for _ in 0..400 {
                tokio::time::sleep(Duration::from_millis(10)).await;
                if done(&self.observation) {
                    return true;
                }
            }
            false
        }

        async fn stop(self) -> Arc<Observation> {
            self.cancel.cancel();
            let _ = self.handle.await;
            self.observation
        }
    }

    async fn settled(
        sources: Vec<CalendarSource>,
        done: impl Fn(&Observation) -> bool,
    ) -> Arc<Observation> {
        let live = Live::start(sources);
        live.until(done).await;
        live.stop().await
    }

    fn marks(observation: &Observation) -> Vec<Option<DateTime<Utc>>> {
        observation
            .published()
            .into_iter()
            .map(|payload| payload.truncated_from)
            .collect()
    }

    fn holds(observation: &Observation, summary: &str) -> bool {
        published(observation)
            .into_iter()
            .flatten()
            .any(|event| event.summary == summary)
    }

    fn published(observation: &Observation) -> Vec<Vec<CalendarEvent>> {
        observation
            .published()
            .into_iter()
            .map(|payload| payload.events)
            .collect()
    }

    #[tokio::test]
    async fn a_calendar_with_no_sources_publishes_an_empty_list_and_stays_healthy() {
        let mock = settled(Vec::new(), |mock| !published(mock).is_empty()).await;

        assert_eq!(published(&mock).last(), Some(&Vec::new()));
        assert!(
            !mock
                .health()
                .iter()
                .any(|state| matches!(state, ServiceState::Degraded { .. })),
            "nothing configured is a working calendar, got {:?}",
            mock.health()
        );
    }

    /// A feed URL is a bearer token: whoever holds it reads the calendar. It must not reach the
    /// health report, which every client can read.
    #[tokio::test]
    async fn a_source_that_cannot_be_read_degrades_without_naming_its_uri() {
        let uri = "file:///nonexistent/glimpse-test-secret-token.ics";
        let mock = settled(
            vec![source("work", CalendarSourceKind::Ical, uri)],
            |mock| {
                mock.health()
                    .iter()
                    .any(|state| matches!(state, ServiceState::Degraded { .. }))
            },
        )
        .await;

        let degraded: Vec<String> = mock
            .health()
            .into_iter()
            .filter_map(|state| match state {
                ServiceState::Degraded { reason } => Some(reason),
                _ => None,
            })
            .collect();

        assert!(
            degraded.iter().any(|reason| reason.contains("work")),
            "the reason names the source that failed, got {degraded:?}"
        );
        assert!(
            !degraded
                .iter()
                .any(|reason| reason.contains("secret-token")),
            "the reason must never carry the uri, got {degraded:?}"
        );
    }

    #[tokio::test]
    async fn a_directory_source_reads_the_ics_files_in_it() {
        let root = tempfile::tempdir().expect("a scratch directory");
        tokio::fs::write(root.path().join("a.ics"), DOCUMENT)
            .await
            .expect("a written fixture");
        tokio::fs::write(root.path().join("notes.txt"), "ignored")
            .await
            .expect("a written decoy");

        let read = read(
            None,
            CalendarSourceKind::Directory,
            root.path().to_string_lossy().into_owned(),
        )
        .await;

        let fetched = read.expect("a readable directory");
        assert!(
            !fetched.remote,
            "a directory never reaches the network, so it must not be given a timer"
        );
        let summaries: BTreeSet<String> = fetched
            .calendars
            .iter()
            .flat_map(|calendar| expand(calendar, window()))
            .map(|occurrence| occurrence.summary)
            .collect();
        assert!(summaries.contains("Standup"));
        assert!(summaries.contains("Holiday"));
    }

    fn ics(uid: &str, summary: &str, at: DateTime<Utc>) -> String {
        let stamp = at.format("%Y%m%dT%H%M%SZ");
        format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:{uid}\r\nDTSTAMP:{stamp}\r\nDTSTART:{stamp}\r\n\
             DTEND:{stamp}\r\nSUMMARY:{summary}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
        )
    }

    /// The cap belongs to the payload, not to a source: capping each source first would publish
    /// more than the cap, and capping only the first would drop a whole calendar.
    #[test]
    fn the_cap_applies_to_the_merged_list_rather_than_to_each_source() {
        let base = now();
        let early = parsed(&crowded(base, EVENTS));
        let late = parsed(&crowded(base + TimeDelta::days(1), EVENTS));

        let merged = payload(expanding(
            0,
            vec![loaded("early", early), loaded("late", late)],
            window(),
        ));

        assert_eq!(merged.events.len(), EVENTS);
        let mark = merged.truncated_from.expect("a truncated payload");
        assert!(
            merged.events.iter().all(|kept| kept.start <= mark),
            "everything kept starts no later than the first entry dropped"
        );
        assert!(
            merged.events.iter().any(|event| event.source == "early")
                && merged.events.iter().any(|event| event.source == "late"),
            "both sources reach the payload, so the cap fell on the merge"
        );
    }

    /// A mark that outlives the truncation tells a surface a day is missing entries when it is
    /// not, and the placeholder it drives says so on every day from there on.
    #[tokio::test]
    async fn a_source_that_comes_back_under_the_cap_clears_its_mark() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let file = root.path().join("crowd.ics");
        let base = whole_second();
        tokio::fs::write(&file, crowded(base, EVENTS + 20))
            .await
            .expect("a written fixture");

        let live = Live::start(vec![source(
            "crowd",
            CalendarSourceKind::Directory,
            &root.path().to_string_lossy(),
        )]);
        assert!(
            live.until(|mock| marks(mock).iter().any(|mark| mark.is_some()))
                .await,
            "the crowded directory is marked to begin with"
        );

        tokio::fs::write(&file, crowded(base, 3))
            .await
            .expect("a rewritten fixture");
        let cleared = live.until(|mock| marks(mock).last() == Some(&None)).await;

        let mock = live.stop().await;
        assert!(
            cleared,
            "shrinking under the cap clears the mark, got {:?}",
            marks(&mock)
        );
    }

    /// A mark belongs to a source. Deleting the source and keeping the mark tells every surface
    /// that a month is missing when nothing is even configured.
    #[tokio::test]
    async fn removing_a_truncated_source_removes_its_mark() {
        let root = tempfile::tempdir().expect("a scratch directory");
        tokio::fs::write(
            root.path().join("crowd.ics"),
            crowded(whole_second(), EVENTS + 20),
        )
        .await
        .expect("a written fixture");

        let live = Live::start(vec![source(
            "crowd",
            CalendarSourceKind::Directory,
            &root.path().to_string_lossy(),
        )]);
        assert!(
            live.until(|mock| marks(mock).iter().any(|mark| mark.is_some()))
                .await,
            "the crowded source is marked to begin with"
        );

        live.sender
            .send(Input::Config(Config {
                poll_interval: MIN_POLL,
                sources: Vec::new(),
            }))
            .await
            .expect("queued");
        let gone = live
            .until(|mock| {
                marks(mock).last() == Some(&None) && published(mock).last() == Some(&Vec::new())
            })
            .await;

        let mock = live.stop().await;
        assert!(
            gone,
            "the mark and the events went with the source, got {:?}",
            marks(&mock)
        );
    }

    /// Truncation used to be silent, which a surface cannot tell apart from a quiet month: the
    /// list simply stopped and nothing said where. `truncated_from` is the start of the first
    /// entry that was dropped, so everything before it is known complete.
    #[tokio::test]
    async fn a_source_over_the_cap_says_where_its_list_stops() {
        let payload = crowd(EVENTS + 40).await;

        assert_eq!(payload.events.len(), EVENTS);
        let from = payload.truncated_from.expect("a truncated payload");
        assert_eq!(
            from,
            payload.events[EVENTS - 1].start + TimeDelta::minutes(30),
            "the mark is the first entry dropped, not the last one kept"
        );
        assert!(
            payload.events.iter().all(|kept| kept.start < from),
            "everything published is complete strictly before the mark"
        );
    }

    /// The window is what a surface asked for, so a list that fits inside it is complete. Marking
    /// the horizon instead is what made every day past it read as one the daemon had skipped.
    #[tokio::test]
    async fn a_source_inside_the_cap_carries_no_mark_at_all() {
        let payload = crowd(12).await;

        assert_eq!(payload.events.len(), 12);
        assert_eq!(payload.truncated_from, None);
    }

    /// A directory source declares no timer at all, so both reads here can only be the watch: the
    /// first is the leading read it opens with, the second the file arriving.
    #[tokio::test]
    async fn a_watched_directory_is_read_at_start_and_again_when_a_file_arrives() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let at = Utc::now() + TimeDelta::hours(1);
        tokio::fs::write(
            root.path().join("first.ics"),
            ics("one@example", "First", at),
        )
        .await
        .expect("a written fixture");

        let live = Live::start(vec![source(
            "local",
            CalendarSourceKind::Directory,
            &root.path().to_string_lossy(),
        )]);

        assert!(
            live.until(|mock| holds(mock, "First")).await,
            "the watch opens with a read, since nothing else ever reads a directory"
        );

        tokio::fs::write(
            root.path().join("second.ics"),
            ics("two@example", "Second", at),
        )
        .await
        .expect("a written fixture");
        let arrived = live.until(|mock| holds(mock, "Second")).await;

        live.stop().await;
        assert!(arrived, "a file dropped in reached the topic");
    }

    /// Nothing polls a directory source any more, so a `directory` pointed at a feed would sit
    /// silent forever if its watch just gave up quietly.
    /// The whole local/remote split lives here, and every row is a source someone can write. A
    /// timer on a local file is the regression this guards: it is invisible in a short test,
    /// because the floor is a minute.
    #[test]
    fn only_a_source_that_reaches_the_network_is_given_a_timer() {
        let fresh = false;
        let sidecar = true;

        let table = [
            (
                "dir bare path",
                source("d", CalendarSourceKind::Directory, "/tmp/cal"),
                fresh,
                true,
                false,
            ),
            (
                "dir file url",
                source("d", CalendarSourceKind::Directory, "file:///tmp/cal"),
                fresh,
                true,
                false,
            ),
            (
                "dir given a feed",
                source(
                    "d",
                    CalendarSourceKind::Directory,
                    "https://example.test/a.ics",
                ),
                fresh,
                true,
                false,
            ),
            (
                "ical https",
                source("h", CalendarSourceKind::Ical, "https://example.test/a.ics"),
                fresh,
                false,
                true,
            ),
            (
                "ical bare path",
                source("f", CalendarSourceKind::Ical, "/tmp/a.ics"),
                fresh,
                true,
                false,
            ),
            (
                "ical file url",
                source("f", CalendarSourceKind::Ical, "file:///tmp/a.ics"),
                fresh,
                true,
                false,
            ),
            (
                "ical webcal",
                source("w", CalendarSourceKind::Ical, "webcal://x.test/a.ics"),
                fresh,
                false,
                true,
            ),
            (
                "sidecar, known remote",
                source("side", CalendarSourceKind::Ical, "file:///tmp/a.url"),
                sidecar,
                true,
                true,
            ),
        ];

        for (name, source, remote, watch, timer) in table {
            let got = declares(&source, remote);
            assert_eq!(got.watch, watch, "{name}: watch");
            assert_eq!(got.timer, timer, "{name}: timer");
        }
    }

    /// Whether a source counts as remote decides whether it is given a timer, and for a sidecar
    /// that answer is only knowable by reading the file. Deciding it here rather than beside the
    /// fetch is what lets it be checked without a server.
    #[tokio::test]
    async fn a_sidecar_resolves_to_a_fetch_and_a_plain_file_does_not() {
        let root = tempfile::tempdir().expect("a scratch directory");

        let pointer = root.path().join("work.url");
        tokio::fs::write(&pointer, "https://example.test/private/a.ics\n")
            .await
            .expect("a written sidecar");
        let resolved = resolve(&pointer.to_string_lossy())
            .await
            .expect("a readable sidecar");
        assert!(
            resolved.remote(),
            "a sidecar names a calendar on the network"
        );
        assert!(matches!(resolved, Fetching::Remote(_)));

        let document = root.path().join("a.ics");
        tokio::fs::write(&document, DOCUMENT)
            .await
            .expect("a written calendar");
        let resolved = resolve(&document.to_string_lossy())
            .await
            .expect("a readable calendar");
        assert!(
            !resolved.remote(),
            "a local calendar is watched, not fetched"
        );
        assert!(matches!(resolved, Fetching::Document(_)));
    }

    #[tokio::test]
    async fn a_directory_source_pointed_at_a_feed_says_so_instead_of_going_quiet() {
        let mock = settled(
            vec![source(
                "wrong",
                CalendarSourceKind::Directory,
                "https://example.test/a.ics",
            )],
            |mock| {
                mock.health()
                    .iter()
                    .any(|state| matches!(state, ServiceState::Degraded { .. }))
            },
        )
        .await;

        assert!(
            mock.health().iter().any(|state| matches!(
                state,
                ServiceState::Degraded { reason } if reason.contains("wrong")
            )),
            "expected a Degraded naming the source, got {:?}",
            mock.health()
        );
    }

    /// A local `ical` source declares no timer at all, so the second read here can only be the
    /// watch on the file's own directory. Before the split it was polled like a remote feed.
    #[tokio::test]
    async fn a_local_ical_file_is_re_read_when_it_changes() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let file = root.path().join("one.ics");
        let at = Utc::now() + TimeDelta::hours(1);
        tokio::fs::write(&file, ics("one@example", "Before", at))
            .await
            .expect("a written fixture");

        let live = Live::start(vec![source(
            "localfile",
            CalendarSourceKind::Ical,
            &format!("file://{}", file.display()),
        )]);

        assert!(
            live.until(|mock| holds(mock, "Before")).await,
            "the watch opens with a read"
        );

        tokio::fs::write(&file, ics("one@example", "After", at))
            .await
            .expect("a rewritten fixture");
        let reread = live.until(|mock| holds(mock, "After")).await;

        live.stop().await;
        assert!(
            reread,
            "an edited local .ics reached the topic with no timer to carry it"
        );
    }

    /// Both failures are true and both name the source, but only one says the path is wrong.
    /// The watch used to arrive second and overwrite the read, so a typo in `uri` reported that
    /// the directory was not being watched — correct, and no help at all.
    #[tokio::test]
    async fn a_directory_that_is_not_there_reports_the_read_rather_than_the_watch() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let missing = root.path().join("no-such-directory");

        let mock = settled(
            vec![source(
                "typo",
                CalendarSourceKind::Directory,
                &missing.to_string_lossy(),
            )],
            |mock| {
                mock.health()
                    .iter()
                    .any(|state| matches!(state, ServiceState::Degraded { .. }))
            },
        )
        .await;

        let degraded: Vec<String> = mock
            .health()
            .into_iter()
            .filter_map(|state| match state {
                ServiceState::Degraded { reason } => Some(reason),
                _ => None,
            })
            .collect();

        assert!(
            degraded
                .last()
                .is_some_and(|reason| reason.contains("cannot be read")),
            "the read's message is the one that tells you the path is wrong, got {degraded:?}"
        );
    }

    #[tokio::test]
    async fn a_directory_with_nothing_in_it_is_reported_rather_than_read_as_empty() {
        let root = tempfile::tempdir().expect("a scratch directory");

        let read = read(
            None,
            CalendarSourceKind::Directory,
            root.path().to_string_lossy().into_owned(),
        )
        .await;

        assert!(read.is_err());
    }
}

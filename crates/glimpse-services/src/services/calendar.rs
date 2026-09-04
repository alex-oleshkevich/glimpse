use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, NaiveTime, TimeDelta, TimeZone as _, Utc};
use futures_util::{StreamExt as _, stream, stream::BoxStream};
use glimpse_config::{CalendarSource, CalendarSourceKind, Update};
use glimpse_contracts::{
    CalendarEvent, CalendarEvents, CalendarRefresh, Command as _, Message as _,
};
use glimpse_ipc::CallError;
use icalendar::{
    Calendar as ICalendar, CalendarDateTime, Component as _, DatePerhapsTime, Event as IEvent,
    EventLike as _,
};
use reqwest::Url;
use serde_json::Value;
use tokio::fs;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{Input, Service, ServiceError, unknown_command},
    subscription::Sub,
};

const BACK: i64 = 7;
const AHEAD: i64 = 62;
const OCCURRENCES: u16 = 512;
const EVENTS: usize = 512;
const FILES: usize = 256;
const SUMMARY: usize = 120;
const DETAIL: usize = 120;
const SIDECAR: usize = 2048;
const REASON: usize = 240;
const MIN_POLL: u64 = 60;
const TIMEOUT: Duration = Duration::from_secs(20);
const AGENT: &str = concat!("glimpse/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, PartialEq)]
pub enum Command {
    Refresh,
}

pub enum Event {
    Fetched {
        id: String,
        result: Result<Fetch, String>,
    },
}

pub struct Fetch {
    occurrences: Vec<Occurrence>,
    remote: bool,
}

pub struct Occurrence {
    summary: String,
    detail: String,
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

fn declares(source: &CalendarSource, remote: &BTreeSet<String>) -> Declares {
    let local = matches!(location(&source.uri), Ok(Location::Local(_)));
    Declares {
        watch: local || source.kind == CalendarSourceKind::Directory,
        timer: source.kind == CalendarSourceKind::Ical && (!local || remote.contains(&source.id)),
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
}

pub struct Calendar {
    events: Publisher<CalendarEvents>,
    client: Option<reqwest::Client>,
    sources: Vec<CalendarSource>,
    poll: u64,
    attempt: u64,
    fetched: BTreeMap<String, Vec<Occurrence>>,
    failures: BTreeMap<String, String>,
    remote: BTreeSet<String>,
}

impl Service for Calendar {
    const NAME: &'static str = "calendar";
    const TOPICS: &'static [&'static str] = &[CalendarEvents::NAME];
    const METHODS: &'static [&'static str] = &[CalendarRefresh::NAME];

    type Config = Config;
    type Command = Command;
    type Event = Event;
    type SubKey = Watch;

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut declared = Vec::new();

        for source in &self.sources {
            let declares = declares(source, &self.remote);

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

        declared
    }

    fn decode(method: &str, _args: Value) -> Result<Self::Command, CallError> {
        match method {
            CalendarRefresh::NAME => Ok(Command::Refresh),
            _ => Err(unknown_command(Self::NAME, method)),
        }
    }

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, ServiceError> {
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

        let mut service = Self {
            events: ctx.publisher::<CalendarEvents>(),
            client,
            sources: config.sources,
            poll: config.poll_interval,
            attempt: 0,
            fetched: BTreeMap::new(),
            failures: BTreeMap::new(),
            remote: BTreeSet::new(),
        };
        service.publish();
        Ok(service)
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Fetched { id, .. }) if !self.knows(&id) => {}
            Input::Event(Event::Fetched { id, result }) => {
                match result {
                    Ok(fetch) => {
                        self.failures.remove(&id);
                        if fetch.remote {
                            self.remote.insert(id.clone());
                        } else {
                            self.remote.remove(&id);
                        }
                        self.fetched.insert(id, fetch.occurrences);
                    }
                    Err(reason) => {
                        self.failures.insert(id, reason);
                    }
                }
                self.report(ctx);
                self.publish();
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
                self.remote.retain(|id| known.contains(id));
                self.report(ctx);
                self.publish();
            }
            Input::Command(Command::Refresh, responder) => {
                self.attempt += 1;
                responder.ok(());
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

    fn publish(&mut self) {
        let mut events: Vec<CalendarEvent> = self
            .sources
            .iter()
            .flat_map(|source| {
                self.fetched
                    .get(&source.id)
                    .into_iter()
                    .flatten()
                    .map(|occurrence| CalendarEvent {
                        source: source.id.clone(),
                        summary: occurrence.summary.clone(),
                        detail: occurrence.detail.clone(),
                        start: occurrence.start,
                        end: occurrence.end,
                        all_day: occurrence.all_day,
                        color: source.color.clone(),
                    })
            })
            .collect();
        events.sort_by(|left, right| {
            left.start
                .cmp(&right.start)
                .then_with(|| left.summary.cmp(&right.summary))
        });
        self.events.set(CalendarEvents { events });
    }
}

enum Location {
    Remote(Url),
    Local(PathBuf),
}

fn location(uri: &str) -> Result<Location, String> {
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
    match Url::parse(trimmed) {
        Ok(url) if matches!(url.scheme(), "http" | "https") => Some(url),
        _ => None,
    }
}

fn transport(error: reqwest::Error) -> String {
    match error.is_timeout() {
        true => "the request timed out".to_owned(),
        false => error.without_url().to_string(),
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
    now: DateTime<Utc>,
) -> Result<Fetch, String> {
    let (documents, remote) = match kind {
        CalendarSourceKind::Ical => {
            let (document, remote) = feed(client, &uri).await?;
            (vec![document], remote)
        }
        CalendarSourceKind::Directory => (directory(&uri).await?, false),
    };

    let mut occurrences = Vec::new();
    for document in &documents {
        occurrences.extend(expand(document, now)?);
    }
    occurrences.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.summary.cmp(&right.summary))
    });
    occurrences.truncate(EVENTS);
    Ok(Fetch {
        occurrences,
        remote,
    })
}

fn expand(document: &str, now: DateTime<Utc>) -> Result<Vec<Occurrence>, String> {
    let calendar: ICalendar = document
        .parse()
        .map_err(|_| "its document is not iCalendar".to_owned())?;

    let from = bound(now - TimeDelta::days(BACK));
    let to = bound(now + TimeDelta::days(AHEAD));

    let mut occurrences = Vec::new();
    for entry in calendar.calendar_events() {
        let event = entry.event();
        let Some(start) = event.get_start() else {
            continue;
        };
        let all_day = matches!(start, DatePerhapsTime::Date(_));
        let length = length(&start, event.get_end().as_ref(), all_day);
        let Ok(set) = entry.get_recurrence() else {
            continue;
        };
        let summary = clean(event.get_summary().unwrap_or_default(), SUMMARY);
        let detail = detail(event);

        for date in set.after(from).before(to).all(OCCURRENCES).dates {
            let start = date.with_timezone(&Utc);
            occurrences.push(Occurrence {
                summary: summary.clone(),
                detail: detail.clone(),
                start,
                end: start + length,
                all_day,
            });
        }
    }
    Ok(occurrences)
}

async fn reread(
    client: Option<reqwest::Client>,
    id: String,
    uri: String,
    kind: CalendarSourceKind,
) -> Event {
    let result = read(client, kind, uri, Utc::now()).await;
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
                Update::Unavailable(reason) => Event::Fetched {
                    id,
                    result: Err(format!("its directory is not being watched: {reason}")),
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

fn length(start: &DatePerhapsTime, end: Option<&DatePerhapsTime>, all_day: bool) -> TimeDelta {
    let span = end.map_or_else(TimeDelta::zero, |end| moment(end) - moment(start));
    match all_day {
        true => span.max(TimeDelta::days(1)) - TimeDelta::seconds(1),
        false => span.max(TimeDelta::zero()),
    }
}

fn detail(event: &IEvent) -> String {
    let location = event
        .get_location()
        .map(str::trim)
        .filter(|text| !text.is_empty());
    let described = event.get_description().and_then(|description| {
        description
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
    });
    clean(location.or(described).unwrap_or_default(), DETAIL)
}

fn clean(text: &str, cap: usize) -> String {
    let mut cleaned = String::new();
    let mut length = 0;
    let mut spaced = false;

    for character in text.chars() {
        if character.is_whitespace() || character.is_control() {
            spaced = length > 0;
            continue;
        }
        if length >= cap {
            cleaned.push('…');
            break;
        }
        if spaced {
            cleaned.push(' ');
            length += 1;
            spaced = false;
        }
        cleaned.push(character);
        length += 1;
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use chrono::{Datelike as _, TimeZone};
    use glimpse_dbus::Buses;
    use glimpse_ipc::ErrorCode;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{BrokerHandle, MockBroker, ServiceState, service::ServiceRuntime};

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

    #[test]
    fn declared_topics_and_methods_exist() {
        crate::service::assert_declarations::<Calendar>();
    }

    #[test]
    fn decode_answers_the_method_it_declares_and_refuses_the_rest() {
        assert_eq!(
            Calendar::decode(CalendarRefresh::NAME, Value::Null).expect("declared"),
            Command::Refresh
        );
        assert_eq!(
            Calendar::decode("calendar.set_events", Value::Null)
                .expect_err("never declared")
                .code,
            ErrorCode::UnknownCommand
        );
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
            location("webcal://example.test/a.ics").is_err(),
            "a scheme nothing here fetches must say so rather than become a path"
        );
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
    }

    #[test]
    fn text_off_a_feed_is_flattened_and_capped() {
        assert_eq!(clean("  Design\treview\n\n", 120), "Design review");
        assert_eq!(
            clean("a\u{7}b", 120),
            "a b",
            "a control character separates rather than vanishes, so it cannot splice two words"
        );
        assert_eq!(clean("abcdef", 4), "abcd…");
        assert_eq!(
            clean("ééééé", 3),
            "ééé…",
            "the cap counts characters, and slicing bytes would panic here"
        );
        assert_eq!(clean("   ", 120), "");
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
            length(&start, Some(&end), true),
            TimeDelta::days(1) - TimeDelta::seconds(1),
            "iCalendar writes an exclusive end; a surface asks which day the entry is on"
        );
        assert_eq!(
            length(&start, None, true),
            TimeDelta::days(1) - TimeDelta::seconds(1),
            "an all-day entry with no end still covers its own day"
        );
    }

    #[test]
    fn a_timed_entry_keeps_its_own_length_and_survives_a_missing_end() {
        let start = DatePerhapsTime::DateTime(CalendarDateTime::Utc(now()));
        let end = DatePerhapsTime::DateTime(CalendarDateTime::Utc(now() + TimeDelta::minutes(30)));

        assert_eq!(length(&start, Some(&end), false), TimeDelta::minutes(30));
        assert_eq!(length(&start, None, false), TimeDelta::zero());
        assert_eq!(
            length(&end, Some(&start), false),
            TimeDelta::zero(),
            "an end before its start is a broken entry, not one that lasts negative time"
        );
    }

    fn expanded(summary: &str) -> Vec<Occurrence> {
        expand(DOCUMENT, now())
            .expect("a parseable document")
            .into_iter()
            .filter(|occurrence| occurrence.summary == summary)
            .collect()
    }

    #[test]
    fn a_single_entry_carries_its_summary_location_and_instants() {
        let found = expanded("Standup");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].detail, "Meeting room 2");
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

    #[test]
    fn a_document_that_is_not_icalendar_is_a_failure_rather_than_an_empty_calendar() {
        assert!(expand("<html>not a calendar</html>", now()).is_err());
    }

    /// A fetch reaches the filesystem through tokio's blocking pool, which yielding does not
    /// advance: without waiting on real time the inbox is still empty when the assertion runs.
    async fn settled(
        sources: Vec<CalendarSource>,
        done: impl Fn(&MockBroker) -> bool,
    ) -> Arc<MockBroker> {
        let mock = Arc::new(MockBroker::default());
        let broker: Arc<dyn BrokerHandle> = mock.clone();
        let cancel = CancellationToken::new();
        let mut runtime = ServiceRuntime::<Calendar>::new(
            broker,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );

        let handle = tokio::spawn(async move {
            let _ = runtime
                .run(Config {
                    poll_interval: MIN_POLL,
                    sources,
                })
                .await;
        });

        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if done(&mock) {
                break;
            }
        }

        cancel.cancel();
        let _ = handle.await;
        mock
    }

    fn published(mock: &MockBroker) -> Vec<Vec<CalendarEvent>> {
        mock.published()
            .into_iter()
            .filter(|(topic, _)| topic == CalendarEvents::NAME)
            .filter_map(|(_, data)| serde_json::from_value::<CalendarEvents>(data).ok())
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
                .any(|(_, state)| matches!(state, ServiceState::Degraded { .. })),
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
                    .any(|(_, state)| matches!(state, ServiceState::Degraded { .. }))
            },
        )
        .await;

        let degraded: Vec<String> = mock
            .health()
            .into_iter()
            .filter_map(|(_, state)| match state {
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
            now(),
        )
        .await;

        let fetched = read.expect("a readable directory");
        assert!(
            !fetched.remote,
            "a directory never reaches the network, so it must not be given a timer"
        );
        let summaries: BTreeSet<String> = fetched
            .occurrences
            .into_iter()
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

        let mock = Arc::new(MockBroker::default());
        let broker: Arc<dyn BrokerHandle> = mock.clone();
        let cancel = CancellationToken::new();
        let mut runtime = ServiceRuntime::<Calendar>::new(
            broker,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let sources = vec![source(
            "local",
            CalendarSourceKind::Directory,
            &root.path().to_string_lossy(),
        )];
        let handle = tokio::spawn(async move {
            let _ = runtime
                .run(Config {
                    poll_interval: MIN_POLL,
                    sources,
                })
                .await;
        });

        let holds = |mock: &MockBroker, summary: &str| {
            published(mock)
                .into_iter()
                .flatten()
                .any(|event| event.summary == summary)
        };

        for _ in 0..400 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if holds(&mock, "First") {
                break;
            }
        }
        assert!(
            holds(&mock, "First"),
            "the watch opens with a read, since nothing else ever reads a directory"
        );

        tokio::fs::write(
            root.path().join("second.ics"),
            ics("two@example", "Second", at),
        )
        .await
        .expect("a written fixture");

        for _ in 0..400 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if holds(&mock, "Second") {
                break;
            }
        }

        cancel.cancel();
        let _ = handle.await;

        assert!(
            holds(&mock, "Second"),
            "a file dropped in reached the topic"
        );
    }

    /// Nothing polls a directory source any more, so a `directory` pointed at a feed would sit
    /// silent forever if its watch just gave up quietly.
    /// The whole local/remote split lives here, and every row is a source someone can write. A
    /// timer on a local file is the regression this guards: it is invisible in a short test,
    /// because the floor is a minute.
    #[test]
    fn only_a_source_that_reaches_the_network_is_given_a_timer() {
        let none = BTreeSet::new();
        let sidecars: BTreeSet<String> = ["side".to_owned()].into_iter().collect();

        let table = [
            (
                "dir bare path",
                source("d", CalendarSourceKind::Directory, "/tmp/cal"),
                &none,
                true,
                false,
            ),
            (
                "dir file url",
                source("d", CalendarSourceKind::Directory, "file:///tmp/cal"),
                &none,
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
                &none,
                true,
                false,
            ),
            (
                "ical https",
                source("h", CalendarSourceKind::Ical, "https://example.test/a.ics"),
                &none,
                false,
                true,
            ),
            (
                "ical bare path",
                source("f", CalendarSourceKind::Ical, "/tmp/a.ics"),
                &none,
                true,
                false,
            ),
            (
                "ical file url",
                source("f", CalendarSourceKind::Ical, "file:///tmp/a.ics"),
                &none,
                true,
                false,
            ),
            (
                "ical bad scheme",
                source("w", CalendarSourceKind::Ical, "webcal://x.test/a.ics"),
                &none,
                false,
                true,
            ),
            (
                "sidecar, known remote",
                source("side", CalendarSourceKind::Ical, "file:///tmp/a.url"),
                &sidecars,
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
                    .any(|(_, state)| matches!(state, ServiceState::Degraded { .. }))
            },
        )
        .await;

        assert!(
            mock.health().iter().any(|(_, state)| matches!(
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

        let mock = Arc::new(MockBroker::default());
        let broker: Arc<dyn BrokerHandle> = mock.clone();
        let cancel = CancellationToken::new();
        let mut runtime = ServiceRuntime::<Calendar>::new(
            broker,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let sources = vec![source(
            "localfile",
            CalendarSourceKind::Ical,
            &format!("file://{}", file.display()),
        )];
        let handle = tokio::spawn(async move {
            let _ = runtime
                .run(Config {
                    poll_interval: MIN_POLL,
                    sources,
                })
                .await;
        });

        let holds = |mock: &MockBroker, summary: &str| {
            published(mock)
                .into_iter()
                .flatten()
                .any(|event| event.summary == summary)
        };

        for _ in 0..400 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if holds(&mock, "Before") {
                break;
            }
        }
        assert!(holds(&mock, "Before"), "the watch opens with a read");

        tokio::fs::write(&file, ics("one@example", "After", at))
            .await
            .expect("a rewritten fixture");

        for _ in 0..400 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if holds(&mock, "After") {
                break;
            }
        }

        cancel.cancel();
        let _ = handle.await;

        assert!(
            holds(&mock, "After"),
            "an edited local .ics reached the topic with no timer to carry it"
        );
    }

    #[tokio::test]
    async fn a_directory_with_nothing_in_it_is_reported_rather_than_read_as_empty() {
        let root = tempfile::tempdir().expect("a scratch directory");

        let read = read(
            None,
            CalendarSourceKind::Directory,
            root.path().to_string_lossy().into_owned(),
            now(),
        )
        .await;

        assert!(read.is_err());
    }
}

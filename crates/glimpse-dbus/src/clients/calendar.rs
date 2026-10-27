use chrono::{DateTime, TimeDelta, Utc};
use glimpse_utils::clean;
use serde::{Deserialize, Serialize};
use zbus::proxy::CacheProperties;

use super::follower::{
    Changes, Follow, Provider, ProviderError, ProviderHandle, REASON, Snapshots, call, changes,
};
use super::{epoch, optional_clean};

pub const GLIMPSE_CALENDAR_BUS_NAME: &str = "me.aresa.Glimpse.Calendar";
pub const GLIMPSE_CALENDAR_OBJECT_PATH: &str = "/me/aresa/Glimpse/Calendar";
pub const EVENT_CAP: usize = 512;
pub const SPAN: i64 = 400;
pub const SUMMARY: usize = 120;
pub const DETAIL: usize = 120;
pub const ORGANIZER: usize = 80;
pub const MEETING_URL: usize = 512;

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
    pub revision: u64,
}

pub type CalendarEventWire = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    (bool, u32, u32),
    bool,
    i64,
    i64,
    bool,
    String,
);
pub type CalendarSnapshot = (String, (bool, i64), Vec<CalendarEventWire>, u64);
pub type CalendarRange = ((bool, i64), Vec<CalendarEventWire>);

#[zbus::proxy(
    interface = "me.aresa.Glimpse.Calendar1",
    default_service = "me.aresa.Glimpse.Calendar",
    default_path = "/me/aresa/Glimpse/Calendar"
)]
pub trait Calendar1 {
    #[zbus(property)]
    fn snapshot(&self) -> zbus::Result<CalendarSnapshot>;
    fn events(&self, from_us: i64, to_us: i64) -> zbus::Result<CalendarRange>;
    fn refresh(&self) -> zbus::Result<()>;
}

pub type CalendarProvider = Provider<CalendarProviderState>;
pub type CalendarProviderHandle = ProviderHandle<CalendarProviderState>;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CalendarProviderState {
    pub events: Vec<CalendarEvent>,
    pub truncated_from: Option<DateTime<Utc>>,
    pub available: bool,
    pub reason: Option<String>,
    pub revision: u64,
}

impl Follow for CalendarProviderState {
    type Proxy = Calendar1Proxy<'static>;
    type Error = CalendarProviderError;

    const NAME: &'static str = "calendar";

    fn lost(reason: String, previous: Option<&Self>) -> Self {
        Self {
            events: previous
                .map(|state| state.events.clone())
                .unwrap_or_default(),
            truncated_from: previous.and_then(|state| state.truncated_from),
            available: false,
            reason: Some(reason),
            revision: previous.map(|state| state.revision).unwrap_or_default(),
        }
    }

    fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

impl Snapshots for CalendarProviderState {
    type Snapshot = CalendarSnapshot;

    const BUS_NAME: &'static str = GLIMPSE_CALENDAR_BUS_NAME;

    async fn build(
        connection: &zbus::Connection,
        owner: zbus::names::OwnedUniqueName,
    ) -> zbus::Result<Self::Proxy> {
        Calendar1Proxy::builder(connection)
            .destination(owner)?
            .cache_properties(CacheProperties::Yes)
            .build()
            .await
    }

    async fn read(proxy: &Self::Proxy) -> zbus::Result<Self::Snapshot> {
        match proxy.cached_snapshot()? {
            Some(snapshot) => Ok(snapshot),
            None => proxy.snapshot().await,
        }
    }

    async fn changes(proxy: &Self::Proxy) -> Changes<Self::Snapshot> {
        changes(proxy.receive_snapshot_changed().await)
    }

    fn decode(snapshot: Self::Snapshot) -> Result<Self, String> {
        decode_snapshot(snapshot)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CalendarProviderError {
    #[error("calendar range is invalid: {0}")]
    InvalidRange(String),
    #[error("calendar provider unavailable: {0}")]
    Unavailable(String),
    #[error("calendar provider call timed out")]
    TimedOut,
    #[error("calendar provider call failed: {0}")]
    Call(String),
}

impl ProviderError for CalendarProviderError {
    fn unavailable(reason: String) -> Self {
        Self::Unavailable(reason)
    }
    fn timed_out() -> Self {
        Self::TimedOut
    }
    fn call(reason: String) -> Self {
        Self::Call(reason)
    }

    fn named(name: &str, reason: String) -> Option<Self> {
        match name {
            "me.aresa.Glimpse.Calendar1.Error.InvalidRange" => Some(Self::InvalidRange(reason)),
            "me.aresa.Glimpse.Calendar1.Error.Unavailable" => Some(Self::Unavailable(reason)),
            _ => None,
        }
    }
}

impl CalendarProviderHandle {
    pub async fn events(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<CalendarEvents, CalendarProviderError> {
        let range = call(
            self.proxy()
                .await?
                .events(from.timestamp_micros(), to.timestamp_micros()),
        )
        .await?;
        decode_range(range).map_err(CalendarProviderError::Call)
    }

    pub async fn refresh(&self) -> Result<(), CalendarProviderError> {
        call(self.proxy().await?.refresh()).await
    }
}

pub fn encode_snapshot(events: &CalendarEvents, reason: Option<&str>) -> CalendarSnapshot {
    let (truncated_from, encoded) = encode_range(events);
    (
        reason.unwrap_or_default().to_owned(),
        truncated_from,
        encoded,
        events.revision,
    )
}

pub fn decode_snapshot(snapshot: CalendarSnapshot) -> Result<CalendarProviderState, String> {
    let (reason, truncated_from, events, revision) = snapshot;
    let events = decode_range((truncated_from, events))?;
    Ok(CalendarProviderState {
        events: events.events,
        truncated_from: events.truncated_from,
        available: true,
        reason: optional_clean(reason, REASON),
        revision,
    })
}

pub fn encode_range(events: &CalendarEvents) -> CalendarRange {
    (
        (
            events.truncated_from.is_some(),
            events
                .truncated_from
                .map(|instant| instant.timestamp_micros())
                .unwrap_or_default(),
        ),
        events.events.iter().map(encode_event).collect(),
    )
}

pub fn decode_range(range: CalendarRange) -> Result<CalendarEvents, String> {
    let ((present, truncated_from), events) = range;
    Ok(CalendarEvents {
        events: events
            .into_iter()
            .take(EVENT_CAP)
            .map(decode_event)
            .collect::<Result<_, _>>()?,
        truncated_from: present.then(|| epoch(truncated_from)).transpose()?,
        revision: 0,
    })
}

fn encode_event(event: &CalendarEvent) -> CalendarEventWire {
    (
        event.source.clone(),
        event.calendar.clone(),
        event.summary.clone(),
        event.location.clone(),
        event.description.clone(),
        event.meeting_url.clone().unwrap_or_default(),
        event.event_url.clone().unwrap_or_default(),
        event.organizer.clone().unwrap_or_default(),
        event
            .guests
            .map(|guests| (true, guests.total, guests.accepted))
            .unwrap_or_default(),
        event.tentative,
        event.start.timestamp_micros(),
        event.end.timestamp_micros(),
        event.all_day,
        event.color.clone().unwrap_or_default(),
    )
}

fn decode_event(event: CalendarEventWire) -> Result<CalendarEvent, String> {
    let (
        source,
        calendar,
        summary,
        location,
        description,
        meeting_url,
        event_url,
        organizer,
        (guests, total, accepted),
        tentative,
        start,
        end,
        all_day,
        color,
    ) = event;
    let start = epoch(start)?;
    let end = bounded(start, epoch(end)?);
    Ok(CalendarEvent {
        source: clean(&source, DETAIL),
        calendar: clean(&calendar, DETAIL),
        summary: clean(&summary, SUMMARY),
        location: clean(&location, DETAIL),
        description: clean(&description, DETAIL),
        meeting_url: optional_clean(meeting_url, MEETING_URL),
        event_url: optional_clean(event_url, MEETING_URL),
        organizer: optional_clean(organizer, ORGANIZER),
        guests: guests.then_some(GuestCounts { total, accepted }),
        tentative,
        start,
        end,
        all_day,
        color: optional_clean(color, DETAIL),
    })
}

fn bounded(start: DateTime<Utc>, end: DateTime<Utc>) -> DateTime<Utc> {
    let span = end
        .signed_duration_since(start)
        .max(TimeDelta::zero())
        .min(TimeDelta::days(SPAN));
    start.checked_add_signed(span).unwrap_or(start)
}

#[cfg(test)]
mod tests;
